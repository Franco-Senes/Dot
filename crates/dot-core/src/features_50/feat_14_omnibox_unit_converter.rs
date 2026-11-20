//! Omnibox unit converter for Search Linux.
//!
//! Parses natural unit conversion queries typed into the browser omnibox
//! (e.g. `"10 km to mi"`, `"-40C in F"`, `"5.5 GiB -> MB"`) and computes
//! validated, formatted conversion results across physical and digital units.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Supported measurement categories for omnibox unit conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitCategory {
    Length,
    Mass,
    Temperature,
    Volume,
    Area,
    Speed,
    Time,
    DigitalStorage,
    DataRate,
    Energy,
    Pressure,
    Angle,
}

impl UnitCategory {
    /// Human-readable label for the category.
    pub fn label(self) -> &'static str {
        match self {
            Self::Length => "Length",
            Self::Mass => "Mass",
            Self::Temperature => "Temperature",
            Self::Volume => "Volume",
            Self::Area => "Area",
            Self::Speed => "Speed",
            Self::Time => "Time",
            Self::DigitalStorage => "Digital Storage",
            Self::DataRate => "Data Rate",
            Self::Energy => "Energy",
            Self::Pressure => "Pressure",
            Self::Angle => "Angle",
        }
    }
}

/// Public metadata describing a unit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitInfo {
    pub symbol: String,
    pub name: String,
    pub category: UnitCategory,
}

/// Internal unit specification using an affine transform to the category base unit:
/// `base_value = value * scale + offset`.
#[derive(Debug, Clone, Copy)]
struct UnitSpec {
    symbol: &'static str,
    name: &'static str,
    category: UnitCategory,
    scale: f64,
    offset: f64,
    aliases: &'static [&'static str],
}

impl UnitSpec {
    const fn linear(
        symbol: &'static str,
        name: &'static str,
        category: UnitCategory,
        scale: f64,
        aliases: &'static [&'static str],
    ) -> Self {
        Self {
            symbol,
            name,
            category,
            scale,
            offset: 0.0,
            aliases,
        }
    }

    const fn affine(
        symbol: &'static str,
        name: &'static str,
        category: UnitCategory,
        scale: f64,
        offset: f64,
        aliases: &'static [&'static str],
    ) -> Self {
        Self {
            symbol,
            name,
            category,
            scale,
            offset,
            aliases,
        }
    }

    fn to_info(self) -> UnitInfo {
        UnitInfo {
            symbol: self.symbol.to_string(),
            name: self.name.to_string(),
            category: self.category,
        }
    }
}

/// Errors returned by omnibox unit parsing and conversion.
#[derive(Debug, Clone, PartialEq, Error, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum UnitConverterError {
    #[error("conversion query is empty")]
    EmptyQuery,
    #[error("query does not match a supported unit conversion pattern")]
    InvalidSyntax,
    #[error("invalid numeric value: {0}")]
    InvalidNumber(String),
    #[error("input value must be finite")]
    NonFiniteInput,
    #[error("unknown unit: {0}")]
    UnknownUnit(String),
    #[error("cannot convert between incompatible categories: {from:?} ({from_unit}) and {to:?} ({to_unit})")]
    IncompatibleCategories {
        from: UnitCategory,
        to: UnitCategory,
        from_unit: String,
        to_unit: String,
    },
    #[error("temperature {value} {unit} is below absolute zero")]
    BelowAbsoluteZero { value: f64, unit: String },
    #[error("conversion result overflowed or is not finite")]
    NonFiniteResult,
}

/// Configuration for [`OmniboxUnitConverter`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversionConfig {
    pub max_decimals: usize,
    pub trim_trailing_zeros: bool,
    pub reject_below_absolute_zero: bool,
    pub allow_case_insensitive_fallback: bool,
}

impl Default for ConversionConfig {
    fn default() -> Self {
        Self {
            max_decimals: 6,
            trim_trailing_zeros: true,
            reject_below_absolute_zero: true,
            allow_case_insensitive_fallback: true,
        }
    }
}

/// Parsed representation of an omnibox conversion query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedConversionQuery {
    pub value: f64,
    pub raw_from_unit: String,
    pub raw_to_unit: String,
    pub connector: String,
}

/// Result of a successful omnibox unit conversion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OmniboxConversionResult {
    pub input_value: f64,
    pub formatted_input: String,
    pub from_unit: UnitInfo,
    pub to_unit: UnitInfo,
    pub output_value: f64,
    pub formatted_output: String,
    pub display_text: String,
    pub category: UnitCategory,
    pub confidence: f32,
}

/// Stateless/configurable omnibox unit converter.
#[derive(Debug, Clone, Default)]
pub struct OmniboxUnitConverter {
    config: ConversionConfig,
}

impl OmniboxUnitConverter {
    /// Creates a converter with default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a converter with custom configuration.
    pub fn with_config(config: ConversionConfig) -> Self {
        Self { config }
    }

    /// Returns the active configuration.
    pub fn config(&self) -> &ConversionConfig {
        &self.config
    }

    /// Parses and evaluates an omnibox query string.
    pub fn evaluate(&self, query: &str) -> Result<OmniboxConversionResult, UnitConverterError> {
        let parsed = parse_conversion_query(query)?;
        self.convert(parsed.value, &parsed.raw_from_unit, &parsed.raw_to_unit)
    }

    /// Convenience helper for omnibox suggestion pipelines: returns `None` if the
    /// query is not a valid conversion.
    pub fn try_evaluate(&self, query: &str) -> Option<OmniboxConversionResult> {
        self.evaluate(query).ok()
    }

    /// Converts an explicit numeric value between two unit strings.
    pub fn convert(
        &self,
        value: f64,
        from_unit_str: &str,
        to_unit_str: &str,
    ) -> Result<OmniboxConversionResult, UnitConverterError> {
        if !value.is_finite() {
            return Err(UnitConverterError::NonFiniteInput);
        }

        let from = resolve_unit(from_unit_str, self.config.allow_case_insensitive_fallback)
            .ok_or_else(|| UnitConverterError::UnknownUnit(from_unit_str.trim().to_string()))?;
        let to = resolve_unit(to_unit_str, self.config.allow_case_insensitive_fallback)
            .ok_or_else(|| UnitConverterError::UnknownUnit(to_unit_str.trim().to_string()))?;

        if from.category != to.category {
            return Err(UnitConverterError::IncompatibleCategories {
                from: from.category,
                to: to.category,
                from_unit: from.symbol.to_string(),
                to_unit: to.symbol.to_string(),
            });
        }

        let base_value = value * from.scale + from.offset;
        if !base_value.is_finite() {
            return Err(UnitConverterError::NonFiniteResult);
        }

        if self.config.reject_below_absolute_zero
            && from.category == UnitCategory::Temperature
            && base_value < -1e-9
        {
            return Err(UnitConverterError::BelowAbsoluteZero {
                value,
                unit: from.symbol.to_string(),
            });
        }

        let raw_output = (base_value - to.offset) / to.scale;
        if !raw_output.is_finite() {
            return Err(UnitConverterError::NonFiniteResult);
        }

        // Clamp tiny negative zero or floating-point epsilon around zero.
        let output_value = if (raw_output.abs() < 1e-12 && value != 0.0 && to.offset != 0.0)
            || raw_output == 0.0
        {
            0.0
        } else {
            raw_output
        };

        let formatted_input = format_number(
            value,
            self.config.max_decimals,
            self.config.trim_trailing_zeros,
        );
        let formatted_output = format_number(
            output_value,
            self.config.max_decimals,
            self.config.trim_trailing_zeros,
        );
        let display_text = format!(
            "{} {} = {} {}",
            formatted_input, from.symbol, formatted_output, to.symbol
        );

        Ok(OmniboxConversionResult {
            input_value: value,
            formatted_input,
            from_unit: from.to_info(),
            to_unit: to.to_info(),
            output_value,
            formatted_output,
            display_text,
            category: from.category,
            confidence: 1.0,
        })
    }

    /// Lists all supported units, optionally filtered by category.
    pub fn list_units(&self, category: Option<UnitCategory>) -> Vec<UnitInfo> {
        UNITS
            .iter()
            .filter(|u| category.is_none_or(|c| u.category == c))
            .map(|u| u.to_info())
            .collect()
    }
}

/// Parses an omnibox conversion query such as `"10 km to mi"` or `"-40C -> F"`.
pub fn parse_conversion_query(query: &str) -> Result<ParsedConversionQuery, UnitConverterError> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(UnitConverterError::EmptyQuery);
    }

    let without_trailing = trimmed.trim_end_matches(['?', '=', ' ']).trim();
    if without_trailing.is_empty() {
        return Err(UnitConverterError::InvalidSyntax);
    }

    // Optional leading "convert " prefix (case-insensitive).
    let body =
        if without_trailing.len() > 8 && without_trailing[..8].eq_ignore_ascii_case("convert ") {
            without_trailing[8..].trim_start()
        } else {
            without_trailing
        };

    let (num_str, rest) = split_leading_number(body).ok_or(UnitConverterError::InvalidSyntax)?;
    let cleaned_num = num_str.replace([',', '_'], "");
    let value: f64 = cleaned_num
        .parse()
        .map_err(|_| UnitConverterError::InvalidNumber(num_str.to_string()))?;
    if !value.is_finite() {
        return Err(UnitConverterError::NonFiniteInput);
    }

    let rest = rest.trim_start();
    if rest.is_empty() {
        return Err(UnitConverterError::InvalidSyntax);
    }

    let (from_unit, connector, to_unit) =
        split_unit_connector(rest).ok_or(UnitConverterError::InvalidSyntax)?;

    if from_unit.is_empty() || to_unit.is_empty() {
        return Err(UnitConverterError::InvalidSyntax);
    }

    Ok(ParsedConversionQuery {
        value,
        raw_from_unit: from_unit.to_string(),
        raw_to_unit: to_unit.to_string(),
        connector: connector.to_string(),
    })
}

fn split_leading_number(input: &str) -> Option<(&str, &str)> {
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    if i < len && (bytes[i] == b'+' || bytes[i] == b'-') {
        i += 1;
    }

    let mut has_digits = false;
    let mut has_dot = false;

    while i < len {
        let b = bytes[i];
        if b.is_ascii_digit() {
            has_digits = true;
            i += 1;
        } else if (b == b',' || b == b'_') && has_digits {
            if i + 1 < len && bytes[i + 1].is_ascii_digit() {
                i += 1;
            } else {
                break;
            }
        } else if b == b'.' && !has_dot {
            has_dot = true;
            i += 1;
        } else {
            break;
        }
    }

    if !has_digits {
        return None;
    }

    // Optional scientific notation exponent (e.g. 1.5e3, 2E-4), only if followed by digits.
    if i < len && (bytes[i] == b'e' || bytes[i] == b'E') {
        let mut j = i + 1;
        if j < len && (bytes[j] == b'+' || bytes[j] == b'-') {
            j += 1;
        }
        let exp_start = j;
        while j < len && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp_start {
            i = j;
        }
    }

    Some((&input[..i], &input[i..]))
}

fn split_unit_connector(input: &str) -> Option<(&str, &str, &str)> {
    // Symbolic connectors can appear with or without spaces.
    for sym in ["->", "=>", "="] {
        if let Some(pos) = input.find(sym) {
            let left = input[..pos].trim();
            let right = input[pos + sym.len()..].trim();
            if !left.is_empty() && !right.is_empty() {
                return Some((left, sym, right));
            }
        }
    }

    // Word connectors must be surrounded by whitespace.
    let lower = input.to_ascii_lowercase();
    for word in [" into ", " to ", " in ", " as "] {
        if let Some(pos) = lower.find(word) {
            let left = input[..pos].trim();
            let conn = input[pos..pos + word.len()].trim();
            let right = input[pos + word.len()..].trim();
            if !left.is_empty() && !right.is_empty() {
                return Some((left, conn, right));
            }
        }
    }

    None
}

fn resolve_unit(raw: &str, allow_case_insensitive: bool) -> Option<&'static UnitSpec> {
    let cleaned = normalize_unit_token(raw);
    if cleaned.is_empty() {
        return None;
    }

    // Pass 1: exact match on canonical symbol or alias.
    if let Some(unit) = find_unit_exact(&cleaned) {
        return Some(unit);
    }

    if !allow_case_insensitive {
        return None;
    }

    // Pass 2: case-insensitive match.
    let lower = cleaned.to_ascii_lowercase();
    find_unit_case_insensitive(&lower)
}

fn normalize_unit_token(raw: &str) -> String {
    let trimmed = raw.trim().trim_start_matches('°');
    // Collapse internal whitespace into single spaces.
    let mut out = String::with_capacity(trimmed.len());
    let mut prev_space = false;
    for ch in trimmed.chars() {
        if ch.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out
}

fn find_unit_exact(token: &str) -> Option<&'static UnitSpec> {
    UNITS
        .iter()
        .find(|spec| spec.symbol == token || spec.aliases.contains(&token))
}

fn find_unit_case_insensitive(lower_token: &str) -> Option<&'static UnitSpec> {
    UNITS.iter().find(|spec| {
        spec.symbol.eq_ignore_ascii_case(lower_token)
            || spec
                .aliases
                .iter()
                .any(|&alias| alias.eq_ignore_ascii_case(lower_token))
    })
}

fn format_number(value: f64, max_decimals: usize, trim_trailing_zeros: bool) -> String {
    if value == 0.0 {
        return "0".to_string();
    }

    let abs = value.abs();
    let min_fixed = 10f64.powi(-(max_decimals as i32));
    if (abs < min_fixed || abs >= 1e12) && max_decimals > 0 {
        // Use scientific notation for very small or very large numbers so precision is preserved.
        let sci = format!("{value:.max_decimals$e}");
        return trim_scientific(sci, trim_trailing_zeros);
    }

    let mut formatted = format!("{value:.max_decimals$}");
    if trim_trailing_zeros && formatted.contains('.') {
        while formatted.ends_with('0') {
            formatted.pop();
        }
        if formatted.ends_with('.') {
            formatted.pop();
        }
    }
    if formatted == "-0" {
        "0".to_string()
    } else {
        formatted
    }
}

fn trim_scientific(sci: String, trim_trailing_zeros: bool) -> String {
    if !trim_trailing_zeros {
        return sci;
    }
    let Some(e_pos) = sci.find('e') else {
        return sci;
    };
    let (mantissa, exp) = sci.split_at(e_pos);
    let mut m = mantissa.to_string();
    if m.contains('.') {
        while m.ends_with('0') {
            m.pop();
        }
        if m.ends_with('.') {
            m.pop();
        }
    }
    format!("{m}{exp}")
}

const KELVIN_OFFSET_C: f64 = 273.15;
const FAHRENHEIT_SCALE: f64 = 5.0 / 9.0;
const FAHRENHEIT_OFFSET_K: f64 = 459.67 * (5.0 / 9.0);

static UNITS: &[UnitSpec] = &[
    // Length (base: meter)
    UnitSpec::linear(
        "nm",
        "nanometer",
        UnitCategory::Length,
        1e-9,
        &["nanometer", "nanometers", "nanometre", "nanometres"],
    ),
    UnitSpec::linear(
        "um",
        "micrometer",
        UnitCategory::Length,
        1e-6,
        &[
            "µm",
            "micrometer",
            "micrometers",
            "micrometre",
            "micrometres",
            "micron",
            "microns",
        ],
    ),
    UnitSpec::linear(
        "mm",
        "millimeter",
        UnitCategory::Length,
        1e-3,
        &["millimeter", "millimeters", "millimetre", "millimetres"],
    ),
    UnitSpec::linear(
        "cm",
        "centimeter",
        UnitCategory::Length,
        1e-2,
        &["centimeter", "centimeters", "centimetre", "centimetres"],
    ),
    UnitSpec::linear(
        "m",
        "meter",
        UnitCategory::Length,
        1.0,
        &["meter", "meters", "metre", "metres"],
    ),
    UnitSpec::linear(
        "km",
        "kilometer",
        UnitCategory::Length,
        1_000.0,
        &["kilometer", "kilometers", "kilometre", "kilometres"],
    ),
    UnitSpec::linear(
        "in",
        "inch",
        UnitCategory::Length,
        0.0254,
        &["inch", "inches"],
    ),
    UnitSpec::linear(
        "ft",
        "foot",
        UnitCategory::Length,
        0.3048,
        &["foot", "feet"],
    ),
    UnitSpec::linear(
        "yd",
        "yard",
        UnitCategory::Length,
        0.9144,
        &["yard", "yards"],
    ),
    UnitSpec::linear(
        "mi",
        "mile",
        UnitCategory::Length,
        1_609.344,
        &["mile", "miles"],
    ),
    UnitSpec::linear(
        "nmi",
        "nautical mile",
        UnitCategory::Length,
        1_852.0,
        &["nautical mile", "nautical miles"],
    ),
    // Mass (base: kilogram)
    UnitSpec::linear(
        "ug",
        "microgram",
        UnitCategory::Mass,
        1e-9,
        &["µg", "mcg", "microgram", "micrograms"],
    ),
    UnitSpec::linear(
        "mg",
        "milligram",
        UnitCategory::Mass,
        1e-6,
        &["milligram", "milligrams"],
    ),
    UnitSpec::linear("g", "gram", UnitCategory::Mass, 1e-3, &["gram", "grams"]),
    UnitSpec::linear(
        "kg",
        "kilogram",
        UnitCategory::Mass,
        1.0,
        &["kilogram", "kilograms", "kilo", "kilos"],
    ),
    UnitSpec::linear(
        "t",
        "metric ton",
        UnitCategory::Mass,
        1_000.0,
        &["tonne", "tonnes", "metric ton", "metric tons"],
    ),
    UnitSpec::linear(
        "oz",
        "ounce",
        UnitCategory::Mass,
        0.028_349_523_125,
        &["ounce", "ounces"],
    ),
    UnitSpec::linear(
        "lb",
        "pound",
        UnitCategory::Mass,
        0.453_592_37,
        &["lbs", "pound", "pounds"],
    ),
    UnitSpec::linear(
        "st",
        "stone",
        UnitCategory::Mass,
        6.350_293_18,
        &["stone", "stones"],
    ),
    // Temperature (base: kelvin)
    UnitSpec::affine(
        "K",
        "kelvin",
        UnitCategory::Temperature,
        1.0,
        0.0,
        &["kelvin", "kelvins"],
    ),
    UnitSpec::affine(
        "C",
        "Celsius",
        UnitCategory::Temperature,
        1.0,
        KELVIN_OFFSET_C,
        &["degC", "celsius", "centigrade"],
    ),
    UnitSpec::affine(
        "F",
        "Fahrenheit",
        UnitCategory::Temperature,
        FAHRENHEIT_SCALE,
        FAHRENHEIT_OFFSET_K,
        &["degF", "fahrenheit"],
    ),
    UnitSpec::affine(
        "R",
        "Rankine",
        UnitCategory::Temperature,
        FAHRENHEIT_SCALE,
        0.0,
        &["rankine"],
    ),
    // Volume (base: liter)
    UnitSpec::linear(
        "mL",
        "milliliter",
        UnitCategory::Volume,
        1e-3,
        &[
            "ml",
            "milliliter",
            "milliliters",
            "millilitre",
            "millilitres",
        ],
    ),
    UnitSpec::linear(
        "L",
        "liter",
        UnitCategory::Volume,
        1.0,
        &["l", "liter", "liters", "litre", "litres"],
    ),
    UnitSpec::linear(
        "m3",
        "cubic meter",
        UnitCategory::Volume,
        1_000.0,
        &[
            "m^3",
            "cubic meter",
            "cubic meters",
            "cubic metre",
            "cubic metres",
        ],
    ),
    UnitSpec::linear(
        "tsp",
        "US teaspoon",
        UnitCategory::Volume,
        0.004_928_921_593_75,
        &["teaspoon", "teaspoons"],
    ),
    UnitSpec::linear(
        "tbsp",
        "US tablespoon",
        UnitCategory::Volume,
        0.014_786_764_781_25,
        &["tablespoon", "tablespoons"],
    ),
    UnitSpec::linear(
        "floz",
        "US fluid ounce",
        UnitCategory::Volume,
        0.029_573_529_562_5,
        &["fl oz", "fl.oz", "fluid ounce", "fluid ounces"],
    ),
    UnitSpec::linear(
        "cup",
        "US cup",
        UnitCategory::Volume,
        0.236_588_236_5,
        &["cups"],
    ),
    UnitSpec::linear(
        "pt",
        "US pint",
        UnitCategory::Volume,
        0.473_176_473,
        &["pint", "pints"],
    ),
    UnitSpec::linear(
        "qt",
        "US quart",
        UnitCategory::Volume,
        0.946_352_946,
        &["quart", "quarts"],
    ),
    UnitSpec::linear(
        "gal",
        "US gallon",
        UnitCategory::Volume,
        3.785_411_784,
        &["gallon", "gallons"],
    ),
    // Area (base: square meter)
    UnitSpec::linear(
        "mm2",
        "square millimeter",
        UnitCategory::Area,
        1e-6,
        &["mm^2", "sq mm", "square millimeter", "square millimeters"],
    ),
    UnitSpec::linear(
        "cm2",
        "square centimeter",
        UnitCategory::Area,
        1e-4,
        &["cm^2", "sq cm", "square centimeter", "square centimeters"],
    ),
    UnitSpec::linear(
        "m2",
        "square meter",
        UnitCategory::Area,
        1.0,
        &["m^2", "sq m", "square meter", "square meters"],
    ),
    UnitSpec::linear(
        "km2",
        "square kilometer",
        UnitCategory::Area,
        1_000_000.0,
        &["km^2", "sq km", "square kilometer", "square kilometers"],
    ),
    UnitSpec::linear(
        "ha",
        "hectare",
        UnitCategory::Area,
        10_000.0,
        &["hectare", "hectares"],
    ),
    UnitSpec::linear(
        "in2",
        "square inch",
        UnitCategory::Area,
        0.000_645_16,
        &["in^2", "sq in", "square inch", "square inches"],
    ),
    UnitSpec::linear(
        "ft2",
        "square foot",
        UnitCategory::Area,
        0.092_903_04,
        &["ft^2", "sq ft", "sqft", "square foot", "square feet"],
    ),
    UnitSpec::linear(
        "yd2",
        "square yard",
        UnitCategory::Area,
        0.836_127_36,
        &["yd^2", "sq yd", "square yard", "square yards"],
    ),
    UnitSpec::linear(
        "mi2",
        "square mile",
        UnitCategory::Area,
        2_589_988.110_336,
        &["mi^2", "sq mi", "square mile", "square miles"],
    ),
    UnitSpec::linear(
        "acre",
        "acre",
        UnitCategory::Area,
        4_046.856_422_4,
        &["acres"],
    ),
    // Speed (base: meters per second)
    UnitSpec::linear(
        "m/s",
        "meters per second",
        UnitCategory::Speed,
        1.0,
        &["mps", "meters per second", "metres per second"],
    ),
    UnitSpec::linear(
        "km/h",
        "kilometers per hour",
        UnitCategory::Speed,
        1.0 / 3.6,
        &[
            "kph",
            "kmh",
            "kmph",
            "kilometers per hour",
            "kilometres per hour",
        ],
    ),
    UnitSpec::linear(
        "mph",
        "miles per hour",
        UnitCategory::Speed,
        0.447_04,
        &["mi/h", "miles per hour"],
    ),
    UnitSpec::linear(
        "ft/s",
        "feet per second",
        UnitCategory::Speed,
        0.3048,
        &["fps", "feet per second"],
    ),
    UnitSpec::linear(
        "kn",
        "knot",
        UnitCategory::Speed,
        1_852.0 / 3_600.0,
        &["kt", "kts", "knot", "knots"],
    ),
    // Time (base: second)
    UnitSpec::linear(
        "ns",
        "nanosecond",
        UnitCategory::Time,
        1e-9,
        &["nanosecond", "nanoseconds"],
    ),
    UnitSpec::linear(
        "us",
        "microsecond",
        UnitCategory::Time,
        1e-6,
        &["µs", "microsecond", "microseconds"],
    ),
    UnitSpec::linear(
        "ms",
        "millisecond",
        UnitCategory::Time,
        1e-3,
        &["millisecond", "milliseconds"],
    ),
    UnitSpec::linear(
        "s",
        "second",
        UnitCategory::Time,
        1.0,
        &["sec", "secs", "second", "seconds"],
    ),
    UnitSpec::linear(
        "min",
        "minute",
        UnitCategory::Time,
        60.0,
        &["mins", "minute", "minutes"],
    ),
    UnitSpec::linear(
        "h",
        "hour",
        UnitCategory::Time,
        3_600.0,
        &["hr", "hrs", "hour", "hours"],
    ),
    UnitSpec::linear("d", "day", UnitCategory::Time, 86_400.0, &["day", "days"]),
    UnitSpec::linear(
        "wk",
        "week",
        UnitCategory::Time,
        604_800.0,
        &["week", "weeks"],
    ),
    UnitSpec::linear(
        "yr",
        "Julian year",
        UnitCategory::Time,
        31_557_600.0,
        &["year", "years"],
    ),
    // Digital Storage (base: byte)
    UnitSpec::linear(
        "b",
        "bit",
        UnitCategory::DigitalStorage,
        0.125,
        &["bit", "bits"],
    ),
    UnitSpec::linear(
        "B",
        "byte",
        UnitCategory::DigitalStorage,
        1.0,
        &["byte", "bytes"],
    ),
    UnitSpec::linear(
        "KB",
        "kilobyte",
        UnitCategory::DigitalStorage,
        1_000.0,
        &["kilobyte", "kilobytes"],
    ),
    UnitSpec::linear(
        "MB",
        "megabyte",
        UnitCategory::DigitalStorage,
        1_000_000.0,
        &["megabyte", "megabytes"],
    ),
    UnitSpec::linear(
        "GB",
        "gigabyte",
        UnitCategory::DigitalStorage,
        1_000_000_000.0,
        &["gigabyte", "gigabytes"],
    ),
    UnitSpec::linear(
        "TB",
        "terabyte",
        UnitCategory::DigitalStorage,
        1_000_000_000_000.0,
        &["terabyte", "terabytes"],
    ),
    UnitSpec::linear(
        "PB",
        "petabyte",
        UnitCategory::DigitalStorage,
        1_000_000_000_000_000.0,
        &["petabyte", "petabytes"],
    ),
    UnitSpec::linear(
        "KiB",
        "kibibyte",
        UnitCategory::DigitalStorage,
        1_024.0,
        &["kibibyte", "kibibytes"],
    ),
    UnitSpec::linear(
        "MiB",
        "mebibyte",
        UnitCategory::DigitalStorage,
        1_048_576.0,
        &["mebibyte", "mebibytes"],
    ),
    UnitSpec::linear(
        "GiB",
        "gibibyte",
        UnitCategory::DigitalStorage,
        1_073_741_824.0,
        &["gibibyte", "gibibytes"],
    ),
    UnitSpec::linear(
        "TiB",
        "tebibyte",
        UnitCategory::DigitalStorage,
        1_099_511_627_776.0,
        &["tebibyte", "tebibytes"],
    ),
    UnitSpec::linear(
        "PiB",
        "pebibyte",
        UnitCategory::DigitalStorage,
        1_125_899_906_842_624.0,
        &["pebibyte", "pebibytes"],
    ),
    // Data Rate (base: bits per second)
    UnitSpec::linear(
        "bps",
        "bits per second",
        UnitCategory::DataRate,
        1.0,
        &["b/s", "bit/s"],
    ),
    UnitSpec::linear(
        "kbps",
        "kilobits per second",
        UnitCategory::DataRate,
        1_000.0,
        &["kb/s", "kbit/s"],
    ),
    UnitSpec::linear(
        "Mbps",
        "megabits per second",
        UnitCategory::DataRate,
        1_000_000.0,
        &["Mb/s", "Mbit/s"],
    ),
    UnitSpec::linear(
        "Gbps",
        "gigabits per second",
        UnitCategory::DataRate,
        1_000_000_000.0,
        &["Gb/s", "Gbit/s"],
    ),
    UnitSpec::linear(
        "Tbps",
        "terabits per second",
        UnitCategory::DataRate,
        1_000_000_000_000.0,
        &["Tb/s", "Tbit/s"],
    ),
    UnitSpec::linear(
        "B/s",
        "bytes per second",
        UnitCategory::DataRate,
        8.0,
        &["Bps"],
    ),
    UnitSpec::linear(
        "KB/s",
        "kilobytes per second",
        UnitCategory::DataRate,
        8_000.0,
        &["KBps"],
    ),
    UnitSpec::linear(
        "MB/s",
        "megabytes per second",
        UnitCategory::DataRate,
        8_000_000.0,
        &["MBps"],
    ),
    UnitSpec::linear(
        "GB/s",
        "gigabytes per second",
        UnitCategory::DataRate,
        8_000_000_000.0,
        &["GBps"],
    ),
    // Energy (base: joule)
    UnitSpec::linear(
        "J",
        "joule",
        UnitCategory::Energy,
        1.0,
        &["joule", "joules"],
    ),
    UnitSpec::linear(
        "kJ",
        "kilojoule",
        UnitCategory::Energy,
        1_000.0,
        &["kilojoule", "kilojoules"],
    ),
    UnitSpec::linear(
        "MJ",
        "megajoule",
        UnitCategory::Energy,
        1_000_000.0,
        &["megajoule", "megajoules"],
    ),
    UnitSpec::linear(
        "cal",
        "calorie",
        UnitCategory::Energy,
        4.184,
        &["calorie", "calories"],
    ),
    UnitSpec::linear(
        "kcal",
        "kilocalorie",
        UnitCategory::Energy,
        4_184.0,
        &["kilocalorie", "kilocalories"],
    ),
    UnitSpec::linear(
        "Wh",
        "watt-hour",
        UnitCategory::Energy,
        3_600.0,
        &["watt hour", "watt-hour", "watt-hours"],
    ),
    UnitSpec::linear(
        "kWh",
        "kilowatt-hour",
        UnitCategory::Energy,
        3_600_000.0,
        &["kilowatt hour", "kilowatt-hour", "kilowatt-hours"],
    ),
    UnitSpec::linear(
        "BTU",
        "British thermal unit",
        UnitCategory::Energy,
        1_055.055_852_62,
        &["btu"],
    ),
    UnitSpec::linear(
        "eV",
        "electronvolt",
        UnitCategory::Energy,
        1.602_176_634e-19,
        &["electronvolt", "electronvolts"],
    ),
    // Pressure (base: pascal)
    UnitSpec::linear(
        "Pa",
        "pascal",
        UnitCategory::Pressure,
        1.0,
        &["pascal", "pascals"],
    ),
    UnitSpec::linear(
        "kPa",
        "kilopascal",
        UnitCategory::Pressure,
        1_000.0,
        &["kilopascal", "kilopascals"],
    ),
    UnitSpec::linear(
        "MPa",
        "megapascal",
        UnitCategory::Pressure,
        1_000_000.0,
        &["megapascal", "megapascals"],
    ),
    UnitSpec::linear("bar", "bar", UnitCategory::Pressure, 100_000.0, &["bars"]),
    UnitSpec::linear(
        "mbar",
        "millibar",
        UnitCategory::Pressure,
        100.0,
        &["millibar", "millibars"],
    ),
    UnitSpec::linear(
        "atm",
        "standard atmosphere",
        UnitCategory::Pressure,
        101_325.0,
        &["atmosphere", "atmospheres"],
    ),
    UnitSpec::linear(
        "psi",
        "pounds per square inch",
        UnitCategory::Pressure,
        6_894.757_293_168_361,
        &[],
    ),
    UnitSpec::linear(
        "mmHg",
        "millimeter of mercury",
        UnitCategory::Pressure,
        133.322_387_415,
        &["torr"],
    ),
    // Angle (base: radian)
    UnitSpec::linear(
        "rad",
        "radian",
        UnitCategory::Angle,
        1.0,
        &["radian", "radians"],
    ),
    UnitSpec::linear(
        "deg",
        "degree",
        UnitCategory::Angle,
        std::f64::consts::PI / 180.0,
        &["degree", "degrees"],
    ),
    UnitSpec::linear(
        "grad",
        "gradian",
        UnitCategory::Angle,
        std::f64::consts::PI / 200.0,
        &["gradian", "gradians"],
    ),
    UnitSpec::linear(
        "turn",
        "turn",
        UnitCategory::Angle,
        std::f64::consts::TAU,
        &["turns", "rev", "revolution", "revolutions"],
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_length_and_formats_display_text() {
        let converter = OmniboxUnitConverter::new();
        let res = converter.evaluate("10 km to mi").unwrap();
        assert_eq!(res.category, UnitCategory::Length);
        assert_eq!(res.from_unit.symbol, "km");
        assert_eq!(res.to_unit.symbol, "mi");
        assert!((res.output_value - 6.213_711_922_373_34).abs() < 1e-9);
        assert_eq!(res.display_text, "10 km = 6.213712 mi");
    }

    #[test]
    fn converts_temperatures_across_scales_and_negatives() {
        let converter = OmniboxUnitConverter::new();

        let boiling = converter.evaluate("100 °C in F").unwrap();
        assert_eq!(boiling.category, UnitCategory::Temperature);
        assert!((boiling.output_value - 212.0).abs() < 1e-9);
        assert_eq!(boiling.formatted_output, "212");

        let crossover = converter.evaluate("-40C -> F").unwrap();
        assert!((crossover.output_value - (-40.0)).abs() < 1e-9);
        assert_eq!(crossover.display_text, "-40 C = -40 F");

        let abs_zero = converter.evaluate("0 K to C").unwrap();
        assert!((abs_zero.output_value - (-273.15)).abs() < 1e-9);
        assert_eq!(abs_zero.formatted_output, "-273.15");
    }

    #[test]
    fn rejects_temperature_below_absolute_zero() {
        let converter = OmniboxUnitConverter::new();
        let err = converter.evaluate("-300 C to K").unwrap_err();
        assert_eq!(
            err,
            UnitConverterError::BelowAbsoluteZero {
                value: -300.0,
                unit: "C".to_string(),
            }
        );
    }

    #[test]
    fn handles_case_sensitive_bits_bytes_and_scientific_notation() {
        let converter = OmniboxUnitConverter::new();

        let bits = converter.evaluate("16 b to B").unwrap();
        assert!((bits.output_value - 2.0).abs() < 1e-12);
        assert_eq!(bits.from_unit.symbol, "b");
        assert_eq!(bits.to_unit.symbol, "B");

        let gib = converter.evaluate("1 GiB in MB").unwrap();
        assert!((gib.output_value - 1_073.741_824).abs() < 1e-6);

        let sci = converter.evaluate("1.5e3 m = km").unwrap();
        assert!((sci.output_value - 1.5).abs() < 1e-12);
        assert_eq!(sci.formatted_output, "1.5");
    }

    #[test]
    fn rejects_incompatible_categories_and_bad_syntax() {
        let converter = OmniboxUnitConverter::new();

        let mismatch = converter.evaluate("10 kg to m").unwrap_err();
        assert_eq!(
            mismatch,
            UnitConverterError::IncompatibleCategories {
                from: UnitCategory::Mass,
                to: UnitCategory::Length,
                from_unit: "kg".to_string(),
                to_unit: "m".to_string(),
            }
        );

        assert_eq!(
            converter.evaluate("   ").unwrap_err(),
            UnitConverterError::EmptyQuery
        );
        assert_eq!(
            converter.evaluate("hello world").unwrap_err(),
            UnitConverterError::InvalidSyntax
        );
        assert_eq!(
            converter.evaluate("10 foo to bar").unwrap_err(),
            UnitConverterError::UnknownUnit("foo".to_string())
        );
        assert!(converter.try_evaluate("not a conversion").is_none());
    }

    #[test]
    fn supports_comma_numbers_convert_prefix_and_serde_roundtrip() {
        let converter = OmniboxUnitConverter::new();
        let res = converter.evaluate("convert 1,000 mm into m?").unwrap();
        assert!((res.output_value - 1.0).abs() < 1e-12);

        let json = serde_json::to_string(&res).unwrap();
        let decoded: OmniboxConversionResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, res);
    }
}
