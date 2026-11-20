//! UX polish specification for Picture-in-Picture (PiP) hover controls (`ux_47_pip_hover_controls`).
//!
//! Enforces `better-ui` motion/surface/hover-gating rules and `better-writing`
//! Spanish sentence-case verb-first accessible labels for PiP overlay controls.

/// Exact press scale for interactive PiP control buttons (`better-ui`).
pub const PRESS_SCALE: f32 = 0.96;

/// Exact duration in milliseconds for high-frequency hover reveal and press feedback.
pub const HOVER_TRANSITION_MS: u16 = 150;

/// Exact duration in milliseconds for contextual icon cross-fades (play/pause, mute/unmute).
pub const ICON_CROSSFADE_MS: u16 = 300;

/// Exact easing curve for contextual icon cross-fades (`better-ui`).
pub const ICON_CROSSFADE_EASING: &str = "cubic-bezier(0.2, 0, 0, 1)";

/// Exact easing curve for hover reveal and press scale.
pub const HOVER_EASING: &str = "ease-out";

/// Inner radius (px) of PiP control buttons.
pub const INNER_RADIUS_PX: u16 = 8;

/// Uniform inset padding (px) between the PiP outer frame and its control pill.
pub const INSET_PADDING_PX: u16 = 8;

/// Concentric outer radius (px) = `INNER_RADIUS_PX + INSET_PADDING_PX`.
pub const OUTER_RADIUS_PX: u16 = INNER_RADIUS_PX + INSET_PADDING_PX;

/// Icon stroke width (px) matching 400-weight UI copy.
pub const ICON_STROKE_WIDTH_PX: f32 = 1.5;

/// Image/video frame inset outline in light mode (pure black at 10% alpha).
pub const FRAME_OUTLINE_LIGHT: &str = "oklch(0 0 0 / 0.1)";

/// Image/video frame inset outline in dark mode (pure white at 10% alpha).
pub const FRAME_OUTLINE_DARK: &str = "oklch(1 0 0 / 0.1)";

/// Structured UX rule specification for `ux_47_pip_hover_controls`.
#[derive(Debug, Clone, PartialEq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub press_scale: f32,
    pub hover_transition_ms: u16,
    pub icon_crossfade_ms: u16,
    pub icon_crossfade_easing: &'static str,
    pub inner_radius_px: u16,
    pub inset_padding_px: u16,
    pub outer_radius_px: u16,
    pub requires_hover_media_query: bool,
    pub requires_reduced_motion_gate: bool,
    pub tabular_nums_for_timecode: bool,
    pub css_fragment: &'static str,
}

/// Spanish sentence-case, verb-first accessible labels for PiP icon-only controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipControlLabels {
    pub play: &'static str,
    pub pause: &'static str,
    pub mute: &'static str,
    pub unmute: &'static str,
    pub return_to_tab: &'static str,
    pub close_pip: &'static str,
}

/// Returns the canonical Spanish sentence-case, verb-first `aria-label` set.
pub const fn control_labels() -> PipControlLabels {
    PipControlLabels {
        play: "Reproducir video",
        pause: "Pausar video",
        mute: "Silenciar audio",
        unmute: "Activar audio",
        return_to_tab: "Volver a la pestaña",
        close_pip: "Cerrar imagen en imagen",
    }
}

/// Returns the contextual `aria-label` for the play/pause toggle.
pub const fn play_pause_label(is_playing: bool) -> &'static str {
    let labels = control_labels();
    if is_playing {
        labels.pause
    } else {
        labels.play
    }
}

/// Returns the contextual `aria-label` for the mute/unmute toggle.
pub const fn mute_unmute_label(is_muted: bool) -> &'static str {
    let labels = control_labels();
    if is_muted {
        labels.unmute
    } else {
        labels.mute
    }
}

/// Visibility state of the PiP hover controls overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipControlsState {
    Hidden,
    VisibleHover,
    VisibleFocus,
    VisibleTouchToggle,
}

impl PipControlsState {
    /// Returns `true` when the control overlay should be interactive and visible.
    pub const fn is_visible(self) -> bool {
        !matches!(self, Self::Hidden)
    }

    /// Computes the next visibility state from pointer/focus/touch inputs.
    pub const fn resolve(
        pointer_hovering: bool,
        focus_within: bool,
        touch_toggled_open: bool,
    ) -> Self {
        if focus_within {
            Self::VisibleFocus
        } else if pointer_hovering {
            Self::VisibleHover
        } else if touch_toggled_open {
            Self::VisibleTouchToggle
        } else {
            Self::Hidden
        }
    }
}

/// CSS fragment implementing PiP hover controls with `@media (hover: hover)`,
/// `@media (prefers-reduced-motion: no-preference)`, concentric radii, `0.96` press scale,
/// `tabular-nums` timecode, and `currentColor` icon states.
pub const PIP_HOVER_CONTROLS_CSS: &str = r#".sl-pip-window {
  position: relative;
  border-radius: 16px;
  padding: 8px;
  border: 1px solid transparent;
  outline: 1px solid oklch(1 0 0 / 0.1);
  outline-offset: -1px;
  overflow: hidden;
}

.sl-pip-controls {
  display: flex;
  align-items: center;
  gap: 6px;
  opacity: 0;
  transition-property: opacity;
  transition-duration: 150ms;
  transition-timing-function: ease-out;
}

.sl-pip-window:focus-within .sl-pip-controls,
.sl-pip-window[data-controls-open="true"] .sl-pip-controls {
  opacity: 1;
}

@media (hover: hover) {
  .sl-pip-window:hover .sl-pip-controls {
    opacity: 1;
  }
}

.sl-pip-btn {
  border-radius: 8px;
  color: currentColor;
  -webkit-tap-highlight-color: transparent;
  transition-property: scale, opacity, background-color;
  transition-duration: 150ms;
  transition-timing-function: ease-out;
}

.sl-pip-btn:not(:disabled):active {
  scale: 0.96;
}

.sl-pip-icon {
  stroke: currentColor;
  stroke-width: 1.5px;
  transition-property: opacity;
  transition-duration: 150ms;
  transition-timing-function: ease-out;
}

@media (prefers-reduced-motion: no-preference) {
  .sl-pip-icon-swap {
    transition-property: scale, opacity, filter;
    transition-duration: 300ms;
    transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
  }
}

.sl-pip-timecode {
  font-variant-numeric: tabular-nums;
}"#;

/// Returns the complete UX rule specification for `ux_47_pip_hover_controls`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_47_pip_hover_controls",
        title: "Picture-in-Picture hover controls",
        press_scale: PRESS_SCALE,
        hover_transition_ms: HOVER_TRANSITION_MS,
        icon_crossfade_ms: ICON_CROSSFADE_MS,
        icon_crossfade_easing: ICON_CROSSFADE_EASING,
        inner_radius_px: INNER_RADIUS_PX,
        inset_padding_px: INSET_PADDING_PX,
        outer_radius_px: OUTER_RADIUS_PX,
        requires_hover_media_query: true,
        requires_reduced_motion_gate: true,
        tabular_nums_for_timecode: true,
        css_fragment: PIP_HOVER_CONTROLS_CSS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_enforces_exact_ui_polish_values() {
        let s = spec();
        assert_eq!(s.id, "ux_47_pip_hover_controls");
        assert!((s.press_scale - 0.96).abs() < f32::EPSILON);
        assert_eq!(s.hover_transition_ms, 150);
        assert_eq!(s.icon_crossfade_ms, 300);
        assert_eq!(s.icon_crossfade_easing, "cubic-bezier(0.2, 0, 0, 1)");
        assert_eq!(s.outer_radius_px, s.inner_radius_px + s.inset_padding_px);
        assert_eq!(s.outer_radius_px, 16);
        assert!(s.css_fragment.contains("@media (hover: hover)"));
        assert!(s
            .css_fragment
            .contains("@media (prefers-reduced-motion: no-preference)"));
        assert!(s.css_fragment.contains("scale: 0.96;"));
        assert!(s.css_fragment.contains("cubic-bezier(0.2, 0, 0, 1)"));
        assert!(s.css_fragment.contains("tabular-nums"));
        assert!(s.css_fragment.contains(":not(:disabled):active"));
        assert!(!s.css_fragment.contains("transition-property: all"));
    }

    #[test]
    fn spanish_labels_are_verb_first_and_sentence_case() {
        let labels = control_labels();
        assert_eq!(play_pause_label(false), "Reproducir video");
        assert_eq!(play_pause_label(true), "Pausar video");
        assert_eq!(mute_unmute_label(false), "Silenciar audio");
        assert_eq!(mute_unmute_label(true), "Activar audio");
        assert_eq!(labels.return_to_tab, "Volver a la pestaña");
        assert_eq!(labels.close_pip, "Cerrar imagen en imagen");

        for label in [
            labels.play,
            labels.pause,
            labels.mute,
            labels.unmute,
            labels.return_to_tab,
            labels.close_pip,
        ] {
            let first = label.chars().next().expect("non-empty label");
            assert!(first.is_uppercase(), "must start uppercase: {}", label);
            assert!(
                !label.contains("icono"),
                "must name action, not glyph: {}",
                label
            );
        }
    }

    #[test]
    fn controls_state_prioritizes_focus_and_hover_cleanly() {
        assert!(!PipControlsState::Hidden.is_visible());
        assert_eq!(
            PipControlsState::resolve(false, false, false),
            PipControlsState::Hidden
        );
        assert_eq!(
            PipControlsState::resolve(true, false, false),
            PipControlsState::VisibleHover
        );
        assert_eq!(
            PipControlsState::resolve(false, true, false),
            PipControlsState::VisibleFocus
        );
        assert_eq!(
            PipControlsState::resolve(false, false, true),
            PipControlsState::VisibleTouchToggle
        );
        assert!(PipControlsState::VisibleFocus.is_visible());
    }
}
