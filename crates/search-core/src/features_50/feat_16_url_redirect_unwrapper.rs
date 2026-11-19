use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;
use url::Url;

/// Maximum allowed recursion depth that can be configured to prevent unbounded work.
pub const MAX_ALLOWED_DEPTH: usize = 64;

/// Errors produced when configuring or running the URL redirect unwrapper.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RedirectUnwrapperError {
    #[error("invalid input URL '{input}': {reason}")]
    InvalidInputUrl { input: String, reason: String },

    #[error("disallowed input URL scheme '{scheme}'")]
    DisallowedInputScheme { scheme: String },

    #[error("invalid unwrapper configuration: {0}")]
    InvalidConfig(String),

    #[error("invalid redirect rule '{rule_id}': {reason}")]
    InvalidRule { rule_id: String, reason: String },
}

/// Host matching strategy for a redirect wrapper rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum HostMatch {
    /// Matches an exact hostname (case-insensitive).
    Exact(String),
    /// Matches the domain itself or any subdomain of it (case-insensitive).
    DomainOrSubdomain(String),
    /// Matches any host.
    Any,
}

impl HostMatch {
    fn validate(&self, rule_id: &str) -> Result<(), RedirectUnwrapperError> {
        match self {
            HostMatch::Exact(host) | HostMatch::DomainOrSubdomain(host) => {
                let trimmed = host.trim().trim_matches('.');
                if trimmed.is_empty() {
                    return Err(RedirectUnwrapperError::InvalidRule {
                        rule_id: rule_id.to_string(),
                        reason: "host pattern must not be empty".to_string(),
                    });
                }
                Ok(())
            }
            HostMatch::Any => Ok(()),
        }
    }

    fn matches(&self, candidate_host: &str) -> bool {
        let host = candidate_host.trim_end_matches('.').to_ascii_lowercase();
        match self {
            HostMatch::Exact(expected) => {
                host == expected.trim().trim_end_matches('.').to_ascii_lowercase()
            }
            HostMatch::DomainOrSubdomain(domain) => {
                let dom = domain.trim().trim_matches('.').to_ascii_lowercase();
                if dom.is_empty() {
                    return false;
                }
                if host == dom {
                    return true;
                }
                host.len() > dom.len()
                    && host.ends_with(&dom)
                    && host.as_bytes()[host.len() - dom.len() - 1] == b'.'
            }
            HostMatch::Any => true,
        }
    }
}

/// Where the wrapped destination URL is located inside the wrapper URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExtractionSource {
    /// Extract from the first non-empty matching query parameter.
    QueryParam { keys: Vec<String> },
    /// Extract from a path segment starting with `prefix` (percent-decoded).
    PathSegmentPrefix { prefix: String },
}

impl ExtractionSource {
    fn validate(&self, rule_id: &str) -> Result<(), RedirectUnwrapperError> {
        match self {
            ExtractionSource::QueryParam { keys } => {
                if keys.is_empty() || keys.iter().all(|k| k.trim().is_empty()) {
                    return Err(RedirectUnwrapperError::InvalidRule {
                        rule_id: rule_id.to_string(),
                        reason: "query_param rule must define at least one non-empty key"
                            .to_string(),
                    });
                }
                Ok(())
            }
            ExtractionSource::PathSegmentPrefix { prefix } => {
                if prefix.is_empty() {
                    return Err(RedirectUnwrapperError::InvalidRule {
                        rule_id: rule_id.to_string(),
                        reason: "path_segment_prefix must not be empty".to_string(),
                    });
                }
                Ok(())
            }
        }
    }
}

/// Declarative rule describing a known redirect/tracking wrapper URL pattern.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedirectRule {
    pub id: String,
    pub description: String,
    pub host: HostMatch,
    pub path_prefix: String,
    pub source: ExtractionSource,
    pub enabled: bool,
}

impl RedirectRule {
    /// Validates the rule's fields.
    pub fn validate(&self) -> Result<(), RedirectUnwrapperError> {
        if self.id.trim().is_empty() {
            return Err(RedirectUnwrapperError::InvalidRule {
                rule_id: self.id.clone(),
                reason: "rule id must not be empty".to_string(),
            });
        }
        if !self.path_prefix.starts_with('/') {
            return Err(RedirectUnwrapperError::InvalidRule {
                rule_id: self.id.clone(),
                reason: "path_prefix must start with '/'".to_string(),
            });
        }
        self.host.validate(&self.id)?;
        self.source.validate(&self.id)?;
        Ok(())
    }

    fn matches_path(&self, path: &str) -> bool {
        if self.path_prefix == "/" {
            return true;
        }
        let normalized_prefix = self.path_prefix.trim_end_matches('/');
        if path == normalized_prefix {
            return true;
        }
        path.starts_with(normalized_prefix)
            && path.as_bytes().get(normalized_prefix.len()) == Some(&b'/')
    }

    fn extract_target(&self, url: &Url) -> Option<(String, String)> {
        match &self.source {
            ExtractionSource::QueryParam { keys } => {
                for key in keys {
                    if key.trim().is_empty() {
                        continue;
                    }
                    for (param_key, param_val) in url.query_pairs() {
                        if param_key == key.as_str() {
                            let trimmed = param_val.trim();
                            if !trimmed.is_empty() {
                                return Some((format!("query:{}", key), trimmed.to_string()));
                            }
                        }
                    }
                }
                None
            }
            ExtractionSource::PathSegmentPrefix { prefix } => {
                let segments = url.path_segments()?;
                for segment in segments {
                    if let Some(encoded) = segment.strip_prefix(prefix.as_str()) {
                        let decoded = percent_decode(encoded)?;
                        let trimmed = decoded.trim();
                        if !trimmed.is_empty() {
                            return Some((format!("path:{}", prefix), trimmed.to_string()));
                        }
                    }
                }
                None
            }
        }
    }
}

/// Configuration for `UrlRedirectUnwrapper`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnwrapperConfig {
    pub max_depth: usize,
    pub allowed_schemes: Vec<String>,
    pub reject_credentials_in_url: bool,
    pub rules: Vec<RedirectRule>,
}

impl Default for UnwrapperConfig {
    fn default() -> Self {
        Self {
            max_depth: 8,
            allowed_schemes: vec!["http".to_string(), "https".to_string()],
            reject_credentials_in_url: true,
            rules: default_redirect_rules(),
        }
    }
}

impl UnwrapperConfig {
    /// Validates configuration invariants and all contained rules.
    pub fn validate(&self) -> Result<(), RedirectUnwrapperError> {
        if self.max_depth == 0 || self.max_depth > MAX_ALLOWED_DEPTH {
            return Err(RedirectUnwrapperError::InvalidConfig(format!(
                "max_depth must be between 1 and {}",
                MAX_ALLOWED_DEPTH
            )));
        }
        if self.allowed_schemes.is_empty()
            || self.allowed_schemes.iter().any(|s| s.trim().is_empty())
        {
            return Err(RedirectUnwrapperError::InvalidConfig(
                "allowed_schemes must contain at least one non-empty scheme".to_string(),
            ));
        }
        let mut seen_ids = HashSet::new();
        for rule in &self.rules {
            rule.validate()?;
            if !seen_ids.insert(rule.id.as_str()) {
                return Err(RedirectUnwrapperError::InvalidConfig(format!(
                    "duplicate rule id '{}'",
                    rule.id
                )));
            }
        }
        Ok(())
    }
}

/// Reason why recursive unwrapping stopped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    NoMatchingRule,
    MaxDepthReached,
    LoopDetected,
    InvalidTargetUrl,
    DisallowedTargetScheme,
    DisallowedTargetCredentials,
}

/// Details of a single redirect-unwrapping step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnwrapHop {
    pub step: usize,
    pub wrapper_url: String,
    pub rule_id: String,
    pub extracted_via: String,
    pub target_url: String,
}

/// Complete report of an unwrap operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnwrapReport {
    pub original_url: String,
    pub final_url: String,
    pub unwrapped: bool,
    pub hops: Vec<UnwrapHop>,
    pub stop_reason: StopReason,
}

/// Recursive URL redirect unwrapper with loop detection and scheme validation.
#[derive(Debug, Clone)]
pub struct UrlRedirectUnwrapper {
    config: UnwrapperConfig,
    normalized_schemes: Vec<String>,
}

impl Default for UrlRedirectUnwrapper {
    fn default() -> Self {
        Self::with_defaults()
    }
}

impl UrlRedirectUnwrapper {
    /// Creates a new unwrapper after validating `config`.
    pub fn new(config: UnwrapperConfig) -> Result<Self, RedirectUnwrapperError> {
        config.validate()?;
        let normalized_schemes = config
            .allowed_schemes
            .iter()
            .map(|s| s.trim().to_ascii_lowercase())
            .collect();
        Ok(Self {
            config,
            normalized_schemes,
        })
    }

    /// Creates an unwrapper with the built-in default ruleset.
    pub fn with_defaults() -> Self {
        let config = UnwrapperConfig::default();
        let normalized_schemes = config
            .allowed_schemes
            .iter()
            .map(|s| s.trim().to_ascii_lowercase())
            .collect();
        Self {
            config,
            normalized_schemes,
        }
    }

    /// Returns a reference to the active configuration.
    pub fn config(&self) -> &UnwrapperConfig {
        &self.config
    }

    /// Unwraps nested redirect wrapper URLs starting from `input`.
    pub fn unwrap_url(&self, input: &str) -> Result<UnwrapReport, RedirectUnwrapperError> {
        let trimmed_input = input.trim();
        let mut current =
            Url::parse(trimmed_input).map_err(|err| RedirectUnwrapperError::InvalidInputUrl {
                input: input.to_string(),
                reason: err.to_string(),
            })?;

        if !self.is_scheme_allowed(current.scheme()) {
            return Err(RedirectUnwrapperError::DisallowedInputScheme {
                scheme: current.scheme().to_string(),
            });
        }

        let original_url = current.to_string();
        let mut visited = HashSet::new();
        visited.insert(original_url.clone());

        let mut hops = Vec::new();
        let stop_reason = loop {
            if hops.len() >= self.config.max_depth {
                break StopReason::MaxDepthReached;
            }

            let Some((rule_id, extracted_via, raw_target)) = self.match_and_extract(&current)
            else {
                break StopReason::NoMatchingRule;
            };

            let Ok(next_url) = Url::parse(&raw_target) else {
                break StopReason::InvalidTargetUrl;
            };

            if !self.is_scheme_allowed(next_url.scheme()) {
                break StopReason::DisallowedTargetScheme;
            }

            if self.config.reject_credentials_in_url
                && (!next_url.username().is_empty() || next_url.password().is_some())
            {
                break StopReason::DisallowedTargetCredentials;
            }

            let next_url_str = next_url.to_string();
            if !visited.insert(next_url_str.clone()) {
                break StopReason::LoopDetected;
            }

            hops.push(UnwrapHop {
                step: hops.len() + 1,
                wrapper_url: current.to_string(),
                rule_id,
                extracted_via,
                target_url: next_url_str,
            });

            current = next_url;
        };

        Ok(UnwrapReport {
            original_url,
            final_url: current.to_string(),
            unwrapped: !hops.is_empty(),
            hops,
            stop_reason,
        })
    }

    /// Convenience helper returning the unwrapped URL or the original input if parsing fails.
    pub fn unwrap_if_possible(&self, input: &str) -> String {
        match self.unwrap_url(input) {
            Ok(report) => report.final_url,
            Err(_) => input.to_string(),
        }
    }

    fn is_scheme_allowed(&self, scheme: &str) -> bool {
        let lower = scheme.to_ascii_lowercase();
        self.normalized_schemes.iter().any(|s| s == &lower)
    }

    fn match_and_extract(&self, url: &Url) -> Option<(String, String, String)> {
        let host = url.host_str()?;
        let path = url.path();

        for rule in &self.config.rules {
            if !rule.enabled {
                continue;
            }
            if !rule.host.matches(host) {
                continue;
            }
            if !rule.matches_path(path) {
                continue;
            }
            if let Some((via, target)) = rule.extract_target(url) {
                return Some((rule.id.clone(), via, target));
            }
        }
        None
    }
}

/// Returns the built-in default redirect ruleset for common search engines and link trackers.
pub fn default_redirect_rules() -> Vec<RedirectRule> {
    vec![
        RedirectRule {
            id: "google-url".to_string(),
            description: "Google search and outbound /url redirector".to_string(),
            host: HostMatch::DomainOrSubdomain("google.com".to_string()),
            path_prefix: "/url".to_string(),
            source: ExtractionSource::QueryParam {
                keys: vec!["q".to_string(), "url".to_string()],
            },
            enabled: true,
        },
        RedirectRule {
            id: "youtube-redirect".to_string(),
            description: "YouTube outbound /redirect wrapper".to_string(),
            host: HostMatch::DomainOrSubdomain("youtube.com".to_string()),
            path_prefix: "/redirect".to_string(),
            source: ExtractionSource::QueryParam {
                keys: vec!["q".to_string()],
            },
            enabled: true,
        },
        RedirectRule {
            id: "facebook-linkshim".to_string(),
            description: "Facebook outbound linkshim l.facebook.com".to_string(),
            host: HostMatch::Exact("l.facebook.com".to_string()),
            path_prefix: "/l.php".to_string(),
            source: ExtractionSource::QueryParam {
                keys: vec!["u".to_string()],
            },
            enabled: true,
        },
        RedirectRule {
            id: "facebook-mobile-linkshim".to_string(),
            description: "Facebook mobile outbound linkshim lm.facebook.com".to_string(),
            host: HostMatch::Exact("lm.facebook.com".to_string()),
            path_prefix: "/l.php".to_string(),
            source: ExtractionSource::QueryParam {
                keys: vec!["u".to_string()],
            },
            enabled: true,
        },
        RedirectRule {
            id: "instagram-linkshim".to_string(),
            description: "Instagram outbound linkshim".to_string(),
            host: HostMatch::Exact("l.instagram.com".to_string()),
            path_prefix: "/".to_string(),
            source: ExtractionSource::QueryParam {
                keys: vec!["u".to_string()],
            },
            enabled: true,
        },
        RedirectRule {
            id: "duckduckgo-l".to_string(),
            description: "DuckDuckGo /l/ redirector".to_string(),
            host: HostMatch::DomainOrSubdomain("duckduckgo.com".to_string()),
            path_prefix: "/l".to_string(),
            source: ExtractionSource::QueryParam {
                keys: vec!["uddg".to_string()],
            },
            enabled: true,
        },
        RedirectRule {
            id: "outlook-safelinks".to_string(),
            description: "Microsoft Outlook SafeLinks wrapper".to_string(),
            host: HostMatch::DomainOrSubdomain("safelinks.protection.outlook.com".to_string()),
            path_prefix: "/".to_string(),
            source: ExtractionSource::QueryParam {
                keys: vec!["url".to_string()],
            },
            enabled: true,
        },
        RedirectRule {
            id: "reddit-out".to_string(),
            description: "Reddit out.reddit.com outbound tracker".to_string(),
            host: HostMatch::Exact("out.reddit.com".to_string()),
            path_prefix: "/".to_string(),
            source: ExtractionSource::QueryParam {
                keys: vec!["url".to_string()],
            },
            enabled: true,
        },
        RedirectRule {
            id: "steam-linkfilter".to_string(),
            description: "Steam Community linkfilter".to_string(),
            host: HostMatch::DomainOrSubdomain("steamcommunity.com".to_string()),
            path_prefix: "/linkfilter".to_string(),
            source: ExtractionSource::QueryParam {
                keys: vec!["u".to_string(), "url".to_string()],
            },
            enabled: true,
        },
        RedirectRule {
            id: "linkedin-redir".to_string(),
            description: "LinkedIn /redir/redirect wrapper".to_string(),
            host: HostMatch::DomainOrSubdomain("linkedin.com".to_string()),
            path_prefix: "/redir/redirect".to_string(),
            source: ExtractionSource::QueryParam {
                keys: vec!["url".to_string()],
            },
            enabled: true,
        },
        RedirectRule {
            id: "yahoo-search-ru".to_string(),
            description: "Yahoo Search r.search.yahoo.com RU= path redirector".to_string(),
            host: HostMatch::DomainOrSubdomain("r.search.yahoo.com".to_string()),
            path_prefix: "/".to_string(),
            source: ExtractionSource::PathSegmentPrefix {
                prefix: "RU=".to_string(),
            },
            enabled: true,
        },
    ]
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return None;
            }
            let hi = hex_val(bytes[i + 1])?;
            let lo = hex_val(bytes[i + 2])?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwraps_single_and_nested_redirects() {
        let unwrapper = UrlRedirectUnwrapper::with_defaults();

        let google = "https://www.google.com/url?q=https%3A%2F%2Fexample.org%2Fdocs%3Fa%3D1&sa=U";
        let report = unwrapper.unwrap_url(google).expect("unwrap google url");
        assert!(report.unwrapped);
        assert_eq!(report.final_url, "https://example.org/docs?a=1");
        assert_eq!(report.hops.len(), 1);
        assert_eq!(report.hops[0].rule_id, "google-url");
        assert_eq!(report.stop_reason, StopReason::NoMatchingRule);

        // Nested: Outlook SafeLinks -> Facebook linkshim -> DuckDuckGo -> final target
        let ddg = "https://duckduckgo.com/l/?uddg=https%3A%2F%2Frust-lang.org%2F";
        let fb = format!(
            "https://l.facebook.com/l.php?u={}",
            url::form_urlencoded::byte_serialize(ddg.as_bytes()).collect::<String>()
        );
        let safelinks = format!(
            "https://eur01.safelinks.protection.outlook.com/?url={}&data=05",
            url::form_urlencoded::byte_serialize(fb.as_bytes()).collect::<String>()
        );

        let nested = unwrapper
            .unwrap_url(&safelinks)
            .expect("unwrap nested chain");
        assert!(nested.unwrapped);
        assert_eq!(nested.final_url, "https://rust-lang.org/");
        assert_eq!(nested.hops.len(), 3);
        assert_eq!(nested.hops[0].rule_id, "outlook-safelinks");
        assert_eq!(nested.hops[1].rule_id, "facebook-linkshim");
        assert_eq!(nested.hops[2].rule_id, "duckduckgo-l");
        assert_eq!(nested.stop_reason, StopReason::NoMatchingRule);
    }

    #[test]
    fn unwraps_yahoo_path_segment_and_preserves_clean_urls() {
        let unwrapper = UrlRedirectUnwrapper::with_defaults();

        let yahoo = "https://r.search.yahoo.com/_ylt=Awr9/RV=2/RE=1700000000/RO=10/RU=https%3a%2f%2fkernel.org%2f/RK=2/RS=abc";
        let report = unwrapper.unwrap_url(yahoo).expect("unwrap yahoo url");
        assert!(report.unwrapped);
        assert_eq!(report.final_url, "https://kernel.org/");
        assert_eq!(report.hops[0].extracted_via, "path:RU=");

        let clean = "https://crates.io/crates/serde";
        let clean_report = unwrapper.unwrap_url(clean).expect("clean url");
        assert!(!clean_report.unwrapped);
        assert_eq!(clean_report.final_url, clean);
        assert!(clean_report.hops.is_empty());
        assert_eq!(clean_report.stop_reason, StopReason::NoMatchingRule);
    }

    #[test]
    fn enforces_security_scheme_and_credentials_checks() {
        let unwrapper = UrlRedirectUnwrapper::with_defaults();

        // Disallowed target scheme (javascript:)
        let xss = "https://www.google.com/url?q=javascript%3Aalert(1)";
        let xss_report = unwrapper.unwrap_url(xss).expect("report stops safely");
        assert!(!xss_report.unwrapped);
        assert_eq!(xss_report.final_url, xss);
        assert_eq!(xss_report.stop_reason, StopReason::DisallowedTargetScheme);

        // Disallowed embedded credentials in target
        let creds = "https://www.google.com/url?q=https%3A%2F%2Fuser%3Apass%40example.com%2F";
        let creds_report = unwrapper.unwrap_url(creds).expect("report stops safely");
        assert!(!creds_report.unwrapped);
        assert_eq!(
            creds_report.stop_reason,
            StopReason::DisallowedTargetCredentials
        );

        // Disallowed input scheme
        let bad_input = unwrapper.unwrap_url("file:///etc/passwd");
        assert!(matches!(
            bad_input,
            Err(RedirectUnwrapperError::DisallowedInputScheme { .. })
        ));
    }

    #[test]
    fn respects_max_depth_and_handles_invalid_target() {
        let cfg = UnwrapperConfig {
            max_depth: 10,
            ..Default::default()
        };
        let custom = UrlRedirectUnwrapper::new(cfg).expect("valid config");

        let invalid_target = "https://out.reddit.com/?url=not-a-valid-url";
        let r_invalid = custom
            .unwrap_url(invalid_target)
            .expect("stops at invalid target");
        assert!(!r_invalid.unwrapped);
        assert_eq!(r_invalid.stop_reason, StopReason::InvalidTargetUrl);

        // Max depth = 1 stops after 1 hop
        let shallow_cfg = UnwrapperConfig {
            max_depth: 1,
            ..Default::default()
        };
        let shallow = UrlRedirectUnwrapper::new(shallow_cfg).expect("valid config");
        let two_hops = "https://l.facebook.com/l.php?u=https%3A%2F%2Fwww.google.com%2Furl%3Fq%3Dhttps%253A%252F%252Fexample.com%252F";
        let shallow_report = shallow.unwrap_url(two_hops).expect("shallow unwrap");
        assert_eq!(shallow_report.hops.len(), 1);
        assert_eq!(
            shallow_report.final_url,
            "https://www.google.com/url?q=https%3A%2F%2Fexample.com%2F"
        );
        assert_eq!(shallow_report.stop_reason, StopReason::MaxDepthReached);
    }

    #[test]
    fn config_validation_and_serde_roundtrip() {
        let bad_cfg = UnwrapperConfig {
            max_depth: 0,
            ..Default::default()
        };
        assert!(matches!(
            UrlRedirectUnwrapper::new(bad_cfg),
            Err(RedirectUnwrapperError::InvalidConfig(_))
        ));

        let unwrapper = UrlRedirectUnwrapper::with_defaults();
        let json = serde_json::to_string(unwrapper.config()).expect("serialize config");
        let decoded: UnwrapperConfig = serde_json::from_str(&json).expect("deserialize config");
        assert_eq!(&decoded, unwrapper.config());

        let report = unwrapper
            .unwrap_url("https://www.youtube.com/redirect?q=https%3A%2F%2Fexample.com%2F")
            .expect("unwrap youtube");
        let report_json = serde_json::to_string(&report).expect("serialize report");
        let report_decoded: UnwrapReport =
            serde_json::from_str(&report_json).expect("deserialize report");
        assert_eq!(report_decoded, report);
    }
}
