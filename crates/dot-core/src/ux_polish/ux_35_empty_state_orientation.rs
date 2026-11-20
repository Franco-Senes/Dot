//! UX-35: Empty state orientation and forward-pointing recovery.
//!
//! Ensures every empty surface in Search Linux explains what the view is for,
//! how to populate it, and offers one clear verb-first Spanish action in
//! sentence case, with staggered entrance motion gated behind
//! `prefers-reduced-motion`.

/// Maximum padding (in pixels) where nested surfaces share concentric radii.
pub const CONCENTRIC_PADDING_CUTOFF_PX: u16 = 24;

/// Stagger delay between semantic empty-state chunks (title, description, CTA).
pub const STAGGER_STEP_MS: u16 = 100;

/// Entrance animation duration in milliseconds.
pub const ENTER_DURATION_MS: u16 = 300;

/// Vertical entrance offset in pixels.
pub const ENTER_TRANSLATE_Y_PX: u16 = 12;

/// Entrance blur radius in pixels.
pub const ENTER_BLUR_PX: u16 = 4;

/// Press feedback scale for enabled CTA buttons.
pub const CTA_PRESS_SCALE: f32 = 0.96;

/// Press feedback transition duration in milliseconds.
pub const CTA_PRESS_DURATION_MS: u16 = 150;

/// Standard Search Linux expressive easing curve.
pub const EASING_EMPHASIZED: &str = "cubic-bezier(0.2, 0, 0, 1)";

/// CSS fragment implementing empty-state entrance, reduced-motion fallback,
/// hover gating, and `:not(:disabled):active` press feedback.
pub const EMPTY_STATE_CSS: &str = r#"
.sl-empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  padding: 24px;
  border-radius: 16px;
  border: 1px solid transparent;
}

.sl-empty-state__title {
  margin: 0 0 6px;
  font-weight: 600;
}

.sl-empty-state__desc {
  margin: 0 0 16px;
  max-width: 42ch;
}

.sl-empty-state__action {
  border-radius: 8px;
  border: 1px solid transparent;
  -webkit-tap-highlight-color: transparent;
  transition-property: transform, opacity, background-color;
  transition-duration: 150ms;
  transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
}

.sl-empty-state__action:not(:disabled):active {
  transform: scale(0.96);
}

@media (hover: hover) {
  .sl-empty-state__action:not(:disabled):hover {
    opacity: 0.92;
  }
}

@media (prefers-reduced-motion: no-preference) {
  .sl-empty-state__title,
  .sl-empty-state__desc,
  .sl-empty-state__action {
    animation-name: sl-empty-enter;
    animation-duration: 300ms;
    animation-timing-function: cubic-bezier(0.2, 0, 0, 1);
    animation-fill-mode: both;
  }

  .sl-empty-state__title {
    animation-delay: 0ms;
  }

  .sl-empty-state__desc {
    animation-delay: 100ms;
  }

  .sl-empty-state__action {
    animation-delay: 200ms;
  }
}

@media (prefers-reduced-motion: reduce) {
  .sl-empty-state__title,
  .sl-empty-state__desc,
  .sl-empty-state__action {
    animation-name: sl-empty-fade;
    animation-duration: 150ms;
    animation-timing-function: ease-out;
    animation-fill-mode: both;
  }
}

@keyframes sl-empty-enter {
  from {
    opacity: 0;
    filter: blur(4px);
    transform: translateY(12px);
  }
  to {
    opacity: 1;
    filter: blur(0px);
    transform: translateY(0);
  }
}

@keyframes sl-empty-fade {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}
"#;

/// High-level specification for the UX-35 empty-state orientation rule.
#[derive(Debug, Clone, PartialEq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub summary: &'static str,
    pub motion: EmptyStateMotionSpec,
    pub css_fragment: &'static str,
}

/// Motion parameters for infrequent empty-state entrances and CTA feedback.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmptyStateMotionSpec {
    pub stagger_ms: u16,
    pub enter_duration_ms: u16,
    pub enter_translate_y_px: u16,
    pub enter_blur_px: u16,
    pub easing: &'static str,
    pub cta_press_scale: f32,
    pub cta_press_duration_ms: u16,
}

/// Surfaces in Search Linux that require forward-pointing empty states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmptyStateSurface {
    Bookmarks,
    History,
    Downloads,
    PinFolders,
    Extensions,
}

/// Structured Spanish copy for an unfiltered empty state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyStateCopy {
    pub heading: &'static str,
    pub description: &'static str,
    pub action_label: &'static str,
    pub aria_label: &'static str,
    /// Empty states disappear once content exists; never store persistent info here.
    pub is_ephemeral_only: bool,
}

/// Structured Spanish copy for a filtered or search-query empty state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilteredEmptyState {
    pub message: String,
    pub clear_action_label: &'static str,
    pub clear_aria_label: &'static str,
}

/// Returns the canonical UX-35 rule specification.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_35_empty_state_orientation",
        title: "Empty state orientation and next step",
        category: "writing+ui",
        summary: "Orient the user on empty surfaces with warm Spanish sentence-case copy, one verb-first CTA, and reduced-motion-safe staggered entrance.",
        motion: EmptyStateMotionSpec {
            stagger_ms: STAGGER_STEP_MS,
            enter_duration_ms: ENTER_DURATION_MS,
            enter_translate_y_px: ENTER_TRANSLATE_Y_PX,
            enter_blur_px: ENTER_BLUR_PX,
            easing: EASING_EMPHASIZED,
            cta_press_scale: CTA_PRESS_SCALE,
            cta_press_duration_ms: CTA_PRESS_DURATION_MS,
        },
        css_fragment: EMPTY_STATE_CSS,
    }
}

/// Returns the Spanish sentence-case copy for a given Search Linux surface.
pub const fn copy_for_surface(surface: EmptyStateSurface) -> EmptyStateCopy {
    match surface {
        EmptyStateSurface::Bookmarks => EmptyStateCopy {
            heading: "Todavía no hay marcadores",
            description: "Guarda tus páginas frecuentes para abrirlas desde cualquier pestaña.",
            action_label: "Importar marcadores",
            aria_label: "Importar marcadores desde otro navegador o archivo",
            is_ephemeral_only: true,
        },
        EmptyStateSurface::History => EmptyStateCopy {
            heading: "El historial está vacío",
            description: "Aquí aparecerán las páginas que visites mientras navegas.",
            action_label: "Abrir nueva pestaña",
            aria_label: "Abrir nueva pestaña para comenzar a navegar",
            is_ephemeral_only: true,
        },
        EmptyStateSurface::Downloads => EmptyStateCopy {
            heading: "No hay descargas recientes",
            description: "Los archivos que guardes desde la web se mostrarán en esta lista.",
            action_label: "Explorar carpeta de descargas",
            aria_label: "Explorar carpeta de descargas del sistema",
            is_ephemeral_only: true,
        },
        EmptyStateSurface::PinFolders => EmptyStateCopy {
            heading: "Sin carpetas fijadas",
            description: "Fija carpetas locales para acceder rápido a tus documentos.",
            action_label: "Fijar carpeta",
            aria_label: "Fijar carpeta local en la barra lateral",
            is_ephemeral_only: true,
        },
        EmptyStateSurface::Extensions => EmptyStateCopy {
            heading: "No hay extensiones instaladas",
            description: "Agrega complementos compatibles para adaptar el navegador a tu flujo.",
            action_label: "Instalar extensión",
            aria_label: "Instalar extensión desde paquete local o catálogo",
            is_ephemeral_only: true,
        },
    }
}

/// Builds a whole-sentence Spanish empty state for a filtered query with an exit action.
pub fn filtered_empty_state(query: &str) -> FilteredEmptyState {
    let trimmed = query.trim();
    let message = if trimmed.is_empty() {
        "No hay resultados para los filtros actuales.".to_string()
    } else {
        format!("No hay resultados para «{trimmed}».")
    };

    FilteredEmptyState {
        message,
        clear_action_label: "Limpiar filtros",
        clear_aria_label: "Limpiar filtros y mostrar todos los elementos",
    }
}

/// Computes concentric outer border radius (`inner + padding + border`) when
/// `padding_px <= 24`; past `24px`, layers are treated as separate surfaces.
pub const fn concentric_outer_radius_px(
    inner_radius_px: u16,
    padding_px: u16,
    border_px: u16,
) -> Option<u16> {
    if padding_px > CONCENTRIC_PADDING_CUTOFF_PX {
        return None;
    }
    Some(
        inner_radius_px
            .saturating_add(padding_px)
            .saturating_add(border_px),
    )
}

/// Validates that empty-state copy follows Search Linux writing rules:
/// non-empty, no exclamation marks, ephemeral-only, and verb-first Spanish CTA.
pub fn validate_copy(copy: &EmptyStateCopy) -> Result<(), &'static str> {
    if copy.heading.trim().is_empty()
        || copy.description.trim().is_empty()
        || copy.action_label.trim().is_empty()
        || copy.aria_label.trim().is_empty()
    {
        return Err("empty-state copy fields must not be blank");
    }
    if !copy.is_ephemeral_only {
        return Err("persistent information must not be parked in an empty state");
    }
    if copy.heading.contains('!')
        || copy.heading.contains('¡')
        || copy.description.contains('!')
        || copy.description.contains('¡')
        || copy.action_label.contains('!')
        || copy.action_label.contains('¡')
    {
        return Err("empty-state copy must not contain exclamation marks");
    }
    if !starts_with_spanish_action_verb(copy.action_label) {
        return Err("CTA label must start with a Spanish action verb in sentence case");
    }
    Ok(())
}

fn starts_with_spanish_action_verb(label: &str) -> bool {
    const VERB_PREFIXES: &[&str] = &[
        "Abrir ",
        "Agregar ",
        "Buscar ",
        "Cargar ",
        "Crear ",
        "Explorar ",
        "Fijar ",
        "Guardar ",
        "Importar ",
        "Instalar ",
        "Limpiar ",
        "Restablecer ",
    ];
    VERB_PREFIXES.iter().any(|prefix| label.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_matches_exact_motion_and_ui_tokens() {
        let s = spec();
        assert_eq!(s.id, "ux_35_empty_state_orientation");
        assert_eq!(s.motion.easing, "cubic-bezier(0.2, 0, 0, 1)");
        assert!((s.motion.cta_press_scale - 0.96).abs() < f32::EPSILON);
        assert_eq!(s.motion.stagger_ms, 100);
        assert_eq!(s.motion.enter_duration_ms, 300);
        assert_eq!(s.motion.enter_translate_y_px, 12);
        assert_eq!(s.motion.enter_blur_px, 4);
        assert_eq!(s.motion.cta_press_duration_ms, 150);

        assert!(s.css_fragment.contains("cubic-bezier(0.2, 0, 0, 1)"));
        assert!(s.css_fragment.contains("scale(0.96)"));
        assert!(s.css_fragment.contains(":not(:disabled):active"));
        assert!(s
            .css_fragment
            .contains("@media (prefers-reduced-motion: no-preference)"));
        assert!(s
            .css_fragment
            .contains("@media (prefers-reduced-motion: reduce)"));
        assert!(s.css_fragment.contains("@media (hover: hover)"));
        assert!(!s.css_fragment.contains("transition: all"));
    }

    #[test]
    fn concentric_radii_respects_padding_cutoff() {
        assert_eq!(concentric_outer_radius_px(8, 16, 1), Some(25));
        assert_eq!(concentric_outer_radius_px(8, 24, 0), Some(32));
        assert_eq!(concentric_outer_radius_px(8, 25, 0), None);
    }

    #[test]
    fn all_surface_copies_are_valid_spanish_sentence_case_verb_first() {
        let surfaces = [
            EmptyStateSurface::Bookmarks,
            EmptyStateSurface::History,
            EmptyStateSurface::Downloads,
            EmptyStateSurface::PinFolders,
            EmptyStateSurface::Extensions,
        ];
        for surface in surfaces {
            let copy = copy_for_surface(surface);
            assert!(
                validate_copy(&copy).is_ok(),
                "invalid copy for {surface:?}: {copy:?}"
            );
        }
    }

    #[test]
    fn filtered_empty_state_names_query_and_offers_exit() {
        let state = filtered_empty_state("  trimestral  ");
        assert_eq!(state.message, "No hay resultados para «trimestral».");
        assert_eq!(state.clear_action_label, "Limpiar filtros");

        let empty_query = filtered_empty_state("   ");
        assert_eq!(
            empty_query.message,
            "No hay resultados para los filtros actuales."
        );
    }

    #[test]
    fn validate_copy_rejects_persistent_info_and_non_verb_cta() {
        let bad = EmptyStateCopy {
            heading: "Sin elementos",
            description: "Nota permanente.",
            action_label: "OK",
            aria_label: "Aceptar",
            is_ephemeral_only: false,
        };
        assert!(validate_copy(&bad).is_err());
    }
}
