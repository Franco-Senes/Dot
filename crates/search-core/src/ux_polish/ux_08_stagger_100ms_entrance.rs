//! UX-08: Stagger infrequent entrances by 100ms.
//!
//! Pure-Rust specification and CSS/state helpers for staged semantic-chunk
//! entrances in Search Linux, following `better-ui` motion rules and
//! `prefers-reduced-motion` accessibility requirements.

/// Stagger interval in milliseconds between consecutive semantic chunks.
pub const STAGGER_STEP_MS: u32 = 100;

/// Entrance animation duration in milliseconds.
pub const ENTER_DURATION_MS: u32 = 300;

/// CSS easing function for staged entrances.
pub const ENTER_EASING: &str = "ease-out";

/// Initial blur radius in pixels for full-motion entrances.
pub const ENTER_BLUR_PX: u32 = 4;

/// Initial vertical offset in pixels (`translateY`) for full-motion entrances.
pub const ENTER_TRANSLATE_Y_PX: u32 = 12;

/// CSS animation fill mode so hidden `from` state never strands elements invisible.
pub const ANIMATION_FILL_MODE: &str = "both";

/// Structured specification for the UX-08 stagger-entrance rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UxRuleSpec {
    /// Unique identifier for this UX rule.
    pub id: &'static str,
    /// Human-readable rule title.
    pub title: &'static str,
    /// Design-system category.
    pub category: &'static str,
    /// Concise summary of the rule.
    pub summary: &'static str,
    /// Delay step between semantic chunks in milliseconds.
    pub stagger_step_ms: u32,
    /// Duration of each chunk's entrance in milliseconds.
    pub enter_duration_ms: u32,
    /// CSS timing function for the entrance.
    pub enter_easing: &'static str,
    /// Initial blur radius in pixels under `prefers-reduced-motion: no-preference`.
    pub enter_blur_px: u32,
    /// Initial vertical translation in pixels under `prefers-reduced-motion: no-preference`.
    pub enter_translate_y_px: u32,
    /// Fallback behavior when reduced motion is preferred.
    pub reduced_motion_fallback: &'static str,
    /// Whether routine interactions may use staggered entrances.
    pub applies_to_routine_interactions: bool,
}

/// UI context in which an element or view is appearing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntranceContext {
    /// Initial load of a welcome or hero view.
    FirstLoadHero,
    /// First-time onboarding sequence.
    Onboarding,
    /// Empty state introducing a surface and its next action.
    EmptyState,
    /// Infrequent completion or success confirmation view.
    SuccessState,
    /// Routine UI interaction (must respond instantly, never staggered).
    RoutineInteraction,
    /// Switching tabs (high-frequency interaction).
    TabSwitch,
    /// Hovering a row or result item (high-frequency interaction).
    RowHover,
    /// Typing or search-as-you-type keystroke feedback.
    Keystroke,
}

/// Returns the canonical specification for `ux_08_stagger_100ms_entrance`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_08_stagger_100ms_entrance",
        title: "Stagger infrequent entrances by 100ms",
        category: "motion",
        summary: "Split infrequent staged entrances into semantic chunks staggered 100ms apart, entering over 300ms ease-out with opacity, 4px blur, and 12px translateY, gated behind prefers-reduced-motion: no-preference.",
        stagger_step_ms: STAGGER_STEP_MS,
        enter_duration_ms: ENTER_DURATION_MS,
        enter_easing: ENTER_EASING,
        enter_blur_px: ENTER_BLUR_PX,
        enter_translate_y_px: ENTER_TRANSLATE_Y_PX,
        reduced_motion_fallback: "opacity-only fade-in over 300ms ease-out",
        applies_to_routine_interactions: false,
    }
}

/// Returns whether a staged 100ms staggered entrance is appropriate for `context`.
///
/// Routine, high-frequency interactions must respond immediately and never stagger.
pub fn is_stagger_allowed(context: EntranceContext) -> bool {
    matches!(
        context,
        EntranceContext::FirstLoadHero
            | EntranceContext::Onboarding
            | EntranceContext::EmptyState
            | EntranceContext::SuccessState
    )
}

/// Computes the entrance delay in milliseconds for a zero-based semantic `chunk_index`.
pub fn stagger_delay_ms(chunk_index: usize) -> u32 {
    let idx = u32::try_from(chunk_index).unwrap_or(u32::MAX);
    idx.saturating_mul(STAGGER_STEP_MS)
}

/// Computes the entrance delay in milliseconds, capped at `max_chunks - 1`.
pub fn stagger_delay_capped_ms(chunk_index: usize, max_chunks: usize) -> u32 {
    if max_chunks == 0 {
        return 0;
    }
    let bounded = chunk_index.min(max_chunks.saturating_sub(1));
    stagger_delay_ms(bounded)
}

/// Generates the CSS rules for `.stagger-item` with `prefers-reduced-motion` gating.
///
/// The hidden state lives in the `from` keyframes with `animation-fill-mode: both`
/// so elements remain visible if animations do not run.
pub fn css_rules(max_chunks: usize) -> String {
    let mut out = String::from(
        ".stagger-item {\n  animation: fade-in 300ms ease-out both;\n}\n\n\
         @media (prefers-reduced-motion: no-preference) {\n  .stagger-item {\n    animation-name: fade-in-up;\n  }\n}\n",
    );

    if max_chunks > 1 {
        out.push('\n');
        for nth in 2..=max_chunks {
            let delay = stagger_delay_ms(nth - 1);
            out.push_str(&format!(
                ".stagger-item:nth-child({nth}) {{ animation-delay: {delay}ms; }}\n"
            ));
        }
    }

    out.push_str(
        "\n@keyframes fade-in {\n  from { opacity: 0; }\n}\n\n\
         @keyframes fade-in-up {\n  from {\n    opacity: 0;\n    transform: translateY(12px);\n    filter: blur(4px);\n  }\n}\n",
    );
    out
}

/// Returns an inline CSS `animation` declaration for a single semantic chunk.
pub fn inline_chunk_style(chunk_index: usize, reduced_motion: bool) -> String {
    let name = if reduced_motion {
        "fade-in"
    } else {
        "fade-in-up"
    };
    let delay = stagger_delay_ms(chunk_index);
    format!(
        "animation: {name} {ENTER_DURATION_MS}ms {ENTER_EASING} {delay}ms {ANIMATION_FILL_MODE};"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_matches_exact_motion_constants() {
        let s = spec();
        assert_eq!(s.id, "ux_08_stagger_100ms_entrance");
        assert_eq!(s.stagger_step_ms, 100);
        assert_eq!(s.enter_duration_ms, 300);
        assert_eq!(s.enter_easing, "ease-out");
        assert_eq!(s.enter_blur_px, 4);
        assert_eq!(s.enter_translate_y_px, 12);
        assert!(!s.applies_to_routine_interactions);
    }

    #[test]
    fn stagger_delays_and_contexts_behave_correctly() {
        assert_eq!(stagger_delay_ms(0), 0);
        assert_eq!(stagger_delay_ms(1), 100);
        assert_eq!(stagger_delay_ms(2), 200);
        assert_eq!(stagger_delay_capped_ms(5, 3), 200);
        assert_eq!(stagger_delay_capped_ms(2, 0), 0);

        assert!(is_stagger_allowed(EntranceContext::FirstLoadHero));
        assert!(is_stagger_allowed(EntranceContext::EmptyState));
        assert!(!is_stagger_allowed(EntranceContext::RoutineInteraction));
        assert!(!is_stagger_allowed(EntranceContext::TabSwitch));
        assert!(!is_stagger_allowed(EntranceContext::RowHover));
        assert!(!is_stagger_allowed(EntranceContext::Keystroke));
    }

    #[test]
    fn css_rules_include_reduced_motion_gate_and_keyframes() {
        let css = css_rules(3);
        assert!(css.contains("animation: fade-in 300ms ease-out both;"));
        assert!(css.contains("@media (prefers-reduced-motion: no-preference)"));
        assert!(css.contains("animation-name: fade-in-up;"));
        assert!(css.contains(".stagger-item:nth-child(2) { animation-delay: 100ms; }"));
        assert!(css.contains(".stagger-item:nth-child(3) { animation-delay: 200ms; }"));
        assert!(css.contains("transform: translateY(12px);"));
        assert!(css.contains("filter: blur(4px);"));

        assert_eq!(
            inline_chunk_style(1, true),
            "animation: fade-in 300ms ease-out 100ms both;"
        );
        assert_eq!(
            inline_chunk_style(2, false),
            "animation: fade-in-up 300ms ease-out 200ms both;"
        );
    }
}
