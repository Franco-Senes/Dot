//! UX-13: Contain scroll inside overlays (`overscroll-behavior: contain`).
//!
//! Scrollable dialogs, drawers, menus, and side panels must set
//! `overscroll-behavior: contain` so scrolling past their boundary
//! never chains into the underlying page or viewport.

/// Specification metadata for the UX-13 overscroll containment rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UxRuleSpec {
    /// Stable rule identifier.
    pub id: &'static str,
    /// Human-readable rule title.
    pub title: &'static str,
    /// Design rationale and behavioral requirement.
    pub rationale: &'static str,
    /// Required CSS declaration.
    pub css_declaration: &'static str,
    /// Ready-to-use CSS fragment for overlay scroll containers.
    pub css_fragment: &'static str,
}

/// Overlay container kinds that host independent scroll regions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlaySurface {
    /// Modal or non-modal dialog body.
    Dialog,
    /// Slide-over drawer panel.
    Drawer,
    /// Dropdown or context menu list.
    Menu,
    /// Persistent or collapsible side panel (bookmarks, history, reading list).
    SidePanel,
    /// Floating popover or autocomplete list.
    Popover,
}

/// CSS `overscroll-behavior` property values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverscrollBehavior {
    /// Prevent scroll chaining to ancestor scroll containers while keeping local bounce/scroll.
    Contain,
    /// Prevent both scroll chaining and local overscroll effects.
    None,
    /// Default browser scroll chaining.
    Auto,
}

impl OverscrollBehavior {
    /// Returns the exact CSS keyword for this behavior.
    pub const fn as_css_value(self) -> &'static str {
        match self {
            Self::Contain => "contain",
            Self::None => "none",
            Self::Auto => "auto",
        }
    }
}

/// Exact CSS declaration required on scrollable overlay surfaces.
pub const OVERSCROLL_CONTAIN_DECL: &str = "overscroll-behavior: contain;";

/// CSS ruleset applying `overscroll-behavior: contain` to scrollable overlays.
pub const OVERLAY_SCROLL_CSS: &str = "\
.ux-overlay-scroll,
[role=\"dialog\"] .scroll-body,
[role=\"menu\"],
.ux-drawer-scroll,
.ux-side-panel-scroll {
    overscroll-behavior: contain;
}";

/// Returns the canonical rule specification for UX-13.
pub const fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_13_overscroll_contain",
        title: "Contain scroll inside overlays",
        rationale: "Give scrollable dialogs, drawers, menus, and side panels `overscroll-behavior: contain` so scrolling past their end never scrolls the page behind.",
        css_declaration: OVERSCROLL_CONTAIN_DECL,
        css_fragment: OVERLAY_SCROLL_CSS,
    }
}

/// Returns the required `overscroll-behavior` for a given overlay surface.
pub const fn overscroll_for_surface(surface: OverlaySurface) -> OverscrollBehavior {
    match surface {
        OverlaySurface::Dialog
        | OverlaySurface::Drawer
        | OverlaySurface::Menu
        | OverlaySurface::SidePanel
        | OverlaySurface::Popover => OverscrollBehavior::Contain,
    }
}

/// Returns `true` if `declarations` enables vertical/auto scrolling without
/// specifying an `overscroll-behavior` rule.
pub fn needs_overscroll_contain(declarations: &str) -> bool {
    let lower = declarations.to_ascii_lowercase();
    let has_scroll = lower.contains("overflow: auto")
        || lower.contains("overflow:auto")
        || lower.contains("overflow-y: auto")
        || lower.contains("overflow-y:auto")
        || lower.contains("overflow-y-auto")
        || lower.contains("overflow-auto");
    let has_overscroll = lower.contains("overscroll-behavior");
    has_scroll && !has_overscroll
}

/// Audits an overlay CSS block and returns the remediation declaration if missing.
pub fn audit_overlay_css(surface: OverlaySurface, declarations: &str) -> Option<&'static str> {
    if needs_overscroll_contain(declarations) {
        let _ = overscroll_for_surface(surface);
        Some(OVERSCROLL_CONTAIN_DECL)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_exposes_exact_overscroll_contain_declaration() {
        let s = spec();
        assert_eq!(s.id, "ux_13_overscroll_contain");
        assert_eq!(s.css_declaration, "overscroll-behavior: contain;");
        assert!(s.css_fragment.contains("overscroll-behavior: contain;"));
    }

    #[test]
    fn all_overlay_surfaces_require_contain() {
        let surfaces = [
            OverlaySurface::Dialog,
            OverlaySurface::Drawer,
            OverlaySurface::Menu,
            OverlaySurface::SidePanel,
            OverlaySurface::Popover,
        ];
        for surface in surfaces {
            assert_eq!(overscroll_for_surface(surface).as_css_value(), "contain");
        }
    }

    #[test]
    fn audit_flags_scrollable_overlay_without_overscroll_behavior() {
        let bad_css = "max-height: 70vh; overflow-y: auto;";
        assert_eq!(
            audit_overlay_css(OverlaySurface::Dialog, bad_css),
            Some("overscroll-behavior: contain;")
        );

        let bad_utility = "overflow-y-auto rounded-lg";
        assert!(needs_overscroll_contain(bad_utility));
    }

    #[test]
    fn audit_passes_when_overscroll_behavior_is_present() {
        let good_css = "overflow-y: auto; overscroll-behavior: contain;";
        assert_eq!(audit_overlay_css(OverlaySurface::SidePanel, good_css), None);
        assert!(!needs_overscroll_contain(good_css));
    }
}
