//! Cookie banner detection, selector matching, and consent-action rules.
//!
//! Provides domain models, validation, built-in CMP (Consent Management Platform)
//! rulesets, host/URL matching, and CSS hiding snippet generation.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

/// Action to take when a cookie consent banner is matched on a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ConsentAction {
    /// Click a "Reject All" / "Decline" button if present, otherwise hide.
    #[default]
    RejectAll,
    /// Click a "Save / Accept Necessary Only" button if present.
    AcceptNecessary,
    /// Hide the banner and any modal backdrop via CSS without clicking.
    HideOnly,
}

/// Errors produced when validating or evaluating cookie banner selector rules.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SelectorError {
    #[error("rule id must not be empty")]
    EmptyRuleId,
    #[error("rule name must not be empty")]
    EmptyRuleName,
    #[error("rule '{rule_id}' must define at least one banner selector")]
    EmptyBannerSelectors { rule_id: String },
    #[error("invalid CSS selector '{selector}' in rule '{rule_id}': {reason}")]
    InvalidSelector {
        rule_id: String,
        selector: String,
        reason: String,
    },
    #[error("invalid domain pattern '{pattern}'")]
    InvalidDomainPattern { pattern: String },
    #[error("duplicate rule id '{rule_id}'")]
    DuplicateRuleId { rule_id: String },
    #[error("invalid URL '{url}': {reason}")]
    InvalidUrl { url: String, reason: String },
    #[error("serialization error: {0}")]
    Serialization(String),
}

/// A single cookie banner detection and dismissal rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BannerRule {
    /// Unique identifier for the rule (e.g., `"onetrust"`).
    pub id: String,
    /// Human-readable name of the CMP or site rule.
    pub name: String,
    /// Host patterns this rule applies to. Empty means universal (all hosts).
    #[serde(default)]
    pub domains: Vec<String>,
    /// CSS selectors matching the cookie banner root element(s).
    pub banner_selectors: Vec<String>,
    /// CSS selectors for the "Reject All" / "Decline" button.
    #[serde(default)]
    pub reject_selectors: Vec<String>,
    /// CSS selectors for "Accept Necessary Only" / "Save Preferences" button.
    #[serde(default)]
    pub accept_necessary_selectors: Vec<String>,
    /// CSS selectors for modal backdrops or overlays associated with the banner.
    #[serde(default)]
    pub backdrop_selectors: Vec<String>,
    /// Preferred action for this rule.
    #[serde(default)]
    pub action: ConsentAction,
    /// Higher priority rules are evaluated first when multiple rules match.
    #[serde(default = "default_priority")]
    pub priority: u16,
    /// Whether this rule is active.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_priority() -> u16 {
    100
}

fn default_enabled() -> bool {
    true
}

impl BannerRule {
    /// Validates all fields of the rule.
    pub fn validate(&self) -> Result<(), SelectorError> {
        if self.id.trim().is_empty() {
            return Err(SelectorError::EmptyRuleId);
        }
        if self.name.trim().is_empty() {
            return Err(SelectorError::EmptyRuleName);
        }
        if self.banner_selectors.is_empty() {
            return Err(SelectorError::EmptyBannerSelectors {
                rule_id: self.id.clone(),
            });
        }

        for pattern in &self.domains {
            validate_domain_pattern(pattern)?;
        }

        for selector in self
            .banner_selectors
            .iter()
            .chain(self.reject_selectors.iter())
            .chain(self.accept_necessary_selectors.iter())
            .chain(self.backdrop_selectors.iter())
        {
            validate_css_selector(&self.id, selector)?;
        }

        Ok(())
    }

    /// Returns true if this rule applies to the given normalized lowercase host.
    pub fn matches_host(&self, host: &str) -> bool {
        if self.domains.is_empty() {
            return true;
        }
        let host = host.trim().to_ascii_lowercase();
        self.domains
            .iter()
            .any(|pattern| domain_pattern_matches(pattern, &host))
    }

    /// Returns the ordered list of action button selectors for the given effective action.
    pub fn action_selectors_for(&self, effective_action: ConsentAction) -> Vec<String> {
        match effective_action {
            ConsentAction::RejectAll => {
                if !self.reject_selectors.is_empty() {
                    self.reject_selectors.clone()
                } else {
                    self.accept_necessary_selectors.clone()
                }
            }
            ConsentAction::AcceptNecessary => {
                if !self.accept_necessary_selectors.is_empty() {
                    self.accept_necessary_selectors.clone()
                } else {
                    self.reject_selectors.clone()
                }
            }
            ConsentAction::HideOnly => Vec::new(),
        }
    }
}

/// Result of matching a host or URL against the cookie banner rule registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BannerMatchResult {
    pub rule_id: String,
    pub rule_name: String,
    pub action: ConsentAction,
    pub banner_selectors: Vec<String>,
    pub action_selectors: Vec<String>,
    pub backdrop_selectors: Vec<String>,
    /// CSS rule string that hides the banner and backdrop elements.
    pub generated_css: String,
}

/// Configuration and rule registry for cookie banner selector handling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CookieBannerRegistry {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub default_action: ConsentAction,
    #[serde(default)]
    pub excluded_domains: Vec<String>,
    #[serde(default)]
    pub rules: Vec<BannerRule>,
}

impl Default for CookieBannerRegistry {
    fn default() -> Self {
        Self::with_builtin_rules()
    }
}

impl CookieBannerRegistry {
    /// Creates an empty registry with default settings.
    pub fn empty() -> Self {
        Self {
            enabled: true,
            default_action: ConsentAction::RejectAll,
            excluded_domains: Vec::new(),
            rules: Vec::new(),
        }
    }

    /// Creates a registry pre-populated with common CMP rules.
    pub fn with_builtin_rules() -> Self {
        Self {
            enabled: true,
            default_action: ConsentAction::RejectAll,
            excluded_domains: Vec::new(),
            rules: builtin_cmp_rules(),
        }
    }

    /// Validates the entire registry (excluded domains, rules, and unique rule IDs).
    pub fn validate(&self) -> Result<(), SelectorError> {
        for pattern in &self.excluded_domains {
            validate_domain_pattern(pattern)?;
        }

        let mut seen_ids = HashSet::with_capacity(self.rules.len());
        for rule in &self.rules {
            rule.validate()?;
            if !seen_ids.insert(rule.id.as_str()) {
                return Err(SelectorError::DuplicateRuleId {
                    rule_id: rule.id.clone(),
                });
            }
        }

        Ok(())
    }

    /// Adds a new rule after validating it and checking for duplicate IDs.
    pub fn add_rule(&mut self, rule: BannerRule) -> Result<(), SelectorError> {
        rule.validate()?;
        if self.rules.iter().any(|existing| existing.id == rule.id) {
            return Err(SelectorError::DuplicateRuleId { rule_id: rule.id });
        }
        self.rules.push(rule);
        Ok(())
    }

    /// Removes a rule by ID, returning true if it existed.
    pub fn remove_rule(&mut self, rule_id: &str) -> bool {
        let before = self.rules.len();
        self.rules.retain(|r| r.id != rule_id);
        self.rules.len() != before
    }

    /// Enables or disables a rule by ID, returning true if found.
    pub fn set_rule_enabled(&mut self, rule_id: &str, enabled: bool) -> bool {
        if let Some(rule) = self.rules.iter_mut().find(|r| r.id == rule_id) {
            rule.enabled = enabled;
            true
        } else {
            false
        }
    }

    /// Adds a domain pattern to the exclusion list if not already present.
    pub fn exclude_domain(&mut self, pattern: impl Into<String>) -> Result<(), SelectorError> {
        let pattern = pattern.into().trim().to_ascii_lowercase();
        validate_domain_pattern(&pattern)?;
        if !self.excluded_domains.iter().any(|d| d == &pattern) {
            self.excluded_domains.push(pattern);
        }
        Ok(())
    }

    /// Returns true if the given host is excluded from cookie banner handling.
    pub fn is_host_excluded(&self, host: &str) -> bool {
        let host = host.trim().to_ascii_lowercase();
        self.excluded_domains
            .iter()
            .any(|pattern| domain_pattern_matches(pattern, &host))
    }

    /// Matches a URL string against the registry.
    pub fn match_url(&self, raw_url: &str) -> Result<Option<BannerMatchResult>, SelectorError> {
        let parsed = Url::parse(raw_url).map_err(|e| SelectorError::InvalidUrl {
            url: raw_url.to_string(),
            reason: e.to_string(),
        })?;

        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return Ok(None);
        }

        let host = match parsed.host_str() {
            Some(h) if !h.is_empty() => h,
            _ => {
                return Err(SelectorError::InvalidUrl {
                    url: raw_url.to_string(),
                    reason: "URL has no host".to_string(),
                });
            }
        };

        Ok(self.match_host(host))
    }

    /// Matches a hostname against enabled rules, returning the highest-priority match.
    ///
    /// Domain-specific rules take precedence over universal rules at the same priority.
    pub fn match_host(&self, host: &str) -> Option<BannerMatchResult> {
        if !self.enabled {
            return None;
        }
        let host = host.trim().to_ascii_lowercase();
        if host.is_empty() || self.is_host_excluded(&host) {
            return None;
        }

        let mut candidates: Vec<&BannerRule> = self
            .rules
            .iter()
            .filter(|rule| rule.enabled && rule.matches_host(&host))
            .collect();

        candidates.sort_by(|a, b| {
            let a_specific = !a.domains.is_empty();
            let b_specific = !b.domains.is_empty();
            b.priority
                .cmp(&a.priority)
                .then_with(|| b_specific.cmp(&a_specific))
                .then_with(|| a.id.cmp(&b.id))
        });

        let best = candidates.first()?;
        let effective_action = best.action;
        let action_selectors = best.action_selectors_for(effective_action);
        let generated_css = generate_hide_css(&best.banner_selectors, &best.backdrop_selectors);

        Some(BannerMatchResult {
            rule_id: best.id.clone(),
            rule_name: best.name.clone(),
            action: effective_action,
            banner_selectors: best.banner_selectors.clone(),
            action_selectors,
            backdrop_selectors: best.backdrop_selectors.clone(),
            generated_css,
        })
    }

    /// Returns all matching rules for a host in priority order.
    pub fn match_all_for_host(&self, host: &str) -> Vec<BannerMatchResult> {
        if !self.enabled {
            return Vec::new();
        }
        let host = host.trim().to_ascii_lowercase();
        if host.is_empty() || self.is_host_excluded(&host) {
            return Vec::new();
        }

        let mut candidates: Vec<&BannerRule> = self
            .rules
            .iter()
            .filter(|rule| rule.enabled && rule.matches_host(&host))
            .collect();

        candidates.sort_by(|a, b| {
            let a_specific = !a.domains.is_empty();
            let b_specific = !b.domains.is_empty();
            b.priority
                .cmp(&a.priority)
                .then_with(|| b_specific.cmp(&a_specific))
                .then_with(|| a.id.cmp(&b.id))
        });

        candidates
            .into_iter()
            .map(|rule| BannerMatchResult {
                rule_id: rule.id.clone(),
                rule_name: rule.name.clone(),
                action: rule.action,
                banner_selectors: rule.banner_selectors.clone(),
                action_selectors: rule.action_selectors_for(rule.action),
                backdrop_selectors: rule.backdrop_selectors.clone(),
                generated_css: generate_hide_css(&rule.banner_selectors, &rule.backdrop_selectors),
            })
            .collect()
    }

    /// Serializes the registry to pretty-printed JSON.
    pub fn to_json(&self) -> Result<String, SelectorError> {
        serde_json::to_string_pretty(self).map_err(|e| SelectorError::Serialization(e.to_string()))
    }

    /// Deserializes and validates a registry from JSON.
    pub fn from_json(raw: &str) -> Result<Self, SelectorError> {
        let registry: Self =
            serde_json::from_str(raw).map_err(|e| SelectorError::Serialization(e.to_string()))?;
        registry.validate()?;
        Ok(registry)
    }
}

/// Generates a CSS snippet to hide matched banner and backdrop selectors.
pub fn generate_hide_css(banner_selectors: &[String], backdrop_selectors: &[String]) -> String {
    let all: Vec<&str> = banner_selectors
        .iter()
        .chain(backdrop_selectors.iter())
        .map(String::as_str)
        .filter(|s| !s.trim().is_empty())
        .collect();

    if all.is_empty() {
        return String::new();
    }

    format!(
        "{} {{ display: none !important; visibility: hidden !important; opacity: 0 !important; pointer-events: none !important; }}",
        all.join(", ")
    )
}

/// Validates a domain or wildcard domain pattern (e.g., `"example.com"` or `"*.example.com"`).
pub fn validate_domain_pattern(pattern: &str) -> Result<(), SelectorError> {
    let trimmed = pattern.trim();
    if trimmed.is_empty() {
        return Err(SelectorError::InvalidDomainPattern {
            pattern: pattern.to_string(),
        });
    }

    let host_part = trimmed.strip_prefix("*.").unwrap_or(trimmed);
    if host_part.is_empty()
        || host_part.contains('*')
        || host_part.starts_with('.')
        || host_part.ends_with('.')
    {
        return Err(SelectorError::InvalidDomainPattern {
            pattern: pattern.to_string(),
        });
    }

    let valid_chars = host_part
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
    if !valid_chars || host_part.contains("..") {
        return Err(SelectorError::InvalidDomainPattern {
            pattern: pattern.to_string(),
        });
    }

    Ok(())
}

fn domain_pattern_matches(pattern: &str, host: &str) -> bool {
    let pattern = pattern.trim().to_ascii_lowercase();
    if let Some(suffix) = pattern.strip_prefix("*.") {
        host == suffix || host.ends_with(&format!(".{suffix}"))
    } else {
        host == pattern
    }
}

/// Validates basic CSS selector safety and balanced delimiters.
pub fn validate_css_selector(rule_id: &str, selector: &str) -> Result<(), SelectorError> {
    let trimmed = selector.trim();
    if trimmed.is_empty() {
        return Err(SelectorError::InvalidSelector {
            rule_id: rule_id.to_string(),
            selector: selector.to_string(),
            reason: "selector is empty".to_string(),
        });
    }

    if trimmed.contains('\0') || trimmed.contains('{') || trimmed.contains('}') {
        return Err(SelectorError::InvalidSelector {
            rule_id: rule_id.to_string(),
            selector: selector.to_string(),
            reason: "contains forbidden characters".to_string(),
        });
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("</style") || lower.contains("<script") {
        return Err(SelectorError::InvalidSelector {
            rule_id: rule_id.to_string(),
            selector: selector.to_string(),
            reason: "contains forbidden HTML tag".to_string(),
        });
    }

    let mut paren_depth: i32 = 0;
    let mut bracket_depth: i32 = 0;
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
            '"' | '\'' => in_quote = Some(ch),
            '(' => paren_depth += 1,
            ')' => {
                paren_depth -= 1;
                if paren_depth < 0 {
                    return Err(SelectorError::InvalidSelector {
                        rule_id: rule_id.to_string(),
                        selector: selector.to_string(),
                        reason: "unbalanced ')'".to_string(),
                    });
                }
            }
            '[' => bracket_depth += 1,
            ']' => {
                bracket_depth -= 1;
                if bracket_depth < 0 {
                    return Err(SelectorError::InvalidSelector {
                        rule_id: rule_id.to_string(),
                        selector: selector.to_string(),
                        reason: "unbalanced ']'".to_string(),
                    });
                }
            }
            _ => {}
        }
    }

    if in_quote.is_some() || paren_depth != 0 || bracket_depth != 0 {
        return Err(SelectorError::InvalidSelector {
            rule_id: rule_id.to_string(),
            selector: selector.to_string(),
            reason: "unclosed quote, parenthesis, or bracket".to_string(),
        });
    }

    Ok(())
}

/// Returns built-in rules for widely deployed Consent Management Platforms.
pub fn builtin_cmp_rules() -> Vec<BannerRule> {
    vec![
        BannerRule {
            id: "onetrust".to_string(),
            name: "OneTrust".to_string(),
            domains: Vec::new(),
            banner_selectors: vec![
                "#onetrust-banner-sdk".to_string(),
                "#onetrust-consent-sdk".to_string(),
            ],
            reject_selectors: vec![
                "#onetrust-reject-all-handler".to_string(),
                "button.onetrust-close-btn-handler".to_string(),
            ],
            accept_necessary_selectors: vec!["button.save-preference-btn-handler".to_string()],
            backdrop_selectors: vec![".onetrust-pc-dark-filter".to_string()],
            action: ConsentAction::RejectAll,
            priority: 200,
            enabled: true,
        },
        BannerRule {
            id: "cookiebot".to_string(),
            name: "Cookiebot".to_string(),
            domains: Vec::new(),
            banner_selectors: vec!["#CybotCookiebotDialog".to_string()],
            reject_selectors: vec![
                "#CybotCookiebotDialogBodyButtonDecline".to_string(),
                "#CybotCookiebotDialogBodyLevelButtonDecline".to_string(),
            ],
            accept_necessary_selectors: vec![
                "#CybotCookiebotDialogBodyLevelButtonLevelOptinAllowallSelection".to_string(),
            ],
            backdrop_selectors: vec!["#CybotCookiebotDialogBodyUnderlay".to_string()],
            action: ConsentAction::RejectAll,
            priority: 190,
            enabled: true,
        },
        BannerRule {
            id: "didomi".to_string(),
            name: "Didomi".to_string(),
            domains: Vec::new(),
            banner_selectors: vec!["#didomi-host".to_string(), "#didomi-popup".to_string()],
            reject_selectors: vec![
                "#didomi-notice-disagree-button".to_string(),
                "button[aria-label*='Disagree']".to_string(),
            ],
            accept_necessary_selectors: vec![".didomi-continue-without-agreeing".to_string()],
            backdrop_selectors: vec![".didomi-popup-backdrop".to_string()],
            action: ConsentAction::RejectAll,
            priority: 180,
            enabled: true,
        },
        BannerRule {
            id: "quantcast".to_string(),
            name: "Quantcast Choice".to_string(),
            domains: Vec::new(),
            banner_selectors: vec!["#qc-cmp2-container".to_string()],
            reject_selectors: vec!["#qc-cmp2-ui button[mode='secondary']".to_string()],
            accept_necessary_selectors: Vec::new(),
            backdrop_selectors: Vec::new(),
            action: ConsentAction::RejectAll,
            priority: 170,
            enabled: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_validates_and_matches_url() {
        let registry = CookieBannerRegistry::with_builtin_rules();
        assert!(registry.validate().is_ok());

        let matched = registry
            .match_url("https://news.example.com/article?id=42")
            .unwrap();
        let result = matched.expect("expected a builtin universal rule match");
        assert_eq!(result.rule_id, "onetrust");
        assert_eq!(result.action, ConsentAction::RejectAll);
        assert!(result
            .action_selectors
            .contains(&"#onetrust-reject-all-handler".to_string()));
        assert!(result.generated_css.contains("#onetrust-banner-sdk"));
        assert!(result.generated_css.contains("display: none !important"));

        // Non-HTTP schemes are ignored cleanly.
        let file_match = registry.match_url("file:///tmp/index.html").unwrap();
        assert!(file_match.is_none());
    }

    #[test]
    fn domain_specific_and_excluded_domains_precedence() {
        let mut registry = CookieBannerRegistry::with_builtin_rules();
        let custom = BannerRule {
            id: "example-custom".to_string(),
            name: "Example Site Banner".to_string(),
            domains: vec!["*.example.org".to_string()],
            banner_selectors: vec![".cookie-modal[data-visible='true']".to_string()],
            reject_selectors: vec![".cookie-modal button.reject".to_string()],
            accept_necessary_selectors: Vec::new(),
            backdrop_selectors: vec![".cookie-backdrop".to_string()],
            action: ConsentAction::RejectAll,
            priority: 200, // Equal to onetrust, but domain-specific wins.
            enabled: true,
        };
        registry.add_rule(custom).unwrap();

        let sub_match = registry.match_host("docs.example.org").unwrap();
        assert_eq!(sub_match.rule_id, "example-custom");

        let apex_match = registry.match_host("example.org").unwrap();
        assert_eq!(apex_match.rule_id, "example-custom");

        // Excluding the domain suppresses all matches for it.
        registry.exclude_domain("*.example.org").unwrap();
        assert!(registry.match_host("docs.example.org").is_none());
        assert!(registry.match_host("other.com").is_some());
    }

    #[test]
    fn validation_catches_bad_selectors_domains_and_duplicates() {
        let bad_selector = BannerRule {
            id: "bad-sel".to_string(),
            name: "Bad Selector".to_string(),
            domains: Vec::new(),
            banner_selectors: vec!["div[data-id='unclosed".to_string()],
            reject_selectors: Vec::new(),
            accept_necessary_selectors: Vec::new(),
            backdrop_selectors: Vec::new(),
            action: ConsentAction::HideOnly,
            priority: 50,
            enabled: true,
        };
        assert!(matches!(
            bad_selector.validate(),
            Err(SelectorError::InvalidSelector { .. })
        ));

        let bad_brace = BannerRule {
            id: "bad-brace".to_string(),
            name: "Bad Brace".to_string(),
            domains: Vec::new(),
            banner_selectors: vec!["#banner { display: none }".to_string()],
            reject_selectors: Vec::new(),
            accept_necessary_selectors: Vec::new(),
            backdrop_selectors: Vec::new(),
            action: ConsentAction::HideOnly,
            priority: 50,
            enabled: true,
        };
        assert!(matches!(
            bad_brace.validate(),
            Err(SelectorError::InvalidSelector { .. })
        ));

        assert!(matches!(
            validate_domain_pattern("foo.*.com"),
            Err(SelectorError::InvalidDomainPattern { .. })
        ));

        let mut registry = CookieBannerRegistry::with_builtin_rules();
        let dup = builtin_cmp_rules().into_iter().next().unwrap();
        assert!(matches!(
            registry.add_rule(dup),
            Err(SelectorError::DuplicateRuleId { .. })
        ));
    }

    #[test]
    fn json_roundtrip_and_rule_toggling() {
        let mut registry = CookieBannerRegistry::with_builtin_rules();
        assert!(registry.set_rule_enabled("onetrust", false));
        assert!(!registry.set_rule_enabled("nonexistent", false));

        let json = registry.to_json().unwrap();
        let restored = CookieBannerRegistry::from_json(&json).unwrap();
        assert_eq!(registry, restored);

        // With onetrust disabled, cookiebot (priority 190) becomes top match.
        let top = restored.match_host("example.net").unwrap();
        assert_eq!(top.rule_id, "cookiebot");
    }
}
