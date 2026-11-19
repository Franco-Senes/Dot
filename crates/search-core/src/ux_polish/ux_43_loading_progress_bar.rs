//! UX polish specification and state helpers for the page loading progress bar.
//!
//! Covers:
//! - Exact motion curve (`cubic-bezier(0.2, 0, 0, 1)`) and explicit `transition-property`
//!   (`transform, opacity`, never `all`).
//! - Reduced-motion fallback (`prefers-reduced-motion: reduce`) replacing travel
//!   with opacity cross-fade while keeping static color/label cues.
//! - Tabular numerals (`tabular-nums`) for percentage readouts.
//! - OKLCH bar fill tokens with >= 3:1 non-text contrast against surface/track.
//! - Semantic ARIA progressbar attributes and Spanish sentence-case status copy.

/// Exact easing curve for progress bar scale and opacity transitions.
pub const PROGRESS_EASING: &str = "cubic-bezier(0.2, 0, 0, 1)";

/// Duration in milliseconds for determinate progress advances.
pub const PROGRESS_ADVANCE_MS: u16 = 200;

/// Duration in milliseconds for completion fade-out (exits shorter than enters).
pub const COMPLETION_FADE_MS: u16 = 150;

/// Duration in milliseconds for reduced-motion opacity cross-fade.
pub const REDUCED_MOTION_FADE_MS: u16 = 150;

/// Progress bar height in logical pixels.
pub const BAR_HEIGHT_PX: u8 = 3;

/// Primary fill color in light mode (OKLCH, >= 3:1 non-text contrast on light surface).
pub const FILL_OKLCH_LIGHT: &str = "oklch(0.55 0.20 255)";

/// Primary fill color in dark mode (OKLCH, >= 3:1 non-text contrast on dark surface).
pub const FILL_OKLCH_DARK: &str = "oklch(0.72 0.17 255)";

/// Subtle track background in light mode.
pub const TRACK_OKLCH_LIGHT: &str = "oklch(0.92 0.01 255 / 0.6)";

/// Subtle track background in dark mode.
pub const TRACK_OKLCH_DARK: &str = "oklch(0.26 0.02 255 / 0.6)";

/// Error state fill color (OKLCH).
pub const ERROR_OKLCH: &str = "oklch(0.58 0.21 25)";

/// High-level UX rule specification for `ux_43_loading_progress_bar`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub easing: &'static str,
    pub advance_duration_ms: u16,
    pub exit_duration_ms: u16,
    pub reduced_motion_duration_ms: u16,
    pub css: &'static str,
    pub aria_role: &'static str,
    pub aria_label_es: &'static str,
    pub copy_loading_es: &'static str,
    pub copy_done_es: &'static str,
    pub copy_failed_es: &'static str,
    pub copy_stop_action_es: &'static str,
    pub copy_retry_action_es: &'static str,
}

/// Returns the canonical UX rule specification for the loading progress bar.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_43_loading_progress_bar",
        title: "Barra de progreso de carga",
        category: "motion_and_feedback",
        easing: PROGRESS_EASING,
        advance_duration_ms: PROGRESS_ADVANCE_MS,
        exit_duration_ms: COMPLETION_FADE_MS,
        reduced_motion_duration_ms: REDUCED_MOTION_FADE_MS,
        css: css_fragment(),
        aria_role: "progressbar",
        aria_label_es: "Progreso de carga de la página",
        copy_loading_es: "Cargando página...",
        copy_done_es: "Página cargada",
        copy_failed_es: "No se pudo cargar la página. Comprueba tu conexión e inténtalo de nuevo.",
        copy_stop_action_es: "Detener carga",
        copy_retry_action_es: "Volver a cargar",
    }
}

/// Loading lifecycle state for the progress bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadingProgressState {
    /// No active navigation; bar is hidden.
    Idle,
    /// Active navigation before byte/resource estimates arrive.
    Indeterminate,
    /// Active navigation with a clamped percentage (`0..=100`).
    Determinate { percent: u8 },
    /// Navigation finished; bar reaches 100% and fades out over `COMPLETION_FADE_MS`.
    Completing,
    /// Navigation failed; static error cue and retry action remain visible.
    Failed,
}

/// ARIA attributes derived from the current [`LoadingProgressState`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressAriaSnapshot {
    pub role: &'static str,
    pub label: &'static str,
    pub busy: bool,
    pub value_min: u8,
    pub value_max: u8,
    pub value_now: Option<u8>,
    pub value_text: Option<String>,
}

impl LoadingProgressState {
    /// Constructs a determinate state from a normalized fraction (`0.0..=1.0`),
    /// clamping out-of-range or non-finite values safely.
    pub fn from_fraction(fraction: f32) -> Self {
        Self::Determinate {
            percent: clamp_percent(fraction * 100.0),
        }
    }

    /// Returns whether the progress bar should be rendered as visible.
    pub fn is_visible(&self) -> bool {
        !matches!(self, Self::Idle)
    }

    /// Returns the horizontal `scaleX` factor in `0.0..=1.0`.
    pub fn scale_x(&self) -> f32 {
        match self {
            Self::Idle => 0.0,
            Self::Indeterminate => 0.35,
            Self::Determinate { percent } => f32::from((*percent).min(100)) / 100.0,
            Self::Completing | Self::Failed => 1.0,
        }
    }

    /// Returns a formatted percentage string for tabular numeric display, if applicable.
    pub fn formatted_percent(&self) -> Option<String> {
        match self {
            Self::Determinate { percent } => Some(format!("{}%", (*percent).min(100))),
            Self::Completing => Some("100%".to_string()),
            Self::Idle | Self::Indeterminate | Self::Failed => None,
        }
    }

    /// Returns the Spanish sentence-case status copy for the current state.
    pub fn status_copy_es(&self) -> Option<String> {
        let s = spec();
        match self {
            Self::Idle => None,
            Self::Indeterminate => Some(s.copy_loading_es.to_string()),
            Self::Determinate { percent } => {
                Some(format!("Cargando página: {}%", (*percent).min(100)))
            }
            Self::Completing => Some(s.copy_done_es.to_string()),
            Self::Failed => Some(s.copy_failed_es.to_string()),
        }
    }

    /// Computes semantic ARIA attributes for screen readers.
    pub fn aria_snapshot(&self) -> ProgressAriaSnapshot {
        let s = spec();
        let (busy, value_now) = match self {
            Self::Idle => (false, Some(0)),
            Self::Indeterminate => (true, None),
            Self::Determinate { percent } => (true, Some((*percent).min(100))),
            Self::Completing => (false, Some(100)),
            Self::Failed => (false, None),
        };
        ProgressAriaSnapshot {
            role: s.aria_role,
            label: s.aria_label_es,
            busy,
            value_min: 0,
            value_max: 100,
            value_now,
            value_text: self.status_copy_es(),
        }
    }
}

/// Clamps a floating-point percentage into `0..=100`, handling `NaN` and infinities.
pub fn clamp_percent(raw: f32) -> u8 {
    if !raw.is_finite() {
        return if raw.is_sign_positive() && raw.is_infinite() {
            100
        } else {
            0
        };
    }
    raw.round().clamp(0.0, 100.0) as u8
}

/// Returns the CSS fragment for the loading progress bar.
pub const fn css_fragment() -> &'static str {
    r#".search-loading-bar {
  position: relative;
  width: 100%;
  height: 3px;
  overflow: hidden;
  background-color: oklch(0.92 0.01 255 / 0.6);
  pointer-events: none;
}

.search-loading-bar__fill {
  width: 100%;
  height: 100%;
  transform-origin: left center;
  background-color: oklch(0.55 0.20 255);
  transition-property: transform, opacity;
  transition-duration: 200ms, 150ms;
  transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
  will-change: transform, opacity;
}

.search-loading-bar__fill[data-state="failed"] {
  background-color: oklch(0.58 0.21 25);
}

.search-loading-bar__percent {
  font-variant-numeric: tabular-nums;
}

@media (prefers-color-scheme: dark) {
  .search-loading-bar {
    background-color: oklch(0.26 0.02 255 / 0.6);
  }
  .search-loading-bar__fill {
    background-color: oklch(0.72 0.17 255);
  }
}

@media (prefers-reduced-motion: reduce) {
  .search-loading-bar__fill {
    transition-property: opacity;
    transition-duration: 150ms;
    transition-timing-function: ease-out;
    will-change: opacity;
  }
}"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_matches_exact_motion_tokens_and_spanish_copy() {
        let s = spec();
        assert_eq!(s.id, "ux_43_loading_progress_bar");
        assert_eq!(s.easing, "cubic-bezier(0.2, 0, 0, 1)");
        assert_eq!(s.advance_duration_ms, 200);
        assert_eq!(s.exit_duration_ms, 150);
        assert_eq!(s.aria_role, "progressbar");
        assert_eq!(s.copy_loading_es, "Cargando página...");
        assert_eq!(s.copy_done_es, "Página cargada");
        assert_eq!(s.copy_stop_action_es, "Detener carga");
        assert_eq!(s.copy_retry_action_es, "Volver a cargar");
        assert!(s.css.contains("cubic-bezier(0.2, 0, 0, 1)"));
        assert!(s.css.contains("tabular-nums"));
        assert!(s.css.contains("transition-property: transform, opacity;"));
        assert!(!s.css.contains("transition: all"));
        assert!(s.css.contains("prefers-reduced-motion: reduce"));
        assert!(s.css.contains(FILL_OKLCH_LIGHT));
        assert!(s.css.contains(FILL_OKLCH_DARK));
    }

    #[test]
    fn state_clamping_scale_and_aria_behave_deterministically() {
        assert_eq!(clamp_percent(-12.5), 0);
        assert_eq!(clamp_percent(42.6), 43);
        assert_eq!(clamp_percent(140.0), 100);
        assert_eq!(clamp_percent(f32::NAN), 0);
        assert_eq!(clamp_percent(f32::INFINITY), 100);

        let idle = LoadingProgressState::Idle;
        assert!(!idle.is_visible());
        assert_eq!(idle.scale_x(), 0.0);
        assert_eq!(idle.formatted_percent(), None);

        let indet = LoadingProgressState::Indeterminate;
        assert!(indet.is_visible());
        let indet_aria = indet.aria_snapshot();
        assert!(indet_aria.busy);
        assert_eq!(indet_aria.value_now, None);
        assert_eq!(indet_aria.value_text.as_deref(), Some("Cargando página..."));

        let det = LoadingProgressState::from_fraction(0.64);
        assert!(det.is_visible());
        assert!((det.scale_x() - 0.64).abs() < 0.001);
        assert_eq!(det.formatted_percent().as_deref(), Some("64%"));
        let det_aria = det.aria_snapshot();
        assert!(det_aria.busy);
        assert_eq!(det_aria.value_now, Some(64));
        assert_eq!(det_aria.value_text.as_deref(), Some("Cargando página: 64%"));

        let done = LoadingProgressState::Completing;
        assert_eq!(done.scale_x(), 1.0);
        assert!(!done.aria_snapshot().busy);
        assert_eq!(done.aria_snapshot().value_now, Some(100));
        assert_eq!(done.status_copy_es().as_deref(), Some("Página cargada"));

        let failed = LoadingProgressState::Failed;
        assert!(!failed.aria_snapshot().busy);
        assert_eq!(failed.aria_snapshot().value_now, None);
    }
}
