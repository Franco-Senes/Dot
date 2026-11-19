//! UX-29: Visible 2px `:focus-visible` focus ring specification and helpers.
//!
//! Enforces WCAG 2.2 focus visibility (2.4.7), non-text contrast (1.4.11 >= 3:1),
//! and Focus Appearance (2.4.13 AAA: 2px solid perimeter with >= 3:1 contrast),
//! while preserving platform colors in `@media (forced-colors: active)`.

use std::fmt;

/// Task identifier for this UX polish rule.
pub const RULE_ID: &str = "ux_29_focus_ring_2px_visible";

/// Exact perimeter width in CSS pixels (WCAG 2.4.13).
pub const FOCUS_RING_WIDTH_PX: u8 = 2;

/// Exact outline offset in CSS pixels so the ring clears the control boundary.
pub const FOCUS_RING_OFFSET_PX: u8 = 2;

/// Outline line style.
pub const FOCUS_RING_STYLE: &str = "solid";

/// Project CSS custom property for the verified focus ring color.
pub const FOCUS_RING_TOKEN: &str = "var(--focus-ring)";

/// System color keyword used when Windows High Contrast / forced-colors is active.
pub const FORCED_COLORS_RING_COLOR: &str = "Highlight";

/// Minimum non-text contrast ratio required against adjacent surfaces (WCAG 1.4.11 / 2.4.13).
pub const MIN_NON_TEXT_CONTRAST: f32 = 3.0;

/// Primary CSS pseudo-class for keyboard and assistive-technology focus.
pub const FOCUS_SELECTOR: &str = ":focus-visible";

/// CSS pseudo-class for composite controls such as a search box with inner icons.
pub const COMPOSITE_FOCUS_SELECTOR: &str = ":focus-within";

/// Complete specification for the UX-29 visible focus ring rule.
#[derive(Debug, Clone, PartialEq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub wcag_criteria: &'static [&'static str],
    pub summary: &'static str,
    pub focus_ring: FocusRingSpec,
    pub css: &'static str,
}

/// Typed parameters for a 2px visible focus ring.
#[derive(Debug, Clone, PartialEq)]
pub struct FocusRingSpec {
    pub width_px: u8,
    pub offset_px: u8,
    pub style: &'static str,
    pub color_token: &'static str,
    pub forced_colors_color: &'static str,
    pub min_contrast_ratio: f32,
    pub selector: &'static str,
    pub composite_selector: &'static str,
}

impl FocusRingSpec {
    /// Renders the CSS declarations for `:focus-visible` and `@media (forced-colors: active)`.
    pub fn render_css(&self, target_selector: &str) -> String {
        let trimmed = target_selector.trim();
        let prefix = if trimmed.is_empty() { "" } else { trimmed };
        format!(
            "{prefix}{sel} {{\n  outline: {w}px {style} {color};\n  outline-offset: {offset}px;\n}}\n\n@media (forced-colors: active) {{\n  {prefix}{sel} {{\n    outline: {w}px {style} {forced};\n    outline-offset: {offset}px;\n  }}\n}}",
            prefix = prefix,
            sel = self.selector,
            w = self.width_px,
            style = self.style,
            color = self.color_token,
            offset = self.offset_px,
            forced = self.forced_colors_color,
        )
    }

    /// Returns `true` when the measured contrast ratio meets or exceeds `min_contrast_ratio`.
    pub fn meets_contrast(&self, measured_ratio: f32) -> bool {
        measured_ratio.is_finite() && measured_ratio >= self.min_contrast_ratio
    }
}

/// Reasons a CSS fragment violates the UX-29 visible focus ring rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusRingValidationError {
    /// `outline: none` or `outline: 0` suppresses the focus indicator without replacement.
    OutlineSuppressed,
    /// Bare `:focus` is used instead of `:focus-visible` (or `:focus-within`).
    BareFocusSelector,
    /// Required `2px` solid outline width is missing.
    MissingTwoPxOutline,
    /// Forced-colors fallback is missing when custom outline colors are used.
    MissingForcedColorsFallback,
}

impl fmt::Display for FocusRingValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutlineSuppressed => {
                write!(
                    f,
                    "outline: none/0 is forbidden without a verified focus ring"
                )
            }
            Self::BareFocusSelector => {
                write!(
                    f,
                    "use :focus-visible or :focus-within instead of bare :focus"
                )
            }
            Self::MissingTwoPxOutline => {
                write!(f, "focus ring must specify a 2px solid outline")
            }
            Self::MissingForcedColorsFallback => {
                write!(
                    f,
                    "custom focus ring must preserve @media (forced-colors: active)"
                )
            }
        }
    }
}

/// Canonical CSS fragment for Search Linux interactive controls and composite search inputs.
pub const CANONICAL_FOCUS_RING_CSS: &str = "\
:focus-visible {
  outline: 2px solid var(--focus-ring);
  outline-offset: 2px;
}

.search-box:focus-within {
  outline: 2px solid var(--focus-ring);
  outline-offset: 2px;
}

@media (forced-colors: active) {
  :focus-visible,
  .search-box:focus-within {
    outline: 2px solid Highlight;
    outline-offset: 2px;
  }
}";

/// Returns the canonical UX-29 specification.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: RULE_ID,
        title: "Visible 2px focus-visible ring",
        wcag_criteria: &["1.4.11", "2.4.7", "2.4.13"],
        summary: "Style :focus-visible (and :focus-within for composite search inputs) with a 2px solid outline, 2px offset, >= 3:1 contrast, and forced-colors Highlight fallback.",
        focus_ring: FocusRingSpec {
            width_px: FOCUS_RING_WIDTH_PX,
            offset_px: FOCUS_RING_OFFSET_PX,
            style: FOCUS_RING_STYLE,
            color_token: FOCUS_RING_TOKEN,
            forced_colors_color: FORCED_COLORS_RING_COLOR,
            min_contrast_ratio: MIN_NON_TEXT_CONTRAST,
            selector: FOCUS_SELECTOR,
            composite_selector: COMPOSITE_FOCUS_SELECTOR,
        },
        css: CANONICAL_FOCUS_RING_CSS,
    }
}

/// Validates a CSS snippet against the UX-29 focus ring rules.
pub fn validate_focus_ring_css(css: &str) -> Result<(), FocusRingValidationError> {
    let normalized = css.to_ascii_lowercase();

    if normalized.contains("outline: none")
        || normalized.contains("outline:none")
        || normalized.contains("outline: 0;")
        || normalized.contains("outline:0;")
    {
        return Err(FocusRingValidationError::OutlineSuppressed);
    }

    if has_bare_focus_selector(&normalized) {
        return Err(FocusRingValidationError::BareFocusSelector);
    }

    if !normalized.contains("2px solid") {
        return Err(FocusRingValidationError::MissingTwoPxOutline);
    }

    if !normalized.contains("forced-colors: active") {
        return Err(FocusRingValidationError::MissingForcedColorsFallback);
    }

    Ok(())
}

fn has_bare_focus_selector(normalized_css: &str) -> bool {
    let bytes = normalized_css.as_bytes();
    let needle = b":focus";
    let mut idx = 0;

    while idx + needle.len() <= bytes.len() {
        if &bytes[idx..idx + needle.len()] == needle {
            let rest = &normalized_css[idx + needle.len()..];
            if !rest.starts_with("-visible") && !rest.starts_with("-within") {
                return true;
            }
            idx += needle.len();
        } else {
            idx += 1;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_matches_exact_focus_ring_values() {
        let rule = spec();
        assert_eq!(rule.id, "ux_29_focus_ring_2px_visible");
        assert_eq!(rule.focus_ring.width_px, 2);
        assert_eq!(rule.focus_ring.offset_px, 2);
        assert_eq!(rule.focus_ring.style, "solid");
        assert_eq!(rule.focus_ring.color_token, "var(--focus-ring)");
        assert_eq!(rule.focus_ring.forced_colors_color, "Highlight");
        assert!((rule.focus_ring.min_contrast_ratio - 3.0).abs() < f32::EPSILON);
        assert_eq!(rule.focus_ring.selector, ":focus-visible");
        assert_eq!(rule.focus_ring.composite_selector, ":focus-within");
        assert!(rule.wcag_criteria.contains(&"1.4.11"));
        assert!(rule.wcag_criteria.contains(&"2.4.7"));
        assert!(rule.wcag_criteria.contains(&"2.4.13"));
    }

    #[test]
    fn canonical_css_passes_validation() {
        let rule = spec();
        assert_eq!(validate_focus_ring_css(rule.css), Ok(()));
    }

    #[test]
    fn rendered_css_passes_validation_and_scopes_selector() {
        let ring = spec().focus_ring;
        let rendered = ring.render_css("button.primary");
        assert!(rendered.contains("button.primary:focus-visible"));
        assert!(rendered.contains("outline: 2px solid var(--focus-ring);"));
        assert!(rendered.contains("outline-offset: 2px;"));
        assert!(rendered.contains("@media (forced-colors: active)"));
        assert!(rendered.contains("outline: 2px solid Highlight;"));
        assert_eq!(validate_focus_ring_css(&rendered), Ok(()));
    }

    #[test]
    fn validation_rejects_suppressed_or_bare_focus_and_checks_contrast() {
        assert_eq!(
            validate_focus_ring_css(":focus-visible { outline: none; }"),
            Err(FocusRingValidationError::OutlineSuppressed)
        );
        assert_eq!(
            validate_focus_ring_css("button:focus { outline: 2px solid var(--focus-ring); }"),
            Err(FocusRingValidationError::BareFocusSelector)
        );
        assert_eq!(
            validate_focus_ring_css(":focus-visible { outline: 1px solid var(--focus-ring); }"),
            Err(FocusRingValidationError::MissingTwoPxOutline)
        );
        assert_eq!(
            validate_focus_ring_css(":focus-visible { outline: 2px solid var(--focus-ring); }"),
            Err(FocusRingValidationError::MissingForcedColorsFallback)
        );

        let ring = spec().focus_ring;
        assert!(!ring.meets_contrast(2.99));
        assert!(ring.meets_contrast(3.0));
        assert!(ring.meets_contrast(4.5));
        assert!(!ring.meets_contrast(f32::NAN));
    }
}
