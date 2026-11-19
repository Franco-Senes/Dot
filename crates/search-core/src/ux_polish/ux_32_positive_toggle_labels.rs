//! UX-32: Positive ON-state toggle labels.
//!
//! Toggles and switches must describe what happens when the setting is ON.
//! Negative phrasing ("Don't send...", "Desactivar...", "Ocultar...") creates
//! double negatives when the user turns the switch off.

/// Specification metadata for a UX polish rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub category: &'static str,
    pub title: &'static str,
    pub rule: &'static str,
    pub rationale: &'static str,
    pub aria_pattern: &'static str,
}

/// Supported UI copy languages for toggle validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleLanguage {
    Spanish,
    English,
}

/// Validation failure for a toggle label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelIssue {
    Empty,
    NegativePrefix(&'static str),
    NotSentenceCase,
    TrailingPunctuation,
}

/// Canonical toggle copy mapping for Search Linux settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToggleCopyEntry {
    pub setting_key: &'static str,
    pub positive_label_es: &'static str,
    pub positive_label_en: &'static str,
    pub negative_anti_pattern: &'static str,
}

/// Negative prefixes that turn a toggle into a double negative in Spanish.
pub const FORBIDDEN_PREFIXES_ES: &[&str] = &[
    "no ",
    "desactivar ",
    "deshabilitar ",
    "ocultar ",
    "nunca ",
    "evitar ",
    "impedir ",
    "apagar ",
    "omitir ",
    "dejar de ",
];

/// Negative prefixes that turn a toggle into a double negative in English.
pub const FORBIDDEN_PREFIXES_EN: &[&str] = &[
    "don't ",
    "do not ",
    "disable ",
    "hide ",
    "never ",
    "turn off ",
    "stop ",
    "prevent ",
    "skip ",
    "omit ",
];

/// Canonical positive ON-state toggle labels for Search Linux `AppSettings`.
pub const SEARCH_LINUX_TOGGLES: &[ToggleCopyEntry] = &[
    ToggleCopyEntry {
        setting_key: "restore_session",
        positive_label_es: "Restaurar la sesión anterior al iniciar",
        positive_label_en: "Restore previous session on startup",
        negative_anti_pattern: "No iniciar con pestañas vacías",
    },
    ToggleCopyEntry {
        setting_key: "confirm_quit",
        positive_label_es: "Confirmar antes de cerrar la aplicación",
        positive_label_en: "Confirm before quitting",
        negative_anti_pattern: "No cerrar sin preguntar",
    },
    ToggleCopyEntry {
        setting_key: "tab_show_domain",
        positive_label_es: "Mostrar el dominio en cada pestaña",
        positive_label_en: "Show domain on each tab",
        negative_anti_pattern: "Ocultar dominio en pestañas",
    },
    ToggleCopyEntry {
        setting_key: "protect_pinned",
        positive_label_es: "Proteger las pestañas fijadas al cerrar",
        positive_label_en: "Protect pinned tabs from closing",
        negative_anti_pattern: "Evitar cierre de pestañas fijadas",
    },
    ToggleCopyEntry {
        setting_key: "show_audio_indicators",
        positive_label_es: "Mostrar indicadores de audio en pestañas",
        positive_label_en: "Show audio indicators on tabs",
        negative_anti_pattern: "Ocultar iconos de audio",
    },
    ToggleCopyEntry {
        setting_key: "downloads_ask",
        positive_label_es: "Preguntar dónde guardar cada descarga",
        positive_label_en: "Ask where to save each download",
        negative_anti_pattern: "No guardar automáticamente",
    },
    ToggleCopyEntry {
        setting_key: "shield_enabled",
        positive_label_es: "Activar el escudo de privacidad",
        positive_label_en: "Enable privacy shield",
        negative_anti_pattern: "Desactivar rastreadores",
    },
    ToggleCopyEntry {
        setting_key: "send_gpc",
        positive_label_es: "Enviar señal GPC de privacidad",
        positive_label_en: "Send GPC privacy signal",
        negative_anti_pattern: "No permitir venta de datos",
    },
    ToggleCopyEntry {
        setting_key: "strip_click_ids",
        positive_label_es: "Quitar identificadores de seguimiento en enlaces",
        positive_label_en: "Strip tracking click IDs from links",
        negative_anti_pattern: "Omitir parámetros de rastreo",
    },
    ToggleCopyEntry {
        setting_key: "vault_save",
        positive_label_es: "Guardar contraseñas nuevas en el baúl",
        positive_label_en: "Save new passwords to vault",
        negative_anti_pattern: "Nunca guardar contraseñas",
    },
    ToggleCopyEntry {
        setting_key: "vault_fill",
        positive_label_es: "Completar contraseñas automáticamente",
        positive_label_en: "Autofill saved passwords",
        negative_anti_pattern: "Desactivar autocompletado",
    },
];

/// Returns the UX rule specification for positive toggle labels.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_32_positive_toggle_labels",
        category: "writing",
        title: "Positive ON-state toggle labels",
        rule: "Label every toggle for what happens when it is ON, using sentence case without negative prefixes such as Don't, Disable, Hide, No, Desactivar, or Ocultar.",
        rationale: "Negative toggle labels force users to resolve a double negative when reading the OFF state.",
        aria_pattern: "role=\"switch\" aria-checked=\"true|false\" aria-labelledby=\"toggle-label\"",
    }
}

/// Validates that a toggle label describes an ON state in sentence case without negative prefixes.
pub fn validate_toggle_label(label: &str, lang: ToggleLanguage) -> Result<(), LabelIssue> {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return Err(LabelIssue::Empty);
    }

    if let Some(first) = trimmed.chars().next() {
        if !first.is_uppercase() {
            return Err(LabelIssue::NotSentenceCase);
        }
    }

    if trimmed.ends_with('.') || trimmed.ends_with('!') || trimmed.ends_with('?') {
        return Err(LabelIssue::TrailingPunctuation);
    }

    let lower = trimmed.to_lowercase();
    let prefixes = match lang {
        ToggleLanguage::Spanish => FORBIDDEN_PREFIXES_ES,
        ToggleLanguage::English => FORBIDDEN_PREFIXES_EN,
    };

    for prefix in prefixes {
        if lower.starts_with(prefix) {
            return Err(LabelIssue::NegativePrefix(prefix));
        }
    }

    Ok(())
}

/// Returns true when the toggle label satisfies the positive ON-state rule.
pub fn is_positive_toggle_label(label: &str, lang: ToggleLanguage) -> bool {
    validate_toggle_label(label, lang).is_ok()
}

/// Suggests how to rephrase a rejected negative prefix into an ON-state verb.
pub fn rewrite_hint(issue: LabelIssue) -> &'static str {
    match issue {
        LabelIssue::Empty => "Provide a non-empty verb-first label describing the ON state.",
        LabelIssue::NegativePrefix(_) => {
            "Rephrase to describe what happens when the toggle is ON (e.g., 'Mostrar...' or 'Show...')."
        }
        LabelIssue::NotSentenceCase => "Capitalize the first character for sentence case.",
        LabelIssue::TrailingPunctuation => "Remove trailing punctuation from the toggle label.",
    }
}

/// Formats accessible switch attributes for a toggle control.
pub fn toggle_aria_attributes(checked: bool, label_id: &str) -> String {
    let state = if checked { "true" } else { "false" };
    format!(
        "role=\"switch\" aria-checked=\"{}\" aria-labelledby=\"{}\"",
        state, label_id
    )
}

/// Verifies every entry in `SEARCH_LINUX_TOGGLES` passes both Spanish and English validation.
pub fn all_catalog_labels_valid() -> bool {
    SEARCH_LINUX_TOGGLES.iter().all(|entry| {
        is_positive_toggle_label(entry.positive_label_es, ToggleLanguage::Spanish)
            && is_positive_toggle_label(entry.positive_label_en, ToggleLanguage::English)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_matches_ux_32_contract() {
        let s = spec();
        assert_eq!(s.id, "ux_32_positive_toggle_labels");
        assert_eq!(s.category, "writing");
        assert!(s.aria_pattern.contains("role=\"switch\""));
    }

    #[test]
    fn rejects_negative_prefixes_in_spanish_and_english() {
        assert_eq!(
            validate_toggle_label("Ocultar dominio en pestañas", ToggleLanguage::Spanish),
            Err(LabelIssue::NegativePrefix("ocultar "))
        );
        assert_eq!(
            validate_toggle_label("Desactivar escudo", ToggleLanguage::Spanish),
            Err(LabelIssue::NegativePrefix("desactivar "))
        );
        assert_eq!(
            validate_toggle_label("No enviar recibos", ToggleLanguage::Spanish),
            Err(LabelIssue::NegativePrefix("no "))
        );
        assert_eq!(
            validate_toggle_label("Don't send read receipts", ToggleLanguage::English),
            Err(LabelIssue::NegativePrefix("don't "))
        );
        assert_eq!(
            validate_toggle_label("Disable notifications", ToggleLanguage::English),
            Err(LabelIssue::NegativePrefix("disable "))
        );
        assert_eq!(
            validate_toggle_label("Hide audio icon", ToggleLanguage::English),
            Err(LabelIssue::NegativePrefix("hide "))
        );
    }

    #[test]
    fn enforces_sentence_case_and_no_trailing_punctuation() {
        assert_eq!(
            validate_toggle_label("mostrar dominio", ToggleLanguage::Spanish),
            Err(LabelIssue::NotSentenceCase)
        );
        assert_eq!(
            validate_toggle_label("Mostrar dominio.", ToggleLanguage::Spanish),
            Err(LabelIssue::TrailingPunctuation)
        );
        assert_eq!(
            validate_toggle_label("   ", ToggleLanguage::English),
            Err(LabelIssue::Empty)
        );
    }

    #[test]
    fn catalog_labels_are_all_positive_and_valid() {
        assert!(all_catalog_labels_valid());
        assert!(!SEARCH_LINUX_TOGGLES.is_empty());
        for entry in SEARCH_LINUX_TOGGLES {
            assert!(
                !is_positive_toggle_label(entry.negative_anti_pattern, ToggleLanguage::Spanish),
                "anti-pattern for {} should fail validation",
                entry.setting_key
            );
        }
    }

    #[test]
    fn aria_attributes_and_hints_work() {
        assert_eq!(
            toggle_aria_attributes(true, "lbl-shield"),
            "role=\"switch\" aria-checked=\"true\" aria-labelledby=\"lbl-shield\""
        );
        assert_eq!(
            toggle_aria_attributes(false, "lbl-shield"),
            "role=\"switch\" aria-checked=\"false\" aria-labelledby=\"lbl-shield\""
        );
        assert!(rewrite_hint(LabelIssue::NegativePrefix("no ")).contains("ON"));
    }
}
