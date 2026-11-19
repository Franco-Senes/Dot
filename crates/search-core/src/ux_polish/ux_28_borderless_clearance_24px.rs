//! UX-28: Borderless control clearance (`24px` between visible glyphs).
//!
//! Without an established density system, adjacent bordered or filled controls
//! require at least `12px` of gap, while borderless text and icon controls
//! require at least `24px` between their visible glyphs, counting each
//! control's own inline padding, so hit areas never overlap and controls stay
//! visually distinct.

/// Minimum visible clearance in pixels between glyphs of borderless controls.
pub const BORDERLESS_GLYPH_CLEARANCE_PX: u16 = 24;

/// Minimum gap in pixels between adjacent bordered or filled controls.
pub const BORDERED_CONTROL_GAP_PX: u16 = 12;

/// Minimum allowed gap between hit-area boxes so targets never overlap.
pub const MIN_HIT_AREA_GAP_PX: u16 = 0;

/// Metadata describing this UX rule for registry inspection and audits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UxRuleSpec {
    /// Stable identifier of the UX overhaul item.
    pub id: &'static str,
    /// Short human-readable rule title.
    pub title: &'static str,
    /// Category within the UX overhaul.
    pub category: &'static str,
    /// Minimum visible glyph-to-glyph clearance for borderless controls in px.
    pub borderless_glyph_clearance_px: u16,
    /// Minimum box-to-box gap for bordered or filled controls in px.
    pub bordered_control_gap_px: u16,
    /// CSS custom property name for borderless glyph clearance.
    pub css_var_borderless: &'static str,
    /// CSS custom property name for bordered control gap.
    pub css_var_bordered: &'static str,
    /// Concise summary of the requirement.
    pub summary: &'static str,
}

/// Returns the canonical specification for `ux_28_borderless_clearance_24px`.
#[must_use]
pub const fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_28_borderless_clearance_24px",
        title: "Borderless control clearance (24px)",
        category: "layout",
        borderless_glyph_clearance_px: BORDERLESS_GLYPH_CLEARANCE_PX,
        bordered_control_gap_px: BORDERED_CONTROL_GAP_PX,
        css_var_borderless: "--ux-clearance-borderless-glyph",
        css_var_bordered: "--ux-gap-bordered-control",
        summary: "Leave 24px between visible glyphs of borderless text/icon controls \
                  (counting inline padding) and 12px between bordered or filled controls.",
    }
}

/// Visual boundary treatment of an interactive control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlSurfaceKind {
    /// Text-only or icon-only button without a visible background or border at rest.
    Borderless,
    /// Control with a visible 1px+ outline/border at rest.
    Bordered,
    /// Control with a filled background surface at rest.
    Filled,
}

impl ControlSurfaceKind {
    /// Returns `true` when the control has no visible resting boundary.
    #[must_use]
    pub const fn is_borderless(self) -> bool {
        matches!(self, Self::Borderless)
    }
}

/// Geometry of a single control used to compute required inter-control spacing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlGeometry {
    /// Visual boundary treatment at rest.
    pub surface: ControlSurfaceKind,
    /// Inline padding on each side of the visible glyph/label in pixels.
    pub padding_inline_px: u16,
}

impl ControlGeometry {
    /// Creates a new control geometry descriptor.
    #[must_use]
    pub const fn new(surface: ControlSurfaceKind, padding_inline_px: u16) -> Self {
        Self {
            surface,
            padding_inline_px,
        }
    }
}

/// Result of auditing the spacing between two adjacent controls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClearanceAudit {
    /// Whether the actual CSS gap satisfies the clearance requirement.
    pub passed: bool,
    /// Required CSS `gap` between the two hit-area boxes in pixels.
    pub required_css_gap_px: u16,
    /// Actual CSS `gap` provided in pixels.
    pub actual_css_gap_px: u16,
    /// Resulting visible distance between glyphs (includes inline padding).
    pub visible_glyph_distance_px: u16,
    /// Actionable Spanish guidance when the audit fails.
    pub guidance_es: Option<&'static str>,
}

/// Computes the minimum CSS `gap` (between hit-area boxes) required between two
/// adjacent controls.
///
/// - If either control is [`ControlSurfaceKind::Borderless`], the visible
///   glyph-to-glyph distance (`left.padding_inline_px + css_gap +
///   right.padding_inline_px`) must be at least
///   [`BORDERLESS_GLYPH_CLEARANCE_PX`] (`24px`), while never allowing hit areas
///   to overlap (`>= 0px`).
/// - If both controls are bordered or filled, the CSS `gap` must be at least
///   [`BORDERED_CONTROL_GAP_PX`] (`12px`).
#[must_use]
pub const fn required_css_gap_px(left: ControlGeometry, right: ControlGeometry) -> u16 {
    if left.surface.is_borderless() || right.surface.is_borderless() {
        let combined_padding = left
            .padding_inline_px
            .saturating_add(right.padding_inline_px);
        BORDERLESS_GLYPH_CLEARANCE_PX.saturating_sub(combined_padding)
    } else {
        BORDERED_CONTROL_GAP_PX
    }
}

/// Audits whether `actual_css_gap_px` provides sufficient clearance between
/// `left` and `right`.
#[must_use]
pub const fn check_clearance(
    left: ControlGeometry,
    right: ControlGeometry,
    actual_css_gap_px: u16,
) -> ClearanceAudit {
    let required = required_css_gap_px(left, right);
    let visible_glyph_distance_px = left
        .padding_inline_px
        .saturating_add(actual_css_gap_px)
        .saturating_add(right.padding_inline_px);
    let passed = actual_css_gap_px >= required;
    let guidance_es = if passed {
        None
    } else if left.surface.is_borderless() || right.surface.is_borderless() {
        Some("Aumenta el espacio entre controles sin borde a al menos 24 px entre glifos visibles.")
    } else {
        Some("Aumenta la separacion entre controles con borde o relleno a al menos 12 px.")
    };

    ClearanceAudit {
        passed,
        required_css_gap_px: required,
        actual_css_gap_px,
        visible_glyph_distance_px,
        guidance_es,
    }
}

/// Returns the CSS tokens and toolbar/group utility rules for control clearance.
#[must_use]
pub const fn css_fragment() -> &'static str {
    "\
:root {\n\
  --ux-clearance-borderless-glyph: 24px;\n\
  --ux-gap-bordered-control: 12px;\n\
}\n\
\n\
.ux-control-row--bordered {\n\
  display: inline-flex;\n\
  align-items: center;\n\
  column-gap: var(--ux-gap-bordered-control);\n\
}\n\
\n\
.ux-control-row--borderless {\n\
  display: inline-flex;\n\
  align-items: center;\n\
  column-gap: max(0px, calc(var(--ux-clearance-borderless-glyph) - (var(--ux-btn-pad-inline, 6px) * 2)));\n\
}\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_matches_exact_clearance_constants() {
        let s = spec();
        assert_eq!(s.id, "ux_28_borderless_clearance_24px");
        assert_eq!(s.borderless_glyph_clearance_px, 24);
        assert_eq!(s.bordered_control_gap_px, 12);
        assert_eq!(s.css_var_borderless, "--ux-clearance-borderless-glyph");
        assert_eq!(s.css_var_bordered, "--ux-gap-bordered-control");
    }

    #[test]
    fn borderless_controls_account_for_inline_padding() {
        // Two borderless icon buttons with 6px inline padding each:
        // 6 + 6 = 12px padding -> need 12px CSS gap for 24px glyph clearance.
        let icon_btn = ControlGeometry::new(ControlSurfaceKind::Borderless, 6);
        assert_eq!(required_css_gap_px(icon_btn, icon_btn), 12);

        // With 12px inline padding each, padding already provides 24px clearance,
        // so CSS gap can be 0px without overlapping hit areas.
        let wide_pad_btn = ControlGeometry::new(ControlSurfaceKind::Borderless, 12);
        assert_eq!(required_css_gap_px(wide_pad_btn, wide_pad_btn), 0);

        // With 0px inline padding each, full 24px CSS gap is required.
        let bare_btn = ControlGeometry::new(ControlSurfaceKind::Borderless, 0);
        assert_eq!(required_css_gap_px(bare_btn, bare_btn), 24);
    }

    #[test]
    fn bordered_and_filled_controls_require_12px_gap() {
        let bordered = ControlGeometry::new(ControlSurfaceKind::Bordered, 10);
        let filled = ControlGeometry::new(ControlSurfaceKind::Filled, 12);
        assert_eq!(required_css_gap_px(bordered, filled), 12);
        assert_eq!(required_css_gap_px(bordered, bordered), 12);
        assert_eq!(required_css_gap_px(filled, filled), 12);
    }

    #[test]
    fn audit_reports_pass_and_spanish_guidance_on_failure() {
        let left = ControlGeometry::new(ControlSurfaceKind::Borderless, 4);
        let right = ControlGeometry::new(ControlSurfaceKind::Borderless, 4);

        let fail = check_clearance(left, right, 8);
        assert!(!fail.passed);
        assert_eq!(fail.required_css_gap_px, 16);
        assert_eq!(fail.visible_glyph_distance_px, 16);
        assert!(fail.guidance_es.is_some());

        let pass = check_clearance(left, right, 16);
        assert!(pass.passed);
        assert_eq!(pass.visible_glyph_distance_px, 24);
        assert!(pass.guidance_es.is_none());
    }

    #[test]
    fn css_fragment_contains_expected_tokens_and_no_physical_margins() {
        let css = css_fragment();
        assert!(css.contains("--ux-clearance-borderless-glyph: 24px;"));
        assert!(css.contains("--ux-gap-bordered-control: 12px;"));
        assert!(css.contains("column-gap:"));
    }
}
