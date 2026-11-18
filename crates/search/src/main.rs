//! Punto de entrada de `search` (`io.github.searchlinux.Search`).
//!
//! REGLA CRÍTICA DE ARQUITECTURA CEF:
//! `cef::execute_process` se invoca como primerísima instrucción antes de inicializar GTK4,
//! el sistema de trazas o los directorios XDG, de modo que los subprocesos (`--type=renderer`,
//! `gpu-process`, `utility`, `zygote`) salgan inmediatamente con su propio código de retorno.

mod actions;
mod address;
mod app;
mod bench;
mod bookmarks_ui;
mod browser_view;
mod cef_engine;
mod curtain;
mod dbus;
mod downloads;
mod downloads_ui;
mod extension_shims;
mod extensions;
mod find_bar;
mod history_ui;
mod pip;
mod reader;
mod settings;
mod settings_ui;
mod shield;
mod sidebar_tabs;
mod split_view;
mod style;
mod tab_switcher;
mod updater;
mod vault;
mod vault_ui;
mod window;

use app::CliOptions;
use search_core::paths::SearchPaths;
use settings::SettingsStore;
use std::process::ExitCode;
use std::time::Instant;

fn main() -> ExitCode {
    let startup_instant = Instant::now();

    // 1. Despacho inmediato de subprocesos CEF antes de tocar GTK4 ni el disco.
    let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
    let is_cef_subprocess = std::env::args().any(|arg| arg.starts_with("--type="));
    let cef_args = cef::args::Args::new();
    let mut early_app = cef_engine::create_early_cef_app();
    let sub_code = cef::execute_process(
        Some(cef_args.as_main_args()),
        Some(&mut early_app),
        std::ptr::null_mut(),
    );
    if sub_code >= 0 || is_cef_subprocess {
        std::process::exit(sub_code.max(0));
    }

    // 2. Argumentos propios de línea de comandos del proceso principal.
    let cli = match parse_cli_args() {
        CliParseOutcome::Run(opts) => opts,
        CliParseOutcome::ExitZero => return ExitCode::SUCCESS,
        CliParseOutcome::ExitError(msg) => {
            eprintln!("search: {msg}");
            return ExitCode::from(2);
        }
    };

    // 3. Logging estructurado y rutas XDG (`0700`).
    search_core::trace::init_logging();

    let paths = match SearchPaths::from_env().and_then(|p| {
        p.ensure_dirs()?;
        Ok(p)
    }) {
        Ok(p) => p,
        Err(err) => {
            search_core::error!("No se pudieron preparar los directorios XDG: {err:#}");
            return ExitCode::from(1);
        }
    };

    // 4. Inicialización explícita de GTK4 + libadwaita y carga de ajustes.
    if !gnome_platform::init() {
        search_core::error!(
            "No se pudo inicializar GTK4/libadwaita (¿hay servidor gráfico Wayland/X11 disponible?)"
        );
        return ExitCode::from(1);
    }

    let settings_store = SettingsStore::load(paths.settings_file());
    let exit_status = app::run(startup_instant, paths, settings_store, cli);

    if exit_status == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from((exit_status.clamp(1, 255)) as u8)
    }
}

enum CliParseOutcome {
    Run(CliOptions),
    ExitZero,
    ExitError(String),
}

fn parse_cli_args() -> CliParseOutcome {
    let mut opts = CliOptions::default();
    let mut iter = std::env::args().skip(1);

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return CliParseOutcome::ExitZero;
            }
            "-V" | "--version" => {
                println!("search {}", env!("CARGO_PKG_VERSION"));
                return CliParseOutcome::ExitZero;
            }
            "--smoke-test" => match iter.next() {
                Some(url) if !url.trim().is_empty() => {
                    opts.smoke_test_url = Some(url);
                }
                _ => {
                    return CliParseOutcome::ExitError(
                        "la opción --smoke-test requiere una <url>".to_string(),
                    );
                }
            },
            _ if arg.starts_with("--smoke-test=") => {
                let url = &arg["--smoke-test=".len()..];
                if url.trim().is_empty() {
                    return CliParseOutcome::ExitError(
                        "la opción --smoke-test requiere una <url>".to_string(),
                    );
                }
                opts.smoke_test_url = Some(url.to_string());
            }
            _ if arg.starts_with('-') => {
                // Ignorar flags extra de entorno/GTK/Chromium sin abortar.
            }
            _ => {
                opts.urls.push(arg);
            }
        }
    }

    CliParseOutcome::Run(opts)
}

fn print_help() {
    println!(
        "Search {} — Navegador web minimalista, rápido y privado para Linux\n\n\
         USO:\n  search [OPCIONES] [URL...]\n\n\
         OPCIONES:\n  \
           --smoke-test <URL>  Carga <URL>, verifica renderizado OSR en CEF y sale con código 0\n  \
           -h, --help          Muestra esta ayuda\n  \
           -V, --version       Muestra la versión",
        env!("CARGO_PKG_VERSION")
    );
}
