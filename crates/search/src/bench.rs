//! Servidor del socket Unix `bench.sock` (`0600` + `SO_PEERCRED`) en el bucle principal GLib.
//!
//! Implementa el protocolo JSON-lines documentado en `PORTING.md` §7 y consumido por
//! `crates/search-bench` (`bench`, `test-find` y los tests de integración).

use anyhow::{Context, Result};
use gnome_platform::glib::{self, ControlFlow};
use search_core::bench_proto::{
    decode_json_line, encode_json_line, measure_process_tree_rss_kb, resolve_socket_path,
    MAX_BENCH_LINE_BYTES,
};
use serde_json::{json, Map, Value};
use std::cell::RefCell;
use std::fs::{self, Permissions};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const MAX_TEXT_BYTES: usize = 256 * 1024;

type BenchHandler = Rc<dyn Fn(Map<String, Value>, Box<dyn FnOnce(Value)>)>;

thread_local! {
    static BENCH_HANDLER: RefCell<Option<BenchHandler>> = const { RefCell::new(None) };
}

pub fn is_bench_env_enabled() -> bool {
    let truthy = |name: &str| {
        std::env::var(name).ok().is_some_and(|v| {
            let t = v.trim();
            !t.is_empty() && t != "0" && !t.eq_ignore_ascii_case("false")
        })
    };
    truthy("SEARCH_BENCH") || truthy("SEARCH_PROBE") || truthy("SEARCH_SOCKET")
}

pub fn configured_socket_path() -> Result<PathBuf> {
    let world = std::env::var("SEARCH_PROBE")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    resolve_socket_path(None, world.as_deref())
}

pub struct BenchServer {
    socket_path: PathBuf,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl BenchServer {
    pub fn start<F>(socket_path: PathBuf, handler: F) -> Result<Self>
    where
        F: Fn(Map<String, Value>, Box<dyn FnOnce(Value)>) + 'static,
    {
        if let Some(parent) = socket_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("no se pudo crear '{}'", parent.display()))?;
        }
        if socket_path.exists() {
            let _ = fs::remove_file(&socket_path);
        }

        let listener = UnixListener::bind(&socket_path)
            .with_context(|| format!("no se pudo enlazar '{}'", socket_path.display()))?;
        fs::set_permissions(&socket_path, Permissions::from_mode(0o600))
            .with_context(|| format!("no se pudo fijar 0600 en '{}'", socket_path.display()))?;
        listener
            .set_nonblocking(true)
            .context("no se pudo configurar bench.sock como no bloqueante")?;

        BENCH_HANDLER.with(|slot| {
            *slot.borrow_mut() = Some(Rc::new(handler));
        });

        let stop = Arc::new(AtomicBool::new(false));
        let stop_bg = Arc::clone(&stop);
        let sock_bg = socket_path.clone();

        let handle = thread::Builder::new()
            .name("search-bench".to_string())
            .spawn(move || accept_loop(listener, stop_bg))
            .context("no se pudo lanzar el hilo de bench.sock")?;

        search_core::info!(
            "Socket bench activo en '{}' (0600, SO_PEERCRED)",
            sock_bg.display()
        );
        Ok(Self {
            socket_path,
            stop,
            handle: Some(handle),
        })
    }
}

impl Drop for BenchServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = UnixStream::connect(&self.socket_path);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        let _ = fs::remove_file(&self.socket_path);
        BENCH_HANDLER.with(|slot| {
            *slot.borrow_mut() = None;
        });
    }
}

fn accept_loop(listener: UnixListener, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                if !verify_peer_uid(&stream) {
                    search_core::warn!("Conexión rechazada en bench.sock por SO_PEERCRED");
                    continue;
                }
                thread::spawn(move || handle_client(stream));
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(err) => {
                if !stop.load(Ordering::Relaxed) {
                    search_core::warn!("Error aceptando en bench.sock: {err}");
                }
                break;
            }
        }
    }
}

fn verify_peer_uid(stream: &UnixStream) -> bool {
    let mut cred = libc::ucred {
        pid: 0,
        uid: u32::MAX,
        gid: u32::MAX,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut cred as *mut libc::ucred).cast(),
            &mut len,
        )
    };
    let expected_uid = unsafe { libc::geteuid() };
    rc == 0
        && len == std::mem::size_of::<libc::ucred>() as libc::socklen_t
        && cred.uid == expected_uid
}

fn handle_client(mut stream: UnixStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));

    let Ok(cloned) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(cloned.take((MAX_BENCH_LINE_BYTES + 1) as u64));
    let mut raw = Vec::new();
    if reader.read_until(b'\n', &mut raw).is_err() || raw.is_empty() {
        return;
    }

    let reply_val = match decode_json_line(&raw) {
        Ok(req) => {
            let (tx, rx) = mpsc::sync_channel::<Value>(1);
            glib::idle_add_once(move || {
                let maybe_handler = BENCH_HANDLER.with(|slot| slot.borrow().clone());
                if let Some(handler) = maybe_handler {
                    handler(
                        req,
                        Box::new(move |val| {
                            let _ = tx.send(val);
                        }),
                    );
                } else {
                    let _ = tx.send(json!({ "error": "servidor bench detenido" }));
                }
            });
            rx.recv_timeout(Duration::from_secs(60))
                .unwrap_or_else(|_| json!({ "error": "tiempo agotado ejecutando comando bench" }))
        }
        Err(err) => json!({ "error": err.to_string() }),
    };

    if let Ok(wire) = encode_json_line(&reply_val) {
        let _ = stream.write_all(&wire);
        let _ = stream.flush();
    }
}

pub type SpawnWindowFn = Rc<dyn Fn(bool, Option<&str>) -> Result<crate::window::BrowserWindow>>;

/// Estado compartido para despachar comandos `bench.sock` sobre las ventanas de `AppController`.
pub struct BenchDispatchContext {
    pub startup_instant: Instant,
    pub runtime_dir: PathBuf,
    pub windows: Rc<RefCell<Vec<crate::window::BrowserWindow>>>,
    pub spawn_window: SpawnWindowFn,
    pub save_session: Rc<dyn Fn()>,
    pub quit_app: Rc<dyn Fn()>,
}

pub fn dispatch_command(
    ctx: &BenchDispatchContext,
    req: Map<String, Value>,
    reply: Box<dyn FnOnce(Value)>,
) {
    let Some(verb) = req.get("do").and_then(Value::as_str) else {
        reply(json!({ "error": "falta la clave 'do'" }));
        return;
    };

    let target_window = |idx_opt: Option<usize>| -> Option<crate::window::BrowserWindow> {
        let wins = ctx.windows.borrow();
        if let Some(one_based) = idx_opt {
            wins.get(one_based.saturating_sub(1)).cloned()
        } else {
            wins.iter()
                .find(|w| w.gtk_window().is_active())
                .cloned()
                .or_else(|| wins.first().cloned())
        }
    };
    let win_idx = req
        .get("window")
        .and_then(Value::as_u64)
        .map(|n| n as usize);

    let find_tab_window = |tab_id: u64| -> Option<crate::window::BrowserWindow> {
        ctx.windows
            .borrow()
            .iter()
            .find(|w| w.tab_summary(tab_id).is_some())
            .cloned()
    };

    match verb {
        "tabs" => {
            let tabs: Vec<Value> = if let Some(idx) = win_idx {
                target_window(Some(idx))
                    .map(|w| w.tab_summaries())
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|t| serde_json::to_value(t).ok())
                    .collect()
            } else {
                ctx.windows
                    .borrow()
                    .iter()
                    .flat_map(|w| w.tab_summaries())
                    .filter_map(|t| serde_json::to_value(t).ok())
                    .collect()
            };
            reply(json!({ "tabs": tabs }));
        }
        "open" => {
            let url = req
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or("about:blank");
            let private = req.get("private").and_then(Value::as_bool).unwrap_or(false);
            let existing = if private {
                ctx.windows
                    .borrow()
                    .iter()
                    .find(|w| w.is_private())
                    .cloned()
            } else {
                target_window(win_idx)
            };
            let win = match existing {
                Some(w) => w,
                None => match (ctx.spawn_window)(private, Some(url)) {
                    Ok(w) => {
                        w.present();
                        if let Some(tid) = w.selected_tab_id() {
                            if let Some(summary) = w.tab_summary(tid) {
                                reply(
                                    serde_json::to_value(summary)
                                        .unwrap_or_else(|_| json!({ "id": tid.to_string() })),
                                );
                                return;
                            }
                        }
                        reply(json!({ "error": "ventana creada sin pestaña inicial" }));
                        return;
                    }
                    Err(err) => {
                        reply(json!({ "error": err.to_string() }));
                        return;
                    }
                },
            };
            match win.open_tab(Some(url), true, false, false) {
                Ok(tid) => {
                    let val = win
                        .tab_summary(tid)
                        .and_then(|s| serde_json::to_value(s).ok())
                        .unwrap_or_else(|| json!({ "id": tid.to_string(), "url": url }));
                    reply(val);
                }
                Err(err) => reply(json!({ "error": err.to_string() })),
            }
        }
        "go" => {
            let Some(tab_id) = parse_tab_id(&req) else {
                reply(json!({ "error": "falta 'id' numérico de pestaña" }));
                return;
            };
            let Some(url) = req.get("url").and_then(Value::as_str) else {
                reply(json!({ "error": "falta 'url'" }));
                return;
            };
            let Some(win) = find_tab_window(tab_id) else {
                reply(json!({ "error": format!("pestaña no encontrada: {tab_id}") }));
                return;
            };
            let Some(bv) = win.ensure_tab_awake(tab_id) else {
                reply(json!({ "error": format!("pestaña {tab_id} no tiene vista web") }));
                return;
            };
            bv.load_url(url);
            let val = win
                .tab_summary(tab_id)
                .and_then(|s| serde_json::to_value(s).ok())
                .unwrap_or_else(|| json!({ "id": tab_id.to_string(), "url": url }));
            reply(val);
        }
        "wait" => {
            let Some(tab_id) = parse_tab_id(&req) else {
                reply(json!({ "error": "falta 'id' numérico de pestaña" }));
                return;
            };
            let seconds = req
                .get("seconds")
                .and_then(Value::as_f64)
                .unwrap_or(15.0)
                .clamp(0.05, 120.0);
            let deadline = Instant::now() + Duration::from_secs_f64(seconds);
            let Some(win) = find_tab_window(tab_id) else {
                reply(json!({ "error": format!("pestaña no encontrada: {tab_id}") }));
                return;
            };
            let _ = win.ensure_tab_awake(tab_id);
            let reply_cell = Rc::new(RefCell::new(Some(reply)));
            glib::timeout_add_local(Duration::from_millis(30), move || {
                let summary = win.tab_summary(tab_id);
                let bv = win.tab_browser_view(tab_id);
                let loaded = match (&summary, &bv) {
                    (Some(s), Some(v)) => {
                        !s.loading && v.load_finish_count() > 0 && v.non_empty_paint_count() > 0
                    }
                    (Some(s), None) => !s.loading,
                    (None, _) => true,
                };
                let done = loaded || Instant::now() >= deadline;
                if done {
                    if let Some(cb) = reply_cell.borrow_mut().take() {
                        match summary {
                            Some(s) => cb(serde_json::to_value(s)
                                .unwrap_or_else(|_| json!({ "id": tab_id.to_string() }))),
                            None => cb(
                                json!({ "error": format!("pestaña cerrada durante wait: {tab_id}") }),
                            ),
                        }
                    }
                    ControlFlow::Break
                } else {
                    ControlFlow::Continue
                }
            });
        }
        "select" => {
            let Some(tab_id) = parse_tab_id(&req) else {
                reply(json!({ "error": "falta 'id' numérico de pestaña" }));
                return;
            };
            let Some(win) = find_tab_window(tab_id) else {
                reply(json!({ "error": format!("pestaña no encontrada: {tab_id}") }));
                return;
            };
            win.select_tab_by_id(tab_id);
            win.present();
            let val = win
                .tab_summary(tab_id)
                .and_then(|s| serde_json::to_value(s).ok())
                .unwrap_or_else(|| json!({ "id": tab_id.to_string() }));
            reply(val);
        }
        "sleep" => {
            let Some(tab_id) = parse_tab_id(&req) else {
                reply(json!({ "error": "falta 'id' numérico de pestaña" }));
                return;
            };
            let Some(win) = find_tab_window(tab_id) else {
                reply(json!({ "error": format!("pestaña no encontrada: {tab_id}") }));
                return;
            };
            let val = win
                .tab_summary(tab_id)
                .and_then(|s| serde_json::to_value(s).ok())
                .unwrap_or_else(|| json!({ "id": tab_id.to_string() }));
            reply(val);
        }
        "close" => {
            let raw_id = req.get("id").and_then(Value::as_str).unwrap_or("");
            if raw_id == "all" {
                let mut closed = 0usize;
                let wins: Vec<_> = ctx.windows.borrow().iter().cloned().collect();
                for w in wins {
                    let ids: Vec<u64> = w
                        .tab_summaries()
                        .into_iter()
                        .filter_map(|t| t.id.parse::<u64>().ok())
                        .collect();
                    for tid in ids {
                        if w.close_tab_by_id(tid) {
                            closed += 1;
                        }
                    }
                }
                reply(json!({ "closed": closed }));
                return;
            }
            let Some(tab_id) = parse_tab_id(&req) else {
                reply(json!({ "error": "falta 'id' de pestaña o 'all'" }));
                return;
            };
            let closed = find_tab_window(tab_id)
                .map(|w| usize::from(w.close_tab_by_id(tab_id)))
                .unwrap_or(0);
            reply(json!({ "closed": closed }));
        }
        "text" => {
            let Some(tab_id) = parse_tab_id(&req) else {
                reply(json!({ "error": "falta 'id' numérico de pestaña" }));
                return;
            };
            let Some(win) = find_tab_window(tab_id) else {
                reply(json!({ "error": format!("pestaña no encontrada: {tab_id}") }));
                return;
            };
            let Some(bv) = win.ensure_tab_awake(tab_id) else {
                reply(json!({ "error": format!("pestaña {tab_id} no tiene vista web") }));
                return;
            };
            let url = bv.current_url();
            let title = bv.title();
            let reply_cell = Rc::new(RefCell::new(Some(reply)));
            let reply_to = Rc::clone(&reply_cell);
            glib::timeout_add_local_once(Duration::from_secs(5), move || {
                if let Some(cb) = reply_to.borrow_mut().take() {
                    cb(json!({ "error": "tiempo agotado leyendo texto de página" }));
                }
            });
            bv.eval_js_async("document.body ? document.body.innerText : ''", move |res| {
                if let Some(cb) = reply_cell.borrow_mut().take() {
                    match res {
                        Ok(val) => {
                            let full = val.as_str().unwrap_or("");
                            let (text, truncated) = if full.len() > MAX_TEXT_BYTES {
                                let mut end = MAX_TEXT_BYTES;
                                while !full.is_char_boundary(end) {
                                    end -= 1;
                                }
                                (&full[..end], true)
                            } else {
                                (full, false)
                            };
                            cb(json!({
                                "text": text,
                                "truncated": truncated,
                                "url": url,
                                "title": title,
                            }));
                        }
                        Err(err) => cb(json!({ "error": err })),
                    }
                }
            });
        }
        "eval" => {
            let Some(tab_id) = parse_tab_id(&req) else {
                reply(json!({ "error": "falta 'id' numérico de pestaña" }));
                return;
            };
            let Some(js) = req.get("js").and_then(Value::as_str) else {
                reply(json!({ "error": "falta 'js'" }));
                return;
            };
            let Some(win) = find_tab_window(tab_id) else {
                reply(json!({ "error": format!("pestaña no encontrada: {tab_id}") }));
                return;
            };
            let Some(bv) = win.ensure_tab_awake(tab_id) else {
                reply(json!({ "error": format!("pestaña {tab_id} no tiene vista web") }));
                return;
            };
            let reply_cell = Rc::new(RefCell::new(Some(reply)));
            let reply_to = Rc::clone(&reply_cell);
            glib::timeout_add_local_once(Duration::from_secs(10), move || {
                if let Some(cb) = reply_to.borrow_mut().take() {
                    cb(json!({ "error": "tiempo agotado en eval" }));
                }
            });
            bv.eval_js_async(js, move |res| {
                if let Some(cb) = reply_cell.borrow_mut().take() {
                    match res {
                        Ok(value) => cb(json!({ "value": value })),
                        Err(err) => cb(json!({ "error": err })),
                    }
                }
            });
        }
        "click" | "type" | "submit" => {
            let Some(tab_id) = parse_tab_id(&req) else {
                reply(json!({ "error": "falta 'id' numérico de pestaña" }));
                return;
            };
            let Some(selector) = req.get("selector").and_then(Value::as_str) else {
                reply(json!({ "error": "falta 'selector'" }));
                return;
            };
            let Some(win) = find_tab_window(tab_id) else {
                reply(json!({ "error": format!("pestaña no encontrada: {tab_id}") }));
                return;
            };
            let Some(bv) = win.ensure_tab_awake(tab_id) else {
                reply(json!({ "error": format!("pestaña {tab_id} no tiene vista web") }));
                return;
            };
            let sel_json = serde_json::to_string(selector).unwrap_or_else(|_| "\"\"".to_string());
            let js = match verb {
                "click" => format!(
                    "(() => {{ const el = document.querySelector({sel_json}); if (!el) throw new Error('selector not found'); el.click(); return true; }})()"
                ),
                "type" => {
                    let text_json = serde_json::to_string(
                        req.get("text").and_then(Value::as_str).unwrap_or(""),
                    )
                    .unwrap_or_else(|_| "\"\"".to_string());
                    format!(
                        "(() => {{ const el = document.querySelector({sel_json}); if (!el) throw new Error('selector not found'); el.focus(); el.value = {text_json}; el.dispatchEvent(new Event('input', {{ bubbles: true }})); el.dispatchEvent(new Event('change', {{ bubbles: true }})); return true; }})()"
                    )
                }
                _ => format!(
                    "(() => {{ const el = document.querySelector({sel_json}); if (!el) throw new Error('selector not found'); if (el.form) el.form.submit(); else if (typeof el.submit === 'function') el.submit(); return true; }})()"
                ),
            };
            let reply_cell = Rc::new(RefCell::new(Some(reply)));
            let reply_to = Rc::clone(&reply_cell);
            glib::timeout_add_local_once(Duration::from_secs(5), move || {
                if let Some(cb) = reply_to.borrow_mut().take() {
                    cb(json!({ "error": "tiempo agotado en acción DOM" }));
                }
            });
            bv.eval_js_async(&js, move |res| {
                if let Some(cb) = reply_cell.borrow_mut().take() {
                    match res {
                        Ok(_) => cb(json!({ "ok": true })),
                        Err(err) => cb(json!({ "error": err })),
                    }
                }
            });
        }
        "find" => {
            let Some(text) = req.get("text").and_then(Value::as_str) else {
                reply(json!({ "error": "falta 'text'" }));
                return;
            };
            let Some(win) = target_window(win_idx) else {
                reply(json!({ "error": "no hay ventana activa" }));
                return;
            };
            let Some(bv) = win.ensure_active_browser_view() else {
                reply(json!({ "error": "la pestaña activa no tiene vista web" }));
                return;
            };
            let match_case = req.get("case").and_then(Value::as_bool).unwrap_or(false);
            let is_back = req.get("back").is_some();
            let is_next = req.get("next").is_some();
            let forward = !is_back;
            let find_next = is_next || is_back;

            let started = Instant::now();
            let reply_cell = Rc::new(RefCell::new(Some(reply)));
            let reply_to = Rc::clone(&reply_cell);
            glib::timeout_add_local_once(Duration::from_secs(5), move || {
                if let Some(cb) = reply_to.borrow_mut().take() {
                    cb(json!({ "error": "tiempo agotado en find" }));
                }
            });
            bv.find_async(
                text,
                forward,
                match_case,
                find_next,
                move |ordinal, count| {
                    if let Some(cb) = reply_cell.borrow_mut().take() {
                        cb(json!({
                            "status": "ok",
                            "count": count.max(0) as u32,
                            "index": ordinal.max(0) as u32,
                            "ms": started.elapsed().as_millis() as u64,
                        }));
                    }
                },
            );
        }
        "shot" => {
            let Some(tab_id) = parse_tab_id(&req) else {
                reply(json!({ "error": "falta 'id' numérico de pestaña" }));
                return;
            };
            let Some(win) = find_tab_window(tab_id) else {
                reply(json!({ "error": format!("pestaña no encontrada: {tab_id}") }));
                return;
            };
            let Some(bv) = win.tab_browser_view(tab_id) else {
                reply(json!({ "error": format!("pestaña {tab_id} no está despierta") }));
                return;
            };
            let path = req
                .get("path")
                .and_then(Value::as_str)
                .map(PathBuf::from)
                .unwrap_or_else(|| ctx.runtime_dir.join(format!("shot-{tab_id}.png")));
            match bv.snapshot_png(&path) {
                Ok((w, h)) => reply(json!({
                    "path": path.to_string_lossy(),
                    "width": w,
                    "height": h,
                })),
                Err(err) => reply(json!({ "error": err.to_string() })),
            }
        }
        "probe" => {
            let mem = measure_process_tree_rss_kb(std::process::id()).ok();
            let windows_count = ctx.windows.borrow().len();
            let total_tabs: usize = ctx.windows.borrow().iter().map(|w| w.tab_count()).sum();
            let active_tab = target_window(win_idx)
                .and_then(|w| w.selected_tab_id().and_then(|tid| w.tab_summary(tid)));
            reply(json!({
                "windows": windows_count,
                "tabs": total_tabs,
                "active_tab": active_tab,
                "active_cef": crate::browser_view::active_browser_count(),
                "process_count": mem.as_ref().map_or(1, |m| m.process_count),
                "rss_kb": mem.as_ref().map_or(0, |m| m.rss_kb),
                "startup_ms": ctx.startup_instant.elapsed().as_millis() as u64,
            }));
        }
        "pin" => {
            let Some(tab_id) = parse_tab_id(&req) else {
                reply(json!({ "error": "falta 'id' numérico de pestaña" }));
                return;
            };
            let Some(win) = find_tab_window(tab_id) else {
                reply(json!({ "error": format!("pestaña no encontrada: {tab_id}") }));
                return;
            };
            let off = req.get("off").and_then(Value::as_bool).unwrap_or(false);
            if off {
                win.unpin_tab(tab_id);
            } else if !win.tab_summary(tab_id).is_some_and(|s| s.pinned) {
                win.toggle_pin_tab(tab_id);
            }
            let pinned_state = if win.tab_summary(tab_id).is_some_and(|s| s.pinned) {
                "pinned"
            } else {
                "unpinned"
            };
            reply(json!({
                "pin": pinned_state,
                "pinned": win.pinned_count(),
            }));
        }
        "split" => {
            handle_split_command(ctx, &req, target_window(win_idx), find_tab_window, reply);
        }
        "quit" => {
            let quit_fn = Rc::clone(&ctx.quit_app);
            glib::idle_add_local_once(move || {
                quit_fn();
            });
            reply(json!({ "quitting": true }));
        }
        other => {
            reply(json!({ "error": format!("comando desconocido: {other}") }));
        }
    }
}

fn handle_split_command(
    ctx: &BenchDispatchContext,
    req: &Map<String, Value>,
    target_win: Option<crate::window::BrowserWindow>,
    find_tab_window: impl Fn(u64) -> Option<crate::window::BrowserWindow>,
    reply: Box<dyn FnOnce(Value)>,
) {
    let action = req.get("action").and_then(Value::as_str).unwrap_or("state");
    let Some(win) = target_win else {
        reply(json!({ "error": "no hay ventana abierta" }));
        return;
    };

    match action {
        "state" => reply(win.split_state_json()),
        "open" => {
            let url = req
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or("about:blank");
            let fg = req
                .get("foreground")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            match win.open_tab(Some(url), fg, false, false) {
                Ok(tid) => {
                    let mut state = win.split_state_json();
                    if let Some(obj) = state.as_object_mut() {
                        obj.insert("resultID".into(), json!(tid.to_string()));
                        obj.insert("id".into(), json!(tid.to_string()));
                    }
                    reply(state);
                }
                Err(err) => reply(json!({ "error": err.to_string() })),
            }
        }
        "pair" => {
            let Some(dragged) = req
                .get("id")
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<u64>().ok())
            else {
                reply(json!({ "error": "falta 'id' numérico para split pair" }));
                return;
            };
            let Some(with_id) = req
                .get("with")
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<u64>().ok())
            else {
                reply(json!({ "error": "falta 'with' numérico para split pair" }));
                return;
            };
            let side = req.get("side").and_then(Value::as_str).unwrap_or("right");
            if win.pair_split_tabs(dragged, with_id, side) {
                reply(win.split_state_json());
            } else {
                reply(json!({ "error": "no se pudo emparejar las pestañas en Split View" }));
            }
        }
        "focus" => {
            let Some(tid) = parse_tab_id(req) else {
                reply(json!({ "error": "falta 'id' de pestaña" }));
                return;
            };
            let target = find_tab_window(tid).unwrap_or(win);
            target.focus_split_pane(tid);
            reply(target.split_state_json());
        }
        "select" => {
            let Some(tid) = parse_tab_id(req) else {
                reply(json!({ "error": "falta 'id' de pestaña" }));
                return;
            };
            let target = find_tab_window(tid).unwrap_or(win);
            target.select_tab_by_id(tid);
            reply(target.split_state_json());
        }
        "fraction" => {
            let Some(split_id) = req
                .get("split")
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<u64>().ok())
            else {
                reply(json!({ "error": "falta 'split' numérico" }));
                return;
            };
            let Some(frac) = req.get("fraction").and_then(Value::as_f64) else {
                reply(json!({ "error": "falta 'fraction' numérico" }));
                return;
            };
            let _ = win.set_split_fraction(split_id, frac);
            reply(win.split_state_json());
        }
        "even" => {
            if let Some(first_id) = win
                .split_state_json()
                .get("splits")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
                .and_then(|s| s.get("id"))
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<u64>().ok())
            {
                let _ = win.set_split_fraction(first_id, 0.5);
            }
            reply(win.split_state_json());
        }
        "swap" => {
            win.swap_active_split();
            reply(win.split_state_json());
        }
        "detach" => {
            if let Some(tid) = parse_tab_id(req) {
                win.separate_split_for_tab(tid);
            } else {
                win.separate_active_split();
            }
            reply(win.split_state_json());
        }
        "close" => {
            if let Some(tid) = parse_tab_id(req) {
                let _ = win.close_tab_by_id(tid);
            } else {
                win.close_active_split_pane();
            }
            reply(win.split_state_json());
        }
        "save" => {
            (ctx.save_session)();
            reply(win.split_state_json());
        }
        other => reply(json!({ "error": format!("acción split desconocida: {other}") })),
    }
}

fn parse_tab_id(req: &Map<String, Value>) -> Option<u64> {
    req.get("id").and_then(|v| {
        v.as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .or_else(|| v.as_u64())
    })
}
