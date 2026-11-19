//! Reader Bionic Formatter (Feature 31).
//!
//! Transforms plain text, Markdown, or HTML fragments into bionic-reading
//! formatted text by emphasizing the initial fixation prefix of eligible words
//! while preserving HTML tags, entities, code spans, URLs, and UTF-8 boundaries.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Default maximum input size in bytes (2 MiB).
pub const DEFAULT_MAX_INPUT_BYTES: usize = 2 * 1024 * 1024;

/// Errors returned by [`BionicConfig::validate`] and [`BionicFormatter::format`].
#[derive(Debug, Error, Clone, PartialEq)]
pub enum BionicError {
    #[error("fixation_ratio must be finite and in (0.0, 1.0], got {0}")]
    InvalidFixationRatio(f32),

    #[error("saccade_stride must be at least 1, got {0}")]
    InvalidSaccadeStride(usize),

    #[error("min_word_len must be at least 1, got {0}")]
    InvalidMinWordLen(usize),

    #[error("max_fixation_chars must be at least 1 when set, got 0")]
    InvalidMaxFixationChars,

    #[error("custom output markers must not both be empty")]
    EmptyCustomMarkers,

    #[error("input length {len} bytes exceeds maximum allowed {max} bytes")]
    InputTooLarge { len: usize, max: usize },
}

/// Output markup format for fixation prefixes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[derive(Default)]
pub enum OutputFormat {
    /// Wraps fixation prefixes in `<b>...</b>`.
    #[default]
    HtmlBold,
    /// Wraps fixation prefixes in `<strong>...</strong>`.
    HtmlStrong,
    /// Wraps fixation prefixes in `**...**`.
    Markdown,
    /// Wraps fixation prefixes with caller-supplied prefix and suffix strings.
    Custom { prefix: String, suffix: String },
}

impl OutputFormat {
    fn markers(&self) -> (&str, &str) {
        match self {
            Self::HtmlBold => ("<b>", "</b>"),
            Self::HtmlStrong => ("<strong>", "</strong>"),
            Self::Markdown => ("**", "**"),
            Self::Custom { prefix, suffix } => (prefix.as_str(), suffix.as_str()),
        }
    }

    fn is_html(&self) -> bool {
        matches!(self, Self::HtmlBold | Self::HtmlStrong)
    }
}

/// Configuration for [`BionicFormatter`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BionicConfig {
    /// Fraction of each eligible word's characters to emphasize, in `(0.0, 1.0]`.
    pub fixation_ratio: f32,
    /// Format every `saccade_stride`-th eligible word (`1` formats every eligible word).
    pub saccade_stride: usize,
    /// Minimum alphabetic/alphanumeric word length (in Unicode scalar values) to format.
    pub min_word_len: usize,
    /// Optional upper bound on emphasized characters per word.
    pub max_fixation_chars: Option<usize>,
    /// Skip URLs (`http://`, `https://`, `www.`) without formatting.
    pub skip_urls: bool,
    /// Skip inline code spans (`` `...` ``) and `<code>` / `<pre>` blocks.
    pub skip_code_spans: bool,
    /// Preserve `<...>` HTML tags and `&...;` entities verbatim.
    pub preserve_html_tags: bool,
    /// Escape `&`, `<`, and `>` in plain text when `preserve_html_tags` is false and output is HTML.
    pub escape_html_text: bool,
    /// Markup format used for emphasized prefixes.
    pub format: OutputFormat,
    /// Maximum accepted input size in bytes.
    pub max_input_bytes: usize,
}

impl Default for BionicConfig {
    fn default() -> Self {
        Self {
            fixation_ratio: 0.45,
            saccade_stride: 1,
            min_word_len: 2,
            max_fixation_chars: None,
            skip_urls: true,
            skip_code_spans: true,
            preserve_html_tags: true,
            escape_html_text: false,
            format: OutputFormat::HtmlBold,
            max_input_bytes: DEFAULT_MAX_INPUT_BYTES,
        }
    }
}

impl BionicConfig {
    /// Validates all configuration invariants.
    pub fn validate(&self) -> Result<(), BionicError> {
        if !self.fixation_ratio.is_finite()
            || self.fixation_ratio <= 0.0
            || self.fixation_ratio > 1.0
        {
            return Err(BionicError::InvalidFixationRatio(self.fixation_ratio));
        }
        if self.saccade_stride == 0 {
            return Err(BionicError::InvalidSaccadeStride(self.saccade_stride));
        }
        if self.min_word_len == 0 {
            return Err(BionicError::InvalidMinWordLen(self.min_word_len));
        }
        if self.max_fixation_chars == Some(0) {
            return Err(BionicError::InvalidMaxFixationChars);
        }
        if let OutputFormat::Custom { prefix, suffix } = &self.format {
            if prefix.is_empty() && suffix.is_empty() {
                return Err(BionicError::EmptyCustomMarkers);
            }
        }
        Ok(())
    }
}

/// Summary statistics collected during a formatting pass.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BionicStats {
    /// Total word tokens encountered (excluding HTML tags, entities, and skipped code/URLs).
    pub total_words: usize,
    /// Words that received fixation emphasis.
    pub formatted_words: usize,
    /// Words shorter than `min_word_len` or skipped by `saccade_stride`.
    pub skipped_words: usize,
    /// URL tokens skipped when `skip_urls` is enabled.
    pub skipped_urls: usize,
    /// Code blocks/spans skipped when `skip_code_spans` is enabled.
    pub skipped_code_segments: usize,
    /// Total number of characters emphasized across all formatted words.
    pub total_fixated_chars: usize,
}

/// Output of [`BionicFormatter::format`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BionicOutput {
    /// Formatted text.
    pub formatted: String,
    /// Per-document formatting statistics.
    pub stats: BionicStats,
}

/// Stateful bionic reading formatter built from a validated [`BionicConfig`].
#[derive(Debug, Clone)]
pub struct BionicFormatter {
    config: BionicConfig,
}

impl BionicFormatter {
    /// Creates a new formatter after validating `config`.
    pub fn new(config: BionicConfig) -> Result<Self, BionicError> {
        config.validate()?;
        Ok(Self { config })
    }

    /// Creates a formatter with [`BionicConfig::default`].
    pub fn with_defaults() -> Self {
        Self {
            config: BionicConfig::default(),
        }
    }

    /// Returns the active configuration.
    pub fn config(&self) -> &BionicConfig {
        &self.config
    }

    /// Formats `input` and returns both the formatted string and statistics.
    pub fn format(&self, input: &str) -> Result<BionicOutput, BionicError> {
        if input.len() > self.config.max_input_bytes {
            return Err(BionicError::InputTooLarge {
                len: input.len(),
                max: self.config.max_input_bytes,
            });
        }

        let mut out = String::with_capacity(input.len() + input.len() / 3);
        let mut stats = BionicStats::default();
        let (prefix, suffix) = self.config.format.markers();
        let escape_text = !self.config.preserve_html_tags
            && self.config.escape_html_text
            && self.config.format.is_html();

        let bytes = input.as_bytes();
        let mut i = 0usize;
        let mut eligible_word_index = 0usize;
        let mut raw_tag_depth = 0usize; // <script> or <style>
        let mut code_tag_depth = 0usize; // <code> or <pre>

        while i < input.len() {
            let rest = &input[i..];

            // 1. HTML tag handling
            if self.config.preserve_html_tags && bytes[i] == b'<' && looks_like_tag_start(rest) {
                let tag_end = find_tag_end(rest);
                let tag = &rest[..tag_end];
                update_tag_depths(tag, &mut raw_tag_depth, &mut code_tag_depth);
                out.push_str(tag);
                i += tag_end;
                continue;
            }

            // Inside <script> or <style>, always copy verbatim.
            if raw_tag_depth > 0 {
                if let Some(ch) = rest.chars().next() {
                    out.push(ch);
                    i += ch.len_utf8();
                } else {
                    break;
                }
                continue;
            }

            // Inside <code> or <pre> when skipping code spans.
            if self.config.skip_code_spans && code_tag_depth > 0 {
                if let Some(ch) = rest.chars().next() {
                    out.push(ch);
                    i += ch.len_utf8();
                } else {
                    break;
                }
                continue;
            }

            // 2. HTML entity handling (&amp;, &#39;, &#x27;, etc.)
            if self.config.preserve_html_tags && bytes[i] == b'&' {
                if let Some(entity_len) = match_html_entity(rest) {
                    out.push_str(&rest[..entity_len]);
                    i += entity_len;
                    continue;
                }
            }

            // 3. Markdown code blocks (```...```) and inline code (`...`)
            if self.config.skip_code_spans && bytes[i] == b'`' {
                let span_len = match_backtick_span(rest);
                out.push_str(&rest[..span_len]);
                stats.skipped_code_segments += 1;
                i += span_len;
                continue;
            }

            // 4. URL detection
            if self.config.skip_urls && looks_like_url_start(rest) {
                let url_len = scan_url_token(rest);
                if url_len > 0 {
                    push_maybe_escaped(&mut out, &rest[..url_len], escape_text);
                    stats.skipped_urls += 1;
                    i += url_len;
                    continue;
                }
            }

            // 5. Word or non-word character
            let Some(ch) = rest.chars().next() else {
                break;
            };

            if is_word_char(ch) {
                let word_end = rest
                    .char_indices()
                    .find(|&(_, c)| !is_word_char(c))
                    .map(|(idx, _)| idx)
                    .unwrap_or(rest.len());
                let word = &rest[..word_end];
                let char_len = word.chars().count();
                stats.total_words += 1;

                if char_len < self.config.min_word_len {
                    push_maybe_escaped(&mut out, word, escape_text);
                    stats.skipped_words += 1;
                } else {
                    let should_format =
                        eligible_word_index.is_multiple_of(self.config.saccade_stride);
                    eligible_word_index += 1;

                    if should_format {
                        let fix_chars = fixation_char_count(char_len, &self.config);
                        let split_byte = word
                            .char_indices()
                            .nth(fix_chars)
                            .map(|(idx, _)| idx)
                            .unwrap_or(word.len());
                        let (head, tail) = word.split_at(split_byte);

                        out.push_str(prefix);
                        push_maybe_escaped(&mut out, head, escape_text);
                        out.push_str(suffix);
                        push_maybe_escaped(&mut out, tail, escape_text);

                        stats.formatted_words += 1;
                        stats.total_fixated_chars += fix_chars;
                    } else {
                        push_maybe_escaped(&mut out, word, escape_text);
                        stats.skipped_words += 1;
                    }
                }

                i += word_end;
            } else {
                push_char_maybe_escaped(&mut out, ch, escape_text);
                i += ch.len_utf8();
            }
        }

        Ok(BionicOutput {
            formatted: out,
            stats,
        })
    }

    /// Convenience wrapper returning only the formatted string.
    pub fn format_text(&self, input: &str) -> Result<String, BionicError> {
        self.format(input).map(|output| output.formatted)
    }
}

/// Computes how many leading characters of a word of `char_len` characters should be emphasized.
pub fn fixation_char_count(char_len: usize, config: &BionicConfig) -> usize {
    if char_len == 0 || char_len < config.min_word_len {
        return 0;
    }
    let raw = (char_len as f32 * config.fixation_ratio).ceil() as usize;
    let mut count = raw.clamp(1, char_len);
    if let Some(cap) = config.max_fixation_chars {
        count = count.min(cap);
    }
    count.max(1).min(char_len)
}

#[inline]
fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric()
}

fn looks_like_tag_start(s: &str) -> bool {
    let mut chars = s.chars();
    if chars.next() != Some('<') {
        return false;
    }
    match chars.next() {
        Some(c) => c.is_ascii_alphabetic() || c == '/' || c == '!' || c == '?',
        None => false,
    }
}

fn find_tag_end(s: &str) -> usize {
    let mut in_quote: Option<char> = None;
    for (idx, ch) in s.char_indices().skip(1) {
        match in_quote {
            Some(q) if ch == q => in_quote = None,
            Some(_) => {}
            None => {
                if ch == '"' || ch == '\'' {
                    in_quote = Some(ch);
                } else if ch == '>' {
                    return idx + 1;
                }
            }
        }
    }
    s.len()
}

fn update_tag_depths(tag: &str, raw_depth: &mut usize, code_depth: &mut usize) {
    let inner = tag.trim_start_matches('<').trim_end_matches('>');
    let is_closing = inner.starts_with('/');
    let is_self_closing = inner.ends_with('/');
    if is_self_closing {
        return;
    }
    let name_part = inner.trim_start_matches('/');
    let tag_name_end = name_part
        .find(|c: char| c.is_ascii_whitespace() || c == '/')
        .unwrap_or(name_part.len());
    let name = &name_part[..tag_name_end];

    if name.eq_ignore_ascii_case("script") || name.eq_ignore_ascii_case("style") {
        if is_closing {
            *raw_depth = raw_depth.saturating_sub(1);
        } else {
            *raw_depth += 1;
        }
    } else if name.eq_ignore_ascii_case("code") || name.eq_ignore_ascii_case("pre") {
        if is_closing {
            *code_depth = code_depth.saturating_sub(1);
        } else {
            *code_depth += 1;
        }
    }
}

fn match_html_entity(s: &str) -> Option<usize> {
    if !s.starts_with('&') {
        return None;
    }
    let max_scan = s.len().min(16);
    let slice = &s[..max_scan];
    let semi_pos = slice.find(';')?;
    if semi_pos < 2 {
        return None;
    }
    let body = &slice[1..semi_pos];
    let valid = if let Some(hex) = body.strip_prefix("#x").or_else(|| body.strip_prefix("#X")) {
        !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit())
    } else if let Some(dec) = body.strip_prefix('#') {
        !dec.is_empty() && dec.chars().all(|c| c.is_ascii_digit())
    } else {
        body.chars().all(|c| c.is_ascii_alphanumeric())
    };
    if valid {
        Some(semi_pos + 1)
    } else {
        None
    }
}

fn match_backtick_span(s: &str) -> usize {
    let fence_len = s.bytes().take_while(|&b| b == b'`').count();
    if fence_len == 0 {
        return 0;
    }
    let after_open = &s[fence_len..];
    let mut idx = 0usize;
    while idx < after_open.len() {
        if after_open.as_bytes()[idx] == b'`' {
            let run = after_open[idx..].bytes().take_while(|&b| b == b'`').count();
            if run >= fence_len {
                return fence_len + idx + run;
            }
            idx += run;
        } else if let Some(ch) = after_open[idx..].chars().next() {
            idx += ch.len_utf8();
        } else {
            break;
        }
    }
    // Unclosed backtick: consume just the opening run so normal text isn't swallowed.
    fence_len
}

fn looks_like_url_start(s: &str) -> bool {
    s.starts_with("https://") || s.starts_with("http://") || s.starts_with("www.")
}

fn scan_url_token(s: &str) -> usize {
    let mut end = s
        .char_indices()
        .find(|&(_, c)| c.is_whitespace() || c == '<' || c == '>' || c == '"' || c == '\'')
        .map(|(idx, _)| idx)
        .unwrap_or(s.len());

    while end > 0 {
        let last = s[..end].chars().next_back();
        match last {
            Some('.' | ',' | ';' | ':' | '!' | '?' | ')') => {
                if let Some(ch) = last {
                    end -= ch.len_utf8();
                }
            }
            _ => break,
        }
    }
    end
}

fn push_maybe_escaped(out: &mut String, text: &str, escape: bool) {
    if !escape {
        out.push_str(text);
        return;
    }
    for ch in text.chars() {
        push_char_maybe_escaped(out, ch, true);
    }
}

fn push_char_maybe_escaped(out: &mut String, ch: char, escape: bool) {
    if !escape {
        out.push(ch);
        return;
    }
    match ch {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        _ => out.push(ch),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_plain_text_html_bold_and_tracks_stats() {
        let formatter = BionicFormatter::with_defaults();
        let output = formatter
            .format("A quick brown fox jumps.")
            .expect("valid input");

        // "A" (len 1) is skipped; "quick"(5)->3, "brown"(5)->3, "fox"(3)->2, "jumps"(5)->3.
        assert_eq!(
            output.formatted,
            "A <b>qui</b>ck <b>bro</b>wn <b>fo</b>x <b>jum</b>ps."
        );
        assert_eq!(output.stats.total_words, 5);
        assert_eq!(output.stats.formatted_words, 4);
        assert_eq!(output.stats.skipped_words, 1);
        assert_eq!(output.stats.total_fixated_chars, 11);
    }

    #[test]
    fn preserves_html_tags_entities_code_and_urls() {
        let formatter = BionicFormatter::with_defaults();
        let input = r#"<p class="lead">Read &amp; visit https://example.com/docs, or run `cargo test` inside <code>search_core</code>.</p>"#;
        let output = formatter.format(input).expect("valid html");

        assert!(output.formatted.starts_with(r#"<p class="lead">"#));
        assert!(output.formatted.contains("&amp;"));
        assert!(output.formatted.contains("https://example.com/docs,"));
        assert!(output.formatted.contains("`cargo test`"));
        assert!(output.formatted.contains("<code>search_core</code>"));
        assert!(output.formatted.contains("<b>Re</b>ad"));
        assert!(output.formatted.contains("<b>vis</b>it"));
        assert_eq!(output.stats.skipped_urls, 1);
        assert_eq!(output.stats.skipped_code_segments, 1);
    }

    #[test]
    fn handles_unicode_markdown_and_saccade_stride() {
        let config = BionicConfig {
            fixation_ratio: 0.5,
            saccade_stride: 2,
            min_word_len: 2,
            max_fixation_chars: Some(3),
            format: OutputFormat::Markdown,
            ..Default::default()
        };
        let formatter = BionicFormatter::new(config).expect("valid config");
        let output = formatter
            .format("über café résumé programming")
            .expect("valid unicode");

        // Eligible words:
        // 0: "über" (4 -> 2) formatted
        // 1: "café" skipped by stride
        // 2: "résumé" (6 -> 3) formatted
        // 3: "programming" skipped by stride
        assert_eq!(output.formatted, "**üb**er café **rés**umé programming");
        assert_eq!(output.stats.formatted_words, 2);
        assert_eq!(output.stats.skipped_words, 2);
    }

    #[test]
    fn validates_config_and_input_limits() {
        let bad_ratio = BionicConfig {
            fixation_ratio: 1.5,
            ..Default::default()
        };
        assert_eq!(
            BionicFormatter::new(bad_ratio).unwrap_err(),
            BionicError::InvalidFixationRatio(1.5)
        );

        let bad_stride = BionicConfig {
            saccade_stride: 0,
            ..Default::default()
        };
        assert_eq!(
            BionicFormatter::new(bad_stride).unwrap_err(),
            BionicError::InvalidSaccadeStride(0)
        );

        let bad_custom = BionicConfig {
            format: OutputFormat::Custom {
                prefix: String::new(),
                suffix: String::new(),
            },
            ..Default::default()
        };
        assert_eq!(
            BionicFormatter::new(bad_custom).unwrap_err(),
            BionicError::EmptyCustomMarkers
        );

        let small_limit = BionicConfig {
            max_input_bytes: 5,
            ..Default::default()
        };
        let formatter = BionicFormatter::new(small_limit).expect("valid config");
        assert_eq!(
            formatter.format("hello world").unwrap_err(),
            BionicError::InputTooLarge { len: 11, max: 5 }
        );
    }

    #[test]
    fn round_trips_config_and_output_via_serde() {
        let cfg = BionicConfig {
            format: OutputFormat::Custom {
                prefix: "[".to_string(),
                suffix: "]".to_string(),
            },
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).expect("serialize config");
        let decoded: BionicConfig = serde_json::from_str(&json).expect("deserialize config");
        assert_eq!(cfg, decoded);

        let formatter = BionicFormatter::new(decoded).expect("valid config");
        let out = formatter.format("Hello").expect("format");
        assert_eq!(out.formatted, "[Hel]lo");
    }
}
