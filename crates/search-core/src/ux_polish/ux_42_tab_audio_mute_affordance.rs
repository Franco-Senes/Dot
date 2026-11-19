//! UX Polish Slice 42: Tab Audio Mute Affordance (`ux_42_tab_audio_mute_affordance`).
//!
//! Defines the state model, concentric geometry, motion/hover CSS rules,
//! OKLCH color tokens, ARIA attributes, and Spanish sentence-case verb-first
//! copy for the tab bar audio-playing and mute/unmute toggle button.

/// Exact easing curve from the UI polish specification.
pub const EASE_STANDARD: &str = "cubic-bezier(0.2, 0, 0, 1)";

/// Exact press-scale factor for interactive buttons.
pub const PRESS_SCALE: f32 = 0.96;

/// Press feedback duration in milliseconds.
pub const PRESS_DURATION_MS: u16 = 150;

/// High-frequency tab icon transition duration in milliseconds.
pub const ICON_TRANSITION_MS: u16 = 150;

/// Concentric radius geometry for the tab and nested mute button (in px).
pub const TAB_OUTER_RADIUS_PX: u8 = 8;
pub const TAB_BUTTON_INSET_PX: u8 = 4;
pub const MUTE_BUTTON_RADIUS_PX: u8 = TAB_OUTER_RADIUS_PX - TAB_BUTTON_INSET_PX;

/// Minimum hit-target dimension (in px) for pointer and touch ergonomics.
pub const MIN_HIT_TARGET_PX: u8 = 28;

/// Icon stroke width (in px) matched to 500-weight tab title text.
pub const ICON_STROKE_WIDTH_PX: f32 = 1.75;

/// High-level specification metadata for this UX overhaul slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub area: &'static str,
    pub press_scale_thousandths: u16,
    pub transition_ms: u16,
    pub easing: &'static str,
    pub outer_radius_px: u8,
    pub inner_radius_px: u8,
    pub inset_px: u8,
    pub min_hit_target_px: u8,
    pub oklch_light_active: &'static str,
    pub oklch_dark_active: &'static str,
    pub oklch_light_muted: &'static str,
    pub oklch_dark_muted: &'static str,
    pub css_fragment: &'static str,
}

/// Returns the canonical UX rule specification for `ux_42_tab_audio_mute_affordance`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_42_tab_audio_mute_affordance",
        title: "Tab audio indicator and mute/unmute affordance",
        area: "tab-bar",
        press_scale_thousandths: 960,
        transition_ms: ICON_TRANSITION_MS,
        easing: EASE_STANDARD,
        outer_radius_px: TAB_OUTER_RADIUS_PX,
        inner_radius_px: MUTE_BUTTON_RADIUS_PX,
        inset_px: TAB_BUTTON_INSET_PX,
        min_hit_target_px: MIN_HIT_TARGET_PX,
        oklch_light_active: "oklch(0.45 0.16 250)",
        oklch_dark_active: "oklch(0.76 0.14 250)",
        oklch_light_muted: "oklch(0.48 0.03 260)",
        oklch_dark_muted: "oklch(0.72 0.03 260)",
        css_fragment: CSS_FRAGMENT,
    }
}

/// Audio and mute state of a browser tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabAudioState {
    /// Tab is not producing audio and is not muted; affordance is hidden.
    Silent,
    /// Tab is actively producing audio and is unmuted.
    Playing,
    /// Tab audio is muted by the user (remains visible even if audio pauses).
    Muted,
}

impl TabAudioState {
    /// Derives the tab audio state from raw playback and mute flags.
    pub fn from_flags(is_audible: bool, is_muted: bool) -> Self {
        if is_muted {
            Self::Muted
        } else if is_audible {
            Self::Playing
        } else {
            Self::Silent
        }
    }

    /// Whether the mute/unmute button should be rendered in the tab strip.
    pub fn is_affordance_visible(self) -> bool {
        !matches!(self, Self::Silent)
    }

    /// Returns the next state when the user activates the mute affordance.
    pub fn toggle_mute(self) -> Self {
        match self {
            Self::Silent => Self::Muted,
            Self::Playing => Self::Muted,
            Self::Muted => Self::Playing,
        }
    }

    /// Whether `aria-pressed` should be `"true"` on the mute toggle button.
    pub fn aria_pressed(self) -> Option<bool> {
        match self {
            Self::Silent => None,
            Self::Playing => Some(false),
            Self::Muted => Some(true),
        }
    }
}

/// Accessible attributes and Spanish sentence-case verb-first copy for the mute button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MuteAffordanceA11y {
    pub visible: bool,
    pub aria_label: String,
    pub tooltip: String,
    pub aria_pressed: bool,
    pub icon_variant: IconVariant,
}

/// Single-SVG state variant; both use `currentColor` and `1.75px` stroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconVariant {
    None,
    SpeakerWaves,
    SpeakerMuted,
}

/// Builds the accessible label, tooltip, and icon state for a tab's audio button.
///
/// Uses Spanish sentence-case, verb-first copy naming the action rather than the glyph.
pub fn build_affordance_a11y(state: TabAudioState, tab_title: Option<&str>) -> MuteAffordanceA11y {
    let trimmed_title = tab_title.map(str::trim).filter(|s| !s.is_empty());

    match state {
        TabAudioState::Silent => MuteAffordanceA11y {
            visible: false,
            aria_label: String::new(),
            tooltip: String::new(),
            aria_pressed: false,
            icon_variant: IconVariant::None,
        },
        TabAudioState::Playing => {
            let base = "Silenciar pestaña";
            let aria_label = match trimmed_title {
                Some(title) => format!("{base}: {title}"),
                None => base.to_string(),
            };
            MuteAffordanceA11y {
                visible: true,
                aria_label,
                tooltip: base.to_string(),
                aria_pressed: false,
                icon_variant: IconVariant::SpeakerWaves,
            }
        }
        TabAudioState::Muted => {
            let base = "Activar sonido de la pestaña";
            let aria_label = match trimmed_title {
                Some(title) => format!("{base}: {title}"),
                None => base.to_string(),
            };
            MuteAffordanceA11y {
                visible: true,
                aria_label,
                tooltip: base.to_string(),
                aria_pressed: true,
                icon_variant: IconVariant::SpeakerMuted,
            }
        }
    }
}

/// Polite live-region announcement in Spanish after toggling mute state.
pub fn toggle_announcement(new_state: TabAudioState) -> &'static str {
    match new_state {
        TabAudioState::Muted => "Pestaña silenciada",
        TabAudioState::Playing => "Sonido de la pestaña activado",
        TabAudioState::Silent => "",
    }
}

/// Checks concentric border-radius invariant: `outer == inner + inset`.
pub fn is_concentric_radius(outer_px: u8, inner_px: u8, inset_px: u8) -> bool {
    outer_px == inner_px.saturating_add(inset_px)
}

/// CSS ruleset for `.tab-audio-mute` implementing exact motion, concentric radius,
/// hover gating, forced-colors border, and reduced-motion fallback.
pub const CSS_FRAGMENT: &str = r#".tab-item {
  border-radius: 8px;
  padding: 4px;
}

.tab-audio-mute {
  min-width: 28px;
  min-height: 28px;
  padding: 4px 4px 4px 2px;
  border-radius: 4px;
  border: 1px solid transparent;
  background-color: transparent;
  color: oklch(0.45 0.16 250);
  -webkit-tap-highlight-color: transparent;
  transition-property: scale, opacity, background-color, color;
  transition-duration: 150ms;
  transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
}

.tab-audio-mute[aria-pressed="true"] {
  color: oklch(0.48 0.03 260);
}

.tab-audio-mute svg {
  stroke: currentColor;
  stroke-width: 1.75px;
  fill: none;
}

.tab-audio-mute:not(:disabled):active {
  scale: 0.96;
}

@media (hover: hover) {
  .tab-audio-mute:not(:disabled):hover {
    background-color: oklch(0 0 0 / 0.08);
  }
}

@media (prefers-reduced-motion: reduce) {
  .tab-audio-mute {
    transition-property: opacity, background-color, color;
  }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_matches_exact_ui_polish_values() {
        let s = spec();
        assert_eq!(s.id, "ux_42_tab_audio_mute_affordance");
        assert_eq!(s.easing, "cubic-bezier(0.2, 0, 0, 1)");
        assert_eq!(s.press_scale_thousandths, 960);
        assert_eq!(s.transition_ms, 150);
        assert!(is_concentric_radius(
            s.outer_radius_px,
            s.inner_radius_px,
            s.inset_px
        ));
        assert!(s.css_fragment.contains("scale: 0.96;"));
        assert!(s.css_fragment.contains("cubic-bezier(0.2, 0, 0, 1)"));
        assert!(s.css_fragment.contains(":not(:disabled):active"));
        assert!(s.css_fragment.contains("@media (hover: hover)"));
        assert!(s
            .css_fragment
            .contains("@media (prefers-reduced-motion: reduce)"));
        assert!(s.css_fragment.contains("border: 1px solid transparent;"));
        assert!(!s.css_fragment.contains("transition: all"));
    }

    #[test]
    fn state_transitions_and_spanish_verb_first_copy() {
        assert_eq!(
            TabAudioState::from_flags(false, false),
            TabAudioState::Silent
        );
        assert!(!TabAudioState::Silent.is_affordance_visible());

        let playing = TabAudioState::from_flags(true, false);
        assert_eq!(playing, TabAudioState::Playing);
        assert!(playing.is_affordance_visible());
        assert_eq!(playing.aria_pressed(), Some(false));

        let playing_a11y = build_affordance_a11y(playing, Some("Música en vivo"));
        assert!(playing_a11y.visible);
        assert_eq!(playing_a11y.tooltip, "Silenciar pestaña");
        assert_eq!(playing_a11y.aria_label, "Silenciar pestaña: Música en vivo");
        assert!(!playing_a11y.aria_pressed);
        assert_eq!(playing_a11y.icon_variant, IconVariant::SpeakerWaves);

        let muted = playing.toggle_mute();
        assert_eq!(muted, TabAudioState::Muted);
        assert_eq!(muted.aria_pressed(), Some(true));
        assert_eq!(toggle_announcement(muted), "Pestaña silenciada");

        let muted_a11y = build_affordance_a11y(muted, None);
        assert!(muted_a11y.visible);
        assert_eq!(muted_a11y.tooltip, "Activar sonido de la pestaña");
        assert_eq!(muted_a11y.aria_label, "Activar sonido de la pestaña");
        assert!(muted_a11y.aria_pressed);
        assert_eq!(muted_a11y.icon_variant, IconVariant::SpeakerMuted);
        assert_eq!(
            toggle_announcement(muted.toggle_mute()),
            "Sonido de la pestaña activado"
        );
    }
}
