//! UX-25: Group with space, not lines (`ux_25_space_over_lines_grouping`).
//!
//! Enforces the layout hierarchy for visual grouping:
//! 1. Negative space (default): inter-group gap >= 2x intra-group gap (e.g. 8px inside, >= 16px between).
//! 2. Background shapes: cards or filled containers when a group must read as one unit.
//! 3. Separator lines: low-contrast hairlines reserved for dense data (tables, long settings lists),
//!    never paired with a large gap that already communicates the boundary.

/// Minimum multiplier between inter-group spacing and intra-group spacing.
pub const MIN_GROUP_GAP_RATIO: u32 = 2;

/// Default intra-group gap in pixels (e.g. `.field-group { gap: 8px; }`).
pub const DEFAULT_INTRA_GROUP_GAP_PX: u32 = 8;

/// Default inter-group gap in pixels (at least 2x `DEFAULT_INTRA_GROUP_GAP_PX`).
pub const DEFAULT_INTER_GROUP_GAP_PX: u32 = 16;

/// Recommended section-level inter-group gap in pixels for forms and panels.
pub const SECTION_INTER_GROUP_GAP_PX: u32 = 24;

/// Maximum gap (in px) at which a dense-list hairline separator still adds clarity
/// without doubling up on negative space.
pub const MAX_GAP_WITH_HAIRLINE_PX: u32 = 12;

/// Canonical CSS snippet for space-first grouping.
pub const CSS_SPACE_GROUPING: &str = "\
.ux-field-group { display: flex; flex-direction: column; gap: 8px; }\n\
.ux-group-stack { display: flex; flex-direction: column; gap: 24px; }\n\
.ux-dense-row + .ux-dense-row { border-block-start: 1px solid var(--ux-hairline, oklch(0 0 0 / 0.1)); }";

/// Ordered hierarchy of grouping mechanisms from preferred to last resort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupingTier {
    /// Default: negative space separates groups (>= 2x intra-group gap).
    NegativeSpace,
    /// Secondary: card or filled surface when items must read as one selectable/draggable unit.
    BackgroundShape,
    /// Last resort: low-contrast 1px hairline for dense data where space costs too much.
    SeparatorLine,
}

/// Specification for UX rule 25: space-over-lines grouping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub min_gap_ratio: u32,
    pub default_intra_gap_px: u32,
    pub default_inter_gap_px: u32,
    pub section_inter_gap_px: u32,
    pub css_fragment: &'static str,
    pub summary_es: &'static str,
}

/// Returns the canonical specification for `ux_25_space_over_lines_grouping`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_25_space_over_lines_grouping",
        title: "Group with space, not lines",
        min_gap_ratio: MIN_GROUP_GAP_RATIO,
        default_intra_gap_px: DEFAULT_INTRA_GROUP_GAP_PX,
        default_inter_gap_px: DEFAULT_INTER_GROUP_GAP_PX,
        section_inter_gap_px: SECTION_INTER_GROUP_GAP_PX,
        css_fragment: CSS_SPACE_GROUPING,
        summary_es:
            "Agrupar elementos con espacio negativo antes de usar fondos o líneas separadoras.",
    }
}

/// Computes the minimum valid inter-group gap (in px) for a given intra-group gap.
pub const fn min_inter_group_gap_px(intra_group_gap_px: u32) -> u32 {
    intra_group_gap_px.saturating_mul(MIN_GROUP_GAP_RATIO)
}

/// Validates that the inter-group gap is at least `2x` the intra-group gap.
pub const fn is_valid_group_spacing(intra_group_gap_px: u32, inter_group_gap_px: u32) -> bool {
    inter_group_gap_px >= min_inter_group_gap_px(intra_group_gap_px)
}

/// Chooses the appropriate grouping tier according to the layout hierarchy.
pub const fn recommended_grouping_tier(
    dense_data: bool,
    must_read_as_single_unit: bool,
) -> GroupingTier {
    if must_read_as_single_unit {
        GroupingTier::BackgroundShape
    } else if dense_data {
        GroupingTier::SeparatorLine
    } else {
        GroupingTier::NegativeSpace
    }
}

/// Returns `true` only when a hairline separator is justified:
/// the layout is dense data and the gap is not already large enough to separate groups on its own.
pub const fn should_render_hairline(
    dense_data: bool,
    intra_group_gap_px: u32,
    inter_group_gap_px: u32,
) -> bool {
    dense_data
        && inter_group_gap_px <= MAX_GAP_WITH_HAIRLINE_PX
        && !is_valid_group_spacing(intra_group_gap_px, inter_group_gap_px)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_exposes_exact_grouping_values() {
        let s = spec();
        assert_eq!(s.id, "ux_25_space_over_lines_grouping");
        assert_eq!(s.min_gap_ratio, 2);
        assert_eq!(s.default_intra_gap_px, 8);
        assert_eq!(s.default_inter_gap_px, 16);
        assert_eq!(s.section_inter_gap_px, 24);
        assert!(s.css_fragment.contains("gap: 8px"));
        assert!(s.css_fragment.contains("gap: 24px"));
        assert!(s.css_fragment.contains("border-block-start: 1px solid"));
    }

    #[test]
    fn gap_ratio_enforces_two_x_minimum() {
        assert_eq!(min_inter_group_gap_px(8), 16);
        assert!(is_valid_group_spacing(8, 16));
        assert!(is_valid_group_spacing(8, 24));
        assert!(!is_valid_group_spacing(8, 12));
    }

    #[test]
    fn tier_selection_follows_space_then_surface_then_line() {
        assert_eq!(
            recommended_grouping_tier(false, false),
            GroupingTier::NegativeSpace
        );
        assert_eq!(
            recommended_grouping_tier(false, true),
            GroupingTier::BackgroundShape
        );
        assert_eq!(
            recommended_grouping_tier(true, true),
            GroupingTier::BackgroundShape
        );
        assert_eq!(
            recommended_grouping_tier(true, false),
            GroupingTier::SeparatorLine
        );
    }

    #[test]
    fn hairline_suppressed_when_space_already_groups() {
        // Dense table with tight 8px/8px spacing can use a hairline.
        assert!(should_render_hairline(true, 8, 8));
        // Once inter-group gap reaches 2x (16px), space already does the job.
        assert!(!should_render_hairline(true, 8, 16));
        // Non-dense layouts never use hairlines.
        assert!(!should_render_hairline(false, 8, 8));
    }
}
