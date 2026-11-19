//! UX-17: OKLCH accent ramp and semantic role tokens.
//!
//! Defines a single-hue OKLCH accent system mapped strictly to semantic roles
//! (`--color-accent-subtle`, `--color-accent-border`, `--color-accent-solid`,
//! `--color-accent-solid-hover`, `--color-accent-text`, `--color-text-on-accent`).
//! Enforces one color = one meaning (15-degree hue separation from status hues),
//! one filled primary action per view, and separate light/dark/high-contrast
//! token blocks.

/// Minimum OKLCH hue separation (in degrees) required between the accent hue
/// and any status hue (success, warning, danger).
pub const MIN_STATUS_HUE_SEPARATION_DEG: f32 = 15.0;

/// Maximum number of filled accent primary actions permitted in a single view.
pub const MAX_FILLED_PRIMARY_ACTIONS_PER_VIEW: usize = 1;

/// Default Search Linux accent hue angle in OKLCH (degrees).
pub const ACCENT_HUE_DEG: f32 = 255.0;

/// Canonical semantic accent token names.
pub const SEMANTIC_ACCENT_TOKENS: [&str; 6] = [
    "--color-accent-subtle",
    "--color-accent-border",
    "--color-accent-solid",
    "--color-accent-solid-hover",
    "--color-accent-text",
    "--color-text-on-accent",
];

/// Specification metadata for this UX polish rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub summary: &'static str,
    pub css_formula: &'static str,
}

/// Returns the canonical specification for `ux_17_oklch_accent_role`.
pub const fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_17_oklch_accent_role",
        title: "OKLCH accent ramp and semantic role tokens",
        category: "colors",
        summary: "Map an OKLCH accent ramp to semantic role tokens, keep status hues >= 15 deg away, and fill at most one primary action per view.",
        css_formula: "oklch(L C H) -> --color-accent-{subtle|border|solid|solid-hover|text}",
    }
}

/// An OKLCH color value (`L` in `0.0..=1.0`, `C >= 0.0`, `H` in `0.0..360.0`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OklchColor {
    pub l: f32,
    pub c: f32,
    pub h: f32,
}

impl OklchColor {
    /// Formats the color as a CSS `oklch(...)` function string.
    pub fn to_css(self) -> String {
        format!("oklch({:.3} {:.3} {:.1})", self.l, self.c, self.h)
    }
}

/// Semantic accent role in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccentRole {
    SubtleBg,
    Border,
    Solid,
    SolidHover,
    Text,
    OnAccent,
}

impl AccentRole {
    /// Returns the CSS custom property name for this role.
    pub const fn token_name(self) -> &'static str {
        match self {
            Self::SubtleBg => "--color-accent-subtle",
            Self::Border => "--color-accent-border",
            Self::Solid => "--color-accent-solid",
            Self::SolidHover => "--color-accent-solid-hover",
            Self::Text => "--color-accent-text",
            Self::OnAccent => "--color-text-on-accent",
        }
    }
}

/// Appearance mode for token resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeAppearance {
    Light,
    Dark,
    LightMoreContrast,
    DarkMoreContrast,
}

/// Returns the OKLCH color assigned to an accent role for a given appearance.
///
/// Dark mode lowers chroma slightly and adjusts lightness rather than simply
/// reversing the light ramp; `more` contrast variants widen lightness separation.
pub const fn accent_role_color(role: AccentRole, appearance: ThemeAppearance) -> OklchColor {
    let h = ACCENT_HUE_DEG;
    match (appearance, role) {
        (ThemeAppearance::Light, AccentRole::SubtleBg) => OklchColor {
            l: 0.96,
            c: 0.02,
            h,
        },
        (ThemeAppearance::Light, AccentRole::Border) => OklchColor {
            l: 0.82,
            c: 0.07,
            h,
        },
        (ThemeAppearance::Light, AccentRole::Solid) => OklchColor {
            l: 0.52,
            c: 0.19,
            h,
        },
        (ThemeAppearance::Light, AccentRole::SolidHover) => OklchColor {
            l: 0.46,
            c: 0.20,
            h,
        },
        (ThemeAppearance::Light, AccentRole::Text) => OklchColor {
            l: 0.45,
            c: 0.18,
            h,
        },
        (ThemeAppearance::Light, AccentRole::OnAccent) => OklchColor {
            l: 0.99,
            c: 0.00,
            h,
        },

        (ThemeAppearance::Dark, AccentRole::SubtleBg) => OklchColor {
            l: 0.24,
            c: 0.04,
            h,
        },
        (ThemeAppearance::Dark, AccentRole::Border) => OklchColor {
            l: 0.42,
            c: 0.09,
            h,
        },
        (ThemeAppearance::Dark, AccentRole::Solid) => OklchColor {
            l: 0.62,
            c: 0.16,
            h,
        },
        (ThemeAppearance::Dark, AccentRole::SolidHover) => OklchColor {
            l: 0.68,
            c: 0.15,
            h,
        },
        (ThemeAppearance::Dark, AccentRole::Text) => OklchColor {
            l: 0.74,
            c: 0.14,
            h,
        },
        (ThemeAppearance::Dark, AccentRole::OnAccent) => OklchColor {
            l: 0.14,
            c: 0.01,
            h,
        },

        (ThemeAppearance::LightMoreContrast, AccentRole::SubtleBg) => OklchColor {
            l: 0.94,
            c: 0.03,
            h,
        },
        (ThemeAppearance::LightMoreContrast, AccentRole::Border) => OklchColor {
            l: 0.62,
            c: 0.12,
            h,
        },
        (ThemeAppearance::LightMoreContrast, AccentRole::Solid) => OklchColor {
            l: 0.38,
            c: 0.19,
            h,
        },
        (ThemeAppearance::LightMoreContrast, AccentRole::SolidHover) => OklchColor {
            l: 0.32,
            c: 0.19,
            h,
        },
        (ThemeAppearance::LightMoreContrast, AccentRole::Text) => OklchColor {
            l: 0.35,
            c: 0.18,
            h,
        },
        (ThemeAppearance::LightMoreContrast, AccentRole::OnAccent) => OklchColor {
            l: 1.00,
            c: 0.00,
            h,
        },

        (ThemeAppearance::DarkMoreContrast, AccentRole::SubtleBg) => OklchColor {
            l: 0.20,
            c: 0.05,
            h,
        },
        (ThemeAppearance::DarkMoreContrast, AccentRole::Border) => OklchColor {
            l: 0.56,
            c: 0.12,
            h,
        },
        (ThemeAppearance::DarkMoreContrast, AccentRole::Solid) => OklchColor {
            l: 0.76,
            c: 0.15,
            h,
        },
        (ThemeAppearance::DarkMoreContrast, AccentRole::SolidHover) => OklchColor {
            l: 0.82,
            c: 0.14,
            h,
        },
        (ThemeAppearance::DarkMoreContrast, AccentRole::Text) => OklchColor {
            l: 0.84,
            c: 0.13,
            h,
        },
        (ThemeAppearance::DarkMoreContrast, AccentRole::OnAccent) => OklchColor {
            l: 0.08,
            c: 0.00,
            h,
        },
    }
}

/// Computes the shortest circular angular distance (in degrees, `0.0..=180.0`)
/// between two OKLCH hues.
pub fn hue_distance_deg(hue_a: f32, hue_b: f32) -> f32 {
    let diff = (hue_a - hue_b).rem_euclid(360.0);
    if diff > 180.0 {
        360.0 - diff
    } else {
        diff
    }
}

/// Validates that a status hue (e.g., danger, warning, success) is separated
/// from the accent hue by at least `15°` so users do not confuse status with accent.
pub fn validate_status_hue_separation(
    accent_hue: f32,
    status_hue: f32,
) -> Result<(), &'static str> {
    if !accent_hue.is_finite() || !status_hue.is_finite() {
        return Err("Los ángulos de tono OKLCH deben ser finitos");
    }
    if hue_distance_deg(accent_hue, status_hue) < MIN_STATUS_HUE_SEPARATION_DEG {
        return Err("El tono de estado debe separarse al menos 15 grados del tono de acento");
    }
    Ok(())
}

/// Validates that a semantic token name does not encode a literal hue or component
/// and does not collide with `--color-primary`.
pub fn validate_semantic_token_name(name: &str) -> Result<(), &'static str> {
    if !name.starts_with("--color-") {
        return Err("El token semántico debe comenzar con --color-");
    }
    if name == "--color-primary" {
        return Err("Usar --color-accent-* en lugar de --color-primary para evitar colisión con --color-text-primary");
    }
    let forbidden_hues = ["blue", "red", "green", "gray", "zinc", "slate"];
    for hue in forbidden_hues {
        if name.contains(hue) {
            return Err("El nombre del token semántico no debe incluir un tono literal");
        }
    }
    Ok(())
}

/// Validates that a view has at most one filled accent primary action.
pub fn validate_primary_action_count(filled_accent_buttons: usize) -> Result<(), &'static str> {
    if filled_accent_buttons > MAX_FILLED_PRIMARY_ACTIONS_PER_VIEW {
        return Err("Usar como máximo una acción primaria rellena con acento por vista");
    }
    Ok(())
}

/// Generates the CSS custom property declarations for light, dark, and
/// `prefers-contrast: more` appearances.
pub fn css_tokens_block() -> String {
    let roles = [
        AccentRole::SubtleBg,
        AccentRole::Border,
        AccentRole::Solid,
        AccentRole::SolidHover,
        AccentRole::Text,
        AccentRole::OnAccent,
    ];

    let mut out = String::from(":root {\n");
    for role in roles {
        let c = accent_role_color(role, ThemeAppearance::Light).to_css();
        out.push_str(&format!("  {}: {};\n", role.token_name(), c));
    }
    out.push_str("}\n\n@media (prefers-contrast: more) {\n  :root {\n");
    for role in roles {
        let c = accent_role_color(role, ThemeAppearance::LightMoreContrast).to_css();
        out.push_str(&format!("    {}: {};\n", role.token_name(), c));
    }
    out.push_str("  }\n}\n\n@media (prefers-color-scheme: dark) {\n  :root {\n");
    for role in roles {
        let c = accent_role_color(role, ThemeAppearance::Dark).to_css();
        out.push_str(&format!("    {}: {};\n", role.token_name(), c));
    }
    out.push_str("  }\n\n  @media (prefers-contrast: more) {\n    :root {\n");
    for role in roles {
        let c = accent_role_color(role, ThemeAppearance::DarkMoreContrast).to_css();
        out.push_str(&format!("      {}: {};\n", role.token_name(), c));
    }
    out.push_str("    }\n  }\n}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_and_role_tokens_match_accent_naming_rules() {
        let s = spec();
        assert_eq!(s.id, "ux_17_oklch_accent_role");
        assert_eq!(s.category, "colors");

        for token in SEMANTIC_ACCENT_TOKENS {
            assert!(validate_semantic_token_name(token).is_ok());
        }
        assert!(validate_semantic_token_name("--color-primary").is_err());
        assert!(validate_semantic_token_name("--color-blue-button").is_err());
    }

    #[test]
    fn status_hues_require_fifteen_degree_separation_and_single_primary_action() {
        assert!(validate_status_hue_separation(255.0, 25.0).is_ok());
        assert!(validate_status_hue_separation(255.0, 265.0).is_err());
        assert!(validate_status_hue_separation(355.0, 5.0).is_err());
        assert!(validate_status_hue_separation(355.0, 15.0).is_ok());

        assert!(validate_primary_action_count(0).is_ok());
        assert!(validate_primary_action_count(1).is_ok());
        assert!(validate_primary_action_count(2).is_err());
    }

    #[test]
    fn css_tokens_include_light_dark_and_more_contrast_blocks() {
        let css = css_tokens_block();
        assert!(css.contains("--color-accent-solid: oklch(0.520 0.190 255.0);"));
        assert!(css.contains("--color-text-on-accent: oklch(0.990 0.000 255.0);"));
        assert!(css.contains("@media (prefers-color-scheme: dark)"));
        assert!(css.contains("@media (prefers-contrast: more)"));
    }
}
