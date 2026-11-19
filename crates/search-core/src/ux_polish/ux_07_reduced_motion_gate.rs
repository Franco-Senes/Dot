//! UX-07: Reduced-motion gate and opacity cross-fade fallback.
//!
//! Gates movement, blur, and entrance/exit scale behind
//! `@media (prefers-reduced-motion: no-preference)`, replacing them with an
//! opacity cross-fade under reduced motion while keeping brief press scale
//! (`0.96`) feedback intact.

/// Specification metadata and exact tokens for `ux_07_reduced_motion_gate`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub summary: &'static str,
    pub full_motion_query: &'static str,
    pub reduced_motion_query: &'static str,
    pub fallback_opacity_ms: u16,
    pub expressive_duration_ms: u16,
    pub press_scale: f32,
    pub press_duration_ms: u16,
    pub easing: &'static str,
    pub static_cue_required: bool,
    pub css_fragment: &'static str,
}

/// Classification of an animated UI property or transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionKind {
    Travel,
    Blur,
    EntranceScale,
    ExitScale,
    PressScale,
    OpacityCrossFade,
}

/// Outcome of evaluating a motion kind against the user's reduced-motion setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MotionGateDecision {
    pub allowed: bool,
    pub fallback_opacity_only: bool,
    pub duration_ms: u16,
    pub transition_property: &'static str,
}

pub const FULL_MOTION_MEDIA_QUERY: &str = "@media (prefers-reduced-motion: no-preference)";
pub const REDUCED_MOTION_MEDIA_QUERY: &str = "@media (prefers-reduced-motion: reduce)";
pub const FALLBACK_OPACITY_DURATION_MS: u16 = 150;
pub const EXPRESSIVE_DURATION_MS: u16 = 300;
pub const PRESS_SCALE: f32 = 0.96;
pub const PRESS_DURATION_MS: u16 = 150;
pub const MOTION_EASING: &str = "cubic-bezier(0.2, 0, 0, 1)";

pub const REDUCED_MOTION_CSS: &str = r#".ux-motion-surface {
  transition-property: opacity;
  transition-duration: 150ms;
  transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
}

@media (prefers-reduced-motion: no-preference) {
  .ux-motion-surface {
    transition-property: opacity, translate, scale, filter;
    transition-duration: 300ms;
    transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
  }
}

.ux-pressable {
  transition-property: scale;
  transition-duration: 150ms;
  transition-timing-function: ease-out;
}

.ux-pressable:not(:disabled):active {
  scale: 0.96;
}"#;

/// Returns the canonical UX rule specification for `ux_07_reduced_motion_gate`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_07_reduced_motion_gate",
        title: "Reduced-motion gate with opacity cross-fade fallback",
        category: "motion-accessibility",
        summary: "Run travel, blur, and entrance/exit scale only under prefers-reduced-motion: no-preference; fall back to a 150ms opacity cross-fade while keeping 0.96 press scale.",
        full_motion_query: FULL_MOTION_MEDIA_QUERY,
        reduced_motion_query: REDUCED_MOTION_MEDIA_QUERY,
        fallback_opacity_ms: FALLBACK_OPACITY_DURATION_MS,
        expressive_duration_ms: EXPRESSIVE_DURATION_MS,
        press_scale: PRESS_SCALE,
        press_duration_ms: PRESS_DURATION_MS,
        easing: MOTION_EASING,
        static_cue_required: true,
        css_fragment: REDUCED_MOTION_CSS,
    }
}

/// Evaluates whether a given `MotionKind` runs as expressive motion or falls
/// back to an opacity cross-fade under `prefers_reduced_motion`.
pub fn evaluate_motion_gate(kind: MotionKind, prefers_reduced_motion: bool) -> MotionGateDecision {
    if prefers_reduced_motion {
        return match kind {
            MotionKind::PressScale => MotionGateDecision {
                allowed: true,
                fallback_opacity_only: false,
                duration_ms: PRESS_DURATION_MS,
                transition_property: "scale",
            },
            MotionKind::OpacityCrossFade => MotionGateDecision {
                allowed: true,
                fallback_opacity_only: false,
                duration_ms: FALLBACK_OPACITY_DURATION_MS,
                transition_property: "opacity",
            },
            MotionKind::Travel
            | MotionKind::Blur
            | MotionKind::EntranceScale
            | MotionKind::ExitScale => MotionGateDecision {
                allowed: false,
                fallback_opacity_only: true,
                duration_ms: FALLBACK_OPACITY_DURATION_MS,
                transition_property: "opacity",
            },
        };
    }

    match kind {
        MotionKind::PressScale => MotionGateDecision {
            allowed: true,
            fallback_opacity_only: false,
            duration_ms: PRESS_DURATION_MS,
            transition_property: "scale",
        },
        MotionKind::OpacityCrossFade => MotionGateDecision {
            allowed: true,
            fallback_opacity_only: false,
            duration_ms: FALLBACK_OPACITY_DURATION_MS,
            transition_property: "opacity",
        },
        MotionKind::ExitScale => MotionGateDecision {
            allowed: true,
            fallback_opacity_only: false,
            duration_ms: FALLBACK_OPACITY_DURATION_MS,
            transition_property: "opacity, translate, filter",
        },
        MotionKind::Travel | MotionKind::Blur | MotionKind::EntranceScale => MotionGateDecision {
            allowed: true,
            fallback_opacity_only: false,
            duration_ms: EXPRESSIVE_DURATION_MS,
            transition_property: "opacity, translate, scale, filter",
        },
    }
}

/// Returns Spanish sentence-case ON-state toggle copy for the reduced-motion setting.
pub fn spanish_toggle_copy() -> (&'static str, &'static str) {
    (
        "Reducir movimiento",
        "Usa transiciones suaves de opacidad y desactiva desplazamientos o desenfoques.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_matches_exact_motion_tokens() {
        let s = spec();
        assert_eq!(s.id, "ux_07_reduced_motion_gate");
        assert_eq!(
            s.full_motion_query,
            "@media (prefers-reduced-motion: no-preference)"
        );
        assert_eq!(
            s.reduced_motion_query,
            "@media (prefers-reduced-motion: reduce)"
        );
        assert_eq!(s.fallback_opacity_ms, 150);
        assert_eq!(s.expressive_duration_ms, 300);
        assert!((s.press_scale - 0.96).abs() < f32::EPSILON);
        assert_eq!(s.press_duration_ms, 150);
        assert_eq!(s.easing, "cubic-bezier(0.2, 0, 0, 1)");
        assert!(s.static_cue_required);
        assert!(s
            .css_fragment
            .contains("prefers-reduced-motion: no-preference"));
        assert!(s.css_fragment.contains("scale: 0.96;"));
        assert!(!s.css_fragment.contains("transition: all"));
    }

    #[test]
    fn reduced_motion_gates_travel_and_blur_but_keeps_press_scale() {
        let travel = evaluate_motion_gate(MotionKind::Travel, true);
        assert!(!travel.allowed);
        assert!(travel.fallback_opacity_only);
        assert_eq!(travel.duration_ms, 150);
        assert_eq!(travel.transition_property, "opacity");

        let blur = evaluate_motion_gate(MotionKind::Blur, true);
        assert!(!blur.allowed);
        assert!(blur.fallback_opacity_only);
        assert_eq!(blur.transition_property, "opacity");

        let press = evaluate_motion_gate(MotionKind::PressScale, true);
        assert!(press.allowed);
        assert!(!press.fallback_opacity_only);
        assert_eq!(press.duration_ms, 150);
        assert_eq!(press.transition_property, "scale");
    }

    #[test]
    fn no_preference_enables_expressive_motion() {
        let entrance = evaluate_motion_gate(MotionKind::EntranceScale, false);
        assert!(entrance.allowed);
        assert!(!entrance.fallback_opacity_only);
        assert_eq!(entrance.duration_ms, 300);
        assert_eq!(
            entrance.transition_property,
            "opacity, translate, scale, filter"
        );

        let exit = evaluate_motion_gate(MotionKind::ExitScale, false);
        assert!(exit.allowed);
        assert_eq!(exit.duration_ms, 150);
    }
}
