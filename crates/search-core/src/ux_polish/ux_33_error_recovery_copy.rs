//! UX-33: Error recovery copy specification, validator, CSS, and Spanish templates.
//!
//! Enforces `better-writing` and `better-ui` rules for error recovery in Search Linux (`es`):
//! - Errors state what did not happen and give a concrete next step right beside the failure.
//! - Calm, plain tone: zero blame, zero "oops"/"ups", zero exclamation marks, zero "we" deflection.
//! - Positive phrasing for constraints ("Elige una contraseña de al menos 8 caracteres").
//! - Recovery actions use sentence-case, verb-first Spanish infinitives (`Reintentar`, `Volver atrás`).
//! - Error surfaces use concentric radii (`14px` outer = `8px` inner + `6px` inset),
//!   `tabular-nums` for error codes/counts, `0.96` press scale, and `cubic-bezier(0.2, 0, 0, 1)`.

/// Specification metadata for `ux_33_error_recovery_copy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub locale: &'static str,
    pub capitalization_policy: &'static str,
    pub summary: &'static str,
    pub press_scale: &'static str,
    pub easing_standard: &'static str,
    pub enter_duration_ms: u16,
    pub exit_duration_ms: u16,
    pub outer_radius_px: u8,
    pub inner_radius_px: u8,
    pub inset_padding_px: u8,
    pub font_variant_numeric: &'static str,
    pub inline_aria_role: &'static str,
    pub inline_aria_live: &'static str,
}

/// Returns the canonical UX rule specification for `ux_33_error_recovery_copy`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_33_error_recovery_copy",
        title: "Error recovery copy and actionable guidance",
        category: "writing-and-error-recovery",
        locale: "es",
        capitalization_policy: "sentence-case",
        summary: "State what failed plainly without blame, explain how to fix it next to the control, and offer a verb-first recovery action.",
        press_scale: "0.96",
        easing_standard: "cubic-bezier(0.2, 0, 0, 1)",
        enter_duration_ms: 300,
        exit_duration_ms: 150,
        outer_radius_px: 14,
        inner_radius_px: 8,
        inset_padding_px: 6,
        font_variant_numeric: "tabular-nums",
        inline_aria_role: "alert",
        inline_aria_live: "polite",
    }
}

/// Domain category of an error recovery surface in Search Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    NetworkLoad,
    DnsLookup,
    TlsCertificate,
    DownloadFailed,
    FieldValidation,
    PermissionDenied,
    PartialBatch,
}

/// Canonical Spanish error recovery template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorRecoveryTemplate {
    pub kind: ErrorKind,
    pub headline: &'static str,
    pub guidance: &'static str,
    pub primary_action: &'static str,
    pub secondary_action: Option<&'static str>,
    pub replaces_anti_pattern: &'static str,
}

/// Violations detected when validating error recovery copy against UX-33.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCopyViolation {
    EmptyHeadline,
    EmptyGuidance,
    EmptyAction,
    ContainsExclamation,
    PlayfulOrBlamingPhrase(&'static str),
    DeflectingFirstPersonPlural(&'static str),
    GenericNonVerbAction(&'static str),
    NegativeConstraintWithoutFix,
    ActionNotSentenceCase,
    ActionMissingGuidance,
}

const BANNED_PLAYFUL_OR_BLAME: &[&str] = &[
    "ups",
    "oops",
    "vaya",
    "uy",
    "algo salió mal",
    "algo salio mal",
    "fallaste",
    "hiciste mal",
    "tu culpa",
    "error fatal",
];

const BANNED_DEFLECTION_PRONOUNS: &[&str] =
    &["nosotros", "nuestro", "nuestra", "nuestros", "nuestras"];

const BANNED_GENERIC_ACTIONS: &[&str] = &[
    "ok",
    "ok!",
    "aceptar",
    "sí",
    "si",
    "no",
    "listo",
    "entendido",
    "de acuerdo",
    "cerrar",
];

/// Canonical Spanish error recovery templates for Search Linux.
pub const TEMPLATES: &[ErrorRecoveryTemplate] = &[
    ErrorRecoveryTemplate {
        kind: ErrorKind::NetworkLoad,
        headline: "No se pudo abrir esta página",
        guidance: "Revisa tu conexión e inténtalo de nuevo.",
        primary_action: "Reintentar",
        secondary_action: Some("Comprobar conexión"),
        replaces_anti_pattern: "Error de red. OK.",
    },
    ErrorRecoveryTemplate {
        kind: ErrorKind::DnsLookup,
        headline: "No se encontró el servidor",
        guidance: "Comprueba que la dirección esté bien escrita.",
        primary_action: "Editar dirección",
        secondary_action: Some("Reintentar"),
        replaces_anti_pattern: "DNS_PROBE_FINISHED_NXDOMAIN",
    },
    ErrorRecoveryTemplate {
        kind: ErrorKind::TlsCertificate,
        headline: "Conexión no privada",
        guidance: "El certificado del sitio no es válido. Vuelve atrás para proteger tus datos.",
        primary_action: "Volver a un sitio seguro",
        secondary_action: Some("Ver detalles técnicos"),
        replaces_anti_pattern: "¡Peligro! Certificado inválido.",
    },
    ErrorRecoveryTemplate {
        kind: ErrorKind::DownloadFailed,
        headline: "No se pudo completar la descarga",
        guidance: "Comprueba tu conexión y el espacio disponible en disco.",
        primary_action: "Reintentar descarga",
        secondary_action: Some("Abrir carpeta de descargas"),
        replaces_anti_pattern: "Descarga fallida.",
    },
    ErrorRecoveryTemplate {
        kind: ErrorKind::FieldValidation,
        headline: "Contraseña demasiado corta",
        guidance: "Elige una contraseña de al menos 8 caracteres.",
        primary_action: "Corregir contraseña",
        secondary_action: None,
        replaces_anti_pattern: "No uses menos de 8 caracteres.",
    },
    ErrorRecoveryTemplate {
        kind: ErrorKind::PermissionDenied,
        headline: "No se pudo leer la carpeta seleccionada",
        guidance: "Concede permiso de lectura para importar tus marcadores.",
        primary_action: "Revisar permisos",
        secondary_action: Some("Elegir otra carpeta"),
        replaces_anti_pattern: "Acceso denegado.",
    },
    ErrorRecoveryTemplate {
        kind: ErrorKind::PartialBatch,
        headline: "Importación completada con avisos",
        guidance: "Revisa los elementos pendientes y vuelve a intentarlo.",
        primary_action: "Reintentar pendientes",
        secondary_action: Some("Ver detalles"),
        replaces_anti_pattern: "Ocurrió un error al importar.",
    },
];

/// Looks up the canonical recovery template for a given `ErrorKind`.
pub fn template_for(kind: ErrorKind) -> &'static ErrorRecoveryTemplate {
    for tpl in TEMPLATES {
        if tpl.kind == kind {
            return tpl;
        }
    }
    &TEMPLATES[0]
}

/// Formats a partial-batch recovery message with counts for both outcomes (`tabular-nums` ready).
pub fn format_partial_recovery(succeeded: usize, total: usize) -> String {
    let failed = total.saturating_sub(succeeded);
    if failed == 0 {
        return format!("{succeeded} de {total} elementos completados.");
    }
    let item_word = if failed == 1 {
        "elemento que falló"
    } else {
        "elementos que fallaron"
    };
    format!("{succeeded} de {total} elementos completados. Reintenta {failed} {item_word}.")
}

/// Validates custom Spanish error recovery copy against UX-33 rules.
pub fn validate_error_copy(
    headline: &str,
    guidance: &str,
    action_label: &str,
) -> Result<(), ErrorCopyViolation> {
    let h = headline.trim();
    let g = guidance.trim();
    let a = action_label.trim();

    if h.is_empty() {
        return Err(ErrorCopyViolation::EmptyHeadline);
    }
    if g.is_empty() {
        return Err(ErrorCopyViolation::EmptyGuidance);
    }
    if a.is_empty() {
        return Err(ErrorCopyViolation::EmptyAction);
    }

    if h.contains('!') || h.contains('¡') || g.contains('!') || g.contains('¡') || a.contains('!')
    {
        return Err(ErrorCopyViolation::ContainsExclamation);
    }

    let combined_lower = format!("{} {}", h.to_lowercase(), g.to_lowercase());
    for phrase in BANNED_PLAYFUL_OR_BLAME {
        if contains_word_or_phrase(&combined_lower, phrase) {
            return Err(ErrorCopyViolation::PlayfulOrBlamingPhrase(phrase));
        }
    }

    for pronoun in BANNED_DEFLECTION_PRONOUNS {
        if contains_word_or_phrase(&combined_lower, pronoun) {
            return Err(ErrorCopyViolation::DeflectingFirstPersonPlural(pronoun));
        }
    }

    let g_lower = g.to_lowercase();
    if g_lower.starts_with("no uses ")
        || g_lower.starts_with("no ingreses ")
        || g_lower == "inválido"
        || g_lower == "invalido"
    {
        return Err(ErrorCopyViolation::NegativeConstraintWithoutFix);
    }

    if g.split_whitespace().count() < 2 {
        return Err(ErrorCopyViolation::ActionMissingGuidance);
    }

    let a_lower = a.to_lowercase();
    for generic in BANNED_GENERIC_ACTIONS {
        if a_lower == *generic {
            return Err(ErrorCopyViolation::GenericNonVerbAction(generic));
        }
    }

    if !is_sentence_case(a) {
        return Err(ErrorCopyViolation::ActionNotSentenceCase);
    }

    Ok(())
}

fn contains_word_or_phrase(haystack: &str, needle: &str) -> bool {
    if needle.contains(' ') {
        return haystack.contains(needle);
    }
    haystack
        .split(|c: char| !c.is_alphanumeric())
        .any(|token| token == needle)
}

fn is_sentence_case(text: &str) -> bool {
    let mut chars = text.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };
    if !first.is_uppercase() {
        return false;
    }
    let mut words = text.split_whitespace();
    let _ = words.next();
    for word in words {
        let mut w_chars = word.chars();
        if let Some(wc) = w_chars.next() {
            if wc.is_uppercase() && word.chars().skip(1).any(|c| c.is_lowercase()) {
                return false;
            }
        }
    }
    true
}

/// Returns the CSS fragment for error recovery cards and inline recovery banners.
///
/// Uses OKLCH tokens with >= 4.5:1 text contrast, concentric radii (`14px` outer, `8px` inner),
/// `tabular-nums` for error codes/counts, `0.96` press scale, `cubic-bezier(0.2, 0, 0, 1)`
/// motion, `@media (hover: hover)` gating, and `prefers-reduced-motion` fallback.
pub fn error_recovery_css() -> &'static str {
    r#".ux-error-card {
  border-radius: 14px;
  padding: 24px;
  border: 1px solid oklch(0.88 0.01 260);
  background: oklch(0.99 0.002 260);
  color: oklch(0.22 0.015 260);
  max-width: 65ch;
}

.ux-error-card__code,
.ux-error-card__counts {
  font-variant-numeric: tabular-nums;
  color: oklch(0.45 0.015 260);
}

.ux-error-card__action {
  border-radius: 8px;
  border: 1px solid transparent;
  padding: 8px 16px;
  background: oklch(0.24 0.02 260);
  color: oklch(0.98 0.002 260);
  transition-property: scale, background-color, opacity;
  transition-duration: 150ms;
  transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
}

.ux-error-card__action:not(:disabled):active {
  scale: 0.96;
}

@media (hover: hover) {
  .ux-error-card__action:not(:disabled):hover {
    background: oklch(0.32 0.02 260);
  }
}

@media (prefers-reduced-motion: reduce) {
  .ux-error-card__action {
    transition-property: opacity;
    transition-duration: 150ms;
  }
}"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_matches_exact_ux_constants() {
        let s = spec();
        assert_eq!(s.id, "ux_33_error_recovery_copy");
        assert_eq!(s.locale, "es");
        assert_eq!(s.capitalization_policy, "sentence-case");
        assert_eq!(s.press_scale, "0.96");
        assert_eq!(s.easing_standard, "cubic-bezier(0.2, 0, 0, 1)");
        assert_eq!(s.outer_radius_px, s.inner_radius_px + s.inset_padding_px);
        assert_eq!(s.font_variant_numeric, "tabular-nums");
        assert_eq!(s.inline_aria_role, "alert");
    }

    #[test]
    fn all_canonical_templates_pass_validation() {
        for tpl in TEMPLATES {
            assert_eq!(
                validate_error_copy(tpl.headline, tpl.guidance, tpl.primary_action),
                Ok(()),
                "template {:?} failed validation",
                tpl.kind
            );
            if let Some(sec) = tpl.secondary_action {
                assert_eq!(
                    validate_error_copy(tpl.headline, tpl.guidance, sec),
                    Ok(()),
                    "secondary action on {:?} failed validation",
                    tpl.kind
                );
            }
        }
    }

    #[test]
    fn rejects_exclamations_blame_deflection_and_generic_buttons() {
        assert_eq!(
            validate_error_copy("¡Ups! Algo salió mal", "Inténtalo más tarde.", "Reintentar"),
            Err(ErrorCopyViolation::ContainsExclamation)
        );
        assert_eq!(
            validate_error_copy(
                "Error al guardar",
                "Ups, algo salio mal con el archivo.",
                "Reintentar"
            ),
            Err(ErrorCopyViolation::PlayfulOrBlamingPhrase("ups"))
        );
        assert_eq!(
            validate_error_copy(
                "No se pudo conectar",
                "Nuestros servidores están ocupados.",
                "Reintentar"
            ),
            Err(ErrorCopyViolation::DeflectingFirstPersonPlural("nuestros"))
        );
        assert_eq!(
            validate_error_copy(
                "Contraseña no válida",
                "No uses menos de 8 caracteres.",
                "Corregir"
            ),
            Err(ErrorCopyViolation::NegativeConstraintWithoutFix)
        );
        assert_eq!(
            validate_error_copy(
                "No se pudo abrir esta página",
                "Revisa tu conexión e inténtalo de nuevo.",
                "Aceptar"
            ),
            Err(ErrorCopyViolation::GenericNonVerbAction("aceptar"))
        );
        assert_eq!(
            validate_error_copy(
                "No se pudo abrir esta página",
                "Revisa tu conexión e inténtalo de nuevo.",
                "Reintentar Ahora"
            ),
            Err(ErrorCopyViolation::ActionNotSentenceCase)
        );
    }

    #[test]
    fn partial_recovery_and_css_expose_required_tokens() {
        assert_eq!(
            format_partial_recovery(9, 12),
            "9 de 12 elementos completados. Reintenta 3 elementos que fallaron."
        );
        assert_eq!(
            format_partial_recovery(4, 5),
            "4 de 5 elementos completados. Reintenta 1 elemento que falló."
        );

        let css = error_recovery_css();
        assert!(css.contains("cubic-bezier(0.2, 0, 0, 1)"));
        assert!(css.contains("scale: 0.96;"));
        assert!(css.contains("tabular-nums"));
        assert!(css.contains("border-radius: 14px;"));
        assert!(css.contains("border-radius: 8px;"));
        assert!(css.contains("oklch("));
        assert!(css.contains("@media (hover: hover)"));
        assert!(css.contains("prefers-reduced-motion: reduce"));
    }
}
