//! UX Polish Rule 22: Deliberate text wrapping with `text-wrap: balance` and `text-wrap: pretty`.
//!
//! - Headings and short descriptions (up to 6 lines, the Chromium limit) use `text-wrap: balance`.
//! - Paragraphs and multi-line body copy use `text-wrap: pretty` to prevent orphan words on the last line.
//! - Long unbroken tokens (URLs, IDs) use `overflow-wrap: break-word`.
//! - Compact labels and badges use `white-space: nowrap`.
//! - Interface text stays at `text-align: start`.

/// Maximum number of lines Chromium balances with `text-wrap: balance`.
pub const MAX_BALANCE_LINES: usize = 6;

pub const CSS_TEXT_WRAP_BALANCE: &str = "text-wrap: balance;";
pub const CSS_TEXT_WRAP_PRETTY: &str = "text-wrap: pretty;";
pub const CSS_OVERFLOW_WRAP_BREAK_WORD: &str = "overflow-wrap: break-word;";
pub const CSS_WHITE_SPACE_NOWRAP: &str = "white-space: nowrap;";
pub const CSS_TEXT_ALIGN_START: &str = "text-align: start;";

/// High-level specification for a UX polish rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub category: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub css_fragment: &'static str,
    pub notes: &'static [&'static str],
}

/// Semantic role of a text element for wrapping decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextWrapRole {
    Heading,
    ShortDescription,
    Paragraph,
    LabelOrBadge,
    LongTokenOrUrl,
}

/// Resolved CSS text-wrapping strategy for a given role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextWrapMode {
    Balance,
    Pretty,
    Nowrap,
    BreakWord,
}

/// Resolved wrapping rule with exact CSS declarations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextWrapRule {
    pub role: TextWrapRole,
    pub mode: TextWrapMode,
    pub wrap_declaration: &'static str,
    pub align_declaration: &'static str,
}

/// Returns the canonical UX specification for `ux_22_text_wrap_balance_pretty`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_22_text_wrap_balance_pretty",
        category: "typography",
        title: "Deliberate text wrapping with balance and pretty",
        summary: "Use text-wrap: balance on headings and short descriptions (<= 6 lines), and text-wrap: pretty on paragraphs to avoid orphan words.",
        css_fragment: "h1, h2, h3, .ux-heading, .ux-short-desc {\n  text-wrap: balance;\n  text-align: start;\n}\n\np, .ux-body-copy {\n  text-wrap: pretty;\n  text-align: start;\n}\n\n.ux-url, .ux-token {\n  overflow-wrap: break-word;\n}\n\n.ux-badge, .ux-label-nowrap {\n  white-space: nowrap;\n}",
        notes: &[
            "Never apply text-wrap: balance to long-form paragraphs; Chromium ignores it past 6 lines.",
            "text-wrap: pretty keeps a lone word off the last line and is safe on long-form prose.",
            "Interface text uses text-align: start rather than justify.",
        ],
    }
}

/// Returns true when `text-wrap: balance` is appropriate for the given role and line count.
pub fn should_use_balance(role: TextWrapRole, estimated_lines: usize) -> bool {
    matches!(role, TextWrapRole::Heading | TextWrapRole::ShortDescription)
        && estimated_lines <= MAX_BALANCE_LINES
}

/// Resolves the appropriate wrapping rule for a text element.
pub fn wrap_rule_for_role(role: TextWrapRole, estimated_lines: usize) -> TextWrapRule {
    let (mode, wrap_declaration) = match role {
        TextWrapRole::Heading | TextWrapRole::ShortDescription => {
            if should_use_balance(role, estimated_lines) {
                (TextWrapMode::Balance, CSS_TEXT_WRAP_BALANCE)
            } else {
                (TextWrapMode::Pretty, CSS_TEXT_WRAP_PRETTY)
            }
        }
        TextWrapRole::Paragraph => (TextWrapMode::Pretty, CSS_TEXT_WRAP_PRETTY),
        TextWrapRole::LabelOrBadge => (TextWrapMode::Nowrap, CSS_WHITE_SPACE_NOWRAP),
        TextWrapRole::LongTokenOrUrl => (TextWrapMode::BreakWord, CSS_OVERFLOW_WRAP_BREAK_WORD),
    };

    TextWrapRule {
        role,
        mode,
        wrap_declaration,
        align_declaration: CSS_TEXT_ALIGN_START,
    }
}

/// Formats a CSS rule block for a given selector, role, and estimated line count.
pub fn format_css_rule(selector: &str, role: TextWrapRole, estimated_lines: usize) -> String {
    let rule = wrap_rule_for_role(role, estimated_lines);
    format!(
        "{selector} {{\n  {}\n  {}\n}}",
        rule.wrap_declaration, rule.align_declaration
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_exposes_exact_id_and_css_rules() {
        let s = spec();
        assert_eq!(s.id, "ux_22_text_wrap_balance_pretty");
        assert!(s.css_fragment.contains(CSS_TEXT_WRAP_BALANCE));
        assert!(s.css_fragment.contains(CSS_TEXT_WRAP_PRETTY));
        assert!(s.css_fragment.contains(CSS_OVERFLOW_WRAP_BREAK_WORD));
        assert!(s.css_fragment.contains(CSS_WHITE_SPACE_NOWRAP));
        assert!(s.css_fragment.contains(CSS_TEXT_ALIGN_START));
    }

    #[test]
    fn headings_use_balance_up_to_six_lines_then_fall_back_to_pretty() {
        let balanced = wrap_rule_for_role(TextWrapRole::Heading, 3);
        assert_eq!(balanced.mode, TextWrapMode::Balance);
        assert_eq!(balanced.wrap_declaration, "text-wrap: balance;");

        let at_limit = wrap_rule_for_role(TextWrapRole::ShortDescription, MAX_BALANCE_LINES);
        assert_eq!(at_limit.mode, TextWrapMode::Balance);

        let over_limit = wrap_rule_for_role(TextWrapRole::Heading, MAX_BALANCE_LINES + 1);
        assert_eq!(over_limit.mode, TextWrapMode::Pretty);
        assert_eq!(over_limit.wrap_declaration, "text-wrap: pretty;");
    }

    #[test]
    fn paragraphs_never_use_balance_and_always_use_pretty() {
        for lines in [1, 2, 6, 12] {
            assert!(!should_use_balance(TextWrapRole::Paragraph, lines));
            let rule = wrap_rule_for_role(TextWrapRole::Paragraph, lines);
            assert_eq!(rule.mode, TextWrapMode::Pretty);
            assert_eq!(rule.wrap_declaration, "text-wrap: pretty;");
            assert_eq!(rule.align_declaration, "text-align: start;");
        }
    }

    #[test]
    fn labels_and_urls_use_nowrap_and_break_word() {
        let badge_css = format_css_rule(".tab-badge", TextWrapRole::LabelOrBadge, 1);
        assert!(badge_css.contains("white-space: nowrap;"));
        assert!(badge_css.contains("text-align: start;"));

        let url_css = format_css_rule(".omnibox-url", TextWrapRole::LongTokenOrUrl, 2);
        assert!(url_css.contains("overflow-wrap: break-word;"));
        assert!(url_css.contains("text-align: start;"));
    }
}
