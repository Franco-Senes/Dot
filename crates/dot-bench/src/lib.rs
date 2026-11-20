//! Cliente Unix socket `bench.sock`, servidor HTTP local en Rust puro y suites de prueba.
//!
//! Cero Python: sustituye a `upstream/bench`, `upstream/test-find` y el servidor
//! HTTP + cliente de `upstream/Tests/split_view.py` usando exclusivamente Rust.

use anyhow::{Context, Result};
use dot_core::bench_proto::{decode_json_line, encode_json_line, MAX_BENCH_LINE_BYTES};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use thiserror::Error;

/// Tiempo de espera por defecto para comandos sobre `bench.sock`.
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Errores del cliente `bench.sock`.
#[derive(Debug, Error)]
pub enum BenchClientError {
    #[error("Dot no está escuchando en '{path}' — actívalo en Ajustes o inicia con DOT_BENCH=1")]
    NotListening { path: PathBuf },
    #[error("una ventana apareció en pantalla durante una ejecución de prueba: {0}")]
    OnScreenViolation(String),
    #[error("{0}")]
    RemoteError(String),
    #[error("error de protocolo en bench.sock: {0}")]
    Protocol(String),
    #[error("error de E/S en bench.sock: {0}")]
    Io(#[from] std::io::Error),
}

/// Cliente síncrono para el protocolo JSON-lines sobre `$XDG_RUNTIME_DIR/dot/bench.sock`.
#[derive(Debug, Clone)]
pub struct BenchClient {
    socket_path: PathBuf,
    timeout: Duration,
    window: Option<usize>,
}

impl BenchClient {
    /// Crea un cliente apuntando a `socket_path` con timeout por defecto (30 s).
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            window: None,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_window(mut self, window: Option<usize>) -> Self {
        self.window = window;
        self
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Envía una solicitud JSON (inyectando `"window"` si está configurado) y devuelve
    /// el objeto JSON de respuesta tras validar `onScreen` y `error`.
    pub fn send(
        &self,
        mut request: Map<String, Value>,
    ) -> Result<Map<String, Value>, BenchClientError> {
        if let Some(win) = self.window {
            request
                .entry("window".to_owned())
                .or_insert_with(|| json!(win));
        }

        let payload = encode_json_line(&Value::Object(request))
            .map_err(|err| BenchClientError::Protocol(err.to_string()))?;

        let mut stream =
            UnixStream::connect(&self.socket_path).map_err(|err| match err.kind() {
                std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused => {
                    BenchClientError::NotListening {
                        path: self.socket_path.clone(),
                    }
                }
                _ => BenchClientError::Io(err),
            })?;

        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;
        stream.write_all(&payload)?;
        stream.flush()?;

        let mut raw = Vec::new();
        let mut limited = (&mut stream).take((MAX_BENCH_LINE_BYTES + 1) as u64);
        limited.read_to_end(&mut raw)?;
        if raw.len() > MAX_BENCH_LINE_BYTES {
            return Err(BenchClientError::Protocol(
                "la respuesta superó MAX_BENCH_LINE_BYTES".into(),
            ));
        }

        let answer =
            decode_json_line(&raw).map_err(|err| BenchClientError::Protocol(err.to_string()))?;

        if let Some(on_screen) = answer.get("onScreen").and_then(Value::as_array) {
            if !on_screen.is_empty() {
                let joined: Vec<String> = on_screen
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| v.to_string())
                    })
                    .collect();
                return Err(BenchClientError::OnScreenViolation(joined.join("; ")));
            }
        }

        if let Some(err_val) = answer.get("error") {
            let msg = err_val
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| err_val.to_string());
            return Err(BenchClientError::RemoteError(msg));
        }

        Ok(answer)
    }

    /// Envía una solicitud JSON a partir de un `serde_json::Value` que debe ser objeto.
    pub fn send_value(&self, request: Value) -> Result<Map<String, Value>, BenchClientError> {
        match request {
            Value::Object(map) => self.send(map),
            _ => Err(BenchClientError::Protocol(
                "la solicitud debe ser un objeto JSON".into(),
            )),
        }
    }
}

/// Respuesta HTTP simple para `TestHttpServer`.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status_code: u16,
    pub status_text: String,
    pub content_type: String,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn html(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status_code: 200,
            status_text: "OK".into(),
            content_type: "text/html; charset=utf-8".into(),
            body: body.into(),
        }
    }

    pub fn text(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status_code: 200,
            status_text: "OK".into(),
            content_type: "text/plain; charset=utf-8".into(),
            body: body.into(),
        }
    }

    pub fn rss(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status_code: 200,
            status_text: "OK".into(),
            content_type: "application/rss+xml; charset=utf-8".into(),
            body: body.into(),
        }
    }
}

/// Servidor HTTP local multihilo en Rust puro (`std::net::TcpListener`, cero Python).
///
/// Sirve las rutas de prueba de `upstream/Tests/split_view.py`, el fixture de `PageFind`
/// (`/find-fixture`) y cualquier ruta personalizada registrada dinámicamente.
pub struct TestHttpServer {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    routes: Arc<RwLock<HashMap<String, HttpResponse>>>,
    feed_posts: Arc<AtomicUsize>,
    handle: Option<JoinHandle<()>>,
}

impl TestHttpServer {
    /// Arranca el servidor en `127.0.0.1:0` (puerto efímero).
    pub fn start() -> Result<Self> {
        let listener =
            TcpListener::bind("127.0.0.1:0").context("no se pudo abrir TcpListener local")?;
        let addr = listener
            .local_addr()
            .context("no se pudo consultar la dirección local del servidor HTTP")?;
        listener
            .set_nonblocking(true)
            .context("no se pudo poner TcpListener en modo no bloqueante")?;

        let stop = Arc::new(AtomicBool::new(false));
        let routes: Arc<RwLock<HashMap<String, HttpResponse>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let feed_posts = Arc::new(AtomicUsize::new(3));

        let stop_bg = Arc::clone(&stop);
        let routes_bg = Arc::clone(&routes);
        let feed_bg = Arc::clone(&feed_posts);
        let port = addr.port();

        let handle = thread::spawn(move || {
            while !stop_bg.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let routes_req = Arc::clone(&routes_bg);
                        let feed_req = Arc::clone(&feed_bg);
                        thread::spawn(move || {
                            let _ = handle_http_connection(stream, port, &routes_req, &feed_req);
                        });
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            addr,
            stop,
            routes,
            feed_posts,
            handle: Some(handle),
        })
    }

    /// URL base del servidor, p. ej. `http://127.0.0.1:41234`.
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Registra o sustituye una ruta exacta (p. ej. `"/custom"`).
    pub fn set_route(&self, path: impl Into<String>, response: HttpResponse) {
        if let Ok(mut guard) = self.routes.write() {
            guard.insert(path.into(), response);
        }
    }

    /// Ajusta cuántos elementos sirve `/feed.xml` (como `FEED_POSTS` en `split_view.py`).
    pub fn set_feed_posts(&self, count: usize) {
        self.feed_posts.store(count, Ordering::Relaxed);
    }
}

impl Drop for TestHttpServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_millis(100));
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn handle_http_connection(
    mut stream: TcpStream,
    port: u16,
    routes: &RwLock<HashMap<String, HttpResponse>>,
    feed_posts: &AtomicUsize,
) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(());
    }

    // Drenar encabezados hasta línea en blanco.
    let mut header_line = String::new();
    loop {
        header_line.clear();
        let n = reader.read_line(&mut header_line)?;
        if n == 0 || header_line == "\r\n" || header_line == "\n" {
            break;
        }
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let raw_path = parts.next().unwrap_or("/");
    let path = raw_path.split('?').next().unwrap_or("/");

    let response = if method != "GET" && method != "HEAD" {
        HttpResponse {
            status_code: 405,
            status_text: "Method Not Allowed".into(),
            content_type: "text/plain; charset=utf-8".into(),
            body: b"Method Not Allowed".to_vec(),
        }
    } else if let Some(custom) = routes.read().ok().and_then(|map| map.get(path).cloned()) {
        custom
    } else {
        default_route_response(path, port, feed_posts.load(Ordering::Relaxed))
    };

    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status_code,
        response.status_text,
        response.content_type,
        response.body.len()
    )?;
    if method != "HEAD" {
        stream.write_all(&response.body)?;
    }
    stream.flush()?;
    Ok(())
}

fn default_route_response(path: &str, port: u16, feed_posts: usize) -> HttpResponse {
    match path {
        "/asker" => HttpResponse::html(format!(
            "<!doctype html><title>asker</title><iframe src='http://127.0.0.1:{port}/frame'></iframe>"
        )),
        "/long" => HttpResponse::html(
            "<!doctype html><title>long</title><div style='height:20000px'>long</div>",
        ),
        "/tall" => HttpResponse::html(
            "<!doctype html><title>tall</title><style>body{margin:0}</style>\
             <div id=top style='height:4800px;background:#fff'>\
             <div id=box style='margin:40px;width:300px;height:150px;background:#0a0'></div></div>\
             <div style='height:200px;background:#f00'></div>",
        ),
        "/frame" => HttpResponse::html(
            "<!doctype html><script>setTimeout(function(){ alert('from the frame') }, 1200)</script>",
        ),
        "/feed.xml" => {
            let mut items = String::new();
            for n in (1..=feed_posts).rev() {
                items.push_str(&format!(
                    "<item><title>Post {n}</title><link>/post/{n}</link></item>"
                ));
            }
            HttpResponse::rss(format!(
                "<?xml version='1.0'?><rss version='2.0'><channel><title>Probe Feed</title>{items}</channel></rss>"
            ))
        }
        "/blog" => HttpResponse::html(
            "<!doctype html><title>blog</title><link rel='alternate' type='application/rss+xml' href='/feed.xml'><p>blog",
        ),
        "/find-fixture" => HttpResponse::html(page_find_fixture_html()),
        other => {
            let clean_title = escape_html(other.trim_matches('/'));
            let clean_path = escape_html(other);
            HttpResponse::html(format!(
                "<!doctype html><title>{clean_title}</title><p>{clean_path}</p>"
            ))
        }
    }
}

fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Fixture HTML portado directamente de `upstream/Tests/PageFindHarness.swift`.
pub fn page_find_fixture_html() -> String {
    let seventeen = format!(
        "{}<span>or</span><b>chid</b>",
        "<span>orchid</span>".repeat(16)
    );
    let long_text = "filler words keep this paragraph well beyond the viewport. ".repeat(220);
    let side_filler = "side filler ".repeat(70);
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><style>\
         html, body {{ margin: 0; font: 16px sans-serif; }}\
         #long {{ width: 240px; }}\
         </style></head><body>\
         <div id=\"seventeen\">{seventeen}</div>\
         <p id=\"case\">Alpha alpha ALPHA</p>\
         <p id=\"words\">cat scatter concatenate</p>\
         <p id=\"unicode\">ÄÖÜ äöü café CAFÉ cafe\u{0301} cafeine</p>\
         <p id=\"literal\">A \"quote\" and path\\segment</p>\
         <div hidden>ghostword</div><div style=\"display:none\">ghostword</div>\
         <div style=\"visibility:hidden\">ghostword</div><span>ghostword</span>\
         <input id=\"field\" value=\"fieldpivot\">\
         <textarea id=\"area\">areapivot</textarea>\
         <p><i id=\"mainoriginal\">mainoriginal</i> <span id=\"sharedmain\">sharedmark</span></p>\
         <input id=\"sharedcontrol\" value=\"sharedmark\">\
         <iframe id=\"frame\" srcdoc=\"&lt;p&gt;&lt;i id='original'&gt;originalframe&lt;/i&gt; frameword &lt;span&gt;sharedmark&lt;/span&gt;&lt;/p&gt;\"></iframe>\
         <div id=\"dynamic\"><span>dynamicword</span></div>\
         <div id=\"live\" style=\"display:none\">livepivot</div>\
         <div id=\"horizontal\" style=\"width:180px;height:48px;overflow:auto;white-space:nowrap\"><span>{side_filler} horizontalword</span></div>\
         <p id=\"long\">{long_text} farendword</p>\
         </body></html>"
    )
}

/// Resultado agregado de una batería de comprobaciones sobre `bench.sock`.
#[derive(Debug, Default, Clone)]
pub struct SuiteReport {
    pub passed: usize,
    pub failed: usize,
    pub messages: Vec<String>,
}

impl SuiteReport {
    pub fn check(&mut self, cond: bool, label: impl Into<String>) {
        let label = label.into();
        if cond {
            self.passed += 1;
            self.messages.push(format!("PASS: {label}"));
        } else {
            self.failed += 1;
            self.messages.push(format!("FAIL: {label}"));
        }
    }

    pub fn total(&self) -> usize {
        self.passed + self.failed
    }

    pub fn is_ok(&self) -> bool {
        self.failed == 0
    }
}

/// Ejecuta la batería de verificación de búsqueda en página (`test-find`) sobre `bench.sock`.
pub fn run_find_suite(client: &BenchClient, fixture_url: &str) -> Result<SuiteReport> {
    let mut report = SuiteReport::default();

    let opened = client
        .send_value(json!({ "do": "open", "url": fixture_url }))
        .context("no se pudo abrir la pestaña de pruebas para test-find")?;
    let tab_id = opened
        .get("id")
        .and_then(Value::as_str)
        .context("respuesta de open sin campo 'id'")?
        .to_owned();

    let run_result = (|| -> Result<()> {
        let _ = client.send_value(json!({ "do": "wait", "id": tab_id, "seconds": 15.0 }))?;
        let _ = client.send_value(json!({ "do": "select", "id": tab_id }));

        let first = client.send_value(json!({ "do": "find", "text": "orchid" }))?;
        let count = first.get("count").and_then(Value::as_u64).unwrap_or(0);
        report.check(
            count == 17,
            format!(
                "orchid encuentra 17 coincidencias incluyendo nodos divididos (obtenido {count})"
            ),
        );

        let next = client.send_value(json!({ "do": "find", "text": "orchid", "next": 1 }))?;
        let next_count = next.get("count").and_then(Value::as_u64).unwrap_or(0);
        report.check(
            next_count == 17,
            format!("orchid next 1 mantiene 17 coincidencias (obtenido {next_count})"),
        );

        let back = client.send_value(json!({ "do": "find", "text": "orchid", "back": 1 }))?;
        let back_count = back.get("count").and_then(Value::as_u64).unwrap_or(0);
        report.check(
            back_count == 17,
            format!("orchid back 1 mantiene 17 coincidencias (obtenido {back_count})"),
        );

        let ci = client.send_value(json!({ "do": "find", "text": "alpha" }))?;
        let ci_count = ci.get("count").and_then(Value::as_u64).unwrap_or(0);
        report.check(
            ci_count == 3,
            format!("alpha sin distinguir mayúsculas encuentra 3 (obtenido {ci_count})"),
        );

        let cs = client.send_value(json!({ "do": "find", "text": "alpha", "case": true }))?;
        let cs_count = cs.get("count").and_then(Value::as_u64).unwrap_or(0);
        report.check(
            cs_count == 1,
            format!("alpha con case=true encuentra 1 (obtenido {cs_count})"),
        );

        let hidden = client.send_value(json!({ "do": "find", "text": "ghostword" }))?;
        let hidden_count = hidden.get("count").and_then(Value::as_u64).unwrap_or(0);
        report.check(
            hidden_count == 1,
            format!(
                "ghostword excluye texto oculto y encuentra 1 visible (obtenido {hidden_count})"
            ),
        );

        let literal = client.send_value(json!({
            "do": "find",
            "text": "A \"quote\" and path\\segment"
        }))?;
        let lit_count = literal.get("count").and_then(Value::as_u64).unwrap_or(0);
        report.check(
            lit_count == 1,
            format!("comillas y barras invertidas literales encuentran 1 (obtenido {lit_count})"),
        );

        let absent = client.send_value(json!({ "do": "find", "text": "not-on-this-page" }))?;
        let absent_count = absent
            .get("count")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX);
        report.check(
            absent_count == 0,
            format!("cadena ausente devuelve 0 (obtenido {absent_count})"),
        );

        Ok(())
    })();

    let _ = client.send_value(json!({ "do": "close", "id": tab_id }));
    run_result?;
    Ok(report)
}

/// Port en Rust puro de las comprobaciones principales de `upstream/Tests/split_view.py`
/// (`case_slice1`: emparejar, foco, fracción, selección externa y guardado).
pub fn run_split_view_suite(client: &BenchClient, base_url: &str) -> Result<SuiteReport> {
    let mut report = SuiteReport::default();

    let open_page = |name: &str| -> Result<String> {
        let resp = client.send_value(json!({
            "do": "split",
            "action": "open",
            "url": format!("{base_url}/{name}")
        }))?;
        resp.get("resultID")
            .or_else(|| resp.get("id"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .context("respuesta de split open sin resultID/id")
    };

    let a = open_page("a")?;
    let b = open_page("b")?;
    let c = open_page("c")?;

    let paired = client.send_value(json!({
        "do": "split",
        "action": "pair",
        "id": b,
        "with": a,
        "side": "right"
    }))?;

    let first_split = paired
        .get("splits")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .cloned()
        .unwrap_or(Value::Null);

    let tabs_match = first_split.get("tabs") == Some(&json!([a, b]));
    let axis_match = first_split.get("axis").and_then(Value::as_str) == Some("horizontal");
    let sizes_match = first_split.get("sizes") == Some(&json!([0.5, 0.5]));
    report.check(
        tabs_match && axis_match && sizes_match,
        "pair crea [a, b] horizontal 50/50",
    );
    report.check(
        first_split.get("focused").and_then(Value::as_str) == Some(b.as_str()),
        "el foco inicial queda en la pestaña arrastrada (b)",
    );

    let focused = client.send_value(json!({
        "do": "split",
        "action": "focus",
        "id": a
    }))?;
    let focused_id = focused
        .get("splits")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|s| s.get("focused"))
        .and_then(Value::as_str);
    report.check(focused_id == Some(a.as_str()), "focus recuerda el panel a");

    if let Some(split_id) = first_split.get("id").and_then(Value::as_str) {
        let _ = client.send_value(json!({
            "do": "split",
            "action": "fraction",
            "split": split_id,
            "fraction": 0.3
        }))?;
    }

    let after_select = client.send_value(json!({
        "do": "split",
        "action": "select",
        "id": c
    }))?;
    let kept_focus = after_select
        .get("splits")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|s| s.get("focused"))
        .and_then(Value::as_str);
    report.check(
        kept_focus == Some(a.as_str()),
        "salir hacia otra pestaña conserva el foco del par",
    );

    let _ = client.send_value(json!({ "do": "split", "action": "save" }))?;
    let _ = client.send_value(json!({ "do": "split", "action": "detach", "id": b }));
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::net::UnixListener;

    static MOCK_SEQ: AtomicUsize = AtomicUsize::new(1);

    struct MockBenchServer {
        socket_path: PathBuf,
        stop: Arc<AtomicBool>,
        handle: Option<JoinHandle<()>>,
    }

    impl MockBenchServer {
        fn start<F>(handler: F) -> Self
        where
            F: Fn(Map<String, Value>) -> Value + Send + Sync + 'static,
        {
            let seq = MOCK_SEQ.fetch_add(1, Ordering::Relaxed);
            let socket_path = std::env::temp_dir().join(format!(
                "search-bench-mock-{}-{seq}.sock",
                std::process::id()
            ));
            let _ = fs::remove_file(&socket_path);
            let listener = UnixListener::bind(&socket_path).expect("bind unix socket");
            listener.set_nonblocking(true).expect("nonblocking");

            let stop = Arc::new(AtomicBool::new(false));
            let stop_bg = Arc::clone(&stop);
            let handler = Arc::new(handler);

            let handle = thread::spawn(move || {
                while !stop_bg.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let mut line = String::new();
                            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                            if reader.read_line(&mut line).is_ok() && !line.is_empty() {
                                let req = decode_json_line(line.as_bytes()).unwrap_or_default();
                                let resp = handler(req);
                                let wire = encode_json_line(&resp).expect("encode resp");
                                let _ = stream.write_all(&wire);
                                let _ = stream.flush();
                            }
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => break,
                    }
                }
            });

            Self {
                socket_path,
                stop,
                handle: Some(handle),
            }
        }
    }

    impl Drop for MockBenchServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            let _ = UnixStream::connect(&self.socket_path);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
            let _ = fs::remove_file(&self.socket_path);
        }
    }

    fn http_get(addr: &str, path: &str) -> String {
        let mut stream = TcpStream::connect(addr).expect("connect http");
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
        )
        .expect("write req");
        let mut resp = String::new();
        stream.read_to_string(&mut resp).expect("read resp");
        resp
    }

    #[test]
    fn test_http_server_serves_builtin_and_custom_routes() {
        let srv = TestHttpServer::start().expect("start http server");
        let addr = format!("127.0.0.1:{}", srv.port());

        let hello = http_get(&addr, "/alpha");
        assert!(hello.starts_with("HTTP/1.1 200 OK"));
        assert!(hello.contains("<title>alpha</title>"));

        let fixture = http_get(&addr, "/find-fixture");
        assert!(fixture.contains("ghostword"));
        assert!(fixture.contains("Alpha alpha ALPHA"));

        srv.set_feed_posts(5);
        let feed = http_get(&addr, "/feed.xml");
        assert!(feed.contains("application/rss+xml"));
        assert!(feed.contains("Post 5"));

        srv.set_route("/custom", HttpResponse::text("hola-rust"));
        let custom = http_get(&addr, "/custom");
        assert!(custom.contains("hola-rust"));
    }

    #[test]
    fn bench_client_roundtrip_and_error_handling() {
        let mock = MockBenchServer::start(|req| {
            let verb = req.get("do").and_then(Value::as_str).unwrap_or("");
            match verb {
                "tabs" => json!({
                    "tabs": [
                        {"id": "t1", "title": "Example", "url": "https://example.com", "active": true}
                    ]
                }),
                "boom" => json!({ "error": "fallo controlado" }),
                "screen" => json!({ "onScreen": ["MainWindow"] }),
                _ => json!({ "ok": true, "echo": verb }),
            }
        });

        let client = BenchClient::new(&mock.socket_path);
        let tabs = client.send_value(json!({ "do": "tabs" })).expect("tabs");
        assert_eq!(
            tabs.get("tabs").and_then(Value::as_array).map(Vec::len),
            Some(1)
        );

        let err = client.send_value(json!({ "do": "boom" }));
        assert!(
            matches!(err, Err(BenchClientError::RemoteError(msg)) if msg == "fallo controlado")
        );

        let screen = client.send_value(json!({ "do": "screen" }));
        assert!(matches!(
            screen,
            Err(BenchClientError::OnScreenViolation(_))
        ));
    }

    #[test]
    fn find_and_split_suites_pass_against_mock_server() {
        let srv = TestHttpServer::start().expect("http server");
        let split_focus = Arc::new(RwLock::new(String::from("b")));
        let split_focus_bg = Arc::clone(&split_focus);
        let tab_seq = Arc::new(AtomicUsize::new(0));
        let tab_seq_bg = Arc::clone(&tab_seq);

        let mock = MockBenchServer::start(move |req| {
            let verb = req.get("do").and_then(Value::as_str).unwrap_or("");
            match verb {
                "open" => {
                    json!({ "id": "tab-find-1", "url": req.get("url").cloned().unwrap_or(Value::Null) })
                }
                "wait" | "select" | "close" => json!({ "ok": true }),
                "find" => {
                    let text = req.get("text").and_then(Value::as_str).unwrap_or("");
                    let case = req.get("case").and_then(Value::as_bool).unwrap_or(false);
                    let count = match (text, case) {
                        ("orchid", _) => 17,
                        ("alpha", false) => 3,
                        ("alpha", true) => 1,
                        ("ghostword", _) => 1,
                        ("A \"quote\" and path\\segment", _) => 1,
                        _ => 0,
                    };
                    json!({ "status": format!("1 / {count}"), "count": count, "ms": 2 })
                }
                "split" => {
                    let action = req.get("action").and_then(Value::as_str).unwrap_or("state");
                    match action {
                        "open" => {
                            let idx = tab_seq_bg.fetch_add(1, Ordering::Relaxed);
                            let id = ["a", "b", "c"][idx % 3];
                            json!({ "resultID": id })
                        }
                        "pair" => {
                            if let Ok(mut g) = split_focus_bg.write() {
                                *g = "b".into();
                            }
                            json!({
                                "splits": [{
                                    "id": "sp-1",
                                    "tabs": ["a", "b"],
                                    "axis": "horizontal",
                                    "sizes": [0.5, 0.5],
                                    "focused": "b"
                                }]
                            })
                        }
                        "focus" => {
                            let target = req.get("id").and_then(Value::as_str).unwrap_or("a");
                            if let Ok(mut g) = split_focus_bg.write() {
                                *g = target.to_owned();
                            }
                            json!({
                                "splits": [{
                                    "id": "sp-1",
                                    "tabs": ["a", "b"],
                                    "axis": "horizontal",
                                    "sizes": [0.5, 0.5],
                                    "focused": target
                                }]
                            })
                        }
                        "select" | "fraction" | "save" | "detach" | "state" => {
                            let cur = split_focus_bg
                                .read()
                                .map(|g| g.clone())
                                .unwrap_or_else(|_| "a".into());
                            json!({
                                "splits": [{
                                    "id": "sp-1",
                                    "tabs": ["a", "b"],
                                    "axis": "horizontal",
                                    "sizes": [0.3, 0.7],
                                    "focused": cur
                                }]
                            })
                        }
                        _ => json!({ "ok": true }),
                    }
                }
                _ => json!({ "ok": true }),
            }
        });

        let client = BenchClient::new(&mock.socket_path);
        let find_report = run_find_suite(&client, &format!("{}/find-fixture", srv.base_url()))
            .expect("find suite");
        assert!(find_report.is_ok(), "{:?}", find_report.messages);
        assert_eq!(find_report.passed, 8);

        let split_report = run_split_view_suite(&client, &srv.base_url()).expect("split suite");
        assert!(split_report.is_ok(), "{:?}", split_report.messages);
        assert_eq!(split_report.passed, 4);
    }
}
