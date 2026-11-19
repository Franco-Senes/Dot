//! Pure-Rust download checksum computation, manifest parsing, and verification.
//!
//! Supports streaming SHA-256, SHA-384, and SHA-512 digest computation via `ring`,
//! GNU/BSD checksum manifest parsing, constant-time digest comparison, policy
//! enforcement, and JSON-serializable verification records.

use ring::digest::{Context, SHA256, SHA384, SHA512};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use thiserror::Error;
use time::OffsetDateTime;

/// Default read buffer size (64 KiB) for streaming file verification.
pub const STREAM_BUFFER_SIZE: usize = 64 * 1024;

/// Maximum number of verification records retained in a [`VerificationStore`].
pub const MAX_VERIFICATION_RECORDS: usize = 500;

/// Supported cryptographic hash algorithms for download verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChecksumAlgorithm {
    Sha256,
    Sha384,
    Sha512,
}

impl ChecksumAlgorithm {
    /// Returns the canonical lowercase algorithm name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sha256 => "sha256",
            Self::Sha384 => "sha384",
            Self::Sha512 => "sha512",
        }
    }

    /// Returns the expected digest length in raw bytes.
    pub fn digest_len(self) -> usize {
        match self {
            Self::Sha256 => 32,
            Self::Sha384 => 48,
            Self::Sha512 => 64,
        }
    }

    /// Returns the expected digest length in hexadecimal characters.
    pub fn hex_len(self) -> usize {
        self.digest_len() * 2
    }

    /// Parses an algorithm identifier (case-insensitive, optional hyphen).
    pub fn parse(raw: &str) -> Result<Self, ChecksumError> {
        let normalized = raw.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "sha256" | "sha-256" => Ok(Self::Sha256),
            "sha384" | "sha-384" => Ok(Self::Sha384),
            "sha512" | "sha-512" => Ok(Self::Sha512),
            _ => Err(ChecksumError::UnsupportedAlgorithm(raw.trim().to_string())),
        }
    }

    /// Infers the algorithm from a raw hexadecimal digest length.
    pub fn infer_from_hex_len(hex_len: usize) -> Result<Self, ChecksumError> {
        match hex_len {
            64 => Ok(Self::Sha256),
            96 => Ok(Self::Sha384),
            128 => Ok(Self::Sha512),
            other => Err(ChecksumError::InvalidHexLength {
                expected: 64,
                actual: other,
            }),
        }
    }

    fn new_ring_context(self) -> Context {
        match self {
            Self::Sha256 => Context::new(&SHA256),
            Self::Sha384 => Context::new(&SHA384),
            Self::Sha512 => Context::new(&SHA512),
        }
    }
}

impl fmt::Display for ChecksumAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Errors produced during checksum parsing, computation, or verification.
#[derive(Debug, Error)]
pub enum ChecksumError {
    #[error("checksum digest is empty")]
    EmptyDigest,

    #[error("unsupported checksum algorithm: {0}")]
    UnsupportedAlgorithm(String),

    #[error("invalid hex digest length: expected {expected} chars, got {actual}")]
    InvalidHexLength { expected: usize, actual: usize },

    #[error("invalid hex character '{ch}' at byte offset {index}")]
    InvalidHexCharacter { ch: char, index: usize },

    #[error("invalid checksum manifest line {line}: {reason}")]
    ManifestParseError { line: usize, reason: String },

    #[error("no checksum entry found for filename '{0}'")]
    EntryNotFound(String),

    #[error("algorithm {0} is disallowed by verification policy")]
    AlgorithmDisallowed(ChecksumAlgorithm),

    #[error("file size {actual} bytes exceeds policy limit of {limit} bytes")]
    FileTooLarge { actual: u64, limit: u64 },

    #[error("missing filename on path '{0}'")]
    MissingFilename(String),

    #[error("I/O error while reading download: {0}")]
    Io(#[from] std::io::Error),
}

/// A validated expected checksum with its algorithm and normalized lowercase hex digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedChecksum {
    pub algorithm: ChecksumAlgorithm,
    pub hex_digest: String,
}

impl ExpectedChecksum {
    /// Creates an [`ExpectedChecksum`] from an explicit algorithm and hex digest string.
    pub fn new(algorithm: ChecksumAlgorithm, raw_hex: &str) -> Result<Self, ChecksumError> {
        let normalized = normalize_and_validate_hex(raw_hex, Some(algorithm.hex_len()))?;
        Ok(Self {
            algorithm,
            hex_digest: normalized,
        })
    }

    /// Parses a checksum string in either `algo:hex` form (e.g. `sha256:abcd...`)
    /// or bare hex form (inferring the algorithm from digest length).
    pub fn parse(input: &str) -> Result<Self, ChecksumError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(ChecksumError::EmptyDigest);
        }

        if let Some((algo_part, hex_part)) = trimmed.split_once(':') {
            let algorithm = ChecksumAlgorithm::parse(algo_part)?;
            return Self::new(algorithm, hex_part);
        }

        let normalized = normalize_and_validate_hex(trimmed, None)?;
        let algorithm = ChecksumAlgorithm::infer_from_hex_len(normalized.len())?;
        Ok(Self {
            algorithm,
            hex_digest: normalized,
        })
    }

    /// Decodes the normalized hex digest into raw bytes.
    pub fn as_bytes(&self) -> Result<Vec<u8>, ChecksumError> {
        hex_to_bytes(&self.hex_digest)
    }
}

impl fmt::Display for ExpectedChecksum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.algorithm, self.hex_digest)
    }
}

/// Result of computing a digest over a byte stream or file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputedChecksum {
    pub algorithm: ChecksumAlgorithm,
    pub hex_digest: String,
    pub bytes_processed: u64,
}

impl ComputedChecksum {
    /// Compares this computed digest against an [`ExpectedChecksum`] in constant time.
    pub fn matches(&self, expected: &ExpectedChecksum) -> bool {
        if self.algorithm != expected.algorithm {
            return false;
        }
        constant_time_eq(self.hex_digest.as_bytes(), expected.hex_digest.as_bytes())
    }
}

/// Streaming incremental checksum calculator.
pub struct ChecksumVerifier {
    algorithm: ChecksumAlgorithm,
    context: Context,
    bytes_processed: u64,
}

impl ChecksumVerifier {
    /// Starts a new streaming verifier for the given algorithm.
    pub fn new(algorithm: ChecksumAlgorithm) -> Self {
        Self {
            algorithm,
            context: algorithm.new_ring_context(),
            bytes_processed: 0,
        }
    }

    /// Feeds a chunk of bytes into the hash state.
    pub fn update(&mut self, chunk: &[u8]) {
        self.context.update(chunk);
        self.bytes_processed = self.bytes_processed.saturating_add(chunk.len() as u64);
    }

    /// Returns the number of bytes fed so far.
    pub fn bytes_processed(&self) -> u64 {
        self.bytes_processed
    }

    /// Finalizes the hash computation and returns the computed digest.
    pub fn finalize(self) -> ComputedChecksum {
        let digest = self.context.finish();
        ComputedChecksum {
            algorithm: self.algorithm,
            hex_digest: bytes_to_hex(digest.as_ref()),
            bytes_processed: self.bytes_processed,
        }
    }

    /// Finalizes the hash computation and verifies it against `expected`.
    pub fn verify(self, expected: &ExpectedChecksum) -> VerificationReport {
        let computed = self.finalize();
        let matched = computed.matches(expected);
        VerificationReport {
            algorithm: expected.algorithm,
            expected_hex: expected.hex_digest.clone(),
            actual_hex: computed.hex_digest,
            bytes_verified: computed.bytes_processed,
            matched,
            status: if matched {
                VerificationStatus::Verified
            } else {
                VerificationStatus::Mismatch
            },
            verified_at_unix: current_unix_timestamp(),
        }
    }
}

/// High-level outcome status for a download verification check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Verified,
    Mismatch,
    MissingExpected,
    PolicyRejected,
}

/// Detailed report produced after verifying a download against an expected checksum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationReport {
    pub algorithm: ChecksumAlgorithm,
    pub expected_hex: String,
    pub actual_hex: String,
    pub bytes_verified: u64,
    pub matched: bool,
    pub status: VerificationStatus,
    pub verified_at_unix: i64,
}

/// Policy controlling how download checksum verification is enforced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationPolicy {
    pub require_checksum: bool,
    pub allowed_algorithms: Vec<ChecksumAlgorithm>,
    pub quarantine_on_mismatch: bool,
    pub max_file_size_bytes: Option<u64>,
}

impl Default for VerificationPolicy {
    fn default() -> Self {
        Self {
            require_checksum: false,
            allowed_algorithms: vec![
                ChecksumAlgorithm::Sha256,
                ChecksumAlgorithm::Sha384,
                ChecksumAlgorithm::Sha512,
            ],
            quarantine_on_mismatch: true,
            max_file_size_bytes: None,
        }
    }
}

impl VerificationPolicy {
    /// Checks whether an algorithm is permitted by this policy.
    pub fn validate_algorithm(&self, algorithm: ChecksumAlgorithm) -> Result<(), ChecksumError> {
        if self.allowed_algorithms.contains(&algorithm) {
            Ok(())
        } else {
            Err(ChecksumError::AlgorithmDisallowed(algorithm))
        }
    }

    /// Checks whether a file size is within the policy's configured maximum.
    pub fn validate_file_size(&self, size_bytes: u64) -> Result<(), ChecksumError> {
        if let Some(limit) = self.max_file_size_bytes {
            if size_bytes > limit {
                return Err(ChecksumError::FileTooLarge {
                    actual: size_bytes,
                    limit,
                });
            }
        }
        Ok(())
    }
}

/// A single entry parsed from a GNU or BSD checksum manifest file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub filename: String,
    pub checksum: ExpectedChecksum,
    pub binary_mode: bool,
}

/// Parsed collection of checksum entries from a `SHA256SUMS`-style manifest.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChecksumManifest {
    entries: BTreeMap<String, ManifestEntry>,
}

impl ChecksumManifest {
    /// Parses a GNU coreutils or BSD-style checksum manifest.
    ///
    /// Supports:
    /// - GNU format: `<hex>  <filename>` or `<hex> *<filename>` (optional `algo:` prefix on hex)
    /// - BSD format: `SHA256 (<filename>) = <hex>`
    /// - Blank lines and `#` comments are skipped.
    pub fn parse(
        text: &str,
        default_algorithm: Option<ChecksumAlgorithm>,
    ) -> Result<Self, ChecksumError> {
        let mut entries = BTreeMap::new();

        for (idx, raw_line) in text.lines().enumerate() {
            let line_num = idx + 1;
            let trimmed = raw_line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let entry = if let Some(bsd) = try_parse_bsd_line(trimmed, line_num)? {
                bsd
            } else {
                parse_gnu_line(trimmed, line_num, default_algorithm)?
            };

            entries.insert(entry.filename.clone(), entry);
        }

        Ok(Self { entries })
    }

    /// Looks up an entry by exact filename or basename.
    pub fn find_for_filename(&self, filename: &str) -> Option<&ManifestEntry> {
        if let Some(exact) = self.entries.get(filename) {
            return Some(exact);
        }
        let base = Path::new(filename)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(filename);
        self.entries.get(base)
    }

    /// Returns the number of entries in the manifest.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the manifest contains no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterates over all manifest entries in deterministic filename order.
    pub fn iter(&self) -> impl Iterator<Item = &ManifestEntry> {
        self.entries.values()
    }
}

/// Persistent audit record of a download verification attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadVerificationRecord {
    pub download_id: String,
    pub filename: String,
    pub report: VerificationReport,
}

/// JSON-serializable store of recent download verification records.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationStore {
    records: Vec<DownloadVerificationRecord>,
}

impl VerificationStore {
    /// Creates an empty verification store.
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    /// Appends a verification record, evicting the oldest if [`MAX_VERIFICATION_RECORDS`] is exceeded.
    pub fn record(&mut self, entry: DownloadVerificationRecord) {
        self.records.push(entry);
        if self.records.len() > MAX_VERIFICATION_RECORDS {
            let overflow = self.records.len() - MAX_VERIFICATION_RECORDS;
            self.records.drain(0..overflow);
        }
    }

    /// Returns all stored verification records.
    pub fn records(&self) -> &[DownloadVerificationRecord] {
        &self.records
    }

    /// Finds the most recent verification record for a given `download_id`.
    pub fn find_by_download_id(&self, download_id: &str) -> Option<&DownloadVerificationRecord> {
        self.records
            .iter()
            .rev()
            .find(|r| r.download_id == download_id)
    }

    /// Serializes the store to pretty-printed JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserializes a store from JSON.
    pub fn from_json(raw: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(raw)
    }
}

/// Computes the checksum of an in-memory byte slice.
pub fn compute_bytes_checksum(data: &[u8], algorithm: ChecksumAlgorithm) -> ComputedChecksum {
    let mut verifier = ChecksumVerifier::new(algorithm);
    verifier.update(data);
    verifier.finalize()
}

/// Verifies an in-memory byte slice against an [`ExpectedChecksum`].
pub fn verify_bytes(data: &[u8], expected: &ExpectedChecksum) -> VerificationReport {
    let mut verifier = ChecksumVerifier::new(expected.algorithm);
    verifier.update(data);
    verifier.verify(expected)
}

/// Computes the checksum of any [`Read`] stream using a 64 KiB buffer.
pub fn compute_reader_checksum<R: Read>(
    reader: R,
    algorithm: ChecksumAlgorithm,
) -> Result<ComputedChecksum, ChecksumError> {
    let mut buf_reader = BufReader::with_capacity(STREAM_BUFFER_SIZE, reader);
    let mut verifier = ChecksumVerifier::new(algorithm);
    let mut buffer = vec![0u8; STREAM_BUFFER_SIZE];

    loop {
        let read_bytes = buf_reader.read(&mut buffer)?;
        if read_bytes == 0 {
            break;
        }
        verifier.update(&buffer[..read_bytes]);
    }

    Ok(verifier.finalize())
}

/// Computes the checksum of a file on disk using streaming I/O.
pub fn compute_file_checksum(
    path: &Path,
    algorithm: ChecksumAlgorithm,
) -> Result<ComputedChecksum, ChecksumError> {
    let file = File::open(path)?;
    compute_reader_checksum(file, algorithm)
}

/// Verifies a file on disk against an [`ExpectedChecksum`].
pub fn verify_file(
    path: &Path,
    expected: &ExpectedChecksum,
) -> Result<VerificationReport, ChecksumError> {
    verify_file_with_policy(path, expected, &VerificationPolicy::default())
}

/// Verifies a file on disk against an [`ExpectedChecksum`] while enforcing a [`VerificationPolicy`].
pub fn verify_file_with_policy(
    path: &Path,
    expected: &ExpectedChecksum,
    policy: &VerificationPolicy,
) -> Result<VerificationReport, ChecksumError> {
    policy.validate_algorithm(expected.algorithm)?;
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    policy.validate_file_size(metadata.len())?;

    let computed = compute_reader_checksum(file, expected.algorithm)?;
    let matched = computed.matches(expected);

    Ok(VerificationReport {
        algorithm: expected.algorithm,
        expected_hex: expected.hex_digest.clone(),
        actual_hex: computed.hex_digest,
        bytes_verified: computed.bytes_processed,
        matched,
        status: if matched {
            VerificationStatus::Verified
        } else {
            VerificationStatus::Mismatch
        },
        verified_at_unix: current_unix_timestamp(),
    })
}

/// Verifies a file on disk by looking up its basename in a checksum manifest.
pub fn verify_file_against_manifest(
    path: &Path,
    manifest_text: &str,
    default_algorithm: Option<ChecksumAlgorithm>,
    policy: &VerificationPolicy,
) -> Result<VerificationReport, ChecksumError> {
    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| ChecksumError::MissingFilename(path.display().to_string()))?;

    let manifest = ChecksumManifest::parse(manifest_text, default_algorithm)?;
    let entry = manifest
        .find_for_filename(filename)
        .ok_or_else(|| ChecksumError::EntryNotFound(filename.to_string()))?;

    verify_file_with_policy(path, &entry.checksum, policy)
}

fn try_parse_bsd_line(line: &str, line_num: usize) -> Result<Option<ManifestEntry>, ChecksumError> {
    // BSD format: `SHA256 (filename) = <hex>`
    let Some(open_paren) = line.find('(') else {
        return Ok(None);
    };
    let Some(eq_idx) = line.rfind(") = ") else {
        return Ok(None);
    };
    if eq_idx <= open_paren {
        return Ok(None);
    }

    let algo_str = line[..open_paren].trim();
    let filename = line[open_paren + 1..eq_idx].trim();
    let hex_str = line[eq_idx + 4..].trim();

    if filename.is_empty() {
        return Err(ChecksumError::ManifestParseError {
            line: line_num,
            reason: "empty filename in BSD checksum entry".to_string(),
        });
    }

    let algorithm = ChecksumAlgorithm::parse(algo_str)?;
    let checksum = ExpectedChecksum::new(algorithm, hex_str)?;

    Ok(Some(ManifestEntry {
        filename: sanitize_manifest_filename(filename, line_num)?,
        checksum,
        binary_mode: true,
    }))
}

fn parse_gnu_line(
    line: &str,
    line_num: usize,
    default_algorithm: Option<ChecksumAlgorithm>,
) -> Result<ManifestEntry, ChecksumError> {
    let mut parts = line.splitn(2, char::is_whitespace);
    let raw_digest = parts.next().unwrap_or("");
    let rest = parts
        .next()
        .ok_or_else(|| ChecksumError::ManifestParseError {
            line: line_num,
            reason: "missing filename after digest".to_string(),
        })?
        .trim_start();

    if rest.is_empty() {
        return Err(ChecksumError::ManifestParseError {
            line: line_num,
            reason: "empty filename after digest".to_string(),
        });
    }

    let (binary_mode, raw_filename) = if let Some(stripped) = rest.strip_prefix('*') {
        (true, stripped)
    } else {
        (false, rest)
    };

    let checksum = if raw_digest.contains(':') {
        ExpectedChecksum::parse(raw_digest)?
    } else if let Some(algo) = default_algorithm {
        ExpectedChecksum::new(algo, raw_digest)?
    } else {
        ExpectedChecksum::parse(raw_digest)?
    };

    Ok(ManifestEntry {
        filename: sanitize_manifest_filename(raw_filename.trim(), line_num)?,
        checksum,
        binary_mode,
    })
}

fn sanitize_manifest_filename(raw: &str, line_num: usize) -> Result<String, ChecksumError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ChecksumError::ManifestParseError {
            line: line_num,
            reason: "empty filename".to_string(),
        });
    }
    if trimmed.split(['/', '\\']).any(|seg| seg == "..") {
        return Err(ChecksumError::ManifestParseError {
            line: line_num,
            reason: "path traversal ('..') is not allowed in manifest filenames".to_string(),
        });
    }
    Ok(trimmed.to_string())
}

fn normalize_and_validate_hex(
    raw: &str,
    expected_len: Option<usize>,
) -> Result<String, ChecksumError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ChecksumError::EmptyDigest);
    }

    if let Some(expected) = expected_len {
        if trimmed.len() != expected {
            return Err(ChecksumError::InvalidHexLength {
                expected,
                actual: trimmed.len(),
            });
        }
    }

    let mut normalized = String::with_capacity(trimmed.len());
    for (index, ch) in trimmed.char_indices() {
        if !ch.is_ascii_hexdigit() {
            return Err(ChecksumError::InvalidHexCharacter { ch, index });
        }
        normalized.push(ch.to_ascii_lowercase());
    }

    Ok(normalized)
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX_CHARS[(byte >> 4) as usize] as char);
        out.push(HEX_CHARS[(byte & 0x0f) as usize] as char);
    }
    out
}

fn hex_to_bytes(hex: &str) -> Result<Vec<u8>, ChecksumError> {
    if !hex.len().is_multiple_of(2) {
        return Err(ChecksumError::InvalidHexLength {
            expected: hex.len() + 1,
            actual: hex.len(),
        });
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for idx in (0..hex.len()).step_by(2) {
        let pair = &hex[idx..idx + 2];
        let val = u8::from_str_radix(pair, 16).map_err(|_| ChecksumError::InvalidHexCharacter {
            ch: pair.chars().next().unwrap_or('?'),
            index: idx,
        })?;
        bytes.push(val);
    }
    Ok(bytes)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    std::hint::black_box(diff) == 0
}

fn current_unix_timestamp() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const HELLO_SHA256: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    const HELLO_SHA512: &str = "9b71d224bd62f3785d96d46ad3ea3d73319bfbc2890caadae2dff72519673ca72323c3d99ba5c11d7c7acc6e14b8c5da0c4663475c2e5c3adef46f73bcdec043";

    #[test]
    fn verifies_bytes_and_streaming_sha256_and_sha512() {
        let expected_256 =
            ExpectedChecksum::parse(&format!("SHA256:{}", HELLO_SHA256.to_uppercase()))
                .expect("valid sha256");
        let report_256 = verify_bytes(b"hello", &expected_256);
        assert!(report_256.matched);
        assert_eq!(report_256.status, VerificationStatus::Verified);
        assert_eq!(report_256.bytes_verified, 5);
        assert_eq!(report_256.actual_hex, HELLO_SHA256);

        let mut verifier = ChecksumVerifier::new(ChecksumAlgorithm::Sha512);
        verifier.update(b"hel");
        verifier.update(b"lo");
        assert_eq!(verifier.bytes_processed(), 5);
        let expected_512 = ExpectedChecksum::parse(HELLO_SHA512).expect("valid sha512");
        let report_512 = verifier.verify(&expected_512);
        assert!(report_512.matched);
        assert_eq!(report_512.algorithm, ChecksumAlgorithm::Sha512);
    }

    #[test]
    fn detects_mismatch_and_invalid_hex_inputs() {
        let expected =
            ExpectedChecksum::new(ChecksumAlgorithm::Sha256, HELLO_SHA256).expect("valid expected");
        let report = verify_bytes(b"hello!", &expected);
        assert!(!report.matched);
        assert_eq!(report.status, VerificationStatus::Mismatch);

        assert!(matches!(
            ExpectedChecksum::parse(""),
            Err(ChecksumError::EmptyDigest)
        ));
        assert!(matches!(
            ExpectedChecksum::new(ChecksumAlgorithm::Sha256, "abcd"),
            Err(ChecksumError::InvalidHexLength {
                expected: 64,
                actual: 4
            })
        ));
        assert!(matches!(
            ExpectedChecksum::parse("md5:d41d8cd98f00b204e9800998ecf8427e"),
            Err(ChecksumError::UnsupportedAlgorithm(_))
        ));
    }

    #[test]
    fn parses_gnu_and_bsd_manifests_and_verifies_temp_file() {
        let manifest_text = format!(
            "# release checksums\n\
             {HELLO_SHA256} *sample.bin\n\
             SHA512 (other.tar.gz) = {HELLO_SHA512}\n"
        );
        let manifest = ChecksumManifest::parse(&manifest_text, None).expect("manifest parses");
        assert_eq!(manifest.len(), 2);

        let sample_entry = manifest.find_for_filename("/tmp/downloads/sample.bin");
        assert!(sample_entry.is_some());
        assert!(sample_entry.expect("entry").binary_mode);

        let temp_dir = std::env::temp_dir().join("search_linux_feat24_test");
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let file_path = temp_dir.join("sample.bin");
        {
            let mut f = File::create(&file_path).expect("create temp file");
            f.write_all(b"hello").expect("write temp file");
        }

        let report = verify_file_against_manifest(
            &file_path,
            &manifest_text,
            None,
            &VerificationPolicy::default(),
        )
        .expect("file verification succeeds");
        assert!(report.matched);
        assert_eq!(report.status, VerificationStatus::Verified);

        let strict_policy = VerificationPolicy {
            allowed_algorithms: vec![ChecksumAlgorithm::Sha512],
            ..Default::default()
        };
        let policy_err =
            verify_file_against_manifest(&file_path, &manifest_text, None, &strict_policy);
        assert!(matches!(
            policy_err,
            Err(ChecksumError::AlgorithmDisallowed(
                ChecksumAlgorithm::Sha256
            ))
        ));

        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    #[test]
    fn rejects_path_traversal_in_manifest_and_roundtrips_store_json() {
        let bad_manifest = format!("{HELLO_SHA256}  ../evil.bin\n");
        let err = ChecksumManifest::parse(&bad_manifest, Some(ChecksumAlgorithm::Sha256));
        assert!(matches!(err, Err(ChecksumError::ManifestParseError { .. })));

        let expected = ExpectedChecksum::parse(HELLO_SHA256).expect("valid sha256");
        let report = verify_bytes(b"hello", &expected);
        let mut store = VerificationStore::new();
        store.record(DownloadVerificationRecord {
            download_id: "dl-42".to_string(),
            filename: "hello.txt".to_string(),
            report,
        });

        let json = store.to_json().expect("serialize store");
        let restored = VerificationStore::from_json(&json).expect("deserialize store");
        assert_eq!(store, restored);
        assert!(restored.find_by_download_id("dl-42").is_some());
    }
}
