//! Modo lectura: saneamiento puro en Rust del artículo extraído, cálculo de tiempo de lectura
//! y plantilla tipográfica limpia claro/oscuro con tamaño de letra configurable (`Reader.swift`).

use crate::domain::site_host;
use crate::settings::ColorLook;
use serde::{Deserialize, Serialize};

/// Umbral mínimo de caracteres de texto para considerar una página como artículo legible.
pub const MIN_ARTICLE_TEXT_CHARS: usize = 250;

/// Marcador HTML que identifica documentos del modo lectura como internos ante `BrowserView`.
pub const READER_INTERNAL_MARKER: &str = "<!--search-internal--><!--search-reader-->";

/// Tema visual del modo lectura.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderTheme {
    Paper,
    Sepia,
    Charcoal,
    Amoled,
}

impl ReaderTheme {
    pub fn as_attr(self) -> &'static str {
        match self {
            Self::Paper => "paper",
            Self::Sepia => "sepia",
            Self::Charcoal => "charcoal",
            Self::Amoled => "amoled",
        }
    }
}

/// Familia tipográfica del cuerpo en modo lectura.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReaderFont {
    Sans,
    #[default]
    Serif,
    Mono,
}

impl ReaderFont {
    pub fn css_stack(self) -> &'static str {
        match self {
            Self::Sans => r#"system-ui, -apple-system, "Cantarell", "Inter", sans-serif"#,
            Self::Serif => {
                r#"ui-serif, Georgia, "Liberation Serif", "DejaVu Serif", "Times New Roman", serif"#
            }
            Self::Mono => r#"ui-monospace, "JetBrains Mono", "DejaVu Sans Mono", monospace"#,
        }
    }
}

/// Medida máxima de línea en caracteres (`ch`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReaderMeasure {
    Narrow,
    #[default]
    Medium,
    Wide,
}

impl ReaderMeasure {
    pub fn ch(self) -> u16 {
        match self {
            Self::Narrow => 58,
            Self::Medium => 68,
            Self::Wide => 78,
        }
    }
}

/// Interlineado unitless del cuerpo en modo lectura.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReaderLineHeight {
    Compact,
    #[default]
    Normal,
    Relaxed,
}

impl ReaderLineHeight {
    pub fn css_value(self) -> &'static str {
        match self {
            Self::Compact => "1.5",
            Self::Normal => "1.7",
            Self::Relaxed => "1.9",
        }
    }
}

/// Opciones de presentación del modo lectura.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReaderOptions {
    pub look: ColorLook,
    pub theme: Option<ReaderTheme>,
    pub font: ReaderFont,
    pub measure: ReaderMeasure,
    pub line_height: ReaderLineHeight,
    pub font_size_px: i32,
}

impl Default for ReaderOptions {
    fn default() -> Self {
        Self {
            look: ColorLook::System,
            theme: None,
            font: ReaderFont::Serif,
            measure: ReaderMeasure::Medium,
            line_height: ReaderLineHeight::Normal,
            font_size_px: 18,
        }
    }
}

impl ReaderOptions {
    pub fn clamped_font_size(self) -> i32 {
        self.font_size_px.clamp(12, 36)
    }
}

/// Artículo extraído de la página por el extractor en página (`third_party/readability/readability.js`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReaderArticle {
    pub title: String,
    #[serde(default)]
    pub site_name: String,
    #[serde(default)]
    pub byline: Option<String>,
    #[serde(default)]
    pub url: String,
    pub content_html: String,
    #[serde(default)]
    pub text_length: usize,
}

impl ReaderArticle {
    /// Verifica si el artículo tiene contenido sustancial para abrir el modo lectura.
    pub fn is_readable(&self) -> bool {
        let len = if self.text_length > 0 {
            self.text_length
        } else {
            visible_text_len(&self.content_html)
        };
        len >= MIN_ARTICLE_TEXT_CHARS && !self.content_html.trim().is_empty()
    }

    /// Cuenta las palabras visibles del artículo a partir del HTML extraído.
    pub fn word_count(&self) -> usize {
        let mut in_tag = false;
        let mut words = 0usize;
        let mut in_word = false;
        for ch in self.content_html.chars() {
            match ch {
                '<' => {
                    in_tag = true;
                    if in_word {
                        words += 1;
                        in_word = false;
                    }
                }
                '>' => in_tag = false,
                _ if !in_tag => {
                    if ch.is_whitespace() {
                        if in_word {
                            words += 1;
                            in_word = false;
                        }
                    } else {
                        in_word = true;
                    }
                }
                _ => {}
            }
        }
        if in_word {
            words += 1;
        }
        if words > 0 {
            words
        } else {
            self.text_length / 5
        }
    }

    /// Estima el tiempo de lectura en minutos (220 palabras/minuto, mínimo 1 min).
    pub fn estimated_minutes(&self) -> u32 {
        let words = self.word_count();
        ((words as f64) / 220.0).round().max(1.0) as u32
    }

    pub fn display_source(&self) -> String {
        let trimmed = self.site_name.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
        site_host(&self.url).unwrap_or_default()
    }
}

/// Escapa caracteres especiales HTML (`&`, `<`, `>`, `"`, `'`).
pub fn escape_html(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
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

/// Estima la longitud de texto visible eliminando etiquetas HTML simples.
pub fn visible_text_len(html: &str) -> usize {
    let mut in_tag = false;
    let mut count = 0usize;
    let mut prev_ws = true;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => {
                if ch.is_whitespace() {
                    if !prev_ws {
                        count += 1;
                        prev_ws = true;
                    }
                } else {
                    count += 1;
                    prev_ws = false;
                }
            }
            _ => {}
        }
    }
    count
}

const DROP_SUBTREE_TAGS: &[&str] = &[
    "script", "style", "noscript", "template", "object", "embed", "applet", "form", "input",
    "button", "select", "textarea", "svg", "canvas",
];

const ALLOWED_TAGS: &[&str] = &[
    "article",
    "section",
    "div",
    "p",
    "br",
    "hr",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "blockquote",
    "pre",
    "code",
    "kbd",
    "samp",
    "ul",
    "ol",
    "li",
    "dl",
    "dt",
    "dd",
    "figure",
    "figcaption",
    "picture",
    "source",
    "img",
    "video",
    "audio",
    "table",
    "thead",
    "tbody",
    "tfoot",
    "tr",
    "th",
    "td",
    "caption",
    "a",
    "strong",
    "b",
    "em",
    "i",
    "u",
    "s",
    "del",
    "ins",
    "mark",
    "small",
    "sub",
    "sup",
    "abbr",
    "cite",
    "q",
    "time",
    "span",
    "details",
    "summary",
    "iframe",
];

const VOID_TAGS: &[&str] = &["br", "hr", "img", "source"];

const ALLOWED_IFRAME_HOSTS: &[&str] = &[
    "youtube.com",
    "youtube-nocookie.com",
    "youtu.be",
    "vimeo.com",
    "player.vimeo.com",
    "dailymotion.com",
    "loom.com",
    "streamable.com",
    "wistia.com",
    "ted.com",
    "flo.uri.sh",
    "datawrapper.dwcdn.net",
    "dwcdn.net",
    "infogram.com",
    "public.tableau.com",
    "ig.ft.com",
];

/// Sanea HTML de un artículo en Rust puro (defensa en profundidad frente a XSS, scripts,
/// estilos en línea, eventos `on*`, URLs `javascript:` e iframes no permitidos).
pub fn sanitize_article_html(raw_html: &str) -> String {
    let mut out = String::with_capacity(raw_html.len());
    let bytes = raw_html.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;
    let mut drop_stack: Vec<String> = Vec::new();

    while i < len {
        if bytes[i] != b'<' {
            let start = i;
            while i < len && bytes[i] != b'<' {
                i += 1;
            }
            if drop_stack.is_empty() {
                out.push_str(&raw_html[start..i]);
            }
            continue;
        }

        // Comentario HTML `<!-- ... -->`.
        if raw_html[i..].starts_with("<!--") {
            if let Some(end_rel) = raw_html[i + 4..].find("-->") {
                i += 4 + end_rel + 3;
            } else {
                break;
            }
            continue;
        }

        // Buscar cierre `>` respetando comillas.
        let Some(tag_end) = find_tag_end(raw_html, i + 1) else {
            break;
        };
        let inside = raw_html[i + 1..tag_end].trim();
        i = tag_end + 1;

        if inside.is_empty() || inside.starts_with('!') || inside.starts_with('?') {
            continue;
        }

        let is_closing = inside.starts_with('/');
        let body = if is_closing {
            inside[1..].trim()
        } else {
            inside
        };
        let self_closing = !is_closing && body.ends_with('/');
        let body = body.trim_end_matches('/').trim();

        let name_end = body
            .find(|c: char| c.is_ascii_whitespace() || c == '/')
            .unwrap_or(body.len());
        let tag_name = body[..name_end].to_ascii_lowercase();
        let attr_str = body[name_end..].trim();

        if tag_name.is_empty() {
            continue;
        }

        if is_closing {
            if let Some(pos) = drop_stack.iter().rposition(|t| t == &tag_name) {
                drop_stack.truncate(pos);
            } else if drop_stack.is_empty()
                && ALLOWED_TAGS.contains(&tag_name.as_str())
                && !VOID_TAGS.contains(&tag_name.as_str())
            {
                out.push_str("</");
                out.push_str(&tag_name);
                out.push('>');
            }
            continue;
        }

        if DROP_SUBTREE_TAGS.contains(&tag_name.as_str()) {
            if !self_closing && !VOID_TAGS.contains(&tag_name.as_str()) {
                drop_stack.push(tag_name);
            }
            continue;
        }

        if !drop_stack.is_empty() {
            continue;
        }

        if !ALLOWED_TAGS.contains(&tag_name.as_str()) {
            // Desenvolver etiquetas desconocidas manteniendo su texto interior.
            continue;
        }

        let attrs = parse_html_attributes(attr_str);
        if tag_name == "iframe" {
            let src = attrs
                .iter()
                .find(|(k, _)| k == "src")
                .map(|(_, v)| v.as_str())
                .unwrap_or("");
            if !is_allowed_iframe_src(src) {
                if !self_closing {
                    drop_stack.push(tag_name);
                }
                continue;
            }
        }

        out.push('<');
        out.push_str(&tag_name);
        for (name, val) in attrs {
            if let Some(clean_val) = sanitize_attribute(&tag_name, &name, &val) {
                out.push(' ');
                out.push_str(&name);
                if !clean_val.is_empty()
                    || name != "controls" && name != "open" && name != "playsinline"
                {
                    out.push_str("=\"");
                    out.push_str(&escape_html(&clean_val));
                    out.push('"');
                }
            }
        }
        if tag_name == "a" {
            out.push_str(" rel=\"noopener noreferrer\"");
        }
        out.push('>');
    }

    out
}

fn find_tag_end(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = start;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) => {
                if b == q {
                    quote = None;
                }
            }
            None => match b {
                b'"' | b'\'' => quote = Some(b),
                b'>' => return Some(i),
                _ => {}
            },
        }
        i += 1;
    }
    None
}

fn parse_html_attributes(raw: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let bytes = raw.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;

    while i < len {
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= len {
            break;
        }
        let name_start = i;
        while i < len && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' && bytes[i] != b'/' {
            i += 1;
        }
        let name = raw[name_start..i].to_ascii_lowercase();
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if i < len && bytes[i] == b'=' {
            i += 1;
            while i < len && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < len && (bytes[i] == b'"' || bytes[i] == b'\'') {
                let q = bytes[i];
                i += 1;
                let val_start = i;
                while i < len && bytes[i] != q {
                    i += 1;
                }
                value = decode_basic_entities(&raw[val_start..i]);
                if i < len {
                    i += 1;
                }
            } else {
                let val_start = i;
                while i < len && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                value = decode_basic_entities(&raw[val_start..i]);
            }
        }
        if !name.is_empty() {
            attrs.push((name, value));
        }
    }

    attrs
}

fn decode_basic_entities(raw: &str) -> String {
    raw.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn sanitize_attribute(tag: &str, name: &str, value: &str) -> Option<String> {
    if name.starts_with("on") || name == "style" || name == "srcdoc" {
        return None;
    }
    let val = value.trim();
    match name {
        "href" if tag == "a" => is_safe_url(val, false).then(|| val.to_string()),
        "src" if matches!(tag, "img" | "video" | "audio" | "source") => {
            is_safe_url(val, tag == "img").then(|| val.to_string())
        }
        "src" if tag == "iframe" => is_allowed_iframe_src(val).then(|| val.to_string()),
        "poster" if tag == "video" => is_safe_url(val, true).then(|| val.to_string()),
        "srcset" if matches!(tag, "img" | "source") => sanitize_srcset(val),
        "alt" | "title" | "cite" | "datetime" | "lang" | "dir" => Some(val.to_string()),
        "colspan" | "rowspan" | "width" | "height" => val
            .chars()
            .all(|c| c.is_ascii_digit())
            .then(|| val.to_string()),
        "controls" | "playsinline" | "open" => Some(String::new()),
        "preload" if matches!(tag, "video" | "audio") => {
            matches!(val, "none" | "metadata" | "auto").then(|| val.to_string())
        }
        "class" => {
            let safe_classes: Vec<&str> = val
                .split_whitespace()
                .filter(|c| c.starts_with("office-"))
                .collect();
            if safe_classes.is_empty() {
                None
            } else {
                Some(safe_classes.join(" "))
            }
        }
        _ => None,
    }
}

fn sanitize_srcset(raw: &str) -> Option<String> {
    let mut kept = Vec::new();
    for part in raw.split(',') {
        let trimmed = part.trim();
        let mut iter = trimmed.split_whitespace();
        let Some(url_part) = iter.next() else {
            continue;
        };
        if is_safe_url(url_part, true) {
            kept.push(trimmed.to_string());
        }
    }
    if kept.is_empty() {
        None
    } else {
        Some(kept.join(", "))
    }
}

fn is_safe_url(raw: &str, allow_raster_data: bool) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return false;
    }
    // Bloquear esquemas peligrosos incluso con espacios/caracteres de control intercalados.
    let collapsed: String = trimmed
        .chars()
        .filter(|c| !c.is_ascii_control() && !c.is_ascii_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();

    if collapsed.starts_with("javascript:")
        || collapsed.starts_with("vbscript:")
        || collapsed.starts_with("file:")
    {
        return false;
    }
    if collapsed.starts_with("data:") {
        if !allow_raster_data {
            return false;
        }
        return collapsed.starts_with("data:image/png;")
            || collapsed.starts_with("data:image/jpeg;")
            || collapsed.starts_with("data:image/jpg;")
            || collapsed.starts_with("data:image/webp;")
            || collapsed.starts_with("data:image/gif;")
            || collapsed.starts_with("data:image/avif;");
    }
    if collapsed.starts_with("http://")
        || collapsed.starts_with("https://")
        || collapsed.starts_with('/')
        || collapsed.starts_with('#')
    {
        return true;
    }
    // URL relativa sin esquema explícito (`images/photo.jpg`).
    !collapsed.contains(':')
}

fn is_allowed_iframe_src(raw: &str) -> bool {
    let Ok(parsed) = url::Url::parse(raw.trim()) else {
        return false;
    };
    if !matches!(parsed.scheme(), "https" | "http") {
        return false;
    }
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let h = host.to_ascii_lowercase();
    ALLOWED_IFRAME_HOSTS
        .iter()
        .any(|allowed| h == *allowed || h.ends_with(&format!(".{allowed}")))
}

/// Hoja de estilos limpia y legible del modo lectura (`Reader.swift` adaptada a Linux).
pub const READER_CSS: &str = r#"
:root {
  color-scheme: light;
  --r-ground: #ffffff;
  --r-ink: #171717;
  --r-quiet: #737373;
  --r-aside: #525252;
  --r-rule: #e6e6e6;
  --r-code: #f5f5f5;
  --r-accent: #2563eb;
  --r-picture: none;
  --r-font-size: 18px;
  --r-font-family: ui-serif, Georgia, "Liberation Serif", "DejaVu Serif", "Times New Roman", serif;
  --r-measure: 68ch;
  --r-line-height: 1.7;
}
@media (prefers-color-scheme: dark) {
  :root:not([data-office-look="light"]):not([data-office-theme]) {
    color-scheme: dark;
    --r-ground: #1c1c1c;
    --r-ink: #e0e0e0;
    --r-quiet: #8a8a8a;
    --r-aside: #a8a8a8;
    --r-rule: #333333;
    --r-code: #2a2a2a;
    --r-accent: #60a5fa;
    --r-picture: brightness(.92);
  }
}
:root[data-office-look="dark"] {
  color-scheme: dark;
  --r-ground: #1c1c1c;
  --r-ink: #e0e0e0;
  --r-quiet: #8a8a8a;
  --r-aside: #a8a8a8;
  --r-rule: #333333;
  --r-code: #2a2a2a;
  --r-accent: #60a5fa;
  --r-picture: brightness(.92);
}
:root[data-office-theme="paper"] {
  color-scheme: light;
  --r-ground: #fcfbf8;
  --r-ink: #1b1a17;
  --r-quiet: #6f6b63;
  --r-aside: #4b4842;
  --r-rule: #e7e3da;
  --r-code: #f3f0e8;
  --r-accent: #2563eb;
  --r-picture: none;
}
:root[data-office-theme="sepia"] {
  color-scheme: light;
  --r-ground: #f4ecd8;
  --r-ink: #3b2e1e;
  --r-quiet: #7a6852;
  --r-aside: #594835;
  --r-rule: #dfd3b8;
  --r-code: #eadfc6;
  --r-accent: #9a5b13;
  --r-picture: sepia(.08);
}
:root[data-office-theme="charcoal"] {
  color-scheme: dark;
  --r-ground: #1e1f22;
  --r-ink: #e2e4e9;
  --r-quiet: #8e939e;
  --r-aside: #b0b5bf;
  --r-rule: #32353b;
  --r-code: #282a2f;
  --r-accent: #60a5fa;
  --r-picture: brightness(.92);
}
:root[data-office-theme="amoled"] {
  color-scheme: dark;
  --r-ground: #000000;
  --r-ink: #e4e4e7;
  --r-quiet: #82828a;
  --r-aside: #a1a1aa;
  --r-rule: #222226;
  --r-code: #121214;
  --r-accent: #60a5fa;
  --r-picture: brightness(.88);
}
html, body {
  background: var(--r-ground);
  color: var(--r-ink);
  margin: 0;
  padding: 0;
  -webkit-font-smoothing: antialiased;
}
#office-reader-progress {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  height: 3px;
  z-index: 9999;
  background: transparent;
  pointer-events: none;
}
#office-reader-progress-bar {
  height: 100%;
  width: 0%;
  background: var(--r-accent);
  transition: width 80ms linear;
}
#office-reader {
  max-width: var(--r-measure);
  margin: 0 auto;
  padding: 64px 24px 140px;
  font-family: var(--r-font-family);
  font-size: var(--r-font-size);
  line-height: var(--r-line-height);
  color: var(--r-ink);
  word-wrap: break-word;
}
#office-reader h1 {
  font-family: system-ui, -apple-system, "Cantarell", "Inter", sans-serif;
  font-weight: 650;
  font-size: 1.72em;
  line-height: 1.24;
  margin: 0 0 10px;
  letter-spacing: -0.015em;
}
#office-reader .office-from {
  font-family: system-ui, -apple-system, "Cantarell", sans-serif;
  font-size: 12px;
  line-height: 1.4;
  color: var(--r-quiet);
  margin: 0 0 28px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  font-variant-numeric: tabular-nums;
}
#office-reader .office-byline {
  font-family: system-ui, -apple-system, "Cantarell", sans-serif;
  font-size: 0.82em;
  line-height: 1.5;
  color: var(--r-aside);
  margin: -18px 0 30px;
}
#office-reader p {
  margin: 0 0 1.35em;
}
#office-reader img, #office-reader video, #office-reader iframe {
  max-width: 100%;
  height: auto;
  border-radius: 6px;
  margin: 1.6em 0;
  display: block;
}
#office-reader img {
  filter: var(--r-picture);
}
#office-reader iframe {
  width: 100%;
  aspect-ratio: 16 / 9;
  border: 0;
}
#office-reader figure {
  margin: 1.8em 0;
}
#office-reader figcaption {
  font-family: system-ui, -apple-system, "Cantarell", sans-serif;
  font-size: 13px;
  line-height: 1.5;
  color: var(--r-quiet);
  margin-top: 0.6em;
}
#office-reader a {
  color: var(--r-ink);
  text-underline-offset: 3px;
}
#office-reader h2, #office-reader h3 {
  font-family: system-ui, -apple-system, "Cantarell", sans-serif;
  font-weight: 600;
  font-size: 1.18em;
  line-height: 1.32;
  margin: 2em 0 0.6em;
}
#office-reader h4, #office-reader h5, #office-reader h6 {
  font-family: system-ui, -apple-system, "Cantarell", sans-serif;
  font-weight: 600;
  font-size: 0.96em;
  line-height: 1.4;
  margin: 1.7em 0 0.5em;
}
#office-reader pre, #office-reader code, #office-reader kbd {
  font-family: ui-monospace, "JetBrains Mono", "DejaVu Sans Mono", monospace;
  font-size: 0.82em;
}
#office-reader pre {
  background: var(--r-code);
  padding: 14px 16px;
  border-radius: 8px;
  overflow-x: auto;
  line-height: 1.55;
}
#office-reader :not(pre) > code {
  background: var(--r-code);
  padding: 0.12em 0.35em;
  border-radius: 4px;
}
#office-reader blockquote {
  margin: 1.6em 0;
  padding-left: 1.2em;
  border-left: 2px solid var(--r-rule);
  color: var(--r-aside);
}
#office-reader ul, #office-reader ol {
  margin: 0 0 1.35em;
  padding-left: 1.45em;
}
#office-reader ul { list-style: disc outside; }
#office-reader ol { list-style: decimal outside; }
#office-reader li { margin: 0.32em 0; }
#office-reader hr {
  border: 0;
  border-top: 1px solid var(--r-rule);
  margin: 2.4em 0;
}
#office-reader table {
  border-collapse: collapse;
  width: 100%;
  margin: 1.6em 0;
  font-family: system-ui, -apple-system, "Cantarell", sans-serif;
  font-size: 0.85em;
}
#office-reader th, #office-reader td {
  border-bottom: 1px solid var(--r-rule);
  padding: 8px 12px 8px 0;
  text-align: left;
  vertical-align: top;
}
#office-reader th { font-weight: 600; }
@media print {
  :root, :root[data-office-look], :root[data-office-theme] {
    color-scheme: light !important;
    --r-ground: #ffffff !important;
    --r-ink: #000000 !important;
    --r-quiet: #444444 !important;
    --r-aside: #333333 !important;
    --r-rule: #cccccc !important;
    --r-code: #f5f5f5 !important;
    --r-picture: none !important;
  }
  #office-reader-progress { display: none !important; }
  html, body { background: #ffffff !important; color: #000000 !important; }
  #office-reader {
    max-width: 100% !important;
    margin: 0 !important;
    padding: 0 !important;
    font-size: 11pt !important;
    line-height: 1.5 !important;
  }
  #office-reader h1, #office-reader h2, #office-reader h3, #office-reader h4 {
    break-after: avoid;
  }
  #office-reader pre, #office-reader blockquote, #office-reader figure, #office-reader img, #office-reader table {
    break-inside: avoid;
  }
  #office-reader a { text-decoration: underline; }
}
"#;

/// Renderiza el documento HTML5 completo y saneado para mostrar el artículo en modo lectura.
pub fn render_reader_document(article: &ReaderArticle, options: ReaderOptions) -> String {
    let safe_title = escape_html(if article.title.trim().is_empty() {
        "Modo lectura"
    } else {
        article.title.trim()
    });
    let font_size = options.clamped_font_size();
    let font_family = options.font.css_stack();
    let measure_ch = options.measure.ch();
    let line_height = options.line_height.css_value();
    let look_attr = match options.look {
        ColorLook::System => String::new(),
        ColorLook::Light => r#" data-office-look="light""#.to_string(),
        ColorLook::Dark => r#" data-office-look="dark""#.to_string(),
    };
    let theme_attr = match options.theme {
        Some(t) => format!(r#" data-office-theme="{}""#, t.as_attr()),
        None => String::new(),
    };
    let base_tag = if article.url.starts_with("http://") || article.url.starts_with("https://") {
        format!("<base href=\"{}\">", escape_html(&article.url))
    } else {
        String::new()
    };

    let source = article.display_source();
    let words = article.word_count();
    let minutes = article.estimated_minutes();
    let meta_line = if source.is_empty() {
        format!("{words} palabras · {minutes} min de lectura")
    } else {
        format!("{source} · {words} palabras · {minutes} min de lectura")
    };

    let byline_html = article
        .byline
        .as_deref()
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .map(|b| format!("<p class=\"office-byline\">{}</p>", escape_html(b)))
        .unwrap_or_default();

    let clean_body = sanitize_article_html(&article.content_html);

    format!(
        r#"{READER_INTERNAL_MARKER}<!doctype html>
<html{look_attr}{theme_attr} style="--r-font-size:{font_size}px;--r-font-family:{font_family};--r-measure:{measure_ch}ch;--r-line-height:{line_height}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
{base_tag}
<title>{safe_title}</title>
<style id="office-reader-sheet">{READER_CSS}</style>
</head>
<body>
  <div id="office-reader-progress" aria-hidden="true"><div id="office-reader-progress-bar"></div></div>
  <article id="office-reader">
    <h1>{safe_title}</h1>
    <p class="office-from">{meta_line}</p>
    {byline_html}
    <div class="office-reader-body">{clean_body}</div>
  </article>
  <script>(function(){{var bar=document.getElementById('office-reader-progress-bar');if(!bar)return;function u(){{var doc=document.documentElement;var max=doc.scrollHeight-window.innerHeight;var pct=max>0?Math.min(100,Math.max(0,(window.scrollY/max)*100)):0;bar.style.width=pct.toFixed(1)+'%';}}window.addEventListener('scroll',u,{{passive:true}});window.addEventListener('resize',u,{{passive:true}});u();}})();</script>
</body>
</html>"#,
        meta_line = escape_html(&meta_line)
    )
}

/// Script mínimo para actualizar en vivo el tema, tipografía, medida, interlineado y tamaño
/// dentro de una vista de modo lectura ya abierta sin recargar el documento.
pub fn build_apply_reader_options_script(options: ReaderOptions) -> String {
    let font_size = options.clamped_font_size();
    let font_family = options.font.css_stack();
    let measure_ch = options.measure.ch();
    let line_height = options.line_height.css_value();
    let look_js = match options.look {
        ColorLook::System => "document.documentElement.removeAttribute('data-office-look');",
        ColorLook::Light => "document.documentElement.setAttribute('data-office-look','light');",
        ColorLook::Dark => "document.documentElement.setAttribute('data-office-look','dark');",
    };
    let theme_js = match options.theme {
        Some(t) => format!(
            "document.documentElement.setAttribute('data-office-theme','{}');",
            t.as_attr()
        ),
        None => "document.documentElement.removeAttribute('data-office-theme');".to_string(),
    };
    format!(
        "(function(){{var s=document.documentElement.style;{look_js}{theme_js}s.setProperty('--r-font-size','{font_size}px');s.setProperty('--r-font-family',{font_family_js});s.setProperty('--r-measure','{measure_ch}ch');s.setProperty('--r-line-height','{line_height}');}})();",
        font_family_js = serde_json::to_string(font_family).unwrap_or_else(|_| "\"serif\"".to_string())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_scripts_event_handlers_javascript_urls_and_untrusted_iframes() {
        let dirty = r#"
            <p onclick="alert(1)" style="color:red">Texto <strong>limpio</strong>.</p>
            <script>window.pwned = true;</script>
            <a href="javascript:alert(2)">malo</a>
            <a href="https://example.com/post">bueno</a>
            <img src="https://example.com/pic.jpg" onerror="evil()" alt="Foto">
            <img src="data:image/svg+xml,<svg onload=alert(1)>">
            <iframe src="https://ads.evil.test/frame"></iframe>
            <iframe src="https://www.youtube.com/embed/abc123"></iframe>
        "#;
        let clean = sanitize_article_html(dirty);
        assert!(!clean.contains("<script"));
        assert!(!clean.contains("pwned"));
        assert!(!clean.contains("onclick"));
        assert!(!clean.contains("onerror"));
        assert!(!clean.contains("style="));
        assert!(!clean.contains("javascript:"));
        assert!(!clean.contains("svg+xml"));
        assert!(!clean.contains("ads.evil.test"));
        assert!(clean.contains("<strong>limpio</strong>"));
        assert!(clean.contains("https://example.com/post"));
        assert!(clean.contains("https://example.com/pic.jpg"));
        assert!(clean.contains("https://www.youtube.com/embed/abc123"));
    }

    #[test]
    fn renders_clean_document_with_theme_font_size_and_reading_time() {
        let body = format!(
            "<p>{}</p>",
            "Palabra de prueba para un artículo largo. ".repeat(30)
        );
        let article = ReaderArticle {
            title: "Un título <seguro>".to_string(),
            site_name: String::new(),
            byline: Some("Por Ada Lovelace".to_string()),
            url: "https://www.example.org/blog/rust".to_string(),
            content_html: body,
            text_length: 1200,
        };
        assert!(article.is_readable());
        assert_eq!(article.display_source(), "example.org");
        assert!(article.estimated_minutes() >= 1);

        let html = render_reader_document(
            &article,
            ReaderOptions {
                look: ColorLook::Dark,
                theme: Some(ReaderTheme::Sepia),
                font: ReaderFont::Sans,
                measure: ReaderMeasure::Wide,
                line_height: ReaderLineHeight::Relaxed,
                font_size_px: 22,
            },
        );
        assert!(html.starts_with(READER_INTERNAL_MARKER));
        assert!(html.contains(r#"data-office-look="dark""#));
        assert!(html.contains(r#"data-office-theme="sepia""#));
        assert!(html.contains("--r-font-size:22px"));
        assert!(html.contains("--r-measure:78ch"));
        assert!(html.contains("--r-line-height:1.9"));
        assert!(html.contains("max-width: var(--r-measure);"));
        assert!(html.contains("office-reader-progress-bar"));
        assert!(html.contains("@media print"));
        assert!(html.contains("Un título &lt;seguro&gt;"));
        assert!(html.contains("example.org · 210 palabras · 1 min de lectura"));
        assert!(html.contains("Por Ada Lovelace"));
        assert!(html.contains(r#"<base href="https://www.example.org/blog/rust">"#));
    }

    #[test]
    fn rejects_short_non_article_content() {
        let short = ReaderArticle {
            title: "Inicio".to_string(),
            site_name: "example.com".to_string(),
            byline: None,
            url: "https://example.com/".to_string(),
            content_html: "<p>Hola</p>".to_string(),
            text_length: 4,
        };
        assert!(!short.is_readable());
    }

    #[test]
    fn live_options_script_clamps_font_size_and_sets_look() {
        let js_dark = build_apply_reader_options_script(ReaderOptions {
            look: ColorLook::Dark,
            theme: Some(ReaderTheme::Amoled),
            font: ReaderFont::Mono,
            measure: ReaderMeasure::Narrow,
            line_height: ReaderLineHeight::Compact,
            font_size_px: 99,
        });
        assert!(js_dark.contains("setAttribute('data-office-look','dark')"));
        assert!(js_dark.contains("setAttribute('data-office-theme','amoled')"));
        assert!(js_dark.contains("36px"));
        assert!(js_dark.contains("58ch"));
        assert!(js_dark.contains("1.5"));

        let js_sys = build_apply_reader_options_script(ReaderOptions {
            look: ColorLook::System,
            theme: None,
            font: ReaderFont::Serif,
            measure: ReaderMeasure::Medium,
            line_height: ReaderLineHeight::Normal,
            font_size_px: 4,
        });
        assert!(js_sys.contains("removeAttribute('data-office-look')"));
        assert!(js_sys.contains("removeAttribute('data-office-theme')"));
        assert!(js_sys.contains("12px"));
        assert!(js_sys.contains("68ch"));
        assert!(js_sys.contains("1.7"));
    }
}
