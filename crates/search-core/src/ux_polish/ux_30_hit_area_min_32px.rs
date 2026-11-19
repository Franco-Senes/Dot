//! UX-30: Minimum 32px interactive hit area for controls and icon buttons.
//!
//! Ensures every interactive control in Search Linux meets a 32x32px minimum
//! target size on fine pointers (mouse/trackpad) and 44x44px on coarse pointers
//! (touch), exceeding WCAG 2.2 2.5.8 (24px) without distorting compact icons.

/// Minimum interactive hit-area dimension in CSS pixels for fine pointers.
pub const MIN_HIT_AREA_PX: u16 = 32;

/// Recommended interactive hit-area dimension in CSS pixels for coarse/touch pointers.
pub const TOUCH_HIT_AREA_PX: u16 = 44;

/// Minimum gap in CSS pixels between adjacent interactive hit targets.
pub const MIN_ADJACENT_GAP_PX: u16 = 4;

/// Baseline WCAG 2.2 Target Size (Minimum) threshold in CSS pixels.
pub const WCAG_MINIMUM_PX: u16 = 24;

/// Exact CSS rules enforcing 32px minimum hit areas and 44px coarse-pointer targets.
pub const CSS_HIT_AREA_RULES: &str = r#"/* UX-30: Minimum 32px hit area for interactive controls */
.sl-hit-target {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 32px;
  min-height: 32px;
  -webkit-tap-highlight-color: transparent;
}

.sl-hit-target--compact {
  position: relative;
}

.sl-hit-target--compact::after {
  content: "";
  position: absolute;
  inset: -8px;
}

.sl-hit-group {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

@media (pointer: coarse) {
  .sl-hit-target {
    min-width: 44px;
    min-height: 44px;
  }

  .sl-hit-target--compact::after {
    inset: -14px;
  }
}
"#;

/// Specification metadata for a Search Linux UX overhaul rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub min_hit_area_px: u16,
    pub touch_hit_area_px: u16,
    pub min_adjacent_gap_px: u16,
    pub css: &'static str,
}

/// Pointer precision class used to determine the required hit-area size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerKind {
    Fine,
    Coarse,
}

/// Measured dimensions of an interactive control and its spacing to neighbors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HitAreaBox {
    pub width_px: u16,
    pub height_px: u16,
    pub gap_px: u16,
}

/// Result of auditing a control's hit area against UX-30 requirements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HitAreaAudit {
    pub meets_minimum: bool,
    pub meets_gap: bool,
    pub required_px: u16,
    pub expand_x_px: u16,
    pub expand_y_px: u16,
}

/// Returns the specification for UX-30.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_30_hit_area_min_32px",
        title: "Minimum 32px interactive hit area",
        category: "accessibility",
        description: "Interactive controls must provide at least a 32x32px hit target on fine pointers and 44x44px on coarse pointers, with at least 4px separation.",
        min_hit_area_px: MIN_HIT_AREA_PX,
        touch_hit_area_px: TOUCH_HIT_AREA_PX,
        min_adjacent_gap_px: MIN_ADJACENT_GAP_PX,
        css: CSS_HIT_AREA_RULES,
    }
}

/// Returns the required hit-area dimension in CSS pixels for the given pointer kind.
pub const fn required_hit_area_px(pointer: PointerKind) -> u16 {
    match pointer {
        PointerKind::Fine => MIN_HIT_AREA_PX,
        PointerKind::Coarse => TOUCH_HIT_AREA_PX,
    }
}

/// Audits a measured control box against the required hit area and neighbor gap.
pub const fn audit_hit_area(target: HitAreaBox, pointer: PointerKind) -> HitAreaAudit {
    let required_px = required_hit_area_px(pointer);
    let expand_x_px = required_px.saturating_sub(target.width_px);
    let expand_y_px = required_px.saturating_sub(target.height_px);
    let meets_minimum = expand_x_px == 0 && expand_y_px == 0;
    let meets_gap = target.gap_px >= MIN_ADJACENT_GAP_PX;

    HitAreaAudit {
        meets_minimum,
        meets_gap,
        required_px,
        expand_x_px,
        expand_y_px,
    }
}

/// Computes an asymmetric CSS `inset` declaration for a `::after` hit-target
/// overlay so a compact visual icon reaches the required hit area centered.
pub fn expanded_inset_css(
    visual_width_px: u16,
    visual_height_px: u16,
    pointer: PointerKind,
) -> String {
    let required = required_hit_area_px(pointer);
    let pad_x = required.saturating_sub(visual_width_px).div_ceil(2);
    let pad_y = required.saturating_sub(visual_height_px).div_ceil(2);

    if pad_x == 0 && pad_y == 0 {
        return "inset: 0;".to_string();
    }
    if pad_x == pad_y {
        return format!("inset: -{pad_x}px;");
    }
    format!("inset: -{pad_y}px -{pad_x}px;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_exposes_exact_hit_area_thresholds_and_css() {
        let s = spec();
        assert_eq!(s.id, "ux_30_hit_area_min_32px");
        assert_eq!(s.min_hit_area_px, 32);
        assert_eq!(s.touch_hit_area_px, 44);
        assert_eq!(s.min_adjacent_gap_px, 4);
        assert!(s.min_hit_area_px >= WCAG_MINIMUM_PX);
        assert!(s.css.contains("min-width: 32px;"));
        assert!(s.css.contains("min-height: 32px;"));
        assert!(s.css.contains("@media (pointer: coarse)"));
        assert!(s.css.contains("min-width: 44px;"));
    }

    #[test]
    fn audit_detects_undersized_targets_and_computes_expansion() {
        let icon_btn = HitAreaBox {
            width_px: 16,
            height_px: 16,
            gap_px: 4,
        };
        let fine = audit_hit_area(icon_btn, PointerKind::Fine);
        assert!(!fine.meets_minimum);
        assert!(fine.meets_gap);
        assert_eq!(fine.required_px, 32);
        assert_eq!(fine.expand_x_px, 16);
        assert_eq!(fine.expand_y_px, 16);

        let coarse = audit_hit_area(icon_btn, PointerKind::Coarse);
        assert!(!coarse.meets_minimum);
        assert_eq!(coarse.required_px, 44);
        assert_eq!(coarse.expand_x_px, 28);
        assert_eq!(coarse.expand_y_px, 28);

        let compliant = HitAreaBox {
            width_px: 32,
            height_px: 32,
            gap_px: 8,
        };
        let ok = audit_hit_area(compliant, PointerKind::Fine);
        assert!(ok.meets_minimum);
        assert!(ok.meets_gap);
        assert_eq!(ok.expand_x_px, 0);
        assert_eq!(ok.expand_y_px, 0);
    }

    #[test]
    fn expanded_inset_css_centers_compact_visuals() {
        assert_eq!(
            expanded_inset_css(16, 16, PointerKind::Fine),
            "inset: -8px;"
        );
        assert_eq!(
            expanded_inset_css(16, 16, PointerKind::Coarse),
            "inset: -14px;"
        );
        assert_eq!(
            expanded_inset_css(24, 16, PointerKind::Fine),
            "inset: -8px -4px;"
        );
        assert_eq!(expanded_inset_css(32, 32, PointerKind::Fine), "inset: 0;");
    }
}
