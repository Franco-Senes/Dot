//! Generador HTML para la página de nueva pestaña `plasma://newtab` / `dot://newtab`.
//!
//! Produce HTML5 autocontenido con estética editorial minimalista en blanco, negro y carbón
//! propia de Dot (sin emojis), barra de búsqueda limpia, Speed Dial minimalista, píldoras
//! de espacios de trabajo, vista previa de lista de lectura y referencia discreta de atajos.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

/// Tema visual para `plasma://newtab` / `dot://newtab`.
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

/// Elemento resumido de la lista de lectura mostrado en la nueva pestaña.
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

/// Datos de entrada para renderizar `plasma://newtab` / `dot://newtab`.
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

fn sanitize_monochrome_indicator(raw: Option<&str>) -> &'static str {
    match raw.map(str::trim) {
        Some(c) if c.starts_with('#') && (c.len() == 4 || c.len() == 7) => {
            if c[1..].chars().all(|ch| ch.is_ascii_hexdigit()) {
                return "var(--border-strong)";
            }
            "var(--border)"
        }
        _ => "var(--border)",
    }
}

fn theme_css(theme: NewTabTheme) -> &'static str {
    match theme {
        NewTabTheme::Dark => {
            ":root { --bg: #0a0a0b; --surface: #111113; --surface-2: #18181b; --border: #242428; --border-strong: #3f3f46; --text: #f4f4f5; --muted: #8e8e93; --accent: #f4f4f5; --accent-fg: #0a0a0b; }"
        }
        NewTabTheme::Light => {
            ":root { --bg: #fafafa; --surface: #ffffff; --surface-2: #f4f4f5; --border: #e4e4e7; --border-strong: #a1a1aa; --text: #09090b; --muted: #71717a; --accent: #09090b; --accent-fg: #ffffff; }"
        }
        NewTabTheme::System => {
            ":root { --bg: #fafafa; --surface: #ffffff; --surface-2: #f4f4f5; --border: #e4e4e7; --border-strong: #a1a1aa; --text: #09090b; --muted: #71717a; --accent: #09090b; --accent-fg: #ffffff; }\n@media (prefers-color-scheme: dark) { :root { --bg: #0a0a0b; --surface: #111113; --surface-2: #18181b; --border: #242428; --border-strong: #3f3f46; --text: #f4f4f5; --muted: #8e8e93; --accent: #f4f4f5; --accent-fg: #0a0a0b; } }"
        }
    }
}

/// Renderiza la página HTML completa para `plasma://newtab` / `dot://newtab`.
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
        "<!DOCTYPE html>\n<html lang=\"es\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>Dot</title>\n<style>\n{}\n* {{ box-sizing: border-box; }}\nbody {{ margin: 0; font-family: \"Inter\", system-ui, -apple-system, sans-serif; background: var(--bg); color: var(--text); line-height: 1.5; -webkit-font-smoothing: antialiased; }}\nmain {{ max-width: 760px; margin: 0 auto; padding: 32px 24px 64px; }}\n.topbar {{ display: flex; flex-wrap: wrap; justify-content: space-between; align-items: center; gap: 12px; min-height: 32px; margin-bottom: 44px; }}\n.pills {{ display: flex; flex-wrap: wrap; gap: 6px; }}\n.pill {{ padding: 4px 10px; border-radius: 999px; border: 1px solid var(--border); background: transparent; color: var(--muted); font-size: 12px; letter-spacing: 0.01em; }}\n.pill.active {{ border-color: var(--border-strong); background: var(--surface-2); color: var(--text); font-weight: 500; }}\n.closed-meta {{ font-size: 12px; color: var(--muted); }}\n.hero {{ display: flex; flex-direction: column; align-items: center; gap: 10px; margin: 24px 0 28px; }}\n.dot-mark {{ display: block; color: var(--text); }}\n.wordmark {{ font-size: 13px; font-weight: 500; letter-spacing: 0.22em; text-transform: uppercase; color: var(--muted); }}\n.search-form {{ display: flex; gap: 8px; max-width: 560px; margin: 0 auto 44px; }}\n.search-input {{ flex: 1; padding: 12px 16px; border-radius: 10px; border: 1px solid var(--border); background: var(--surface); color: var(--text); font-size: 15px; transition: border-color 120ms ease; }}\n.search-input::placeholder {{ color: var(--muted); }}\n.search-input:focus {{ outline: none; border-color: var(--border-strong); }}\n.search-btn {{ padding: 0 18px; border-radius: 10px; border: 1px solid var(--border); background: var(--accent); color: var(--accent-fg); font-size: 14px; font-weight: 500; cursor: pointer; }}\n.search-btn:hover {{ opacity: 0.9; }}\nh2 {{ font-size: 11px; font-weight: 500; text-transform: uppercase; letter-spacing: 0.12em; color: var(--muted); margin: 32px 0 12px; }}\n.grid {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(156px, 1fr)); gap: 10px; }}\n.card {{ display: block; padding: 12px 14px; border-radius: 10px; border: 1px solid var(--border); background: var(--surface); color: var(--text); text-decoration: none; transition: background 120ms ease, border-color 120ms ease; }}\n.card:hover {{ background: var(--surface-2); border-color: var(--border-strong); }}\n.card-title {{ font-weight: 500; font-size: 13px; margin-bottom: 3px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }}\n.card-url {{ font-size: 11px; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }}\n.list {{ list-style: none; padding: 0; margin: 0; display: grid; gap: 6px; }}\n.list-item {{ display: flex; justify-content: space-between; align-items: center; padding: 10px 14px; border-radius: 8px; border: 1px solid var(--border); background: var(--surface); }}\n.list-item a {{ color: var(--text); text-decoration: none; font-size: 13px; font-weight: 500; }}\n.list-item a:hover {{ text-decoration: underline; }}\n.meta {{ font-size: 12px; color: var(--muted); }}\n.shortcuts {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 6px; }}\n.shortcut {{ display: flex; justify-content: space-between; align-items: center; padding: 7px 10px; border-radius: 8px; border: 1px solid var(--border); background: transparent; color: var(--muted); font-size: 12px; }}\nkbd {{ font-family: ui-monospace, monospace; font-size: 11px; padding: 2px 6px; border-radius: 4px; background: var(--surface); border: 1px solid var(--border); color: var(--text); }}\n</style>\n</head>\n<body>\n<main>\n",
        theme_css(data.theme)
    );

    // Topbar: minimal Workspaces pills + quiet Recently Closed indicator
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
    if data.recently_closed_count > 0 {
        let _ = writeln!(
            html,
            "<div class=\"closed-meta\">Cerradas recientemente: {}</div>",
            data.recently_closed_count
        );
    }
    html.push_str("</div>\n");

    // Centered Dot mark + subtle wordmark
    html.push_str(
        "<header class=\"hero\">\n<svg class=\"dot-mark\" xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 36 36\" width=\"32\" height=\"32\" role=\"img\" aria-label=\"Dot\"><circle cx=\"18\" cy=\"18\" r=\"10\" fill=\"currentColor\"/></svg>\n<div class=\"wordmark\">Dot</div>\n</header>\n",
    );

    // Clean search form
    let _ = writeln!(
        html,
        "<form class=\"search-form\" method=\"get\" action=\"{action_url}\">\n<input class=\"search-input\" type=\"search\" name=\"q\" placeholder=\"Buscar con {engine_name} o escribir una dirección\" autofocus aria-label=\"Buscar\">\n<button class=\"search-btn\" type=\"submit\">Buscar</button>\n</form>\n"
    );

    // Minimal Speed Dial
    if !data.speed_dial.is_empty() {
        html.push_str("<h2>Speed Dial</h2>\n<div class=\"grid\">\n");
        for item in &data.speed_dial {
            let border_top = sanitize_monochrome_indicator(item.accent_color.as_deref());
            let _ = writeln!(
                html,
                "<a class=\"card\" href=\"{}\" style=\"border-top: 2px solid {border_top};\"><div class=\"card-title\">{}</div><div class=\"card-url\">{}</div></a>",
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

    // Quiet keyboard shortcuts reference
    html.push_str("<h2>Atajos</h2>\n<div class=\"shortcuts\">\n");
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
    fn renders_dot_editorial_sections_and_escapes_html() {
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
        assert!(html.contains("<title>Dot</title>"));
        assert!(html.contains("class=\"dot-mark\""));
        assert!(html.contains("<circle cx=\"18\" cy=\"18\" r=\"10\" fill=\"currentColor\"/>"));
        assert!(html.contains("<div class=\"wordmark\">Dot</div>"));
        assert!(html.contains("Search &lt;Test&gt;"));
        assert!(html.contains("Docs &amp; &lt;Rust&gt;"));
        assert!(html.contains("Dev (4)"));
        assert!(html.contains("Cerradas recientemente: 3"));
        assert!(html.contains("Async Rust"));
        assert!(html.contains("6 min"));
        assert!(html.contains("<kbd>Ctrl+Shift+T</kbd>"));
        assert!(html.contains("--bg: #0a0a0b;"));
        assert!(!html.contains("#4f8cff"));
        assert!(!html.contains("#3b82f6"));
    }

    #[test]
    fn light_and_system_themes_emit_expected_css() {
        let light = render_newtab_html(&NewTabData {
            theme: NewTabTheme::Light,
            ..Default::default()
        });
        assert!(light.contains("--bg: #fafafa;"));
        assert!(!light.contains("prefers-color-scheme: dark"));
        assert!(!light.contains("Cerradas recientemente:"));

        let system = render_newtab_html(&NewTabData::default());
        assert!(system.contains("prefers-color-scheme: dark"));
    }
}
