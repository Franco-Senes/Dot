//! UX-11: Transition only what changes (exact CSS properties, never `all`).
//!
//! Enforces explicit `transition-property` lists and compositor-safe
//! `will-change` hints so interactive state changes never animate unintended
//! properties (such as colors on theme switches) or trigger per-frame layout.

use std::fmt;

/// Specification metadata for the `ux_11_transition_exact_props` UX rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub summary: &'static str,
    pub canonical_css: &'static str,
    pub forbidden_patterns: &'static [&'static str],
}

/// Returns the canonical specification for `ux_11_transition_exact_props`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_11_transition_exact_props",
        title: "Transition only exact changing properties",
        category: "ui-polish/performance",
        summary: "Name exact changing CSS properties in transition-property and will-change; never use `all`.",
        canonical_css: concat!(
            "transition-property: scale, background-color;\n",
            "transition-duration: 150ms;\n",
            "transition-timing-function: ease-out;"
        ),
        forbidden_patterns: &[
            "transition: all",
            "transition-property: all",
            "transition-all",
            "will-change: all",
        ],
    }
}

/// Classification of a CSS property for transition and `will-change` safety.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertySafety {
    /// Runs on the compositor (`transform`, `translate`, `scale`, `rotate`, `opacity`, `filter`).
    Compositor,
    /// Triggers paint without layout (`background-color`, `color`, `border-color`, `fill`, `stroke`).
    PaintOnly,
    /// Triggers layout recalculation on every frame (`width`, `height`, `top`, `left`, `padding`, `margin`).
    LayoutThrashing,
    /// Blanket `all` keyword, which is always forbidden.
    ForbiddenAll,
}

/// Errors produced when validating transition or `will-change` property lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionValidationError {
    EmptyPropertyList,
    ForbiddenAllKeyword,
    DuplicateProperty(String),
    LayoutPropertyInTransition(String),
    NonCompositedWillChange(String),
    EmptyTimingFunction,
}

impl fmt::Display for TransitionValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPropertyList => write!(f, "property list must not be empty"),
            Self::ForbiddenAllKeyword => {
                write!(f, "`all` is forbidden; name exact changing properties")
            }
            Self::DuplicateProperty(prop) => write!(f, "duplicate property `{prop}`"),
            Self::LayoutPropertyInTransition(prop) => {
                write!(
                    f,
                    "layout-thrashing property `{prop}` should not be transitioned"
                )
            }
            Self::NonCompositedWillChange(prop) => {
                write!(
                    f,
                    "property `{prop}` does not composite and must not appear in will-change"
                )
            }
            Self::EmptyTimingFunction => write!(f, "timing function must not be empty"),
        }
    }
}

impl std::error::Error for TransitionValidationError {}

/// Classifies a CSS property name by its rendering pipeline cost.
pub fn classify_property(prop: &str) -> PropertySafety {
    let normalized = prop.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "all" => PropertySafety::ForbiddenAll,
        "transform" | "translate" | "scale" | "rotate" | "opacity" | "filter" => {
            PropertySafety::Compositor
        }
        "width" | "height" | "min-width" | "max-width" | "min-height" | "max-height" | "top"
        | "right" | "bottom" | "left" | "padding" | "padding-top" | "padding-right"
        | "padding-bottom" | "padding-left" | "margin" | "margin-top" | "margin-right"
        | "margin-bottom" | "margin-left" | "border-width" | "gap" | "font-size" => {
            PropertySafety::LayoutThrashing
        }
        _ => PropertySafety::PaintOnly,
    }
}

/// Validates that a transition property list is non-empty, unique, free of `all`,
/// and free of layout-thrashing properties.
pub fn validate_transition_properties(
    props: &[&str],
) -> Result<Vec<String>, TransitionValidationError> {
    if props.is_empty() {
        return Err(TransitionValidationError::EmptyPropertyList);
    }

    let mut validated = Vec::with_capacity(props.len());
    for raw in props {
        let prop = raw.trim().to_ascii_lowercase();
        if prop.is_empty() {
            return Err(TransitionValidationError::EmptyPropertyList);
        }
        match classify_property(&prop) {
            PropertySafety::ForbiddenAll => {
                return Err(TransitionValidationError::ForbiddenAllKeyword);
            }
            PropertySafety::LayoutThrashing => {
                return Err(TransitionValidationError::LayoutPropertyInTransition(prop));
            }
            PropertySafety::Compositor | PropertySafety::PaintOnly => {}
        }
        if validated.iter().any(|existing| existing == &prop) {
            return Err(TransitionValidationError::DuplicateProperty(prop));
        }
        validated.push(prop);
    }
    Ok(validated)
}

/// Renders explicit CSS transition declarations for the given changing properties.
pub fn render_transition_css(
    props: &[&str],
    duration_ms: u32,
    timing_function: &str,
) -> Result<String, TransitionValidationError> {
    let timing = timing_function.trim();
    if timing.is_empty() {
        return Err(TransitionValidationError::EmptyTimingFunction);
    }
    let validated = validate_transition_properties(props)?;
    Ok(format!(
        "transition-property: {};\ntransition-duration: {}ms;\ntransition-timing-function: {};",
        validated.join(", "),
        duration_ms,
        timing
    ))
}

/// Validates and formats a `will-change` declaration, allowing only compositor-promoted properties.
pub fn render_will_change_css(props: &[&str]) -> Result<String, TransitionValidationError> {
    if props.is_empty() {
        return Err(TransitionValidationError::EmptyPropertyList);
    }

    let mut validated = Vec::with_capacity(props.len());
    for raw in props {
        let prop = raw.trim().to_ascii_lowercase();
        if prop.is_empty() {
            return Err(TransitionValidationError::EmptyPropertyList);
        }
        match classify_property(&prop) {
            PropertySafety::ForbiddenAll => {
                return Err(TransitionValidationError::ForbiddenAllKeyword);
            }
            PropertySafety::Compositor => {}
            PropertySafety::PaintOnly | PropertySafety::LayoutThrashing => {
                return Err(TransitionValidationError::NonCompositedWillChange(prop));
            }
        }
        if validated.iter().any(|existing| existing == &prop) {
            return Err(TransitionValidationError::DuplicateProperty(prop));
        }
        validated.push(prop);
    }

    Ok(format!("will-change: {};", validated.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_exposes_exact_rule_and_forbidden_patterns() {
        let rule = spec();
        assert_eq!(rule.id, "ux_11_transition_exact_props");
        assert!(rule.forbidden_patterns.contains(&"transition: all"));
        assert!(rule.forbidden_patterns.contains(&"transition-all"));
        assert!(rule.forbidden_patterns.contains(&"will-change: all"));
        assert!(rule
            .canonical_css
            .contains("transition-property: scale, background-color;"));
        assert!(rule.canonical_css.contains("transition-duration: 150ms;"));
        assert!(rule
            .canonical_css
            .contains("transition-timing-function: ease-out;"));
    }

    #[test]
    fn render_transition_css_formats_exact_properties() {
        let css = render_transition_css(&["scale", "background-color"], 150, "ease-out")
            .expect("valid transition properties should format");
        assert_eq!(
            css,
            "transition-property: scale, background-color;\ntransition-duration: 150ms;\ntransition-timing-function: ease-out;"
        );

        let bezier_css =
            render_transition_css(&["opacity", "transform"], 300, "cubic-bezier(0.2, 0, 0, 1)")
                .expect("cubic-bezier transition should format");
        assert!(bezier_css.contains("transition-property: opacity, transform;"));
        assert!(bezier_css.contains("cubic-bezier(0.2, 0, 0, 1)"));
    }

    #[test]
    fn rejects_all_duplicates_and_layout_properties() {
        assert_eq!(
            render_transition_css(&["all"], 150, "ease-out"),
            Err(TransitionValidationError::ForbiddenAllKeyword)
        );
        assert_eq!(
            render_transition_css(&["opacity", "opacity"], 150, "ease-out"),
            Err(TransitionValidationError::DuplicateProperty(
                "opacity".to_string()
            ))
        );
        assert_eq!(
            render_transition_css(&["width"], 150, "ease-out"),
            Err(TransitionValidationError::LayoutPropertyInTransition(
                "width".to_string()
            ))
        );
        assert_eq!(
            render_transition_css(&[], 150, "ease-out"),
            Err(TransitionValidationError::EmptyPropertyList)
        );
    }

    #[test]
    fn will_change_allows_only_compositor_properties() {
        let css = render_will_change_css(&["scale", "opacity"])
            .expect("composited properties should be accepted");
        assert_eq!(css, "will-change: scale, opacity;");

        assert_eq!(
            render_will_change_css(&["all"]),
            Err(TransitionValidationError::ForbiddenAllKeyword)
        );
        assert_eq!(
            render_will_change_css(&["background-color"]),
            Err(TransitionValidationError::NonCompositedWillChange(
                "background-color".to_string()
            ))
        );
        assert_eq!(
            render_will_change_css(&["padding"]),
            Err(TransitionValidationError::NonCompositedWillChange(
                "padding".to_string()
            ))
        );
    }
}
