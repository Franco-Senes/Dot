//! UX-03: Layered transparent shadows for elevation instead of decorative borders.
//!
//! Replaces depth-only borders with layered transparent `box-shadow` tokens
//! while preserving `border: 1px solid transparent` so `forced-colors` mode
//! still draws a visible edge and keeps box dimensions identical.

/// Specification metadata for this UX polish rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub category: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub forced_colors_border: &'static str,
    pub transition_property: &'static str,
    pub transition_duration_ms: u16,
    pub transition_timing: &'static str,
}

/// Color theme mode for shadow token selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
}

/// Surface role determining whether elevation shadows or structural borders apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceRole {
    /// Card, panel, or popover whose border existed only to create depth.
    ElevationSurface,
    /// Divider, separator, or table cell boundary.
    StructuralDivider,
    /// Selected item indicator.
    SelectedState,
    /// Form input boundary requiring 3:1 non-text contrast.
    FormInput,
}

/// Exact CSS custom properties for light-mode three-layer elevation shadows.
pub const LIGHT_SHADOW_BORDER: &str = "0px 0px 0px 1px oklch(0 0 0 / 0.06), \
     0px 1px 2px -1px oklch(0 0 0 / 0.06), \
     0px 2px 4px 0px oklch(0 0 0 / 0.04)";

/// Exact CSS custom properties for light-mode hover elevation shadows.
pub const LIGHT_SHADOW_BORDER_HOVER: &str = "0px 0px 0px 1px oklch(0 0 0 / 0.08), \
     0px 1px 2px -1px oklch(0 0 0 / 0.08), \
     0px 2px 4px 0px oklch(0 0 0 / 0.06)";

/// Exact CSS custom property for dark-mode white ring shadow.
pub const DARK_SHADOW_BORDER: &str = "0 0 0 1px oklch(1 0 0 / 0.08)";

/// Exact CSS custom property for dark-mode hover white ring shadow.
pub const DARK_SHADOW_BORDER_HOVER: &str = "0 0 0 1px oklch(1 0 0 / 0.13)";

/// Transparent border kept under shadow rings for `forced-colors` fallback.
pub const FORCED_COLORS_BORDER: &str = "1px solid transparent";

/// Returns the canonical rule specification for `ux_03_layered_shadows`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_03_layered_shadows",
        category: "ui-surfaces",
        title: "Shadows for elevation, borders for structure",
        summary: "Replace depth-only borders with layered transparent oklch box-shadows, \
                  keep 1px solid transparent for forced-colors, and gate hover elevation \
                  behind @media (hover: hover).",
        forced_colors_border: FORCED_COLORS_BORDER,
        transition_property: "box-shadow",
        transition_duration_ms: 150,
        transition_timing: "ease-out",
    }
}

/// Returns `(resting, hover)` `box-shadow` values for the given theme mode.
pub fn shadow_tokens(mode: ThemeMode) -> (&'static str, &'static str) {
    match mode {
        ThemeMode::Light => (LIGHT_SHADOW_BORDER, LIGHT_SHADOW_BORDER_HOVER),
        ThemeMode::Dark => (DARK_SHADOW_BORDER, DARK_SHADOW_BORDER_HOVER),
    }
}

/// Returns `true` when the surface should use layered `box-shadow` instead of a solid border.
pub fn uses_layered_shadow(role: SurfaceRole) -> bool {
    matches!(role, SurfaceRole::ElevationSurface)
}

/// Complete CSS snippet defining light/dark shadow tokens and elevated surface rules.
pub fn layered_shadow_css() -> &'static str {
    concat!(
        ":root {\n",
        "  --shadow-border:\n",
        "    0px 0px 0px 1px oklch(0 0 0 / 0.06),\n",
        "    0px 1px 2px -1px oklch(0 0 0 / 0.06),\n",
        "    0px 2px 4px 0px oklch(0 0 0 / 0.04);\n",
        "  --shadow-border-hover:\n",
        "    0px 0px 0px 1px oklch(0 0 0 / 0.08),\n",
        "    0px 1px 2px -1px oklch(0 0 0 / 0.08),\n",
        "    0px 2px 4px 0px oklch(0 0 0 / 0.06);\n",
        "}\n\n",
        ":root[data-theme=\"dark\"] {\n",
        "  --shadow-border: 0 0 0 1px oklch(1 0 0 / 0.08);\n",
        "  --shadow-border-hover: 0 0 0 1px oklch(1 0 0 / 0.13);\n",
        "}\n\n",
        ".surface-elevated {\n",
        "  border: 1px solid transparent;\n",
        "  box-shadow: var(--shadow-border);\n",
        "  transition-property: box-shadow;\n",
        "  transition-duration: 150ms;\n",
        "  transition-timing-function: ease-out;\n",
        "}\n\n",
        "@media (hover: hover) {\n",
        "  .surface-elevated:hover {\n",
        "    box-shadow: var(--shadow-border-hover);\n",
        "  }\n",
        "}\n",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_matches_exact_shadow_and_transition_rules() {
        let s = spec();
        assert_eq!(s.id, "ux_03_layered_shadows");
        assert_eq!(s.forced_colors_border, "1px solid transparent");
        assert_eq!(s.transition_property, "box-shadow");
        assert_eq!(s.transition_duration_ms, 150);
        assert_eq!(s.transition_timing, "ease-out");
    }

    #[test]
    fn light_and_dark_shadow_tokens_have_exact_oklch_layers() {
        let (light_rest, light_hover) = shadow_tokens(ThemeMode::Light);
        assert_eq!(light_rest.split(',').count(), 3);
        assert_eq!(light_hover.split(',').count(), 3);
        assert!(light_rest.contains("oklch(0 0 0 / 0.06)"));
        assert!(light_rest.contains("oklch(0 0 0 / 0.04)"));
        assert!(light_hover.contains("oklch(0 0 0 / 0.08)"));

        let (dark_rest, dark_hover) = shadow_tokens(ThemeMode::Dark);
        assert_eq!(dark_rest, "0 0 0 1px oklch(1 0 0 / 0.08)");
        assert_eq!(dark_hover, "0 0 0 1px oklch(1 0 0 / 0.13)");
    }

    #[test]
    fn css_includes_forced_colors_border_and_hover_media_query() {
        let css = layered_shadow_css();
        assert!(css.contains("border: 1px solid transparent;"));
        assert!(css.contains("transition-property: box-shadow;"));
        assert!(css.contains("transition-duration: 150ms;"));
        assert!(css.contains("@media (hover: hover)"));
        assert!(!css.contains("transition: all"));
    }

    #[test]
    fn structural_and_input_borders_are_preserved() {
        assert!(uses_layered_shadow(SurfaceRole::ElevationSurface));
        assert!(!uses_layered_shadow(SurfaceRole::StructuralDivider));
        assert!(!uses_layered_shadow(SurfaceRole::SelectedState));
        assert!(!uses_layered_shadow(SurfaceRole::FormInput));
    }
}
