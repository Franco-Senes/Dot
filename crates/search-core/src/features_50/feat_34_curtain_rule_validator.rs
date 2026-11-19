//! Curtain Rule Validator (`feat_34_curtain_rule_validator`).
//!
//! Validates privacy/content-curtain filtering rules before they are loaded
//! into the browser's request-interception pipeline: syntax checks, regex
//! compilation and safety heuristics, URL/domain normalization, action
//! invariants, duplicate IDs, and priority shadowing detection.

use std::collections::{HashMap, HashSet};
use std::fmt;

use regex::Regex;
use ring::digest::{Context, SHA256};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

/// Errors raised when parsing or validating a serialized curtain ruleset.
#[derive(Debug, Error)]
pub enum CurtainValidationError {
    #[error("failed to parse curtain rules JSON: {0}")]
    JsonParse(#[from] serde_json::Error),

    #[error("ruleset exceeds maximum allowed size ({count} > {max})")]
    TooManyRules { count: usize, max: usize },
}

/// Action taken when a curtain rule matches a network request or resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CurtainAction {
    Block,
    Allow,
    Mask,
    Warn,
    Redirect { target: String },
    StripParams { params: Vec<String> },
}

/// Resource types a curtain rule can target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Document,
    SubDocument,
    Script,
    Image,
    Stylesheet,
    Font,
    Xhr,
    Fetch,
    WebSocket,
    Media,
    Other,
}

/// How the `pattern` field of a rule should be interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternKind {
    Domain,
    UrlPrefix,
    Glob,
    Regex,
}

/// Severity of a validation finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    Info,
    Warning,
    Error,
}

/// A single curtain rule definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurtainRule {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub priority: i32,
    pub pattern: String,
    pub pattern_kind: PatternKind,
    pub action: CurtainAction,
    #[serde(default)]
    pub resource_kinds: Vec<ResourceKind>,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub exclude_domains: Vec<String>,
    #[serde(default)]
    pub third_party_only: bool,
    #[serde(default)]
    pub max_hits_per_minute: Option<u32>,
    #[serde(default)]
    pub notes: Option<String>,
}

fn default_true() -> bool {
    true
}

/// A single validation issue discovered in a rule or ruleset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub rule_id: String,
    pub field: String,
    pub code: String,
    pub severity: ValidationSeverity,
    pub message: String,
}

/// Summary report produced by validating a slice of curtain rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationReport {
    pub valid: bool,
    pub total_rules: usize,
    pub enabled_rules: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
    pub ruleset_sha256: String,
    pub issues: Vec<ValidationIssue>,
}

/// Configurable policy knobs for `CurtainRuleValidator`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurtainValidatorConfig {
    pub allow_regex_rules: bool,
    pub max_regex_len: usize,
    pub max_pattern_len: usize,
    pub max_rules: usize,
    pub disallow_catch_all_allow: bool,
    pub check_shadowing: bool,
}

impl Default for CurtainValidatorConfig {
    fn default() -> Self {
        Self {
            allow_regex_rules: true,
            max_regex_len: 512,
            max_pattern_len: 2048,
            max_rules: 10_000,
            disallow_catch_all_allow: true,
            check_shadowing: true,
        }
    }
}

/// Stateless validator for curtain filtering rules.
#[derive(Debug, Clone, Default)]
pub struct CurtainRuleValidator {
    config: CurtainValidatorConfig,
}

impl CurtainRuleValidator {
    /// Creates a validator with the given configuration.
    pub fn new(config: CurtainValidatorConfig) -> Self {
        Self { config }
    }

    /// Returns the active validator configuration.
    pub fn config(&self) -> &CurtainValidatorConfig {
        &self.config
    }

    /// Parses a JSON array of `CurtainRule` items and validates the entire ruleset.
    pub fn parse_and_validate_json(
        &self,
        json_str: &str,
    ) -> Result<(Vec<CurtainRule>, ValidationReport), CurtainValidationError> {
        let rules: Vec<CurtainRule> = serde_json::from_str(json_str)?;
        if rules.len() > self.config.max_rules {
            return Err(CurtainValidationError::TooManyRules {
                count: rules.len(),
                max: self.config.max_rules,
            });
        }
        let report = self.validate_ruleset(&rules);
        Ok((rules, report))
    }

    /// Validates a single rule in isolation (does not check cross-rule duplicates or shadowing).
    pub fn validate_rule(&self, rule: &CurtainRule) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        self.check_identity(rule, &mut issues);
        self.check_pattern(rule, &mut issues);
        self.check_action(rule, &mut issues);
        self.check_domain_scopes(rule, &mut issues);
        self.check_rate_limit(rule, &mut issues);
        issues
    }

    /// Validates an entire ruleset, including cross-rule duplicate IDs and priority shadowing.
    pub fn validate_ruleset(&self, rules: &[CurtainRule]) -> ValidationReport {
        let mut issues = Vec::new();
        let mut seen_ids: HashMap<&str, usize> = HashMap::new();

        if rules.len() > self.config.max_rules {
            issues.push(ValidationIssue {
                rule_id: "<ruleset>".to_string(),
                field: "rules".to_string(),
                code: "RULESET_TOO_LARGE".to_string(),
                severity: ValidationSeverity::Error,
                message: format!(
                    "ruleset contains {} rules, exceeding maximum of {}",
                    rules.len(),
                    self.config.max_rules
                ),
            });
        }

        for (idx, rule) in rules.iter().enumerate() {
            if !rule.id.trim().is_empty() {
                if let Some(prev_idx) = seen_ids.insert(rule.id.as_str(), idx) {
                    issues.push(ValidationIssue {
                        rule_id: rule.id.clone(),
                        field: "id".to_string(),
                        code: "DUPLICATE_RULE_ID".to_string(),
                        severity: ValidationSeverity::Error,
                        message: format!(
                            "duplicate rule id '{}' (first seen at index {})",
                            rule.id, prev_idx
                        ),
                    });
                }
            }
            issues.extend(self.validate_rule(rule));
        }

        if self.config.check_shadowing {
            self.check_rule_shadowing(rules, &mut issues);
        }

        let enabled_rules = rules.iter().filter(|r| r.enabled).count();
        let error_count = issues
            .iter()
            .filter(|i| i.severity == ValidationSeverity::Error)
            .count();
        let warning_count = issues
            .iter()
            .filter(|i| i.severity == ValidationSeverity::Warning)
            .count();
        let info_count = issues
            .iter()
            .filter(|i| i.severity == ValidationSeverity::Info)
            .count();

        ValidationReport {
            valid: error_count == 0,
            total_rules: rules.len(),
            enabled_rules,
            error_count,
            warning_count,
            info_count,
            ruleset_sha256: compute_ruleset_digest(rules),
            issues,
        }
    }

    fn check_identity(&self, rule: &CurtainRule, issues: &mut Vec<ValidationIssue>) {
        let trimmed_id = rule.id.trim();
        if trimmed_id.is_empty() {
            issues.push(ValidationIssue {
                rule_id: rule.id.clone(),
                field: "id".to_string(),
                code: "EMPTY_RULE_ID".to_string(),
                severity: ValidationSeverity::Error,
                message: "rule id must not be empty".to_string(),
            });
        } else {
            if trimmed_id.len() != rule.id.len() {
                issues.push(ValidationIssue {
                    rule_id: rule.id.clone(),
                    field: "id".to_string(),
                    code: "WHITESPACE_IN_ID".to_string(),
                    severity: ValidationSeverity::Warning,
                    message: "rule id has leading or trailing whitespace".to_string(),
                });
            }
            if !rule
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            {
                issues.push(ValidationIssue {
                    rule_id: rule.id.clone(),
                    field: "id".to_string(),
                    code: "INVALID_ID_CHARS".to_string(),
                    severity: ValidationSeverity::Error,
                    message: "rule id may only contain ASCII alphanumerics, '-', '_', or '.'"
                        .to_string(),
                });
            }
        }

        if rule.name.trim().is_empty() {
            issues.push(ValidationIssue {
                rule_id: rule.id.clone(),
                field: "name".to_string(),
                code: "EMPTY_RULE_NAME".to_string(),
                severity: ValidationSeverity::Warning,
                message: "rule name should not be empty".to_string(),
            });
        }
    }

    fn check_pattern(&self, rule: &CurtainRule, issues: &mut Vec<ValidationIssue>) {
        let pat = rule.pattern.trim();
        if pat.is_empty() {
            issues.push(ValidationIssue {
                rule_id: rule.id.clone(),
                field: "pattern".to_string(),
                code: "EMPTY_PATTERN".to_string(),
                severity: ValidationSeverity::Error,
                message: "rule pattern must not be empty".to_string(),
            });
            return;
        }

        if pat.len() > self.config.max_pattern_len {
            issues.push(ValidationIssue {
                rule_id: rule.id.clone(),
                field: "pattern".to_string(),
                code: "PATTERN_TOO_LONG".to_string(),
                severity: ValidationSeverity::Error,
                message: format!(
                    "pattern length {} exceeds maximum {}",
                    pat.len(),
                    self.config.max_pattern_len
                ),
            });
            return;
        }

        match rule.pattern_kind {
            PatternKind::Domain => {
                if !is_valid_domain_pattern(pat) {
                    issues.push(ValidationIssue {
                        rule_id: rule.id.clone(),
                        field: "pattern".to_string(),
                        code: "INVALID_DOMAIN_PATTERN".to_string(),
                        severity: ValidationSeverity::Error,
                        message: format!("'{pat}' is not a valid domain pattern"),
                    });
                }
            }
            PatternKind::UrlPrefix => match Url::parse(pat) {
                Ok(parsed) => {
                    if !matches!(parsed.scheme(), "http" | "https" | "ws" | "wss") {
                        issues.push(ValidationIssue {
                            rule_id: rule.id.clone(),
                            field: "pattern".to_string(),
                            code: "UNSUPPORTED_URL_SCHEME".to_string(),
                            severity: ValidationSeverity::Warning,
                            message: format!(
                                "url_prefix uses non-web scheme '{}'",
                                parsed.scheme()
                            ),
                        });
                    }
                }
                Err(err) => {
                    issues.push(ValidationIssue {
                        rule_id: rule.id.clone(),
                        field: "pattern".to_string(),
                        code: "INVALID_URL_PREFIX".to_string(),
                        severity: ValidationSeverity::Error,
                        message: format!("invalid url_prefix '{pat}': {err}"),
                    });
                }
            },
            PatternKind::Glob => {
                if pat == "*" || pat == "**" || pat == "*://*/*" {
                    let is_unscoped_allow = matches!(rule.action, CurtainAction::Allow)
                        && rule.domains.is_empty()
                        && rule.resource_kinds.is_empty();
                    if is_unscoped_allow && self.config.disallow_catch_all_allow {
                        issues.push(ValidationIssue {
                            rule_id: rule.id.clone(),
                            field: "pattern".to_string(),
                            code: "CATCH_ALL_ALLOW".to_string(),
                            severity: ValidationSeverity::Error,
                            message: "unscoped catch-all Allow rule is disallowed by policy"
                                .to_string(),
                        });
                    } else {
                        issues.push(ValidationIssue {
                            rule_id: rule.id.clone(),
                            field: "pattern".to_string(),
                            code: "BROAD_GLOB_PATTERN".to_string(),
                            severity: ValidationSeverity::Warning,
                            message: format!("glob pattern '{pat}' matches all URLs"),
                        });
                    }
                }
            }
            PatternKind::Regex => {
                if !self.config.allow_regex_rules {
                    issues.push(ValidationIssue {
                        rule_id: rule.id.clone(),
                        field: "pattern_kind".to_string(),
                        code: "REGEX_DISALLOWED".to_string(),
                        severity: ValidationSeverity::Error,
                        message: "regex rules are disabled by validator configuration".to_string(),
                    });
                    return;
                }
                if pat.len() > self.config.max_regex_len {
                    issues.push(ValidationIssue {
                        rule_id: rule.id.clone(),
                        field: "pattern".to_string(),
                        code: "REGEX_TOO_LONG".to_string(),
                        severity: ValidationSeverity::Error,
                        message: format!(
                            "regex length {} exceeds maximum {}",
                            pat.len(),
                            self.config.max_regex_len
                        ),
                    });
                    return;
                }
                if let Err(err) = Regex::new(pat) {
                    issues.push(ValidationIssue {
                        rule_id: rule.id.clone(),
                        field: "pattern".to_string(),
                        code: "INVALID_REGEX".to_string(),
                        severity: ValidationSeverity::Error,
                        message: format!("failed to compile regex: {err}"),
                    });
                    return;
                }
                if has_nested_quantifier_heuristic(pat) {
                    issues.push(ValidationIssue {
                        rule_id: rule.id.clone(),
                        field: "pattern".to_string(),
                        code: "COMPLEX_REGEX_QUANTIFIER".to_string(),
                        severity: ValidationSeverity::Warning,
                        message: "regex contains nested quantifiers that may be slow to evaluate"
                            .to_string(),
                    });
                }
                if (pat == ".*" || pat == "^.*$")
                    && matches!(rule.action, CurtainAction::Allow)
                    && rule.domains.is_empty()
                    && rule.resource_kinds.is_empty()
                    && self.config.disallow_catch_all_allow
                {
                    issues.push(ValidationIssue {
                        rule_id: rule.id.clone(),
                        field: "pattern".to_string(),
                        code: "CATCH_ALL_ALLOW".to_string(),
                        severity: ValidationSeverity::Error,
                        message: "unscoped catch-all Allow regex is disallowed by policy"
                            .to_string(),
                    });
                }
            }
        }
    }

    fn check_action(&self, rule: &CurtainRule, issues: &mut Vec<ValidationIssue>) {
        match &rule.action {
            CurtainAction::Redirect { target } => {
                let t = target.trim();
                if t.is_empty() {
                    issues.push(ValidationIssue {
                        rule_id: rule.id.clone(),
                        field: "action.target".to_string(),
                        code: "EMPTY_REDIRECT_TARGET".to_string(),
                        severity: ValidationSeverity::Error,
                        message: "redirect target URL must not be empty".to_string(),
                    });
                    return;
                }
                match Url::parse(t) {
                    Ok(parsed) => {
                        if !matches!(parsed.scheme(), "http" | "https" | "data" | "about") {
                            issues.push(ValidationIssue {
                                rule_id: rule.id.clone(),
                                field: "action.target".to_string(),
                                code: "UNSAFE_REDIRECT_SCHEME".to_string(),
                                severity: ValidationSeverity::Error,
                                message: format!(
                                    "redirect scheme '{}' is not permitted",
                                    parsed.scheme()
                                ),
                            });
                        }
                        if rule.pattern_kind == PatternKind::UrlPrefix
                            && normalize_pattern(&rule.pattern, PatternKind::UrlPrefix)
                                == t.to_ascii_lowercase()
                        {
                            issues.push(ValidationIssue {
                                rule_id: rule.id.clone(),
                                field: "action.target".to_string(),
                                code: "SELF_REDIRECT".to_string(),
                                severity: ValidationSeverity::Error,
                                message: "redirect target matches the rule's own url_prefix"
                                    .to_string(),
                            });
                        }
                    }
                    Err(err) => {
                        issues.push(ValidationIssue {
                            rule_id: rule.id.clone(),
                            field: "action.target".to_string(),
                            code: "INVALID_REDIRECT_URL".to_string(),
                            severity: ValidationSeverity::Error,
                            message: format!("invalid redirect target '{t}': {err}"),
                        });
                    }
                }
            }
            CurtainAction::StripParams { params } => {
                if params.is_empty() {
                    issues.push(ValidationIssue {
                        rule_id: rule.id.clone(),
                        field: "action.params".to_string(),
                        code: "EMPTY_STRIP_PARAMS".to_string(),
                        severity: ValidationSeverity::Error,
                        message: "strip_params action requires at least one parameter name"
                            .to_string(),
                    });
                }
                let mut seen = HashSet::new();
                for param in params {
                    let p = param.trim();
                    if p.is_empty()
                        || p.contains('=')
                        || p.contains('&')
                        || p.chars().any(char::is_whitespace)
                    {
                        issues.push(ValidationIssue {
                            rule_id: rule.id.clone(),
                            field: "action.params".to_string(),
                            code: "INVALID_STRIP_PARAM_NAME".to_string(),
                            severity: ValidationSeverity::Error,
                            message: format!("invalid query parameter name '{param}'"),
                        });
                    } else if !seen.insert(p) {
                        issues.push(ValidationIssue {
                            rule_id: rule.id.clone(),
                            field: "action.params".to_string(),
                            code: "DUPLICATE_STRIP_PARAM".to_string(),
                            severity: ValidationSeverity::Warning,
                            message: format!("duplicate strip parameter '{p}'"),
                        });
                    }
                }
            }
            CurtainAction::Block
            | CurtainAction::Allow
            | CurtainAction::Mask
            | CurtainAction::Warn => {}
        }
    }

    fn check_domain_scopes(&self, rule: &CurtainRule, issues: &mut Vec<ValidationIssue>) {
        let mut included = HashSet::new();
        for domain in &rule.domains {
            let norm = domain.trim().to_ascii_lowercase();
            if !is_valid_domain_pattern(&norm) {
                issues.push(ValidationIssue {
                    rule_id: rule.id.clone(),
                    field: "domains".to_string(),
                    code: "INVALID_SCOPE_DOMAIN".to_string(),
                    severity: ValidationSeverity::Error,
                    message: format!("invalid domain in domains scope: '{domain}'"),
                });
            } else {
                included.insert(norm);
            }
        }

        for domain in &rule.exclude_domains {
            let norm = domain.trim().to_ascii_lowercase();
            if !is_valid_domain_pattern(&norm) {
                issues.push(ValidationIssue {
                    rule_id: rule.id.clone(),
                    field: "exclude_domains".to_string(),
                    code: "INVALID_EXCLUDE_DOMAIN".to_string(),
                    severity: ValidationSeverity::Error,
                    message: format!("invalid domain in exclude_domains scope: '{domain}'"),
                });
            } else if included.contains(&norm) {
                issues.push(ValidationIssue {
                    rule_id: rule.id.clone(),
                    field: "exclude_domains".to_string(),
                    code: "CONFLICTING_DOMAIN_SCOPE".to_string(),
                    severity: ValidationSeverity::Error,
                    message: format!("domain '{norm}' appears in both domains and exclude_domains"),
                });
            }
        }
    }

    fn check_rate_limit(&self, rule: &CurtainRule, issues: &mut Vec<ValidationIssue>) {
        if let Some(0) = rule.max_hits_per_minute {
            issues.push(ValidationIssue {
                rule_id: rule.id.clone(),
                field: "max_hits_per_minute".to_string(),
                code: "ZERO_RATE_LIMIT".to_string(),
                severity: ValidationSeverity::Warning,
                message: "max_hits_per_minute is 0, which prevents the rule from ever firing"
                    .to_string(),
            });
        }
    }

    fn check_rule_shadowing(&self, rules: &[CurtainRule], issues: &mut Vec<ValidationIssue>) {
        let mut enabled: Vec<&CurtainRule> = rules.iter().filter(|r| r.enabled).collect();
        enabled.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.id.cmp(&b.id)));

        for i in 0..enabled.len() {
            let high = enabled[i];
            if !matches!(high.action, CurtainAction::Block | CurtainAction::Allow) {
                continue;
            }
            let high_norm = normalize_pattern(&high.pattern, high.pattern_kind);

            for low in enabled.iter().skip(i + 1) {
                if high.priority <= low.priority {
                    continue;
                }
                if high.pattern_kind != low.pattern_kind {
                    continue;
                }
                let low_norm = normalize_pattern(&low.pattern, low.pattern_kind);
                if high_norm != low_norm {
                    continue;
                }
                if scopes_shadow(high, low) {
                    issues.push(ValidationIssue {
                        rule_id: low.id.clone(),
                        field: "priority".to_string(),
                        code: "SHADOWED_RULE".to_string(),
                        severity: ValidationSeverity::Warning,
                        message: format!(
                            "rule '{}' (priority {}) is shadowed by higher-priority rule '{}' (priority {})",
                            low.id, low.priority, high.id, high.priority
                        ),
                    });
                }
            }
        }
    }
}

/// Normalizes a rule pattern for comparison and deduplication.
pub fn normalize_pattern(pattern: &str, kind: PatternKind) -> String {
    let trimmed = pattern.trim();
    match kind {
        PatternKind::Domain => trimmed.trim_start_matches("*.").to_ascii_lowercase(),
        PatternKind::UrlPrefix => trimmed.to_ascii_lowercase(),
        PatternKind::Glob | PatternKind::Regex => trimmed.to_string(),
    }
}

/// Returns `true` if `candidate` is a syntactically valid domain or `*.domain` pattern.
pub fn is_valid_domain_pattern(candidate: &str) -> bool {
    let s = candidate.trim();
    if s.is_empty() || s.len() > 253 {
        return false;
    }
    let host = s.strip_prefix("*.").unwrap_or(s);
    if host.is_empty() || host.starts_with('.') || host.ends_with('.') || host.contains("..") {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    })
}

fn has_nested_quantifier_heuristic(regex: &str) -> bool {
    let bytes = regex.as_bytes();
    let mut i = 0;
    let mut group_had_quantifier = Vec::new();
    let mut current_has_quantifier = false;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\\' {
            i += 2;
            continue;
        }
        match b {
            b'(' => {
                group_had_quantifier.push(current_has_quantifier);
                current_has_quantifier = false;
            }
            b')' => {
                let inner_had = current_has_quantifier;
                current_has_quantifier = group_had_quantifier.pop().unwrap_or(false);
                if inner_had
                    && i + 1 < bytes.len()
                    && matches!(bytes[i + 1], b'*' | b'+' | b'?' | b'{')
                {
                    return true;
                }
            }
            b'*' | b'+' => {
                current_has_quantifier = true;
            }
            _ => {}
        }
        i += 1;
    }
    false
}

fn scopes_shadow(high: &CurtainRule, low: &CurtainRule) -> bool {
    let domains_cover = high.domains.is_empty()
        || (!low.domains.is_empty()
            && low.domains.iter().all(|d| {
                let norm = d.trim().to_ascii_lowercase();
                high.domains
                    .iter()
                    .any(|hd| hd.trim().to_ascii_lowercase() == norm)
            }));

    let resources_cover = high.resource_kinds.is_empty()
        || (!low.resource_kinds.is_empty()
            && low
                .resource_kinds
                .iter()
                .all(|rk| high.resource_kinds.contains(rk)));

    let party_covers = !high.third_party_only || low.third_party_only;

    domains_cover && resources_cover && party_covers && high.exclude_domains.is_empty()
}

fn compute_ruleset_digest(rules: &[CurtainRule]) -> String {
    let mut ctx = Context::new(&SHA256);
    for rule in rules {
        ctx.update(rule.id.as_bytes());
        ctx.update(b"\0");
        ctx.update(rule.pattern.as_bytes());
        ctx.update(b"\0");
        ctx.update(&rule.priority.to_le_bytes());
        ctx.update(if rule.enabled { b"\x01" } else { b"\x00" });
    }
    let digest = ctx.finish();
    hex_encode(digest.as_ref())
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

impl fmt::Display for ValidationSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationSeverity::Info => write!(f, "info"),
            ValidationSeverity::Warning => write!(f, "warning"),
            ValidationSeverity::Error => write!(f, "error"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_block_rule(id: &str, pattern: &str, priority: i32) -> CurtainRule {
        CurtainRule {
            id: id.to_string(),
            name: format!("Rule {id}"),
            enabled: true,
            priority,
            pattern: pattern.to_string(),
            pattern_kind: PatternKind::Domain,
            action: CurtainAction::Block,
            resource_kinds: vec![ResourceKind::Script],
            domains: vec![],
            exclude_domains: vec![],
            third_party_only: false,
            max_hits_per_minute: None,
            notes: None,
        }
    }

    #[test]
    fn valid_ruleset_json_parses_and_passes() {
        let json = r#"[
            {
                "id": "block-tracker-1",
                "name": "Block Example Tracker",
                "enabled": true,
                "priority": 100,
                "pattern": "*.tracker.example.com",
                "pattern_kind": "domain",
                "action": { "type": "block" },
                "resource_kinds": ["script", "xhr"],
                "third_party_only": true
            },
            {
                "id": "strip-utm",
                "name": "Strip UTM params",
                "priority": 50,
                "pattern": "https://news.example.org/",
                "pattern_kind": "url_prefix",
                "action": { "type": "strip_params", "params": ["utm_source", "utm_medium"] }
            }
        ]"#;

        let validator = CurtainRuleValidator::default();
        let (rules, report) = validator
            .parse_and_validate_json(json)
            .expect("valid JSON should parse");

        assert_eq!(rules.len(), 2);
        assert!(
            report.valid,
            "expected valid report, got: {:?}",
            report.issues
        );
        assert_eq!(report.error_count, 0);
        assert_eq!(report.warning_count, 0);
        assert_eq!(report.enabled_rules, 2);
        assert_eq!(report.ruleset_sha256.len(), 64);
    }

    #[test]
    fn rejects_invalid_patterns_redirects_and_domain_conflicts() {
        let validator = CurtainRuleValidator::default();
        let bad_rules = vec![
            CurtainRule {
                id: "bad id!".to_string(),
                name: "".to_string(),
                enabled: true,
                priority: 10,
                pattern: "not_a_valid..domain".to_string(),
                pattern_kind: PatternKind::Domain,
                action: CurtainAction::Redirect {
                    target: "javascript:alert(1)".to_string(),
                },
                resource_kinds: vec![],
                domains: vec!["example.com".to_string()],
                exclude_domains: vec!["example.com".to_string()],
                third_party_only: false,
                max_hits_per_minute: Some(0),
                notes: None,
            },
            CurtainRule {
                id: "bad-regex".to_string(),
                name: "Broken Regex".to_string(),
                enabled: true,
                priority: 10,
                pattern: "([unclosed".to_string(),
                pattern_kind: PatternKind::Regex,
                action: CurtainAction::StripParams { params: vec![] },
                resource_kinds: vec![],
                domains: vec![],
                exclude_domains: vec![],
                third_party_only: false,
                max_hits_per_minute: None,
                notes: None,
            },
        ];

        let report = validator.validate_ruleset(&bad_rules);
        assert!(!report.valid);
        let codes: HashSet<&str> = report.issues.iter().map(|i| i.code.as_str()).collect();
        assert!(codes.contains("INVALID_ID_CHARS"));
        assert!(codes.contains("EMPTY_RULE_NAME"));
        assert!(codes.contains("INVALID_DOMAIN_PATTERN"));
        assert!(codes.contains("UNSAFE_REDIRECT_SCHEME"));
        assert!(codes.contains("CONFLICTING_DOMAIN_SCOPE"));
        assert!(codes.contains("ZERO_RATE_LIMIT"));
        assert!(codes.contains("INVALID_REGEX"));
        assert!(codes.contains("EMPTY_STRIP_PARAMS"));
    }

    #[test]
    fn detects_duplicate_ids_catch_all_allow_and_shadowing() {
        let validator = CurtainRuleValidator::default();
        let high = CurtainRule {
            resource_kinds: vec![],
            ..sample_block_rule("rule-high", "ads.example.net", 200)
        };
        let low = sample_block_rule("rule-low", "ads.example.net", 50);
        let dup = sample_block_rule("rule-low", "other.example.net", 10);
        let catch_all_allow = CurtainRule {
            id: "allow-all".to_string(),
            name: "Catch all".to_string(),
            enabled: true,
            priority: 1,
            pattern: "*".to_string(),
            pattern_kind: PatternKind::Glob,
            action: CurtainAction::Allow,
            resource_kinds: vec![],
            domains: vec![],
            exclude_domains: vec![],
            third_party_only: false,
            max_hits_per_minute: None,
            notes: None,
        };

        let report = validator.validate_ruleset(&[high, low, dup, catch_all_allow]);
        assert!(!report.valid);
        let codes: HashSet<&str> = report.issues.iter().map(|i| i.code.as_str()).collect();
        assert!(codes.contains("SHADOWED_RULE"));
        assert!(codes.contains("DUPLICATE_RULE_ID"));
        assert!(codes.contains("CATCH_ALL_ALLOW"));
    }

    #[test]
    fn detects_nested_quantifier_regex_warning() {
        let validator = CurtainRuleValidator::default();
        let rule = CurtainRule {
            id: "nested-q".to_string(),
            name: "Nested Quantifier".to_string(),
            enabled: true,
            priority: 5,
            pattern: r"^([a-z]+)+$".to_string(),
            pattern_kind: PatternKind::Regex,
            action: CurtainAction::Block,
            resource_kinds: vec![],
            domains: vec![],
            exclude_domains: vec![],
            third_party_only: false,
            max_hits_per_minute: None,
            notes: None,
        };

        let report = validator.validate_ruleset(&[rule]);
        assert!(report.valid);
        assert_eq!(report.warning_count, 1);
        assert_eq!(report.issues[0].code, "COMPLEX_REGEX_QUANTIFIER");
    }
}
