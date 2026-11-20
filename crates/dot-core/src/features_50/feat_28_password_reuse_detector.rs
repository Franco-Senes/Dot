use std::collections::{BTreeMap, HashMap, HashSet};

use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use url::Url;
use zeroize::Zeroize;

const HMAC_KEY_LEN: usize = 32;
const GROUP_ID_BYTES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ReuseSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ReuseMatchKind {
    SuffixVariant,
    CaseInsensitive,
    Exact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReusePolicy {
    pub detect_case_variants: bool,
    pub detect_suffix_variants: bool,
    pub ignore_same_site_reuse: bool,
    pub min_password_len_for_variant: usize,
    pub max_entries: usize,
}

impl Default for ReusePolicy {
    fn default() -> Self {
        Self {
            detect_case_variants: true,
            detect_suffix_variants: true,
            ignore_same_site_reuse: false,
            min_password_len_for_variant: 6,
            max_entries: 10_000,
        }
    }
}

impl ReusePolicy {
    pub fn validate(&self) -> Result<(), ReuseDetectorError> {
        if self.min_password_len_for_variant == 0 {
            return Err(ReuseDetectorError::InvalidConfig(
                "min_password_len_for_variant must be at least 1".to_string(),
            ));
        }
        if self.max_entries == 0 {
            return Err(ReuseDetectorError::InvalidConfig(
                "max_entries must be at least 1".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct CredentialInput {
    id: String,
    origin: String,
    username: String,
    password: String,
    updated_at_unix: Option<i64>,
}

impl CredentialInput {
    pub fn new(
        id: impl Into<String>,
        origin: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        updated_at_unix: Option<i64>,
    ) -> Self {
        Self {
            id: id.into(),
            origin: origin.into(),
            username: username.into(),
            password: password.into(),
            updated_at_unix,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn updated_at_unix(&self) -> Option<i64> {
        self.updated_at_unix
    }
}

impl Drop for CredentialInput {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReusedCredentialRef {
    pub id: String,
    pub origin: String,
    pub site_key: String,
    pub username: String,
    pub updated_at_unix: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReuseGroup {
    pub group_id: String,
    pub match_kind: ReuseMatchKind,
    pub severity: ReuseSeverity,
    pub distinct_origins: usize,
    pub distinct_sites: usize,
    pub entries: Vec<ReusedCredentialRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReuseAuditReport {
    pub total_credentials_scanned: usize,
    pub unique_exact_passwords: usize,
    pub reused_credential_count: usize,
    pub critical_groups: usize,
    pub high_groups: usize,
    pub medium_groups: usize,
    pub low_groups: usize,
    pub groups: Vec<ReuseGroup>,
    pub generated_at_unix: i64,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReuseDetectorError {
    #[error("credential id must not be empty")]
    EmptyCredentialId,
    #[error("duplicate credential id: {0}")]
    DuplicateCredentialId(String),
    #[error("invalid origin for credential '{id}' ('{origin}'): {reason}")]
    InvalidOrigin {
        id: String,
        origin: String,
        reason: String,
    },
    #[error("password must not be empty for credential '{id}'")]
    EmptyPassword { id: String },
    #[error("invalid HMAC key length: expected {expected} bytes, got {actual}")]
    InvalidKeyLength { expected: usize, actual: usize },
    #[error("credential count exceeds configured limit of {max}")]
    CapacityExceeded { max: usize },
    #[error("invalid detector configuration: {0}")]
    InvalidConfig(String),
    #[error("failed to generate random key")]
    KeyGenerationFailed,
}

pub struct PasswordReuseDetector {
    hmac_key: hmac::Key,
    policy: ReusePolicy,
}

impl PasswordReuseDetector {
    pub fn new(
        hmac_key_bytes: [u8; HMAC_KEY_LEN],
        policy: ReusePolicy,
    ) -> Result<Self, ReuseDetectorError> {
        policy.validate()?;
        let hmac_key = hmac::Key::new(hmac::HMAC_SHA256, &hmac_key_bytes);
        Ok(Self { hmac_key, policy })
    }

    pub fn from_key_slice(key: &[u8], policy: ReusePolicy) -> Result<Self, ReuseDetectorError> {
        if key.len() != HMAC_KEY_LEN {
            return Err(ReuseDetectorError::InvalidKeyLength {
                expected: HMAC_KEY_LEN,
                actual: key.len(),
            });
        }
        let mut buf = [0u8; HMAC_KEY_LEN];
        buf.copy_from_slice(key);
        let detector = Self::new(buf, policy);
        buf.zeroize();
        detector
    }

    pub fn with_ephemeral_key(policy: ReusePolicy) -> Result<Self, ReuseDetectorError> {
        policy.validate()?;
        let rng = SystemRandom::new();
        let mut key_bytes = [0u8; HMAC_KEY_LEN];
        if rng.fill(&mut key_bytes).is_err() {
            key_bytes.zeroize();
            return Err(ReuseDetectorError::KeyGenerationFailed);
        }
        let detector = Self::new(key_bytes, policy);
        key_bytes.zeroize();
        detector
    }

    pub fn policy(&self) -> &ReusePolicy {
        &self.policy
    }

    pub fn analyze(
        &self,
        credentials: &[CredentialInput],
    ) -> Result<ReuseAuditReport, ReuseDetectorError> {
        if credentials.len() > self.policy.max_entries {
            return Err(ReuseDetectorError::CapacityExceeded {
                max: self.policy.max_entries,
            });
        }

        let indexed = self.index_credentials(credentials)?;
        let total_credentials_scanned = indexed.len();

        let mut exact_buckets: BTreeMap<[u8; 32], Vec<usize>> = BTreeMap::new();
        let mut case_buckets: BTreeMap<[u8; 32], Vec<usize>> = BTreeMap::new();
        let mut suffix_buckets: BTreeMap<[u8; 32], Vec<usize>> = BTreeMap::new();

        for (idx, item) in indexed.iter().enumerate() {
            exact_buckets
                .entry(item.exact_digest)
                .or_default()
                .push(idx);
            if let Some(digest) = item.case_digest {
                case_buckets.entry(digest).or_default().push(idx);
            }
            if let Some(digest) = item.suffix_digest {
                suffix_buckets.entry(digest).or_default().push(idx);
            }
        }

        let unique_exact_passwords = exact_buckets.len();
        let mut groups = Vec::new();
        let mut covered_exact_sets: HashSet<Vec<usize>> = HashSet::new();

        for (digest, member_indices) in &exact_buckets {
            if member_indices.len() < 2 {
                continue;
            }
            covered_exact_sets.insert(member_indices.clone());
            if let Some(group) =
                self.build_group(digest, ReuseMatchKind::Exact, member_indices, &indexed)
            {
                groups.push(group);
            }
        }

        if self.policy.detect_case_variants {
            for (digest, member_indices) in &case_buckets {
                if member_indices.len() < 2 || covered_exact_sets.contains(member_indices) {
                    continue;
                }
                covered_exact_sets.insert(member_indices.clone());
                if let Some(group) = self.build_group(
                    digest,
                    ReuseMatchKind::CaseInsensitive,
                    member_indices,
                    &indexed,
                ) {
                    groups.push(group);
                }
            }
        }

        if self.policy.detect_suffix_variants {
            for (digest, member_indices) in &suffix_buckets {
                if member_indices.len() < 2 || covered_exact_sets.contains(member_indices) {
                    continue;
                }
                if let Some(group) = self.build_group(
                    digest,
                    ReuseMatchKind::SuffixVariant,
                    member_indices,
                    &indexed,
                ) {
                    groups.push(group);
                }
            }
        }

        groups.sort_by(|a, b| {
            b.severity
                .cmp(&a.severity)
                .then_with(|| b.match_kind.cmp(&a.match_kind))
                .then_with(|| b.entries.len().cmp(&a.entries.len()))
                .then_with(|| a.group_id.cmp(&b.group_id))
        });

        let mut reused_ids: HashSet<&str> = HashSet::new();
        let mut critical_groups = 0usize;
        let mut high_groups = 0usize;
        let mut medium_groups = 0usize;
        let mut low_groups = 0usize;

        for group in &groups {
            match group.severity {
                ReuseSeverity::Critical => critical_groups += 1,
                ReuseSeverity::High => high_groups += 1,
                ReuseSeverity::Medium => medium_groups += 1,
                ReuseSeverity::Low => low_groups += 1,
            }
            for entry in &group.entries {
                reused_ids.insert(entry.id.as_str());
            }
        }

        Ok(ReuseAuditReport {
            total_credentials_scanned,
            unique_exact_passwords,
            reused_credential_count: reused_ids.len(),
            critical_groups,
            high_groups,
            medium_groups,
            low_groups,
            groups,
            generated_at_unix: OffsetDateTime::now_utc().unix_timestamp(),
        })
    }

    pub fn check_candidate(
        &self,
        candidate: &CredentialInput,
        existing: &[CredentialInput],
    ) -> Result<Vec<ReuseGroup>, ReuseDetectorError> {
        if existing.len().saturating_add(1) > self.policy.max_entries {
            return Err(ReuseDetectorError::CapacityExceeded {
                max: self.policy.max_entries,
            });
        }

        let mut combined: Vec<CredentialInput> = Vec::with_capacity(existing.len() + 1);
        combined.push(CredentialInput::new(
            candidate.id.as_str(),
            candidate.origin.as_str(),
            candidate.username.as_str(),
            candidate.password.as_str(),
            candidate.updated_at_unix,
        ));
        for item in existing {
            if item.id == candidate.id {
                continue;
            }
            combined.push(CredentialInput::new(
                item.id.as_str(),
                item.origin.as_str(),
                item.username.as_str(),
                item.password.as_str(),
                item.updated_at_unix,
            ));
        }

        let report = self.analyze(&combined)?;
        let matching_groups = report
            .groups
            .into_iter()
            .filter(|group| group.entries.iter().any(|e| e.id == candidate.id))
            .collect();

        Ok(matching_groups)
    }

    fn index_credentials(
        &self,
        credentials: &[CredentialInput],
    ) -> Result<Vec<IndexedCredential>, ReuseDetectorError> {
        let mut seen_ids: HashMap<&str, ()> = HashMap::with_capacity(credentials.len());
        let mut indexed = Vec::with_capacity(credentials.len());

        for cred in credentials {
            if cred.id.trim().is_empty() {
                return Err(ReuseDetectorError::EmptyCredentialId);
            }
            if seen_ids.insert(cred.id.as_str(), ()).is_some() {
                return Err(ReuseDetectorError::DuplicateCredentialId(cred.id.clone()));
            }
            if cred.password.is_empty() {
                return Err(ReuseDetectorError::EmptyPassword {
                    id: cred.id.clone(),
                });
            }

            let (normalized_origin, site_key) = normalize_origin(&cred.id, &cred.origin)?;
            let exact_digest = self.compute_digest(b"exact:", cred.password.as_bytes());

            let mut lower = cred.password.to_lowercase();
            let case_digest = if self.policy.detect_case_variants {
                Some(self.compute_digest(b"case:", lower.as_bytes()))
            } else {
                None
            };

            let suffix_digest = if self.policy.detect_suffix_variants {
                let mut stripped = strip_trailing_suffix(&lower);
                let digest = if stripped.chars().count() >= self.policy.min_password_len_for_variant
                    && stripped != lower
                {
                    Some(self.compute_digest(b"suffix:", stripped.as_bytes()))
                } else {
                    None
                };
                stripped.zeroize();
                digest
            } else {
                None
            };

            lower.zeroize();

            indexed.push(IndexedCredential {
                cred_ref: ReusedCredentialRef {
                    id: cred.id.clone(),
                    origin: normalized_origin,
                    site_key,
                    username: cred.username.clone(),
                    updated_at_unix: cred.updated_at_unix,
                },
                exact_digest,
                case_digest,
                suffix_digest,
            });
        }

        Ok(indexed)
    }

    fn compute_digest(&self, domain_sep: &[u8], payload: &[u8]) -> [u8; 32] {
        let mut ctx = hmac::Context::with_key(&self.hmac_key);
        ctx.update(domain_sep);
        ctx.update(payload);
        let tag = ctx.sign();
        let mut out = [0u8; 32];
        out.copy_from_slice(tag.as_ref());
        out
    }

    fn build_group(
        &self,
        digest: &[u8; 32],
        match_kind: ReuseMatchKind,
        member_indices: &[usize],
        indexed: &[IndexedCredential],
    ) -> Option<ReuseGroup> {
        let mut entries: Vec<ReusedCredentialRef> = member_indices
            .iter()
            .filter_map(|&idx| indexed.get(idx).map(|item| item.cred_ref.clone()))
            .collect();

        entries.sort_by(|a, b| {
            a.site_key
                .cmp(&b.site_key)
                .then_with(|| a.origin.cmp(&b.origin))
                .then_with(|| a.username.cmp(&b.username))
                .then_with(|| a.id.cmp(&b.id))
        });

        let distinct_origins = entries
            .iter()
            .map(|e| e.origin.as_str())
            .collect::<HashSet<_>>()
            .len();
        let distinct_sites = entries
            .iter()
            .map(|e| e.site_key.as_str())
            .collect::<HashSet<_>>()
            .len();

        if self.policy.ignore_same_site_reuse && distinct_sites < 2 {
            return None;
        }

        let severity = classify_severity(match_kind, distinct_sites, distinct_origins);

        Some(ReuseGroup {
            group_id: encode_hex_prefix(digest, GROUP_ID_BYTES),
            match_kind,
            severity,
            distinct_origins,
            distinct_sites,
            entries,
        })
    }
}

struct IndexedCredential {
    cred_ref: ReusedCredentialRef,
    exact_digest: [u8; 32],
    case_digest: Option<[u8; 32]>,
    suffix_digest: Option<[u8; 32]>,
}

fn classify_severity(
    match_kind: ReuseMatchKind,
    distinct_sites: usize,
    distinct_origins: usize,
) -> ReuseSeverity {
    match match_kind {
        ReuseMatchKind::Exact => {
            if distinct_sites >= 2 {
                ReuseSeverity::Critical
            } else if distinct_origins >= 2 {
                ReuseSeverity::High
            } else {
                ReuseSeverity::Medium
            }
        }
        ReuseMatchKind::CaseInsensitive | ReuseMatchKind::SuffixVariant => {
            if distinct_sites >= 2 {
                ReuseSeverity::High
            } else if distinct_origins >= 2 {
                ReuseSeverity::Medium
            } else {
                ReuseSeverity::Low
            }
        }
    }
}

fn normalize_origin(id: &str, raw_origin: &str) -> Result<(String, String), ReuseDetectorError> {
    let trimmed = raw_origin.trim();
    let parsed = Url::parse(trimmed).map_err(|err| ReuseDetectorError::InvalidOrigin {
        id: id.to_string(),
        origin: raw_origin.to_string(),
        reason: err.to_string(),
    })?;

    let host = parsed
        .host_str()
        .ok_or_else(|| ReuseDetectorError::InvalidOrigin {
            id: id.to_string(),
            origin: raw_origin.to_string(),
            reason: "origin must include a host".to_string(),
        })?;

    let scheme = parsed.scheme().to_ascii_lowercase();
    let host_lower = host.to_ascii_lowercase();
    let normalized = match parsed.port() {
        Some(port) => format!("{scheme}://{host_lower}:{port}"),
        None => format!("{scheme}://{host_lower}"),
    };
    let site_key = extract_site_key(&host_lower);

    Ok((normalized, site_key))
}

fn extract_site_key(host_lower: &str) -> String {
    let trimmed = host_lower.trim_end_matches('.');
    if trimmed.contains(':') || trimmed.parse::<std::net::Ipv4Addr>().is_ok() {
        return trimmed.to_string();
    }

    let labels: Vec<&str> = trimmed.split('.').filter(|s| !s.is_empty()).collect();
    if labels.len() <= 2 {
        return trimmed.to_string();
    }

    const TWO_PART_SUFFIXES: &[(&str, &str)] = &[
        ("co", "uk"),
        ("org", "uk"),
        ("ac", "uk"),
        ("gov", "uk"),
        ("com", "au"),
        ("net", "au"),
        ("org", "au"),
        ("co", "jp"),
        ("or", "jp"),
        ("com", "br"),
        ("co", "nz"),
    ];

    let last = labels[labels.len() - 1];
    let second_last = labels[labels.len() - 2];
    if TWO_PART_SUFFIXES.contains(&(second_last, last)) && labels.len() >= 3 {
        let third_last = labels[labels.len() - 3];
        return format!("{third_last}.{second_last}.{last}");
    }

    format!("{second_last}.{last}")
}

fn strip_trailing_suffix(input: &str) -> String {
    let trimmed = input.trim_end_matches(|c: char| {
        c.is_ascii_digit()
            || matches!(
                c,
                '!' | '@' | '#' | '$' | '%' | '^' | '&' | '*' | '_' | '-' | '.' | '?'
            )
    });
    trimmed.to_string()
}

fn encode_hex_prefix(bytes: &[u8], max_bytes: usize) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let count = bytes.len().min(max_bytes);
    let mut out = String::with_capacity(count * 2);
    for &b in &bytes[..count] {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_detector(policy: ReusePolicy) -> PasswordReuseDetector {
        PasswordReuseDetector::new([0x42u8; 32], policy).expect("valid test detector")
    }

    #[test]
    fn happy_path_exact_cross_site_reuse_and_serialization() {
        let detector = test_detector(ReusePolicy::default());
        let creds = vec![
            CredentialInput::new(
                "cred-1",
                "https://bank.example.com/login",
                "alice@example.com",
                "Sup3rS3cret!Pass",
                Some(1_700_000_000),
            ),
            CredentialInput::new(
                "cred-2",
                "https://shop.other.org:8443/signin",
                "alice",
                "Sup3rS3cret!Pass",
                Some(1_700_000_100),
            ),
            CredentialInput::new(
                "cred-3",
                "https://unique.example.net",
                "bob",
                "TotallyDifferent#99",
                None,
            ),
        ];

        let report = detector.analyze(&creds).expect("analysis succeeds");
        assert_eq!(report.total_credentials_scanned, 3);
        assert_eq!(report.unique_exact_passwords, 2);
        assert_eq!(report.reused_credential_count, 2);
        assert_eq!(report.critical_groups, 1);
        assert_eq!(report.high_groups, 0);
        assert_eq!(report.groups.len(), 1);

        let group = &report.groups[0];
        assert_eq!(group.match_kind, ReuseMatchKind::Exact);
        assert_eq!(group.severity, ReuseSeverity::Critical);
        assert_eq!(group.distinct_sites, 2);
        assert_eq!(group.distinct_origins, 2);
        assert_eq!(group.entries[0].site_key, "example.com");
        assert_eq!(group.entries[1].site_key, "other.org");
        assert_eq!(group.entries[1].origin, "https://shop.other.org:8443");

        let json = serde_json::to_string(&report).expect("serialize report");
        assert!(!json.contains("Sup3rS3cret!Pass"));
        let decoded: ReuseAuditReport = serde_json::from_str(&json).expect("deserialize report");
        assert_eq!(decoded, report);
    }

    #[test]
    fn variant_detection_case_and_suffix_without_duplicate_exact_groups() {
        let detector = test_detector(ReusePolicy::default());
        let creds = vec![
            CredentialInput::new(
                "c1",
                "https://alpha.example.com",
                "user1",
                "CorrectHorse!1",
                None,
            ),
            CredentialInput::new(
                "c2",
                "https://beta.example.org",
                "user2",
                "correcthorse!1",
                None,
            ),
            CredentialInput::new(
                "c3",
                "https://gamma.example.net",
                "user3",
                "CorrectHorse2026!!",
                None,
            ),
            CredentialInput::new(
                "e1",
                "https://exact1.example.com",
                "u1",
                "ExactOnlyPass",
                None,
            ),
            CredentialInput::new(
                "e2",
                "https://exact2.example.org",
                "u2",
                "ExactOnlyPass",
                None,
            ),
        ];

        let report = detector.analyze(&creds).expect("analysis succeeds");
        assert_eq!(report.critical_groups, 1);
        assert_eq!(report.high_groups, 2);

        let exact_groups: Vec<_> = report
            .groups
            .iter()
            .filter(|g| g.match_kind == ReuseMatchKind::Exact)
            .collect();
        assert_eq!(exact_groups.len(), 1);
        assert_eq!(exact_groups[0].entries.len(), 2);

        let case_groups: Vec<_> = report
            .groups
            .iter()
            .filter(|g| g.match_kind == ReuseMatchKind::CaseInsensitive)
            .collect();
        assert_eq!(case_groups.len(), 1);
        assert_eq!(case_groups[0].entries.len(), 2);

        let suffix_groups: Vec<_> = report
            .groups
            .iter()
            .filter(|g| g.match_kind == ReuseMatchKind::SuffixVariant)
            .collect();
        assert_eq!(suffix_groups.len(), 1);
        assert_eq!(suffix_groups[0].entries.len(), 3);
    }

    #[test]
    fn same_site_subdomains_and_ignore_policy() {
        let creds = vec![
            CredentialInput::new(
                "s1",
                "https://login.example.co.uk",
                "alice",
                "SharedSitePass!1",
                None,
            ),
            CredentialInput::new(
                "s2",
                "https://portal.example.co.uk",
                "alice",
                "SharedSitePass!1",
                None,
            ),
        ];

        let default_detector = test_detector(ReusePolicy::default());
        let report = default_detector.analyze(&creds).expect("analysis succeeds");
        assert_eq!(report.groups.len(), 1);
        assert_eq!(report.groups[0].distinct_sites, 1);
        assert_eq!(report.groups[0].distinct_origins, 2);
        assert_eq!(report.groups[0].severity, ReuseSeverity::High);
        assert_eq!(report.groups[0].entries[0].site_key, "example.co.uk");

        let strict_policy = ReusePolicy {
            ignore_same_site_reuse: true,
            ..ReusePolicy::default()
        };
        let strict_detector = test_detector(strict_policy);
        let strict_report = strict_detector.analyze(&creds).expect("analysis succeeds");
        assert!(strict_report.groups.is_empty());
        assert_eq!(strict_report.reused_credential_count, 0);
    }

    #[test]
    fn check_candidate_detects_reuse_before_saving() {
        let detector = PasswordReuseDetector::with_ephemeral_key(ReusePolicy::default())
            .expect("ephemeral key detector");

        let existing = vec![
            CredentialInput::new(
                "vault-1",
                "https://mail.example.com",
                "alice",
                "MyReusedPass#1",
                None,
            ),
            CredentialInput::new(
                "vault-2",
                "https://notes.example.org",
                "alice",
                "AnotherUniquePass#2",
                None,
            ),
        ];

        let candidate = CredentialInput::new(
            "pending-new",
            "https://cloud.another.io",
            "alice@another.io",
            "MyReusedPass#1",
            None,
        );

        let hits = detector
            .check_candidate(&candidate, &existing)
            .expect("candidate check succeeds");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].severity, ReuseSeverity::Critical);
        assert_eq!(hits[0].entries.len(), 2);
    }

    #[test]
    fn validation_errors_and_edge_cases() {
        let bad_policy = ReusePolicy {
            min_password_len_for_variant: 0,
            ..ReusePolicy::default()
        };
        assert_eq!(
            PasswordReuseDetector::new([1u8; 32], bad_policy)
                .err()
                .expect("invalid config"),
            ReuseDetectorError::InvalidConfig(
                "min_password_len_for_variant must be at least 1".to_string()
            )
        );

        assert_eq!(
            PasswordReuseDetector::from_key_slice(&[0u8; 16], ReusePolicy::default())
                .err()
                .expect("invalid key length"),
            ReuseDetectorError::InvalidKeyLength {
                expected: 32,
                actual: 16
            }
        );

        let detector = test_detector(ReusePolicy {
            max_entries: 2,
            ..ReusePolicy::default()
        });

        let empty_id = vec![CredentialInput::new(
            "",
            "https://a.com",
            "u",
            "secret12",
            None,
        )];
        assert_eq!(
            detector.analyze(&empty_id).expect_err("empty id"),
            ReuseDetectorError::EmptyCredentialId
        );

        let dup_ids = vec![
            CredentialInput::new("dup", "https://a.com", "u1", "secret12", None),
            CredentialInput::new("dup", "https://b.com", "u2", "secret34", None),
        ];
        assert_eq!(
            detector.analyze(&dup_ids).expect_err("duplicate id"),
            ReuseDetectorError::DuplicateCredentialId("dup".to_string())
        );

        let bad_origin = vec![CredentialInput::new(
            "x1",
            "not-a-valid-url",
            "u",
            "secret12",
            None,
        )];
        assert!(matches!(
            detector.analyze(&bad_origin),
            Err(ReuseDetectorError::InvalidOrigin { .. })
        ));

        let empty_pw = vec![CredentialInput::new("x2", "https://a.com", "u", "", None)];
        assert_eq!(
            detector.analyze(&empty_pw).expect_err("empty password"),
            ReuseDetectorError::EmptyPassword {
                id: "x2".to_string()
            }
        );

        let too_many = vec![
            CredentialInput::new("1", "https://a.com", "u", "pass1111", None),
            CredentialInput::new("2", "https://b.com", "u", "pass2222", None),
            CredentialInput::new("3", "https://c.com", "u", "pass3333", None),
        ];
        assert_eq!(
            detector.analyze(&too_many).expect_err("capacity exceeded"),
            ReuseDetectorError::CapacityExceeded { max: 2 }
        );
    }
}
