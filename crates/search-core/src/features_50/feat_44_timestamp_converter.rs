//! Feature 44: Timestamp Converter
//!
//! Parses and converts Unix timestamps (seconds, milliseconds, microseconds,
//! nanoseconds), ISO-8601 / RFC-3339 strings, calendar dates, and `now`
//! expressions into structured UTC and optional target-offset representations.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset, Weekday};

/// Supported Unix timestamp precisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimestampUnit {
    Seconds,
    Milliseconds,
    Microseconds,
    Nanoseconds,
}

/// Detected input representation for a timestamp query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputFormat {
    UnixSeconds,
    UnixMilliseconds,
    UnixMicroseconds,
    UnixNanoseconds,
    Iso8601,
    DateOnly,
    KeywordNow,
}

/// Errors produced when validating or converting a timestamp.
#[derive(Debug, Error, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimestampError {
    #[error("timestamp input cannot be empty")]
    EmptyInput,
    #[error("unrecognized timestamp format: {0}")]
    UnrecognizedFormat(String),
    #[error("timestamp value out of supported range: {0}")]
    OutOfRange(String),
    #[error("invalid calendar date or time: {0}")]
    InvalidDateTime(String),
    #[error("invalid UTC offset seconds: {0}")]
    InvalidOffset(i32),
}

/// Request parameters for converting a timestamp expression.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimestampConversionRequest {
    pub input: String,
    pub target_offset_seconds: Option<i32>,
    pub reference_now_nanos: Option<i128>,
}

impl TimestampConversionRequest {
    /// Creates a new conversion request using UTC and system clock as reference.
    pub fn new(input: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            target_offset_seconds: None,
            reference_now_nanos: None,
        }
    }

    /// Sets a target UTC offset in whole seconds (e.g. `-18000` for `-05:00`).
    pub fn with_target_offset_seconds(mut self, offset_seconds: i32) -> Self {
        self.target_offset_seconds = Some(offset_seconds);
        self
    }

    /// Sets an explicit reference `now` timestamp in Unix nanoseconds.
    pub fn with_reference_now_nanos(mut self, now_nanos: i128) -> Self {
        self.reference_now_nanos = Some(now_nanos);
        self
    }
}

/// Decomposed calendar and clock fields for a specific UTC offset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimestampBreakdown {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub nanosecond: u32,
    pub weekday: String,
    pub day_of_year: u16,
    pub iso_week: u8,
    pub offset_seconds: i32,
    pub offset_label: String,
}

/// Full conversion output containing Unix epochs, formatted strings, and breakdowns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConvertedTimestamp {
    pub input_normalized: String,
    pub detected_format: InputFormat,
    pub unix_seconds: i64,
    pub unix_milliseconds: i128,
    pub unix_microseconds: i128,
    pub unix_nanoseconds: i128,
    pub iso8601_utc: String,
    pub rfc2822_utc: String,
    pub iso8601_target: Option<String>,
    pub utc_breakdown: TimestampBreakdown,
    pub target_breakdown: Option<TimestampBreakdown>,
    pub relative_to_now: Option<String>,
}

/// Stateless converter for Search Linux timestamp queries.
#[derive(Debug, Default, Clone, Copy)]
pub struct TimestampConverter;

impl TimestampConverter {
    /// Creates a new [`TimestampConverter`].
    pub fn new() -> Self {
        Self
    }

    /// Parses a raw omnibox query (optionally prefixed with `ts `, `timestamp `,
    /// `epoch `, or `time `) and converts it in UTC.
    pub fn parse_query(
        &self,
        raw: &str,
        reference_now_nanos: Option<i128>,
    ) -> Result<ConvertedTimestamp, TimestampError> {
        let trimmed = strip_query_prefix(raw.trim());
        let mut req = TimestampConversionRequest::new(trimmed);
        req.reference_now_nanos = reference_now_nanos;
        self.convert(&req)
    }

    /// Converts a [`TimestampConversionRequest`] into a [`ConvertedTimestamp`].
    pub fn convert(
        &self,
        request: &TimestampConversionRequest,
    ) -> Result<ConvertedTimestamp, TimestampError> {
        let normalized = strip_query_prefix(request.input.trim());
        if normalized.is_empty() {
            return Err(TimestampError::EmptyInput);
        }

        let target_offset = match request.target_offset_seconds {
            Some(secs) => Some(
                UtcOffset::from_whole_seconds(secs)
                    .map_err(|_| TimestampError::InvalidOffset(secs))?,
            ),
            None => None,
        };

        let ref_now_nanos = request.reference_now_nanos.or_else(system_now_nanos);
        let (dt_utc, detected_format) = parse_timestamp(normalized, ref_now_nanos)?;

        let unix_nanoseconds = dt_utc.unix_timestamp_nanos();
        let unix_seconds = dt_utc.unix_timestamp();
        let unix_milliseconds = unix_nanoseconds / 1_000_000;
        let unix_microseconds = unix_nanoseconds / 1_000;

        let utc_breakdown = build_breakdown(dt_utc);
        let iso8601_utc = format_iso8601(dt_utc);
        let rfc2822_utc = format_rfc2822_utc(dt_utc);

        let (iso8601_target, target_breakdown) = match target_offset {
            Some(offset) => {
                let dt_target = dt_utc.to_offset(offset);
                (
                    Some(format_iso8601(dt_target)),
                    Some(build_breakdown(dt_target)),
                )
            }
            None => (None, None),
        };

        let relative_to_now = ref_now_nanos.map(|now_ns| {
            let now_secs = (now_ns / 1_000_000_000) as i64;
            let delta = unix_seconds.saturating_sub(now_secs);
            Self::format_relative(delta)
        });

        Ok(ConvertedTimestamp {
            input_normalized: normalized.to_string(),
            detected_format,
            unix_seconds,
            unix_milliseconds,
            unix_microseconds,
            unix_nanoseconds,
            iso8601_utc,
            rfc2822_utc,
            iso8601_target,
            utc_breakdown,
            target_breakdown,
            relative_to_now,
        })
    }

    /// Formats a signed second delta (`target - now`) into a human-readable phrase.
    pub fn format_relative(delta_seconds: i64) -> String {
        if delta_seconds == 0 {
            return "just now".to_string();
        }
        let future = delta_seconds > 0;
        let abs = delta_seconds.unsigned_abs();
        let (value, unit) = if abs < 60 {
            (abs, if abs == 1 { "second" } else { "seconds" })
        } else if abs < 3_600 {
            let mins = abs / 60;
            (mins, if mins == 1 { "minute" } else { "minutes" })
        } else if abs < 86_400 {
            let hrs = abs / 3_600;
            (hrs, if hrs == 1 { "hour" } else { "hours" })
        } else if abs < 2_592_000 {
            let days = abs / 86_400;
            (days, if days == 1 { "day" } else { "days" })
        } else if abs < 31_536_000 {
            let months = abs / 2_592_000;
            (months, if months == 1 { "month" } else { "months" })
        } else {
            let years = abs / 31_536_000;
            (years, if years == 1 { "year" } else { "years" })
        };

        if future {
            format!("in {value} {unit}")
        } else {
            format!("{value} {unit} ago")
        }
    }
}

fn strip_query_prefix(input: &str) -> &str {
    let lower = input.to_ascii_lowercase();
    for prefix in ["timestamp ", "epoch ", "time ", "ts "] {
        if lower.starts_with(prefix) {
            return input[prefix.len()..].trim();
        }
    }
    input
}

fn system_now_nanos() -> Option<i128> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i128::try_from(d.as_nanos()).ok())
}

fn parse_timestamp(
    input: &str,
    ref_now_nanos: Option<i128>,
) -> Result<(OffsetDateTime, InputFormat), TimestampError> {
    let lower = input.to_ascii_lowercase();
    if lower == "now" || lower == "now utc" {
        let now_ns = ref_now_nanos.ok_or_else(|| {
            TimestampError::OutOfRange("system clock unavailable for 'now'".to_string())
        })?;
        let dt = OffsetDateTime::from_unix_timestamp_nanos(now_ns)
            .map_err(|e| TimestampError::OutOfRange(e.to_string()))?;
        return Ok((dt, InputFormat::KeywordNow));
    }

    if let Some((unit, digits_str)) = parse_suffixed_unix(input) {
        return parse_unix_with_unit(digits_str, unit);
    }

    if is_signed_integer(input) {
        let unit = infer_unix_unit(input);
        return parse_unix_with_unit(input, unit);
    }

    if let Some(dt) = parse_iso8601(input)? {
        let format = if input.len() == 10 && !input.contains('T') && !input.contains(' ') {
            InputFormat::DateOnly
        } else {
            InputFormat::Iso8601
        };
        return Ok((dt, format));
    }

    Err(TimestampError::UnrecognizedFormat(input.to_string()))
}

fn parse_suffixed_unix(input: &str) -> Option<(TimestampUnit, &str)> {
    let lower = input.to_ascii_lowercase();
    let suffixes = [
        ("ns", TimestampUnit::Nanoseconds),
        ("us", TimestampUnit::Microseconds),
        ("ms", TimestampUnit::Milliseconds),
        ("s", TimestampUnit::Seconds),
    ];
    for (suffix, unit) in suffixes {
        if lower.ends_with(suffix) {
            let head = input[..input.len() - suffix.len()].trim();
            if is_signed_integer(head) {
                return Some((unit, head));
            }
        }
    }
    None
}

fn is_signed_integer(s: &str) -> bool {
    let body = s
        .strip_prefix('-')
        .or_else(|| s.strip_prefix('+'))
        .unwrap_or(s);
    !body.is_empty() && body.bytes().all(|b| b.is_ascii_digit())
}

fn infer_unix_unit(s: &str) -> TimestampUnit {
    let digits = s
        .strip_prefix('-')
        .or_else(|| s.strip_prefix('+'))
        .unwrap_or(s);
    match digits.len() {
        0..=11 => TimestampUnit::Seconds,
        12..=14 => TimestampUnit::Milliseconds,
        15..=17 => TimestampUnit::Microseconds,
        _ => TimestampUnit::Nanoseconds,
    }
}

fn parse_unix_with_unit(
    digits_str: &str,
    unit: TimestampUnit,
) -> Result<(OffsetDateTime, InputFormat), TimestampError> {
    let raw: i128 = digits_str
        .parse()
        .map_err(|_| TimestampError::OutOfRange(digits_str.to_string()))?;

    let (nanos, format) = match unit {
        TimestampUnit::Seconds => (
            raw.checked_mul(1_000_000_000)
                .ok_or_else(|| TimestampError::OutOfRange(digits_str.to_string()))?,
            InputFormat::UnixSeconds,
        ),
        TimestampUnit::Milliseconds => (
            raw.checked_mul(1_000_000)
                .ok_or_else(|| TimestampError::OutOfRange(digits_str.to_string()))?,
            InputFormat::UnixMilliseconds,
        ),
        TimestampUnit::Microseconds => (
            raw.checked_mul(1_000)
                .ok_or_else(|| TimestampError::OutOfRange(digits_str.to_string()))?,
            InputFormat::UnixMicroseconds,
        ),
        TimestampUnit::Nanoseconds => (raw, InputFormat::UnixNanoseconds),
    };

    let dt = OffsetDateTime::from_unix_timestamp_nanos(nanos)
        .map_err(|e| TimestampError::OutOfRange(e.to_string()))?;
    Ok((dt, format))
}

fn parse_iso8601(input: &str) -> Result<Option<OffsetDateTime>, TimestampError> {
    // Support:
    // - YYYY-MM-DD
    // - YYYY-MM-DD[T|t| ]HH:MM:SS[.fffffffff][Z|z|+HH:MM|-HH:MM|+HHMM|-HHMM]
    if input.len() < 10 {
        return Ok(None);
    }

    let bytes = input.as_bytes();
    if bytes.get(4) != Some(&b'-') || bytes.get(7) != Some(&b'-') {
        return Ok(None);
    }

    let year: i32 = input[0..4]
        .parse()
        .map_err(|_| TimestampError::InvalidDateTime(input.to_string()))?;
    let month_num: u8 = input[5..7]
        .parse()
        .map_err(|_| TimestampError::InvalidDateTime(input.to_string()))?;
    let day: u8 = input[8..10]
        .parse()
        .map_err(|_| TimestampError::InvalidDateTime(input.to_string()))?;

    let month = Month::try_from(month_num)
        .map_err(|_| TimestampError::InvalidDateTime(input.to_string()))?;
    let date = Date::from_calendar_date(year, month, day)
        .map_err(|e| TimestampError::InvalidDateTime(e.to_string()))?;

    if input.len() == 10 {
        let dt = PrimitiveDateTime::new(date, Time::MIDNIGHT).assume_utc();
        return Ok(Some(dt));
    }

    let sep = bytes[10];
    if sep != b'T' && sep != b't' && sep != b' ' {
        return Err(TimestampError::InvalidDateTime(input.to_string()));
    }

    let time_and_tz = &input[11..];
    let (time_part, tz_part) = split_time_and_tz(time_and_tz);
    let time = parse_time_part(time_part)?;
    let offset = parse_tz_part(tz_part)?;

    let dt = PrimitiveDateTime::new(date, time)
        .assume_offset(offset)
        .to_offset(UtcOffset::UTC);
    Ok(Some(dt))
}

fn split_time_and_tz(s: &str) -> (&str, &str) {
    if let Some(stripped) = s.strip_suffix('Z').or_else(|| s.strip_suffix('z')) {
        return (stripped, "Z");
    }
    if let Some(idx) = s.rfind(['+', '-']) {
        return (&s[..idx], &s[idx..]);
    }
    (s, "")
}

fn parse_time_part(s: &str) -> Result<Time, TimestampError> {
    let (hms, frac_str) = match s.split_once('.') {
        Some((h, f)) => (h, Some(f)),
        None => (s, None),
    };

    let mut parts = hms.split(':');
    let hour: u8 = parts
        .next()
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| TimestampError::InvalidDateTime(s.to_string()))?;
    let minute: u8 = parts
        .next()
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| TimestampError::InvalidDateTime(s.to_string()))?;
    let second: u8 = parts
        .next()
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| TimestampError::InvalidDateTime(s.to_string()))?;
    if parts.next().is_some() {
        return Err(TimestampError::InvalidDateTime(s.to_string()));
    }

    let nanos = match frac_str {
        Some(f) => parse_nanos_fraction(f)?,
        None => 0,
    };

    Time::from_hms_nano(hour, minute, second, nanos)
        .map_err(|e| TimestampError::InvalidDateTime(e.to_string()))
}

fn parse_nanos_fraction(frac: &str) -> Result<u32, TimestampError> {
    if frac.is_empty() || frac.len() > 9 || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return Err(TimestampError::InvalidDateTime(format!(
            "invalid fractional seconds: .{frac}"
        )));
    }
    let mut padded = [b'0'; 9];
    for (i, b) in frac.bytes().enumerate() {
        padded[i] = b;
    }
    let s = std::str::from_utf8(&padded)
        .map_err(|_| TimestampError::InvalidDateTime(frac.to_string()))?;
    s.parse::<u32>()
        .map_err(|_| TimestampError::InvalidDateTime(frac.to_string()))
}

fn parse_tz_part(tz: &str) -> Result<UtcOffset, TimestampError> {
    if tz.is_empty() || tz.eq_ignore_ascii_case("z") {
        return Ok(UtcOffset::UTC);
    }

    let sign: i32 = if tz.starts_with('+') {
        1
    } else if tz.starts_with('-') {
        -1
    } else {
        return Err(TimestampError::InvalidDateTime(tz.to_string()));
    };

    let rest = &tz[1..];
    let (hh_str, mm_str) = if let Some((h, m)) = rest.split_once(':') {
        (h, m)
    } else if rest.len() == 4 {
        (&rest[0..2], &rest[2..4])
    } else {
        return Err(TimestampError::InvalidDateTime(tz.to_string()));
    };

    let hours: i32 = hh_str
        .parse()
        .map_err(|_| TimestampError::InvalidDateTime(tz.to_string()))?;
    let minutes: i32 = mm_str
        .parse()
        .map_err(|_| TimestampError::InvalidDateTime(tz.to_string()))?;

    if !(0..=23).contains(&hours) || !(0..=59).contains(&minutes) {
        return Err(TimestampError::InvalidDateTime(tz.to_string()));
    }

    let total_seconds = sign * (hours * 3_600 + minutes * 60);
    UtcOffset::from_whole_seconds(total_seconds)
        .map_err(|_| TimestampError::InvalidOffset(total_seconds))
}

fn build_breakdown(dt: OffsetDateTime) -> TimestampBreakdown {
    let offset_seconds = dt.offset().whole_seconds();
    TimestampBreakdown {
        year: dt.year(),
        month: dt.month() as u8,
        day: dt.day(),
        hour: dt.hour(),
        minute: dt.minute(),
        second: dt.second(),
        nanosecond: dt.nanosecond(),
        weekday: weekday_name(dt.weekday()).to_string(),
        day_of_year: dt.ordinal(),
        iso_week: dt.iso_week(),
        offset_seconds,
        offset_label: format_offset_label(offset_seconds),
    }
}

fn format_offset_label(offset_seconds: i32) -> String {
    if offset_seconds == 0 {
        return "Z".to_string();
    }
    let sign = if offset_seconds >= 0 { '+' } else { '-' };
    let abs = offset_seconds.abs();
    let hours = abs / 3_600;
    let minutes = (abs % 3_600) / 60;
    format!("{sign}{hours:02}:{minutes:02}")
}

fn format_iso8601(dt: OffsetDateTime) -> String {
    let offset_label = format_offset_label(dt.offset().whole_seconds());
    let base = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        dt.year(),
        dt.month() as u8,
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second()
    );
    let nanos = dt.nanosecond();
    if nanos == 0 {
        format!("{base}{offset_label}")
    } else {
        let frac = format!("{nanos:09}");
        let trimmed = frac.trim_end_matches('0');
        format!("{base}.{trimmed}{offset_label}")
    }
}

fn format_rfc2822_utc(dt: OffsetDateTime) -> String {
    let utc = dt.to_offset(UtcOffset::UTC);
    format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} +0000",
        weekday_short(utc.weekday()),
        utc.day(),
        month_short(utc.month()),
        utc.year(),
        utc.hour(),
        utc.minute(),
        utc.second()
    )
}

fn weekday_name(w: Weekday) -> &'static str {
    match w {
        Weekday::Monday => "Monday",
        Weekday::Tuesday => "Tuesday",
        Weekday::Wednesday => "Wednesday",
        Weekday::Thursday => "Thursday",
        Weekday::Friday => "Friday",
        Weekday::Saturday => "Saturday",
        Weekday::Sunday => "Sunday",
    }
}

fn weekday_short(w: Weekday) -> &'static str {
    match w {
        Weekday::Monday => "Mon",
        Weekday::Tuesday => "Tue",
        Weekday::Wednesday => "Wed",
        Weekday::Thursday => "Thu",
        Weekday::Friday => "Fri",
        Weekday::Saturday => "Sat",
        Weekday::Sunday => "Sun",
    }
}

fn month_short(m: Month) -> &'static str {
    match m {
        Month::January => "Jan",
        Month::February => "Feb",
        Month::March => "Mar",
        Month::April => "Apr",
        Month::May => "May",
        Month::June => "Jun",
        Month::July => "Jul",
        Month::August => "Aug",
        Month::September => "Sep",
        Month::October => "Oct",
        Month::November => "Nov",
        Month::December => "Dec",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_unix_seconds_and_target_offset() {
        let converter = TimestampConverter::new();
        let req = TimestampConversionRequest::new("1700000000")
            .with_target_offset_seconds(-5 * 3_600)
            .with_reference_now_nanos(1_700_003_600_i128 * 1_000_000_000);

        let result = converter.convert(&req).expect("valid unix seconds");
        assert_eq!(result.detected_format, InputFormat::UnixSeconds);
        assert_eq!(result.unix_seconds, 1_700_000_000);
        assert_eq!(result.unix_milliseconds, 1_700_000_000_000);
        assert_eq!(result.iso8601_utc, "2023-11-14T22:13:20Z");
        assert_eq!(result.rfc2822_utc, "Tue, 14 Nov 2023 22:13:20 +0000");
        assert_eq!(
            result.iso8601_target.as_deref(),
            Some("2023-11-14T17:13:20-05:00")
        );
        assert_eq!(result.relative_to_now.as_deref(), Some("1 hour ago"));
        assert_eq!(result.utc_breakdown.weekday, "Tuesday");
    }

    #[test]
    fn converts_iso8601_with_offset_and_fractional_seconds() {
        let converter = TimestampConverter::new();
        let result = converter
            .parse_query(
                "ts 2023-11-14T17:13:20.25-05:00",
                Some(1_700_000_000_000_000_000),
            )
            .expect("valid iso8601");

        assert_eq!(result.detected_format, InputFormat::Iso8601);
        assert_eq!(result.unix_seconds, 1_700_000_000);
        assert_eq!(result.unix_milliseconds, 1_700_000_000_250);
        assert_eq!(result.iso8601_utc, "2023-11-14T22:13:20.25Z");
        assert_eq!(result.utc_breakdown.nanosecond, 250_000_000);
        assert_eq!(result.relative_to_now.as_deref(), Some("just now"));
    }

    #[test]
    fn converts_milliseconds_nanoseconds_date_only_and_now() {
        let converter = TimestampConverter::new();

        let ms = converter
            .parse_query("1700000000123", None)
            .expect("valid unix millis");
        assert_eq!(ms.detected_format, InputFormat::UnixMilliseconds);
        assert_eq!(ms.unix_seconds, 1_700_000_000);
        assert_eq!(ms.iso8601_utc, "2023-11-14T22:13:20.123Z");

        let ns = converter
            .parse_query("1700000000s", None)
            .expect("valid suffixed seconds");
        assert_eq!(ns.detected_format, InputFormat::UnixSeconds);
        assert_eq!(ns.unix_seconds, 1_700_000_000);

        let date_only = converter
            .parse_query("2024-02-29", None)
            .expect("valid leap day");
        assert_eq!(date_only.detected_format, InputFormat::DateOnly);
        assert_eq!(date_only.iso8601_utc, "2024-02-29T00:00:00Z");

        let now = converter
            .parse_query("now", Some(1_700_000_000_i128 * 1_000_000_000))
            .expect("valid now");
        assert_eq!(now.detected_format, InputFormat::KeywordNow);
        assert_eq!(now.unix_seconds, 1_700_000_000);
    }

    #[test]
    fn rejects_invalid_inputs_and_serializes_roundtrip() {
        let converter = TimestampConverter::new();

        assert_eq!(
            converter.parse_query("   ", None),
            Err(TimestampError::EmptyInput)
        );
        assert!(matches!(
            converter.parse_query("not-a-timestamp", None),
            Err(TimestampError::UnrecognizedFormat(_))
        ));
        assert!(matches!(
            converter.parse_query("2023-02-29T00:00:00Z", None),
            Err(TimestampError::InvalidDateTime(_))
        ));

        let bad_offset = TimestampConversionRequest::new("0").with_target_offset_seconds(100_000);
        assert_eq!(
            converter.convert(&bad_offset),
            Err(TimestampError::InvalidOffset(100_000))
        );

        let ok = converter
            .parse_query("1700000000", Some(1_699_996_400_i128 * 1_000_000_000))
            .expect("valid conversion");
        let json = serde_json::to_string(&ok).expect("serialize");
        let decoded: ConvertedTimestamp = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, ok);
        assert_eq!(decoded.relative_to_now.as_deref(), Some("in 1 hour"));
    }
}
