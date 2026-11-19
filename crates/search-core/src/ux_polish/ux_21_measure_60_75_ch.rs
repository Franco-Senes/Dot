//! UX-21: Cap long-form text measure at 60-75 characters per line (`45ch`-`60ch`).
//!
//! Pure-Rust specification, CSS fragments, line-length audit helpers, and
//! Spanish sentence-case UI copy for Search Linux long-form surfaces
//! (Reader Mode, documentation, release notes, and settings descriptions).

/// Specification metadata for a single UX overhaul rule.
#[derive(Debug, Clone, PartialEq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub skill_source: &'static str,
    pub summary: &'static str,
    pub min_chars_per_line: usize,
    pub max_chars_per_line: usize,
    pub min_ch: f32,
    pub recommended_ch: f32,
    pub max_ch: f32,
    pub body_font_size_px: f32,
    pub body_line_height: f32,
    pub css_snippet: &'static str,
}

/// Minimum target characters per line for comfortable long-form reading.
pub const MIN_CHARS_PER_LINE: usize = 60;

/// Maximum target characters per line before eye-tracking fatigue increases.
pub const MAX_CHARS_PER_LINE: usize = 75;

/// Lower bound in CSS `ch` units (width of `0`, wider than average glyph).
pub const MIN_MEASURE_CH: f32 = 45.0;

/// Balanced default in CSS `ch` units, landing near the middle of 60-75 chars.
pub const RECOMMENDED_MEASURE_CH: f32 = 54.0;

/// Upper bound in CSS `ch` units; `65ch` (`max-w-prose`) exceeds 75 chars.
pub const MAX_MEASURE_CH: f32 = 60.0;

/// Default body font size floor in pixels (`1rem`).
pub const BODY_FONT_SIZE_PX: f32 = 16.0;

/// Unitless body line-height paired with a 60-75 character measure.
pub const BODY_LINE_HEIGHT: f32 = 1.5;

/// Approximate minimum pixel width at `16px` body size.
pub const MIN_MEASURE_PX_AT_16: f32 = 420.0;

/// Approximate maximum pixel width at `16px` body size.
pub const MAX_MEASURE_PX_AT_16: f32 = 570.0;

/// Returns the canonical UX rule specification for `ux_21_measure_60_75_ch`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_21_measure_60_75_ch",
        title: "Cap long-form measure at 60-75 characters (45ch-60ch)",
        category: "typography",
        skill_source: "better-typography",
        summary: "Cap long-form prose containers at 45ch-60ch (60-75 characters per line) with unitless 1.5 line-height, start alignment, and pretty wrapping.",
        min_chars_per_line: MIN_CHARS_PER_LINE,
        max_chars_per_line: MAX_CHARS_PER_LINE,
        min_ch: MIN_MEASURE_CH,
        recommended_ch: RECOMMENDED_MEASURE_CH,
        max_ch: MAX_MEASURE_CH,
        body_font_size_px: BODY_FONT_SIZE_PX,
        body_line_height: BODY_LINE_HEIGHT,
        css_snippet: CSS_PROSE_MEASURE,
    }
}

/// Reusable CSS fragment for long-form prose containers in Search Linux.
pub const CSS_PROSE_MEASURE: &str = "\
:root {
  --search-prose-measure-min: 45ch;
  --search-prose-measure: 54ch;
  --search-prose-measure-max: 60ch;
  --search-prose-line-height: 1.5;
}

.search-prose {
  max-inline-size: min(100%, var(--search-prose-measure));
  font-size: 1rem;
  line-height: var(--search-prose-line-height);
  text-align: start;
  text-wrap: pretty;
  overflow-wrap: break-word;
}
";

/// Reader-mode measure presets bounded within `45ch`-`60ch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasurePreset {
    Narrow,
    Balanced,
    Wide,
}

impl MeasurePreset {
    /// Returns the CSS `ch` cap for this preset.
    pub const fn ch(self) -> f32 {
        match self {
            Self::Narrow => MIN_MEASURE_CH,
            Self::Balanced => RECOMMENDED_MEASURE_CH,
            Self::Wide => MAX_MEASURE_CH,
        }
    }

    /// Returns the Spanish sentence-case option label for this preset.
    pub const fn label_es(self) -> &'static str {
        match self {
            Self::Narrow => "Estrecho (45 ch)",
            Self::Balanced => "Equilibrado (54 ch)",
            Self::Wide => "Amplio (60 ch)",
        }
    }
}

/// Result of auditing a line length or CSS `ch` cap against UX-21.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasureStatus {
    TooNarrow,
    Optimal,
    TooWide,
}

/// Checks whether a measured character count per line falls within `60..=75`.
pub const fn audit_chars_per_line(chars_per_line: usize) -> MeasureStatus {
    if chars_per_line < MIN_CHARS_PER_LINE {
        MeasureStatus::TooNarrow
    } else if chars_per_line > MAX_CHARS_PER_LINE {
        MeasureStatus::TooWide
    } else {
        MeasureStatus::Optimal
    }
}

/// Checks whether a CSS `ch` cap falls within the `45.0..=60.0` range.
///
/// Values that are `NaN` or outside `[45.0, 60.0]` are reported as out-of-range
/// (`TooNarrow` if `< 45.0` or `NaN`, `TooWide` if `> 60.0`).
pub fn audit_css_ch_cap(ch: f32) -> MeasureStatus {
    if !ch.is_finite() || ch < MIN_MEASURE_CH {
        MeasureStatus::TooNarrow
    } else if ch > MAX_MEASURE_CH {
        MeasureStatus::TooWide
    } else {
        MeasureStatus::Optimal
    }
}

/// Clamps an arbitrary `ch` setting into the safe `45.0..=60.0` range.
pub fn clamp_measure_ch(ch: f32) -> f32 {
    if !ch.is_finite() {
        return RECOMMENDED_MEASURE_CH;
    }
    ch.clamp(MIN_MEASURE_CH, MAX_MEASURE_CH)
}

/// Generates an inline CSS `max-inline-size` declaration for a preset.
pub fn preset_css_declaration(preset: MeasurePreset) -> String {
    format!("max-inline-size: min(100%, {}ch);", preset.ch() as u32)
}

/// Spanish sentence-case UI label for the Reader Mode measure control.
pub const fn label_reader_measure_es() -> &'static str {
    "Ancho de lectura"
}

/// Spanish sentence-case verb-first action to restore the default measure.
pub const fn action_reset_measure_es() -> &'static str {
    "Restablecer ancho predeterminado"
}

/// Spanish sentence-case helper text explaining the 60-75 character target.
pub const fn hint_measure_range_es() -> &'static str {
    "Mantiene entre 60 y 75 caracteres por linea para facilitar la lectura."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_matches_exact_typography_bounds() {
        let s = spec();
        assert_eq!(s.id, "ux_21_measure_60_75_ch");
        assert_eq!(s.min_chars_per_line, 60);
        assert_eq!(s.max_chars_per_line, 75);
        assert_eq!(s.min_ch, 45.0);
        assert_eq!(s.recommended_ch, 54.0);
        assert_eq!(s.max_ch, 60.0);
        assert_eq!(s.body_font_size_px, 16.0);
        assert_eq!(s.body_line_height, 1.5);
        assert!(s
            .css_snippet
            .contains("max-inline-size: min(100%, var(--search-prose-measure));"));
        assert!(s.css_snippet.contains("text-wrap: pretty;"));
        assert!(s.css_snippet.contains("overflow-wrap: break-word;"));
        assert!(s.css_snippet.contains("text-align: start;"));
    }

    #[test]
    fn audit_chars_per_line_enforces_60_to_75() {
        assert_eq!(audit_chars_per_line(59), MeasureStatus::TooNarrow);
        assert_eq!(audit_chars_per_line(60), MeasureStatus::Optimal);
        assert_eq!(audit_chars_per_line(68), MeasureStatus::Optimal);
        assert_eq!(audit_chars_per_line(75), MeasureStatus::Optimal);
        assert_eq!(audit_chars_per_line(76), MeasureStatus::TooWide);
    }

    #[test]
    fn audit_css_ch_cap_rejects_65ch_prose_and_accepts_45_to_60() {
        assert_eq!(audit_css_ch_cap(44.9), MeasureStatus::TooNarrow);
        assert_eq!(audit_css_ch_cap(45.0), MeasureStatus::Optimal);
        assert_eq!(audit_css_ch_cap(54.0), MeasureStatus::Optimal);
        assert_eq!(audit_css_ch_cap(60.0), MeasureStatus::Optimal);
        assert_eq!(audit_css_ch_cap(65.0), MeasureStatus::TooWide);
        assert_eq!(audit_css_ch_cap(f32::NAN), MeasureStatus::TooNarrow);
    }

    #[test]
    fn presets_clamp_and_spanish_copy_are_valid() {
        assert_eq!(clamp_measure_ch(30.0), 45.0);
        assert_eq!(clamp_measure_ch(80.0), 60.0);
        assert_eq!(clamp_measure_ch(f32::INFINITY), 54.0);

        assert_eq!(
            preset_css_declaration(MeasurePreset::Balanced),
            "max-inline-size: min(100%, 54ch);"
        );
        assert_eq!(MeasurePreset::Narrow.label_es(), "Estrecho (45 ch)");
        assert_eq!(label_reader_measure_es(), "Ancho de lectura");
        assert_eq!(
            action_reset_measure_es(),
            "Restablecer ancho predeterminado"
        );
        assert!(hint_measure_range_es().contains("60 y 75"));
    }
}
