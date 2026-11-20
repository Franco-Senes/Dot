//! Protocolo JSON-lines del socket `bench.sock` y medición RSS del árbol de procesos CEF.
//!
//! Compatible con el contrato documentado en `PORTING.md` §7 y `upstream/bench`.

use crate::paths::DotPaths;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

/// Tamaño máximo aceptado por línea JSON en `bench.sock` (4 MiB, igual que `Bench.swift`).
pub const MAX_BENCH_LINE_BYTES: usize = 4 * 1024 * 1024;

/// Variable de entorno para sobreescribir directamente la ruta del socket de bench.
pub const ENV_DOT_SOCKET: &str = "DOT_SOCKET";
/// Alias obsoleto de [`ENV_DOT_SOCKET`].
pub const ENV_LEGACY_SEARCH_SOCKET: &str = "SEARCH_SOCKET";

/// Resumen de una pestaña devuelto por los comandos `tabs`, `open`, `go`, `wait` y `select`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabSummary {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub bench: bool,
    #[serde(default)]
    pub loading: bool,
    #[serde(default)]
    pub asleep: bool,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub private: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

/// Resultado de `find` sobre el protocolo de bench.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindReply {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub count: u32,
    #[serde(default)]
    pub ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
}

/// Estado de memoria del árbol de procesos CEF (proceso principal + descendientes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessTreeMemory {
    pub root_pid: u32,
    pub process_count: usize,
    pub rss_kb: u64,
}

/// Resuelve la ruta del socket Unix `bench.sock`:
/// 1. Si `explicit` es `Some`, usa esa ruta.
/// 2. Si `DOT_SOCKET` (o el respaldo obsoleto `SEARCH_SOCKET`) está definida y no vacía, usa esa ruta.
/// 3. Si `world` es `Some(name)`, usa `$XDG_RUNTIME_DIR/dot/bench-<name>.sock` (saneado).
/// 4. En caso contrario usa `$XDG_RUNTIME_DIR/dot/bench.sock` vía `DotPaths::from_env()`.
pub fn resolve_socket_path(explicit: Option<&PathBuf>, world: Option<&str>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path.clone());
    }
    if let Some(env_sock) = crate::env::var(ENV_DOT_SOCKET, ENV_LEGACY_SEARCH_SOCKET) {
        let trimmed = env_sock.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }
    let paths = DotPaths::from_env()?;
    if let Some(world_name) = world {
        let clean = sanitize_world_name(world_name);
        return Ok(paths.runtime_dir.join(format!("bench-{clean}.sock")));
    }
    Ok(paths.bench_socket())
}

/// Normaliza el nombre de un mundo de pruebas (`--world NAME`) igual que `upstream/bench`.
pub fn sanitize_world_name(raw: &str) -> String {
    let clean: String = raw
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    if clean.is_empty() || clean == "1" {
        "test".to_owned()
    } else {
        clean
    }
}

/// Sanea títulos o URLs antes de imprimirlos en la terminal para impedir secuencias
/// de control o cambios bidireccionales maliciosos (port directo de `clean()` en `upstream/bench`).
pub fn sanitize_terminal_text(text: &str) -> String {
    text.chars()
        .filter(|&ch| {
            let code = ch as u32;
            let is_ctrl = code <= 0x1f || code == 0x7f;
            let is_bidi = matches!(
                code,
                0x200e | 0x200f | 0x202a..=0x202e | 0x2066..=0x2069
            );
            !is_ctrl && !is_bidi
        })
        .collect()
}

/// Serializa un objeto de solicitud en una sola línea JSON terminada en `\n`.
pub fn encode_json_line(value: &Value) -> Result<Vec<u8>> {
    if !value.is_object() {
        return Err(anyhow!("la solicitud de bench debe ser un objeto JSON"));
    }
    let mut bytes = serde_json::to_vec(value).context("error al serializar solicitud JSON")?;
    if bytes.len() + 1 > MAX_BENCH_LINE_BYTES {
        return Err(anyhow!(
            "solicitud JSON excede el límite máximo ({MAX_BENCH_LINE_BYTES} bytes)"
        ));
    }
    bytes.push(b'\n');
    Ok(bytes)
}

/// Decodifica una línea JSON recibida desde el socket `bench.sock`.
pub fn decode_json_line(raw: &[u8]) -> Result<Map<String, Value>> {
    if raw.len() > MAX_BENCH_LINE_BYTES {
        return Err(anyhow!(
            "respuesta JSON excede el límite máximo ({MAX_BENCH_LINE_BYTES} bytes)"
        ));
    }
    let line = match raw.iter().position(|&b| b == b'\n') {
        Some(idx) => &raw[..idx],
        None => raw,
    };
    if line.is_empty() {
        return Ok(Map::new());
    }
    let parsed: Value =
        serde_json::from_slice(line).context("respuesta de bench.sock no es JSON válido")?;
    parsed
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow!("respuesta de bench.sock no es un objeto JSON"))
}

/// Formatea una fila de `bench tabs` con el mismo diseño que `upstream/bench`.
pub fn format_tab_row(tab: &TabSummary) -> String {
    let mark = if tab.bench {
        "⚗"
    } else if tab.active {
        "●"
    } else {
        " "
    };
    let state = if tab.loading {
        " …"
    } else if tab.asleep {
        " z"
    } else {
        ""
    };
    let title = sanitize_terminal_text(&tab.title);
    let display_title = if title.is_empty() { "—" } else { &title };
    let url = sanitize_terminal_text(&tab.url);
    let group = tab
        .group
        .as_deref()
        .filter(|g| !g.is_empty())
        .map(|g| format!("  [{}]", sanitize_terminal_text(g)))
        .unwrap_or_default();
    format!("{mark} {}  {display_title}  {url}{state}{group}", tab.id)
}

/// Suma el consumo RSS (en KiB) de `root_pid` y de todos sus procesos descendientes
/// leyendo `/proc/<pid>/status` en Linux (`PPid:` y `VmRSS:`).
pub fn measure_process_tree_rss_kb(root_pid: u32) -> Result<ProcessTreeMemory> {
    let mut children_map: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut rss_map: HashMap<u32, u64> = HashMap::new();

    for entry in fs::read_dir("/proc").context("no se pudo leer /proc")? {
        let Ok(entry) = entry else {
            continue;
        };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Ok(pid) = name.parse::<u32>() else {
            continue;
        };
        let status_path = entry.path().join("status");
        let Ok(status) = fs::read_to_string(status_path) else {
            continue;
        };
        if let Some((ppid, rss_kb)) = parse_proc_status(&status) {
            children_map.entry(ppid).or_default().push(pid);
            rss_map.insert(pid, rss_kb);
        }
    }

    let mut visited = HashSet::new();
    let mut stack = vec![root_pid];
    let mut total_rss_kb = 0u64;

    while let Some(pid) = stack.pop() {
        if !visited.insert(pid) {
            continue;
        }
        if let Some(&rss) = rss_map.get(&pid) {
            total_rss_kb = total_rss_kb.saturating_add(rss);
        }
        if let Some(children) = children_map.get(&pid) {
            for &child in children {
                if !visited.contains(&child) {
                    stack.push(child);
                }
            }
        }
    }

    Ok(ProcessTreeMemory {
        root_pid,
        process_count: visited.len(),
        rss_kb: total_rss_kb,
    })
}

fn parse_proc_status(status: &str) -> Option<(u32, u64)> {
    let mut ppid: Option<u32> = None;
    let mut rss_kb: u64 = 0;

    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("PPid:") {
            ppid = rest.trim().parse::<u32>().ok();
        } else if let Some(rest) = line.strip_prefix("VmRSS:") {
            let num = rest.split_whitespace().next().unwrap_or("0");
            rss_kb = num.parse::<u64>().unwrap_or(0);
        }
    }

    Some((ppid?, rss_kb))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sanitize_terminal_strips_controls_and_bidi() {
        let dirty = "Hola\x1b[31m\u{202e}Mundo\x07\nFin";
        assert_eq!(sanitize_terminal_text(dirty), "Hola[31mMundoFin");
    }

    #[test]
    fn sanitize_world_matches_upstream_rules() {
        assert_eq!(sanitize_world_name("1"), "test");
        assert_eq!(sanitize_world_name(""), "test");
        assert_eq!(sanitize_world_name("Split_Tests-2!"), "splittests-2");
    }

    #[test]
    fn encode_and_decode_json_lines() {
        let req = json!({ "do": "open", "url": "https://example.com" });
        let wire = encode_json_line(&req).expect("encode");
        assert!(wire.ends_with(b"\n"));
        let decoded = decode_json_line(&wire).expect("decode");
        assert_eq!(decoded.get("do").and_then(Value::as_str), Some("open"));
        assert_eq!(
            decoded.get("url").and_then(Value::as_str),
            Some("https://example.com")
        );
    }

    #[test]
    fn format_tab_row_renders_expected_markers() {
        let tab = TabSummary {
            id: "abcd1234".into(),
            title: "Example\x1b[0m".into(),
            url: "https://example.com".into(),
            active: true,
            bench: false,
            loading: true,
            asleep: false,
            pinned: false,
            private: false,
            group: Some("Docs".into()),
        };
        assert_eq!(
            format_tab_row(&tab),
            "● abcd1234  Example[0m  https://example.com …  [Docs]"
        );
    }

    #[test]
    fn measure_current_process_tree_rss_returns_nonzero() {
        let mem = measure_process_tree_rss_kb(std::process::id()).expect("measure /proc");
        assert_eq!(mem.root_pid, std::process::id());
        assert!(mem.process_count >= 1);
        assert!(mem.rss_kb > 0);
    }
}
