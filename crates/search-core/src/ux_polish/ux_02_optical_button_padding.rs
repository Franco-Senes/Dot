//! UX-02: Optical button padding.
//!
//! Buttons with a leading or trailing icon look visually off-center when
//! given equal horizontal padding because glyphs carry less visual weight at
//! their outer edge than text characters. Reducing padding on the icon side by
//! `2px` (using logical properties `padding-inline-start` and
//! `padding-inline-end` so RTL layouts mirror automatically) restores optical
//! balance. Play-triangle icons are nudged `2px` toward their point using a
//! physical `translateX(2px)` because playback icons never mirror in RTL.

/// Specification metadata and CSS recipe for `ux_02_optical_button_padding`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub summary: &'static str,
    pub css_fragment: &'static str,
    pub optical_offset_px: u16,
    pub default_inline_padding_px: u16,
    pub default_icon_side_padding_px: u16,
    pub play_triangle_nudge_px: u16,
}

/// Which side of the button label holds an icon, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconSide {
    None,
    Leading,
    Trailing,
}

/// Computed logical padding and icon gap values (in pixels) for a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpticalButtonPadding {
    pub inline_start_px: u16,
    pub inline_end_px: u16,
    pub block_px: u16,
    pub icon_gap_px: u16,
}

/// Optical padding reduction applied to the icon side of a text+icon button.
pub const OPTICAL_ICON_NUDGE_PX: u16 = 2;

/// Default horizontal padding on the text side of a standard button.
pub const DEFAULT_INLINE_PADDING_PX: u16 = 16;

/// Default vertical padding for a standard button.
pub const DEFAULT_BLOCK_PADDING_PX: u16 = 8;

/// Default gap between icon and label inside a button.
pub const DEFAULT_ICON_GAP_PX: u16 = 8;

/// Physical X-axis nudge toward the point of a play-triangle SVG.
pub const PLAY_TRIANGLE_NUDGE_PX: u16 = 2;

/// Reusable CSS fragment implementing optical button padding with logical properties.
pub const CSS_FRAGMENT: &str = "\
.sl-btn {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding-block: 8px;
  padding-inline: 16px;
}

.sl-btn--icon-start {
  padding-inline-start: 14px;
  padding-inline-end: 16px;
}

.sl-btn--icon-end {
  padding-inline-start: 16px;
  padding-inline-end: 14px;
}

.sl-btn--play svg {
  transform: translateX(2px);
}
";

/// Computes logical button padding, reducing the icon side by `2px`.
pub const fn optical_padding(
    base_inline_px: u16,
    block_px: u16,
    icon_gap_px: u16,
    side: IconSide,
) -> OpticalButtonPadding {
    let icon_side_px = base_inline_px.saturating_sub(OPTICAL_ICON_NUDGE_PX);
    match side {
        IconSide::None => OpticalButtonPadding {
            inline_start_px: base_inline_px,
            inline_end_px: base_inline_px,
            block_px,
            icon_gap_px,
        },
        IconSide::Leading => OpticalButtonPadding {
            inline_start_px: icon_side_px,
            inline_end_px: base_inline_px,
            block_px,
            icon_gap_px,
        },
        IconSide::Trailing => OpticalButtonPadding {
            inline_start_px: base_inline_px,
            inline_end_px: icon_side_px,
            block_px,
            icon_gap_px,
        },
    }
}

/// Formats computed `OpticalButtonPadding` into logical CSS declarations.
pub fn to_css_inline_style(padding: OpticalButtonPadding) -> String {
    format!(
        "padding-inline-start: {}px; padding-inline-end: {}px; padding-block: {}px; gap: {}px;",
        padding.inline_start_px, padding.inline_end_px, padding.block_px, padding.icon_gap_px,
    )
}

/// Returns the canonical UX rule specification for optical button padding.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_02_optical_button_padding",
        title: "Optical button padding",
        category: "ui_polish",
        summary: "Reduce button inline padding by 2px on the icon side using logical properties, and nudge play triangles 2px toward their point.",
        css_fragment: CSS_FRAGMENT,
        optical_offset_px: OPTICAL_ICON_NUDGE_PX,
        default_inline_padding_px: DEFAULT_INLINE_PADDING_PX,
        default_icon_side_padding_px: DEFAULT_INLINE_PADDING_PX - OPTICAL_ICON_NUDGE_PX,
        play_triangle_nudge_px: PLAY_TRIANGLE_NUDGE_PX,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_exposes_exact_optical_values() {
        let s = spec();
        assert_eq!(s.id, "ux_02_optical_button_padding");
        assert_eq!(s.optical_offset_px, 2);
        assert_eq!(s.default_inline_padding_px, 16);
        assert_eq!(s.default_icon_side_padding_px, 14);
        assert_eq!(s.play_triangle_nudge_px, 2);
        assert!(s.css_fragment.contains("padding-inline-start: 16px;"));
        assert!(s.css_fragment.contains("padding-inline-end: 14px;"));
        assert!(s.css_fragment.contains("translateX(2px)"));
    }

    #[test]
    fn optical_padding_reduces_icon_side_by_2px() {
        let text_only = optical_padding(
            DEFAULT_INLINE_PADDING_PX,
            DEFAULT_BLOCK_PADDING_PX,
            DEFAULT_ICON_GAP_PX,
            IconSide::None,
        );
        assert_eq!(text_only.inline_start_px, 16);
        assert_eq!(text_only.inline_end_px, 16);

        let leading = optical_padding(
            DEFAULT_INLINE_PADDING_PX,
            DEFAULT_BLOCK_PADDING_PX,
            DEFAULT_ICON_GAP_PX,
            IconSide::Leading,
        );
        assert_eq!(leading.inline_start_px, 14);
        assert_eq!(leading.inline_end_px, 16);

        let trailing = optical_padding(
            DEFAULT_INLINE_PADDING_PX,
            DEFAULT_BLOCK_PADDING_PX,
            DEFAULT_ICON_GAP_PX,
            IconSide::Trailing,
        );
        assert_eq!(trailing.inline_start_px, 16);
        assert_eq!(trailing.inline_end_px, 14);

        let clamped = optical_padding(1, 4, 4, IconSide::Trailing);
        assert_eq!(clamped.inline_end_px, 0);
    }

    #[test]
    fn to_css_inline_style_uses_logical_properties() {
        let trailing = optical_padding(16, 8, 8, IconSide::Trailing);
        let style = to_css_inline_style(trailing);
        assert_eq!(
            style,
            "padding-inline-start: 16px; padding-inline-end: 14px; padding-block: 8px; gap: 8px;"
        );
        assert!(!style.contains("padding-left"));
        assert!(!style.contains("padding-right"));
    }
}
