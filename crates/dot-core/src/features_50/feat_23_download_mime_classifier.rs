//! Download MIME classifier for Search Linux.
//!
//! Classifies incoming downloads by combining declared `Content-Type` headers,
//! filename/URL extensions, and magic-byte signatures to determine the effective
//! MIME type, category, mismatch status, risk level, and recommended action.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

/// Default maximum number of leading bytes inspected during magic-byte sniffing.
pub const DEFAULT_MAX_SNIFF_BYTES: usize = 512;

/// Errors returned by download MIME classification and policy validation.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ClassifierError {
    /// The provided MIME type string is syntactically invalid.
    #[error("invalid MIME type: {0}")]
    InvalidMimeType(String),

    /// The provided download URL failed parsing.
    #[error("invalid download URL: {0}")]
    InvalidUrl(String),

    /// The classifier policy contains invalid configuration values.
    #[error("invalid classifier policy: {0}")]
    InvalidPolicy(String),

    /// No URL, filename, Content-Type, or magic bytes were supplied.
    #[error("classification input is empty: provide at least one of url, filename, content_type, or magic_bytes")]
    EmptyInput,

    /// JSON serialization or deserialization failed.
    #[error("policy serialization error: {0}")]
    Serialization(String),
}

/// High-level semantic category of a downloaded resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadCategory {
    Document,
    Image,
    Audio,
    Video,
    Archive,
    Executable,
    Script,
    Installer,
    DiskImage,
    Font,
    WebContent,
    Data,
    Unknown,
}

impl fmt::Display for DownloadCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Document => "document",
            Self::Image => "image",
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Archive => "archive",
            Self::Executable => "executable",
            Self::Script => "script",
            Self::Installer => "installer",
            Self::DiskImage => "disk_image",
            Self::Font => "font",
            Self::WebContent => "web_content",
            Self::Data => "data",
            Self::Unknown => "unknown",
        };
        f.write_str(label)
    }
}

/// Security and integrity risk level assigned to a download.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Safe,
    Low,
    Medium,
    High,
    Critical,
}

/// Type of discrepancy detected between declared MIME, extension, and magic bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MismatchKind {
    None,
    ExtensionMimeMismatch,
    MagicMimeMismatch,
    MagicExtensionMismatch,
    ExecutableDisguised,
}

/// Recommended user-agent or download-manager action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadAction {
    Allow,
    Prompt,
    Warn,
    Block,
}

/// Policy controlling how the MIME classifier evaluates risk and actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MimeClassifierPolicy {
    /// Block downloads whose magic bytes indicate an executable/script disguised as another type.
    pub block_disguised_executables: bool,
    /// Allow downloads whose MIME type and extension cannot be determined.
    pub allow_unknown_types: bool,
    /// Respect `X-Content-Type-Options: nosniff` by keeping the declared MIME as effective MIME.
    pub respect_nosniff: bool,
    /// Maximum number of magic bytes to inspect.
    pub max_sniff_bytes: usize,
    /// Optional per-MIME action overrides (keys are normalized `type/subtype`).
    pub mime_overrides: HashMap<String, DownloadAction>,
}

impl Default for MimeClassifierPolicy {
    fn default() -> Self {
        Self {
            block_disguised_executables: true,
            allow_unknown_types: true,
            respect_nosniff: true,
            max_sniff_bytes: DEFAULT_MAX_SNIFF_BYTES,
            mime_overrides: HashMap::new(),
        }
    }
}

impl MimeClassifierPolicy {
    /// Validates policy invariants and normalizes override keys.
    pub fn validate(&self) -> Result<(), ClassifierError> {
        if self.max_sniff_bytes == 0 || self.max_sniff_bytes > 65_536 {
            return Err(ClassifierError::InvalidPolicy(
                "max_sniff_bytes must be between 1 and 65536".to_string(),
            ));
        }
        for mime in self.mime_overrides.keys() {
            parse_mime_essence(mime)?;
        }
        Ok(())
    }

    /// Serializes the policy to a pretty JSON string.
    pub fn to_json(&self) -> Result<String, ClassifierError> {
        self.validate()?;
        serde_json::to_string_pretty(self)
            .map_err(|err| ClassifierError::Serialization(err.to_string()))
    }

    /// Deserializes and validates a policy from JSON.
    pub fn from_json(raw: &str) -> Result<Self, ClassifierError> {
        let policy: Self = serde_json::from_str(raw)
            .map_err(|err| ClassifierError::Serialization(err.to_string()))?;
        policy.validate()?;
        Ok(policy)
    }
}

/// Input metadata and leading bytes for a download to classify.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MimeClassificationInput {
    /// Source URL of the download, if known.
    pub url: Option<String>,
    /// Suggested filename (e.g., from `Content-Disposition`), if known.
    pub filename: Option<String>,
    /// Raw `Content-Type` header value, if present.
    pub content_type: Option<String>,
    /// Whether `X-Content-Type-Options: nosniff` was sent by the server.
    pub nosniff: bool,
    /// Leading bytes of the response body for magic-byte sniffing.
    pub magic_bytes: Vec<u8>,
    /// Optional total content length in bytes.
    pub content_length: Option<u64>,
}

/// Output of classifying a download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MimeClassificationResult {
    /// Final normalized MIME type selected by the classifier.
    pub effective_mime: String,
    /// Normalized MIME type from `Content-Type`, if valid and present.
    pub declared_mime: Option<String>,
    /// Normalized MIME type inferred from magic bytes, if recognized.
    pub sniffed_mime: Option<String>,
    /// Normalized MIME type inferred from filename/URL extension, if recognized.
    pub extension_mime: Option<String>,
    /// Extracted normalized file extension (without leading dot), if any.
    pub extension: Option<String>,
    /// Semantic category of the download.
    pub category: DownloadCategory,
    /// Security risk level.
    pub risk_level: RiskLevel,
    /// Discrepancy classification across signals.
    pub mismatch: MismatchKind,
    /// Recommended download handling action.
    pub recommended_action: DownloadAction,
    /// Suggested safe file extension (without leading dot) for `effective_mime`.
    pub suggested_extension: Option<String>,
    /// Human-readable explanations for the classification decision.
    pub reasons: Vec<String>,
}

/// Stateless download MIME classifier parameterized by a [`MimeClassifierPolicy`].
#[derive(Debug, Clone, Default)]
pub struct DownloadMimeClassifier {
    policy: MimeClassifierPolicy,
}

impl DownloadMimeClassifier {
    /// Creates a new classifier after validating the provided policy.
    pub fn new(policy: MimeClassifierPolicy) -> Result<Self, ClassifierError> {
        policy.validate()?;
        Ok(Self { policy })
    }

    /// Returns a reference to the active policy.
    pub fn policy(&self) -> &MimeClassifierPolicy {
        &self.policy
    }

    /// Classifies a download input according to the active policy.
    pub fn classify(
        &self,
        input: &MimeClassificationInput,
    ) -> Result<MimeClassificationResult, ClassifierError> {
        let has_url = input.url.as_deref().is_some_and(|u| !u.trim().is_empty());
        let has_filename = input
            .filename
            .as_deref()
            .is_some_and(|f| !f.trim().is_empty());
        let has_ct = input
            .content_type
            .as_deref()
            .is_some_and(|c| !c.trim().is_empty());
        let has_bytes = !input.magic_bytes.is_empty();

        if !has_url && !has_filename && !has_ct && !has_bytes {
            return Err(ClassifierError::EmptyInput);
        }

        let mut reasons = Vec::new();

        let declared_mime = match input.content_type.as_deref() {
            Some(raw) if !raw.trim().is_empty() => Some(parse_mime_essence(raw)?),
            _ => None,
        };

        let extension = extract_extension(input.filename.as_deref(), input.url.as_deref())?;
        let extension_mime = extension
            .as_deref()
            .and_then(mime_for_extension)
            .map(str::to_string);

        let sniff_len = input.magic_bytes.len().min(self.policy.max_sniff_bytes);
        let sniffed_mime = sniff_magic_bytes(&input.magic_bytes[..sniff_len]).map(str::to_string);

        let effective_mime = select_effective_mime(
            declared_mime.as_deref(),
            sniffed_mime.as_deref(),
            extension_mime.as_deref(),
            input.nosniff && self.policy.respect_nosniff,
            &mut reasons,
        );

        let mismatch = detect_mismatch(
            declared_mime.as_deref(),
            sniffed_mime.as_deref(),
            extension_mime.as_deref(),
            &mut reasons,
        );

        let category = categorize_mime(&effective_mime, extension.as_deref());
        let risk_level = assess_risk(
            &effective_mime,
            category,
            mismatch,
            sniffed_mime.as_deref(),
            &mut reasons,
        );

        let recommended_action = self.recommend_action(
            &effective_mime,
            category,
            risk_level,
            mismatch,
            &mut reasons,
        );

        let suggested_extension = default_extension_for_mime(&effective_mime).map(str::to_string);

        Ok(MimeClassificationResult {
            effective_mime,
            declared_mime,
            sniffed_mime,
            extension_mime,
            extension,
            category,
            risk_level,
            mismatch,
            recommended_action,
            suggested_extension,
            reasons,
        })
    }

    fn recommend_action(
        &self,
        effective_mime: &str,
        category: DownloadCategory,
        risk_level: RiskLevel,
        mismatch: MismatchKind,
        reasons: &mut Vec<String>,
    ) -> DownloadAction {
        if let Some(override_action) = self.policy.mime_overrides.get(effective_mime) {
            reasons.push(format!(
                "policy override applied for {effective_mime}: {override_action:?}"
            ));
            return *override_action;
        }

        if mismatch == MismatchKind::ExecutableDisguised && self.policy.block_disguised_executables
        {
            reasons.push("blocked disguised executable per policy".to_string());
            return DownloadAction::Block;
        }

        if category == DownloadCategory::Unknown && !self.policy.allow_unknown_types {
            reasons.push("unknown download type requires prompt per policy".to_string());
            return DownloadAction::Prompt;
        }

        match risk_level {
            RiskLevel::Critical => DownloadAction::Block,
            RiskLevel::High => DownloadAction::Warn,
            RiskLevel::Medium => DownloadAction::Prompt,
            RiskLevel::Low | RiskLevel::Safe => DownloadAction::Allow,
        }
    }
}

/// Parses and normalizes a `Content-Type` header or MIME string to `type/subtype`.
pub fn parse_mime_essence(raw: &str) -> Result<String, ClassifierError> {
    let essence = raw
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let Some((ty, sub)) = essence.split_once('/') else {
        return Err(ClassifierError::InvalidMimeType(raw.to_string()));
    };

    if ty.is_empty() || sub.is_empty() || sub.contains('/') {
        return Err(ClassifierError::InvalidMimeType(raw.to_string()));
    }

    if !is_valid_mime_token(ty) || !is_valid_mime_token(sub) {
        return Err(ClassifierError::InvalidMimeType(raw.to_string()));
    }

    Ok(format!("{ty}/{sub}"))
}

fn is_valid_mime_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'+' | b'.' | b'_' | b'*'))
}

fn extract_extension(
    filename: Option<&str>,
    url_str: Option<&str>,
) -> Result<Option<String>, ClassifierError> {
    if let Some(name) = filename {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return Ok(extension_from_name(trimmed));
        }
    }

    if let Some(raw_url) = url_str {
        let trimmed = raw_url.trim();
        if !trimmed.is_empty() {
            let parsed = Url::parse(trimmed)
                .map_err(|_| ClassifierError::InvalidUrl(trimmed.to_string()))?;
            if let Some(mut segments) = parsed.path_segments() {
                if let Some(last) = segments.next_back() {
                    if !last.is_empty() {
                        return Ok(extension_from_name(last));
                    }
                }
            }
        }
    }

    Ok(None)
}

fn extension_from_name(name: &str) -> Option<String> {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name).trim();
    if base.is_empty() || base.starts_with('.') && !base[1..].contains('.') {
        return None;
    }

    let lower = base.to_ascii_lowercase();
    for compound in ["tar.gz", "tar.bz2", "tar.xz", "tar.zst", "tar.lz"] {
        let suffix = format!(".{compound}");
        if lower.ends_with(&suffix) && lower.len() > suffix.len() {
            return Some(compound.to_string());
        }
    }

    let (_, ext) = lower.rsplit_once('.')?;
    if ext.is_empty() || ext.len() > 16 || !ext.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    Some(ext.to_string())
}

fn select_effective_mime(
    declared: Option<&str>,
    sniffed: Option<&str>,
    ext_mime: Option<&str>,
    honour_nosniff: bool,
    reasons: &mut Vec<String>,
) -> String {
    if honour_nosniff {
        if let Some(decl) = declared {
            if !is_generic_mime(decl) {
                reasons.push(format!("nosniff enabled; keeping declared MIME {decl}"));
                return decl.to_string();
            }
        }
    }

    match (declared, sniffed, ext_mime) {
        (Some(decl), Some(sniff), _) if is_generic_mime(decl) => {
            reasons.push(format!(
                "upgraded generic declared MIME {decl} using magic bytes to {sniff}"
            ));
            sniff.to_string()
        }
        (Some(decl), Some(sniff), _)
            if is_container_mime(decl) && is_specialized_zip_mime(sniff) =>
        {
            reasons.push(format!("refined container MIME {decl} to {sniff}"));
            sniff.to_string()
        }
        (Some(decl), Some(sniff), _) if is_executable_mime(sniff) && !is_executable_mime(decl) => {
            reasons.push(format!(
                "overrode declared MIME {decl} with sniffed executable MIME {sniff}"
            ));
            sniff.to_string()
        }
        (Some(decl), _, Some(ext)) if is_generic_mime(decl) => {
            reasons.push(format!(
                "upgraded generic declared MIME {decl} using extension to {ext}"
            ));
            ext.to_string()
        }
        (Some(decl), _, _) => decl.to_string(),
        (None, Some(sniff), _) => {
            reasons.push(format!("inferred MIME {sniff} from magic bytes"));
            sniff.to_string()
        }
        (None, None, Some(ext)) => {
            reasons.push(format!("inferred MIME {ext} from file extension"));
            ext.to_string()
        }
        (None, None, None) => {
            reasons.push(
                "unable to determine MIME; falling back to application/octet-stream".to_string(),
            );
            "application/octet-stream".to_string()
        }
    }
}

fn detect_mismatch(
    declared: Option<&str>,
    sniffed: Option<&str>,
    ext_mime: Option<&str>,
    reasons: &mut Vec<String>,
) -> MismatchKind {
    if let Some(sniff) = sniffed {
        if is_executable_mime(sniff) {
            let declared_non_exec =
                declared.is_some_and(|d| !is_executable_mime(d) && !is_generic_mime(d));
            let ext_non_exec = ext_mime.is_some_and(|e| !is_executable_mime(e));
            if declared_non_exec || ext_non_exec {
                reasons.push(format!(
                    "executable magic bytes ({sniff}) conflict with non-executable metadata"
                ));
                return MismatchKind::ExecutableDisguised;
            }
        }
    }

    if let (Some(decl), Some(sniff)) = (declared, sniffed) {
        if !is_generic_mime(decl) && !mimes_compatible(decl, sniff) {
            reasons.push(format!(
                "declared MIME {decl} does not match sniffed MIME {sniff}"
            ));
            return MismatchKind::MagicMimeMismatch;
        }
    }

    if let (Some(sniff), Some(ext)) = (sniffed, ext_mime) {
        if !mimes_compatible(sniff, ext) {
            reasons.push(format!(
                "sniffed MIME {sniff} does not match extension MIME {ext}"
            ));
            return MismatchKind::MagicExtensionMismatch;
        }
    }

    if let (Some(decl), Some(ext)) = (declared, ext_mime) {
        if !is_generic_mime(decl) && !mimes_compatible(decl, ext) {
            reasons.push(format!(
                "declared MIME {decl} does not match extension MIME {ext}"
            ));
            return MismatchKind::ExtensionMimeMismatch;
        }
    }

    MismatchKind::None
}

fn mimes_compatible(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    // ZIP container is the underlying format for OOXML, ODF, APK, JAR, and EPUB.
    let zip_family = [
        "application/zip",
        "application/java-archive",
        "application/vnd.android.package-archive",
        "application/epub+zip",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "application/vnd.oasis.opendocument.text",
    ];
    if zip_family.contains(&a) && zip_family.contains(&b) {
        return true;
    }
    // Text subtypes like json/xml/html/csv/shellscript are often served as text/plain.
    if (a == "text/plain" && b.starts_with("text/"))
        || (b == "text/plain" && a.starts_with("text/"))
        || (a == "text/plain"
            && matches!(
                b,
                "application/json" | "application/xml" | "application/javascript"
            ))
        || (b == "text/plain"
            && matches!(
                a,
                "application/json" | "application/xml" | "application/javascript"
            ))
    {
        return true;
    }
    // Ogg container can carry audio or video.
    if (a == "application/ogg" && (b == "audio/ogg" || b == "video/ogg"))
        || (b == "application/ogg" && (a == "audio/ogg" || a == "video/ogg"))
    {
        return true;
    }
    // EBML container for WebM and Matroska.
    if (a == "video/webm" && b == "video/x-matroska")
        || (b == "video/webm" && a == "video/x-matroska")
    {
        return true;
    }
    false
}

fn is_generic_mime(mime: &str) -> bool {
    matches!(
        mime,
        "application/octet-stream" | "binary/octet-stream" | "application/unknown"
    )
}

fn is_container_mime(mime: &str) -> bool {
    matches!(mime, "application/zip" | "application/ogg")
}

fn is_specialized_zip_mime(mime: &str) -> bool {
    matches!(
        mime,
        "application/java-archive"
            | "application/vnd.android.package-archive"
            | "application/epub+zip"
    )
}

fn is_executable_mime(mime: &str) -> bool {
    matches!(
        mime,
        "application/x-elf"
            | "application/x-executable"
            | "application/x-sharedlib"
            | "application/vnd.microsoft.portable-executable"
            | "application/x-msdownload"
            | "application/x-dosexec"
            | "application/x-mach-binary"
            | "application/x-shellscript"
            | "text/x-shellscript"
            | "application/x-python"
            | "text/x-python"
            | "application/javascript"
            | "text/javascript"
            | "application/x-msdos-program"
            | "application/x-bat"
            | "application/x-powershell"
    )
}

fn categorize_mime(mime: &str, ext: Option<&str>) -> DownloadCategory {
    if matches!(
        ext,
        Some("deb" | "rpm" | "msi" | "pkg" | "appimage" | "flatpak" | "snap" | "apk")
    ) || matches!(
        mime,
        "application/vnd.debian.binary-package"
            | "application/x-rpm"
            | "application/x-msi"
            | "application/vnd.android.package-archive"
            | "application/x-appimage"
    ) {
        return DownloadCategory::Installer;
    }

    if matches!(ext, Some("iso" | "img" | "dmg" | "vhd" | "vhdx" | "qcow2"))
        || matches!(
            mime,
            "application/x-iso9660-image" | "application/x-apple-diskimage"
        )
    {
        return DownloadCategory::DiskImage;
    }

    if matches!(
        mime,
        "application/x-elf"
            | "application/x-executable"
            | "application/x-sharedlib"
            | "application/vnd.microsoft.portable-executable"
            | "application/x-msdownload"
            | "application/x-dosexec"
            | "application/x-mach-binary"
            | "application/java-archive"
    ) {
        return DownloadCategory::Executable;
    }

    if matches!(
        mime,
        "application/x-shellscript"
            | "text/x-shellscript"
            | "application/x-python"
            | "text/x-python"
            | "application/javascript"
            | "text/javascript"
            | "application/x-bat"
            | "application/x-powershell"
    ) {
        return DownloadCategory::Script;
    }

    if mime.starts_with("image/") {
        return DownloadCategory::Image;
    }
    if mime.starts_with("audio/") {
        return DownloadCategory::Audio;
    }
    if mime.starts_with("video/") {
        return DownloadCategory::Video;
    }
    if mime.starts_with("font/")
        || matches!(
            mime,
            "application/font-woff" | "application/font-woff2" | "application/vnd.ms-fontobject"
        )
    {
        return DownloadCategory::Font;
    }

    if matches!(
        mime,
        "application/zip"
            | "application/gzip"
            | "application/x-gzip"
            | "application/x-bzip2"
            | "application/x-xz"
            | "application/zstd"
            | "application/x-7z-compressed"
            | "application/vnd.rar"
            | "application/x-tar"
    ) {
        return DownloadCategory::Archive;
    }

    if matches!(
        mime,
        "application/pdf"
            | "text/plain"
            | "text/markdown"
            | "text/csv"
            | "application/rtf"
            | "application/epub+zip"
            | "application/msword"
            | "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            | "application/vnd.openxmlformats-officedocument.presentationml.presentation"
            | "application/vnd.oasis.opendocument.text"
    ) {
        return DownloadCategory::Document;
    }

    if matches!(mime, "text/html" | "application/xhtml+xml" | "text/css") {
        return DownloadCategory::WebContent;
    }

    if matches!(
        mime,
        "application/json"
            | "application/xml"
            | "text/xml"
            | "application/yaml"
            | "application/vnd.sqlite3"
    ) {
        return DownloadCategory::Data;
    }

    if is_generic_mime(mime) {
        return DownloadCategory::Unknown;
    }

    DownloadCategory::Data
}

fn assess_risk(
    effective_mime: &str,
    category: DownloadCategory,
    mismatch: MismatchKind,
    sniffed_mime: Option<&str>,
    reasons: &mut Vec<String>,
) -> RiskLevel {
    if mismatch == MismatchKind::ExecutableDisguised {
        return RiskLevel::Critical;
    }

    if effective_mime == "image/svg+xml" && mismatch != MismatchKind::None {
        reasons.push("SVG with mismatched metadata may contain active script".to_string());
        return RiskLevel::High;
    }

    match category {
        DownloadCategory::Executable | DownloadCategory::Script | DownloadCategory::Installer => {
            reasons.push(format!(
                "{category} download carries elevated execution risk"
            ));
            RiskLevel::High
        }
        DownloadCategory::DiskImage => {
            reasons.push("disk image may contain executable payloads".to_string());
            RiskLevel::Medium
        }
        DownloadCategory::Archive => {
            if mismatch != MismatchKind::None {
                RiskLevel::Medium
            } else {
                RiskLevel::Low
            }
        }
        DownloadCategory::Unknown => {
            if sniffed_mime.is_none() {
                RiskLevel::Medium
            } else {
                RiskLevel::Low
            }
        }
        DownloadCategory::Document
        | DownloadCategory::Image
        | DownloadCategory::Audio
        | DownloadCategory::Video
        | DownloadCategory::Font
        | DownloadCategory::WebContent
        | DownloadCategory::Data => {
            if mismatch != MismatchKind::None {
                RiskLevel::Medium
            } else if matches!(
                category,
                DownloadCategory::Image
                    | DownloadCategory::Audio
                    | DownloadCategory::Video
                    | DownloadCategory::Font
            ) {
                RiskLevel::Safe
            } else {
                RiskLevel::Low
            }
        }
    }
}

/// Sniffs a MIME type from leading magic bytes.
pub fn sniff_magic_bytes(bytes: &[u8]) -> Option<&'static str> {
    if bytes.is_empty() {
        return None;
    }

    // Executables & scripts first so disguised binaries are always caught.
    if bytes.starts_with(b"\x7fELF") {
        return Some("application/x-elf");
    }
    if bytes.starts_with(b"MZ") && bytes.len() >= 64 {
        return Some("application/vnd.microsoft.portable-executable");
    }
    if bytes.starts_with(b"\xfe\xed\xfa\xce")
        || bytes.starts_with(b"\xfe\xed\xfa\xcf")
        || bytes.starts_with(b"\xce\xfa\xed\xfe")
        || bytes.starts_with(b"\xcf\xfa\xed\xfe")
    {
        return Some("application/x-mach-binary");
    }
    if bytes.starts_with(b"#!") {
        return Some("text/x-shellscript");
    }

    // Documents & archives.
    if bytes.starts_with(b"%PDF-") {
        return Some("application/pdf");
    }
    if bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
    {
        return Some("application/zip");
    }
    if bytes.starts_with(b"\x1f\x8b") {
        return Some("application/gzip");
    }
    if bytes.starts_with(b"BZh") {
        return Some("application/x-bzip2");
    }
    if bytes.starts_with(b"\xfd7zXZ\x00") {
        return Some("application/x-xz");
    }
    if bytes.starts_with(b"\x28\xb5\x2f\xfd") {
        return Some("application/zstd");
    }
    if bytes.starts_with(b"7z\xbc\xaf\x27\x1c") {
        return Some("application/x-7z-compressed");
    }
    if bytes.starts_with(b"Rar!\x1a\x07") {
        return Some("application/vnd.rar");
    }
    if bytes.starts_with(b"SQLite format 3\0") {
        return Some("application/vnd.sqlite3");
    }

    // Images.
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(b"\xff\xd8\xff") {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }

    // Audio / Video.
    if bytes.starts_with(b"fLaC") {
        return Some("audio/flac");
    }
    if bytes.starts_with(b"OggS") {
        return Some("application/ogg");
    }
    if bytes.starts_with(b"ID3")
        || (bytes.len() >= 2 && bytes[0] == 0xff && (bytes[1] & 0xe0) == 0xe0)
    {
        return Some("audio/mpeg");
    }
    if bytes.len() >= 8 && &bytes[4..8] == b"ftyp" {
        return Some("video/mp4");
    }
    if bytes.starts_with(b"\x1a\x45\xdf\xa3") {
        return Some("video/webm");
    }

    // Fonts.
    if bytes.starts_with(b"wOFF") {
        return Some("font/woff");
    }
    if bytes.starts_with(b"wOF2") {
        return Some("font/woff2");
    }

    // Text-based markup / data sniffing.
    if let Ok(text) = std::str::from_utf8(bytes) {
        let trimmed = text.trim_start();
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("<!doctype html") || lower.starts_with("<html") {
            return Some("text/html");
        }
        if lower.starts_with("<svg") {
            return Some("image/svg+xml");
        }
        if lower.starts_with("<?xml") {
            if lower.contains("<svg") {
                return Some("image/svg+xml");
            }
            return Some("application/xml");
        }
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            return Some("application/json");
        }
        if !trimmed.is_empty()
            && trimmed
                .chars()
                .all(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
        {
            return Some("text/plain");
        }
    }

    None
}

/// Maps a normalized file extension (without leading dot) to its canonical MIME type.
pub fn mime_for_extension(ext: &str) -> Option<&'static str> {
    match ext {
        "pdf" => Some("application/pdf"),
        "txt" | "log" => Some("text/plain"),
        "md" | "markdown" => Some("text/markdown"),
        "csv" => Some("text/csv"),
        "rtf" => Some("application/rtf"),
        "epub" => Some("application/epub+zip"),
        "doc" => Some("application/msword"),
        "docx" => Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
        "xlsx" => Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
        "pptx" => Some("application/vnd.openxmlformats-officedocument.presentationml.presentation"),
        "odt" => Some("application/vnd.oasis.opendocument.text"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "svg" => Some("image/svg+xml"),
        "ico" => Some("image/x-icon"),
        "mp3" => Some("audio/mpeg"),
        "flac" => Some("audio/flac"),
        "ogg" | "oga" => Some("audio/ogg"),
        "wav" => Some("audio/wav"),
        "mp4" | "m4v" => Some("video/mp4"),
        "webm" => Some("video/webm"),
        "mkv" => Some("video/x-matroska"),
        "zip" => Some("application/zip"),
        "gz" | "gzip" | "tar.gz" => Some("application/gzip"),
        "bz2" | "tar.bz2" => Some("application/x-bzip2"),
        "xz" | "tar.xz" => Some("application/x-xz"),
        "zst" | "tar.zst" => Some("application/zstd"),
        "7z" => Some("application/x-7z-compressed"),
        "rar" => Some("application/vnd.rar"),
        "tar" | "tar.lz" => Some("application/x-tar"),
        "exe" | "dll" => Some("application/vnd.microsoft.portable-executable"),
        "elf" | "bin" => Some("application/x-elf"),
        "jar" => Some("application/java-archive"),
        "apk" => Some("application/vnd.android.package-archive"),
        "deb" => Some("application/vnd.debian.binary-package"),
        "rpm" => Some("application/x-rpm"),
        "msi" => Some("application/x-msi"),
        "appimage" => Some("application/x-appimage"),
        "iso" => Some("application/x-iso9660-image"),
        "dmg" => Some("application/x-apple-diskimage"),
        "sh" | "bash" | "zsh" => Some("text/x-shellscript"),
        "py" => Some("text/x-python"),
        "js" | "mjs" => Some("application/javascript"),
        "ps1" => Some("application/x-powershell"),
        "bat" | "cmd" => Some("application/x-bat"),
        "html" | "htm" => Some("text/html"),
        "css" => Some("text/css"),
        "json" => Some("application/json"),
        "xml" => Some("application/xml"),
        "yaml" | "yml" => Some("application/yaml"),
        "sqlite" | "db" => Some("application/vnd.sqlite3"),
        "woff" => Some("font/woff"),
        "woff2" => Some("font/woff2"),
        "ttf" => Some("font/ttf"),
        "otf" => Some("font/otf"),
        _ => None,
    }
}

/// Returns a canonical file extension (without leading dot) for a normalized MIME type.
pub fn default_extension_for_mime(mime: &str) -> Option<&'static str> {
    match mime {
        "application/pdf" => Some("pdf"),
        "text/plain" => Some("txt"),
        "text/markdown" => Some("md"),
        "text/csv" => Some("csv"),
        "application/rtf" => Some("rtf"),
        "application/epub+zip" => Some("epub"),
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/svg+xml" => Some("svg"),
        "audio/mpeg" => Some("mp3"),
        "audio/flac" => Some("flac"),
        "audio/ogg" | "application/ogg" => Some("ogg"),
        "video/mp4" => Some("mp4"),
        "video/webm" => Some("webm"),
        "video/x-matroska" => Some("mkv"),
        "application/zip" => Some("zip"),
        "application/gzip" | "application/x-gzip" => Some("gz"),
        "application/x-bzip2" => Some("bz2"),
        "application/x-xz" => Some("xz"),
        "application/zstd" => Some("zst"),
        "application/x-7z-compressed" => Some("7z"),
        "application/vnd.rar" => Some("rar"),
        "application/x-tar" => Some("tar"),
        "application/vnd.microsoft.portable-executable"
        | "application/x-msdownload"
        | "application/x-dosexec" => Some("exe"),
        "application/x-elf" | "application/x-executable" => Some("elf"),
        "application/java-archive" => Some("jar"),
        "application/vnd.android.package-archive" => Some("apk"),
        "application/vnd.debian.binary-package" => Some("deb"),
        "application/x-rpm" => Some("rpm"),
        "application/x-appimage" => Some("AppImage"),
        "application/x-iso9660-image" => Some("iso"),
        "text/x-shellscript" | "application/x-shellscript" => Some("sh"),
        "text/x-python" | "application/x-python" => Some("py"),
        "application/javascript" | "text/javascript" => Some("js"),
        "text/html" => Some("html"),
        "text/css" => Some("css"),
        "application/json" => Some("json"),
        "application/xml" | "text/xml" => Some("xml"),
        "application/vnd.sqlite3" => Some("sqlite"),
        "font/woff" => Some("woff"),
        "font/woff2" => Some("woff2"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_clean_pdf_download() {
        let classifier = DownloadMimeClassifier::default();
        let input = MimeClassificationInput {
            url: Some("https://example.org/docs/annual-report.pdf?download=1".to_string()),
            filename: Some("annual-report.pdf".to_string()),
            content_type: Some("Application/PDF; charset=utf-8".to_string()),
            nosniff: true,
            magic_bytes: b"%PDF-1.7\n%\xe2\xe3\xcf\xd3".to_vec(),
            content_length: Some(128_000),
        };

        let result = classifier.classify(&input).expect("valid PDF input");
        assert_eq!(result.effective_mime, "application/pdf");
        assert_eq!(result.declared_mime.as_deref(), Some("application/pdf"));
        assert_eq!(result.sniffed_mime.as_deref(), Some("application/pdf"));
        assert_eq!(result.extension.as_deref(), Some("pdf"));
        assert_eq!(result.category, DownloadCategory::Document);
        assert_eq!(result.risk_level, RiskLevel::Low);
        assert_eq!(result.mismatch, MismatchKind::None);
        assert_eq!(result.recommended_action, DownloadAction::Allow);
        assert_eq!(result.suggested_extension.as_deref(), Some("pdf"));
    }

    #[test]
    fn detects_and_blocks_disguised_executable() {
        let classifier = DownloadMimeClassifier::default();
        let input = MimeClassificationInput {
            url: Some("https://cdn.example.net/files/photo.png".to_string()),
            filename: Some("vacation-photo.png".to_string()),
            content_type: Some("image/png".to_string()),
            nosniff: true,
            magic_bytes: b"\x7fELF\x02\x01\x01\x00\x00\x00\x00\x00".to_vec(),
            content_length: Some(42_000),
        };

        let result = classifier.classify(&input).expect("valid disguised input");
        assert_eq!(result.mismatch, MismatchKind::ExecutableDisguised);
        assert_eq!(result.risk_level, RiskLevel::Critical);
        assert_eq!(result.recommended_action, DownloadAction::Block);
        assert_eq!(result.sniffed_mime.as_deref(), Some("application/x-elf"));
    }

    #[test]
    fn upgrades_generic_octet_stream_and_handles_compound_extension() {
        let classifier = DownloadMimeClassifier::default();
        let input = MimeClassificationInput {
            url: Some("https://mirror.example.org/releases/search-linux-1.2.tar.gz".to_string()),
            filename: None,
            content_type: Some("application/octet-stream".to_string()),
            nosniff: true,
            magic_bytes: vec![0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00],
            content_length: Some(5_000_000),
        };

        let result = classifier.classify(&input).expect("valid tar.gz input");
        assert_eq!(result.extension.as_deref(), Some("tar.gz"));
        assert_eq!(result.effective_mime, "application/gzip");
        assert_eq!(result.category, DownloadCategory::Archive);
        assert_eq!(result.mismatch, MismatchKind::None);
        assert_eq!(result.recommended_action, DownloadAction::Allow);
    }

    #[test]
    fn rejects_invalid_inputs_and_serializes_policy_roundtrip() {
        let classifier = DownloadMimeClassifier::default();
        assert_eq!(
            classifier.classify(&MimeClassificationInput::default()),
            Err(ClassifierError::EmptyInput)
        );

        let bad_mime = MimeClassificationInput {
            content_type: Some("not-a-mime".to_string()),
            ..Default::default()
        };
        assert!(matches!(
            classifier.classify(&bad_mime),
            Err(ClassifierError::InvalidMimeType(_))
        ));

        let mut policy = MimeClassifierPolicy::default();
        policy
            .mime_overrides
            .insert("application/zip".to_string(), DownloadAction::Prompt);
        let json = policy.to_json().expect("serialize policy");
        let parsed = MimeClassifierPolicy::from_json(&json).expect("deserialize policy");
        assert_eq!(parsed, policy);
    }
}
