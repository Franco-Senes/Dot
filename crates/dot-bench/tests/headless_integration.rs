//! Suite de tests de integración headless en Rust puro (cero Python).
//!
//! Se ejecuta bajo `.toolchain/sdk-xvfb` mediante `scripts/test-headless.sh`
//! (`cargo test -p dot-bench --test headless_integration -- --ignored --nocapture`).

use dot_bench::{run_find_suite, run_split_view_suite, BenchClient, HttpResponse, TestHttpServer};
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

static TEST_SEQ: AtomicUsize = AtomicUsize::new(1);

struct TempXdg {
    root: PathBuf,
    config_home: PathBuf,
    data_home: PathBuf,
    cache_home: PathBuf,
    runtime_dir: PathBuf,
}

impl TempXdg {
    fn new(prefix: &str) -> Self {
        let seq = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "dot-headless-{prefix}-{}-{seq}",
            std::process::id()
        ));
        let config_home = root.join("config");
        let data_home = root.join("data");
        let cache_home = root.join("cache");
        let runtime_dir = root.join("runtime");
        for dir in [&config_home, &data_home, &cache_home, &runtime_dir] {
            fs::create_dir_all(dir).expect("create xdg dir");
        }
        fs::set_permissions(&runtime_dir, fs::Permissions::from_mode(0o700))
            .expect("chmod 0700 runtime_dir");
        Self {
            root,
            config_home,
            data_home,
            cache_home,
            runtime_dir,
        }
    }

    fn socket_path(&self) -> PathBuf {
        self.runtime_dir.join("dot/bench.sock")
    }

    fn apply_env(&self, cmd: &mut Command) {
        cmd.env("XDG_CONFIG_HOME", &self.config_home)
            .env("XDG_DATA_HOME", &self.data_home)
            .env("XDG_CACHE_HOME", &self.cache_home)
            .env("XDG_RUNTIME_DIR", &self.runtime_dir)
            .env("NO_AT_BRIDGE", "1");
    }
}

impl Drop for TempXdg {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct ChildGuard {
    child: Option<Child>,
}

impl ChildGuard {
    fn spawn(mut cmd: Command) -> Self {
        let child = cmd.spawn().expect("spawn search process");
        Self { child: Some(child) }
    }

    fn wait_exit(&mut self, timeout: Duration) -> bool {
        let Some(ref mut child) = self.child else {
            return true;
        };
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Ok(Some(_)) = child.try_wait() {
                self.child = None;
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        false
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn resolve_dot_bin() -> PathBuf {
    if let Some(explicit) = dot_core::env::var("DOT_BIN", "SEARCH_BIN") {
        let p = PathBuf::from(explicit);
        if p.is_file() {
            return p;
        }
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    for profile in ["release", "debug"] {
        let candidate = workspace_root.join("target").join(profile).join("dot");
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!("No se encontró el binario 'dot'; define DOT_BIN o compila el workspace");
}

fn wait_for_socket(path: &Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.exists() {
            if let Ok(resp) = BenchClient::new(path).send_value(json!({ "do": "probe" })) {
                if resp.get("windows").and_then(Value::as_u64).unwrap_or(0) >= 1 {
                    return;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!(
        "Tiempo agotado esperando a que '{}' responda a probe",
        path.display()
    );
}

#[test]
#[ignore = "requiere entorno gráfico headless (ejecutar vía scripts/test-headless.sh)"]
fn headless_split_view_find_and_bench_commands() {
    let bin = resolve_dot_bin();
    let xdg = TempXdg::new("split-find");
    let srv = TestHttpServer::start().expect("start TestHttpServer");
    srv.set_route(
        "/dom",
        HttpResponse::html(
            "<!doctype html><title>DOM Test</title>\
             <input id='inp' value=''>\
             <button id='btn' onclick=\"document.title = document.getElementById('inp').value || 'clicked'\">Go</button>",
        ),
    );

    let mut cmd = Command::new(&bin);
    xdg.apply_env(&mut cmd);
    cmd.env("DOT_BENCH", "1")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let mut proc_guard = ChildGuard::spawn(cmd);

    let sock = xdg.socket_path();
    wait_for_socket(&sock, Duration::from_secs(20));

    // Verificar permisos 0600 en bench.sock.
    let mode = fs::metadata(&sock)
        .expect("socket metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600, "bench.sock debe tener permisos 0600");

    let client = BenchClient::new(&sock);

    // 1. Suite de Split View (port de upstream/Tests/split_view.py).
    let split_report =
        run_split_view_suite(&client, &srv.base_url()).expect("run_split_view_suite");
    for msg in &split_report.messages {
        println!("[split] {msg}");
    }
    assert!(split_report.is_ok(), "fallos en run_split_view_suite");

    // 2. Suite de búsqueda en página (test-find).
    let fixture_url = format!("{}/find-fixture", srv.base_url());
    let find_report = run_find_suite(&client, &fixture_url).expect("run_find_suite");
    for msg in &find_report.messages {
        println!("[find] {msg}");
    }
    assert!(find_report.is_ok(), "fallos en run_find_suite");

    // 3. Comandos DOM, eval, text, shot y probe.
    let dom_url = format!("{}/dom", srv.base_url());
    let opened = client
        .send_value(json!({ "do": "open", "url": dom_url }))
        .expect("open /dom");
    let tab_id = opened
        .get("id")
        .and_then(Value::as_str)
        .expect("tab id")
        .to_owned();
    let _ = client
        .send_value(json!({ "do": "wait", "id": tab_id, "seconds": 15.0 }))
        .expect("wait /dom");

    let _ = client
        .send_value(json!({ "do": "type", "id": tab_id, "selector": "#inp", "text": "RustOK" }))
        .expect("type");
    let _ = client
        .send_value(json!({ "do": "click", "id": tab_id, "selector": "#btn" }))
        .expect("click");
    let eval_res = client
        .send_value(json!({ "do": "eval", "id": tab_id, "js": "document.title" }))
        .expect("eval title");
    assert_eq!(
        eval_res.get("value").and_then(Value::as_str),
        Some("RustOK")
    );

    let text_res = client
        .send_value(json!({ "do": "text", "id": tab_id }))
        .expect("text");
    assert!(text_res
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .contains("Go"));

    let shot_path = xdg.root.join("shot.png");
    let shot_res = client
        .send_value(json!({ "do": "shot", "id": tab_id, "path": shot_path }))
        .expect("shot");
    assert!(shot_res.get("width").and_then(Value::as_i64).unwrap_or(0) > 0);
    let png_bytes = fs::read(&shot_path).expect("read shot png");
    assert!(png_bytes.starts_with(b"\x89PNG\r\n\x1a\n"));

    let probe = client.send_value(json!({ "do": "probe" })).expect("probe");
    assert!(probe.get("rss_kb").and_then(Value::as_u64).unwrap_or(0) > 0);

    let _ = client.send_value(json!({ "do": "quit" })).expect("quit");
    assert!(
        proc_guard.wait_exit(Duration::from_secs(10)),
        "search no salió limpiamente tras quit"
    );
}

#[test]
#[ignore = "requiere entorno gráfico headless (ejecutar vía scripts/test-headless.sh)"]
fn headless_session_open_tabs_save_and_restore() {
    let bin = resolve_dot_bin();
    let xdg = TempXdg::new("session");
    let srv = TestHttpServer::start().expect("start TestHttpServer");
    let sock = xdg.socket_path();

    // Primera ejecución: abrir 3 pestañas locales y salir limpiamente.
    {
        let mut cmd = Command::new(&bin);
        xdg.apply_env(&mut cmd);
        cmd.env("DOT_BENCH", "1")
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        let mut guard = ChildGuard::spawn(cmd);
        wait_for_socket(&sock, Duration::from_secs(20));

        let client = BenchClient::new(&sock);
        for slug in ["alpha", "beta", "gamma"] {
            let opened = client
                .send_value(json!({ "do": "open", "url": format!("{}/{slug}", srv.base_url()) }))
                .expect("open tab");
            let tid = opened.get("id").and_then(Value::as_str).expect("id");
            let _ = client
                .send_value(json!({ "do": "wait", "id": tid, "seconds": 15.0 }))
                .expect("wait tab");
        }

        let tabs = client.send_value(json!({ "do": "tabs" })).expect("tabs");
        let count = tabs.get("tabs").and_then(Value::as_array).map(Vec::len);
        assert_eq!(count, Some(4), "deben existir 4 pestañas antes de cerrar");

        let _ = client.send_value(json!({ "do": "quit" })).expect("quit");
        assert!(guard.wait_exit(Duration::from_secs(10)));
    }

    let session_file = xdg.data_home.join("dot/session.json");
    assert!(
        session_file.is_file(),
        "session.json debe existir tras quit en '{}'",
        session_file.display()
    );

    // Segunda ejecución: restaurar sesión y comprobar pestañas + carga diferida.
    {
        let mut cmd = Command::new(&bin);
        xdg.apply_env(&mut cmd);
        cmd.env("DOT_BENCH", "1")
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        let mut guard = ChildGuard::spawn(cmd);
        wait_for_socket(&sock, Duration::from_secs(20));

        let client = BenchClient::new(&sock);
        let tabs = client.send_value(json!({ "do": "tabs" })).expect("tabs");
        let tab_arr = tabs.get("tabs").and_then(Value::as_array).expect("array");
        assert_eq!(tab_arr.len(), 4, "deben restaurarse las 4 pestañas");

        let urls: Vec<&str> = tab_arr
            .iter()
            .filter_map(|t| t.get("url").and_then(Value::as_str))
            .collect();
        for slug in ["alpha", "beta", "gamma"] {
            let expected = format!("{}/{slug}", srv.base_url());
            assert!(
                urls.iter().any(|u| *u == expected),
                "falta URL restaurada '{expected}' en {urls:?}"
            );
        }

        let probe = client.send_value(json!({ "do": "probe" })).expect("probe");
        assert_eq!(
            probe.get("active_cef").and_then(Value::as_u64),
            Some(1),
            "solo la pestaña activa debe tener CEF despierto tras restaurar"
        );

        let _ = client.send_value(json!({ "do": "quit" })).expect("quit");
        assert!(guard.wait_exit(Duration::from_secs(10)));
    }
}

#[test]
#[ignore = "requiere entorno gráfico headless (ejecutar vía scripts/test-headless.sh)"]
fn headless_dbus_and_handles_open_single_instance() {
    let bin = resolve_dot_bin();
    let xdg = TempXdg::new("dbus");
    let srv = TestHttpServer::start().expect("start TestHttpServer");

    // Activar bench por settings.json (sin DOT_BENCH=1) para probar instancia única HANDLES_OPEN + D-Bus.
    let cfg_dir = xdg.config_home.join("dot");
    fs::create_dir_all(&cfg_dir).expect("cfg dir");
    fs::write(cfg_dir.join("settings.json"), r#"{"bench":true}"#).expect("write settings.json");

    let mut cmd = Command::new(&bin);
    xdg.apply_env(&mut cmd);
    cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    let mut guard = ChildGuard::spawn(cmd);

    let sock = xdg.socket_path();
    wait_for_socket(&sock, Duration::from_secs(20));

    // 1. Verificar método D-Bus GetVersion.
    let gdbus_out = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "dev.senes.Dot",
            "--object-path",
            "/dev/senes/Dot1",
            "--method",
            "dev.senes.Dot1.GetVersion",
        ])
        .output()
        .expect("run gdbus GetVersion");
    assert!(
        gdbus_out.status.success(),
        "gdbus GetVersion falló: {}",
        String::from_utf8_lossy(&gdbus_out.stderr)
    );
    let stdout = String::from_utf8_lossy(&gdbus_out.stdout);
    assert!(
        stdout.contains("0.1.0"),
        "GetVersion debe devolver la versión; obtenido: {stdout}"
    );

    // 2. Verificar instancia única HANDLES_OPEN: `search <url>` abre pestaña en la instancia existente.
    let target_url = format!("{}/single-instance", srv.base_url());
    let mut second = Command::new(&bin);
    xdg.apply_env(&mut second);
    second.arg(&target_url);
    let second_status = second.status().expect("run second search <url>");
    assert!(second_status.success(), "segunda invocación debe salir 0");

    let client = BenchClient::new(&sock);
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut found = false;
    while Instant::now() < deadline {
        let tabs = client.send_value(json!({ "do": "tabs" })).expect("tabs");
        if let Some(arr) = tabs.get("tabs").and_then(Value::as_array) {
            if arr
                .iter()
                .any(|t| t.get("url").and_then(Value::as_str) == Some(target_url.as_str()))
            {
                found = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        found,
        "HANDLES_OPEN debe abrir '{target_url}' en la instancia existente"
    );

    let _ = client.send_value(json!({ "do": "quit" })).expect("quit");
    assert!(guard.wait_exit(Duration::from_secs(10)));
}
