//! UX-49: Retroalimentación de coincidencias en la barra de búsqueda (`Ctrl+F`).
//!
//! Módulo puro en Rust que define la especificación visual, tipográfica,
//! de movimiento, contraste OKLCH, accesibilidad (ARIA live regions) y
//! copy en español (sentence-case, verb-first) para la barra de búsqueda en página.

/// Especificación principal de la regla UX-49.
#[derive(Debug, Clone, PartialEq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub summary: &'static str,
    pub motion: MotionSpec,
    pub radii: ConcentricRadii,
    pub typography: TypographySpec,
    pub colors: ColorTokens,
    pub copy: FindBarCopy,
}

/// Parámetros de movimiento y respuesta táctil para la barra de búsqueda.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionSpec {
    pub emphasized_easing: &'static str,
    pub press_scale: f32,
    pub press_duration_ms: u16,
    pub press_easing: &'static str,
    pub feedback_transition_ms: u16,
    pub reduced_motion_query: &'static str,
    pub hover_capable_query: &'static str,
}

/// Radios concéntricos: `outer_px = inner_px + padding_px`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConcentricRadii {
    pub outer_px: u16,
    pub padding_px: u16,
    pub inner_px: u16,
}

impl ConcentricRadii {
    /// Verifica la invariante concéntrica (`outer == inner + padding`).
    pub const fn is_concentric(&self) -> bool {
        self.outer_px == self.inner_px + self.padding_px
    }
}

/// Reglas tipográficas para evitar saltos horizontales al recorrer coincidencias.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypographySpec {
    pub counter_font_variant_numeric: &'static str,
    pub min_counter_ch: u8,
}

/// Tokens de color en espacio OKLCH para modos claro y oscuro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorTokens {
    pub light_surface: &'static str,
    pub light_text: &'static str,
    pub light_no_match_border: &'static str,
    pub light_no_match_text: &'static str,
    pub dark_surface: &'static str,
    pub dark_text: &'static str,
    pub dark_no_match_border: &'static str,
    pub dark_no_match_text: &'static str,
}

/// Copy de interfaz en español (sentence-case, verb-first, sin exclamaciones).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FindBarCopy {
    pub input_label: &'static str,
    pub input_placeholder: &'static str,
    pub prev_match_label: &'static str,
    pub next_match_label: &'static str,
    pub case_sensitive_toggle: &'static str,
    pub clear_query_label: &'static str,
    pub close_bar_label: &'static str,
    pub no_matches_badge: &'static str,
    pub wrap_forward_notice: &'static str,
    pub wrap_backward_notice: &'static str,
}

/// Dirección de navegación entre coincidencias.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapDirection {
    Forward,
    Backward,
}

/// Estado normalizado del contador de coincidencias de la barra de búsqueda.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchFeedbackState<'a> {
    /// Consulta vacía: no se muestran errores ni contadores.
    Idle,
    /// Hay al menos una coincidencia (`current` es 1-based).
    Matched {
        current: usize,
        total: usize,
        wrapped: Option<WrapDirection>,
    },
    /// Consulta activa sin resultados en el documento.
    NoMatches { query: &'a str },
}

/// Salida lista para renderizar en UI y anunciar por lector de pantalla.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchFeedbackOutput {
    /// Texto compacto del contador (`"3 de 12"`, `"Sin coincidencias"`, o vacío).
    pub counter_text: String,
    /// Anuncio completo para la región viva (`aria-live="polite"`).
    pub aria_announcement: String,
    /// Indica si se debe mostrar el indicador estático de cero coincidencias.
    pub is_no_matches: bool,
    /// Indica si la búsqueda dio la vuelta al inicio o al final del documento.
    pub did_wrap: bool,
}

/// Devuelve la especificación completa de la regla UX-49.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_49_find_bar_match_feedback",
        title: "Retroalimentación de coincidencias en la barra de búsqueda",
        category: "find_bar",
        summary: "Contador tabular-nums, estado de cero coincidencias con señal estática, aviso de reinicio de búsqueda, radios concéntricos, contraste OKLCH y controles verb-first en español.",
        motion: MotionSpec {
            emphasized_easing: "cubic-bezier(0.2, 0, 0, 1)",
            press_scale: 0.96,
            press_duration_ms: 150,
            press_easing: "ease-out",
            feedback_transition_ms: 120,
            reduced_motion_query: "@media (prefers-reduced-motion: reduce)",
            hover_capable_query: "@media (hover: hover)",
        },
        radii: ConcentricRadii {
            outer_px: 14,
            padding_px: 6,
            inner_px: 8,
        },
        typography: TypographySpec {
            counter_font_variant_numeric: "tabular-nums",
            min_counter_ch: 7,
        },
        colors: ColorTokens {
            light_surface: "oklch(0.985 0.002 247)",
            light_text: "oklch(0.205 0.015 247)",
            light_no_match_border: "oklch(0.55 0.16 25)",
            light_no_match_text: "oklch(0.45 0.16 25)",
            dark_surface: "oklch(0.21 0.012 247)",
            dark_text: "oklch(0.95 0.005 247)",
            dark_no_match_border: "oklch(0.70 0.14 25)",
            dark_no_match_text: "oklch(0.78 0.12 25)",
        },
        copy: FindBarCopy {
            input_label: "Buscar en la página",
            input_placeholder: "Ej. configuración",
            prev_match_label: "Ir a la coincidencia anterior",
            next_match_label: "Ir a la coincidencia siguiente",
            case_sensitive_toggle: "Coincidir mayúsculas y minúsculas",
            clear_query_label: "Limpiar búsqueda",
            close_bar_label: "Cerrar barra de búsqueda",
            no_matches_badge: "Sin coincidencias",
            wrap_forward_notice: "Búsqueda reiniciada desde el principio",
            wrap_backward_notice: "Búsqueda reiniciada desde el final",
        },
    }
}

/// Construye el estado de retroalimentación a partir de la consulta, índice actual (1-based) y total.
pub fn build_feedback_state<'a>(
    query: &'a str,
    current_one_based: usize,
    total: usize,
    wrapped: Option<WrapDirection>,
) -> MatchFeedbackState<'a> {
    if query.is_empty() {
        return MatchFeedbackState::Idle;
    }
    if total == 0 {
        return MatchFeedbackState::NoMatches { query };
    }
    let clamped_current = current_one_based.clamp(1, total);
    MatchFeedbackState::Matched {
        current: clamped_current,
        total,
        wrapped,
    }
}

/// Formatea el contador visible y el anuncio accesible para la barra de búsqueda.
pub fn format_match_feedback(state: MatchFeedbackState<'_>) -> MatchFeedbackOutput {
    let copy = spec().copy;
    match state {
        MatchFeedbackState::Idle => MatchFeedbackOutput {
            counter_text: String::new(),
            aria_announcement: String::new(),
            is_no_matches: false,
            did_wrap: false,
        },
        MatchFeedbackState::NoMatches { query } => MatchFeedbackOutput {
            counter_text: copy.no_matches_badge.to_string(),
            aria_announcement: format!("Sin coincidencias para «{}»", query),
            is_no_matches: true,
            did_wrap: false,
        },
        MatchFeedbackState::Matched {
            current,
            total,
            wrapped,
        } => {
            let counter_text = format!("{} de {}", current, total);
            let aria_announcement = match wrapped {
                Some(WrapDirection::Forward) => {
                    format!(
                        "Coincidencia {} de {}, {}",
                        current, total, copy.wrap_forward_notice
                    )
                }
                Some(WrapDirection::Backward) => {
                    format!(
                        "Coincidencia {} de {}, {}",
                        current, total, copy.wrap_backward_notice
                    )
                }
                None => format!("Coincidencia {} de {}", current, total),
            };
            MatchFeedbackOutput {
                counter_text,
                aria_announcement,
                is_no_matches: false,
                did_wrap: wrapped.is_some(),
            }
        }
    }
}

/// Fragmento CSS con radios concéntricos, `tabular-nums`, `:not(:disabled):active` a `0.96`
/// y compuertas de `hover: hover` y `prefers-reduced-motion: reduce`.
pub fn css_fragment() -> &'static str {
    r#".search-find-bar {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px;
  border-radius: 14px;
  border: 1px solid transparent;
  background-color: oklch(0.985 0.002 247);
  color: oklch(0.205 0.015 247);
  box-shadow: 0 2px 6px oklch(0 0 0 / 0.08), 0 8px 24px oklch(0 0 0 / 0.12);
  transition-property: opacity, transform;
  transition-duration: 150ms;
  transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
}

.search-find-input {
  border-radius: 8px;
  border: 1px solid oklch(0.75 0.01 247);
  transition-property: border-color, background-color;
  transition-duration: 120ms;
  transition-timing-function: ease-out;
}

.search-find-bar[data-state="no-matches"] .search-find-input {
  border-color: oklch(0.55 0.16 25);
}

.search-find-counter {
  font-variant-numeric: tabular-nums;
  min-width: 7ch;
  text-align: right;
}

.search-find-bar[data-state="no-matches"] .search-find-counter {
  color: oklch(0.45 0.16 25);
}

.search-find-btn {
  border-radius: 8px;
  -webkit-tap-highlight-color: transparent;
  transition-property: scale, background-color, opacity;
  transition-duration: 150ms;
  transition-timing-function: ease-out;
}

.search-find-btn:not(:disabled):active {
  scale: 0.96;
}

@media (hover: hover) {
  .search-find-btn:not(:disabled):hover {
    background-color: oklch(0 0 0 / 0.06);
  }
}

@media (prefers-reduced-motion: reduce) {
  .search-find-bar {
    transition-property: opacity;
    transition-duration: 120ms;
  }
}
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_exposes_exact_ui_and_copy_tokens() {
        let s = spec();
        assert_eq!(s.id, "ux_49_find_bar_match_feedback");
        assert_eq!(s.motion.emphasized_easing, "cubic-bezier(0.2, 0, 0, 1)");
        assert!((s.motion.press_scale - 0.96).abs() < f32::EPSILON);
        assert_eq!(s.motion.press_duration_ms, 150);
        assert_eq!(s.motion.press_easing, "ease-out");
        assert!(s.radii.is_concentric());
        assert_eq!(s.typography.counter_font_variant_numeric, "tabular-nums");
        assert!(s.colors.light_surface.starts_with("oklch("));
        assert!(s.colors.dark_no_match_border.starts_with("oklch("));
        assert_eq!(s.copy.prev_match_label, "Ir a la coincidencia anterior");
        assert_eq!(s.copy.next_match_label, "Ir a la coincidencia siguiente");
        assert_eq!(s.copy.close_bar_label, "Cerrar barra de búsqueda");
        assert!(!s.copy.no_matches_badge.contains('!'));
    }

    #[test]
    fn match_feedback_handles_idle_matched_wrapped_and_zero_matches() {
        let idle = format_match_feedback(build_feedback_state("", 0, 0, None));
        assert_eq!(idle.counter_text, "");
        assert!(!idle.is_no_matches);
        assert!(!idle.did_wrap);

        let no_match = format_match_feedback(build_feedback_state("xyz", 0, 0, None));
        assert_eq!(no_match.counter_text, "Sin coincidencias");
        assert_eq!(no_match.aria_announcement, "Sin coincidencias para «xyz»");
        assert!(no_match.is_no_matches);
        assert!(!no_match.did_wrap);

        let wrapped = format_match_feedback(build_feedback_state(
            "buscar",
            99,
            12,
            Some(WrapDirection::Forward),
        ));
        assert_eq!(wrapped.counter_text, "12 de 12");
        assert!(wrapped.did_wrap);
        assert!(!wrapped.is_no_matches);
        assert!(wrapped
            .aria_announcement
            .contains("Búsqueda reiniciada desde el principio"));
    }

    #[test]
    fn css_fragment_respects_motion_hover_and_no_transition_all() {
        let css = css_fragment();
        assert!(!css.contains("transition: all"));
        assert!(!css.contains("transition-property: all"));
        assert!(css.contains("font-variant-numeric: tabular-nums;"));
        assert!(css.contains("cubic-bezier(0.2, 0, 0, 1)"));
        assert!(css.contains(".search-find-btn:not(:disabled):active"));
        assert!(css.contains("scale: 0.96;"));
        assert!(css.contains("@media (hover: hover)"));
        assert!(css.contains("@media (prefers-reduced-motion: reduce)"));
        assert!(css.contains("border: 1px solid transparent;"));
    }
}
