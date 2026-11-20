//! Screenshot filename formatter for Search Linux (`feat_37_screenshot_filename_fmt`).
//!
//! Provides configurable template parsing, metadata-driven token expansion,
//! POSIX/cross-platform filename sanitization, UTF-8 byte-budget truncation,
//! and pure conflict-resolution helpers.

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;
use time::{OffsetDateTime, UtcOffset};
use url::Url;

/// Maximum filename length in bytes on standard Linux filesystems (NAME_MAX).
pub const MAX_POSIX_FILENAME_BYTES: usize = 255;

/// Minimum allowed configured maximum filename length in bytes.
pub const MIN_CONFIGURED_FILENAME_BYTES: usize = 16;

/// Supported output image formats for screenshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    #[default]
    Png,
    Jpeg,
    Webp,
    Avif,
}

impl ImageFormat {
    /// Returns the lowercase file extension without a leading dot.
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
            Self::Avif => "avif",
        }
    }

    /// Returns the canonical MIME type for the image format.
    pub const fn mime_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Webp => "image/webp",
            Self::Avif => "image/avif",
        }
    }
}

impl fmt::Display for ImageFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.extension())
    }
}

/// Capture mode of the screenshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScreenshotMode {
    #[default]
    Visible,
    FullPage,
    Selection,
    Element,
    Window,
}

impl ScreenshotMode {
    /// Returns the slug representation used by `{mode}`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Visible => "visible",
            Self::FullPage => "fullpage",
            Self::Selection => "selection",
            Self::Element => "element",
            Self::Window => "window",
        }
    }
}

impl fmt::Display for ScreenshotMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Case normalization strategy applied to the formatted filename stem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CaseStyle {
    #[default]
    Preserve,
    Lowercase,
    Uppercase,
    KebabCase,
    SnakeCase,
}

/// Strategy for resolving filename collisions when a target path already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStrategy {
    #[default]
    AppendCounter,
    AppendTimestamp,
    Overwrite,
    Skip,
}

/// Supported template placeholder tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenKind {
    Title,
    Domain,
    Host,
    Port,
    PathSlug,
    Date,
    Time,
    Datetime,
    Timestamp,
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second,
    Mode,
    Width,
    Height,
    Resolution,
    Index,
}

impl TokenKind {
    /// Parses a token name from inside `{...}`.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "title" => Some(Self::Title),
            "domain" => Some(Self::Domain),
            "host" => Some(Self::Host),
            "port" => Some(Self::Port),
            "path_slug" => Some(Self::PathSlug),
            "date" => Some(Self::Date),
            "time" => Some(Self::Time),
            "datetime" => Some(Self::Datetime),
            "timestamp" => Some(Self::Timestamp),
            "year" => Some(Self::Year),
            "month" => Some(Self::Month),
            "day" => Some(Self::Day),
            "hour" => Some(Self::Hour),
            "minute" => Some(Self::Minute),
            "second" => Some(Self::Second),
            "mode" => Some(Self::Mode),
            "width" => Some(Self::Width),
            "height" => Some(Self::Height),
            "resolution" => Some(Self::Resolution),
            "index" => Some(Self::Index),
            _ => None,
        }
    }

    /// Returns the canonical token name.
    pub const fn as_name(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Domain => "domain",
            Self::Host => "host",
            Self::Port => "port",
            Self::PathSlug => "path_slug",
            Self::Date => "date",
            Self::Time => "time",
            Self::Datetime => "datetime",
            Self::Timestamp => "timestamp",
            Self::Year => "year",
            Self::Month => "month",
            Self::Day => "day",
            Self::Hour => "hour",
            Self::Minute => "minute",
            Self::Second => "second",
            Self::Mode => "mode",
            Self::Width => "width",
            Self::Height => "height",
            Self::Resolution => "resolution",
            Self::Index => "index",
        }
    }
}

/// A parsed segment of a screenshot filename template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum TemplateSegment {
    Literal(String),
    Token(TokenKind),
}

/// Errors produced during template parsing, config validation, or formatting.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FormatError {
    #[error("template must not be empty")]
    EmptyTemplate,

    #[error("unclosed '{{' in template at byte offset {0}")]
    UnclosedBrace(usize),

    #[error("unexpected '}}' in template at byte offset {0}")]
    UnexpectedClosingBrace(usize),

    #[error("empty token '{{}}' in template at byte offset {0}")]
    EmptyToken(usize),

    #[error("unknown template token '{{{0}}}'")]
    UnknownToken(String),

    #[error("max_filename_len ({0}) must be between {MIN_CONFIGURED_FILENAME_BYTES} and {MAX_POSIX_FILENAME_BYTES}")]
    InvalidMaxFilenameLen(usize),

    #[error("max_component_len ({0}) must be between 4 and {MAX_POSIX_FILENAME_BYTES}")]
    InvalidMaxComponentLen(usize),

    #[error("replacement_char '{0:?}' is not permitted in filenames")]
    InvalidReplacementChar(char),

    #[error("fallback_stem must produce a non-empty sanitized stem")]
    InvalidFallbackStem,

    #[error("template contains forbidden path traversal segment '..'")]
    PathTraversalNotAllowed,

    #[error("invalid timestamp ({timestamp}) or utc_offset_minutes ({offset_minutes})")]
    InvalidDateTime { timestamp: i64, offset_minutes: i32 },

    #[error("target filename already exists and conflict strategy is 'skip': {0}")]
    SkippedExisting(String),

    #[error("exhausted conflict resolution attempts for base path: {0}")]
    ConflictExhausted(String),
}

/// Configuration for formatting screenshot filenames.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilenameFormatConfig {
    pub template: String,
    pub format: ImageFormat,
    pub max_filename_len: usize,
    pub max_component_len: usize,
    pub replacement_char: char,
    pub collapse_repeated_separators: bool,
    pub trim_leading_trailing_separators: bool,
    pub allow_subdirs: bool,
    pub case_style: CaseStyle,
    pub fallback_stem: String,
    pub conflict_strategy: ConflictStrategy,
    pub ascii_only: bool,
}

impl Default for FilenameFormatConfig {
    fn default() -> Self {
        Self {
            template: "Screenshot_{date}_{time}_{domain}".to_string(),
            format: ImageFormat::Png,
            max_filename_len: 200,
            max_component_len: 80,
            replacement_char: '_',
            collapse_repeated_separators: true,
            trim_leading_trailing_separators: true,
            allow_subdirs: false,
            case_style: CaseStyle::Preserve,
            fallback_stem: "screenshot".to_string(),
            conflict_strategy: ConflictStrategy::AppendCounter,
            ascii_only: false,
        }
    }
}

impl FilenameFormatConfig {
    /// Validates the configuration and returns the parsed template segments on success.
    pub fn validate(&self) -> Result<Vec<TemplateSegment>, FormatError> {
        if !(MIN_CONFIGURED_FILENAME_BYTES..=MAX_POSIX_FILENAME_BYTES)
            .contains(&self.max_filename_len)
        {
            return Err(FormatError::InvalidMaxFilenameLen(self.max_filename_len));
        }

        if !(4..=MAX_POSIX_FILENAME_BYTES).contains(&self.max_component_len) {
            return Err(FormatError::InvalidMaxComponentLen(self.max_component_len));
        }

        if !is_safe_replacement_char(self.replacement_char) {
            return Err(FormatError::InvalidReplacementChar(self.replacement_char));
        }

        let sanitized_fallback = sanitize_component(
            &self.fallback_stem,
            self.replacement_char,
            self.collapse_repeated_separators,
            self.trim_leading_trailing_separators,
            self.ascii_only,
        );
        if sanitized_fallback.is_empty() || sanitized_fallback == "." || sanitized_fallback == ".."
        {
            return Err(FormatError::InvalidFallbackStem);
        }

        let segments = parse_template(&self.template)?;

        if self.allow_subdirs {
            // Check literal segments for explicit '..' path traversal.
            let mut joined = String::new();
            for seg in &segments {
                match seg {
                    TemplateSegment::Literal(s) => joined.push_str(s),
                    TemplateSegment::Token(_) => joined.push_str("token"),
                }
            }
            for part in joined.split(['/', '\\']) {
                if part == ".." {
                    return Err(FormatError::PathTraversalNotAllowed);
                }
            }
        }

        Ok(segments)
    }
}

/// Metadata about a captured screenshot used to populate template tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenshotMetadata {
    pub url: Option<String>,
    pub title: Option<String>,
    pub captured_at_unix_secs: i64,
    pub utc_offset_minutes: i32,
    pub mode: ScreenshotMode,
    pub width: u32,
    pub height: u32,
    pub counter: Option<u32>,
}

impl Default for ScreenshotMetadata {
    fn default() -> Self {
        Self {
            url: None,
            title: None,
            captured_at_unix_secs: 0,
            utc_offset_minutes: 0,
            mode: ScreenshotMode::Visible,
            width: 1280,
            height: 720,
            counter: None,
        }
    }
}

/// Result of formatting a screenshot filename.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormattedFilename {
    /// Relative path including optional subdirectory prefix and `file_name`.
    pub relative_path: String,
    /// Optional sanitized subdirectory path (never contains `..` or leading `/`).
    pub directory: Option<String>,
    /// Final filename including extension (guaranteed `<= max_filename_len` bytes).
    pub file_name: String,
    /// Filename stem without extension.
    pub stem: String,
    /// Lowercase extension without leading dot.
    pub extension: String,
}

/// Parses a filename template into literal and token segments.
///
/// Escaped braces `{{` and `}}` are decoded to literal `{` and `}`.
pub fn parse_template(template: &str) -> Result<Vec<TemplateSegment>, FormatError> {
    if template.trim().is_empty() {
        return Err(FormatError::EmptyTemplate);
    }

    let mut segments = Vec::new();
    let mut literal = String::new();
    let bytes = template.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                    literal.push('{');
                    i += 2;
                    continue;
                }
                if !literal.is_empty() {
                    segments.push(TemplateSegment::Literal(std::mem::take(&mut literal)));
                }
                let start = i + 1;
                let mut end = start;
                while end < bytes.len() && bytes[end] != b'}' {
                    if bytes[end] == b'{' {
                        return Err(FormatError::UnclosedBrace(i));
                    }
                    end += 1;
                }
                if end >= bytes.len() {
                    return Err(FormatError::UnclosedBrace(i));
                }
                let token_name = &template[start..end];
                if token_name.is_empty() {
                    return Err(FormatError::EmptyToken(i));
                }
                let kind = TokenKind::from_name(token_name)
                    .ok_or_else(|| FormatError::UnknownToken(token_name.to_string()))?;
                segments.push(TemplateSegment::Token(kind));
                i = end + 1;
            }
            b'}' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'}' {
                    literal.push('}');
                    i += 2;
                } else {
                    return Err(FormatError::UnexpectedClosingBrace(i));
                }
            }
            _ => {
                let ch = template[i..]
                    .chars()
                    .next()
                    .ok_or(FormatError::UnclosedBrace(i))?;
                literal.push(ch);
                i += ch.len_utf8();
            }
        }
    }

    if !literal.is_empty() {
        segments.push(TemplateSegment::Literal(literal));
    }

    Ok(segments)
}

/// Formats a screenshot filename according to `config` and `meta`.
pub fn format_filename(
    config: &FilenameFormatConfig,
    meta: &ScreenshotMetadata,
) -> Result<FormattedFilename, FormatError> {
    let segments = config.validate()?;
    let dt = build_datetime(meta.captured_at_unix_secs, meta.utc_offset_minutes)?;
    let parsed_url = meta.url.as_deref().and_then(|raw| Url::parse(raw).ok());

    let mut rendered = String::new();
    for seg in &segments {
        match seg {
            TemplateSegment::Literal(text) => {
                rendered.push_str(text);
            }
            TemplateSegment::Token(kind) => {
                let raw_val = resolve_token(*kind, meta, parsed_url.as_ref(), dt);
                let sanitized_val = sanitize_component(
                    &raw_val,
                    config.replacement_char,
                    config.collapse_repeated_separators,
                    config.trim_leading_trailing_separators,
                    config.ascii_only,
                );
                let bounded = truncate_utf8_bytes(&sanitized_val, config.max_component_len);
                rendered.push_str(bounded);
            }
        }
    }

    let (raw_dir, raw_stem) = if config.allow_subdirs {
        split_subdirs(&rendered)
    } else {
        (None, rendered)
    };

    let directory = raw_dir.and_then(|dir_str| {
        let mut clean_parts = Vec::new();
        for part in dir_str.split(['/', '\\']) {
            let cleaned = sanitize_component(
                part,
                config.replacement_char,
                config.collapse_repeated_separators,
                config.trim_leading_trailing_separators,
                config.ascii_only,
            );
            if !cleaned.is_empty() && cleaned != "." && cleaned != ".." {
                let bounded = truncate_utf8_bytes(&cleaned, config.max_component_len);
                if !bounded.is_empty() {
                    clean_parts.push(escape_reserved_windows_name(
                        bounded,
                        config.replacement_char,
                    ));
                }
            }
        }
        if clean_parts.is_empty() {
            None
        } else {
            Some(clean_parts.join("/"))
        }
    });

    let mut stem = sanitize_component(
        &raw_stem,
        config.replacement_char,
        config.collapse_repeated_separators,
        config.trim_leading_trailing_separators,
        config.ascii_only,
    );

    stem = apply_case_style(&stem, config.case_style, config.replacement_char);

    if stem.is_empty() || stem == "." || stem == ".." {
        let fallback = sanitize_component(
            &config.fallback_stem,
            config.replacement_char,
            config.collapse_repeated_separators,
            config.trim_leading_trailing_separators,
            config.ascii_only,
        );
        stem = apply_case_style(&fallback, config.case_style, config.replacement_char);
    }

    stem = escape_reserved_windows_name(&stem, config.replacement_char);

    let ext = config.format.extension();
    let ext_overhead = 1 + ext.len(); // '.' + ext
    let max_stem_bytes = config.max_filename_len.saturating_sub(ext_overhead);
    let truncated_stem = truncate_utf8_bytes(&stem, max_stem_bytes).to_string();
    let final_stem = if truncated_stem.is_empty() {
        "shot".to_string()
    } else {
        truncated_stem
    };

    let file_name = format!("{final_stem}.{ext}");
    let relative_path = match &directory {
        Some(dir) => format!("{dir}/{file_name}"),
        None => file_name.clone(),
    };

    Ok(FormattedFilename {
        relative_path,
        directory,
        file_name,
        stem: final_stem,
        extension: ext.to_string(),
    })
}

/// Resolves filename collisions using a caller-supplied existence predicate.
///
/// The predicate `exists(candidate_path)` allows pure in-memory testing as well
/// as real filesystem checks without coupling this module to `std::fs`.
pub fn resolve_conflict<F>(
    formatted: &FormattedFilename,
    strategy: ConflictStrategy,
    timestamp_secs: i64,
    max_filename_len: usize,
    mut exists: F,
) -> Result<FormattedFilename, FormatError>
where
    F: FnMut(&str) -> bool,
{
    if !exists(&formatted.relative_path) {
        return Ok(formatted.clone());
    }

    match strategy {
        ConflictStrategy::Overwrite => Ok(formatted.clone()),
        ConflictStrategy::Skip => Err(FormatError::SkippedExisting(
            formatted.relative_path.clone(),
        )),
        ConflictStrategy::AppendTimestamp => {
            let suffix = format!("_{timestamp_secs}");
            let candidate = build_suffixed_filename(formatted, &suffix, max_filename_len);
            if !exists(&candidate.relative_path) {
                return Ok(candidate);
            }
            // Fall through to counter if same-second collision occurs.
            append_counter_loop(formatted, &candidate.stem, max_filename_len, exists)
        }
        ConflictStrategy::AppendCounter => {
            append_counter_loop(formatted, &formatted.stem, max_filename_len, exists)
        }
    }
}

fn append_counter_loop<F>(
    base: &FormattedFilename,
    stem_base: &str,
    max_filename_len: usize,
    mut exists: F,
) -> Result<FormattedFilename, FormatError>
where
    F: FnMut(&str) -> bool,
{
    for idx in 1u32..=9999 {
        let suffix = format!("_{idx}");
        let mut candidate = base.clone();
        candidate.stem = stem_base.to_string();
        candidate = build_suffixed_filename(&candidate, &suffix, max_filename_len);
        if !exists(&candidate.relative_path) {
            return Ok(candidate);
        }
    }
    Err(FormatError::ConflictExhausted(base.relative_path.clone()))
}

fn build_suffixed_filename(
    base: &FormattedFilename,
    suffix: &str,
    max_filename_len: usize,
) -> FormattedFilename {
    let ext_overhead = 1 + base.extension.len();
    let max_stem_bytes = max_filename_len
        .min(MAX_POSIX_FILENAME_BYTES)
        .saturating_sub(ext_overhead);
    let allowed_prefix = max_stem_bytes.saturating_sub(suffix.len());
    let prefix = truncate_utf8_bytes(&base.stem, allowed_prefix);
    let new_stem = format!("{prefix}{suffix}");
    let file_name = format!("{new_stem}.{}", base.extension);
    let relative_path = match &base.directory {
        Some(dir) => format!("{dir}/{file_name}"),
        None => file_name.clone(),
    };

    FormattedFilename {
        relative_path,
        directory: base.directory.clone(),
        file_name,
        stem: new_stem,
        extension: base.extension.clone(),
    }
}

fn build_datetime(unix_secs: i64, offset_minutes: i32) -> Result<OffsetDateTime, FormatError> {
    if !(-1439..=1439).contains(&offset_minutes) {
        return Err(FormatError::InvalidDateTime {
            timestamp: unix_secs,
            offset_minutes,
        });
    }
    let offset = UtcOffset::from_whole_seconds(offset_minutes * 60).map_err(|_| {
        FormatError::InvalidDateTime {
            timestamp: unix_secs,
            offset_minutes,
        }
    })?;
    let utc = OffsetDateTime::from_unix_timestamp(unix_secs).map_err(|_| {
        FormatError::InvalidDateTime {
            timestamp: unix_secs,
            offset_minutes,
        }
    })?;
    Ok(utc.to_offset(offset))
}

fn resolve_token(
    kind: TokenKind,
    meta: &ScreenshotMetadata,
    parsed_url: Option<&Url>,
    dt: OffsetDateTime,
) -> String {
    match kind {
        TokenKind::Title => meta
            .title
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("untitled")
            .to_string(),
        TokenKind::Domain => parsed_url
            .and_then(|u| u.host_str())
            .map(|h| h.strip_prefix("www.").unwrap_or(h))
            .unwrap_or("local")
            .to_string(),
        TokenKind::Host => parsed_url
            .and_then(|u| u.host_str())
            .unwrap_or("local")
            .to_string(),
        TokenKind::Port => parsed_url
            .and_then(|u| u.port_or_known_default())
            .map(|p| p.to_string())
            .unwrap_or_else(|| "0".to_string()),
        TokenKind::PathSlug => parsed_url
            .map(|u| {
                let trimmed = u.path().trim_matches('/');
                if trimmed.is_empty() {
                    "index".to_string()
                } else {
                    trimmed.replace('/', "_")
                }
            })
            .unwrap_or_else(|| "index".to_string()),
        TokenKind::Date => format!(
            "{:04}-{:02}-{:02}",
            dt.year(),
            u8::from(dt.month()),
            dt.day()
        ),
        TokenKind::Time => format!("{:02}-{:02}-{:02}", dt.hour(), dt.minute(), dt.second()),
        TokenKind::Datetime => format!(
            "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}",
            dt.year(),
            u8::from(dt.month()),
            dt.day(),
            dt.hour(),
            dt.minute(),
            dt.second()
        ),
        TokenKind::Timestamp => meta.captured_at_unix_secs.to_string(),
        TokenKind::Year => format!("{:04}", dt.year()),
        TokenKind::Month => format!("{:02}", u8::from(dt.month())),
        TokenKind::Day => format!("{:02}", dt.day()),
        TokenKind::Hour => format!("{:02}", dt.hour()),
        TokenKind::Minute => format!("{:02}", dt.minute()),
        TokenKind::Second => format!("{:02}", dt.second()),
        TokenKind::Mode => meta.mode.as_str().to_string(),
        TokenKind::Width => meta.width.to_string(),
        TokenKind::Height => meta.height.to_string(),
        TokenKind::Resolution => format!("{}x{}", meta.width, meta.height),
        TokenKind::Index => meta.counter.unwrap_or(1).to_string(),
    }
}

fn is_safe_replacement_char(ch: char) -> bool {
    !ch.is_control()
        && !matches!(
            ch,
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0'
        )
}

fn is_forbidden_filename_char(ch: char) -> bool {
    ch.is_control()
        || matches!(
            ch,
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0'
        )
}

fn sanitize_component(
    input: &str,
    replacement: char,
    collapse_repeated: bool,
    trim_edges: bool,
    ascii_only: bool,
) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_was_rep = false;

    for ch in input.chars() {
        let mapped = if is_forbidden_filename_char(ch)
            || ch.is_whitespace()
            || (ascii_only && !ch.is_ascii())
        {
            Some(replacement)
        } else {
            Some(ch)
        };

        if let Some(c) = mapped {
            if collapse_repeated && c == replacement {
                if prev_was_rep {
                    continue;
                }
                prev_was_rep = true;
            } else {
                prev_was_rep = false;
            }
            out.push(c);
        }
    }

    if trim_edges {
        let trimmed = out.trim_matches(|c: char| c == replacement || c == '.' || c.is_whitespace());
        trimmed.to_string()
    } else {
        out
    }
}

fn apply_case_style(input: &str, style: CaseStyle, replacement: char) -> String {
    match style {
        CaseStyle::Preserve => input.to_string(),
        CaseStyle::Lowercase => input.to_lowercase(),
        CaseStyle::Uppercase => input.to_uppercase(),
        CaseStyle::KebabCase => input
            .chars()
            .map(|c| {
                if c == replacement || c == '_' || c.is_whitespace() {
                    '-'
                } else {
                    c
                }
            })
            .collect::<String>()
            .to_lowercase(),
        CaseStyle::SnakeCase => input
            .chars()
            .map(|c| {
                if c == replacement || c == '-' || c.is_whitespace() {
                    '_'
                } else {
                    c
                }
            })
            .collect::<String>()
            .to_lowercase(),
    }
}

fn split_subdirs(rendered: &str) -> (Option<String>, String) {
    match rendered.rfind(['/', '\\']) {
        Some(idx) => {
            let dir = &rendered[..idx];
            let file = &rendered[idx + 1..];
            if dir.is_empty() {
                (None, file.to_string())
            } else {
                (Some(dir.to_string()), file.to_string())
            }
        }
        None => (None, rendered.to_string()),
    }
}

fn truncate_utf8_bytes(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn escape_reserved_windows_name(stem: &str, replacement: char) -> String {
    let upper = stem.to_ascii_uppercase();
    let is_reserved = matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );
    if is_reserved {
        format!("{stem}{replacement}file")
    } else {
        stem.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn sample_meta() -> ScreenshotMetadata {
        ScreenshotMetadata {
            url: Some("https://www.example.com/docs/getting-started?q=1".to_string()),
            title: Some("Getting Started: Search Linux / Docs!".to_string()),
            // 2026-11-19 19:39:05 UTC
            captured_at_unix_secs: 1_795_117_145,
            utc_offset_minutes: -240, // 15:39:05 local
            mode: ScreenshotMode::FullPage,
            width: 1920,
            height: 1080,
            counter: Some(3),
        }
    }

    #[test]
    fn test_default_format_happy_path() {
        let cfg = FilenameFormatConfig::default();
        let meta = sample_meta();
        let out = format_filename(&cfg, &meta).unwrap();

        assert_eq!(out.extension, "png");
        assert_eq!(out.directory, None);
        assert_eq!(
            out.file_name,
            "Screenshot_2026-11-19_15-39-05_example.com.png"
        );
        assert_eq!(out.relative_path, out.file_name);
        assert!(out.file_name.len() <= cfg.max_filename_len);
    }

    #[test]
    fn test_template_escaping_and_all_tokens() {
        let cfg = FilenameFormatConfig {
            template: "{{shot}}_{year}{month}{day}_{mode}_{resolution}_{path_slug}_{index}"
                .to_string(),
            format: ImageFormat::Webp,
            case_style: CaseStyle::KebabCase,
            ..Default::default()
        };
        let meta = sample_meta();
        let out = format_filename(&cfg, &meta).unwrap();

        assert_eq!(out.extension, "webp");
        assert_eq!(
            out.file_name,
            "{shot}-20261119-fullpage-1920x1080-docs-getting-started-3.webp"
        );
    }

    #[test]
    fn test_sanitization_utf8_truncation_and_reserved_names() {
        let cfg = FilenameFormatConfig {
            template: "{title}".to_string(),
            format: ImageFormat::Jpeg,
            max_filename_len: 24,
            max_component_len: 100,
            ..Default::default()
        };

        // Multi-byte UTF-8 characters must not split mid-codepoint during truncation.
        let mut meta = sample_meta();
        meta.title = Some("Café_Über_日本語_Very_Long_Title".to_string());
        let out = format_filename(&cfg, &meta).unwrap();
        assert!(out.file_name.len() <= 24);
        assert!(out.file_name.ends_with(".jpg"));
        assert!(std::str::from_utf8(out.file_name.as_bytes()).is_ok());

        // Windows reserved name must be escaped.
        meta.title = Some("CON".to_string());
        let reserved = format_filename(&cfg, &meta).unwrap();
        assert_eq!(reserved.file_name, "CON_file.jpg");
    }

    #[test]
    fn test_subdirs_and_path_traversal_rejection() {
        let bad_cfg = FilenameFormatConfig {
            template: "../{domain}/{title}".to_string(),
            allow_subdirs: true,
            ..Default::default()
        };
        assert_eq!(
            bad_cfg.validate(),
            Err(FormatError::PathTraversalNotAllowed)
        );

        let good_cfg = FilenameFormatConfig {
            template: "{domain}/{date}_{title}".to_string(),
            allow_subdirs: true,
            ..Default::default()
        };
        let mut meta = sample_meta();
        meta.title = Some("../../etc/passwd".to_string());
        let out = format_filename(&good_cfg, &meta).unwrap();
        assert_eq!(out.directory.as_deref(), Some("example.com"));
        assert!(!out.relative_path.contains(".."));
    }

    #[test]
    fn test_conflict_resolution_strategies() {
        let cfg = FilenameFormatConfig::default();
        let meta = sample_meta();
        let base = format_filename(&cfg, &meta).unwrap();

        let mut existing = HashSet::new();
        existing.insert(base.relative_path.clone());
        existing.insert("Screenshot_2026-11-19_15-39-05_example.com_1.png".to_string());

        let resolved = resolve_conflict(
            &base,
            ConflictStrategy::AppendCounter,
            meta.captured_at_unix_secs,
            cfg.max_filename_len,
            |p| existing.contains(p),
        )
        .unwrap();
        assert_eq!(
            resolved.file_name,
            "Screenshot_2026-11-19_15-39-05_example.com_2.png"
        );

        let skipped = resolve_conflict(
            &base,
            ConflictStrategy::Skip,
            meta.captured_at_unix_secs,
            cfg.max_filename_len,
            |p| existing.contains(p),
        );
        assert!(matches!(skipped, Err(FormatError::SkippedExisting(_))));
    }

    #[test]
    fn test_config_serde_roundtrip_and_template_errors() {
        let cfg = FilenameFormatConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let decoded: FilenameFormatConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, decoded);

        assert_eq!(parse_template("   "), Err(FormatError::EmptyTemplate));
        assert_eq!(parse_template("{title"), Err(FormatError::UnclosedBrace(0)));
        assert_eq!(
            parse_template("shot}"),
            Err(FormatError::UnexpectedClosingBrace(4))
        );
        assert_eq!(parse_template("{}"), Err(FormatError::EmptyToken(0)));
        assert_eq!(
            parse_template("{nonexistent}"),
            Err(FormatError::UnknownToken("nonexistent".to_string()))
        );
    }
}
