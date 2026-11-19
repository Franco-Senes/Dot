//! UX-20: Unitless line-height by semantic text role.
//!
//! - Display text uses `1.1`, titles `1.2`, headings `1.2`–`1.3`, body copy
//!   `1.5`–`1.6`, and captions `1.4`.
//! - Always emit unitless `line-height` values so leading scales with
//!   `font-size` rather than locking to fixed `px` or `rem` heights.
//! - Tight line-height (`< 1.4`) is reserved for short text; anything wrapping
//!   to 3 or more lines requires at least `1.4`, even in height-constrained rows.

/// Unitless line-height for display text (`2.25rem` / 36px).
pub const LINE_HEIGHT_DISPLAY: f32 = 1.1;

/// Unitless line-height for section titles (`1.5rem` / 24px).
pub const LINE_HEIGHT_TITLE: f32 = 1.2;

/// Unitless line-height for headings (`1.125rem` / 18px).
pub const LINE_HEIGHT_HEADING: f32 = 1.3;

/// Default unitless line-height for body copy (`1rem` / 16px).
pub const LINE_HEIGHT_BODY: f32 = 1.5;

/// Upper bound of the recommended unitless line-height range for body copy.
pub const LINE_HEIGHT_BODY_MAX: f32 = 1.6;

/// Unitless line-height for captions and helper text (`0.8125rem` / 13px).
pub const LINE_HEIGHT_CAPTION: f32 = 1.4;

/// Minimum unitless line-height for any text wrapping to 3 or more lines.
pub const MIN_WRAPPING_LINE_HEIGHT: f32 = 1.4;

/// Line count threshold at which tight leading must expand to at least `1.4`.
pub const WRAPPING_LINE_THRESHOLD: usize = 3;

/// High-level specification for this UX polish rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub category: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub css_fragment: &'static str,
    pub notes: &'static [&'static str],
}

/// Semantic typographic role for Search Linux surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextRole {
    Display,
    Title,
    Heading,
    Body,
    Caption,
}

/// Paired font size, unitless line-height, and weight for a semantic role.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoleTypography {
    pub role: TextRole,
    pub font_size_rem: f32,
    pub line_height: f32,
    pub font_weight: u16,
}

/// Returns the canonical UX specification for `ux_20_line_height_by_role`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_20_line_height_by_role",
        category: "typography",
        title: "Unitless line-height by semantic text role",
        summary: "Use unitless line-height values per role (1.1 display, 1.2 title, 1.3 heading, 1.5-1.6 body, 1.4 caption) and enforce >= 1.4 when text wraps to 3+ lines.",
        css_fragment: ":root {\n  --lh-display: 1.1;\n  --lh-title: 1.2;\n  --lh-heading: 1.3;\n  --lh-body: 1.5;\n  --lh-caption: 1.4;\n}\n\n.ux-display { font-size: 2.25rem; line-height: 1.1; font-weight: 600; }\n.ux-title { font-size: 1.5rem; line-height: 1.2; font-weight: 600; }\n.ux-heading { font-size: 1.125rem; line-height: 1.3; font-weight: 600; }\n.ux-body { font-size: 1rem; line-height: 1.5; font-weight: 400; }\n.ux-caption { font-size: 0.8125rem; line-height: 1.4; font-weight: 400; }\n.ux-multiline-3plus { line-height: 1.4; }",
        notes: &[
            "Use unitless line-height so leading scales with font-size; never use fixed px or rem.",
            "Tight line-height (< 1.4) is only for short text; 3 or more wrapped lines require >= 1.4.",
            "Pair size, unitless line-height, and weight per role so typography is one decision.",
        ],
    }
}

/// Returns the base typographic pairing for a semantic text role.
pub fn typography_for_role(role: TextRole) -> RoleTypography {
    match role {
        TextRole::Display => RoleTypography {
            role,
            font_size_rem: 2.25,
            line_height: LINE_HEIGHT_DISPLAY,
            font_weight: 600,
        },
        TextRole::Title => RoleTypography {
            role,
            font_size_rem: 1.5,
            line_height: LINE_HEIGHT_TITLE,
            font_weight: 600,
        },
        TextRole::Heading => RoleTypography {
            role,
            font_size_rem: 1.125,
            line_height: LINE_HEIGHT_HEADING,
            font_weight: 600,
        },
        TextRole::Body => RoleTypography {
            role,
            font_size_rem: 1.0,
            line_height: LINE_HEIGHT_BODY,
            font_weight: 400,
        },
        TextRole::Caption => RoleTypography {
            role,
            font_size_rem: 0.8125,
            line_height: LINE_HEIGHT_CAPTION,
            font_weight: 400,
        },
    }
}

/// Resolves the unitless line-height for a role and estimated line count.
///
/// Any text that wraps to `WRAPPING_LINE_THRESHOLD` (3) or more lines is
/// clamped up to at least `MIN_WRAPPING_LINE_HEIGHT` (`1.4`).
pub fn line_height_for_role(role: TextRole, estimated_lines: usize) -> f32 {
    let base = typography_for_role(role).line_height;
    if estimated_lines >= WRAPPING_LINE_THRESHOLD && base < MIN_WRAPPING_LINE_HEIGHT {
        MIN_WRAPPING_LINE_HEIGHT
    } else {
        base
    }
}

/// Returns `true` when a CSS `line-height` value is a positive finite unitless number.
pub fn is_unitless_css_line_height(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    match trimmed.parse::<f32>() {
        Ok(num) => num.is_finite() && num > 0.0,
        Err(_) => false,
    }
}

/// Formats a CSS rule block with `font-size`, unitless `line-height`, and `font-weight`.
pub fn format_css_rule(selector: &str, role: TextRole, estimated_lines: usize) -> String {
    let typo = typography_for_role(role);
    let line_height = line_height_for_role(role, estimated_lines);
    format!(
        "{selector} {{\n  font-size: {}rem;\n  line-height: {line_height};\n  font-weight: {};\n}}",
        typo.font_size_rem, typo.font_weight
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_exposes_exact_role_line_heights() {
        let s = spec();
        assert_eq!(s.id, "ux_20_line_height_by_role");
        assert_eq!(s.category, "typography");
        assert_eq!(typography_for_role(TextRole::Display).line_height, 1.1);
        assert_eq!(typography_for_role(TextRole::Title).line_height, 1.2);
        assert_eq!(typography_for_role(TextRole::Heading).line_height, 1.3);
        assert_eq!(typography_for_role(TextRole::Body).line_height, 1.5);
        assert_eq!(typography_for_role(TextRole::Caption).line_height, 1.4);
    }

    #[test]
    fn wrapping_three_or_more_lines_enforces_min_1_4() {
        assert_eq!(line_height_for_role(TextRole::Display, 1), 1.1);
        assert_eq!(line_height_for_role(TextRole::Title, 2), 1.2);
        assert_eq!(line_height_for_role(TextRole::Display, 3), 1.4);
        assert_eq!(line_height_for_role(TextRole::Heading, 4), 1.4);
        assert_eq!(line_height_for_role(TextRole::Body, 5), 1.5);
    }

    #[test]
    fn rejects_unitful_line_heights_and_accepts_unitless() {
        assert!(is_unitless_css_line_height("1.5"));
        assert!(is_unitless_css_line_height(" 1.1 "));
        assert!(!is_unitless_css_line_height("24px"));
        assert!(!is_unitless_css_line_height("1.5rem"));
        assert!(!is_unitless_css_line_height("150%"));
        assert!(!is_unitless_css_line_height("0"));
        assert!(!is_unitless_css_line_height("-1.2"));
    }

    #[test]
    fn formats_css_rule_with_unitless_line_height() {
        let css = format_css_rule(".card-description", TextRole::Heading, 3);
        assert!(css.contains("font-size: 1.125rem;"));
        assert!(css.contains("line-height: 1.4;"));
        assert!(css.contains("font-weight: 600;"));
    }
}
