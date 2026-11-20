//! UX-19: Tabular numbers on changing values and counters.
//!
//! Apply `font-variant-numeric: tabular-nums` to timers, counters, prices,
//! and numeric table columns so every digit keeps a uniform width and
//! layout never jitters as values update. Keep figures in prose proportional.
//! Prefer the CSS property `font-variant-numeric: tabular-nums` over the raw
//! OpenType tag `font-feature-settings: "tnum" 1` so fallback fonts still work.

/// Canonical CSS declaration for tabular figures.
pub const CSS_TABULAR_NUMS: &str = "font-variant-numeric: tabular-nums;";

/// Canonical CSS declaration for proportional figures in running prose.
pub const CSS_PROPORTIONAL_NUMS: &str = "font-variant-numeric: proportional-nums;";

/// Raw OpenType feature tag that must be avoided in favor of `font-variant-numeric`.
pub const DISCOURAGED_RAW_TNUM_TAG: &str = "font-feature-settings";

/// Ready-to-use CSS fragment for Search Linux counters, timers, and numeric columns.
pub const TABULAR_COUNTERS_CSS: &str = "\
.ux-counter,
.ux-timer,
.ux-find-count,
.ux-download-progress,
.ux-numeric-cell,
[data-ux-tabular-nums=\"true\"] {
  font-variant-numeric: tabular-nums;
}

.ux-prose-figure {
  font-variant-numeric: proportional-nums;
}";

/// High-level specification for UX-19.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UxRuleSpec {
    /// Stable rule identifier.
    pub id: &'static str,
    /// Domain category.
    pub category: &'static str,
    /// Short rule title.
    pub title: &'static str,
    /// Summary of the requirement and rationale.
    pub summary: &'static str,
    /// Required CSS declaration for tabular numbers.
    pub css_declaration: &'static str,
    /// Ready-to-use CSS ruleset.
    pub css_fragment: &'static str,
    /// Key constraints and review checks.
    pub notes: &'static [&'static str],
}

/// Numeric rendering contexts in the Search Linux UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericContext {
    /// Live or frequently updating counter (e.g., find-in-page `3 / 18`, tab badge).
    Counter,
    /// Elapsed time, countdown, or ETA (e.g., download ETA `01:24`).
    Timer,
    /// Bandwidth, percentage, or byte transfer readout.
    LiveMetric,
    /// Aligned numeric column in a table or list (sizes, dates, durations).
    TableColumn,
    /// Currency or price figure.
    Price,
    /// Number embedded inline within a sentence or prose paragraph.
    Prose,
}

/// Resolved CSS numeric figure style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericFigureStyle {
    /// Equal-width digits (`font-variant-numeric: tabular-nums;`).
    Tabular,
    /// Variable-width digits (`font-variant-numeric: proportional-nums;`).
    Proportional,
}

impl NumericFigureStyle {
    /// Returns the exact CSS declaration for this figure style.
    pub const fn css_declaration(self) -> &'static str {
        match self {
            Self::Tabular => CSS_TABULAR_NUMS,
            Self::Proportional => CSS_PROPORTIONAL_NUMS,
        }
    }

    /// Returns the CSS keyword value for `font-variant-numeric`.
    pub const fn css_value(self) -> &'static str {
        match self {
            Self::Tabular => "tabular-nums",
            Self::Proportional => "proportional-nums",
        }
    }
}

/// Returns the canonical `UxRuleSpec` for `ux_19_tabular_nums_counters`.
pub const fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_19_tabular_nums_counters",
        category: "typography",
        title: "Tabular numbers on changing values and counters",
        summary: "Apply font-variant-numeric: tabular-nums to timers, counters, prices, and numeric table columns so digits keep one width and never shift; leave figures in prose proportional.",
        css_declaration: CSS_TABULAR_NUMS,
        css_fragment: TABULAR_COUNTERS_CSS,
        notes: &[
            "Use font-variant-numeric: tabular-nums instead of font-feature-settings: \"tnum\" 1.",
            "Apply tabular-nums to find-in-page counters, download ETAs, tab counts, and numeric table columns.",
            "Keep inline numbers inside prose sentences proportional.",
        ],
    }
}

/// Returns `true` if the given numeric context requires `tabular-nums`.
pub const fn requires_tabular_nums(context: NumericContext) -> bool {
    !matches!(context, NumericContext::Prose)
}

/// Resolves the required `NumericFigureStyle` for a given `NumericContext`.
pub const fn resolve_figure_style(context: NumericContext) -> NumericFigureStyle {
    if requires_tabular_nums(context) {
        NumericFigureStyle::Tabular
    } else {
        NumericFigureStyle::Proportional
    }
}

/// Checks whether a CSS snippet relies on raw `font-feature-settings` with `"tnum"`
/// instead of the standard `font-variant-numeric: tabular-nums` property.
pub fn uses_raw_tnum_feature_setting(css: &str) -> bool {
    let lower = css.to_ascii_lowercase();
    lower.contains(DISCOURAGED_RAW_TNUM_TAG) && lower.contains("tnum")
}

/// Formats a bounded `current / total` counter (e.g., find-in-page matches)
/// with consistent clamped bounds so `current` never exceeds `total` when `total > 0`.
pub fn format_ratio_counter(current: usize, total: usize) -> String {
    if total == 0 {
        return "0 / 0".to_string();
    }
    let clamped = current.min(total);
    format!("{clamped} / {total}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_exposes_exact_tabular_nums_declaration() {
        let s = spec();
        assert_eq!(s.id, "ux_19_tabular_nums_counters");
        assert_eq!(s.category, "typography");
        assert_eq!(s.css_declaration, "font-variant-numeric: tabular-nums;");
        assert!(s
            .css_fragment
            .contains("font-variant-numeric: tabular-nums;"));
        assert!(!uses_raw_tnum_feature_setting(s.css_fragment));
    }

    #[test]
    fn resolves_tabular_for_counters_and_proportional_for_prose() {
        assert_eq!(
            resolve_figure_style(NumericContext::Counter),
            NumericFigureStyle::Tabular
        );
        assert_eq!(
            resolve_figure_style(NumericContext::Timer),
            NumericFigureStyle::Tabular
        );
        assert_eq!(
            resolve_figure_style(NumericContext::LiveMetric),
            NumericFigureStyle::Tabular
        );
        assert_eq!(
            resolve_figure_style(NumericContext::TableColumn),
            NumericFigureStyle::Tabular
        );
        assert_eq!(
            resolve_figure_style(NumericContext::Price),
            NumericFigureStyle::Tabular
        );
        assert_eq!(
            resolve_figure_style(NumericContext::Prose),
            NumericFigureStyle::Proportional
        );
        assert_eq!(
            NumericFigureStyle::Tabular.css_declaration(),
            "font-variant-numeric: tabular-nums;"
        );
        assert_eq!(
            NumericFigureStyle::Proportional.css_declaration(),
            "font-variant-numeric: proportional-nums;"
        );
    }

    #[test]
    fn detects_discouraged_raw_tnum_tag() {
        assert!(uses_raw_tnum_feature_setting(
            r#"font-feature-settings: "tnum" 1;"#
        ));
        assert!(!uses_raw_tnum_feature_setting(
            "font-variant-numeric: tabular-nums;"
        ));
    }

    #[test]
    fn formats_ratio_counter_cleanly() {
        assert_eq!(format_ratio_counter(0, 0), "0 / 0");
        assert_eq!(format_ratio_counter(3, 18), "3 / 18");
        assert_eq!(format_ratio_counter(25, 18), "18 / 18");
    }
}
