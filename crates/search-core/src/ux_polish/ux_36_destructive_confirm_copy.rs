//! UX polish specification for slice `ux_36_destructive_confirm_copy`.
//!
//! Defines Spanish sentence-case, verb-first copy patterns, validation rules,
//! ARIA attributes, and CSS fragments for destructive confirmations and undo
//! toasts in Search Linux.

/// Top-level specification metadata for this UX polish rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UxRuleSpec {
    /// Stable identifier for this UX polish slice.
    pub id: &'static str,
    /// Short human-readable title of the rule.
    pub title: &'static str,
    /// Primary UX domain covered by this rule.
    pub category: &'static str,
    /// Concise summary of the enforced behavior.
    pub summary: &'static str,
    /// Whether this rule applies to Spanish interface copy.
    pub locale: &'static str,
    /// Motion easing curve used for dialog/toast transitions.
    pub easing: &'static str,
    /// Active press scale applied to enabled confirmation buttons.
    pub press_scale: &'static str,
}

/// Returns the canonical specification for `ux_36_destructive_confirm_copy`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_36_destructive_confirm_copy",
        title: "Copy de confirmacion destructiva en espanol",
        category: "better-writing",
        summary: "Titulos con accion y objeto, cuerpo con consecuencias irreversibles, botones verbales en sentence case y deshacer para acciones reversibles.",
        locale: "es",
        easing: "cubic-bezier(0.2, 0, 0, 1)",
        press_scale: "0.96",
    }
}

/// Reversibility and scope of a destructive user action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestructiveScope {
    /// Reversible, frequent action: act immediately and offer an undo toast.
    ReversibleToast,
    /// Irreversible action affecting a single named item.
    IrreversibleSingle,
    /// Irreversible action affecting multiple items at once.
    IrreversibleBatch,
    /// High-stakes irreversible action on a container (profile, vault, workspace)
    /// that requires typing the object's name to enable confirmation.
    IrreversibleTypedConfirm,
}

/// Structured Spanish copy for a destructive confirmation dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestructiveDialogCopy {
    /// Dialog heading naming the action and target object.
    pub title: String,
    /// Body explaining what is lost and that the action cannot be undone.
    pub body: String,
    /// Sentence-case, verb-first label for the destructive action button.
    pub confirm_label: String,
    /// Label for the safe cancellation button.
    pub cancel_label: String,
    /// Optional typed-confirmation field label for high-stakes containers.
    pub typed_confirm_label: Option<String>,
    /// Required ARIA role for destructive confirmation dialogs.
    pub aria_role: &'static str,
}

/// Structured Spanish copy for an immediate-action undo toast.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoToastCopy {
    /// Past-tense status message without filler words like "exitosamente".
    pub message: String,
    /// Verb-first undo action label.
    pub undo_label: String,
}

/// Exact CSS fragment for destructive confirmation dialogs and buttons.
pub const DESTRUCTIVE_DIALOG_CSS: &str = "\
.sl-destructive-dialog {\n\
  overscroll-behavior: contain;\n\
  transition-property: opacity, transform;\n\
  transition-duration: 150ms;\n\
  transition-timing-function: cubic-bezier(0.2, 0, 0, 1);\n\
}\n\
.sl-destructive-confirm:not(:disabled):active {\n\
  transform: scale(0.96);\n\
  transition-property: transform;\n\
  transition-duration: 150ms;\n\
  transition-timing-function: ease-out;\n\
}\n";

/// Forbidden vague or playful phrases in destructive confirmation copy.
pub const FORBIDDEN_CONFIRM_PHRASES: &[&str] = &[
    "estas seguro",
    "estás seguro",
    "are you sure",
    "ups",
    "oops",
    "exitosamente",
    "por favor",
];

/// Forbidden generic button labels that lack a verb and object.
pub const FORBIDDEN_BUTTON_LABELS: &[&str] =
    &["si", "sí", "no", "ok", "aceptar", "confirmar", "yes"];

/// Builds Spanish confirmation copy for deleting a single named item.
///
/// Uses sentence case, names both the action and the object in the title,
/// states the irreversible consequence in the body, and uses a verb-first
/// confirm button label.
pub fn single_item_delete_copy(
    object_kind: &str,
    item_name: &str,
    consequence: &str,
) -> DestructiveDialogCopy {
    let clean_kind = object_kind.trim();
    let clean_name = item_name.trim();
    let clean_consequence = consequence.trim();

    let title = format!("¿Eliminar «{clean_name}»?");
    let body = if clean_consequence.is_empty() {
        "Esta acción no se puede deshacer.".to_string()
    } else {
        format!("{clean_consequence} Esta acción no se puede deshacer.")
    };
    let confirm_label = if clean_kind.is_empty() {
        "Eliminar elemento".to_string()
    } else {
        format!("Eliminar {clean_kind}")
    };

    DestructiveDialogCopy {
        title,
        body,
        confirm_label,
        cancel_label: "Cancelar".to_string(),
        typed_confirm_label: None,
        aria_role: "alertdialog",
    }
}

/// Builds Spanish confirmation copy for deleting multiple items at once.
pub fn batch_delete_copy(
    count: usize,
    plural_noun: &str,
    singular_noun: &str,
) -> DestructiveDialogCopy {
    let noun = if count == 1 {
        singular_noun.trim()
    } else {
        plural_noun.trim()
    };

    DestructiveDialogCopy {
        title: format!("¿Eliminar {count} {noun}?"),
        body: "Esta acción no se puede deshacer.".to_string(),
        confirm_label: format!("Eliminar {count} {noun}"),
        cancel_label: "Cancelar".to_string(),
        typed_confirm_label: None,
        aria_role: "alertdialog",
    }
}

/// Builds Spanish confirmation copy for high-stakes containers (e.g. profile, vault)
/// requiring the user to type the container's exact name to confirm.
pub fn typed_confirm_delete_copy(
    container_kind: &str,
    container_name: &str,
    consequence: &str,
) -> DestructiveDialogCopy {
    let mut base = single_item_delete_copy(container_kind, container_name, consequence);
    base.typed_confirm_label = Some(format!(
        "Escribe «{}» para confirmar",
        container_name.trim()
    ));
    base
}

/// Builds Spanish copy for a reversible action toast with an Undo action.
pub fn undo_toast_copy(past_tense_summary: &str) -> UndoToastCopy {
    UndoToastCopy {
        message: past_tense_summary.trim().to_string(),
        undo_label: "Deshacer".to_string(),
    }
}

/// Returns `true` when the user's typed confirmation exactly matches the target name.
pub fn matches_typed_confirmation(expected_name: &str, user_input: &str) -> bool {
    !expected_name.is_empty() && expected_name == user_input
}

/// Validates a `DestructiveDialogCopy` against tone, clarity, and button rules.
///
/// Returns a list of violation descriptions; an empty vector means the copy passes.
pub fn validate_dialog_copy(copy: &DestructiveDialogCopy) -> Vec<&'static str> {
    let mut violations = Vec::new();
    let title_lower = copy.title.to_lowercase();
    let body_lower = copy.body.to_lowercase();
    let confirm_lower = copy.confirm_label.trim().to_lowercase();

    for phrase in FORBIDDEN_CONFIRM_PHRASES {
        if title_lower.contains(phrase) || body_lower.contains(phrase) {
            violations.push("El titulo o cuerpo contiene una frase prohibida o vaga.");
            break;
        }
    }

    if copy.title.contains('!') || copy.body.contains('!') {
        violations.push("El copy destructivo no debe usar signos de exclamacion.");
    }

    if FORBIDDEN_BUTTON_LABELS.contains(&confirm_lower.as_str()) {
        violations.push("El boton de confirmacion debe empezar por verbo y nombrar el objeto.");
    }

    if copy.confirm_label.split_whitespace().count() < 2 {
        violations.push("El boton de confirmacion debe incluir verbo y objeto.");
    }

    if copy.aria_role != "alertdialog" {
        violations.push("El dialogo destructivo debe usar role=\"alertdialog\".");
    }

    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_matches_exact_ux_tokens() {
        let s = spec();
        assert_eq!(s.id, "ux_36_destructive_confirm_copy");
        assert_eq!(s.locale, "es");
        assert_eq!(s.easing, "cubic-bezier(0.2, 0, 0, 1)");
        assert_eq!(s.press_scale, "0.96");
        assert!(DESTRUCTIVE_DIALOG_CSS.contains("cubic-bezier(0.2, 0, 0, 1)"));
        assert!(DESTRUCTIVE_DIALOG_CSS.contains("scale(0.96)"));
        assert!(DESTRUCTIVE_DIALOG_CSS.contains("overscroll-behavior: contain"));
    }

    #[test]
    fn single_and_batch_copy_follow_spanish_verb_first_rules() {
        let single = single_item_delete_copy(
            "marcador",
            "Documentacion de Rust",
            "Se perderan sus etiquetas asociadas.",
        );
        assert_eq!(single.title, "¿Eliminar «Documentacion de Rust»?");
        assert_eq!(
            single.body,
            "Se perderan sus etiquetas asociadas. Esta acción no se puede deshacer."
        );
        assert_eq!(single.confirm_label, "Eliminar marcador");
        assert_eq!(single.cancel_label, "Cancelar");
        assert!(validate_dialog_copy(&single).is_empty());

        let batch = batch_delete_copy(12, "elementos", "elemento");
        assert_eq!(batch.title, "¿Eliminar 12 elementos?");
        assert_eq!(batch.confirm_label, "Eliminar 12 elementos");
        assert!(validate_dialog_copy(&batch).is_empty());
    }

    #[test]
    fn typed_confirmation_and_validation_catch_antipatterns() {
        let vault = typed_confirm_delete_copy(
            "boveda",
            "Trabajo",
            "Se eliminaran todas las credenciales guardadas.",
        );
        assert_eq!(
            vault.typed_confirm_label.as_deref(),
            Some("Escribe «Trabajo» para confirmar")
        );
        assert!(matches_typed_confirmation("Trabajo", "Trabajo"));
        assert!(!matches_typed_confirmation("Trabajo", "trabajo"));
        assert!(validate_dialog_copy(&vault).is_empty());

        let bad = DestructiveDialogCopy {
            title: "¿Estás seguro?".to_string(),
            body: "¡Ups! Se borro todo.".to_string(),
            confirm_label: "Sí".to_string(),
            cancel_label: "No".to_string(),
            typed_confirm_label: None,
            aria_role: "dialog",
        };
        let errors = validate_dialog_copy(&bad);
        assert_eq!(errors.len(), 5);

        let toast = undo_toast_copy("Pestaña cerrada");
        assert_eq!(toast.message, "Pestaña cerrada");
        assert_eq!(toast.undo_label, "Deshacer");
    }
}
