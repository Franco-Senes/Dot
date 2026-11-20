//! UX Rule 09: Exits are shorter and smaller than enters (150ms).
//!
//! Subtle exits animate opacity to `0`, `translateY` by a fixed `-12px`, and
//! `4px` of blur over `150ms` with `ease-out` (half the `300ms` enter duration).
//! Movement and blur are gated behind `prefers-reduced-motion: no-preference`,
//! falling back to an opacity-only fade when reduced motion is preferred.

/// Standard enter duration in milliseconds for staged or subtle surfaces.
pub const ENTER_DURATION_MS: u32 = 300;

/// Standard subtle exit duration in milliseconds.
pub const EXIT_DURATION_MS: u32 = 150;

/// Meaningful-destination exit duration in milliseconds (e.g., closing a drawer).
pub const FULL_EXIT_DURATION_MS: u32 = 200;

/// Fixed vertical exit offset in pixels (never the full container height).
pub const EXIT_TRANSLATE_Y_PX: i32 = -12;

/// Exit blur radius in pixels.
pub const EXIT_BLUR_PX: u32 = 4;

/// Final opacity for exiting elements.
pub const EXIT_TARGET_OPACITY: f32 = 0.0;

/// CSS easing function for exit transitions.
pub const EXIT_EASING: &str = "ease-out";

/// Metadata describing this UX polish rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub enter_duration_ms: u32,
    pub exit_duration_ms: u32,
    pub translate_y_px: i32,
    pub blur_px: u32,
    pub easing: &'static str,
    pub css_fragment: &'static str,
}

/// Kind of exit transition for an overlay, popover, toast, or drawer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitMotionKind {
    /// Default subtle exit: opacity 0, translateY(-12px), blur(4px) over 150ms.
    Subtle,
    /// Full-travel exit only when destination carries meaning (e.g., drawer closing).
    FullDrawer,
    /// Instant removal when motion adds no information.
    Instant,
}

/// Resolved exit animation parameters for a given kind and motion preference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExitSpec {
    pub duration_ms: u32,
    pub easing: &'static str,
    pub target_opacity: f32,
    pub translate_y_px: i32,
    pub blur_px: u32,
}

/// Returns the canonical specification for UX Rule 09.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_09_shorter_exits_150ms",
        title: "Exits are shorter and smaller than enters",
        summary: "Exit with opacity 0, 4px blur, and -12px translateY over 150ms ease-out; gate travel and blur behind prefers-reduced-motion.",
        enter_duration_ms: ENTER_DURATION_MS,
        exit_duration_ms: EXIT_DURATION_MS,
        translate_y_px: EXIT_TRANSLATE_Y_PX,
        blur_px: EXIT_BLUR_PX,
        easing: EXIT_EASING,
        css_fragment: SUBTLE_EXIT_CSS,
    }
}

/// Computes the resolved exit parameters for `kind` and `reduced_motion`.
pub fn exit_spec(kind: ExitMotionKind, reduced_motion: bool) -> ExitSpec {
    match kind {
        ExitMotionKind::Instant => ExitSpec {
            duration_ms: 0,
            easing: EXIT_EASING,
            target_opacity: EXIT_TARGET_OPACITY,
            translate_y_px: 0,
            blur_px: 0,
        },
        ExitMotionKind::Subtle => ExitSpec {
            duration_ms: EXIT_DURATION_MS,
            easing: EXIT_EASING,
            target_opacity: EXIT_TARGET_OPACITY,
            translate_y_px: if reduced_motion {
                0
            } else {
                EXIT_TRANSLATE_Y_PX
            },
            blur_px: if reduced_motion { 0 } else { EXIT_BLUR_PX },
        },
        ExitMotionKind::FullDrawer => ExitSpec {
            duration_ms: if reduced_motion {
                EXIT_DURATION_MS
            } else {
                FULL_EXIT_DURATION_MS
            },
            easing: EXIT_EASING,
            target_opacity: EXIT_TARGET_OPACITY,
            translate_y_px: 0,
            blur_px: 0,
        },
    }
}

/// Generates a CSS exit rule for `selector` that names exact changing properties
/// and gates translate/blur behind `@media (prefers-reduced-motion: no-preference)`.
pub fn css_exit_rule(selector: &str, kind: ExitMotionKind) -> String {
    let target = selector.trim();
    if target.is_empty() {
        return String::new();
    }

    match kind {
        ExitMotionKind::Instant => format!("{target} {{\n  opacity: 0;\n  transition: none;\n}}"),
        ExitMotionKind::Subtle => format!(
            "{target} {{\n\
             \x20 opacity: 0;\n\
             \x20 transition-property: opacity;\n\
             \x20 transition-duration: {EXIT_DURATION_MS}ms;\n\
             \x20 transition-timing-function: {EXIT_EASING};\n\
             }}\n\
             @media (prefers-reduced-motion: no-preference) {{\n\
             \x20 {target} {{\n\
             \x20   translate: 0 {EXIT_TRANSLATE_Y_PX}px;\n\
             \x20   filter: blur({EXIT_BLUR_PX}px);\n\
             \x20   transition-property: opacity, translate, filter;\n\
             \x20 }}\n\
             }}"
        ),
        ExitMotionKind::FullDrawer => format!(
            "{target} {{\n\
             \x20 opacity: 0;\n\
             \x20 transition-property: opacity;\n\
             \x20 transition-duration: {EXIT_DURATION_MS}ms;\n\
             \x20 transition-timing-function: {EXIT_EASING};\n\
             }}\n\
             @media (prefers-reduced-motion: no-preference) {{\n\
             \x20 {target} {{\n\
             \x20   translate: -100% 0;\n\
             \x20   transition-property: opacity, translate;\n\
             \x20   transition-duration: {FULL_EXIT_DURATION_MS}ms;\n\
             \x20 }}\n\
             }}"
        ),
    }
}

/// Canonical CSS snippet for a subtle 150ms exit with reduced-motion fallback.
pub const SUBTLE_EXIT_CSS: &str = "\
.ux-exit-subtle {\n\
  opacity: 0;\n\
  transition-property: opacity;\n\
  transition-duration: 150ms;\n\
  transition-timing-function: ease-out;\n\
}\n\
@media (prefers-reduced-motion: no-preference) {\n\
  .ux-exit-subtle {\n\
    translate: 0 -12px;\n\
    filter: blur(4px);\n\
    transition-property: opacity, translate, filter;\n\
  }\n\
}";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_has_exact_shorter_exit_values() {
        let s = spec();
        assert_eq!(s.id, "ux_09_shorter_exits_150ms");
        assert_eq!(s.enter_duration_ms, 300);
        assert_eq!(s.exit_duration_ms, 150);
        assert!(s.exit_duration_ms < s.enter_duration_ms);
        assert_eq!(s.translate_y_px, -12);
        assert_eq!(s.blur_px, 4);
        assert_eq!(s.easing, "ease-out");
    }

    #[test]
    fn exit_spec_respects_reduced_motion_and_instant() {
        let full = exit_spec(ExitMotionKind::Subtle, false);
        assert_eq!(full.duration_ms, 150);
        assert_eq!(full.translate_y_px, -12);
        assert_eq!(full.blur_px, 4);
        assert_eq!(full.target_opacity, 0.0);

        let reduced = exit_spec(ExitMotionKind::Subtle, true);
        assert_eq!(reduced.duration_ms, 150);
        assert_eq!(reduced.translate_y_px, 0);
        assert_eq!(reduced.blur_px, 0);
        assert_eq!(reduced.target_opacity, 0.0);

        let instant = exit_spec(ExitMotionKind::Instant, false);
        assert_eq!(instant.duration_ms, 0);
        assert_eq!(instant.translate_y_px, 0);
        assert_eq!(instant.blur_px, 0);
    }

    #[test]
    fn css_exit_rule_gates_travel_and_names_properties() {
        let css = css_exit_rule(".toast-exit", ExitMotionKind::Subtle);
        assert!(css.contains("transition-duration: 150ms;"));
        assert!(css.contains("transition-timing-function: ease-out;"));
        assert!(css.contains("@media (prefers-reduced-motion: no-preference)"));
        assert!(css.contains("translate: 0 -12px;"));
        assert!(css.contains("filter: blur(4px);"));
        assert!(css.contains("transition-property: opacity, translate, filter;"));
        assert!(!css.contains("transition: all"));
        assert!(!css.contains("transition-property: all"));
        assert_eq!(css_exit_rule("   ", ExitMotionKind::Subtle), "");
    }
}
