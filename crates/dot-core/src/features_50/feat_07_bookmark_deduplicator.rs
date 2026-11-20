//! Bookmark deduplicator module for Search Linux.
//!
//! Provides URL canonicalization, tracking-parameter stripping, SHA-256
//! fingerprinting, configurable duplicate detection, and deterministic
//! metadata merging for bookmark collections.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use ring::digest;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

/// Default query parameters treated as tracking parameters when stripping is enabled.
const DEFAULT_TRACKING_PARAMS: &[&str] = &[
    "fbclid",
    "gclid",
    "dclid",
    "msclkid",
    "mc_cid",
    "mc_eid",
    "igshid",
    "yclid",
    "wickedid",
    "_hsenc",
    "_hsmi",
    "vero_id",
    "oly_anon_id",
    "oly_enc_id",
];

/// Errors produced by bookmark validation, canonicalization, or deduplication.
#[derive(Debug, Error)]
pub enum DeduplicatorError {
    #[error("bookmark id cannot be empty")]
    EmptyId,
    #[error("invalid bookmark url `{url}`: {reason}")]
    InvalidUrl { url: String, reason: String },
    #[error("unsupported url scheme `{scheme}`")]
    UnsupportedScheme { scheme: String },
    #[error("invalid deduplication configuration: {reason}")]
    InvalidConfig { reason: String },
    #[error("json serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Represents a single bookmark entry subject to deduplication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookmarkRecord {
    pub id: String,
    pub title: String,
    pub url: String,
    pub folder_path: Vec<String>,
    pub tags: Vec<String>,
    pub created_at_unix: i64,
    pub updated_at_unix: i64,
    pub visit_count: u64,
    pub notes: Option<String>,
}

impl BookmarkRecord {
    /// Validates that the bookmark has a non-empty identifier and a supported URL.
    pub fn validate(&self) -> Result<(), DeduplicatorError> {
        if self.id.trim().is_empty() {
            return Err(DeduplicatorError::EmptyId);
        }
        let parsed = Url::parse(self.url.trim()).map_err(|err| DeduplicatorError::InvalidUrl {
            url: self.url.clone(),
            reason: err.to_string(),
        })?;

        match parsed.scheme() {
            "http" | "https" => {
                if parsed.host_str().is_none() {
                    return Err(DeduplicatorError::InvalidUrl {
                        url: self.url.clone(),
                        reason: "http/https url must include a host".to_string(),
                    });
                }
            }
            "file" | "ftp" => {}
            other => {
                return Err(DeduplicatorError::UnsupportedScheme {
                    scheme: other.to_string(),
                });
            }
        }

        Ok(())
    }
}

/// Controls how bookmark URLs are normalized into canonical comparison keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalizationConfig {
    pub strip_tracking_params: bool,
    pub custom_stripped_params: Vec<String>,
    pub strip_fragment: bool,
    pub normalize_trailing_slash: bool,
    pub ignore_www_prefix: bool,
    pub ignore_scheme: bool,
    pub sort_query_params: bool,
    pub case_sensitive_path: bool,
}

impl Default for CanonicalizationConfig {
    fn default() -> Self {
        Self {
            strip_tracking_params: true,
            custom_stripped_params: Vec::new(),
            strip_fragment: true,
            normalize_trailing_slash: true,
            ignore_www_prefix: true,
            ignore_scheme: true,
            sort_query_params: true,
            case_sensitive_path: true,
        }
    }
}

impl CanonicalizationConfig {
    /// Validates the canonicalization configuration.
    pub fn validate(&self) -> Result<(), DeduplicatorError> {
        for param in &self.custom_stripped_params {
            if param.trim().is_empty() {
                return Err(DeduplicatorError::InvalidConfig {
                    reason: "custom_stripped_params cannot contain empty entries".to_string(),
                });
            }
        }
        Ok(())
    }
}

/// Strategy used to pick or synthesize the surviving bookmark in a duplicate group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    KeepNewest,
    KeepOldest,
    KeepMostVisited,
    KeepLongestMetadata,
    MergeMetadata,
}

/// Top-level configuration for [`BookmarkDeduplicator`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeduplicationConfig {
    pub canonicalization: CanonicalizationConfig,
    pub merge_strategy: MergeStrategy,
    pub allow_cross_folder_dedup: bool,
    pub prefer_https: bool,
}

impl Default for DeduplicationConfig {
    fn default() -> Self {
        Self {
            canonicalization: CanonicalizationConfig::default(),
            merge_strategy: MergeStrategy::MergeMetadata,
            allow_cross_folder_dedup: true,
            prefer_https: true,
        }
    }
}

impl DeduplicationConfig {
    /// Validates the deduplication configuration.
    pub fn validate(&self) -> Result<(), DeduplicatorError> {
        self.canonicalization.validate()
    }
}

/// A cluster of bookmarks that share the same canonical key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub canonical_key: String,
    pub fingerprint_hex: String,
    pub keeper: BookmarkRecord,
    pub duplicates: Vec<BookmarkRecord>,
    pub merged_tags: Vec<String>,
    pub combined_visit_count: u64,
}

/// Summary of an invalid bookmark skipped during lenient deduplication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedBookmark {
    pub id: String,
    pub url: String,
    pub reason: String,
}

/// Complete output of a deduplication run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeduplicationReport {
    pub total_input: usize,
    pub unique_count: usize,
    pub duplicate_count: usize,
    pub groups: Vec<DuplicateGroup>,
    pub deduplicated: Vec<BookmarkRecord>,
    pub skipped_invalid: Vec<SkippedBookmark>,
}

/// Returns true if a query parameter name should be stripped according to config.
fn is_stripped_param(key: &str, cfg: &CanonicalizationConfig) -> bool {
    let lower = key.to_ascii_lowercase();
    if cfg.strip_tracking_params
        && (lower.starts_with("utm_") || DEFAULT_TRACKING_PARAMS.contains(&lower.as_str()))
    {
        return true;
    }
    cfg.custom_stripped_params
        .iter()
        .any(|custom| custom.trim().eq_ignore_ascii_case(&lower))
}

/// Canonicalizes a URL string according to the provided [`CanonicalizationConfig`].
pub fn canonicalize_url(
    raw_url: &str,
    cfg: &CanonicalizationConfig,
) -> Result<String, DeduplicatorError> {
    cfg.validate()?;

    let mut parsed = Url::parse(raw_url.trim()).map_err(|err| DeduplicatorError::InvalidUrl {
        url: raw_url.to_string(),
        reason: err.to_string(),
    })?;

    let scheme = parsed.scheme().to_ascii_lowercase();
    match scheme.as_str() {
        "http" | "https" | "ftp" | "file" => {}
        other => {
            return Err(DeduplicatorError::UnsupportedScheme {
                scheme: other.to_string(),
            });
        }
    }

    let normalized_scheme = if cfg.ignore_scheme && (scheme == "http" || scheme == "https") {
        "https".to_string()
    } else {
        scheme.clone()
    };

    if parsed.set_scheme(&normalized_scheme).is_err() {
        return Err(DeduplicatorError::InvalidUrl {
            url: raw_url.to_string(),
            reason: format!("failed to normalize scheme `{normalized_scheme}`"),
        });
    }

    if let Some(host) = parsed.host_str() {
        let mut normalized_host = host.to_ascii_lowercase();
        if cfg.ignore_www_prefix {
            if let Some(stripped) = normalized_host.strip_prefix("www.") {
                if !stripped.is_empty() {
                    normalized_host = stripped.to_string();
                }
            }
        }
        if parsed.set_host(Some(&normalized_host)).is_err() {
            return Err(DeduplicatorError::InvalidUrl {
                url: raw_url.to_string(),
                reason: format!("failed to normalize host `{normalized_host}`"),
            });
        }
    }

    // Clear explicit default ports so `https://example.com:443/` matches `https://example.com/`.
    let clear_port = matches!(
        (normalized_scheme.as_str(), parsed.port()),
        ("http", Some(80)) | ("https", Some(443)) | ("ftp", Some(21))
    );
    if clear_port {
        let _ = parsed.set_port(None);
    }

    // Normalize path.
    let mut path = parsed.path().to_string();
    if !cfg.case_sensitive_path {
        path = path.to_ascii_lowercase();
    }
    if cfg.normalize_trailing_slash && path.len() > 1 {
        while path.len() > 1 && path.ends_with('/') {
            path.pop();
        }
    }
    parsed.set_path(&path);

    // Filter and optionally sort query parameters.
    let mut query_pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .filter(|(k, _)| !is_stripped_param(k, cfg))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();

    if cfg.sort_query_params {
        query_pairs.sort();
    }

    if query_pairs.is_empty() {
        parsed.set_query(None);
    } else {
        let mut serializer = parsed.query_pairs_mut();
        serializer.clear();
        for (k, v) in &query_pairs {
            serializer.append_pair(k, v);
        }
    }

    if cfg.strip_fragment {
        parsed.set_fragment(None);
    }

    Ok(parsed.to_string())
}

/// Computes a lowercase hexadecimal SHA-256 digest for a canonical key.
pub fn fingerprint_key(canonical_key: &str) -> String {
    let hash = digest::digest(&digest::SHA256, canonical_key.as_bytes());
    let mut hex = String::with_capacity(hash.as_ref().len() * 2);
    for byte in hash.as_ref() {
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

/// Deduplicates and merges bookmark collections according to [`DeduplicationConfig`].
#[derive(Debug, Clone)]
pub struct BookmarkDeduplicator {
    config: DeduplicationConfig,
}

impl BookmarkDeduplicator {
    /// Creates a new [`BookmarkDeduplicator`] after validating the configuration.
    pub fn new(config: DeduplicationConfig) -> Result<Self, DeduplicatorError> {
        config.validate()?;
        Ok(Self { config })
    }

    /// Returns a reference to the active configuration.
    pub fn config(&self) -> &DeduplicationConfig {
        &self.config
    }

    /// Computes the deduplication key for a validated bookmark record.
    pub fn record_key(&self, record: &BookmarkRecord) -> Result<String, DeduplicatorError> {
        record.validate()?;
        let canonical_url = canonicalize_url(&record.url, &self.config.canonicalization)?;
        if self.config.allow_cross_folder_dedup {
            Ok(canonical_url)
        } else {
            let folder = record
                .folder_path
                .iter()
                .map(|seg| seg.trim().to_ascii_lowercase())
                .collect::<Vec<_>>()
                .join("/");
            Ok(format!("folder:{folder}|url:{canonical_url}"))
        }
    }

    /// Deduplicates a slice of bookmarks, failing on the first invalid record or config error.
    pub fn deduplicate(
        &self,
        records: &[BookmarkRecord],
    ) -> Result<DeduplicationReport, DeduplicatorError> {
        self.config.validate()?;

        let mut buckets: BTreeMap<String, Vec<BookmarkRecord>> = BTreeMap::new();
        for record in records {
            let key = self.record_key(record)?;
            buckets.entry(key).or_default().push(record.clone());
        }

        Ok(self.build_report(records.len(), buckets, Vec::new()))
    }

    /// Deduplicates a slice of bookmarks, collecting invalid entries in `skipped_invalid`.
    pub fn deduplicate_lenient(&self, records: &[BookmarkRecord]) -> DeduplicationReport {
        let mut buckets: BTreeMap<String, Vec<BookmarkRecord>> = BTreeMap::new();
        let mut skipped = Vec::new();

        for record in records {
            match self.record_key(record) {
                Ok(key) => {
                    buckets.entry(key).or_default().push(record.clone());
                }
                Err(err) => {
                    skipped.push(SkippedBookmark {
                        id: record.id.clone(),
                        url: record.url.clone(),
                        reason: err.to_string(),
                    });
                }
            }
        }

        self.build_report(records.len(), buckets, skipped)
    }

    /// Finds all records in `existing` that share a canonical key with `candidate`.
    pub fn find_duplicates_for(
        &self,
        candidate: &BookmarkRecord,
        existing: &[BookmarkRecord],
    ) -> Result<Vec<BookmarkRecord>, DeduplicatorError> {
        let target_key = self.record_key(candidate)?;
        let mut matches = Vec::new();
        for item in existing {
            if item.id == candidate.id {
                continue;
            }
            if self.record_key(item)? == target_key {
                matches.push(item.clone());
            }
        }
        Ok(matches)
    }

    /// Serializes a [`DeduplicationReport`] to pretty JSON.
    pub fn to_json_report(report: &DeduplicationReport) -> Result<String, DeduplicatorError> {
        Ok(serde_json::to_string_pretty(report)?)
    }

    /// Deserializes a list of [`BookmarkRecord`] values from JSON.
    pub fn from_json_records(json: &str) -> Result<Vec<BookmarkRecord>, DeduplicatorError> {
        Ok(serde_json::from_str(json)?)
    }

    fn build_report(
        &self,
        total_input: usize,
        buckets: BTreeMap<String, Vec<BookmarkRecord>>,
        skipped_invalid: Vec<SkippedBookmark>,
    ) -> DeduplicationReport {
        let mut groups = Vec::new();
        let mut deduplicated = Vec::with_capacity(buckets.len());
        let mut duplicate_count = 0usize;

        for (canonical_key, mut items) in buckets {
            if items.is_empty() {
                continue;
            }

            let keeper_idx = self.select_keeper_index(&items);
            let mut keeper = items.swap_remove(keeper_idx);
            // Sort remaining duplicates deterministically by id.
            items.sort_by(|a, b| a.id.cmp(&b.id));

            let mut tag_set: BTreeSet<String> = keeper
                .tags
                .iter()
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect();

            let mut combined_visits = keeper.visit_count;
            for dup in &items {
                combined_visits = combined_visits.saturating_add(dup.visit_count);
                for tag in &dup.tags {
                    let trimmed = tag.trim();
                    if !trimmed.is_empty() {
                        tag_set.insert(trimmed.to_string());
                    }
                }
            }

            let merged_tags: Vec<String> = tag_set.into_iter().collect();

            if self.config.merge_strategy == MergeStrategy::MergeMetadata && !items.is_empty() {
                keeper.tags = merged_tags.clone();
                keeper.visit_count = combined_visits;

                if keeper.title.trim().is_empty() {
                    if let Some(title) =
                        items.iter().map(|d| d.title.trim()).find(|t| !t.is_empty())
                    {
                        keeper.title = title.to_string();
                    }
                }

                if keeper.folder_path.is_empty() {
                    if let Some(folder) = items.iter().find(|d| !d.folder_path.is_empty()) {
                        keeper.folder_path = folder.folder_path.clone();
                    }
                }

                if keeper
                    .notes
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or("")
                    .is_empty()
                {
                    if let Some(note) = items
                        .iter()
                        .filter_map(|d| d.notes.as_deref())
                        .map(str::trim)
                        .find(|n| !n.is_empty())
                    {
                        keeper.notes = Some(note.to_string());
                    }
                }

                keeper.created_at_unix = items
                    .iter()
                    .map(|d| d.created_at_unix)
                    .fold(keeper.created_at_unix, i64::min);
                keeper.updated_at_unix = items
                    .iter()
                    .map(|d| d.updated_at_unix)
                    .fold(keeper.updated_at_unix, i64::max);
            }

            if self.config.prefer_https {
                if let Some(https_url) = std::iter::once(&keeper)
                    .chain(items.iter())
                    .map(|r| r.url.as_str())
                    .find(|u| u.trim().to_ascii_lowercase().starts_with("https://"))
                {
                    keeper.url = https_url.to_string();
                }
            }

            duplicate_count += items.len();
            if !items.is_empty() {
                groups.push(DuplicateGroup {
                    fingerprint_hex: fingerprint_key(&canonical_key),
                    canonical_key,
                    keeper: keeper.clone(),
                    duplicates: items,
                    merged_tags,
                    combined_visit_count: combined_visits,
                });
            }

            deduplicated.push(keeper);
        }

        deduplicated.sort_by(|a, b| a.id.cmp(&b.id));

        DeduplicationReport {
            total_input,
            unique_count: deduplicated.len(),
            duplicate_count,
            groups,
            deduplicated,
            skipped_invalid,
        }
    }

    fn select_keeper_index(&self, items: &[BookmarkRecord]) -> usize {
        let mut best_idx = 0usize;
        for idx in 1..items.len() {
            if self.is_better_keeper(&items[idx], &items[best_idx]) {
                best_idx = idx;
            }
        }
        best_idx
    }

    fn is_better_keeper(&self, candidate: &BookmarkRecord, current: &BookmarkRecord) -> bool {
        match self.config.merge_strategy {
            MergeStrategy::KeepNewest | MergeStrategy::MergeMetadata => {
                (
                    candidate.updated_at_unix,
                    candidate.visit_count,
                    candidate.created_at_unix,
                ) > (
                    current.updated_at_unix,
                    current.visit_count,
                    current.created_at_unix,
                ) || ((
                    candidate.updated_at_unix,
                    candidate.visit_count,
                    candidate.created_at_unix,
                ) == (
                    current.updated_at_unix,
                    current.visit_count,
                    current.created_at_unix,
                ) && candidate.id < current.id)
            }
            MergeStrategy::KeepOldest => {
                (candidate.created_at_unix, candidate.updated_at_unix)
                    < (current.created_at_unix, current.updated_at_unix)
                    || ((candidate.created_at_unix, candidate.updated_at_unix)
                        == (current.created_at_unix, current.updated_at_unix)
                        && candidate.id < current.id)
            }
            MergeStrategy::KeepMostVisited => {
                (candidate.visit_count, candidate.updated_at_unix)
                    > (current.visit_count, current.updated_at_unix)
                    || ((candidate.visit_count, candidate.updated_at_unix)
                        == (current.visit_count, current.updated_at_unix)
                        && candidate.id < current.id)
            }
            MergeStrategy::KeepLongestMetadata => {
                let candidate_score = metadata_richness(candidate);
                let current_score = metadata_richness(current);
                candidate_score > current_score
                    || (candidate_score == current_score && candidate.id < current.id)
            }
        }
    }
}

fn metadata_richness(record: &BookmarkRecord) -> usize {
    let notes_len = record.notes.as_deref().map(str::len).unwrap_or(0);
    record.title.len() + record.tags.len() * 16 + record.folder_path.len() * 8 + notes_len
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bookmark(id: &str, url: &str) -> BookmarkRecord {
        BookmarkRecord {
            id: id.to_string(),
            title: format!("Title {id}"),
            url: url.to_string(),
            folder_path: vec!["Reading".to_string()],
            tags: vec!["rust".to_string()],
            created_at_unix: 1_700_000_000,
            updated_at_unix: 1_700_000_100,
            visit_count: 3,
            notes: None,
        }
    }

    #[test]
    fn canonicalize_strips_tracking_www_trailing_slash_and_sorts_query() {
        let cfg = CanonicalizationConfig::default();
        let a = canonicalize_url(
            "http://WWW.Example.com:80/docs/guide/?b=2&utm_source=news&a=1&fbclid=xyz#section",
            &cfg,
        )
        .expect("canonicalize a");
        let b = canonicalize_url("https://example.com/docs/guide?a=1&b=2", &cfg)
            .expect("canonicalize b");

        assert_eq!(a, b);
        assert_eq!(a, "https://example.com/docs/guide?a=1&b=2");
    }

    #[test]
    fn deduplicate_merges_metadata_and_prefers_https() {
        let mut old_http = sample_bookmark("bm-1", "http://example.com/article?utm_medium=email");
        old_http.created_at_unix = 1_600_000_000;
        old_http.updated_at_unix = 1_600_000_050;
        old_http.visit_count = 4;
        old_http.tags = vec!["linux".to_string(), "search".to_string()];
        old_http.notes = Some("Useful reference".to_string());

        let mut new_https = sample_bookmark("bm-2", "https://www.example.com/article/");
        new_https.created_at_unix = 1_700_000_000;
        new_https.updated_at_unix = 1_700_000_500;
        new_https.visit_count = 6;
        new_https.tags = vec!["rust".to_string()];

        let distinct = sample_bookmark("bm-3", "https://example.com/other");

        let dedup = BookmarkDeduplicator::new(DeduplicationConfig::default())
            .expect("valid deduplicator config");
        let report = dedup
            .deduplicate(&[old_http, new_https, distinct])
            .expect("deduplicate succeeds");

        assert_eq!(report.total_input, 3);
        assert_eq!(report.unique_count, 2);
        assert_eq!(report.duplicate_count, 1);
        assert_eq!(report.groups.len(), 1);

        let group = &report.groups[0];
        assert_eq!(group.keeper.id, "bm-2");
        assert_eq!(group.keeper.url, "https://www.example.com/article/");
        assert_eq!(group.keeper.visit_count, 10);
        assert_eq!(group.keeper.created_at_unix, 1_600_000_000);
        assert_eq!(group.keeper.updated_at_unix, 1_700_000_500);
        assert_eq!(group.keeper.notes.as_deref(), Some("Useful reference"));
        assert_eq!(
            group.merged_tags,
            vec![
                "linux".to_string(),
                "rust".to_string(),
                "search".to_string()
            ]
        );
        assert_eq!(group.fingerprint_hex.len(), 64);

        let json = BookmarkDeduplicator::to_json_report(&report).expect("serialize report");
        assert!(json.contains("\"duplicate_count\": 1"));
    }

    #[test]
    fn cross_folder_disabled_keeps_separate_folder_bookmarks() {
        let mut b1 = sample_bookmark("bm-1", "https://example.com/page");
        b1.folder_path = vec!["Work".to_string()];
        let mut b2 = sample_bookmark("bm-2", "https://example.com/page");
        b2.folder_path = vec!["Personal".to_string()];

        let cfg = DeduplicationConfig {
            allow_cross_folder_dedup: false,
            ..DeduplicationConfig::default()
        };
        let dedup = BookmarkDeduplicator::new(cfg).expect("valid config");
        let report = dedup.deduplicate(&[b1, b2]).expect("deduplicate succeeds");

        assert_eq!(report.unique_count, 2);
        assert_eq!(report.duplicate_count, 0);
        assert!(report.groups.is_empty());
    }

    #[test]
    fn lenient_mode_skips_invalid_urls_and_empty_ids() {
        let valid = sample_bookmark("bm-1", "https://example.com/ok");
        let bad_url = sample_bookmark("bm-2", "not-a-valid-url");
        let empty_id = sample_bookmark("   ", "https://example.com/ok");
        let bad_scheme = sample_bookmark("bm-3", "javascript:alert(1)");

        let dedup =
            BookmarkDeduplicator::new(DeduplicationConfig::default()).expect("valid config");
        let report = dedup.deduplicate_lenient(&[valid, bad_url, empty_id, bad_scheme]);

        assert_eq!(report.total_input, 4);
        assert_eq!(report.unique_count, 1);
        assert_eq!(report.skipped_invalid.len(), 3);
    }

    #[test]
    fn validation_rejects_empty_custom_stripped_param() {
        let cfg = DeduplicationConfig {
            canonicalization: CanonicalizationConfig {
                custom_stripped_params: vec!["   ".to_string()],
                ..CanonicalizationConfig::default()
            },
            ..DeduplicationConfig::default()
        };
        let err = BookmarkDeduplicator::new(cfg).expect_err("must reject empty param");
        assert!(matches!(err, DeduplicatorError::InvalidConfig { .. }));
    }
}
