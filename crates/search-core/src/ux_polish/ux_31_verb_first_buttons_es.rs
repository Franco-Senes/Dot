//! UX-31: Verb-first, sentence-case Spanish button labels and icon-only accessible names.
//!
//! Enforces `better-writing` rules for Spanish (`es`) UI controls in Search Linux:
//! - Every button label starts with an action verb (Spanish infinitive) naming what happens.
//! - Sentence case is used consistently across all buttons (`Guardar borrador`, not `Guardar Borrador`).
//! - Generic or vague labels (`OK`, `OK!`, `Aceptar`, `Sí`, `No`, `Vamos`, `Listo`) are rejected.
//! - Icon-only buttons carry verb-first accessible names describing the action, never the glyph.

/// Specification metadata for the UX-31 verb-first Spanish button copy rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub locale: &'static str,
    pub capitalization_policy: &'static str,
    pub summary: &'static str,
    pub banned_generic_labels: &'static [&'static str],
    pub banned_glyph_prefixes: &'static [&'static str],
}

/// Visual and semantic role of a button in the interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Destructive,
    IconOnly,
}

/// Canonical Spanish button copy entry with its accessible name and replaced anti-pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonCopyEs {
    pub id: &'static str,
    pub kind: ButtonKind,
    pub label: &'static str,
    pub accessible_name: &'static str,
    pub replaces_anti_pattern: &'static str,
}

/// Reasons a Spanish button label or accessible name can fail UX-31 validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelValidationError {
    Empty,
    TrailingExclamation,
    GenericNonVerb(&'static str),
    DescribesGlyph(&'static str),
    NotSentenceCase,
    NotVerbFirst,
}

const BANNED_GENERIC_LABELS: &[&str] = &[
    "ok",
    "ok!",
    "aceptar",
    "sí",
    "si",
    "no",
    "listo",
    "vamos",
    "entendido",
    "de acuerdo",
    "continuar?",
];

const BANNED_GLYPH_PREFIXES: &[&str] = &[
    "icono ",
    "ícono ",
    "símbolo ",
    "simbolo ",
    "imagen ",
    "gráfico ",
    "grafico ",
    "botón de icono",
];

const ALLOWED_ACRONYMS: &[&str] = &["URL", "PDF", "HTML", "JSON", "HTTPS", "DNS", "CSV"];

/// Canonical Spanish verb-first, sentence-case button catalog for Search Linux.
pub const BUTTON_CATALOG_ES: &[ButtonCopyEs] = &[
    ButtonCopyEs {
        id: "send",
        kind: ButtonKind::Primary,
        label: "Enviar",
        accessible_name: "Enviar",
        replaces_anti_pattern: "OK!",
    },
    ButtonCopyEs {
        id: "save_draft",
        kind: ButtonKind::Primary,
        label: "Guardar borrador",
        accessible_name: "Guardar borrador",
        replaces_anti_pattern: "Guardar Borrador",
    },
    ButtonCopyEs {
        id: "delete_project",
        kind: ButtonKind::Destructive,
        label: "Eliminar proyecto",
        accessible_name: "Eliminar proyecto",
        replaces_anti_pattern: "Sí",
    },
    ButtonCopyEs {
        id: "cancel_action",
        kind: ButtonKind::Secondary,
        label: "Cancelar",
        accessible_name: "Cancelar",
        replaces_anti_pattern: "No",
    },
    ButtonCopyEs {
        id: "close_tabs",
        kind: ButtonKind::Destructive,
        label: "Cerrar pestañas",
        accessible_name: "Cerrar pestañas",
        replaces_anti_pattern: "Aceptar",
    },
    ButtonCopyEs {
        id: "clear_history",
        kind: ButtonKind::Destructive,
        label: "Borrar historial",
        accessible_name: "Borrar historial",
        replaces_anti_pattern: "Listo",
    },
    ButtonCopyEs {
        id: "open_new_tab",
        kind: ButtonKind::IconOnly,
        label: "",
        accessible_name: "Abrir pestaña nueva",
        replaces_anti_pattern: "Icono más",
    },
    ButtonCopyEs {
        id: "delete_bookmark",
        kind: ButtonKind::IconOnly,
        label: "",
        accessible_name: "Eliminar marcador",
        replaces_anti_pattern: "Icono de papelera",
    },
    ButtonCopyEs {
        id: "copy_link",
        kind: ButtonKind::IconOnly,
        label: "",
        accessible_name: "Copiar enlace",
        replaces_anti_pattern: "Icono de portapapeles",
    },
    ButtonCopyEs {
        id: "download_file",
        kind: ButtonKind::Primary,
        label: "Descargar archivo",
        accessible_name: "Descargar archivo",
        replaces_anti_pattern: "Descargar Archivo",
    },
    ButtonCopyEs {
        id: "restore_session",
        kind: ButtonKind::Primary,
        label: "Restaurar sesión",
        accessible_name: "Restaurar sesión",
        replaces_anti_pattern: "Vamos",
    },
    ButtonCopyEs {
        id: "retry_connection",
        kind: ButtonKind::Primary,
        label: "Reintentar conexión",
        accessible_name: "Reintentar conexión",
        replaces_anti_pattern: "OK",
    },
];

/// Returns the UX-31 rule specification.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_31_verb_first_buttons_es",
        title: "Verb-first sentence-case Spanish button labels",
        category: "writing",
        locale: "es",
        capitalization_policy: "sentence-case",
        summary: "Button labels and icon-only accessible names in Spanish start with an infinitive action verb in sentence case and never use generic confirmations or glyph descriptions.",
        banned_generic_labels: BANNED_GENERIC_LABELS,
        banned_glyph_prefixes: BANNED_GLYPH_PREFIXES,
    }
}

/// Looks up a canonical Spanish button entry by its identifier.
pub fn lookup_button_es(id: &str) -> Option<&'static ButtonCopyEs> {
    BUTTON_CATALOG_ES.iter().find(|entry| entry.id == id)
}

/// Formats a Spanish verb and object into a sentence-case, verb-first button label.
pub fn format_accessible_action_es(verb: &str, object: &str) -> String {
    let clean_verb = verb.trim();
    let clean_object = object.trim();
    let cap_verb = capitalize_first_es(clean_verb);
    if clean_object.is_empty() {
        return cap_verb;
    }
    let formatted_obj: Vec<String> = clean_object
        .split_whitespace()
        .map(|word| {
            if ALLOWED_ACRONYMS.contains(&word) {
                word.to_string()
            } else {
                word.to_lowercase()
            }
        })
        .collect();
    format!("{} {}", cap_verb, formatted_obj.join(" "))
}

/// Validates that a Spanish button label (or icon-only accessible name) is verb-first,
/// sentence-case, free of exclamation marks, and not a generic or glyph-describing label.
pub fn validate_button_label_es(
    label: &str,
    is_icon_only: bool,
) -> Result<(), LabelValidationError> {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return Err(LabelValidationError::Empty);
    }
    if trimmed.contains('!') || trimmed.contains('¡') {
        return Err(LabelValidationError::TrailingExclamation);
    }

    let lower = trimmed.to_lowercase();
    for &banned in BANNED_GENERIC_LABELS {
        if lower == banned {
            return Err(LabelValidationError::GenericNonVerb(banned));
        }
    }

    if is_icon_only {
        for &prefix in BANNED_GLYPH_PREFIXES {
            if lower.starts_with(prefix) {
                return Err(LabelValidationError::DescribesGlyph(prefix));
            }
        }
    }

    let mut words = trimmed.split_whitespace();
    let first_word = match words.next() {
        Some(w) => w,
        None => return Err(LabelValidationError::Empty),
    };

    let mut first_chars = first_word.chars();
    let starts_upper = matches!(first_chars.next(), Some(c) if c.is_uppercase());
    if !starts_upper {
        return Err(LabelValidationError::NotSentenceCase);
    }

    for word in words {
        if ALLOWED_ACRONYMS.contains(&word) {
            continue;
        }
        if let Some(c) = word.chars().next() {
            if c.is_uppercase() {
                return Err(LabelValidationError::NotSentenceCase);
            }
        }
    }

    if !is_spanish_infinitive_verb(&first_word.to_lowercase()) {
        return Err(LabelValidationError::NotVerbFirst);
    }

    Ok(())
}

fn capitalize_first_es(input: &str) -> String {
    let mut chars = input.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let upper: String = first.to_uppercase().collect();
            let rest: String = chars.as_str().to_lowercase();
            format!("{}{}", upper, rest)
        }
    }
}

fn is_spanish_infinitive_verb(word: &str) -> bool {
    let clean = word.trim_matches(|c: char| !c.is_alphabetic());
    if clean.len() < 4 {
        return false;
    }
    // Spanish infinitives end in -ar, -er, -ir (or reflexive -arse, -erse, -irse)
    clean.ends_with("ar")
        || clean.ends_with("er")
        || clean.ends_with("ir")
        || clean.ends_with("arse")
        || clean.ends_with("erse")
        || clean.ends_with("irse")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_matches_ux_31_requirements() {
        let s = spec();
        assert_eq!(s.id, "ux_31_verb_first_buttons_es");
        assert_eq!(s.locale, "es");
        assert_eq!(s.capitalization_policy, "sentence-case");
        assert!(s.banned_generic_labels.contains(&"ok!"));
        assert!(s.banned_generic_labels.contains(&"aceptar"));
        assert!(s.banned_glyph_prefixes.contains(&"icono "));
    }

    #[test]
    fn catalog_entries_all_pass_validation() {
        assert!(!BUTTON_CATALOG_ES.is_empty());
        for entry in BUTTON_CATALOG_ES {
            let is_icon = entry.kind == ButtonKind::IconOnly;
            if !is_icon {
                assert_eq!(
                    validate_button_label_es(entry.label, false),
                    Ok(()),
                    "visible label failed for {}",
                    entry.id
                );
            } else {
                assert!(
                    entry.label.is_empty(),
                    "icon-only button {} should not carry visible label",
                    entry.id
                );
            }
            assert_eq!(
                validate_button_label_es(entry.accessible_name, is_icon),
                Ok(()),
                "accessible name failed for {}",
                entry.id
            );
        }
    }

    #[test]
    fn rejects_generic_title_case_glyph_and_exclamation_anti_patterns() {
        assert_eq!(
            validate_button_label_es("OK!", false),
            Err(LabelValidationError::TrailingExclamation)
        );
        assert_eq!(
            validate_button_label_es("Aceptar", false),
            Err(LabelValidationError::GenericNonVerb("aceptar"))
        );
        assert_eq!(
            validate_button_label_es("Sí", false),
            Err(LabelValidationError::GenericNonVerb("sí"))
        );
        assert_eq!(
            validate_button_label_es("Guardar Borrador", false),
            Err(LabelValidationError::NotSentenceCase)
        );
        assert_eq!(
            validate_button_label_es("Icono de papelera", true),
            Err(LabelValidationError::DescribesGlyph("icono "))
        );
        assert_eq!(
            validate_button_label_es("Proyecto nuevo", false),
            Err(LabelValidationError::NotVerbFirst)
        );
    }

    #[test]
    fn format_accessible_action_preserves_sentence_case_and_acronyms() {
        assert_eq!(
            format_accessible_action_es("copiar", "URL Actual"),
            "Copiar URL actual"
        );
        assert_eq!(
            format_accessible_action_es("exportar", "Marcadores en JSON"),
            "Exportar marcadores en JSON"
        );
        assert_eq!(
            lookup_button_es("delete_bookmark").map(|b| b.accessible_name),
            Some("Eliminar marcador")
        );
    }
}
