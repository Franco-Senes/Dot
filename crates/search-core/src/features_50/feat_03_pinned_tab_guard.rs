//! Pinned Tab Guard (`feat_03_pinned_tab_guard`).
//!
//! Protects pinned browser tabs against accidental closure, bulk-close actions,
//! and unintended cross-origin or cross-domain navigation. Supports configurable
//! close-guard policies, navigation lock modes, per-tab overrides, and
//! time-bounded confirmation challenges.

use std::collections::{BTreeMap, HashSet};

use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

/// Errors returned by [`PinnedTabGuard`] operations and validation.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PinnedTabGuardError {
    #[error("tab_id must not be empty")]
    EmptyTabId,
    #[error("url must not be empty")]
    EmptyUrl,
    #[error("invalid url: {0}")]
    InvalidUrl(String),
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("tab '{0}' is already pinned")]
    TabAlreadyPinned(String),
    #[error("tab '{0}' is not pinned")]
    TabNotPinned(String),
    #[error("maximum pinned tabs limit ({max}) reached")]
    MaxPinnedTabsExceeded { max: usize },
    #[error("reorder list does not match current pinned tabs")]
    InvalidReorderList,
    #[error("confirmation token is invalid or unknown")]
    InvalidConfirmationToken,
    #[error("confirmation token has expired")]
    ConfirmationExpired,
    #[error("serialization error: {0}")]
    SerializationError(String),
}

/// Policy controlling how close attempts on a pinned tab are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CloseGuardMode {
    /// Block all close attempts while the tab remains pinned.
    Block,
    /// Require an explicit time-bounded confirmation token to close.
    #[default]
    RequireConfirmation,
    /// Block shortcuts, middle-clicks, and bulk closes; allow explicit close button only.
    AllowExplicitOnly,
}

/// Policy controlling how navigation from a pinned tab is constrained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NavigationLockMode {
    /// Allow navigation to any URL inside the pinned tab.
    Unrestricted,
    /// Restrict in-tab navigation to the exact scheme, host, and port of the pinned URL.
    #[default]
    SameOrigin,
    /// Restrict in-tab navigation to the pinned host or its subdomains.
    SameHostOrSubdomain,
    /// Restrict in-tab navigation to the exact pinned URL (ignoring fragment).
    ExactUrl,
}

/// Source trigger for a tab close request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseTrigger {
    KeyboardShortcut,
    MiddleClick,
    CloseButton,
    BulkCloseOthers,
    BulkCloseRight,
    BulkCloseAll,
    ScriptOrApi,
}

impl CloseTrigger {
    /// Returns `true` when the trigger represents a bulk-close operation.
    pub fn is_bulk(&self) -> bool {
        matches!(
            self,
            Self::BulkCloseOthers | Self::BulkCloseRight | Self::BulkCloseAll
        )
    }
}

/// Configuration for [`PinnedTabGuard`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedTabGuardConfig {
    pub close_guard: CloseGuardMode,
    pub navigation_lock: NavigationLockMode,
    pub max_pinned_tabs: usize,
    pub confirmation_ttl_secs: u64,
    pub open_blocked_navigation_in_new_tab: bool,
    pub preserve_pinned_on_session_restore: bool,
}

impl Default for PinnedTabGuardConfig {
    fn default() -> Self {
        Self {
            close_guard: CloseGuardMode::RequireConfirmation,
            navigation_lock: NavigationLockMode::SameOrigin,
            max_pinned_tabs: 24,
            confirmation_ttl_secs: 30,
            open_blocked_navigation_in_new_tab: true,
            preserve_pinned_on_session_restore: true,
        }
    }
}

/// Metadata and per-tab policy overrides for a single pinned tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedTabEntry {
    pub tab_id: String,
    pub pinned_url: String,
    pub pinned_origin: String,
    pub pinned_host: Option<String>,
    pub title: String,
    pub pin_order: u32,
    pub pinned_at_unix: i64,
    pub navigation_lock_override: Option<NavigationLockMode>,
    pub close_guard_override: Option<CloseGuardMode>,
    pub custom_allowed_hosts: Vec<String>,
}

/// Action awaiting user confirmation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PendingAction {
    CloseTab { tab_id: String },
    NavigateTab { tab_id: String, target_url: String },
}

/// A time-bounded confirmation challenge issued by the guard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingConfirmation {
    pub token: String,
    pub action: PendingAction,
    pub created_at_unix: i64,
    pub expires_at_unix: i64,
}

/// Decision returned when evaluating a close or navigation attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum GuardDecision {
    /// Action is permitted on the pinned tab.
    Allow,
    /// Action is blocked outright.
    Block { reason: String },
    /// Navigation is blocked in the pinned tab and should open in a new tab instead.
    RedirectToNewTab { target_url: String, reason: String },
    /// Action requires user confirmation with the provided token.
    RequiresConfirmation {
        token: String,
        prompt: String,
        expires_at_unix: i64,
    },
}

/// Result of filtering a bulk-close candidate list against pinned tabs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BulkCloseFilterResult {
    pub closable_tab_ids: Vec<String>,
    pub protected_tab_ids: Vec<String>,
}

/// Stateful guard managing pinned tabs, navigation locks, and confirmation challenges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedTabGuard {
    config: PinnedTabGuardConfig,
    entries: BTreeMap<String, PinnedTabEntry>,
    pending_confirmations: BTreeMap<String, PendingConfirmation>,
    next_pin_order: u32,
    token_nonce: u64,
}

impl PinnedTabGuard {
    /// Creates a new [`PinnedTabGuard`] after validating `config`.
    pub fn new(config: PinnedTabGuardConfig) -> Result<Self, PinnedTabGuardError> {
        Self::validate_config(&config)?;
        Ok(Self {
            config,
            entries: BTreeMap::new(),
            pending_confirmations: BTreeMap::new(),
            next_pin_order: 0,
            token_nonce: 0,
        })
    }

    /// Validates a [`PinnedTabGuardConfig`].
    pub fn validate_config(config: &PinnedTabGuardConfig) -> Result<(), PinnedTabGuardError> {
        if config.max_pinned_tabs == 0 {
            return Err(PinnedTabGuardError::InvalidConfig(
                "max_pinned_tabs must be greater than 0".to_string(),
            ));
        }
        if config.confirmation_ttl_secs == 0 {
            return Err(PinnedTabGuardError::InvalidConfig(
                "confirmation_ttl_secs must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }

    /// Returns a reference to the active configuration.
    pub fn config(&self) -> &PinnedTabGuardConfig {
        &self.config
    }

    /// Updates the guard configuration after validating it.
    pub fn update_config(
        &mut self,
        config: PinnedTabGuardConfig,
    ) -> Result<(), PinnedTabGuardError> {
        Self::validate_config(&config)?;
        if self.entries.len() > config.max_pinned_tabs {
            return Err(PinnedTabGuardError::MaxPinnedTabsExceeded {
                max: config.max_pinned_tabs,
            });
        }
        self.config = config;
        Ok(())
    }

    /// Pins a tab with the given identifier, URL, title, and timestamp.
    pub fn pin_tab(
        &mut self,
        tab_id: &str,
        url_str: &str,
        title: &str,
        now_unix: i64,
    ) -> Result<&PinnedTabEntry, PinnedTabGuardError> {
        let tab_id = tab_id.trim();
        if tab_id.is_empty() {
            return Err(PinnedTabGuardError::EmptyTabId);
        }
        if self.entries.contains_key(tab_id) {
            return Err(PinnedTabGuardError::TabAlreadyPinned(tab_id.to_string()));
        }
        if self.entries.len() >= self.config.max_pinned_tabs {
            return Err(PinnedTabGuardError::MaxPinnedTabsExceeded {
                max: self.config.max_pinned_tabs,
            });
        }

        let parsed = parse_and_validate_url(url_str)?;
        let pinned_origin = parsed.origin().ascii_serialization();
        let pinned_host = parsed.host_str().map(|h| h.to_ascii_lowercase());
        let pin_order = self.next_pin_order;
        self.next_pin_order = self.next_pin_order.saturating_add(1);

        let entry = PinnedTabEntry {
            tab_id: tab_id.to_string(),
            pinned_url: parsed.to_string(),
            pinned_origin,
            pinned_host,
            title: title.trim().to_string(),
            pin_order,
            pinned_at_unix: now_unix,
            navigation_lock_override: None,
            close_guard_override: None,
            custom_allowed_hosts: Vec::new(),
        };

        self.entries.insert(tab_id.to_string(), entry);
        self.entries
            .get(tab_id)
            .ok_or_else(|| PinnedTabGuardError::TabNotPinned(tab_id.to_string()))
    }

    /// Unpins a tab and removes any pending confirmations for it.
    pub fn unpin_tab(&mut self, tab_id: &str) -> Result<PinnedTabEntry, PinnedTabGuardError> {
        let tab_id = tab_id.trim();
        if tab_id.is_empty() {
            return Err(PinnedTabGuardError::EmptyTabId);
        }
        let removed = self
            .entries
            .remove(tab_id)
            .ok_or_else(|| PinnedTabGuardError::TabNotPinned(tab_id.to_string()))?;

        self.pending_confirmations
            .retain(|_, pending| match &pending.action {
                PendingAction::CloseTab { tab_id: id } => id != tab_id,
                PendingAction::NavigateTab { tab_id: id, .. } => id != tab_id,
            });

        Ok(removed)
    }

    /// Configures per-tab policy overrides and custom allowed hosts.
    pub fn set_tab_overrides(
        &mut self,
        tab_id: &str,
        navigation_lock_override: Option<NavigationLockMode>,
        close_guard_override: Option<CloseGuardMode>,
        custom_allowed_hosts: Vec<String>,
    ) -> Result<(), PinnedTabGuardError> {
        let tab_id = tab_id.trim();
        if tab_id.is_empty() {
            return Err(PinnedTabGuardError::EmptyTabId);
        }
        let entry = self
            .entries
            .get_mut(tab_id)
            .ok_or_else(|| PinnedTabGuardError::TabNotPinned(tab_id.to_string()))?;

        let mut normalized_hosts = Vec::with_capacity(custom_allowed_hosts.len());
        for host in custom_allowed_hosts {
            let trimmed = host.trim().to_ascii_lowercase();
            if !trimmed.is_empty() && !normalized_hosts.contains(&trimmed) {
                normalized_hosts.push(trimmed);
            }
        }

        entry.navigation_lock_override = navigation_lock_override;
        entry.close_guard_override = close_guard_override;
        entry.custom_allowed_hosts = normalized_hosts;
        Ok(())
    }

    /// Returns `true` if `tab_id` is currently pinned.
    pub fn is_pinned(&self, tab_id: &str) -> bool {
        self.entries.contains_key(tab_id.trim())
    }

    /// Returns all pinned tabs ordered by `pin_order`.
    pub fn pinned_tabs(&self) -> Vec<&PinnedTabEntry> {
        let mut list: Vec<&PinnedTabEntry> = self.entries.values().collect();
        list.sort_by_key(|entry| entry.pin_order);
        list
    }

    /// Reorders pinned tabs according to `ordered_tab_ids`.
    pub fn reorder_pinned(
        &mut self,
        ordered_tab_ids: &[String],
    ) -> Result<(), PinnedTabGuardError> {
        if ordered_tab_ids.len() != self.entries.len() {
            return Err(PinnedTabGuardError::InvalidReorderList);
        }
        let mut seen = HashSet::with_capacity(ordered_tab_ids.len());
        for id in ordered_tab_ids {
            if !self.entries.contains_key(id.as_str()) || !seen.insert(id.as_str()) {
                return Err(PinnedTabGuardError::InvalidReorderList);
            }
        }

        for (idx, id) in ordered_tab_ids.iter().enumerate() {
            if let Some(entry) = self.entries.get_mut(id.as_str()) {
                entry.pin_order = idx as u32;
            }
        }
        self.next_pin_order = ordered_tab_ids.len() as u32;
        Ok(())
    }

    /// Evaluates whether a tab close request should be allowed, blocked, or confirmed.
    pub fn evaluate_close(
        &mut self,
        tab_id: &str,
        trigger: CloseTrigger,
        now_unix: i64,
    ) -> GuardDecision {
        let tab_id = tab_id.trim();
        let Some(entry) = self.entries.get(tab_id) else {
            return GuardDecision::Allow;
        };

        let mode = entry
            .close_guard_override
            .unwrap_or(self.config.close_guard);

        if trigger.is_bulk() {
            return GuardDecision::Block {
                reason: format!("pinned tab '{tab_id}' is protected from bulk close"),
            };
        }

        match mode {
            CloseGuardMode::Block => GuardDecision::Block {
                reason: format!("pinned tab '{tab_id}' cannot be closed while pinned"),
            },
            CloseGuardMode::AllowExplicitOnly => {
                if trigger == CloseTrigger::CloseButton {
                    GuardDecision::Allow
                } else {
                    GuardDecision::Block {
                        reason: format!(
                            "pinned tab '{tab_id}' only allows explicit close-button closure"
                        ),
                    }
                }
            }
            CloseGuardMode::RequireConfirmation => {
                let title = if entry.title.is_empty() {
                    entry.pinned_url.clone()
                } else {
                    entry.title.clone()
                };
                let (token, expires_at_unix) = self.issue_confirmation(
                    PendingAction::CloseTab {
                        tab_id: tab_id.to_string(),
                    },
                    now_unix,
                );
                GuardDecision::RequiresConfirmation {
                    token,
                    prompt: format!("Confirm closing pinned tab '{title}'"),
                    expires_at_unix,
                }
            }
        }
    }

    /// Evaluates whether a navigation inside `tab_id` to `target_url` is permitted.
    pub fn evaluate_navigation(
        &mut self,
        tab_id: &str,
        target_url: &str,
        now_unix: i64,
    ) -> Result<GuardDecision, PinnedTabGuardError> {
        let tab_id = tab_id.trim();
        if tab_id.is_empty() {
            return Err(PinnedTabGuardError::EmptyTabId);
        }
        let target_parsed = parse_and_validate_url(target_url)?;

        let Some(entry) = self.entries.get(tab_id) else {
            return Ok(GuardDecision::Allow);
        };

        let mode = entry
            .navigation_lock_override
            .unwrap_or(self.config.navigation_lock);

        let target_host = target_parsed.host_str().map(|h| h.to_ascii_lowercase());
        if let Some(ref host) = target_host {
            if entry
                .custom_allowed_hosts
                .iter()
                .any(|allowed| host_matches_domain(host, allowed))
            {
                return Ok(GuardDecision::Allow);
            }
        }

        let permitted = match mode {
            NavigationLockMode::Unrestricted => true,
            NavigationLockMode::SameOrigin => {
                target_parsed.origin().ascii_serialization() == entry.pinned_origin
            }
            NavigationLockMode::SameHostOrSubdomain => match (&entry.pinned_host, &target_host) {
                (Some(pinned_host), Some(target_h)) => host_matches_domain(target_h, pinned_host),
                _ => target_parsed.origin().ascii_serialization() == entry.pinned_origin,
            },
            NavigationLockMode::ExactUrl => {
                urls_match_ignoring_fragment(&entry.pinned_url, &target_parsed)
            }
        };

        if permitted {
            return Ok(GuardDecision::Allow);
        }

        let normalized_target = target_parsed.to_string();
        let reason = format!(
            "navigation to '{normalized_target}' violates pinned tab '{tab_id}' lock policy"
        );

        if self.config.open_blocked_navigation_in_new_tab {
            Ok(GuardDecision::RedirectToNewTab {
                target_url: normalized_target,
                reason,
            })
        } else {
            let (token, expires_at_unix) = self.issue_confirmation(
                PendingAction::NavigateTab {
                    tab_id: tab_id.to_string(),
                    target_url: normalized_target.clone(),
                },
                now_unix,
            );
            Ok(GuardDecision::RequiresConfirmation {
                token,
                prompt: format!(
                    "Confirm navigating pinned tab '{tab_id}' to '{normalized_target}'"
                ),
                expires_at_unix,
            })
        }
    }

    /// Validates and consumes a pending confirmation token.
    pub fn confirm_action(
        &mut self,
        token: &str,
        now_unix: i64,
    ) -> Result<PendingAction, PinnedTabGuardError> {
        let pending = self
            .pending_confirmations
            .remove(token.trim())
            .ok_or(PinnedTabGuardError::InvalidConfirmationToken)?;

        if now_unix > pending.expires_at_unix {
            return Err(PinnedTabGuardError::ConfirmationExpired);
        }

        Ok(pending.action)
    }

    /// Removes all expired confirmation tokens and returns the number pruned.
    pub fn prune_expired_confirmations(&mut self, now_unix: i64) -> usize {
        let before = self.pending_confirmations.len();
        self.pending_confirmations
            .retain(|_, pending| now_unix <= pending.expires_at_unix);
        before.saturating_sub(self.pending_confirmations.len())
    }

    /// Partitions a bulk-close candidate list into closable and protected tab IDs.
    pub fn filter_bulk_close(
        &self,
        candidate_tab_ids: &[String],
        trigger: CloseTrigger,
    ) -> BulkCloseFilterResult {
        let mut closable_tab_ids = Vec::new();
        let mut protected_tab_ids = Vec::new();

        for id in candidate_tab_ids {
            if self.is_pinned(id)
                && (trigger.is_bulk() || self.is_protected_from_trigger(id, trigger))
            {
                protected_tab_ids.push(id.clone());
            } else {
                closable_tab_ids.push(id.clone());
            }
        }

        BulkCloseFilterResult {
            closable_tab_ids,
            protected_tab_ids,
        }
    }

    /// Serializes the entire guard state to JSON.
    pub fn to_json(&self) -> Result<String, PinnedTabGuardError> {
        serde_json::to_string_pretty(self)
            .map_err(|err| PinnedTabGuardError::SerializationError(err.to_string()))
    }

    /// Deserializes guard state from JSON and validates its configuration.
    pub fn from_json(raw: &str) -> Result<Self, PinnedTabGuardError> {
        let guard: Self = serde_json::from_str(raw)
            .map_err(|err| PinnedTabGuardError::SerializationError(err.to_string()))?;
        Self::validate_config(&guard.config)?;
        if guard.entries.len() > guard.config.max_pinned_tabs {
            return Err(PinnedTabGuardError::MaxPinnedTabsExceeded {
                max: guard.config.max_pinned_tabs,
            });
        }
        Ok(guard)
    }

    fn is_protected_from_trigger(&self, tab_id: &str, trigger: CloseTrigger) -> bool {
        let Some(entry) = self.entries.get(tab_id.trim()) else {
            return false;
        };
        let mode = entry
            .close_guard_override
            .unwrap_or(self.config.close_guard);
        match mode {
            CloseGuardMode::Block | CloseGuardMode::RequireConfirmation => true,
            CloseGuardMode::AllowExplicitOnly => trigger != CloseTrigger::CloseButton,
        }
    }

    fn issue_confirmation(&mut self, action: PendingAction, now_unix: i64) -> (String, i64) {
        self.token_nonce = self.token_nonce.saturating_add(1);
        let ttl = i64::try_from(self.config.confirmation_ttl_secs).unwrap_or(i64::MAX);
        let expires_at_unix = now_unix.saturating_add(ttl);

        let material = match &action {
            PendingAction::CloseTab { tab_id } => {
                format!("close:{tab_id}:{now_unix}:{}", self.token_nonce)
            }
            PendingAction::NavigateTab { tab_id, target_url } => {
                format!("nav:{tab_id}:{target_url}:{now_unix}:{}", self.token_nonce)
            }
        };

        let hash = digest(&SHA256, material.as_bytes());
        let token = hex_encode(&hash.as_ref()[..16]);

        self.pending_confirmations.insert(
            token.clone(),
            PendingConfirmation {
                token: token.clone(),
                action,
                created_at_unix: now_unix,
                expires_at_unix,
            },
        );

        (token, expires_at_unix)
    }
}

fn parse_and_validate_url(raw: &str) -> Result<Url, PinnedTabGuardError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(PinnedTabGuardError::EmptyUrl);
    }
    let parsed =
        Url::parse(trimmed).map_err(|err| PinnedTabGuardError::InvalidUrl(err.to_string()))?;
    if parsed.scheme().is_empty() {
        return Err(PinnedTabGuardError::InvalidUrl(
            "URL scheme must not be empty".to_string(),
        ));
    }
    Ok(parsed)
}

fn host_matches_domain(host: &str, domain: &str) -> bool {
    if host == domain {
        return true;
    }
    host.ends_with(&format!(".{domain}"))
}

fn urls_match_ignoring_fragment(pinned_raw: &str, target: &Url) -> bool {
    let Ok(mut pinned) = Url::parse(pinned_raw) else {
        return false;
    };
    let mut normalized_target = target.clone();
    pinned.set_fragment(None);
    normalized_target.set_fragment(None);
    pinned == normalized_target
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_close_confirmation_and_expiration_flow() {
        let mut guard = PinnedTabGuard::new(PinnedTabGuardConfig::default()).unwrap();
        let entry = guard
            .pin_tab("tab-1", "https://mail.example.com/inbox", "Mail", 1_000)
            .unwrap();
        assert_eq!(entry.pinned_origin, "https://mail.example.com");
        assert!(guard.is_pinned("tab-1"));

        // Bulk close always blocks pinned tabs.
        let bulk = guard.evaluate_close("tab-1", CloseTrigger::BulkCloseOthers, 1_005);
        assert!(matches!(bulk, GuardDecision::Block { .. }));

        // Keyboard shortcut close triggers confirmation in RequireConfirmation mode.
        let decision = guard.evaluate_close("tab-1", CloseTrigger::KeyboardShortcut, 1_010);
        let (token, expires_at) = match decision {
            GuardDecision::RequiresConfirmation {
                token,
                expires_at_unix,
                ..
            } => (token, expires_at_unix),
            other => panic!("expected RequiresConfirmation, got {other:?}"),
        };
        assert_eq!(expires_at, 1_040);

        // Confirming within TTL succeeds and consumes token.
        let action = guard.confirm_action(&token, 1_025).unwrap();
        assert_eq!(
            action,
            PendingAction::CloseTab {
                tab_id: "tab-1".to_string()
            }
        );
        assert_eq!(
            guard.confirm_action(&token, 1_026),
            Err(PinnedTabGuardError::InvalidConfirmationToken)
        );

        // Expired token is rejected.
        let decision2 = guard.evaluate_close("tab-1", CloseTrigger::MiddleClick, 2_000);
        let token2 = match decision2 {
            GuardDecision::RequiresConfirmation { token, .. } => token,
            other => panic!("expected RequiresConfirmation, got {other:?}"),
        };
        assert_eq!(
            guard.confirm_action(&token2, 2_031),
            Err(PinnedTabGuardError::ConfirmationExpired)
        );
    }

    #[test]
    fn navigation_lock_modes_and_custom_allowed_hosts() {
        let mut guard = PinnedTabGuard::new(PinnedTabGuardConfig::default()).unwrap();
        guard
            .pin_tab("tab-docs", "https://docs.example.com/guide", "Docs", 500)
            .unwrap();

        // Same-origin navigation is allowed.
        let same_origin = guard
            .evaluate_navigation("tab-docs", "https://docs.example.com/api/v1", 501)
            .unwrap();
        assert_eq!(same_origin, GuardDecision::Allow);

        // Cross-origin navigation redirects to a new tab by default.
        let cross_origin = guard
            .evaluate_navigation("tab-docs", "https://external.org/article", 502)
            .unwrap();
        assert!(matches!(
            cross_origin,
            GuardDecision::RedirectToNewTab { .. }
        ));

        // Allow subdomain navigation and a custom SSO host override.
        guard
            .set_tab_overrides(
                "tab-docs",
                Some(NavigationLockMode::SameHostOrSubdomain),
                Some(CloseGuardMode::AllowExplicitOnly),
                vec!["sso.partner.net".to_string()],
            )
            .unwrap();

        assert_eq!(
            guard
                .evaluate_navigation("tab-docs", "https://sub.docs.example.com/ref", 503)
                .unwrap(),
            GuardDecision::Allow
        );
        assert_eq!(
            guard
                .evaluate_navigation("tab-docs", "https://sso.partner.net/login", 504)
                .unwrap(),
            GuardDecision::Allow
        );

        // AllowExplicitOnly blocks Ctrl+W but permits CloseButton.
        assert!(matches!(
            guard.evaluate_close("tab-docs", CloseTrigger::KeyboardShortcut, 505),
            GuardDecision::Block { .. }
        ));
        assert_eq!(
            guard.evaluate_close("tab-docs", CloseTrigger::CloseButton, 506),
            GuardDecision::Allow
        );
    }

    #[test]
    fn bulk_filter_reorder_validation_and_json_roundtrip() {
        let bad_cfg = PinnedTabGuardConfig {
            max_pinned_tabs: 0,
            ..PinnedTabGuardConfig::default()
        };
        assert!(matches!(
            PinnedTabGuard::new(bad_cfg),
            Err(PinnedTabGuardError::InvalidConfig(_))
        ));

        let cfg = PinnedTabGuardConfig {
            max_pinned_tabs: 2,
            ..PinnedTabGuardConfig::default()
        };
        let mut guard = PinnedTabGuard::new(cfg).unwrap();
        guard
            .pin_tab("t1", "https://a.example.com", "A", 10)
            .unwrap();
        guard
            .pin_tab("t2", "https://b.example.com", "B", 11)
            .unwrap();

        // Exceeding max_pinned_tabs fails cleanly.
        assert_eq!(
            guard.pin_tab("t3", "https://c.example.com", "C", 12),
            Err(PinnedTabGuardError::MaxPinnedTabsExceeded { max: 2 })
        );

        // Reorder and verify order.
        guard
            .reorder_pinned(&["t2".to_string(), "t1".to_string()])
            .unwrap();
        let ordered_ids: Vec<&str> = guard
            .pinned_tabs()
            .into_iter()
            .map(|e| e.tab_id.as_str())
            .collect();
        assert_eq!(ordered_ids, vec!["t2", "t1"]);

        // Bulk filter protects pinned tabs and leaves unpinned tabs closable.
        let filter = guard.filter_bulk_close(
            &["t1".to_string(), "t3".to_string(), "t2".to_string()],
            CloseTrigger::BulkCloseAll,
        );
        assert_eq!(filter.closable_tab_ids, vec!["t3".to_string()]);
        assert_eq!(
            filter.protected_tab_ids,
            vec!["t1".to_string(), "t2".to_string()]
        );

        // JSON round-trip preserves state.
        let json = guard.to_json().unwrap();
        let restored = PinnedTabGuard::from_json(&json).unwrap();
        assert_eq!(restored, guard);
    }
}
