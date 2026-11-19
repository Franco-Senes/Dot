//! UX-46: Borde de foco para paneles divididos (`ux_46_split_pane_focus_border`).
//!
//! Especificacion pura en Rust para el borde de foco de Split View en Search Linux:
//! - Borde estructural persistente de `2px` en todos los paneles (`transparent` en reposo,
//!   color de acento con contraste >= 3:1 en foco) para evitar saltos de maquetacion al
//!   cambiar de panel.
//! - Resplandor en capas con `box-shadow` transparente y respaldo obligatorio en
//!   `@media (forced-colors: active)` usando `Highlight`.
//! - Radios concentricos exactos (`radio_externo = radio_interno + relleno + borde`).
//! - Transicion explicita (`border-color`, `box-shadow`, `opacity`) en `140ms` con
//!   `cubic-bezier(0.2, 0, 0, 1)` para interaccion de alta frecuencia, reducida a `0ms`
//!   bajo `prefers-reduced-motion: reduce`.
//! - Etiquetas ARIA y acciones con verbo inicial en espanol en estilo oracion.

use std::fmt;

/// Identificador unico de la regla UX.
pub const RULE_ID: &str = "ux_46_split_pane_focus_border";

/// Curva de aceleracion canonica para transiciones de estado interactivo.
pub const FOCUS_EASING: &str = "cubic-bezier(0.2, 0, 0, 1)";

/// Duracion en milisegundos para el cambio de foco entre paneles (alta frecuencia <= 150ms).
pub const FOCUS_DURATION_MS: u16 = 140;

/// Duracion en milisegundos cuando `prefers-reduced-motion: reduce` esta activo.
pub const REDUCED_MOTION_DURATION_MS: u16 = 0;

/// Ancho exacto del borde del panel en pixeles CSS (sin desplazamiento de layout).
pub const PANE_BORDER_WIDTH_PX: u8 = 2;

/// Contraste minimo no textual requerido por WCAG 1.4.11 / 2.4.13.
pub const MIN_NON_TEXT_CONTRAST: f32 = 3.0;

/// Escala de pulsacion estandar para controles de barra de panel (`better-ui`).
pub const CONTROL_PRESS_SCALE: f32 = 0.96;

/// Etiquetas de accion con verbo inicial en espanol (estilo oracion) para Split View.
pub const ACTION_FOCUS_PANE_ES: &str = "Enfocar panel";
pub const ACTION_SWAP_PANES_ES: &str = "Intercambiar paneles";
pub const ACTION_SEPARATE_PANE_ES: &str = "Separar panel";
pub const ACTION_EQUALIZE_PANES_ES: &str = "Igualar tamano de paneles";

/// Especificacion estructurada de la regla UX-46.
#[derive(Debug, Clone, PartialEq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub easing: &'static str,
    pub duration_ms: u16,
    pub reduced_motion_duration_ms: u16,
    pub min_contrast_ratio: f32,
    pub tokens: SplitPaneFocusTokens,
    pub primary_actions_es: [&'static str; 4],
    pub css_snippet: &'static str,
}

/// Tokens numericos exactos para el borde de foco de un panel dividido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitPaneFocusTokens {
    pub outer_radius_px: u8,
    pub inner_radius_px: u8,
    pub inner_padding_px: u8,
    pub border_width_px: u8,
    pub ring_spread_px: u8,
    pub duration_ms: u16,
    pub reduced_motion_duration_ms: u16,
    pub easing: &'static str,
}

/// Estado interactivo de un panel dentro de Split View.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneFocusState {
    Inactive,
    Hovered,
    Focused,
    KeyboardFocused,
}

/// Propiedades resueltas para pintar el borde y la senal estatica de un panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitPaneBorderStyle {
    pub border_width_px: u8,
    pub border_color: &'static str,
    pub box_shadow: &'static str,
    pub transition_duration_ms: u16,
    pub aria_current: Option<&'static str>,
    pub state_label_es: &'static str,
}

/// Errores de validacion para fragmentos CSS de borde de foco en paneles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitPaneCssError {
    UsesTransitionAll,
    MissingCanonicalEasing,
    MissingHoverCapabilityGate,
    MissingReducedMotionGate,
    MissingForcedColorsFallback,
}

impl fmt::Display for SplitPaneCssError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UsesTransitionAll => {
                write!(f, "no usar transition: all; nombrar propiedades exactas")
            }
            Self::MissingCanonicalEasing => {
                write!(f, "falta la curva canonica cubic-bezier(0.2, 0, 0, 1)")
            }
            Self::MissingHoverCapabilityGate => {
                write!(
                    f,
                    "los estilos :hover deben ir dentro de @media (hover: hover)"
                )
            }
            Self::MissingReducedMotionGate => {
                write!(
                    f,
                    "falta el respaldo @media (prefers-reduced-motion: reduce)"
                )
            }
            Self::MissingForcedColorsFallback => {
                write!(f, "falta el respaldo @media (forced-colors: active)")
            }
        }
    }
}

/// Calcula el radio exterior concentrico: `radio_interno + relleno + borde`.
pub const fn concentric_outer_radius(inner_radius_px: u8, padding_px: u8, border_px: u8) -> u8 {
    inner_radius_px
        .saturating_add(padding_px)
        .saturating_add(border_px)
}

/// Devuelve los tokens exactos para el borde de foco de Split View.
pub const fn focus_border_tokens() -> SplitPaneFocusTokens {
    let inner_radius_px = 8;
    let inner_padding_px = 2;
    let border_width_px = PANE_BORDER_WIDTH_PX;
    SplitPaneFocusTokens {
        outer_radius_px: concentric_outer_radius(
            inner_radius_px,
            inner_padding_px,
            border_width_px,
        ),
        inner_radius_px,
        inner_padding_px,
        border_width_px,
        ring_spread_px: 2,
        duration_ms: FOCUS_DURATION_MS,
        reduced_motion_duration_ms: REDUCED_MOTION_DURATION_MS,
        easing: FOCUS_EASING,
    }
}

/// Resuelve el estilo visual y semantico del borde para un estado y preferencias dadas.
pub const fn resolve_pane_border(
    state: PaneFocusState,
    forced_colors: bool,
    reduced_motion: bool,
) -> SplitPaneBorderStyle {
    let duration = if reduced_motion {
        REDUCED_MOTION_DURATION_MS
    } else {
        FOCUS_DURATION_MS
    };

    if forced_colors {
        return match state {
            PaneFocusState::Inactive | PaneFocusState::Hovered => SplitPaneBorderStyle {
                border_width_px: PANE_BORDER_WIDTH_PX,
                border_color: "transparent",
                box_shadow: "none",
                transition_duration_ms: duration,
                aria_current: None,
                state_label_es: "Panel secundario inactivo",
            },
            PaneFocusState::Focused | PaneFocusState::KeyboardFocused => SplitPaneBorderStyle {
                border_width_px: PANE_BORDER_WIDTH_PX,
                border_color: "Highlight",
                box_shadow: "none",
                transition_duration_ms: duration,
                aria_current: Some("true"),
                state_label_es: "Panel activo enfocado",
            },
        };
    }

    match state {
        PaneFocusState::Inactive => SplitPaneBorderStyle {
            border_width_px: PANE_BORDER_WIDTH_PX,
            border_color: "transparent",
            box_shadow: "none",
            transition_duration_ms: duration,
            aria_current: None,
            state_label_es: "Panel secundario inactivo",
        },
        PaneFocusState::Hovered => SplitPaneBorderStyle {
            border_width_px: PANE_BORDER_WIDTH_PX,
            border_color: "oklch(0.68 0.02 250 / 0.35)",
            box_shadow: "none",
            transition_duration_ms: duration,
            aria_current: None,
            state_label_es: "Panel bajo el puntero",
        },
        PaneFocusState::Focused => SplitPaneBorderStyle {
            border_width_px: PANE_BORDER_WIDTH_PX,
            border_color: "var(--accent-border, oklch(0.62 0.18 250))",
            box_shadow: "0 0 0 2px oklch(0.62 0.18 250 / 0.24)",
            transition_duration_ms: duration,
            aria_current: Some("true"),
            state_label_es: "Panel activo enfocado",
        },
        PaneFocusState::KeyboardFocused => SplitPaneBorderStyle {
            border_width_px: PANE_BORDER_WIDTH_PX,
            border_color: "var(--focus-ring, oklch(0.62 0.18 250))",
            box_shadow: "0 0 0 2px oklch(0.62 0.18 250 / 0.36)",
            transition_duration_ms: duration,
            aria_current: Some("true"),
            state_label_es: "Panel activo enfocado con teclado",
        },
    }
}

/// Construye la etiqueta ARIA en espanol para un panel dentro de Split View.
pub fn pane_aria_label_es(
    pane_index_zero_based: usize,
    total_panes: usize,
    focused: bool,
) -> String {
    let total = total_panes.max(1);
    let current = pane_index_zero_based.saturating_add(1).min(total);
    if focused {
        format!("Panel {current} de {total}, activo")
    } else {
        format!("Panel {current} de {total}, inactivo")
    }
}

/// Comprueba que el contraste medido cumpla el minimo no textual (>= 3.0:1).
pub fn meets_border_contrast(measured_ratio: f32) -> bool {
    measured_ratio.is_finite() && measured_ratio >= MIN_NON_TEXT_CONTRAST
}

/// Valida que un fragmento CSS cumpla las reglas de `better-ui` y accesibilidad.
pub fn validate_css_fragment(css: &str) -> Result<(), SplitPaneCssError> {
    if css.contains("transition: all") || css.contains("transition-all") {
        return Err(SplitPaneCssError::UsesTransitionAll);
    }
    if !css.contains(FOCUS_EASING) {
        return Err(SplitPaneCssError::MissingCanonicalEasing);
    }
    if !css.contains("@media (hover: hover)") {
        return Err(SplitPaneCssError::MissingHoverCapabilityGate);
    }
    if !css.contains("@media (prefers-reduced-motion: reduce)") {
        return Err(SplitPaneCssError::MissingReducedMotionGate);
    }
    if !css.contains("@media (forced-colors: active)") {
        return Err(SplitPaneCssError::MissingForcedColorsFallback);
    }
    Ok(())
}

/// Fragmento CSS canonico para el borde de foco de paneles en Split View.
pub const SPLIT_PANE_FOCUS_CSS: &str = r#".split-pane {
    border: 2px solid transparent;
    border-radius: 12px;
    padding: 2px;
    box-shadow: none;
    transition-property: border-color, box-shadow, opacity;
    transition-duration: 140ms;
    transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
}

.split-pane__viewport {
    border-radius: 8px;
    overflow: hidden;
}

@media (hover: hover) {
    .split-pane:not([data-pane-focused="true"]):hover {
        border-color: oklch(0.68 0.02 250 / 0.35);
    }
}

.split-pane[data-pane-focused="true"] {
    border: 2px solid var(--accent-border, oklch(0.62 0.18 250));
    box-shadow: 0 0 0 2px oklch(0.62 0.18 250 / 0.24);
}

.split-pane:focus-visible {
    outline: none;
    border: 2px solid var(--focus-ring, oklch(0.62 0.18 250));
    box-shadow: 0 0 0 2px oklch(0.62 0.18 250 / 0.36);
}

@media (prefers-reduced-motion: reduce) {
    .split-pane {
        transition-duration: 0ms;
    }
}

@media (forced-colors: active) {
    .split-pane {
        border: 2px solid transparent;
        box-shadow: none;
    }

    .split-pane[data-pane-focused="true"],
    .split-pane:focus-visible {
        border: 2px solid Highlight;
        box-shadow: none;
    }
}"#;

/// Devuelve la especificacion completa de UX-46.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: RULE_ID,
        title: "Borde de foco en paneles divididos",
        category: "ui-polish",
        easing: FOCUS_EASING,
        duration_ms: FOCUS_DURATION_MS,
        reduced_motion_duration_ms: REDUCED_MOTION_DURATION_MS,
        min_contrast_ratio: MIN_NON_TEXT_CONTRAST,
        tokens: focus_border_tokens(),
        primary_actions_es: [
            ACTION_FOCUS_PANE_ES,
            ACTION_SWAP_PANES_ES,
            ACTION_SEPARATE_PANE_ES,
            ACTION_EQUALIZE_PANES_ES,
        ],
        css_snippet: SPLIT_PANE_FOCUS_CSS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_exposes_exact_motion_and_concentric_radius_values() {
        let rule = spec();
        assert_eq!(rule.id, "ux_46_split_pane_focus_border");
        assert_eq!(rule.easing, "cubic-bezier(0.2, 0, 0, 1)");
        assert!(rule.duration_ms <= 150);
        assert_eq!(rule.reduced_motion_duration_ms, 0);
        assert_eq!(rule.tokens.border_width_px, 2);
        assert_eq!(
            rule.tokens.outer_radius_px,
            rule.tokens.inner_radius_px
                + rule.tokens.inner_padding_px
                + rule.tokens.border_width_px
        );
        assert_eq!(rule.tokens.outer_radius_px, 12);
        assert!((CONTROL_PRESS_SCALE - 0.96).abs() < f32::EPSILON);
    }

    #[test]
    fn resolve_pane_border_preserves_width_and_handles_forced_colors() {
        let inactive = resolve_pane_border(PaneFocusState::Inactive, false, false);
        let focused = resolve_pane_border(PaneFocusState::Focused, false, false);
        let forced = resolve_pane_border(PaneFocusState::KeyboardFocused, true, true);

        assert_eq!(inactive.border_width_px, 2);
        assert_eq!(focused.border_width_px, 2);
        assert_eq!(inactive.aria_current, None);
        assert_eq!(focused.aria_current, Some("true"));
        assert_eq!(forced.border_color, "Highlight");
        assert_eq!(forced.box_shadow, "none");
        assert_eq!(forced.transition_duration_ms, 0);
    }

    #[test]
    fn css_fragment_passes_validation_and_contrast_check() {
        assert_eq!(validate_css_fragment(SPLIT_PANE_FOCUS_CSS), Ok(()));
        assert!(SPLIT_PANE_FOCUS_CSS
            .contains("transition-property: border-color, box-shadow, opacity;"));
        assert!(meets_border_contrast(3.0));
        assert!(!meets_border_contrast(2.9));
    }

    #[test]
    fn spanish_copy_is_sentence_case_and_verb_first() {
        let rule = spec();
        for action in rule.primary_actions_es {
            assert!(!action.is_empty());
            assert!(!action.ends_with('!'));
            let first = action.chars().next();
            assert!(first.is_some_and(|c| c.is_uppercase()));
        }
        assert_eq!(pane_aria_label_es(0, 2, true), "Panel 1 de 2, activo");
        assert_eq!(pane_aria_label_es(1, 2, false), "Panel 2 de 2, inactivo");
    }
}
