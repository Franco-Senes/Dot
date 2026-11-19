//! UX-12: Hover effects only on hover-capable pointers.
//!
//! Hover-only styling must be wrapped in `@media (hover: hover)` so touch
//! devices do not latch `:hover` after a tap and display a stuck selected
//! state. Controls that draw their own pressed state (such as `scale: 0.96`)
//! must also set `-webkit-tap-highlight-color: transparent` so the platform
//! gray tap flash does not double the pressed feedback.

use std::fmt;

/// Required CSS media query wrapping hover-only styling rules.
pub const HOVER_MEDIA_QUERY: &str = "@media (hover: hover)";

/// Strict media query for fine hover-capable pointers (mouse/trackpad).
pub const HOVER_FINE_MEDIA_QUERY: &str = "@media (hover: hover) and (pointer: fine)";

/// CSS declaration suppressing the default touch tap highlight flash when a
/// control draws its own pressed state.
pub const TAP_HIGHLIGHT_TRANSPARENT: &str = "-webkit-tap-highlight-color: transparent;";

/// Exact scale factor for a pressed interactive control.
pub const PRESS_SCALE: &str = "0.96";

/// Standard decelerating easing curve for responsive UI transitions.
pub const HOVER_EASING: &str = "cubic-bezier(0.2, 0, 0, 1)";

/// Maximum transition duration in milliseconds for hover feedback.
pub const MAX_HOVER_TRANSITION_MS: u16 = 150;

/// Reference CSS demonstrating `@media (hover: hover)` gating, explicit
/// transition properties, `0.96` press scale, and tap-highlight suppression.
pub const REFERENCE_CSS: &str = "\
.ux-interactive-row {
    -webkit-tap-highlight-color: transparent;
    transition-property: background-color, opacity, scale;
    transition-duration: 150ms;
    transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
}

.ux-interactive-row:not(:disabled):active {
    scale: 0.96;
}

@media (hover: hover) {
    .ux-interactive-row:hover {
        background-color: oklch(0.96 0 0 / 0.08);
    }
}";

/// Canonical specification for UX-12 (`ux_12_hover_pointer_query`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UxRuleSpec {
    /// Stable rule identifier.
    pub id: &'static str,
    /// Human-readable rule title.
    pub title: &'static str,
    /// Owning UX skill category.
    pub category: &'static str,
    /// Required media query around `:hover` rules.
    pub hover_media_query: &'static str,
    /// Required declaration when custom pressed states are drawn.
    pub tap_highlight_declaration: &'static str,
    /// Exact press scale value.
    pub press_scale: &'static str,
    /// Exact CSS easing curve.
    pub easing: &'static str,
    /// Maximum hover transition duration in milliseconds.
    pub max_transition_ms: u16,
    /// Reference CSS fragment.
    pub reference_css: &'static str,
    /// Spanish sentence-case verb-first label example for interactive rows.
    pub spanish_action_label: &'static str,
}

/// Returns the canonical `UxRuleSpec` for `ux_12_hover_pointer_query`.
pub const fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_12_hover_pointer_query",
        title: "Hover effects only on hover-capable pointers",
        category: "better-ui",
        hover_media_query: HOVER_MEDIA_QUERY,
        tap_highlight_declaration: TAP_HIGHLIGHT_TRANSPARENT,
        press_scale: PRESS_SCALE,
        easing: HOVER_EASING,
        max_transition_ms: MAX_HOVER_TRANSITION_MS,
        reference_css: REFERENCE_CSS,
        spanish_action_label: "Abrir resultado",
    }
}

/// Primary input pointer capability reported by the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerCapability {
    /// Pointer can hover freely (mouse, trackpad, stylus with hover).
    HoverCapable,
    /// Touch-first input where `:hover` latches after tap.
    TouchOnly,
}

impl PointerCapability {
    /// Returns `true` when hover-only visual states should be active.
    pub const fn allows_hover_styles(self) -> bool {
        matches!(self, Self::HoverCapable)
    }
}

/// Policy for `-webkit-tap-highlight-color` on interactive controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapHighlightPolicy {
    /// Control draws its own pressed state; suppress the browser gray flash.
    Transparent,
    /// Control relies on platform default tap feedback.
    PlatformDefault,
}

impl TapHighlightPolicy {
    /// Returns the CSS declaration if suppression is required, or `None`.
    pub const fn css_declaration(self) -> Option<&'static str> {
        match self {
            Self::Transparent => Some(TAP_HIGHLIGHT_TRANSPARENT),
            Self::PlatformDefault => None,
        }
    }
}

/// Determines the tap-highlight policy for a control.
pub const fn tap_highlight_for_control(draws_own_pressed_state: bool) -> TapHighlightPolicy {
    if draws_own_pressed_state {
        TapHighlightPolicy::Transparent
    } else {
        TapHighlightPolicy::PlatformDefault
    }
}

/// Errors returned when constructing a gated `:hover` CSS rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoverQueryError {
    /// CSS selector was empty or whitespace-only.
    EmptySelector,
    /// CSS declarations block was empty or whitespace-only.
    EmptyDeclarations,
}

impl fmt::Display for HoverQueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySelector => write!(f, "CSS selector cannot be empty"),
            Self::EmptyDeclarations => write!(f, "CSS declarations cannot be empty"),
        }
    }
}

impl std::error::Error for HoverQueryError {}

/// Wraps a selector's `:hover` declarations inside `@media (hover: hover)`.
///
/// If `selector` does not already contain `:hover`, `:hover` is appended.
pub fn wrap_hover_rule(selector: &str, declarations: &str) -> Result<String, HoverQueryError> {
    let trimmed_sel = selector.trim();
    if trimmed_sel.is_empty() {
        return Err(HoverQueryError::EmptySelector);
    }

    let trimmed_decls = declarations.trim();
    if trimmed_decls.is_empty() {
        return Err(HoverQueryError::EmptyDeclarations);
    }

    let hover_selector = if trimmed_sel.contains(":hover") {
        trimmed_sel.to_string()
    } else {
        format!("{trimmed_sel}:hover")
    };

    let indented_decls: String = trimmed_decls
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| format!("        {line}"))
        .collect::<Vec<_>>()
        .join("\n");

    Ok(format!(
        "{HOVER_MEDIA_QUERY} {{\n    {hover_selector} {{\n{indented_decls}\n    }}\n}}"
    ))
}

/// Audit findings for CSS hover and tap-highlight compliance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoverAuditFinding {
    /// A `:hover` selector appeared outside `@media (hover: hover)`.
    UngatedHoverSelector { line: usize },
    /// A custom `:active` state was defined without `-webkit-tap-highlight-color: transparent`.
    MissingTapHighlightReset,
    /// `transition: all` or `transition-property: all` was used.
    TransitionAllForbidden { line: usize },
}

/// Audits a CSS snippet for UX-12 compliance.
///
/// Checks that every `:hover` selector is nested inside `@media (hover: hover)`,
/// that custom `:active` rules pair with `-webkit-tap-highlight-color: transparent`,
/// and that `transition: all` is never used.
pub fn audit_hover_css(css: &str) -> Vec<HoverAuditFinding> {
    let mut findings = Vec::new();
    let mut brace_depth: usize = 0;
    let mut hover_media_depth: Option<usize> = None;
    let mut has_active_rule = false;
    let mut has_tap_highlight_transparent = false;

    for (idx, raw_line) in css.lines().enumerate() {
        let line_num = idx + 1;
        let lower = raw_line.to_ascii_lowercase();
        let trimmed = lower.trim();

        if trimmed.contains("-webkit-tap-highlight-color") && trimmed.contains("transparent") {
            has_tap_highlight_transparent = true;
        }

        if trimmed.contains("transition: all") || trimmed.contains("transition-property: all") {
            findings.push(HoverAuditFinding::TransitionAllForbidden { line: line_num });
        }

        if trimmed.contains(":active") {
            has_active_rule = true;
        }

        if trimmed.starts_with("@media") && trimmed.contains("hover: hover") {
            hover_media_depth = Some(brace_depth);
        }

        if trimmed.contains(":hover") && hover_media_depth.is_none() {
            findings.push(HoverAuditFinding::UngatedHoverSelector { line: line_num });
        }

        for ch in raw_line.chars() {
            match ch {
                '{' => brace_depth = brace_depth.saturating_add(1),
                '}' => {
                    brace_depth = brace_depth.saturating_sub(1);
                    if let Some(depth) = hover_media_depth {
                        if brace_depth <= depth {
                            hover_media_depth = None;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if has_active_rule && !has_tap_highlight_transparent {
        findings.push(HoverAuditFinding::MissingTapHighlightReset);
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_matches_exact_ux_values() {
        let rule = spec();
        assert_eq!(rule.id, "ux_12_hover_pointer_query");
        assert_eq!(rule.hover_media_query, "@media (hover: hover)");
        assert_eq!(
            rule.tap_highlight_declaration,
            "-webkit-tap-highlight-color: transparent;"
        );
        assert_eq!(rule.press_scale, "0.96");
        assert_eq!(rule.easing, "cubic-bezier(0.2, 0, 0, 1)");
        assert_eq!(rule.max_transition_ms, 150);
        assert_eq!(rule.spanish_action_label, "Abrir resultado");
        assert!(audit_hover_css(rule.reference_css).is_empty());
    }

    #[test]
    fn pointer_capability_and_tap_highlight_policies() {
        assert!(PointerCapability::HoverCapable.allows_hover_styles());
        assert!(!PointerCapability::TouchOnly.allows_hover_styles());

        assert_eq!(
            tap_highlight_for_control(true).css_declaration(),
            Some("-webkit-tap-highlight-color: transparent;")
        );
        assert_eq!(tap_highlight_for_control(false).css_declaration(), None);
    }

    #[test]
    fn wrap_hover_rule_generates_gated_css_and_validates_inputs() {
        let wrapped = wrap_hover_rule(".omnibox-row", "background-color: oklch(0.95 0 0);");
        assert!(wrapped.is_ok());
        if let Ok(css) = wrapped {
            assert!(css.starts_with("@media (hover: hover) {"));
            assert!(css.contains(".omnibox-row:hover {"));
            assert!(audit_hover_css(&css).is_empty());
        }

        assert_eq!(
            wrap_hover_rule("   ", "opacity: 1;"),
            Err(HoverQueryError::EmptySelector)
        );
        assert_eq!(
            wrap_hover_rule(".btn", "  "),
            Err(HoverQueryError::EmptyDeclarations)
        );
    }

    #[test]
    fn audit_detects_ungated_hover_missing_tap_highlight_and_transition_all() {
        let bad_css = "\
.tab-item {
    transition: all 150ms ease-out;
}
.tab-item:hover {
    opacity: 0.9;
}
.tab-item:active {
    scale: 0.96;
}";
        let findings = audit_hover_css(bad_css);
        assert_eq!(
            findings,
            vec![
                HoverAuditFinding::TransitionAllForbidden { line: 2 },
                HoverAuditFinding::UngatedHoverSelector { line: 4 },
                HoverAuditFinding::MissingTapHighlightReset,
            ]
        );
    }
}
