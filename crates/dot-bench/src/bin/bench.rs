//! Cliente CLI `bench` en Rust puro para el protocolo JSON-lines de `bench.sock`.

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use dot_bench::{BenchClient, BenchClientError, DEFAULT_TIMEOUT_SECS};
use dot_core::bench_proto::{format_tab_row, resolve_socket_path, TabSummary};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(
    name = "bench",
    about = "Controla una instancia de Dot a través de $XDG_RUNTIME_DIR/dot/bench.sock"
)]
struct Cli {
    /// Ruta explícita al socket Unix (anula DOT_SOCKET y XDG_RUNTIME_DIR)
    #[arg(long, global = true)]
    socket: Option<PathBuf>,

    /// Conecta a la instancia de pruebas (`world = "test"`)
    #[arg(long, global = true)]
    test: bool,

    /// Conecta a un mundo de pruebas con nombre (`DOT_PROBE=NAME`)
    #[arg(long, global = true)]
    world: Option<String>,

    /// Dirige el comando a la ventana N (empezando en 1)
    #[arg(long, global = true)]
    window: Option<usize>,

    /// Tiempo máximo de espera en segundos
    #[arg(long, global = true, default_value_t = DEFAULT_TIMEOUT_SECS)]
    timeout: u64,

    /// Imprime siempre la respuesta JSON completa sin formato abreviado
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Lista todas las pestañas abiertas
    Tabs,
    /// Abre una pestaña de bench con una URL
    Open {
        url: String,
        #[arg(long)]
        private: bool,
    },
    /// Navega una pestaña existente hacia otra URL
    Go { id: String, url: String },
    /// Espera a que termine de cargar una pestaña
    Wait { id: String, seconds: Option<f64> },
    /// Activa una pestaña (en ejecuciones de prueba)
    Select { id: String },
    /// Pone una pestaña en reposo
    Sleep { id: String },
    /// Cierra una pestaña de bench por ID o "all"
    Close { id: String },
    /// Devuelve el texto visible de la página
    Text { id: String },
    /// Ejecuta JavaScript en una pestaña
    Eval {
        id: String,
        js: String,
        /// Ejecuta en el mundo aislado de Dot
        #[arg(long)]
        search_world: bool,
    },
    /// Hace clic en un selector CSS
    Click { id: String, selector: String },
    /// Escribe texto en un selector CSS
    Type {
        id: String,
        selector: String,
        text: String,
    },
    /// Envía el formulario que contiene el selector CSS
    Submit { id: String, selector: String },
    /// Ejecuta búsqueda en la página actual
    Find {
        text: String,
        #[arg(long)]
        next: Option<usize>,
        #[arg(long)]
        back: Option<usize>,
        #[arg(long = "case")]
        match_case: bool,
        #[arg(long)]
        words: bool,
    },
    /// Captura un PNG del viewport de una pestaña
    Shot {
        id: String,
        path: Option<PathBuf>,
        #[arg(long)]
        width: Option<f64>,
    },
    /// Inspecciona ventanas, pestaña activa, memoria RSS del árbol CEF y arranque
    Probe,
    /// Fija, desfija o lleva al inicio una pestaña
    Pin {
        id: String,
        #[arg(long, conflicts_with = "home")]
        off: bool,
        #[arg(long, conflicts_with = "off")]
        home: bool,
    },
    /// Operaciones sobre Split View
    Split {
        #[command(subcommand)]
        action: SplitAction,
    },
    /// Cambia ajustes de interfaz en ejecuciones de prueba
    Ui { key: String, value: String },
    /// Solicita el cierre limpio de la aplicación
    Quit,
    /// Envía un objeto JSON arbitrario al socket (compatibilidad avanzada)
    Raw { json: String },
}

#[derive(Debug, Subcommand)]
enum SplitAction {
    State,
    Pair {
        dragged: String,
        target: String,
        side: String,
    },
    Focus {
        id: String,
    },
    Select {
        id: String,
    },
    Detach {
        id: String,
    },
    Fraction {
        split: String,
        value: f64,
    },
    Even,
    Swap,
    Close {
        id: String,
    },
    Save,
    Open {
        url: String,
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        background: bool,
    },
}

fn absolutize(path: &Path) -> Result<String> {
    if path.is_absolute() {
        return Ok(path.to_string_lossy().into_owned());
    }
    let cwd = std::env::current_dir().context("no se pudo leer el directorio actual")?;
    Ok(cwd.join(path).to_string_lossy().into_owned())
}

fn build_request(cmd: &Command) -> Result<(&'static str, Map<String, Value>)> {
    let mut map = Map::new();
    let verb = match cmd {
        Command::Tabs => "tabs",
        Command::Open { url, private } => {
            map.insert("url".into(), json!(url));
            if *private {
                map.insert("private".into(), json!(true));
            }
            "open"
        }
        Command::Go { id, url } => {
            map.insert("id".into(), json!(id));
            map.insert("url".into(), json!(url));
            "go"
        }
        Command::Wait { id, seconds } => {
            map.insert("id".into(), json!(id));
            if let Some(secs) = seconds {
                map.insert("seconds".into(), json!(secs));
            }
            "wait"
        }
        Command::Select { id } => {
            map.insert("id".into(), json!(id));
            "select"
        }
        Command::Sleep { id } => {
            map.insert("id".into(), json!(id));
            "sleep"
        }
        Command::Close { id } => {
            map.insert("id".into(), json!(id));
            "close"
        }
        Command::Text { id } => {
            map.insert("id".into(), json!(id));
            "text"
        }
        Command::Eval {
            id,
            js,
            search_world,
        } => {
            map.insert("id".into(), json!(id));
            map.insert("js".into(), json!(js));
            if *search_world {
                map.insert("world".into(), json!("search"));
            }
            "eval"
        }
        Command::Click { id, selector } => {
            map.insert("id".into(), json!(id));
            map.insert("selector".into(), json!(selector));
            "click"
        }
        Command::Type { id, selector, text } => {
            map.insert("id".into(), json!(id));
            map.insert("selector".into(), json!(selector));
            map.insert("text".into(), json!(text));
            "type"
        }
        Command::Submit { id, selector } => {
            map.insert("id".into(), json!(id));
            map.insert("selector".into(), json!(selector));
            "submit"
        }
        Command::Find {
            text,
            next,
            back,
            match_case,
            words,
        } => {
            map.insert("text".into(), json!(text));
            if let Some(n) = next {
                map.insert("next".into(), json!(n));
            }
            if let Some(b) = back {
                map.insert("back".into(), json!(b));
            }
            if *match_case {
                map.insert("case".into(), json!(true));
            }
            if *words {
                map.insert("words".into(), json!(true));
            }
            "find"
        }
        Command::Shot { id, path, width } => {
            map.insert("id".into(), json!(id));
            if let Some(p) = path {
                map.insert("path".into(), json!(absolutize(p)?));
            }
            if let Some(w) = width {
                map.insert("width".into(), json!(w));
            }
            "shot"
        }
        Command::Probe => "probe",
        Command::Pin { id, off, home } => {
            map.insert("id".into(), json!(id));
            map.insert("off".into(), json!(off));
            map.insert("home".into(), json!(home));
            "pin"
        }
        Command::Split { action } => {
            match action {
                SplitAction::State => {
                    map.insert("action".into(), json!("state"));
                }
                SplitAction::Pair {
                    dragged,
                    target,
                    side,
                } => {
                    if side != "left" && side != "right" {
                        return Err(anyhow!("side debe ser 'left' o 'right'"));
                    }
                    map.insert("action".into(), json!("pair"));
                    map.insert("id".into(), json!(dragged));
                    map.insert("with".into(), json!(target));
                    map.insert("side".into(), json!(side));
                }
                SplitAction::Focus { id } => {
                    map.insert("action".into(), json!("focus"));
                    map.insert("id".into(), json!(id));
                }
                SplitAction::Select { id } => {
                    map.insert("action".into(), json!("select"));
                    map.insert("id".into(), json!(id));
                }
                SplitAction::Detach { id } => {
                    map.insert("action".into(), json!("detach"));
                    map.insert("id".into(), json!(id));
                }
                SplitAction::Fraction { split, value } => {
                    map.insert("action".into(), json!("fraction"));
                    map.insert("split".into(), json!(split));
                    map.insert("fraction".into(), json!(value));
                }
                SplitAction::Even => {
                    map.insert("action".into(), json!("even"));
                }
                SplitAction::Swap => {
                    map.insert("action".into(), json!("swap"));
                }
                SplitAction::Close { id } => {
                    map.insert("action".into(), json!("close"));
                    map.insert("id".into(), json!(id));
                }
                SplitAction::Save => {
                    map.insert("action".into(), json!("save"));
                }
                SplitAction::Open {
                    url,
                    from,
                    background,
                } => {
                    map.insert("action".into(), json!("open"));
                    map.insert("url".into(), json!(url));
                    if let Some(src) = from {
                        map.insert("from".into(), json!(src));
                    }
                    map.insert("foreground".into(), json!(!background));
                }
            }
            "split"
        }
        Command::Ui { key, value } => {
            let val = if matches!(
                key.as_str(),
                "look" | "edittab" | "peeklink" | "import" | "side" | "pages120"
            ) {
                json!(value)
            } else {
                let truthy = matches!(
                    value.to_ascii_lowercase().as_str(),
                    "on" | "true" | "1" | "yes"
                );
                json!(truthy)
            };
            map.insert(key.clone(), val);
            "ui"
        }
        Command::Quit => "quit",
        Command::Raw { json } => {
            let parsed: Value =
                serde_json::from_str(json).context("el argumento de `raw` no es JSON válido")?;
            let obj = parsed
                .as_object()
                .cloned()
                .ok_or_else(|| anyhow!("el argumento de `raw` debe ser un objeto JSON"))?;
            return Ok(("raw", obj));
        }
    };

    map.insert("do".into(), json!(verb));
    Ok((verb, map))
}

fn print_answer(verb: &str, raw_json: bool, answer: &Map<String, Value>) -> Result<()> {
    if raw_json {
        println!("{}", serde_json::to_string_pretty(answer)?);
        return Ok(());
    }

    match verb {
        "text" => {
            println!(
                "{}",
                answer.get("text").and_then(Value::as_str).unwrap_or("")
            );
            if answer.get("truncated").and_then(Value::as_bool) == Some(true) {
                eprintln!("\n[… truncated]");
            }
        }
        "shot" => {
            if let Some(path) = answer.get("path").and_then(Value::as_str) {
                println!("{path}");
            } else {
                println!("{}", serde_json::to_string_pretty(answer)?);
            }
        }
        "open" => {
            if let Some(id) = answer.get("id").and_then(Value::as_str) {
                println!("{id}");
            } else {
                println!("{}", serde_json::to_string_pretty(answer)?);
            }
        }
        "eval" => match answer.get("value") {
            Some(Value::String(s)) => println!("{s}"),
            Some(v) => println!("{}", serde_json::to_string_pretty(v)?),
            None => println!("null"),
        },
        "tabs" => {
            if let Some(tabs) = answer.get("tabs").and_then(Value::as_array) {
                for item in tabs {
                    if let Ok(summary) = serde_json::from_value::<TabSummary>(item.clone()) {
                        println!("{}", format_tab_row(&summary));
                    }
                }
            }
        }
        _ => {
            println!("{}", serde_json::to_string_pretty(answer)?);
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
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

    let (verb, request) = match build_request(&cli.command) {
        Ok(pair) => pair,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(1);
        }
    };

    let client = BenchClient::new(socket_path)
        .with_timeout(Duration::from_secs(cli.timeout))
        .with_window(cli.window);

    match client.send(request) {
        Ok(answer) => {
            if let Err(err) = print_answer(verb, cli.json, &answer) {
                eprintln!("error: {err}");
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(BenchClientError::OnScreenViolation(details)) => {
            eprintln!(
                "error: a window came onto a screen in this test run — stop the probe: {details}"
            );
            ExitCode::from(3)
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(1)
        }
    }
}
