//! Curtain Preset Packs: curated, versioned, and verifiable CSS element-hiding
//! rule packs for Search Linux's Curtain system.
//!
//! Provides domain types, strict selector/host validation, SHA-256 integrity
//! verification, per-pack and per-rule toggle state, URL/host matching,
//! isolated CSS compilation, and JSON import/export.

use ring::digest;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;
use url::Url;

/// Maximum allowed length for pack and rule identifiers.
const MAX_ID_LEN: usize = 64;

/// Maximum allowed length for a CSS selector in a preset rule.
const MAX_SELECTOR_LEN: usize = 512;

/// Maximum allowed length for a host pattern.
const MAX_HOST_PATTERN_LEN: usize = 253;

/// Errors returned by Curtain Preset Pack operations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PresetPackError {
    #[error("pack id must not be empty")]
    EmptyPackId,
    #[error("invalid pack id '{0}': must be 1..=64 ASCII alphanumeric, '-', or '_' characters")]
    InvalidPackId(String),
    #[error("rule id must not be empty")]
    EmptyRuleId,
    #[error("invalid rule id '{0}': must be 1..=64 ASCII alphanumeric, '-', or '_' characters")]
    InvalidRuleId(String),
    #[error("duplicate rule id '{0}' within pack '{1}'")]
    DuplicateRuleId(String, String),
    #[error("pack '{0}' has an empty name")]
    EmptyPackName(String),
    #[error("pack '{0}' has an empty version")]
    EmptyPackVersion(String),
    #[error("invalid host pattern '{0}'")]
    InvalidHostPattern(String),
    #[error("invalid CSS selector '{selector}': {reason}")]
    InvalidSelector { selector: String, reason: String },
    #[error("pack '{0}' already exists")]
    DuplicatePackId(String),
    #[error("pack '{0}' not found")]
    PackNotFound(String),
    #[error("rule '{rule_id}' not found in pack '{pack_id}'")]
    RuleNotFound { pack_id: String, rule_id: String },
    #[error("checksum mismatch for pack '{pack_id}': expected {expected}, got {actual}")]
    ChecksumMismatch {
        pack_id: String,
        expected: String,
        actual: String,
    },
    #[error("invalid URL '{0}': {1}")]
    InvalidUrl(String, String),
    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Logical category for a Curtain preset pack.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetCategory {
    CookieBanners,
    NewsletterModals,
    SocialWidgets,
    StickyHeaders,
    ChatWidgets,
    ShortsAndReels,
    SponsoredCards,
    DistractionFreeReading,
    Custom(String),
}

/// A single CSS element-hiding rule inside a preset pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresetRule {
    pub id: String,
    pub host_pattern: String,
    pub selector: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default = "default_true")]
    pub enabled_by_default: bool,
}

fn default_true() -> bool {
    true
}

/// A versioned collection of Curtain rules targeting a common annoyance or workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurtainPresetPack {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub category: PresetCategory,
    pub rules: Vec<PresetRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum_sha256: Option<String>,
}

/// User installation and override state for a preset pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledPackState {
    pub pack_id: String,
    pub enabled: bool,
    #[serde(default)]
    pub disabled_rule_ids: BTreeSet<String>,
    #[serde(default)]
    pub enabled_rule_overrides: BTreeSet<String>,
    pub installed_at_unix: i64,
    pub updated_at_unix: i64,
}

/// A resolved rule ready to be compiled into Curtain's injected CSS stylesheet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedVeil {
    pub pack_id: String,
    pub rule_id: String,
    pub selector: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub category: PresetCategory,
}

/// Canonical payload used for SHA-256 pack checksums.
#[derive(Serialize)]
struct ChecksumPayload<'a> {
    id: &'a str,
    version: &'a str,
    rules: &'a [PresetRule],
}

/// Persistent catalog managing available preset packs and their per-user activation state.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresetPackCatalog {
    pub packs: BTreeMap<String, CurtainPresetPack>,
    pub states: BTreeMap<String, InstalledPackState>,
}

impl PresetPackCatalog {
    /// Creates an empty catalog.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a catalog pre-populated with built-in Search Linux preset packs.
    /// Built-in packs are installed in a disabled state by default unless explicitly enabled.
    pub fn with_builtins(now_unix: i64) -> Self {
        let mut catalog = Self::new();
        for mut pack in builtin_preset_packs() {
            if let Ok(sum) = compute_pack_checksum(&pack) {
                pack.checksum_sha256 = Some(sum);
            }
            let _ = catalog.install_pack(pack, false, now_unix);
        }
        catalog
    }

    /// Validates and installs a preset pack into the catalog.
    pub fn install_pack(
        &mut self,
        mut pack: CurtainPresetPack,
        enabled: bool,
        now_unix: i64,
    ) -> Result<(), PresetPackError> {
        validate_pack(&pack)?;
        if self.packs.contains_key(&pack.id) {
            return Err(PresetPackError::DuplicatePackId(pack.id));
        }

        // Normalize host patterns on install for deterministic matching.
        for rule in &mut pack.rules {
            rule.host_pattern = normalize_host_pattern(&rule.host_pattern)?;
        }

        // Recompute checksum if none was provided; if one was provided, validate_pack already checked it.
        if pack.checksum_sha256.is_none() {
            pack.checksum_sha256 = Some(compute_pack_checksum(&pack)?);
        }

        let pack_id = pack.id.clone();
        self.packs.insert(pack_id.clone(), pack);
        self.states.insert(
            pack_id.clone(),
            InstalledPackState {
                pack_id,
                enabled,
                disabled_rule_ids: BTreeSet::new(),
                enabled_rule_overrides: BTreeSet::new(),
                installed_at_unix: now_unix,
                updated_at_unix: now_unix,
            },
        );
        Ok(())
    }

    /// Upgrades or replaces an existing pack, preserving rule overrides for rules that still exist.
    pub fn update_pack(
        &mut self,
        mut pack: CurtainPresetPack,
        now_unix: i64,
    ) -> Result<(), PresetPackError> {
        validate_pack(&pack)?;
        if !self.packs.contains_key(&pack.id) {
            return Err(PresetPackError::PackNotFound(pack.id));
        }

        for rule in &mut pack.rules {
            rule.host_pattern = normalize_host_pattern(&rule.host_pattern)?;
        }
        if pack.checksum_sha256.is_none() {
            pack.checksum_sha256 = Some(compute_pack_checksum(&pack)?);
        }

        let valid_rule_ids: BTreeSet<String> = pack.rules.iter().map(|r| r.id.clone()).collect();
        if let Some(state) = self.states.get_mut(&pack.id) {
            state
                .disabled_rule_ids
                .retain(|id| valid_rule_ids.contains(id));
            state
                .enabled_rule_overrides
                .retain(|id| valid_rule_ids.contains(id));
            state.updated_at_unix = now_unix;
        }

        self.packs.insert(pack.id.clone(), pack);
        Ok(())
    }

    /// Removes a pack and its state from the catalog.
    pub fn uninstall_pack(&mut self, pack_id: &str) -> Result<CurtainPresetPack, PresetPackError> {
        self.states.remove(pack_id);
        self.packs
            .remove(pack_id)
            .ok_or_else(|| PresetPackError::PackNotFound(pack_id.to_string()))
    }

    /// Enables or disables an installed pack.
    pub fn set_pack_enabled(
        &mut self,
        pack_id: &str,
        enabled: bool,
        now_unix: i64,
    ) -> Result<(), PresetPackError> {
        let state = self
            .states
            .get_mut(pack_id)
            .ok_or_else(|| PresetPackError::PackNotFound(pack_id.to_string()))?;
        state.enabled = enabled;
        state.updated_at_unix = now_unix;
        Ok(())
    }

    /// Enables or disables a specific rule within an installed pack.
    pub fn set_rule_enabled(
        &mut self,
        pack_id: &str,
        rule_id: &str,
        enabled: bool,
        now_unix: i64,
    ) -> Result<(), PresetPackError> {
        let pack = self
            .packs
            .get(pack_id)
            .ok_or_else(|| PresetPackError::PackNotFound(pack_id.to_string()))?;
        let rule = pack.rules.iter().find(|r| r.id == rule_id).ok_or_else(|| {
            PresetPackError::RuleNotFound {
                pack_id: pack_id.to_string(),
                rule_id: rule_id.to_string(),
            }
        })?;

        let state = self
            .states
            .get_mut(pack_id)
            .ok_or_else(|| PresetPackError::PackNotFound(pack_id.to_string()))?;

        if enabled {
            state.disabled_rule_ids.remove(rule_id);
            if !rule.enabled_by_default {
                state.enabled_rule_overrides.insert(rule_id.to_string());
            }
        } else {
            state.enabled_rule_overrides.remove(rule_id);
            if rule.enabled_by_default {
                state.disabled_rule_ids.insert(rule_id.to_string());
            }
        }
        state.updated_at_unix = now_unix;
        Ok(())
    }

    /// Returns whether a rule in a pack is currently active (assuming its pack is enabled).
    pub fn is_rule_active(&self, pack_id: &str, rule_id: &str) -> Result<bool, PresetPackError> {
        let pack = self
            .packs
            .get(pack_id)
            .ok_or_else(|| PresetPackError::PackNotFound(pack_id.to_string()))?;
        let rule = pack.rules.iter().find(|r| r.id == rule_id).ok_or_else(|| {
            PresetPackError::RuleNotFound {
                pack_id: pack_id.to_string(),
                rule_id: rule_id.to_string(),
            }
        })?;
        let state = self
            .states
            .get(pack_id)
            .ok_or_else(|| PresetPackError::PackNotFound(pack_id.to_string()))?;

        if !state.enabled {
            return Ok(false);
        }
        if state.disabled_rule_ids.contains(rule_id) {
            return Ok(false);
        }
        if state.enabled_rule_overrides.contains(rule_id) {
            return Ok(true);
        }
        Ok(rule.enabled_by_default)
    }

    /// Resolves all active preset rules matching the given host (with or without `www.`).
    /// Deduplicates identical selectors deterministically.
    pub fn resolve_for_host(&self, raw_host: &str) -> Vec<ResolvedVeil> {
        let normalized_host = normalize_host(raw_host);
        if normalized_host.is_empty() {
            return Vec::new();
        }

        let mut resolved = Vec::new();
        let mut seen_selectors = BTreeSet::new();

        for (pack_id, pack) in &self.packs {
            let Some(state) = self.states.get(pack_id) else {
                continue;
            };
            if !state.enabled {
                continue;
            }

            for rule in &pack.rules {
                let active = if state.disabled_rule_ids.contains(&rule.id) {
                    false
                } else if state.enabled_rule_overrides.contains(&rule.id) {
                    true
                } else {
                    rule.enabled_by_default
                };

                if !active {
                    continue;
                }

                if host_matches_pattern(&normalized_host, &rule.host_pattern)
                    && seen_selectors.insert(rule.selector.clone())
                {
                    resolved.push(ResolvedVeil {
                        pack_id: pack.id.clone(),
                        rule_id: rule.id.clone(),
                        selector: rule.selector.clone(),
                        label: rule.label.clone(),
                        note: rule.note.clone(),
                        category: pack.category.clone(),
                    });
                }
            }
        }

        resolved
    }

    /// Resolves all active preset rules matching the host of an HTTP/HTTPS URL.
    pub fn resolve_for_url(&self, raw_url: &str) -> Result<Vec<ResolvedVeil>, PresetPackError> {
        let parsed = Url::parse(raw_url)
            .map_err(|err| PresetPackError::InvalidUrl(raw_url.to_string(), err.to_string()))?;
        let host = parsed.host_str().ok_or_else(|| {
            PresetPackError::InvalidUrl(raw_url.to_string(), "missing host".into())
        })?;
        Ok(self.resolve_for_host(host))
    }

    /// Serializes a single pack to pretty-printed JSON with its SHA-256 checksum populated.
    pub fn export_pack_json(&self, pack_id: &str) -> Result<String, PresetPackError> {
        let mut pack = self
            .packs
            .get(pack_id)
            .cloned()
            .ok_or_else(|| PresetPackError::PackNotFound(pack_id.to_string()))?;
        pack.checksum_sha256 = Some(compute_pack_checksum(&pack)?);
        serde_json::to_string_pretty(&pack)
            .map_err(|err| PresetPackError::Serialization(err.to_string()))
    }

    /// Parses, validates, verifies checksum (if present), and installs a pack from JSON.
    pub fn import_pack_json(
        &mut self,
        json: &str,
        enabled: bool,
        now_unix: i64,
    ) -> Result<String, PresetPackError> {
        let pack: CurtainPresetPack = serde_json::from_str(json)
            .map_err(|err| PresetPackError::Serialization(err.to_string()))?;
        let pack_id = pack.id.clone();
        self.install_pack(pack, enabled, now_unix)?;
        Ok(pack_id)
    }

    /// Serializes the entire catalog to JSON.
    pub fn to_json(&self) -> Result<String, PresetPackError> {
        serde_json::to_string_pretty(self)
            .map_err(|err| PresetPackError::Serialization(err.to_string()))
    }

    /// Deserializes a catalog from JSON and validates all contained packs.
    pub fn from_json(json: &str) -> Result<Self, PresetPackError> {
        let catalog: Self = serde_json::from_str(json)
            .map_err(|err| PresetPackError::Serialization(err.to_string()))?;
        for pack in catalog.packs.values() {
            validate_pack(pack)?;
        }
        Ok(catalog)
    }
}

/// Validates a `CurtainPresetPack`, including its metadata, rules, and optional checksum.
pub fn validate_pack(pack: &CurtainPresetPack) -> Result<(), PresetPackError> {
    validate_identifier(&pack.id, true)?;
    if pack.name.trim().is_empty() {
        return Err(PresetPackError::EmptyPackName(pack.id.clone()));
    }
    if pack.version.trim().is_empty() {
        return Err(PresetPackError::EmptyPackVersion(pack.id.clone()));
    }

    let mut seen_rule_ids = BTreeSet::new();
    for rule in &pack.rules {
        validate_identifier(&rule.id, false)?;
        if !seen_rule_ids.insert(rule.id.clone()) {
            return Err(PresetPackError::DuplicateRuleId(
                rule.id.clone(),
                pack.id.clone(),
            ));
        }
        normalize_host_pattern(&rule.host_pattern)?;
        validate_css_selector(&rule.selector)?;
    }

    if let Some(expected) = &pack.checksum_sha256 {
        let actual = compute_pack_checksum(pack)?;
        if !expected.eq_ignore_ascii_case(&actual) {
            return Err(PresetPackError::ChecksumMismatch {
                pack_id: pack.id.clone(),
                expected: expected.clone(),
                actual,
            });
        }
    }

    Ok(())
}

/// Validates a pack or rule identifier (`1..=64` ASCII alphanumeric, `-`, or `_`).
fn validate_identifier(id: &str, is_pack: bool) -> Result<(), PresetPackError> {
    if id.is_empty() {
        return Err(if is_pack {
            PresetPackError::EmptyPackId
        } else {
            PresetPackError::EmptyRuleId
        });
    }
    let valid = id.len() <= MAX_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if !valid {
        return Err(if is_pack {
            PresetPackError::InvalidPackId(id.to_string())
        } else {
            PresetPackError::InvalidRuleId(id.to_string())
        });
    }
    Ok(())
}

/// Normalizes a host string to lowercase without leading `www.` or trailing `.`.
pub fn normalize_host(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('.').to_ascii_lowercase();
    trimmed.strip_prefix("www.").unwrap_or(&trimmed).to_string()
}

/// Validates and normalizes a host pattern (`*`, `*.domain.tld`, or `domain.tld`).
pub fn normalize_host_pattern(pattern: &str) -> Result<String, PresetPackError> {
    let trimmed = pattern.trim().trim_end_matches('.').to_ascii_lowercase();
    if trimmed.is_empty() || trimmed.len() > MAX_HOST_PATTERN_LEN {
        return Err(PresetPackError::InvalidHostPattern(pattern.to_string()));
    }
    if trimmed == "*" {
        return Ok(trimmed);
    }

    let (is_wildcard, domain_part) = if let Some(rest) = trimmed.strip_prefix("*.") {
        (true, rest)
    } else {
        (false, trimmed.as_str())
    };

    let domain_part = domain_part.strip_prefix("www.").unwrap_or(domain_part);
    if domain_part.is_empty()
        || domain_part.contains('*')
        || domain_part.contains('/')
        || domain_part.contains(':')
        || domain_part.chars().any(char::is_whitespace)
    {
        return Err(PresetPackError::InvalidHostPattern(pattern.to_string()));
    }

    // Require valid hostname characters on each label.
    for label in domain_part.split('.') {
        if label.is_empty()
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(PresetPackError::InvalidHostPattern(pattern.to_string()));
        }
    }

    if is_wildcard {
        Ok(format!("*.{domain_part}"))
    } else {
        Ok(domain_part.to_string())
    }
}

/// Checks whether a normalized host matches a normalized host pattern.
pub fn host_matches_pattern(normalized_host: &str, normalized_pattern: &str) -> bool {
    if normalized_pattern == "*" {
        return true;
    }
    if let Some(base) = normalized_pattern.strip_prefix("*.") {
        if normalized_host == base {
            return true;
        }
        if let Some(prefix) = normalized_host.strip_suffix(base) {
            return prefix.ends_with('.');
        }
        return false;
    }
    normalized_host == normalized_pattern
}

/// Validates a CSS selector to ensure it is non-empty, balanced, and cannot break out of a rule block.
pub fn validate_css_selector(selector: &str) -> Result<(), PresetPackError> {
    let trimmed = selector.trim();
    if trimmed.is_empty() {
        return Err(PresetPackError::InvalidSelector {
            selector: selector.to_string(),
            reason: "selector must not be empty".to_string(),
        });
    }
    if trimmed.len() > MAX_SELECTOR_LEN {
        return Err(PresetPackError::InvalidSelector {
            selector: selector.to_string(),
            reason: format!("selector exceeds {MAX_SELECTOR_LEN} bytes"),
        });
    }

    let mut paren_depth: usize = 0;
    let mut bracket_depth: usize = 0;
    let mut in_quote: Option<char> = None;
    let mut escaped = false;

    for ch in trimmed.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(q) = in_quote {
            if ch == q {
                in_quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => in_quote = Some(ch),
            '(' => paren_depth += 1,
            ')' => {
                if paren_depth == 0 {
                    return Err(PresetPackError::InvalidSelector {
                        selector: selector.to_string(),
                        reason: "unbalanced ')'".to_string(),
                    });
                }
                paren_depth -= 1;
            }
            '[' => bracket_depth += 1,
            ']' => {
                if bracket_depth == 0 {
                    return Err(PresetPackError::InvalidSelector {
                        selector: selector.to_string(),
                        reason: "unbalanced ']'".to_string(),
                    });
                }
                bracket_depth -= 1;
            }
            '{' | '}' | '<' | ';' | '\n' | '\r' => {
                return Err(PresetPackError::InvalidSelector {
                    selector: selector.to_string(),
                    reason: format!("disallowed character '{ch}' outside quoted string"),
                });
            }
            _ => {}
        }
    }

    if escaped || in_quote.is_some() || paren_depth != 0 || bracket_depth != 0 {
        return Err(PresetPackError::InvalidSelector {
            selector: selector.to_string(),
            reason: "unterminated quote, escape, parenthesis, or bracket".to_string(),
        });
    }

    Ok(())
}

/// Computes the canonical SHA-256 hex digest of a pack's identity, version, and rules.
pub fn compute_pack_checksum(pack: &CurtainPresetPack) -> Result<String, PresetPackError> {
    let payload = ChecksumPayload {
        id: &pack.id,
        version: &pack.version,
        rules: &pack.rules,
    };
    let bytes = serde_json::to_vec(&payload)
        .map_err(|err| PresetPackError::Serialization(err.to_string()))?;
    let hash = digest::digest(&digest::SHA256, &bytes);
    let mut hex = String::with_capacity(hash.as_ref().len() * 2);
    for byte in hash.as_ref() {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    Ok(hex)
}

/// Compiles resolved veils into an isolated CSS stylesheet where each selector occupies
/// its own rule block so a single invalid selector in the browser never invalidates others.
pub fn compile_isolated_css(veils: &[ResolvedVeil]) -> String {
    let mut out = String::new();
    for veil in veils {
        out.push_str(&veil.selector);
        out.push_str(" { display: none !important; }\n");
    }
    out
}

/// Returns the built-in curated Curtain preset packs shipped with Search Linux.
pub fn builtin_preset_packs() -> Vec<CurtainPresetPack> {
    vec![
        CurtainPresetPack {
            id: "cookie-banners".to_string(),
            name: "Cookie Consent Banners".to_string(),
            description: "Hides common GDPR and cookie consent overlays across the web."
                .to_string(),
            version: "1.0.0".to_string(),
            author: "Search Linux".to_string(),
            category: PresetCategory::CookieBanners,
            rules: vec![
                PresetRule {
                    id: "onetrust-banner".to_string(),
                    host_pattern: "*".to_string(),
                    selector: "#onetrust-consent-sdk".to_string(),
                    label: "OneTrust consent banner".to_string(),
                    note: None,
                    enabled_by_default: true,
                },
                PresetRule {
                    id: "cookiebot-dialog".to_string(),
                    host_pattern: "*".to_string(),
                    selector: "#CybotCookiebotDialog".to_string(),
                    label: "Cookiebot consent dialog".to_string(),
                    note: None,
                    enabled_by_default: true,
                },
            ],
            checksum_sha256: None,
        },
        CurtainPresetPack {
            id: "clean-video-feeds".to_string(),
            name: "Clean Video Feeds".to_string(),
            description: "Hides short-form video shelves and distracting end-screen cards."
                .to_string(),
            version: "1.0.0".to_string(),
            author: "Search Linux".to_string(),
            category: PresetCategory::ShortsAndReels,
            rules: vec![
                PresetRule {
                    id: "yt-shorts-shelf".to_string(),
                    host_pattern: "*.youtube.com".to_string(),
                    selector: "ytd-rich-shelf-renderer[is-shorts]".to_string(),
                    label: "YouTube Shorts home shelf".to_string(),
                    note: None,
                    enabled_by_default: true,
                },
                PresetRule {
                    id: "yt-endscreen-cards".to_string(),
                    host_pattern: "youtube.com".to_string(),
                    selector: ".ytp-ce-element".to_string(),
                    label: "YouTube end-screen overlay cards".to_string(),
                    note: Some("Optional rule for uninterrupted video endings".to_string()),
                    enabled_by_default: false,
                },
            ],
            checksum_sha256: None,
        },
        CurtainPresetPack {
            id: "distraction-free-reading".to_string(),
            name: "Distraction-Free Reading".to_string(),
            description: "Hides newsletter signup modals and floating chat widgets.".to_string(),
            version: "1.0.0".to_string(),
            author: "Search Linux".to_string(),
            category: PresetCategory::DistractionFreeReading,
            rules: vec![
                PresetRule {
                    id: "intercom-launcher".to_string(),
                    host_pattern: "*".to_string(),
                    selector: "#intercom-container, .intercom-lightweight-app".to_string(),
                    label: "Intercom chat launcher".to_string(),
                    note: None,
                    enabled_by_default: true,
                },
                PresetRule {
                    id: "substack-subscribe-footer".to_string(),
                    host_pattern: "*.substack.com".to_string(),
                    selector: ".subscribe-widget".to_string(),
                    label: "Substack inline subscribe prompt".to_string(),
                    note: None,
                    enabled_by_default: true,
                },
            ],
            checksum_sha256: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_install_toggle_and_resolve_for_url() {
        let mut catalog = PresetPackCatalog::with_builtins(1_700_000_000);
        // Disabled by default upon installation.
        let initial = catalog
            .resolve_for_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ")
            .unwrap();
        assert!(initial.is_empty());

        // Enable cookie-banners and clean-video-feeds.
        catalog
            .set_pack_enabled("cookie-banners", true, 1_700_000_010)
            .unwrap();
        catalog
            .set_pack_enabled("clean-video-feeds", true, 1_700_000_010)
            .unwrap();

        let veils = catalog
            .resolve_for_url("https://www.youtube.com/watch?v=123")
            .unwrap();
        let ids: Vec<&str> = veils.iter().map(|v| v.rule_id.as_str()).collect();
        assert!(ids.contains(&"onetrust-banner"));
        assert!(ids.contains(&"cookiebot-dialog"));
        assert!(ids.contains(&"yt-shorts-shelf"));
        // yt-endscreen-cards has enabled_by_default = false.
        assert!(!ids.contains(&"yt-endscreen-cards"));

        // Opt-in to the optional rule and disable a default rule.
        catalog
            .set_rule_enabled(
                "clean-video-feeds",
                "yt-endscreen-cards",
                true,
                1_700_000_020,
            )
            .unwrap();
        catalog
            .set_rule_enabled("clean-video-feeds", "yt-shorts-shelf", false, 1_700_000_020)
            .unwrap();

        let updated = catalog.resolve_for_host("m.youtube.com");
        let updated_ids: Vec<&str> = updated.iter().map(|v| v.rule_id.as_str()).collect();
        // *.youtube.com matches m.youtube.com, but exact youtube.com does not.
        assert!(!updated_ids.contains(&"yt-shorts-shelf"));
        assert!(!updated_ids.contains(&"yt-endscreen-cards"));

        let root_yt = catalog.resolve_for_host("www.youtube.com");
        let root_ids: Vec<&str> = root_yt.iter().map(|v| v.rule_id.as_str()).collect();
        assert!(root_ids.contains(&"yt-endscreen-cards"));

        let css = compile_isolated_css(&root_yt);
        assert!(css.contains(".ytp-ce-element { display: none !important; }\n"));
    }

    #[test]
    fn import_export_roundtrip_and_checksum_verification() {
        let catalog = PresetPackCatalog::with_builtins(1_700_000_000);
        let exported = catalog.export_pack_json("cookie-banners").unwrap();

        let mut fresh = PresetPackCatalog::new();
        let imported_id = fresh
            .import_pack_json(&exported, true, 1_700_000_100)
            .unwrap();
        assert_eq!(imported_id, "cookie-banners");
        assert_eq!(fresh.resolve_for_host("example.org").len(), 2);

        // Tamper with a selector in the exported JSON without updating checksum_sha256.
        let tampered = exported.replace("#CybotCookiebotDialog", "#TamperedDialog");
        let mut target = PresetPackCatalog::new();
        let err = target
            .import_pack_json(&tampered, true, 1_700_000_200)
            .unwrap_err();
        assert!(matches!(err, PresetPackError::ChecksumMismatch { .. }));
    }

    #[test]
    fn validation_rejects_unsafe_selectors_bad_hosts_and_duplicate_rules() {
        assert!(validate_css_selector("div[data-testid='banner'] > span").is_ok());
        assert!(validate_css_selector("div > span").is_ok());
        // Wait: '>' combinator is valid CSS! Let's verify '>' is allowed in CSS selectors.
        // Actually in our match arm above we had `ch == '<'`, not `'>'`! Let's double check.
        assert!(validate_css_selector("div { display: block; }").is_err());
        assert!(validate_css_selector("div[unclosed").is_err());
        assert!(validate_css_selector("   ").is_err());

        assert_eq!(
            normalize_host_pattern("*.WWW.Example.COM.").unwrap(),
            "*.example.com"
        );
        assert!(normalize_host_pattern("https://example.com").is_err());
        assert!(normalize_host_pattern("bad host.com").is_err());

        let dup_pack = CurtainPresetPack {
            id: "dup-pack".to_string(),
            name: "Dup".to_string(),
            description: "Test".to_string(),
            version: "1.0.0".to_string(),
            author: "Tester".to_string(),
            category: PresetCategory::Custom("test".to_string()),
            rules: vec![
                PresetRule {
                    id: "same-id".to_string(),
                    host_pattern: "*".to_string(),
                    selector: ".a".to_string(),
                    label: "A".to_string(),
                    note: None,
                    enabled_by_default: true,
                },
                PresetRule {
                    id: "same-id".to_string(),
                    host_pattern: "*".to_string(),
                    selector: ".b".to_string(),
                    label: "B".to_string(),
                    note: None,
                    enabled_by_default: true,
                },
            ],
            checksum_sha256: None,
        };
        assert_eq!(
            validate_pack(&dup_pack).unwrap_err(),
            PresetPackError::DuplicateRuleId("same-id".to_string(), "dup-pack".to_string())
        );
    }

    #[test]
    fn catalog_json_roundtrip_and_deduplication() {
        let mut catalog = PresetPackCatalog::with_builtins(1_700_000_000);
        let custom = CurtainPresetPack {
            id: "custom-dedup".to_string(),
            name: "Custom Dedup".to_string(),
            description: "Shares a selector with cookie-banners".to_string(),
            version: "1.0.0".to_string(),
            author: "User".to_string(),
            category: PresetCategory::Custom("local".to_string()),
            rules: vec![PresetRule {
                id: "dup-onetrust".to_string(),
                host_pattern: "example.com".to_string(),
                selector: "#onetrust-consent-sdk".to_string(),
                label: "Duplicate OneTrust".to_string(),
                note: None,
                enabled_by_default: true,
            }],
            checksum_sha256: None,
        };
        catalog.install_pack(custom, true, 1_700_000_050).unwrap();
        catalog
            .set_pack_enabled("cookie-banners", true, 1_700_000_050)
            .unwrap();

        let serialized = catalog.to_json().unwrap();
        let restored = PresetPackCatalog::from_json(&serialized).unwrap();
        assert_eq!(catalog, restored);

        let veils = restored.resolve_for_host("example.com");
        let onetrust_count = veils
            .iter()
            .filter(|v| v.selector == "#onetrust-consent-sdk")
            .count();
        assert_eq!(onetrust_count, 1);
    }
}
