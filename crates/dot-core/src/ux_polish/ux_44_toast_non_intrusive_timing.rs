//! UX polish specification for non-intrusive toast notifications and timing.
//!
//! Covers auto-dismiss durations, hover/focus timer pause with grace window,
//! stacking limits, `better-ui` motion/surface tokens, ARIA live-region
//! politeness, and Spanish sentence-case verb-first copy.

/// Top-level specification for a UX polish rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub timing: ToastTimingSpec,
    pub motion: ToastMotionSpec,
    pub surface: ToastSurfaceSpec,
    pub css_fragment: &'static str,
}

/// Auto-dismiss and pause/resume timing rules for non-intrusive toasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToastTimingSpec {
    pub info_ms: u32,
    pub success_ms: u32,
    pub action_ms: u32,
    pub warning_ms: u32,
    pub resume_grace_ms: u32,
    pub max_visible_stack: u8,
    pub pause_on_hover: bool,
    pub pause_on_focus: bool,
}

/// Motion values aligned with `better-ui`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToastMotionSpec {
    pub enter_duration_ms: u32,
    pub enter_easing: &'static str,
    pub enter_translate_y_px: i32,
    pub enter_blur_px: u32,
    pub exit_duration_ms: u32,
    pub exit_easing: &'static str,
    pub exit_translate_y_px: i32,
    pub exit_blur_px: u32,
    pub press_scale: &'static str,
    pub press_duration_ms: u32,
}

/// Surface and concentric radius tokens aligned with `better-ui`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToastSurfaceSpec {
    pub inner_radius_px: u32,
    pub padding_px: u32,
    pub outer_radius_px: u32,
    pub viewport_offset_px: u32,
}

/// Severity / intent of a toast notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

/// ARIA live-region attributes for a toast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToastAriaSpec {
    pub role: &'static str,
    pub aria_live: &'static str,
    pub aria_atomic: bool,
    pub dismiss_label: &'static str,
}

/// Runtime timer state for a single toast, supporting pause on hover/focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToastTimerState {
    pub remaining_ms: Option<u32>,
    pub paused_by_hover: bool,
    pub paused_by_focus: bool,
}

pub const DEFAULT_TIMING: ToastTimingSpec = ToastTimingSpec {
    info_ms: 4_000,
    success_ms: 4_000,
    action_ms: 6_000,
    warning_ms: 6_000,
    resume_grace_ms: 1_500,
    max_visible_stack: 3,
    pause_on_hover: true,
    pause_on_focus: true,
};

pub const DEFAULT_MOTION: ToastMotionSpec = ToastMotionSpec {
    enter_duration_ms: 300,
    enter_easing: "cubic-bezier(0.2, 0, 0, 1)",
    enter_translate_y_px: 12,
    enter_blur_px: 4,
    exit_duration_ms: 150,
    exit_easing: "ease-out",
    exit_translate_y_px: -12,
    exit_blur_px: 4,
    press_scale: "0.96",
    press_duration_ms: 150,
};

pub const DEFAULT_SURFACE: ToastSurfaceSpec = ToastSurfaceSpec {
    inner_radius_px: 6,
    padding_px: 8,
    outer_radius_px: 14,
    viewport_offset_px: 16,
};

pub const TOAST_CSS: &str = r#".sl-toast-viewport {
  position: fixed;
  inset-block-end: 16px;
  inset-inline-end: 16px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  pointer-events: none;
  z-index: 60;
}

.sl-toast {
  pointer-events: auto;
  padding: 8px 12px;
  border-radius: 14px;
  border: 1px solid transparent;
  box-shadow: 0 8px 24px oklch(0 0 0 / 0.16), 0 1px 3px oklch(0 0 0 / 0.08);
  transition-property: opacity, transform, filter;
  transition-duration: 300ms;
  transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
}

.sl-toast[data-state="entering"] {
  opacity: 0;
  transform: translateY(12px);
  filter: blur(4px);
}

.sl-toast[data-state="open"] {
  opacity: 1;
  transform: translateY(0);
  filter: blur(0px);
}

.sl-toast[data-state="closing"] {
  opacity: 0;
  transform: translateY(-12px);
  filter: blur(4px);
  transition-duration: 150ms;
  transition-timing-function: ease-out;
}

.sl-toast-action {
  border-radius: 6px;
  transition-property: scale, background-color, opacity;
  transition-duration: 150ms;
  transition-timing-function: ease-out;
}

.sl-toast-action:not(:disabled):active {
  scale: 0.96;
}

@media (hover: hover) {
  .sl-toast-action:not(:disabled):hover {
    opacity: 0.9;
  }
}

@media (prefers-reduced-motion: reduce) {
  .sl-toast,
  .sl-toast[data-state="entering"],
  .sl-toast[data-state="closing"] {
    transform: none;
    filter: none;
    transition-property: opacity;
    transition-duration: 150ms;
    transition-timing-function: ease-out;
  }
}
"#;

/// Returns the complete UX specification for `ux_44_toast_non_intrusive_timing`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_44_toast_non_intrusive_timing",
        title: "Non-intrusive toast timing and motion",
        summary: "Auto-dismisses routine toasts in 4s (6s with actions), keeps errors persistent, pauses on hover/focus with a 1.5s resume grace window, and gates travel/blur behind prefers-reduced-motion.",
        timing: DEFAULT_TIMING,
        motion: DEFAULT_MOTION,
        surface: DEFAULT_SURFACE,
        css_fragment: TOAST_CSS,
    }
}

/// Computes the auto-dismiss duration in milliseconds (`None` means persistent).
pub const fn auto_dismiss_ms(kind: ToastKind, has_action: bool) -> Option<u32> {
    match kind {
        ToastKind::Error => None,
        ToastKind::Warning => Some(DEFAULT_TIMING.warning_ms),
        ToastKind::Info | ToastKind::Success => {
            if has_action {
                Some(DEFAULT_TIMING.action_ms)
            } else {
                Some(DEFAULT_TIMING.info_ms)
            }
        }
    }
}

/// Returns ARIA live-region semantics and Spanish verb-first dismiss label.
pub const fn aria_spec(kind: ToastKind) -> ToastAriaSpec {
    match kind {
        ToastKind::Error => ToastAriaSpec {
            role: "alert",
            aria_live: "assertive",
            aria_atomic: true,
            dismiss_label: "Cerrar notificación",
        },
        ToastKind::Info | ToastKind::Success | ToastKind::Warning => ToastAriaSpec {
            role: "status",
            aria_live: "polite",
            aria_atomic: true,
            dismiss_label: "Cerrar notificación",
        },
    }
}

/// Spanish sentence-case, verb-first copy helpers for common toast patterns.
pub mod copy {
    /// Undoable archive toast: states the completed fact without "exitosamente".
    pub const ARCHIVED_MESSAGE: &str = "Elemento archivado";
    pub const UNDO_ACTION: &str = "Deshacer";

    /// Network/save error: calm, plain, states what failed and the next step.
    pub const SAVE_ERROR_MESSAGE: &str =
        "No se pudo guardar. Comprueba tu conexión e inténtalo de nuevo.";
    pub const RETRY_ACTION: &str = "Reintentar";
}

impl ToastTimerState {
    /// Initializes a timer for the given toast kind and action presence.
    pub const fn new(kind: ToastKind, has_action: bool) -> Self {
        Self {
            remaining_ms: auto_dismiss_ms(kind, has_action),
            paused_by_hover: false,
            paused_by_focus: false,
        }
    }

    /// Whether countdown is currently paused by pointer hover or keyboard focus.
    pub const fn is_paused(&self) -> bool {
        self.paused_by_hover || self.paused_by_focus
    }

    /// Advances the timer by `elapsed_ms`. Returns `true` when the toast should dismiss.
    pub fn tick(&mut self, elapsed_ms: u32) -> bool {
        if self.is_paused() {
            return false;
        }
        match self.remaining_ms {
            None => false,
            Some(rem) => {
                let next = rem.saturating_sub(elapsed_ms);
                self.remaining_ms = Some(next);
                next == 0
            }
        }
    }

    /// Updates hover state and applies a grace window when resuming near expiry.
    pub fn set_hover(&mut self, hovering: bool) {
        let was_paused = self.is_paused();
        self.paused_by_hover = hovering;
        self.apply_resume_grace(was_paused);
    }

    /// Updates focus state and applies a grace window when resuming near expiry.
    pub fn set_focus(&mut self, focused: bool) {
        let was_paused = self.is_paused();
        self.paused_by_focus = focused;
        self.apply_resume_grace(was_paused);
    }

    fn apply_resume_grace(&mut self, was_paused: bool) {
        if was_paused && !self.is_paused() {
            if let Some(rem) = self.remaining_ms {
                if rem < DEFAULT_TIMING.resume_grace_ms {
                    self.remaining_ms = Some(DEFAULT_TIMING.resume_grace_ms);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_tokens_match_better_ui_and_concentric_radii() {
        let s = spec();
        assert_eq!(s.id, "ux_44_toast_non_intrusive_timing");
        assert_eq!(s.motion.enter_easing, "cubic-bezier(0.2, 0, 0, 1)");
        assert_eq!(s.motion.press_scale, "0.96");
        assert_eq!(s.motion.enter_duration_ms, 300);
        assert_eq!(s.motion.exit_duration_ms, 150);
        assert_eq!(
            s.surface.inner_radius_px + s.surface.padding_px,
            s.surface.outer_radius_px
        );
        assert!(s.css_fragment.contains("prefers-reduced-motion: reduce"));
        assert!(s.css_fragment.contains("@media (hover: hover)"));
        assert!(!s.css_fragment.contains("transition: all"));
    }

    #[test]
    fn timing_pauses_on_hover_and_focus_with_grace_window() {
        let mut timer = ToastTimerState::new(ToastKind::Success, true);
        assert_eq!(timer.remaining_ms, Some(6_000));
        assert!(!timer.tick(5_500));
        assert_eq!(timer.remaining_ms, Some(500));

        timer.set_hover(true);
        assert!(!timer.tick(5_000));
        assert_eq!(timer.remaining_ms, Some(500));

        timer.set_hover(false);
        assert_eq!(timer.remaining_ms, Some(1_500));
        assert!(!timer.tick(1_000));
        assert!(timer.tick(500));
    }

    #[test]
    fn error_toasts_are_persistent_and_assertive() {
        let mut timer = ToastTimerState::new(ToastKind::Error, true);
        assert_eq!(timer.remaining_ms, None);
        assert!(!timer.tick(60_000));

        let aria = aria_spec(ToastKind::Error);
        assert_eq!(aria.role, "alert");
        assert_eq!(aria.aria_live, "assertive");
        assert_eq!(aria.dismiss_label, "Cerrar notificación");

        let info_aria = aria_spec(ToastKind::Info);
        assert_eq!(info_aria.role, "status");
        assert_eq!(info_aria.aria_live, "polite");
    }

    #[test]
    fn spanish_copy_is_sentence_case_and_verb_first() {
        assert_eq!(copy::UNDO_ACTION, "Deshacer");
        assert_eq!(copy::RETRY_ACTION, "Reintentar");
        assert!(!copy::ARCHIVED_MESSAGE.contains("exitosamente"));
        assert!(!copy::SAVE_ERROR_MESSAGE.contains('!'));
    }
}
