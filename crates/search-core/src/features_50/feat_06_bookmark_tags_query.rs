//! Feature 06: Bookmark Tags & Query Language.
//!
//! Provides validated hierarchical bookmark tags, an inverted tag index,
//! JSON persistence, and a boolean query parser/evaluator supporting:
//! - Tag filters (`tag:rust`, `#rust`, `tag:dev/*`)
//! - Domain/URL/title/folder/notes filters (`site:github.com`, `title:"async"`, `url:docs`, `folder:work`)
//! - State & time filters (`is:pinned`, `is:untagged`, `after:1700000000`, `before:1800000000`)
//! - Regex filters (`re:^https://docs\.rs/`)
//! - Bare terms and quoted phrases
//! - Boolean operators (`AND`, `OR`, `NOT`, `-` negation, parentheses)
//! - Sorting and limit directives (`sort:relevance|recent|visits|title`, `limit:20`)

use regex::RegexBuilder;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use thiserror::Error;
use url::Url;

/// Maximum length of a normalized tag in bytes.
pub const MAX_TAG_LEN: usize = 64;

/// Maximum number of tags allowed on a single bookmark by default.
pub const DEFAULT_MAX_TAGS_PER_BOOKMARK: usize = 64;

/// Maximum nesting depth for parenthesized query expressions.
pub const MAX_QUERY_DEPTH: usize = 32;

/// Maximum compiled regex size in bytes for `re:` predicates.
const MAX_REGEX_SIZE_BYTES: usize = 64 * 1024;

/// Errors returned by bookmark tag and query operations.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum BookmarkTagError {
    #[error("invalid tag: {0}")]
    InvalidTag(String),

    #[error("invalid bookmark id: {0}")]
    InvalidBookmarkId(String),

    #[error("invalid bookmark url: {0}")]
    InvalidUrl(String),

    #[error("bookmark not found: {0}")]
    BookmarkNotFound(String),

    #[error("tag limit exceeded (max {max})")]
    TagLimitExceeded { max: usize },

    #[error("query parse error at byte {position}: {message}")]
    QueryParseError { position: usize, message: String },

    #[error("query nesting exceeds max depth of {max_depth}")]
    QueryTooDeep { max_depth: usize },

    #[error("invalid regex predicate: {0}")]
    InvalidRegex(String),

    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Validated, canonicalized bookmark tag (lowercase ASCII/Unicode-free slug with optional `/` hierarchy).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Tag(String);

impl Tag {
    /// Parses and normalizes a raw tag string.
    ///
    /// Rules:
    /// - Leading `#` is stripped if present.
    /// - Trimmed and lowercased.
    /// - Spaces are converted to `-`.
    /// - Allowed characters: ASCII `[a-z0-9\-_/.:]`.
    /// - Cannot be empty, exceed `MAX_TAG_LEN`, start/end with `/`, or contain `//`.
    pub fn parse(raw: &str) -> Result<Self, BookmarkTagError> {
        let trimmed = raw.trim().trim_start_matches('#').trim();
        if trimmed.is_empty() {
            return Err(BookmarkTagError::InvalidTag(
                "tag cannot be empty".to_string(),
            ));
        }

        let mut normalized = String::with_capacity(trimmed.len());
        let mut prev_dash = false;
        for ch in trimmed.chars() {
            let lower = ch.to_ascii_lowercase();
            if lower.is_whitespace() {
                if !prev_dash && !normalized.is_empty() {
                    normalized.push('-');
                    prev_dash = true;
                }
                continue;
            }
            prev_dash = lower == '-';
            if lower.is_ascii_alphanumeric() || matches!(lower, '-' | '_' | '/' | '.' | ':') {
                normalized.push(lower);
            } else {
                return Err(BookmarkTagError::InvalidTag(format!(
                    "unsupported character '{ch}' in tag '{raw}'"
                )));
            }
        }

        let normalized = normalized.trim_matches('-').to_string();
        if normalized.is_empty() {
            return Err(BookmarkTagError::InvalidTag(
                "tag cannot be empty after normalization".to_string(),
            ));
        }
        if normalized.len() > MAX_TAG_LEN {
            return Err(BookmarkTagError::InvalidTag(format!(
                "tag exceeds maximum length of {MAX_TAG_LEN} bytes"
            )));
        }
        if normalized.starts_with('/') || normalized.ends_with('/') || normalized.contains("//") {
            return Err(BookmarkTagError::InvalidTag(format!(
                "invalid hierarchy separator in tag '{normalized}'"
            )));
        }

        Ok(Self(normalized))
    }

    /// Returns the normalized tag string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns true if this tag equals `prefix` or is nested under `prefix/`.
    pub fn matches_prefix(&self, prefix: &str) -> bool {
        let clean = prefix
            .trim()
            .trim_end_matches('/')
            .trim_end_matches('*')
            .trim_end_matches('/');
        if clean.is_empty() {
            return false;
        }
        let clean_lower = clean.to_ascii_lowercase();
        self.0 == clean_lower
            || (self.0.starts_with(&clean_lower)
                && self.0.as_bytes().get(clean_lower.len()) == Some(&b'/'))
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Tag {
    type Error = BookmarkTagError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<Tag> for String {
    fn from(tag: Tag) -> Self {
        tag.0
    }
}

/// A tagged bookmark record stored in `TagStore`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaggedBookmark {
    pub id: String,
    pub title: String,
    pub url: String,
    pub domain: String,
    #[serde(default)]
    pub tags: BTreeSet<Tag>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    #[serde(default)]
    pub pinned: bool,
    pub created_at_unix: i64,
    pub updated_at_unix: i64,
    #[serde(default)]
    pub visit_count: u64,
}

impl TaggedBookmark {
    /// Creates a validated `TaggedBookmark`, extracting the normalized domain from `url`.
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        raw_url: impl AsRef<str>,
        created_at_unix: i64,
    ) -> Result<Self, BookmarkTagError> {
        let id = id.into().trim().to_string();
        if id.is_empty() {
            return Err(BookmarkTagError::InvalidBookmarkId(
                "bookmark id cannot be empty".to_string(),
            ));
        }

        let parsed = Url::parse(raw_url.as_ref().trim())
            .map_err(|e| BookmarkTagError::InvalidUrl(e.to_string()))?;
        let domain = parsed
            .host_str()
            .map(|h| h.to_ascii_lowercase())
            .unwrap_or_default();

        let raw_title = title.into();
        let title = if raw_title.trim().is_empty() {
            if domain.is_empty() {
                parsed.to_string()
            } else {
                domain.clone()
            }
        } else {
            raw_title.trim().to_string()
        };

        Ok(Self {
            id,
            title,
            url: parsed.to_string(),
            domain,
            tags: BTreeSet::new(),
            notes: None,
            folder: None,
            pinned: false,
            created_at_unix,
            updated_at_unix: created_at_unix,
            visit_count: 0,
        })
    }

    /// Builder helper to attach validated tags.
    pub fn with_tags<I, S>(mut self, raw_tags: I) -> Result<Self, BookmarkTagError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for raw in raw_tags {
            let tag = Tag::parse(raw.as_ref())?;
            self.tags.insert(tag);
        }
        Ok(self)
    }

    /// Builder helper to set optional folder.
    pub fn with_folder(mut self, folder: impl Into<String>) -> Self {
        let f = folder.into().trim().to_string();
        self.folder = if f.is_empty() { None } else { Some(f) };
        self
    }

    /// Builder helper to set optional notes.
    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        let n = notes.into().trim().to_string();
        self.notes = if n.is_empty() { None } else { Some(n) };
        self
    }

    /// Builder helper to mark as pinned.
    pub fn with_pinned(mut self, pinned: bool) -> Self {
        self.pinned = pinned;
        self
    }

    /// Builder helper to set visit count.
    pub fn with_visit_count(mut self, visit_count: u64) -> Self {
        self.visit_count = visit_count;
        self
    }
}

/// Summary count for a single tag in the store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagStats {
    pub tag: Tag,
    pub count: usize,
}

/// Sort order for query results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    #[default]
    Relevance,
    Recent,
    Visits,
    Title,
}

/// A parsed query with its boolean expression tree and optional result modifiers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedQuery {
    pub raw: String,
    pub expr: Option<QueryExpr>,
    pub sort: SortOrder,
    pub limit: Option<usize>,
}

/// Boolean query AST.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryExpr {
    And(Vec<QueryExpr>),
    Or(Vec<QueryExpr>),
    Not(Box<QueryExpr>),
    Predicate(Predicate),
}

/// Individual search predicate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Predicate {
    TagExact(Tag),
    TagPrefix(String),
    Site(String),
    Title(String),
    Url(String),
    Folder(String),
    Pinned(bool),
    Untagged,
    AfterUnix(i64),
    BeforeUnix(i64),
    Regex(String),
    Text(String),
}

/// Single matched bookmark with relevance score and matched tags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryMatch {
    pub bookmark: TaggedBookmark,
    pub score: u32,
    pub matched_tags: Vec<Tag>,
}

/// Result of executing a query against `TagStore`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryResult {
    pub total_matches: usize,
    pub matches: Vec<QueryMatch>,
    pub parsed_query: ParsedQuery,
}

/// In-memory bookmark store with an inverted tag index and JSON serialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagStore {
    bookmarks: BTreeMap<String, TaggedBookmark>,
    #[serde(skip)]
    tag_index: BTreeMap<Tag, BTreeSet<String>>,
    #[serde(default = "default_max_tags")]
    max_tags_per_bookmark: usize,
}

fn default_max_tags() -> usize {
    DEFAULT_MAX_TAGS_PER_BOOKMARK
}

impl Default for TagStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TagStore {
    /// Creates an empty `TagStore` with default limits.
    pub fn new() -> Self {
        Self {
            bookmarks: BTreeMap::new(),
            tag_index: BTreeMap::new(),
            max_tags_per_bookmark: DEFAULT_MAX_TAGS_PER_BOOKMARK,
        }
    }

    /// Creates a `TagStore` with a custom per-bookmark tag limit.
    pub fn with_max_tags_per_bookmark(max_tags: usize) -> Self {
        Self {
            bookmarks: BTreeMap::new(),
            tag_index: BTreeMap::new(),
            max_tags_per_bookmark: max_tags,
        }
    }

    /// Returns the number of bookmarks in the store.
    pub fn len(&self) -> usize {
        self.bookmarks.len()
    }

    /// Returns true if the store has no bookmarks.
    pub fn is_empty(&self) -> bool {
        self.bookmarks.is_empty()
    }

    /// Retrieves a bookmark by id.
    pub fn get(&self, id: &str) -> Option<&TaggedBookmark> {
        self.bookmarks.get(id)
    }

    /// Inserts or replaces a bookmark, updating the inverted tag index.
    pub fn upsert_bookmark(&mut self, bookmark: TaggedBookmark) -> Result<(), BookmarkTagError> {
        if bookmark.id.trim().is_empty() {
            return Err(BookmarkTagError::InvalidBookmarkId(
                "bookmark id cannot be empty".to_string(),
            ));
        }
        if bookmark.tags.len() > self.max_tags_per_bookmark {
            return Err(BookmarkTagError::TagLimitExceeded {
                max: self.max_tags_per_bookmark,
            });
        }

        // Remove old tag index entries if replacing an existing bookmark.
        if let Some(existing) = self.bookmarks.get(&bookmark.id) {
            for old_tag in &existing.tags {
                if let Some(ids) = self.tag_index.get_mut(old_tag) {
                    ids.remove(&bookmark.id);
                    if ids.is_empty() {
                        self.tag_index.remove(old_tag);
                    }
                }
            }
        }

        for tag in &bookmark.tags {
            self.tag_index
                .entry(tag.clone())
                .or_default()
                .insert(bookmark.id.clone());
        }
        self.bookmarks.insert(bookmark.id.clone(), bookmark);
        Ok(())
    }

    /// Removes a bookmark and cleans up its tag index entries.
    pub fn remove_bookmark(&mut self, id: &str) -> Result<TaggedBookmark, BookmarkTagError> {
        let removed = self
            .bookmarks
            .remove(id)
            .ok_or_else(|| BookmarkTagError::BookmarkNotFound(id.to_string()))?;
        for tag in &removed.tags {
            if let Some(ids) = self.tag_index.get_mut(tag) {
                ids.remove(id);
                if ids.is_empty() {
                    self.tag_index.remove(tag);
                }
            }
        }
        Ok(removed)
    }

    /// Adds a tag to an existing bookmark. Returns `true` if newly added.
    pub fn add_tag(
        &mut self,
        bookmark_id: &str,
        raw_tag: &str,
        now_unix: i64,
    ) -> Result<bool, BookmarkTagError> {
        let tag = Tag::parse(raw_tag)?;
        let bookmark = self
            .bookmarks
            .get_mut(bookmark_id)
            .ok_or_else(|| BookmarkTagError::BookmarkNotFound(bookmark_id.to_string()))?;

        if bookmark.tags.contains(&tag) {
            return Ok(false);
        }
        if bookmark.tags.len() >= self.max_tags_per_bookmark {
            return Err(BookmarkTagError::TagLimitExceeded {
                max: self.max_tags_per_bookmark,
            });
        }

        bookmark.tags.insert(tag.clone());
        bookmark.updated_at_unix = now_unix;
        self.tag_index
            .entry(tag)
            .or_default()
            .insert(bookmark_id.to_string());
        Ok(true)
    }

    /// Removes a tag from an existing bookmark. Returns `true` if it was present.
    pub fn remove_tag(
        &mut self,
        bookmark_id: &str,
        raw_tag: &str,
        now_unix: i64,
    ) -> Result<bool, BookmarkTagError> {
        let tag = Tag::parse(raw_tag)?;
        let bookmark = self
            .bookmarks
            .get_mut(bookmark_id)
            .ok_or_else(|| BookmarkTagError::BookmarkNotFound(bookmark_id.to_string()))?;

        if !bookmark.tags.remove(&tag) {
            return Ok(false);
        }
        bookmark.updated_at_unix = now_unix;

        if let Some(ids) = self.tag_index.get_mut(&tag) {
            ids.remove(bookmark_id);
            if ids.is_empty() {
                self.tag_index.remove(&tag);
            }
        }
        Ok(true)
    }

    /// Renames `old_raw` tag to `new_raw` across all bookmarks. Returns the number of updated bookmarks.
    pub fn rename_tag(
        &mut self,
        old_raw: &str,
        new_raw: &str,
        now_unix: i64,
    ) -> Result<usize, BookmarkTagError> {
        let old_tag = Tag::parse(old_raw)?;
        let new_tag = Tag::parse(new_raw)?;
        if old_tag == new_tag {
            return Ok(0);
        }

        let Some(affected_ids) = self.tag_index.remove(&old_tag) else {
            return Ok(0);
        };

        let mut updated_count = 0;
        for id in affected_ids {
            if let Some(bookmark) = self.bookmarks.get_mut(&id) {
                bookmark.tags.remove(&old_tag);
                bookmark.tags.insert(new_tag.clone());
                bookmark.updated_at_unix = now_unix;
                self.tag_index
                    .entry(new_tag.clone())
                    .or_default()
                    .insert(id);
                updated_count += 1;
            }
        }
        Ok(updated_count)
    }

    /// Lists all tags with usage counts, sorted by count descending then tag name ascending.
    pub fn list_tags(&self) -> Vec<TagStats> {
        let mut stats: Vec<TagStats> = self
            .tag_index
            .iter()
            .map(|(tag, ids)| TagStats {
                tag: tag.clone(),
                count: ids.len(),
            })
            .collect();
        stats.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.tag.cmp(&b.tag)));
        stats
    }

    /// Suggests tags matching a prefix (substring or hierarchy), sorted by count descending.
    pub fn suggest_tags(&self, prefix: &str, limit: usize) -> Vec<TagStats> {
        let needle = prefix.trim().trim_start_matches('#').to_ascii_lowercase();
        let mut stats: Vec<TagStats> = self
            .tag_index
            .iter()
            .filter(|(tag, _)| needle.is_empty() || tag.as_str().contains(&needle))
            .map(|(tag, ids)| TagStats {
                tag: tag.clone(),
                count: ids.len(),
            })
            .collect();
        stats.sort_by(|a, b| {
            let a_starts = a.tag.as_str().starts_with(&needle);
            let b_starts = b.tag.as_str().starts_with(&needle);
            b_starts
                .cmp(&a_starts)
                .then_with(|| b.count.cmp(&a.count))
                .then_with(|| a.tag.cmp(&b.tag))
        });
        stats.truncate(limit);
        stats
    }

    /// Parses and executes a bookmark query string.
    pub fn query(&self, raw_query: &str) -> Result<QueryResult, BookmarkTagError> {
        let parsed = parse_query(raw_query)?;
        self.execute_parsed(parsed)
    }

    /// Executes an already-parsed query against the store.
    pub fn execute_parsed(&self, parsed: ParsedQuery) -> Result<QueryResult, BookmarkTagError> {
        let mut matches = Vec::new();
        for bookmark in self.bookmarks.values() {
            let matched = match &parsed.expr {
                Some(expr) => eval_expr(expr, bookmark)?,
                None => EvalHit {
                    matched: true,
                    score: 10,
                    matched_tags: Vec::new(),
                },
            };

            if matched.matched {
                let mut final_score = matched.score;
                if bookmark.pinned {
                    final_score = final_score.saturating_add(15);
                }
                let visit_bonus = (bookmark.visit_count.min(100) / 5) as u32;
                final_score = final_score.saturating_add(visit_bonus);

                matches.push(QueryMatch {
                    bookmark: bookmark.clone(),
                    score: final_score,
                    matched_tags: matched.matched_tags,
                });
            }
        }

        let total_matches = matches.len();
        sort_matches(&mut matches, parsed.sort);
        if let Some(limit) = parsed.limit {
            matches.truncate(limit);
        }

        Ok(QueryResult {
            total_matches,
            matches,
            parsed_query: parsed,
        })
    }

    /// Serializes the store to JSON.
    pub fn to_json(&self) -> Result<String, BookmarkTagError> {
        serde_json::to_string_pretty(self)
            .map_err(|e| BookmarkTagError::Serialization(e.to_string()))
    }

    /// Deserializes a store from JSON and rebuilds the inverted tag index.
    pub fn from_json(json: &str) -> Result<Self, BookmarkTagError> {
        let mut store: Self = serde_json::from_str(json)
            .map_err(|e| BookmarkTagError::Serialization(e.to_string()))?;
        store.rebuild_index();
        Ok(store)
    }

    fn rebuild_index(&mut self) {
        self.tag_index.clear();
        for (id, bookmark) in &self.bookmarks {
            for tag in &bookmark.tags {
                self.tag_index
                    .entry(tag.clone())
                    .or_default()
                    .insert(id.clone());
            }
        }
    }
}

fn sort_matches(matches: &mut [QueryMatch], sort: SortOrder) {
    match sort {
        SortOrder::Relevance => matches.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| b.bookmark.updated_at_unix.cmp(&a.bookmark.updated_at_unix))
                .then_with(|| a.bookmark.title.cmp(&b.bookmark.title))
        }),
        SortOrder::Recent => matches.sort_by(|a, b| {
            b.bookmark
                .updated_at_unix
                .cmp(&a.bookmark.updated_at_unix)
                .then_with(|| b.score.cmp(&a.score))
        }),
        SortOrder::Visits => matches.sort_by(|a, b| {
            b.bookmark
                .visit_count
                .cmp(&a.bookmark.visit_count)
                .then_with(|| b.bookmark.updated_at_unix.cmp(&a.bookmark.updated_at_unix))
        }),
        SortOrder::Title => matches.sort_by(|a, b| {
            a.bookmark
                .title
                .to_ascii_lowercase()
                .cmp(&b.bookmark.title.to_ascii_lowercase())
                .then_with(|| a.bookmark.id.cmp(&b.bookmark.id))
        }),
    }
}

struct EvalHit {
    matched: bool,
    score: u32,
    matched_tags: Vec<Tag>,
}

fn eval_expr(expr: &QueryExpr, bookmark: &TaggedBookmark) -> Result<EvalHit, BookmarkTagError> {
    match expr {
        QueryExpr::And(children) => {
            let mut total_score = 0u32;
            let mut tags = Vec::new();
            for child in children {
                let hit = eval_expr(child, bookmark)?;
                if !hit.matched {
                    return Ok(EvalHit {
                        matched: false,
                        score: 0,
                        matched_tags: Vec::new(),
                    });
                }
                total_score = total_score.saturating_add(hit.score);
                for t in hit.matched_tags {
                    if !tags.contains(&t) {
                        tags.push(t);
                    }
                }
            }
            Ok(EvalHit {
                matched: true,
                score: total_score,
                matched_tags: tags,
            })
        }
        QueryExpr::Or(children) => {
            let mut best_score = 0u32;
            let mut any_matched = false;
            let mut tags = Vec::new();
            for child in children {
                let hit = eval_expr(child, bookmark)?;
                if hit.matched {
                    any_matched = true;
                    best_score = best_score.max(hit.score);
                    for t in hit.matched_tags {
                        if !tags.contains(&t) {
                            tags.push(t);
                        }
                    }
                }
            }
            Ok(EvalHit {
                matched: any_matched,
                score: best_score,
                matched_tags: tags,
            })
        }
        QueryExpr::Not(inner) => {
            let hit = eval_expr(inner, bookmark)?;
            Ok(EvalHit {
                matched: !hit.matched,
                score: if hit.matched { 0 } else { 5 },
                matched_tags: Vec::new(),
            })
        }
        QueryExpr::Predicate(pred) => eval_predicate(pred, bookmark),
    }
}

fn eval_predicate(
    pred: &Predicate,
    bookmark: &TaggedBookmark,
) -> Result<EvalHit, BookmarkTagError> {
    let mut matched_tags = Vec::new();
    let score = match pred {
        Predicate::TagExact(tag) => {
            if bookmark.tags.contains(tag) {
                matched_tags.push(tag.clone());
                50
            } else {
                0
            }
        }
        Predicate::TagPrefix(prefix) => {
            for t in &bookmark.tags {
                if t.matches_prefix(prefix) {
                    matched_tags.push(t.clone());
                }
            }
            if matched_tags.is_empty() {
                0
            } else {
                40
            }
        }
        Predicate::Site(domain) => {
            let target = domain.to_ascii_lowercase();
            let host = bookmark.domain.as_str();
            let is_match = host == target
                || (host.ends_with(&target)
                    && host.len() > target.len()
                    && host.as_bytes()[host.len() - target.len() - 1] == b'.');
            if is_match {
                35
            } else {
                0
            }
        }
        Predicate::Title(needle) => {
            let lower = needle.to_ascii_lowercase();
            if bookmark.title.to_ascii_lowercase().contains(&lower) {
                30
            } else {
                0
            }
        }
        Predicate::Url(needle) => {
            let lower = needle.to_ascii_lowercase();
            if bookmark.url.to_ascii_lowercase().contains(&lower) {
                25
            } else {
                0
            }
        }
        Predicate::Folder(folder) => {
            let lower = folder.to_ascii_lowercase();
            let is_match = bookmark
                .folder
                .as_ref()
                .is_some_and(|f| f.to_ascii_lowercase() == lower);
            if is_match {
                25
            } else {
                0
            }
        }
        Predicate::Pinned(expected) => {
            if bookmark.pinned == *expected {
                20
            } else {
                0
            }
        }
        Predicate::Untagged => {
            if bookmark.tags.is_empty() {
                20
            } else {
                0
            }
        }
        Predicate::AfterUnix(ts) => {
            if bookmark.updated_at_unix >= *ts {
                15
            } else {
                0
            }
        }
        Predicate::BeforeUnix(ts) => {
            if bookmark.updated_at_unix <= *ts {
                15
            } else {
                0
            }
        }
        Predicate::Regex(pattern) => {
            let re = RegexBuilder::new(pattern)
                .size_limit(MAX_REGEX_SIZE_BYTES)
                .build()
                .map_err(|e| BookmarkTagError::InvalidRegex(e.to_string()))?;
            if re.is_match(&bookmark.title) || re.is_match(&bookmark.url) {
                30
            } else {
                0
            }
        }
        Predicate::Text(needle) => {
            let lower = needle.to_ascii_lowercase();
            let mut text_score = 0u32;
            if bookmark.title.to_ascii_lowercase().contains(&lower) {
                text_score = text_score.saturating_add(30);
            }
            if bookmark.url.to_ascii_lowercase().contains(&lower) {
                text_score = text_score.saturating_add(15);
            }
            if bookmark
                .notes
                .as_ref()
                .is_some_and(|n| n.to_ascii_lowercase().contains(&lower))
            {
                text_score = text_score.saturating_add(15);
            }
            for t in &bookmark.tags {
                if t.as_str().contains(&lower) {
                    matched_tags.push(t.clone());
                    text_score = text_score.saturating_add(20);
                }
            }
            text_score
        }
    };

    Ok(EvalHit {
        matched: score > 0,
        score,
        matched_tags,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    LParen,
    RParen,
    And,
    Or,
    Not,
    NegatePrefix,
    Word(String),
    Quoted(String),
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    pos: usize,
}

fn tokenize(input: &str) -> Result<Vec<Token>, BookmarkTagError> {
    let mut tokens = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let ch = input[i..]
            .chars()
            .next()
            .ok_or_else(|| BookmarkTagError::QueryParseError {
                position: i,
                message: "unexpected end of input".to_string(),
            })?;

        if ch.is_whitespace() {
            i += ch.len_utf8();
            continue;
        }

        match ch {
            '(' => {
                tokens.push(Token {
                    kind: TokenKind::LParen,
                    pos: i,
                });
                i += 1;
            }
            ')' => {
                tokens.push(Token {
                    kind: TokenKind::RParen,
                    pos: i,
                });
                i += 1;
            }
            '"' => {
                let start = i;
                i += 1;
                let mut buf = String::new();
                let mut closed = false;
                while i < bytes.len() {
                    let c = input[i..].chars().next().unwrap_or('\0');
                    if c == '\\' {
                        i += 1;
                        if i < bytes.len() {
                            let esc = input[i..].chars().next().unwrap_or('\0');
                            buf.push(esc);
                            i += esc.len_utf8();
                        }
                        continue;
                    }
                    if c == '"' {
                        i += 1;
                        closed = true;
                        break;
                    }
                    buf.push(c);
                    i += c.len_utf8();
                }
                if !closed {
                    return Err(BookmarkTagError::QueryParseError {
                        position: start,
                        message: "unterminated quoted string".to_string(),
                    });
                }
                tokens.push(Token {
                    kind: TokenKind::Quoted(buf),
                    pos: start,
                });
            }
            '-' => {
                let next_is_space_or_end = input[i + 1..]
                    .chars()
                    .next()
                    .is_none_or(|c| c.is_whitespace());
                if next_is_space_or_end {
                    return Err(BookmarkTagError::QueryParseError {
                        position: i,
                        message: "dangling '-' negation operator".to_string(),
                    });
                }
                tokens.push(Token {
                    kind: TokenKind::NegatePrefix,
                    pos: i,
                });
                i += 1;
            }
            _ => {
                let start = i;
                let mut word = String::new();
                while i < bytes.len() {
                    let c = input[i..].chars().next().unwrap_or('\0');
                    if c.is_whitespace() || c == '(' || c == ')' {
                        break;
                    }
                    if c == '"' {
                        // Allow quoted value inside key:"value"
                        i += 1;
                        let mut closed = false;
                        while i < bytes.len() {
                            let qc = input[i..].chars().next().unwrap_or('\0');
                            if qc == '\\' {
                                i += 1;
                                if i < bytes.len() {
                                    let esc = input[i..].chars().next().unwrap_or('\0');
                                    word.push(esc);
                                    i += esc.len_utf8();
                                }
                                continue;
                            }
                            if qc == '"' {
                                i += 1;
                                closed = true;
                                break;
                            }
                            word.push(qc);
                            i += qc.len_utf8();
                        }
                        if !closed {
                            return Err(BookmarkTagError::QueryParseError {
                                position: start,
                                message: "unterminated quoted value".to_string(),
                            });
                        }
                        continue;
                    }
                    word.push(c);
                    i += c.len_utf8();
                }

                let kind = match word.as_str() {
                    "AND" => TokenKind::And,
                    "OR" => TokenKind::Or,
                    "NOT" => TokenKind::Not,
                    _ => TokenKind::Word(word),
                };
                tokens.push(Token { kind, pos: start });
            }
        }
    }

    Ok(tokens)
}

/// Parses a query string into a `ParsedQuery`.
pub fn parse_query(raw: &str) -> Result<ParsedQuery, BookmarkTagError> {
    let tokens = tokenize(raw)?;
    let mut sort = SortOrder::Relevance;
    let mut limit = None;
    let mut expr_tokens = Vec::with_capacity(tokens.len());

    // Extract top-level `sort:` and `limit:` directives before boolean parsing.
    for tok in tokens {
        if let TokenKind::Word(ref w) = tok.kind {
            if let Some(val) = w.strip_prefix("sort:") {
                sort = match val.to_ascii_lowercase().as_str() {
                    "relevance" => SortOrder::Relevance,
                    "recent" => SortOrder::Recent,
                    "visits" => SortOrder::Visits,
                    "title" => SortOrder::Title,
                    other => {
                        return Err(BookmarkTagError::QueryParseError {
                            position: tok.pos,
                            message: format!("unknown sort order '{other}'"),
                        })
                    }
                };
                continue;
            }
            if let Some(val) = w.strip_prefix("limit:") {
                let parsed_limit =
                    val.parse::<usize>()
                        .map_err(|_| BookmarkTagError::QueryParseError {
                            position: tok.pos,
                            message: format!("invalid limit '{val}'"),
                        })?;
                if parsed_limit == 0 {
                    return Err(BookmarkTagError::QueryParseError {
                        position: tok.pos,
                        message: "limit must be greater than zero".to_string(),
                    });
                }
                limit = Some(parsed_limit);
                continue;
            }
        }
        expr_tokens.push(tok);
    }

    let expr = if expr_tokens.is_empty() {
        None
    } else {
        let mut parser = Parser {
            tokens: &expr_tokens,
            cursor: 0,
        };
        let parsed_expr = parser.parse_or(0)?;
        if let Some(leftover) = parser.peek() {
            return Err(BookmarkTagError::QueryParseError {
                position: leftover.pos,
                message: "unexpected trailing token".to_string(),
            });
        }
        Some(parsed_expr)
    };

    Ok(ParsedQuery {
        raw: raw.to_string(),
        expr,
        sort,
        limit,
    })
}

struct Parser<'a> {
    tokens: &'a [Token],
    cursor: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.cursor)
    }

    fn next(&mut self) -> Option<&Token> {
        let tok = self.tokens.get(self.cursor)?;
        self.cursor += 1;
        Some(tok)
    }

    fn parse_or(&mut self, depth: usize) -> Result<QueryExpr, BookmarkTagError> {
        let mut items = vec![self.parse_and(depth)?];
        while matches!(self.peek().map(|t| &t.kind), Some(TokenKind::Or)) {
            let or_pos = self.next().map(|t| t.pos).unwrap_or(0);
            if self.peek().is_none() {
                return Err(BookmarkTagError::QueryParseError {
                    position: or_pos,
                    message: "expected expression after OR".to_string(),
                });
            }
            items.push(self.parse_and(depth)?);
        }
        if items.len() == 1 {
            Ok(items.remove(0))
        } else {
            Ok(QueryExpr::Or(items))
        }
    }

    fn parse_and(&mut self, depth: usize) -> Result<QueryExpr, BookmarkTagError> {
        let mut items = vec![self.parse_unary(depth)?];
        loop {
            match self.peek().map(|t| &t.kind) {
                Some(TokenKind::And) => {
                    let and_pos = self.next().map(|t| t.pos).unwrap_or(0);
                    if self.peek().is_none() {
                        return Err(BookmarkTagError::QueryParseError {
                            position: and_pos,
                            message: "expected expression after AND".to_string(),
                        });
                    }
                    items.push(self.parse_unary(depth)?);
                }
                Some(
                    TokenKind::LParen
                    | TokenKind::Not
                    | TokenKind::NegatePrefix
                    | TokenKind::Word(_)
                    | TokenKind::Quoted(_),
                ) => {
                    // Implicit AND between adjacent terms.
                    items.push(self.parse_unary(depth)?);
                }
                _ => break,
            }
        }
        if items.len() == 1 {
            Ok(items.remove(0))
        } else {
            Ok(QueryExpr::And(items))
        }
    }

    fn parse_unary(&mut self, depth: usize) -> Result<QueryExpr, BookmarkTagError> {
        if let Some(tok) = self.peek() {
            if matches!(tok.kind, TokenKind::Not | TokenKind::NegatePrefix) {
                let pos = tok.pos;
                self.next();
                if self.peek().is_none() {
                    return Err(BookmarkTagError::QueryParseError {
                        position: pos,
                        message: "expected expression after negation".to_string(),
                    });
                }
                let inner = self.parse_unary(depth)?;
                return Ok(QueryExpr::Not(Box::new(inner)));
            }
        }
        self.parse_primary(depth)
    }

    fn parse_primary(&mut self, depth: usize) -> Result<QueryExpr, BookmarkTagError> {
        let tok = self
            .next()
            .ok_or_else(|| BookmarkTagError::QueryParseError {
                position: 0,
                message: "unexpected end of query".to_string(),
            })?
            .clone();

        match tok.kind {
            TokenKind::LParen => {
                if depth + 1 > MAX_QUERY_DEPTH {
                    return Err(BookmarkTagError::QueryTooDeep {
                        max_depth: MAX_QUERY_DEPTH,
                    });
                }
                if matches!(self.peek().map(|t| &t.kind), Some(TokenKind::RParen)) {
                    return Err(BookmarkTagError::QueryParseError {
                        position: tok.pos,
                        message: "empty parenthesized group".to_string(),
                    });
                }
                let inner = self.parse_or(depth + 1)?;
                match self.next() {
                    Some(close) if close.kind == TokenKind::RParen => Ok(inner),
                    _ => Err(BookmarkTagError::QueryParseError {
                        position: tok.pos,
                        message: "unclosed '('".to_string(),
                    }),
                }
            }
            TokenKind::Word(w) => {
                let pred = parse_word_predicate(&w, tok.pos)?;
                Ok(QueryExpr::Predicate(pred))
            }
            TokenKind::Quoted(q) => {
                if q.is_empty() {
                    return Err(BookmarkTagError::QueryParseError {
                        position: tok.pos,
                        message: "empty quoted search term".to_string(),
                    });
                }
                Ok(QueryExpr::Predicate(Predicate::Text(q)))
            }
            TokenKind::RParen | TokenKind::And | TokenKind::Or => {
                Err(BookmarkTagError::QueryParseError {
                    position: tok.pos,
                    message: "unexpected operator or ')'".to_string(),
                })
            }
            TokenKind::Not | TokenKind::NegatePrefix => Err(BookmarkTagError::QueryParseError {
                position: tok.pos,
                message: "unexpected negation".to_string(),
            }),
        }
    }
}

fn parse_word_predicate(word: &str, pos: usize) -> Result<Predicate, BookmarkTagError> {
    if let Some(tag_part) = word.strip_prefix('#') {
        return parse_tag_predicate(tag_part, pos);
    }

    if let Some((key, val)) = word.split_once(':') {
        let key_lower = key.to_ascii_lowercase();
        if val.is_empty() {
            return Err(BookmarkTagError::QueryParseError {
                position: pos,
                message: format!("missing value for '{key}:'"),
            });
        }
        return match key_lower.as_str() {
            "tag" => parse_tag_predicate(val, pos),
            "site" | "domain" => Ok(Predicate::Site(val.to_ascii_lowercase())),
            "title" => Ok(Predicate::Title(val.to_string())),
            "url" => Ok(Predicate::Url(val.to_string())),
            "folder" => Ok(Predicate::Folder(val.to_string())),
            "is" => match val.to_ascii_lowercase().as_str() {
                "pinned" => Ok(Predicate::Pinned(true)),
                "unpinned" => Ok(Predicate::Pinned(false)),
                "untagged" => Ok(Predicate::Untagged),
                other => Err(BookmarkTagError::QueryParseError {
                    position: pos,
                    message: format!("unknown 'is:{other}' state"),
                }),
            },
            "after" => {
                let ts = val
                    .parse::<i64>()
                    .map_err(|_| BookmarkTagError::QueryParseError {
                        position: pos,
                        message: format!("invalid unix timestamp '{val}'"),
                    })?;
                Ok(Predicate::AfterUnix(ts))
            }
            "before" => {
                let ts = val
                    .parse::<i64>()
                    .map_err(|_| BookmarkTagError::QueryParseError {
                        position: pos,
                        message: format!("invalid unix timestamp '{val}'"),
                    })?;
                Ok(Predicate::BeforeUnix(ts))
            }
            "re" | "regex" => {
                // Validate regex eagerly at parse time.
                RegexBuilder::new(val)
                    .size_limit(MAX_REGEX_SIZE_BYTES)
                    .build()
                    .map_err(|e| BookmarkTagError::InvalidRegex(e.to_string()))?;
                Ok(Predicate::Regex(val.to_string()))
            }
            _ => Ok(Predicate::Text(word.to_string())),
        };
    }

    Ok(Predicate::Text(word.to_string()))
}

fn parse_tag_predicate(raw: &str, pos: usize) -> Result<Predicate, BookmarkTagError> {
    if raw.ends_with("/*") || raw.ends_with('/') {
        let clean = raw.trim_end_matches('*').trim_end_matches('/');
        if clean.is_empty() {
            return Err(BookmarkTagError::QueryParseError {
                position: pos,
                message: "empty tag prefix".to_string(),
            });
        }
        // Validate prefix as a tag to ensure legal characters.
        let validated = Tag::parse(clean)?;
        return Ok(Predicate::TagPrefix(validated.as_str().to_string()));
    }
    let tag = Tag::parse(raw)?;
    Ok(Predicate::TagExact(tag))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_store() -> TagStore {
        let mut store = TagStore::new();
        let b1 = TaggedBookmark::new(
            "b1",
            "The Rustonomicon",
            "https://doc.rust-lang.org/nomicon/",
            1000,
        )
        .expect("valid bookmark")
        .with_tags(["dev/rust", "unsafe", "docs"])
        .expect("valid tags")
        .with_folder("programming")
        .with_pinned(true)
        .with_visit_count(42);
        let b2 = TaggedBookmark::new(
            "b2",
            "Tokio Tutorial",
            "https://tokio.rs/tokio/tutorial",
            2000,
        )
        .expect("valid bookmark")
        .with_tags(["dev/rust", "async"])
        .expect("valid tags")
        .with_folder("programming")
        .with_notes("Great guide for async runtime")
        .with_visit_count(15);
        let b3 = TaggedBookmark::new("b3", "Linux Kernel Archives", "https://kernel.org/", 3000)
            .expect("valid bookmark")
            .with_tags(["os/linux", "kernel"])
            .expect("valid tags")
            .with_visit_count(5);
        let b4 = TaggedBookmark::new("b4", "Unsorted Link", "https://example.com/page", 4000)
            .expect("valid bookmark");

        store.upsert_bookmark(b1).expect("upsert b1");
        store.upsert_bookmark(b2).expect("upsert b2");
        store.upsert_bookmark(b3).expect("upsert b3");
        store.upsert_bookmark(b4).expect("upsert b4");
        store
    }

    #[test]
    fn tag_normalization_and_validation() {
        let t1 = Tag::parse("  #Dev/Rust  ").expect("valid tag");
        assert_eq!(t1.as_str(), "dev/rust");
        assert!(t1.matches_prefix("dev"));
        assert!(t1.matches_prefix("dev/*"));
        assert!(!t1.matches_prefix("de"));

        let t2 = Tag::parse("My Cool Tag").expect("spaces normalized");
        assert_eq!(t2.as_str(), "my-cool-tag");

        assert!(Tag::parse("").is_err());
        assert!(Tag::parse("###").is_err());
        assert!(Tag::parse("/leading").is_err());
        assert!(Tag::parse("a//b").is_err());
        assert!(Tag::parse("bad$char").is_err());
        let long = "a".repeat(MAX_TAG_LEN + 1);
        assert!(Tag::parse(&long).is_err());
    }

    #[test]
    fn boolean_query_and_predicates() {
        let store = sample_store();

        // Prefix tag + negation
        let res = store.query("tag:dev/* -tag:unsafe").expect("query ok");
        assert_eq!(res.total_matches, 1);
        assert_eq!(res.matches[0].bookmark.id, "b2");

        // OR + parentheses + site subdomain
        let res = store
            .query("(#kernel OR site:rust-lang.org) sort:title")
            .expect("query ok");
        assert_eq!(res.total_matches, 2);
        assert_eq!(res.matches[0].bookmark.id, "b3");
        assert_eq!(res.matches[1].bookmark.id, "b1");

        // State, folder, and quoted phrase in notes
        let res = store
            .query("folder:programming \"async runtime\"")
            .expect("query ok");
        assert_eq!(res.total_matches, 1);
        assert_eq!(res.matches[0].bookmark.id, "b2");

        // Untagged + limit + recent sort
        let res = store.query("is:untagged").expect("query ok");
        assert_eq!(res.total_matches, 1);
        assert_eq!(res.matches[0].bookmark.id, "b4");

        // Regex predicate
        let res = store.query(r"re:^https://tokio\.rs/").expect("regex ok");
        assert_eq!(res.total_matches, 1);
        assert_eq!(res.matches[0].bookmark.id, "b2");
    }

    #[test]
    fn store_mutations_and_json_roundtrip() {
        let mut store = sample_store();

        assert!(store.add_tag("b4", "reading", 4100).expect("add tag"));
        assert!(!store.add_tag("b4", "READING", 4100).expect("duplicate"));

        let renamed = store
            .rename_tag("dev/rust", "lang/rust", 5000)
            .expect("rename ok");
        assert_eq!(renamed, 2);
        assert_eq!(store.query("#dev/rust").expect("ok").total_matches, 0);
        assert_eq!(store.query("#lang/rust").expect("ok").total_matches, 2);

        let json = store.to_json().expect("serialize");
        let restored = TagStore::from_json(&json).expect("deserialize");
        assert_eq!(restored.len(), 4);
        assert_eq!(restored.query("#lang/rust").expect("ok").total_matches, 2);

        let suggestions = restored.suggest_tags("lang", 5);
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].tag.as_str(), "lang/rust");
        assert_eq!(suggestions[0].count, 2);
    }

    #[test]
    fn query_error_and_edge_cases() {
        let store = sample_store();

        assert!(matches!(
            store.query("(tag:rust"),
            Err(BookmarkTagError::QueryParseError { .. })
        ));
        assert!(matches!(
            store.query("tag:rust OR"),
            Err(BookmarkTagError::QueryParseError { .. })
        ));
        assert!(matches!(
            store.query("limit:0"),
            Err(BookmarkTagError::QueryParseError { .. })
        ));
        assert!(matches!(
            store.query("re:[unclosed"),
            Err(BookmarkTagError::InvalidRegex(_))
        ));

        let deep = format!(
            "{}#rust{}",
            "(".repeat(MAX_QUERY_DEPTH + 1),
            ")".repeat(MAX_QUERY_DEPTH + 1)
        );
        assert!(matches!(
            store.query(&deep),
            Err(BookmarkTagError::QueryTooDeep { .. })
        ));
    }
}
