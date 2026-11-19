use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

/// Errors returned when validating, managing, or evaluating tab mute domain rules.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum MuteRuleError {
    #[error("domain pattern cannot be empty")]
    EmptyPattern,

    #[error("invalid domain pattern '{0}'")]
    InvalidPattern(String),

    #[error("invalid URL '{0}'")]
    InvalidUrl(String),

    #[error("URL '{0}' has no host")]
    MissingHost(String),

    #[error("mute rule for pattern '{0}' already exists")]
    DuplicatePattern(String),

    #[error("mute rule for pattern '{0}' was not found")]
    RuleNotFound(String),

    #[error("serialization error: {0}")]
    Serialization(String),
}

impl From<serde_json::Error> for MuteRuleError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}

/// Action to take on tab audio when a rule or override applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MuteAction {
    Mute,
    Unmute,
}

impl MuteAction {
    /// Returns `true` when the action mutes audio.
    pub const fn is_muted(self) -> bool {
        matches!(self, Self::Mute)
    }
}

impl fmt::Display for MuteAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mute => f.write_str("mute"),
            Self::Unmute => f.write_str("unmute"),
        }
    }
}

/// Parsed representation of a validated domain pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ParsedPattern {
    /// Matches only the exact normalized host (e.g. `example.com`).
    Exact(String),
    /// Matches any subdomain of the base domain (e.g. `*.example.com` matches `a.example.com`).
    WildcardSubdomain(String),
}

impl ParsedPattern {
    fn parse(raw: &str) -> Result<(String, Self), MuteRuleError> {
        let trimmed = raw.trim().to_ascii_lowercase();
        if trimmed.is_empty() {
            return Err(MuteRuleError::EmptyPattern);
        }

        if trimmed.contains("://")
            || trimmed.contains('/')
            || trimmed.contains('?')
            || trimmed.contains('#')
            || trimmed.chars().any(char::is_whitespace)
        {
            return Err(MuteRuleError::InvalidPattern(raw.to_string()));
        }

        if let Some(rest) = trimmed.strip_prefix("*.") {
            validate_domain_host(rest, raw)?;
            if !rest.contains('.') {
                return Err(MuteRuleError::InvalidPattern(raw.to_string()));
            }
            let normalized = format!("*.{rest}");
            Ok((normalized, Self::WildcardSubdomain(rest.to_string())))
        } else {
            if trimmed.contains('*') {
                return Err(MuteRuleError::InvalidPattern(raw.to_string()));
            }
            validate_domain_host(&trimmed, raw)?;
            Ok((trimmed.clone(), Self::Exact(trimmed)))
        }
    }

    fn matches(&self, normalized_host: &str) -> bool {
        match self {
            Self::Exact(domain) => normalized_host == domain,
            Self::WildcardSubdomain(base) => {
                if normalized_host.len() <= base.len() + 1 {
                    return false;
                }
                normalized_host.ends_with(base)
                    && normalized_host.as_bytes()[normalized_host.len() - base.len() - 1] == b'.'
            }
        }
    }

    fn is_exact(&self) -> bool {
        matches!(self, Self::Exact(_))
    }

    fn specificity_len(&self) -> usize {
        match self {
            Self::Exact(d) | Self::WildcardSubdomain(d) => d.len(),
        }
    }
}

fn validate_domain_host(host: &str, original: &str) -> Result<(), MuteRuleError> {
    if host.is_empty() || host.starts_with('.') || host.ends_with('.') || host.contains("..") {
        return Err(MuteRuleError::InvalidPattern(original.to_string()));
    }
    if host.contains('*') || host.contains(':') {
        return Err(MuteRuleError::InvalidPattern(original.to_string()));
    }

    for label in host.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err(MuteRuleError::InvalidPattern(original.to_string()));
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(MuteRuleError::InvalidPattern(original.to_string()));
        }
        if !label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(MuteRuleError::InvalidPattern(original.to_string()));
        }
    }

    Ok(())
}

/// A persisted domain-level tab mute rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabMuteRule {
    pub pattern: String,
    pub action: MuteAction,
    pub priority: i32,
    pub enabled: bool,
    pub created_at_unix: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl TabMuteRule {
    /// Validates and constructs a new `TabMuteRule` with a normalized pattern.
    pub fn new(
        pattern: &str,
        action: MuteAction,
        priority: i32,
        enabled: bool,
        created_at_unix: i64,
        note: Option<String>,
    ) -> Result<Self, MuteRuleError> {
        let (normalized, _) = ParsedPattern::parse(pattern)?;
        Ok(Self {
            pattern: normalized,
            action,
            priority,
            enabled,
            created_at_unix,
            note,
        })
    }
}

/// Explains why a tab resolved to its effective mute state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EffectiveMuteReason {
    TabOverride,
    MatchedRule { pattern: String, action: MuteAction },
    DefaultUnmuted,
}

/// Effective mute state computed for a tab and URL/host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveMuteState {
    pub muted: bool,
    pub reason: EffectiveMuteReason,
}

/// Collection of domain mute rules and transient per-tab overrides.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabMuteRuleSet {
    rules: Vec<TabMuteRule>,
    #[serde(default)]
    tab_overrides: HashMap<u64, MuteAction>,
}

impl TabMuteRuleSet {
    /// Creates an empty rule set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a new rule. Fails if a rule with the same normalized pattern already exists.
    pub fn add_rule(
        &mut self,
        pattern: &str,
        action: MuteAction,
        priority: i32,
        created_at_unix: i64,
        note: Option<String>,
    ) -> Result<&TabMuteRule, MuteRuleError> {
        let rule = TabMuteRule::new(pattern, action, priority, true, created_at_unix, note)?;
        if self.rules.iter().any(|r| r.pattern == rule.pattern) {
            return Err(MuteRuleError::DuplicatePattern(rule.pattern));
        }
        self.rules.push(rule);
        self.rules
            .last()
            .ok_or_else(|| MuteRuleError::RuleNotFound(pattern.to_string()))
    }

    /// Inserts a rule or updates an existing rule with the same normalized pattern.
    pub fn upsert_rule(
        &mut self,
        pattern: &str,
        action: MuteAction,
        priority: i32,
        enabled: bool,
        created_at_unix: i64,
        note: Option<String>,
    ) -> Result<&TabMuteRule, MuteRuleError> {
        let rule = TabMuteRule::new(pattern, action, priority, enabled, created_at_unix, note)?;
        if let Some(pos) = self.rules.iter().position(|r| r.pattern == rule.pattern) {
            self.rules[pos] = rule;
            Ok(&self.rules[pos])
        } else {
            self.rules.push(rule);
            self.rules
                .last()
                .ok_or_else(|| MuteRuleError::RuleNotFound(pattern.to_string()))
        }
    }

    /// Removes a rule matching the given pattern.
    pub fn remove_rule(&mut self, pattern: &str) -> Result<TabMuteRule, MuteRuleError> {
        let (normalized, _) = ParsedPattern::parse(pattern)?;
        let pos = self
            .rules
            .iter()
            .position(|r| r.pattern == normalized)
            .ok_or(MuteRuleError::RuleNotFound(normalized))?;
        Ok(self.rules.remove(pos))
    }

    /// Enables or disables an existing rule.
    pub fn set_rule_enabled(&mut self, pattern: &str, enabled: bool) -> Result<(), MuteRuleError> {
        let (normalized, _) = ParsedPattern::parse(pattern)?;
        let rule = self
            .rules
            .iter_mut()
            .find(|r| r.pattern == normalized)
            .ok_or(MuteRuleError::RuleNotFound(normalized))?;
        rule.enabled = enabled;
        Ok(())
    }

    /// Returns all configured rules.
    pub fn rules(&self) -> &[TabMuteRule] {
        &self.rules
    }

    /// Sets a per-tab manual mute/unmute override that takes precedence over domain rules.
    pub fn set_tab_override(&mut self, tab_id: u64, action: MuteAction) {
        self.tab_overrides.insert(tab_id, action);
    }

    /// Clears any per-tab override for `tab_id`.
    pub fn clear_tab_override(&mut self, tab_id: u64) -> Option<MuteAction> {
        self.tab_overrides.remove(&tab_id)
    }

    /// Resolves the effective mute state for a full URL.
    pub fn resolve_for_url(
        &self,
        tab_id: Option<u64>,
        raw_url: &str,
    ) -> Result<EffectiveMuteState, MuteRuleError> {
        let parsed =
            Url::parse(raw_url).map_err(|_| MuteRuleError::InvalidUrl(raw_url.to_string()))?;
        let host = parsed
            .host_str()
            .ok_or_else(|| MuteRuleError::MissingHost(raw_url.to_string()))?;
        self.resolve_for_host(tab_id, host)
    }

    /// Resolves the effective mute state for a host name.
    pub fn resolve_for_host(
        &self,
        tab_id: Option<u64>,
        host: &str,
    ) -> Result<EffectiveMuteState, MuteRuleError> {
        if let Some(id) = tab_id {
            if let Some(action) = self.tab_overrides.get(&id).copied() {
                return Ok(EffectiveMuteState {
                    muted: action.is_muted(),
                    reason: EffectiveMuteReason::TabOverride,
                });
            }
        }

        let normalized_host = host.trim().trim_end_matches('.').to_ascii_lowercase();
        if normalized_host.is_empty() {
            return Err(MuteRuleError::MissingHost(host.to_string()));
        }

        let mut best: Option<(&TabMuteRule, ParsedPattern)> = None;

        for rule in self.rules.iter().filter(|r| r.enabled) {
            let Ok((_, parsed)) = ParsedPattern::parse(&rule.pattern) else {
                continue;
            };
            if !parsed.matches(&normalized_host) {
                continue;
            }

            let replace = match &best {
                None => true,
                Some((current_rule, current_parsed)) => {
                    is_more_specific(rule, &parsed, current_rule, current_parsed)
                }
            };
            if replace {
                best = Some((rule, parsed));
            }
        }

        if let Some((rule, _)) = best {
            Ok(EffectiveMuteState {
                muted: rule.action.is_muted(),
                reason: EffectiveMuteReason::MatchedRule {
                    pattern: rule.pattern.clone(),
                    action: rule.action,
                },
            })
        } else {
            Ok(EffectiveMuteState {
                muted: false,
                reason: EffectiveMuteReason::DefaultUnmuted,
            })
        }
    }

    /// Serializes the rule set to JSON.
    pub fn to_json(&self) -> Result<String, MuteRuleError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Deserializes a rule set from JSON and validates every rule pattern.
    pub fn from_json(json: &str) -> Result<Self, MuteRuleError> {
        let mut loaded: Self = serde_json::from_str(json)?;
        let mut seen = Vec::with_capacity(loaded.rules.len());
        for rule in &mut loaded.rules {
            let (normalized, _) = ParsedPattern::parse(&rule.pattern)?;
            if seen.contains(&normalized) {
                return Err(MuteRuleError::DuplicatePattern(normalized));
            }
            rule.pattern = normalized.clone();
            seen.push(normalized);
        }
        Ok(loaded)
    }
}

fn is_more_specific(
    candidate_rule: &TabMuteRule,
    candidate_parsed: &ParsedPattern,
    current_rule: &TabMuteRule,
    current_parsed: &ParsedPattern,
) -> bool {
    if candidate_rule.priority != current_rule.priority {
        return candidate_rule.priority > current_rule.priority;
    }
    if candidate_parsed.is_exact() != current_parsed.is_exact() {
        return candidate_parsed.is_exact();
    }
    let cand_len = candidate_parsed.specificity_len();
    let curr_len = current_parsed.specificity_len();
    if cand_len != curr_len {
        return cand_len > curr_len;
    }
    candidate_rule.pattern < current_rule.pattern
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_and_wildcard_matching_and_precedence() {
        let mut set = TabMuteRuleSet::new();
        set.add_rule("*.example.com", MuteAction::Mute, 0, 100, None)
            .unwrap();
        set.add_rule("music.example.com", MuteAction::Unmute, 0, 101, None)
            .unwrap();

        // Apex domain does not match `*.example.com`.
        let apex = set
            .resolve_for_url(None, "https://example.com/watch")
            .unwrap();
        assert!(!apex.muted);
        assert_eq!(apex.reason, EffectiveMuteReason::DefaultUnmuted);

        // Any other subdomain matches wildcard mute.
        let video = set
            .resolve_for_url(None, "https://video.example.com/clip")
            .unwrap();
        assert!(video.muted);
        assert_eq!(
            video.reason,
            EffectiveMuteReason::MatchedRule {
                pattern: "*.example.com".to_string(),
                action: MuteAction::Mute,
            }
        );

        // Exact rule beats wildcard rule at equal priority.
        let music = set
            .resolve_for_url(None, "https://MUSIC.EXAMPLE.COM:8443/track")
            .unwrap();
        assert!(!music.muted);
        assert_eq!(
            music.reason,
            EffectiveMuteReason::MatchedRule {
                pattern: "music.example.com".to_string(),
                action: MuteAction::Unmute,
            }
        );
    }

    #[test]
    fn tab_override_and_disabled_rules() {
        let mut set = TabMuteRuleSet::new();
        set.add_rule(
            "news.org",
            MuteAction::Mute,
            10,
            200,
            Some("autoplay".into()),
        )
        .unwrap();

        // Tab override wins over domain rule.
        set.set_tab_override(42, MuteAction::Unmute);
        let overridden = set
            .resolve_for_url(Some(42), "https://news.org/live")
            .unwrap();
        assert!(!overridden.muted);
        assert_eq!(overridden.reason, EffectiveMuteReason::TabOverride);

        // Clearing override restores rule evaluation.
        assert_eq!(set.clear_tab_override(42), Some(MuteAction::Unmute));
        let rule_state = set
            .resolve_for_url(Some(42), "https://news.org/live")
            .unwrap();
        assert!(rule_state.muted);

        // Disabling rule falls back to default unmuted.
        set.set_rule_enabled("NEWS.ORG", false).unwrap();
        let disabled_state = set
            .resolve_for_url(Some(42), "https://news.org/live")
            .unwrap();
        assert!(!disabled_state.muted);
        assert_eq!(disabled_state.reason, EffectiveMuteReason::DefaultUnmuted);
    }

    #[test]
    fn validation_and_json_roundtrip() {
        let mut set = TabMuteRuleSet::new();
        assert_eq!(
            set.add_rule("", MuteAction::Mute, 0, 1, None),
            Err(MuteRuleError::EmptyPattern)
        );
        assert!(matches!(
            set.add_rule("https://example.com", MuteAction::Mute, 0, 1, None),
            Err(MuteRuleError::InvalidPattern(_))
        ));
        assert!(matches!(
            set.add_rule("*.*.example.com", MuteAction::Mute, 0, 1, None),
            Err(MuteRuleError::InvalidPattern(_))
        ));
        assert!(matches!(
            set.add_rule("*.com", MuteAction::Mute, 0, 1, None),
            Err(MuteRuleError::InvalidPattern(_))
        ));

        set.add_rule("  *.Social.Example  ", MuteAction::Mute, 5, 300, None)
            .unwrap();
        assert_eq!(
            set.add_rule("*.social.example", MuteAction::Unmute, 1, 301, None),
            Err(MuteRuleError::DuplicatePattern("*.social.example".into()))
        );

        let json = set.to_json().unwrap();
        let restored = TabMuteRuleSet::from_json(&json).unwrap();
        assert_eq!(set, restored);

        let bad_json = r#"{"rules":[{"pattern":"bad/path","action":"mute","priority":0,"enabled":true,"created_at_unix":1}]}"#;
        assert!(matches!(
            TabMuteRuleSet::from_json(bad_json),
            Err(MuteRuleError::InvalidPattern(_))
        ));
    }
}
