use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;
use url::Url;

pub const MAX_QUOTE_LEN: usize = 4096;
pub const MAX_CONTEXT_LEN: usize = 256;
pub const MAX_NOTE_LEN: usize = 8192;
pub const MAX_TAGS_PER_HIGHLIGHT: usize = 16;
pub const MAX_TAG_LEN: usize = 48;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReaderHighlightError {
    #[error("invalid document URL: {0}")]
    InvalidUrl(String),
    #[error("highlight ID must not be empty")]
    EmptyId,
    #[error("highlight quote must not be empty")]
    EmptyQuote,
    #[error("invalid text span: start ({start}) must be less than end ({end})")]
    InvalidSpan { start: usize, end: usize },
    #[error("quote length {len} exceeds maximum {max}")]
    QuoteTooLong { len: usize, max: usize },
    #[error("note length {len} exceeds maximum {max}")]
    NoteTooLong { len: usize, max: usize },
    #[error("invalid hex color '{0}': expected #RRGGBB")]
    InvalidColorHex(String),
    #[error("invalid tag '{0}'")]
    InvalidTag(String),
    #[error("too many tags ({count}), maximum is {max}")]
    TooManyTags { count: usize, max: usize },
    #[error("duplicate highlight ID: {0}")]
    DuplicateId(String),
    #[error("highlight not found: {0}")]
    NotFound(String),
    #[error("serialization error: {0}")]
    Serialization(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HighlightColor {
    Yellow,
    Green,
    Blue,
    Pink,
    Purple,
    Custom(String),
}

impl HighlightColor {
    pub fn validate(&self) -> Result<(), ReaderHighlightError> {
        if let Self::Custom(hex) = self {
            if !is_valid_hex_color(hex) {
                return Err(ReaderHighlightError::InvalidColorHex(hex.clone()));
            }
        }
        Ok(())
    }

    pub fn as_hex(&self) -> &str {
        match self {
            Self::Yellow => "#FEF08A",
            Self::Green => "#BBF7D0",
            Self::Blue => "#BFDBFE",
            Self::Pink => "#FBCFE8",
            Self::Purple => "#DDD6FE",
            Self::Custom(hex) => hex.as_str(),
        }
    }
}

fn is_valid_hex_color(hex: &str) -> bool {
    let bytes = hex.as_bytes();
    bytes.len() == 7 && bytes[0] == b'#' && bytes[1..].iter().all(|b| b.is_ascii_hexdigit())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextAnchor {
    pub start_offset: usize,
    pub end_offset: usize,
    pub quote: String,
    pub prefix_context: String,
    pub suffix_context: String,
    pub section_heading: Option<String>,
}

impl TextAnchor {
    pub fn new(
        start_offset: usize,
        end_offset: usize,
        quote: impl Into<String>,
    ) -> Result<Self, ReaderHighlightError> {
        let quote = quote.into();
        let anchor = Self {
            start_offset,
            end_offset,
            quote,
            prefix_context: String::new(),
            suffix_context: String::new(),
            section_heading: None,
        };
        anchor.validate()?;
        Ok(anchor)
    }

    pub fn validate(&self) -> Result<(), ReaderHighlightError> {
        if self.start_offset >= self.end_offset {
            return Err(ReaderHighlightError::InvalidSpan {
                start: self.start_offset,
                end: self.end_offset,
            });
        }
        let trimmed = self.quote.trim();
        if trimmed.is_empty() {
            return Err(ReaderHighlightError::EmptyQuote);
        }
        if self.quote.len() > MAX_QUOTE_LEN {
            return Err(ReaderHighlightError::QuoteTooLong {
                len: self.quote.len(),
                max: MAX_QUOTE_LEN,
            });
        }
        Ok(())
    }

    pub fn with_context(
        mut self,
        prefix: impl Into<String>,
        suffix: impl Into<String>,
        section_heading: Option<String>,
    ) -> Self {
        self.prefix_context = truncate_tail(&prefix.into(), MAX_CONTEXT_LEN);
        self.suffix_context = truncate_head(&suffix.into(), MAX_CONTEXT_LEN);
        self.section_heading = section_heading.and_then(|h| {
            let trimmed = h.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighlightNote {
    pub id: String,
    pub document_url: String,
    pub document_title: String,
    pub anchor: TextAnchor,
    pub color: HighlightColor,
    pub note: Option<String>,
    pub tags: Vec<String>,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone)]
pub struct CreateHighlightRequest {
    pub id: String,
    pub document_url: String,
    pub document_title: String,
    pub anchor: TextAnchor,
    pub color: HighlightColor,
    pub note: Option<String>,
    pub tags: Vec<String>,
    pub timestamp_unix_ms: i64,
}

#[derive(Debug, Clone, Default)]
pub struct HighlightQuery {
    pub document_url: Option<String>,
    pub color: Option<HighlightColor>,
    pub tag: Option<String>,
    pub text_contains: Option<String>,
    pub only_with_notes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReanchorReport {
    pub unchanged: usize,
    pub relocated: usize,
    pub orphaned_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HighlightStore {
    highlights: BTreeMap<String, HighlightNote>,
}

impl HighlightStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.highlights.len()
    }

    pub fn is_empty(&self) -> bool {
        self.highlights.is_empty()
    }

    pub fn add_highlight(
        &mut self,
        req: CreateHighlightRequest,
    ) -> Result<&HighlightNote, ReaderHighlightError> {
        let id = req.id.trim().to_string();
        if id.is_empty() {
            return Err(ReaderHighlightError::EmptyId);
        }
        if self.highlights.contains_key(&id) {
            return Err(ReaderHighlightError::DuplicateId(id));
        }

        let canonical_url = normalize_document_url(&req.document_url)?;
        req.anchor.validate()?;
        req.color.validate()?;
        let note = normalize_note(req.note)?;
        let tags = normalize_tags(req.tags)?;

        let entry = HighlightNote {
            id: id.clone(),
            document_url: canonical_url,
            document_title: req.document_title.trim().to_string(),
            anchor: req.anchor,
            color: req.color,
            note,
            tags,
            created_at_unix_ms: req.timestamp_unix_ms,
            updated_at_unix_ms: req.timestamp_unix_ms,
        };

        self.highlights.insert(id.clone(), entry);
        self.highlights
            .get(&id)
            .ok_or(ReaderHighlightError::NotFound(id))
    }

    pub fn get(&self, id: &str) -> Option<&HighlightNote> {
        self.highlights.get(id)
    }

    pub fn update_note(
        &mut self,
        id: &str,
        note: Option<String>,
        updated_at_unix_ms: i64,
    ) -> Result<&HighlightNote, ReaderHighlightError> {
        let normalized = normalize_note(note)?;
        let item = self
            .highlights
            .get_mut(id)
            .ok_or_else(|| ReaderHighlightError::NotFound(id.to_string()))?;
        item.note = normalized;
        item.updated_at_unix_ms = updated_at_unix_ms;
        Ok(item)
    }

    pub fn update_color(
        &mut self,
        id: &str,
        color: HighlightColor,
        updated_at_unix_ms: i64,
    ) -> Result<&HighlightNote, ReaderHighlightError> {
        color.validate()?;
        let item = self
            .highlights
            .get_mut(id)
            .ok_or_else(|| ReaderHighlightError::NotFound(id.to_string()))?;
        item.color = color;
        item.updated_at_unix_ms = updated_at_unix_ms;
        Ok(item)
    }

    pub fn set_tags(
        &mut self,
        id: &str,
        tags: Vec<String>,
        updated_at_unix_ms: i64,
    ) -> Result<&HighlightNote, ReaderHighlightError> {
        let normalized = normalize_tags(tags)?;
        let item = self
            .highlights
            .get_mut(id)
            .ok_or_else(|| ReaderHighlightError::NotFound(id.to_string()))?;
        item.tags = normalized;
        item.updated_at_unix_ms = updated_at_unix_ms;
        Ok(item)
    }

    pub fn remove_highlight(&mut self, id: &str) -> Result<HighlightNote, ReaderHighlightError> {
        self.highlights
            .remove(id)
            .ok_or_else(|| ReaderHighlightError::NotFound(id.to_string()))
    }

    pub fn list_for_url(
        &self,
        document_url: &str,
    ) -> Result<Vec<&HighlightNote>, ReaderHighlightError> {
        let canonical = normalize_document_url(document_url)?;
        let mut results: Vec<&HighlightNote> = self
            .highlights
            .values()
            .filter(|h| h.document_url == canonical)
            .collect();
        results.sort_by_key(|h| (h.anchor.start_offset, h.created_at_unix_ms));
        Ok(results)
    }

    pub fn search(&self, query: &HighlightQuery) -> Vec<&HighlightNote> {
        let canonical_url = query
            .document_url
            .as_deref()
            .and_then(|u| normalize_document_url(u).ok());
        if query.document_url.is_some() && canonical_url.is_none() {
            return Vec::new();
        }
        let needle = query.text_contains.as_deref().map(|s| s.to_lowercase());
        let tag_filter = query.tag.as_deref().map(|t| t.trim().to_lowercase());

        let mut results: Vec<&HighlightNote> = self
            .highlights
            .values()
            .filter(|h| {
                if let Some(ref url) = canonical_url {
                    if &h.document_url != url {
                        return false;
                    }
                }
                if let Some(ref color) = query.color {
                    if &h.color != color {
                        return false;
                    }
                }
                if let Some(ref tag) = tag_filter {
                    if !h.tags.iter().any(|t| t == tag) {
                        return false;
                    }
                }
                if query.only_with_notes && h.note.is_none() {
                    return false;
                }
                if let Some(ref q) = needle {
                    let in_quote = h.anchor.quote.to_lowercase().contains(q);
                    let in_note = h
                        .note
                        .as_deref()
                        .map(|n| n.to_lowercase().contains(q))
                        .unwrap_or(false);
                    if !in_quote && !in_note {
                        return false;
                    }
                }
                true
            })
            .collect();

        results.sort_by_key(|h| (h.anchor.start_offset, h.created_at_unix_ms));
        results
    }

    pub fn reanchor_in_text(
        &mut self,
        document_url: &str,
        current_text: &str,
        now_unix_ms: i64,
    ) -> Result<ReanchorReport, ReaderHighlightError> {
        let canonical = normalize_document_url(document_url)?;
        let mut report = ReanchorReport {
            unchanged: 0,
            relocated: 0,
            orphaned_ids: Vec::new(),
        };

        for item in self
            .highlights
            .values_mut()
            .filter(|h| h.document_url == canonical)
        {
            if matches_at_offset(
                current_text,
                item.anchor.start_offset,
                item.anchor.end_offset,
                &item.anchor.quote,
            ) {
                report.unchanged += 1;
                continue;
            }

            if let Some((new_start, new_end)) = find_anchor_position(current_text, &item.anchor) {
                item.anchor.start_offset = new_start;
                item.anchor.end_offset = new_end;
                item.updated_at_unix_ms = now_unix_ms;
                report.relocated += 1;
            } else {
                report.orphaned_ids.push(item.id.clone());
            }
        }

        Ok(report)
    }

    pub fn export_markdown(&self, document_url: &str) -> Result<String, ReaderHighlightError> {
        let items = self.list_for_url(document_url)?;
        let mut out = String::new();
        let title = items
            .first()
            .map(|h| h.document_title.as_str())
            .filter(|t| !t.is_empty())
            .unwrap_or("Reader Highlights");
        out.push_str(&format!("# {}\n\nSource: {}\n\n", title, document_url));

        for item in items {
            if let Some(ref heading) = item.anchor.section_heading {
                out.push_str(&format!("## {}\n\n", heading));
            }
            out.push_str(&format!("> {}\n\n", item.anchor.quote.trim()));
            if let Some(ref note) = item.note {
                out.push_str(&format!("Note: {}\n\n", note));
            }
            if !item.tags.is_empty() {
                out.push_str(&format!("Tags: {}\n\n", item.tags.join(", ")));
            }
        }

        Ok(out)
    }

    pub fn to_json(&self) -> Result<String, ReaderHighlightError> {
        serde_json::to_string_pretty(self)
            .map_err(|e| ReaderHighlightError::Serialization(e.to_string()))
    }

    pub fn from_json(raw: &str) -> Result<Self, ReaderHighlightError> {
        let store: Self = serde_json::from_str(raw)
            .map_err(|e| ReaderHighlightError::Serialization(e.to_string()))?;
        for item in store.highlights.values() {
            if item.id.trim().is_empty() {
                return Err(ReaderHighlightError::EmptyId);
            }
            normalize_document_url(&item.document_url)?;
            item.anchor.validate()?;
            item.color.validate()?;
            if let Some(ref note) = item.note {
                if note.len() > MAX_NOTE_LEN {
                    return Err(ReaderHighlightError::NoteTooLong {
                        len: note.len(),
                        max: MAX_NOTE_LEN,
                    });
                }
            }
        }
        Ok(store)
    }
}

fn normalize_document_url(raw: &str) -> Result<String, ReaderHighlightError> {
    let mut parsed =
        Url::parse(raw.trim()).map_err(|_| ReaderHighlightError::InvalidUrl(raw.to_string()))?;
    match parsed.scheme() {
        "http" | "https" | "file" => {}
        _ => return Err(ReaderHighlightError::InvalidUrl(raw.to_string())),
    }
    parsed.set_fragment(None);
    Ok(parsed.to_string())
}

fn normalize_note(note: Option<String>) -> Result<Option<String>, ReaderHighlightError> {
    match note {
        None => Ok(None),
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else if trimmed.len() > MAX_NOTE_LEN {
                Err(ReaderHighlightError::NoteTooLong {
                    len: trimmed.len(),
                    max: MAX_NOTE_LEN,
                })
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
    }
}

fn normalize_tags(tags: Vec<String>) -> Result<Vec<String>, ReaderHighlightError> {
    let mut unique = BTreeSet::new();
    for raw in tags {
        let normalized = raw.trim().to_lowercase();
        if normalized.is_empty()
            || normalized.len() > MAX_TAG_LEN
            || normalized.chars().any(char::is_whitespace)
        {
            return Err(ReaderHighlightError::InvalidTag(raw));
        }
        unique.insert(normalized);
    }
    if unique.len() > MAX_TAGS_PER_HIGHLIGHT {
        return Err(ReaderHighlightError::TooManyTags {
            count: unique.len(),
            max: MAX_TAGS_PER_HIGHLIGHT,
        });
    }
    Ok(unique.into_iter().collect())
}

fn matches_at_offset(text: &str, start: usize, end: usize, quote: &str) -> bool {
    text.get(start..end)
        .map(|slice| slice == quote)
        .unwrap_or(false)
}

fn find_anchor_position(text: &str, anchor: &TextAnchor) -> Option<(usize, usize)> {
    if !anchor.prefix_context.is_empty() || !anchor.suffix_context.is_empty() {
        let composite = format!(
            "{}{}{}",
            anchor.prefix_context, anchor.quote, anchor.suffix_context
        );
        if let Some(pos) = text.find(&composite) {
            let start = pos + anchor.prefix_context.len();
            let end = start + anchor.quote.len();
            return Some((start, end));
        }
    }
    text.find(&anchor.quote)
        .map(|start| (start, start + anchor.quote.len()))
}

fn truncate_tail(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut idx = s.len() - max_bytes;
    while !s.is_char_boundary(idx) && idx < s.len() {
        idx += 1;
    }
    s[idx..].to_string()
}

fn truncate_head(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut idx = max_bytes;
    while !s.is_char_boundary(idx) && idx > 0 {
        idx -= 1;
    }
    s[..idx].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request(id: &str, start: usize, end: usize, quote: &str) -> CreateHighlightRequest {
        CreateHighlightRequest {
            id: id.to_string(),
            document_url: "https://example.com/articles/rust-reader#section-1".to_string(),
            document_title: "Rust Reader Architecture".to_string(),
            anchor: TextAnchor::new(start, end, quote).unwrap(),
            color: HighlightColor::Yellow,
            note: Some("  Important design insight  ".to_string()),
            tags: vec!["Rust".to_string(), "reader".to_string(), "rust".to_string()],
            timestamp_unix_ms: 1_700_000_000_000,
        }
    }

    #[test]
    fn add_update_search_and_export_happy_path() {
        let mut store = HighlightStore::new();
        let created = store
            .add_highlight(sample_request("hl-1", 10, 25, "ownership model"))
            .unwrap()
            .clone();

        assert_eq!(
            created.document_url,
            "https://example.com/articles/rust-reader"
        );
        assert_eq!(created.note.as_deref(), Some("Important design insight"));
        assert_eq!(created.tags, vec!["reader", "rust"]);

        store
            .update_color(
                "hl-1",
                HighlightColor::Custom("#12AB34".to_string()),
                1_700_000_005_000,
            )
            .unwrap();
        store
            .update_note("hl-1", Some("Updated note".to_string()), 1_700_000_010_000)
            .unwrap();

        let matches = store.search(&HighlightQuery {
            tag: Some("RUST".to_string()),
            text_contains: Some("updated".to_string()),
            only_with_notes: true,
            ..Default::default()
        });
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].color.as_hex(), "#12AB34");

        let md = store
            .export_markdown("https://example.com/articles/rust-reader")
            .unwrap();
        assert!(md.contains("# Rust Reader Architecture"));
        assert!(md.contains("> ownership model"));
        assert!(md.contains("Note: Updated note"));
        assert!(md.contains("Tags: reader, rust"));
    }

    #[test]
    fn validation_rejects_invalid_inputs() {
        let mut store = HighlightStore::new();

        assert_eq!(
            TextAnchor::new(10, 10, "abc"),
            Err(ReaderHighlightError::InvalidSpan { start: 10, end: 10 })
        );
        assert_eq!(
            TextAnchor::new(0, 5, "   "),
            Err(ReaderHighlightError::EmptyQuote)
        );
        assert_eq!(
            HighlightColor::Custom("#12345Z".to_string()).validate(),
            Err(ReaderHighlightError::InvalidColorHex("#12345Z".to_string()))
        );

        let mut bad_url = sample_request("hl-bad", 0, 5, "hello");
        bad_url.document_url = "javascript:alert(1)".to_string();
        assert!(matches!(
            store.add_highlight(bad_url),
            Err(ReaderHighlightError::InvalidUrl(_))
        ));

        let mut bad_tag = sample_request("hl-tag", 0, 5, "hello");
        bad_tag.tags = vec!["invalid tag".to_string()];
        assert!(matches!(
            store.add_highlight(bad_tag),
            Err(ReaderHighlightError::InvalidTag(_))
        ));

        store
            .add_highlight(sample_request("hl-dup", 0, 5, "hello"))
            .unwrap();
        assert_eq!(
            store.add_highlight(sample_request("hl-dup", 0, 5, "hello")),
            Err(ReaderHighlightError::DuplicateId("hl-dup".to_string()))
        );
    }

    #[test]
    fn reanchor_relocates_shifted_quotes_and_reports_orphans() {
        let mut store = HighlightStore::new();
        let doc_url = "https://example.com/doc";

        let anchor_shifted = TextAnchor::new(0, 11, "target text").unwrap().with_context(
            "before ",
            " after",
            Some("Intro".to_string()),
        );
        store
            .add_highlight(CreateHighlightRequest {
                id: "hl-shift".to_string(),
                document_url: doc_url.to_string(),
                document_title: "Doc".to_string(),
                anchor: anchor_shifted,
                color: HighlightColor::Blue,
                note: None,
                tags: vec![],
                timestamp_unix_ms: 100,
            })
            .unwrap();

        let anchor_orphan = TextAnchor::new(5, 16, "removed text").unwrap();
        store
            .add_highlight(CreateHighlightRequest {
                id: "hl-orphan".to_string(),
                document_url: doc_url.to_string(),
                document_title: "Doc".to_string(),
                anchor: anchor_orphan,
                color: HighlightColor::Pink,
                note: None,
                tags: vec![],
                timestamp_unix_ms: 100,
            })
            .unwrap();

        let updated_doc = "New header. target text elsewhere. Real before target text after end.";
        let report = store.reanchor_in_text(doc_url, updated_doc, 200).unwrap();

        assert_eq!(report.unchanged, 0);
        assert_eq!(report.relocated, 1);
        assert_eq!(report.orphaned_ids, vec!["hl-orphan"]);

        let relocated = store.get("hl-shift").unwrap();
        assert_eq!(
            &updated_doc[relocated.anchor.start_offset..relocated.anchor.end_offset],
            "target text"
        );
        // Should match the occurrence with prefix/suffix context, not the earlier one
        assert!(relocated.anchor.start_offset > 30);
    }

    #[test]
    fn json_roundtrip_and_removal() {
        let mut store = HighlightStore::new();
        store
            .add_highlight(sample_request("hl-json", 2, 7, "rusty"))
            .unwrap();

        let json = store.to_json().unwrap();
        let mut restored = HighlightStore::from_json(&json).unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored.get("hl-json"), store.get("hl-json"));

        let removed = restored.remove_highlight("hl-json").unwrap();
        assert_eq!(removed.id, "hl-json");
        assert!(restored.is_empty());
    }
}
