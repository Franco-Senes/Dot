//! UX-26: One primary action per view.
//!
//! Each view exposes at most one primary action and up to three directly
//! visible secondary actions. Any additional actions collapse into an overflow
//! menu (`"Más acciones"`), keeping top-level views scannable and focused.

/// Maximum number of primary actions allowed in a single view.
pub const MAX_PRIMARY_ACTIONS: usize = 1;

/// Maximum number of secondary actions shown inline before collapsing into a menu.
pub const MAX_VISIBLE_SECONDARY_ACTIONS: usize = 3;

/// Spanish sentence-case accessible label for the overflow actions menu trigger.
pub const OVERFLOW_MENU_LABEL_ES: &str = "Más acciones";

/// Standard easing curve for interactive action transitions.
pub const ACTION_EASING: &str = "cubic-bezier(0.2, 0, 0, 1)";

/// Press scale factor applied to enabled action buttons.
pub const ACTION_PRESS_SCALE: f32 = 0.96;

/// CSS rules enforcing primary/secondary/overflow action hierarchy and polish.
pub const VIEW_ACTIONS_CSS: &str = r#".ux-view-actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px;
}

.ux-action-primary,
.ux-action-secondary,
.ux-action-overflow-trigger {
  min-height: 32px;
  padding-inline: 12px;
  border-radius: 8px;
  transition-property: background-color, opacity, scale;
  transition-duration: 150ms;
  transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
  -webkit-tap-highlight-color: transparent;
}

.ux-action-primary:not(:disabled):active,
.ux-action-secondary:not(:disabled):active,
.ux-action-overflow-trigger:not(:disabled):active {
  scale: 0.96;
}

@media (hover: hover) {
  .ux-action-secondary:not(:disabled):hover,
  .ux-action-overflow-trigger:not(:disabled):hover {
    background-color: oklch(0 0 0 / 0.06);
  }
}
"#;

/// Specification metadata for UX rule 26.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub summary: &'static str,
    pub max_primary_actions: usize,
    pub max_visible_secondary_actions: usize,
    pub overflow_menu_label_es: &'static str,
    pub press_scale: f32,
    pub easing: &'static str,
    pub css: &'static str,
}

/// Returns the static specification for UX-26.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_26_one_primary_action_view",
        title: "One primary action per view",
        category: "layout",
        summary: "Allow exactly one primary action per view and collapse more than three secondary actions into an overflow menu.",
        max_primary_actions: MAX_PRIMARY_ACTIONS,
        max_visible_secondary_actions: MAX_VISIBLE_SECONDARY_ACTIONS,
        overflow_menu_label_es: OVERFLOW_MENU_LABEL_ES,
        press_scale: ACTION_PRESS_SCALE,
        easing: ACTION_EASING,
        css: VIEW_ACTIONS_CSS,
    }
}

/// Visual and semantic tier of an action within a view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionTier {
    Primary,
    Secondary,
    Destructive,
}

/// Descriptor for a single action in a view's action bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewAction<'a> {
    pub id: &'a str,
    pub label_es: &'a str,
    pub tier: ActionTier,
}

/// Result of partitioning view actions according to UX-26.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionedActions<'a> {
    pub primary: ViewAction<'a>,
    pub visible_secondary: Vec<ViewAction<'a>>,
    pub overflow_menu: Vec<ViewAction<'a>>,
}

/// Errors produced when a view violates the single-primary-action hierarchy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionHierarchyError {
    MissingPrimaryAction,
    MultiplePrimaryActions { count: usize },
    EmptyLabel { id: String },
}

/// Validates and partitions a view's actions into one primary action, at most
/// three visible secondary actions, and an overflow menu for the rest.
pub fn partition_view_actions<'a>(
    actions: &[ViewAction<'a>],
) -> Result<PartitionedActions<'a>, ActionHierarchyError> {
    let mut primary: Option<ViewAction<'a>> = None;
    let mut primary_count: usize = 0;
    let mut non_primary: Vec<ViewAction<'a>> = Vec::new();

    for &action in actions {
        if action.label_es.trim().is_empty() {
            return Err(ActionHierarchyError::EmptyLabel {
                id: action.id.to_string(),
            });
        }
        match action.tier {
            ActionTier::Primary => {
                primary_count += 1;
                if primary.is_none() {
                    primary = Some(action);
                }
            }
            ActionTier::Secondary | ActionTier::Destructive => {
                non_primary.push(action);
            }
        }
    }

    if primary_count > MAX_PRIMARY_ACTIONS {
        return Err(ActionHierarchyError::MultiplePrimaryActions {
            count: primary_count,
        });
    }

    let primary = primary.ok_or(ActionHierarchyError::MissingPrimaryAction)?;

    let split_at = non_primary.len().min(MAX_VISIBLE_SECONDARY_ACTIONS);
    let overflow_menu = non_primary.split_off(split_at);

    Ok(PartitionedActions {
        primary,
        visible_secondary: non_primary,
        overflow_menu,
    })
}

/// Returns ARIA attributes for the overflow menu trigger button.
pub fn overflow_menu_aria_attributes(expanded: bool) -> [(&'static str, &'static str); 3] {
    [
        ("aria-haspopup", "menu"),
        ("aria-expanded", if expanded { "true" } else { "false" }),
        ("aria-label", OVERFLOW_MENU_LABEL_ES),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_exposes_exact_thresholds_and_tokens() {
        let s = spec();
        assert_eq!(s.id, "ux_26_one_primary_action_view");
        assert_eq!(s.max_primary_actions, 1);
        assert_eq!(s.max_visible_secondary_actions, 3);
        assert_eq!(s.overflow_menu_label_es, "Más acciones");
        assert!((s.press_scale - 0.96).abs() < f32::EPSILON);
        assert_eq!(s.easing, "cubic-bezier(0.2, 0, 0, 1)");
        assert!(s.css.contains("scale: 0.96;"));
        assert!(s.css.contains("@media (hover: hover)"));
    }

    #[test]
    fn partitions_one_primary_three_visible_and_overflow() {
        let actions = [
            ViewAction {
                id: "save",
                label_es: "Guardar cambios",
                tier: ActionTier::Primary,
            },
            ViewAction {
                id: "preview",
                label_es: "Previsualizar",
                tier: ActionTier::Secondary,
            },
            ViewAction {
                id: "duplicate",
                label_es: "Duplicar",
                tier: ActionTier::Secondary,
            },
            ViewAction {
                id: "export",
                label_es: "Exportar",
                tier: ActionTier::Secondary,
            },
            ViewAction {
                id: "delete",
                label_es: "Eliminar",
                tier: ActionTier::Destructive,
            },
        ];

        let partitioned = partition_view_actions(&actions).expect("valid action hierarchy");
        assert_eq!(partitioned.primary.id, "save");
        assert_eq!(partitioned.visible_secondary.len(), 3);
        assert_eq!(partitioned.overflow_menu.len(), 1);
        assert_eq!(partitioned.overflow_menu[0].id, "delete");
    }

    #[test]
    fn rejects_zero_or_multiple_primary_actions() {
        let no_primary = [ViewAction {
            id: "cancel",
            label_es: "Cancelar",
            tier: ActionTier::Secondary,
        }];
        assert_eq!(
            partition_view_actions(&no_primary),
            Err(ActionHierarchyError::MissingPrimaryAction)
        );

        let two_primaries = [
            ViewAction {
                id: "save",
                label_es: "Guardar",
                tier: ActionTier::Primary,
            },
            ViewAction {
                id: "publish",
                label_es: "Publicar",
                tier: ActionTier::Primary,
            },
        ];
        assert_eq!(
            partition_view_actions(&two_primaries),
            Err(ActionHierarchyError::MultiplePrimaryActions { count: 2 })
        );
    }

    #[test]
    fn overflow_aria_attributes_reflect_expanded_state() {
        assert_eq!(
            overflow_menu_aria_attributes(false),
            [
                ("aria-haspopup", "menu"),
                ("aria-expanded", "false"),
                ("aria-label", "Más acciones"),
            ]
        );
        assert_eq!(
            overflow_menu_aria_attributes(true)[1],
            ("aria-expanded", "true")
        );
    }
}
