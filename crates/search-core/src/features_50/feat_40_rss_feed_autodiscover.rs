//! RSS, Atom, and JSON Feed autodiscovery for Search Linux.
//!
//! Parses HTML documents to extract `<link rel="alternate">` feed declarations,
//! resolves relative URLs against `<base href>` or the page URL, normalizes and
//! deduplicates candidates, and ranks discovered feeds deterministically.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use url::Url;

/// Errors returned by RSS/Atom/JSON Feed autodiscovery.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum FeedDiscoveryError {
    /// The supplied page URL could not be parsed.
    #[error("invalid page URL: {0}")]
    InvalidBaseUrl(#[from] url::ParseError),

    /// The supplied page URL uses a scheme other than HTTP or HTTPS (or HTTP when disallowed).
    #[error("unsupported base URL scheme: {0}")]
    UnsupportedBaseScheme(String),

    /// The HTML payload exceeds the configured maximum byte limit.
    #[error("HTML input size ({size} bytes) exceeds maximum ({max_size} bytes)")]
    HtmlTooLarge { size: usize, max_size: usize },

    /// The discovery configuration is invalid.
    #[error("invalid discovery configuration: {0}")]
    InvalidConfig(String),
}

/// Supported syndication feed formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedFormat {
    /// RSS 0.9x / 1.0 (RDF) / 2.0.
    Rss,
    /// Atom 1.0.
    Atom,
    /// JSON Feed 1.0 / 1.1.
    JsonFeed,
}

impl FeedFormat {
    /// Maps a normalized MIME type (without parameters) to a `FeedFormat`.
    pub fn from_mime_type(mime: &str) -> Option<Self> {
        let base = mime.split(';').next()?.trim().to_ascii_lowercase();
        match base.as_str() {
            "application/rss+xml" | "text/rss+xml" | "application/rdf+xml" => Some(Self::Rss),
            "application/atom+xml" | "text/atom+xml" => Some(Self::Atom),
            "application/feed+json" => Some(Self::JsonFeed),
            _ => None,
        }
    }

    /// Returns the canonical MIME type for this feed format.
    pub fn canonical_mime_type(self) -> &'static str {
        match self {
            Self::Rss => "application/rss+xml",
            Self::Atom => "application/atom+xml",
            Self::JsonFeed => "application/feed+json",
        }
    }

    /// Infers a feed format from a URL path extension or well-known suffix.
    pub fn infer_from_url_path(path: &str) -> Option<Self> {
        let lower = path.trim_end_matches('/').to_ascii_lowercase();
        if lower.ends_with(".atom") || lower.ends_with("/atom.xml") || lower.ends_with("/atom") {
            Some(Self::Atom)
        } else if lower.ends_with(".rss")
            || lower.ends_with("/rss.xml")
            || lower.ends_with("/rss")
            || lower.ends_with("/feed")
            || lower.ends_with("/feed.xml")
        {
            Some(Self::Rss)
        } else if lower.ends_with("/feed.json") || lower.ends_with(".feed.json") {
            Some(Self::JsonFeed)
        } else {
            None
        }
    }
}

/// How a feed candidate was discovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoverySource {
    /// Declared via `<link rel="alternate" type="..." href="...">`.
    LinkAlternate,
    /// Inferred from an `<a href="...">` link matching common feed paths.
    AnchorHeuristic,
    /// Synthesized well-known feed path on the page's origin.
    WellKnownPath,
}

/// A single discovered syndication feed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredFeed {
    /// Normalized absolute URL of the feed (without fragment).
    pub url: String,
    /// Detected feed format.
    pub format: FeedFormat,
    /// Raw or canonical MIME type associated with the feed.
    pub mime_type: String,
    /// Optional human-readable feed title.
    pub title: Option<String>,
    /// Optional `hreflang` language tag from the `<link>` element.
    pub hreflang: Option<String>,
    /// Discovery mechanism that produced this candidate.
    pub source: DiscoverySource,
    /// Whether the feed appears to be a comments-only feed.
    pub is_comments_feed: bool,
    /// Deterministic ranking score (higher is more preferred).
    pub score: u32,
}

/// Configuration for [`FeedAutodiscoverer`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    /// Maximum HTML document size in bytes to scan.
    pub max_html_bytes: usize,
    /// Whether to include comment feeds (e.g. WordPress `/comments/feed/`).
    pub include_comment_feeds: bool,
    /// Whether to inspect `<a href>` tags when searching for feeds.
    pub enable_anchor_fallback: bool,
    /// Whether to append well-known feed URL candidates when no feeds are found in HTML.
    pub enable_well_known_candidates: bool,
    /// Whether `http://` URLs are permitted (in addition to `https://`).
    pub allow_http: bool,
    /// Maximum number of feeds returned in a [`DiscoveryReport`].
    pub max_results: usize,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            max_html_bytes: 2 * 1024 * 1024,
            include_comment_feeds: true,
            enable_anchor_fallback: false,
            enable_well_known_candidates: false,
            allow_http: true,
            max_results: 32,
        }
    }
}

/// Result of running feed autodiscovery on an HTML document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryReport {
    /// Normalized page URL passed to discovery.
    pub page_url: String,
    /// Effective base URL used for resolving relative `href` attributes.
    pub effective_base_url: String,
    /// Ranked, deduplicated list of discovered feeds.
    pub feeds: Vec<DiscoveredFeed>,
    /// Number of candidate `<link>`/`<a>` tags skipped due to invalid or disallowed URLs.
    pub skipped_links: usize,
}

impl DiscoveryReport {
    /// Serializes this report to a JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserializes a `DiscoveryReport` from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Stateless RSS, Atom, and JSON Feed autodiscoverer.
#[derive(Debug, Clone)]
pub struct FeedAutodiscoverer {
    config: DiscoveryConfig,
}

impl FeedAutodiscoverer {
    /// Creates a new `FeedAutodiscoverer` after validating `config`.
    pub fn new(config: DiscoveryConfig) -> Result<Self, FeedDiscoveryError> {
        if config.max_html_bytes == 0 {
            return Err(FeedDiscoveryError::InvalidConfig(
                "max_html_bytes must be greater than 0".to_string(),
            ));
        }
        if config.max_results == 0 {
            return Err(FeedDiscoveryError::InvalidConfig(
                "max_results must be greater than 0".to_string(),
            ));
        }
        Ok(Self { config })
    }

    /// Returns a reference to the active configuration.
    pub fn config(&self) -> &DiscoveryConfig {
        &self.config
    }

    /// Discovers syndication feeds from `html` fetched from `page_url`.
    pub fn discover(
        &self,
        page_url: &str,
        html: &str,
    ) -> Result<DiscoveryReport, FeedDiscoveryError> {
        if html.len() > self.config.max_html_bytes {
            return Err(FeedDiscoveryError::HtmlTooLarge {
                size: html.len(),
                max_size: self.config.max_html_bytes,
            });
        }

        let parsed_page = Url::parse(page_url)?;
        self.validate_scheme(&parsed_page)?;

        let mut effective_base = parsed_page.clone();
        let mut by_url: HashMap<String, DiscoveredFeed> = HashMap::new();
        let mut skipped_links: usize = 0;
        let mut seen_base_tag = false;
        let mut in_head = true;

        for tag in iter_html_tags(html) {
            let name_lower = tag.name.to_ascii_lowercase();
            if name_lower == "/head" || name_lower == "body" {
                in_head = false;
            }

            if name_lower == "base" && in_head && !seen_base_tag {
                if let Some(href) = tag.attr("href") {
                    let decoded_href = decode_html_entities(href.trim());
                    if let Ok(resolved) = parsed_page.join(&decoded_href) {
                        if self.validate_scheme(&resolved).is_ok() {
                            effective_base = resolved;
                            seen_base_tag = true;
                        }
                    }
                }
                continue;
            }

            if name_lower == "link" {
                self.process_link_tag(&tag, &effective_base, &mut by_url, &mut skipped_links);
            } else if name_lower == "a" && self.config.enable_anchor_fallback {
                self.process_anchor_tag(&tag, &effective_base, &mut by_url, &mut skipped_links);
            }
        }

        if by_url.is_empty() && self.config.enable_well_known_candidates {
            self.add_well_known_candidates(&effective_base, &mut by_url);
        }

        let mut feeds: Vec<DiscoveredFeed> = by_url.into_values().collect();
        feeds.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.url.cmp(&b.url)));
        feeds.truncate(self.config.max_results);

        Ok(DiscoveryReport {
            page_url: parsed_page.to_string(),
            effective_base_url: effective_base.to_string(),
            feeds,
            skipped_links,
        })
    }

    fn validate_scheme(&self, url: &Url) -> Result<(), FeedDiscoveryError> {
        match url.scheme() {
            "https" => Ok(()),
            "http" if self.config.allow_http => Ok(()),
            other => Err(FeedDiscoveryError::UnsupportedBaseScheme(other.to_string())),
        }
    }

    fn process_link_tag(
        &self,
        tag: &HtmlTag<'_>,
        base: &Url,
        by_url: &mut HashMap<String, DiscoveredFeed>,
        skipped_links: &mut usize,
    ) {
        let Some(rel_raw) = tag.attr("rel") else {
            return;
        };
        let has_alternate = rel_raw
            .split_ascii_whitespace()
            .any(|tok| tok.eq_ignore_ascii_case("alternate") || tok.eq_ignore_ascii_case("feed"));
        if !has_alternate {
            return;
        }

        let Some(type_raw) = tag.attr("type") else {
            return;
        };
        let Some(format) = FeedFormat::from_mime_type(type_raw) else {
            return;
        };

        let Some(href_raw) = tag.attr("href") else {
            *skipped_links += 1;
            return;
        };
        let Some(normalized_url) = self.resolve_feed_url(base, href_raw) else {
            *skipped_links += 1;
            return;
        };

        let title = tag
            .attr("title")
            .map(|t| decode_html_entities(t.trim()))
            .filter(|t| !t.is_empty());
        let hreflang = tag
            .attr("hreflang")
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty());

        let is_comments = is_comments_feed(&normalized_url, title.as_deref());
        if is_comments && !self.config.include_comment_feeds {
            return;
        }

        let mut score: u32 = 100;
        if title.is_some() {
            score += 10;
        }
        if matches!(format, FeedFormat::Atom | FeedFormat::Rss) {
            score += 5;
        }
        if is_comments {
            score = score.saturating_sub(30);
        }

        let candidate = DiscoveredFeed {
            url: normalized_url.clone(),
            format,
            mime_type: type_raw
                .split(';')
                .next()
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| format.canonical_mime_type().to_string()),
            title,
            hreflang,
            source: DiscoverySource::LinkAlternate,
            is_comments_feed: is_comments,
            score,
        };

        insert_or_merge(by_url, candidate);
    }

    fn process_anchor_tag(
        &self,
        tag: &HtmlTag<'_>,
        base: &Url,
        by_url: &mut HashMap<String, DiscoveredFeed>,
        skipped_links: &mut usize,
    ) {
        let Some(href_raw) = tag.attr("href") else {
            return;
        };
        let Some(normalized_url) = self.resolve_feed_url(base, href_raw) else {
            // Only count as skipped if it looked like a feed path.
            if FeedFormat::infer_from_url_path(href_raw).is_some() {
                *skipped_links += 1;
            }
            return;
        };

        let Ok(parsed) = Url::parse(&normalized_url) else {
            return;
        };
        let Some(format) = FeedFormat::infer_from_url_path(parsed.path()) else {
            return;
        };

        let title = tag
            .attr("title")
            .map(|t| decode_html_entities(t.trim()))
            .filter(|t| !t.is_empty());

        let is_comments = is_comments_feed(&normalized_url, title.as_deref());
        if is_comments && !self.config.include_comment_feeds {
            return;
        }

        let mut score: u32 = 50;
        if title.is_some() {
            score += 5;
        }
        if is_comments {
            score = score.saturating_sub(25);
        }

        let candidate = DiscoveredFeed {
            url: normalized_url,
            format,
            mime_type: format.canonical_mime_type().to_string(),
            title,
            hreflang: None,
            source: DiscoverySource::AnchorHeuristic,
            is_comments_feed: is_comments,
            score,
        };

        insert_or_merge(by_url, candidate);
    }

    fn add_well_known_candidates(&self, base: &Url, by_url: &mut HashMap<String, DiscoveredFeed>) {
        const WELL_KNOWN: [(&str, FeedFormat, u32); 4] = [
            ("/feed", FeedFormat::Rss, 20),
            ("/rss.xml", FeedFormat::Rss, 19),
            ("/atom.xml", FeedFormat::Atom, 18),
            ("/feed.json", FeedFormat::JsonFeed, 17),
        ];

        for (path, format, score) in WELL_KNOWN {
            if let Ok(mut candidate_url) = base.join(path) {
                candidate_url.set_fragment(None);
                if self.validate_scheme(&candidate_url).is_ok() {
                    let candidate = DiscoveredFeed {
                        url: candidate_url.to_string(),
                        format,
                        mime_type: format.canonical_mime_type().to_string(),
                        title: None,
                        hreflang: None,
                        source: DiscoverySource::WellKnownPath,
                        is_comments_feed: false,
                        score,
                    };
                    insert_or_merge(by_url, candidate);
                }
            }
        }
    }

    fn resolve_feed_url(&self, base: &Url, raw_href: &str) -> Option<String> {
        let decoded = decode_html_entities(raw_href.trim());
        if decoded.is_empty() {
            return None;
        }
        let mut resolved = base.join(&decoded).ok()?;
        self.validate_scheme(&resolved).ok()?;
        resolved.set_fragment(None);
        Some(resolved.to_string())
    }
}

fn insert_or_merge(by_url: &mut HashMap<String, DiscoveredFeed>, candidate: DiscoveredFeed) {
    match by_url.get_mut(&candidate.url) {
        Some(existing) => {
            if candidate.score > existing.score {
                let fallback_title = existing.title.take();
                let fallback_lang = existing.hreflang.take();
                *existing = candidate;
                if existing.title.is_none() {
                    existing.title = fallback_title;
                }
                if existing.hreflang.is_none() {
                    existing.hreflang = fallback_lang;
                }
            } else {
                if existing.title.is_none() {
                    existing.title = candidate.title;
                }
                if existing.hreflang.is_none() {
                    existing.hreflang = candidate.hreflang;
                }
            }
        }
        None => {
            by_url.insert(candidate.url.clone(), candidate);
        }
    }
}

fn is_comments_feed(url: &str, title: Option<&str>) -> bool {
    let url_lower = url.to_ascii_lowercase();
    if url_lower.contains("/comments/feed")
        || url_lower.contains("comments-feed")
        || url_lower.contains("comment_feed")
    {
        return true;
    }
    if let Some(t) = title {
        let t_lower = t.to_ascii_lowercase();
        if t_lower.contains("comments feed") || t_lower.contains("comments for ") {
            return true;
        }
    }
    false
}

/// Minimal, panic-free HTML tag representation for attribute inspection.
struct HtmlTag<'a> {
    name: &'a str,
    attrs: Vec<(&'a str, &'a str)>,
}

impl<'a> HtmlTag<'a> {
    fn attr(&self, key: &str) -> Option<&'a str> {
        self.attrs
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| *v)
    }
}

/// Iterates over HTML tags (`<...>`) in `html`, skipping comments (`<!-- ... -->`).
fn iter_html_tags(html: &str) -> Vec<HtmlTag<'_>> {
    let mut tags = Vec::new();
    let bytes = html.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        let Some(rel_start) = memchr::memchr(b'<', &bytes[i..]) else {
            break;
        };
        let start = i + rel_start;

        // Skip HTML comments.
        if bytes[start..].starts_with(b"<!--") {
            if let Some(end_offset) = find_subslice(&bytes[start + 4..], b"-->") {
                i = start + 4 + end_offset + 3;
            } else {
                break;
            }
            continue;
        }

        // Find closing `>` respecting quoted attribute values.
        let mut j = start + 1;
        let mut quote: Option<u8> = None;
        while j < len {
            let b = bytes[j];
            match quote {
                Some(q) => {
                    if b == q {
                        quote = None;
                    }
                }
                None => {
                    if b == b'"' || b == b'\'' {
                        quote = Some(b);
                    } else if b == b'>' {
                        break;
                    }
                }
            }
            j += 1;
        }

        if j >= len {
            break;
        }

        let inner = &html[start + 1..j];
        if let Some(tag) = parse_tag_inner(inner) {
            tags.push(tag);
        }
        i = j + 1;
    }

    tags
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn parse_tag_inner(inner: &str) -> Option<HtmlTag<'_>> {
    let trimmed = inner.trim();
    if trimmed.is_empty() || trimmed.starts_with('!') || trimmed.starts_with('?') {
        return None;
    }

    let bytes = trimmed.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() && !bytes[idx].is_ascii_whitespace() && bytes[idx] != b'/' {
        idx += 1;
    }
    if idx == 0 {
        return None;
    }

    let name = &trimmed[..idx];
    let mut attrs = Vec::new();

    while idx < bytes.len() {
        while idx < bytes.len() && (bytes[idx].is_ascii_whitespace() || bytes[idx] == b'/') {
            idx += 1;
        }
        if idx >= bytes.len() {
            break;
        }

        let key_start = idx;
        while idx < bytes.len()
            && !bytes[idx].is_ascii_whitespace()
            && bytes[idx] != b'='
            && bytes[idx] != b'/'
        {
            idx += 1;
        }
        let key = &trimmed[key_start..idx];
        if key.is_empty() {
            idx += 1;
            continue;
        }

        while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
            idx += 1;
        }

        if idx < bytes.len() && bytes[idx] == b'=' {
            idx += 1;
            while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
                idx += 1;
            }
            if idx >= bytes.len() {
                attrs.push((key, ""));
                break;
            }
            let val = if bytes[idx] == b'"' || bytes[idx] == b'\'' {
                let q = bytes[idx];
                idx += 1;
                let val_start = idx;
                while idx < bytes.len() && bytes[idx] != q {
                    idx += 1;
                }
                let val_slice = &trimmed[val_start..idx];
                if idx < bytes.len() {
                    idx += 1;
                }
                val_slice
            } else {
                let val_start = idx;
                while idx < bytes.len() && !bytes[idx].is_ascii_whitespace() {
                    idx += 1;
                }
                &trimmed[val_start..idx]
            };
            attrs.push((key, val));
        } else {
            attrs.push((key, ""));
        }
    }

    Some(HtmlTag { name, attrs })
}

fn decode_html_entities(input: &str) -> String {
    if !input.contains('&') {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after_amp = &rest[amp + 1..];
        if let Some(semi) = after_amp.find(';') {
            let entity = &after_amp[..semi];
            if let Some(ch) = decode_single_entity(entity) {
                out.push(ch);
                rest = &after_amp[semi + 1..];
                continue;
            }
        }
        out.push('&');
        rest = after_amp;
    }
    out.push_str(rest);
    out
}

fn decode_single_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "#39" => Some('\''),
        _ if entity.starts_with("#x") || entity.starts_with("#X") => {
            u32::from_str_radix(&entity[2..], 16)
                .ok()
                .and_then(char::from_u32)
        }
        _ if entity.starts_with('#') => entity[1..].parse::<u32>().ok().and_then(char::from_u32),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_rss_atom_json_deduplicates_and_serializes() {
        let html = r#"
        <!doctype html>
        <html>
          <head>
            <title>Example Blog</title>
            <link rel="stylesheet" href="/style.css">
            <link href="/feed.xml" type="application/rss+xml; charset=utf-8" rel="alternate" title="News &amp; Updates">
            <link rel="alternate" type="application/atom+xml" title="Atom Feed" href="https://example.com/atom.xml#top">
            <link rel="alternate" type="application/feed+json" href="/feed.json">
            <!-- duplicate RSS without title should merge cleanly -->
            <link rel="alternate" type="application/rss+xml" href="https://example.com/feed.xml">
          </head>
          <body><p>Hello</p></body>
        </html>
        "#;

        let discoverer = FeedAutodiscoverer::new(DiscoveryConfig::default()).unwrap();
        let report = discoverer
            .discover("https://example.com/posts/hello", html)
            .unwrap();

        assert_eq!(report.page_url, "https://example.com/posts/hello");
        assert_eq!(report.effective_base_url, "https://example.com/posts/hello");
        assert_eq!(report.skipped_links, 0);
        assert_eq!(report.feeds.len(), 3);

        // Highest score: RSS & Atom with titles (115), then JSON Feed without title (100).
        assert_eq!(report.feeds[0].format, FeedFormat::Atom);
        assert_eq!(report.feeds[0].url, "https://example.com/atom.xml");
        assert_eq!(report.feeds[0].title.as_deref(), Some("Atom Feed"));

        assert_eq!(report.feeds[1].format, FeedFormat::Rss);
        assert_eq!(report.feeds[1].url, "https://example.com/feed.xml");
        assert_eq!(report.feeds[1].title.as_deref(), Some("News & Updates"));
        assert_eq!(report.feeds[1].mime_type, "application/rss+xml");

        assert_eq!(report.feeds[2].format, FeedFormat::JsonFeed);
        assert_eq!(report.feeds[2].url, "https://example.com/feed.json");
        assert_eq!(report.feeds[2].title, None);

        let json = report.to_json().unwrap();
        let round_trip = DiscoveryReport::from_json(&json).unwrap();
        assert_eq!(round_trip, report);
    }

    #[test]
    fn respects_base_tag_filters_comments_and_rejects_unsafe_schemes() {
        let html = r#"
        <html>
          <head>
            <base href="https://cdn.example.org/blog/">
            <link rel="alternate" type="application/rss+xml" title="Main RSS" href="rss.xml">
            <link rel="alternate" type="application/rss+xml" title="Comments Feed" href="/comments/feed/">
            <link rel="alternate" type="application/atom+xml" title="Bad" href="javascript:alert(1)">
          </head>
        </html>
        "#;

        let config = DiscoveryConfig {
            include_comment_feeds: false,
            ..DiscoveryConfig::default()
        };
        let discoverer = FeedAutodiscoverer::new(config).unwrap();
        let report = discoverer
            .discover("https://example.org/index.html", html)
            .unwrap();

        assert_eq!(report.effective_base_url, "https://cdn.example.org/blog/");
        assert_eq!(report.skipped_links, 1);
        assert_eq!(report.feeds.len(), 1);
        assert_eq!(report.feeds[0].url, "https://cdn.example.org/blog/rss.xml");
        assert!(!report.feeds[0].is_comments_feed);
    }

    #[test]
    fn anchor_fallback_and_well_known_candidates() {
        let html_with_anchor = r#"
        <html><body>
          <a href="/blog/feed.rss" title="Blog RSS">Subscribe</a>
          <a href="/about">About</a>
        </body></html>
        "#;

        let config_anchor = DiscoveryConfig {
            enable_anchor_fallback: true,
            ..DiscoveryConfig::default()
        };
        let discoverer = FeedAutodiscoverer::new(config_anchor).unwrap();
        let report = discoverer
            .discover("https://example.net/", html_with_anchor)
            .unwrap();

        assert_eq!(report.feeds.len(), 1);
        assert_eq!(report.feeds[0].url, "https://example.net/blog/feed.rss");
        assert_eq!(report.feeds[0].source, DiscoverySource::AnchorHeuristic);
        assert_eq!(report.feeds[0].format, FeedFormat::Rss);

        let config_well_known = DiscoveryConfig {
            enable_well_known_candidates: true,
            max_results: 2,
            ..DiscoveryConfig::default()
        };
        let discoverer_wk = FeedAutodiscoverer::new(config_well_known).unwrap();
        let report_wk = discoverer_wk
            .discover(
                "https://example.net/article",
                "<html><body>No links</body></html>",
            )
            .unwrap();

        assert_eq!(report_wk.feeds.len(), 2);
        assert_eq!(report_wk.feeds[0].source, DiscoverySource::WellKnownPath);
        assert_eq!(report_wk.feeds[0].url, "https://example.net/feed");
        assert_eq!(report_wk.feeds[1].url, "https://example.net/rss.xml");
    }

    #[test]
    fn validates_errors_and_handles_malformed_html() {
        let bad_cfg = DiscoveryConfig {
            max_html_bytes: 0,
            ..DiscoveryConfig::default()
        };
        assert!(matches!(
            FeedAutodiscoverer::new(bad_cfg),
            Err(FeedDiscoveryError::InvalidConfig(_))
        ));

        let https_only = DiscoveryConfig {
            allow_http: false,
            max_html_bytes: 64,
            ..DiscoveryConfig::default()
        };
        let discoverer = FeedAutodiscoverer::new(https_only).unwrap();

        assert!(matches!(
            discoverer.discover("http://example.com", "<html/>"),
            Err(FeedDiscoveryError::UnsupportedBaseScheme(ref s)) if s == "http"
        ));

        let oversized = "a".repeat(65);
        assert!(matches!(
            discoverer.discover("https://example.com", &oversized),
            Err(FeedDiscoveryError::HtmlTooLarge {
                size: 65,
                max_size: 64
            })
        ));

        // Unquoted attributes & mixed case should still parse without panicking.
        let quirky = "<LINK REL=alternate TYPE=application/atom+xml HREF=/a.atom>";
        let ok_discoverer = FeedAutodiscoverer::new(DiscoveryConfig::default()).unwrap();
        let report = ok_discoverer
            .discover("https://example.com", quirky)
            .unwrap();
        assert_eq!(report.feeds.len(), 1);
        assert_eq!(report.feeds[0].url, "https://example.com/a.atom");
        assert_eq!(report.feeds[0].format, FeedFormat::Atom);
    }
}
