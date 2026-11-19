//! UX-06: Instant feedback and <=150ms hover/tab transitions.
//!
//! High-frequency interactions (keystrokes, row hovers, tab switches, and
//! hover-revealed row actions) must respond immediately or transition within
//! `150ms` on `opacity` or `background-color` (never `transition: all`).
//! Hover-only rules must be gated behind `@media (hover: hover)` so touch
//! devices do not latch `:hover` after a tap, and every animated state change
//! must retain a static cue (color, icon, or label).

use std::fmt;

/// Maximum allowed transition duration in milliseconds for high-frequency
/// interactions such as row hovers, tab switches, and hover-revealed actions.
pub const MAX_HIGH_FREQ_DURATION_MS: u16 = 150;

/// Standard decelerating easing curve for responsive UI transitions.
pub const EASING_DECELERATE: &str = "cubic-bezier(0.2, 0, 0, 1)";

/// Media query required around `:hover` rules to prevent sticky hover on touch.
pub const HOVER_CAPABLE_MEDIA_QUERY: &str = "@media (hover: hover)";

/// Properties permitted for high-frequency hover/tab transitions.
pub const ALLOWED_HIGH_FREQ_PROPERTIES: &[&str] =
    &["opacity", "background-color", "color", "border-color"];

/// Canonical specification for a UX polish rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UxRuleSpec {
    /// Unique identifier for the UX overhaul item.
    pub id: &'static str,
    /// Human-readable rule title.
    pub title: &'static str,
    /// Skill category owning this rule.
    pub category: &'static str,
    /// Maximum transition duration in milliseconds (`0` for instant).
    pub max_duration_ms: u16,
    /// Exact CSS easing function.
    pub easing: &'static str,
    /// CSS properties allowed in `transition-property`.
    pub allowed_properties: &'static [&'static str],
    /// Whether `:hover` rules must be wrapped in `@media (hover: hover)`.
    pub requires_hover_media_query: bool,
    /// Whether `transition: all` / `transition-all` is forbidden.
    pub forbids_transition_all: bool,
    /// Whether a non-motion static cue (color, icon, or label) is required.
    pub requires_static_cue: bool,
    /// Reference CSS snippet demonstrating compliant hover styling.
    pub reference_css: &'static str,
    /// Spanish UI copy guidance for hover-revealed action labels.
    pub spanish_action_label_example: &'static str,
}

/// High-frequency interaction category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionKind {
    /// Typing or shortcut feedback: must be instant (`0ms`).
    Keystroke,
    /// List/table/omnibox row hover highlight (`<= 150ms`).
    RowHover,
    /// Tab bar switch indicator (`<= 150ms`).
    TabSwitch,
    /// Secondary row action buttons revealed on hover (`<= 150ms`).
    ActionReveal,
}

impl InteractionKind {
    /// Recommended transition duration in milliseconds for this interaction.
    pub const fn recommended_duration_ms(self) -> u16 {
        match self {
            Self::Keystroke => 0,
            Self::RowHover | Self::TabSwitch | Self::ActionReveal => MAX_HIGH_FREQ_DURATION_MS,
        }
    }
}

/// Errors returned when building or validating high-frequency hover CSS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoverSpecError {
    /// Duration exceeded `MAX_HIGH_FREQ_DURATION_MS` (150ms).
    DurationTooLong { got_ms: u16, max_ms: u16 },
    /// No transition properties were provided.
    EmptyProperties,
    /// `all` is forbidden in `transition-property`.
    TransitionAllForbidden,
    /// A property is not in `ALLOWED_HIGH_FREQ_PROPERTIES`.
    UnsupportedProperty(String),
    /// Selector must not be empty.
    EmptySelector,
    /// Validated CSS contains `:hover` outside `@media (hover: hover)`.
    UngatedHoverRule,
}

impl fmt::Display for HoverSpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DurationTooLong { got_ms, max_ms } => {
                write!(f, "duration {got_ms}ms exceeds maximum {max_ms}ms")
            }
            Self::EmptyProperties => write!(f, "at least one transition property is required"),
            Self::TransitionAllForbidden => {
                write!(
                    f,
                    "transition-property: all is forbidden; name exact properties"
                )
            }
            Self::UnsupportedProperty(prop) => {
                write!(
                    f,
                    "property '{prop}' is not permitted for high-frequency hover"
                )
            }
            Self::EmptySelector => write!(f, "CSS selector cannot be empty"),
            Self::UngatedHoverRule => {
                write!(f, ":hover rule must be wrapped in @media (hover: hover)")
            }
        }
    }
}

impl std::error::Error for HoverSpecError {}

/// Returns the canonical `UxRuleSpec` for `ux_06_instant_hover_150ms`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_06_instant_hover_150ms",
        title: "High-frequency interactions respond instantly or within 150ms",
        category: "better-ui",
        max_duration_ms: MAX_HIGH_FREQ_DURATION_MS,
        easing: EASING_DECELERATE,
        allowed_properties: ALLOWED_HIGH_FREQ_PROPERTIES,
        requires_hover_media_query: true,
        forbids_transition_all: true,
        requires_static_cue: true,
        reference_css: concat!(
            ".row-item {\n",
            "  -webkit-tap-highlight-color: transparent;\n",
            "  transition-property: background-color, opacity;\n",
            "  transition-duration: 150ms;\n",
            "  transition-timing-function: cubic-bezier(0.2, 0, 0, 1);\n",
            "}\n",
            "@media (hover: hover) {\n",
            "  .row-item:hover {\n",
            "    background-color: var(--surface-hover);\n",
            "  }\n",
            "}\n",
        ),
        spanish_action_label_example: "Abrir en nueva pestaña",
    }
}

/// Generates compliant CSS for a high-frequency hoverable element.
///
/// Validates that `duration_ms <= 150`, `properties` is non-empty and excludes
/// `"all"`, and wraps the `:hover` rule inside `@media (hover: hover)`.
pub fn hover_transition_css(
    selector: &str,
    properties: &[&str],
    duration_ms: u16,
    hover_background: &str,
) -> Result<String, HoverSpecError> {
    let trimmed_selector = selector.trim();
    if trimmed_selector.is_empty() {
        return Err(HoverSpecError::EmptySelector);
    }
    if duration_ms > MAX_HIGH_FREQ_DURATION_MS {
        return Err(HoverSpecError::DurationTooLong {
            got_ms: duration_ms,
            max_ms: MAX_HIGH_FREQ_DURATION_MS,
        });
    }
    if properties.is_empty() {
        return Err(HoverSpecError::EmptyProperties);
    }

    for &prop in properties {
        let p = prop.trim();
        if p.eq_ignore_ascii_case("all") {
            return Err(HoverSpecError::TransitionAllForbidden);
        }
        if !ALLOWED_HIGH_FREQ_PROPERTIES.contains(&p) {
            return Err(HoverSpecError::UnsupportedProperty(p.to_string()));
        }
    }

    let joined_props = properties
        .iter()
        .map(|p| p.trim())
        .collect::<Vec<_>>()
        .join(", ");

    Ok(format!(
        "{trimmed_selector} {{\n  \
         -webkit-tap-highlight-color: transparent;\n  \
         transition-property: {joined_props};\n  \
         transition-duration: {duration_ms}ms;\n  \
         transition-timing-function: {EASING_DECELERATE};\n\
         }}\n\
         {HOVER_CAPABLE_MEDIA_QUERY} {{\n  \
         {trimmed_selector}:hover {{\n    \
         background-color: {hover_background};\n  \
         }}\n\
         }}\n"
    ))
}

/// Validates a CSS fragment against the `ux_06_instant_hover_150ms` rules.
pub fn validate_transition_css(css: &str) -> Result<(), HoverSpecError> {
    let lower = css.to_ascii_lowercase();
    if lower.contains("transition: all")
        || lower.contains("transition-property: all")
        || lower.contains("transition-all")
    {
        return Err(HoverSpecError::TransitionAllForbidden);
    }

    if lower.contains(":hover") && !lower.contains("@media (hover: hover)") {
        return Err(HoverSpecError::UngatedHoverRule);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_enforces_150ms_and_exact_properties() {
        let s = spec();
        assert_eq!(s.id, "ux_06_instant_hover_150ms");
        assert_eq!(s.max_duration_ms, 150);
        assert_eq!(s.easing, "cubic-bezier(0.2, 0, 0, 1)");
        assert!(s.requires_hover_media_query);
        assert!(s.forbids_transition_all);
        assert!(s.requires_static_cue);
        assert!(s.allowed_properties.contains(&"opacity"));
        assert!(s.allowed_properties.contains(&"background-color"));
        assert_eq!(InteractionKind::Keystroke.recommended_duration_ms(), 0);
        assert_eq!(InteractionKind::RowHover.recommended_duration_ms(), 150);
        assert!(validate_transition_css(s.reference_css).is_ok());
    }

    #[test]
    fn hover_transition_css_wraps_in_media_hover_and_rejects_all() {
        let css = hover_transition_css(
            ".omnibox-row",
            &["background-color", "opacity"],
            120,
            "var(--row-hover)",
        )
        .expect("valid hover css");

        assert!(css.contains("@media (hover: hover)"));
        assert!(css.contains("-webkit-tap-highlight-color: transparent;"));
        assert!(css.contains("transition-property: background-color, opacity;"));
        assert!(css.contains("transition-duration: 120ms;"));
        assert!(css.contains("cubic-bezier(0.2, 0, 0, 1)"));
        assert!(validate_transition_css(&css).is_ok());

        let err = hover_transition_css(".omnibox-row", &["all"], 120, "var(--row-hover)")
            .expect_err("all must be rejected");
        assert_eq!(err, HoverSpecError::TransitionAllForbidden);
    }

    #[test]
    fn hover_transition_css_rejects_durations_over_150ms() {
        let err = hover_transition_css(".tab-btn", &["opacity"], 200, "var(--tab-hover)")
            .expect_err("200ms must be rejected");
        assert_eq!(
            err,
            HoverSpecError::DurationTooLong {
                got_ms: 200,
                max_ms: 150
            }
        );
    }

    #[test]
    fn validate_transition_css_detects_ungated_hover_and_all() {
        assert_eq!(
            validate_transition_css(".btn:hover { opacity: 0.9; }"),
            Err(HoverSpecError::UngatedHoverRule)
        );
        assert_eq!(
            validate_transition_css(".btn { transition: all 150ms ease-out; }"),
            Err(HoverSpecError::TransitionAllForbidden)
        );
    }
}
