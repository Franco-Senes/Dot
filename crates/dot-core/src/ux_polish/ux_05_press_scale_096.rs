//! UX-05: Press scale 0.96 over 150ms ease-out.
//!
//! Pressed interactive controls scale to `0.96` over `150ms` with `ease-out`.
//! Disabled controls and controls opted out via `static` never scale.
//! Press scale is preserved under `prefers-reduced-motion: reduce` because it
//! is brief tactile feedback rather than spatial travel.

/// Exact scale factor applied to a pressed interactive control.
pub const PRESS_SCALE: f32 = 0.96;

/// Resting scale factor for an unpressed, disabled, or static control.
pub const REST_SCALE: f32 = 1.0;

/// Transition duration in milliseconds for the press state.
pub const PRESS_DURATION_MS: u16 = 150;

/// Transition timing function for the press state.
pub const PRESS_EASING: &str = "ease-out";

/// Transitioned CSS property (never `all`).
pub const TRANSITION_PROPERTY: &str = "scale";

/// Tap highlight suppression for controls that draw their own pressed state.
pub const TAP_HIGHLIGHT_COLOR: &str = "transparent";

/// Specification metadata for a single UX polish rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub summary: &'static str,
    pub css_snippet: &'static str,
    pub invariants: &'static [&'static str],
}

/// Returns the canonical specification for `ux_05_press_scale_096`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_05_press_scale_096",
        title: "Press scales to 0.96",
        category: "motion",
        summary: "Pressed buttons scale to 0.96 over 150ms with ease-out; disabled and static buttons never scale.",
        css_snippet: concat!(
            ".sl-pressable {\n",
            "  transition-property: scale;\n",
            "  transition-duration: 150ms;\n",
            "  transition-timing-function: ease-out;\n",
            "  -webkit-tap-highlight-color: transparent;\n",
            "}\n",
            ".sl-pressable:not(:disabled):not([data-static=\"true\"]):active {\n",
            "  scale: 0.96;\n",
            "}\n",
        ),
        invariants: &[
            "Active press scale is exactly 0.96, never 0.95 or lower",
            "Duration is 150ms with ease-out timing",
            "Disabled controls never scale on :active",
            "Controls with static opt-out never scale on :active",
            "Transition names `scale` explicitly, never `all`",
            "Sets -webkit-tap-highlight-color: transparent to avoid double flash",
            "Preserved under prefers-reduced-motion because it is brief feedback, not travel",
        ],
    }
}

/// Configuration state for computing press-scale behavior on a control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PressScaleOptions {
    pub disabled: bool,
    pub is_static: bool,
    pub reduced_motion: bool,
}

impl PressScaleOptions {
    /// Returns `true` when the control is eligible to scale on press.
    ///
    /// Disabled controls and controls with `is_static` never scale.
    /// `reduced_motion` does not suppress press scale because it is brief
    /// feedback rather than travel.
    pub const fn should_apply_press_scale(self) -> bool {
        !self.disabled && !self.is_static
    }

    /// Computes the effective scale for the control given whether it is pressed.
    pub const fn effective_scale(self, pressed: bool) -> f32 {
        if pressed && self.should_apply_press_scale() {
            PRESS_SCALE
        } else {
            REST_SCALE
        }
    }
}

/// Validates that a candidate press scale matches the exact `0.96` token.
pub fn is_valid_press_scale(scale: f32) -> bool {
    (scale - PRESS_SCALE).abs() < 1e-6
}

/// Generates a scoped CSS rule enforcing `ux_05_press_scale_096` for `selector`.
///
/// Falls back to `.sl-pressable` if `selector` is empty or whitespace-only.
pub fn css_rule_for(selector: &str) -> String {
    let target = selector.trim();
    let target = if target.is_empty() {
        ".sl-pressable"
    } else {
        target
    };

    format!(
        "{target} {{\n  transition-property: {TRANSITION_PROPERTY};\n  transition-duration: {PRESS_DURATION_MS}ms;\n  transition-timing-function: {PRESS_EASING};\n  -webkit-tap-highlight-color: {TAP_HIGHLIGHT_COLOR};\n}}\n{target}:not(:disabled):not([data-static=\"true\"]):active {{\n  scale: {PRESS_SCALE:.2};\n}}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_exposes_exact_press_scale_values() {
        let s = spec();
        assert_eq!(s.id, "ux_05_press_scale_096");
        assert!(s.css_snippet.contains("scale: 0.96;"));
        assert!(s.css_snippet.contains("transition-duration: 150ms;"));
        assert!(s
            .css_snippet
            .contains("transition-timing-function: ease-out;"));
        assert!(s.css_snippet.contains("transition-property: scale;"));
        assert!(!s.css_snippet.contains("transition: all"));
        assert!(s.css_snippet.contains(":not(:disabled)"));
        assert!(s
            .css_snippet
            .contains("-webkit-tap-highlight-color: transparent;"));
    }

    #[test]
    fn disabled_and_static_controls_never_scale() {
        let enabled = PressScaleOptions::default();
        assert!(enabled.should_apply_press_scale());
        assert!((enabled.effective_scale(true) - 0.96).abs() < 1e-6);
        assert!((enabled.effective_scale(false) - 1.0).abs() < 1e-6);

        let disabled = PressScaleOptions {
            disabled: true,
            ..Default::default()
        };
        assert!(!disabled.should_apply_press_scale());
        assert!((disabled.effective_scale(true) - 1.0).abs() < 1e-6);

        let static_opt_out = PressScaleOptions {
            is_static: true,
            ..Default::default()
        };
        assert!(!static_opt_out.should_apply_press_scale());
        assert!((static_opt_out.effective_scale(true) - 1.0).abs() < 1e-6);

        let reduced = PressScaleOptions {
            reduced_motion: true,
            ..Default::default()
        };
        assert!(reduced.should_apply_press_scale());
        assert!((reduced.effective_scale(true) - 0.96).abs() < 1e-6);
    }

    #[test]
    fn scale_validator_rejects_approximations() {
        assert!(is_valid_press_scale(0.96));
        assert!(!is_valid_press_scale(0.95));
        assert!(!is_valid_press_scale(0.90));
    }

    #[test]
    fn css_generator_handles_custom_and_empty_selectors() {
        let custom = css_rule_for(".btn-primary");
        assert!(custom.contains(".btn-primary {"));
        assert!(custom.contains(".btn-primary:not(:disabled):not([data-static=\"true\"]):active"));
        assert!(custom.contains("scale: 0.96;"));

        let fallback = css_rule_for("   ");
        assert!(fallback.starts_with(".sl-pressable {"));
    }
}
