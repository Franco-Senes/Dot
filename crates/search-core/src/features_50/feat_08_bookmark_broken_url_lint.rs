//! Bookmark broken URL linter for Search Linux.
//!
//! Performs offline structural validation of bookmark URLs (malformed syntax,
//! missing hosts, blocked/unsupported schemes, embedded credentials, excessive
//! length, local/loopback hosts, and insecure HTTP) and optionally correlates
//! external HTTP/network probe results (4xx/5xx, DNS/TLS/timeout errors, and
//! permanent redirects) to produce a structured `LintReport` with suggested
//! fixes.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use thiserror::Error;
use time::OffsetDateTime;
use url::{Host, Url};

/// Errors produced by the bookmark broken-URL linter.
#[derive(Debug, Error)]
pub enum LintError {
    #[error("invalid lint configuration: {0}")]
    InvalidConfig(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// A bookmark node or folder in a hierarchical bookmark tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookmarkEntry {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<BookmarkEntry>>,
}

impl BookmarkEntry {
    /// Creates a leaf bookmark entry with a title and URL.
    pub fn bookmark(
        id: impl Into<String>,
        title: impl Into<String>,
        url: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            url: Some(url.into()),
            children: None,
        }
    }

    /// Creates a folder entry containing child bookmark entries.
    pub fn folder(
        id: impl Into<String>,
        title: impl Into<String>,
        children: Vec<BookmarkEntry>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            url: None,
            children: Some(children),
        }
    }

    /// Returns `true` if this entry represents a folder.
    pub fn is_folder(&self) -> bool {
        self.children.is_some() && self.url.is_none()
    }
}

/// Severity level of a detected bookmark URL issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintSeverity {
    Info,
    Warning,
    Error,
}

/// Specific reason a bookmark URL was flagged by the linter.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "reason_kind", rename_all = "snake_case")]
pub enum BrokenUrlReason {
    EmptyUrl,
    LeadingOrTrailingWhitespace,
    MalformedUrl {
        detail: String,
    },
    BlockedScheme {
        scheme: String,
    },
    UnsupportedScheme {
        scheme: String,
    },
    MissingHost,
    InvalidPort,
    ExcessiveLength {
        length: usize,
        max: usize,
    },
    EmbeddedCredentials,
    LocalOrLoopbackHost {
        host: String,
    },
    InsecureHttpScheme,
    HttpErrorStatus {
        status: u16,
    },
    PermanentRedirect {
        status: u16,
        target: String,
    },
    NetworkFailure {
        kind: NetworkErrorKind,
        detail: String,
    },
    RedirectLoop,
}

/// Classification of network-level probe failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkErrorKind {
    DnsResolution,
    ConnectionRefused,
    Timeout,
    TlsHandshake,
    Other,
}

/// Outcome of an optional external URL reachability probe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status_kind", rename_all = "snake_case")]
pub enum ProbeOutcome {
    Ok {
        status: u16,
    },
    Redirect {
        status: u16,
        target: String,
    },
    HttpError {
        status: u16,
    },
    NetworkError {
        kind: NetworkErrorKind,
        detail: String,
    },
    RedirectLoop,
}

/// Result of probing a single URL externally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UrlProbeResult {
    pub url: String,
    pub outcome: ProbeOutcome,
    pub checked_at_unix: i64,
}

/// Configuration controlling which URL checks are performed and their thresholds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintConfig {
    pub allowed_schemes: Vec<String>,
    pub blocked_schemes: Vec<String>,
    pub warn_insecure_http: bool,
    pub warn_local_hosts: bool,
    pub warn_embedded_credentials: bool,
    pub flag_empty_leaf_urls: bool,
    pub flag_permanent_redirects: bool,
    pub max_url_length: usize,
}

impl Default for LintConfig {
    fn default() -> Self {
        Self {
            allowed_schemes: vec![
                "http".to_string(),
                "https".to_string(),
                "file".to_string(),
                "ftp".to_string(),
                "about".to_string(),
                "search".to_string(),
            ],
            blocked_schemes: vec![
                "javascript".to_string(),
                "vbscript".to_string(),
                "data".to_string(),
            ],
            warn_insecure_http: true,
            warn_local_hosts: true,
            warn_embedded_credentials: true,
            flag_empty_leaf_urls: true,
            flag_permanent_redirects: true,
            max_url_length: 4096,
        }
    }
}

impl LintConfig {
    /// Validates configuration invariants.
    pub fn validate(&self) -> Result<(), LintError> {
        if self.max_url_length == 0 {
            return Err(LintError::InvalidConfig(
                "max_url_length must be greater than zero".to_string(),
            ));
        }
        for scheme in self
            .allowed_schemes
            .iter()
            .chain(self.blocked_schemes.iter())
        {
            if !is_valid_scheme_name(scheme) {
                return Err(LintError::InvalidConfig(format!(
                    "invalid scheme name: '{scheme}'"
                )));
            }
        }
        Ok(())
    }
}

/// A single issue reported for a bookmark entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintIssue {
    pub bookmark_id: String,
    pub title: String,
    pub folder_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_url: Option<String>,
    pub severity: LintSeverity,
    pub reason: BrokenUrlReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_fix: Option<String>,
}

/// Aggregate statistics for a lint run.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LintSummary {
    pub total_nodes_scanned: usize,
    pub total_bookmarks: usize,
    pub total_folders: usize,
    pub issues_found: usize,
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub healthy_bookmarks: usize,
}

/// Full report produced by linting a bookmark tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintReport {
    pub generated_at_unix: i64,
    pub summary: LintSummary,
    pub issues: Vec<LintIssue>,
}

/// Lints a bookmark tree against `config` and optional `probes`.
pub fn lint_bookmark_tree(
    roots: &[BookmarkEntry],
    config: &LintConfig,
    probes: &[UrlProbeResult],
) -> Result<LintReport, LintError> {
    config.validate()?;

    let mut probe_map: HashMap<&str, &UrlProbeResult> = HashMap::with_capacity(probes.len());
    for probe in probes {
        probe_map.insert(probe.url.as_str(), probe);
    }

    let mut summary = LintSummary::default();
    let mut issues = Vec::new();
    let mut unhealthy_bookmark_ids: HashSet<String> = HashSet::new();

    for root in roots {
        visit_node(
            root,
            "",
            config,
            &probe_map,
            &mut summary,
            &mut issues,
            &mut unhealthy_bookmark_ids,
        );
    }

    summary.issues_found = issues.len();
    for issue in &issues {
        match issue.severity {
            LintSeverity::Error => summary.errors += 1,
            LintSeverity::Warning => summary.warnings += 1,
            LintSeverity::Info => summary.infos += 1,
        }
    }
    summary.healthy_bookmarks = summary
        .total_bookmarks
        .saturating_sub(unhealthy_bookmark_ids.len());

    Ok(LintReport {
        generated_at_unix: OffsetDateTime::now_utc().unix_timestamp(),
        summary,
        issues,
    })
}

fn visit_node(
    node: &BookmarkEntry,
    parent_path: &str,
    config: &LintConfig,
    probe_map: &HashMap<&str, &UrlProbeResult>,
    summary: &mut LintSummary,
    issues: &mut Vec<LintIssue>,
    unhealthy_ids: &mut HashSet<String>,
) {
    summary.total_nodes_scanned += 1;

    if node.is_folder() {
        summary.total_folders += 1;
        let current_path = if parent_path.is_empty() {
            node.title.clone()
        } else {
            format!("{parent_path}/{}", node.title)
        };
        if let Some(children) = &node.children {
            for child in children {
                visit_node(
                    child,
                    &current_path,
                    config,
                    probe_map,
                    summary,
                    issues,
                    unhealthy_ids,
                );
            }
        }
        return;
    }

    summary.total_bookmarks += 1;
    let folder_path = if parent_path.is_empty() {
        "/".to_string()
    } else {
        parent_path.to_string()
    };

    let mut node_has_issue = false;

    match node.url.as_deref() {
        None => {
            if config.flag_empty_leaf_urls {
                node_has_issue = true;
                issues.push(LintIssue {
                    bookmark_id: node.id.clone(),
                    title: node.title.clone(),
                    folder_path: folder_path.clone(),
                    raw_url: None,
                    severity: LintSeverity::Error,
                    reason: BrokenUrlReason::EmptyUrl,
                    suggested_fix: None,
                });
            }
        }
        Some(raw_url) => {
            let findings = lint_single_url(raw_url, config);
            for (severity, reason, suggested_fix) in findings {
                node_has_issue = true;
                issues.push(LintIssue {
                    bookmark_id: node.id.clone(),
                    title: node.title.clone(),
                    folder_path: folder_path.clone(),
                    raw_url: Some(raw_url.to_string()),
                    severity,
                    reason,
                    suggested_fix,
                });
            }

            let trimmed = raw_url.trim();
            if let Some(probe) = probe_map.get(raw_url).or_else(|| probe_map.get(trimmed)) {
                if let Some((severity, reason, suggested_fix)) = evaluate_probe(probe, config) {
                    node_has_issue = true;
                    issues.push(LintIssue {
                        bookmark_id: node.id.clone(),
                        title: node.title.clone(),
                        folder_path: folder_path.clone(),
                        raw_url: Some(raw_url.to_string()),
                        severity,
                        reason,
                        suggested_fix,
                    });
                }
            }
        }
    }

    if node_has_issue {
        unhealthy_ids.insert(node.id.clone());
    }

    // Also traverse children if a malformed node happens to have both url and children.
    if let Some(children) = &node.children {
        let current_path = if parent_path.is_empty() {
            node.title.clone()
        } else {
            format!("{parent_path}/{}", node.title)
        };
        for child in children {
            visit_node(
                child,
                &current_path,
                config,
                probe_map,
                summary,
                issues,
                unhealthy_ids,
            );
        }
    }
}

/// Validates a single URL string against `config` without performing network I/O.
pub fn lint_single_url(
    raw_url: &str,
    config: &LintConfig,
) -> Vec<(LintSeverity, BrokenUrlReason, Option<String>)> {
    let mut findings = Vec::new();
    let trimmed = raw_url.trim();

    if trimmed.is_empty() {
        if config.flag_empty_leaf_urls {
            findings.push((LintSeverity::Error, BrokenUrlReason::EmptyUrl, None));
        }
        return findings;
    }

    if trimmed.len() != raw_url.len() {
        findings.push((
            LintSeverity::Warning,
            BrokenUrlReason::LeadingOrTrailingWhitespace,
            Some(trimmed.to_string()),
        ));
    }

    if trimmed.len() > config.max_url_length {
        findings.push((
            LintSeverity::Error,
            BrokenUrlReason::ExcessiveLength {
                length: trimmed.len(),
                max: config.max_url_length,
            },
            None,
        ));
    }

    let parsed = match Url::parse(trimmed) {
        Ok(url) => url,
        Err(err) => {
            findings.push((
                LintSeverity::Error,
                BrokenUrlReason::MalformedUrl {
                    detail: err.to_string(),
                },
                suggest_malformed_fix(trimmed),
            ));
            return findings;
        }
    };

    let scheme = parsed.scheme().to_ascii_lowercase();
    if config
        .blocked_schemes
        .iter()
        .any(|s| s.eq_ignore_ascii_case(&scheme))
    {
        findings.push((
            LintSeverity::Error,
            BrokenUrlReason::BlockedScheme {
                scheme: scheme.clone(),
            },
            None,
        ));
        return findings;
    }

    if !config.allowed_schemes.is_empty()
        && !config
            .allowed_schemes
            .iter()
            .any(|s| s.eq_ignore_ascii_case(&scheme))
    {
        findings.push((
            LintSeverity::Warning,
            BrokenUrlReason::UnsupportedScheme {
                scheme: scheme.clone(),
            },
            None,
        ));
    }

    let requires_host = matches!(scheme.as_str(), "http" | "https" | "ftp" | "ws" | "wss");
    if requires_host {
        match parsed.host() {
            None => {
                findings.push((LintSeverity::Error, BrokenUrlReason::MissingHost, None));
            }
            Some(host) => {
                if config.warn_local_hosts && is_local_or_loopback(&host) {
                    findings.push((
                        LintSeverity::Info,
                        BrokenUrlReason::LocalOrLoopbackHost {
                            host: host.to_string(),
                        },
                        None,
                    ));
                }
            }
        }
    }

    if parsed.port() == Some(0) {
        findings.push((LintSeverity::Error, BrokenUrlReason::InvalidPort, None));
    }

    if config.warn_embedded_credentials
        && (!parsed.username().is_empty() || parsed.password().is_some())
    {
        let mut sanitized = parsed.clone();
        let _ = sanitized.set_username("");
        let _ = sanitized.set_password(None);
        findings.push((
            LintSeverity::Warning,
            BrokenUrlReason::EmbeddedCredentials,
            Some(sanitized.to_string()),
        ));
    }

    if config.warn_insecure_http && scheme == "http" {
        let mut upgraded = parsed;
        if upgraded.set_scheme("https").is_ok() {
            findings.push((
                LintSeverity::Warning,
                BrokenUrlReason::InsecureHttpScheme,
                Some(upgraded.to_string()),
            ));
        } else {
            findings.push((
                LintSeverity::Warning,
                BrokenUrlReason::InsecureHttpScheme,
                None,
            ));
        }
    }

    findings
}

fn evaluate_probe(
    probe: &UrlProbeResult,
    config: &LintConfig,
) -> Option<(LintSeverity, BrokenUrlReason, Option<String>)> {
    match &probe.outcome {
        ProbeOutcome::Ok { .. } => None,
        ProbeOutcome::HttpError { status } => Some((
            LintSeverity::Error,
            BrokenUrlReason::HttpErrorStatus { status: *status },
            None,
        )),
        ProbeOutcome::NetworkError { kind, detail } => Some((
            LintSeverity::Error,
            BrokenUrlReason::NetworkFailure {
                kind: *kind,
                detail: detail.clone(),
            },
            None,
        )),
        ProbeOutcome::RedirectLoop => {
            Some((LintSeverity::Error, BrokenUrlReason::RedirectLoop, None))
        }
        ProbeOutcome::Redirect { status, target } => {
            if config.flag_permanent_redirects && (*status == 301 || *status == 308) {
                Some((
                    LintSeverity::Warning,
                    BrokenUrlReason::PermanentRedirect {
                        status: *status,
                        target: target.clone(),
                    },
                    Some(target.clone()),
                ))
            } else {
                None
            }
        }
    }
}

fn suggest_malformed_fix(trimmed: &str) -> Option<String> {
    if !trimmed.contains("://") && trimmed.contains('.') && !trimmed.contains(' ') {
        let candidate = format!("https://{trimmed}");
        if Url::parse(&candidate).is_ok() {
            return Some(candidate);
        }
    }
    None
}

fn is_local_or_loopback(host: &Host<&str>) -> bool {
    match host {
        Host::Domain(domain) => {
            let lower = domain.to_ascii_lowercase();
            lower == "localhost"
                || lower.ends_with(".localhost")
                || lower.ends_with(".local")
                || lower == "127.0.0.1"
        }
        Host::Ipv4(ip) => ip.is_loopback() || ip.is_unspecified() || ip.is_private(),
        Host::Ipv6(ip) => ip.is_loopback() || ip.is_unspecified(),
    }
}

fn is_valid_scheme_name(scheme: &str) -> bool {
    let mut chars = scheme.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Applies safe suggested fixes from `report` (whitespace trimming and permanent
/// redirect targets) onto `roots`, returning the number of updated bookmarks.
pub fn apply_suggested_fixes(roots: &mut [BookmarkEntry], report: &LintReport) -> usize {
    let mut fix_by_id: HashMap<&str, &str> = HashMap::new();
    for issue in &report.issues {
        if let Some(fix) = issue.suggested_fix.as_deref() {
            let is_safe_autofix = matches!(
                issue.reason,
                BrokenUrlReason::LeadingOrTrailingWhitespace
                    | BrokenUrlReason::PermanentRedirect { .. }
            );
            if is_safe_autofix {
                fix_by_id.entry(issue.bookmark_id.as_str()).or_insert(fix);
            }
        }
    }

    let mut updated = 0;
    for root in roots {
        updated += apply_fixes_recursive(root, &fix_by_id);
    }
    updated
}

fn apply_fixes_recursive(node: &mut BookmarkEntry, fix_by_id: &HashMap<&str, &str>) -> usize {
    let mut updated = 0;
    if let Some(new_url) = fix_by_id.get(node.id.as_str()) {
        if node.url.as_deref() != Some(*new_url) {
            node.url = Some((*new_url).to_string());
            updated += 1;
        }
    }
    if let Some(children) = &mut node.children {
        for child in children {
            updated += apply_fixes_recursive(child, fix_by_id);
        }
    }
    updated
}

/// Serializes a `LintReport` into pretty-printed JSON.
pub fn export_report_json(report: &LintReport) -> Result<String, LintError> {
    Ok(serde_json::to_string_pretty(report)?)
}

/// Deserializes a slice of `BookmarkEntry` nodes from JSON.
pub fn import_bookmarks_json(json: &str) -> Result<Vec<BookmarkEntry>, LintError> {
    if json.trim().is_empty() {
        return Err(LintError::InvalidInput(
            "bookmark JSON input is empty".to_string(),
        ));
    }
    Ok(serde_json::from_str(json)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_healthy_and_local_bookmarks() {
        let tree = vec![BookmarkEntry::folder(
            "f1",
            "Dev",
            vec![
                BookmarkEntry::bookmark("b1", "Rust", "https://www.rust-lang.org/"),
                BookmarkEntry::bookmark("b2", "Docs", "https://docs.rs/url/latest/url/"),
            ],
        )];

        let config = LintConfig::default();
        let probes = vec![UrlProbeResult {
            url: "https://www.rust-lang.org/".to_string(),
            outcome: ProbeOutcome::Ok { status: 200 },
            checked_at_unix: 1_700_000_000,
        }];

        let report = lint_bookmark_tree(&tree, &config, &probes).expect("lint should succeed");
        assert_eq!(report.summary.total_folders, 1);
        assert_eq!(report.summary.total_bookmarks, 2);
        assert_eq!(report.summary.issues_found, 0);
        assert_eq!(report.summary.healthy_bookmarks, 2);
    }

    #[test]
    fn detects_malformed_blocked_empty_and_insecure_urls() {
        let tree = vec![BookmarkEntry::folder(
            "root",
            "Bookmarks Bar",
            vec![
                BookmarkEntry::bookmark("b1", "Empty", "   "),
                BookmarkEntry::bookmark("b2", "XSS", "javascript:alert(1)"),
                BookmarkEntry::bookmark("b3", "Missing Scheme", "example.com/docs"),
                BookmarkEntry::bookmark("b4", "Plain HTTP", "http://example.org/blog"),
                BookmarkEntry::bookmark("b5", "Creds", "https://alice:secret@example.net/"),
                BookmarkEntry::bookmark("b6", "Local", "http://127.0.0.1:8080/admin"),
            ],
        )];

        let report =
            lint_bookmark_tree(&tree, &LintConfig::default(), &[]).expect("lint should succeed");

        assert_eq!(report.summary.total_bookmarks, 6);
        assert_eq!(report.summary.healthy_bookmarks, 0);
        assert!(report.summary.errors >= 3);
        assert!(report.summary.warnings >= 2);
        assert!(report.summary.infos >= 1);

        let missing_scheme = report
            .issues
            .iter()
            .find(|i| i.bookmark_id == "b3")
            .expect("b3 issue");
        assert_eq!(
            missing_scheme.suggested_fix.as_deref(),
            Some("https://example.com/docs")
        );

        let creds = report
            .issues
            .iter()
            .find(|i| {
                i.bookmark_id == "b5" && matches!(i.reason, BrokenUrlReason::EmbeddedCredentials)
            })
            .expect("b5 creds issue");
        assert_eq!(creds.suggested_fix.as_deref(), Some("https://example.net/"));
    }

    #[test]
    fn probe_correlation_and_safe_autofix_application() {
        let mut tree = vec![
            BookmarkEntry::bookmark("b1", "Moved", "https://old.example.com/page"),
            BookmarkEntry::bookmark("b2", "Padded", "  https://valid.example.com/  "),
            BookmarkEntry::bookmark("b3", "Dead", "https://gone.example.com/404"),
            BookmarkEntry::bookmark("b4", "Timeout", "https://slow.example.com/"),
        ];

        let probes = vec![
            UrlProbeResult {
                url: "https://old.example.com/page".to_string(),
                outcome: ProbeOutcome::Redirect {
                    status: 301,
                    target: "https://new.example.com/page".to_string(),
                },
                checked_at_unix: 1_700_000_100,
            },
            UrlProbeResult {
                url: "https://gone.example.com/404".to_string(),
                outcome: ProbeOutcome::HttpError { status: 404 },
                checked_at_unix: 1_700_000_101,
            },
            UrlProbeResult {
                url: "https://slow.example.com/".to_string(),
                outcome: ProbeOutcome::NetworkError {
                    kind: NetworkErrorKind::Timeout,
                    detail: "timed out after 10s".to_string(),
                },
                checked_at_unix: 1_700_000_102,
            },
        ];

        let report =
            lint_bookmark_tree(&tree, &LintConfig::default(), &probes).expect("lint succeeds");
        assert_eq!(report.summary.issues_found, 4);
        assert_eq!(report.summary.errors, 2);
        assert_eq!(report.summary.warnings, 2);

        let updated = apply_suggested_fixes(&mut tree, &report);
        assert_eq!(updated, 2);
        assert_eq!(tree[0].url.as_deref(), Some("https://new.example.com/page"));
        assert_eq!(tree[1].url.as_deref(), Some("https://valid.example.com/"));
        assert_eq!(tree[2].url.as_deref(), Some("https://gone.example.com/404"));
    }

    #[test]
    fn config_validation_and_json_roundtrip() {
        let bad_config = LintConfig {
            max_url_length: 0,
            ..LintConfig::default()
        };
        assert!(matches!(
            lint_bookmark_tree(&[], &bad_config, &[]),
            Err(LintError::InvalidConfig(_))
        ));

        let bad_scheme_config = LintConfig {
            allowed_schemes: vec!["123bad".to_string()],
            ..LintConfig::default()
        };
        assert!(matches!(
            bad_scheme_config.validate(),
            Err(LintError::InvalidConfig(_))
        ));

        let json_input = r#"[{"id":"1","title":"Test","url":"https://example.com"}]"#;
        let parsed = import_bookmarks_json(json_input).expect("valid json");
        let report =
            lint_bookmark_tree(&parsed, &LintConfig::default(), &[]).expect("lint succeeds");
        let report_json = export_report_json(&report).expect("export succeeds");
        let decoded: LintReport = serde_json::from_str(&report_json).expect("decode succeeds");
        assert_eq!(decoded.summary.healthy_bookmarks, 1);
    }
}
