//! UX Rule 37: Placeholders show a realistic example, never an instruction.
//!
//! Grounded in `better-writing`:
//! - A placeholder shows a realistic example in the exact format the field
//!   accepts (such as `nombre@ejemplo.com` or `DD/MM/AAAA`), never an
//!   instruction (such as `"Escribe tu correo..."` or `"Introduce una URL"`).
//! - Visible labels carry the field's name in Spanish sentence case; the
//!   placeholder only demonstrates format and sample content.

use core::fmt;

/// Specification metadata for a single UX polish rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub category: &'static str,
    pub title_es: &'static str,
    pub summary_es: &'static str,
    pub css_fragment: &'static str,
    pub rationale: &'static str,
}

/// Browser input fields that display format-example placeholders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldKind {
    OmniboxUrl,
    BookmarkTitle,
    BookmarkFolder,
    VaultUsername,
    VaultWebsite,
    DateInput,
    ProxyHostPort,
    SearchEngineKeyword,
    SearchEngineTemplate,
    FindInPage,
    DownloadFilename,
}

/// Pairing of a Spanish sentence-case visible label and a format-example placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaceholderExample {
    pub field: FieldKind,
    pub visible_label_es: &'static str,
    pub placeholder_example: &'static str,
    pub accepted_format: &'static str,
}

/// Reasons a candidate placeholder string violates the example-only rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceholderViolation {
    Empty,
    InstructionalPhrase,
    TrailingEllipsis,
    TrailingPeriod,
}

impl fmt::Display for PlaceholderViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("placeholder must not be empty"),
            Self::InstructionalPhrase => {
                f.write_str("placeholder must show an example, not an instruction")
            }
            Self::TrailingEllipsis => f.write_str("placeholder must not end with ellipsis"),
            Self::TrailingPeriod => {
                f.write_str("placeholder example must not end with a sentence period")
            }
        }
    }
}

/// Canonical placeholder CSS fragment keeping sufficient contrast and clean rendering.
pub const PLACEHOLDER_CSS: &str = "\
input::placeholder,
textarea::placeholder {
    opacity: 1;
    color: var(--text-muted);
}";

/// Canonical catalog of Search Linux input placeholders using realistic examples.
pub const PLACEHOLDER_EXAMPLES: &[PlaceholderExample] = &[
    PlaceholderExample {
        field: FieldKind::OmniboxUrl,
        visible_label_es: "Dirección web",
        placeholder_example: "https://searchlinux.org",
        accepted_format: "URL o término de consulta",
    },
    PlaceholderExample {
        field: FieldKind::BookmarkTitle,
        visible_label_es: "Nombre del marcador",
        placeholder_example: "Documentación de Rust",
        accepted_format: "Texto breve",
    },
    PlaceholderExample {
        field: FieldKind::BookmarkFolder,
        visible_label_es: "Carpeta",
        placeholder_example: "Lectura técnica",
        accepted_format: "Nombre de carpeta",
    },
    PlaceholderExample {
        field: FieldKind::VaultUsername,
        visible_label_es: "Usuario o correo",
        placeholder_example: "nombre@ejemplo.com",
        accepted_format: "usuario o correo electrónico",
    },
    PlaceholderExample {
        field: FieldKind::VaultWebsite,
        visible_label_es: "Sitio web",
        placeholder_example: "https://cuentas.ejemplo.com",
        accepted_format: "URL con esquema https://",
    },
    PlaceholderExample {
        field: FieldKind::DateInput,
        visible_label_es: "Fecha",
        placeholder_example: "DD/MM/AAAA",
        accepted_format: "DD/MM/AAAA",
    },
    PlaceholderExample {
        field: FieldKind::ProxyHostPort,
        visible_label_es: "Servidor proxy",
        placeholder_example: "127.0.0.1:8080",
        accepted_format: "host:puerto",
    },
    PlaceholderExample {
        field: FieldKind::SearchEngineKeyword,
        visible_label_es: "Atajo",
        placeholder_example: "docs",
        accepted_format: "Palabra clave sin espacios",
    },
    PlaceholderExample {
        field: FieldKind::SearchEngineTemplate,
        visible_label_es: "URL de búsqueda",
        placeholder_example: "https://ejemplo.com/buscar?q=%s",
        accepted_format: "URL con marcador %s",
    },
    PlaceholderExample {
        field: FieldKind::FindInPage,
        visible_label_es: "Buscar en la página",
        placeholder_example: "privacidad",
        accepted_format: "Texto literal",
    },
    PlaceholderExample {
        field: FieldKind::DownloadFilename,
        visible_label_es: "Nombre del archivo",
        placeholder_example: "informe-2026.pdf",
        accepted_format: "nombre.extensión",
    },
];

/// Returns the UX rule specification for placeholder examples.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_37_placeholder_examples",
        category: "better-writing",
        title_es: "Los marcadores de posición muestran un ejemplo",
        summary_es: "Usar ejemplos realistas con el formato aceptado en lugar de instrucciones.",
        css_fragment: PLACEHOLDER_CSS,
        rationale: "Placeholders disappear on focus; showing a concrete format example (e.g. nombre@ejemplo.com, DD/MM/AAAA) teaches syntax without duplicating the visible label.",
    }
}

/// Looks up the canonical placeholder entry for a given field kind.
pub fn example_for(field: FieldKind) -> &'static PlaceholderExample {
    match field {
        FieldKind::OmniboxUrl => &PLACEHOLDER_EXAMPLES[0],
        FieldKind::BookmarkTitle => &PLACEHOLDER_EXAMPLES[1],
        FieldKind::BookmarkFolder => &PLACEHOLDER_EXAMPLES[2],
        FieldKind::VaultUsername => &PLACEHOLDER_EXAMPLES[3],
        FieldKind::VaultWebsite => &PLACEHOLDER_EXAMPLES[4],
        FieldKind::DateInput => &PLACEHOLDER_EXAMPLES[5],
        FieldKind::ProxyHostPort => &PLACEHOLDER_EXAMPLES[6],
        FieldKind::SearchEngineKeyword => &PLACEHOLDER_EXAMPLES[7],
        FieldKind::SearchEngineTemplate => &PLACEHOLDER_EXAMPLES[8],
        FieldKind::FindInPage => &PLACEHOLDER_EXAMPLES[9],
        FieldKind::DownloadFilename => &PLACEHOLDER_EXAMPLES[10],
    }
}

/// Returns only the placeholder example string for a given field kind.
pub fn placeholder_for(field: FieldKind) -> &'static str {
    example_for(field).placeholder_example
}

/// Imperative prefixes in Spanish and English that indicate an instruction rather than an example.
const INSTRUCTION_PREFIXES: &[&str] = &[
    "escribe ",
    "escriba ",
    "introduce ",
    "introduzca ",
    "ingresa ",
    "ingrese ",
    "busca ",
    "buscar ",
    "filtra ",
    "filtrar ",
    "selecciona ",
    "elige ",
    "pon ",
    "añade ",
    "agrega ",
    "enter ",
    "type ",
    "search ",
    "filter ",
    "input ",
    "write ",
];

/// Returns `true` if `text` reads like an instruction instead of a format example.
pub fn is_instruction_placeholder(text: &str) -> bool {
    let normalized = text.trim().to_lowercase();
    if normalized.is_empty() {
        return false;
    }
    if INSTRUCTION_PREFIXES
        .iter()
        .any(|prefix| normalized.starts_with(prefix))
    {
        return true;
    }
    normalized.contains(" aquí") || normalized.contains(" here")
}

/// Validates that a candidate placeholder string is a clean example, not an instruction.
pub fn validate_placeholder(text: &str) -> Result<(), PlaceholderViolation> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(PlaceholderViolation::Empty);
    }
    if trimmed.ends_with("...") || trimmed.ends_with('\u{2026}') {
        return Err(PlaceholderViolation::TrailingEllipsis);
    }
    if trimmed.ends_with('.') {
        return Err(PlaceholderViolation::TrailingPeriod);
    }
    if is_instruction_placeholder(trimmed) {
        return Err(PlaceholderViolation::InstructionalPhrase);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_has_exact_rule_metadata_and_css() {
        let s = spec();
        assert_eq!(s.id, "ux_37_placeholder_examples");
        assert_eq!(s.category, "better-writing");
        assert!(s.css_fragment.contains("input::placeholder"));
        assert!(s.css_fragment.contains("opacity: 1;"));
    }

    #[test]
    fn all_catalog_placeholders_pass_validation_and_are_not_instructions() {
        assert_eq!(PLACEHOLDER_EXAMPLES.len(), 11);
        for item in PLACEHOLDER_EXAMPLES {
            assert_eq!(
                validate_placeholder(item.placeholder_example),
                Ok(()),
                "invalid catalog placeholder: {}",
                item.placeholder_example
            );
            assert!(!item.visible_label_es.is_empty());
            assert!(!item.accepted_format.is_empty());
        }
    }

    #[test]
    fn instructional_placeholders_are_rejected() {
        assert_eq!(
            validate_placeholder("Escribe tu correo"),
            Err(PlaceholderViolation::InstructionalPhrase)
        );
        assert_eq!(
            validate_placeholder("Introduce una URL..."),
            Err(PlaceholderViolation::TrailingEllipsis)
        );
        assert_eq!(
            validate_placeholder("Search tabs here"),
            Err(PlaceholderViolation::InstructionalPhrase)
        );
        assert_eq!(
            validate_placeholder("nombre@ejemplo.com."),
            Err(PlaceholderViolation::TrailingPeriod)
        );
        assert_eq!(
            validate_placeholder("   "),
            Err(PlaceholderViolation::Empty)
        );
    }

    #[test]
    fn lookup_by_field_returns_expected_spanish_examples() {
        assert_eq!(
            placeholder_for(FieldKind::VaultUsername),
            "nombre@ejemplo.com"
        );
        assert_eq!(placeholder_for(FieldKind::DateInput), "DD/MM/AAAA");
        assert_eq!(placeholder_for(FieldKind::ProxyHostPort), "127.0.0.1:8080");
        assert_eq!(
            example_for(FieldKind::OmniboxUrl).visible_label_es,
            "Dirección web"
        );
    }
}
