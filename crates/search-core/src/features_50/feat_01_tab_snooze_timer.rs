use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

/// Maximum snooze duration: 365 days in seconds.
pub const DEFAULT_MAX_DURATION_SECS: u64 = 365 * 24 * 60 * 60;

/// Default maximum number of concurrently active (scheduled or due) snoozed tabs.
pub const DEFAULT_MAX_ACTIVE_SNOOZES: usize = 500;

/// Default retention period for completed (woken or cancelled) entries before pruning.
pub const DEFAULT_AUTO_PRUNE_AFTER_SECS: u64 = 7 * 24 * 60 * 60;

const SECS_PER_HOUR: u64 = 3_600;
const SECS_PER_DAY: u64 = 86_400;
const SECS_PER_WEEK: u64 = 7 * SECS_PER_DAY;

/// Errors returned by tab snooze timer operations.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SnoozeError {
    #[error("tab_id must not be empty")]
    EmptyTabId,

    #[error("tab title must not be empty")]
    EmptyTitle,

    #[error("invalid tab URL: {0}")]
    InvalidUrl(String),

    #[error("snooze duration must be greater than zero")]
    ZeroDuration,

    #[error("wake timestamp {wake_at_epoch_secs} is not in the future (now={now_epoch_secs})")]
    WakeTimeInPast {
        wake_at_epoch_secs: u64,
        now_epoch_secs: u64,
    },

    #[error("snooze duration {duration_secs}s exceeds maximum allowed {max_secs}s")]
    DurationTooLong { duration_secs: u64, max_secs: u64 },

    #[error("snooze entry not found: {0}")]
    SnoozeNotFound(String),

    #[error("tab '{0}' already has an active snooze")]
    AlreadySnoozed(String),

    #[error("invalid state transition for '{id}': {from:?} -> {to:?}")]
    InvalidStateTransition {
        id: String,
        from: SnoozeStatus,
        to: SnoozeStatus,
    },

    #[error("active snooze limit reached (max={max})")]
    MaxSnoozedLimitReached { max: usize },

    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Lifecycle state of a snoozed tab entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnoozeStatus {
    Scheduled,
    Due,
    Woken,
    Cancelled,
}

impl SnoozeStatus {
    /// Returns true if the entry is still waiting to wake or awaiting user/UI wake handling.
    pub fn is_active(self) -> bool {
        matches!(self, Self::Scheduled | Self::Due)
    }
}

/// Optional recurrence policy when a snoozed tab wakes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SnoozeRepeat {
    #[default]
    Once,
    Daily,
    Weekly,
}

/// Preset or explicit schedule used to compute a wake timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnoozePreset {
    /// Wake in 3 hours.
    LaterToday,
    /// Wake in 8 hours.
    ThisEvening,
    /// Wake in 24 hours.
    Tomorrow,
    /// Wake in 7 days.
    NextWeek,
    /// Wake after a custom number of seconds from `now`.
    CustomDurationSecs(u64),
    /// Wake at an explicit Unix epoch timestamp in seconds.
    AtTimestamp(u64),
}

/// Configuration limits for [`TabSnoozeTimer`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnoozeConfig {
    pub max_active_snoozes: usize,
    pub max_duration_secs: u64,
    pub auto_prune_after_secs: u64,
}

impl Default for SnoozeConfig {
    fn default() -> Self {
        Self {
            max_active_snoozes: DEFAULT_MAX_ACTIVE_SNOOZES,
            max_duration_secs: DEFAULT_MAX_DURATION_SECS,
            auto_prune_after_secs: DEFAULT_AUTO_PRUNE_AFTER_SECS,
        }
    }
}

/// Input request to snooze a browser tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnoozeRequest {
    pub tab_id: String,
    pub url: String,
    pub title: String,
    pub workspace_id: Option<String>,
    pub note: Option<String>,
    pub preset: SnoozePreset,
    pub repeat: SnoozeRepeat,
}

/// Persisted representation of a snoozed tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnoozedTab {
    pub id: String,
    pub tab_id: String,
    pub url: String,
    pub title: String,
    pub workspace_id: Option<String>,
    pub note: Option<String>,
    pub created_at_epoch_secs: u64,
    pub wake_at_epoch_secs: u64,
    pub updated_at_epoch_secs: u64,
    pub snooze_count: u32,
    pub repeat: SnoozeRepeat,
    pub status: SnoozeStatus,
}

impl SnoozedTab {
    /// Seconds remaining until `wake_at_epoch_secs` relative to `now_epoch_secs`.
    pub fn remaining_secs(&self, now_epoch_secs: u64) -> u64 {
        self.wake_at_epoch_secs.saturating_sub(now_epoch_secs)
    }

    /// Returns true if this entry is active and its wake time has been reached.
    pub fn is_due(&self, now_epoch_secs: u64) -> bool {
        self.status.is_active() && now_epoch_secs >= self.wake_at_epoch_secs
    }
}

/// Resolves a [`SnoozePreset`] into a validated future Unix epoch timestamp (in seconds).
pub fn resolve_wake_timestamp(
    preset: SnoozePreset,
    now_epoch_secs: u64,
    max_duration_secs: u64,
) -> Result<u64, SnoozeError> {
    let duration_secs = match preset {
        SnoozePreset::LaterToday => 3 * SECS_PER_HOUR,
        SnoozePreset::ThisEvening => 8 * SECS_PER_HOUR,
        SnoozePreset::Tomorrow => SECS_PER_DAY,
        SnoozePreset::NextWeek => SECS_PER_WEEK,
        SnoozePreset::CustomDurationSecs(secs) => {
            if secs == 0 {
                return Err(SnoozeError::ZeroDuration);
            }
            secs
        }
        SnoozePreset::AtTimestamp(ts) => {
            if ts <= now_epoch_secs {
                return Err(SnoozeError::WakeTimeInPast {
                    wake_at_epoch_secs: ts,
                    now_epoch_secs,
                });
            }
            ts - now_epoch_secs
        }
    };

    if duration_secs > max_duration_secs {
        return Err(SnoozeError::DurationTooLong {
            duration_secs,
            max_secs: max_duration_secs,
        });
    }

    now_epoch_secs
        .checked_add(duration_secs)
        .ok_or(SnoozeError::DurationTooLong {
            duration_secs,
            max_secs: max_duration_secs,
        })
}

fn validate_tab_url(raw: &str) -> Result<String, SnoozeError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(SnoozeError::InvalidUrl("empty URL".to_string()));
    }
    let parsed = Url::parse(trimmed).map_err(|err| SnoozeError::InvalidUrl(err.to_string()))?;
    match parsed.scheme() {
        "http" | "https" | "file" | "about" => Ok(parsed.to_string()),
        other => Err(SnoozeError::InvalidUrl(format!(
            "unsupported scheme '{other}'"
        ))),
    }
}

/// Stateful manager for scheduling, polling, waking, resnoozing, and serializing snoozed tabs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabSnoozeTimer {
    config: SnoozeConfig,
    next_seq: u64,
    entries: BTreeMap<String, SnoozedTab>,
}

impl Default for TabSnoozeTimer {
    fn default() -> Self {
        Self::new(SnoozeConfig::default())
    }
}

impl TabSnoozeTimer {
    /// Creates an empty timer with the given configuration.
    pub fn new(config: SnoozeConfig) -> Self {
        Self {
            config,
            next_seq: 1,
            entries: BTreeMap::new(),
        }
    }

    /// Returns the active configuration.
    pub fn config(&self) -> &SnoozeConfig {
        &self.config
    }

    /// Schedules a tab to be snoozed according to `req`.
    pub fn schedule_tab(
        &mut self,
        req: SnoozeRequest,
        now_epoch_secs: u64,
    ) -> Result<SnoozedTab, SnoozeError> {
        let tab_id = req.tab_id.trim();
        if tab_id.is_empty() {
            return Err(SnoozeError::EmptyTabId);
        }

        let title = req.title.trim();
        if title.is_empty() {
            return Err(SnoozeError::EmptyTitle);
        }

        let normalized_url = validate_tab_url(&req.url)?;

        if self
            .entries
            .values()
            .any(|e| e.status.is_active() && e.tab_id == tab_id)
        {
            return Err(SnoozeError::AlreadySnoozed(tab_id.to_string()));
        }

        let active_count = self
            .entries
            .values()
            .filter(|e| e.status.is_active())
            .count();
        if active_count >= self.config.max_active_snoozes {
            return Err(SnoozeError::MaxSnoozedLimitReached {
                max: self.config.max_active_snoozes,
            });
        }

        let wake_at =
            resolve_wake_timestamp(req.preset, now_epoch_secs, self.config.max_duration_secs)?;

        let id = format!("snz-{:06}", self.next_seq);
        self.next_seq = self.next_seq.saturating_add(1);

        let entry = SnoozedTab {
            id: id.clone(),
            tab_id: tab_id.to_string(),
            url: normalized_url,
            title: title.to_string(),
            workspace_id: req
                .workspace_id
                .map(|w| w.trim().to_string())
                .filter(|w| !w.is_empty()),
            note: req
                .note
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty()),
            created_at_epoch_secs: now_epoch_secs,
            wake_at_epoch_secs: wake_at,
            updated_at_epoch_secs: now_epoch_secs,
            snooze_count: 1,
            repeat: req.repeat,
            status: SnoozeStatus::Scheduled,
        };

        self.entries.insert(id, entry.clone());
        Ok(entry)
    }

    /// Transitions all `Scheduled` tabs whose wake time has arrived to `Due`,
    /// returning them ordered by `(wake_at_epoch_secs, id)`.
    pub fn poll_due(&mut self, now_epoch_secs: u64) -> Vec<SnoozedTab> {
        let mut due = Vec::new();
        for entry in self.entries.values_mut() {
            if entry.status == SnoozeStatus::Scheduled && now_epoch_secs >= entry.wake_at_epoch_secs
            {
                entry.status = SnoozeStatus::Due;
                entry.updated_at_epoch_secs = now_epoch_secs;
                due.push(entry.clone());
            }
        }
        due.sort_by(|a, b| {
            a.wake_at_epoch_secs
                .cmp(&b.wake_at_epoch_secs)
                .then_with(|| a.id.cmp(&b.id))
        });
        due
    }

    /// Marks an active (`Scheduled` or `Due`) snooze entry as `Woken`.
    ///
    /// If the entry has a recurring [`SnoozeRepeat`] policy (`Daily` or `Weekly`),
    /// a new `Scheduled` entry is automatically created for the next occurrence.
    pub fn wake_tab(
        &mut self,
        snooze_id: &str,
        now_epoch_secs: u64,
    ) -> Result<SnoozedTab, SnoozeError> {
        let woken = {
            let entry = self
                .entries
                .get_mut(snooze_id)
                .ok_or_else(|| SnoozeError::SnoozeNotFound(snooze_id.to_string()))?;

            if !entry.status.is_active() {
                return Err(SnoozeError::InvalidStateTransition {
                    id: snooze_id.to_string(),
                    from: entry.status,
                    to: SnoozeStatus::Woken,
                });
            }

            entry.status = SnoozeStatus::Woken;
            entry.updated_at_epoch_secs = now_epoch_secs;
            entry.clone()
        };

        let repeat_delta = match woken.repeat {
            SnoozeRepeat::Once => None,
            SnoozeRepeat::Daily => Some(SECS_PER_DAY),
            SnoozeRepeat::Weekly => Some(SECS_PER_WEEK),
        };

        if let Some(delta) = repeat_delta {
            let next_wake =
                now_epoch_secs
                    .checked_add(delta)
                    .ok_or(SnoozeError::DurationTooLong {
                        duration_secs: delta,
                        max_secs: self.config.max_duration_secs,
                    })?;

            let next_id = format!("snz-{:06}", self.next_seq);
            self.next_seq = self.next_seq.saturating_add(1);

            let next_entry = SnoozedTab {
                id: next_id.clone(),
                tab_id: woken.tab_id.clone(),
                url: woken.url.clone(),
                title: woken.title.clone(),
                workspace_id: woken.workspace_id.clone(),
                note: woken.note.clone(),
                created_at_epoch_secs: now_epoch_secs,
                wake_at_epoch_secs: next_wake,
                updated_at_epoch_secs: now_epoch_secs,
                snooze_count: 1,
                repeat: woken.repeat,
                status: SnoozeStatus::Scheduled,
            };
            self.entries.insert(next_id, next_entry);
        }

        Ok(woken)
    }

    /// Reschedules an active (`Scheduled` or `Due`) snooze entry with a new preset.
    pub fn resnooze(
        &mut self,
        snooze_id: &str,
        preset: SnoozePreset,
        now_epoch_secs: u64,
    ) -> Result<SnoozedTab, SnoozeError> {
        let wake_at =
            resolve_wake_timestamp(preset, now_epoch_secs, self.config.max_duration_secs)?;

        let entry = self
            .entries
            .get_mut(snooze_id)
            .ok_or_else(|| SnoozeError::SnoozeNotFound(snooze_id.to_string()))?;

        if !entry.status.is_active() {
            return Err(SnoozeError::InvalidStateTransition {
                id: snooze_id.to_string(),
                from: entry.status,
                to: SnoozeStatus::Scheduled,
            });
        }

        entry.wake_at_epoch_secs = wake_at;
        entry.updated_at_epoch_secs = now_epoch_secs;
        entry.snooze_count = entry.snooze_count.saturating_add(1);
        entry.status = SnoozeStatus::Scheduled;

        Ok(entry.clone())
    }

    /// Cancels an active (`Scheduled` or `Due`) snooze entry.
    pub fn cancel_snooze(
        &mut self,
        snooze_id: &str,
        now_epoch_secs: u64,
    ) -> Result<SnoozedTab, SnoozeError> {
        let entry = self
            .entries
            .get_mut(snooze_id)
            .ok_or_else(|| SnoozeError::SnoozeNotFound(snooze_id.to_string()))?;

        if !entry.status.is_active() {
            return Err(SnoozeError::InvalidStateTransition {
                id: snooze_id.to_string(),
                from: entry.status,
                to: SnoozeStatus::Cancelled,
            });
        }

        entry.status = SnoozeStatus::Cancelled;
        entry.updated_at_epoch_secs = now_epoch_secs;
        Ok(entry.clone())
    }

    /// Looks up a snooze entry by its unique ID.
    pub fn get(&self, snooze_id: &str) -> Option<&SnoozedTab> {
        self.entries.get(snooze_id)
    }

    /// Returns all active (`Scheduled` or `Due`) entries sorted by wake time ascending.
    pub fn list_active(&self) -> Vec<&SnoozedTab> {
        let mut active: Vec<&SnoozedTab> = self
            .entries
            .values()
            .filter(|e| e.status.is_active())
            .collect();
        active.sort_by(|a, b| {
            a.wake_at_epoch_secs
                .cmp(&b.wake_at_epoch_secs)
                .then_with(|| a.id.cmp(&b.id))
        });
        active
    }

    /// Returns the earliest `wake_at_epoch_secs` among `Scheduled` entries, if any.
    pub fn next_wake_epoch_secs(&self) -> Option<u64> {
        self.entries
            .values()
            .filter(|e| e.status == SnoozeStatus::Scheduled)
            .map(|e| e.wake_at_epoch_secs)
            .min()
    }

    /// Removes completed (`Woken` or `Cancelled`) entries older than `auto_prune_after_secs`.
    pub fn prune_completed(&mut self, now_epoch_secs: u64) -> usize {
        let cutoff = now_epoch_secs.saturating_sub(self.config.auto_prune_after_secs);
        let before = self.entries.len();
        self.entries
            .retain(|_, e| e.status.is_active() || e.updated_at_epoch_secs > cutoff);
        before.saturating_sub(self.entries.len())
    }

    /// Serializes the timer state to JSON.
    pub fn to_json(&self) -> Result<String, SnoozeError> {
        serde_json::to_string(self).map_err(|err| SnoozeError::Serialization(err.to_string()))
    }

    /// Restores timer state from JSON, validating all entries.
    pub fn from_json(json: &str) -> Result<Self, SnoozeError> {
        let timer: Self = serde_json::from_str(json)
            .map_err(|err| SnoozeError::Serialization(err.to_string()))?;
        for entry in timer.entries.values() {
            if entry.tab_id.trim().is_empty() {
                return Err(SnoozeError::EmptyTabId);
            }
            if entry.title.trim().is_empty() {
                return Err(SnoozeError::EmptyTitle);
            }
            validate_tab_url(&entry.url)?;
        }
        Ok(timer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request(tab_id: &str, preset: SnoozePreset) -> SnoozeRequest {
        SnoozeRequest {
            tab_id: tab_id.to_string(),
            url: "https://example.com/articles/rust".to_string(),
            title: "Rust Async Patterns".to_string(),
            workspace_id: Some("work".to_string()),
            note: Some("Read after standup".to_string()),
            preset,
            repeat: SnoozeRepeat::Once,
        }
    }

    #[test]
    fn schedule_poll_wake_and_serialize_happy_path() {
        let now = 1_700_000_000;
        let mut timer = TabSnoozeTimer::default();

        let entry = timer
            .schedule_tab(sample_request("tab-1", SnoozePreset::LaterToday), now)
            .expect("schedule should succeed");

        assert_eq!(entry.status, SnoozeStatus::Scheduled);
        assert_eq!(entry.wake_at_epoch_secs, now + 3 * SECS_PER_HOUR);
        assert_eq!(entry.remaining_secs(now + 600), 3 * SECS_PER_HOUR - 600);
        assert_eq!(timer.next_wake_epoch_secs(), Some(now + 3 * SECS_PER_HOUR));

        // Before wake time: nothing due
        assert!(timer.poll_due(now + 3 * SECS_PER_HOUR - 1).is_empty());

        // At wake time: transitions to Due
        let due = timer.poll_due(now + 3 * SECS_PER_HOUR);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, entry.id);
        assert_eq!(due[0].status, SnoozeStatus::Due);
        assert_eq!(timer.next_wake_epoch_secs(), None);

        // Wake the tab
        let woken = timer
            .wake_tab(&entry.id, now + 3 * SECS_PER_HOUR + 5)
            .expect("wake should succeed");
        assert_eq!(woken.status, SnoozeStatus::Woken);
        assert!(timer.list_active().is_empty());

        // Round-trip JSON serialization
        let json = timer.to_json().expect("serialize should succeed");
        let restored = TabSnoozeTimer::from_json(&json).expect("deserialize should succeed");
        assert_eq!(restored, timer);
    }

    #[test]
    fn validation_and_duplicate_or_limit_errors() {
        let now = 1_700_000_000;
        let mut timer = TabSnoozeTimer::new(SnoozeConfig {
            max_active_snoozes: 1,
            max_duration_secs: 86_400,
            auto_prune_after_secs: 3_600,
        });

        // Empty tab_id
        let mut bad = sample_request("   ", SnoozePreset::LaterToday);
        assert_eq!(
            timer.schedule_tab(bad.clone(), now),
            Err(SnoozeError::EmptyTabId)
        );

        // Invalid URL scheme
        bad.tab_id = "tab-x".to_string();
        bad.url = "javascript:alert(1)".to_string();
        assert!(matches!(
            timer.schedule_tab(bad.clone(), now),
            Err(SnoozeError::InvalidUrl(_))
        ));

        // Zero custom duration
        bad.url = "https://example.com".to_string();
        bad.preset = SnoozePreset::CustomDurationSecs(0);
        assert_eq!(
            timer.schedule_tab(bad.clone(), now),
            Err(SnoozeError::ZeroDuration)
        );

        // Timestamp in the past
        bad.preset = SnoozePreset::AtTimestamp(now);
        assert_eq!(
            timer.schedule_tab(bad.clone(), now),
            Err(SnoozeError::WakeTimeInPast {
                wake_at_epoch_secs: now,
                now_epoch_secs: now,
            })
        );

        // Duration exceeding max_duration_secs (NextWeek > 86_400)
        bad.preset = SnoozePreset::NextWeek;
        assert_eq!(
            timer.schedule_tab(bad, now),
            Err(SnoozeError::DurationTooLong {
                duration_secs: SECS_PER_WEEK,
                max_secs: 86_400,
            })
        );

        // Schedule valid tab-1
        timer
            .schedule_tab(sample_request("tab-1", SnoozePreset::LaterToday), now)
            .expect("first tab should succeed");

        // Duplicate active tab_id
        assert_eq!(
            timer.schedule_tab(sample_request("tab-1", SnoozePreset::LaterToday), now),
            Err(SnoozeError::AlreadySnoozed("tab-1".to_string()))
        );

        // Max active snoozes reached
        assert_eq!(
            timer.schedule_tab(sample_request("tab-2", SnoozePreset::LaterToday), now),
            Err(SnoozeError::MaxSnoozedLimitReached { max: 1 })
        );
    }

    #[test]
    fn resnooze_recurring_wake_cancel_and_prune() {
        let now = 1_700_000_000;
        let mut timer = TabSnoozeTimer::new(SnoozeConfig {
            max_active_snoozes: 10,
            max_duration_secs: DEFAULT_MAX_DURATION_SECS,
            auto_prune_after_secs: 1_000,
        });

        let mut req = sample_request("tab-daily", SnoozePreset::CustomDurationSecs(600));
        req.repeat = SnoozeRepeat::Daily;
        let first = timer.schedule_tab(req, now).expect("schedule daily tab");

        // Resnooze bumps snooze_count and pushes wake time out
        let resnoozed = timer
            .resnooze(
                &first.id,
                SnoozePreset::CustomDurationSecs(1_200),
                now + 100,
            )
            .expect("resnooze should succeed");
        assert_eq!(resnoozed.snooze_count, 2);
        assert_eq!(resnoozed.wake_at_epoch_secs, now + 100 + 1_200);
        assert_eq!(resnoozed.status, SnoozeStatus::Scheduled);

        // Poll and wake daily tab -> should auto-schedule next day's occurrence
        let due = timer.poll_due(now + 1_300);
        assert_eq!(due.len(), 1);
        let woken = timer
            .wake_tab(&first.id, now + 1_300)
            .expect("wake daily tab");
        assert_eq!(woken.status, SnoozeStatus::Woken);

        // Cannot wake an already-woken tab
        assert_eq!(
            timer.wake_tab(&first.id, now + 1_305),
            Err(SnoozeError::InvalidStateTransition {
                id: first.id.clone(),
                from: SnoozeStatus::Woken,
                to: SnoozeStatus::Woken,
            })
        );

        let active = timer.list_active();
        assert_eq!(active.len(), 1);
        assert_ne!(active[0].id, first.id);
        assert_eq!(active[0].wake_at_epoch_secs, now + 1_300 + SECS_PER_DAY);

        // Cancel the next occurrence
        let next_id = active[0].id.clone();
        let cancelled = timer
            .cancel_snooze(&next_id, now + 1_400)
            .expect("cancel should succeed");
        assert_eq!(cancelled.status, SnoozeStatus::Cancelled);
        assert!(timer.list_active().is_empty());

        // Prune completed entries older than 1000s
        let pruned = timer.prune_completed(now + 2_400);
        assert_eq!(pruned, 2);
        assert!(timer.get(&first.id).is_none());
        assert!(timer.get(&next_id).is_none());
    }
}
