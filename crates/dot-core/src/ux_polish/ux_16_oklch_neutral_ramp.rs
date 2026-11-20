//! UX-16: Perceptually uniform OKLCH neutral ramp and two-tier semantic tokens.
//!
//! Defines a constant-hue neutral ramp (`50` through `950`) in OKLCH space with
//! finer lightness steps at the light end (`0.04`–`0.05` in `L`), peak chroma
//! mid-ramp, and a two-tier CSS custom property mapping for light and dark
//! appearances.

use std::fmt;

/// Specification metadata for the `ux_16_oklch_neutral_ramp` polish rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub summary: &'static str,
    pub css_fragment: &'static str,
    pub rationale: &'static str,
}

/// A color represented in the perceptual OKLCH color space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OklchColor {
    /// Perceived lightness in `[0.0, 1.0]`.
    pub l: f64,
    /// Chroma (saturation), typically `0.0` to `0.4`.
    pub c: f64,
    /// Hue angle in degrees `[0.0, 360.0)`.
    pub h: f64,
    /// Optional alpha channel in `[0.0, 1.0]`.
    pub alpha: Option<f64>,
}

impl OklchColor {
    /// Creates an opaque OKLCH color.
    pub const fn new(l: f64, c: f64, h: f64) -> Self {
        Self {
            l,
            c,
            h,
            alpha: None,
        }
    }

    /// Creates an OKLCH color with an explicit alpha channel.
    pub const fn with_alpha(l: f64, c: f64, h: f64, alpha: f64) -> Self {
        Self {
            l,
            c,
            h,
            alpha: Some(alpha),
        }
    }

    /// Returns `true` when all components are finite and within valid ranges.
    pub fn is_valid(&self) -> bool {
        let alpha_ok = match self.alpha {
            Some(a) => a.is_finite() && (0.0..=1.0).contains(&a),
            None => true,
        };
        self.l.is_finite()
            && (0.0..=1.0).contains(&self.l)
            && self.c.is_finite()
            && self.c >= 0.0
            && self.h.is_finite()
            && (0.0..360.0).contains(&self.h)
            && alpha_ok
    }

    /// Formats the color as a standard CSS `oklch(...)` function string.
    pub fn to_css(&self) -> String {
        match self.alpha {
            Some(a) => format!(
                "oklch({:.3} {:.3} {:.1} / {:.2})",
                self.l, self.c, self.h, a
            ),
            None => format!("oklch({:.3} {:.3} {:.1})", self.l, self.c, self.h),
        }
    }

    /// Converts this OKLCH color to linear sRGB `[r, g, b]`, clamped to `[0.0, 1.0]`.
    pub fn to_linear_srgb(&self) -> [f64; 3] {
        let h_rad = self.h.to_radians();
        let a = self.c * h_rad.cos();
        let b = self.c * h_rad.sin();

        let l_ = self.l + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
        let m_ = self.l - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
        let s_ = self.l - 0.089_484_177_5 * a - 1.291_485_548_0 * b;

        let l = l_ * l_ * l_;
        let m = m_ * m_ * m_;
        let s = s_ * s_ * s_;

        let r = 4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s;
        let g = -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s;
        let b_ch = -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701_0 * s;

        [r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b_ch.clamp(0.0, 1.0)]
    }

    /// Computes WCAG 2.2 relative luminance `Y` in `[0.0, 1.0]`.
    pub fn relative_luminance(&self) -> f64 {
        let [r, g, b] = self.to_linear_srgb();
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    /// Computes the WCAG 2.2 contrast ratio (`1.0` to `21.0`) against `other`.
    pub fn contrast_ratio(&self, other: &Self) -> f64 {
        let l1 = self.relative_luminance();
        let l2 = other.relative_luminance();
        let (lighter, darker) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
        (lighter + 0.05) / (darker + 0.05)
    }
}

/// A single step in the 11-step neutral primitive ramp.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NeutralStep {
    /// Tailwind-compatible numeric step (`50`, `100`, ..., `950`).
    pub step: u16,
    /// OKLCH value for this step.
    pub color: OklchColor,
    /// Primary structural role in light mode.
    pub light_role: &'static str,
}

/// Constant cool-neutral hue (250 deg) shared by every step in the ramp.
pub const NEUTRAL_HUE_DEG: f64 = 250.0;

/// Minimum perceptual lightness separation between adjacent neutral steps.
pub const MIN_ADJACENT_LIGHTNESS_DELTA: f64 = 0.03;

/// Maximum lightness step size for the fine light-end (`50` to `200`).
pub const MAX_LIGHT_END_STEP_DELTA: f64 = 0.055;

/// The canonical 11-step OKLCH neutral ramp for Search Linux.
pub const NEUTRAL_RAMP: [NeutralStep; 11] = [
    NeutralStep {
        step: 50,
        color: OklchColor::new(0.985, 0.004, NEUTRAL_HUE_DEG),
        light_role: "page background",
    },
    NeutralStep {
        step: 100,
        color: OklchColor::new(0.945, 0.006, NEUTRAL_HUE_DEG),
        light_role: "component background",
    },
    NeutralStep {
        step: 200,
        color: OklchColor::new(0.900, 0.008, NEUTRAL_HUE_DEG),
        light_role: "component hover and subtle border",
    },
    NeutralStep {
        step: 300,
        color: OklchColor::new(0.820, 0.010, NEUTRAL_HUE_DEG),
        light_role: "default border",
    },
    NeutralStep {
        step: 400,
        color: OklchColor::new(0.710, 0.012, NEUTRAL_HUE_DEG),
        light_role: "hovered border and disabled text",
    },
    NeutralStep {
        step: 500,
        color: OklchColor::new(0.590, 0.012, NEUTRAL_HUE_DEG),
        light_role: "muted icon and placeholder text",
    },
    NeutralStep {
        step: 600,
        color: OklchColor::new(0.480, 0.011, NEUTRAL_HUE_DEG),
        light_role: "strong border",
    },
    NeutralStep {
        step: 700,
        color: OklchColor::new(0.370, 0.010, NEUTRAL_HUE_DEG),
        light_role: "secondary text",
    },
    NeutralStep {
        step: 800,
        color: OklchColor::new(0.270, 0.008, NEUTRAL_HUE_DEG),
        light_role: "elevated dark surface",
    },
    NeutralStep {
        step: 900,
        color: OklchColor::new(0.200, 0.006, NEUTRAL_HUE_DEG),
        light_role: "primary text",
    },
    NeutralStep {
        step: 950,
        color: OklchColor::new(0.145, 0.005, NEUTRAL_HUE_DEG),
        light_role: "dark page background",
    },
];

/// Semantic neutral token mapping for a specific theme appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticNeutralMapping {
    pub bg_page: u16,
    pub bg_surface: u16,
    pub bg_raised: u16,
    pub bg_sunken: u16,
    pub text_primary: u16,
    pub text_secondary: u16,
    pub text_disabled: u16,
    pub text_inverse: u16,
    pub border_subtle: u16,
    pub border_default: u16,
    pub border_strong: u16,
}

/// Light-mode semantic token mapping onto `NEUTRAL_RAMP`.
pub const LIGHT_SEMANTIC_TOKENS: SemanticNeutralMapping = SemanticNeutralMapping {
    bg_page: 50,
    bg_surface: 100,
    bg_raised: 50,
    bg_sunken: 100,
    text_primary: 900,
    text_secondary: 700,
    text_disabled: 400,
    text_inverse: 50,
    border_subtle: 200,
    border_default: 300,
    border_strong: 600,
};

/// Dark-mode semantic token mapping onto `NEUTRAL_RAMP` (tuned, not naively inverted).
pub const DARK_SEMANTIC_TOKENS: SemanticNeutralMapping = SemanticNeutralMapping {
    bg_page: 950,
    bg_surface: 900,
    bg_raised: 800,
    bg_sunken: 950,
    text_primary: 50,
    text_secondary: 300,
    text_disabled: 500,
    text_inverse: 950,
    border_subtle: 800,
    border_default: 700,
    border_strong: 500,
};

/// Errors produced when validating an OKLCH neutral ramp.
#[derive(Debug, Clone, PartialEq)]
pub enum RampValidationError {
    EmptyRamp,
    InvalidColor {
        step: u16,
    },
    ReachesPureExtreme {
        step: u16,
        l: f64,
    },
    HueDrift {
        step: u16,
        expected: f64,
        actual: f64,
    },
    NonMonotonicOrTooClose {
        prev_step: u16,
        next_step: u16,
        delta_l: f64,
    },
    LightEndTooCoarse {
        prev_step: u16,
        next_step: u16,
        delta_l: f64,
    },
}

impl fmt::Display for RampValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyRamp => write!(f, "neutral ramp must not be empty"),
            Self::InvalidColor { step } => write!(f, "step {step} has invalid OKLCH values"),
            Self::ReachesPureExtreme { step, l } => {
                write!(f, "step {step} lightness {l:.3} reaches pure black or white")
            }
            Self::HueDrift {
                step,
                expected,
                actual,
            } => write!(
                f,
                "step {step} hue {actual:.1} drifts from constant {expected:.1}"
            ),
            Self::NonMonotonicOrTooClose {
                prev_step,
                next_step,
                delta_l,
            } => write!(
                f,
                "steps {prev_step}->{next_step} lightness delta {delta_l:.3} is below {MIN_ADJACENT_LIGHTNESS_DELTA:.3}"
            ),
            Self::LightEndTooCoarse {
                prev_step,
                next_step,
                delta_l,
            } => write!(
                f,
                "light-end steps {prev_step}->{next_step} delta {delta_l:.3} exceeds {MAX_LIGHT_END_STEP_DELTA:.3}"
            ),
        }
    }
}

impl std::error::Error for RampValidationError {}

/// Validates perceptual spacing, constant hue, and non-extreme endpoints of a ramp.
pub fn validate_ramp(steps: &[NeutralStep]) -> Result<(), RampValidationError> {
    let first = steps.first().ok_or(RampValidationError::EmptyRamp)?;
    let base_hue = first.color.h;

    for (idx, item) in steps.iter().enumerate() {
        if !item.color.is_valid() {
            return Err(RampValidationError::InvalidColor { step: item.step });
        }
        if item.color.l <= 0.05 || item.color.l >= 0.995 {
            return Err(RampValidationError::ReachesPureExtreme {
                step: item.step,
                l: item.color.l,
            });
        }
        if (item.color.h - base_hue).abs() > 1e-6 {
            return Err(RampValidationError::HueDrift {
                step: item.step,
                expected: base_hue,
                actual: item.color.h,
            });
        }
        if idx > 0 {
            let prev = steps[idx - 1];
            let delta_l = prev.color.l - item.color.l;
            if delta_l < MIN_ADJACENT_LIGHTNESS_DELTA {
                return Err(RampValidationError::NonMonotonicOrTooClose {
                    prev_step: prev.step,
                    next_step: item.step,
                    delta_l,
                });
            }
            if item.step <= 200 && delta_l > MAX_LIGHT_END_STEP_DELTA {
                return Err(RampValidationError::LightEndTooCoarse {
                    prev_step: prev.step,
                    next_step: item.step,
                    delta_l,
                });
            }
        }
    }

    Ok(())
}

/// Looks up a neutral step's `OklchColor` by numeric step (`50`..=`950`).
pub fn color_for_step(step: u16) -> Option<OklchColor> {
    NEUTRAL_RAMP
        .iter()
        .find(|entry| entry.step == step)
        .map(|entry| entry.color)
}

/// Generates the two-tier CSS custom properties (`:root` primitives + semantics, `.dark` overrides).
pub fn render_css_tokens() -> String {
    let mut out = String::from(":root {\n  /* Tier 1: OKLCH neutral primitives */\n");
    for entry in NEUTRAL_RAMP {
        out.push_str(&format!(
            "  --neutral-{}: {};\n",
            entry.step,
            entry.color.to_css()
        ));
    }
    out.push_str("\n  /* Tier 2: Semantic neutral roles (light) */\n");
    push_semantic_vars(&mut out, &LIGHT_SEMANTIC_TOKENS);
    out.push_str("}\n\n.dark {\n  /* Tier 2: Semantic neutral roles (dark) */\n");
    push_semantic_vars(&mut out, &DARK_SEMANTIC_TOKENS);
    out.push_str("}\n");
    out
}

fn push_semantic_vars(out: &mut String, map: &SemanticNeutralMapping) {
    let pairs = [
        ("bg-page", map.bg_page),
        ("bg-surface", map.bg_surface),
        ("bg-raised", map.bg_raised),
        ("bg-sunken", map.bg_sunken),
        ("text-primary", map.text_primary),
        ("text-secondary", map.text_secondary),
        ("text-disabled", map.text_disabled),
        ("text-inverse", map.text_inverse),
        ("border-subtle", map.border_subtle),
        ("border-default", map.border_default),
        ("border-strong", map.border_strong),
    ];
    for (name, step) in pairs {
        out.push_str(&format!("  --color-{name}: var(--neutral-{step});\n"));
    }
}

/// Returns the canonical UX specification for `ux_16_oklch_neutral_ramp`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_16_oklch_neutral_ramp",
        title: "Perceptually uniform OKLCH neutral ramp",
        category: "color-system",
        summary: "Define an 11-step constant-hue OKLCH neutral ramp (50-950) with fine light-end steps and two-tier semantic tokens for light and dark themes.",
        css_fragment: ":root { --neutral-50: oklch(0.985 0.004 250.0); --neutral-950: oklch(0.145 0.005 250.0); --color-bg-page: var(--neutral-50); --color-text-primary: var(--neutral-900); }",
        rationale: "OKLCH lightness L changes perceptually, avoiding bunched light steps from HSL, while separating primitive and semantic tiers enables clean dark-mode repointing.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_ramp_satisfies_perceptual_and_hue_rules() {
        assert_eq!(NEUTRAL_RAMP.len(), 11);
        assert!(validate_ramp(&NEUTRAL_RAMP).is_ok());

        for entry in NEUTRAL_RAMP {
            assert!(
                (entry.color.h - NEUTRAL_HUE_DEG).abs() < f64::EPSILON,
                "step {} hue drifted",
                entry.step
            );
        }
    }

    #[test]
    fn light_and_dark_text_pairs_meet_wcag_aa_contrast() {
        let light_bg = color_for_step(LIGHT_SEMANTIC_TOKENS.bg_page).expect("light bg");
        let light_text = color_for_step(LIGHT_SEMANTIC_TOKENS.text_primary).expect("light text");
        let light_secondary =
            color_for_step(LIGHT_SEMANTIC_TOKENS.text_secondary).expect("light secondary");

        assert!(light_bg.contrast_ratio(&light_text) >= 7.0);
        assert!(light_bg.contrast_ratio(&light_secondary) >= 4.5);

        let dark_bg = color_for_step(DARK_SEMANTIC_TOKENS.bg_page).expect("dark bg");
        let dark_text = color_for_step(DARK_SEMANTIC_TOKENS.text_primary).expect("dark text");
        let dark_secondary =
            color_for_step(DARK_SEMANTIC_TOKENS.text_secondary).expect("dark secondary");

        assert!(dark_bg.contrast_ratio(&dark_text) >= 7.0);
        assert!(dark_bg.contrast_ratio(&dark_secondary) >= 4.5);
    }

    #[test]
    fn css_output_contains_two_tier_tokens_and_spec_matches() {
        let css = render_css_tokens();
        assert!(css.contains("--neutral-50: oklch(0.985 0.004 250.0);"));
        assert!(css.contains("--neutral-950: oklch(0.145 0.005 250.0);"));
        assert!(css.contains("--color-bg-page: var(--neutral-50);"));
        assert!(css.contains(".dark {"));
        assert!(css.contains("--color-bg-page: var(--neutral-950);"));

        let rule = spec();
        assert_eq!(rule.id, "ux_16_oklch_neutral_ramp");
        assert!(rule.css_fragment.contains("oklch(0.985 0.004 250.0)"));
    }

    #[test]
    fn validate_ramp_rejects_hue_drift_and_coarse_light_steps() {
        let bad_hue = [
            NeutralStep {
                step: 50,
                color: OklchColor::new(0.98, 0.004, 250.0),
                light_role: "bg",
            },
            NeutralStep {
                step: 100,
                color: OklchColor::new(0.94, 0.006, 210.0),
                light_role: "surface",
            },
        ];
        assert!(matches!(
            validate_ramp(&bad_hue),
            Err(RampValidationError::HueDrift { step: 100, .. })
        ));

        let too_coarse = [
            NeutralStep {
                step: 50,
                color: OklchColor::new(0.98, 0.004, 250.0),
                light_role: "bg",
            },
            NeutralStep {
                step: 100,
                color: OklchColor::new(0.88, 0.006, 250.0),
                light_role: "surface",
            },
        ];
        assert!(matches!(
            validate_ramp(&too_coarse),
            Err(RampValidationError::LightEndTooCoarse { .. })
        ));
    }
}
