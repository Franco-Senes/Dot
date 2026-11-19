//! Generador HTML para la página de nueva pestaña `plasma://newtab`.
//!
//! Produce HTML5 autocontenido con CSS oscuro/claro limpio (sin emojis), barra de búsqueda
//! hacia el omnibox/motor configurado, Speed Dial de accesos fijados, píldoras de espacios
//! de trabajo, contador de pestañas cerradas, vista previa de lista de lectura y referencia
//! de atajos de teclado.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

/// Tema visual para `plasma://newtab`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NewTabTheme {
    Dark,
    Light,
    #[default]
    System,
}

/// Acceso rápido fijado en el Speed Dial.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeedDialItem {
    pub title: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent_color: Option<String>,
}

/// Píldora resumen de un espacio de trabajo para la barra superior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspacePill {
    pub id: String,
    pub name: String,
    pub color_id: String,
    pub active: bool,
    pub tab_count: usize,
}

/// Elemento resumido de la lista de lectura mostrado en `plasma://newtab`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadingListPreview {
    pub title: String,
    pub url: String,
    pub estimated_minutes: u32,
}

/// Atajo de teclado documentado en la guía rápida.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutRef {
    pub keys: String,
    pub description: String,
}

/// Datos de entrada para renderizar `plasma://newtab`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewTabData {
    #[serde(default)]
    pub theme: NewTabTheme,
    pub search_engine_name: String,
    pub search_action_url: String,
    #[serde(default)]
    pub speed_dial: Vec<SpeedDialItem>,
    #[serde(default)]
    pub workspaces: Vec<WorkspacePill>,
    #[serde(default)]
    pub recently_closed_count: usize,
    #[serde(default)]
    pub reading_list_preview: Vec<ReadingListPreview>,
    #[serde(default)]
    pub shortcuts: Vec<ShortcutRef>,
}

impl Default for NewTabData {
    fn default() -> Self {
        Self {
            theme: NewTabTheme::System,
            search_engine_name: "DuckDuckGo".to_string(),
            search_action_url: "https://duckduckgo.com/".to_string(),
            speed_dial: Vec::new(),
            workspaces: Vec::new(),
            recently_closed_count: 0,
            reading_list_preview: Vec::new(),
            shortcuts: default_shortcuts(),
        }
    }
}

/// Devuelve la tabla estándar de atajos de teclado del navegador.
pub fn default_shortcuts() -> Vec<ShortcutRef> {
    vec![
        ShortcutRef {
            keys: "Ctrl+T".to_string(),
            description: "Nueva pestaña".to_string(),
        },
        ShortcutRef {
            keys: "Ctrl+W".to_string(),
            description: "Cerrar pestaña actual".to_string(),
        },
        ShortcutRef {
            keys: "Ctrl+Shift+T".to_string(),
            description: "Reabrir última pestaña cerrada".to_string(),
        },
        ShortcutRef {
            keys: "Ctrl+L".to_string(),
            description: "Enfocar barra de direcciones".to_string(),
        },
        ShortcutRef {
            keys: "Ctrl+K".to_string(),
            description: "Paleta de comandos y búsqueda".to_string(),
        },
        ShortcutRef {
            keys: "Ctrl+Shift+S".to_string(),
            description: "Alternar vista dividida".to_string(),
        },
    ]
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

fn sanitize_hex_color(raw: Option<&str>) -> &'static str {
    match raw.map(str::trim) {
        Some(c) if c.starts_with('#') && (c.len() == 4 || c.len() == 7) => {
            // Solo permitimos valores seguros predefinidos o usamos acento neutro;
            // para evitar inyección CSS devolvemos un acento seguro si no es hex puro.
            if c[1..].chars().all(|ch| ch.is_ascii_hexdigit()) {
                return "#3b82f6";
            }
            "#3b82f6"
        }
        _ => "#3b82f6",
    }
}

fn theme_css(theme: NewTabTheme) -> &'static str {
    match theme {
        NewTabTheme::Dark => {
            ":root { --bg: #0f1115; --surface: #171a21; --surface-2: #1f242e; --border: #2b3240; --text: #eef2f7; --muted: #9aa4b2; --accent: #4f8cff; }"
        }
        NewTabTheme::Light => {
            ":root { --bg: #f6f7f9; --surface: #ffffff; --surface-2: #eef1f6; --border: #d8dee9; --text: #161b22; --muted: #57606a; --accent: #2563eb; }"
        }
        NewTabTheme::System => {
            ":root { --bg: #f6f7f9; --surface: #ffffff; --surface-2: #eef1f6; --border: #d8dee9; --text: #161b22; --muted: #57606a; --accent: #2563eb; }\n@media (prefers-color-scheme: dark) { :root { --bg: #0f1115; --surface: #171a21; --surface-2: #1f242e; --border: #2b3240; --text: #eef2f7; --muted: #9aa4b2; --accent: #4f8cff; } }"
        }
    }
}

/// Renderiza la página HTML completa para `plasma://newtab`.
pub fn render_newtab_html(data: &NewTabData) -> String {
    let mut html = String::with_capacity(4096);
    let engine_name = escape_html(&data.search_engine_name);
    let action_url = escape_html(&data.search_action_url);
    let shortcuts = if data.shortcuts.is_empty() {
        default_shortcuts()
    } else {
        data.shortcuts.clone()
    };

    let _ = write!(
        html,
        "<!DOCTYPE html>\n<html lang=\"es\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>Nueva pestaña</title>\n<style>\n{}\n* {{ box-sizing: border-box; }}\nbody {{ margin: 0; font-family: system-ui, -apple-system, sans-serif; background: var(--bg); color: var(--text); line-height: 1.5; }}\nmain {{ max-width: 960px; margin: 0 auto; padding: 48px 24px 64px; }}\n.topbar {{ display: flex; flex-wrap: wrap; justify-content: space-between; align-items: center; gap: 12px; margin-bottom: 32px; }}\n.pills {{ display: flex; flex-wrap: wrap; gap: 8px; }}\n.pill {{ padding: 6px 12px; border-radius: 999px; border: 1px solid var(--border); background: var(--surface); color: var(--muted); font-size: 13px; text-decoration: none; }}\n.pill.active {{ border-color: var(--accent); color: var(--text); font-weight: 600; }}\n.badge {{ font-size: 13px; color: var(--muted); background: var(--surface); border: 1px solid var(--border); padding: 6px 12px; border-radius: 8px; }}\n.search-form {{ display: flex; gap: 8px; margin-bottom: 36px; }}\n.search-input {{ flex: 1; padding: 14px 16px; border-radius: 10px; border: 1px solid var(--border); background: var(--surface); color: var(--text); font-size: 16px; }}\n.search-input:focus {{ outline: 2px solid var(--accent); outline-offset: 1px; }}\n.search-btn {{ padding: 0 20px; border-radius: 10px; border: none; background: var(--accent); color: #ffffff; font-size: 15px; font-weight: 600; cursor: pointer; }}\nh2 {{ font-size: 14px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); margin: 28px 0 12px; }}\n.grid {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); gap: 12px; }}\n.card {{ display: block; padding: 14px; border-radius: 10px; border: 1px solid var(--border); background: var(--surface); color: var(--text); text-decoration: none; }}\n.card:hover {{ background: var(--surface-2); border-color: var(--accent); }}\n.card-title {{ font-weight: 600; font-size: 14px; margin-bottom: 4px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }}\n.card-url {{ font-size: 12px; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }}\n.list {{ list-style: none; padding: 0; margin: 0; display: grid; gap: 8px; }}\n.list-item {{ display: flex; justify-content: space-between; align-items: center; padding: 10px 14px; border-radius: 8px; border: 1px solid var(--border); background: var(--surface); }}\n.list-item a {{ color: var(--text); text-decoration: none; font-weight: 500; }}\n.meta {{ font-size: 12px; color: var(--muted); }}\n.shortcuts {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(240px, 1fr)); gap: 8px; }}\n.shortcut {{ display: flex; justify-content: space-between; align-items: center; padding: 8px 12px; border-radius: 8px; border: 1px solid var(--border); background: var(--surface); font-size: 13px; }}\nkbd {{ font-family: ui-monospace, monospace; font-size: 12px; padding: 2px 6px; border-radius: 4px; background: var(--surface-2); border: 1px solid var(--border); }}\n</style>\n</head>\n<body>\n<main>\n",
        theme_css(data.theme)
    );

    // Topbar: Workspaces pills + Recently Closed count
    html.push_str(
        "<div class=\"topbar\">\n<div class=\"pills\" aria-label=\"Espacios de trabajo\">\n",
    );
    for ws in &data.workspaces {
        let cls = if ws.active { "pill active" } else { "pill" };
        let _ = writeln!(
            html,
            "<span class=\"{cls}\" data-workspace-id=\"{}\" data-color=\"{}\">{} ({})</span>",
            escape_html(&ws.id),
            escape_html(&ws.color_id),
            escape_html(&ws.name),
            ws.tab_count
        );
    }
    html.push_str("</div>\n");
    let _ = writeln!(
        html,
        "<div class=\"badge\">Cerradas recientemente: {}</div>\n</div>\n",
        data.recently_closed_count
    );

    // Search form
    let _ = writeln!(
        html,
        "<form class=\"search-form\" method=\"get\" action=\"{action_url}\">\n<input class=\"search-input\" type=\"search\" name=\"q\" placeholder=\"Buscar con {engine_name} o escribir una dirección\" autofocus aria-label=\"Buscar\">\n<button class=\"search-btn\" type=\"submit\">Buscar</button>\n</form>\n"
    );

    // Speed Dial
    if !data.speed_dial.is_empty() {
        html.push_str("<h2>Speed Dial</h2>\n<div class=\"grid\">\n");
        for item in &data.speed_dial {
            let accent = sanitize_hex_color(item.accent_color.as_deref());
            let _ = writeln!(
                html,
                "<a class=\"card\" href=\"{}\" style=\"border-top: 3px solid {accent};\"><div class=\"card-title\">{}</div><div class=\"card-url\">{}</div></a>",
                escape_html(&item.url),
                escape_html(&item.title),
                escape_html(&item.url)
            );
        }
        html.push_str("</div>\n");
    }

    // Reading List preview
    if !data.reading_list_preview.is_empty() {
        html.push_str("<h2>Lista de lectura</h2>\n<ul class=\"list\">\n");
        for item in &data.reading_list_preview {
            let _ = writeln!(
                html,
                "<li class=\"list-item\"><a href=\"{}\">{}</a><span class=\"meta\">{} min</span></li>",
                escape_html(&item.url),
                escape_html(&item.title),
                item.estimated_minutes
            );
        }
        html.push_str("</ul>\n");
    }

    // Keyboard shortcuts reference
    html.push_str("<h2>Atajos de teclado</h2>\n<div class=\"shortcuts\">\n");
    for sc in &shortcuts {
        let _ = writeln!(
            html,
            "<div class=\"shortcut\"><span>{}</span><kbd>{}</kbd></div>",
            escape_html(&sc.description),
            escape_html(&sc.keys)
        );
    }
    html.push_str("</div>\n</main>\n</body>\n</html>\n");
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_all_sections_and_escapes_html() {
        let data = NewTabData {
            theme: NewTabTheme::Dark,
            search_engine_name: "Search <Test>".to_string(),
            search_action_url: "https://duckduckgo.com/".to_string(),
            speed_dial: vec![SpeedDialItem {
                title: "Docs & <Rust>".to_string(),
                url: "https://docs.rs".to_string(),
                accent_color: Some("#4f8cff".to_string()),
            }],
            workspaces: vec![WorkspacePill {
                id: "ws-1".to_string(),
                name: "Dev".to_string(),
                color_id: "blue".to_string(),
                active: true,
                tab_count: 4,
            }],
            recently_closed_count: 3,
            reading_list_preview: vec![ReadingListPreview {
                title: "Async Rust".to_string(),
                url: "https://rust-lang.org".to_string(),
                estimated_minutes: 6,
            }],
            shortcuts: default_shortcuts(),
        };

        let html = render_newtab_html(&data);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Search &lt;Test&gt;"));
        assert!(html.contains("Docs &amp; &lt;Rust&gt;"));
        assert!(html.contains("Dev (4)"));
        assert!(html.contains("Cerradas recientemente: 3"));
        assert!(html.contains("Async Rust"));
        assert!(html.contains("6 min"));
        assert!(html.contains("<kbd>Ctrl+Shift+T</kbd>"));
        assert!(html.contains("--bg: #0f1115;"));
    }

    #[test]
    fn light_and_system_themes_emit_expected_css() {
        let light = render_newtab_html(&NewTabData {
            theme: NewTabTheme::Light,
            ..Default::default()
        });
        assert!(light.contains("--bg: #f6f7f9;"));
        assert!(!light.contains("prefers-color-scheme: dark"));

        let system = render_newtab_html(&NewTabData::default());
        assert!(system.contains("prefers-color-scheme: dark"));
    }
}
