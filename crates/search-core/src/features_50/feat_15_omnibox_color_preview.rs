//! Omnibox color preview and conversion engine for Search Linux.
//!
//! Detects color literals and expressions entered in the omnibox (hex, `rgb`/`rgba`,
//! `hsl`/`hsla`, `hsv`/`hsb`, and CSS named colors), normalizes them to RGBA,
//! computes WCAG 2.1 relative luminance and contrast assessments, and generates
//! formatted conversions for live swatch rendering.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Default maximum input length accepted by the omnibox color previewer.
pub const DEFAULT_MAX_INPUT_LEN: usize = 128;

/// WCAG 2.1 minimum contrast ratio for normal text (AA).
pub const WCAG_AA_NORMAL_CONTRAST: f64 = 4.5;

/// WCAG 2.1 minimum contrast ratio for enhanced contrast (AAA).
pub const WCAG_AAA_NORMAL_CONTRAST: f64 = 7.0;

/// Supported color input and output formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorFormat {
    /// 3- or 6-digit hexadecimal color (`#rgb` or `#rrggbb`).
    Hex,
    /// 4- or 8-digit hexadecimal color with alpha (`#rgba` or `#rrggbbaa`).
    HexAlpha,
    /// `rgb(...)` functional notation.
    Rgb,
    /// `rgba(...)` functional notation.
    Rgba,
    /// `hsl(...)` functional notation.
    Hsl,
    /// `hsla(...)` functional notation.
    Hsla,
    /// `hsv(...)` / `hsb(...)` functional notation.
    Hsv,
    /// Standard CSS named color.
    Named,
}

/// Errors produced during color preview configuration, parsing, or validation.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ColorPreviewError {
    /// The omnibox input was empty or whitespace-only.
    #[error("color input is empty")]
    EmptyInput,

    /// The input exceeded the configured maximum byte length.
    #[error("color input length {len} exceeds maximum {max}")]
    InputTooLong { len: usize, max: usize },

    /// The input did not match any supported color syntax.
    #[error("unrecognized color expression: {0}")]
    UnrecognizedColor(String),

    /// A hexadecimal color literal had invalid length or non-hex digits.
    #[error("invalid hex color: {0}")]
    InvalidHex(String),

    /// A functional color expression had invalid syntax or out-of-range components.
    #[error("invalid {format} component: {detail}")]
    InvalidComponent {
        format: &'static str,
        detail: String,
    },

    /// The engine configuration is invalid.
    #[error("invalid color preview configuration: {0}")]
    InvalidConfig(String),
}

/// Configuration for [`OmniboxColorPreviewer`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorPreviewConfig {
    /// Whether bare CSS named colors (without `color:` prefix) are recognized.
    pub allow_bare_named_colors: bool,
    /// Whether 6- or 8-digit hex strings without a leading `#` are recognized.
    pub allow_hashless_hex: bool,
    /// Maximum allowed input length in bytes.
    pub max_input_len: usize,
    /// Background color used when compositing translucent swatches.
    pub default_background: ColorRgba,
}

impl Default for ColorPreviewConfig {
    fn default() -> Self {
        Self {
            allow_bare_named_colors: true,
            allow_hashless_hex: false,
            max_input_len: DEFAULT_MAX_INPUT_LEN,
            default_background: ColorRgba::rgb(255, 255, 255),
        }
    }
}

impl ColorPreviewConfig {
    /// Validates the configuration parameters.
    pub fn validate(&self) -> Result<(), ColorPreviewError> {
        if self.max_input_len == 0 {
            return Err(ColorPreviewError::InvalidConfig(
                "max_input_len must be greater than zero".to_string(),
            ));
        }
        if self.default_background.a == 0 {
            return Err(ColorPreviewError::InvalidConfig(
                "default_background must not be fully transparent".to_string(),
            ));
        }
        Ok(())
    }
}

/// 8-bit per channel RGBA color representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ColorRgba {
    /// Red channel (`0..=255`).
    pub r: u8,
    /// Green channel (`0..=255`).
    pub g: u8,
    /// Blue channel (`0..=255`).
    pub b: u8,
    /// Alpha channel (`0..=255`, where `255` is fully opaque).
    pub a: u8,
}

impl ColorRgba {
    /// Creates an opaque RGBA color.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Creates an RGBA color with explicit alpha.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Returns the normalized alpha channel in `[0.0, 1.0]`.
    pub fn alpha_f64(self) -> f64 {
        f64::from(self.a) / 255.0
    }

    /// Formats the color as `#rrggbb` (lowercase).
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Formats the color as `#rrggbbaa` (lowercase).
    pub fn to_hex_alpha(self) -> String {
        format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
    }

    /// Formats the color as `rgb(r, g, b)`.
    pub fn to_rgb_string(self) -> String {
        format!("rgb({}, {}, {})", self.r, self.g, self.b)
    }

    /// Formats the color as `rgba(r, g, b, alpha)`.
    pub fn to_rgba_string(self) -> String {
        format!(
            "rgba({}, {}, {}, {})",
            self.r,
            self.g,
            self.b,
            format_alpha(self.alpha_f64())
        )
    }

    /// Converts this RGBA color to HSL.
    pub fn to_hsl(self) -> ColorHsl {
        ColorHsl::from_rgba(self)
    }

    /// Converts this RGBA color to HSV.
    pub fn to_hsv(self) -> ColorHsv {
        ColorHsv::from_rgba(self)
    }

    /// Alpha-composites `self` over an opaque or translucent `background`.
    pub fn blend_over(self, background: Self) -> Self {
        let fg_a = self.alpha_f64();
        let bg_a = background.alpha_f64();
        let out_a = fg_a + bg_a * (1.0 - fg_a);
        if out_a <= f64::EPSILON {
            return Self::rgba(0, 0, 0, 0);
        }
        let blend_ch = |fg: u8, bg: u8| -> u8 {
            let num = f64::from(fg) * fg_a + f64::from(bg) * bg_a * (1.0 - fg_a);
            (num / out_a).round().clamp(0.0, 255.0) as u8
        };
        Self {
            r: blend_ch(self.r, background.r),
            g: blend_ch(self.g, background.g),
            b: blend_ch(self.b, background.b),
            a: (out_a * 255.0).round().clamp(0.0, 255.0) as u8,
        }
    }

    /// Computes WCAG 2.1 relative luminance in `[0.0, 1.0]` (ignoring alpha).
    pub fn relative_luminance(self) -> f64 {
        let lin = |c: u8| -> f64 {
            let srgb = f64::from(c) / 255.0;
            if srgb <= 0.04045 {
                srgb / 12.92
            } else {
                ((srgb + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }

    /// Computes the WCAG 2.1 contrast ratio against `other` in `[1.0, 21.0]`.
    pub fn contrast_ratio_against(self, other: Self) -> f64 {
        let l1 = self.relative_luminance();
        let l2 = other.relative_luminance();
        let (lighter, darker) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
        (lighter + 0.05) / (darker + 0.05)
    }
}

/// HSL color representation (`h` in `[0, 360)`, `s`/`l` in `[0, 100]`, `a` in `[0, 1]`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ColorHsl {
    /// Hue in degrees `[0.0, 360.0)`.
    pub h: f64,
    /// Saturation percentage `[0.0, 100.0]`.
    pub s: f64,
    /// Lightness percentage `[0.0, 100.0]`.
    pub l: f64,
    /// Alpha in `[0.0, 1.0]`.
    pub a: f64,
}

impl ColorHsl {
    /// Converts an RGBA color to HSL.
    pub fn from_rgba(rgba: ColorRgba) -> Self {
        let r = f64::from(rgba.r) / 255.0;
        let g = f64::from(rgba.g) / 255.0;
        let b = f64::from(rgba.b) / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        let l = (max + min) / 2.0;

        let s = if delta <= 1e-12 {
            0.0
        } else {
            delta / (1.0 - (2.0 * l - 1.0).abs())
        };

        let mut h = if delta <= 1e-12 {
            0.0
        } else if (max - r).abs() <= 1e-12 {
            60.0 * (((g - b) / delta) % 6.0)
        } else if (max - g).abs() <= 1e-12 {
            60.0 * (((b - r) / delta) + 2.0)
        } else {
            60.0 * (((r - g) / delta) + 4.0)
        };
        if h < 0.0 {
            h += 360.0;
        }
        h = normalize_hue(h);

        Self {
            h,
            s: (s * 100.0).clamp(0.0, 100.0),
            l: (l * 100.0).clamp(0.0, 100.0),
            a: rgba.alpha_f64(),
        }
    }

    /// Converts this HSL color to 8-bit RGBA.
    pub fn to_rgba(self) -> ColorRgba {
        let h = normalize_hue(self.h);
        let s = (self.s / 100.0).clamp(0.0, 1.0);
        let l = (self.l / 100.0).clamp(0.0, 1.0);
        let a = (self.a.clamp(0.0, 1.0) * 255.0).round() as u8;

        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let m = l - c / 2.0;

        let (rp, gp, bp) = if h < 60.0 {
            (c, x, 0.0)
        } else if h < 120.0 {
            (x, c, 0.0)
        } else if h < 180.0 {
            (0.0, c, x)
        } else if h < 240.0 {
            (0.0, x, c)
        } else if h < 300.0 {
            (x, 0.0, c)
        } else {
            (c, 0.0, x)
        };

        let to_u8 = |v: f64| -> u8 { ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8 };
        ColorRgba::rgba(to_u8(rp), to_u8(gp), to_u8(bp), a)
    }

    /// Formats as `hsl(h, s%, l%)`.
    pub fn to_hsl_string(self) -> String {
        format!(
            "hsl({}, {}%, {}%)",
            self.h.round() as i64,
            self.s.round() as i64,
            self.l.round() as i64
        )
    }

    /// Formats as `hsla(h, s%, l%, a)`.
    pub fn to_hsla_string(self) -> String {
        format!(
            "hsla({}, {}%, {}%, {})",
            self.h.round() as i64,
            self.s.round() as i64,
            self.l.round() as i64,
            format_alpha(self.a)
        )
    }
}

/// HSV/HSB color representation (`h` in `[0, 360)`, `s`/`v` in `[0, 100]`, `a` in `[0, 1]`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ColorHsv {
    /// Hue in degrees `[0.0, 360.0)`.
    pub h: f64,
    /// Saturation percentage `[0.0, 100.0]`.
    pub s: f64,
    /// Value/brightness percentage `[0.0, 100.0]`.
    pub v: f64,
    /// Alpha in `[0.0, 1.0]`.
    pub a: f64,
}

impl ColorHsv {
    /// Converts an RGBA color to HSV.
    pub fn from_rgba(rgba: ColorRgba) -> Self {
        let r = f64::from(rgba.r) / 255.0;
        let g = f64::from(rgba.g) / 255.0;
        let b = f64::from(rgba.b) / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;

        let s = if max <= 1e-12 { 0.0 } else { delta / max };
        let mut h = if delta <= 1e-12 {
            0.0
        } else if (max - r).abs() <= 1e-12 {
            60.0 * (((g - b) / delta) % 6.0)
        } else if (max - g).abs() <= 1e-12 {
            60.0 * (((b - r) / delta) + 2.0)
        } else {
            60.0 * (((r - g) / delta) + 4.0)
        };
        if h < 0.0 {
            h += 360.0;
        }
        h = normalize_hue(h);

        Self {
            h,
            s: (s * 100.0).clamp(0.0, 100.0),
            v: (max * 100.0).clamp(0.0, 100.0),
            a: rgba.alpha_f64(),
        }
    }

    /// Converts this HSV color to 8-bit RGBA.
    pub fn to_rgba(self) -> ColorRgba {
        let h = normalize_hue(self.h);
        let s = (self.s / 100.0).clamp(0.0, 1.0);
        let v = (self.v / 100.0).clamp(0.0, 1.0);
        let a = (self.a.clamp(0.0, 1.0) * 255.0).round() as u8;

        let c = v * s;
        let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let m = v - c;

        let (rp, gp, bp) = if h < 60.0 {
            (c, x, 0.0)
        } else if h < 120.0 {
            (x, c, 0.0)
        } else if h < 180.0 {
            (0.0, c, x)
        } else if h < 240.0 {
            (0.0, x, c)
        } else if h < 300.0 {
            (x, 0.0, c)
        } else {
            (c, 0.0, x)
        };

        let to_u8 = |ch: f64| -> u8 { ((ch + m) * 255.0).round().clamp(0.0, 255.0) as u8 };
        ColorRgba::rgba(to_u8(rp), to_u8(gp), to_u8(bp), a)
    }

    /// Formats as `hsv(h, s%, v%)`.
    pub fn to_hsv_string(self) -> String {
        format!(
            "hsv({}, {}%, {}%)",
            self.h.round() as i64,
            self.s.round() as i64,
            self.v.round() as i64
        )
    }
}

/// WCAG 2.1 contrast assessment for a color swatch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContrastAssessment {
    /// Contrast ratio of the color (composited over white if translucent) against `#ffffff`.
    pub vs_white: f64,
    /// Contrast ratio of the color (composited over black if translucent) against `#000000`.
    pub vs_black: f64,
    /// Recommended foreground color (`#000000` or `#ffffff`) for text rendered on the swatch.
    pub recommended_text: ColorRgba,
    /// Whether the color meets WCAG AA (4.5:1) against white.
    pub wcag_aa_on_white: bool,
    /// Whether the color meets WCAG AA (4.5:1) against black.
    pub wcag_aa_on_black: bool,
    /// Whether the color meets WCAG AAA (7.0:1) against white.
    pub wcag_aaa_on_white: bool,
    /// Whether the color meets WCAG AAA (7.0:1) against black.
    pub wcag_aaa_on_black: bool,
}

/// A single copyable color conversion displayed in the omnibox dropdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorConversion {
    /// Target format of this conversion.
    pub format: ColorFormat,
    /// Human-readable label (e.g., `"HEX"`, `"RGB"`, `"HSL"`).
    pub label: String,
    /// Formatted color string.
    pub value: String,
}

/// Full omnibox color preview payload returned to the UI layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OmniboxColorPreview {
    /// Normalized query string that produced this preview.
    pub query: String,
    /// Format detected from the user's input.
    pub detected_format: ColorFormat,
    /// Normalized 8-bit RGBA color.
    pub rgba: ColorRgba,
    /// Swatch hex string (`#rrggbb`).
    pub swatch_hex: String,
    /// Swatch hex string with alpha (`#rrggbbaa`).
    pub swatch_hex_alpha: String,
    /// Exact CSS named color match, if one exists.
    pub matched_name: Option<String>,
    /// WCAG 2.1 relative luminance of the composited color.
    pub relative_luminance: f64,
    /// Contrast assessment for legibility and accessibility badges.
    pub contrast: ContrastAssessment,
    /// Ordered list of copyable color conversions.
    pub conversions: Vec<ColorConversion>,
}

/// Stateless/configurable engine for parsing omnibox inputs into [`OmniboxColorPreview`].
#[derive(Debug, Clone)]
pub struct OmniboxColorPreviewer {
    config: ColorPreviewConfig,
}

impl OmniboxColorPreviewer {
    /// Creates a new previewer after validating `config`.
    pub fn new(config: ColorPreviewConfig) -> Result<Self, ColorPreviewError> {
        config.validate()?;
        Ok(Self { config })
    }

    /// Creates a previewer with [`ColorPreviewConfig::default`].
    pub fn with_defaults() -> Self {
        Self {
            config: ColorPreviewConfig::default(),
        }
    }

    /// Returns the active configuration.
    pub fn config(&self) -> &ColorPreviewConfig {
        &self.config
    }

    /// Attempts to parse `input` and build a full [`OmniboxColorPreview`].
    pub fn preview(&self, input: &str) -> Result<OmniboxColorPreview, ColorPreviewError> {
        if input.len() > self.config.max_input_len {
            return Err(ColorPreviewError::InputTooLong {
                len: input.len(),
                max: self.config.max_input_len,
            });
        }

        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(ColorPreviewError::EmptyInput);
        }

        let (had_prefix, expr) = strip_omnibox_prefix(trimmed);
        if expr.is_empty() {
            return Err(ColorPreviewError::EmptyInput);
        }

        let (detected_format, rgba) = self.parse_color_expr(expr, had_prefix)?;
        Ok(self.build_preview(trimmed.to_string(), detected_format, rgba))
    }

    /// Convenience wrapper returning `None` when `input` is not a valid color expression.
    pub fn try_preview(&self, input: &str) -> Option<OmniboxColorPreview> {
        self.preview(input).ok()
    }

    fn parse_color_expr(
        &self,
        expr: &str,
        had_prefix: bool,
    ) -> Result<(ColorFormat, ColorRgba), ColorPreviewError> {
        if let Some(hex_digits) = expr.strip_prefix('#') {
            return parse_hex_digits(hex_digits);
        }

        if let Some((func_name, inner)) = split_functional_expr(expr) {
            let lower_fn = func_name.to_ascii_lowercase();
            return match lower_fn.as_str() {
                "rgb" | "rgba" => parse_rgb_functional(inner, lower_fn == "rgba"),
                "hsl" | "hsla" => parse_hsl_functional(inner, lower_fn == "hsla"),
                "hsv" | "hsb" => parse_hsv_functional(inner),
                _ => Err(ColorPreviewError::UnrecognizedColor(expr.to_string())),
            };
        }

        if (self.config.allow_bare_named_colors || had_prefix)
            && expr.chars().all(|c| c.is_ascii_alphabetic())
        {
            if let Some(rgba) = lookup_css_named_color(expr) {
                return Ok((ColorFormat::Named, rgba));
            }
        }

        if self.config.allow_hashless_hex
            && (expr.len() == 6 || expr.len() == 8)
            && expr.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return parse_hex_digits(expr);
        }

        Err(ColorPreviewError::UnrecognizedColor(expr.to_string()))
    }

    fn build_preview(
        &self,
        query: String,
        detected_format: ColorFormat,
        rgba: ColorRgba,
    ) -> OmniboxColorPreview {
        let white = ColorRgba::rgb(255, 255, 255);
        let black = ColorRgba::rgb(0, 0, 0);

        let over_white = rgba.blend_over(white);
        let over_black = rgba.blend_over(black);
        let over_default = rgba.blend_over(self.config.default_background);

        let vs_white = round_2(over_white.contrast_ratio_against(white));
        let vs_black = round_2(over_black.contrast_ratio_against(black));

        let contrast_default_white = over_default.contrast_ratio_against(white);
        let contrast_default_black = over_default.contrast_ratio_against(black);
        let recommended_text = if contrast_default_white >= contrast_default_black {
            white
        } else {
            black
        };

        let contrast = ContrastAssessment {
            vs_white,
            vs_black,
            recommended_text,
            wcag_aa_on_white: vs_white >= WCAG_AA_NORMAL_CONTRAST,
            wcag_aa_on_black: vs_black >= WCAG_AA_NORMAL_CONTRAST,
            wcag_aaa_on_white: vs_white >= WCAG_AAA_NORMAL_CONTRAST,
            wcag_aaa_on_black: vs_black >= WCAG_AAA_NORMAL_CONTRAST,
        };

        let hsl = rgba.to_hsl();
        let hsv = rgba.to_hsv();
        let matched_name = css_name_for_rgba(rgba).map(str::to_string);

        let mut conversions = vec![
            ColorConversion {
                format: ColorFormat::Hex,
                label: "HEX".to_string(),
                value: rgba.to_hex(),
            },
            ColorConversion {
                format: ColorFormat::Rgb,
                label: "RGB".to_string(),
                value: if rgba.a == 255 {
                    rgba.to_rgb_string()
                } else {
                    rgba.to_rgba_string()
                },
            },
            ColorConversion {
                format: if rgba.a == 255 {
                    ColorFormat::Hsl
                } else {
                    ColorFormat::Hsla
                },
                label: "HSL".to_string(),
                value: if rgba.a == 255 {
                    hsl.to_hsl_string()
                } else {
                    hsl.to_hsla_string()
                },
            },
            ColorConversion {
                format: ColorFormat::Hsv,
                label: "HSV".to_string(),
                value: hsv.to_hsv_string(),
            },
        ];

        if rgba.a < 255 {
            conversions.insert(
                1,
                ColorConversion {
                    format: ColorFormat::HexAlpha,
                    label: "HEXA".to_string(),
                    value: rgba.to_hex_alpha(),
                },
            );
        }

        if let Some(ref name) = matched_name {
            conversions.push(ColorConversion {
                format: ColorFormat::Named,
                label: "CSS".to_string(),
                value: name.clone(),
            });
        }

        OmniboxColorPreview {
            query,
            detected_format,
            rgba,
            swatch_hex: rgba.to_hex(),
            swatch_hex_alpha: rgba.to_hex_alpha(),
            matched_name,
            relative_luminance: round_4(over_default.relative_luminance()),
            contrast,
            conversions,
        }
    }
}

fn strip_omnibox_prefix(input: &str) -> (bool, &str) {
    let lower = input.to_ascii_lowercase();
    for prefix in ["color:", "colour:", "="] {
        if lower.starts_with(prefix) {
            return (true, input[prefix.len()..].trim());
        }
    }
    (false, input)
}

fn split_functional_expr(expr: &str) -> Option<(&str, &str)> {
    let open = expr.find('(')?;
    if !expr.ends_with(')') || open == 0 {
        return None;
    }
    let name = expr[..open].trim();
    if !name.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let inner = expr[open + 1..expr.len() - 1].trim();
    Some((name, inner))
}

fn parse_hex_digits(hex: &str) -> Result<(ColorFormat, ColorRgba), ColorPreviewError> {
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ColorPreviewError::InvalidHex(format!("#{hex}")));
    }

    let nibble = |idx: usize| -> Result<u8, ColorPreviewError> {
        let s = hex
            .get(idx..idx + 1)
            .ok_or_else(|| ColorPreviewError::InvalidHex(format!("#{hex}")))?;
        let v = u8::from_str_radix(s, 16)
            .map_err(|_| ColorPreviewError::InvalidHex(format!("#{hex}")))?;
        Ok(v * 17)
    };

    let byte = |idx: usize| -> Result<u8, ColorPreviewError> {
        let s = hex
            .get(idx..idx + 2)
            .ok_or_else(|| ColorPreviewError::InvalidHex(format!("#{hex}")))?;
        u8::from_str_radix(s, 16).map_err(|_| ColorPreviewError::InvalidHex(format!("#{hex}")))
    };

    match hex.len() {
        3 => Ok((
            ColorFormat::Hex,
            ColorRgba::rgb(nibble(0)?, nibble(1)?, nibble(2)?),
        )),
        4 => Ok((
            ColorFormat::HexAlpha,
            ColorRgba::rgba(nibble(0)?, nibble(1)?, nibble(2)?, nibble(3)?),
        )),
        6 => Ok((
            ColorFormat::Hex,
            ColorRgba::rgb(byte(0)?, byte(2)?, byte(4)?),
        )),
        8 => Ok((
            ColorFormat::HexAlpha,
            ColorRgba::rgba(byte(0)?, byte(2)?, byte(4)?, byte(6)?),
        )),
        _ => Err(ColorPreviewError::InvalidHex(format!("#{hex}"))),
    }
}

fn split_func_args(inner: &str) -> Result<(Vec<&str>, Option<&str>), ColorPreviewError> {
    if inner.contains(',') {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        if parts.iter().any(|p| p.is_empty()) {
            return Err(ColorPreviewError::InvalidComponent {
                format: "function",
                detail: "empty argument in comma-separated list".to_string(),
            });
        }
        if parts.len() == 3 {
            return Ok((parts, None));
        }
        if parts.len() == 4 {
            return Ok((parts[..3].to_vec(), Some(parts[3])));
        }
        return Err(ColorPreviewError::InvalidComponent {
            format: "function",
            detail: format!("expected 3 or 4 arguments, found {}", parts.len()),
        });
    }

    let (channels_part, alpha_part) = if let Some((left, right)) = inner.split_once('/') {
        let right = right.trim();
        if right.is_empty() || right.contains('/') {
            return Err(ColorPreviewError::InvalidComponent {
                format: "function",
                detail: "invalid alpha separator".to_string(),
            });
        }
        (left.trim(), Some(right))
    } else {
        (inner, None)
    };

    let channels: Vec<&str> = channels_part.split_whitespace().collect();
    if channels.len() != 3 {
        return Err(ColorPreviewError::InvalidComponent {
            format: "function",
            detail: format!("expected 3 color channels, found {}", channels.len()),
        });
    }
    Ok((channels, alpha_part))
}

fn parse_rgb_functional(
    inner: &str,
    explicit_rgba: bool,
) -> Result<(ColorFormat, ColorRgba), ColorPreviewError> {
    let (channels, alpha_opt) = split_func_args(inner)?;
    let r = parse_rgb_channel(channels[0])?;
    let g = parse_rgb_channel(channels[1])?;
    let b = parse_rgb_channel(channels[2])?;
    let a = match alpha_opt {
        Some(a_str) => parse_alpha_u8(a_str, "rgb")?,
        None => 255,
    };
    let format = if explicit_rgba || alpha_opt.is_some() {
        ColorFormat::Rgba
    } else {
        ColorFormat::Rgb
    };
    Ok((format, ColorRgba::rgba(r, g, b, a)))
}

fn parse_rgb_channel(token: &str) -> Result<u8, ColorPreviewError> {
    if let Some(pct) = token.strip_suffix('%') {
        let v: f64 = pct
            .trim()
            .parse()
            .map_err(|_| ColorPreviewError::InvalidComponent {
                format: "rgb",
                detail: format!("invalid percentage channel '{token}'"),
            })?;
        if !v.is_finite() || !(0.0..=100.0).contains(&v) {
            return Err(ColorPreviewError::InvalidComponent {
                format: "rgb",
                detail: format!("percentage channel out of range: '{token}'"),
            });
        }
        return Ok(((v / 100.0) * 255.0).round() as u8);
    }

    let v: f64 = token
        .parse()
        .map_err(|_| ColorPreviewError::InvalidComponent {
            format: "rgb",
            detail: format!("invalid numeric channel '{token}'"),
        })?;
    if !v.is_finite() || !(0.0..=255.0).contains(&v) {
        return Err(ColorPreviewError::InvalidComponent {
            format: "rgb",
            detail: format!("channel out of 0..=255 range: '{token}'"),
        });
    }
    Ok(v.round() as u8)
}

fn parse_hsl_functional(
    inner: &str,
    explicit_hsla: bool,
) -> Result<(ColorFormat, ColorRgba), ColorPreviewError> {
    let (channels, alpha_opt) = split_func_args(inner)?;
    let h = parse_hue_degrees(channels[0], "hsl")?;
    let s = parse_percentage(channels[1], "hsl")?;
    let l = parse_percentage(channels[2], "hsl")?;
    let a = match alpha_opt {
        Some(a_str) => parse_alpha_f64(a_str, "hsl")?,
        None => 1.0,
    };
    let rgba = ColorHsl { h, s, l, a }.to_rgba();
    let format = if explicit_hsla || alpha_opt.is_some() {
        ColorFormat::Hsla
    } else {
        ColorFormat::Hsl
    };
    Ok((format, rgba))
}

fn parse_hsv_functional(inner: &str) -> Result<(ColorFormat, ColorRgba), ColorPreviewError> {
    let (channels, alpha_opt) = split_func_args(inner)?;
    let h = parse_hue_degrees(channels[0], "hsv")?;
    let s = parse_percentage(channels[1], "hsv")?;
    let v = parse_percentage(channels[2], "hsv")?;
    let a = match alpha_opt {
        Some(a_str) => parse_alpha_f64(a_str, "hsv")?,
        None => 1.0,
    };
    let rgba = ColorHsv { h, s, v, a }.to_rgba();
    Ok((ColorFormat::Hsv, rgba))
}

fn parse_hue_degrees(token: &str, format: &'static str) -> Result<f64, ColorPreviewError> {
    let lower = token.to_ascii_lowercase();
    let (num_str, multiplier) = if let Some(s) = lower.strip_suffix("deg") {
        (s.trim(), 1.0)
    } else if let Some(s) = lower.strip_suffix("turn") {
        (s.trim(), 360.0)
    } else if let Some(s) = lower.strip_suffix("grad") {
        (s.trim(), 0.9)
    } else if let Some(s) = lower.strip_suffix("rad") {
        (s.trim(), 180.0 / std::f64::consts::PI)
    } else {
        (lower.as_str(), 1.0)
    };

    let val: f64 = num_str
        .parse()
        .map_err(|_| ColorPreviewError::InvalidComponent {
            format,
            detail: format!("invalid hue '{token}'"),
        })?;
    if !val.is_finite() {
        return Err(ColorPreviewError::InvalidComponent {
            format,
            detail: format!("non-finite hue '{token}'"),
        });
    }
    Ok(normalize_hue(val * multiplier))
}

fn parse_percentage(token: &str, format: &'static str) -> Result<f64, ColorPreviewError> {
    let num_str = token
        .strip_suffix('%')
        .ok_or_else(|| ColorPreviewError::InvalidComponent {
            format,
            detail: format!("expected percentage for '{token}'"),
        })?
        .trim();
    let v: f64 = num_str
        .parse()
        .map_err(|_| ColorPreviewError::InvalidComponent {
            format,
            detail: format!("invalid percentage '{token}'"),
        })?;
    if !v.is_finite() || !(0.0..=100.0).contains(&v) {
        return Err(ColorPreviewError::InvalidComponent {
            format,
            detail: format!("percentage out of 0..=100 range: '{token}'"),
        });
    }
    Ok(v)
}

fn parse_alpha_f64(token: &str, format: &'static str) -> Result<f64, ColorPreviewError> {
    if let Some(pct) = token.strip_suffix('%') {
        let v: f64 = pct
            .trim()
            .parse()
            .map_err(|_| ColorPreviewError::InvalidComponent {
                format,
                detail: format!("invalid alpha percentage '{token}'"),
            })?;
        if !v.is_finite() || !(0.0..=100.0).contains(&v) {
            return Err(ColorPreviewError::InvalidComponent {
                format,
                detail: format!("alpha percentage out of range: '{token}'"),
            });
        }
        return Ok(v / 100.0);
    }

    let v: f64 = token
        .parse()
        .map_err(|_| ColorPreviewError::InvalidComponent {
            format,
            detail: format!("invalid alpha '{token}'"),
        })?;
    if !v.is_finite() || !(0.0..=1.0).contains(&v) {
        return Err(ColorPreviewError::InvalidComponent {
            format,
            detail: format!("alpha out of 0..=1 range: '{token}'"),
        });
    }
    Ok(v)
}

fn parse_alpha_u8(token: &str, format: &'static str) -> Result<u8, ColorPreviewError> {
    let a = parse_alpha_f64(token, format)?;
    Ok((a * 255.0).round().clamp(0.0, 255.0) as u8)
}

fn normalize_hue(deg: f64) -> f64 {
    let mut h = deg % 360.0;
    if h < 0.0 {
        h += 360.0;
    }
    if (h - 360.0).abs() < 1e-9 {
        0.0
    } else {
        h
    }
}

fn format_alpha(a: f64) -> String {
    let rounded = (a * 1000.0).round() / 1000.0;
    let s = format!("{rounded:.3}");
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

fn round_2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn round_4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}

/// Table of standard CSS named colors (lowercase).
const CSS_NAMED_COLORS: &[(&str, ColorRgba)] = &[
    ("aliceblue", ColorRgba::rgb(240, 248, 255)),
    ("aqua", ColorRgba::rgb(0, 255, 255)),
    ("aquamarine", ColorRgba::rgb(127, 255, 212)),
    ("azure", ColorRgba::rgb(240, 255, 255)),
    ("beige", ColorRgba::rgb(245, 245, 220)),
    ("black", ColorRgba::rgb(0, 0, 0)),
    ("blue", ColorRgba::rgb(0, 0, 255)),
    ("blueviolet", ColorRgba::rgb(138, 43, 226)),
    ("brown", ColorRgba::rgb(165, 42, 42)),
    ("cadetblue", ColorRgba::rgb(95, 158, 160)),
    ("chartreuse", ColorRgba::rgb(127, 255, 0)),
    ("chocolate", ColorRgba::rgb(210, 105, 30)),
    ("coral", ColorRgba::rgb(255, 127, 80)),
    ("cornflowerblue", ColorRgba::rgb(100, 149, 237)),
    ("crimson", ColorRgba::rgb(220, 20, 60)),
    ("cyan", ColorRgba::rgb(0, 255, 255)),
    ("darkblue", ColorRgba::rgb(0, 0, 139)),
    ("darkcyan", ColorRgba::rgb(0, 139, 139)),
    ("darkgray", ColorRgba::rgb(169, 169, 169)),
    ("darkgreen", ColorRgba::rgb(0, 100, 0)),
    ("darkred", ColorRgba::rgb(139, 0, 0)),
    ("deeppink", ColorRgba::rgb(255, 20, 147)),
    ("deepskyblue", ColorRgba::rgb(0, 191, 255)),
    ("dodgerblue", ColorRgba::rgb(30, 144, 255)),
    ("firebrick", ColorRgba::rgb(178, 34, 34)),
    ("forestgreen", ColorRgba::rgb(34, 139, 34)),
    ("fuchsia", ColorRgba::rgb(255, 0, 255)),
    ("gold", ColorRgba::rgb(255, 215, 0)),
    ("goldenrod", ColorRgba::rgb(218, 165, 32)),
    ("gray", ColorRgba::rgb(128, 128, 128)),
    ("green", ColorRgba::rgb(0, 128, 0)),
    ("greenyellow", ColorRgba::rgb(173, 255, 47)),
    ("grey", ColorRgba::rgb(128, 128, 128)),
    ("hotpink", ColorRgba::rgb(255, 105, 180)),
    ("indianred", ColorRgba::rgb(205, 92, 92)),
    ("indigo", ColorRgba::rgb(75, 0, 130)),
    ("ivory", ColorRgba::rgb(255, 255, 240)),
    ("khaki", ColorRgba::rgb(240, 230, 140)),
    ("lavender", ColorRgba::rgb(230, 230, 250)),
    ("lightblue", ColorRgba::rgb(173, 216, 230)),
    ("lightgray", ColorRgba::rgb(211, 211, 211)),
    ("lightgreen", ColorRgba::rgb(144, 238, 144)),
    ("lime", ColorRgba::rgb(0, 255, 0)),
    ("limegreen", ColorRgba::rgb(50, 205, 50)),
    ("magenta", ColorRgba::rgb(255, 0, 255)),
    ("maroon", ColorRgba::rgb(128, 0, 0)),
    ("midnightblue", ColorRgba::rgb(25, 25, 112)),
    ("navy", ColorRgba::rgb(0, 0, 128)),
    ("olive", ColorRgba::rgb(128, 128, 0)),
    ("orange", ColorRgba::rgb(255, 165, 0)),
    ("orangered", ColorRgba::rgb(255, 69, 0)),
    ("orchid", ColorRgba::rgb(218, 112, 214)),
    ("pink", ColorRgba::rgb(255, 192, 203)),
    ("plum", ColorRgba::rgb(221, 160, 221)),
    ("purple", ColorRgba::rgb(128, 0, 128)),
    ("rebeccapurple", ColorRgba::rgb(102, 51, 153)),
    ("red", ColorRgba::rgb(255, 0, 0)),
    ("royalblue", ColorRgba::rgb(65, 105, 225)),
    ("salmon", ColorRgba::rgb(250, 128, 114)),
    ("seagreen", ColorRgba::rgb(46, 139, 87)),
    ("silver", ColorRgba::rgb(192, 192, 192)),
    ("skyblue", ColorRgba::rgb(135, 206, 235)),
    ("slateblue", ColorRgba::rgb(106, 90, 205)),
    ("slategray", ColorRgba::rgb(112, 128, 144)),
    ("snow", ColorRgba::rgb(255, 250, 250)),
    ("springgreen", ColorRgba::rgb(0, 255, 127)),
    ("steelblue", ColorRgba::rgb(70, 130, 180)),
    ("tan", ColorRgba::rgb(210, 180, 140)),
    ("teal", ColorRgba::rgb(0, 128, 128)),
    ("tomato", ColorRgba::rgb(255, 99, 71)),
    ("transparent", ColorRgba::rgba(0, 0, 0, 0)),
    ("turquoise", ColorRgba::rgb(64, 224, 208)),
    ("violet", ColorRgba::rgb(238, 130, 238)),
    ("wheat", ColorRgba::rgb(245, 222, 179)),
    ("white", ColorRgba::rgb(255, 255, 255)),
    ("yellow", ColorRgba::rgb(255, 255, 0)),
    ("yellowgreen", ColorRgba::rgb(154, 205, 50)),
];

/// Looks up a CSS named color (case-insensitive).
pub fn lookup_css_named_color(name: &str) -> Option<ColorRgba> {
    let lower = name.trim().to_ascii_lowercase();
    CSS_NAMED_COLORS
        .iter()
        .find(|(n, _)| *n == lower)
        .map(|(_, rgba)| *rgba)
}

/// Returns the first CSS named color matching `rgba`, if any.
pub fn css_name_for_rgba(rgba: ColorRgba) -> Option<&'static str> {
    CSS_NAMED_COLORS
        .iter()
        .find(|(_, c)| *c == rgba)
        .map(|(n, _)| *n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_prefix_parsing_happy_path() {
        let previewer = OmniboxColorPreviewer::with_defaults();

        let p6 = previewer.preview("#FF5733").unwrap();
        assert_eq!(p6.detected_format, ColorFormat::Hex);
        assert_eq!(p6.rgba, ColorRgba::rgb(255, 87, 51));
        assert_eq!(p6.swatch_hex, "#ff5733");
        assert_eq!(p6.swatch_hex_alpha, "#ff5733ff");

        let p3 = previewer.preview("color: #f53").unwrap();
        assert_eq!(p3.detected_format, ColorFormat::Hex);
        assert_eq!(p3.rgba, ColorRgba::rgb(255, 85, 51));

        let p8 = previewer.preview("= #33669980").unwrap();
        assert_eq!(p8.detected_format, ColorFormat::HexAlpha);
        assert_eq!(p8.rgba, ColorRgba::rgba(51, 102, 153, 128));
        assert!(p8
            .conversions
            .iter()
            .any(|c| c.format == ColorFormat::HexAlpha));
    }

    #[test]
    fn functional_rgb_hsl_hsv_and_named_colors() {
        let previewer = OmniboxColorPreviewer::with_defaults();

        let rgb_space = previewer.preview("rgb(255 0 0 / 50%)").unwrap();
        assert_eq!(rgb_space.detected_format, ColorFormat::Rgba);
        assert_eq!(rgb_space.rgba, ColorRgba::rgba(255, 0, 0, 128));

        let hsl = previewer.preview("hsl(0.5turn, 100%, 50%)").unwrap();
        assert_eq!(hsl.detected_format, ColorFormat::Hsl);
        assert_eq!(hsl.rgba, ColorRgba::rgb(0, 255, 255));
        assert_eq!(hsl.matched_name.as_deref(), Some("aqua"));

        let hsv = previewer.preview("hsv(120deg 100% 100%)").unwrap();
        assert_eq!(hsv.detected_format, ColorFormat::Hsv);
        assert_eq!(hsv.rgba, ColorRgba::rgb(0, 255, 0));
        assert_eq!(hsv.matched_name.as_deref(), Some("lime"));

        let named = previewer.preview("RebeccaPurple").unwrap();
        assert_eq!(named.detected_format, ColorFormat::Named);
        assert_eq!(named.rgba, ColorRgba::rgb(102, 51, 153));
        assert_eq!(
            named.contrast.recommended_text,
            ColorRgba::rgb(255, 255, 255)
        );
        assert!(named.contrast.wcag_aa_on_white);
    }

    #[test]
    fn validation_edge_cases_and_serde_roundtrip() {
        let previewer = OmniboxColorPreviewer::with_defaults();

        assert_eq!(previewer.preview("   "), Err(ColorPreviewError::EmptyInput));
        assert!(matches!(
            previewer.preview("#12345"),
            Err(ColorPreviewError::InvalidHex(_))
        ));
        assert!(matches!(
            previewer.preview("rgb(300, 0, 0)"),
            Err(ColorPreviewError::InvalidComponent { format: "rgb", .. })
        ));
        assert!(matches!(
            previewer.preview("notacolor"),
            Err(ColorPreviewError::UnrecognizedColor(_))
        ));

        let long = "a".repeat(DEFAULT_MAX_INPUT_LEN + 1);
        assert!(matches!(
            previewer.preview(&long),
            Err(ColorPreviewError::InputTooLong { .. })
        ));

        let bad_cfg = ColorPreviewConfig {
            max_input_len: 0,
            ..ColorPreviewConfig::default()
        };
        assert!(matches!(
            OmniboxColorPreviewer::new(bad_cfg),
            Err(ColorPreviewError::InvalidConfig(_))
        ));

        let preview = previewer.preview("#1e90ff").unwrap();
        let json = serde_json::to_string(&preview).unwrap();
        let decoded: OmniboxColorPreview = serde_json::from_str(&json).unwrap();
        assert_eq!(preview, decoded);
    }
}
