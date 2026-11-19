//! UX-39: Sidebar hover close reveal.
//!
//! Defines the interaction state machine, CSS rules, motion timings,
//! concentric radii, SVG icon constraints, and Spanish verb-first
//! accessibility copy for revealing the close button on sidebar rows.

/// Exact easing curve for UI state transitions.
pub const REVEAL_EASING: &str = "cubic-bezier(0.2, 0, 0, 1)";

/// High-frequency hover/focus reveal duration in milliseconds (<= 150ms).
pub const REVEAL_DURATION_MS: u16 = 150;

/// Press feedback easing.
pub const PRESS_EASING: &str = "ease-out";

/// Press feedback duration in milliseconds.
pub const PRESS_DURATION_MS: u16 = 150;

/// Exact scale factor when an enabled close button is pressed.
pub const PRESS_SCALE: f32 = 0.96;

/// Resting scale factor.
pub const REST_SCALE: f32 = 1.0;

/// Inner border radius of the sidebar close button in pixels.
pub const CLOSE_BUTTON_INNER_RADIUS_PX: u16 = 4;

/// Even padding inside the sidebar row around the close button in pixels.
pub const SIDEBAR_ROW_PADDING_PX: u16 = 4;

/// Concentric outer border radius of the sidebar row (`inner + padding`).
pub const SIDEBAR_ROW_OUTER_RADIUS_PX: u16 = CLOSE_BUTTON_INNER_RADIUS_PX + SIDEBAR_ROW_PADDING_PX;

/// Stroke width for the close icon SVG at regular text weight (400).
pub const CLOSE_ICON_STROKE_PX: f32 = 1.5;

/// SVG color keyword ensuring a single asset recolors per state.
pub const CLOSE_ICON_COLOR: &str = "currentColor";

/// Default verb-first Spanish accessible label for closing a tab.
pub const LABEL_CLOSE_TAB: &str = "Cerrar pestana";

/// Default verb-first Spanish accessible label for closing a generic sidebar item.
pub const LABEL_CLOSE_ITEM: &str = "Cerrar elemento";

/// Actionable Spanish tooltip when a pinned tab cannot be closed directly.
pub const TOOLTIP_PINNED_UNCLOSABLE: &str = "Desfija la pestana para cerrarla";

/// Pointer capability of the active input device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerCapability {
    /// Device supports true hover (`@media (hover: hover)`).
    HoverCapable,
    /// Touch-only or non-hover device (`@media (hover: none)`).
    TouchOnly,
}

/// Interactive state of a sidebar row and its close button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RowInteractionState {
    pub hovered: bool,
    pub focus_within: bool,
    pub button_focused: bool,
    pub row_selected: bool,
    pub pressed: bool,
    pub disabled: bool,
    pub reduced_motion: bool,
}

/// Resolved visual and interactive state of the sidebar close button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseRevealState {
    /// Visually hidden (`opacity: 0`, `pointer-events: none`) on idle hover-capable rows.
    Hidden,
    /// Revealed (`opacity: 1`, `pointer-events: auto`).
    Visible,
    /// Actively pressed (`scale(0.96)`), only when not disabled.
    Pressed,
    /// Disabled (`opacity: 0.4` when revealed, never scales on press).
    Disabled,
}

/// Structured UX specification for `ux_39_sidebar_hover_close_reveal`.
#[derive(Debug, Clone, PartialEq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub reveal_duration_ms: u16,
    pub reveal_easing: &'static str,
    pub press_duration_ms: u16,
    pub press_easing: &'static str,
    pub press_scale: f32,
    pub inner_radius_px: u16,
    pub row_padding_px: u16,
    pub outer_radius_px: u16,
    pub icon_stroke_px: f32,
    pub icon_color: &'static str,
    pub default_aria_label: &'static str,
    pub css_fragment: &'static str,
}

/// Canonical CSS fragment implementing hover-capable reveal, keyboard focus-within,
/// touch fallback, press scale `0.96`, `:disabled` exclusion, and reduced-motion gating.
pub const SIDEBAR_HOVER_CLOSE_CSS: &str = r#".sidebar-row {
  border-radius: 8px;
  padding: 4px;
}

.sidebar-row__close {
  border-radius: 4px;
  color: currentColor;
  opacity: 1;
  pointer-events: auto;
  -webkit-tap-highlight-color: transparent;
  transition-property: opacity, transform, background-color;
  transition-duration: 150ms;
  transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
}

@media (hover: hover) {
  .sidebar-row__close {
    opacity: 0;
    pointer-events: none;
  }

  .sidebar-row:hover .sidebar-row__close,
  .sidebar-row:focus-within .sidebar-row__close,
  .sidebar-row[aria-selected="true"] .sidebar-row__close,
  .sidebar-row__close:focus-visible {
    opacity: 1;
    pointer-events: auto;
  }
}

.sidebar-row__close:not(:disabled):not([aria-disabled="true"]):active {
  transform: scale(0.96);
  transition-duration: 150ms;
  transition-timing-function: ease-out;
}

.sidebar-row__close:disabled,
.sidebar-row__close[aria-disabled="true"] {
  opacity: 0.4;
  transform: none;
}

@media (prefers-reduced-motion: reduce) {
  .sidebar-row__close {
    transition-property: opacity, background-color;
    transition-duration: 150ms;
  }
}
"#;

/// Returns the complete UX rule specification for `ux_39_sidebar_hover_close_reveal`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_39_sidebar_hover_close_reveal",
        title: "Sidebar hover close reveal",
        reveal_duration_ms: REVEAL_DURATION_MS,
        reveal_easing: REVEAL_EASING,
        press_duration_ms: PRESS_DURATION_MS,
        press_easing: PRESS_EASING,
        press_scale: PRESS_SCALE,
        inner_radius_px: CLOSE_BUTTON_INNER_RADIUS_PX,
        row_padding_px: SIDEBAR_ROW_PADDING_PX,
        outer_radius_px: SIDEBAR_ROW_OUTER_RADIUS_PX,
        icon_stroke_px: CLOSE_ICON_STROKE_PX,
        icon_color: CLOSE_ICON_COLOR,
        default_aria_label: LABEL_CLOSE_TAB,
        css_fragment: SIDEBAR_HOVER_CLOSE_CSS,
    }
}

/// Computes the concentric outer border radius (`inner + padding`).
pub const fn concentric_outer_radius_px(inner_radius_px: u16, padding_px: u16) -> u16 {
    inner_radius_px.saturating_add(padding_px)
}

/// Resolves whether the sidebar close button is hidden, visible, pressed, or disabled.
///
/// On touch-only devices (`PointerCapability::TouchOnly`), the close button is never
/// gated behind hover so users are never trapped by un-hoverable or sticky `:hover` states.
pub fn resolve_close_reveal(
    pointer: PointerCapability,
    state: RowInteractionState,
) -> CloseRevealState {
    let revealed = match pointer {
        PointerCapability::TouchOnly => true,
        PointerCapability::HoverCapable => {
            state.hovered || state.focus_within || state.button_focused || state.row_selected
        }
    };

    if !revealed {
        return CloseRevealState::Hidden;
    }

    if state.disabled {
        return CloseRevealState::Disabled;
    }

    if state.pressed {
        return CloseRevealState::Pressed;
    }

    CloseRevealState::Visible
}

/// Returns the scale factor for the close button (`0.96` on active press, `1.0` otherwise).
/// Disabled buttons never scale.
pub fn close_button_scale(state: RowInteractionState) -> f32 {
    if state.pressed && !state.disabled {
        PRESS_SCALE
    } else {
        REST_SCALE
    }
}

/// Returns the target opacity for the close button given pointer capability and state.
pub fn close_button_opacity(pointer: PointerCapability, state: RowInteractionState) -> f32 {
    match resolve_close_reveal(pointer, state) {
        CloseRevealState::Hidden => 0.0,
        CloseRevealState::Visible | CloseRevealState::Pressed => 1.0,
        CloseRevealState::Disabled => 0.4,
    }
}

/// Formats a Spanish sentence-case, verb-first `aria-label` for closing a sidebar tab.
///
/// Builds the string whole rather than concatenating fragments around a variable.
pub fn format_close_tab_label(tab_title: Option<&str>) -> String {
    match tab_title.map(str::trim).filter(|t| !t.is_empty()) {
        Some(title) => format!("Cerrar pestana: {title}"),
        None => LABEL_CLOSE_TAB.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_matches_exact_motion_radii_and_icon_tokens() {
        let s = spec();
        assert_eq!(s.id, "ux_39_sidebar_hover_close_reveal");
        assert_eq!(s.reveal_easing, "cubic-bezier(0.2, 0, 0, 1)");
        assert_eq!(s.reveal_duration_ms, 150);
        assert_eq!(s.press_easing, "ease-out");
        assert_eq!(s.press_duration_ms, 150);
        assert!((s.press_scale - 0.96).abs() < f32::EPSILON);
        assert_eq!(s.outer_radius_px, concentric_outer_radius_px(4, 4));
        assert_eq!(s.outer_radius_px, 8);
        assert!((s.icon_stroke_px - 1.5).abs() < f32::EPSILON);
        assert_eq!(s.icon_color, "currentColor");
    }

    #[test]
    fn css_gates_hover_and_reduced_motion_without_transition_all() {
        let css = spec().css_fragment;
        assert!(css.contains("@media (hover: hover)"));
        assert!(css.contains("@media (prefers-reduced-motion: reduce)"));
        assert!(css.contains("transition-property: opacity, transform, background-color;"));
        assert!(!css.contains("transition: all"));
        assert!(!css.contains("transition-all"));
        assert!(css.contains("scale(0.96)"));
        assert!(css.contains(":not(:disabled)"));
        assert!(css.contains("-webkit-tap-highlight-color: transparent"));
        assert!(css.contains(":focus-within"));
        assert!(css.contains(":focus-visible"));
    }

    #[test]
    fn hover_capable_idle_row_hides_close_until_hover_focus_or_selection() {
        let idle = RowInteractionState::default();
        assert_eq!(
            resolve_close_reveal(PointerCapability::HoverCapable, idle),
            CloseRevealState::Hidden
        );
        assert_eq!(
            close_button_opacity(PointerCapability::HoverCapable, idle),
            0.0
        );

        let hovered = RowInteractionState {
            hovered: true,
            ..Default::default()
        };
        assert_eq!(
            resolve_close_reveal(PointerCapability::HoverCapable, hovered),
            CloseRevealState::Visible
        );

        let keyboard_focus = RowInteractionState {
            focus_within: true,
            ..Default::default()
        };
        assert_eq!(
            resolve_close_reveal(PointerCapability::HoverCapable, keyboard_focus),
            CloseRevealState::Visible
        );

        let selected = RowInteractionState {
            row_selected: true,
            ..Default::default()
        };
        assert_eq!(
            resolve_close_reveal(PointerCapability::HoverCapable, selected),
            CloseRevealState::Visible
        );
    }

    #[test]
    fn touch_pointer_keeps_close_accessible_without_hover() {
        let idle = RowInteractionState::default();
        assert_eq!(
            resolve_close_reveal(PointerCapability::TouchOnly, idle),
            CloseRevealState::Visible
        );
        assert_eq!(
            close_button_opacity(PointerCapability::TouchOnly, idle),
            1.0
        );
    }

    #[test]
    fn press_scales_to_0_96_only_when_enabled() {
        let pressed = RowInteractionState {
            hovered: true,
            pressed: true,
            disabled: false,
            ..Default::default()
        };
        assert_eq!(
            resolve_close_reveal(PointerCapability::HoverCapable, pressed),
            CloseRevealState::Pressed
        );
        assert!((close_button_scale(pressed) - 0.96).abs() < f32::EPSILON);

        let disabled_pressed = RowInteractionState {
            hovered: true,
            pressed: true,
            disabled: true,
            ..Default::default()
        };
        assert_eq!(
            resolve_close_reveal(PointerCapability::HoverCapable, disabled_pressed),
            CloseRevealState::Disabled
        );
        assert!((close_button_scale(disabled_pressed) - 1.0).abs() < f32::EPSILON);
        assert!(
            (close_button_opacity(PointerCapability::HoverCapable, disabled_pressed) - 0.4).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn spanish_verb_first_copy_formats_cleanly() {
        assert_eq!(format_close_tab_label(None), "Cerrar pestana");
        assert_eq!(format_close_tab_label(Some("   ")), "Cerrar pestana");
        assert_eq!(
            format_close_tab_label(Some("Documentacion de Rust")),
            "Cerrar pestana: Documentacion de Rust"
        );
        assert!(LABEL_CLOSE_ITEM.starts_with("Cerrar"));
        assert!(!TOOLTIP_PINNED_UNCLOSABLE.contains('!'));
    }
}
