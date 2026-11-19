//! Cálculo de dominio registrable (eTLD+1), comparación `same-site`, normalización de hosts
//! y coincidencia de patrones `if-domain` / `unless-domain` (`Registrable.swift`).

use std::net::{Ipv4Addr, Ipv6Addr};
use url::Url;

/// Sufijos públicos multiparte más comunes (PSL) para resolver eTLD+1 sin red ni archivos externos.
const MULTIPART_PUBLIC_SUFFIXES: &[&str] = &[
    // Reino Unido
    "co.uk",
    "org.uk",
    "ac.uk",
    "gov.uk",
    "me.uk",
    "net.uk",
    "ltd.uk",
    "plc.uk",
    "sch.uk",
    // España
    "com.es",
    "nom.es",
    "org.es",
    "gob.es",
    "edu.es",
    // Australia y Nueva Zelanda
    "com.au",
    "net.au",
    "org.au",
    "edu.au",
    "gov.au",
    "asn.au",
    "id.au",
    "co.nz",
    "net.nz",
    "org.nz",
    "govt.nz",
    "ac.nz",
    // Japón y Corea
    "co.jp",
    "ne.jp",
    "or.jp",
    "go.jp",
    "ac.jp",
    "ed.jp",
    "gr.jp",
    "co.kr",
    "or.kr",
    "ne.kr",
    "go.kr",
    "ac.kr",
    // Latinoamérica
    "com.br",
    "org.br",
    "net.br",
    "gov.br",
    "edu.br",
    "com.mx",
    "org.mx",
    "gob.mx",
    "edu.mx",
    "net.mx",
    "com.ar",
    "org.ar",
    "gob.ar",
    "gov.ar",
    "edu.ar",
    "com.co",
    "org.co",
    "gov.co",
    "edu.co",
    "com.pe",
    "org.pe",
    "gob.pe",
    "edu.pe",
    "com.ve",
    "com.cl",
    "com.ec",
    "com.uy",
    "com.py",
    "com.bo",
    "com.gt",
    "com.do",
    "com.pr",
    // Otros ccTLD frecuentes
    "com.cn",
    "net.cn",
    "org.cn",
    "gov.cn",
    "edu.cn",
    "co.in",
    "net.in",
    "org.in",
    "gov.in",
    "ac.in",
    "co.za",
    "org.za",
    "gov.za",
    "ac.za",
    "com.tw",
    "org.tw",
    "gov.tw",
    "edu.tw",
    "com.hk",
    "org.hk",
    "gov.hk",
    "edu.hk",
    "com.sg",
    "org.sg",
    "gov.sg",
    "edu.sg",
    "com.tr",
    "org.tr",
    "gov.tr",
    "edu.tr",
    "com.ua",
    "org.ua",
    "gov.ua",
    "co.il",
    "org.il",
    "gov.il",
    "ac.il",
    "com.pl",
    "net.pl",
    "org.pl",
    "gov.pl",
    "com.pt",
    "org.pt",
    "gov.pt",
    "edu.pt",
    "com.my",
    "org.my",
    "gov.my",
    "com.ph",
    "org.ph",
    "gov.ph",
    "com.vn",
    "org.vn",
    "gov.vn",
    "co.id",
    "or.id",
    "go.id",
    "ac.id",
    "co.th",
    "or.th",
    "go.th",
    "ac.th",
    "com.sa",
    "org.sa",
    "gov.sa",
    "com.eg",
    "org.eg",
    "gov.eg",
    "com.ng",
    "org.ng",
    "gov.ng",
    "co.ke",
    "or.ke",
    "go.ke",
    "eu.org",
    // Plataformas de alojamiento con aislamiento por subdominio
    "github.io",
    "gitlab.io",
    "pages.dev",
    "vercel.app",
    "netlify.app",
    "herokuapp.com",
    "appspot.com",
    "cloudfront.net",
    "s3.amazonaws.com",
    "blogspot.com",
    "wordpress.com",
    "tumblr.com",
];

/// Normaliza un nombre de host: minúsculas, sin corchetes IPv6 ni puntos finales.
pub fn normalize_host(host: &str) -> String {
    host.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

/// Devuelve `true` si `host` es un literal IPv4 o IPv6.
pub fn is_ip_address(host: &str) -> bool {
    let bare = normalize_host(host);
    bare.parse::<Ipv4Addr>().is_ok() || bare.parse::<Ipv6Addr>().is_ok()
}

/// Extrae el host de sitio (sin `www.` inicial) de una URL `http`/`https` o de un host directo.
/// Coincide con `Curtain.host(of:)` y el esquema de `shield.paused`.
pub fn site_host(url_or_host: &str) -> Option<String> {
    let trimmed = url_or_host.trim();
    if trimmed.is_empty() {
        return None;
    }
    let raw_host = if trimmed.contains("://") {
        let parsed = Url::parse(trimmed).ok()?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return None;
        }
        parsed.host_str()?.to_string()
    } else if trimmed.starts_with("about:")
        || trimmed.starts_with("data:")
        || trimmed.starts_with("file:")
        || trimmed.starts_with("search:")
    {
        return None;
    } else {
        trimmed.to_string()
    };

    let norm = normalize_host(&raw_host);
    if norm.is_empty() {
        return None;
    }
    if !is_ip_address(&norm) && norm.starts_with("www.") && norm.matches('.').count() >= 2 {
        Some(norm[4..].to_string())
    } else {
        Some(norm)
    }
}

/// Extrae el host completo normalizado (conservando `www.`) de una URL `http`/`https`.
pub fn raw_url_host(raw_url: &str) -> Option<String> {
    let parsed = Url::parse(raw_url.trim()).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let host = parsed.host_str()?;
    let norm = normalize_host(host);
    if norm.is_empty() {
        None
    } else {
        Some(norm)
    }
}

/// Predicado integrado de sufijos públicos: reconoce TLDs simples y la tabla de sufijos multiparte.
pub fn is_known_public_suffix(candidate: &str) -> bool {
    let norm = normalize_host(candidate);
    if norm.is_empty() {
        return false;
    }
    if !norm.contains('.') {
        return norm.len() >= 2 && norm.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    }
    MULTIPART_PUBLIC_SUFFIXES.contains(&norm.as_str())
}

/// Calcula el dominio registrable (eTLD+1) usando un predicado de sufijos públicos personalizado,
/// port directo de `Registrable.domain(of:isSuffix:)`.
pub fn registrable_domain_with<F>(host: &str, is_suffix: F) -> String
where
    F: Fn(&str) -> bool,
{
    let name = normalize_host(host);
    if name.is_empty() {
        return name;
    }
    let labels: Vec<&str> = name.split('.').collect();
    let is_addr = name.contains(':')
        || labels
            .iter()
            .all(|l| !l.is_empty() && l.bytes().all(|b| b.is_ascii_digit()));
    if is_addr || labels.len() <= 1 || is_suffix(&name) {
        return name;
    }
    for count in (1..labels.len()).rev() {
        let suffix = labels[labels.len() - count..].join(".");
        if is_suffix(&suffix) {
            return labels[labels.len() - (count + 1)..].join(".");
        }
    }
    name
}

/// Calcula el dominio registrable (eTLD+1) con la tabla integrada de sufijos públicos.
pub fn registrable_domain(host: &str) -> String {
    registrable_domain_with(host, is_known_public_suffix)
}

/// Determina si dos hosts pertenecen al mismo sitio registrable (`Registrable.sameSite`).
pub fn is_same_site_host(one: &str, other: &str) -> bool {
    let a = normalize_host(one);
    let b = normalize_host(other);
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    if is_ip_address(&a) || is_ip_address(&b) {
        return false;
    }
    let left = registrable_domain(&a);
    let right = registrable_domain(&b);
    left.contains('.') && left == right
}

/// Determina si dos URLs pertenecen al mismo sitio registrable.
pub fn is_same_site_url(url_a: &str, url_b: &str) -> bool {
    let (Some(a), Some(b)) = (raw_url_host(url_a), raw_url_host(url_b)) else {
        return true;
    };
    is_same_site_host(&a, &b)
}

/// Comprueba si `host` coincide con un patrón de dominio estilo `WKContentRuleList`
/// (`*example.com` coincide con `example.com` y cualquier subdominio `*.example.com`;
/// `example.com` sin comodín exige coincidencia exacta).
pub fn matches_domain_pattern(host: &str, pattern: &str) -> bool {
    let h = normalize_host(host);
    let p = pattern.trim().to_ascii_lowercase();
    if h.is_empty() || p.is_empty() {
        return false;
    }
    if let Some(rest) = p.strip_prefix('*') {
        let base = rest.trim_start_matches('.');
        if base.is_empty() {
            return true;
        }
        h == base || h.ends_with(&format!(".{base}"))
    } else {
        h == normalize_host(&p)
    }
}

/// Construye la clave de origen exacto (`scheme://host[:port]` sin puerto por defecto).
pub fn exact_origin(raw_url: &str) -> Option<String> {
    let parsed = Url::parse(raw_url.trim()).ok()?;
    let scheme = parsed.scheme().to_ascii_lowercase();
    if !matches!(scheme.as_str(), "http" | "https") {
        return None;
    }
    let host = parsed.host_str()?.to_ascii_lowercase();
    match parsed.port() {
        Some(port) => Some(format!("{scheme}://{host}:{port}")),
        None => Some(format!("{scheme}://{host}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registrable_domain_handles_standard_and_multipart_suffixes() {
        assert_eq!(registrable_domain("www.example.com"), "example.com");
        assert_eq!(registrable_domain("a.b.bbc.co.uk"), "bbc.co.uk");
        assert_eq!(registrable_domain("cdn.elpais.com.es"), "elpais.com.es");
        assert_eq!(registrable_domain("user.github.io"), "user.github.io");
        assert_eq!(registrable_domain("127.0.0.1"), "127.0.0.1");
        assert_eq!(registrable_domain("[::1]"), "::1");
        assert_eq!(registrable_domain("localhost"), "localhost");
    }

    #[test]
    fn same_site_matches_subdomains_and_separates_cross_site() {
        assert!(is_same_site_host("idmsa.apple.com", "account.apple.com"));
        assert!(is_same_site_host("www.bbc.co.uk", "news.bbc.co.uk"));
        assert!(!is_same_site_host("doubleclick.net", "example.com"));
        assert!(!is_same_site_host("a.github.io", "b.github.io"));
        assert!(!is_same_site_host("127.0.0.1", "127.0.0.2"));
        assert!(is_same_site_host("127.0.0.1", "127.0.0.1"));
    }

    #[test]
    fn site_host_strips_www_and_rejects_non_web_schemes() {
        assert_eq!(
            site_host("https://www.nytimes.com/2026/news").as_deref(),
            Some("nytimes.com")
        );
        assert_eq!(
            site_host("https://news.ycombinator.com/").as_deref(),
            Some("news.ycombinator.com")
        );
        assert_eq!(site_host("about:blank"), None);
        assert_eq!(site_host("data:text/html,<h1>Hi</h1>"), None);
    }

    #[test]
    fn matches_domain_pattern_supports_wildcard_and_exact() {
        assert!(matches_domain_pattern("www.elpais.com", "*elpais.com"));
        assert!(matches_domain_pattern("elpais.com", "*elpais.com"));
        assert!(!matches_domain_pattern("notelpais.com", "*elpais.com"));
        assert!(matches_domain_pattern("youtube.com", "youtube.com"));
        assert!(!matches_domain_pattern("www.youtube.com", "youtube.com"));
    }

    #[test]
    fn exact_origin_normalizes_scheme_host_and_port() {
        assert_eq!(
            exact_origin("https://Example.COM:443/path?a=1").as_deref(),
            Some("https://example.com")
        );
        assert_eq!(
            exact_origin("https://example.com:8443/path").as_deref(),
            Some("https://example.com:8443")
        );
        assert_eq!(exact_origin("about:blank"), None);
    }
}
