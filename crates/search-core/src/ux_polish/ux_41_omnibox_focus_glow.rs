//! UX-41: Resplandor de foco del omnibox (`ux_41_omnibox_focus_glow`).
//!
//! Especificacion pura en Rust para el estado de foco de la barra de direcciones
//! (`entry.search-address-entry`) en Search Linux:
//! - Anillo y resplandor por capas con `box-shadow` transparente + borde solido de `1px`
//!   para conservar el contraste en modo de colores forzados.
//! - Transicion explicita de propiedades (`background-color`, `border-color`, `box-shadow`)
//!   con curva `cubic-bezier(0.2, 0, 0, 1)` y duracion `<= 150ms` para interaccion de alta
//!   frecuencia, reducida a `0ms` cuando `reduced_motion` esta activo.
//! - Radios concentricos (`radio_externo = radio_interno + relleno + borde`).
//! - Etiquetas accesibles en espanol con verbo inicial en estilo oracion.

/// Identificador unico de la regla UX.
pub const RULE_ID: &str = "ux_41_omnibox_focus_glow";

/// Curva de aceleracion estandar para transiciones de estado del omnibox.
pub const FOCUS_EASING: &str = "cubic-bezier(0.2, 0, 0, 1)";

/// Duracion en milisegundos para la transicion de foco (interaccion de alta frecuencia <= 150ms).
pub const FOCUS_DURATION_MS: u16 = 140;

/// Duracion en milisegundos cuando el usuario prefiere movimiento reducido.
pub const REDUCED_MOTION_DURATION_MS: u16 = 0;

/// Especificacion estructurada de la regla UX.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub easing: &'static str,
    pub duration_ms: u16,
    pub reduced_motion_duration_ms: u16,
    pub accessible_label_es: &'static str,
    pub placeholder_example_es: &'static str,
    pub insecure_focus_hint_es: &'static str,
    pub css_snippet: &'static str,
}

/// Estado visual e interactivo del omnibox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OmniboxFocusState {
    Rest,
    Hover,
    Focused,
    FocusedInsecure,
}

/// Tokens numericos exactos para el resplandor de foco y geometria concentrica.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusGlowTokens {
    pub outer_radius_px: u8,
    pub inner_radius_px: u8,
    pub inner_padding_px: u8,
    pub border_width_px: u8,
    pub ring_spread_px: u8,
    pub glow_blur_px: u8,
    pub glow_offset_y_px: u8,
    pub duration_ms: u16,
    pub easing: &'static str,
}

/// Devuelve los tokens exactos del resplandor de foco del omnibox.
pub const fn focus_glow_tokens() -> FocusGlowTokens {
    FocusGlowTokens {
        outer_radius_px: 10,
        inner_radius_px: 7,
        inner_padding_px: 2,
        border_width_px: 1,
        ring_spread_px: 2,
        glow_blur_px: 10,
        glow_offset_y_px: 2,
        duration_ms: FOCUS_DURATION_MS,
        easing: FOCUS_EASING,
    }
}

/// Calcula el radio exterior concentrico a partir del radio interior, relleno y borde.
pub const fn concentric_outer_radius(inner_radius_px: u8, padding_px: u8, border_px: u8) -> u8 {
    inner_radius_px
        .saturating_add(padding_px)
        .saturating_add(border_px)
}

/// Devuelve la descripcion corta en espanol del estado de foco para depuracion o ARIA.
pub const fn state_descriptor(state: OmniboxFocusState) -> &'static str {
    match state {
        OmniboxFocusState::Rest => "Barra de direcciones en reposo",
        OmniboxFocusState::Hover => "Barra de direcciones bajo el puntero",
        OmniboxFocusState::Focused => "Barra de direcciones enfocada",
        OmniboxFocusState::FocusedInsecure => "Barra de direcciones enfocada en sitio sin cifrar",
    }
}

/// Fragmento CSS GTK4 canonico para el estado de foco con resplandor en capas.
pub const OMNIBOX_FOCUS_GLOW_CSS: &str = r#"entry.search-address-entry {
    min-height: 31px;
    border-radius: 10px;
    padding-left: 12px;
    padding-right: 12px;
    font-size: 0.94rem;
    background-color: alpha(currentColor, 0.055);
    border: 1px solid alpha(currentColor, 0.14);
    box-shadow: none;
    transition-property: background-color, border-color, box-shadow;
    transition-duration: 140ms;
    transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
}

entry.search-address-entry:hover {
    background-color: alpha(currentColor, 0.08);
    border-color: alpha(currentColor, 0.22);
}

entry.search-address-entry:focus-within {
    background-color: @view_bg_color;
    border: 1px solid alpha(@accent_bg_color, 0.78);
    box-shadow: 0 0 0 2px alpha(@accent_bg_color, 0.26), 0 2px 10px alpha(@accent_bg_color, 0.14);
}

entry.search-address-entry.insecure-http {
    border: 1px solid alpha(@warning_color, 0.72);
}

entry.search-address-entry.insecure-http:focus-within {
    border: 1px solid alpha(@warning_color, 0.88);
    box-shadow: 0 0 0 2px alpha(@warning_color, 0.24), 0 2px 10px alpha(@warning_color, 0.12);
}"#;

/// Genera la hoja CSS GTK4 aplicando el modo de movimiento reducido si corresponde.
pub fn render_gtk_css(reduced_motion: bool) -> String {
    if reduced_motion {
        OMNIBOX_FOCUS_GLOW_CSS.replace("transition-duration: 140ms;", "transition-duration: 0ms;")
    } else {
        OMNIBOX_FOCUS_GLOW_CSS.to_string()
    }
}

/// Devuelve la especificacion completa de la regla UX-41.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: RULE_ID,
        title: "Resplandor de foco del omnibox con borde accesible y transicion nitida",
        category: "ui_polish_focus_glow",
        easing: FOCUS_EASING,
        duration_ms: FOCUS_DURATION_MS,
        reduced_motion_duration_ms: REDUCED_MOTION_DURATION_MS,
        accessible_label_es: "Buscar o escribir dirección",
        placeholder_example_es: "ejemplo.com",
        insecure_focus_hint_es: "Conexión sin cifrar. Revisa la dirección antes de enviar datos.",
        css_snippet: OMNIBOX_FOCUS_GLOW_CSS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_matches_exact_motion_and_copy_rules() {
        let s = spec();
        assert_eq!(s.id, "ux_41_omnibox_focus_glow");
        assert_eq!(s.easing, "cubic-bezier(0.2, 0, 0, 1)");
        assert!(s.duration_ms <= 150);
        assert_eq!(s.reduced_motion_duration_ms, 0);
        assert_eq!(s.accessible_label_es, "Buscar o escribir dirección");
        assert_eq!(s.placeholder_example_es, "ejemplo.com");
        assert!(!s.insecure_focus_hint_es.contains('!'));
        assert!(!s.accessible_label_es.ends_with('.'));
    }

    #[test]
    fn concentric_radii_are_consistent() {
        let tokens = focus_glow_tokens();
        let computed = concentric_outer_radius(
            tokens.inner_radius_px,
            tokens.inner_padding_px,
            tokens.border_width_px,
        );
        assert_eq!(computed, tokens.outer_radius_px);
        assert_eq!(tokens.outer_radius_px, 10);
    }

    #[test]
    fn gtk_css_avoids_transition_all_and_preserves_forced_colors_border() {
        let css = render_gtk_css(false);
        assert!(!css.contains("transition: all"));
        assert!(!css.contains("transition-property: all"));
        assert!(css.contains("transition-property: background-color, border-color, box-shadow;"));
        assert!(css.contains("cubic-bezier(0.2, 0, 0, 1)"));
        assert!(css.contains("border: 1px solid alpha(@accent_bg_color, 0.78);"));
        assert!(css.contains("entry.search-address-entry.insecure-http:focus-within"));
        assert!(css.contains("box-shadow: 0 0 0 2px alpha(@accent_bg_color, 0.26), 0 2px 10px alpha(@accent_bg_color, 0.14);"));
    }

    #[test]
    fn reduced_motion_sets_zero_ms_duration() {
        let css = render_gtk_css(true);
        assert!(css.contains("transition-duration: 0ms;"));
        assert!(!css.contains("transition-duration: 140ms;"));
    }

    #[test]
    fn state_descriptors_are_non_empty_spanish_labels() {
        let states = [
            OmniboxFocusState::Rest,
            OmniboxFocusState::Hover,
            OmniboxFocusState::Focused,
            OmniboxFocusState::FocusedInsecure,
        ];
        for state in states {
            let desc = state_descriptor(state);
            assert!(!desc.is_empty());
            assert!(!desc.contains('!'));
        }
    }
}
