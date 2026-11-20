//! UX-15: Zero-emoji & vector icon audit for Search Linux.
//!
//! Ensures UI chrome, empty states, status badges, and accessible labels contain
//! zero raw Unicode emojis and instead use `currentColor` vector SVGs with
//! stroke weights matched to adjacent typography and verb-first labels.

/// Specification metadata for a Search Linux UX polish rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UxRuleSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub summary: &'static str,
    pub css_tokens: &'static [(&'static str, &'static str)],
    pub invariants: &'static [&'static str],
}

/// Icon state variant: outline by default, filled when active or selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorIconVariant {
    Outline,
    Filled,
}

/// Vector icon specification enforcing `currentColor` and stroke-weight pairing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VectorIconSpec {
    pub name: &'static str,
    pub stroke_px: f32,
    pub color: &'static str,
    pub variant: VectorIconVariant,
    pub mirror_in_rtl: bool,
    pub aria_label: &'static str,
}

/// Severity level for an audit finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditSeverity {
    Error,
    Warning,
}

/// Structured finding returned by the zero-emoji and vector SVG auditors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditFinding {
    pub rule_id: &'static str,
    pub severity: AuditSeverity,
    pub message: &'static str,
    pub remediation: &'static str,
}

/// Exact CSS color token required on all UI vector icons.
pub const ICON_COLOR_TOKEN: &str = "currentColor";

/// Icon stroke width paired with regular (400) body text.
pub const ICON_STROKE_REGULAR_PX: f32 = 1.5;

/// Icon stroke width paired with bold (700) heading or label text.
pub const ICON_STROKE_BOLD_PX: f32 = 2.5;

/// Exact easing curve for contextual icon cross-fades.
pub const ICON_TRANSITION_EASING: &str = "cubic-bezier(0.2, 0, 0, 1)";

/// Duration in milliseconds for contextual icon cross-fades.
pub const ICON_CROSSFADE_DURATION_MS: u16 = 300;

/// Exact press scale for interactive icon buttons.
pub const PRESS_SCALE: f32 = 0.96;

/// Duration in milliseconds for interactive press feedback.
pub const PRESS_DURATION_MS: u16 = 150;

/// Canonical CSS fragment for `currentColor` vector icons and press feedback.
pub const VECTOR_ICON_CSS: &str = r#".sl-icon {
  color: currentColor;
  stroke: currentColor;
  stroke-width: 1.5px;
  fill: none;
  transition-property: opacity, scale, filter;
  transition-duration: 300ms;
  transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
}

.sl-icon--bold {
  stroke-width: 2.5px;
}

.sl-icon--active {
  fill: currentColor;
}

.sl-icon-btn:not(:disabled):active {
  scale: 0.96;
  transition-property: scale;
  transition-duration: 150ms;
  transition-timing-function: ease-out;
}

@media (prefers-reduced-motion: reduce) {
  .sl-icon {
    transition-property: opacity;
    transition-duration: 150ms;
    filter: none;
  }
}
"#;

/// Verb-first Spanish accessible labels for core Search Linux icon-only controls.
pub const SPANISH_ICON_LABELS: &[(&str, &str)] = &[
    ("search", "Buscar en la web"),
    ("close_tab", "Cerrar pestana"),
    ("reload", "Recargar pagina"),
    ("bookmark", "Guardar marcador"),
    ("delete", "Eliminar elemento"),
];

/// Returns the canonical specification for `ux_15_zero_emoji_vector_audit`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_15_zero_emoji_vector_audit",
        title: "Zero-emoji & vector icon audit",
        category: "icons-and-copy",
        summary: "Prohibit raw Unicode emojis in UI chrome and enforce single-SVG currentColor vector icons with stroke matched to text weight and verb-first accessible labels.",
        css_tokens: &[
            ("--sl-icon-color", ICON_COLOR_TOKEN),
            ("--sl-icon-stroke-regular", "1.5px"),
            ("--sl-icon-stroke-bold", "2.5px"),
            ("--sl-icon-ease", ICON_TRANSITION_EASING),
            ("--sl-icon-press-scale", "0.96"),
        ],
        invariants: &[
            "Zero Unicode emoji or pictograph code points in UI strings or labels",
            "Every SVG icon uses currentColor instead of hardcoded hex fill or stroke",
            "Icon stroke tracks adjacent text weight from 1.5px (400) to 2.5px (700)",
            "Icon-only buttons use sentence-case verb-first action labels, never glyph names",
            "Contextual icon swaps ease with cubic-bezier(0.2, 0, 0, 1) over 300ms",
        ],
    }
}

/// Returns the recommended stroke width in pixels for a given CSS font weight.
pub fn stroke_px_for_font_weight(weight: u16) -> f32 {
    if weight >= 700 {
        ICON_STROKE_BOLD_PX
    } else if weight <= 400 {
        ICON_STROKE_REGULAR_PX
    } else {
        let t = f32::from(weight - 400) / 300.0;
        ICON_STROKE_REGULAR_PX + t * (ICON_STROKE_BOLD_PX - ICON_STROKE_REGULAR_PX)
    }
}

/// Returns `true` if `ch` is a Unicode emoji, pictograph, or emoji-presentation control.
pub fn is_disallowed_ui_emoji(ch: char) -> bool {
    let cp = ch as u32;
    matches!(
        cp,
        0x231A..=0x231B
            | 0x23E9..=0x23FA
            | 0x25FD..=0x25FE
            | 0x2600..=0x27BF
            | 0x2B50
            | 0x2B55
            | 0xFE0F
            | 0x1F000..=0x1F02F
            | 0x1F0A0..=0x1F0FF
            | 0x1F100..=0x1F2FF
            | 0x1F300..=0x1FAFF
    )
}

/// Returns `true` if `text` contains any disallowed UI emoji code point.
pub fn contains_disallowed_emoji(text: &str) -> bool {
    text.chars().any(is_disallowed_ui_emoji)
}

/// Strips disallowed emoji code points from `text` and normalizes spacing.
pub fn strip_disallowed_emojis(text: &str) -> String {
    let filtered: String = text
        .chars()
        .filter(|&ch| !is_disallowed_ui_emoji(ch))
        .collect();
    filtered.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Audits a UI label or copy string for emojis and glyph-describing accessible names.
pub fn audit_ui_string(label: &str, is_icon_only: bool) -> Vec<AuditFinding> {
    let mut findings = Vec::new();

    if contains_disallowed_emoji(label) {
        findings.push(AuditFinding {
            rule_id: "ux_15_zero_emoji_vector_audit",
            severity: AuditSeverity::Error,
            message: "UI string contains a raw Unicode emoji.",
            remediation:
                "Remove the emoji and pair plain sentence-case copy with a currentColor SVG icon.",
        });
    }

    if is_icon_only {
        let lower = label.trim().to_ascii_lowercase();
        let describes_glyph = lower.is_empty()
            || lower == "x"
            || lower.contains("icon")
            || lower.contains("icono")
            || lower.contains("emoji")
            || lower.contains("glyph")
            || lower == "trash";
        if describes_glyph {
            findings.push(AuditFinding {
                rule_id: "ux_15_zero_emoji_vector_audit",
                severity: AuditSeverity::Warning,
                message: "Icon-only accessible label describes the glyph or is empty.",
                remediation:
                    "Use a sentence-case verb-first action label such as \"Eliminar elemento\".",
            });
        }
    }

    findings
}

/// Audits inline SVG markup for hardcoded hex colors, `transition: all`, or raw emojis.
pub fn audit_svg_markup(svg: &str) -> Vec<AuditFinding> {
    let mut findings = Vec::new();
    let lower = svg.to_ascii_lowercase();

    if contains_disallowed_emoji(svg) {
        findings.push(AuditFinding {
            rule_id: "ux_15_zero_emoji_vector_audit",
            severity: AuditSeverity::Error,
            message: "SVG markup contains a raw Unicode emoji.",
            remediation: "Replace emoji text nodes with vector path geometry.",
        });
    }

    if lower.contains("fill=\"#") || lower.contains("stroke=\"#") {
        findings.push(AuditFinding {
            rule_id: "ux_15_zero_emoji_vector_audit",
            severity: AuditSeverity::Error,
            message: "SVG icon uses a hardcoded hex color instead of currentColor.",
            remediation: "Set fill or stroke to currentColor so CSS drives hover, selected, and disabled states.",
        });
    }

    if lower.contains("transition: all") || lower.contains("transition-all") {
        findings.push(AuditFinding {
            rule_id: "ux_15_zero_emoji_vector_audit",
            severity: AuditSeverity::Warning,
            message: "SVG or icon button transitions all properties.",
            remediation: "Transition only explicit properties such as opacity, scale, and filter.",
        });
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_exposes_exact_ux_polish_tokens() {
        let rule = spec();
        assert_eq!(rule.id, "ux_15_zero_emoji_vector_audit");
        assert!(rule
            .css_tokens
            .contains(&("--sl-icon-color", "currentColor")));
        assert!(rule
            .css_tokens
            .contains(&("--sl-icon-ease", "cubic-bezier(0.2, 0, 0, 1)")));
        assert!(rule.css_tokens.contains(&("--sl-icon-press-scale", "0.96")));
        assert!((stroke_px_for_font_weight(400) - 1.5).abs() < f32::EPSILON);
        assert!((stroke_px_for_font_weight(700) - 2.5).abs() < f32::EPSILON);
        assert!(VECTOR_ICON_CSS.contains("cubic-bezier(0.2, 0, 0, 1)"));
        assert!(VECTOR_ICON_CSS.contains("scale: 0.96;"));
        assert_eq!(SPANISH_ICON_LABELS[0], ("search", "Buscar en la web"));
    }

    #[test]
    fn emoji_detection_and_stripping_catch_pictographs() {
        let with_emoji = "\u{1F50D} Buscar en la web \u{2705}".to_string();
        assert!(contains_disallowed_emoji(&with_emoji));
        assert_eq!(strip_disallowed_emojis(&with_emoji), "Buscar en la web");
        assert!(!contains_disallowed_emoji("Buscar en la web"));
    }

    #[test]
    fn audit_ui_string_flags_emojis_and_glyph_labels() {
        let emoji_label = "\u{26A0}\u{FE0F} Error de red".to_string();
        let emoji_findings = audit_ui_string(&emoji_label, false);
        assert_eq!(emoji_findings.len(), 1);
        assert_eq!(emoji_findings[0].severity, AuditSeverity::Error);

        let glyph_findings = audit_ui_string("Trash icon", true);
        assert_eq!(glyph_findings.len(), 1);
        assert_eq!(glyph_findings[0].severity, AuditSeverity::Warning);

        let clean_findings = audit_ui_string("Eliminar elemento", true);
        assert!(clean_findings.is_empty());
    }

    #[test]
    fn audit_svg_markup_enforces_current_color_and_explicit_transitions() {
        let bad_svg = r##"<svg fill="#ff0000" class="transition-all"><path d="M0 0"/></svg>"##;
        let findings = audit_svg_markup(bad_svg);
        assert_eq!(findings.len(), 2);

        let good_svg = r#"<svg stroke="currentColor" fill="none"><path d="M0 0"/></svg>"#;
        assert!(audit_svg_markup(good_svg).is_empty());
    }
}
