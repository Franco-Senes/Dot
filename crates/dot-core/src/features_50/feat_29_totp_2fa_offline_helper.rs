//! Offline TOTP (RFC 6238) and 2FA secret vault helper.
//!
//! Provides pure-Rust, offline-first Time-based One-Time Password generation,
//! verification with configurable clock-skew tolerance, RFC 4648 Base32
//! decoding/encoding, `otpauth://totp/...` URI parsing/formatting, and
//! zeroizing secret storage with JSON serialization.

use std::fmt;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use ring::hmac;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;
use zeroize::Zeroize;

/// Minimum allowed TOTP code length in digits.
pub const MIN_TOTP_DIGITS: u8 = 6;
/// Maximum allowed TOTP code length in digits.
pub const MAX_TOTP_DIGITS: u8 = 8;
/// Default TOTP code length in digits.
pub const DEFAULT_TOTP_DIGITS: u8 = 6;
/// Default TOTP time-step period in seconds.
pub const DEFAULT_TOTP_PERIOD_SECS: u64 = 30;
/// Maximum allowed TOTP time-step period in seconds (1 hour).
pub const MAX_TOTP_PERIOD_SECS: u64 = 3600;
/// Maximum allowed clock-skew steps during verification.
pub const MAX_SKEW_STEPS: u32 = 10;

/// Errors produced by the offline TOTP 2FA helper.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TotpError {
    #[error("totp secret cannot be empty")]
    EmptySecret,

    #[error("invalid base32 secret: {0}")]
    InvalidBase32(String),

    #[error("invalid digit count {0}: must be between {MIN_TOTP_DIGITS} and {MAX_TOTP_DIGITS}")]
    InvalidDigits(u8),

    #[error("invalid period {0}s: must be between 1 and {MAX_TOTP_PERIOD_SECS}")]
    InvalidPeriod(u64),

    #[error("unsupported totp algorithm: {0}")]
    UnsupportedAlgorithm(String),

    #[error("invalid otpauth uri: {0}")]
    InvalidUri(String),

    #[error("account id cannot be empty")]
    EmptyAccountId,

    #[error("account label cannot be empty")]
    EmptyLabel,

    #[error("duplicate account id: {0}")]
    DuplicateAccountId(String),

    #[error("account not found: {0}")]
    AccountNotFound(String),

    #[error("skew steps {0} exceeds maximum {MAX_SKEW_STEPS}")]
    SkewTooLarge(u32),

    #[error("system clock error: {0}")]
    ClockError(String),

    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Supported HMAC hash algorithms for RFC 6238 TOTP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum TotpAlgorithm {
    #[default]
    Sha1,
    Sha256,
    Sha512,
}

impl TotpAlgorithm {
    /// Returns the canonical uppercase algorithm name used in `otpauth://` URIs.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sha1 => "SHA1",
            Self::Sha256 => "SHA256",
            Self::Sha512 => "SHA512",
        }
    }

    fn hmac_algorithm(self) -> hmac::Algorithm {
        match self {
            Self::Sha1 => hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY,
            Self::Sha256 => hmac::HMAC_SHA256,
            Self::Sha512 => hmac::HMAC_SHA512,
        }
    }
}

impl fmt::Display for TotpAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TotpAlgorithm {
    type Err = TotpError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "SHA1" | "SHA-1" => Ok(Self::Sha1),
            "SHA256" | "SHA-256" => Ok(Self::Sha256),
            "SHA512" | "SHA-512" => Ok(Self::Sha512),
            other => Err(TotpError::UnsupportedAlgorithm(other.to_string())),
        }
    }
}

/// Raw TOTP secret key bytes that are zeroized on drop.
#[derive(Clone, PartialEq, Eq)]
pub struct TotpSecret {
    bytes: Vec<u8>,
}

impl TotpSecret {
    /// Creates a `TotpSecret` from raw key bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, TotpError> {
        if bytes.is_empty() {
            return Err(TotpError::EmptySecret);
        }
        Ok(Self { bytes })
    }

    /// Decodes a Base32-encoded secret (RFC 4648, case-insensitive, ignoring spaces and hyphens).
    pub fn from_base32(encoded: &str) -> Result<Self, TotpError> {
        let bytes = decode_base32(encoded)?;
        Self::from_bytes(bytes)
    }

    /// Returns the secret encoded as unpadded uppercase RFC 4648 Base32.
    pub fn to_base32(&self) -> String {
        encode_base32(&self.bytes)
    }

    /// Exposes the raw key bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl fmt::Debug for TotpSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TotpSecret")
            .field("len", &self.bytes.len())
            .finish()
    }
}

impl Drop for TotpSecret {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

/// A generated TOTP code with its validity window metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TotpCode {
    pub code: String,
    pub period_secs: u64,
    pub valid_from_unix: u64,
    pub expires_at_unix: u64,
    pub remaining_secs: u64,
}

/// Stored 2FA account configuration for offline TOTP generation.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TotpAccount {
    pub id: String,
    pub label: String,
    pub issuer: Option<String>,
    pub secret_base32: String,
    pub algorithm: TotpAlgorithm,
    pub digits: u8,
    pub period_secs: u64,
    pub created_at_unix: u64,
    pub notes: Option<String>,
}

impl fmt::Debug for TotpAccount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TotpAccount")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("issuer", &self.issuer)
            .field("secret_base32", &"[REDACTED]")
            .field("algorithm", &self.algorithm)
            .field("digits", &self.digits)
            .field("period_secs", &self.period_secs)
            .field("created_at_unix", &self.created_at_unix)
            .field("notes", &self.notes)
            .finish()
    }
}

impl Drop for TotpAccount {
    fn drop(&mut self) {
        self.secret_base32.zeroize();
    }
}

impl TotpAccount {
    /// Creates and validates a new `TotpAccount`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        issuer: Option<String>,
        secret_base32: impl Into<String>,
        algorithm: TotpAlgorithm,
        digits: u8,
        period_secs: u64,
        created_at_unix: u64,
    ) -> Result<Self, TotpError> {
        let account = Self {
            id: id.into().trim().to_string(),
            label: label.into().trim().to_string(),
            issuer: issuer
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            secret_base32: normalize_base32_input(&secret_base32.into()),
            algorithm,
            digits,
            period_secs,
            created_at_unix,
            notes: None,
        };
        account.validate()?;
        Ok(account)
    }

    /// Parses an `otpauth://totp/...` URI into a validated `TotpAccount`.
    pub fn from_otpauth_uri(
        id: impl Into<String>,
        uri: &str,
        created_at_unix: u64,
    ) -> Result<Self, TotpError> {
        let parsed = Url::parse(uri.trim())
            .map_err(|e| TotpError::InvalidUri(format!("malformed url: {e}")))?;

        if parsed.scheme() != "otpauth" {
            return Err(TotpError::InvalidUri(format!(
                "expected otpauth scheme, got {}",
                parsed.scheme()
            )));
        }

        let host = parsed.host_str().unwrap_or_default();
        if !host.eq_ignore_ascii_case("totp") {
            return Err(TotpError::InvalidUri(format!(
                "only totp is supported, got '{host}'"
            )));
        }

        let raw_path = parsed.path().trim_start_matches('/');
        let decoded_path = percent_decode_str(raw_path)?;
        if decoded_path.is_empty() {
            return Err(TotpError::InvalidUri(
                "missing account label in path".into(),
            ));
        }

        let (path_issuer, label) = match decoded_path.split_once(':') {
            Some((iss, lbl)) => (
                Some(iss.trim().to_string()).filter(|s| !s.is_empty()),
                lbl.trim().to_string(),
            ),
            None => (None, decoded_path.trim().to_string()),
        };

        let mut secret_opt: Option<String> = None;
        let mut query_issuer: Option<String> = None;
        let mut algorithm = TotpAlgorithm::Sha1;
        let mut digits = DEFAULT_TOTP_DIGITS;
        let mut period_secs = DEFAULT_TOTP_PERIOD_SECS;

        for (key, value) in parsed.query_pairs() {
            match key.as_ref() {
                "secret" => secret_opt = Some(value.into_owned()),
                "issuer" => {
                    let trimmed = value.trim().to_string();
                    if !trimmed.is_empty() {
                        query_issuer = Some(trimmed);
                    }
                }
                "algorithm" => algorithm = TotpAlgorithm::from_str(&value)?,
                "digits" => {
                    digits = value.parse::<u8>().map_err(|_| {
                        TotpError::InvalidUri(format!("invalid digits parameter: {value}"))
                    })?;
                }
                "period" => {
                    period_secs = value.parse::<u64>().map_err(|_| {
                        TotpError::InvalidUri(format!("invalid period parameter: {value}"))
                    })?;
                }
                _ => {}
            }
        }

        let secret = secret_opt
            .ok_or_else(|| TotpError::InvalidUri("missing required secret parameter".into()))?;
        let issuer = query_issuer.or(path_issuer);

        Self::new(
            id,
            label,
            issuer,
            secret,
            algorithm,
            digits,
            period_secs,
            created_at_unix,
        )
    }

    /// Formats this account as an `otpauth://totp/...` URI.
    pub fn to_otpauth_uri(&self) -> Result<String, TotpError> {
        self.validate()?;
        let label_part = match &self.issuer {
            Some(iss) => format!("{}:{}", iss, self.label),
            None => self.label.clone(),
        };
        let mut url = Url::parse("otpauth://totp/")
            .map_err(|e| TotpError::InvalidUri(format!("failed to build base uri: {e}")))?;
        {
            let mut segs = url
                .path_segments_mut()
                .map_err(|_| TotpError::InvalidUri("cannot modify otpauth path".into()))?;
            segs.push(&label_part);
        }
        {
            let mut qp = url.query_pairs_mut();
            qp.append_pair("secret", &self.secret_base32);
            if let Some(iss) = &self.issuer {
                qp.append_pair("issuer", iss);
            }
            qp.append_pair("algorithm", self.algorithm.as_str());
            qp.append_pair("digits", &self.digits.to_string());
            qp.append_pair("period", &self.period_secs.to_string());
        }
        Ok(url.into())
    }

    /// Validates all account fields and the Base32 secret.
    pub fn validate(&self) -> Result<(), TotpError> {
        if self.id.trim().is_empty() {
            return Err(TotpError::EmptyAccountId);
        }
        if self.label.trim().is_empty() {
            return Err(TotpError::EmptyLabel);
        }
        if !(MIN_TOTP_DIGITS..=MAX_TOTP_DIGITS).contains(&self.digits) {
            return Err(TotpError::InvalidDigits(self.digits));
        }
        if self.period_secs == 0 || self.period_secs > MAX_TOTP_PERIOD_SECS {
            return Err(TotpError::InvalidPeriod(self.period_secs));
        }
        let _secret = TotpSecret::from_base32(&self.secret_base32)?;
        Ok(())
    }

    /// Generates the TOTP code for this account at `unix_timestamp`.
    pub fn generate_code_at(&self, unix_timestamp: u64) -> Result<TotpCode, TotpError> {
        self.validate()?;
        let secret = TotpSecret::from_base32(&self.secret_base32)?;
        let code = generate_totp_at(
            secret.as_bytes(),
            self.algorithm,
            self.digits,
            self.period_secs,
            unix_timestamp,
        )?;
        let step = unix_timestamp / self.period_secs;
        let valid_from_unix = step.saturating_mul(self.period_secs);
        let expires_at_unix = valid_from_unix.saturating_add(self.period_secs);
        let remaining_secs = expires_at_unix.saturating_sub(unix_timestamp);
        Ok(TotpCode {
            code,
            period_secs: self.period_secs,
            valid_from_unix,
            expires_at_unix,
            remaining_secs,
        })
    }

    /// Verifies a candidate TOTP code against this account at `unix_timestamp` with `skew_steps`.
    pub fn verify_code_at(
        &self,
        candidate: &str,
        unix_timestamp: u64,
        skew_steps: u32,
    ) -> Result<bool, TotpError> {
        self.validate()?;
        let secret = TotpSecret::from_base32(&self.secret_base32)?;
        verify_totp_at(
            secret.as_bytes(),
            self.algorithm,
            self.digits,
            self.period_secs,
            candidate,
            unix_timestamp,
            skew_steps,
        )
    }
}

/// Offline vault storing multiple TOTP 2FA accounts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TotpVault {
    pub version: u32,
    pub accounts: Vec<TotpAccount>,
}

impl TotpVault {
    /// Current schema version for serialized vaults.
    pub const CURRENT_VERSION: u32 = 1;

    /// Creates an empty offline TOTP vault.
    pub fn new() -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            accounts: Vec::new(),
        }
    }

    /// Adds a validated account, rejecting duplicate IDs.
    pub fn add_account(&mut self, account: TotpAccount) -> Result<(), TotpError> {
        account.validate()?;
        if self.accounts.iter().any(|a| a.id == account.id) {
            return Err(TotpError::DuplicateAccountId(account.id.clone()));
        }
        self.accounts.push(account);
        Ok(())
    }

    /// Removes an account by ID, returning it if found.
    pub fn remove_account(&mut self, id: &str) -> Result<TotpAccount, TotpError> {
        let pos = self
            .accounts
            .iter()
            .position(|a| a.id == id)
            .ok_or_else(|| TotpError::AccountNotFound(id.to_string()))?;
        Ok(self.accounts.remove(pos))
    }

    /// Looks up an account by ID.
    pub fn get_account(&self, id: &str) -> Option<&TotpAccount> {
        self.accounts.iter().find(|a| a.id == id)
    }

    /// Searches accounts by case-insensitive substring match against `issuer` or `label`.
    pub fn search(&self, query: &str) -> Vec<&TotpAccount> {
        let q = query.trim().to_ascii_lowercase();
        if q.is_empty() {
            return self.accounts.iter().collect();
        }
        self.accounts
            .iter()
            .filter(|a| {
                a.label.to_ascii_lowercase().contains(&q)
                    || a.issuer
                        .as_deref()
                        .map(|iss| iss.to_ascii_lowercase().contains(&q))
                        .unwrap_or(false)
            })
            .collect()
    }

    /// Generates a TOTP code for a specific account at `unix_timestamp`.
    pub fn generate_code(&self, id: &str, unix_timestamp: u64) -> Result<TotpCode, TotpError> {
        let account = self
            .get_account(id)
            .ok_or_else(|| TotpError::AccountNotFound(id.to_string()))?;
        account.generate_code_at(unix_timestamp)
    }

    /// Generates TOTP codes for all accounts in the vault at `unix_timestamp`.
    pub fn generate_all_codes(
        &self,
        unix_timestamp: u64,
    ) -> Result<Vec<(String, TotpCode)>, TotpError> {
        let mut out = Vec::with_capacity(self.accounts.len());
        for account in &self.accounts {
            let code = account.generate_code_at(unix_timestamp)?;
            out.push((account.id.clone(), code));
        }
        Ok(out)
    }

    /// Verifies a candidate code for a specific account at `unix_timestamp`.
    pub fn verify_code(
        &self,
        id: &str,
        candidate: &str,
        unix_timestamp: u64,
        skew_steps: u32,
    ) -> Result<bool, TotpError> {
        let account = self
            .get_account(id)
            .ok_or_else(|| TotpError::AccountNotFound(id.to_string()))?;
        account.verify_code_at(candidate, unix_timestamp, skew_steps)
    }

    /// Serializes the vault to pretty-printed JSON after validating all accounts.
    pub fn to_json(&self) -> Result<String, TotpError> {
        for account in &self.accounts {
            account.validate()?;
        }
        serde_json::to_string_pretty(self).map_err(|e| TotpError::Serialization(e.to_string()))
    }

    /// Deserializes a vault from JSON and validates all accounts and unique IDs.
    pub fn from_json(json: &str) -> Result<Self, TotpError> {
        let vault: Self =
            serde_json::from_str(json).map_err(|e| TotpError::Serialization(e.to_string()))?;
        let mut seen_ids = std::collections::HashSet::with_capacity(vault.accounts.len());
        for account in &vault.accounts {
            account.validate()?;
            if !seen_ids.insert(account.id.as_str()) {
                return Err(TotpError::DuplicateAccountId(account.id.clone()));
            }
        }
        Ok(vault)
    }
}

/// Returns the current UNIX timestamp in seconds.
pub fn current_unix_timestamp() -> Result<u64, TotpError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|e| TotpError::ClockError(e.to_string()))
}

/// Generates an RFC 4226 HOTP code for an arbitrary 64-bit `counter`.
pub fn generate_hotp(
    secret: &[u8],
    algorithm: TotpAlgorithm,
    digits: u8,
    counter: u64,
) -> Result<String, TotpError> {
    if secret.is_empty() {
        return Err(TotpError::EmptySecret);
    }
    if !(MIN_TOTP_DIGITS..=MAX_TOTP_DIGITS).contains(&digits) {
        return Err(TotpError::InvalidDigits(digits));
    }

    let key = hmac::Key::new(algorithm.hmac_algorithm(), secret);
    let tag = hmac::sign(&key, &counter.to_be_bytes());
    let digest = tag.as_ref();

    let offset = (digest[digest.len() - 1] & 0x0f) as usize;
    let bin_code: u32 = ((u32::from(digest[offset]) & 0x7f) << 24)
        | (u32::from(digest[offset + 1]) << 16)
        | (u32::from(digest[offset + 2]) << 8)
        | u32::from(digest[offset + 3]);

    let modulo = 10u32.pow(u32::from(digits));
    let otp = bin_code % modulo;
    Ok(format!("{otp:0width$}", width = usize::from(digits)))
}

/// Generates an RFC 6238 TOTP code for `unix_timestamp` and `period_secs`.
pub fn generate_totp_at(
    secret: &[u8],
    algorithm: TotpAlgorithm,
    digits: u8,
    period_secs: u64,
    unix_timestamp: u64,
) -> Result<String, TotpError> {
    if period_secs == 0 || period_secs > MAX_TOTP_PERIOD_SECS {
        return Err(TotpError::InvalidPeriod(period_secs));
    }
    let counter = unix_timestamp / period_secs;
    generate_hotp(secret, algorithm, digits, counter)
}

/// Verifies a candidate TOTP code in constant time across `[-skew_steps, +skew_steps]`.
pub fn verify_totp_at(
    secret: &[u8],
    algorithm: TotpAlgorithm,
    digits: u8,
    period_secs: u64,
    candidate: &str,
    unix_timestamp: u64,
    skew_steps: u32,
) -> Result<bool, TotpError> {
    if period_secs == 0 || period_secs > MAX_TOTP_PERIOD_SECS {
        return Err(TotpError::InvalidPeriod(period_secs));
    }
    if skew_steps > MAX_SKEW_STEPS {
        return Err(TotpError::SkewTooLarge(skew_steps));
    }
    if secret.is_empty() {
        return Err(TotpError::EmptySecret);
    }
    if !(MIN_TOTP_DIGITS..=MAX_TOTP_DIGITS).contains(&digits) {
        return Err(TotpError::InvalidDigits(digits));
    }

    let trimmed = candidate.trim();
    if trimmed.len() != usize::from(digits) || !trimmed.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(false);
    }

    let base_counter = unix_timestamp / period_secs;
    let skew = u64::from(skew_steps);
    let min_counter = base_counter.saturating_sub(skew);
    let max_counter = base_counter.saturating_add(skew);

    let mut matched = false;
    for counter in min_counter..=max_counter {
        let expected = generate_hotp(secret, algorithm, digits, counter)?;
        if constant_time_eq(expected.as_bytes(), trimmed.as_bytes()) {
            matched = true;
        }
    }
    Ok(matched)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Decodes an RFC 4648 Base32 string (case-insensitive, spaces/hyphens ignored, optional `=`).
pub fn decode_base32(input: &str) -> Result<Vec<u8>, TotpError> {
    let cleaned = normalize_base32_input(input);
    if cleaned.is_empty() {
        return Err(TotpError::EmptySecret);
    }

    let unpadded = cleaned.trim_end_matches('=');
    let pad_len = cleaned.len() - unpadded.len();
    if pad_len > 0 && !cleaned.len().is_multiple_of(8) {
        return Err(TotpError::InvalidBase32(
            "padded base32 length must be a multiple of 8".into(),
        ));
    }
    if unpadded.is_empty() || unpadded.contains('=') {
        return Err(TotpError::InvalidBase32(
            "misplaced padding character".into(),
        ));
    }

    let mut out = Vec::with_capacity((unpadded.len() * 5) / 8);
    let mut buffer: u32 = 0;
    let mut bits_left: u8 = 0;

    for (idx, ch) in unpadded.bytes().enumerate() {
        let val = match ch {
            b'A'..=b'Z' => ch - b'A',
            b'2'..=b'7' => ch - b'2' + 26,
            _ => {
                return Err(TotpError::InvalidBase32(format!(
                    "unexpected character '{}' at index {idx}",
                    ch as char
                )));
            }
        };
        buffer = (buffer << 5) | u32::from(val);
        bits_left += 5;
        if bits_left >= 8 {
            bits_left -= 8;
            out.push(((buffer >> bits_left) & 0xff) as u8);
        }
    }

    if bits_left > 0 {
        let mask = (1u32 << bits_left) - 1;
        if (buffer & mask) != 0 {
            return Err(TotpError::InvalidBase32(
                "non-zero trailing padding bits".into(),
            ));
        }
    }

    Ok(out)
}

/// Encodes raw bytes as unpadded uppercase RFC 4648 Base32.
pub fn encode_base32(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    if bytes.is_empty() {
        return String::new();
    }

    let mut out = String::with_capacity((bytes.len() * 8).div_ceil(5));
    let mut buffer: u32 = 0;
    let mut bits_left: u8 = 0;

    for &byte in bytes {
        buffer = (buffer << 8) | u32::from(byte);
        bits_left += 8;
        while bits_left >= 5 {
            bits_left -= 5;
            let idx = ((buffer >> bits_left) & 0x1f) as usize;
            out.push(ALPHABET[idx] as char);
        }
    }

    if bits_left > 0 {
        let idx = ((buffer << (5 - bits_left)) & 0x1f) as usize;
        out.push(ALPHABET[idx] as char);
    }

    out
}

fn normalize_base32_input(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

fn percent_decode_str(input: &str) -> Result<String, TotpError> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(TotpError::InvalidUri("truncated percent-encoding".into()));
            }
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3])
                .map_err(|_| TotpError::InvalidUri("invalid percent-encoding".into()))?;
            let val = u8::from_str_radix(hex, 16)
                .map_err(|_| TotpError::InvalidUri("invalid hex in percent-encoding".into()))?;
            out.push(val);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| TotpError::InvalidUri("invalid utf-8 in path".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc6238_test_vectors() {
        // RFC 6238 Appendix B test secrets (ASCII).
        let secret_sha1 = b"12345678901234567890";
        let secret_sha256 = b"12345678901234567890123456789012";
        let secret_sha512 = b"1234567890123456789012345678901234567890123456789012345678901234";

        let vectors: &[(u64, &str, &str, &str)] = &[
            (59, "94287082", "46119246", "90693936"),
            (1_111_111_109, "07081804", "68084774", "25091201"),
            (1_111_111_111, "14050471", "67062674", "99943326"),
            (1_234_567_890, "89005924", "91819424", "93441116"),
            (2_000_000_000, "69279037", "90698825", "38618901"),
            (20_000_000_000, "65353130", "77737706", "47863826"),
        ];

        for &(ts, exp_sha1, exp_sha256, exp_sha512) in vectors {
            assert_eq!(
                generate_totp_at(secret_sha1, TotpAlgorithm::Sha1, 8, 30, ts).unwrap(),
                exp_sha1
            );
            assert_eq!(
                generate_totp_at(secret_sha256, TotpAlgorithm::Sha256, 8, 30, ts).unwrap(),
                exp_sha256
            );
            assert_eq!(
                generate_totp_at(secret_sha512, TotpAlgorithm::Sha512, 8, 30, ts).unwrap(),
                exp_sha512
            );
        }
    }

    #[test]
    fn base32_round_trip_and_validation() {
        let raw = b"12345678901234567890";
        let encoded = encode_base32(raw);
        assert_eq!(encoded, "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
        assert_eq!(decode_base32(&encoded).unwrap(), raw);

        // Lowercase, whitespace, hyphens, and valid padding should all decode.
        let messy = "gezd-gnbv gy3t qojq gezd gnbv gy3t qojq";
        assert_eq!(decode_base32(messy).unwrap(), raw);
        assert_eq!(decode_base32("MZXW6===").unwrap(), b"foo");

        // Invalid inputs.
        assert!(matches!(decode_base32(""), Err(TotpError::EmptySecret)));
        assert!(matches!(
            decode_base32("INVALID1"),
            Err(TotpError::InvalidBase32(_))
        ));
        assert!(matches!(
            decode_base32("MZXW6="),
            Err(TotpError::InvalidBase32(_))
        ));
    }

    #[test]
    fn otpauth_uri_parse_and_round_trip() {
        let uri = "otpauth://totp/Search%20Linux:alice%40example.com?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&issuer=Search%20Linux&algorithm=SHA256&digits=6&period=30";
        let acct = TotpAccount::from_otpauth_uri("acc-1", uri, 1_700_000_000).unwrap();
        assert_eq!(acct.label, "alice@example.com");
        assert_eq!(acct.issuer.as_deref(), Some("Search Linux"));
        assert_eq!(acct.algorithm, TotpAlgorithm::Sha256);
        assert_eq!(acct.digits, 6);
        assert_eq!(acct.period_secs, 30);

        let regenerated = acct.to_otpauth_uri().unwrap();
        let reparsed = TotpAccount::from_otpauth_uri("acc-1", &regenerated, 1_700_000_000).unwrap();
        assert_eq!(acct, reparsed);

        // Reject hotp and missing secret.
        assert!(matches!(
            TotpAccount::from_otpauth_uri("x", "otpauth://hotp/test?secret=JBSWY3DPEHPK3PXP", 0),
            Err(TotpError::InvalidUri(_))
        ));
        assert!(matches!(
            TotpAccount::from_otpauth_uri("x", "otpauth://totp/test", 0),
            Err(TotpError::InvalidUri(_))
        ));
    }

    #[test]
    fn vault_crud_verification_skew_and_json() {
        let mut vault = TotpVault::new();
        let acct = TotpAccount::new(
            "gh-1",
            "dev@example.com",
            Some("GitHub".to_string()),
            "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ",
            TotpAlgorithm::Sha1,
            6,
            30,
            1_700_000_000,
        )
        .unwrap();
        vault.add_account(acct.clone()).unwrap();
        assert!(matches!(
            vault.add_account(acct),
            Err(TotpError::DuplicateAccountId(_))
        ));

        let code = vault.generate_code("gh-1", 59).unwrap();
        assert_eq!(code.code, "287082");
        assert_eq!(code.valid_from_unix, 30);
        assert_eq!(code.expires_at_unix, 60);
        assert_eq!(code.remaining_secs, 1);

        // Exact match at t=59, and valid within 1 step at t=65 with skew=1, invalid with skew=0.
        assert!(vault.verify_code("gh-1", "287082", 59, 0).unwrap());
        assert!(!vault.verify_code("gh-1", "287082", 65, 0).unwrap());
        assert!(vault.verify_code("gh-1", "287082", 65, 1).unwrap());
        assert!(!vault.verify_code("gh-1", "00000", 59, 1).unwrap());

        assert_eq!(vault.search("github").len(), 1);
        assert_eq!(vault.search("gitlab").len(), 0);

        let json = vault.to_json().unwrap();
        let restored = TotpVault::from_json(&json).unwrap();
        assert_eq!(vault, restored);

        let removed = vault.remove_account("gh-1").unwrap();
        assert_eq!(removed.id, "gh-1");
        assert!(vault.get_account("gh-1").is_none());
    }
}
