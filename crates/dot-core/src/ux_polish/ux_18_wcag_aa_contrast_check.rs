//! UX-18: WCAG 2.2 AA contrast verification for rendered color pairs.
//!
//! Measures rendered foreground-against-background pairs in sRGB and OKLCH
//! against WCAG 2.2 AA thresholds: `4.5:1` for normal text, `3.0:1` for
//! large text (`>= 24px` regular or `>= 18.67px` bold), and `3.0:1` for
//! non-text UI components (input boundaries, icons, and focus indicators).
//! Semi-transparent foregrounds are composited over their actual background
//! before measuring.

/// WCAG 2.2 AA minimum contrast ratio for normal-sized text.
pub const WCAG_AA_NORMAL_TEXT_RATIO: f64 = 4.5;

/// WCAG 2.2 AA minimum contrast ratio for large text.
pub const WCAG_AA_LARGE_TEXT_RATIO: f64 = 3.0;

/// WCAG 2.2 AA minimum contrast ratio for non-text UI components and graphics.
pub const WCAG_AA_UI_COMPONENT_RATIO: f64 = 3.0;

/// Minimum font size in CSS pixels for regular-weight large text (18pt).
pub const WCAG_AA_LARGE_TEXT_MIN_PX: f32 = 24.0;

/// Minimum font size in CSS pixels for bold (>= 700) large text (14pt).
pub const WCAG_AA_LARGE_BOLD_MIN_PX: f32 = 18.67;

/// Minimum font weight considered bold for WCAG large-text classification.
pub const WCAG_BOLD_FONT_WEIGHT: u16 = 700;

/// Specification metadata for this UX polish rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub summary: &'static str,
    pub css_formula: &'static str,
}

/// Returns the canonical specification for `ux_18_wcag_aa_contrast_check`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_18_wcag_aa_contrast_check",
        title: "WCAG 2.2 AA contrast verification",
        category: "colors",
        summary: "Measures rendered foreground-background pairs in sRGB and OKLCH against WCAG AA thresholds (4.5:1 normal text, 3.0:1 large text and UI components).",
        css_formula: "(L1 + 0.05) / (L2 + 0.05) >= 4.5 (normal) | 3.0 (large/UI)",
    }
}

/// Target element category determining the required WCAG AA contrast ratio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContrastTarget {
    /// Body text, labels, and captions smaller than large-text thresholds.
    NormalText,
    /// Regular text >= 24px (18pt) or bold (>= 700) text >= 18.67px (14pt).
    LargeText,
    /// Non-text UI boundaries, form input borders, icons, and focus rings.
    UiComponent,
}

impl ContrastTarget {
    /// Returns the minimum WCAG 2.2 AA contrast ratio for this target.
    pub const fn min_ratio(self) -> f64 {
        match self {
            Self::NormalText => WCAG_AA_NORMAL_TEXT_RATIO,
            Self::LargeText => WCAG_AA_LARGE_TEXT_RATIO,
            Self::UiComponent => WCAG_AA_UI_COMPONENT_RATIO,
        }
    }

    /// Classifies text into `LargeText` or `NormalText` from its CSS pixel size
    /// and numeric font weight.
    pub fn for_text(font_size_px: f32, font_weight: u16) -> Self {
        if font_size_px >= WCAG_AA_LARGE_TEXT_MIN_PX
            || (font_weight >= WCAG_BOLD_FONT_WEIGHT && font_size_px >= WCAG_AA_LARGE_BOLD_MIN_PX)
        {
            Self::LargeText
        } else {
            Self::NormalText
        }
    }
}

/// An sRGB color with normalized channels in `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SrgbColor {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

impl SrgbColor {
    /// Creates an `SrgbColor` from normalized channels clamped to `0.0..=1.0`.
    pub fn new(r: f64, g: f64, b: f64) -> Self {
        Self {
            r: r.clamp(0.0, 1.0),
            g: g.clamp(0.0, 1.0),
            b: b.clamp(0.0, 1.0),
        }
    }

    /// Creates an `SrgbColor` from 8-bit `0..=255` channel values.
    pub fn from_u8(r: u8, g: u8, b: u8) -> Self {
        Self {
            r: f64::from(r) / 255.0,
            g: f64::from(g) / 255.0,
            b: f64::from(b) / 255.0,
        }
    }

    /// Parses a `#RRGGBB` or `RRGGBB` hex string into an `SrgbColor`.
    pub fn from_hex(hex: &str) -> Option<Self> {
        let trimmed = hex.trim().strip_prefix('#').unwrap_or(hex.trim());
        if trimmed.len() != 6 || !trimmed.is_ascii() {
            return None;
        }
        let r = u8::from_str_radix(trimmed.get(0..2)?, 16).ok()?;
        let g = u8::from_str_radix(trimmed.get(2..4)?, 16).ok()?;
        let b = u8::from_str_radix(trimmed.get(4..6)?, 16).ok()?;
        Some(Self::from_u8(r, g, b))
    }

    /// Composites this foreground color with `alpha` (`0.0..=1.0`) over an
    /// opaque background color in sRGB space.
    pub fn composite_over(self, alpha: f64, background: Self) -> Self {
        let a = alpha.clamp(0.0, 1.0);
        let inv = 1.0 - a;
        Self::new(
            self.r * a + background.r * inv,
            self.g * a + background.g * inv,
            self.b * a + background.b * inv,
        )
    }

    /// Converts this sRGB color to OKLCH (`L` in `0.0..=1.0`, `C >= 0.0`, `H` in `0.0..360.0`).
    pub fn to_oklch(self) -> OklchColor {
        let r = srgb_to_linear(self.r);
        let g = srgb_to_linear(self.g);
        let b = srgb_to_linear(self.b);

        let l_lms = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
        let m_lms = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
        let s_lms = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();

        let l = 0.2104542553 * l_lms + 0.7936177850 * m_lms - 0.0040720468 * s_lms;
        let a = 1.9779984951 * l_lms - 2.4285922050 * m_lms + 0.4505937099 * s_lms;
        let b_ok = 0.0259040371 * l_lms + 0.7827717662 * m_lms - 0.8086757660 * s_lms;

        let c = (a * a + b_ok * b_ok).sqrt();
        let h = if c < 1e-7 {
            0.0
        } else {
            b_ok.atan2(a).to_degrees().rem_euclid(360.0)
        };

        OklchColor { l, c, h }
    }
}

/// An OKLCH color with perceived lightness `l` (`0.0..=1.0`), chroma `c`, and
/// hue angle `h` in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OklchColor {
    pub l: f64,
    pub c: f64,
    pub h: f64,
}

impl OklchColor {
    /// Creates an `OklchColor` with lightness clamped to `0.0..=1.0` and
    /// non-negative chroma.
    pub fn new(l: f64, c: f64, h: f64) -> Self {
        Self {
            l: l.clamp(0.0, 1.0),
            c: c.max(0.0),
            h: h.rem_euclid(360.0),
        }
    }

    /// Converts this OKLCH color to linear sRGB channels before gamut clamping.
    fn to_srgb_unclamped(self) -> (f64, f64, f64) {
        let h_rad = self.h.to_radians();
        let a = self.c * h_rad.cos();
        let b_ok = self.c * h_rad.sin();

        let l_ = self.l + 0.3963377774 * a + 0.2158037573 * b_ok;
        let m_ = self.l - 0.1055613458 * a - 0.0638541728 * b_ok;
        let s_ = self.l - 0.0894841775 * a - 1.2914855480 * b_ok;

        let l_lms = l_ * l_ * l_;
        let m_lms = m_ * m_ * m_;
        let s_lms = s_ * s_ * s_;

        let r_lin = 4.0767416621 * l_lms - 3.3077115913 * m_lms + 0.2309699292 * s_lms;
        let g_lin = -1.2684380046 * l_lms + 2.6097574011 * m_lms - 0.3413193965 * s_lms;
        let b_lin = -0.0041960863 * l_lms - 0.7034186147 * m_lms + 1.7076147010 * s_lms;

        (
            linear_to_srgb(r_lin),
            linear_to_srgb(g_lin),
            linear_to_srgb(b_lin),
        )
    }

    /// Returns `true` if this OKLCH color fits inside the sRGB gamut without
    /// channel clipping.
    pub fn is_in_srgb_gamut(self) -> bool {
        let (r, g, b) = self.to_srgb_unclamped();
        const EPS: f64 = 1e-4;
        (-EPS..=1.0 + EPS).contains(&r)
            && (-EPS..=1.0 + EPS).contains(&g)
            && (-EPS..=1.0 + EPS).contains(&b)
    }

    /// Converts this OKLCH color to `SrgbColor`, clamping out-of-gamut channels
    /// to `0.0..=1.0`.
    pub fn to_srgb(self) -> SrgbColor {
        let (r, g, b) = self.to_srgb_unclamped();
        SrgbColor::new(r, g, b)
    }
}

/// Outcome of evaluating a foreground-background pair against a WCAG AA target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContrastCheckResult {
    pub ratio: f64,
    pub required_ratio: f64,
    pub passed: bool,
    pub shortfall: f64,
    pub target: ContrastTarget,
}

/// Converts a single sRGB channel (`0.0..=1.0`) to linear light.
pub fn srgb_to_linear(channel: f64) -> f64 {
    let c = channel.clamp(0.0, 1.0);
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Converts a single linear-light channel to sRGB (`0.0..=1.0` when in gamut).
pub fn linear_to_srgb(channel: f64) -> f64 {
    if channel <= 0.0031308 {
        12.92 * channel
    } else {
        1.055 * channel.powf(1.0 / 2.4) - 0.055
    }
}

/// Computes the WCAG 2.x relative luminance (`0.0..=1.0`) of an `SrgbColor`.
pub fn relative_luminance(color: SrgbColor) -> f64 {
    0.2126 * srgb_to_linear(color.r)
        + 0.7152 * srgb_to_linear(color.g)
        + 0.0722 * srgb_to_linear(color.b)
}

/// Computes the WCAG 2.x contrast ratio (`1.0..=21.0`) between two opaque sRGB colors.
pub fn contrast_ratio(fg: SrgbColor, bg: SrgbColor) -> f64 {
    let l1 = relative_luminance(fg);
    let l2 = relative_luminance(bg);
    let (lighter, darker) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
    (lighter + 0.05) / (darker + 0.05)
}

/// Checks an opaque sRGB foreground-background pair against a WCAG AA target.
pub fn check_contrast(fg: SrgbColor, bg: SrgbColor, target: ContrastTarget) -> ContrastCheckResult {
    let ratio = contrast_ratio(fg, bg);
    let required_ratio = target.min_ratio();
    let passed = ratio >= required_ratio;
    let shortfall = if passed { 0.0 } else { required_ratio - ratio };
    ContrastCheckResult {
        ratio,
        required_ratio,
        passed,
        shortfall,
        target,
    }
}

/// Checks a semi-transparent sRGB foreground composited over an opaque sRGB
/// background against a WCAG AA target.
pub fn check_rendered_pair(
    fg: SrgbColor,
    fg_alpha: f64,
    bg: SrgbColor,
    target: ContrastTarget,
) -> ContrastCheckResult {
    let rendered_fg = fg.composite_over(fg_alpha, bg);
    check_contrast(rendered_fg, bg, target)
}

/// Checks an OKLCH foreground-background pair against a WCAG AA target.
pub fn check_oklch_contrast(
    fg: OklchColor,
    bg: OklchColor,
    target: ContrastTarget,
) -> ContrastCheckResult {
    check_contrast(fg.to_srgb(), bg.to_srgb(), target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_metadata_matches_task() {
        let rule = spec();
        assert_eq!(rule.id, "ux_18_wcag_aa_contrast_check");
        assert_eq!(rule.category, "colors");
        assert!(rule.css_formula.contains("4.5"));
        assert!(rule.css_formula.contains("3.0"));
    }

    #[test]
    fn black_on_white_is_21_and_same_color_is_1() {
        let black = SrgbColor::from_u8(0, 0, 0);
        let white = SrgbColor::from_u8(255, 255, 255);

        let max_ratio = contrast_ratio(black, white);
        assert!((max_ratio - 21.0).abs() < 1e-6);

        let min_ratio = contrast_ratio(white, white);
        assert!((min_ratio - 1.0).abs() < 1e-6);
    }

    #[test]
    fn wcag_aa_thresholds_distinguish_normal_large_and_ui() {
        // #767676 on #FFFFFF has contrast ~4.54:1 (passes NormalText 4.5:1)
        let aa_pass_gray = SrgbColor::from_hex("#767676").expect("valid hex");
        // #777777 on #FFFFFF has contrast ~4.48:1 (fails NormalText 4.5:1, passes LargeText/UiComponent 3.0:1)
        let aa_borderline_gray = SrgbColor::from_hex("#777777").expect("valid hex");
        let white = SrgbColor::from_hex("#FFFFFF").expect("valid hex");

        let pass_normal = check_contrast(aa_pass_gray, white, ContrastTarget::NormalText);
        assert!(pass_normal.passed);
        assert_eq!(pass_normal.shortfall, 0.0);

        let fail_normal = check_contrast(aa_borderline_gray, white, ContrastTarget::NormalText);
        assert!(!fail_normal.passed);
        assert!(fail_normal.ratio > 4.4 && fail_normal.ratio < 4.5);
        assert!(fail_normal.shortfall > 0.0);

        let pass_large = check_contrast(aa_borderline_gray, white, ContrastTarget::LargeText);
        assert!(pass_large.passed);

        let pass_ui = check_contrast(aa_borderline_gray, white, ContrastTarget::UiComponent);
        assert!(pass_ui.passed);
    }

    #[test]
    fn alpha_compositing_measures_rendered_pair() {
        let black = SrgbColor::from_u8(0, 0, 0);
        let white = SrgbColor::from_u8(255, 255, 255);

        // 50% black on white renders as mid-gray (~3.95:1), which fails 4.5:1 normal text
        // even though opaque black is 21:1.
        let half_alpha = check_rendered_pair(black, 0.5, white, ContrastTarget::NormalText);
        assert!(!half_alpha.passed);
        assert!(half_alpha.ratio < WCAG_AA_NORMAL_TEXT_RATIO);

        // 75% black on white passes 4.5:1.
        let three_quarter = check_rendered_pair(black, 0.75, white, ContrastTarget::NormalText);
        assert!(three_quarter.passed);
    }

    #[test]
    fn oklch_conversion_roundtrip_and_contrast() {
        let fg = OklchColor::new(0.25, 0.02, 250.0);
        let bg = OklchColor::new(0.98, 0.005, 250.0);
        assert!(fg.is_in_srgb_gamut());
        assert!(bg.is_in_srgb_gamut());

        let res = check_oklch_contrast(fg, bg, ContrastTarget::NormalText);
        assert!(res.passed);
        assert!(res.ratio >= WCAG_AA_NORMAL_TEXT_RATIO);

        let srgb = fg.to_srgb();
        let back = srgb.to_oklch();
        assert!((back.l - fg.l).abs() < 1e-3);
        assert!((back.c - fg.c).abs() < 1e-3);
    }

    #[test]
    fn text_classification_handles_size_and_bold_weight() {
        assert_eq!(
            ContrastTarget::for_text(16.0, 400),
            ContrastTarget::NormalText
        );
        assert_eq!(
            ContrastTarget::for_text(18.67, 400),
            ContrastTarget::NormalText
        );
        assert_eq!(
            ContrastTarget::for_text(18.67, 700),
            ContrastTarget::LargeText
        );
        assert_eq!(
            ContrastTarget::for_text(24.0, 400),
            ContrastTarget::LargeText
        );
    }

    #[test]
    fn invalid_hex_returns_none() {
        assert!(SrgbColor::from_hex("#GGG000").is_none());
        assert!(SrgbColor::from_hex("#123").is_none());
        assert!(SrgbColor::from_hex("").is_none());
    }
}
