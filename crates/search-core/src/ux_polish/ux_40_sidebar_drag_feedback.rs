//! UX-40: Sidebar drag-and-drop visual and accessible feedback (`ux_40_sidebar_drag_feedback`).
//!
//! Provides exact motion, surface, static-cue, and Spanish accessibility copy
//! specifications for dragging and reordering tabs or favorite tiles in the
//! Search Linux sidebar (`sidebar_tabs.rs`).
//!
//! Rules enforced:
//! - Transitions only changed properties (`opacity`, `transform`, `border-color`,
//!   `background-color`), never `transition: all`.
//! - Exact easing `cubic-bezier(0.2, 0, 0, 1)` and `0.96` pickup/press scale.
//! - High-frequency drag-over feedback responds within `150ms` and always pairs
//!   motion with a static cue (accent border + dimmed source row).
//! - Gates transform/scale travel behind `prefers-reduced-motion` while keeping
//!   static color/border cues intact.
//! - Spanish sentence-case, verb-first accessible labels and live announcements.

/// Standard easing curve for UI state transitions.
pub const EASING_STANDARD: &str = "cubic-bezier(0.2, 0, 0, 1)";

/// Scale applied when a sidebar item is pressed or picked up for drag.
pub const DRAG_PRESS_SCALE: f32 = 0.96;

/// Duration in milliseconds for high-frequency drag hover/drop-indicator transitions.
pub const FEEDBACK_DURATION_MS: u32 = 150;

/// Minimum pointer travel in pixels before a press becomes a drag gesture.
pub const DRAG_THRESHOLD_PX: f64 = 6.0;

/// Opacity of the original sidebar row while its drag preview is in flight.
pub const DRAG_SOURCE_OPACITY: f32 = 0.45;

/// Thickness in pixels of the drop-target insertion line.
pub const DROP_INDICATOR_THICKNESS_PX: u32 = 2;

/// CSS class applied to the sidebar row currently being dragged.
pub const CLASS_DRAG_SOURCE: &str = "search-sidebar-row-dragging";

/// CSS class applied when a drop target will insert before the hovered row.
pub const CLASS_DROP_BEFORE: &str = "search-sidebar-drop-before";

/// CSS class applied when a drop target will insert after the hovered row.
pub const CLASS_DROP_AFTER: &str = "search-sidebar-drop-after";

/// Canonical CSS fragment for sidebar drag-and-drop feedback.
///
/// Uses explicit `transition-property` (never `all`), `border: 1px solid transparent`
/// so forced-colors mode preserves boundaries, and a `prefers-reduced-motion: reduce`
/// fallback that removes transform travel while retaining static cues.
pub const CSS_SIDEBAR_DRAG_FEEDBACK: &str = "\
.search-sidebar-row {\n\
  border: 1px solid transparent;\n\
  transition-property: opacity, transform, border-color, background-color;\n\
  transition-duration: 150ms;\n\
  transition-timing-function: cubic-bezier(0.2, 0, 0, 1);\n\
}\n\
.search-sidebar-row:active:not(:disabled) {\n\
  transform: scale(0.96);\n\
}\n\
.search-sidebar-row-dragging {\n\
  opacity: 0.45;\n\
  transform: scale(0.96);\n\
}\n\
.search-sidebar-drop-before {\n\
  border-top: 2px solid @accent_bg_color;\n\
  background-color: alpha(@accent_bg_color, 0.08);\n\
}\n\
.search-sidebar-drop-after {\n\
  border-bottom: 2px solid @accent_bg_color;\n\
  background-color: alpha(@accent_bg_color, 0.08);\n\
}\n\
@media (prefers-reduced-motion: reduce) {\n\
  .search-sidebar-row {\n\
    transition-property: opacity, border-color, background-color;\n\
    transition-duration: 150ms;\n\
  }\n\
  .search-sidebar-row-dragging {\n\
    transform: none;\n\
  }\n\
}";

/// Reduced-motion CSS override when motion preferences disable transform travel.
pub const CSS_SIDEBAR_DRAG_REDUCED_MOTION: &str = "\
.search-sidebar-row {\n\
  transition-property: opacity, border-color, background-color;\n\
  transition-duration: 150ms;\n\
}\n\
.search-sidebar-row-dragging {\n\
  opacity: 0.45;\n\
  transform: none;\n\
}";

/// Relative insertion position when hovering over a sidebar drop target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropPosition {
    /// Insert above / before the target item.
    Before,
    /// Insert below / after the target item.
    After,
}

impl DropPosition {
    /// Returns the CSS class associated with this drop position.
    pub const fn css_class(self) -> &'static str {
        match self {
            Self::Before => CLASS_DROP_BEFORE,
            Self::After => CLASS_DROP_AFTER,
        }
    }
}

/// Lifecycle state for a sidebar drag-and-drop interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragFeedbackState {
    /// No drag gesture in progress.
    Idle,
    /// Pointer pressed; waiting to exceed `DRAG_THRESHOLD_PX`.
    Pending,
    /// Drag active; source row is dimmed (`DRAG_SOURCE_OPACITY`).
    Dragging,
    /// Drag active and hovering over a valid drop target.
    OverTarget(DropPosition),
}

/// Spanish sentence-case, verb-first accessible copy for sidebar drag interactions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragCopyEs {
    pub action_move_tab: &'static str,
    pub action_move_favorite: &'static str,
    pub hint_reorder: &'static str,
    pub status_cancelled: &'static str,
}

/// Canonical Spanish copy constants (sentence case, verb-first, no exclamation marks).
pub const COPY_ES: DragCopyEs = DragCopyEs {
    action_move_tab: "Mover pestaña",
    action_move_favorite: "Mover favorito",
    hint_reorder: "Arrastra para reordenar o usa Alt y flechas",
    status_cancelled: "Reordenamiento cancelado",
};

/// Specification for UX rule 40: sidebar drag-and-drop feedback.
#[derive(Debug, Clone, PartialEq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub easing: &'static str,
    pub press_scale: f32,
    pub feedback_duration_ms: u32,
    pub drag_threshold_px: f64,
    pub drag_source_opacity: f32,
    pub drop_indicator_thickness_px: u32,
    pub css_fragment: &'static str,
    pub copy_es: DragCopyEs,
    pub summary_es: &'static str,
}

/// Returns the canonical specification for `ux_40_sidebar_drag_feedback`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_40_sidebar_drag_feedback",
        title: "Sidebar drag-and-drop visual and accessible feedback",
        easing: EASING_STANDARD,
        press_scale: DRAG_PRESS_SCALE,
        feedback_duration_ms: FEEDBACK_DURATION_MS,
        drag_threshold_px: DRAG_THRESHOLD_PX,
        drag_source_opacity: DRAG_SOURCE_OPACITY,
        drop_indicator_thickness_px: DROP_INDICATOR_THICKNESS_PX,
        css_fragment: CSS_SIDEBAR_DRAG_FEEDBACK,
        copy_es: COPY_ES,
        summary_es: "Mostrar origen atenuado, línea de inserción de 2 px y avisos accesibles al arrastrar en la barra lateral.",
    }
}

/// Returns `true` when pointer displacement exceeds `DRAG_THRESHOLD_PX`.
pub fn should_start_drag(dx: f64, dy: f64) -> bool {
    if !dx.is_finite() || !dy.is_finite() {
        return false;
    }
    (dx * dx + dy * dy) >= (DRAG_THRESHOLD_PX * DRAG_THRESHOLD_PX)
}

/// Computes whether a pointer at `pointer_y` inside a row of `row_height`
/// targets `DropPosition::Before` or `DropPosition::After`.
///
/// Defaults to `DropPosition::Before` for non-finite or non-positive heights.
pub fn compute_drop_position(pointer_y: f64, row_height: f64) -> DropPosition {
    if !pointer_y.is_finite() || !row_height.is_finite() || row_height <= 0.0 {
        return DropPosition::Before;
    }
    if pointer_y < (row_height * 0.5) {
        DropPosition::Before
    } else {
        DropPosition::After
    }
}

/// Returns the appropriate CSS fragment based on the user's reduced-motion setting.
pub const fn css_for_motion_preference(reduced_motion: bool) -> &'static str {
    if reduced_motion {
        CSS_SIDEBAR_DRAG_REDUCED_MOTION
    } else {
        CSS_SIDEBAR_DRAG_FEEDBACK
    }
}

/// Formats a complete Spanish screen-reader announcement for a sidebar drop target.
pub fn format_drop_announcement_es(
    source_title: &str,
    target_title: &str,
    pos: DropPosition,
) -> String {
    let src = source_title.trim();
    let dst = target_title.trim();
    match pos {
        DropPosition::Before => format!("Soltar {src} antes de {dst}"),
        DropPosition::After => format!("Soltar {src} después de {dst}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_exposes_exact_ui_and_copy_values() {
        let s = spec();
        assert_eq!(s.id, "ux_40_sidebar_drag_feedback");
        assert_eq!(s.easing, "cubic-bezier(0.2, 0, 0, 1)");
        assert!((s.press_scale - 0.96).abs() < f32::EPSILON);
        assert_eq!(s.feedback_duration_ms, 150);
        assert_eq!(s.drop_indicator_thickness_px, 2);
        assert!(s.css_fragment.contains("cubic-bezier(0.2, 0, 0, 1)"));
        assert!(s.css_fragment.contains("scale(0.96)"));
        assert!(s.css_fragment.contains("prefers-reduced-motion: reduce"));
        assert!(!s.css_fragment.contains("transition: all"));
        assert!(!s.css_fragment.contains("transition-property: all"));
        assert_eq!(s.copy_es.action_move_tab, "Mover pestaña");
        assert_eq!(s.copy_es.status_cancelled, "Reordenamiento cancelado");
    }

    #[test]
    fn drag_threshold_and_drop_position_behave_predictably() {
        assert!(!should_start_drag(2.0, 2.0));
        assert!(should_start_drag(6.0, 0.0));
        assert!(should_start_drag(4.0, 5.0));
        assert!(!should_start_drag(f64::NAN, 10.0));

        assert_eq!(compute_drop_position(10.0, 40.0), DropPosition::Before);
        assert_eq!(compute_drop_position(25.0, 40.0), DropPosition::After);
        assert_eq!(compute_drop_position(0.0, 0.0), DropPosition::Before);

        assert_eq!(
            DropPosition::Before.css_class(),
            "search-sidebar-drop-before"
        );
        assert_eq!(DropPosition::After.css_class(), "search-sidebar-drop-after");
    }

    #[test]
    fn announcements_and_reduced_motion_css_are_well_formed() {
        assert_eq!(
            format_drop_announcement_es("Inicio", "Ajustes", DropPosition::Before),
            "Soltar Inicio antes de Ajustes"
        );
        assert_eq!(
            format_drop_announcement_es("Inicio", "Ajustes", DropPosition::After),
            "Soltar Inicio después de Ajustes"
        );

        let reduced = css_for_motion_preference(true);
        assert!(reduced.contains("transform: none"));
        assert!(!reduced.contains("scale(0.96)"));

        let full = css_for_motion_preference(false);
        assert!(full.contains("scale(0.96)"));
    }
}
