//! UX-01: Concentric border radii for nested surfaces.
//!
//! Where nested surfaces share a visible, even inset, the outer radius equals
//! `inner_radius + padding + border_width`. Past `24px` of padding, or where
//! padding is deliberately asymmetric, treat the layers as separate surfaces
//! and keep each surface's independent radius token.

/// Maximum even padding (in CSS pixels) for which nested surfaces share a
/// concentric border-radius relationship.
pub const MAX_CONCENTRIC_PADDING_PX: f32 = 24.0;

/// Default inner radius (in CSS pixels) for Search Linux nested cards/panels.
pub const DEFAULT_INNER_RADIUS_PX: f32 = 12.0;

/// Default even padding (in CSS pixels) between outer and inner surfaces.
pub const DEFAULT_PADDING_PX: f32 = 8.0;

/// Default concentric outer radius (`12px + 8px = 20px`).
pub const DEFAULT_OUTER_RADIUS_PX: f32 = DEFAULT_INNER_RADIUS_PX + DEFAULT_PADDING_PX;

/// Specification metadata for this UX polish rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub summary: &'static str,
    pub css_formula: &'static str,
}

/// Returns the canonical specification for `ux_01_concentric_radii`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_01_concentric_radii",
        title: "Concentric border radii on nested surfaces",
        category: "surfaces",
        summary: "Outer radius equals inner radius plus padding plus border width up to 24px even padding.",
        css_formula: "outerRadius = innerRadius + padding + borderWidth",
    }
}

/// Parameters describing a nested surface pair.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConcentricSurface {
    pub inner_radius_px: f32,
    pub padding_px: f32,
    pub border_width_px: f32,
    pub asymmetric_padding: bool,
}

/// Result of evaluating whether two nested surfaces should use concentric radii
/// or independent surface tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RadiusResolution {
    Concentric {
        outer_radius_px: f32,
        inner_radius_px: f32,
    },
    SeparateSurfaces,
}

/// Computes the outer border radius from an inner radius, padding, and border width.
pub fn outer_radius_px(inner_radius_px: f32, padding_px: f32, border_width_px: f32) -> f32 {
    (inner_radius_px.max(0.0) + padding_px.max(0.0) + border_width_px.max(0.0)).max(0.0)
}

/// Computes the inner border radius from an outer radius, padding, and border width,
/// clamping at `0.0` so negative radii are never emitted.
pub fn inner_radius_px(outer_radius_px: f32, padding_px: f32, border_width_px: f32) -> f32 {
    (outer_radius_px.max(0.0) - padding_px.max(0.0) - border_width_px.max(0.0)).max(0.0)
}

/// Resolves whether a nested surface pair should be coupled concentrically or
/// treated as separate surfaces.
pub fn resolve_concentric_radius(surface: ConcentricSurface) -> RadiusResolution {
    if surface.asymmetric_padding || surface.padding_px > MAX_CONCENTRIC_PADDING_PX {
        return RadiusResolution::SeparateSurfaces;
    }

    let inner = surface.inner_radius_px.max(0.0);
    let outer = outer_radius_px(inner, surface.padding_px, surface.border_width_px);
    RadiusResolution::Concentric {
        outer_radius_px: outer,
        inner_radius_px: inner,
    }
}

/// Formats a CSS snippet pairing an outer container and inner element with
/// exact concentric `border-radius` and `padding` values.
pub fn css_pair(
    outer_selector: &str,
    inner_selector: &str,
    inner_radius: f32,
    padding: f32,
    border_width: f32,
) -> String {
    let outer = outer_radius_px(inner_radius, padding, border_width);
    let inner = inner_radius.max(0.0);
    let pad = padding.max(0.0);
    format!(
        "{outer_selector} {{\n  border-radius: {outer}px;\n  padding: {pad}px;\n}}\n{inner_selector} {{\n  border-radius: {inner}px;\n}}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_matches_concentric_radii_contract() {
        let s = spec();
        assert_eq!(s.id, "ux_01_concentric_radii");
        assert_eq!(
            s.css_formula,
            "outerRadius = innerRadius + padding + borderWidth"
        );
        assert_eq!(DEFAULT_OUTER_RADIUS_PX, 20.0);
    }

    #[test]
    fn concentric_formula_includes_padding_and_border() {
        assert_eq!(outer_radius_px(12.0, 8.0, 0.0), 20.0);
        assert_eq!(outer_radius_px(12.0, 8.0, 1.0), 21.0);
        assert_eq!(inner_radius_px(16.0, 8.0, 0.0), 8.0);
        assert_eq!(inner_radius_px(6.0, 8.0, 1.0), 0.0);
    }

    #[test]
    fn cutoff_and_asymmetric_padding_treat_layers_as_separate() {
        let over_cutoff = resolve_concentric_radius(ConcentricSurface {
            inner_radius_px: 12.0,
            padding_px: 25.0,
            border_width_px: 0.0,
            asymmetric_padding: false,
        });
        assert_eq!(over_cutoff, RadiusResolution::SeparateSurfaces);

        let asymmetric = resolve_concentric_radius(ConcentricSurface {
            inner_radius_px: 12.0,
            padding_px: 8.0,
            border_width_px: 0.0,
            asymmetric_padding: true,
        });
        assert_eq!(asymmetric, RadiusResolution::SeparateSurfaces);

        let at_cutoff = resolve_concentric_radius(ConcentricSurface {
            inner_radius_px: 12.0,
            padding_px: 24.0,
            border_width_px: 1.0,
            asymmetric_padding: false,
        });
        assert_eq!(
            at_cutoff,
            RadiusResolution::Concentric {
                outer_radius_px: 37.0,
                inner_radius_px: 12.0,
            }
        );
    }

    #[test]
    fn css_pair_emits_exact_concentric_values() {
        let css = css_pair(".card", ".card-inner", 12.0, 8.0, 0.0);
        assert!(css.contains("border-radius: 20px;"));
        assert!(css.contains("padding: 8px;"));
        assert!(css.contains("border-radius: 12px;"));
    }
}
