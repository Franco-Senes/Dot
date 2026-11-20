use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;

use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_MAX_SNAPSHOTS: usize = 64;
pub const DEFAULT_MAX_AUTO_SAVES: usize = 16;
pub const DEFAULT_MAX_TABS_PER_SNAPSHOT: usize = 2000;

#[derive(Debug, Error)]
pub enum SnapshotError {
    #[error("snapshot id must not be empty")]
    EmptySnapshotId,
    #[error("snapshot name must not be empty")]
    EmptyName,
    #[error("tab title must not be empty for tab {0}")]
    EmptyTabTitle(u64),
    #[error("invalid unix timestamp: {0}")]
    InvalidTimestamp(i64),
    #[error("invalid tab url '{url}': {reason}")]
    InvalidUrl { url: String, reason: String },
    #[error("duplicate window id: {0}")]
    DuplicateWindowId(u64),
    #[error("duplicate tab id: {0}")]
    DuplicateTabId(u64),
    #[error("active tab id {tab_id} not found in window {window_id}")]
    ActiveTabNotInWindow { window_id: u64, tab_id: u64 },
    #[error("snapshot '{0}' not found")]
    SnapshotNotFound(String),
    #[error("snapshot '{0}' already exists")]
    DuplicateSnapshotId(String),
    #[error("unsupported schema version: {0}")]
    UnsupportedSchemaVersion(u32),
    #[error("too many tabs in snapshot: {actual} exceeds limit {max}")]
    TooManyTabs { max: usize, actual: usize },
    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotKind {
    Manual,
    AutoSave,
    PreUpdate,
    CrashRecovery,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

impl Default for WindowGeometry {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            width: 1280,
            height: 800,
            maximized: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabSnapshot {
    pub id: u64,
    pub url: String,
    pub title: String,
    pub pinned: bool,
    pub muted: bool,
    pub scroll_y: u32,
    pub last_accessed_unix_secs: i64,
    pub group_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowSnapshot {
    pub id: u64,
    pub workspace: Option<String>,
    pub geometry: WindowGeometry,
    pub active_tab_id: Option<u64>,
    pub tabs: Vec<TabSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub kind: SnapshotKind,
    pub created_at_unix_secs: i64,
    pub windows: Vec<WindowSnapshot>,
    pub tags: Vec<String>,
    pub notes: Option<String>,
    pub checksum_sha256: String,
}

#[derive(Serialize)]
struct SnapshotPayloadRef<'a> {
    schema_version: u32,
    id: &'a str,
    name: &'a str,
    kind: SnapshotKind,
    created_at_unix_secs: i64,
    windows: &'a [WindowSnapshot],
    tags: &'a [String],
    notes: &'a Option<String>,
}

impl SessionSnapshot {
    pub fn total_tabs(&self) -> usize {
        self.windows.iter().map(|w| w.tabs.len()).sum()
    }

    pub fn compute_checksum(&self) -> Result<String, SnapshotError> {
        let payload = SnapshotPayloadRef {
            schema_version: self.schema_version,
            id: &self.id,
            name: &self.name,
            kind: self.kind,
            created_at_unix_secs: self.created_at_unix_secs,
            windows: &self.windows,
            tags: &self.tags,
            notes: &self.notes,
        };
        let bytes = serde_json::to_vec(&payload)?;
        let digest_val = digest(&SHA256, &bytes);
        Ok(hex_encode(digest_val.as_ref()))
    }

    pub fn verify_checksum(&self) -> Result<(), SnapshotError> {
        let actual = self.compute_checksum()?;
        if actual != self.checksum_sha256 {
            return Err(SnapshotError::ChecksumMismatch {
                expected: self.checksum_sha256.clone(),
                actual,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotDiff {
    pub added_tabs: Vec<TabSnapshot>,
    pub removed_tabs: Vec<TabSnapshot>,
    pub unchanged_tab_count: usize,
    pub window_count_delta: isize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotRetentionPolicy {
    pub max_snapshots: usize,
    pub max_auto_saves: usize,
    pub max_tabs_per_snapshot: usize,
}

impl Default for SnapshotRetentionPolicy {
    fn default() -> Self {
        Self {
            max_snapshots: DEFAULT_MAX_SNAPSHOTS,
            max_auto_saves: DEFAULT_MAX_AUTO_SAVES,
            max_tabs_per_snapshot: DEFAULT_MAX_TABS_PER_SNAPSHOT,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SnapshotStore {
    policy: SnapshotRetentionPolicy,
    snapshots: BTreeMap<String, SessionSnapshot>,
}

impl SnapshotStore {
    pub fn new(policy: SnapshotRetentionPolicy) -> Self {
        Self {
            policy,
            snapshots: BTreeMap::new(),
        }
    }

    pub fn policy(&self) -> &SnapshotRetentionPolicy {
        &self.policy
    }

    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn capture(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        kind: SnapshotKind,
        created_at_unix_secs: i64,
        windows: Vec<WindowSnapshot>,
        tags: Vec<String>,
        notes: Option<String>,
    ) -> Result<&SessionSnapshot, SnapshotError> {
        let id = id.into();
        if self.snapshots.contains_key(&id) {
            return Err(SnapshotError::DuplicateSnapshotId(id));
        }

        let mut snapshot = SessionSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            id,
            name: name.into(),
            kind,
            created_at_unix_secs,
            windows,
            tags: normalize_tags(tags),
            notes,
            checksum_sha256: String::new(),
        };

        validate_snapshot(&snapshot, self.policy.max_tabs_per_snapshot)?;
        snapshot.checksum_sha256 = snapshot.compute_checksum()?;

        let key = snapshot.id.clone();
        self.snapshots.insert(key.clone(), snapshot);
        self.prune_retention();

        self.snapshots
            .get(&key)
            .ok_or(SnapshotError::SnapshotNotFound(key))
    }

    pub fn get(&self, id: &str) -> Option<&SessionSnapshot> {
        self.snapshots.get(id)
    }

    pub fn list(&self) -> Vec<&SessionSnapshot> {
        let mut items: Vec<&SessionSnapshot> = self.snapshots.values().collect();
        items.sort_by(|a, b| {
            b.created_at_unix_secs
                .cmp(&a.created_at_unix_secs)
                .then_with(|| a.id.cmp(&b.id))
        });
        items
    }

    pub fn list_by_kind(&self, kind: SnapshotKind) -> Vec<&SessionSnapshot> {
        self.list()
            .into_iter()
            .filter(|snap| snap.kind == kind)
            .collect()
    }

    pub fn search(&self, query: &str) -> Vec<&SessionSnapshot> {
        let q = query.trim().to_ascii_lowercase();
        if q.is_empty() {
            return self.list();
        }
        self.list()
            .into_iter()
            .filter(|snap| snapshot_matches_query(snap, &q))
            .collect()
    }

    pub fn delete(&mut self, id: &str) -> Result<SessionSnapshot, SnapshotError> {
        self.snapshots
            .remove(id)
            .ok_or_else(|| SnapshotError::SnapshotNotFound(id.to_string()))
    }

    pub fn diff(&self, older_id: &str, newer_id: &str) -> Result<SnapshotDiff, SnapshotError> {
        let older = self
            .snapshots
            .get(older_id)
            .ok_or_else(|| SnapshotError::SnapshotNotFound(older_id.to_string()))?;
        let newer = self
            .snapshots
            .get(newer_id)
            .ok_or_else(|| SnapshotError::SnapshotNotFound(newer_id.to_string()))?;

        let older_urls: HashSet<&str> = older
            .windows
            .iter()
            .flat_map(|w| w.tabs.iter().map(|t| t.url.as_str()))
            .collect();
        let newer_urls: HashSet<&str> = newer
            .windows
            .iter()
            .flat_map(|w| w.tabs.iter().map(|t| t.url.as_str()))
            .collect();

        let added_tabs: Vec<TabSnapshot> = newer
            .windows
            .iter()
            .flat_map(|w| w.tabs.iter())
            .filter(|t| !older_urls.contains(t.url.as_str()))
            .cloned()
            .collect();

        let removed_tabs: Vec<TabSnapshot> = older
            .windows
            .iter()
            .flat_map(|w| w.tabs.iter())
            .filter(|t| !newer_urls.contains(t.url.as_str()))
            .cloned()
            .collect();

        let unchanged_tab_count = newer
            .windows
            .iter()
            .flat_map(|w| w.tabs.iter())
            .filter(|t| older_urls.contains(t.url.as_str()))
            .count();

        let window_count_delta =
            (newer.windows.len() as isize).saturating_sub(older.windows.len() as isize);

        Ok(SnapshotDiff {
            added_tabs,
            removed_tabs,
            unchanged_tab_count,
            window_count_delta,
        })
    }

    pub fn export_json(&self, id: &str) -> Result<String, SnapshotError> {
        let snap = self
            .snapshots
            .get(id)
            .ok_or_else(|| SnapshotError::SnapshotNotFound(id.to_string()))?;
        Ok(serde_json::to_string_pretty(snap)?)
    }

    pub fn import_json(&mut self, json: &str) -> Result<&SessionSnapshot, SnapshotError> {
        let snapshot: SessionSnapshot = serde_json::from_str(json)?;
        if self.snapshots.contains_key(&snapshot.id) {
            return Err(SnapshotError::DuplicateSnapshotId(snapshot.id));
        }
        validate_snapshot(&snapshot, self.policy.max_tabs_per_snapshot)?;
        snapshot.verify_checksum()?;

        let key = snapshot.id.clone();
        self.snapshots.insert(key.clone(), snapshot);
        self.prune_retention();

        self.snapshots
            .get(&key)
            .ok_or(SnapshotError::SnapshotNotFound(key))
    }

    fn prune_retention(&mut self) {
        if self.policy.max_auto_saves > 0 {
            loop {
                let auto_ids = self.sorted_ids_oldest_first(Some(SnapshotKind::AutoSave));
                if auto_ids.len() <= self.policy.max_auto_saves {
                    break;
                }
                if let Some(oldest) = auto_ids.first() {
                    self.snapshots.remove(oldest);
                } else {
                    break;
                }
            }
        } else {
            let auto_ids = self.sorted_ids_oldest_first(Some(SnapshotKind::AutoSave));
            for id in auto_ids {
                self.snapshots.remove(&id);
            }
        }

        if self.policy.max_snapshots == 0 {
            self.snapshots.clear();
            return;
        }

        while self.snapshots.len() > self.policy.max_snapshots {
            let auto_oldest = self.sorted_ids_oldest_first(Some(SnapshotKind::AutoSave));
            if let Some(id) = auto_oldest.first() {
                self.snapshots.remove(id);
                continue;
            }
            let all_oldest = self.sorted_ids_oldest_first(None);
            if let Some(id) = all_oldest.first() {
                self.snapshots.remove(id);
            } else {
                break;
            }
        }
    }

    fn sorted_ids_oldest_first(&self, kind_filter: Option<SnapshotKind>) -> Vec<String> {
        let mut items: Vec<&SessionSnapshot> = self
            .snapshots
            .values()
            .filter(|s| kind_filter.is_none_or(|k| s.kind == k))
            .collect();
        items.sort_by(|a, b| {
            a.created_at_unix_secs
                .cmp(&b.created_at_unix_secs)
                .then_with(|| a.id.cmp(&b.id))
        });
        items.into_iter().map(|s| s.id.clone()).collect()
    }
}

pub fn validate_snapshot(snapshot: &SessionSnapshot, max_tabs: usize) -> Result<(), SnapshotError> {
    if snapshot.schema_version != SNAPSHOT_SCHEMA_VERSION {
        return Err(SnapshotError::UnsupportedSchemaVersion(
            snapshot.schema_version,
        ));
    }
    if snapshot.id.trim().is_empty() {
        return Err(SnapshotError::EmptySnapshotId);
    }
    if snapshot.name.trim().is_empty() {
        return Err(SnapshotError::EmptyName);
    }
    if snapshot.created_at_unix_secs < 0 {
        return Err(SnapshotError::InvalidTimestamp(
            snapshot.created_at_unix_secs,
        ));
    }

    let total_tabs = snapshot.total_tabs();
    if total_tabs > max_tabs {
        return Err(SnapshotError::TooManyTabs {
            max: max_tabs,
            actual: total_tabs,
        });
    }

    let mut seen_windows = HashSet::new();
    let mut seen_tabs = HashSet::new();

    for window in &snapshot.windows {
        if !seen_windows.insert(window.id) {
            return Err(SnapshotError::DuplicateWindowId(window.id));
        }

        let mut window_tab_ids = HashSet::new();
        for tab in &window.tabs {
            if !seen_tabs.insert(tab.id) {
                return Err(SnapshotError::DuplicateTabId(tab.id));
            }
            window_tab_ids.insert(tab.id);

            if tab.title.trim().is_empty() {
                return Err(SnapshotError::EmptyTabTitle(tab.id));
            }
            if tab.last_accessed_unix_secs < 0 {
                return Err(SnapshotError::InvalidTimestamp(tab.last_accessed_unix_secs));
            }
            validate_tab_url(&tab.url)?;
        }

        if let Some(active_id) = window.active_tab_id {
            if !window_tab_ids.contains(&active_id) {
                return Err(SnapshotError::ActiveTabNotInWindow {
                    window_id: window.id,
                    tab_id: active_id,
                });
            }
        }
    }

    Ok(())
}

fn validate_tab_url(raw_url: &str) -> Result<(), SnapshotError> {
    let parsed = Url::parse(raw_url).map_err(|err| SnapshotError::InvalidUrl {
        url: raw_url.to_string(),
        reason: err.to_string(),
    })?;
    match parsed.scheme() {
        "http" | "https" | "about" | "file" | "search" => Ok(()),
        other => Err(SnapshotError::InvalidUrl {
            url: raw_url.to_string(),
            reason: format!("unsupported url scheme '{other}'"),
        }),
    }
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for tag in tags {
        let trimmed = tag.trim().to_ascii_lowercase();
        if !trimmed.is_empty() && seen.insert(trimmed.clone()) {
            out.push(trimmed);
        }
    }
    out
}

fn snapshot_matches_query(snap: &SessionSnapshot, q_lower: &str) -> bool {
    if snap.name.to_ascii_lowercase().contains(q_lower)
        || snap.id.to_ascii_lowercase().contains(q_lower)
        || snap.tags.iter().any(|t| t.contains(q_lower))
    {
        return true;
    }
    snap.windows.iter().any(|w| {
        w.tabs.iter().any(|t| {
            t.title.to_ascii_lowercase().contains(q_lower)
                || t.url.to_ascii_lowercase().contains(q_lower)
        })
    })
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tab(id: u64, url: &str, title: &str) -> TabSnapshot {
        TabSnapshot {
            id,
            url: url.to_string(),
            title: title.to_string(),
            pinned: false,
            muted: false,
            scroll_y: 0,
            last_accessed_unix_secs: 1_700_000_000,
            group_name: None,
        }
    }

    fn sample_window(id: u64, tabs: Vec<TabSnapshot>, active: Option<u64>) -> WindowSnapshot {
        WindowSnapshot {
            id,
            workspace: Some("dev".to_string()),
            geometry: WindowGeometry::default(),
            active_tab_id: active,
            tabs,
        }
    }

    #[test]
    fn capture_export_import_and_verify_checksum() {
        let mut store = SnapshotStore::new(SnapshotRetentionPolicy::default());
        let win = sample_window(
            1,
            vec![
                sample_tab(10, "https://example.org/docs", "Docs"),
                sample_tab(11, "https://rust-lang.org/", "Rust"),
            ],
            Some(10),
        );

        let snap = store
            .capture(
                "snap-1",
                "Work Session",
                SnapshotKind::Manual,
                1_700_000_100,
                vec![win],
                vec!["Work".to_string(), "work".to_string(), "Rust".to_string()],
                Some("Before lunch".to_string()),
            )
            .expect("capture should succeed");

        assert_eq!(snap.total_tabs(), 2);
        assert_eq!(snap.tags, vec!["work".to_string(), "rust".to_string()]);
        assert!(snap.verify_checksum().is_ok());
        let expected_checksum = snap.checksum_sha256.clone();

        let exported = store.export_json("snap-1").expect("export should succeed");

        let mut store2 = SnapshotStore::new(SnapshotRetentionPolicy::default());
        let imported = store2
            .import_json(&exported)
            .expect("import should succeed");
        assert_eq!(imported.id, "snap-1");
        assert_eq!(imported.checksum_sha256, expected_checksum);

        // Tamper with JSON and verify checksum fails
        let mut store3 = SnapshotStore::new(SnapshotRetentionPolicy::default());
        let tampered = exported.replace("Work Session", "Tampered Session");
        let err = store3
            .import_json(&tampered)
            .expect_err("tampered json must fail checksum");
        assert!(matches!(err, SnapshotError::ChecksumMismatch { .. }));
    }

    #[test]
    fn validation_rejects_bad_urls_duplicates_and_missing_active_tab() {
        let mut store = SnapshotStore::new(SnapshotRetentionPolicy::default());

        let bad_url_win = sample_window(
            1,
            vec![sample_tab(1, "javascript:alert(1)", "Bad")],
            Some(1),
        );
        let err = store
            .capture(
                "s1",
                "Bad URL",
                SnapshotKind::Manual,
                100,
                vec![bad_url_win],
                vec![],
                None,
            )
            .expect_err("unsupported scheme must fail");
        assert!(matches!(err, SnapshotError::InvalidUrl { .. }));

        let dup_tab_win = sample_window(
            1,
            vec![
                sample_tab(5, "https://a.example/", "A"),
                sample_tab(5, "https://b.example/", "B"),
            ],
            Some(5),
        );
        let err = store
            .capture(
                "s2",
                "Dup Tab",
                SnapshotKind::Manual,
                100,
                vec![dup_tab_win],
                vec![],
                None,
            )
            .expect_err("duplicate tab id must fail");
        assert!(matches!(err, SnapshotError::DuplicateTabId(5)));

        let bad_active_win =
            sample_window(2, vec![sample_tab(7, "https://a.example/", "A")], Some(999));
        let err = store
            .capture(
                "s3",
                "Missing Active",
                SnapshotKind::Manual,
                100,
                vec![bad_active_win],
                vec![],
                None,
            )
            .expect_err("missing active tab id must fail");
        assert!(matches!(
            err,
            SnapshotError::ActiveTabNotInWindow {
                window_id: 2,
                tab_id: 999
            }
        ));
    }

    #[test]
    fn diff_search_and_retention_pruning() {
        let policy = SnapshotRetentionPolicy {
            max_snapshots: 3,
            max_auto_saves: 2,
            max_tabs_per_snapshot: 50,
        };
        let mut store = SnapshotStore::new(policy);

        store
            .capture(
                "auto-1",
                "Auto 1",
                SnapshotKind::AutoSave,
                100,
                vec![sample_window(
                    1,
                    vec![
                        sample_tab(1, "https://one.example/", "One"),
                        sample_tab(2, "https://two.example/", "Two"),
                    ],
                    Some(1),
                )],
                vec![],
                None,
            )
            .expect("auto-1");

        store
            .capture(
                "manual-1",
                "Important Manual",
                SnapshotKind::Manual,
                200,
                vec![sample_window(
                    1,
                    vec![
                        sample_tab(1, "https://one.example/", "One"),
                        sample_tab(3, "https://three.example/", "Three"),
                    ],
                    Some(3),
                )],
                vec!["milestone".to_string()],
                None,
            )
            .expect("manual-1");

        let diff = store
            .diff("auto-1", "manual-1")
            .expect("diff should succeed");
        assert_eq!(diff.added_tabs.len(), 1);
        assert_eq!(diff.added_tabs[0].url, "https://three.example/");
        assert_eq!(diff.removed_tabs.len(), 1);
        assert_eq!(diff.removed_tabs[0].url, "https://two.example/");
        assert_eq!(diff.unchanged_tab_count, 1);
        assert_eq!(diff.window_count_delta, 0);

        store
            .capture(
                "auto-2",
                "Auto 2",
                SnapshotKind::AutoSave,
                300,
                vec![sample_window(
                    1,
                    vec![sample_tab(4, "https://four.example/", "Four")],
                    Some(4),
                )],
                vec![],
                None,
            )
            .expect("auto-2");

        store
            .capture(
                "auto-3",
                "Auto 3",
                SnapshotKind::AutoSave,
                400,
                vec![sample_window(
                    1,
                    vec![sample_tab(5, "https://five.example/", "Five")],
                    Some(5),
                )],
                vec![],
                None,
            )
            .expect("auto-3");

        // max_auto_saves = 2 should have evicted auto-1 while keeping manual-1, auto-2, auto-3
        assert_eq!(store.len(), 3);
        assert!(store.get("auto-1").is_none());
        assert!(store.get("manual-1").is_some());
        assert!(store.get("auto-2").is_some());
        assert!(store.get("auto-3").is_some());

        let search_hits = store.search("milestone");
        assert_eq!(search_hits.len(), 1);
        assert_eq!(search_hits[0].id, "manual-1");
    }
}
