//! Generador HTML para la página de nueva pestaña `dot://newtab` / `plasma://newtab`.
//!
//! Estética minimalista pura para Dot: espacio negativo real, un único punto geométrico,
//! una barra de búsqueda limpia sin botones redundantes y elementos secundarios discretos
//! solo cuando existen.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

/// Tema visual para `dot://newtab` / `plasma://newtab`.
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

/// Píldora resumen de un espacio de trabajo.
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

/// Atajo de teclado documentado en el pie discreto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutRef {
    pub keys: String,
    pub description: String,
}

/// Datos de entrada para renderizar `dot://newtab` / `plasma://newtab`.
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

/// Atajos mínimos para referencia discreta en el pie.
pub fn default_shortcuts() -> Vec<ShortcutRef> {
    vec![
        ShortcutRef {
            keys: "Ctrl+K".to_string(),
            description: "Comandos".to_string(),
        },
        ShortcutRef {
            keys: "Ctrl+S".to_string(),
            description: "Barra lateral".to_string(),
        },
        ShortcutRef {
            keys: "Ctrl+Shift+T".to_string(),
            description: "Reabrir".to_string(),
        },
        ShortcutRef {
            keys: "Ctrl+,".to_string(),
            description: "Ajustes".to_string(),
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

fn short_host(raw: &str) -> String {
    let trimmed = raw
        .trim()
        .strip_prefix("https://")
        .or_else(|| raw.trim().strip_prefix("http://"))
        .unwrap_or(raw.trim());
    let host = trimmed.split('/').next().unwrap_or(trimmed);
    host.strip_prefix("www.").unwrap_or(host).to_string()
}

fn theme_css(theme: NewTabTheme) -> &'static str {
    match theme {
        // Exact libadwaita dark palette (@view_bg_color = #1e1e1e)
        NewTabTheme::Dark => {
            ":root { --bg: #1e1e1e; --field: rgba(255,255,255,0.06); --field-hover: rgba(255,255,255,0.09); --field-focus: #262626; --line: rgba(255,255,255,0.09); --line-focus: rgba(255,255,255,0.28); --fg: rgba(255,255,255,0.92); --muted: rgba(255,255,255,0.55); --faint: rgba(255,255,255,0.36); }"
        }
        // Exact libadwaita light palette (@view_bg_color = #ffffff)
        NewTabTheme::Light => {
            ":root { --bg: #ffffff; --field: rgba(0,0,0,0.05); --field-hover: rgba(0,0,0,0.075); --field-focus: #ffffff; --line: rgba(0,0,0,0.09); --line-focus: rgba(0,0,0,0.28); --fg: rgba(0,0,0,0.84); --muted: rgba(0,0,0,0.55); --faint: rgba(0,0,0,0.38); }"
        }
        NewTabTheme::System => {
            ":root { --bg: #ffffff; --field: rgba(0,0,0,0.05); --field-hover: rgba(0,0,0,0.075); --field-focus: #ffffff; --line: rgba(0,0,0,0.09); --line-focus: rgba(0,0,0,0.28); --fg: rgba(0,0,0,0.84); --muted: rgba(0,0,0,0.55); --faint: rgba(0,0,0,0.38); }\n@media (prefers-color-scheme: dark) { :root { --bg: #1e1e1e; --field: rgba(255,255,255,0.06); --field-hover: rgba(255,255,255,0.09); --field-focus: #262626; --line: rgba(255,255,255,0.09); --line-focus: rgba(255,255,255,0.28); --fg: rgba(255,255,255,0.92); --muted: rgba(255,255,255,0.55); --faint: rgba(255,255,255,0.36); } }"
        }
    }
}

/// Renderiza la página HTML minimalista para `dot://newtab` / `plasma://newtab`.
pub fn render_newtab_html(data: &NewTabData) -> String {
    let mut html = String::with_capacity(3072);
    let engine_name = escape_html(&data.search_engine_name);
    let action_url = escape_html(&data.search_action_url);
    let shortcuts = if data.shortcuts.is_empty() {
        default_shortcuts()
    } else {
        data.shortcuts.clone()
    };

    let _ = write!(
        html,
        r#"<!DOCTYPE html>
<html lang="es">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Dot</title>
<style>
{}
*, *::before, *::after {{ box-sizing: border-box; }}
html, body {{
  margin: 0;
  height: 100%;
  background: var(--bg);
  color: var(--fg);
  font-family: "Adwaita Sans", "Cantarell", "Noto Sans", system-ui, -apple-system, sans-serif;
  font-size: 14px;
  line-height: 1.5;
  -webkit-font-smoothing: antialiased;
}}
body {{
  display: grid;
  grid-template-rows: 1fr auto;
  min-height: 100dvh;
}}
main {{
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  width: 100%;
  max-width: 520px;
  margin: 0 auto;
  padding: 24px 20px 64px;
}}
.dot-mark {{
  display: block;
  margin-bottom: 26px;
  color: var(--fg);
}}
.search-form {{
  width: 100%;
  position: relative;
}}
.search-ico {{
  position: absolute;
  left: 15px;
  top: 50%;
  transform: translateY(-50%);
  width: 15px;
  height: 15px;
  stroke: var(--muted);
  stroke-width: 1.7;
  fill: none;
  pointer-events: none;
}}
.search-input {{
  width: 100%;
  height: 40px;
  padding: 0 16px 0 40px;
  border-radius: 999px;
  border: 1px solid var(--line);
  background: var(--field);
  color: var(--fg);
  font: inherit;
  font-size: 14px;
  outline: none;
  transition: border-color 140ms ease, background-color 140ms ease;
}}
.search-input::placeholder {{ color: var(--muted); }}
.search-input:hover {{ background: var(--field-hover); }}
.search-input:focus {{ border-color: var(--line-focus); background: var(--field-focus); }}
.dial {{
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 6px;
  margin-top: 24px;
  width: 100%;
}}
.dial-item {{
  display: inline-flex;
  align-items: baseline;
  gap: 6px;
  padding: 6px 12px;
  border-radius: 999px;
  color: var(--fg);
  text-decoration: none;
  font-size: 13px;
  font-weight: 450;
  transition: background-color 120ms ease;
}}
.dial-item:hover {{ background: var(--field-hover); }}
.dial-host {{ color: var(--muted); font-size: 11.5px; font-weight: 400; }}
.reading {{
  width: 100%;
  margin-top: 28px;
  list-style: none;
  padding: 0;
}}
.reading li {{
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: 12px;
  padding: 6px 10px;
  border-radius: 8px;
}}
.reading li:hover {{ background: var(--field-hover); }}
.reading a {{
  color: var(--fg);
  text-decoration: none;
  font-size: 13px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}}
.reading span {{ color: var(--muted); font-size: 12px; font-variant-numeric: tabular-nums; flex-shrink: 0; }}
footer {{
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  align-items: center;
  gap: 12px 20px;
  padding: 14px 24px;
  font-size: 11.5px;
  color: var(--faint);
}}
.foot-group {{
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 14px;
}}
.ws-active {{ color: var(--muted); font-weight: 500; }}
.hint {{ display: inline-flex; align-items: center; gap: 5px; }}
.hint kbd {{
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 10.5px;
  color: var(--muted);
}}
</style>
</head>
<body>
<main>
  <svg class="dot-mark" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="22" height="22" role="img" aria-label="Dot"><circle cx="12" cy="12" r="9" fill="currentColor"/></svg>
  <form class="search-form" method="get" action="{action_url}">
    <svg class="search-ico" viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="7"/><path d="M20 20l-3.5-3.5"/></svg>
    <input class="search-input" type="search" name="q" placeholder="Buscar con {engine_name}…" autofocus autocomplete="off" aria-label="Buscar">
  </form>
"#,
        theme_css(data.theme)
    );

    if !data.speed_dial.is_empty() {
        html.push_str("  <nav class=\"dial\" aria-label=\"Accesos rápidos\">\n");
        for item in &data.speed_dial {
            let host = short_host(&item.url);
            let _ = writeln!(
                html,
                "    <a class=\"dial-item\" href=\"{}\"><span>{}</span><span class=\"dial-host\">{}</span></a>",
                escape_html(&item.url),
                escape_html(&item.title),
                escape_html(&host)
            );
        }
        html.push_str("  </nav>\n");
    }

    if !data.reading_list_preview.is_empty() {
        html.push_str("  <ul class=\"reading\" aria-label=\"Lista de lectura\">\n");
        for item in &data.reading_list_preview {
            let _ = writeln!(
                html,
                "    <li><a href=\"{}\">{}</a><span>{} min</span></li>",
                escape_html(&item.url),
                escape_html(&item.title),
                item.estimated_minutes
            );
        }
        html.push_str("  </ul>\n");
    }

    html.push_str("</main>\n<footer>\n  <div class=\"foot-group\">\n");
    for ws in &data.workspaces {
        let cls = if ws.active { "ws-active" } else { "" };
        let _ = writeln!(
            html,
            "    <span class=\"{cls}\" data-workspace-id=\"{}\">{} ({})</span>",
            escape_html(&ws.id),
            escape_html(&ws.name),
            ws.tab_count
        );
    }
    if data.recently_closed_count > 0 {
        let _ = writeln!(
            html,
            "    <span>Cerradas: {}</span>",
            data.recently_closed_count
        );
    }
    html.push_str("  </div>\n  <div class=\"foot-group\">\n");
    for sc in shortcuts.iter().take(4) {
        let _ = writeln!(
            html,
            "    <span class=\"hint\"><kbd>{}</kbd> {}</span>",
            escape_html(&sc.keys),
            escape_html(&sc.description)
        );
    }
    html.push_str("  </div>\n</footer>\n</body>\n</html>\n");
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_dot_minimal_newtab_and_escapes_html() {
        let data = NewTabData {
            theme: NewTabTheme::Dark,
            search_engine_name: "Search <Test>".to_string(),
            search_action_url: "https://duckduckgo.com/".to_string(),
            speed_dial: vec![SpeedDialItem {
                title: "Docs & <Rust>".to_string(),
                url: "https://docs.rs/tokio".to_string(),
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
        assert!(html.contains("Search &lt;Test&gt;"));
        assert!(html.contains("Docs &amp; &lt;Rust&gt;"));
        assert!(html.contains("docs.rs"));
        assert!(html.contains("Dev (4)"));
        assert!(html.contains("Cerradas: 3"));
        assert!(html.contains("Async Rust"));
        assert!(html.contains("6 min"));
        assert!(html.contains("<kbd>Ctrl+Shift+T</kbd>"));
        assert!(html.contains("--bg: #1e1e1e;"));
        assert!(!html.contains("#4f8cff"));
        assert!(!html.contains("<h2>"));
    }

    #[test]
    fn light_and_system_themes_emit_expected_css() {
        let light = render_newtab_html(&NewTabData {
            theme: NewTabTheme::Light,
            ..Default::default()
        });
        assert!(light.contains("--bg: #ffffff;"));
        assert!(!light.contains("prefers-color-scheme: dark"));
        assert!(!light.contains("Cerradas:"));

        let system = render_newtab_html(&NewTabData::default());
        assert!(system.contains("prefers-color-scheme: dark"));
    }
}
