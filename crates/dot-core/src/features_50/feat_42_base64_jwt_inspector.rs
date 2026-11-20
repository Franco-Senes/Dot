use ring::hmac;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use time::OffsetDateTime;
use zeroize::Zeroizing;

/// Errors produced by Base64 and JWT inspection or verification.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum InspectorError {
    #[error("input is empty")]
    EmptyInput,

    #[error("invalid base64 length: {len}")]
    InvalidBase64Length { len: usize },

    #[error("invalid base64 byte 0x{byte:02x} at offset {offset}")]
    InvalidBase64Byte { byte: u8, offset: usize },

    #[error("invalid base64 padding at offset {offset}")]
    InvalidPadding { offset: usize },

    #[error("invalid JWT structure: expected {expected_parts} dot-separated parts, found {actual_parts}")]
    InvalidJwtStructure {
        expected_parts: usize,
        actual_parts: usize,
    },

    #[error("invalid JWT header: {0}")]
    InvalidJwtHeader(String),

    #[error("invalid JWT claims: {0}")]
    InvalidJwtClaims(String),

    #[error("invalid JWT signature encoding: {0}")]
    InvalidSignatureFormat(String),

    #[error("unsupported JWT algorithm for HMAC operation: {0}")]
    UnsupportedAlgorithm(String),

    #[error("invalid key: {0}")]
    InvalidKey(String),

    #[error("JSON parse or serialization error: {0}")]
    JsonError(String),
}

impl From<serde_json::Error> for InspectorError {
    fn from(err: serde_json::Error) -> Self {
        Self::JsonError(err.to_string())
    }
}

/// Supported Base64 encoding variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Base64Variant {
    Standard,
    StandardNoPad,
    UrlSafe,
    UrlSafeNoPad,
}

impl Base64Variant {
    pub fn is_url_safe(self) -> bool {
        matches!(self, Self::UrlSafe | Self::UrlSafeNoPad)
    }

    pub fn uses_padding(self) -> bool {
        matches!(self, Self::Standard | Self::UrlSafe)
    }
}

/// Options controlling Base64 decoding and inspection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Base64DecodeOptions {
    pub strip_whitespace: bool,
    pub allow_missing_padding: bool,
    pub variant: Option<Base64Variant>,
}

impl Default for Base64DecodeOptions {
    fn default() -> Self {
        Self {
            strip_whitespace: true,
            allow_missing_padding: true,
            variant: None,
        }
    }
}

/// Result of inspecting a Base64-encoded payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Base64Inspection {
    pub detected_variant: Base64Variant,
    pub raw_len: usize,
    pub decoded_bytes_len: usize,
    pub decoded_hex: String,
    pub decoded_utf8: Option<String>,
    pub is_valid_json: bool,
    pub pretty_json: Option<String>,
}

const STANDARD_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL_SAFE_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Encodes raw bytes into Base64 using the specified variant.
pub fn encode_base64(input: &[u8], variant: Base64Variant) -> String {
    let alphabet = if variant.is_url_safe() {
        URL_SAFE_ALPHABET
    } else {
        STANDARD_ALPHABET
    };
    let pad = variant.uses_padding();

    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let (chunks, rem) = input.as_chunks::<3>();

    for chunk in chunks {
        let b0 = chunk[0] as usize;
        let b1 = chunk[1] as usize;
        let b2 = chunk[2] as usize;

        out.push(alphabet[(b0 >> 2) & 0x3f] as char);
        out.push(alphabet[((b0 & 0x03) << 4) | ((b1 >> 4) & 0x0f)] as char);
        out.push(alphabet[((b1 & 0x0f) << 2) | ((b2 >> 6) & 0x03)] as char);
        out.push(alphabet[b2 & 0x3f] as char);
    }

    match rem.len() {
        1 => {
            let b0 = rem[0] as usize;
            out.push(alphabet[(b0 >> 2) & 0x3f] as char);
            out.push(alphabet[(b0 & 0x03) << 4] as char);
            if pad {
                out.push('=');
                out.push('=');
            }
        }
        2 => {
            let b0 = rem[0] as usize;
            let b1 = rem[1] as usize;
            out.push(alphabet[(b0 >> 2) & 0x3f] as char);
            out.push(alphabet[((b0 & 0x03) << 4) | ((b1 >> 4) & 0x0f)] as char);
            out.push(alphabet[(b1 & 0x0f) << 2] as char);
            if pad {
                out.push('=');
            }
        }
        _ => {}
    }

    out
}

fn decode_sextet(byte: u8, url_safe: bool) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' if !url_safe => Some(62),
        b'/' if !url_safe => Some(63),
        b'-' if url_safe => Some(62),
        b'_' if url_safe => Some(63),
        _ => None,
    }
}

fn detect_base64_variant(cleaned: &[u8]) -> Base64Variant {
    let has_url_chars = cleaned.iter().any(|&b| b == b'-' || b == b'_');
    let has_padding = cleaned.ends_with(b"=");
    match (has_url_chars, has_padding) {
        (true, true) => Base64Variant::UrlSafe,
        (true, false) => Base64Variant::UrlSafeNoPad,
        (false, true) => Base64Variant::Standard,
        (false, false) => Base64Variant::StandardNoPad,
    }
}

/// Decodes a Base64-encoded string, returning the decoded bytes and the variant used.
pub fn decode_base64(
    input: &str,
    options: &Base64DecodeOptions,
) -> Result<(Vec<u8>, Base64Variant), InspectorError> {
    let cleaned: Vec<u8> = if options.strip_whitespace {
        input.bytes().filter(|b| !b.is_ascii_whitespace()).collect()
    } else {
        input.as_bytes().to_vec()
    };

    if cleaned.is_empty() {
        return Err(InspectorError::EmptyInput);
    }

    let variant = options
        .variant
        .unwrap_or_else(|| detect_base64_variant(&cleaned));

    if variant.uses_padding() && !options.allow_missing_padding && !cleaned.len().is_multiple_of(4)
    {
        return Err(InspectorError::InvalidBase64Length { len: cleaned.len() });
    }

    let pad_count = cleaned.iter().rev().take_while(|&&b| b == b'=').count();
    if pad_count > 2 {
        return Err(InspectorError::InvalidPadding {
            offset: cleaned.len().saturating_sub(pad_count),
        });
    }
    if pad_count > 0 && !variant.uses_padding() && options.variant.is_some() {
        return Err(InspectorError::InvalidPadding {
            offset: cleaned.len() - pad_count,
        });
    }

    let data_len = cleaned.len() - pad_count;
    if data_len % 4 == 1 {
        return Err(InspectorError::InvalidBase64Length { len: cleaned.len() });
    }
    if pad_count > 0 && !cleaned.len().is_multiple_of(4) {
        return Err(InspectorError::InvalidPadding {
            offset: cleaned.len() - pad_count,
        });
    }

    let url_safe = variant.is_url_safe();
    let mut sextets = Vec::with_capacity(data_len);
    for (idx, &byte) in cleaned[..data_len].iter().enumerate() {
        if byte == b'=' {
            return Err(InspectorError::InvalidPadding { offset: idx });
        }
        let val = decode_sextet(byte, url_safe)
            .ok_or(InspectorError::InvalidBase64Byte { byte, offset: idx })?;
        sextets.push(val);
    }

    let mut out = Vec::with_capacity((sextets.len() * 3) / 4);
    let (chunks, rem) = sextets.as_chunks::<4>();
    for chunk in chunks {
        out.push((chunk[0] << 2) | (chunk[1] >> 4));
        out.push((chunk[1] << 4) | (chunk[2] >> 2));
        out.push((chunk[2] << 6) | chunk[3]);
    }

    match rem.len() {
        2 => {
            out.push((rem[0] << 2) | (rem[1] >> 4));
        }
        3 => {
            out.push((rem[0] << 2) | (rem[1] >> 4));
            out.push((rem[1] << 4) | (rem[2] >> 2));
        }
        _ => {}
    }

    Ok((out, variant))
}

fn to_hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// Decodes and inspects a Base64 string, extracting UTF-8 and JSON views when valid.
pub fn inspect_base64(
    input: &str,
    options: &Base64DecodeOptions,
) -> Result<Base64Inspection, InspectorError> {
    let (decoded, variant) = decode_base64(input, options)?;
    let decoded_hex = to_hex_lower(&decoded);
    let decoded_bytes_len = decoded.len();

    let decoded_utf8 = String::from_utf8(decoded).ok();
    let (is_valid_json, pretty_json) = match &decoded_utf8 {
        Some(text) => match serde_json::from_str::<Value>(text) {
            Ok(val) => {
                let pretty = serde_json::to_string_pretty(&val).ok();
                (true, pretty)
            }
            Err(_) => (false, None),
        },
        None => (false, None),
    };

    Ok(Base64Inspection {
        detected_variant: variant,
        raw_len: input.len(),
        decoded_bytes_len,
        decoded_hex,
        decoded_utf8,
        is_valid_json,
        pretty_json,
    })
}

/// Known JWT signature algorithms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JwtAlgorithm {
    HS256,
    HS384,
    HS512,
    RS256,
    RS384,
    RS512,
    ES256,
    ES384,
    PS256,
    EdDSA,
    None,
    Unknown(String),
}

impl JwtAlgorithm {
    pub fn from_alg_str(alg: &str) -> Self {
        match alg {
            "HS256" => Self::HS256,
            "HS384" => Self::HS384,
            "HS512" => Self::HS512,
            "RS256" => Self::RS256,
            "RS384" => Self::RS384,
            "RS512" => Self::RS512,
            "ES256" => Self::ES256,
            "ES384" => Self::ES384,
            "PS256" => Self::PS256,
            "EdDSA" => Self::EdDSA,
            "none" => Self::None,
            other => Self::Unknown(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::HS256 => "HS256",
            Self::HS384 => "HS384",
            Self::HS512 => "HS512",
            Self::RS256 => "RS256",
            Self::RS384 => "RS384",
            Self::RS512 => "RS512",
            Self::ES256 => "ES256",
            Self::ES384 => "ES384",
            Self::PS256 => "PS256",
            Self::EdDSA => "EdDSA",
            Self::None => "none",
            Self::Unknown(s) => s.as_str(),
        }
    }
}

/// Parsed JWT header fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JwtHeader {
    pub alg: String,
    pub typ: Option<String>,
    pub kid: Option<String>,
    pub cty: Option<String>,
    pub extra: serde_json::Map<String, Value>,
}

/// Standard registered JWT claims (RFC 7519).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct JwtStandardClaims {
    pub iss: Option<String>,
    pub sub: Option<String>,
    pub aud: Option<Value>,
    pub exp: Option<i64>,
    pub nbf: Option<i64>,
    pub iat: Option<i64>,
    pub jti: Option<String>,
}

/// Temporal validity status of a JWT relative to a reference timestamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum JwtTimeStatus {
    Active,
    Expired { expired_seconds_ago: i64 },
    NotYetValid { valid_in_seconds: i64 },
    NoTimeClaims,
}

/// Security and hygiene warnings detected during JWT inspection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JwtSecurityWarning {
    AlgNone,
    MissingExpiration,
    ExpiredToken,
    NotYetValidToken,
    EmptySignature,
    UnrecognizedAlgorithm(String),
}

/// Complete inspection report for a JWT token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JwtInspection {
    pub raw_header: String,
    pub raw_payload: String,
    pub raw_signature: String,
    pub header: JwtHeader,
    pub algorithm: JwtAlgorithm,
    pub header_json: Value,
    pub claims: JwtStandardClaims,
    pub payload_json: Value,
    pub signature_bytes_len: usize,
    pub time_status: JwtTimeStatus,
    pub exp_iso8601: Option<String>,
    pub nbf_iso8601: Option<String>,
    pub iat_iso8601: Option<String>,
    pub warnings: Vec<JwtSecurityWarning>,
}

fn format_unix_rfc3339(ts: i64) -> Option<String> {
    let dt = OffsetDateTime::from_unix_timestamp(ts).ok()?;
    Some(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        dt.year(),
        dt.month() as u8,
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second()
    ))
}

fn parse_jwt_header(val: &Value) -> Result<JwtHeader, InspectorError> {
    let obj = val
        .as_object()
        .ok_or_else(|| InspectorError::InvalidJwtHeader("header must be a JSON object".into()))?;

    let alg = obj
        .get("alg")
        .and_then(|v| v.as_str())
        .ok_or_else(|| InspectorError::InvalidJwtHeader("missing string 'alg' field".into()))?
        .to_string();

    let typ = obj
        .get("typ")
        .and_then(|v| v.as_str())
        .map(ToString::to_string);
    let kid = obj
        .get("kid")
        .and_then(|v| v.as_str())
        .map(ToString::to_string);
    let cty = obj
        .get("cty")
        .and_then(|v| v.as_str())
        .map(ToString::to_string);

    let mut extra = serde_json::Map::new();
    for (k, v) in obj {
        if !matches!(k.as_str(), "alg" | "typ" | "kid" | "cty") {
            extra.insert(k.clone(), v.clone());
        }
    }

    Ok(JwtHeader {
        alg,
        typ,
        kid,
        cty,
        extra,
    })
}

fn parse_jwt_claims(val: &Value) -> Result<JwtStandardClaims, InspectorError> {
    let obj = val
        .as_object()
        .ok_or_else(|| InspectorError::InvalidJwtClaims("payload must be a JSON object".into()))?;

    Ok(JwtStandardClaims {
        iss: obj
            .get("iss")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        sub: obj
            .get("sub")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
        aud: obj.get("aud").cloned(),
        exp: obj.get("exp").and_then(|v| v.as_i64()),
        nbf: obj.get("nbf").and_then(|v| v.as_i64()),
        iat: obj.get("iat").and_then(|v| v.as_i64()),
        jti: obj
            .get("jti")
            .and_then(|v| v.as_str())
            .map(ToString::to_string),
    })
}

fn evaluate_time_status(claims: &JwtStandardClaims, now_unix_secs: i64) -> JwtTimeStatus {
    if let Some(nbf) = claims.nbf {
        if now_unix_secs < nbf {
            return JwtTimeStatus::NotYetValid {
                valid_in_seconds: nbf.saturating_sub(now_unix_secs),
            };
        }
    }
    if let Some(exp) = claims.exp {
        if now_unix_secs >= exp {
            return JwtTimeStatus::Expired {
                expired_seconds_ago: now_unix_secs.saturating_sub(exp),
            };
        }
        return JwtTimeStatus::Active;
    }
    if claims.nbf.is_some() || claims.iat.is_some() {
        return JwtTimeStatus::Active;
    }
    JwtTimeStatus::NoTimeClaims
}

fn split_jwt_parts(token: &str) -> Result<(&str, &str, &str), InspectorError> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err(InspectorError::EmptyInput);
    }
    let parts: Vec<&str> = trimmed.split('.').collect();
    if parts.len() != 3 {
        return Err(InspectorError::InvalidJwtStructure {
            expected_parts: 3,
            actual_parts: parts.len(),
        });
    }
    Ok((parts[0], parts[1], parts[2]))
}

/// Inspects a JWT token at a caller-supplied Unix timestamp (in seconds).
pub fn inspect_jwt(token: &str, now_unix_secs: i64) -> Result<JwtInspection, InspectorError> {
    let (raw_header, raw_payload, raw_signature) = split_jwt_parts(token)?;

    let b64_opts = Base64DecodeOptions {
        strip_whitespace: false,
        allow_missing_padding: true,
        variant: Some(Base64Variant::UrlSafeNoPad),
    };

    let (header_bytes, _) = decode_base64(raw_header, &b64_opts)
        .map_err(|e| InspectorError::InvalidJwtHeader(e.to_string()))?;
    let header_json: Value = serde_json::from_slice(&header_bytes)
        .map_err(|e| InspectorError::InvalidJwtHeader(e.to_string()))?;
    let header = parse_jwt_header(&header_json)?;
    let algorithm = JwtAlgorithm::from_alg_str(&header.alg);

    let (payload_bytes, _) = decode_base64(raw_payload, &b64_opts)
        .map_err(|e| InspectorError::InvalidJwtClaims(e.to_string()))?;
    let payload_json: Value = serde_json::from_slice(&payload_bytes)
        .map_err(|e| InspectorError::InvalidJwtClaims(e.to_string()))?;
    let claims = parse_jwt_claims(&payload_json)?;

    let signature_bytes_len = if raw_signature.is_empty() {
        0
    } else {
        let (sig_bytes, _) = decode_base64(raw_signature, &b64_opts)
            .map_err(|e| InspectorError::InvalidSignatureFormat(e.to_string()))?;
        sig_bytes.len()
    };

    let time_status = evaluate_time_status(&claims, now_unix_secs);

    let mut warnings = Vec::new();
    match &algorithm {
        JwtAlgorithm::None => warnings.push(JwtSecurityWarning::AlgNone),
        JwtAlgorithm::Unknown(name) => {
            warnings.push(JwtSecurityWarning::UnrecognizedAlgorithm(name.clone()));
        }
        _ => {}
    }
    if claims.exp.is_none() {
        warnings.push(JwtSecurityWarning::MissingExpiration);
    }
    match time_status {
        JwtTimeStatus::Expired { .. } => warnings.push(JwtSecurityWarning::ExpiredToken),
        JwtTimeStatus::NotYetValid { .. } => warnings.push(JwtSecurityWarning::NotYetValidToken),
        _ => {}
    }
    if signature_bytes_len == 0 {
        warnings.push(JwtSecurityWarning::EmptySignature);
    }

    Ok(JwtInspection {
        raw_header: raw_header.to_string(),
        raw_payload: raw_payload.to_string(),
        raw_signature: raw_signature.to_string(),
        header,
        algorithm,
        header_json,
        claims: claims.clone(),
        payload_json,
        signature_bytes_len,
        time_status,
        exp_iso8601: claims.exp.and_then(format_unix_rfc3339),
        nbf_iso8601: claims.nbf.and_then(format_unix_rfc3339),
        iat_iso8601: claims.iat.and_then(format_unix_rfc3339),
        warnings,
    })
}

/// Inspects a JWT token using the current UTC wall-clock time.
pub fn inspect_jwt_now(token: &str) -> Result<JwtInspection, InspectorError> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    inspect_jwt(token, now)
}

fn hmac_ring_algorithm(alg: &JwtAlgorithm) -> Result<hmac::Algorithm, InspectorError> {
    match alg {
        JwtAlgorithm::HS256 => Ok(hmac::HMAC_SHA256),
        JwtAlgorithm::HS384 => Ok(hmac::HMAC_SHA384),
        JwtAlgorithm::HS512 => Ok(hmac::HMAC_SHA512),
        other => Err(InspectorError::UnsupportedAlgorithm(
            other.as_str().to_string(),
        )),
    }
}

/// Verifies an HMAC-signed JWT (HS256, HS384, HS512) in constant time.
pub fn verify_jwt_hmac(token: &str, secret: &[u8]) -> Result<bool, InspectorError> {
    if secret.is_empty() {
        return Err(InspectorError::InvalidKey(
            "HMAC secret cannot be empty".into(),
        ));
    }

    let (raw_header, raw_payload, raw_signature) = split_jwt_parts(token)?;
    let b64_opts = Base64DecodeOptions {
        strip_whitespace: false,
        allow_missing_padding: true,
        variant: Some(Base64Variant::UrlSafeNoPad),
    };

    let (header_bytes, _) = decode_base64(raw_header, &b64_opts)
        .map_err(|e| InspectorError::InvalidJwtHeader(e.to_string()))?;
    let header_json: Value = serde_json::from_slice(&header_bytes)
        .map_err(|e| InspectorError::InvalidJwtHeader(e.to_string()))?;
    let header = parse_jwt_header(&header_json)?;
    let alg = JwtAlgorithm::from_alg_str(&header.alg);
    let ring_alg = hmac_ring_algorithm(&alg)?;

    if raw_signature.is_empty() {
        return Ok(false);
    }
    let (sig_bytes, _) = decode_base64(raw_signature, &b64_opts)
        .map_err(|e| InspectorError::InvalidSignatureFormat(e.to_string()))?;

    let signing_input = Zeroizing::new(format!("{raw_header}.{raw_payload}"));
    let key = hmac::Key::new(ring_alg, secret);
    Ok(hmac::verify(&key, signing_input.as_bytes(), &sig_bytes).is_ok())
}

/// Signs a header and JSON payload into an HMAC JWT (HS256, HS384, HS512).
pub fn sign_jwt_hmac(
    header: &JwtHeader,
    payload: &Value,
    secret: &[u8],
) -> Result<String, InspectorError> {
    if secret.is_empty() {
        return Err(InspectorError::InvalidKey(
            "HMAC secret cannot be empty".into(),
        ));
    }
    if !payload.is_object() {
        return Err(InspectorError::InvalidJwtClaims(
            "payload must be a JSON object".into(),
        ));
    }

    let alg = JwtAlgorithm::from_alg_str(&header.alg);
    let ring_alg = hmac_ring_algorithm(&alg)?;

    let mut header_map = header.extra.clone();
    header_map.insert("alg".to_string(), Value::String(header.alg.clone()));
    if let Some(typ) = &header.typ {
        header_map.insert("typ".to_string(), Value::String(typ.clone()));
    }
    if let Some(kid) = &header.kid {
        header_map.insert("kid".to_string(), Value::String(kid.clone()));
    }
    if let Some(cty) = &header.cty {
        header_map.insert("cty".to_string(), Value::String(cty.clone()));
    }

    let header_bytes = serde_json::to_vec(&Value::Object(header_map))?;
    let payload_bytes = serde_json::to_vec(payload)?;

    let encoded_header = encode_base64(&header_bytes, Base64Variant::UrlSafeNoPad);
    let encoded_payload = encode_base64(&payload_bytes, Base64Variant::UrlSafeNoPad);
    let signing_input = Zeroizing::new(format!("{encoded_header}.{encoded_payload}"));

    let key = hmac::Key::new(ring_alg, secret);
    let tag = hmac::sign(&key, signing_input.as_bytes());
    let encoded_sig = encode_base64(tag.as_ref(), Base64Variant::UrlSafeNoPad);

    Ok(format!("{encoded_header}.{encoded_payload}.{encoded_sig}"))
}

/// Classification of an arbitrary input string for auto-inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    Jwt,
    Base64,
    Unknown,
}

/// Unified inspection output for auto-detected inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum UnifiedInspection {
    Jwt(JwtInspection),
    Base64(Base64Inspection),
}

/// Heuristically detects whether an input string is a JWT, Base64, or unknown.
pub fn detect_input_kind(input: &str) -> InputKind {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return InputKind::Unknown;
    }
    if trimmed.split('.').count() == 3 && inspect_jwt(trimmed, 0).is_ok() {
        return InputKind::Jwt;
    }
    if decode_base64(trimmed, &Base64DecodeOptions::default()).is_ok() {
        return InputKind::Base64;
    }
    InputKind::Unknown
}

/// Automatically detects the input format and runs the appropriate inspector.
pub fn inspect_auto(input: &str, now_unix_secs: i64) -> Result<UnifiedInspection, InspectorError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(InspectorError::EmptyInput);
    }
    if trimmed.split('.').count() == 3 {
        if let Ok(jwt) = inspect_jwt(trimmed, now_unix_secs) {
            return Ok(UnifiedInspection::Jwt(jwt));
        }
    }
    let b64 = inspect_base64(trimmed, &Base64DecodeOptions::default())?;
    Ok(UnifiedInspection::Base64(b64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn base64_roundtrip_and_json_inspection() {
        let raw_json = br#"{"user":"franco","roles":["admin","dev"]}"#;
        let encoded_std = encode_base64(raw_json, Base64Variant::Standard);
        let encoded_url = encode_base64(raw_json, Base64Variant::UrlSafeNoPad);

        let inspected_std = inspect_base64(&encoded_std, &Base64DecodeOptions::default()).unwrap();
        assert_eq!(inspected_std.detected_variant, Base64Variant::Standard);
        assert!(inspected_std.is_valid_json);
        assert_eq!(
            inspected_std.decoded_utf8.as_deref(),
            Some(r#"{"user":"franco","roles":["admin","dev"]}"#)
        );
        assert!(inspected_std.pretty_json.is_some());

        let inspected_url = inspect_base64(&encoded_url, &Base64DecodeOptions::default()).unwrap();
        assert_eq!(inspected_url.decoded_hex, inspected_std.decoded_hex);
    }

    #[test]
    fn base64_rejects_invalid_bytes_and_padding() {
        let opts = Base64DecodeOptions {
            strip_whitespace: false,
            allow_missing_padding: false,
            variant: Some(Base64Variant::Standard),
        };
        assert_eq!(decode_base64("", &opts), Err(InspectorError::EmptyInput));
        assert_eq!(
            decode_base64("abc", &opts),
            Err(InspectorError::InvalidBase64Length { len: 3 })
        );
        assert_eq!(
            decode_base64("ab$d", &opts),
            Err(InspectorError::InvalidBase64Byte {
                byte: b'$',
                offset: 2
            })
        );
        assert_eq!(
            decode_base64("a=cd", &opts),
            Err(InspectorError::InvalidPadding { offset: 1 })
        );
    }

    #[test]
    fn jwt_sign_inspect_and_verify_hmac_happy_path() {
        let header = JwtHeader {
            alg: "HS256".to_string(),
            typ: Some("JWT".to_string()),
            kid: Some("key-1".to_string()),
            cty: None,
            extra: serde_json::Map::new(),
        };
        let payload = json!({
            "iss": "search-linux",
            "sub": "user-42",
            "iat": 1_700_000_000_i64,
            "nbf": 1_700_000_100_i64,
            "exp": 1_700_003_600_i64
        });
        let secret = b"super-secret-signing-key";

        let token = sign_jwt_hmac(&header, &payload, secret).unwrap();
        assert!(verify_jwt_hmac(&token, secret).unwrap());
        assert!(!verify_jwt_hmac(&token, b"wrong-secret").unwrap());

        let active = inspect_jwt(&token, 1_700_001_000).unwrap();
        assert_eq!(active.algorithm, JwtAlgorithm::HS256);
        assert_eq!(active.header.kid.as_deref(), Some("key-1"));
        assert_eq!(active.claims.sub.as_deref(), Some("user-42"));
        assert_eq!(active.time_status, JwtTimeStatus::Active);
        assert!(active.warnings.is_empty());
        assert_eq!(active.exp_iso8601.as_deref(), Some("2023-11-14T23:13:20Z"));

        let expired = inspect_jwt(&token, 1_700_004_000).unwrap();
        assert_eq!(
            expired.time_status,
            JwtTimeStatus::Expired {
                expired_seconds_ago: 400
            }
        );
        assert!(expired.warnings.contains(&JwtSecurityWarning::ExpiredToken));

        let not_yet = inspect_jwt(&token, 1_700_000_050).unwrap();
        assert_eq!(
            not_yet.time_status,
            JwtTimeStatus::NotYetValid {
                valid_in_seconds: 50
            }
        );
        assert!(not_yet
            .warnings
            .contains(&JwtSecurityWarning::NotYetValidToken));
    }

    #[test]
    fn jwt_detects_alg_none_and_missing_exp_and_auto_inspects() {
        let header_b64 = encode_base64(
            br#"{"alg":"none","typ":"JWT"}"#,
            Base64Variant::UrlSafeNoPad,
        );
        let payload_b64 = encode_base64(br#"{"sub":"anon"}"#, Base64Variant::UrlSafeNoPad);
        let token = format!("{header_b64}.{payload_b64}.");

        let report = inspect_jwt(&token, 1_700_000_000).unwrap();
        assert_eq!(report.algorithm, JwtAlgorithm::None);
        assert_eq!(report.time_status, JwtTimeStatus::NoTimeClaims);
        assert!(report.warnings.contains(&JwtSecurityWarning::AlgNone));
        assert!(report
            .warnings
            .contains(&JwtSecurityWarning::MissingExpiration));
        assert!(report
            .warnings
            .contains(&JwtSecurityWarning::EmptySignature));
        assert_eq!(
            verify_jwt_hmac(&token, b"secret"),
            Err(InspectorError::UnsupportedAlgorithm("none".to_string()))
        );

        assert_eq!(detect_input_kind(&token), InputKind::Jwt);
        match inspect_auto(&token, 1_700_000_000).unwrap() {
            UnifiedInspection::Jwt(j) => assert_eq!(j.claims.sub.as_deref(), Some("anon")),
            UnifiedInspection::Base64(_) => panic!("expected JWT inspection"),
        }
    }
}
