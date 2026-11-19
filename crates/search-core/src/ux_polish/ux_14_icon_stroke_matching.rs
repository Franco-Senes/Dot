//! UX-14: Icon stroke matching to adjacent text weight.
//!
//! Ensures icon stroke widths track the weight of adjacent typography
//! (`1.5px` at 400, `2px` at 500-600, `2.5px` at 700 or emphasized standalone),
//! preserve exact rendered pixel width via `vector-effect: non-scaling-stroke`
//! or grid-ratio compensation, size inline icons between `1em` and `1.25em`,
//! and draw with `currentColor` rather than hardcoded hex values.

/// Canonical rule specification for UX-14.
#[derive(Debug, Clone, PartialEq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub summary: &'static str,
    pub regular_stroke_px: f32,
    pub medium_semibold_stroke_px: f32,
    pub bold_stroke_px: f32,
    pub inline_size_min_em: f32,
    pub inline_size_max_em: f32,
    pub vector_effect: &'static str,
    pub color_token: &'static str,
}

/// Returns the canonical UX specification for icon stroke matching.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_14_icon_stroke_matching",
        title: "Match icon stroke to text weight",
        category: "better-ui/icons",
        summary: "Scale icon stroke width to match adjacent text weight (1.5px/2px/2.5px) with non-scaling-stroke and currentColor.",
        regular_stroke_px: STROKE_REGULAR_PX,
        medium_semibold_stroke_px: STROKE_MEDIUM_SEMIBOLD_PX,
        bold_stroke_px: STROKE_BOLD_PX,
        inline_size_min_em: INLINE_ICON_MIN_EM,
        inline_size_max_em: INLINE_ICON_MAX_EM,
        vector_effect: "non-scaling-stroke",
        color_token: "currentColor",
    }
}

/// Rendered stroke width for regular (400) adjacent text, 14-16px.
pub const STROKE_REGULAR_PX: f32 = 1.5;

/// Rendered stroke width for medium or semibold (500-600) adjacent text.
pub const STROKE_MEDIUM_SEMIBOLD_PX: f32 = 2.0;

/// Rendered stroke width for bold (700) text or an emphasized standalone icon.
pub const STROKE_BOLD_PX: f32 = 2.5;

/// Minimum recommended inline icon size relative to adjacent font size.
pub const INLINE_ICON_MIN_EM: f32 = 1.0;

/// Maximum recommended inline icon size relative to adjacent font size.
pub const INLINE_ICON_MAX_EM: f32 = 1.25;

/// Default SVG icon viewBox grid size in pixels.
pub const DEFAULT_GRID_SIZE_PX: f32 = 24.0;

/// Typical small inline icon render size in pixels.
pub const SMALL_RENDER_SIZE_PX: f32 = 16.0;

/// Typography weight tier paired with an icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextWeightTier {
    /// Font weight <= 450 (e.g., 400 regular).
    Regular,
    /// Font weight 451..=650 (e.g., 500 medium, 600 semibold).
    MediumOrSemibold,
    /// Font weight > 650 (e.g., 700 bold) or emphasized standalone icon.
    BoldOrEmphasized,
}

impl TextWeightTier {
    /// Classifies a numeric CSS font weight (100-900) into a stroke tier.
    pub fn from_font_weight(weight: u16, standalone_emphasized: bool) -> Self {
        if standalone_emphasized || weight >= 651 {
            Self::BoldOrEmphasized
        } else if weight >= 451 {
            Self::MediumOrSemibold
        } else {
            Self::Regular
        }
    }

    /// Returns the target rendered stroke width in CSS pixels.
    pub const fn rendered_stroke_px(self) -> f32 {
        match self {
            Self::Regular => STROKE_REGULAR_PX,
            Self::MediumOrSemibold => STROKE_MEDIUM_SEMIBOLD_PX,
            Self::BoldOrEmphasized => STROKE_BOLD_PX,
        }
    }

    /// Returns a clean CSS suffix for the tier.
    pub const fn css_modifier(self) -> &'static str {
        match self {
            Self::Regular => "regular",
            Self::MediumOrSemibold => "semibold",
            Self::BoldOrEmphasized => "bold",
        }
    }
}

/// Configuration for rendering an SVG icon beside text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IconStrokeConfig {
    pub font_weight: u16,
    pub standalone_emphasized: bool,
    pub grid_size_px: f32,
    pub render_size_px: f32,
    pub use_non_scaling_stroke: bool,
}

impl IconStrokeConfig {
    /// Creates a standard configuration using `vector-effect: non-scaling-stroke`.
    pub const fn new(font_weight: u16, standalone_emphasized: bool) -> Self {
        Self {
            font_weight,
            standalone_emphasized,
            grid_size_px: DEFAULT_GRID_SIZE_PX,
            render_size_px: SMALL_RENDER_SIZE_PX,
            use_non_scaling_stroke: true,
        }
    }

    /// Returns the resolved weight tier.
    pub fn tier(&self) -> TextWeightTier {
        TextWeightTier::from_font_weight(self.font_weight, self.standalone_emphasized)
    }

    /// Returns the desired rendered stroke width in pixels.
    pub fn target_rendered_stroke_px(&self) -> f32 {
        self.tier().rendered_stroke_px()
    }

    /// Computes the `stroke-width` attribute value to place on the SVG.
    ///
    /// When `use_non_scaling_stroke` is true, the browser paints the exact
    /// target pixel width regardless of viewBox scaling. When false, the
    /// stroke is pre-compensated by `grid_size_px / render_size_px` so a
    /// 24px-grid icon drawn at 16px does not thin out to ~1.33px.
    pub fn svg_stroke_width(&self) -> f32 {
        let target = self.target_rendered_stroke_px();
        if self.use_non_scaling_stroke || self.render_size_px <= 0.0 || self.grid_size_px <= 0.0 {
            target
        } else {
            target * (self.grid_size_px / self.render_size_px)
        }
    }
}

/// Returns the clamped inline icon size in `em` within `[1.0, 1.25]`.
pub fn clamp_inline_icon_em(requested_em: f32) -> f32 {
    if requested_em.is_nan() {
        return INLINE_ICON_MIN_EM;
    }
    requested_em.clamp(INLINE_ICON_MIN_EM, INLINE_ICON_MAX_EM)
}

/// Checks whether an SVG fragment uses `currentColor` instead of hardcoded hex colors.
pub fn uses_current_color_only(svg_snippet: &str) -> bool {
    let lower = svg_snippet.to_ascii_lowercase();
    let has_hardcoded_stroke = lower.contains("stroke=\"#") || lower.contains("stroke='#");
    let has_hardcoded_fill = lower.contains("fill=\"#") || lower.contains("fill='#");
    !has_hardcoded_stroke && !has_hardcoded_fill
}

/// Generates CSS utility rules for weight-matched icon strokes and `currentColor`.
pub fn css_rules() -> &'static str {
    concat!(
        ".sl-icon {\n",
        "  display: inline-block;\n",
        "  width: 1.125em;\n",
        "  height: 1.125em;\n",
        "  flex-shrink: 0;\n",
        "  fill: none;\n",
        "  stroke: currentColor;\n",
        "  stroke-linecap: round;\n",
        "  stroke-linejoin: round;\n",
        "  stroke-width: 1.5px;\n",
        "}\n",
        ".sl-icon > * {\n",
        "  vector-effect: non-scaling-stroke;\n",
        "}\n",
        ".sl-icon--regular {\n",
        "  stroke-width: 1.5px;\n",
        "}\n",
        ".sl-icon--semibold {\n",
        "  stroke-width: 2px;\n",
        "}\n",
        ".sl-icon--bold {\n",
        "  stroke-width: 2.5px;\n",
        "}\n",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_matches_exact_stroke_and_size_values() {
        let s = spec();
        assert_eq!(s.id, "ux_14_icon_stroke_matching");
        assert!((s.regular_stroke_px - 1.5).abs() < f32::EPSILON);
        assert!((s.medium_semibold_stroke_px - 2.0).abs() < f32::EPSILON);
        assert!((s.bold_stroke_px - 2.5).abs() < f32::EPSILON);
        assert!((s.inline_size_min_em - 1.0).abs() < f32::EPSILON);
        assert!((s.inline_size_max_em - 1.25).abs() < f32::EPSILON);
        assert_eq!(s.vector_effect, "non-scaling-stroke");
        assert_eq!(s.color_token, "currentColor");
    }

    #[test]
    fn stroke_matches_text_weight_tiers() {
        assert_eq!(
            TextWeightTier::from_font_weight(400, false).rendered_stroke_px(),
            1.5
        );
        assert_eq!(
            TextWeightTier::from_font_weight(500, false).rendered_stroke_px(),
            2.0
        );
        assert_eq!(
            TextWeightTier::from_font_weight(600, false).rendered_stroke_px(),
            2.0
        );
        assert_eq!(
            TextWeightTier::from_font_weight(700, false).rendered_stroke_px(),
            2.5
        );
        assert_eq!(
            TextWeightTier::from_font_weight(400, true).rendered_stroke_px(),
            2.5
        );
    }

    #[test]
    fn grid_compensation_and_non_scaling_stroke() {
        let non_scaling = IconStrokeConfig::new(600, false);
        assert!((non_scaling.svg_stroke_width() - 2.0).abs() < f32::EPSILON);

        let scaled = IconStrokeConfig {
            font_weight: 600,
            standalone_emphasized: false,
            grid_size_px: 24.0,
            render_size_px: 16.0,
            use_non_scaling_stroke: false,
        };
        assert!((scaled.svg_stroke_width() - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn current_color_and_inline_clamp_checks() {
        assert!(uses_current_color_only(
            r#"<svg fill="none" stroke="currentColor" stroke-width="2"></svg>"#
        ));
        assert!(!uses_current_color_only(
            r##"<svg fill="#666" stroke="currentColor"></svg>"##
        ));
        assert!((clamp_inline_icon_em(0.8) - 1.0).abs() < f32::EPSILON);
        assert!((clamp_inline_icon_em(1.5) - 1.25).abs() < f32::EPSILON);
        assert!((clamp_inline_icon_em(f32::NAN) - 1.0).abs() < f32::EPSILON);
        assert!(css_rules().contains("vector-effect: non-scaling-stroke;"));
        assert!(css_rules().contains("stroke: currentColor;"));
    }
}
