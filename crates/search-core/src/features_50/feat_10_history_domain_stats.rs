//! Per-domain browsing history statistics, aggregation, ranking, and serialization.
//!
//! This module extracts normalized domains from history visit records, aggregates
//! visit counts, dwell durations, transition breakdowns, unique URL counts, and
//! time-decayed frecency scores, and supports filtering, sorting, and JSON
//! serialization of the resulting domain statistics report.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

/// Errors returned by domain extraction, validation, aggregation, and querying.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainStatsError {
    /// The supplied URL string could not be parsed.
    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    /// The parsed URL does not contain a host component (e.g. `about:blank` or `data:`).
    #[error("URL has no host: {0}")]
    MissingHost(String),

    /// A timestamp was negative or otherwise out of valid bounds.
    #[error("invalid timestamp (ms): {timestamp_ms}")]
    InvalidTimestamp { timestamp_ms: i64 },

    /// A query time range had `since_ms > until_ms`.
    #[error("invalid time range: since ({since_ms}) > until ({until_ms})")]
    InvalidTimeRange { since_ms: i64, until_ms: i64 },

    /// A requested result limit was zero when a positive limit was required.
    #[error("invalid limit: {0} (must be greater than 0)")]
    InvalidLimit(usize),

    /// JSON serialization or deserialization failed.
    #[error("serialization error: {0}")]
    SerializationError(String),
}

/// How a history visit was initiated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VisitTransition {
    /// User typed or selected the URL in the address bar.
    Typed,
    /// User clicked a link on another page.
    #[default]
    Link,
    /// User opened a bookmark.
    Bookmark,
    /// User reloaded the page.
    Reload,
    /// Navigation arrived via an HTTP or client-side redirect.
    Redirect,
    /// Any other transition source.
    Other,
}

/// A single raw browsing history visit event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryVisitRecord {
    /// Full visited URL.
    pub url: String,
    /// Visit timestamp in UTC epoch milliseconds (must be non-negative).
    pub visited_at_ms: i64,
    /// Active dwell duration on the page in milliseconds.
    pub duration_ms: u64,
    /// Navigation transition type.
    pub transition: VisitTransition,
    /// Optional page title captured at visit time.
    pub title: Option<String>,
}

impl HistoryVisitRecord {
    /// Validates that the record has a non-empty URL and a non-negative timestamp.
    pub fn validate(&self) -> Result<(), DomainStatsError> {
        if self.visited_at_ms < 0 {
            return Err(DomainStatsError::InvalidTimestamp {
                timestamp_ms: self.visited_at_ms,
            });
        }
        if self.url.trim().is_empty() {
            return Err(DomainStatsError::InvalidUrl(self.url.clone()));
        }
        Ok(())
    }
}

/// Configuration controlling how URLs are mapped to normalized domain keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainExtractionConfig {
    /// Strip a leading `www.` prefix when a registrable remainder remains (e.g. `www.example.com` -> `example.com`).
    pub strip_www: bool,
    /// Lowercase the extracted host name.
    pub lowercase: bool,
    /// Include non-default port numbers in the domain key (e.g. `localhost:8080`).
    pub include_port: bool,
}

impl Default for DomainExtractionConfig {
    fn default() -> Self {
        Self {
            strip_www: true,
            lowercase: true,
            include_port: false,
        }
    }
}

/// Extracts and normalizes the domain key from a raw URL string.
pub fn extract_domain(
    raw_url: &str,
    config: &DomainExtractionConfig,
) -> Result<String, DomainStatsError> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return Err(DomainStatsError::InvalidUrl(raw_url.to_string()));
    }

    let parsed =
        Url::parse(trimmed).map_err(|_| DomainStatsError::InvalidUrl(raw_url.to_string()))?;

    let host = parsed
        .host_str()
        .filter(|h| !h.is_empty())
        .ok_or_else(|| DomainStatsError::MissingHost(raw_url.to_string()))?;

    let mut normalized = if config.lowercase {
        host.to_ascii_lowercase()
    } else {
        host.to_string()
    };

    if config.strip_www {
        let stripped = if config.lowercase {
            normalized.strip_prefix("www.")
        } else if normalized.len() > 4 && normalized[..4].eq_ignore_ascii_case("www.") {
            Some(&normalized[4..])
        } else {
            None
        };

        // Only strip `www.` when the remainder still looks like a multi-label domain,
        // avoiding turning `www.com` into `com`.
        if let Some(rest) = stripped {
            if rest.contains('.') {
                normalized = rest.to_string();
            }
        }
    }

    if config.include_port {
        if let Some(port) = parsed.port() {
            normalized.push(':');
            normalized.push_str(&port.to_string());
        }
    }

    Ok(normalized)
}

/// Aggregated statistics for a single normalized domain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomainStats {
    /// Normalized domain name (e.g. `example.com`).
    pub domain: String,
    /// Total number of recorded visits to this domain.
    pub visit_count: u64,
    /// Number of distinct URLs visited under this domain.
    pub unique_urls: u64,
    /// Number of visits initiated by typing in the address bar.
    pub typed_count: u64,
    /// Number of visits initiated by clicking a link.
    pub link_count: u64,
    /// Number of visits initiated from bookmarks.
    pub bookmark_count: u64,
    /// Number of reload visits.
    pub reload_count: u64,
    /// Sum of dwell durations across all visits in milliseconds.
    pub total_duration_ms: u64,
    /// Mean dwell duration per visit in milliseconds.
    pub avg_duration_ms: u64,
    /// Earliest visit timestamp in UTC epoch milliseconds.
    pub first_visit_ms: i64,
    /// Most recent visit timestamp in UTC epoch milliseconds.
    pub last_visit_ms: i64,
    /// Time-decayed frecency score computed relative to the report generation timestamp.
    pub frecency_score: f64,
    /// Page title from the most recent visit that carried a non-empty title.
    pub last_title: Option<String>,
}

/// Summary of a batch ingestion operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchIngestSummary {
    /// Number of records successfully validated and aggregated.
    pub accepted: usize,
    /// Number of invalid records skipped (when `skip_invalid` is true).
    pub skipped: usize,
}

#[derive(Debug, Clone)]
struct DomainAccumulator {
    visit_count: u64,
    unique_urls: BTreeSet<String>,
    typed_count: u64,
    link_count: u64,
    bookmark_count: u64,
    reload_count: u64,
    total_duration_ms: u64,
    first_visit_ms: i64,
    last_visit_ms: i64,
    last_title: Option<String>,
    last_title_ts_ms: i64,
}

impl DomainAccumulator {
    fn from_first(record: &HistoryVisitRecord) -> Self {
        let mut unique_urls = BTreeSet::new();
        unique_urls.insert(record.url.clone());

        let clean_title = record
            .title
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string);

        let mut acc = Self {
            visit_count: 1,
            unique_urls,
            typed_count: 0,
            link_count: 0,
            bookmark_count: 0,
            reload_count: 0,
            total_duration_ms: record.duration_ms,
            first_visit_ms: record.visited_at_ms,
            last_visit_ms: record.visited_at_ms,
            last_title_ts_ms: if clean_title.is_some() {
                record.visited_at_ms
            } else {
                -1
            },
            last_title: clean_title,
        };
        acc.increment_transition(record.transition);
        acc
    }

    fn add_record(&mut self, record: &HistoryVisitRecord) {
        self.visit_count = self.visit_count.saturating_add(1);
        self.unique_urls.insert(record.url.clone());
        self.increment_transition(record.transition);
        self.total_duration_ms = self.total_duration_ms.saturating_add(record.duration_ms);

        if record.visited_at_ms < self.first_visit_ms {
            self.first_visit_ms = record.visited_at_ms;
        }
        if record.visited_at_ms > self.last_visit_ms {
            self.last_visit_ms = record.visited_at_ms;
        }

        if let Some(title) = record
            .title
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            if record.visited_at_ms >= self.last_title_ts_ms {
                self.last_title = Some(title.to_string());
                self.last_title_ts_ms = record.visited_at_ms;
            }
        }
    }

    fn increment_transition(&mut self, transition: VisitTransition) {
        match transition {
            VisitTransition::Typed => self.typed_count = self.typed_count.saturating_add(1),
            VisitTransition::Link => self.link_count = self.link_count.saturating_add(1),
            VisitTransition::Bookmark => {
                self.bookmark_count = self.bookmark_count.saturating_add(1)
            }
            VisitTransition::Reload => self.reload_count = self.reload_count.saturating_add(1),
            VisitTransition::Redirect | VisitTransition::Other => {}
        }
    }

    fn merge_from(&mut self, other: &Self) {
        self.visit_count = self.visit_count.saturating_add(other.visit_count);
        for url in &other.unique_urls {
            self.unique_urls.insert(url.clone());
        }
        self.typed_count = self.typed_count.saturating_add(other.typed_count);
        self.link_count = self.link_count.saturating_add(other.link_count);
        self.bookmark_count = self.bookmark_count.saturating_add(other.bookmark_count);
        self.reload_count = self.reload_count.saturating_add(other.reload_count);
        self.total_duration_ms = self
            .total_duration_ms
            .saturating_add(other.total_duration_ms);

        if other.first_visit_ms < self.first_visit_ms {
            self.first_visit_ms = other.first_visit_ms;
        }
        if other.last_visit_ms > self.last_visit_ms {
            self.last_visit_ms = other.last_visit_ms;
        }
        if other.last_title.is_some() && other.last_title_ts_ms >= self.last_title_ts_ms {
            self.last_title = other.last_title.clone();
            self.last_title_ts_ms = other.last_title_ts_ms;
        }
    }

    fn finalize(&self, domain: &str, reference_now_ms: i64) -> DomainStats {
        let avg_duration_ms = self
            .total_duration_ms
            .checked_div(self.visit_count)
            .unwrap_or(0);
        let frecency_score = compute_frecency(
            self.visit_count,
            self.typed_count,
            self.bookmark_count,
            self.last_visit_ms,
            reference_now_ms,
        );

        DomainStats {
            domain: domain.to_string(),
            visit_count: self.visit_count,
            unique_urls: self.unique_urls.len() as u64,
            typed_count: self.typed_count,
            link_count: self.link_count,
            bookmark_count: self.bookmark_count,
            reload_count: self.reload_count,
            total_duration_ms: self.total_duration_ms,
            avg_duration_ms,
            first_visit_ms: self.first_visit_ms,
            last_visit_ms: self.last_visit_ms,
            frecency_score,
            last_title: self.last_title.clone(),
        }
    }
}

/// Computes a time-decayed frecency score for a domain.
///
/// Half-life is 7 days (604,800,000 ms). Typed and bookmark visits contribute
/// higher intent weight than passive link clicks.
fn compute_frecency(
    visit_count: u64,
    typed_count: u64,
    bookmark_count: u64,
    last_visit_ms: i64,
    reference_now_ms: i64,
) -> f64 {
    const HALF_LIFE_MS: f64 = 7.0 * 24.0 * 60.0 * 60.0 * 1000.0;

    let age_ms = (reference_now_ms.saturating_sub(last_visit_ms)).max(0) as f64;
    let decay = 0.5_f64.powf(age_ms / HALF_LIFE_MS);

    let base_weight = visit_count as f64;
    let intent_bonus = (typed_count as f64) * 1.5 + (bookmark_count as f64) * 1.25;
    ((base_weight + intent_bonus) * decay * 1000.0).round() / 1000.0
}

/// Incremental aggregator that ingests history visits and builds per-domain reports.
#[derive(Debug, Clone)]
pub struct DomainStatsAggregator {
    config: DomainExtractionConfig,
    domains: BTreeMap<String, DomainAccumulator>,
}

impl Default for DomainStatsAggregator {
    fn default() -> Self {
        Self::new(DomainExtractionConfig::default())
    }
}

impl DomainStatsAggregator {
    /// Creates a new aggregator with the given domain extraction configuration.
    pub fn new(config: DomainExtractionConfig) -> Self {
        Self {
            config,
            domains: BTreeMap::new(),
        }
    }

    /// Returns the number of distinct normalized domains currently tracked.
    pub fn domain_count(&self) -> usize {
        self.domains.len()
    }

    /// Removes all accumulated domain statistics.
    pub fn clear(&mut self) {
        self.domains.clear();
    }

    /// Validates and records a single history visit.
    pub fn record_visit(&mut self, record: &HistoryVisitRecord) -> Result<(), DomainStatsError> {
        record.validate()?;
        let domain = extract_domain(&record.url, &self.config)?;

        match self.domains.get_mut(&domain) {
            Some(acc) => acc.add_record(record),
            None => {
                self.domains
                    .insert(domain, DomainAccumulator::from_first(record));
            }
        }
        Ok(())
    }

    /// Records a batch of history visits.
    ///
    /// When `skip_invalid` is `true`, malformed URLs, hostless URLs, or invalid
    /// timestamps are counted in `BatchIngestSummary::skipped` instead of aborting.
    pub fn record_batch<'a, I>(
        &mut self,
        records: I,
        skip_invalid: bool,
    ) -> Result<BatchIngestSummary, DomainStatsError>
    where
        I: IntoIterator<Item = &'a HistoryVisitRecord>,
    {
        let mut accepted = 0usize;
        let mut skipped = 0usize;

        for record in records {
            match self.record_visit(record) {
                Ok(()) => accepted += 1,
                Err(_) if skip_invalid => skipped += 1,
                Err(err) => return Err(err),
            }
        }

        Ok(BatchIngestSummary { accepted, skipped })
    }

    /// Merges another aggregator's accumulated state into `self`.
    pub fn merge(&mut self, other: &Self) {
        for (domain, other_acc) in &other.domains {
            match self.domains.get_mut(domain) {
                Some(acc) => acc.merge_from(other_acc),
                None => {
                    self.domains.insert(domain.clone(), other_acc.clone());
                }
            }
        }
    }

    /// Builds a finalized [`DomainStatsReport`] relative to `reference_now_ms`.
    pub fn build_report(
        &self,
        reference_now_ms: i64,
    ) -> Result<DomainStatsReport, DomainStatsError> {
        if reference_now_ms < 0 {
            return Err(DomainStatsError::InvalidTimestamp {
                timestamp_ms: reference_now_ms,
            });
        }

        let mut total_visits = 0u64;
        let mut total_duration_ms = 0u64;
        let mut domains = Vec::with_capacity(self.domains.len());

        for (domain, acc) in &self.domains {
            let stats = acc.finalize(domain, reference_now_ms);
            total_visits = total_visits.saturating_add(stats.visit_count);
            total_duration_ms = total_duration_ms.saturating_add(stats.total_duration_ms);
            domains.push(stats);
        }

        Ok(DomainStatsReport {
            generated_at_ms: reference_now_ms,
            total_visits,
            total_duration_ms,
            unique_domains: domains.len(),
            domains,
        })
    }
}

/// Ordering criteria for querying domain statistics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DomainSortBy {
    /// Highest visit count first.
    #[default]
    VisitCountDesc,
    /// Most recently visited domain first.
    LastVisitDesc,
    /// Highest total dwell time first.
    TotalDurationDesc,
    /// Highest frecency score first.
    FrecencyDesc,
    /// Most unique URLs visited first.
    UniqueUrlsDesc,
    /// Alphabetical by domain name.
    DomainAsc,
}

/// Filter and sort parameters for querying a [`DomainStatsReport`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DomainStatsQuery {
    /// Include only domains with at least this many visits.
    pub min_visits: Option<u64>,
    /// Include only domains whose `last_visit_ms >= since_ms`.
    pub since_ms: Option<i64>,
    /// Include only domains whose `last_visit_ms <= until_ms`.
    pub until_ms: Option<i64>,
    /// Case-insensitive substring filter on the domain name.
    pub domain_substring: Option<String>,
    /// Sort ordering for matching domains.
    pub sort_by: DomainSortBy,
    /// Maximum number of results to return.
    pub limit: Option<usize>,
}

/// Serializable snapshot of per-domain browsing statistics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomainStatsReport {
    /// Timestamp (UTC epoch ms) at which this report was generated.
    pub generated_at_ms: i64,
    /// Total visits across all domains in the report.
    pub total_visits: u64,
    /// Total dwell duration across all domains in milliseconds.
    pub total_duration_ms: u64,
    /// Number of unique domains in the report.
    pub unique_domains: usize,
    /// Per-domain statistics entries.
    pub domains: Vec<DomainStats>,
}

impl DomainStatsReport {
    /// Filters and sorts domains according to `query`.
    pub fn query(&self, query: &DomainStatsQuery) -> Result<Vec<&DomainStats>, DomainStatsError> {
        if let (Some(since), Some(until)) = (query.since_ms, query.until_ms) {
            if since > until {
                return Err(DomainStatsError::InvalidTimeRange {
                    since_ms: since,
                    until_ms: until,
                });
            }
        }
        if let Some(0) = query.limit {
            return Err(DomainStatsError::InvalidLimit(0));
        }

        let needle = query
            .domain_substring
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_ascii_lowercase);

        let mut matched: Vec<&DomainStats> = self
            .domains
            .iter()
            .filter(|d| {
                if let Some(min) = query.min_visits {
                    if d.visit_count < min {
                        return false;
                    }
                }
                if let Some(since) = query.since_ms {
                    if d.last_visit_ms < since {
                        return false;
                    }
                }
                if let Some(until) = query.until_ms {
                    if d.last_visit_ms > until {
                        return false;
                    }
                }
                if let Some(ref sub) = needle {
                    if !d.domain.to_ascii_lowercase().contains(sub) {
                        return false;
                    }
                }
                true
            })
            .collect();

        matched.sort_by(|a, b| {
            let primary = match query.sort_by {
                DomainSortBy::VisitCountDesc => b.visit_count.cmp(&a.visit_count),
                DomainSortBy::LastVisitDesc => b.last_visit_ms.cmp(&a.last_visit_ms),
                DomainSortBy::TotalDurationDesc => b.total_duration_ms.cmp(&a.total_duration_ms),
                DomainSortBy::FrecencyDesc => b.frecency_score.total_cmp(&a.frecency_score),
                DomainSortBy::UniqueUrlsDesc => b.unique_urls.cmp(&a.unique_urls),
                DomainSortBy::DomainAsc => a.domain.cmp(&b.domain),
            };
            primary.then_with(|| a.domain.cmp(&b.domain))
        });

        if let Some(limit) = query.limit {
            matched.truncate(limit);
        }

        Ok(matched)
    }

    /// Returns the top `limit` domains ordered by `sort_by`.
    pub fn top_domains(
        &self,
        limit: usize,
        sort_by: DomainSortBy,
    ) -> Result<Vec<&DomainStats>, DomainStatsError> {
        self.query(&DomainStatsQuery {
            sort_by,
            limit: Some(limit),
            ..Default::default()
        })
    }

    /// Looks up statistics for an exact normalized domain name.
    pub fn get_domain(&self, domain: &str) -> Option<&DomainStats> {
        self.domains.iter().find(|d| d.domain == domain)
    }

    /// Returns the percentage share (`0.0..=100.0`) of total visits belonging to `domain`.
    pub fn share_of_visits(&self, domain: &str) -> f64 {
        if self.total_visits == 0 {
            return 0.0;
        }
        match self.get_domain(domain) {
            Some(stats) => (stats.visit_count as f64 / self.total_visits as f64) * 100.0,
            None => 0.0,
        }
    }

    /// Serializes the report to a pretty-printed JSON string.
    pub fn to_json(&self) -> Result<String, DomainStatsError> {
        serde_json::to_string_pretty(self)
            .map_err(|e| DomainStatsError::SerializationError(e.to_string()))
    }

    /// Deserializes a report from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, DomainStatsError> {
        serde_json::from_str(json).map_err(|e| DomainStatsError::SerializationError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_visit(
        url: &str,
        visited_at_ms: i64,
        duration_ms: u64,
        transition: VisitTransition,
        title: Option<&str>,
    ) -> HistoryVisitRecord {
        HistoryVisitRecord {
            url: url.to_string(),
            visited_at_ms,
            duration_ms,
            transition,
            title: title.map(str::to_string),
        }
    }

    #[test]
    fn test_domain_extraction_and_normalization() {
        let cfg = DomainExtractionConfig::default();
        assert_eq!(
            extract_domain("https://WWW.Example.COM/path?a=1", &cfg).unwrap(),
            "example.com"
        );
        assert_eq!(
            extract_domain("http://docs.rust-lang.org/std/", &cfg).unwrap(),
            "docs.rust-lang.org"
        );
        // Must not strip `www.` when doing so would leave a bare TLD.
        assert_eq!(extract_domain("https://www.com/", &cfg).unwrap(), "www.com");

        let port_cfg = DomainExtractionConfig {
            include_port: true,
            ..Default::default()
        };
        assert_eq!(
            extract_domain("http://localhost:8080/api", &port_cfg).unwrap(),
            "localhost:8080"
        );

        assert!(matches!(
            extract_domain("about:blank", &cfg),
            Err(DomainStatsError::MissingHost(_))
        ));
        assert!(matches!(
            extract_domain("not a url", &cfg),
            Err(DomainStatsError::InvalidUrl(_))
        ));
    }

    #[test]
    fn test_aggregation_ranking_and_json_roundtrip() {
        let now_ms = 1_700_000_000_000;
        let mut agg = DomainStatsAggregator::default();

        let records = vec![
            make_visit(
                "https://www.rust-lang.org/",
                now_ms - 10_000,
                15_000,
                VisitTransition::Typed,
                Some("Rust Programming Language"),
            ),
            make_visit(
                "https://rust-lang.org/learn",
                now_ms - 5_000,
                25_000,
                VisitTransition::Link,
                Some("Learn Rust"),
            ),
            make_visit(
                "https://rust-lang.org/learn",
                now_ms - 1_000,
                5_000,
                VisitTransition::Reload,
                None,
            ),
            make_visit(
                "https://crates.io/crates/serde",
                now_ms - 2_000,
                12_000,
                VisitTransition::Bookmark,
                Some("serde - crates.io"),
            ),
        ];

        let summary = agg.record_batch(&records, false).unwrap();
        assert_eq!(summary.accepted, 4);
        assert_eq!(summary.skipped, 0);

        let report = agg.build_report(now_ms).unwrap();
        assert_eq!(report.unique_domains, 2);
        assert_eq!(report.total_visits, 4);
        assert_eq!(report.total_duration_ms, 57_000);

        let rust_stats = report.get_domain("rust-lang.org").unwrap();
        assert_eq!(rust_stats.visit_count, 3);
        assert_eq!(rust_stats.unique_urls, 2);
        assert_eq!(rust_stats.typed_count, 1);
        assert_eq!(rust_stats.link_count, 1);
        assert_eq!(rust_stats.reload_count, 1);
        assert_eq!(rust_stats.total_duration_ms, 45_000);
        assert_eq!(rust_stats.avg_duration_ms, 15_000);
        assert_eq!(rust_stats.first_visit_ms, now_ms - 10_000);
        assert_eq!(rust_stats.last_visit_ms, now_ms - 1_000);
        assert_eq!(rust_stats.last_title.as_deref(), Some("Learn Rust"));
        assert!((report.share_of_visits("rust-lang.org") - 75.0).abs() < 1e-9);

        let top = report.top_domains(1, DomainSortBy::VisitCountDesc).unwrap();
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].domain, "rust-lang.org");

        let json = report.to_json().unwrap();
        let restored = DomainStatsReport::from_json(&json).unwrap();
        assert_eq!(report, restored);
    }

    #[test]
    fn test_batch_skip_invalid_merge_and_query_edge_cases() {
        let now_ms = 1_700_000_000_000;
        let mut agg_a = DomainStatsAggregator::default();
        let mut agg_b = DomainStatsAggregator::default();

        let batch_a = vec![
            make_visit(
                "about:config",
                now_ms - 5_000,
                1_000,
                VisitTransition::Typed,
                None,
            ),
            make_visit(
                "https://example.com/a",
                -10,
                1_000,
                VisitTransition::Link,
                None,
            ),
            make_visit(
                "https://example.com/a",
                now_ms - 3_000,
                4_000,
                VisitTransition::Link,
                Some("Example A"),
            ),
        ];

        let summary = agg_a.record_batch(&batch_a, true).unwrap();
        assert_eq!(summary.accepted, 1);
        assert_eq!(summary.skipped, 2);

        agg_b
            .record_visit(&make_visit(
                "https://example.com/b",
                now_ms - 1_000,
                6_000,
                VisitTransition::Typed,
                Some("Example B"),
            ))
            .unwrap();
        agg_b
            .record_visit(&make_visit(
                "https://news.ycombinator.com/",
                now_ms - 500_000,
                2_000,
                VisitTransition::Link,
                Some("Hacker News"),
            ))
            .unwrap();

        agg_a.merge(&agg_b);
        let report = agg_a.build_report(now_ms).unwrap();
        assert_eq!(report.unique_domains, 2);

        let ex = report.get_domain("example.com").unwrap();
        assert_eq!(ex.visit_count, 2);
        assert_eq!(ex.unique_urls, 2);
        assert_eq!(ex.total_duration_ms, 10_000);
        assert_eq!(ex.last_title.as_deref(), Some("Example B"));

        // Substring and time window query.
        let filtered = report
            .query(&DomainStatsQuery {
                min_visits: Some(2),
                since_ms: Some(now_ms - 10_000),
                until_ms: Some(now_ms),
                domain_substring: Some("EXAM".to_string()),
                sort_by: DomainSortBy::FrecencyDesc,
                limit: Some(5),
            })
            .unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].domain, "example.com");

        // Invalid query parameters.
        assert!(matches!(
            report.query(&DomainStatsQuery {
                since_ms: Some(100),
                until_ms: Some(50),
                ..Default::default()
            }),
            Err(DomainStatsError::InvalidTimeRange {
                since_ms: 100,
                until_ms: 50
            })
        ));
        assert!(matches!(
            report.top_domains(0, DomainSortBy::DomainAsc),
            Err(DomainStatsError::InvalidLimit(0))
        ));
    }
}
