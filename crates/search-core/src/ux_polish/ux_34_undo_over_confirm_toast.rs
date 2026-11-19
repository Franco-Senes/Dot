//! UX-34: Undo-over-confirmation toast specification, state machine, CSS, and Spanish copy helpers.
//!
//! Reversible, frequent actions execute immediately and present a non-blocking toast with an
//! explicit verb-first undo action (`Deshacer`) instead of interrupting the user with a
//! confirmation modal. Confirmation dialogs are reserved for irreversible, multi-user, or
//! bulk-destructive operations.

use std::fmt;

/// Specification metadata for this UX polish rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub summary: &'static str,
    pub default_undo_window_ms: u32,
    pub enter_duration_ms: u16,
    pub exit_duration_ms: u16,
    pub press_scale: &'static str,
    pub easing_standard: &'static str,
    pub outer_radius_px: u8,
    pub inner_radius_px: u8,
    pub padding_px: u8,
    pub aria_role: &'static str,
    pub aria_live: &'static str,
}

/// Returns the canonical UX rule specification for `ux_34_undo_over_confirm_toast`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_34_undo_over_confirm_toast",
        title: "Undo over confirmation toast",
        category: "feedback-and-destructive-actions",
        summary: "Act immediately on reversible frequent actions and offer an accessible undo toast instead of a confirmation modal.",
        default_undo_window_ms: 5000,
        enter_duration_ms: 300,
        exit_duration_ms: 150,
        press_scale: "0.96",
        easing_standard: "cubic-bezier(0.2, 0, 0, 1)",
        outer_radius_px: 14,
        inner_radius_px: 6,
        padding_px: 8,
        aria_role: "status",
        aria_live: "polite",
    }
}

/// Classification of an action's risk and reversibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionReversibility {
    /// Action can be undone cleanly (e.g. close tab, archive item, remove bookmark to trash).
    Reversible,
    /// Action cannot be recovered once committed (e.g. permanent purge, clear all site keys).
    Irreversible,
    /// Action affects other users or shared state.
    AffectsOtherPeople,
    /// Action destroys or alters many items in a single operation.
    BulkDestructive { count: usize },
}

/// Recommended UI pattern for an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackStrategy {
    /// Execute immediately and show an undo toast.
    ImmediateWithUndoToast,
    /// Require explicit confirmation before executing.
    ConfirmDialog,
}

/// Default threshold at which a batch operation requires confirmation.
pub const DEFAULT_BULK_CONFIRM_THRESHOLD: usize = 5;

/// Chooses between immediate execution with an undo toast and a confirmation dialog.
pub fn decide_feedback_strategy(
    reversibility: ActionReversibility,
    bulk_threshold: usize,
) -> FeedbackStrategy {
    match reversibility {
        ActionReversibility::Reversible => FeedbackStrategy::ImmediateWithUndoToast,
        ActionReversibility::Irreversible | ActionReversibility::AffectsOtherPeople => {
            FeedbackStrategy::ConfirmDialog
        }
        ActionReversibility::BulkDestructive { count } => {
            if count >= bulk_threshold.max(2) {
                FeedbackStrategy::ConfirmDialog
            } else {
                FeedbackStrategy::ImmediateWithUndoToast
            }
        }
    }
}

/// Spanish sentence-case copy for an undo toast.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoToastCopy {
    /// Sentence-case status message describing the completed action.
    pub message: String,
    /// Verb-first primary action label (`Deshacer`).
    pub undo_button_label: &'static str,
    /// Verb-first accessible name for the undo button.
    pub undo_aria_label: String,
    /// Verb-first accessible name for the dismiss button.
    pub dismiss_aria_label: &'static str,
}

/// Builds Spanish sentence-case copy for an undo toast.
///
/// `completed_action` should be a sentence-case phrase describing what happened
/// (e.g. `"Marcador eliminado"` or `"Pestaña cerrada"`).
pub fn build_undo_toast_copy(completed_action: &str, object_name: Option<&str>) -> UndoToastCopy {
    let trimmed_action = completed_action.trim();
    let message = match object_name.map(str::trim).filter(|s| !s.is_empty()) {
        Some(name) => format!("{trimmed_action}: «{name}»."),
        None => format!("{trimmed_action}."),
    };
    let undo_aria_label = match object_name.map(str::trim).filter(|s| !s.is_empty()) {
        Some(name) => format!("Deshacer {trimmed_action} («{name}»)"),
        None => format!("Deshacer {trimmed_action}"),
    };

    UndoToastCopy {
        message,
        undo_button_label: "Deshacer",
        undo_aria_label,
        dismiss_aria_label: "Cerrar notificación",
    }
}

/// Lifecycle state of an active undo toast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoToastStatus {
    Visible,
    Paused,
    Undone,
    Expired,
    Dismissed,
}

impl fmt::Display for UndoToastStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Visible => "visible",
            Self::Paused => "paused",
            Self::Undone => "undone",
            Self::Expired => "expired",
            Self::Dismissed => "dismissed",
        };
        f.write_str(s)
    }
}

/// Pure-Rust state machine managing the undo toast countdown and user interactions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoToastState {
    pub action_id: String,
    pub copy: UndoToastCopy,
    pub duration_ms: u32,
    pub remaining_ms: u32,
    pub status: UndoToastStatus,
}

impl UndoToastState {
    /// Creates a new visible undo toast with a minimum 3000ms window.
    pub fn new(action_id: impl Into<String>, copy: UndoToastCopy, duration_ms: u32) -> Self {
        let clamped_duration = duration_ms.max(3000);
        Self {
            action_id: action_id.into(),
            copy,
            duration_ms: clamped_duration,
            remaining_ms: clamped_duration,
            status: UndoToastStatus::Visible,
        }
    }

    /// Advances the countdown by `elapsed_ms` when visible.
    pub fn tick(&mut self, elapsed_ms: u32) {
        if self.status != UndoToastStatus::Visible {
            return;
        }
        self.remaining_ms = self.remaining_ms.saturating_sub(elapsed_ms);
        if self.remaining_ms == 0 {
            self.status = UndoToastStatus::Expired;
        }
    }

    /// Pauses auto-dismissal while the pointer hovers or focus is inside the toast.
    pub fn pause(&mut self) {
        if self.status == UndoToastStatus::Visible {
            self.status = UndoToastStatus::Paused;
        }
    }

    /// Resumes auto-dismissal after pointer leave or blur.
    pub fn resume(&mut self) {
        if self.status == UndoToastStatus::Paused && self.remaining_ms > 0 {
            self.status = UndoToastStatus::Visible;
        }
    }

    /// Triggers undo if the toast is still active; returns `true` if undo should execute.
    pub fn trigger_undo(&mut self) -> bool {
        match self.status {
            UndoToastStatus::Visible | UndoToastStatus::Paused => {
                self.status = UndoToastStatus::Undone;
                true
            }
            UndoToastStatus::Undone | UndoToastStatus::Expired | UndoToastStatus::Dismissed => {
                false
            }
        }
    }

    /// Dismisses the toast without undoing the action.
    pub fn dismiss(&mut self) {
        if matches!(
            self.status,
            UndoToastStatus::Visible | UndoToastStatus::Paused
        ) {
            self.status = UndoToastStatus::Dismissed;
        }
    }

    /// Whole seconds remaining, rounded up, suitable for `tabular-nums` display.
    pub fn remaining_seconds_ceil(&self) -> u32 {
        self.remaining_ms.div_ceil(1000)
    }
}

/// CSS fragment implementing concentric radii, motion gates, press scale, `tabular-nums`,
/// and hover-capable pointer isolation for the undo toast.
pub const UNDO_TOAST_CSS: &str = r#".sl-undo-toast {
  --sl-toast-radius-inner: 6px;
  --sl-toast-padding: 8px;
  --sl-toast-radius-outer: calc(var(--sl-toast-radius-inner) + var(--sl-toast-padding));
  border-radius: var(--sl-toast-radius-outer);
  padding: var(--sl-toast-padding);
  border: 1px solid oklch(0 0 0 / 0.12);
  box-shadow: 0 8px 24px oklch(0 0 0 / 0.16);
  display: inline-flex;
  align-items: center;
  gap: 10px;
  transition-property: opacity, transform, filter;
  transition-duration: 300ms;
  transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
}

.sl-undo-toast[data-state="exiting"] {
  transition-duration: 150ms;
  transition-timing-function: ease-out;
  opacity: 0;
}

@media (prefers-reduced-motion: no-preference) {
  .sl-undo-toast[data-state="entering"] {
    opacity: 0;
    transform: translateY(12px);
    filter: blur(4px);
  }

  .sl-undo-toast[data-state="exiting"] {
    opacity: 0;
    transform: translateY(-12px);
    filter: blur(4px);
  }
}

.sl-undo-toast__timer {
  font-variant-numeric: tabular-nums;
}

.sl-undo-toast__action {
  border-radius: var(--sl-toast-radius-inner);
  border: 1px solid transparent;
  transition-property: scale, opacity, background-color;
  transition-duration: 150ms;
  transition-timing-function: ease-out;
  -webkit-tap-highlight-color: transparent;
}

.sl-undo-toast__action:not(:disabled):active {
  scale: 0.96;
}

@media (hover: hover) {
  .sl-undo-toast__action:not(:disabled):hover {
    background-color: oklch(0.92 0.01 250 / 0.14);
  }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_matches_exact_motion_and_concentric_radii() {
        let s = spec();
        assert_eq!(s.id, "ux_34_undo_over_confirm_toast");
        assert_eq!(s.easing_standard, "cubic-bezier(0.2, 0, 0, 1)");
        assert_eq!(s.press_scale, "0.96");
        assert_eq!(s.enter_duration_ms, 300);
        assert_eq!(s.exit_duration_ms, 150);
        assert_eq!(s.outer_radius_px, s.inner_radius_px + s.padding_px);
        assert_eq!(s.aria_role, "status");
        assert_eq!(s.aria_live, "polite");
        assert!(UNDO_TOAST_CSS.contains("tabular-nums"));
        assert!(UNDO_TOAST_CSS.contains("scale: 0.96"));
        assert!(UNDO_TOAST_CSS.contains("cubic-bezier(0.2, 0, 0, 1)"));
        assert!(UNDO_TOAST_CSS.contains("@media (hover: hover)"));
        assert!(UNDO_TOAST_CSS.contains("prefers-reduced-motion: no-preference"));
    }

    #[test]
    fn strategy_prefers_undo_for_reversible_and_small_batches() {
        assert_eq!(
            decide_feedback_strategy(
                ActionReversibility::Reversible,
                DEFAULT_BULK_CONFIRM_THRESHOLD
            ),
            FeedbackStrategy::ImmediateWithUndoToast
        );
        assert_eq!(
            decide_feedback_strategy(
                ActionReversibility::BulkDestructive { count: 3 },
                DEFAULT_BULK_CONFIRM_THRESHOLD
            ),
            FeedbackStrategy::ImmediateWithUndoToast
        );
        assert_eq!(
            decide_feedback_strategy(
                ActionReversibility::BulkDestructive { count: 5 },
                DEFAULT_BULK_CONFIRM_THRESHOLD
            ),
            FeedbackStrategy::ConfirmDialog
        );
        assert_eq!(
            decide_feedback_strategy(
                ActionReversibility::Irreversible,
                DEFAULT_BULK_CONFIRM_THRESHOLD
            ),
            FeedbackStrategy::ConfirmDialog
        );
        assert_eq!(
            decide_feedback_strategy(
                ActionReversibility::AffectsOtherPeople,
                DEFAULT_BULK_CONFIRM_THRESHOLD
            ),
            FeedbackStrategy::ConfirmDialog
        );
    }

    #[test]
    fn copy_uses_spanish_sentence_case_and_verb_first_undo() {
        let copy = build_undo_toast_copy("Marcador eliminado", Some("Documentación Rust"));
        assert_eq!(copy.message, "Marcador eliminado: «Documentación Rust».");
        assert_eq!(copy.undo_button_label, "Deshacer");
        assert_eq!(
            copy.undo_aria_label,
            "Deshacer Marcador eliminado («Documentación Rust»)"
        );
        assert_eq!(copy.dismiss_aria_label, "Cerrar notificación");
    }

    #[test]
    fn state_machine_pauses_ticks_and_triggers_undo_once() {
        let copy = build_undo_toast_copy("Pestaña cerrada", None);
        let mut toast = UndoToastState::new("close-tab-42", copy, 5000);
        assert_eq!(toast.status, UndoToastStatus::Visible);
        assert_eq!(toast.remaining_seconds_ceil(), 5);

        toast.tick(1200);
        assert_eq!(toast.remaining_ms, 3800);
        assert_eq!(toast.remaining_seconds_ceil(), 4);

        toast.pause();
        toast.tick(5000);
        assert_eq!(toast.status, UndoToastStatus::Paused);
        assert_eq!(toast.remaining_ms, 3800);

        toast.resume();
        assert_eq!(toast.status, UndoToastStatus::Visible);

        assert!(toast.trigger_undo());
        assert_eq!(toast.status, UndoToastStatus::Undone);
        assert!(!toast.trigger_undo());
    }
}
