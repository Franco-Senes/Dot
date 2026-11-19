//! UX polish specification and helpers for keyboard shortcut badges (`ux_45`).
//!
//! Covers concentric keycap/group radii, OKLCH light/dark tokens, `tabular-nums`
//! alignment, reduced-motion press feedback (`0.96`, `cubic-bezier(0.2, 0, 0, 1)`),
//! ARIA key-shortcut formatting, and Spanish sentence-case verb-first copy.

/// Exact easing curve for state transitions and icon/badge cross-fades.
pub const EASING_STANDARD: &str = "cubic-bezier(0.2, 0, 0, 1)";

/// Exact press-feedback scale for interactive shortcut badges.
pub const PRESS_SCALE: f32 = 0.96;

/// Transition duration in milliseconds for high-frequency badge feedback.
pub const TRANSITION_DURATION_MS: u16 = 150;

/// Maximum padding (in px) where concentric radius calculation applies.
pub const CONCENTRIC_PADDING_LIMIT_PX: u16 = 24;

/// Inner keycap corner radius in px.
pub const KEYCAP_RADIUS_PX: u16 = 4;

/// Padding between a keycap group container and its keycaps in px.
pub const GROUP_PADDING_PX: u16 = 4;

/// Structural border width on keycaps in px (preserved in forced-colors mode).
pub const KEYCAP_BORDER_PX: u16 = 1;

/// CSS font-variant-numeric value ensuring aligned numeric keys (`1`-`9`, `F1`-`F12`).
pub const FONT_VARIANT_NUMERIC: &str = "tabular-nums";

/// OKLCH color token with lightness, chroma, hue, and optional alpha.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OklchToken {
    pub lightness: f32,
    pub chroma: f32,
    pub hue: f32,
    pub alpha: f32,
}

impl OklchToken {
    /// Formats the token as a valid CSS `oklch(...)` color string.
    pub fn to_css(self) -> String {
        if (self.alpha - 1.0).abs() < f32::EPSILON {
            format!(
                "oklch({:.2} {:.2} {:.0})",
                self.lightness, self.chroma, self.hue
            )
        } else {
            format!(
                "oklch({:.2} {:.2} {:.0} / {:.2})",
                self.lightness, self.chroma, self.hue, self.alpha
            )
        }
    }
}

/// Light and dark mode OKLCH palette for keyboard shortcut badges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BadgeColorPalette {
    pub light_surface: OklchToken,
    pub light_text: OklchToken,
    pub light_border: OklchToken,
    pub dark_surface: OklchToken,
    pub dark_text: OklchToken,
    pub dark_border: OklchToken,
}

/// Default OKLCH palette meeting >= 4.5:1 text contrast and >= 3:1 border contrast.
pub const BADGE_PALETTE: BadgeColorPalette = BadgeColorPalette {
    light_surface: OklchToken {
        lightness: 0.97,
        chroma: 0.0,
        hue: 0.0,
        alpha: 1.0,
    },
    light_text: OklchToken {
        lightness: 0.22,
        chroma: 0.0,
        hue: 0.0,
        alpha: 1.0,
    },
    light_border: OklchToken {
        lightness: 0.84,
        chroma: 0.0,
        hue: 0.0,
        alpha: 1.0,
    },
    dark_surface: OklchToken {
        lightness: 0.24,
        chroma: 0.0,
        hue: 0.0,
        alpha: 1.0,
    },
    dark_text: OklchToken {
        lightness: 0.93,
        chroma: 0.0,
        hue: 0.0,
        alpha: 1.0,
    },
    dark_border: OklchToken {
        lightness: 0.38,
        chroma: 0.0,
        hue: 0.0,
        alpha: 1.0,
    },
};

/// Top-level specification for the `ux_45_keyboard_shortcut_badges` rule.
#[derive(Debug, Clone, PartialEq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub easing: &'static str,
    pub press_scale: f32,
    pub transition_ms: u16,
    pub font_variant_numeric: &'static str,
    pub keycap_radius_px: u16,
    pub group_outer_radius_px: u16,
    pub palette: BadgeColorPalette,
}

/// Returns the canonical UX rule specification for keyboard shortcut badges.
pub fn spec() -> UxRuleSpec {
    let group_outer_radius_px =
        concentric_outer_radius_px(KEYCAP_RADIUS_PX, GROUP_PADDING_PX, KEYCAP_BORDER_PX)
            .unwrap_or(KEYCAP_RADIUS_PX);

    UxRuleSpec {
        id: "ux_45_keyboard_shortcut_badges",
        title: "Insignias de atajos de teclado",
        easing: EASING_STANDARD,
        press_scale: PRESS_SCALE,
        transition_ms: TRANSITION_DURATION_MS,
        font_variant_numeric: FONT_VARIANT_NUMERIC,
        keycap_radius_px: KEYCAP_RADIUS_PX,
        group_outer_radius_px,
        palette: BADGE_PALETTE,
    }
}

/// Computes `outer_radius = inner_radius + padding + border_width`.
///
/// Returns `None` when `padding_px` exceeds `24px`, where nested surfaces
/// should be treated as independent layers with their own radius tokens.
pub fn concentric_outer_radius_px(
    inner_radius_px: u16,
    padding_px: u16,
    border_width_px: u16,
) -> Option<u16> {
    if padding_px > CONCENTRIC_PADDING_LIMIT_PX {
        return None;
    }
    inner_radius_px
        .checked_add(padding_px)
        .and_then(|v| v.checked_add(border_width_px))
}

/// Logical key identifier for Linux keyboard shortcuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutKey {
    Ctrl,
    Shift,
    Alt,
    Super,
    Enter,
    Escape,
    Space,
    Backspace,
    Tab,
    Delete,
    Char(char),
    Fn(u8),
}

impl ShortcutKey {
    /// Returns the localized Spanish keycap label for Linux.
    pub fn display_label(self) -> String {
        match self {
            Self::Ctrl => "Ctrl".to_string(),
            Self::Shift => "Mayús".to_string(),
            Self::Alt => "Alt".to_string(),
            Self::Super => "Super".to_string(),
            Self::Enter => "Intro".to_string(),
            Self::Escape => "Esc".to_string(),
            Self::Space => "Espacio".to_string(),
            Self::Backspace => "Retroceso".to_string(),
            Self::Tab => "Tab".to_string(),
            Self::Delete => "Supr".to_string(),
            Self::Char(c) => c.to_ascii_uppercase().to_string(),
            Self::Fn(n) => format!("F{n}"),
        }
    }

    /// Returns the WAI-ARIA `aria-keyshortcuts` token for this key.
    pub fn aria_token(self) -> String {
        match self {
            Self::Ctrl => "Control".to_string(),
            Self::Shift => "Shift".to_string(),
            Self::Alt => "Alt".to_string(),
            Self::Super => "Meta".to_string(),
            Self::Enter => "Enter".to_string(),
            Self::Escape => "Escape".to_string(),
            Self::Space => "Space".to_string(),
            Self::Backspace => "Backspace".to_string(),
            Self::Tab => "Tab".to_string(),
            Self::Delete => "Delete".to_string(),
            Self::Char(c) => c.to_ascii_uppercase().to_string(),
            Self::Fn(n) => format!("F{n}"),
        }
    }
}

/// A keyboard shortcut entry pairing a Spanish verb-first label with its keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutBadge {
    pub action_label: &'static str,
    pub keys: &'static [ShortcutKey],
}

/// Standard Search Linux shortcuts using Spanish sentence-case, verb-first copy.
pub const DEFAULT_SHORTCUTS: &[ShortcutBadge] = &[
    ShortcutBadge {
        action_label: "Abrir barra de comandos",
        keys: &[ShortcutKey::Ctrl, ShortcutKey::Char('k')],
    },
    ShortcutBadge {
        action_label: "Buscar en la página",
        keys: &[ShortcutKey::Ctrl, ShortcutKey::Char('f')],
    },
    ShortcutBadge {
        action_label: "Abrir nueva pestaña",
        keys: &[ShortcutKey::Ctrl, ShortcutKey::Char('t')],
    },
    ShortcutBadge {
        action_label: "Cerrar pestaña actual",
        keys: &[ShortcutKey::Ctrl, ShortcutKey::Char('w')],
    },
    ShortcutBadge {
        action_label: "Mostrar atajos de teclado",
        keys: &[ShortcutKey::Ctrl, ShortcutKey::Char('/')],
    },
];

/// Formats the WAI-ARIA `aria-keyshortcuts` attribute value (e.g. `"Control+K"`).
pub fn format_aria_keyshortcuts(keys: &[ShortcutKey]) -> String {
    keys.iter()
        .map(|k| k.aria_token())
        .collect::<Vec<_>>()
        .join("+")
}

/// Formats a Spanish screen-reader label for a shortcut badge.
pub fn format_aria_label_es(action_label: &str, keys: &[ShortcutKey]) -> String {
    if keys.is_empty() {
        return action_label.to_string();
    }
    let combo = keys
        .iter()
        .map(|k| k.display_label())
        .collect::<Vec<_>>()
        .join(" más ");
    format!("{action_label}, atajo {combo}")
}

/// Renders accessible HTML for a shortcut badge group with `<kbd>` keycaps
/// and `aria-hidden="true"` separators.
pub fn render_badge_html(badge: &ShortcutBadge) -> String {
    let aria_keys = format_aria_keyshortcuts(badge.keys);
    let aria_label = format_aria_label_es(badge.action_label, badge.keys);
    let mut out = format!(
        "<span class=\"kbd-group\" role=\"group\" aria-label=\"{aria_label}\" aria-keyshortcuts=\"{aria_keys}\">"
    );
    for (idx, key) in badge.keys.iter().enumerate() {
        if idx > 0 {
            out.push_str("<span class=\"kbd-sep\" aria-hidden=\"true\">+</span>");
        }
        out.push_str("<kbd class=\"kbd-badge\">");
        out.push_str(&key.display_label());
        out.push_str("</kbd>");
    }
    out.push_str("</span>");
    out
}

/// Returns the CSS stylesheet fragment for keyboard shortcut badges.
pub fn badge_css() -> String {
    let s = spec();
    format!(
        r#".kbd-group {{
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: {group_pad}px;
  border-radius: {outer_r}px;
}}

.kbd-badge {{
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 22px;
  padding: 2px 6px;
  border-radius: {inner_r}px;
  border: {border_w}px solid {light_border};
  background-color: {light_bg};
  color: {light_fg};
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  font-size: 0.75rem;
  font-weight: 500;
  line-height: 1.25;
  font-variant-numeric: {tabular};
  white-space: nowrap;
  user-select: none;
  box-shadow: 0 1px 0 oklch(0.00 0.00 0 / 0.10);
  transition-property: transform, opacity, background-color;
  transition-duration: {dur}ms;
  transition-timing-function: {easing};
}}

.kbd-badge[data-interactive="true"]:not(:disabled):active {{
  transform: scale({scale});
}}

@media (hover: hover) {{
  .kbd-badge[data-interactive="true"]:hover {{
    background-color: oklch(0.94 0.00 0);
  }}
}}

@media (prefers-reduced-motion: reduce) {{
  .kbd-badge {{
    transition-property: opacity, background-color;
  }}
}}

@media (prefers-color-scheme: dark) {{
  .kbd-badge {{
    border-color: {dark_border};
    background-color: {dark_bg};
    color: {dark_fg};
    box-shadow: 0 1px 0 oklch(0.00 0.00 0 / 0.35);
  }}
}}

.kbd-sep {{
  color: oklch(0.55 0.00 0);
  font-size: 0.75rem;
  user-select: none;
}}"#,
        group_pad = GROUP_PADDING_PX,
        outer_r = s.group_outer_radius_px,
        inner_r = s.keycap_radius_px,
        border_w = KEYCAP_BORDER_PX,
        light_border = s.palette.light_border.to_css(),
        light_bg = s.palette.light_surface.to_css(),
        light_fg = s.palette.light_text.to_css(),
        tabular = s.font_variant_numeric,
        dur = s.transition_ms,
        easing = s.easing,
        scale = s.press_scale,
        dark_border = s.palette.dark_border.to_css(),
        dark_bg = s.palette.dark_surface.to_css(),
        dark_fg = s.palette.dark_text.to_css(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_exposes_exact_ux_constants() {
        let s = spec();
        assert_eq!(s.id, "ux_45_keyboard_shortcut_badges");
        assert_eq!(s.easing, "cubic-bezier(0.2, 0, 0, 1)");
        assert!((s.press_scale - 0.96).abs() < f32::EPSILON);
        assert_eq!(s.transition_ms, 150);
        assert_eq!(s.font_variant_numeric, "tabular-nums");
        assert_eq!(s.keycap_radius_px, 4);
        assert_eq!(s.group_outer_radius_px, 9);
    }

    #[test]
    fn concentric_radius_respects_24px_cutoff() {
        assert_eq!(concentric_outer_radius_px(4, 4, 1), Some(9));
        assert_eq!(concentric_outer_radius_px(6, 24, 0), Some(30));
        assert_eq!(concentric_outer_radius_px(6, 25, 0), None);
    }

    #[test]
    fn oklch_tokens_have_high_contrast_separation() {
        let s = spec();
        assert!(s.palette.light_surface.lightness - s.palette.light_text.lightness >= 0.70);
        assert!(s.palette.dark_text.lightness - s.palette.dark_surface.lightness >= 0.65);
        assert_eq!(s.palette.light_surface.to_css(), "oklch(0.97 0.00 0)");
        assert_eq!(s.palette.dark_surface.to_css(), "oklch(0.24 0.00 0)");
    }

    #[test]
    fn spanish_copy_and_aria_formatting() {
        let first = &DEFAULT_SHORTCUTS[0];
        assert_eq!(first.action_label, "Abrir barra de comandos");
        assert_eq!(format_aria_keyshortcuts(first.keys), "Control+K");
        assert_eq!(
            format_aria_label_es(first.action_label, first.keys),
            "Abrir barra de comandos, atajo Ctrl más K"
        );
        let html = render_badge_html(first);
        assert!(html.contains("aria-keyshortcuts=\"Control+K\""));
        assert!(html.contains("<kbd class=\"kbd-badge\">Ctrl</kbd>"));
        assert!(html.contains("<span class=\"kbd-sep\" aria-hidden=\"true\">+</span>"));
    }

    #[test]
    fn css_includes_required_polish_rules() {
        let css = badge_css();
        assert!(css.contains("cubic-bezier(0.2, 0, 0, 1)"));
        assert!(css.contains("transform: scale(0.96);"));
        assert!(css.contains("font-variant-numeric: tabular-nums;"));
        assert!(css.contains("@media (hover: hover)"));
        assert!(css.contains("@media (prefers-reduced-motion: reduce)"));
        assert!(!css.contains("transition-property: all"));
    }
}
