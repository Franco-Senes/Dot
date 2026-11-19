//! UX-38: Sentence case consistency across UI element types.
//!
//! Enforces a single capitalization policy per element type (sentence case by
//! default for Search Linux's Spanish interface). Only the first letter of a
//! sentence, proper nouns, recognized technical acronyms, and keyboard
//! shortcuts are capitalized; common words in buttons, menu items, headings,
//! toggles, and tooltips remain lowercase so sibling controls never mix
//! `"Guardar Cambios"` with `"Descartar cambios"`.

/// Specification metadata for this UX polish rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub summary: &'static str,
    pub policy: &'static str,
}

/// Returns the canonical specification for `ux_38_sentence_case_consistency`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_38_sentence_case_consistency",
        title: "Sentence case consistency across UI element types",
        category: "writing",
        summary: "Applies Spanish sentence case consistently per element type, preserving acronyms, proper nouns, and keyboard shortcuts.",
        policy: "One capitalization policy per element type; sentence case by default.",
    }
}

/// UI element categories audited for capitalization consistency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiElementKind {
    Button,
    MenuItem,
    Heading,
    SectionLabel,
    ToggleLabel,
    Tooltip,
    TabTitle,
    Placeholder,
    DialogTitle,
}

impl UiElementKind {
    /// Returns a stable identifier for the element type.
    pub fn label(self) -> &'static str {
        match self {
            Self::Button => "button",
            Self::MenuItem => "menu_item",
            Self::Heading => "heading",
            Self::SectionLabel => "section_label",
            Self::ToggleLabel => "toggle_label",
            Self::Tooltip => "tooltip",
            Self::TabTitle => "tab_title",
            Self::Placeholder => "placeholder",
            Self::DialogTitle => "dialog_title",
        }
    }

    /// Search Linux uses sentence case across all Spanish UI element types.
    pub fn prefers_sentence_case(self) -> bool {
        true
    }
}

/// Detected capitalization style of a UI string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseStyle {
    SentenceCase,
    TitleCase,
    AllCaps,
    AllLower,
    MixedOrInconsistent,
}

/// A single capitalization violation inside a UI string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseViolation {
    pub word_index: usize,
    pub token: String,
    pub reason: &'static str,
}

/// Result of auditing a UI string against the sentence-case policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseAudit {
    pub element_kind: UiElementKind,
    pub text: String,
    pub detected_style: CaseStyle,
    pub is_sentence_case: bool,
    pub violations: Vec<CaseViolation>,
    pub suggested_fix: String,
}

/// Canonical `(inconsistent, normalized)` Spanish UI copy pairs for Search Linux.
pub const CANONICAL_SPANISH_PAIRS: &[(&str, &str)] = &[
    ("Guardar Cambios", "Guardar cambios"),
    ("Eliminar Carpeta", "Eliminar carpeta"),
    ("Cerrar Panel (Esc)", "Cerrar panel (Esc)"),
    ("Nueva Pestaña", "Nueva pestaña"),
    ("Abrir En Ventana Privada", "Abrir en ventana privada"),
    ("Buscar En El Historial…", "Buscar en el historial…"),
];

const ALLOWED_ACRONYMS: &[&str] = &[
    "API", "CPU", "CSS", "DNS", "DOM", "GIF", "GPU", "GTK", "HTML", "HTTP", "HTTPS", "ID", "IP",
    "IPv4", "IPv6", "JSON", "OCR", "OTP", "PDF", "PNG", "PiP", "QR", "RAM", "RSS", "SQL", "SSL",
    "SVG", "TLS", "UI", "URL", "URI", "UTF-8", "VPN", "WebGL", "WebRTC", "XML", "2FA",
];

const ALLOWED_PROPER_NOUNS: &[&str] = &[
    "Chromium",
    "Linux",
    "Picture-in-Picture",
    "Search",
    "Wayland",
    "X11",
];

const ALLOWED_KEY_NAMES: &[&str] = &[
    "Alt", "Ctrl", "Del", "End", "Enter", "Esc", "Home", "Ins", "Meta", "PgDn", "PgUp", "Shift",
    "Space", "Super", "Tab",
];

fn strip_wrapping_punctuation(token: &str) -> &str {
    let is_wrap = |c: char| {
        matches!(
            c,
            '(' | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | ','
                | '.'
                | ':'
                | ';'
                | '?'
                | '!'
                | '¿'
                | '¡'
                | '"'
                | '\''
                | '«'
                | '»'
                | '…'
        )
    };
    token.trim_matches(is_wrap)
}

fn is_function_key(token: &str) -> bool {
    if let Some(rest) = token.strip_prefix('F') {
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(num) = rest.parse::<u8>() {
                return (1..=12).contains(&num);
            }
        }
    }
    false
}

fn is_shortcut_combo(token: &str) -> bool {
    if !token.contains('+') {
        return false;
    }
    let parts: Vec<&str> = token.split('+').collect();
    if parts.len() < 2 || parts.iter().any(|p| p.is_empty()) {
        return false;
    }
    parts.iter().all(|part| {
        ALLOWED_KEY_NAMES.contains(part)
            || is_function_key(part)
            || (part.len() == 1 && part.chars().all(|c| c.is_ascii_alphanumeric()))
    })
}

/// Returns `true` if `token` is an acronym, proper noun, or keyboard shortcut
/// whose uppercase letters must be preserved mid-sentence.
pub fn is_allowed_uppercase_token(token: &str) -> bool {
    let clean = strip_wrapping_punctuation(token);
    if clean.is_empty() {
        return false;
    }
    ALLOWED_ACRONYMS.contains(&clean)
        || ALLOWED_PROPER_NOUNS.contains(&clean)
        || ALLOWED_KEY_NAMES.contains(&clean)
        || is_function_key(clean)
        || is_shortcut_combo(clean)
}

fn first_alpha_is_uppercase(token: &str) -> Option<bool> {
    token
        .chars()
        .find(|c| c.is_alphabetic())
        .map(|c| c.is_uppercase())
}

fn ends_sentence(token: &str) -> bool {
    let trimmed = token.trim_end_matches([')', ']', '"', '\'', '»']);
    trimmed.ends_with('.') || trimmed.ends_with('?') || trimmed.ends_with('!')
}

fn capitalize_first_alpha(token: &str) -> String {
    let mut out = String::with_capacity(token.len());
    let mut done = false;
    for ch in token.chars() {
        if !done && ch.is_alphabetic() {
            for upper in ch.to_uppercase() {
                out.push(upper);
            }
            done = true;
        } else {
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
        }
    }
    out
}

fn lowercase_token(token: &str) -> String {
    token.chars().flat_map(|c| c.to_lowercase()).collect()
}

/// Normalizes a UI string to Spanish sentence case while preserving acronyms,
/// proper nouns, keyboard shortcuts, and sentence boundaries.
pub fn to_sentence_case(input: &str) -> String {
    let words: Vec<&str> = input.split_whitespace().collect();
    if words.is_empty() {
        return String::new();
    }

    let mut result = Vec::with_capacity(words.len());
    let mut at_sentence_start = true;

    for word in words {
        let normalized = if is_allowed_uppercase_token(word) {
            word.to_string()
        } else if at_sentence_start {
            capitalize_first_alpha(word)
        } else {
            lowercase_token(word)
        };
        at_sentence_start = ends_sentence(word);
        result.push(normalized);
    }

    result.join(" ")
}

fn classify_style(text: &str, violations: &[CaseViolation], has_alpha: bool) -> CaseStyle {
    if !has_alpha {
        return CaseStyle::SentenceCase;
    }

    let alpha_chars: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();
    if !alpha_chars.is_empty() && alpha_chars.iter().all(|c| c.is_uppercase()) {
        return CaseStyle::AllCaps;
    }
    if !alpha_chars.is_empty() && alpha_chars.iter().all(|c| c.is_lowercase()) {
        return CaseStyle::AllLower;
    }

    if violations.is_empty() {
        return CaseStyle::SentenceCase;
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    let non_allowed_words: Vec<&&str> = words
        .iter()
        .filter(|w| !is_allowed_uppercase_token(w) && first_alpha_is_uppercase(w).is_some())
        .collect();

    if non_allowed_words.len() > 1
        && non_allowed_words
            .iter()
            .all(|w| first_alpha_is_uppercase(w) == Some(true))
    {
        return CaseStyle::TitleCase;
    }

    CaseStyle::MixedOrInconsistent
}

/// Audits a single UI string for Spanish sentence-case compliance.
pub fn audit_sentence_case(kind: UiElementKind, text: &str) -> CaseAudit {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut violations = Vec::new();
    let mut at_sentence_start = true;
    let mut has_alpha = false;

    for (idx, word) in words.iter().enumerate() {
        if let Some(starts_upper) = first_alpha_is_uppercase(word) {
            has_alpha = true;
            if at_sentence_start {
                if !starts_upper && !is_allowed_uppercase_token(word) {
                    violations.push(CaseViolation {
                        word_index: idx,
                        token: (*word).to_string(),
                        reason: "sentence must start with an uppercase letter",
                    });
                }
            } else if !is_allowed_uppercase_token(word) {
                let has_internal_upper = word.chars().any(|c| c.is_uppercase());
                if starts_upper || has_internal_upper {
                    violations.push(CaseViolation {
                        word_index: idx,
                        token: (*word).to_string(),
                        reason: "non-initial common word must be lowercase in sentence case",
                    });
                }
            }
        }
        at_sentence_start = ends_sentence(word);
    }

    let detected_style = classify_style(text, &violations, has_alpha);
    let is_sentence_case = violations.is_empty();
    let suggested_fix = to_sentence_case(text);

    CaseAudit {
        element_kind: kind,
        text: text.to_string(),
        detected_style,
        is_sentence_case,
        violations,
        suggested_fix,
    }
}

/// Audits a group of sibling labels belonging to the same `UiElementKind`.
pub fn audit_sibling_group(kind: UiElementKind, labels: &[&str]) -> Vec<CaseAudit> {
    labels
        .iter()
        .map(|label| audit_sentence_case(kind, label))
        .collect()
}

/// Returns `true` when every label in the sibling group follows sentence case.
pub fn has_consistent_sentence_case(kind: UiElementKind, labels: &[&str]) -> bool {
    audit_sibling_group(kind, labels)
        .iter()
        .all(|audit| audit.is_sentence_case)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_exposes_ux_38_metadata() {
        let rule = spec();
        assert_eq!(rule.id, "ux_38_sentence_case_consistency");
        assert_eq!(rule.category, "writing");
        assert!(UiElementKind::Button.prefers_sentence_case());
        assert_eq!(UiElementKind::DialogTitle.label(), "dialog_title");
    }

    #[test]
    fn canonical_spanish_pairs_normalize_to_sentence_case() {
        for (bad, good) in CANONICAL_SPANISH_PAIRS {
            assert_eq!(to_sentence_case(bad), *good);
            let good_audit = audit_sentence_case(UiElementKind::Button, good);
            assert!(
                good_audit.is_sentence_case,
                "expected `{good}` to pass sentence case audit"
            );
        }
    }

    #[test]
    fn detects_mixed_sibling_buttons_like_save_changes_vs_discard() {
        let siblings = ["Guardar Cambios", "Descartar cambios"];
        assert!(!has_consistent_sentence_case(
            UiElementKind::Button,
            &siblings
        ));

        let audits = audit_sibling_group(UiElementKind::Button, &siblings);
        assert_eq!(audits.len(), 2);
        assert!(!audits[0].is_sentence_case);
        assert_eq!(audits[0].detected_style, CaseStyle::TitleCase);
        assert_eq!(audits[0].suggested_fix, "Guardar cambios");
        assert!(audits[1].is_sentence_case);
    }

    #[test]
    fn preserves_acronyms_shortcuts_and_spanish_punctuation() {
        let valid_labels = [
            "Cerrar panel (Esc)",
            "Abrir en ventana nueva (Ctrl+N)",
            "Configurar DNS sobre HTTPS",
            "¿Guardar la contraseña?",
            "Cerrar Picture-in-Picture",
            "Aún no hay contraseñas. Inicia sesión en un sitio.",
        ];

        for label in valid_labels {
            let audit = audit_sentence_case(UiElementKind::MenuItem, label);
            assert!(
                audit.is_sentence_case,
                "expected `{label}` to be valid sentence case, got violations: {:?}",
                audit.violations
            );
            assert_eq!(audit.suggested_fix, label);
        }
    }

    #[test]
    fn flags_lowercase_start_and_fixes_unicode_spanish_accents() {
        let audit = audit_sentence_case(UiElementKind::Tooltip, "¿eliminar esta carpeta?");
        assert!(!audit.is_sentence_case);
        assert_eq!(audit.suggested_fix, "¿Eliminar esta carpeta?");

        assert_eq!(
            to_sentence_case("ELIMINAR HISTORIAL DE NAVEGACIÓN"),
            "Eliminar historial de navegación"
        );
    }
}
