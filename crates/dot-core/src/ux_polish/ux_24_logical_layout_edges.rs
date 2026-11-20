//! UX-24: Logical layout edges and shared alignment (`ux_24_logical_layout_edges`).
//!
//! Enforces two complementary layout rules from `better-layout`:
//! 1. **Align to shared edges**: pick a small set of alignment edges and repeat one
//!    consistent spacing step (`16px` default) per level of nesting, with `24px`
//!    default section inline padding and `scroll-padding-block-start` for sticky headers.
//! 2. **Logical properties for anything that mirrors**: write direction-dependent
//!    layout as leading/trailing (`margin-inline-start`, `padding-inline-end`,
//!    `inset-inline-start`, `text-align: start`, `border-start-start-radius`, etc.).
//!    Reserve physical `left`/`right` for physical geometry like `env(safe-area-inset-left)`
//!    and `env(safe-area-inset-right)`, and pair `translateX`/`background-position`
//!    with explicit `[dir="rtl"]` overrides.

/// Default inline padding (in px) for top-level content sections.
pub const DEFAULT_SECTION_INLINE_PADDING_PX: u32 = 24;

/// Default inline indent step (in px) per level of nesting.
pub const DEFAULT_NESTING_STEP_PX: u32 = 16;

/// Default inline inset (in px) for mobile content layouts and inset buttons.
pub const DEFAULT_MOBILE_INLINE_INSET_PX: u32 = 16;

/// Mapping entry from a physical CSS property or declaration to its logical equivalent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogicalPropertyMapping {
    pub physical: &'static str,
    pub logical: &'static str,
}

/// Canonical physical-to-logical CSS property mapping table.
pub const LOGICAL_PROPERTY_MAPPINGS: &[LogicalPropertyMapping] = &[
    LogicalPropertyMapping {
        physical: "margin-left",
        logical: "margin-inline-start",
    },
    LogicalPropertyMapping {
        physical: "margin-right",
        logical: "margin-inline-end",
    },
    LogicalPropertyMapping {
        physical: "padding-left",
        logical: "padding-inline-start",
    },
    LogicalPropertyMapping {
        physical: "padding-right",
        logical: "padding-inline-end",
    },
    LogicalPropertyMapping {
        physical: "left",
        logical: "inset-inline-start",
    },
    LogicalPropertyMapping {
        physical: "right",
        logical: "inset-inline-end",
    },
    LogicalPropertyMapping {
        physical: "text-align: left",
        logical: "text-align: start",
    },
    LogicalPropertyMapping {
        physical: "text-align: right",
        logical: "text-align: end",
    },
    LogicalPropertyMapping {
        physical: "border-left",
        logical: "border-inline-start",
    },
    LogicalPropertyMapping {
        physical: "border-right",
        logical: "border-inline-end",
    },
    LogicalPropertyMapping {
        physical: "float: left",
        logical: "float: inline-start",
    },
    LogicalPropertyMapping {
        physical: "float: right",
        logical: "float: inline-end",
    },
    LogicalPropertyMapping {
        physical: "border-top-left-radius",
        logical: "border-start-start-radius",
    },
    LogicalPropertyMapping {
        physical: "border-top-right-radius",
        logical: "border-start-end-radius",
    },
    LogicalPropertyMapping {
        physical: "border-bottom-left-radius",
        logical: "border-end-start-radius",
    },
    LogicalPropertyMapping {
        physical: "border-bottom-right-radius",
        logical: "border-end-end-radius",
    },
];

/// Canonical CSS fragment demonstrating shared alignment edges and logical properties.
pub const CSS_LOGICAL_LAYOUT_EDGES: &str = "\
.ux-section { padding-inline: 24px; scroll-padding-block-start: var(--ux-sticky-header-height, 56px); }\n\
.ux-section .ux-child { margin-inline-start: 16px; }\n\
.ux-text-cell { text-align: start; }\n\
.ux-numeric-cell { text-align: end; }\n\
.ux-leading-badge { inset-inline-start: 0; border-start-start-radius: var(--ux-radius-md, 8px); }";

/// Specification for UX rule 24: logical layout edges and shared alignment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub section_inline_padding_px: u32,
    pub nesting_step_px: u32,
    pub mobile_inline_inset_px: u32,
    pub mappings: &'static [LogicalPropertyMapping],
    pub css_fragment: &'static str,
    pub summary_es: &'static str,
}

/// Returns the canonical specification for `ux_24_logical_layout_edges`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_24_logical_layout_edges",
        title: "Logical layout edges and shared alignment",
        section_inline_padding_px: DEFAULT_SECTION_INLINE_PADDING_PX,
        nesting_step_px: DEFAULT_NESTING_STEP_PX,
        mobile_inline_inset_px: DEFAULT_MOBILE_INLINE_INSET_PX,
        mappings: LOGICAL_PROPERTY_MAPPINGS,
        css_fragment: CSS_LOGICAL_LAYOUT_EDGES,
        summary_es: "Alinear secciones a bordes compartidos y usar propiedades lógicas CSS para soportar RTL.",
    }
}

/// Looks up the logical CSS equivalent for a physical property or declaration.
pub fn to_logical_property(physical: &str) -> Option<&'static str> {
    let normalized = physical.trim();
    LOGICAL_PROPERTY_MAPPINGS
        .iter()
        .find(|entry| entry.physical.eq_ignore_ascii_case(normalized))
        .map(|entry| entry.logical)
}

/// Computes the expected leading inline edge (in px) for a nested element at `depth`
/// starting from `base_edge_px` with a uniform `step_px` per nesting level.
pub const fn nested_inline_indent_px(base_edge_px: u32, depth: u32, step_px: u32) -> u32 {
    base_edge_px.saturating_add(depth.saturating_mul(step_px))
}

/// Validates that every measured leading edge aligns to `base_edge_px + k * step_px`
/// rather than introducing stray, unrelated alignment offsets.
pub fn is_valid_shared_edge_alignment(edges_px: &[u32], base_edge_px: u32, step_px: u32) -> bool {
    if step_px == 0 {
        return edges_px.iter().all(|&edge| edge == base_edge_px);
    }
    edges_px
        .iter()
        .all(|&edge| edge >= base_edge_px && (edge - base_edge_px).is_multiple_of(step_px))
}

/// Scans a CSS snippet for physical directional properties that break RTL mirroring
/// when logical equivalents should be used instead.
///
/// Physical `left`/`right` declarations paired with `safe-area-inset-left` or
/// `safe-area-inset-right` are permitted because safe-area insets are physical geometry.
pub fn detect_physical_property_violations(css: &str) -> Vec<&'static str> {
    let lower = css.to_ascii_lowercase();
    let mut violations = Vec::new();

    let patterns: &[(&str, &str)] = &[
        ("margin-left:", "margin-left"),
        ("margin-right:", "margin-right"),
        ("padding-left:", "padding-left"),
        ("padding-right:", "padding-right"),
        ("text-align: left", "text-align: left"),
        ("text-align: right", "text-align: right"),
        ("border-left:", "border-left"),
        ("border-right:", "border-right"),
        ("float: left", "float: left"),
        ("float: right", "float: right"),
        ("border-top-left-radius:", "border-top-left-radius"),
        ("border-top-right-radius:", "border-top-right-radius"),
        ("border-bottom-left-radius:", "border-bottom-left-radius"),
        ("border-bottom-right-radius:", "border-bottom-right-radius"),
    ];

    for &(needle, label) in patterns {
        if lower.contains(needle) {
            violations.push(label);
        }
    }

    if (lower.contains("inset-inline") || lower.contains("-inline-"))
        && (lower.contains("safe-area-inset-left") || lower.contains("safe-area-inset-right"))
    {
        violations.push("inline-property-with-physical-safe-area-inset");
    }

    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_matches_layout_defaults_and_mappings() {
        let s = spec();
        assert_eq!(s.id, "ux_24_logical_layout_edges");
        assert_eq!(s.section_inline_padding_px, 24);
        assert_eq!(s.nesting_step_px, 16);
        assert_eq!(s.mobile_inline_inset_px, 16);
        assert!(detect_physical_property_violations(s.css_fragment).is_empty());

        assert_eq!(
            to_logical_property("margin-left"),
            Some("margin-inline-start")
        );
        assert_eq!(
            to_logical_property("padding-right"),
            Some("padding-inline-end")
        );
        assert_eq!(to_logical_property("left"), Some("inset-inline-start"));
        assert_eq!(
            to_logical_property("text-align: left"),
            Some("text-align: start")
        );
        assert_eq!(
            to_logical_property("float: left"),
            Some("float: inline-start")
        );
        assert_eq!(
            to_logical_property("border-top-left-radius"),
            Some("border-start-start-radius")
        );
        assert_eq!(to_logical_property("gap"), None);
    }

    #[test]
    fn shared_edge_alignment_and_physical_violation_checks() {
        assert_eq!(nested_inline_indent_px(24, 0, 16), 24);
        assert_eq!(nested_inline_indent_px(24, 1, 16), 40);
        assert_eq!(nested_inline_indent_px(24, 2, 16), 56);

        assert!(is_valid_shared_edge_alignment(&[24, 40, 56], 24, 16));
        assert!(!is_valid_shared_edge_alignment(&[20, 14, 24], 24, 16));

        let bad_css =
            ".card { margin-left: 16px; text-align: left; border-top-right-radius: 8px; }";
        let violations = detect_physical_property_violations(bad_css);
        assert_eq!(
            violations,
            vec!["margin-left", "text-align: left", "border-top-right-radius"]
        );

        let bad_safe_area = ".bar { padding-inline-start: env(safe-area-inset-left); }";
        assert_eq!(
            detect_physical_property_violations(bad_safe_area),
            vec!["inline-property-with-physical-safe-area-inset"]
        );
    }
}
