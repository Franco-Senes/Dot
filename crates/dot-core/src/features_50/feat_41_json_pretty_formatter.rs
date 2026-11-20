//! JSON Pretty Formatter, Minifier, and Validator for Search Linux.
//!
//! Provides configurable indentation, key sorting, Unicode escaping,
//! structural statistics, depth/size guardrails, and detailed syntax error
//! reporting with line and column positions.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Default maximum input payload size (16 MiB).
pub const DEFAULT_MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;

/// Default maximum nesting depth allowed during formatting and analysis.
pub const DEFAULT_MAX_DEPTH: usize = 128;

/// Maximum supported spaces per indentation level.
pub const MAX_INDENT_SPACES: u8 = 16;

/// Maximum configurable nesting depth.
pub const MAX_ALLOWED_DEPTH: usize = 2048;

/// Indentation style used when pretty-printing JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndentStyle {
    /// Indent using a fixed number of ASCII space characters (`1..=16`).
    Spaces(u8),
    /// Indent using a single ASCII tab character (`\t`) per level.
    Tabs,
}

impl Default for IndentStyle {
    fn default() -> Self {
        Self::Spaces(2)
    }
}

/// Ordering strategy applied to JSON object keys during formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum KeySortOrder {
    /// Preserve key order as parsed.
    #[default]
    Preserve,
    /// Sort object keys lexicographically in ascending order.
    AlphabeticalAsc,
    /// Sort object keys lexicographically in descending order.
    AlphabeticalDesc,
}

/// Configuration options for [`JsonPrettyFormatter`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormatConfig {
    /// Indentation style per nesting level.
    pub indent: IndentStyle,
    /// Strategy for ordering object keys.
    pub sort_keys: KeySortOrder,
    /// Append a trailing `\n` to the pretty-printed output.
    pub trailing_newline: bool,
    /// Render empty objects `{}` and empty arrays `[]` on a single line.
    pub compact_empty_containers: bool,
    /// Maximum accepted input size in bytes.
    pub max_input_bytes: usize,
    /// Maximum nesting depth permitted before aborting.
    pub max_depth: usize,
    /// Escape all non-ASCII characters as `\uXXXX` sequences.
    pub escape_unicode: bool,
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            indent: IndentStyle::default(),
            sort_keys: KeySortOrder::default(),
            trailing_newline: false,
            compact_empty_containers: true,
            max_input_bytes: DEFAULT_MAX_INPUT_BYTES,
            max_depth: DEFAULT_MAX_DEPTH,
            escape_unicode: false,
        }
    }
}

impl FormatConfig {
    /// Validate configuration bounds and invariants.
    pub fn validate(&self) -> Result<(), JsonFormatterError> {
        if let IndentStyle::Spaces(n) = self.indent {
            if n == 0 || n > MAX_INDENT_SPACES {
                return Err(JsonFormatterError::InvalidConfig(format!(
                    "space indentation must be between 1 and {MAX_INDENT_SPACES}, got {n}"
                )));
            }
        }
        if self.max_input_bytes == 0 {
            return Err(JsonFormatterError::InvalidConfig(
                "max_input_bytes must be greater than 0".to_string(),
            ));
        }
        if self.max_depth == 0 || self.max_depth > MAX_ALLOWED_DEPTH {
            return Err(JsonFormatterError::InvalidConfig(format!(
                "max_depth must be between 1 and {MAX_ALLOWED_DEPTH}, got {}",
                self.max_depth
            )));
        }
        Ok(())
    }

    fn indent_unit(&self) -> String {
        match self.indent {
            IndentStyle::Spaces(n) => " ".repeat(usize::from(n)),
            IndentStyle::Tabs => "\t".to_string(),
        }
    }
}

/// Structural statistics collected while formatting or validating JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct JsonStats {
    /// Byte length of the raw input string.
    pub input_bytes: usize,
    /// Byte length of the formatted output string.
    pub output_bytes: usize,
    /// Total number of lines in the formatted output.
    pub line_count: usize,
    /// Maximum nesting depth observed (0 for scalar roots).
    pub max_depth_seen: usize,
    /// Total number of JSON objects.
    pub object_count: usize,
    /// Total number of JSON arrays.
    pub array_count: usize,
    /// Total number of object keys across all objects.
    pub key_count: usize,
    /// Total number of string values (excluding object keys).
    pub string_count: usize,
    /// Total number of numeric values.
    pub number_count: usize,
    /// Total number of boolean values.
    pub bool_count: usize,
    /// Total number of `null` values.
    pub null_count: usize,
}

/// Complete output from formatting a JSON document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormatResult {
    /// Pretty-printed JSON representation.
    pub formatted: String,
    /// Compact single-line JSON representation.
    pub minified: String,
    /// Structural metrics for the document.
    pub stats: JsonStats,
    /// Always `true` for a successful [`FormatResult`].
    pub is_valid: bool,
}

/// Diagnostic result produced by [`JsonPrettyFormatter::validate`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationReport {
    /// Whether the input is valid JSON within configured limits.
    pub is_valid: bool,
    /// Human-readable error message when `is_valid` is `false`.
    pub error_message: Option<String>,
    /// 1-based line number of a syntax error, if applicable.
    pub error_line: Option<usize>,
    /// 1-based column number of a syntax error, if applicable.
    pub error_column: Option<usize>,
    /// Structural statistics when validation succeeds.
    pub stats: Option<JsonStats>,
}

/// Errors returned by [`JsonPrettyFormatter`].
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum JsonFormatterError {
    /// Input was empty or contained only whitespace.
    #[error("input JSON is empty")]
    EmptyInput,
    /// Input exceeded the configured `max_input_bytes` limit.
    #[error("input size ({size} bytes) exceeds maximum allowed ({max} bytes)")]
    InputTooLarge {
        /// Actual input byte count.
        size: usize,
        /// Configured maximum byte count.
        max: usize,
    },
    /// Configuration failed validation.
    #[error("invalid formatter configuration: {0}")]
    InvalidConfig(String),
    /// Nesting depth exceeded the configured `max_depth` limit.
    #[error("JSON nesting depth ({depth}) exceeds maximum allowed ({max})")]
    DepthLimitExceeded {
        /// Observed depth.
        depth: usize,
        /// Configured maximum depth.
        max: usize,
    },
    /// Input could not be parsed as valid JSON.
    #[error("JSON parse error at line {line}, column {column}: {message}")]
    ParseError {
        /// Parser diagnostic message.
        message: String,
        /// 1-based line number.
        line: usize,
        /// 1-based column number.
        column: usize,
    },
}

/// Configurable JSON pretty-printer, minifier, and validator.
#[derive(Debug, Clone, Default)]
pub struct JsonPrettyFormatter {
    config: FormatConfig,
}

impl JsonPrettyFormatter {
    /// Construct a new formatter after validating `config`.
    pub fn new(config: FormatConfig) -> Result<Self, JsonFormatterError> {
        config.validate()?;
        Ok(Self { config })
    }

    /// Return a reference to the active [`FormatConfig`].
    pub fn config(&self) -> &FormatConfig {
        &self.config
    }

    /// Parse, validate, pretty-print, and minify a JSON string.
    pub fn format(&self, input: &str) -> Result<FormatResult, JsonFormatterError> {
        let value = self.parse_input(input)?;
        self.format_value_with_input_len(&value, input.len())
    }

    /// Parse and return only the pretty-printed JSON string.
    pub fn format_pretty(&self, input: &str) -> Result<String, JsonFormatterError> {
        Ok(self.format(input)?.formatted)
    }

    /// Parse and return a compact single-line JSON string.
    pub fn minify(&self, input: &str) -> Result<String, JsonFormatterError> {
        Ok(self.format(input)?.minified)
    }

    /// Format an already-parsed [`serde_json::Value`].
    pub fn format_value(&self, value: &Value) -> Result<FormatResult, JsonFormatterError> {
        self.format_value_with_input_len(value, 0)
    }

    /// Validate a JSON string without throwing on syntax or limit errors.
    pub fn validate(&self, input: &str) -> ValidationReport {
        match self.format(input) {
            Ok(res) => ValidationReport {
                is_valid: true,
                error_message: None,
                error_line: None,
                error_column: None,
                stats: Some(res.stats),
            },
            Err(JsonFormatterError::ParseError {
                message,
                line,
                column,
            }) => ValidationReport {
                is_valid: false,
                error_message: Some(message),
                error_line: Some(line),
                error_column: Some(column),
                stats: None,
            },
            Err(err) => ValidationReport {
                is_valid: false,
                error_message: Some(err.to_string()),
                error_line: None,
                error_column: None,
                stats: None,
            },
        }
    }

    fn parse_input(&self, input: &str) -> Result<Value, JsonFormatterError> {
        if input.len() > self.config.max_input_bytes {
            return Err(JsonFormatterError::InputTooLarge {
                size: input.len(),
                max: self.config.max_input_bytes,
            });
        }
        if input.trim().is_empty() {
            return Err(JsonFormatterError::EmptyInput);
        }

        serde_json::from_str::<Value>(input).map_err(|err| JsonFormatterError::ParseError {
            message: err.to_string(),
            line: err.line(),
            column: err.column(),
        })
    }

    fn format_value_with_input_len(
        &self,
        value: &Value,
        input_bytes: usize,
    ) -> Result<FormatResult, JsonFormatterError> {
        let indent_unit = self.config.indent_unit();
        let mut formatted = String::with_capacity(input_bytes.max(64));
        let mut minified = String::with_capacity(input_bytes.max(64));
        let mut stats = JsonStats {
            input_bytes,
            ..JsonStats::default()
        };

        self.write_value(value, 0, &indent_unit, true, &mut formatted, &mut stats)?;
        self.write_value(
            value,
            0,
            &indent_unit,
            false,
            &mut minified,
            &mut JsonStats::default(),
        )?;

        if self.config.trailing_newline && !formatted.ends_with('\n') {
            formatted.push('\n');
        }

        stats.output_bytes = formatted.len();
        stats.line_count = if formatted.is_empty() {
            0
        } else {
            formatted.lines().count() + usize::from(formatted.ends_with('\n'))
        };

        Ok(FormatResult {
            formatted,
            minified,
            stats,
            is_valid: true,
        })
    }

    fn write_value(
        &self,
        value: &Value,
        depth: usize,
        indent_unit: &str,
        pretty: bool,
        out: &mut String,
        stats: &mut JsonStats,
    ) -> Result<(), JsonFormatterError> {
        if depth > self.config.max_depth {
            return Err(JsonFormatterError::DepthLimitExceeded {
                depth,
                max: self.config.max_depth,
            });
        }
        if depth > stats.max_depth_seen {
            stats.max_depth_seen = depth;
        }

        match value {
            Value::Null => {
                stats.null_count += 1;
                out.push_str("null");
            }
            Value::Bool(b) => {
                stats.bool_count += 1;
                out.push_str(if *b { "true" } else { "false" });
            }
            Value::Number(n) => {
                stats.number_count += 1;
                out.push_str(&n.to_string());
            }
            Value::String(s) => {
                stats.string_count += 1;
                write_json_string(out, s, self.config.escape_unicode);
            }
            Value::Array(items) => {
                stats.array_count += 1;
                self.write_array(items, depth, indent_unit, pretty, out, stats)?;
            }
            Value::Object(map) => {
                stats.object_count += 1;
                self.write_object(map, depth, indent_unit, pretty, out, stats)?;
            }
        }
        Ok(())
    }

    fn write_array(
        &self,
        items: &[Value],
        depth: usize,
        indent_unit: &str,
        pretty: bool,
        out: &mut String,
        stats: &mut JsonStats,
    ) -> Result<(), JsonFormatterError> {
        if items.is_empty() {
            if pretty && !self.config.compact_empty_containers {
                out.push_str("[\n");
                push_indent(out, indent_unit, depth);
                out.push(']');
            } else {
                out.push_str("[]");
            }
            return Ok(());
        }

        out.push('[');
        for (idx, item) in items.iter().enumerate() {
            if idx > 0 {
                out.push(',');
            }
            if pretty {
                out.push('\n');
                push_indent(out, indent_unit, depth + 1);
            }
            self.write_value(item, depth + 1, indent_unit, pretty, out, stats)?;
        }
        if pretty {
            out.push('\n');
            push_indent(out, indent_unit, depth);
        }
        out.push(']');
        Ok(())
    }

    fn write_object(
        &self,
        map: &serde_json::Map<String, Value>,
        depth: usize,
        indent_unit: &str,
        pretty: bool,
        out: &mut String,
        stats: &mut JsonStats,
    ) -> Result<(), JsonFormatterError> {
        if map.is_empty() {
            if pretty && !self.config.compact_empty_containers {
                out.push_str("{\n");
                push_indent(out, indent_unit, depth);
                out.push('}');
            } else {
                out.push_str("{}");
            }
            return Ok(());
        }

        let mut entries: Vec<(&String, &Value)> = map.iter().collect();
        match self.config.sort_keys {
            KeySortOrder::Preserve => {}
            KeySortOrder::AlphabeticalAsc => entries.sort_by(|a, b| a.0.cmp(b.0)),
            KeySortOrder::AlphabeticalDesc => entries.sort_by(|a, b| b.0.cmp(a.0)),
        }

        out.push('{');
        for (idx, (key, val)) in entries.iter().enumerate() {
            stats.key_count += 1;
            if idx > 0 {
                out.push(',');
            }
            if pretty {
                out.push('\n');
                push_indent(out, indent_unit, depth + 1);
            }
            write_json_string(out, key, self.config.escape_unicode);
            if pretty {
                out.push_str(": ");
            } else {
                out.push(':');
            }
            self.write_value(val, depth + 1, indent_unit, pretty, out, stats)?;
        }
        if pretty {
            out.push('\n');
            push_indent(out, indent_unit, depth);
        }
        out.push('}');
        Ok(())
    }
}

fn push_indent(out: &mut String, indent_unit: &str, depth: usize) {
    for _ in 0..depth {
        out.push_str(indent_unit);
    }
}

fn write_json_string(out: &mut String, s: &str, escape_unicode: bool) {
    use std::fmt::Write as _;

    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c if escape_unicode && !c.is_ascii() => {
                let code = c as u32;
                if code <= 0xFFFF {
                    let _ = write!(out, "\\u{code:04x}");
                } else {
                    let mut buf = [0u16; 2];
                    for unit in c.encode_utf16(&mut buf) {
                        let _ = write!(out, "\\u{unit:04x}");
                    }
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_pretty_with_sorted_keys_and_stats() {
        let config = FormatConfig {
            indent: IndentStyle::Spaces(4),
            sort_keys: KeySortOrder::AlphabeticalAsc,
            trailing_newline: true,
            ..FormatConfig::default()
        };
        let formatter = JsonPrettyFormatter::new(config).expect("valid config");
        let input = r#"{"z":true,"a":[1,"hi",null,{}],"m":{"b":2,"a":1}}"#;

        let res = formatter.format(input).expect("format should succeed");
        let expected = concat!(
            "{\n",
            "    \"a\": [\n",
            "        1,\n",
            "        \"hi\",\n",
            "        null,\n",
            "        {}\n",
            "    ],\n",
            "    \"m\": {\n",
            "        \"a\": 1,\n",
            "        \"b\": 2\n",
            "    },\n",
            "    \"z\": true\n",
            "}\n"
        );

        assert_eq!(res.formatted, expected);
        assert_eq!(
            res.minified,
            r#"{"a":[1,"hi",null,{}],"m":{"a":1,"b":2},"z":true}"#
        );
        assert!(res.is_valid);
        assert_eq!(res.stats.object_count, 3);
        assert_eq!(res.stats.array_count, 1);
        assert_eq!(res.stats.key_count, 5);
        assert_eq!(res.stats.string_count, 1);
        assert_eq!(res.stats.number_count, 3);
        assert_eq!(res.stats.bool_count, 1);
        assert_eq!(res.stats.null_count, 1);
        assert_eq!(res.stats.max_depth_seen, 2);
    }

    #[test]
    fn validates_errors_and_reports_line_and_column() {
        let formatter = JsonPrettyFormatter::default();

        let empty_report = formatter.validate("   \n\t ");
        assert!(!empty_report.is_valid);
        assert!(empty_report.error_message.is_some());
        assert_eq!(empty_report.error_line, None);

        let bad_json = "{\n  \"ok\": 1,\n  \"broken\": [1, 2,\n}";
        let syntax_report = formatter.validate(bad_json);
        assert!(!syntax_report.is_valid);
        assert_eq!(syntax_report.error_line, Some(4));
        assert_eq!(syntax_report.error_column, Some(1));
        assert!(syntax_report.stats.is_none());
    }

    #[test]
    fn enforces_config_and_runtime_limits() {
        let bad_spaces = FormatConfig {
            indent: IndentStyle::Spaces(0),
            ..FormatConfig::default()
        };
        assert!(matches!(
            JsonPrettyFormatter::new(bad_spaces),
            Err(JsonFormatterError::InvalidConfig(_))
        ));

        let tight_limits = FormatConfig {
            max_input_bytes: 32,
            max_depth: 2,
            ..FormatConfig::default()
        };
        let formatter = JsonPrettyFormatter::new(tight_limits).expect("valid config");

        let too_big = r#"{"long":"012345678901234567890123456789"}"#;
        assert!(matches!(
            formatter.format(too_big),
            Err(JsonFormatterError::InputTooLarge { .. })
        ));

        let too_deep = "[[[1]]]";
        assert!(matches!(
            formatter.format(too_deep),
            Err(JsonFormatterError::DepthLimitExceeded { depth: 3, max: 2 })
        ));
    }

    #[test]
    fn escapes_unicode_and_handles_tabs_and_empty_containers() {
        let config = FormatConfig {
            indent: IndentStyle::Tabs,
            sort_keys: KeySortOrder::AlphabeticalDesc,
            compact_empty_containers: false,
            escape_unicode: true,
            ..FormatConfig::default()
        };
        let formatter = JsonPrettyFormatter::new(config).expect("valid config");
        let input = "{\"a\":\"caf\\u00e9\",\"b\":[]}";

        let formatted = formatter
            .format_pretty(input)
            .expect("format should succeed");
        let expected = "{\n\t\"b\": [\n\t],\n\t\"a\": \"caf\\u00e9\"\n}";
        assert_eq!(formatted, expected);
    }
}
