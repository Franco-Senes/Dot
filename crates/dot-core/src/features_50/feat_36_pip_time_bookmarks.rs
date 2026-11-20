//! Picture-in-Picture (PiP) time bookmarks for saving, organizing, and jumping
//! to timestamped moments in video and audio streams.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

/// Maximum length in bytes for a bookmark title.
pub const MAX_TITLE_LEN: usize = 160;
/// Maximum length in bytes for an optional bookmark note.
pub const MAX_NOTE_LEN: usize = 1024;
/// Maximum length in bytes for a single normalized tag.
pub const MAX_TAG_LEN: usize = 32;
/// Maximum number of tags allowed on a single bookmark.
pub const MAX_TAGS_PER_BOOKMARK: usize = 12;
/// Minimum supported PiP playback rate multiplier.
pub const MIN_PLAYBACK_RATE: f32 = 0.25;
/// Maximum supported PiP playback rate multiplier.
pub const MAX_PLAYBACK_RATE: f32 = 4.0;
/// Default maximum number of bookmarks held in a store.
pub const DEFAULT_MAX_BOOKMARKS: usize = 2_000;
/// Default tolerance in milliseconds when detecting near-duplicate timestamps.
pub const DEFAULT_DEDUP_TOLERANCE_MS: u64 = 1_500;

/// Errors returned by PiP time bookmark validation, storage, and serialization.
#[derive(Debug, Error, PartialEq)]
pub enum PipBookmarkError {
    #[error("bookmark title cannot be empty")]
    EmptyTitle,

    #[error("bookmark title exceeds {max} bytes (got {len})")]
    TitleTooLong { len: usize, max: usize },

    #[error("bookmark note exceeds {max} bytes (got {len})")]
    NoteTooLong { len: usize, max: usize },

    #[error("invalid media URL: {0}")]
    InvalidUrl(String),

    #[error("unsupported media URL scheme: {0}")]
    UnsupportedScheme(String),

    #[error("timestamp {timestamp_ms}ms exceeds media duration {duration_ms:?}ms")]
    InvalidTimestampMs {
        timestamp_ms: u64,
        duration_ms: Option<u64>,
    },

    #[error("invalid loop range: start ({start_ms}ms) must be less than end ({end_ms}ms)")]
    InvalidLoopRange { start_ms: u64, end_ms: u64 },

    #[error("playback rate {0} is out of range ({MIN_PLAYBACK_RATE}..={MAX_PLAYBACK_RATE})")]
    InvalidPlaybackRate(f32),

    #[error("too many tags: {count} exceeds maximum of {max}")]
    TooManyTags { count: usize, max: usize },

    #[error("invalid tag: {0}")]
    InvalidTag(String),

    #[error("duplicate bookmark near {timestamp_ms}ms for media {media_key}")]
    DuplicateBookmark {
        media_key: String,
        timestamp_ms: u64,
    },

    #[error("bookmark not found: {0}")]
    NotFound(String),

    #[error("bookmark store capacity exceeded (max {max})")]
    CapacityExceeded { max: usize },

    #[error("serialization error: {0}")]
    Serialization(String),
}

impl From<serde_json::Error> for PipBookmarkError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}

/// Optional A-B loop segment associated with a PiP bookmark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    pub start_ms: u64,
    pub end_ms: u64,
}

impl TimeRange {
    /// Creates and validates a non-empty `[start_ms, end_ms]` range.
    pub fn new(start_ms: u64, end_ms: u64) -> Result<Self, PipBookmarkError> {
        if start_ms >= end_ms {
            return Err(PipBookmarkError::InvalidLoopRange { start_ms, end_ms });
        }
        Ok(Self { start_ms, end_ms })
    }

    /// Returns the duration of the loop range in milliseconds.
    pub fn duration_ms(&self) -> u64 {
        self.end_ms.saturating_sub(self.start_ms)
    }
}

/// A saved timestamp bookmark for a Picture-in-Picture media item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipTimeBookmark {
    pub id: String,
    pub media_url: String,
    pub media_key: String,
    pub timestamp_ms: u64,
    pub duration_ms: Option<u64>,
    pub title: String,
    pub note: Option<String>,
    pub tags: Vec<String>,
    pub playback_rate: f32,
    pub loop_range: Option<TimeRange>,
    pub pinned: bool,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

impl PipTimeBookmark {
    /// Formats `timestamp_ms` as `MM:SS` or `HH:MM:SS`.
    pub fn formatted_timestamp(&self) -> String {
        format_timestamp_hms(self.timestamp_ms)
    }

    /// Builds a seekable URL targeting this bookmark's timestamp.
    pub fn seek_url(&self) -> Result<String, PipBookmarkError> {
        build_seek_url(&self.media_url, self.timestamp_ms)
    }

    /// Validates all fields of the bookmark.
    pub fn validate(&self) -> Result<(), PipBookmarkError> {
        let _ = canonicalize_media_url(&self.media_url)?;
        validate_title(&self.title)?;
        if let Some(note) = &self.note {
            validate_note(note)?;
        }
        validate_timestamp(self.timestamp_ms, self.duration_ms)?;
        validate_playback_rate(self.playback_rate)?;
        if let Some(range) = self.loop_range {
            let _ = TimeRange::new(range.start_ms, range.end_ms)?;
            if let Some(dur) = self.duration_ms {
                if range.end_ms > dur {
                    return Err(PipBookmarkError::InvalidTimestampMs {
                        timestamp_ms: range.end_ms,
                        duration_ms: Some(dur),
                    });
                }
            }
        }
        let _ = normalize_tags(&self.tags)?;
        Ok(())
    }
}

/// Input payload for creating a new PiP time bookmark.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewPipBookmark {
    pub media_url: String,
    pub timestamp_ms: u64,
    pub duration_ms: Option<u64>,
    pub title: String,
    pub note: Option<String>,
    pub tags: Vec<String>,
    pub playback_rate: Option<f32>,
    pub loop_range: Option<TimeRange>,
    pub pinned: bool,
}

impl NewPipBookmark {
    /// Creates a minimal bookmark input with sensible defaults.
    pub fn new(media_url: impl Into<String>, timestamp_ms: u64, title: impl Into<String>) -> Self {
        Self {
            media_url: media_url.into(),
            timestamp_ms,
            duration_ms: None,
            title: title.into(),
            note: None,
            tags: Vec::new(),
            playback_rate: None,
            loop_range: None,
            pinned: false,
        }
    }
}

/// Partial update payload for an existing PiP time bookmark.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PipBookmarkPatch {
    pub timestamp_ms: Option<u64>,
    pub duration_ms: Option<Option<u64>>,
    pub title: Option<String>,
    pub note: Option<Option<String>>,
    pub tags: Option<Vec<String>>,
    pub playback_rate: Option<f32>,
    pub loop_range: Option<Option<TimeRange>>,
    pub pinned: Option<bool>,
}

/// Filter criteria for querying stored PiP time bookmarks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PipBookmarkQuery {
    pub media_key: Option<String>,
    pub tag: Option<String>,
    pub text_query: Option<String>,
    pub pinned_only: bool,
    pub min_timestamp_ms: Option<u64>,
    pub max_timestamp_ms: Option<u64>,
}

/// Summary of a JSON import operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportReport {
    pub imported: usize,
    pub skipped_duplicates: usize,
    pub rejected: usize,
}

/// In-memory store and query engine for PiP time bookmarks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipBookmarkStore {
    bookmarks: Vec<PipTimeBookmark>,
    max_bookmarks: usize,
    dedup_tolerance_ms: u64,
    next_seq: u64,
}

impl Default for PipBookmarkStore {
    fn default() -> Self {
        Self::new()
    }
}

impl PipBookmarkStore {
    /// Creates a new store with default capacity and duplicate tolerance.
    pub fn new() -> Self {
        Self {
            bookmarks: Vec::new(),
            max_bookmarks: DEFAULT_MAX_BOOKMARKS,
            dedup_tolerance_ms: DEFAULT_DEDUP_TOLERANCE_MS,
            next_seq: 1,
        }
    }

    /// Creates a store with custom capacity and duplicate timestamp tolerance.
    pub fn with_limits(max_bookmarks: usize, dedup_tolerance_ms: u64) -> Self {
        Self {
            bookmarks: Vec::new(),
            max_bookmarks: max_bookmarks.max(1),
            dedup_tolerance_ms,
            next_seq: 1,
        }
    }

    /// Returns the total number of stored bookmarks.
    pub fn len(&self) -> usize {
        self.bookmarks.len()
    }

    /// Returns `true` if no bookmarks are stored.
    pub fn is_empty(&self) -> bool {
        self.bookmarks.is_empty()
    }

    /// Validates and inserts a new PiP time bookmark.
    pub fn add(
        &mut self,
        input: NewPipBookmark,
        now_ms: u64,
    ) -> Result<&PipTimeBookmark, PipBookmarkError> {
        if self.bookmarks.len() >= self.max_bookmarks {
            return Err(PipBookmarkError::CapacityExceeded {
                max: self.max_bookmarks,
            });
        }

        let (media_url, media_key) = canonicalize_media_url(&input.media_url)?;
        let title = validate_title(&input.title)?;
        let note = match input.note {
            Some(ref raw) => Some(validate_note(raw)?),
            None => None,
        };
        validate_timestamp(input.timestamp_ms, input.duration_ms)?;

        let playback_rate = input.playback_rate.unwrap_or(1.0);
        validate_playback_rate(playback_rate)?;

        if let Some(range) = input.loop_range {
            let _ = TimeRange::new(range.start_ms, range.end_ms)?;
            if let Some(dur) = input.duration_ms {
                if range.end_ms > dur {
                    return Err(PipBookmarkError::InvalidTimestampMs {
                        timestamp_ms: range.end_ms,
                        duration_ms: Some(dur),
                    });
                }
            }
        }

        let tags = normalize_tags(&input.tags)?;

        if self.has_near_duplicate(&media_key, input.timestamp_ms, None) {
            return Err(PipBookmarkError::DuplicateBookmark {
                media_key,
                timestamp_ms: input.timestamp_ms,
            });
        }

        let id = self.allocate_id(&media_key, input.timestamp_ms, now_ms);
        let bookmark = PipTimeBookmark {
            id,
            media_url,
            media_key,
            timestamp_ms: input.timestamp_ms,
            duration_ms: input.duration_ms,
            title,
            note,
            tags,
            playback_rate,
            loop_range: input.loop_range,
            pinned: input.pinned,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
        };

        self.bookmarks.push(bookmark);
        let idx = self.bookmarks.len() - 1;
        Ok(&self.bookmarks[idx])
    }

    /// Retrieves a bookmark by its unique ID.
    pub fn get(&self, id: &str) -> Option<&PipTimeBookmark> {
        self.bookmarks.iter().find(|b| b.id == id)
    }

    /// Applies a partial update to an existing bookmark.
    pub fn update(
        &mut self,
        id: &str,
        patch: PipBookmarkPatch,
        now_ms: u64,
    ) -> Result<&PipTimeBookmark, PipBookmarkError> {
        let idx = self
            .bookmarks
            .iter()
            .position(|b| b.id == id)
            .ok_or_else(|| PipBookmarkError::NotFound(id.to_string()))?;

        let mut candidate = self.bookmarks[idx].clone();

        if let Some(new_dur) = patch.duration_ms {
            candidate.duration_ms = new_dur;
        }
        if let Some(new_ts) = patch.timestamp_ms {
            candidate.timestamp_ms = new_ts;
        }
        if let Some(new_title) = patch.title {
            candidate.title = validate_title(&new_title)?;
        }
        if let Some(new_note) = patch.note {
            candidate.note = match new_note {
                Some(ref raw) => Some(validate_note(raw)?),
                None => None,
            };
        }
        if let Some(new_tags) = patch.tags {
            candidate.tags = normalize_tags(&new_tags)?;
        }
        if let Some(new_rate) = patch.playback_rate {
            validate_playback_rate(new_rate)?;
            candidate.playback_rate = new_rate;
        }
        if let Some(new_range) = patch.loop_range {
            candidate.loop_range = new_range;
        }
        if let Some(pinned) = patch.pinned {
            candidate.pinned = pinned;
        }

        validate_timestamp(candidate.timestamp_ms, candidate.duration_ms)?;
        if let Some(range) = candidate.loop_range {
            let _ = TimeRange::new(range.start_ms, range.end_ms)?;
            if let Some(dur) = candidate.duration_ms {
                if range.end_ms > dur {
                    return Err(PipBookmarkError::InvalidTimestampMs {
                        timestamp_ms: range.end_ms,
                        duration_ms: Some(dur),
                    });
                }
            }
        }

        if self.has_near_duplicate(&candidate.media_key, candidate.timestamp_ms, Some(id)) {
            return Err(PipBookmarkError::DuplicateBookmark {
                media_key: candidate.media_key,
                timestamp_ms: candidate.timestamp_ms,
            });
        }

        candidate.updated_at_ms = now_ms;
        self.bookmarks[idx] = candidate;
        Ok(&self.bookmarks[idx])
    }

    /// Removes and returns a bookmark by ID.
    pub fn remove(&mut self, id: &str) -> Result<PipTimeBookmark, PipBookmarkError> {
        let idx = self
            .bookmarks
            .iter()
            .position(|b| b.id == id)
            .ok_or_else(|| PipBookmarkError::NotFound(id.to_string()))?;
        Ok(self.bookmarks.remove(idx))
    }

    /// Lists all bookmarks for a given media URL, sorted by ascending timestamp.
    pub fn for_media(&self, media_url: &str) -> Result<Vec<&PipTimeBookmark>, PipBookmarkError> {
        let (_, media_key) = canonicalize_media_url(media_url)?;
        let mut results: Vec<&PipTimeBookmark> = self
            .bookmarks
            .iter()
            .filter(|b| b.media_key == media_key)
            .collect();
        results.sort_by_key(|b| (b.timestamp_ms, b.created_at_ms));
        Ok(results)
    }

    /// Finds the closest bookmark strictly before `position_ms` for quick PiP jump-back.
    pub fn nearest_before(
        &self,
        media_url: &str,
        position_ms: u64,
    ) -> Result<Option<&PipTimeBookmark>, PipBookmarkError> {
        let items = self.for_media(media_url)?;
        Ok(items
            .into_iter()
            .rev()
            .find(|b| b.timestamp_ms < position_ms))
    }

    /// Finds the closest bookmark strictly after `position_ms` for quick PiP jump-forward.
    pub fn nearest_after(
        &self,
        media_url: &str,
        position_ms: u64,
    ) -> Result<Option<&PipTimeBookmark>, PipBookmarkError> {
        let items = self.for_media(media_url)?;
        Ok(items.into_iter().find(|b| b.timestamp_ms > position_ms))
    }

    /// Searches bookmarks across all media items matching the given query filters.
    pub fn search(&self, query: &PipBookmarkQuery) -> Vec<&PipTimeBookmark> {
        let text_needle = query
            .text_query
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_lowercase);

        let tag_needle = query
            .tag
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_lowercase);

        let mut matches: Vec<&PipTimeBookmark> = self
            .bookmarks
            .iter()
            .filter(|b| {
                if query.pinned_only && !b.pinned {
                    return false;
                }
                if let Some(ref key) = query.media_key {
                    if &b.media_key != key {
                        return false;
                    }
                }
                if let Some(min_ts) = query.min_timestamp_ms {
                    if b.timestamp_ms < min_ts {
                        return false;
                    }
                }
                if let Some(max_ts) = query.max_timestamp_ms {
                    if b.timestamp_ms > max_ts {
                        return false;
                    }
                }
                if let Some(ref tag) = tag_needle {
                    if !b.tags.iter().any(|t| t == tag) {
                        return false;
                    }
                }
                if let Some(ref needle) = text_needle {
                    let in_title = b.title.to_lowercase().contains(needle);
                    let in_note = b
                        .note
                        .as_deref()
                        .map(|n| n.to_lowercase().contains(needle))
                        .unwrap_or(false);
                    let in_tags = b.tags.iter().any(|t| t.contains(needle));
                    if !in_title && !in_note && !in_tags {
                        return false;
                    }
                }
                true
            })
            .collect();

        matches.sort_by(|a, b| {
            b.pinned
                .cmp(&a.pinned)
                .then_with(|| a.media_key.cmp(&b.media_key))
                .then_with(|| a.timestamp_ms.cmp(&b.timestamp_ms))
        });
        matches
    }

    /// Exports all bookmarks as pretty-printed JSON.
    pub fn export_json(&self) -> Result<String, PipBookmarkError> {
        Ok(serde_json::to_string_pretty(&self.bookmarks)?)
    }

    /// Imports bookmarks from a JSON array, either merging or replacing existing entries.
    pub fn import_json(
        &mut self,
        json: &str,
        merge: bool,
    ) -> Result<ImportReport, PipBookmarkError> {
        let incoming: Vec<PipTimeBookmark> = serde_json::from_str(json)?;
        if !merge {
            self.bookmarks.clear();
        }

        let mut report = ImportReport {
            imported: 0,
            skipped_duplicates: 0,
            rejected: 0,
        };

        for mut item in incoming {
            if self.bookmarks.len() >= self.max_bookmarks {
                report.rejected += 1;
                continue;
            }
            if item.validate().is_err() {
                report.rejected += 1;
                continue;
            }
            let Ok((canonical_url, canonical_key)) = canonicalize_media_url(&item.media_url) else {
                report.rejected += 1;
                continue;
            };
            item.media_url = canonical_url;
            item.media_key = canonical_key;
            item.tags = match normalize_tags(&item.tags) {
                Ok(tags) => tags,
                Err(_) => {
                    report.rejected += 1;
                    continue;
                }
            };

            let id_exists = self.bookmarks.iter().any(|b| b.id == item.id);
            let near_dup = self.has_near_duplicate(&item.media_key, item.timestamp_ms, None);
            if id_exists || near_dup {
                report.skipped_duplicates += 1;
                continue;
            }

            if item.id.trim().is_empty() {
                item.id = self.allocate_id(&item.media_key, item.timestamp_ms, item.created_at_ms);
            }
            self.bookmarks.push(item);
            report.imported += 1;
        }

        Ok(report)
    }

    /// Formats all bookmarks for a media URL as a plain-text chapter list (`HH:MM:SS - Title`).
    pub fn export_chapters_text(&self, media_url: &str) -> Result<String, PipBookmarkError> {
        let items = self.for_media(media_url)?;
        let lines: Vec<String> = items
            .into_iter()
            .map(|b| format!("{} - {}", b.formatted_timestamp(), b.title))
            .collect();
        Ok(lines.join("\n"))
    }

    fn has_near_duplicate(
        &self,
        media_key: &str,
        timestamp_ms: u64,
        exclude_id: Option<&str>,
    ) -> bool {
        self.bookmarks.iter().any(|b| {
            if let Some(ex) = exclude_id {
                if b.id == ex {
                    return false;
                }
            }
            b.media_key == media_key
                && b.timestamp_ms.abs_diff(timestamp_ms) <= self.dedup_tolerance_ms
        })
    }

    fn allocate_id(&mut self, media_key: &str, timestamp_ms: u64, now_ms: u64) -> String {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        let hash = fnv1a_64(media_key.as_bytes()) ^ timestamp_ms ^ now_ms ^ seq;
        format!("pipbm_{seq:04x}_{hash:08x}")
    }
}

/// Canonicalizes a media URL and returns `(cleaned_url, media_key)`.
///
/// Strips transient seek parameters (`t`, `time_continue`, `start`) and tracking
/// parameters (`utm_*`, `fbclid`, `gclid`) from `media_key` so bookmarks for the
/// same underlying media stream group together reliably.
pub fn canonicalize_media_url(raw_url: &str) -> Result<(String, String), PipBookmarkError> {
    let trimmed = raw_url.trim();
    let parsed = Url::parse(trimmed).map_err(|e| PipBookmarkError::InvalidUrl(e.to_string()))?;

    let scheme = parsed.scheme().to_ascii_lowercase();
    match scheme.as_str() {
        "http" | "https" | "file" | "blob" => {}
        other => return Err(PipBookmarkError::UnsupportedScheme(other.to_string())),
    }

    if matches!(scheme.as_str(), "http" | "https") && parsed.host_str().is_none() {
        return Err(PipBookmarkError::InvalidUrl(
            "missing host for http/https URL".to_string(),
        ));
    }

    let mut key_url = parsed.clone();
    key_url.set_fragment(None);

    let filtered_pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .filter(|(k, _)| !is_transient_or_tracking_param(k))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();

    if filtered_pairs.is_empty() {
        key_url.set_query(None);
    } else {
        let mut serializer = key_url.query_pairs_mut();
        serializer.clear();
        for (k, v) in &filtered_pairs {
            serializer.append_pair(k, v);
        }
    }

    Ok((parsed.to_string(), key_url.to_string()))
}

/// Builds a seekable URL with `t=<seconds>` for resuming playback at `timestamp_ms`.
pub fn build_seek_url(raw_url: &str, timestamp_ms: u64) -> Result<String, PipBookmarkError> {
    let (cleaned, _) = canonicalize_media_url(raw_url)?;
    let mut parsed =
        Url::parse(&cleaned).map_err(|e| PipBookmarkError::InvalidUrl(e.to_string()))?;

    let secs = timestamp_ms / 1_000;
    if parsed.scheme() == "file" {
        parsed.set_fragment(Some(&format!("t={secs}")));
        return Ok(parsed.to_string());
    }

    let existing_pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .filter(|(k, _)| k != "t" && k != "time_continue")
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();

    {
        let mut qp = parsed.query_pairs_mut();
        qp.clear();
        for (k, v) in &existing_pairs {
            qp.append_pair(k, v);
        }
        qp.append_pair("t", &secs.to_string());
    }

    Ok(parsed.to_string())
}

/// Formats a millisecond timestamp as `MM:SS` or `HH:MM:SS` when >= 1 hour.
pub fn format_timestamp_hms(timestamp_ms: u64) -> String {
    let total_secs = timestamp_ms / 1_000;
    let hours = total_secs / 3_600;
    let minutes = (total_secs % 3_600) / 60;
    let seconds = total_secs % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

/// Parses `MM:SS` or `HH:MM:SS` into milliseconds.
pub fn parse_timestamp_hms(input: &str) -> Option<u64> {
    let parts: Vec<&str> = input.trim().split(':').collect();
    match parts.as_slice() {
        [mm, ss] => {
            let m: u64 = mm.parse().ok()?;
            let s: u64 = ss.parse().ok()?;
            if s >= 60 {
                return None;
            }
            Some((m.checked_mul(60)?.checked_add(s)?).checked_mul(1_000)?)
        }
        [hh, mm, ss] => {
            let h: u64 = hh.parse().ok()?;
            let m: u64 = mm.parse().ok()?;
            let s: u64 = ss.parse().ok()?;
            if m >= 60 || s >= 60 {
                return None;
            }
            let total_secs = h
                .checked_mul(3_600)?
                .checked_add(m.checked_mul(60)?)?
                .checked_add(s)?;
            total_secs.checked_mul(1_000)
        }
        _ => None,
    }
}

fn is_transient_or_tracking_param(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "t" | "start" | "time_continue" | "fbclid" | "gclid" | "mc_cid" | "mc_eid"
    ) || lower.starts_with("utm_")
}

fn validate_title(raw: &str) -> Result<String, PipBookmarkError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(PipBookmarkError::EmptyTitle);
    }
    if trimmed.len() > MAX_TITLE_LEN {
        return Err(PipBookmarkError::TitleTooLong {
            len: trimmed.len(),
            max: MAX_TITLE_LEN,
        });
    }
    Ok(trimmed.to_string())
}

fn validate_note(raw: &str) -> Result<String, PipBookmarkError> {
    let trimmed = raw.trim();
    if trimmed.len() > MAX_NOTE_LEN {
        return Err(PipBookmarkError::NoteTooLong {
            len: trimmed.len(),
            max: MAX_NOTE_LEN,
        });
    }
    Ok(trimmed.to_string())
}

fn validate_timestamp(timestamp_ms: u64, duration_ms: Option<u64>) -> Result<(), PipBookmarkError> {
    if let Some(dur) = duration_ms {
        if timestamp_ms > dur {
            return Err(PipBookmarkError::InvalidTimestampMs {
                timestamp_ms,
                duration_ms: Some(dur),
            });
        }
    }
    Ok(())
}

fn validate_playback_rate(rate: f32) -> Result<(), PipBookmarkError> {
    if !rate.is_finite() || !(MIN_PLAYBACK_RATE..=MAX_PLAYBACK_RATE).contains(&rate) {
        return Err(PipBookmarkError::InvalidPlaybackRate(rate));
    }
    Ok(())
}

fn normalize_tags(tags: &[String]) -> Result<Vec<String>, PipBookmarkError> {
    let mut unique = BTreeSet::new();
    for raw in tags {
        let normalized = raw.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            continue;
        }
        if normalized.len() > MAX_TAG_LEN {
            return Err(PipBookmarkError::InvalidTag(format!(
                "tag exceeds {MAX_TAG_LEN} bytes"
            )));
        }
        if normalized.contains(char::is_whitespace) {
            return Err(PipBookmarkError::InvalidTag(format!(
                "tag '{normalized}' cannot contain whitespace"
            )));
        }
        unique.insert(normalized);
    }
    if unique.len() > MAX_TAGS_PER_BOOKMARK {
        return Err(PipBookmarkError::TooManyTags {
            count: unique.len(),
            max: MAX_TAGS_PER_BOOKMARK,
        });
    }
    Ok(unique.into_iter().collect())
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_add_query_jump_and_chapters() {
        let mut store = PipBookmarkStore::new();

        let mut first = NewPipBookmark::new(
            "https://video.example.com/watch?v=abc123&t=42&utm_source=share",
            125_000,
            "  Architecture overview  ",
        );
        first.duration_ms = Some(600_000);
        first.tags = vec!["Rust".into(), "  pip ".into(), "rust".into()];
        first.playback_rate = Some(1.25);
        first.pinned = true;

        let b1 = store.add(first, 1_000).expect("add first bookmark");
        assert_eq!(b1.title, "Architecture overview");
        assert_eq!(b1.media_key, "https://video.example.com/watch?v=abc123");
        assert_eq!(b1.tags, vec!["pip", "rust"]);
        assert_eq!(b1.formatted_timestamp(), "02:05");

        let second = NewPipBookmark::new(
            "https://video.example.com/watch?v=abc123",
            3_723_000,
            "Deep dive",
        );
        let b2 = store.add(second, 2_000).expect("add second bookmark");
        assert_eq!(b2.formatted_timestamp(), "01:02:03");

        let list = store
            .for_media("https://video.example.com/watch?v=abc123&t=999")
            .expect("list media bookmarks");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].timestamp_ms, 125_000);
        assert_eq!(list[1].timestamp_ms, 3_723_000);

        let prev = store
            .nearest_before("https://video.example.com/watch?v=abc123", 200_000)
            .expect("nearest_before")
            .expect("should find first bookmark");
        assert_eq!(prev.title, "Architecture overview");

        let next = store
            .nearest_after("https://video.example.com/watch?v=abc123", 200_000)
            .expect("nearest_after")
            .expect("should find second bookmark");
        assert_eq!(next.title, "Deep dive");

        let chapters = store
            .export_chapters_text("https://video.example.com/watch?v=abc123")
            .expect("export chapters");
        assert_eq!(
            chapters,
            "02:05 - Architecture overview\n01:02:03 - Deep dive"
        );
    }

    #[test]
    fn validation_and_duplicate_detection_edge_cases() {
        let mut store = PipBookmarkStore::with_limits(10, 1_000);

        let empty_title = NewPipBookmark::new("https://example.com/v.mp4", 1_000, "   ");
        assert_eq!(
            store.add(empty_title, 100),
            Err(PipBookmarkError::EmptyTitle)
        );

        let bad_scheme = NewPipBookmark::new("javascript:alert(1)", 1_000, "XSS");
        assert_eq!(
            store.add(bad_scheme, 100),
            Err(PipBookmarkError::UnsupportedScheme("javascript".into()))
        );

        let mut past_duration = NewPipBookmark::new("https://example.com/v.mp4", 50_000, "End");
        past_duration.duration_ms = Some(30_000);
        assert_eq!(
            store.add(past_duration, 100),
            Err(PipBookmarkError::InvalidTimestampMs {
                timestamp_ms: 50_000,
                duration_ms: Some(30_000),
            })
        );

        let mut bad_rate = NewPipBookmark::new("https://example.com/v.mp4", 5_000, "Fast");
        bad_rate.playback_rate = Some(8.0);
        assert_eq!(
            store.add(bad_rate, 100),
            Err(PipBookmarkError::InvalidPlaybackRate(8.0))
        );

        assert_eq!(
            TimeRange::new(10_000, 10_000),
            Err(PipBookmarkError::InvalidLoopRange {
                start_ms: 10_000,
                end_ms: 10_000,
            })
        );

        let valid = NewPipBookmark::new("https://example.com/v.mp4", 10_000, "Hook");
        store.add(valid, 100).expect("insert valid bookmark");

        let near_dup = NewPipBookmark::new("https://example.com/v.mp4", 10_500, "Near dup");
        assert_eq!(
            store.add(near_dup, 200),
            Err(PipBookmarkError::DuplicateBookmark {
                media_key: "https://example.com/v.mp4".into(),
                timestamp_ms: 10_500,
            })
        );
    }

    #[test]
    fn update_search_and_json_roundtrip() {
        let mut store = PipBookmarkStore::new();
        let mut input =
            NewPipBookmark::new("file:///home/franco/Videos/talk.webm", 45_000, "Intro");
        input.loop_range = Some(TimeRange::new(40_000, 55_000).expect("valid range"));
        let created_id = store.add(input, 1_000).expect("add").id.clone();

        let patch = PipBookmarkPatch {
            title: Some("Keynote Intro".into()),
            note: Some(Some("Check benchmark slide".into())),
            pinned: Some(true),
            tags: Some(vec!["benchmark".into()]),
            ..PipBookmarkPatch::default()
        };
        let updated = store.update(&created_id, patch, 2_000).expect("update");
        assert_eq!(updated.title, "Keynote Intro");
        assert!(updated.pinned);
        assert_eq!(
            updated.seek_url().expect("seek url"),
            "file:///home/franco/Videos/talk.webm#t=45"
        );

        let hits = store.search(&PipBookmarkQuery {
            pinned_only: true,
            text_query: Some("benchmark".into()),
            ..PipBookmarkQuery::default()
        });
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, created_id);

        let exported = store.export_json().expect("export json");

        let mut restored = PipBookmarkStore::new();
        let report_first = restored
            .import_json(&exported, true)
            .expect("import into empty");
        assert_eq!(
            report_first,
            ImportReport {
                imported: 1,
                skipped_duplicates: 0,
                rejected: 0,
            }
        );

        let report_dup = restored
            .import_json(&exported, true)
            .expect("import duplicate");
        assert_eq!(
            report_dup,
            ImportReport {
                imported: 0,
                skipped_duplicates: 1,
                rejected: 0,
            }
        );

        assert_eq!(parse_timestamp_hms("01:02:03"), Some(3_723_000));
        assert_eq!(parse_timestamp_hms("12:75"), None);
    }
}
