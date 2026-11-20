//! Herramienta CLI `test-find` en Rust puro sobre `bench.sock` y `TestHttpServer`.
//!
//! Sustituye a `upstream/test-find` + `PageFindHarness.swift` sin dependencias de Python ni Swift.

use clap::Parser;
use dot_bench::{
    page_find_fixture_html, run_find_suite, BenchClient, TestHttpServer, DEFAULT_TIMEOUT_SECS,
};
use dot_core::bench_proto::resolve_socket_path;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(
    name = "test-find",
    about = "Ejecuta la batería de verificación de búsqueda en página contra Dot vía bench.sock"
)]
struct Cli {
    /// Ruta explícita al socket Unix (anula DOT_SOCKET y XDG_RUNTIME_DIR)
    #[arg(long)]
    socket: Option<PathBuf>,

    /// Conecta a la instancia de pruebas (`world = "test"`)
    #[arg(long)]
    test: bool,

    /// Conecta a un mundo de pruebas con nombre
    #[arg(long)]
    world: Option<String>,

    /// Tiempo máximo de espera por comando en segundos
    #[arg(long, default_value_t = DEFAULT_TIMEOUT_SECS)]
    timeout: u64,

    /// Verifica localmente que el servidor HTTP de fixtures arranca y sirve el HTML esperado
    #[arg(long)]
    fixture_check: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let server = match TestHttpServer::start() {
        Ok(srv) => srv,
        Err(err) => {
            eprintln!("error: no se pudo iniciar TestHttpServer: {err}");
            return ExitCode::from(1);
        }
    };
    let fixture_url = format!("{}/find-fixture", server.base_url());

    if cli.fixture_check {
        let html = page_find_fixture_html();
        let ok = html.contains("orchid")
            && html.contains("Alpha alpha ALPHA")
            && html.contains("ghostword")
            && html.contains("farendword");
        if ok {
            println!("PageFind fixture OK ({fixture_url}, {} bytes)", html.len());
            return ExitCode::SUCCESS;
        }
        eprintln!("FAIL: el fixture HTML de PageFind está incompleto");
        return ExitCode::from(1);
    }

    let world = cli
        .world
        .as_deref()
        .or(if cli.test { Some("test") } else { None });
    let socket_path = match resolve_socket_path(cli.socket.as_ref(), world) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(1);
        }
    };

    let client = BenchClient::new(socket_path).with_timeout(Duration::from_secs(cli.timeout));
    match run_find_suite(&client, &fixture_url) {
        Ok(report) => {
            for line in &report.messages {
                if line.starts_with("FAIL:") {
                    eprintln!("{line}");
                } else {
                    println!("{line}");
                }
            }
            println!(
                "PageFind: {}/{} checks passed",
                report.passed,
                report.total()
            );
            if report.is_ok() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(err) => {
            eprintln!("FAIL: {err}");
            ExitCode::from(1)
        }
    }
}
