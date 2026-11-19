//! UX-27: Indicadores visuales y accesibles de contenido oculto (scroll, truncamiento y plegado).
//!
//! Este módulo define las reglas, constantes de diseño, estados reactivos, fragmentos CSS y textos
//! en español (en minúscula inicial tipo oración y verbo primero) para comunicar de forma clara
//! cuando existe contenido oculto por desplazamiento, truncamiento o secciones contraídas.

/// Especificación declarativa de la regla UX para indicadores de contenido oculto.
#[derive(Debug, Clone, PartialEq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub easing: &'static str,
    pub press_scale: f32,
    pub press_duration_ms: u16,
    pub reveal_duration_ms: u16,
    pub exit_duration_ms: u16,
    pub tabular_nums: bool,
    pub overscroll_behavior: &'static str,
    pub reduced_motion_fallback: &'static str,
}

/// Curva de aceleración exacta para transiciones de iconos e indicadores contextuales.
pub const EASING_EMPHASIS: &str = "cubic-bezier(0.2, 0, 0, 1)";

/// Escala exacta al presionar botones de revelación o expansión.
pub const PRESS_SCALE: f32 = 0.96;

/// Duración de respuesta en interacciones de alta frecuencia (ms).
pub const PRESS_DURATION_MS: u16 = 150;

/// Duración de revelación/cruzado de iconos contextuales (ms).
pub const REVEAL_DURATION_MS: u16 = 300;

/// Duración de salida (más corta que la entrada) (ms).
pub const EXIT_DURATION_MS: u16 = 150;

/// Límite máximo de relleno (px) para aplicar radios concéntricos.
pub const CONCENTRIC_PADDING_CUTOFF_PX: u16 = 24;

/// Grosor de trazo de icono alineado con texto de peso 400 (px).
pub const ICON_STROKE_PX: f32 = 1.5;

/// Tokens de contraste OKLCH para bordes sutiles e indicadores sobre fondos claros y oscuros.
pub const EDGE_OUTLINE_LIGHT_OKLCH: &str = "oklch(0 0 0 / 0.1)";
pub const EDGE_OUTLINE_DARK_OKLCH: &str = "oklch(1 0 0 / 0.1)";
pub const CUE_TEXT_LIGHT_OKLCH: &str = "oklch(0.26 0.01 260)";
pub const CUE_TEXT_DARK_OKLCH: &str = "oklch(0.92 0.01 260)";
pub const CUE_SURFACE_LIGHT_OKLCH: &str = "oklch(0.97 0.005 260)";
pub const CUE_SURFACE_DARK_OKLCH: &str = "oklch(0.22 0.01 260)";

/// Devuelve la especificación completa de UX-27.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_27_hidden_content_cues",
        title: "Indicadores de contenido oculto",
        summary: "Comunica desbordamiento por scroll, texto truncado y secciones plegadas con pistas estáticas, numéricas tabulares y accesibles.",
        easing: EASING_EMPHASIS,
        press_scale: PRESS_SCALE,
        press_duration_ms: PRESS_DURATION_MS,
        reveal_duration_ms: REVEAL_DURATION_MS,
        exit_duration_ms: EXIT_DURATION_MS,
        tabular_nums: true,
        overscroll_behavior: "contain",
        reduced_motion_fallback: "opacity-crossfade",
    }
}

/// Calcula el radio exterior concéntrico cuando un contenedor acolchado envuelve un indicador.
///
/// Si el relleno supera `24px`, las capas se tratan como superficies independientes y se conserva
/// el radio propio de la superficie exterior.
pub fn concentric_outer_radius_px(
    inner_radius_px: u16,
    padding_px: u16,
    border_px: u16,
    independent_outer_token_px: u16,
) -> u16 {
    if padding_px > CONCENTRIC_PADDING_CUTOFF_PX {
        independent_outer_token_px
    } else {
        inner_radius_px
            .saturating_add(padding_px)
            .saturating_add(border_px)
    }
}

/// Estado de los indicadores de desbordamiento en los extremos de un contenedor desplazable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollEdgeCues {
    pub has_before_cue: bool,
    pub has_after_cue: bool,
    pub is_scrollable: bool,
}

impl ScrollEdgeCues {
    /// Evalúa qué extremos tienen contenido oculto fuera del área visible.
    pub fn evaluate(
        scroll_offset_px: f32,
        viewport_extent_px: f32,
        content_extent_px: f32,
        threshold_px: f32,
    ) -> Self {
        if !scroll_offset_px.is_finite()
            || !viewport_extent_px.is_finite()
            || !content_extent_px.is_finite()
            || !threshold_px.is_finite()
            || viewport_extent_px <= 0.0
            || content_extent_px <= viewport_extent_px + threshold_px.max(0.0)
        {
            return Self {
                has_before_cue: false,
                has_after_cue: false,
                is_scrollable: false,
            };
        }

        let safe_threshold = threshold_px.max(0.0);
        let max_offset = (content_extent_px - viewport_extent_px).max(0.0);
        let clamped_offset = scroll_offset_px.clamp(0.0, max_offset);

        Self {
            has_before_cue: clamped_offset > safe_threshold,
            has_after_cue: (max_offset - clamped_offset) > safe_threshold,
            is_scrollable: true,
        }
    }

    /// Devuelve las clases CSS aplicables al contenedor según los extremos con contenido oculto.
    pub fn css_classes(&self) -> Vec<&'static str> {
        let mut classes = Vec::with_capacity(3);
        if self.is_scrollable {
            classes.push("ux-scroll-cue");
        }
        if self.has_before_cue {
            classes.push("ux-scroll-cue--has-before");
        }
        if self.has_after_cue {
            classes.push("ux-scroll-cue--has-after");
        }
        classes
    }
}

/// Estado accesible y visual de un control que revela u oculta contenido colapsado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisclosureCueState {
    pub expanded: bool,
    pub hidden_count: usize,
}

impl DisclosureCueState {
    pub const fn new(expanded: bool, hidden_count: usize) -> Self {
        Self {
            expanded,
            hidden_count,
        }
    }

    /// Valor del atributo `aria-expanded`.
    pub const fn aria_expanded(&self) -> &'static str {
        if self.expanded {
            "true"
        } else {
            "false"
        }
    }

    /// Etiqueta de botón en español, tipo oración y verbo primero.
    pub fn button_label(&self) -> String {
        if self.expanded {
            "Ocultar elementos adicionales".to_string()
        } else if self.hidden_count == 0 {
            "Mostrar contenido".to_string()
        } else if self.hidden_count == 1 {
            "Mostrar 1 elemento más".to_string()
        } else {
            format!("Mostrar {} elementos más", self.hidden_count)
        }
    }

    /// Indicador compacto para insignias numéricas con `tabular-nums`.
    pub fn compact_badge(&self) -> Option<String> {
        if self.expanded || self.hidden_count == 0 {
            None
        } else {
            Some(format!("+{}", self.hidden_count))
        }
    }

    /// Pista estática textual para pantallas sin animación o lectores de pantalla.
    pub fn static_status_cue(&self) -> String {
        if self.expanded {
            "Contenido visible".to_string()
        } else if self.hidden_count == 1 {
            "1 elemento oculto".to_string()
        } else if self.hidden_count > 1 {
            format!("{} elementos ocultos", self.hidden_count)
        } else {
            "Sección contraída".to_string()
        }
    }
}

/// Etiqueta de resumen para pestañas o elementos fuera de vista en una tira horizontal/vertical.
pub fn hidden_tabs_cue(hidden_count: usize) -> Option<String> {
    match hidden_count {
        0 => None,
        1 => Some("Mostrar 1 pestaña oculta".to_string()),
        n => Some(format!("Mostrar {n} pestañas ocultas")),
    }
}

/// Información de pista cuando un texto o URL ha sido truncado visualmente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncationCue {
    pub is_truncated: bool,
    pub full_text: String,
    pub accessible_hint: Option<&'static str>,
}

impl TruncationCue {
    /// Construye la pista comparando longitud visible máxima frente al texto completo.
    pub fn from_char_limit(full_text: &str, max_visible_chars: usize) -> Self {
        let char_count = full_text.chars().count();
        let is_truncated = char_count > max_visible_chars;
        Self {
            is_truncated,
            full_text: full_text.to_string(),
            accessible_hint: if is_truncated {
                Some("Ver texto completo")
            } else {
                None
            },
        }
    }
}

/// Fragmento CSS autocontenido para indicadores de contenido oculto.
///
/// Incluye:
/// - `tabular-nums` en contadores numéricos.
/// - `overscroll-behavior: contain` en paneles desplazables.
/// - Escala `0.96` al presionar botones habilitados.
/// - Curva `cubic-bezier(0.2, 0, 0, 1)` y transición explícita de propiedades.
/// - Efectos `:hover` restringidos a `@media (hover: hover)`.
/// - Alternativa por opacidad bajo `@media (prefers-reduced-motion: reduce)`.
pub fn css_fragment() -> &'static str {
    r#"/* UX-27: Indicadores de contenido oculto */
.ux-scroll-cue {
    position: relative;
    overscroll-behavior: contain;
    -webkit-tap-highlight-color: transparent;
}

.ux-scroll-cue--has-before {
    box-shadow: inset 0 12px 12px -12px oklch(0 0 0 / 0.18);
}

.ux-scroll-cue--has-after {
    box-shadow: inset 0 -12px 12px -12px oklch(0 0 0 / 0.18);
}

.ux-hidden-cue-button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: 1px solid transparent;
    border-radius: 8px;
    padding: 4px 10px;
    color: oklch(0.26 0.01 260);
    background-color: oklch(0.97 0.005 260);
    font-variant-numeric: tabular-nums;
    transition-property: transform, opacity, background-color;
    transition-duration: 150ms;
    transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
}

.ux-hidden-cue-button:not(:disabled):active {
    transform: scale(0.96);
}

.ux-hidden-cue-chevron {
    transition-property: transform, opacity;
    transition-duration: 300ms;
    transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
}

.ux-hidden-cue-button[aria-expanded="true"] .ux-hidden-cue-chevron {
    transform: rotate(180deg);
}

@media (hover: hover) {
    .ux-hidden-cue-button:not(:disabled):hover {
        background-color: oklch(0.93 0.008 260);
    }
}

@media (prefers-reduced-motion: reduce) {
    .ux-hidden-cue-chevron {
        transition-property: opacity;
        transition-duration: 150ms;
        transform: none;
    }

    .ux-hidden-cue-button[aria-expanded="true"] .ux-hidden-cue-chevron {
        transform: none;
    }
}
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_matches_exact_design_tokens() {
        let s = spec();
        assert_eq!(s.id, "ux_27_hidden_content_cues");
        assert_eq!(s.easing, "cubic-bezier(0.2, 0, 0, 1)");
        assert!((s.press_scale - 0.96).abs() < f32::EPSILON);
        assert_eq!(s.press_duration_ms, 150);
        assert_eq!(s.reveal_duration_ms, 300);
        assert_eq!(s.exit_duration_ms, 150);
        assert!(s.tabular_nums);
        assert_eq!(s.overscroll_behavior, "contain");
    }

    #[test]
    fn concentric_radius_respects_cutoff() {
        assert_eq!(concentric_outer_radius_px(8, 6, 1, 20), 15);
        assert_eq!(concentric_outer_radius_px(8, 24, 0, 20), 32);
        assert_eq!(concentric_outer_radius_px(8, 28, 1, 20), 20);
    }

    #[test]
    fn scroll_edge_cues_detect_hidden_overflow() {
        let top_only = ScrollEdgeCues::evaluate(0.0, 200.0, 500.0, 4.0);
        assert!(top_only.is_scrollable);
        assert!(!top_only.has_before_cue);
        assert!(top_only.has_after_cue);
        assert_eq!(
            top_only.css_classes(),
            vec!["ux-scroll-cue", "ux-scroll-cue--has-after"]
        );

        let middle = ScrollEdgeCues::evaluate(120.0, 200.0, 500.0, 4.0);
        assert!(middle.has_before_cue);
        assert!(middle.has_after_cue);

        let bottom = ScrollEdgeCues::evaluate(300.0, 200.0, 500.0, 4.0);
        assert!(bottom.has_before_cue);
        assert!(!bottom.has_after_cue);

        let fits = ScrollEdgeCues::evaluate(0.0, 500.0, 480.0, 4.0);
        assert!(!fits.is_scrollable);
        assert!(fits.css_classes().is_empty());
    }

    #[test]
    fn spanish_verb_first_copy_and_plurals() {
        let single = DisclosureCueState::new(false, 1);
        assert_eq!(single.aria_expanded(), "false");
        assert_eq!(single.button_label(), "Mostrar 1 elemento más");
        assert_eq!(single.compact_badge().as_deref(), Some("+1"));
        assert_eq!(single.static_status_cue(), "1 elemento oculto");

        let multi = DisclosureCueState::new(false, 5);
        assert_eq!(multi.button_label(), "Mostrar 5 elementos más");
        assert_eq!(multi.compact_badge().as_deref(), Some("+5"));
        assert_eq!(multi.static_status_cue(), "5 elementos ocultos");

        let expanded = DisclosureCueState::new(true, 5);
        assert_eq!(expanded.aria_expanded(), "true");
        assert_eq!(expanded.button_label(), "Ocultar elementos adicionales");
        assert_eq!(expanded.compact_badge(), None);

        assert_eq!(
            hidden_tabs_cue(1).as_deref(),
            Some("Mostrar 1 pestaña oculta")
        );
        assert_eq!(
            hidden_tabs_cue(3).as_deref(),
            Some("Mostrar 3 pestañas ocultas")
        );
        assert_eq!(hidden_tabs_cue(0), None);
    }

    #[test]
    fn css_fragment_contains_required_rules() {
        let css = css_fragment();
        assert!(css.contains("cubic-bezier(0.2, 0, 0, 1)"));
        assert!(css.contains("scale(0.96)"));
        assert!(css.contains("tabular-nums"));
        assert!(css.contains("overscroll-behavior: contain"));
        assert!(css.contains("@media (hover: hover)"));
        assert!(css.contains("@media (prefers-reduced-motion: reduce)"));
        assert!(css.contains("border: 1px solid transparent"));
        assert!(!css.contains("transition: all"));
        assert!(!css.contains("transition-property: all"));
    }

    #[test]
    fn truncation_cue_marks_long_text() {
        let short = TruncationCue::from_char_limit("Ajustes", 20);
        assert!(!short.is_truncated);
        assert_eq!(short.accessible_hint, None);

        let long =
            TruncationCue::from_char_limit("https://ejemplo.org/ruta/muy/larga/de/prueba", 20);
        assert!(long.is_truncated);
        assert_eq!(long.accessible_hint, Some("Ver texto completo"));
    }
}
