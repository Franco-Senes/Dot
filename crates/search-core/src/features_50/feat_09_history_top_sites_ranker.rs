//! History Top Sites Ranker (`feat_09_history_top_sites_ranker`).
//!
//! Computes a deterministic, frecency-ranked list of top sites from browsing
//! history, respecting pinned grid slots, blocked sites/origins, host
//! normalization, and exponential time decay.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

const DEFAULT_MAX_SITES: usize = 8;
const MAX_ALLOWED_SITES: usize = 100;
const DEFAULT_HALF_LIFE_SECONDS: u64 = 7 * 86_400;
const MAX_DWELL_BONUS_SECONDS: u32 = 300;
const MAX_DWELL_BONUS_WEIGHT: f64 = 0.5;

/// Errors returned by top-sites validation, configuration, or ranking.
#[derive(Debug, Error)]
pub enum TopSitesError {
    #[error("invalid URL '{url}': {reason}")]
    InvalidUrl { url: String, reason: String },

    #[error("unsupported URL scheme '{scheme}'; expected http or https")]
    UnsupportedScheme { scheme: String },

    #[error("invalid configuration for '{field}': {reason}")]
    InvalidConfig { field: &'static str, reason: String },

    #[error("invalid timestamp: {reason}")]
    InvalidTimestamp { reason: String },

    #[error("pinned slot {slot_index} exceeds max_sites ({max_sites})")]
    InvalidPinnedSlot { slot_index: usize, max_sites: usize },

    #[error("duplicate pinned slot {slot_index}")]
    DuplicatePinnedSlot { slot_index: usize },

    #[error("duplicate pinned site key '{site_key}'")]
    DuplicatePinnedSite { site_key: String },

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// How a history visit was initiated, used to weight frecency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisitTransition {
    Typed,
    Bookmark,
    Link,
    Reload,
    Redirect,
}

/// Configuration controlling top-sites ranking, normalization, and decay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopSitesConfig {
    pub max_sites: usize,
    pub half_life_seconds: u64,
    pub typed_visit_weight: f64,
    pub bookmark_visit_weight: f64,
    pub link_visit_weight: f64,
    pub reload_visit_weight: f64,
    pub redirect_visit_weight: f64,
    pub min_visits: u32,
    pub group_by_host: bool,
    pub strip_www: bool,
    pub allow_http: bool,
}

impl Default for TopSitesConfig {
    fn default() -> Self {
        Self {
            max_sites: DEFAULT_MAX_SITES,
            half_life_seconds: DEFAULT_HALF_LIFE_SECONDS,
            typed_visit_weight: 2.0,
            bookmark_visit_weight: 1.5,
            link_visit_weight: 1.0,
            reload_visit_weight: 0.6,
            redirect_visit_weight: 0.25,
            min_visits: 1,
            group_by_host: true,
            strip_www: true,
            allow_http: true,
        }
    }
}

impl TopSitesConfig {
    /// Validates all configuration parameters.
    pub fn validate(&self) -> Result<(), TopSitesError> {
        if self.max_sites == 0 || self.max_sites > MAX_ALLOWED_SITES {
            return Err(TopSitesError::InvalidConfig {
                field: "max_sites",
                reason: format!("must be between 1 and {MAX_ALLOWED_SITES}"),
            });
        }
        if self.half_life_seconds == 0 {
            return Err(TopSitesError::InvalidConfig {
                field: "half_life_seconds",
                reason: "must be greater than 0".to_string(),
            });
        }
        let weights = [
            ("typed_visit_weight", self.typed_visit_weight),
            ("bookmark_visit_weight", self.bookmark_visit_weight),
            ("link_visit_weight", self.link_visit_weight),
            ("reload_visit_weight", self.reload_visit_weight),
            ("redirect_visit_weight", self.redirect_visit_weight),
        ];
        for (field, value) in weights {
            if !value.is_finite() || value < 0.0 {
                return Err(TopSitesError::InvalidConfig {
                    field,
                    reason: "must be a finite non-negative number".to_string(),
                });
            }
        }
        Ok(())
    }

    fn weight_for(&self, transition: VisitTransition) -> f64 {
        match transition {
            VisitTransition::Typed => self.typed_visit_weight,
            VisitTransition::Bookmark => self.bookmark_visit_weight,
            VisitTransition::Link => self.link_visit_weight,
            VisitTransition::Reload => self.reload_visit_weight,
            VisitTransition::Redirect => self.redirect_visit_weight,
        }
    }
}

/// A single browsing history visit record fed into the ranker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryVisitRecord {
    pub url: String,
    pub title: String,
    pub visited_at_unix: i64,
    pub transition: VisitTransition,
    pub dwell_seconds: u32,
    pub favicon_url: Option<String>,
}

/// A user-pinned site bound to a specific slot on the top-sites grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedSite {
    pub slot_index: usize,
    pub url: String,
    pub title: String,
    pub favicon_url: Option<String>,
    pub pinned_at_unix: i64,
}

/// A ranked entry in the resulting top-sites list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopSiteEntry {
    pub rank: usize,
    pub site_key: String,
    pub url: String,
    pub title: String,
    pub favicon_url: Option<String>,
    pub frecency_score: f64,
    pub visit_count: u32,
    pub typed_count: u32,
    pub last_visited_unix: i64,
    pub pinned: bool,
}

/// Serialized snapshot of ranked top sites and summary counters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopSitesSnapshot {
    pub generated_at_unix: i64,
    pub entries: Vec<TopSiteEntry>,
    pub total_candidates: usize,
    pub blocked_count: usize,
}

impl TopSitesSnapshot {
    /// Serializes the snapshot to compact JSON.
    pub fn to_json(&self) -> Result<String, TopSitesError> {
        Ok(serde_json::to_string(self)?)
    }

    /// Deserializes a snapshot from JSON.
    pub fn from_json(raw: &str) -> Result<Self, TopSitesError> {
        Ok(serde_json::from_str(raw)?)
    }
}

#[derive(Debug, Clone)]
struct SiteAccumulator {
    site_key: String,
    best_url: String,
    best_url_score: f64,
    best_title: String,
    favicon_url: Option<String>,
    frecency_score: f64,
    visit_count: u32,
    typed_count: u32,
    last_visited_unix: i64,
}

/// Stateful top-sites ranker holding configuration, pinned slots, and blocklist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TopSitesRanker {
    config: TopSitesConfig,
    pinned: Vec<PinnedSite>,
    blocked_keys: BTreeSet<String>,
}

impl TopSitesRanker {
    /// Creates a new ranker after validating `config`.
    pub fn new(config: TopSitesConfig) -> Result<Self, TopSitesError> {
        config.validate()?;
        Ok(Self {
            config,
            pinned: Vec::new(),
            blocked_keys: BTreeSet::new(),
        })
    }

    /// Returns the active configuration.
    pub fn config(&self) -> &TopSitesConfig {
        &self.config
    }

    /// Returns currently pinned sites.
    pub fn pinned_sites(&self) -> &[PinnedSite] {
        &self.pinned
    }

    /// Returns the normalized keys currently on the blocklist.
    pub fn blocked_keys(&self) -> &BTreeSet<String> {
        &self.blocked_keys
    }

    /// Parses and validates a URL against the ranker's scheme rules.
    pub fn parse_and_validate_url(&self, raw_url: &str) -> Result<Url, TopSitesError> {
        parse_and_validate_url_with_config(&self.config, raw_url)
    }

    /// Computes the canonical site key for a validated URL.
    pub fn normalize_site_key(&self, parsed: &Url) -> String {
        normalize_site_key_with_config(&self.config, parsed)
    }

    /// Pins a site to its requested grid slot.
    pub fn pin_site(&mut self, site: PinnedSite) -> Result<(), TopSitesError> {
        if site.slot_index >= self.config.max_sites {
            return Err(TopSitesError::InvalidPinnedSlot {
                slot_index: site.slot_index,
                max_sites: self.config.max_sites,
            });
        }
        if site.pinned_at_unix < 0 {
            return Err(TopSitesError::InvalidTimestamp {
                reason: "pinned_at_unix cannot be negative".to_string(),
            });
        }
        let parsed = self.parse_and_validate_url(&site.url)?;
        let key = self.normalize_site_key(&parsed);

        if self.pinned.iter().any(|p| p.slot_index == site.slot_index) {
            return Err(TopSitesError::DuplicatePinnedSlot {
                slot_index: site.slot_index,
            });
        }
        for existing in &self.pinned {
            if let Ok(existing_parsed) = self.parse_and_validate_url(&existing.url) {
                if self.normalize_site_key(&existing_parsed) == key {
                    return Err(TopSitesError::DuplicatePinnedSite { site_key: key });
                }
            }
        }

        self.blocked_keys.remove(&key);
        self.pinned.push(site);
        self.pinned.sort_by_key(|p| p.slot_index);
        Ok(())
    }

    /// Unpins any pinned site matching `raw_url`. Returns `true` if one was removed.
    pub fn unpin_url(&mut self, raw_url: &str) -> Result<bool, TopSitesError> {
        let parsed = self.parse_and_validate_url(raw_url)?;
        let key = self.normalize_site_key(&parsed);
        let before = self.pinned.len();
        let config = &self.config;
        self.pinned.retain(|p| {
            parse_and_validate_url_with_config(config, &p.url)
                .map(|u| normalize_site_key_with_config(config, &u) != key)
                .unwrap_or(true)
        });
        Ok(self.pinned.len() < before)
    }

    /// Adds a site to the blocklist and removes it from pinned sites if present.
    pub fn block_site(&mut self, raw_url: &str) -> Result<(), TopSitesError> {
        let parsed = self.parse_and_validate_url(raw_url)?;
        let key = self.normalize_site_key(&parsed);
        self.blocked_keys.insert(key);
        let _ = self.unpin_url(raw_url)?;
        Ok(())
    }

    /// Removes a site from the blocklist. Returns `true` if it was blocked.
    pub fn unblock_site(&mut self, raw_url: &str) -> Result<bool, TopSitesError> {
        let parsed = self.parse_and_validate_url(raw_url)?;
        let key = self.normalize_site_key(&parsed);
        Ok(self.blocked_keys.remove(&key))
    }

    /// Checks whether a URL's normalized site key is blocked.
    pub fn is_blocked(&self, raw_url: &str) -> Result<bool, TopSitesError> {
        let parsed = self.parse_and_validate_url(raw_url)?;
        let key = self.normalize_site_key(&parsed);
        Ok(self.blocked_keys.contains(&key))
    }

    /// Computes the decayed frecency score contribution of a single visit.
    pub fn compute_visit_score(
        &self,
        visit: &HistoryVisitRecord,
        now_unix: i64,
    ) -> Result<f64, TopSitesError> {
        if now_unix < 0 || visit.visited_at_unix < 0 {
            return Err(TopSitesError::InvalidTimestamp {
                reason: "timestamps must be non-negative unix seconds".to_string(),
            });
        }
        let _ = self.parse_and_validate_url(&visit.url)?;
        if let Some(ref fav) = visit.favicon_url {
            let _ = self.parse_and_validate_url(fav)?;
        }

        let age_seconds = (now_unix - visit.visited_at_unix).max(0) as f64;
        let half_life = self.config.half_life_seconds as f64;
        let decay = 0.5_f64.powf(age_seconds / half_life);

        let base_weight = self.config.weight_for(visit.transition);
        let capped_dwell = visit.dwell_seconds.min(MAX_DWELL_BONUS_SECONDS) as f64;
        let dwell_multiplier =
            1.0 + (capped_dwell / MAX_DWELL_BONUS_SECONDS as f64) * MAX_DWELL_BONUS_WEIGHT;

        Ok(base_weight * decay * dwell_multiplier)
    }

    /// Ranks history visits into a deterministic `TopSitesSnapshot`.
    pub fn rank(
        &self,
        visits: &[HistoryVisitRecord],
        now_unix: i64,
    ) -> Result<TopSitesSnapshot, TopSitesError> {
        if now_unix < 0 {
            return Err(TopSitesError::InvalidTimestamp {
                reason: "now_unix must be non-negative".to_string(),
            });
        }

        let mut pinned_by_slot: BTreeMap<usize, (&PinnedSite, String)> = BTreeMap::new();
        let mut pinned_keys = BTreeSet::new();
        for p in &self.pinned {
            if p.slot_index < self.config.max_sites {
                let parsed = self.parse_and_validate_url(&p.url)?;
                let key = self.normalize_site_key(&parsed);
                pinned_keys.insert(key.clone());
                pinned_by_slot.insert(p.slot_index, (p, key));
            }
        }

        let mut accumulators: BTreeMap<String, SiteAccumulator> = BTreeMap::new();
        let mut blocked_keys_seen = BTreeSet::new();

        for visit in visits {
            let parsed = self.parse_and_validate_url(&visit.url)?;
            let key = self.normalize_site_key(&parsed);
            let score = self.compute_visit_score(visit, now_unix)?;

            if self.blocked_keys.contains(&key) {
                blocked_keys_seen.insert(key);
                continue;
            }

            let entry = accumulators
                .entry(key.clone())
                .or_insert_with(|| SiteAccumulator {
                    site_key: key,
                    best_url: visit.url.trim().to_string(),
                    best_url_score: score,
                    best_title: fallback_title(&visit.title, &parsed),
                    favicon_url: visit.favicon_url.clone(),
                    frecency_score: 0.0,
                    visit_count: 0,
                    typed_count: 0,
                    last_visited_unix: visit.visited_at_unix,
                });

            entry.frecency_score += score;
            entry.visit_count = entry.visit_count.saturating_add(1);
            if visit.transition == VisitTransition::Typed {
                entry.typed_count = entry.typed_count.saturating_add(1);
            }

            if score > entry.best_url_score {
                entry.best_url_score = score;
                entry.best_url = visit.url.trim().to_string();
            }

            if visit.visited_at_unix >= entry.last_visited_unix {
                entry.last_visited_unix = visit.visited_at_unix;
                let trimmed_title = visit.title.trim();
                if !trimmed_title.is_empty() {
                    entry.best_title = trimmed_title.to_string();
                }
                if visit.favicon_url.is_some() {
                    entry.favicon_url = visit.favicon_url.clone();
                }
            }
        }

        let total_candidates = accumulators.len();

        let mut unpinned: Vec<SiteAccumulator> = accumulators
            .into_values()
            .filter(|acc| {
                acc.visit_count >= self.config.min_visits && !pinned_keys.contains(&acc.site_key)
            })
            .collect();

        unpinned.sort_by(|a, b| {
            b.frecency_score
                .total_cmp(&a.frecency_score)
                .then_with(|| b.last_visited_unix.cmp(&a.last_visited_unix))
                .then_with(|| a.site_key.cmp(&b.site_key))
        });

        let mut unpinned_iter = unpinned.into_iter();
        let mut entries = Vec::with_capacity(self.config.max_sites);

        for slot in 0..self.config.max_sites {
            if let Some((pinned, key)) = pinned_by_slot.get(&slot) {
                let stats = visits_summary_for_key(visits, self, key, now_unix);
                let title = if pinned.title.trim().is_empty() {
                    key.clone()
                } else {
                    pinned.title.trim().to_string()
                };
                entries.push(TopSiteEntry {
                    rank: slot,
                    site_key: key.clone(),
                    url: pinned.url.trim().to_string(),
                    title,
                    favicon_url: pinned.favicon_url.clone(),
                    frecency_score: stats.0,
                    visit_count: stats.1,
                    typed_count: stats.2,
                    last_visited_unix: stats.3.unwrap_or(pinned.pinned_at_unix),
                    pinned: true,
                });
            } else if let Some(acc) = unpinned_iter.next() {
                entries.push(TopSiteEntry {
                    rank: slot,
                    site_key: acc.site_key,
                    url: acc.best_url,
                    title: acc.best_title,
                    favicon_url: acc.favicon_url,
                    frecency_score: acc.frecency_score,
                    visit_count: acc.visit_count,
                    typed_count: acc.typed_count,
                    last_visited_unix: acc.last_visited_unix,
                    pinned: false,
                });
            } else {
                break;
            }
        }

        Ok(TopSitesSnapshot {
            generated_at_unix: now_unix,
            entries,
            total_candidates,
            blocked_count: blocked_keys_seen.len(),
        })
    }
}

fn parse_and_validate_url_with_config(
    config: &TopSitesConfig,
    raw_url: &str,
) -> Result<Url, TopSitesError> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return Err(TopSitesError::InvalidUrl {
            url: raw_url.to_string(),
            reason: "URL is empty".to_string(),
        });
    }
    let parsed = Url::parse(trimmed).map_err(|err| TopSitesError::InvalidUrl {
        url: raw_url.to_string(),
        reason: err.to_string(),
    })?;

    let scheme = parsed.scheme();
    let scheme_ok = match scheme {
        "https" => true,
        "http" => config.allow_http,
        _ => false,
    };
    if !scheme_ok {
        return Err(TopSitesError::UnsupportedScheme {
            scheme: scheme.to_string(),
        });
    }

    if parsed.host_str().is_none() {
        return Err(TopSitesError::InvalidUrl {
            url: raw_url.to_string(),
            reason: "URL must include a host".to_string(),
        });
    }

    Ok(parsed)
}

fn normalize_site_key_with_config(config: &TopSitesConfig, parsed: &Url) -> String {
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    let normalized_host = if config.strip_www {
        host.strip_prefix("www.").unwrap_or(&host).to_string()
    } else {
        host
    };

    if config.group_by_host {
        match parsed.port() {
            Some(port) => format!("{normalized_host}:{port}"),
            None => normalized_host,
        }
    } else {
        let mut path = parsed.path().trim_end_matches('/').to_string();
        if path.is_empty() {
            path.push('/');
        }
        match parsed.port() {
            Some(port) => format!("{normalized_host}:{port}{path}"),
            None => format!("{normalized_host}{path}"),
        }
    }
}

fn fallback_title(raw_title: &str, parsed: &Url) -> String {
    let trimmed = raw_title.trim();
    if trimmed.is_empty() {
        parsed.host_str().unwrap_or("site").to_string()
    } else {
        trimmed.to_string()
    }
}

fn visits_summary_for_key(
    visits: &[HistoryVisitRecord],
    ranker: &TopSitesRanker,
    target_key: &str,
    now_unix: i64,
) -> (f64, u32, u32, Option<i64>) {
    let mut score = 0.0;
    let mut count = 0u32;
    let mut typed = 0u32;
    let mut last_seen: Option<i64> = None;

    for visit in visits {
        if let Ok(parsed) = ranker.parse_and_validate_url(&visit.url) {
            if ranker.normalize_site_key(&parsed) == target_key {
                if let Ok(s) = ranker.compute_visit_score(visit, now_unix) {
                    score += s;
                }
                count = count.saturating_add(1);
                if visit.transition == VisitTransition::Typed {
                    typed = typed.saturating_add(1);
                }
                last_seen = Some(match last_seen {
                    Some(prev) => prev.max(visit.visited_at_unix),
                    None => visit.visited_at_unix,
                });
            }
        }
    }

    (score, count, typed, last_seen)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visit(
        url: &str,
        title: &str,
        visited_at_unix: i64,
        transition: VisitTransition,
        dwell_seconds: u32,
    ) -> HistoryVisitRecord {
        HistoryVisitRecord {
            url: url.to_string(),
            title: title.to_string(),
            visited_at_unix,
            transition,
            dwell_seconds,
            favicon_url: None,
        }
    }

    #[test]
    fn ranks_by_frecency_decay_and_groups_www_hosts() {
        let now = 1_700_000_000;
        let ranker = TopSitesRanker::default();
        let visits = vec![
            // Older single link visit to example.org (21 days ago -> 3 half-lives)
            visit(
                "https://example.org/docs",
                "Example Docs",
                now - 21 * 86_400,
                VisitTransition::Link,
                30,
            ),
            // Two recent typed visits to rust-lang.org (with and without www)
            visit(
                "https://www.rust-lang.org/",
                "Rust",
                now - 3_600,
                VisitTransition::Typed,
                120,
            ),
            visit(
                "https://rust-lang.org/learn",
                "Learn Rust",
                now - 1_800,
                VisitTransition::Typed,
                240,
            ),
            // One recent bookmark visit to docs.rs
            visit(
                "https://docs.rs/",
                "Docs.rs",
                now - 7_200,
                VisitTransition::Bookmark,
                60,
            ),
        ];

        let snapshot = ranker.rank(&visits, now).expect("ranking should succeed");
        assert_eq!(snapshot.total_candidates, 3);
        assert_eq!(snapshot.blocked_count, 0);
        assert_eq!(snapshot.entries.len(), 3);

        assert_eq!(snapshot.entries[0].site_key, "rust-lang.org");
        assert_eq!(snapshot.entries[0].visit_count, 2);
        assert_eq!(snapshot.entries[0].typed_count, 2);
        assert_eq!(snapshot.entries[0].title, "Learn Rust");

        assert_eq!(snapshot.entries[1].site_key, "docs.rs");
        assert_eq!(snapshot.entries[2].site_key, "example.org");
        assert!(snapshot.entries[0].frecency_score > snapshot.entries[1].frecency_score);
        assert!(snapshot.entries[1].frecency_score > snapshot.entries[2].frecency_score);
    }

    #[test]
    fn pinned_slots_and_blocklist_are_respected_without_duplicates() {
        let now = 1_700_000_000;
        let config = TopSitesConfig {
            max_sites: 4,
            ..TopSitesConfig::default()
        };
        let mut ranker = TopSitesRanker::new(config).expect("valid config");

        ranker
            .pin_site(PinnedSite {
                slot_index: 1,
                url: "https://www.mozilla.org/".to_string(),
                title: "Mozilla".to_string(),
                favicon_url: None,
                pinned_at_unix: now - 10_000,
            })
            .expect("pin should succeed");

        ranker
            .block_site("https://spam.example.com/ad")
            .expect("block should succeed");

        let visits = vec![
            visit(
                "https://spam.example.com/landing",
                "Spam",
                now - 10,
                VisitTransition::Typed,
                300,
            ),
            visit(
                "https://mozilla.org/en-US/",
                "Mozilla Home",
                now - 50,
                VisitTransition::Typed,
                100,
            ),
            visit(
                "https://kernel.org/",
                "Linux Kernel",
                now - 100,
                VisitTransition::Typed,
                100,
            ),
            visit(
                "https://gnome.org/",
                "GNOME",
                now - 200,
                VisitTransition::Link,
                60,
            ),
        ];

        let snapshot = ranker.rank(&visits, now).expect("ranking should succeed");
        assert_eq!(snapshot.blocked_count, 1);
        assert_eq!(snapshot.entries.len(), 3);

        // Slot 0 gets highest unpinned (kernel.org), slot 1 is pinned mozilla.org,
        // slot 2 gets gnome.org. mozilla.org must not appear twice.
        assert_eq!(snapshot.entries[0].site_key, "kernel.org");
        assert!(!snapshot.entries[0].pinned);

        assert_eq!(snapshot.entries[1].rank, 1);
        assert_eq!(snapshot.entries[1].site_key, "mozilla.org");
        assert!(snapshot.entries[1].pinned);
        assert_eq!(snapshot.entries[1].visit_count, 1);

        assert_eq!(snapshot.entries[2].site_key, "gnome.org");
        assert!(!snapshot.entries[2].pinned);
    }

    #[test]
    fn rejects_invalid_configs_urls_and_roundtrips_json() {
        let bad_config = TopSitesConfig {
            max_sites: 0,
            ..TopSitesConfig::default()
        };
        assert!(matches!(
            TopSitesRanker::new(bad_config),
            Err(TopSitesError::InvalidConfig {
                field: "max_sites",
                ..
            })
        ));

        let mut ranker = TopSitesRanker::new(TopSitesConfig {
            allow_http: false,
            ..TopSitesConfig::default()
        })
        .expect("valid config");

        assert!(matches!(
            ranker.parse_and_validate_url("http://insecure.example"),
            Err(TopSitesError::UnsupportedScheme { .. })
        ));
        assert!(matches!(
            ranker.parse_and_validate_url("javascript:alert(1)"),
            Err(TopSitesError::UnsupportedScheme { .. })
        ));
        assert!(matches!(
            ranker.pin_site(PinnedSite {
                slot_index: 99,
                url: "https://example.com".to_string(),
                title: "Too High".to_string(),
                favicon_url: None,
                pinned_at_unix: 100,
            }),
            Err(TopSitesError::InvalidPinnedSlot { .. })
        ));

        let snapshot = ranker
            .rank(
                &[visit(
                    "https://archlinux.org/",
                    "Arch Linux",
                    1_700_000_000,
                    VisitTransition::Typed,
                    45,
                )],
                1_700_000_100,
            )
            .expect("valid rank");
        let json = snapshot.to_json().expect("serialize");
        let decoded = TopSitesSnapshot::from_json(&json).expect("deserialize");
        assert_eq!(snapshot, decoded);
    }
}
