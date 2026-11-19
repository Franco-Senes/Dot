//! UX-50: Paleta oscura de carbón cálido (`dark_warm_charcoal_palette`).
//!
//! Define una rampa neutral cálida en OKLCH (matiz ~60 deg, croma bajo y controlado)
//! con tokens semánticos por rol, contraste WCAG 2.2 verificado, contorno neutro puro
//! para imágenes en modo oscuro y supresión de transiciones al cambiar de tema.

/// Identificador único de la regla UX dentro del catálogo de pulido.
pub const RULE_ID: &str = "ux_50_dark_warm_charcoal_palette";

/// Curva de aceleración estándar para microtransiciones de estado.
pub const EASING_STANDARD: &str = "cubic-bezier(0.2, 0, 0, 1)";

/// Escala táctil exacta al presionar controles interactivos.
pub const PRESS_SCALE: f32 = 0.96;

/// Contorno de imágenes en modo oscuro: blanco puro al 10% para evitar halos teñidos.
pub const DARK_IMAGE_OUTLINE_OKLCH: &str = "oklch(1 0 0 / 0.1)";

/// Umbral mínimo WCAG 2.2 AA para texto normal.
pub const WCAG_AA_NORMAL_TEXT: f64 = 4.5;

/// Umbral mínimo WCAG 2.2 AA para texto grande y componentes gráficos.
pub const WCAG_AA_LARGE_OR_UI: f64 = 3.0;

/// Token de color con representación sRGB hexadecimal, tupla RGB y notación OKLCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorToken {
    /// Nombre del token CSS semántico (por rol, nunca por matiz).
    pub name: &'static str,
    /// Rol funcional dentro de la interfaz.
    pub role: &'static str,
    /// Valor hexadecimal sRGB (`#RRGGBB`).
    pub hex: &'static str,
    /// Valor en espacio perceptual OKLCH.
    pub oklch: &'static str,
    /// Componentes sRGB de 8 bits para cálculo exacto de luminancia.
    pub rgb: (u8, u8, u8),
}

/// Paleta completa de carbón cálido oscuro para Search Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WarmCharcoalPalette {
    /// Fondo base de ventana sin negro puro (`#000000`) para evitar halación.
    pub bg_canvas: ColorToken,
    /// Fondo de barra lateral y contenedores secundarios.
    pub bg_sidebar: ColorToken,
    /// Superficie de tarjetas y paneles de contenido.
    pub bg_surface: ColorToken,
    /// Superficie elevada para menús, popovers y omnibox enfocado.
    pub bg_elevated: ColorToken,
    /// Estado hover en filas y botones neutros.
    pub bg_hover: ColorToken,
    /// Borde sutil estructural para separadores y tarjetas.
    pub border_subtle: ColorToken,
    /// Texto principal de alto contraste cálido.
    pub text_primary: ColorToken,
    /// Texto secundario legible sobre todas las superficies.
    pub text_secondary: ColorToken,
    /// Texto tenue para metadatos, manteniendo >= 4.5:1 sobre superficie.
    pub text_muted: ColorToken,
    /// Acento cálido para enlaces e iconos activos sobre fondo oscuro.
    pub accent_fg: ColorToken,
    /// Relleno sólido para la acción primaria única por vista.
    pub accent_solid: ColorToken,
    /// Texto sobre relleno sólido primario.
    pub accent_on_solid: ColorToken,
}

/// Textos de interfaz en español con estilo oración y verbo al inicio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpanishCopySpec {
    /// Etiqueta de opción en preferencias de apariencia.
    pub theme_option_label: &'static str,
    /// Descripción breve del beneficio visual.
    pub theme_option_hint: &'static str,
    /// Acción principal para aplicar la paleta.
    pub apply_action_label: &'static str,
    /// Acción para volver a la apariencia predeterminada.
    pub reset_action_label: &'static str,
    /// Nombre accesible para lectores de pantalla.
    pub aria_theme_toggle: &'static str,
}

/// Especificación exportada de la regla UX-50.
#[derive(Debug, Clone, PartialEq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub easing: &'static str,
    pub press_scale: f32,
    pub image_outline: &'static str,
    pub palette: WarmCharcoalPalette,
    pub copy: SpanishCopySpec,
    pub css_fragment: &'static str,
    pub gtk_css_fragment: &'static str,
    pub theme_switch_suppression_css: &'static str,
}

/// Paleta oficial de carbón cálido oscuro.
pub const PALETTE: WarmCharcoalPalette = WarmCharcoalPalette {
    bg_canvas: ColorToken {
        name: "--color-bg-canvas",
        role: "Fondo principal de ventana",
        hex: "#181614",
        oklch: "oklch(0.198 0.007 60)",
        rgb: (0x18, 0x16, 0x14),
    },
    bg_sidebar: ColorToken {
        name: "--color-bg-sidebar",
        role: "Fondo de barra lateral de pestañas",
        hex: "#1E1B18",
        oklch: "oklch(0.222 0.009 60)",
        rgb: (0x1E, 0x1B, 0x18),
    },
    bg_surface: ColorToken {
        name: "--color-bg-surface",
        role: "Superficie de tarjetas y paneles",
        hex: "#24211D",
        oklch: "oklch(0.248 0.010 60)",
        rgb: (0x24, 0x21, 0x1D),
    },
    bg_elevated: ColorToken {
        name: "--color-bg-elevated",
        role: "Superficie elevada para popovers y omnibox",
        hex: "#2C2824",
        oklch: "oklch(0.278 0.011 60)",
        rgb: (0x2C, 0x28, 0x24),
    },
    bg_hover: ColorToken {
        name: "--color-bg-hover",
        role: "Estado hover sobre superficies oscuras",
        hex: "#35302B",
        oklch: "oklch(0.312 0.012 60)",
        rgb: (0x35, 0x30, 0x2B),
    },
    border_subtle: ColorToken {
        name: "--color-border-subtle",
        role: "Borde estructural sutil",
        hex: "#3D3731",
        oklch: "oklch(0.340 0.012 60)",
        rgb: (0x3D, 0x37, 0x31),
    },
    text_primary: ColorToken {
        name: "--color-text-primary",
        role: "Texto principal sin halación",
        hex: "#F2EDE6",
        oklch: "oklch(0.945 0.010 75)",
        rgb: (0xF2, 0xED, 0xE6),
    },
    text_secondary: ColorToken {
        name: "--color-text-secondary",
        role: "Texto secundario de apoyo",
        hex: "#B8B0A6",
        oklch: "oklch(0.755 0.014 65)",
        rgb: (0xB8, 0xB0, 0xA6),
    },
    text_muted: ColorToken {
        name: "--color-text-muted",
        role: "Metadatos y etiquetas terciarias",
        hex: "#9A9186",
        oklch: "oklch(0.652 0.015 60)",
        rgb: (0x9A, 0x91, 0x86),
    },
    accent_fg: ColorToken {
        name: "--color-accent-fg",
        role: "Acento cálido legible para texto e iconos",
        hex: "#E09F5A",
        oklch: "oklch(0.750 0.125 65)",
        rgb: (0xE0, 0x9F, 0x5A),
    },
    accent_solid: ColorToken {
        name: "--color-accent-solid",
        role: "Fondo de acción primaria única",
        hex: "#C97A32",
        oklch: "oklch(0.640 0.140 55)",
        rgb: (0xC9, 0x7A, 0x32),
    },
    accent_on_solid: ColorToken {
        name: "--color-accent-on-solid",
        role: "Texto de alto contraste sobre acento sólido",
        hex: "#181614",
        oklch: "oklch(0.198 0.007 60)",
        rgb: (0x18, 0x16, 0x14),
    },
};

/// Textos en español (estilo oración, verbo al inicio en acciones).
pub const SPANISH_COPY: SpanishCopySpec = SpanishCopySpec {
    theme_option_label: "Carbón cálido oscuro",
    theme_option_hint: "Reduce el brillo frío y evita el negro puro en sesiones largas.",
    apply_action_label: "Activar carbón cálido",
    reset_action_label: "Restaurar paleta predeterminada",
    aria_theme_toggle: "Activar paleta oscura de carbón cálido",
};

/// Fragmento CSS estándar con fallback sRGB y soporte OKLCH.
pub const CSS_FRAGMENT: &str = r#":root[data-theme="dark-warm-charcoal"] {
  --color-bg-canvas: #181614;
  --color-bg-canvas: oklch(0.198 0.007 60);
  --color-bg-sidebar: #1E1B18;
  --color-bg-sidebar: oklch(0.222 0.009 60);
  --color-bg-surface: #24211D;
  --color-bg-surface: oklch(0.248 0.010 60);
  --color-bg-elevated: #2C2824;
  --color-bg-elevated: oklch(0.278 0.011 60);
  --color-bg-hover: #35302B;
  --color-bg-hover: oklch(0.312 0.012 60);
  --color-border-subtle: #3D3731;
  --color-border-subtle: oklch(0.340 0.012 60);
  --color-text-primary: #F2EDE6;
  --color-text-primary: oklch(0.945 0.010 75);
  --color-text-secondary: #B8B0A6;
  --color-text-secondary: oklch(0.755 0.014 65);
  --color-text-muted: #9A9186;
  --color-text-muted: oklch(0.652 0.015 60);
  --color-accent-fg: #E09F5A;
  --color-accent-fg: oklch(0.750 0.125 65);
  --color-accent-solid: #C97A32;
  --color-accent-solid: oklch(0.640 0.140 55);
  --color-accent-on-solid: #181614;
  --color-image-outline: oklch(1 0 0 / 0.1);
}

:root[data-theme="dark-warm-charcoal"] img {
  outline: 1px solid var(--color-image-outline);
  outline-offset: -1px;
}

:root[data-theme="dark-warm-charcoal"] button:not(:disabled):active {
  transform: scale(0.96);
  transition: transform 150ms cubic-bezier(0.2, 0, 0, 1);
}

@media (prefers-reduced-motion: reduce) {
  :root[data-theme="dark-warm-charcoal"] * {
    transition-duration: 0ms;
  }
}
"#;

/// Fragmento GTK4 / Libadwaita CSS para `crates/search/src/style.rs`.
pub const GTK_CSS_FRAGMENT: &str = r#"window.search-dark-warm-charcoal {
    @define-color window_bg_color #181614;
    @define-color window_fg_color #F2EDE6;
    @define-color sidebar_bg_color #1E1B18;
    @define-color sidebar_fg_color #F2EDE6;
    @define-color view_bg_color #24211D;
    @define-color view_fg_color #F2EDE6;
    @define-color popover_bg_color #2C2824;
    @define-color popover_fg_color #F2EDE6;
    @define-color headerbar_bg_color #1E1B18;
    @define-color headerbar_fg_color #F2EDE6;
    @define-color accent_bg_color #C97A32;
    @define-color accent_fg_color #181614;
    @define-color accent_color #E09F5A;
}

window.search-dark-warm-charcoal box.search-stage-card {
    border: 1px solid #3D3731;
}
"#;

/// Supresión temporal de transiciones durante el cambio de tema para evitar arrastre de color.
pub const THEME_SWITCH_SUPPRESSION_CSS: &str = r#"window.search-theme-switching *,
window.search-theme-switching *::before,
window.search-theme-switching *::after {
    transition-property: none;
    transition-duration: 0ms;
}
"#;

/// Devuelve la especificación completa de la regla UX-50.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: RULE_ID,
        title: "Paleta oscura de carbón cálido",
        category: "colors",
        easing: EASING_STANDARD,
        press_scale: PRESS_SCALE,
        image_outline: DARK_IMAGE_OUTLINE_OKLCH,
        palette: PALETTE,
        copy: SPANISH_COPY,
        css_fragment: CSS_FRAGMENT,
        gtk_css_fragment: GTK_CSS_FRAGMENT,
        theme_switch_suppression_css: THEME_SWITCH_SUPPRESSION_CSS,
    }
}

/// Convierte un canal sRGB de 8 bits `[0..255]` a luminancia lineal.
pub fn srgb_channel_to_linear(channel: u8) -> f64 {
    let c = f64::from(channel) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Calcula la luminancia relativa WCAG 2.2 para una tupla RGB `(r, g, b)`.
pub fn relative_luminance(rgb: (u8, u8, u8)) -> f64 {
    let r = srgb_channel_to_linear(rgb.0);
    let g = srgb_channel_to_linear(rgb.1);
    let b = srgb_channel_to_linear(rgb.2);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// Calcula la razón de contraste WCAG 2.2 `[1.0..21.0]` entre dos colores.
pub fn contrast_ratio(a: (u8, u8, u8), b: (u8, u8, u8)) -> f64 {
    let lum_a = relative_luminance(a);
    let lum_b = relative_luminance(b);
    let (lighter, darker) = if lum_a >= lum_b {
        (lum_a, lum_b)
    } else {
        (lum_b, lum_a)
    };
    (lighter + 0.05) / (darker + 0.05)
}

/// Verifica que un par de tokens cumpla con un umbral de contraste dado.
pub fn meets_contrast(fg: ColorToken, bg: ColorToken, min_ratio: f64) -> bool {
    contrast_ratio(fg.rgb, bg.rgb) >= min_ratio
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_exposes_exact_tokens_and_motion_constants() {
        let s = spec();
        assert_eq!(s.id, "ux_50_dark_warm_charcoal_palette");
        assert_eq!(s.easing, "cubic-bezier(0.2, 0, 0, 1)");
        assert!((s.press_scale - 0.96).abs() < f32::EPSILON);
        assert_eq!(s.image_outline, "oklch(1 0 0 / 0.1)");
        assert_eq!(s.palette.bg_canvas.hex, "#181614");
        assert_ne!(s.palette.bg_canvas.hex, "#000000");
        assert_eq!(s.palette.text_primary.hex, "#F2EDE6");
        assert!(s.css_fragment.contains("oklch(0.198 0.007 60)"));
        assert!(s
            .gtk_css_fragment
            .contains("@define-color window_bg_color #181614;"));
        assert!(s
            .theme_switch_suppression_css
            .contains("transition-duration: 0ms;"));
    }

    #[test]
    fn warm_charcoal_ramp_preserves_warm_undertone_and_monotone_lightness() {
        let surfaces = [
            PALETTE.bg_canvas,
            PALETTE.bg_sidebar,
            PALETTE.bg_surface,
            PALETTE.bg_elevated,
            PALETTE.bg_hover,
            PALETTE.border_subtle,
        ];
        for token in surfaces {
            let (r, g, b) = token.rgb;
            assert!(
                r > g && g > b,
                "token {} must maintain warm R > G > B undertone",
                token.name
            );
        }
        for pair in surfaces.windows(2) {
            let lum_low = relative_luminance(pair[0].rgb);
            let lum_high = relative_luminance(pair[1].rgb);
            assert!(
                lum_high > lum_low,
                "{} must be lighter than {}",
                pair[1].name,
                pair[0].name
            );
        }
    }

    #[test]
    fn all_text_and_accent_pairs_pass_wcag_aa_contrast() {
        let surfaces = [
            PALETTE.bg_canvas,
            PALETTE.bg_sidebar,
            PALETTE.bg_surface,
            PALETTE.bg_elevated,
        ];
        for surface in surfaces {
            assert!(
                meets_contrast(PALETTE.text_primary, surface, WCAG_AA_NORMAL_TEXT),
                "primary text failed 4.5:1 on {}",
                surface.name
            );
            assert!(
                meets_contrast(PALETTE.text_secondary, surface, WCAG_AA_NORMAL_TEXT),
                "secondary text failed 4.5:1 on {}",
                surface.name
            );
            assert!(
                meets_contrast(PALETTE.accent_fg, surface, WCAG_AA_NORMAL_TEXT),
                "accent_fg failed 4.5:1 on {}",
                surface.name
            );
        }
        assert!(meets_contrast(
            PALETTE.text_muted,
            PALETTE.bg_surface,
            WCAG_AA_NORMAL_TEXT
        ));
        assert!(meets_contrast(
            PALETTE.accent_on_solid,
            PALETTE.accent_solid,
            WCAG_AA_NORMAL_TEXT
        ));
        assert!(contrast_ratio(PALETTE.text_primary.rgb, PALETTE.bg_canvas.rgb) > 14.0);
    }

    #[test]
    fn spanish_copy_is_sentence_case_and_verb_first_for_actions() {
        let s = spec();
        assert!(s.copy.apply_action_label.starts_with("Activar "));
        assert!(s.copy.reset_action_label.starts_with("Restaurar "));
        assert!(s.copy.aria_theme_toggle.starts_with("Activar "));
        assert!(!s.copy.theme_option_label.contains('!'));
        assert!(!s.copy.theme_option_hint.contains('!'));
    }
}
