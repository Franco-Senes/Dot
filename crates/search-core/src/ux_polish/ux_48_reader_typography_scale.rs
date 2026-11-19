//! UX-48: Escala tipográfica y ritmo de lectura para el modo lectura de Search Linux.
//!
//! Define una escala tipográfica semántica, medidas de línea acotadas (`60ch`–`75ch`),
//! interlineado sin unidad, pesos mínimos legibles por debajo de `18px`, cifras
//! tabulares para contadores de lectura y controles accesibles en español con
//! verbo inicial.

/// Identificador único de la regla UX dentro del overhaul de 50 puntos.
pub const RULE_ID: &str = "ux_48_reader_typography_scale";

/// Medida mínima y máxima recomendada para lectura prolongada (en `ch`).
pub const MIN_MEASURE_CH: u16 = 60;
pub const DEFAULT_MEASURE_CH: u16 = 68;
pub const MAX_MEASURE_CH: u16 = 75;

/// Límites del tamaño base del lector en píxeles CSS (convertible a `rem`).
pub const MIN_FONT_SIZE_PX: u16 = 14;
pub const DEFAULT_FONT_SIZE_PX: u16 = 18;
pub const MAX_FONT_SIZE_PX: u16 = 32;
pub const ROOT_REM_BASE_PX: f32 = 16.0;

/// Curva de transición estándar y escala de pulsación para los controles del lector.
pub const MOTION_EASING: &str = "cubic-bezier(0.2, 0, 0, 1)";
pub const PRESS_SCALE: &str = "0.96";
pub const PRESS_DURATION_MS: u16 = 150;

/// Especificación declarativa de la regla UX.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub area: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub css_fragment: &'static str,
}

/// Rol tipográfico dentro del modo lectura.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderTextRole {
    H1,
    H2,
    H3,
    Body,
    Byline,
    Caption,
    Overline,
}

/// Token tipográfico completo para un rol: tamaño en `rem`, interlineado sin
/// unidad, peso y espaciado entre letras en `em`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReaderTypeToken {
    pub role: ReaderTextRole,
    pub size_rem: f32,
    pub line_height: f32,
    pub font_weight: u16,
    pub letter_spacing_em: f32,
    pub text_wrap: &'static str,
}

/// Escala tipográfica completa del modo lectura, ordenada de mayor a menor jerarquía.
pub const READER_TYPE_SCALE: [ReaderTypeToken; 7] = [
    ReaderTypeToken {
        role: ReaderTextRole::H1,
        size_rem: 1.75,
        line_height: 1.24,
        font_weight: 650,
        letter_spacing_em: -0.015,
        text_wrap: "balance",
    },
    ReaderTypeToken {
        role: ReaderTextRole::H2,
        size_rem: 1.35,
        line_height: 1.28,
        font_weight: 600,
        letter_spacing_em: -0.01,
        text_wrap: "balance",
    },
    ReaderTypeToken {
        role: ReaderTextRole::H3,
        size_rem: 1.125,
        line_height: 1.3,
        font_weight: 600,
        letter_spacing_em: 0.0,
        text_wrap: "balance",
    },
    ReaderTypeToken {
        role: ReaderTextRole::Body,
        size_rem: 1.125,
        line_height: 1.6,
        font_weight: 400,
        letter_spacing_em: 0.0,
        text_wrap: "pretty",
    },
    ReaderTypeToken {
        role: ReaderTextRole::Byline,
        size_rem: 0.875,
        line_height: 1.5,
        font_weight: 400,
        letter_spacing_em: 0.0,
        text_wrap: "pretty",
    },
    ReaderTypeToken {
        role: ReaderTextRole::Caption,
        size_rem: 0.8125,
        line_height: 1.5,
        font_weight: 400,
        letter_spacing_em: 0.0,
        text_wrap: "pretty",
    },
    ReaderTypeToken {
        role: ReaderTextRole::Overline,
        size_rem: 0.75,
        line_height: 1.4,
        font_weight: 500,
        letter_spacing_em: 0.05,
        text_wrap: "nowrap",
    },
];

/// Etiquetas de interfaz y ARIA en español (oración normal, verbo inicial).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReaderScaleCopy {
    pub decrease_label: &'static str,
    pub increase_label: &'static str,
    pub reset_label: &'static str,
    pub toolbar_aria_label: &'static str,
}

pub const READER_SCALE_COPY: ReaderScaleCopy = ReaderScaleCopy {
    decrease_label: "Reducir tamaño de texto",
    increase_label: "Aumentar tamaño de texto",
    reset_label: "Restablecer tamaño de texto",
    toolbar_aria_label: "Controles de tipografía del modo lectura",
};

/// Fragmento CSS completo para la escala tipográfica del modo lectura.
pub const READER_TYPOGRAPHY_CSS: &str = r#":root {
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  --r-font-size: 1.125rem;
  --r-measure: 68ch;
  --r-ease: cubic-bezier(0.2, 0, 0, 1);
}

#office-reader {
  max-width: clamp(60ch, var(--r-measure), 75ch);
  font-size: var(--r-font-size);
  line-height: 1.6;
  font-weight: 400;
  letter-spacing: 0;
  text-align: start;
  overflow-wrap: break-word;
  text-wrap: pretty;
}

#office-reader h1 {
  font-size: 1.75rem;
  line-height: 1.24;
  font-weight: 650;
  letter-spacing: -0.015em;
  text-wrap: balance;
}

#office-reader h2 {
  font-size: 1.35rem;
  line-height: 1.28;
  font-weight: 600;
  letter-spacing: -0.01em;
  text-wrap: balance;
}

#office-reader h3 {
  font-size: 1.125rem;
  line-height: 1.3;
  font-weight: 600;
  letter-spacing: 0;
  text-wrap: balance;
}

#office-reader .office-from {
  font-size: 0.75rem;
  line-height: 1.4;
  font-weight: 500;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  font-variant-numeric: tabular-nums;
}

#office-reader .office-byline {
  font-size: 0.875rem;
  line-height: 1.5;
  font-weight: 400;
}

#office-reader figcaption {
  font-size: 0.8125rem;
  line-height: 1.5;
  font-weight: 400;
}

#office-reader a {
  text-underline-position: from-font;
  text-decoration-thickness: from-font;
}

.reader-reading-stats {
  font-variant-numeric: tabular-nums;
}

.reader-scale-btn {
  transition-property: transform, background-color, opacity;
  transition-duration: 150ms;
  transition-timing-function: var(--r-ease);
}

.reader-scale-btn:not(:disabled):active {
  transform: scale(0.96);
}

@media (prefers-reduced-motion: no-preference) {
  #office-reader {
    transition-property: font-size;
    transition-duration: 200ms;
    transition-timing-function: var(--r-ease);
  }
}
"#;

/// Devuelve la especificación principal de esta regla UX.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: RULE_ID,
        area: "reader_typography_scale",
        title: "Escala tipográfica y medida de línea en modo lectura",
        summary: "Unifica la escala en rem, interlineado sin unidad, medida 60-75ch, tabular-nums y controles en español con verbo inicial.",
        css_fragment: READER_TYPOGRAPHY_CSS,
    }
}

/// Limita el tamaño de fuente solicitado al rango permitido y lo convierte a `rem`.
pub fn clamped_font_size_px(requested_px: u16) -> u16 {
    requested_px.clamp(MIN_FONT_SIZE_PX, MAX_FONT_SIZE_PX)
}

/// Convierte un tamaño en píxeles CSS (acotado) a `rem` sobre base `16px`.
pub fn font_size_rem(requested_px: u16) -> f32 {
    f32::from(clamped_font_size_px(requested_px)) / ROOT_REM_BASE_PX
}

/// Acota la medida de línea al rango legible `60ch`–`75ch`.
pub fn clamped_measure_ch(requested_ch: u16) -> u16 {
    requested_ch.clamp(MIN_MEASURE_CH, MAX_MEASURE_CH)
}

/// Formatea el estado actual del tamaño para lectores de pantalla en español.
pub fn format_scale_status(font_px: u16) -> String {
    let clamped = clamped_font_size_px(font_px);
    format!("Tamaño de texto: {clamped} px")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_exposes_exact_typography_and_motion_values() {
        let s = spec();
        assert_eq!(s.id, "ux_48_reader_typography_scale");
        assert!(s.css_fragment.contains("cubic-bezier(0.2, 0, 0, 1)"));
        assert!(s.css_fragment.contains("scale(0.96)"));
        assert!(s
            .css_fragment
            .contains("font-variant-numeric: tabular-nums"));
        assert!(s.css_fragment.contains("text-wrap: pretty"));
        assert!(s.css_fragment.contains("text-wrap: balance"));
        assert!(s
            .css_fragment
            .contains("clamp(60ch, var(--r-measure), 75ch)"));
        assert!(s
            .css_fragment
            .contains("prefers-reduced-motion: no-preference"));
        assert!(s
            .css_fragment
            .contains("text-underline-position: from-font"));
    }

    #[test]
    fn heading_hierarchy_descends_and_weights_meet_floor() {
        let h1 = READER_TYPE_SCALE[0];
        let h2 = READER_TYPE_SCALE[1];
        let h3 = READER_TYPE_SCALE[2];
        let body = READER_TYPE_SCALE[3];

        assert!(h1.size_rem > h2.size_rem);
        assert!(h2.size_rem > h3.size_rem);
        assert!(h3.size_rem >= body.size_rem);

        assert!((1.5..=1.6).contains(&body.line_height));
        for token in READER_TYPE_SCALE {
            if token.size_rem * ROOT_REM_BASE_PX < 18.0 {
                assert!(
                    token.font_weight >= 400,
                    "weight below 400 for small text: {:?}",
                    token.role
                );
            }
        }
    }

    #[test]
    fn clamping_and_rem_conversion_stay_in_bounds() {
        assert_eq!(clamped_font_size_px(8), MIN_FONT_SIZE_PX);
        assert_eq!(clamped_font_size_px(18), 18);
        assert_eq!(clamped_font_size_px(99), MAX_FONT_SIZE_PX);
        assert!((font_size_rem(18) - 1.125).abs() < f32::EPSILON);
        assert_eq!(clamped_measure_ch(40), 60);
        assert_eq!(clamped_measure_ch(68), 68);
        assert_eq!(clamped_measure_ch(100), 75);
    }

    #[test]
    fn spanish_copy_is_sentence_case_verb_first() {
        let labels = [
            READER_SCALE_COPY.decrease_label,
            READER_SCALE_COPY.increase_label,
            READER_SCALE_COPY.reset_label,
        ];
        for label in labels {
            assert!(!label.ends_with('!'));
            assert!(!label.contains("OK"));
            let first = label.chars().next();
            assert!(first.is_some_and(char::is_uppercase));
        }
        assert_eq!(format_scale_status(18), "Tamaño de texto: 18 px");
    }
}
