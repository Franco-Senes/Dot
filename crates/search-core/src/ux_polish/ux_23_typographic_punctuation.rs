//! UX-23: Natural case and typographic punctuation.
//!
//! Store interface copy in natural case and apply case styling with CSS
//! `text-transform` so a redesign never requires rewriting strings.
//! Rendered UI prose uses typographic characters—curly quotes, en dashes for
//! ranges, em dashes for asides, the single ellipsis character, and
//! non-breaking spaces before units—rather than keyboard stand-ins. Code
//! blocks and monospace inputs keep straight ASCII punctuation.

/// Single-character horizontal ellipsis (`…`, U+2026), replacing `...`.
pub const ELLIPSIS: char = '\u{2026}';

/// Em dash (`—`, U+2014) for parenthetical asides, replacing `--`.
pub const EM_DASH: char = '\u{2014}';

/// En dash (`–`, U+2013) for numeric or date ranges such as `2010–2020`.
pub const EN_DASH: char = '\u{2013}';

/// Left double curly quote (`“`, U+201C).
pub const LEFT_DOUBLE_QUOTE: char = '\u{201C}';

/// Right double curly quote (`”`, U+201D).
pub const RIGHT_DOUBLE_QUOTE: char = '\u{201D}';

/// Left single curly quote (`‘`, U+2018).
pub const LEFT_SINGLE_QUOTE: char = '\u{2018}';

/// Right single curly quote / typographic apostrophe (`’`, U+2019).
pub const RIGHT_SINGLE_QUOTE: char = '\u{2019}';

/// Non-breaking space (U+00A0) used between numbers and units (`10 km`).
pub const NBSP: char = '\u{00A0}';

/// Specification metadata for this UX polish rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub summary: &'static str,
    pub css_property: &'static str,
}

/// Returns the canonical specification for `ux_23_typographic_punctuation`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_23_typographic_punctuation",
        title: "Natural case and typographic punctuation",
        category: "typography",
        summary: "Store copy in natural case with CSS text-transform; use curly quotes, en/em dashes, single-character ellipsis, and non-breaking unit spaces in UI prose while keeping straight quotes in code.",
        css_property: "text-transform: uppercase | lowercase | capitalize | none",
    }
}

/// CSS `text-transform` options for styling natural-case source strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextTransform {
    None,
    Uppercase,
    Lowercase,
    Capitalize,
}

impl TextTransform {
    /// Returns the exact CSS declaration for this transform.
    pub fn css_declaration(self) -> &'static str {
        match self {
            Self::None => "text-transform: none;",
            Self::Uppercase => "text-transform: uppercase;",
            Self::Lowercase => "text-transform: lowercase;",
            Self::Capitalize => "text-transform: capitalize;",
        }
    }
}

/// Context determining whether smart punctuation substitutions apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PunctuationContext {
    /// Rendered UI labels, headings, descriptions, and empty states.
    UiProse,
    /// Inline code, code blocks, CLI flags, or raw query syntax.
    Code,
}

/// A single entry in the smart-punctuation substitution table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubstitutionRule {
    pub ascii_stand_in: &'static str,
    pub typographic: &'static str,
    pub usage: &'static str,
}

/// Canonical smart-punctuation substitution table for UI prose.
pub const SMART_PUNCTUATION_RULES: &[SubstitutionRule] = &[
    SubstitutionRule {
        ascii_stand_in: "\"...\"",
        typographic: "\u{201C}...\u{201D}",
        usage: "Curly double quotes around quoted UI text (keep straight quotes in code)",
    },
    SubstitutionRule {
        ascii_stand_in: "'",
        typographic: "\u{2019}",
        usage: "Typographic apostrophe in contractions and possessives",
    },
    SubstitutionRule {
        ascii_stand_in: "2010-2020",
        typographic: "2010\u{2013}2020",
        usage: "En dash in numeric, date, or time ranges",
    },
    SubstitutionRule {
        ascii_stand_in: "--",
        typographic: "\u{2014}",
        usage: "Em dash for parenthetical asides",
    },
    SubstitutionRule {
        ascii_stand_in: "...",
        typographic: "\u{2026}",
        usage: "Single horizontal ellipsis character",
    },
    SubstitutionRule {
        ascii_stand_in: "10 km",
        typographic: "10\u{00A0}km",
        usage: "Non-breaking space between a numeric value and its unit",
    },
];

/// Kinds of ASCII stand-ins detected in rendered UI prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PunctuationIssue {
    ThreePeriodsEllipsis,
    DoubleHyphenEmDash,
    HyphenatedNumericRange,
    StraightDoubleQuote,
    StraightSingleQuoteOrApostrophe,
}

/// Formats a numeric or date range using an en dash (`2010–2020`).
pub fn format_range(start: &str, end: &str) -> String {
    let mut out = String::with_capacity(start.len() + end.len() + EN_DASH.len_utf8());
    out.push_str(start.trim());
    out.push(EN_DASH);
    out.push_str(end.trim());
    out
}

/// Joins a numeric value and its unit with a non-breaking space (`10\u{00A0}km`).
pub fn format_quantity_with_unit(value: &str, unit: &str) -> String {
    let mut out = String::with_capacity(value.len() + unit.len() + NBSP.len_utf8());
    out.push_str(value.trim());
    out.push(NBSP);
    out.push_str(unit.trim());
    out
}

/// Converts ASCII keyboard stand-ins in `input` to typographic punctuation
/// when `context` is `PunctuationContext::UiProse`. Returns `input` unchanged
/// for `PunctuationContext::Code`.
pub fn format_typographic_punctuation(input: &str, context: PunctuationContext) -> String {
    if context == PunctuationContext::Code {
        return input.to_string();
    }

    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    let mut double_quote_open = false;
    let mut single_quote_open = false;

    while i < chars.len() {
        let ch = chars[i];

        // 1. Three periods `...` -> `…`
        if ch == '.' && i + 2 < chars.len() && chars[i + 1] == '.' && chars[i + 2] == '.' {
            out.push(ELLIPSIS);
            i += 3;
            continue;
        }

        // 2. Two hyphens `--` -> `—` (consume optional third hyphen `---` too)
        if ch == '-' && i + 1 < chars.len() && chars[i + 1] == '-' {
            out.push(EM_DASH);
            if i + 2 < chars.len() && chars[i + 2] == '-' {
                i += 3;
            } else {
                i += 2;
            }
            continue;
        }

        // 3. Hyphen between ASCII digits `2010-2020` -> `2010–2020`
        if ch == '-'
            && i > 0
            && i + 1 < chars.len()
            && chars[i - 1].is_ascii_digit()
            && chars[i + 1].is_ascii_digit()
        {
            out.push(EN_DASH);
            i += 1;
            continue;
        }

        // 4. Straight double quotes `"` -> curly `“` / `”`
        if ch == '"' {
            let prev_is_boundary = i == 0 || is_opening_boundary(chars[i - 1]);
            if !double_quote_open && prev_is_boundary {
                out.push(LEFT_DOUBLE_QUOTE);
                double_quote_open = true;
            } else {
                out.push(RIGHT_DOUBLE_QUOTE);
                double_quote_open = false;
            }
            i += 1;
            continue;
        }

        // 5. Straight single quote / apostrophe `'` -> `‘` / `’`
        if ch == '\'' {
            let prev_is_alnum = i > 0 && chars[i - 1].is_alphanumeric();
            let next_is_alnum = i + 1 < chars.len() && chars[i + 1].is_alphanumeric();

            // Apostrophe inside a word (e.g. `don't`, `users'`) always uses right single quote.
            if prev_is_alnum && next_is_alnum {
                out.push(RIGHT_SINGLE_QUOTE);
            } else if !single_quote_open && (i == 0 || is_opening_boundary(chars[i - 1])) {
                out.push(LEFT_SINGLE_QUOTE);
                single_quote_open = true;
            } else {
                out.push(RIGHT_SINGLE_QUOTE);
                single_quote_open = false;
            }
            i += 1;
            continue;
        }

        out.push(ch);
        i += 1;
    }

    out
}

/// Audits rendered UI prose for ASCII punctuation stand-ins.
pub fn audit_keyboard_standins(text: &str) -> Vec<PunctuationIssue> {
    let chars: Vec<char> = text.chars().collect();
    let mut issues = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        if ch == '.' && i + 2 < chars.len() && chars[i + 1] == '.' && chars[i + 2] == '.' {
            push_unique(&mut issues, PunctuationIssue::ThreePeriodsEllipsis);
            i += 3;
            continue;
        }
        if ch == '-' && i + 1 < chars.len() && chars[i + 1] == '-' {
            push_unique(&mut issues, PunctuationIssue::DoubleHyphenEmDash);
            i += 2;
            continue;
        }
        if ch == '-'
            && i > 0
            && i + 1 < chars.len()
            && chars[i - 1].is_ascii_digit()
            && chars[i + 1].is_ascii_digit()
        {
            push_unique(&mut issues, PunctuationIssue::HyphenatedNumericRange);
        } else if ch == '"' {
            push_unique(&mut issues, PunctuationIssue::StraightDoubleQuote);
        } else if ch == '\'' {
            push_unique(
                &mut issues,
                PunctuationIssue::StraightSingleQuoteOrApostrophe,
            );
        }
        i += 1;
    }

    issues
}

fn is_opening_boundary(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '(' | '[' | '{' | EM_DASH | EN_DASH | LEFT_DOUBLE_QUOTE | LEFT_SINGLE_QUOTE
        )
}

fn push_unique(list: &mut Vec<PunctuationIssue>, issue: PunctuationIssue) {
    if !list.contains(&issue) {
        list.push(issue);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_reports_exact_metadata() {
        let s = spec();
        assert_eq!(s.id, "ux_23_typographic_punctuation");
        assert_eq!(s.category, "typography");
        assert_eq!(
            TextTransform::Uppercase.css_declaration(),
            "text-transform: uppercase;"
        );
    }

    #[test]
    fn converts_ui_prose_to_typographic_punctuation() {
        let input = r#"Searching "quarterly" reports 2010-2020 -- don't close..."#;
        let formatted = format_typographic_punctuation(input, PunctuationContext::UiProse);
        assert_eq!(
            formatted,
            "Searching \u{201C}quarterly\u{201D} reports 2010\u{2013}2020 \u{2014} don\u{2019}t close\u{2026}"
        );
        assert!(audit_keyboard_standins(&formatted).is_empty());
    }

    #[test]
    fn preserves_straight_punctuation_in_code_context() {
        let code = r#"let range = "2010-2020"; // wait... -- 'ok'"#;
        assert_eq!(
            format_typographic_punctuation(code, PunctuationContext::Code),
            code
        );
    }

    #[test]
    fn formats_ranges_and_quantities_with_exact_unicode() {
        assert_eq!(format_range("2010", "2020"), "2010\u{2013}2020");
        assert_eq!(format_quantity_with_unit("10", "km"), "10\u{00A0}km");
    }

    #[test]
    fn audit_detects_all_ascii_standins() {
        let bad = r#"No results for "test" in 2010-2020 -- try 'again'..."#;
        let issues = audit_keyboard_standins(bad);
        assert_eq!(
            issues,
            vec![
                PunctuationIssue::StraightDoubleQuote,
                PunctuationIssue::HyphenatedNumericRange,
                PunctuationIssue::DoubleHyphenEmDash,
                PunctuationIssue::StraightSingleQuoteOrApostrophe,
                PunctuationIssue::ThreePeriodsEllipsis,
            ]
        );
    }
}
