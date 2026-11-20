//! Hash digest calculator feature module for Search Linux.
//!
//! Provides cryptographic hash digest calculation (SHA-256, SHA-384, SHA-512,
//! SHA-512/256, and legacy SHA-1), HMAC-SHA authentication tags with zeroized
//! key handling, multiple output encodings, constant-time checksum
//! verification, and omnibox query parsing.

use ring::{digest, hmac};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::Zeroizing;

/// Default maximum input size accepted by [`HashRequest`] (64 MiB).
pub const DEFAULT_MAX_INPUT_BYTES: usize = 64 * 1024 * 1024;

/// Supported cryptographic hash algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HashAlgorithm {
    /// SHA-256 (32-byte digest).
    Sha256,
    /// SHA-384 (48-byte digest).
    Sha384,
    /// SHA-512 (64-byte digest).
    Sha512,
    /// SHA-512/256 (32-byte truncated SHA-512 digest).
    Sha512_256,
    /// Legacy SHA-1 (20-byte digest). Provided only for compatibility checks.
    Sha1Legacy,
}

impl HashAlgorithm {
    /// All supported algorithms in canonical display order.
    pub const ALL: [Self; 5] = [
        Self::Sha256,
        Self::Sha384,
        Self::Sha512,
        Self::Sha512_256,
        Self::Sha1Legacy,
    ];

    /// Parses an algorithm name case-insensitively.
    pub fn from_str_name(name: &str) -> Result<Self, DigestError> {
        let normalized = name.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "sha256" | "sha-256" | "sha_256" | "sha256sum" => Ok(Self::Sha256),
            "sha384" | "sha-384" | "sha_384" | "sha384sum" => Ok(Self::Sha384),
            "sha512" | "sha-512" | "sha_512" | "sha512sum" => Ok(Self::Sha512),
            "sha512_256" | "sha512/256" | "sha-512/256" | "sha-512-256" => Ok(Self::Sha512_256),
            "sha1" | "sha-1" | "sha_1" | "sha1sum" | "sha1_legacy" => Ok(Self::Sha1Legacy),
            _ => Err(DigestError::UnsupportedAlgorithm(name.trim().to_string())),
        }
    }

    /// Returns the canonical lowercase identifier for the algorithm.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Sha256 => "sha256",
            Self::Sha384 => "sha384",
            Self::Sha512 => "sha512",
            Self::Sha512_256 => "sha512_256",
            Self::Sha1Legacy => "sha1",
        }
    }

    /// Returns the digest output length in bytes.
    pub const fn output_len_bytes(&self) -> usize {
        match self {
            Self::Sha256 | Self::Sha512_256 => 32,
            Self::Sha384 => 48,
            Self::Sha512 => 64,
            Self::Sha1Legacy => 20,
        }
    }

    /// Returns `true` if the algorithm is cryptographically weak / legacy.
    pub const fn is_legacy(&self) -> bool {
        matches!(self, Self::Sha1Legacy)
    }

    fn ring_algorithm(&self) -> &'static digest::Algorithm {
        match self {
            Self::Sha256 => &digest::SHA256,
            Self::Sha384 => &digest::SHA384,
            Self::Sha512 => &digest::SHA512,
            Self::Sha512_256 => &digest::SHA512_256,
            Self::Sha1Legacy => &digest::SHA1_FOR_LEGACY_USE_ONLY,
        }
    }

    fn hmac_algorithm(&self) -> Result<hmac::Algorithm, DigestError> {
        match self {
            Self::Sha256 => Ok(hmac::HMAC_SHA256),
            Self::Sha384 => Ok(hmac::HMAC_SHA384),
            Self::Sha512 => Ok(hmac::HMAC_SHA512),
            Self::Sha512_256 | Self::Sha1Legacy => {
                Err(DigestError::UnsupportedHmacAlgorithm(*self))
            }
        }
    }
}

/// Output encoding format for computed digests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DigestFormat {
    /// Lowercase hexadecimal string without separators.
    #[default]
    HexLower,
    /// Uppercase hexadecimal string without separators.
    HexUpper,
    /// Lowercase hexadecimal bytes separated by colons (`aa:bb:cc`).
    HexColon,
    /// Standard RFC 4648 Base64 encoding with `=` padding.
    Base64,
}

/// Errors produced by digest validation, computation, or serialization.
#[derive(Debug, Error)]
pub enum DigestError {
    /// The requested hash algorithm name is not recognized.
    #[error("unsupported hash algorithm: {0}")]
    UnsupportedAlgorithm(String),

    /// Input hex string had invalid length or non-hexadecimal characters.
    #[error("invalid hexadecimal input: {0}")]
    InvalidHexInput(String),

    /// Input payload exceeded the configured maximum byte limit.
    #[error("input size {actual} bytes exceeds maximum allowed {max} bytes")]
    InputTooLarge { actual: usize, max: usize },

    /// An empty key was supplied for HMAC calculation.
    #[error("HMAC key must not be empty")]
    EmptyHmacKey,

    /// HMAC is not supported for the selected digest algorithm.
    #[error("HMAC is not supported for algorithm {}", .0.as_str())]
    UnsupportedHmacAlgorithm(HashAlgorithm),

    /// Expected digest hex string did not match the algorithm's byte length.
    #[error("expected digest length {expected_len} bytes, got {actual_len} bytes")]
    InvalidExpectedDigest {
        expected_len: usize,
        actual_len: usize,
    },

    /// JSON serialization or deserialization failed.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Request parameters for computing a single hash digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HashRequest {
    /// Algorithm to evaluate.
    pub algorithm: HashAlgorithm,
    /// Output formatting style.
    pub format: DigestFormat,
    /// Raw input bytes to hash.
    pub payload: Vec<u8>,
    /// Maximum allowed payload size in bytes.
    pub max_input_bytes: usize,
}

impl HashRequest {
    /// Creates a request from UTF-8 text using default size limits and lowercase hex output.
    pub fn from_utf8(algorithm: HashAlgorithm, text: &str) -> Self {
        Self {
            algorithm,
            format: DigestFormat::HexLower,
            payload: text.as_bytes().to_vec(),
            max_input_bytes: DEFAULT_MAX_INPUT_BYTES,
        }
    }

    /// Creates a request from raw bytes using default size limits and lowercase hex output.
    pub fn from_bytes(algorithm: HashAlgorithm, bytes: Vec<u8>) -> Self {
        Self {
            algorithm,
            format: DigestFormat::HexLower,
            payload: bytes,
            max_input_bytes: DEFAULT_MAX_INPUT_BYTES,
        }
    }

    /// Creates a request by decoding a hexadecimal string (whitespace and `0x` prefix allowed).
    pub fn from_hex(algorithm: HashAlgorithm, hex_str: &str) -> Result<Self, DigestError> {
        let bytes = decode_hex(hex_str)?;
        Ok(Self::from_bytes(algorithm, bytes))
    }

    /// Sets the desired output format.
    pub fn with_format(mut self, format: DigestFormat) -> Self {
        self.format = format;
        self
    }

    /// Overrides the maximum allowed payload size in bytes.
    pub fn with_max_input_bytes(mut self, max_input_bytes: usize) -> Self {
        self.max_input_bytes = max_input_bytes;
        self
    }

    /// Validates the request parameters before computation.
    pub fn validate(&self) -> Result<(), DigestError> {
        if self.payload.len() > self.max_input_bytes {
            return Err(DigestError::InputTooLarge {
                actual: self.payload.len(),
                max: self.max_input_bytes,
            });
        }
        Ok(())
    }
}

/// Computed digest output with metadata and formatted representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DigestResult {
    /// Algorithm used to compute the digest.
    pub algorithm: HashAlgorithm,
    /// Output encoding format used for `formatted`.
    pub format: DigestFormat,
    /// Number of input bytes hashed.
    pub input_bytes: usize,
    /// Raw digest bytes.
    pub raw_bytes: Vec<u8>,
    /// Formatted digest string according to `format`.
    pub formatted: String,
    /// Optional advisory warning when a legacy algorithm is used.
    pub legacy_warning: Option<String>,
}

/// Result of computing multiple algorithms over the same input payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchDigestResult {
    /// Number of input bytes hashed.
    pub input_bytes: usize,
    /// Individual digest results in requested order.
    pub digests: Vec<DigestResult>,
}

/// Result of comparing a computed digest against an expected hexadecimal checksum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DigestComparison {
    /// Algorithm used for verification.
    pub algorithm: HashAlgorithm,
    /// Normalized lowercase expected hexadecimal digest.
    pub expected_hex: String,
    /// Normalized lowercase computed hexadecimal digest.
    pub actual_hex: String,
    /// Whether the digests match in constant time.
    pub matches: bool,
}

/// Secret key wrapper for HMAC calculations that zeroizes key material on drop.
#[derive(Clone)]
pub struct HmacKey {
    bytes: Zeroizing<Vec<u8>>,
}

impl HmacKey {
    /// Creates a new validated HMAC key from raw bytes.
    pub fn new(key_bytes: Vec<u8>) -> Result<Self, DigestError> {
        if key_bytes.is_empty() {
            return Err(DigestError::EmptyHmacKey);
        }
        Ok(Self {
            bytes: Zeroizing::new(key_bytes),
        })
    }

    /// Returns the key length in bytes.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Returns `true` if the key is empty (always `false` for validated keys).
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

/// Result of an HMAC computation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HmacResult {
    /// Underlying hash algorithm used with HMAC.
    pub algorithm: HashAlgorithm,
    /// Output encoding format used for `formatted`.
    pub format: DigestFormat,
    /// Number of message bytes authenticated.
    pub input_bytes: usize,
    /// Raw HMAC tag bytes.
    pub raw_bytes: Vec<u8>,
    /// Formatted HMAC tag string.
    pub formatted: String,
}

/// Parsed omnibox query for the hash digest calculator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParsedDigestQuery {
    /// Compute a single digest for the given UTF-8 text.
    Compute {
        algorithm: HashAlgorithm,
        text: String,
    },
    /// Verify a UTF-8 text against an expected hexadecimal digest.
    Verify {
        algorithm: HashAlgorithm,
        expected_hex: String,
        text: String,
    },
}

/// Computes a cryptographic hash digest for the given [`HashRequest`].
pub fn compute_digest(request: &HashRequest) -> Result<DigestResult, DigestError> {
    request.validate()?;
    let d = digest::digest(request.algorithm.ring_algorithm(), &request.payload);
    let raw_bytes = d.as_ref().to_vec();
    let formatted = format_digest(&raw_bytes, request.format);
    let legacy_warning = if request.algorithm.is_legacy() {
        Some(
            "SHA-1 is cryptographically weak and should only be used for legacy checksum verification."
                .to_string(),
        )
    } else {
        None
    };

    Ok(DigestResult {
        algorithm: request.algorithm,
        format: request.format,
        input_bytes: request.payload.len(),
        raw_bytes,
        formatted,
        legacy_warning,
    })
}

/// Computes digests for multiple algorithms over the same byte slice.
pub fn compute_batch(
    algorithms: &[HashAlgorithm],
    payload: &[u8],
    format: DigestFormat,
    max_input_bytes: usize,
) -> Result<BatchDigestResult, DigestError> {
    if payload.len() > max_input_bytes {
        return Err(DigestError::InputTooLarge {
            actual: payload.len(),
            max: max_input_bytes,
        });
    }

    let mut digests = Vec::with_capacity(algorithms.len());
    for &algorithm in algorithms {
        let req = HashRequest {
            algorithm,
            format,
            payload: payload.to_vec(),
            max_input_bytes,
        };
        digests.push(compute_digest(&req)?);
    }

    Ok(BatchDigestResult {
        input_bytes: payload.len(),
        digests,
    })
}

/// Computes a digest and compares it in constant time against an expected hex string.
pub fn verify_digest_hex(
    algorithm: HashAlgorithm,
    payload: &[u8],
    expected_hex: &str,
) -> Result<DigestComparison, DigestError> {
    let expected_bytes = decode_hex(expected_hex)?;
    let expected_len = algorithm.output_len_bytes();
    if expected_bytes.len() != expected_len {
        return Err(DigestError::InvalidExpectedDigest {
            expected_len,
            actual_len: expected_bytes.len(),
        });
    }

    let actual_digest = digest::digest(algorithm.ring_algorithm(), payload);
    let actual_bytes = actual_digest.as_ref();
    let matches = actual_bytes.len() == expected_bytes.len()
        && actual_bytes
            .iter()
            .zip(expected_bytes.iter())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0;

    Ok(DigestComparison {
        algorithm,
        expected_hex: encode_hex(&expected_bytes, false),
        actual_hex: encode_hex(actual_bytes, false),
        matches,
    })
}

/// Computes an HMAC tag for `payload` using `key` and `algorithm`.
pub fn compute_hmac(
    algorithm: HashAlgorithm,
    key: &HmacKey,
    payload: &[u8],
    format: DigestFormat,
) -> Result<HmacResult, DigestError> {
    let hmac_alg = algorithm.hmac_algorithm()?;
    let signing_key = hmac::Key::new(hmac_alg, &key.bytes);
    let tag = hmac::sign(&signing_key, payload);
    let raw_bytes = tag.as_ref().to_vec();
    let formatted = format_digest(&raw_bytes, format);

    Ok(HmacResult {
        algorithm,
        format,
        input_bytes: payload.len(),
        raw_bytes,
        formatted,
    })
}

/// Verifies an HMAC tag in constant time against an expected hexadecimal tag.
pub fn verify_hmac_hex(
    algorithm: HashAlgorithm,
    key: &HmacKey,
    payload: &[u8],
    expected_hex: &str,
) -> Result<bool, DigestError> {
    let hmac_alg = algorithm.hmac_algorithm()?;
    let expected_bytes = decode_hex(expected_hex)?;
    let expected_len = algorithm.output_len_bytes();
    if expected_bytes.len() != expected_len {
        return Err(DigestError::InvalidExpectedDigest {
            expected_len,
            actual_len: expected_bytes.len(),
        });
    }
    let verification_key = hmac::Key::new(hmac_alg, &key.bytes);
    Ok(hmac::verify(&verification_key, payload, &expected_bytes).is_ok())
}

/// Parses an omnibox query such as `"sha256: hello"`, `"hash sha512 hello"`, or
/// `"verify sha256 <hex> <text>"`. Returns `Ok(None)` if the query is not a digest command.
pub fn parse_omnibox_digest_query(query: &str) -> Result<Option<ParsedDigestQuery>, DigestError> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    // Check `verify <alg> <expected_hex> <text>`
    if let Some(rest) = strip_prefix_ascii_ci(trimmed, "verify ") {
        let mut parts = rest.trim_start().splitn(3, char::is_whitespace);
        let alg_str = parts.next().unwrap_or_default();
        let expected_hex = parts.next().unwrap_or_default();
        let text = parts.next().unwrap_or_default();
        if alg_str.is_empty() || expected_hex.is_empty() {
            return Ok(None);
        }
        let algorithm = HashAlgorithm::from_str_name(alg_str)?;
        return Ok(Some(ParsedDigestQuery::Verify {
            algorithm,
            expected_hex: expected_hex.to_string(),
            text: text.to_string(),
        }));
    }

    // Check `hash <alg> <text>`
    if let Some(rest) = strip_prefix_ascii_ci(trimmed, "hash ") {
        let mut parts = rest.trim_start().splitn(2, char::is_whitespace);
        let alg_str = parts.next().unwrap_or_default();
        let text = parts.next().unwrap_or_default();
        if alg_str.is_empty() {
            return Ok(None);
        }
        let algorithm = HashAlgorithm::from_str_name(alg_str)?;
        return Ok(Some(ParsedDigestQuery::Compute {
            algorithm,
            text: text.to_string(),
        }));
    }

    // Check `<alg>: <text>`
    if let Some((prefix, rest)) = trimmed.split_once(':') {
        if let Ok(algorithm) = HashAlgorithm::from_str_name(prefix) {
            return Ok(Some(ParsedDigestQuery::Compute {
                algorithm,
                text: rest.trim_start().to_string(),
            }));
        }
    }

    Ok(None)
}

/// Formats raw digest bytes according to [`DigestFormat`].
pub fn format_digest(bytes: &[u8], format: DigestFormat) -> String {
    match format {
        DigestFormat::HexLower => encode_hex(bytes, false),
        DigestFormat::HexUpper => encode_hex(bytes, true),
        DigestFormat::HexColon => encode_hex_colon(bytes),
        DigestFormat::Base64 => encode_base64(bytes),
    }
}

/// Serializes a [`DigestResult`] to pretty-printed JSON.
pub fn serialize_result_json(result: &DigestResult) -> Result<String, DigestError> {
    Ok(serde_json::to_string_pretty(result)?)
}

/// Deserializes a [`DigestResult`] from JSON.
pub fn deserialize_result_json(json: &str) -> Result<DigestResult, DigestError> {
    Ok(serde_json::from_str(json)?)
}

fn strip_prefix_ascii_ci<'a>(input: &'a str, prefix: &str) -> Option<&'a str> {
    let head = input.as_bytes().get(..prefix.len())?;
    if head.eq_ignore_ascii_case(prefix.as_bytes()) {
        input.get(prefix.len()..)
    } else {
        None
    }
}

fn decode_hex(input: &str) -> Result<Vec<u8>, DigestError> {
    let trimmed = input.trim();
    let without_prefix = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);

    let cleaned: String = without_prefix
        .chars()
        .filter(|c| !c.is_ascii_whitespace() && *c != ':')
        .collect();

    if !cleaned.len().is_multiple_of(2) {
        return Err(DigestError::InvalidHexInput(
            "hex string must have an even number of digits".to_string(),
        ));
    }

    let mut out = Vec::with_capacity(cleaned.len() / 2);
    let bytes = cleaned.as_bytes();
    for pair in bytes.as_chunks::<2>().0 {
        let hi = hex_nibble(pair[0])?;
        let lo = hex_nibble(pair[1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_nibble(byte: u8) -> Result<u8, DigestError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(DigestError::InvalidHexInput(format!(
            "unexpected character '{}'",
            byte as char
        ))),
    }
}

fn encode_hex(bytes: &[u8], uppercase: bool) -> String {
    const LOWER: &[u8; 16] = b"0123456789abcdef";
    const UPPER: &[u8; 16] = b"0123456789ABCDEF";
    let alphabet = if uppercase { UPPER } else { LOWER };

    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(alphabet[(b >> 4) as usize] as char);
        out.push(alphabet[(b & 0x0f) as usize] as char);
    }
    out
}

fn encode_hex_colon(bytes: &[u8]) -> String {
    const LOWER: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len().saturating_mul(3));
    for (idx, &b) in bytes.iter().enumerate() {
        if idx > 0 {
            out.push(':');
        }
        out.push(LOWER[(b >> 4) as usize] as char);
        out.push(LOWER[(b & 0x0f) as usize] as char);
    }
    out
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    if bytes.is_empty() {
        return String::new();
    }

    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };

        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((n >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(n & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nist_sha256_and_empty_input() {
        let empty_req = HashRequest::from_utf8(HashAlgorithm::Sha256, "");
        let empty_res = compute_digest(&empty_req).expect("empty sha256 should succeed");
        assert_eq!(
            empty_res.formatted,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(empty_res.input_bytes, 0);
        assert!(empty_res.legacy_warning.is_none());

        let abc_req = HashRequest::from_utf8(HashAlgorithm::Sha256, "abc")
            .with_format(DigestFormat::HexUpper);
        let abc_res = compute_digest(&abc_req).expect("abc sha256 should succeed");
        assert_eq!(
            abc_res.formatted,
            "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD"
        );
    }

    #[test]
    fn test_all_algorithms_batch_and_formats() {
        let batch = compute_batch(
            &HashAlgorithm::ALL,
            b"search-linux",
            DigestFormat::Base64,
            DEFAULT_MAX_INPUT_BYTES,
        )
        .expect("batch digest should succeed");

        assert_eq!(batch.input_bytes, 12);
        assert_eq!(batch.digests.len(), 5);
        assert_eq!(batch.digests[0].raw_bytes.len(), 32);
        assert_eq!(batch.digests[1].raw_bytes.len(), 48);
        assert_eq!(batch.digests[2].raw_bytes.len(), 64);
        assert_eq!(batch.digests[3].raw_bytes.len(), 32);
        assert_eq!(batch.digests[4].raw_bytes.len(), 20);
        assert!(batch.digests[4].legacy_warning.is_some());

        let colon = format_digest(&[0x0a, 0xbc, 0xff], DigestFormat::HexColon);
        assert_eq!(colon, "0a:bc:ff");
        let b64 = format_digest(b"Man", DigestFormat::Base64);
        assert_eq!(b64, "TWFu");
        let b64_pad = format_digest(b"Ma", DigestFormat::Base64);
        assert_eq!(b64_pad, "TWE=");
    }

    #[test]
    fn test_verify_digest_hex_and_errors() {
        let ok = verify_digest_hex(
            HashAlgorithm::Sha256,
            b"abc",
            "0xBA78:16BF:8F01:CFEA:4141:40DE:5DAE:2223:B003:61A3:9617:7A9C:B410:FF61:F200:15AD",
        )
        .expect("verification should succeed");
        assert!(ok.matches);

        let mismatch = verify_digest_hex(
            HashAlgorithm::Sha256,
            b"abd",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        )
        .expect("mismatch should still return comparison");
        assert!(!mismatch.matches);

        let wrong_len = verify_digest_hex(HashAlgorithm::Sha256, b"abc", "deadbeef");
        assert!(matches!(
            wrong_len,
            Err(DigestError::InvalidExpectedDigest {
                expected_len: 32,
                actual_len: 4
            })
        ));

        let oversized = HashRequest::from_bytes(HashAlgorithm::Sha256, vec![1, 2, 3, 4])
            .with_max_input_bytes(2);
        assert!(matches!(
            compute_digest(&oversized),
            Err(DigestError::InputTooLarge { actual: 4, max: 2 })
        ));
    }

    #[test]
    fn test_hmac_and_serialization_roundtrip() {
        assert!(matches!(
            HmacKey::new(vec![]),
            Err(DigestError::EmptyHmacKey)
        ));

        let key = HmacKey::new(b"super-secret-key".to_vec()).expect("valid key");
        let tag = compute_hmac(
            HashAlgorithm::Sha256,
            &key,
            b"payload-data",
            DigestFormat::HexLower,
        )
        .expect("hmac should succeed");
        assert_eq!(tag.raw_bytes.len(), 32);

        let verified =
            verify_hmac_hex(HashAlgorithm::Sha256, &key, b"payload-data", &tag.formatted)
                .expect("hmac verify should succeed");
        assert!(verified);

        let req = HashRequest::from_hex(HashAlgorithm::Sha512_256, "0xdeadbeef")
            .expect("hex input should parse");
        let res = compute_digest(&req).expect("digest should succeed");
        let json = serialize_result_json(&res).expect("serialize should succeed");
        let roundtrip = deserialize_result_json(&json).expect("deserialize should succeed");
        assert_eq!(res, roundtrip);
    }

    #[test]
    fn test_omnibox_query_parsing() {
        let q1 = parse_omnibox_digest_query("sha256: hello world")
            .expect("valid query")
            .expect("should match");
        assert_eq!(
            q1,
            ParsedDigestQuery::Compute {
                algorithm: HashAlgorithm::Sha256,
                text: "hello world".to_string(),
            }
        );

        let q2 = parse_omnibox_digest_query("HASH sha-512 test")
            .expect("valid query")
            .expect("should match");
        assert_eq!(
            q2,
            ParsedDigestQuery::Compute {
                algorithm: HashAlgorithm::Sha512,
                text: "test".to_string(),
            }
        );

        let q3 = parse_omnibox_digest_query("verify sha256 deadbeef hello")
            .expect("valid query")
            .expect("should match");
        assert_eq!(
            q3,
            ParsedDigestQuery::Verify {
                algorithm: HashAlgorithm::Sha256,
                expected_hex: "deadbeef".to_string(),
                text: "hello".to_string(),
            }
        );

        assert!(parse_omnibox_digest_query("open settings")
            .expect("non-hash query")
            .is_none());
        assert!(matches!(
            parse_omnibox_digest_query("hash md5 hello"),
            Err(DigestError::UnsupportedAlgorithm(_))
        ));
    }
}
