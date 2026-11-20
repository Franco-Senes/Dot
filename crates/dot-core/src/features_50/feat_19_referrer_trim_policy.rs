use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

const MAX_CONFIGURED_LENGTH: usize = 8192;

/// Errors produced when validating or evaluating referrer trim policies.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ReferrerPolicyError {
    #[error("invalid URL '{input}': {reason}")]
    InvalidUrl { input: String, reason: String },

    #[error("invalid host pattern '{0}'")]
    InvalidHostPattern(String),

    #[error("invalid Referrer-Policy header value '{0}'")]
    InvalidPolicyHeader(String),

    #[error("invalid max referrer length {0}; expected 1..=8192")]
    InvalidMaxLength(usize),

    #[error("duplicate site referrer rule for pattern '{pattern}'")]
    RuleConflict { pattern: String },
}

/// Standard W3C Referrer-Policy directives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ReferrerPolicy {
    NoReferrer,
    NoReferrerWhenDowngrade,
    Origin,
    OriginWhenCrossOrigin,
    SameOrigin,
    StrictOrigin,
    #[default]
    StrictOriginWhenCrossOrigin,
    UnsafeUrl,
}

impl ReferrerPolicy {
    /// Returns the canonical lowercase header token for this policy.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoReferrer => "no-referrer",
            Self::NoReferrerWhenDowngrade => "no-referrer-when-downgrade",
            Self::Origin => "origin",
            Self::OriginWhenCrossOrigin => "origin-when-cross-origin",
            Self::SameOrigin => "same-origin",
            Self::StrictOrigin => "strict-origin",
            Self::StrictOriginWhenCrossOrigin => "strict-origin-when-cross-origin",
            Self::UnsafeUrl => "unsafe-url",
        }
    }

    /// Parses a single policy token (case-insensitive, trimmed).
    pub fn from_token(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "no-referrer" => Some(Self::NoReferrer),
            "no-referrer-when-downgrade" => Some(Self::NoReferrerWhenDowngrade),
            "origin" => Some(Self::Origin),
            "origin-when-cross-origin" => Some(Self::OriginWhenCrossOrigin),
            "same-origin" => Some(Self::SameOrigin),
            "strict-origin" => Some(Self::StrictOrigin),
            "strict-origin-when-cross-origin" => Some(Self::StrictOriginWhenCrossOrigin),
            "unsafe-url" => Some(Self::UnsafeUrl),
            _ => None,
        }
    }

    /// Parses a comma-separated `Referrer-Policy` header value per W3C spec:
    /// unknown tokens are ignored and the last recognized token wins.
    pub fn from_header_value(header: &str) -> Result<Self, ReferrerPolicyError> {
        let mut resolved: Option<Self> = None;
        for token in header.split(',') {
            if let Some(policy) = Self::from_token(token) {
                resolved = Some(policy);
            }
        }
        resolved.ok_or_else(|| ReferrerPolicyError::InvalidPolicyHeader(header.to_string()))
    }
}

impl fmt::Display for ReferrerPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Granularity of a transmitted referrer URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrimLevel {
    FullWithoutFragment,
    StrippedQuery,
    OriginOnly,
}

/// Reason a referrer header was suppressed completely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuppressionReason {
    PolicyNoReferrer,
    CrossOriginSameOriginOnly,
    DowngradeBlocked,
    NonHttpScheme,
    OpaqueOrigin,
    ExceedsMaxLength,
}

/// Outcome of evaluating a referrer policy for a single navigation or subresource request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ReferrerDecision {
    Suppressed { reason: SuppressionReason },
    Trimmed { referrer: String, level: TrimLevel },
}

/// Per-host or wildcard-subdomain referrer trim override.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteReferrerRule {
    pub pattern: String,
    pub policy: ReferrerPolicy,
    #[serde(default)]
    pub strip_query_on_cross_origin: bool,
    #[serde(default)]
    pub max_length: Option<usize>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

const fn default_true() -> bool {
    true
}

impl SiteReferrerRule {
    /// Validates the host pattern and optional length limit.
    pub fn validate(&self) -> Result<(), ReferrerPolicyError> {
        validate_host_pattern(&self.pattern)?;
        if let Some(len) = self.max_length {
            if len == 0 || len > MAX_CONFIGURED_LENGTH {
                return Err(ReferrerPolicyError::InvalidMaxLength(len));
            }
        }
        Ok(())
    }

    /// Returns true if this rule's pattern matches the given normalized host.
    pub fn matches_host(&self, host: &str) -> bool {
        if !self.enabled {
            return false;
        }
        let pattern = self.pattern.to_ascii_lowercase();
        let host = host.to_ascii_lowercase();
        if let Some(suffix) = pattern.strip_prefix("*.") {
            if host == suffix {
                return false;
            }
            host.ends_with(suffix)
                && host
                    .as_bytes()
                    .get(host.len().saturating_sub(suffix.len() + 1))
                    == Some(&b'.')
        } else {
            host == pattern
        }
    }
}

/// Global configuration for referrer trimming across Search Linux.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferrerTrimConfig {
    #[serde(default)]
    pub default_policy: ReferrerPolicy,
    #[serde(default = "default_true")]
    pub cap_cross_origin_to_origin: bool,
    #[serde(default)]
    pub strip_query_parameters: Vec<String>,
    #[serde(default)]
    pub global_max_length: Option<usize>,
    #[serde(default = "default_true")]
    pub allow_document_override: bool,
    #[serde(default)]
    pub site_rules: Vec<SiteReferrerRule>,
}

impl Default for ReferrerTrimConfig {
    fn default() -> Self {
        Self {
            default_policy: ReferrerPolicy::StrictOriginWhenCrossOrigin,
            cap_cross_origin_to_origin: true,
            strip_query_parameters: vec![
                "utm_source".to_string(),
                "utm_medium".to_string(),
                "utm_campaign".to_string(),
                "fbclid".to_string(),
                "gclid".to_string(),
            ],
            global_max_length: Some(2048),
            allow_document_override: true,
            site_rules: Vec::new(),
        }
    }
}

impl ReferrerTrimConfig {
    /// Validates all fields and rules in the configuration.
    pub fn validate(&self) -> Result<(), ReferrerPolicyError> {
        if let Some(len) = self.global_max_length {
            if len == 0 || len > MAX_CONFIGURED_LENGTH {
                return Err(ReferrerPolicyError::InvalidMaxLength(len));
            }
        }
        let mut seen = Vec::with_capacity(self.site_rules.len());
        for rule in &self.site_rules {
            rule.validate()?;
            let normalized = rule.pattern.to_ascii_lowercase();
            if seen.contains(&normalized) {
                return Err(ReferrerPolicyError::RuleConflict {
                    pattern: rule.pattern.clone(),
                });
            }
            seen.push(normalized);
        }
        Ok(())
    }

    /// Adds a validated site rule, rejecting duplicate patterns (case-insensitive).
    pub fn add_rule(&mut self, rule: SiteReferrerRule) -> Result<(), ReferrerPolicyError> {
        rule.validate()?;
        let normalized = rule.pattern.to_ascii_lowercase();
        if self
            .site_rules
            .iter()
            .any(|existing| existing.pattern.to_ascii_lowercase() == normalized)
        {
            return Err(ReferrerPolicyError::RuleConflict {
                pattern: rule.pattern,
            });
        }
        self.site_rules.push(rule);
        Ok(())
    }

    /// Removes a rule matching the given pattern (case-insensitive).
    pub fn remove_rule(&mut self, pattern: &str) -> bool {
        let normalized = pattern.to_ascii_lowercase();
        let before = self.site_rules.len();
        self.site_rules
            .retain(|r| r.pattern.to_ascii_lowercase() != normalized);
        self.site_rules.len() != before
    }

    /// Serializes the configuration to pretty JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserializes and validates a configuration from JSON.
    pub fn from_json(json: &str) -> Result<Self, ReferrerPolicyError> {
        let cfg: Self =
            serde_json::from_str(json).map_err(|err| ReferrerPolicyError::InvalidUrl {
                input: "<config>".to_string(),
                reason: err.to_string(),
            })?;
        cfg.validate()?;
        Ok(cfg)
    }
}

/// Input parameters for evaluating a referrer header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferrerRequest {
    pub source_url: String,
    pub target_url: String,
    #[serde(default)]
    pub document_policy: Option<ReferrerPolicy>,
    #[serde(default)]
    pub request_override: Option<ReferrerPolicy>,
}

/// Complete evaluation result including header value and metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferrerEvaluation {
    pub header_value: Option<String>,
    pub applied_policy: ReferrerPolicy,
    pub same_origin: bool,
    pub is_downgrade: bool,
    pub decision: ReferrerDecision,
}

/// Evaluates HTTP `Referer` headers according to `ReferrerTrimConfig`.
#[derive(Debug, Clone)]
pub struct ReferrerTrimmer {
    config: ReferrerTrimConfig,
}

impl ReferrerTrimmer {
    /// Creates a new trimmer after validating the configuration.
    pub fn new(config: ReferrerTrimConfig) -> Result<Self, ReferrerPolicyError> {
        config.validate()?;
        Ok(Self { config })
    }

    /// Returns the active configuration.
    pub fn config(&self) -> &ReferrerTrimConfig {
        &self.config
    }

    /// Evaluates what `Referer` header (if any) should be sent for `request`.
    pub fn evaluate(
        &self,
        request: &ReferrerRequest,
    ) -> Result<ReferrerEvaluation, ReferrerPolicyError> {
        let mut source =
            Url::parse(&request.source_url).map_err(|err| ReferrerPolicyError::InvalidUrl {
                input: request.source_url.clone(),
                reason: err.to_string(),
            })?;
        let target =
            Url::parse(&request.target_url).map_err(|err| ReferrerPolicyError::InvalidUrl {
                input: request.target_url.clone(),
                reason: err.to_string(),
            })?;

        let same_origin = is_same_origin(&source, &target);
        let is_downgrade = source.scheme() == "https" && target.scheme() == "http";

        let matched_rule = source.host_str().and_then(|host| self.find_best_rule(host));

        let mut effective_policy = self.resolve_policy(request, matched_rule);

        if !same_origin && self.config.cap_cross_origin_to_origin {
            effective_policy = match effective_policy {
                ReferrerPolicy::UnsafeUrl | ReferrerPolicy::NoReferrerWhenDowngrade => {
                    ReferrerPolicy::StrictOriginWhenCrossOrigin
                }
                other => other,
            };
        }

        if !is_http_or_https(source.scheme()) {
            return Ok(suppressed(
                effective_policy,
                same_origin,
                is_downgrade,
                SuppressionReason::NonHttpScheme,
            ));
        }

        if source.host_str().is_none() {
            return Ok(suppressed(
                effective_policy,
                same_origin,
                is_downgrade,
                SuppressionReason::OpaqueOrigin,
            ));
        }

        // Credentials and fragments must never be leaked in Referer headers.
        let _ = source.set_username("");
        let _ = source.set_password(None);
        source.set_fragment(None);

        let origin_str = match format_origin(&source) {
            Some(o) => o,
            None => {
                return Ok(suppressed(
                    effective_policy,
                    same_origin,
                    is_downgrade,
                    SuppressionReason::OpaqueOrigin,
                ))
            }
        };

        let decision = match effective_policy {
            ReferrerPolicy::NoReferrer => ReferrerDecision::Suppressed {
                reason: SuppressionReason::PolicyNoReferrer,
            },
            ReferrerPolicy::NoReferrerWhenDowngrade => {
                if is_downgrade {
                    ReferrerDecision::Suppressed {
                        reason: SuppressionReason::DowngradeBlocked,
                    }
                } else {
                    self.build_full_or_trimmed(&source, &origin_str, same_origin, matched_rule)
                }
            }
            ReferrerPolicy::SameOrigin => {
                if same_origin {
                    self.build_full_or_trimmed(&source, &origin_str, same_origin, matched_rule)
                } else {
                    ReferrerDecision::Suppressed {
                        reason: SuppressionReason::CrossOriginSameOriginOnly,
                    }
                }
            }
            ReferrerPolicy::Origin => {
                self.apply_length_limit(origin_str, TrimLevel::OriginOnly, None, matched_rule)
            }
            ReferrerPolicy::StrictOrigin => {
                if is_downgrade {
                    ReferrerDecision::Suppressed {
                        reason: SuppressionReason::DowngradeBlocked,
                    }
                } else {
                    self.apply_length_limit(origin_str, TrimLevel::OriginOnly, None, matched_rule)
                }
            }
            ReferrerPolicy::OriginWhenCrossOrigin => {
                if same_origin {
                    self.build_full_or_trimmed(&source, &origin_str, same_origin, matched_rule)
                } else {
                    self.apply_length_limit(origin_str, TrimLevel::OriginOnly, None, matched_rule)
                }
            }
            ReferrerPolicy::StrictOriginWhenCrossOrigin => {
                if same_origin {
                    self.build_full_or_trimmed(&source, &origin_str, same_origin, matched_rule)
                } else if is_downgrade {
                    ReferrerDecision::Suppressed {
                        reason: SuppressionReason::DowngradeBlocked,
                    }
                } else {
                    self.apply_length_limit(origin_str, TrimLevel::OriginOnly, None, matched_rule)
                }
            }
            ReferrerPolicy::UnsafeUrl => {
                self.build_full_or_trimmed(&source, &origin_str, same_origin, matched_rule)
            }
        };

        let header_value = match &decision {
            ReferrerDecision::Suppressed { .. } => None,
            ReferrerDecision::Trimmed { referrer, .. } => Some(referrer.clone()),
        };

        Ok(ReferrerEvaluation {
            header_value,
            applied_policy: effective_policy,
            same_origin,
            is_downgrade,
            decision,
        })
    }

    fn resolve_policy(
        &self,
        request: &ReferrerRequest,
        matched_rule: Option<&SiteReferrerRule>,
    ) -> ReferrerPolicy {
        if let Some(override_policy) = request.request_override {
            return override_policy;
        }
        if let Some(rule) = matched_rule {
            return rule.policy;
        }
        if self.config.allow_document_override {
            if let Some(doc_policy) = request.document_policy {
                return doc_policy;
            }
        }
        self.config.default_policy
    }

    fn find_best_rule(&self, host: &str) -> Option<&SiteReferrerRule> {
        self.config
            .site_rules
            .iter()
            .filter(|rule| rule.matches_host(host))
            .max_by_key(|rule| {
                let is_exact = !rule.pattern.starts_with("*.");
                (is_exact, rule.pattern.len())
            })
    }

    fn build_full_or_trimmed(
        &self,
        source: &Url,
        origin_str: &str,
        same_origin: bool,
        matched_rule: Option<&SiteReferrerRule>,
    ) -> ReferrerDecision {
        let mut cleaned = source.clone();
        let strip_all_query =
            !same_origin && matched_rule.is_some_and(|rule| rule.strip_query_on_cross_origin);

        let mut level = TrimLevel::FullWithoutFragment;
        if strip_all_query {
            if cleaned.query().is_some() {
                cleaned.set_query(None);
                level = TrimLevel::StrippedQuery;
            }
        } else if !self.config.strip_query_parameters.is_empty() && cleaned.query().is_some() {
            let kept: Vec<(String, String)> = cleaned
                .query_pairs()
                .filter(|(k, _)| {
                    !self
                        .config
                        .strip_query_parameters
                        .iter()
                        .any(|param| param.eq_ignore_ascii_case(k))
                })
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();

            if kept.is_empty() {
                cleaned.set_query(None);
                level = TrimLevel::StrippedQuery;
            } else {
                let mut serializer = cleaned.query_pairs_mut();
                serializer.clear();
                for (k, v) in &kept {
                    serializer.append_pair(k, v);
                }
                drop(serializer);
            }
        }

        self.apply_length_limit(cleaned.to_string(), level, Some(origin_str), matched_rule)
    }

    fn apply_length_limit(
        &self,
        candidate: String,
        level: TrimLevel,
        origin_fallback: Option<&str>,
        matched_rule: Option<&SiteReferrerRule>,
    ) -> ReferrerDecision {
        let max_len = matched_rule
            .and_then(|r| r.max_length)
            .or(self.config.global_max_length);

        let Some(limit) = max_len else {
            return ReferrerDecision::Trimmed {
                referrer: candidate,
                level,
            };
        };

        if candidate.len() <= limit {
            return ReferrerDecision::Trimmed {
                referrer: candidate,
                level,
            };
        }

        if let Some(origin) = origin_fallback {
            if origin.len() <= limit {
                return ReferrerDecision::Trimmed {
                    referrer: origin.to_string(),
                    level: TrimLevel::OriginOnly,
                };
            }
        }

        ReferrerDecision::Suppressed {
            reason: SuppressionReason::ExceedsMaxLength,
        }
    }
}

fn suppressed(
    applied_policy: ReferrerPolicy,
    same_origin: bool,
    is_downgrade: bool,
    reason: SuppressionReason,
) -> ReferrerEvaluation {
    ReferrerEvaluation {
        header_value: None,
        applied_policy,
        same_origin,
        is_downgrade,
        decision: ReferrerDecision::Suppressed { reason },
    }
}

fn is_http_or_https(scheme: &str) -> bool {
    matches!(scheme, "http" | "https")
}

fn is_same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str().is_some()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

fn format_origin(url: &Url) -> Option<String> {
    let host = url.host_str()?;
    let scheme = url.scheme();
    match url.port() {
        Some(port) => Some(format!("{scheme}://{host}:{port}/")),
        None => Some(format!("{scheme}://{host}/")),
    }
}

fn validate_host_pattern(pattern: &str) -> Result<(), ReferrerPolicyError> {
    let trimmed = pattern.trim();
    if trimmed.is_empty() || trimmed != pattern {
        return Err(ReferrerPolicyError::InvalidHostPattern(pattern.to_string()));
    }
    let host_part = trimmed.strip_prefix("*.").unwrap_or(trimmed);
    if host_part.is_empty()
        || host_part.contains('*')
        || host_part.starts_with('.')
        || host_part.ends_with('.')
        || host_part.contains("..")
    {
        return Err(ReferrerPolicyError::InvalidHostPattern(pattern.to_string()));
    }
    let valid_chars = host_part
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.');
    if !valid_chars {
        return Err(ReferrerPolicyError::InvalidHostPattern(pattern.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trimmer_default() -> ReferrerTrimmer {
        ReferrerTrimmer::new(ReferrerTrimConfig::default()).expect("default config is valid")
    }

    #[test]
    fn strict_origin_when_cross_origin_trims_cross_site_and_blocks_downgrade() {
        let trimmer = trimmer_default();

        // Same-origin preserves path and non-tracking query, strips fragment and tracking params.
        let same_origin_req = ReferrerRequest {
            source_url: "https://user:pass@example.com/search?q=rust&utm_source=nav#top"
                .to_string(),
            target_url: "https://example.com/result".to_string(),
            document_policy: None,
            request_override: None,
        };
        let eval_same = trimmer.evaluate(&same_origin_req).expect("valid request");
        assert!(eval_same.same_origin);
        assert!(!eval_same.is_downgrade);
        assert_eq!(
            eval_same.header_value.as_deref(),
            Some("https://example.com/search?q=rust")
        );

        // Cross-origin HTTPS -> HTTPS trims to origin only.
        let cross_origin_req = ReferrerRequest {
            source_url: "https://example.com/private/page?token=abc".to_string(),
            target_url: "https://partner.org/landing".to_string(),
            document_policy: None,
            request_override: None,
        };
        let eval_cross = trimmer.evaluate(&cross_origin_req).expect("valid request");
        assert!(!eval_cross.same_origin);
        assert_eq!(
            eval_cross.header_value.as_deref(),
            Some("https://example.com/")
        );
        assert_eq!(
            eval_cross.decision,
            ReferrerDecision::Trimmed {
                referrer: "https://example.com/".to_string(),
                level: TrimLevel::OriginOnly,
            }
        );

        // Downgrade HTTPS -> HTTP is suppressed.
        let downgrade_req = ReferrerRequest {
            source_url: "https://example.com/page".to_string(),
            target_url: "http://legacy.example.net/".to_string(),
            document_policy: None,
            request_override: None,
        };
        let eval_down = trimmer.evaluate(&downgrade_req).expect("valid request");
        assert!(eval_down.is_downgrade);
        assert_eq!(eval_down.header_value, None);
        assert_eq!(
            eval_down.decision,
            ReferrerDecision::Suppressed {
                reason: SuppressionReason::DowngradeBlocked,
            }
        );
    }

    #[test]
    fn header_parsing_follows_w3c_last_valid_token_and_non_http_is_suppressed() {
        let parsed = ReferrerPolicy::from_header_value("origin, unknown-token, same-origin")
            .expect("should parse last valid token");
        assert_eq!(parsed, ReferrerPolicy::SameOrigin);
        assert!(ReferrerPolicy::from_header_value("bogus-only").is_err());

        let trimmer = trimmer_default();
        let about_req = ReferrerRequest {
            source_url: "about:blank".to_string(),
            target_url: "https://example.com/".to_string(),
            document_policy: Some(ReferrerPolicy::UnsafeUrl),
            request_override: None,
        };
        let eval = trimmer.evaluate(&about_req).expect("valid URLs");
        assert_eq!(eval.header_value, None);
        assert_eq!(
            eval.decision,
            ReferrerDecision::Suppressed {
                reason: SuppressionReason::NonHttpScheme,
            }
        );
    }

    #[test]
    fn site_rules_priority_length_fallback_and_config_roundtrip() {
        let mut config = ReferrerTrimConfig::default();
        config
            .add_rule(SiteReferrerRule {
                pattern: "*.example.com".to_string(),
                policy: ReferrerPolicy::NoReferrer,
                strip_query_on_cross_origin: true,
                max_length: None,
                enabled: true,
            })
            .expect("valid wildcard rule");
        config
            .add_rule(SiteReferrerRule {
                pattern: "docs.example.com".to_string(),
                policy: ReferrerPolicy::OriginWhenCrossOrigin,
                strip_query_on_cross_origin: false,
                max_length: Some(32),
                enabled: true,
            })
            .expect("valid exact rule");

        // Duplicate pattern rejected.
        assert!(matches!(
            config.add_rule(SiteReferrerRule {
                pattern: "DOCS.EXAMPLE.COM".to_string(),
                policy: ReferrerPolicy::NoReferrer,
                strip_query_on_cross_origin: false,
                max_length: None,
                enabled: true,
            }),
            Err(ReferrerPolicyError::RuleConflict { .. })
        ));

        let json = config.to_json().expect("serialize");
        let restored = ReferrerTrimConfig::from_json(&json).expect("deserialize");
        let trimmer = ReferrerTrimmer::new(restored).expect("valid trimmer");

        // Exact rule beats wildcard rule, and long same-origin URL falls back to origin when > 32 bytes.
        let req = ReferrerRequest {
            source_url: "https://docs.example.com/very/long/documentation/path/index.html"
                .to_string(),
            target_url: "https://docs.example.com/next".to_string(),
            document_policy: None,
            request_override: None,
        };
        let eval = trimmer.evaluate(&req).expect("valid request");
        assert_eq!(
            eval.header_value.as_deref(),
            Some("https://docs.example.com/")
        );
        assert_eq!(
            eval.decision,
            ReferrerDecision::Trimmed {
                referrer: "https://docs.example.com/".to_string(),
                level: TrimLevel::OriginOnly,
            }
        );

        // Wildcard rule applies to other subdomains.
        let sub_req = ReferrerRequest {
            source_url: "https://mail.example.com/inbox".to_string(),
            target_url: "https://mail.example.com/compose".to_string(),
            document_policy: None,
            request_override: None,
        };
        let sub_eval = trimmer.evaluate(&sub_req).expect("valid request");
        assert_eq!(sub_eval.header_value, None);
        assert_eq!(
            sub_eval.decision,
            ReferrerDecision::Suppressed {
                reason: SuppressionReason::PolicyNoReferrer,
            }
        );
    }
}
