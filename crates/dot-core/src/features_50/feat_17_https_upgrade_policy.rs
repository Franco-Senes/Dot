//! HTTPS Upgrade Policy (`feat_17_https_upgrade_policy`).
//!
//! Provides domain models, HSTS header parsing and lookup, host exemption
//! management, opportunistic fallback caching, and deterministic navigation
//! upgrade evaluation for Search Linux.

use std::collections::BTreeMap;
use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

/// Default TTL (in seconds) for caching a failed HTTPS upgrade in opportunistic mode.
pub const DEFAULT_FALLBACK_TTL_SECS: u64 = 3600;

/// Errors produced by HTTPS upgrade policy validation and evaluation.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HttpsUpgradeError {
    #[error("invalid URL '{0}': {1}")]
    InvalidUrl(String, String),

    #[error("invalid host pattern '{0}': {1}")]
    InvalidHostPattern(String, String),

    #[error("invalid Strict-Transport-Security header: {0}")]
    InvalidHstsHeader(String),

    #[error("invalid policy configuration: {0}")]
    InvalidConfig(String),

    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Global HTTPS upgrade enforcement mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum HttpsUpgradeMode {
    /// Only upgrade when an active HSTS record requires it.
    Disabled,
    /// Attempt HTTPS upgrade on HTTP navigations and fall back to HTTP on failure.
    #[default]
    Opportunistic,
    /// Require HTTPS for all non-exempt navigations; block or prompt on failure.
    Strict,
    /// Strict enforcement without persisting new HSTS or fallback entries.
    IncognitoStrict,
}

/// Lifetime scope of a host exemption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExemptionScope {
    /// Valid only for the current browser session.
    Session,
    /// Persisted across sessions.
    Permanent,
    /// Expires at the specified UNIX timestamp (seconds).
    Timed { expires_at_unix: i64 },
}

/// Reason a host is exempt from HTTPS upgrades.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExemptionReason {
    UserBypass,
    EnterprisePolicy,
    LocalNetwork,
    CaptivePortal,
    IncompatibleTls,
}

/// Validated host matching pattern (exact hostname or `*.domain` wildcard).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct HostPattern(String);

impl HostPattern {
    /// Parses and normalizes a host pattern.
    ///
    /// Accepts exact hostnames (e.g., `example.com`) or single-label-prefix
    /// wildcards (e.g., `*.example.com`). Rejects schemes, paths, ports,
    /// empty labels, and bare `*`.
    pub fn parse(raw: &str) -> Result<Self, HttpsUpgradeError> {
        let trimmed = raw.trim().to_ascii_lowercase();
        if trimmed.is_empty() {
            return Err(HttpsUpgradeError::InvalidHostPattern(
                raw.to_string(),
                "pattern must not be empty".to_string(),
            ));
        }
        if trimmed.contains("://")
            || trimmed.contains('/')
            || trimmed.contains('?')
            || trimmed.contains('#')
            || trimmed.contains(':')
        {
            return Err(HttpsUpgradeError::InvalidHostPattern(
                raw.to_string(),
                "pattern must be a bare host or *.domain without scheme, port, or path".to_string(),
            ));
        }

        let check_domain = if let Some(rest) = trimmed.strip_prefix("*.") {
            if rest.is_empty() || rest.contains('*') {
                return Err(HttpsUpgradeError::InvalidHostPattern(
                    raw.to_string(),
                    "wildcard must be a leading '*.' followed by a valid domain".to_string(),
                ));
            }
            rest
        } else {
            if trimmed.contains('*') {
                return Err(HttpsUpgradeError::InvalidHostPattern(
                    raw.to_string(),
                    "wildcards are only supported as a leading '*.'".to_string(),
                ));
            }
            trimmed.as_str()
        };

        if check_domain.starts_with('.')
            || check_domain.ends_with('.')
            || check_domain.contains("..")
        {
            return Err(HttpsUpgradeError::InvalidHostPattern(
                raw.to_string(),
                "domain contains empty labels".to_string(),
            ));
        }

        for label in check_domain.split('.') {
            if label.is_empty() || label.len() > 63 {
                return Err(HttpsUpgradeError::InvalidHostPattern(
                    raw.to_string(),
                    "invalid domain label length".to_string(),
                ));
            }
            let valid_chars = label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
            if !valid_chars || label.starts_with('-') || label.ends_with('-') {
                return Err(HttpsUpgradeError::InvalidHostPattern(
                    raw.to_string(),
                    format!("invalid characters in label '{label}'"),
                ));
            }
        }

        Ok(Self(trimmed))
    }

    /// Returns the normalized pattern string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns `true` if `host` (case-insensitive) matches this pattern.
    pub fn matches(&self, host: &str) -> bool {
        let host_norm = host.trim().trim_end_matches('.').to_ascii_lowercase();
        if host_norm.is_empty() {
            return false;
        }
        if let Some(suffix) = self.0.strip_prefix("*.") {
            host_norm == suffix || host_norm.ends_with(&format!(".{suffix}"))
        } else {
            host_norm == self.0
        }
    }
}

/// An explicit exemption entry for a host pattern.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExemptionEntry {
    pub pattern: HostPattern,
    pub scope: ExemptionScope,
    pub reason: ExemptionReason,
    pub created_at_unix: i64,
}

impl ExemptionEntry {
    /// Returns `true` if the exemption is still active at `now_unix`.
    pub fn is_active(&self, now_unix: i64) -> bool {
        match self.scope {
            ExemptionScope::Session | ExemptionScope::Permanent => true,
            ExemptionScope::Timed { expires_at_unix } => now_unix < expires_at_unix,
        }
    }
}

/// Parsed HTTP Strict-Transport-Security (HSTS) directive set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HstsDirective {
    pub max_age_secs: u64,
    pub include_subdomains: bool,
    pub preload: bool,
}

/// Parses an RFC 6797 `Strict-Transport-Security` header value.
pub fn parse_hsts_header(header_value: &str) -> Result<HstsDirective, HttpsUpgradeError> {
    let trimmed = header_value.trim();
    if trimmed.is_empty() {
        return Err(HttpsUpgradeError::InvalidHstsHeader(
            "header value is empty".to_string(),
        ));
    }

    let mut max_age_secs: Option<u64> = None;
    let mut include_subdomains = false;
    let mut preload = false;

    for raw_part in trimmed.split(';') {
        let part = raw_part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((name, val)) = part.split_once('=') {
            let name_lower = name.trim().to_ascii_lowercase();
            let val_clean = val.trim().trim_matches('"');
            if name_lower == "max-age" {
                if max_age_secs.is_some() {
                    return Err(HttpsUpgradeError::InvalidHstsHeader(
                        "duplicate max-age directive".to_string(),
                    ));
                }
                let parsed = val_clean.parse::<u64>().map_err(|_| {
                    HttpsUpgradeError::InvalidHstsHeader(format!(
                        "invalid max-age value '{val_clean}'"
                    ))
                })?;
                max_age_secs = Some(parsed);
            }
        } else {
            match part.to_ascii_lowercase().as_str() {
                "includesubdomains" => include_subdomains = true,
                "preload" => preload = true,
                _ => {}
            }
        }
    }

    let max_age = max_age_secs.ok_or_else(|| {
        HttpsUpgradeError::InvalidHstsHeader("missing required max-age directive".to_string())
    })?;

    Ok(HstsDirective {
        max_age_secs: max_age,
        include_subdomains,
        preload,
    })
}

/// Stored HSTS state for a single host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HstsEntry {
    pub host: String,
    pub expires_at_unix: i64,
    pub include_subdomains: bool,
    pub preloaded: bool,
}

impl HstsEntry {
    /// Returns `true` if this entry is active at `now_unix`.
    pub fn is_active(&self, now_unix: i64) -> bool {
        self.preloaded || now_unix < self.expires_at_unix
    }
}

/// Failure classification for an attempted HTTPS upgrade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureReason {
    ConnectionRefused,
    TlsHandshakeFailed,
    CertificateInvalid,
    Timeout,
    HttpRedirectLoop,
}

/// Cached fallback record for a host where HTTPS upgrade recently failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FallbackRecord {
    pub host: String,
    pub failed_at_unix: i64,
    pub expires_at_unix: i64,
    pub reason: FailureReason,
}

impl FallbackRecord {
    pub fn is_active(&self, now_unix: i64) -> bool {
        now_unix < self.expires_at_unix
    }
}

/// Navigation context passed into policy evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpgradeContext {
    pub url: String,
    pub is_incognito: bool,
    pub is_main_frame: bool,
    pub now_unix: i64,
    pub captive_portal_active: bool,
}

/// Why an upgrade to HTTPS is performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpgradeReason {
    HstsPolicy,
    HttpsFirstPolicy,
}

/// Why an HTTP navigation is allowed without upgrading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllowHttpReason {
    ModeDisabled,
    LoopbackOrLocalHost,
    PrivateNetworkIp,
    ExplicitExemption,
    CaptivePortalActive,
    CachedOpportunisticFallback,
    NonStandardPortPreserved,
}

/// Why an HTTP navigation is blocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockReason {
    StrictModeRequiresHttps,
    HstsRequiresHttps,
}

/// Resulting action for a navigation request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum UpgradeAction {
    /// URL is already secure or uses a non-HTTP scheme (`about:`, `file:`, etc.).
    KeepAsIs,
    /// Rewrite the request to `target_url` before sending.
    UpgradeToHttps {
        target_url: String,
        reason: UpgradeReason,
    },
    /// Allow plain HTTP for this request.
    AllowHttp { reason: AllowHttpReason },
    /// Block the request instead of sending plain HTTP.
    Block { reason: BlockReason },
}

/// Full decision record returned by [`HttpsUpgradePolicy::evaluate_navigation`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpgradeDecision {
    pub original_url: String,
    pub action: UpgradeAction,
}

/// Outcome after an upgraded HTTPS connection fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureOutcome {
    /// Downgrade to HTTP and use the cached fallback.
    FallbackToHttp,
    /// Strict mode: block and offer user an explicit bypass prompt.
    PromptUserOrBlock,
    /// HSTS active: hard block, no bypass permitted.
    BlockCannotDowngrade,
}

/// Configuration and mutable runtime state for HTTPS upgrade decisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpsUpgradePolicy {
    pub mode: HttpsUpgradeMode,
    pub upgrade_subresources: bool,
    pub allow_http_on_localhost: bool,
    pub allow_http_on_private_ips: bool,
    pub fallback_ttl_secs: u64,
    pub exemptions: Vec<ExemptionEntry>,
    pub hsts_entries: BTreeMap<String, HstsEntry>,
    pub fallback_cache: BTreeMap<String, FallbackRecord>,
}

impl Default for HttpsUpgradePolicy {
    fn default() -> Self {
        Self {
            mode: HttpsUpgradeMode::Opportunistic,
            upgrade_subresources: true,
            allow_http_on_localhost: true,
            allow_http_on_private_ips: true,
            fallback_ttl_secs: DEFAULT_FALLBACK_TTL_SECS,
            exemptions: Vec::new(),
            hsts_entries: BTreeMap::new(),
            fallback_cache: BTreeMap::new(),
        }
    }
}

impl HttpsUpgradePolicy {
    /// Creates a new policy with the given mode and validates configuration bounds.
    pub fn new(mode: HttpsUpgradeMode) -> Self {
        Self {
            mode,
            ..Self::default()
        }
    }

    /// Validates invariants on the policy configuration.
    pub fn validate(&self) -> Result<(), HttpsUpgradeError> {
        if self.fallback_ttl_secs == 0 {
            return Err(HttpsUpgradeError::InvalidConfig(
                "fallback_ttl_secs must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }

    /// Adds or replaces an exemption for `entry.pattern`.
    pub fn add_exemption(&mut self, entry: ExemptionEntry) {
        self.exemptions.retain(|e| e.pattern != entry.pattern);
        self.exemptions.push(entry);
    }

    /// Removes all exemptions matching `pattern`.
    pub fn remove_exemption(&mut self, pattern: &HostPattern) -> bool {
        let before = self.exemptions.len();
        self.exemptions.retain(|e| &e.pattern != pattern);
        self.exemptions.len() != before
    }

    /// Applies an HSTS response header for `host` at `now_unix`.
    ///
    /// Ignored when `is_incognito` is `true` so private browsing does not
    /// mutate persistent HSTS state.
    pub fn apply_hsts_header(
        &mut self,
        host: &str,
        header_value: &str,
        now_unix: i64,
        is_incognito: bool,
    ) -> Result<(), HttpsUpgradeError> {
        let norm_host = normalize_host(host)?;
        let directive = parse_hsts_header(header_value)?;
        if is_incognito {
            return Ok(());
        }

        if directive.max_age_secs == 0 {
            self.hsts_entries.remove(&norm_host);
            return Ok(());
        }

        let expires_at =
            now_unix.saturating_add(i64::try_from(directive.max_age_secs).unwrap_or(i64::MAX));
        self.hsts_entries.insert(
            norm_host.clone(),
            HstsEntry {
                host: norm_host,
                expires_at_unix: expires_at,
                include_subdomains: directive.include_subdomains,
                preloaded: false,
            },
        );
        Ok(())
    }

    /// Inserts a preloaded HSTS entry for `host`.
    pub fn add_preloaded_hsts(
        &mut self,
        host: &str,
        include_subdomains: bool,
    ) -> Result<(), HttpsUpgradeError> {
        let norm_host = normalize_host(host)?;
        self.hsts_entries.insert(
            norm_host.clone(),
            HstsEntry {
                host: norm_host,
                expires_at_unix: i64::MAX,
                include_subdomains,
                preloaded: true,
            },
        );
        Ok(())
    }

    /// Returns `true` if `host` is covered by an active HSTS entry (exact or parent `includeSubDomains`).
    pub fn is_hsts_active(&self, host: &str, now_unix: i64) -> bool {
        let Ok(norm_host) = normalize_host(host) else {
            return false;
        };
        if let Some(entry) = self.hsts_entries.get(&norm_host) {
            if entry.is_active(now_unix) {
                return true;
            }
        }
        // Walk parent domains for include_subdomains matches.
        let mut remainder = norm_host.as_str();
        while let Some((_, parent)) = remainder.split_once('.') {
            if let Some(entry) = self.hsts_entries.get(parent) {
                if entry.include_subdomains && entry.is_active(now_unix) {
                    return true;
                }
            }
            remainder = parent;
        }
        false
    }

    /// Evaluates how a navigation request should be handled under this policy.
    pub fn evaluate_navigation(
        &self,
        ctx: &UpgradeContext,
    ) -> Result<UpgradeDecision, HttpsUpgradeError> {
        let parsed = Url::parse(&ctx.url)
            .map_err(|e| HttpsUpgradeError::InvalidUrl(ctx.url.clone(), e.to_string()))?;

        if parsed.scheme() != "http" {
            return Ok(UpgradeDecision {
                original_url: ctx.url.clone(),
                action: UpgradeAction::KeepAsIs,
            });
        }

        let host = parsed.host_str().ok_or_else(|| {
            HttpsUpgradeError::InvalidUrl(ctx.url.clone(), "missing host in HTTP URL".to_string())
        })?;
        let norm_host = normalize_host(host)?;

        // HSTS always upgrades HTTP -> HTTPS regardless of mode or exemptions.
        if self.is_hsts_active(&norm_host, ctx.now_unix) {
            let target_url = rewrite_url_to_https(&parsed)?;
            return Ok(UpgradeDecision {
                original_url: ctx.url.clone(),
                action: UpgradeAction::UpgradeToHttps {
                    target_url,
                    reason: UpgradeReason::HstsPolicy,
                },
            });
        }

        if self.mode == HttpsUpgradeMode::Disabled {
            return Ok(UpgradeDecision {
                original_url: ctx.url.clone(),
                action: UpgradeAction::AllowHttp {
                    reason: AllowHttpReason::ModeDisabled,
                },
            });
        }

        if !ctx.is_main_frame && !self.upgrade_subresources {
            return Ok(UpgradeDecision {
                original_url: ctx.url.clone(),
                action: UpgradeAction::AllowHttp {
                    reason: AllowHttpReason::ModeDisabled,
                },
            });
        }

        if self.allow_http_on_localhost && is_localhost_host(&norm_host) {
            return Ok(UpgradeDecision {
                original_url: ctx.url.clone(),
                action: UpgradeAction::AllowHttp {
                    reason: AllowHttpReason::LoopbackOrLocalHost,
                },
            });
        }

        if self.allow_http_on_private_ips && is_private_ip_host(&norm_host) {
            return Ok(UpgradeDecision {
                original_url: ctx.url.clone(),
                action: UpgradeAction::AllowHttp {
                    reason: AllowHttpReason::PrivateNetworkIp,
                },
            });
        }

        if ctx.captive_portal_active {
            return Ok(UpgradeDecision {
                original_url: ctx.url.clone(),
                action: UpgradeAction::AllowHttp {
                    reason: AllowHttpReason::CaptivePortalActive,
                },
            });
        }

        if self.has_active_exemption(&norm_host, ctx.now_unix) {
            return Ok(UpgradeDecision {
                original_url: ctx.url.clone(),
                action: UpgradeAction::AllowHttp {
                    reason: AllowHttpReason::ExplicitExemption,
                },
            });
        }

        // Preserve explicit non-standard ports (anything other than default 80)
        // in opportunistic mode because port 8080/3000 rarely serves TLS on the same port.
        if let Some(port) = parsed.port() {
            if port != 80 && self.mode == HttpsUpgradeMode::Opportunistic {
                return Ok(UpgradeDecision {
                    original_url: ctx.url.clone(),
                    action: UpgradeAction::AllowHttp {
                        reason: AllowHttpReason::NonStandardPortPreserved,
                    },
                });
            }
        }

        if self.mode == HttpsUpgradeMode::Opportunistic {
            if let Some(record) = self.fallback_cache.get(&norm_host) {
                if record.is_active(ctx.now_unix) {
                    return Ok(UpgradeDecision {
                        original_url: ctx.url.clone(),
                        action: UpgradeAction::AllowHttp {
                            reason: AllowHttpReason::CachedOpportunisticFallback,
                        },
                    });
                }
            }
        }

        let target_url = rewrite_url_to_https(&parsed)?;
        Ok(UpgradeDecision {
            original_url: ctx.url.clone(),
            action: UpgradeAction::UpgradeToHttps {
                target_url,
                reason: UpgradeReason::HttpsFirstPolicy,
            },
        })
    }

    /// Records a failed HTTPS upgrade attempt and returns the required recovery action.
    pub fn record_upgrade_failure(
        &mut self,
        host: &str,
        now_unix: i64,
        is_incognito: bool,
        reason: FailureReason,
    ) -> Result<FailureOutcome, HttpsUpgradeError> {
        let norm_host = normalize_host(host)?;

        if self.is_hsts_active(&norm_host, now_unix) {
            return Ok(FailureOutcome::BlockCannotDowngrade);
        }

        match self.mode {
            HttpsUpgradeMode::Strict | HttpsUpgradeMode::IncognitoStrict => {
                Ok(FailureOutcome::PromptUserOrBlock)
            }
            HttpsUpgradeMode::Opportunistic => {
                if !is_incognito {
                    let ttl = i64::try_from(self.fallback_ttl_secs).unwrap_or(i64::MAX);
                    self.fallback_cache.insert(
                        norm_host.clone(),
                        FallbackRecord {
                            host: norm_host,
                            failed_at_unix: now_unix,
                            expires_at_unix: now_unix.saturating_add(ttl),
                            reason,
                        },
                    );
                }
                Ok(FailureOutcome::FallbackToHttp)
            }
            HttpsUpgradeMode::Disabled => Ok(FailureOutcome::FallbackToHttp),
        }
    }

    /// Purges expired exemptions, HSTS records, and fallback cache entries.
    pub fn prune_expired(&mut self, now_unix: i64) {
        self.exemptions.retain(|e| e.is_active(now_unix));
        self.hsts_entries.retain(|_, e| e.is_active(now_unix));
        self.fallback_cache.retain(|_, r| r.is_active(now_unix));
    }

    /// Serializes the policy to JSON.
    pub fn to_json(&self) -> Result<String, HttpsUpgradeError> {
        serde_json::to_string_pretty(self)
            .map_err(|e| HttpsUpgradeError::Serialization(e.to_string()))
    }

    /// Deserializes and validates a policy from JSON.
    pub fn from_json(raw: &str) -> Result<Self, HttpsUpgradeError> {
        let policy: Self = serde_json::from_str(raw)
            .map_err(|e| HttpsUpgradeError::Serialization(e.to_string()))?;
        policy.validate()?;
        Ok(policy)
    }

    fn has_active_exemption(&self, norm_host: &str, now_unix: i64) -> bool {
        self.exemptions
            .iter()
            .any(|e| e.is_active(now_unix) && e.pattern.matches(norm_host))
    }
}

fn normalize_host(host: &str) -> Result<String, HttpsUpgradeError> {
    let trimmed = host
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if trimmed.is_empty() {
        return Err(HttpsUpgradeError::InvalidHostPattern(
            host.to_string(),
            "host must not be empty".to_string(),
        ));
    }
    Ok(trimmed)
}

fn rewrite_url_to_https(parsed: &Url) -> Result<String, HttpsUpgradeError> {
    let mut upgraded = parsed.clone();
    upgraded.set_scheme("https").map_err(|_| {
        HttpsUpgradeError::InvalidUrl(
            parsed.to_string(),
            "failed to set scheme to https".to_string(),
        )
    })?;
    if parsed.port() == Some(80) {
        let _ = upgraded.set_port(None);
    }
    Ok(upgraded.into())
}

fn is_localhost_host(norm_host: &str) -> bool {
    if norm_host == "localhost"
        || norm_host.ends_with(".localhost")
        || norm_host.ends_with(".local")
    {
        return true;
    }
    if let Ok(ip) = norm_host.parse::<IpAddr>() {
        return ip.is_loopback();
    }
    false
}

fn is_private_ip_host(norm_host: &str) -> bool {
    let Ok(ip) = norm_host.parse::<IpAddr>() else {
        return false;
    };
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => {
            let seg0 = v6.segments()[0];
            // Unique local (fc00::/7) or link-local unicast (fe80::/10)
            (seg0 & 0xfe00) == 0xfc00 || (seg0 & 0xffc0) == 0xfe80
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(url: &str) -> UpgradeContext {
        UpgradeContext {
            url: url.to_string(),
            is_incognito: false,
            is_main_frame: true,
            now_unix: 1_700_000_000,
            captive_portal_active: false,
        }
    }

    #[test]
    fn upgrades_plain_http_and_strips_port_80() {
        let policy = HttpsUpgradePolicy::new(HttpsUpgradeMode::Opportunistic);
        let decision = policy
            .evaluate_navigation(&ctx("http://example.com:80/search?q=linux#top"))
            .unwrap();

        assert_eq!(
            decision.action,
            UpgradeAction::UpgradeToHttps {
                target_url: "https://example.com/search?q=linux#top".to_string(),
                reason: UpgradeReason::HttpsFirstPolicy,
            }
        );

        let already_https = policy
            .evaluate_navigation(&ctx("https://example.com/home"))
            .unwrap();
        assert_eq!(already_https.action, UpgradeAction::KeepAsIs);
    }

    #[test]
    fn hsts_header_parsing_and_subdomain_matching() {
        let mut policy = HttpsUpgradePolicy::new(HttpsUpgradeMode::Disabled);
        policy
            .apply_hsts_header(
                "Example.COM",
                "max-age=\"31536000\"; includeSubDomains; preload",
                1_700_000_000,
                false,
            )
            .unwrap();

        // Even with mode=Disabled, HSTS upgrades subdomains.
        let sub_decision = policy
            .evaluate_navigation(&ctx("http://docs.example.com/guide"))
            .unwrap();
        assert_eq!(
            sub_decision.action,
            UpgradeAction::UpgradeToHttps {
                target_url: "https://docs.example.com/guide".to_string(),
                reason: UpgradeReason::HstsPolicy,
            }
        );

        // max-age=0 removes the HSTS entry.
        policy
            .apply_hsts_header("example.com", "max-age=0", 1_700_000_010, false)
            .unwrap();
        let after_clear = policy
            .evaluate_navigation(&ctx("http://docs.example.com/guide"))
            .unwrap();
        assert_eq!(
            after_clear.action,
            UpgradeAction::AllowHttp {
                reason: AllowHttpReason::ModeDisabled,
            }
        );
    }

    #[test]
    fn exemptions_and_wildcard_patterns() {
        let mut policy = HttpsUpgradePolicy::new(HttpsUpgradeMode::Strict);
        let pattern = HostPattern::parse("*.intranet.corp").unwrap();
        policy.add_exemption(ExemptionEntry {
            pattern,
            scope: ExemptionScope::Timed {
                expires_at_unix: 1_700_000_500,
            },
            reason: ExemptionReason::EnterprisePolicy,
            created_at_unix: 1_700_000_000,
        });

        let exempt_decision = policy
            .evaluate_navigation(&ctx("http://wiki.intranet.corp/index"))
            .unwrap();
        assert_eq!(
            exempt_decision.action,
            UpgradeAction::AllowHttp {
                reason: AllowHttpReason::ExplicitExemption,
            }
        );

        // Expired exemption falls back to Strict upgrade.
        let mut expired_ctx = ctx("http://wiki.intranet.corp/index");
        expired_ctx.now_unix = 1_700_000_600;
        let expired_decision = policy.evaluate_navigation(&expired_ctx).unwrap();
        assert!(matches!(
            expired_decision.action,
            UpgradeAction::UpgradeToHttps {
                reason: UpgradeReason::HttpsFirstPolicy,
                ..
            }
        ));
    }

    #[test]
    fn localhost_and_private_ips_allowed_over_http() {
        let policy = HttpsUpgradePolicy::new(HttpsUpgradeMode::Strict);

        for url in [
            "http://localhost:8080/api",
            "http://127.0.0.1:3000/",
            "http://[::1]:8000/test",
        ] {
            let d = policy.evaluate_navigation(&ctx(url)).unwrap();
            assert_eq!(
                d.action,
                UpgradeAction::AllowHttp {
                    reason: AllowHttpReason::LoopbackOrLocalHost,
                }
            );
        }

        let lan = policy
            .evaluate_navigation(&ctx("http://192.168.1.1/status"))
            .unwrap();
        assert_eq!(
            lan.action,
            UpgradeAction::AllowHttp {
                reason: AllowHttpReason::PrivateNetworkIp,
            }
        );
    }

    #[test]
    fn opportunistic_fallback_cache_and_strict_hsts_failure() {
        let mut opp = HttpsUpgradePolicy::new(HttpsUpgradeMode::Opportunistic);
        let outcome = opp
            .record_upgrade_failure(
                "legacy.example.org",
                1_700_000_000,
                false,
                FailureReason::ConnectionRefused,
            )
            .unwrap();
        assert_eq!(outcome, FailureOutcome::FallbackToHttp);

        let next = opp
            .evaluate_navigation(&ctx("http://legacy.example.org/page"))
            .unwrap();
        assert_eq!(
            next.action,
            UpgradeAction::AllowHttp {
                reason: AllowHttpReason::CachedOpportunisticFallback,
            }
        );

        // HSTS cannot downgrade even in opportunistic mode.
        opp.add_preloaded_hsts("bank.example.org", true).unwrap();
        let hsts_fail = opp
            .record_upgrade_failure(
                "app.bank.example.org",
                1_700_000_000,
                false,
                FailureReason::CertificateInvalid,
            )
            .unwrap();
        assert_eq!(hsts_fail, FailureOutcome::BlockCannotDowngrade);
    }

    #[test]
    fn serialization_roundtrip_and_invalid_inputs() {
        let mut policy = HttpsUpgradePolicy::new(HttpsUpgradeMode::Strict);
        policy.add_preloaded_hsts("secure.example", true).unwrap();
        let json = policy.to_json().unwrap();
        let restored = HttpsUpgradePolicy::from_json(&json).unwrap();
        assert_eq!(policy, restored);

        assert!(HostPattern::parse("https://example.com").is_err());
        assert!(HostPattern::parse("*").is_err());
        assert!(parse_hsts_header("includeSubDomains").is_err());
        assert!(policy.evaluate_navigation(&ctx("not a url")).is_err());
    }
}
