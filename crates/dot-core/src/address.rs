//! Resolución de entradas de la barra de direcciones (URL, archivo local o consulta de búsqueda),
//! motores de búsqueda configurables, limpieza de parámetros de seguimiento (`gclid`, `fbclid`,
//! `msclkid`, `utm_*`), presentación segura sin credenciales embebidas y protección anti-homógrafos
//! IDN/Punycode (`Address.swift`, `Engine.swift`, `AtRest.swift`, `CleanLink.swift`).

use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, CONTROLS};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};
use url::Url;

/// Conjunto RFC 3986 para codificar un valor de parámetro de consulta:
/// solo quedan sin escapar `A-Z`, `a-z`, `0-9`, `-`, `.`, `_`, `~`.
const RFC3986_UNRESERVED_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'!')
    .add(b'"')
    .add(b'#')
    .add(b'$')
    .add(b'%')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

const ALLOWED_SCHEMES: &[&str] = &[
    "http", "https", "file", "about", "data", "search", "plasma", "dot",
];

/// Página interna del navegador reconocida bajo el esquema canónico `dot://` y los alias
/// retrocompatibles `plasma://` y `search://`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InternalPage {
    Settings { section: Option<String> },
    History,
    Bookmarks,
    Downloads,
    NewTab,
}

/// Reconoce una URL interna (`dot://settings`, `dot://settings/privacy`,
/// `dot://history`, `dot://bookmarks`, `dot://downloads`, `dot://newtab`
/// y sus alias `plasma://` y `search://`).
pub fn parse_internal_page(raw_url: &str) -> Option<InternalPage> {
    let trimmed = raw_url.trim();
    let (scheme_raw, rest_raw) = trimmed.split_once("://")?;
    let scheme = scheme_raw.to_ascii_lowercase();
    if scheme != "dot" && scheme != "plasma" && scheme != "search" {
        return None;
    }
    let rest = rest_raw.trim_matches('/');
    let mut parts = rest.split('/').filter(|s| !s.is_empty());
    let host = parts.next()?.to_ascii_lowercase();
    match host.as_str() {
        "settings" | "preferences" | "config" => {
            let section = parts.next().map(|s| s.to_ascii_lowercase());
            Some(InternalPage::Settings { section })
        }
        "history" => Some(InternalPage::History),
        "bookmarks" => Some(InternalPage::Bookmarks),
        "downloads" => Some(InternalPage::Downloads),
        "newtab" => Some(InternalPage::NewTab),
        _ => None,
    }
}

/// Devuelve la URL canónica `dot://...` para una página interna.
pub fn canonical_internal_url(page: &InternalPage) -> String {
    match page {
        InternalPage::Settings { section: Some(sec) } if !sec.is_empty() => {
            format!("dot://settings/{sec}")
        }
        InternalPage::Settings { .. } => "dot://settings".to_string(),
        InternalPage::History => "dot://history".to_string(),
        InternalPage::Bookmarks => "dot://bookmarks".to_string(),
        InternalPage::Downloads => "dot://downloads".to_string(),
        InternalPage::NewTab => "dot://newtab".to_string(),
    }
}

/// Identificadores puros de seguimiento de clic eliminados automáticamente al navegar
/// cuando `strip_click_ids` está activo.
const CLICK_ID_PARAMS: &[&str] = &[
    "fbclid", "gclid", "dclid", "gbraid", "wbraid", "msclkid", "yclid", "twclid", "ttclid",
    "mc_eid", "igshid", "igsh", "_hsenc", "_hsmi", "mkt_tok",
];

/// Parámetros de firma criptográfica en URLs pre-firmadas (S3, GCS, CloudFront):
/// si alguno está presente, jamás se altera la query para no invalidar la firma.
const SIGNATURE_PARAMS: &[&str] = &["x-amz-signature", "x-goog-signature", "signature", "sig"];

/// Motores de búsqueda soportados por el navegador (DuckDuckGo por defecto, sin Google por omisión).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SearchEngine {
    #[default]
    DuckDuckGo,
    Google,
    Brave,
    Kagi,
    Startpage,
    Ecosia,
    Qwant,
    Bing,
    Custom,
}

impl SearchEngine {
    pub fn as_id(self) -> &'static str {
        match self {
            Self::DuckDuckGo => "duckduckgo",
            Self::Google => "google",
            Self::Brave => "brave",
            Self::Kagi => "kagi",
            Self::Startpage => "startpage",
            Self::Ecosia => "ecosia",
            Self::Qwant => "qwant",
            Self::Bing => "bing",
            Self::Custom => "custom",
        }
    }

    pub fn from_id(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "duckduckgo" | "ddg" => Self::DuckDuckGo,
            "google" => Self::Google,
            "brave" => Self::Brave,
            "kagi" => Self::Kagi,
            "startpage" => Self::Startpage,
            "ecosia" => Self::Ecosia,
            "qwant" => Self::Qwant,
            "bing" => Self::Bing,
            "custom" => Self::Custom,
            _ => Self::DuckDuckGo,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::DuckDuckGo => "DuckDuckGo",
            Self::Google => "Google",
            Self::Brave => "Brave Search",
            Self::Kagi => "Kagi",
            Self::Startpage => "Startpage",
            Self::Ecosia => "Ecosia",
            Self::Qwant => "Qwant",
            Self::Bing => "Bing",
            Self::Custom => "Personalizado",
        }
    }

    /// Construye la URL de búsqueda codificando la consulta conforme a RFC 3986.
    /// Si el motor es `Custom` y la plantilla no es una URL `http`/`https` válida,
    /// cae limpiamente a DuckDuckGo sin pánico.
    pub fn search_url(self, query: &str, custom_template: &str) -> String {
        let encoded = encode_query_component(query.trim());
        match self {
            Self::DuckDuckGo => format!("https://duckduckgo.com/?q={encoded}"),
            Self::Google => format!("https://www.google.com/search?q={encoded}"),
            Self::Brave => format!("https://search.brave.com/search?q={encoded}"),
            Self::Kagi => format!("https://kagi.com/search?q={encoded}"),
            Self::Startpage => format!("https://www.startpage.com/sp/search?query={encoded}"),
            Self::Ecosia => format!("https://www.ecosia.org/search?q={encoded}"),
            Self::Qwant => format!("https://www.qwant.com/?q={encoded}"),
            Self::Bing => format!("https://www.bing.com/search?q={encoded}"),
            Self::Custom => build_custom_search_url(&encoded, custom_template)
                .unwrap_or_else(|| format!("https://duckduckgo.com/?q={encoded}")),
        }
    }
}

fn encode_query_component(query: &str) -> String {
    utf8_percent_encode(query, RFC3986_UNRESERVED_SET).to_string()
}

fn build_custom_search_url(encoded_query: &str, template: &str) -> Option<String> {
    let trimmed = template.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = if trimmed.contains("%s") {
        trimmed.replace("%s", encoded_query)
    } else if trimmed.contains("{searchTerms}") {
        trimmed.replace("{searchTerms}", encoded_query)
    } else {
        format!("{trimmed}{encoded_query}")
    };

    let parsed = Url::parse(&candidate).ok()?;
    match parsed.scheme() {
        "http" | "https" if parsed.host_str().is_some() => Some(parsed.to_string()),
        _ => None,
    }
}

/// Clasificación de lo que el usuario introdujo en el campo único de dirección/búsqueda.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressKind {
    Blank,
    Url,
    LocalFile,
    Search,
}

/// Resultado de resolver el texto escrito en la barra de direcciones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAddress {
    pub url: String,
    pub kind: AddressKind,
}

impl ResolvedAddress {
    pub fn as_str(&self) -> &str {
        &self.url
    }
}

/// Resuelve `typed` comprobando archivos regulares existentes en disco cuando empieza por `/` o `~/`.
pub fn resolve_address(
    typed: &str,
    engine: SearchEngine,
    custom_template: &str,
) -> ResolvedAddress {
    resolve_address_with_options(typed, engine, custom_template, true)
}

/// Variante parametrizable (permite desactivar lecturas de disco en tests puros o sugerencias).
pub fn resolve_address_with_options(
    typed: &str,
    engine: SearchEngine,
    custom_template: &str,
    check_local_files: bool,
) -> ResolvedAddress {
    let text = typed.trim();
    if text.is_empty() {
        return ResolvedAddress {
            url: "about:blank".to_string(),
            kind: AddressKind::Blank,
        };
    }

    // 1. Rutas locales explícitas `/...` o `~/...` si apuntan a un archivo regular en disco.
    if check_local_files {
        if let Some(file_url) = try_resolve_local_file(text) {
            return ResolvedAddress {
                url: file_url,
                kind: AddressKind::LocalFile,
            };
        }
    }

    // 2. Esquemas explícitos sin espacios (`http://`, `https://`, `file://`, `about:`, `data:`, `search:`).
    if let Some((kind, explicit_url)) = try_resolve_explicit_scheme(text) {
        return ResolvedAddress {
            url: explicit_url,
            kind,
        };
    }

    // 3. Sin espacios intermedios, comprobamos si parece un host/IP/dominio válido.
    if !text.chars().any(char::is_whitespace) {
        if let Some(host_url) = try_resolve_host_like(text) {
            return ResolvedAddress {
                url: host_url,
                kind: AddressKind::Url,
            };
        }
    }

    // 4. Todo lo demás se envía al motor de búsqueda configurado.
    ResolvedAddress {
        url: engine.search_url(text, custom_template),
        kind: AddressKind::Search,
    }
}

fn try_resolve_local_file(text: &str) -> Option<String> {
    let expanded: PathBuf = if let Some(rest) = text.strip_prefix("~/") {
        let home = std::env::var_os("HOME")?;
        PathBuf::from(home).join(rest)
    } else if text.starts_with('/') && !text.starts_with("//") {
        PathBuf::from(text)
    } else {
        return None;
    };

    let meta = std::fs::metadata(&expanded).ok()?;
    if !meta.is_file() {
        return None;
    }
    Url::from_file_path(&expanded).ok().map(|u| u.to_string())
}

fn try_resolve_explicit_scheme(text: &str) -> Option<(AddressKind, String)> {
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("about:") {
        if text.chars().any(char::is_whitespace) {
            return None;
        }
        let parsed = Url::parse(text).ok()?;
        return Some((AddressKind::Url, parsed.to_string()));
    }

    if lower.starts_with("data:") {
        let parsed = Url::parse(text).ok()?;
        return Some((AddressKind::Url, parsed.to_string()));
    }

    if let Some((scheme_raw, _rest)) = text.split_once("://") {
        if text.chars().any(char::is_whitespace) {
            return None;
        }
        let scheme = scheme_raw.to_ascii_lowercase();
        if !ALLOWED_SCHEMES.contains(&scheme.as_str()) {
            return None;
        }
        if scheme == "plasma" || scheme == "search" || scheme == "dot" {
            if let Some(internal) = parse_internal_page(text) {
                return Some((AddressKind::Url, canonical_internal_url(&internal)));
            }
        }
        let parsed = Url::parse(text).ok()?;
        let normalized = rewrite_zero_bind_host(parsed);
        let kind = if scheme == "file" {
            AddressKind::LocalFile
        } else {
            AddressKind::Url
        };
        return Some((kind, normalized.to_string()));
    }

    None
}

fn try_resolve_host_like(text: &str) -> Option<String> {
    let split_idx = text.find(['/', '?', '#']).unwrap_or(text.len());
    let head = &text[..split_idx];
    let suffix = &text[split_idx..];

    if head.is_empty() || head.contains('@') {
        return None;
    }

    // IPv6 entre corchetes: `[::1]`, `[::1]:8080`, `[2001:4860:4860::8888]/dns-query`
    if let Some(after_open) = head.strip_prefix('[') {
        let (addr_str, after_close) = after_open.split_once(']')?;
        let ipv6: Ipv6Addr = addr_str.parse().ok()?;
        if !after_close.is_empty() {
            let port_str = after_close.strip_prefix(':')?;
            let _port: u16 = port_str.parse().ok()?;
        }
        let scheme = if is_local_ipv6(ipv6) {
            "http://"
        } else {
            "https://"
        };
        let candidate = format!("{scheme}{head}{suffix}");
        return Url::parse(&candidate).ok().map(|u| u.to_string());
    }

    let (host_raw, port_opt) = match head.split_once(':') {
        Some((h, p)) => {
            if p.is_empty() {
                return None;
            }
            let _port: u16 = p.parse().ok()?;
            (h, Some(p))
        }
        None => (head, None),
    };

    if host_raw.is_empty() {
        return None;
    }

    let ascii_host = idna::domain_to_ascii(&host_raw.to_ascii_lowercase()).ok()?;
    if !looks_like_host(&ascii_host) {
        return None;
    }

    let use_http = is_local_or_private_host(&ascii_host);
    let effective_host = if ascii_host == "0.0.0.0" {
        "localhost"
    } else {
        ascii_host.as_str()
    };

    let scheme = if use_http { "http://" } else { "https://" };
    let authority = match port_opt {
        Some(port) => format!("{effective_host}:{port}"),
        None => effective_host.to_string(),
    };
    let candidate = format!("{scheme}{authority}{suffix}");
    Url::parse(&candidate).ok().map(|u| u.to_string())
}

/// Reescribe `http://0.0.0.0:puerto` -> `http://localhost:puerto` igual que `Address.reachable`.
pub fn rewrite_zero_bind_host(mut url: Url) -> Url {
    if matches!(url.scheme(), "http" | "https") && url.host_str() == Some("0.0.0.0") {
        let _ = url.set_host(Some("localhost"));
    }
    url
}

fn looks_like_host(ascii_host: &str) -> bool {
    if ascii_host == "localhost" {
        return true;
    }

    if ascii_host.parse::<Ipv4Addr>().is_ok() {
        return true;
    }

    let labels: Vec<&str> = ascii_host.split('.').collect();
    if labels.len() < 2 {
        return false;
    }

    for label in &labels {
        if label.is_empty()
            || label.len() > 63
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return false;
        }
    }

    let tld = labels[labels.len() - 1];
    if let Some(puny) = tld.strip_prefix("xn--") {
        if puny.is_empty() {
            return false;
        }
        let (uni, res) = idna::domain_to_unicode(tld);
        res.is_ok() && uni.chars().count() >= 2 && uni.chars().all(char::is_alphabetic)
    } else {
        tld.len() >= 2 && tld.bytes().all(|b| b.is_ascii_alphabetic())
    }
}

fn is_local_or_private_host(ascii_host: &str) -> bool {
    if ascii_host == "localhost"
        || ascii_host.ends_with(".localhost")
        || ascii_host.ends_with(".local")
        || ascii_host == "0.0.0.0"
    {
        return true;
    }

    if let Ok(ip) = ascii_host.parse::<Ipv4Addr>() {
        let octets = ip.octets();
        return ip.is_loopback()
            || ip.is_private()
            || ip.is_link_local()
            || ip.is_unspecified()
            || (octets[0] == 100 && (64..=127).contains(&octets[1]));
    }

    false
}

fn is_local_ipv6(ip: Ipv6Addr) -> bool {
    let seg0 = ip.segments()[0];
    ip.is_loopback()
        || ip.is_unspecified()
        || (seg0 & 0xfe00) == 0xfc00 // fc00::/7 Unique Local
        || (seg0 & 0xffc0) == 0xfe80 // fe80::/10 Link-Local
}

/// Elimina identificadores de clic publicitario (`gclid`, `fbclid`, `msclkid`, etc.)
/// conservando el orden y codificación exacta del resto de parámetros y respetando URLs firmadas.
pub fn strip_click_ids(raw_url: &str) -> String {
    strip_tracking_internal(raw_url, false)
}

/// Limpieza completa de enlace para copiar (`Ctrl+Shift+C`): además de click-IDs elimina
/// etiquetas de campaña `utm_*` y `si` en YouTube/Spotify (`CleanLink.swift`).
pub fn clean_link(raw_url: &str) -> String {
    strip_tracking_internal(raw_url, true)
}

fn strip_tracking_internal(raw_url: &str, include_campaigns: bool) -> String {
    let Ok(mut parsed) = Url::parse(raw_url) else {
        return raw_url.to_string();
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return raw_url.to_string();
    }
    let Some(raw_query) = parsed.query().map(str::to_string) else {
        return raw_url.to_string();
    };
    if raw_query.is_empty() {
        return raw_url.to_string();
    }

    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    let pieces: Vec<&str> = raw_query.split('&').collect();
    let mut decoded_names: Vec<String> = Vec::with_capacity(pieces.len());

    for piece in &pieces {
        let raw_key = piece.split('=').next().unwrap_or_default();
        let plus_as_space = raw_key.replace('+', " ");
        let decoded = percent_decode_str(&plus_as_space)
            .decode_utf8_lossy()
            .to_ascii_lowercase();
        if SIGNATURE_PARAMS.contains(&decoded.as_str()) {
            return raw_url.to_string();
        }
        decoded_names.push(decoded);
    }

    let mut kept: Vec<&str> = Vec::with_capacity(pieces.len());
    for (piece, key) in pieces.iter().zip(decoded_names.iter()) {
        if is_tracked_param(key, &host, include_campaigns) {
            continue;
        }
        kept.push(*piece);
    }

    if kept.len() == pieces.len() {
        return raw_url.to_string();
    }

    if kept.is_empty() {
        parsed.set_query(None);
    } else {
        parsed.set_query(Some(&kept.join("&")));
    }
    parsed.to_string()
}

fn is_tracked_param(key: &str, host: &str, include_campaigns: bool) -> bool {
    if CLICK_ID_PARAMS.contains(&key) {
        return true;
    }
    if include_campaigns {
        if key.starts_with("utm_") {
            return true;
        }
        if key == "si"
            && ["youtube.com", "youtu.be", "spotify.com"]
                .iter()
                .any(|d| host == *d || host.ends_with(&format!(".{d}")))
        {
            return true;
        }
    }
    false
}

/// Elimina `usuario:contraseña@` de cualquier URL antes de mostrarla en la interfaz
/// para impedir engaños de suplantación de dominio (`https://google.com@sitio-malo.es`).
pub fn sanitize_url_for_ui(raw_url: &str) -> String {
    let Ok(mut parsed) = Url::parse(raw_url) else {
        return raw_url.to_string();
    };

    if !parsed.username().is_empty() || parsed.password().is_some() {
        let _ = parsed.set_username("");
        let _ = parsed.set_password(None);
    }

    if matches!(parsed.scheme(), "http" | "https") {
        if let Some(host) = parsed.host_str().map(str::to_string) {
            let safe_host = safe_display_host(&host);
            // Solo sustituimos en la cadena de visualización si el host era IDN seguro;
            // si contiene homógrafos peligrosos `safe_display_host` devuelve el punycode ASCII.
            if safe_host != host {
                let serialized = parsed.to_string();
                if let Some(scheme_sep) = serialized.find("://") {
                    let after_scheme = &serialized[scheme_sep + 3..];
                    if let Some(rest) = after_scheme.strip_prefix(&host) {
                        return format!("{}://{}{}", parsed.scheme(), safe_host, rest);
                    }
                }
            }
        }
    }

    parsed.to_string()
}

/// Formato compacto en reposo para la barra de direcciones cuando no tiene el foco:
/// muestra el dominio registrable/host limpio (sin credenciales, sin homógrafos confusos y con
/// aviso explícito si es `http://` no local).
pub fn display_at_rest(raw_url: &str) -> String {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() || trimmed == "about:blank" {
        return String::new();
    }

    let Ok(parsed) = Url::parse(trimmed) else {
        return trimmed.to_string();
    };

    match parsed.scheme() {
        "http" | "https" => {
            let Some(host_raw) = parsed.host_str() else {
                return sanitize_url_for_ui(trimmed);
            };
            let stripped_www = if !is_ip_literal(host_raw)
                && host_raw.to_ascii_lowercase().starts_with("www.")
                && host_raw.split('.').count() > 2
            {
                &host_raw[4..]
            } else {
                host_raw
            };
            let shown_host = safe_display_host(stripped_www);
            let port_suffix = parsed.port().map(|p| format!(":{p}")).unwrap_or_default();
            let is_local = is_loopback_host(host_raw);
            if parsed.scheme() == "http" && !is_local {
                format!("No seguro — {shown_host}{port_suffix}")
            } else {
                format!("{shown_host}{port_suffix}")
            }
        }
        "file" => parsed
            .to_file_path()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| sanitize_url_for_ui(trimmed)),
        _ => sanitize_url_for_ui(trimmed),
    }
}

/// Devuelve el nombre de host en Unicode solo si todas sus etiquetas son inmunes a suplantación
/// homógrafa (alfabeto latino seguro o CJK/Hangul/Árabe/Hebreo/Tailandés/Devanagari sin mezcla
/// confusa); en caso contrario devuelve el punycode `xn--...` real (`AtRest.swift`).
pub fn safe_display_host(host: &str) -> String {
    if host.starts_with('[') || is_ip_literal(host) {
        return host.to_ascii_lowercase();
    }
    let Ok(ascii) = idna::domain_to_ascii(&host.to_ascii_lowercase()) else {
        return host.to_string();
    };
    if !ascii.split('.').any(|l| l.starts_with("xn--")) {
        return ascii;
    }

    let mut shown_labels = Vec::new();
    for label in ascii.split('.') {
        if !label.starts_with("xn--") {
            shown_labels.push(label.to_string());
            continue;
        }
        let (decoded, res) = idna::domain_to_unicode(label);
        if res.is_err() || decoded.is_empty() || decoded.is_ascii() || !is_safe_idn_label(&decoded)
        {
            return ascii;
        }
        shown_labels.push(decoded);
    }
    shown_labels.join(".")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum IdnScript {
    Latin,
    LatinExtra,
    Cjk,
    Hangul,
    Arabic,
    Hebrew,
    Thai,
    Devanagari,
}

fn is_safe_idn_label(label: &str) -> bool {
    let mut scripts: HashSet<IdnScript> = HashSet::new();
    let mut cjk_strokes = false;

    for ch in label.chars() {
        let v = ch as u32;
        match v {
            0x30..=0x39 | 0x2D => continue,
            0x61..=0x7A => {
                scripts.insert(IdnScript::Latin);
            }
            // Letras latinas confusables excluidas igual que en Chrome y AtRest.swift:
            // ð, þ, ı, ĸ, ŀ, ŀ, ŉ, ſ
            0xF0 | 0xFE | 0x131 | 0x138 | 0x13F | 0x140 | 0x149 | 0x17F => return false,
            0xDF..=0xF6 | 0xF8..=0xFF | 0x100..=0x17E => {
                scripts.insert(IdnScript::LatinExtra);
            }
            0x30FC | 0x30CE | 0x4E00 | 0x4E28 | 0x4E36 | 0x30FD | 0x30FE => {
                cjk_strokes = true;
                scripts.insert(IdnScript::Cjk);
            }
            0x3041..=0x3096
            | 0x309D..=0x309F
            | 0x30A1..=0x30FA
            | 0x30FF
            | 0x3400..=0x4DBF
            | 0x4E01..=0x9FFF => {
                scripts.insert(IdnScript::Cjk);
            }
            0xAC00..=0xD7A3 => {
                scripts.insert(IdnScript::Hangul);
            }
            0x0620..=0x064A | 0x0660..=0x0669 | 0x0671..=0x06D3 | 0x06F0..=0x06FF => {
                scripts.insert(IdnScript::Arabic);
            }
            0x05D0..=0x05EA => {
                scripts.insert(IdnScript::Hebrew);
            }
            0x0E01..=0x0E3A | 0x0E40..=0x0E4E | 0x0E50..=0x0E59 => {
                scripts.insert(IdnScript::Thai);
            }
            0x0901..=0x0939 | 0x093C..=0x094D | 0x0950 | 0x0966..=0x096F => {
                scripts.insert(IdnScript::Devanagari);
            }
            // Cirílico, griego, armenio, cherokee, IPA, etc. quedan en punycode.
            _ => return false,
        }
    }

    let has_latin = scripts.contains(&IdnScript::Latin) || scripts.contains(&IdnScript::LatinExtra);
    let non_latin: Vec<IdnScript> = scripts
        .into_iter()
        .filter(|s| !matches!(s, IdnScript::Latin | IdnScript::LatinExtra))
        .collect();

    if non_latin.len() > 1 {
        return false;
    }
    if cjk_strokes && has_latin {
        return false;
    }
    if non_latin.is_empty() {
        return true;
    }
    !has_latin || matches!(non_latin[0], IdnScript::Cjk | IdnScript::Hangul)
}

fn is_ip_literal(host: &str) -> bool {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    bare.parse::<Ipv4Addr>().is_ok() || bare.parse::<Ipv6Addr>().is_ok()
}

pub fn is_loopback_host(host: &str) -> bool {
    let bare = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    if bare == "localhost" || bare.ends_with(".localhost") {
        return true;
    }
    if let Ok(v4) = bare.parse::<Ipv4Addr>() {
        return v4.is_loopback();
    }
    if let Ok(v6) = bare.parse::<Ipv6Addr>() {
        return v6.is_loopback();
    }
    false
}

/// Determina si dos URLs pertenecen al mismo sitio registrable aproximado para el filtro
/// de cookies de terceros (`CefCookieAccessFilter`).
pub fn is_same_site(url_a: &str, url_b: &str) -> bool {
    let (Ok(a), Ok(b)) = (Url::parse(url_a), Url::parse(url_b)) else {
        return true;
    };
    let (Some(host_a), Some(host_b)) = (a.host_str(), b.host_str()) else {
        return true;
    };
    registrable_site_key(host_a) == registrable_site_key(host_b)
}

/// Extrae una clave de sitio registrable (eTLD+1) conservadora para comparación `same-site`.
pub fn registrable_site_key(host: &str) -> String {
    let bare = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    if is_ip_literal(&bare) || bare == "localhost" {
        return bare;
    }
    let labels: Vec<&str> = bare.split('.').filter(|s| !s.is_empty()).collect();
    if labels.len() <= 2 {
        return bare;
    }

    // Sufijos multiparte frecuentes (co.uk, com.es, org.uk, com.br, co.jp, com.ar, com.mx, etc.).
    let second_last = labels[labels.len() - 2];
    let last = labels[labels.len() - 1];
    let multipart_second = matches!(
        second_last,
        "co" | "com" | "org" | "net" | "gov" | "edu" | "ac" | "gob"
    ) && last.len() == 2;

    let take = if multipart_second && labels.len() >= 3 {
        3
    } else {
        2
    };
    labels[labels.len() - take..].join(".")
}

/// Comprueba si un archivo local es texto UTF-8 válido (para `LocalText.swift`).
pub fn is_utf8_local_file(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    !bytes.contains(&0) && std::str::from_utf8(&bytes).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_domains_ips_and_localhost_with_right_schemes() {
        let r = resolve_address_with_options("example.com", SearchEngine::DuckDuckGo, "", false);
        assert_eq!(r.kind, AddressKind::Url);
        assert_eq!(r.url, "https://example.com/");

        let r = resolve_address_with_options(
            "docs.rs/cef/latest/cef",
            SearchEngine::DuckDuckGo,
            "",
            false,
        );
        assert_eq!(r.url, "https://docs.rs/cef/latest/cef");

        let r =
            resolve_address_with_options("localhost:3000/api", SearchEngine::DuckDuckGo, "", false);
        assert_eq!(r.kind, AddressKind::Url);
        assert_eq!(r.url, "http://localhost:3000/api");

        let r = resolve_address_with_options("0.0.0.0:5173", SearchEngine::DuckDuckGo, "", false);
        assert_eq!(r.url, "http://localhost:5173/");

        let r = resolve_address_with_options(
            "http://0.0.0.0:8080/health",
            SearchEngine::DuckDuckGo,
            "",
            false,
        );
        assert_eq!(r.url, "http://localhost:8080/health");

        let r = resolve_address_with_options("127.0.0.1:8000", SearchEngine::DuckDuckGo, "", false);
        assert_eq!(r.url, "http://127.0.0.1:8000/");

        let r = resolve_address_with_options("192.168.1.42", SearchEngine::DuckDuckGo, "", false);
        assert_eq!(r.url, "http://192.168.1.42/");

        let r = resolve_address_with_options("1.1.1.1", SearchEngine::DuckDuckGo, "", false);
        assert_eq!(r.url, "https://1.1.1.1/");

        let r =
            resolve_address_with_options("[::1]:9000/test", SearchEngine::DuckDuckGo, "", false);
        assert_eq!(r.url, "http://[::1]:9000/test");

        let r = resolve_address_with_options("nas.local:8096", SearchEngine::DuckDuckGo, "", false);
        assert_eq!(r.url, "http://nas.local:8096/");
    }

    #[test]
    fn non_urls_and_version_numbers_resolve_as_search_queries() {
        let r =
            resolve_address_with_options("rust gtk4 browser", SearchEngine::DuckDuckGo, "", false);
        assert_eq!(r.kind, AddressKind::Search);
        assert_eq!(r.url, "https://duckduckgo.com/?q=rust%20gtk4%20browser");

        // Un número de versión con puntos terminando en dígitos NO es un dominio.
        let r = resolve_address_with_options("1.24.3", SearchEngine::DuckDuckGo, "", false);
        assert_eq!(r.kind, AddressKind::Search);

        // Una dirección de correo no es una URL navegable.
        let r = resolve_address_with_options("user@example.com", SearchEngine::Brave, "", false);
        assert_eq!(r.kind, AddressKind::Search);
        assert_eq!(
            r.url,
            "https://search.brave.com/search?q=user%40example.com"
        );

        // Caracteres especiales RFC 3986 como `+`, `&`, `#`.
        let r = resolve_address_with_options("c++ & rust #1", SearchEngine::Kagi, "", false);
        assert_eq!(
            r.url,
            "https://kagi.com/search?q=c%2B%2B%20%26%20rust%20%231"
        );
    }

    #[test]
    fn custom_search_engine_supports_placeholders_and_safe_fallback() {
        let url = SearchEngine::Custom.search_url(
            "hola mundo",
            "https://searx.example.org/search?q=%s&lang=es",
        );
        assert_eq!(
            url,
            "https://searx.example.org/search?q=hola%20mundo&lang=es"
        );

        let url2 =
            SearchEngine::Custom.search_url("cef linux", "https://buscador.local/q/{searchTerms}");
        assert_eq!(url2, "https://buscador.local/q/cef%20linux");

        // Plantilla inválida o peligrosa cae a DuckDuckGo.
        let fallback = SearchEngine::Custom.search_url("test", "javascript:alert(%s)");
        assert_eq!(fallback, "https://duckduckgo.com/?q=test");
    }

    #[test]
    fn resolves_existing_local_file_to_file_url() {
        let tmp =
            std::env::temp_dir().join(format!("search-addr-test-{}.html", std::process::id()));
        std::fs::write(&tmp, b"<h1>Local</h1>").expect("write temp html");
        let resolved = resolve_address(
            tmp.to_str().expect("utf8 path"),
            SearchEngine::DuckDuckGo,
            "",
        );
        assert_eq!(resolved.kind, AddressKind::LocalFile);
        assert!(resolved.url.starts_with("file:///"));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn strips_click_ids_but_preserves_normal_params_and_signed_urls() {
        let dirty = "https://example.com/item?id=42&gclid=abc123&fbclid=xyz&msclkid=999&page=2#top";
        assert_eq!(
            strip_click_ids(dirty),
            "https://example.com/item?id=42&page=2#top"
        );

        let only_tracking = "https://example.com/?fbclid=12345";
        assert_eq!(strip_click_ids(only_tracking), "https://example.com/");

        let signed = "https://bucket.s3.amazonaws.com/doc.pdf?fbclid=123&X-Amz-Signature=deadbeef";
        assert_eq!(strip_click_ids(signed), signed);

        let campaign = "https://www.youtube.com/watch?v=dQw4w9WgXcQ&si=share123&utm_source=chat";
        assert_eq!(
            clean_link(campaign),
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        );
    }

    #[test]
    fn strips_credentials_and_blocks_cyrillic_homographs_in_display() {
        let spoof = "https://google.com:secret@evil.example.org/login";
        assert_eq!(sanitize_url_for_ui(spoof), "https://evil.example.org/login");

        // "аррӏе.com" en cirílico (`xn--80ak6aa92e.com`) debe permanecer en punycode.
        assert_eq!(
            safe_display_host("xn--80ak6aa92e.com"),
            "xn--80ak6aa92e.com"
        );
        // "bücher.de" (`xn--bcher-kva.de`) es latino acentuado seguro y se muestra legible.
        assert_eq!(safe_display_host("xn--bcher-kva.de"), "bücher.de");

        assert_eq!(
            display_at_rest("http://example.com/path"),
            "No seguro — example.com"
        );
        assert_eq!(
            display_at_rest("http://localhost:3000/path"),
            "localhost:3000"
        );
    }

    #[test]
    fn same_site_detection_handles_subdomains_and_multipart_tlds() {
        assert!(is_same_site(
            "https://app.example.com/a",
            "https://cdn.example.com/b"
        ));
        assert!(!is_same_site(
            "https://example.com/",
            "https://doubleclick.net/pixel"
        ));
        assert!(!is_same_site("https://a.co.uk/", "https://b.co.uk/"));
        assert!(is_same_site(
            "https://news.bbc.co.uk/",
            "https://static.bbc.co.uk/"
        ));
    }

    #[test]
    fn resolves_dot_canonical_internal_urls_without_searching() {
        let r = resolve_address_with_options("dot://settings", SearchEngine::DuckDuckGo, "", false);
        assert_eq!(r.kind, AddressKind::Url);
        assert_eq!(r.url, "dot://settings");
        assert_eq!(
            parse_internal_page(&r.url),
            Some(InternalPage::Settings { section: None })
        );

        let r2 = resolve_address_with_options(
            "dot://settings/privacy/",
            SearchEngine::DuckDuckGo,
            "",
            false,
        );
        assert_eq!(r2.kind, AddressKind::Url);
        assert_eq!(r2.url, "dot://settings/privacy");
        assert_eq!(
            parse_internal_page(&r2.url),
            Some(InternalPage::Settings {
                section: Some("privacy".to_string())
            })
        );

        assert_eq!(
            resolve_address_with_options("dot://newtab", SearchEngine::DuckDuckGo, "", false).url,
            "dot://newtab"
        );
        assert_eq!(
            resolve_address_with_options("dot://history", SearchEngine::DuckDuckGo, "", false).url,
            "dot://history"
        );
        assert_eq!(
            resolve_address_with_options("dot://bookmarks", SearchEngine::DuckDuckGo, "", false)
                .url,
            "dot://bookmarks"
        );
        assert_eq!(
            resolve_address_with_options("dot://downloads", SearchEngine::DuckDuckGo, "", false)
                .url,
            "dot://downloads"
        );
        assert_eq!(display_at_rest("dot://settings"), "dot://settings");
    }

    #[test]
    fn resolves_plasma_and_search_internal_scheme_aliases_to_dot() {
        let cases = [
            (
                "plasma://settings",
                "dot://settings",
                InternalPage::Settings { section: None },
            ),
            (
                "plasma://settings/privacy/",
                "dot://settings/privacy",
                InternalPage::Settings {
                    section: Some("privacy".to_string()),
                },
            ),
            ("plasma://newtab", "dot://newtab", InternalPage::NewTab),
            ("plasma://history", "dot://history", InternalPage::History),
            (
                "plasma://bookmarks",
                "dot://bookmarks",
                InternalPage::Bookmarks,
            ),
            (
                "plasma://downloads",
                "dot://downloads",
                InternalPage::Downloads,
            ),
            (
                "search://settings",
                "dot://settings",
                InternalPage::Settings { section: None },
            ),
            (
                "search://settings/privacy/",
                "dot://settings/privacy",
                InternalPage::Settings {
                    section: Some("privacy".to_string()),
                },
            ),
            ("search://newtab", "dot://newtab", InternalPage::NewTab),
            ("search://history", "dot://history", InternalPage::History),
            (
                "search://bookmarks",
                "dot://bookmarks",
                InternalPage::Bookmarks,
            ),
            (
                "search://downloads",
                "dot://downloads",
                InternalPage::Downloads,
            ),
            ("DOT://NewTab", "dot://newtab", InternalPage::NewTab),
            (
                "PLASMA://Settings",
                "dot://settings",
                InternalPage::Settings { section: None },
            ),
            (
                "SEARCH://Bookmarks",
                "dot://bookmarks",
                InternalPage::Bookmarks,
            ),
        ];

        for (input, expected_url, expected_page) in cases {
            assert_eq!(parse_internal_page(input), Some(expected_page.clone()));
            let resolved = resolve_address_with_options(input, SearchEngine::DuckDuckGo, "", false);
            assert_eq!(resolved.kind, AddressKind::Url);
            assert_eq!(resolved.url, expected_url);
            assert_eq!(parse_internal_page(&resolved.url), Some(expected_page));
        }
    }
}
