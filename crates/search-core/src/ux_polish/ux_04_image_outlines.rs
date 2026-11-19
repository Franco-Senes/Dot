//! UX Polish Rule 04: Image Outlines.
//!
//! Content images receive a `1px` inset outline (`outline-offset: -1px`) using
//! pure black at 10% alpha in light mode (`oklch(0 0 0 / 0.1)`) and pure white
//! at 10% alpha in dark mode (`oklch(1 0 0 / 0.1)`). Transparent artwork such
//! as logos and illustrations are excluded.

/// Pure black at 10% alpha in OKLCH for light mode image outlines.
pub const LIGHT_IMAGE_OUTLINE: &str = "oklch(0 0 0 / 0.1)";

/// Pure white at 10% alpha in OKLCH for dark mode image outlines.
pub const DARK_IMAGE_OUTLINE: &str = "oklch(1 0 0 / 0.1)";

/// Exact outline stroke width in pixels.
pub const OUTLINE_WIDTH_PX: u8 = 1;

/// Exact outline offset in pixels (negative insets the outline inside the edge).
pub const OUTLINE_OFFSET_PX: i8 = -1;

/// Static CSS fragment implementing the image outline tokens and class.
pub const IMAGE_OUTLINE_CSS: &str = concat!(
    ":root {\n",
    "  --image-outline: oklch(0 0 0 / 0.1);\n",
    "}\n\n",
    "@media (prefers-color-scheme: dark) {\n",
    "  :root {\n",
    "    --image-outline: oklch(1 0 0 / 0.1);\n",
    "  }\n",
    "}\n\n",
    ".content-image {\n",
    "  outline: 1px solid var(--image-outline);\n",
    "  outline-offset: -1px;\n",
    "}\n",
);

/// Classification of an image element to determine whether an inset outline applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    /// Photographic or raster content image (e.g., search result preview, article photo).
    ContentPhoto,
    /// Raster thumbnail or card preview image.
    Thumbnail,
    /// Transparent artwork that must not receive a rectangular outline.
    TransparentArtwork,
    /// Brand or site logo (typically transparent SVG/PNG).
    Logo,
    /// Decorative or editorial illustration with transparent background.
    Illustration,
}

/// Specification metadata and exact values for UX rule `ux_04_image_outlines`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UxRuleSpec {
    /// Unique identifier of the UX polish slice.
    pub id: &'static str,
    /// Concise English title of the rule.
    pub title: &'static str,
    /// Skill category owning this rule.
    pub category: &'static str,
    /// Light mode OKLCH outline color.
    pub light_outline_color: &'static str,
    /// Dark mode OKLCH outline color.
    pub dark_outline_color: &'static str,
    /// Outline width in pixels.
    pub outline_width_px: u8,
    /// Outline offset in pixels (`-1` for 1px inset).
    pub outline_offset_px: i8,
    /// CSS fragment implementing the rule.
    pub css_fragment: &'static str,
    /// Spanish sentence-case verb-first guidance for UI/tooling surfaces.
    pub guidance_es: &'static str,
}

/// Returns the canonical specification for `ux_04_image_outlines`.
pub fn spec() -> UxRuleSpec {
    UxRuleSpec {
        id: "ux_04_image_outlines",
        title: "Outline images in pure black or white",
        category: "better-ui",
        light_outline_color: LIGHT_IMAGE_OUTLINE,
        dark_outline_color: DARK_IMAGE_OUTLINE,
        outline_width_px: OUTLINE_WIDTH_PX,
        outline_offset_px: OUTLINE_OFFSET_PX,
        css_fragment: IMAGE_OUTLINE_CSS,
        guidance_es:
            "Aplicar contorno interior de 1px en negro o blanco puro a las imagenes de contenido",
    }
}

/// Returns `true` if the given `ImageKind` should receive the `1px` inset outline.
///
/// Transparent artwork (logos, illustrations, and explicit transparent assets)
/// is excluded to avoid drawing a box around non-rectangular shapes.
pub fn should_apply_outline(kind: ImageKind) -> bool {
    match kind {
        ImageKind::ContentPhoto | ImageKind::Thumbnail => true,
        ImageKind::TransparentArtwork | ImageKind::Logo | ImageKind::Illustration => false,
    }
}

/// Validates that an image outline color is strictly pure black or pure white
/// in OKLCH at 10% alpha, rejecting tinted neutrals, hex near-blacks, or accents.
pub fn validate_outline_color(color: &str) -> Result<(), &'static str> {
    let trimmed = color.trim();
    if trimmed == LIGHT_IMAGE_OUTLINE || trimmed == DARK_IMAGE_OUTLINE {
        Ok(())
    } else if trimmed.starts_with('#') {
        Err("Usar oklch(0 0 0 / 0.1) o oklch(1 0 0 / 0.1) en lugar de valores hexadecimales")
    } else {
        Err("Usar negro o blanco puro en OKLCH sin tinte de color")
    }
}

/// Renders the complete CSS rule string for light and dark image outlines.
pub fn render_css_rules() -> String {
    format!(
        ":root {{\n  --image-outline: {};\n}}\n\n@media (prefers-color-scheme: dark) {{\n  :root {{\n    --image-outline: {};\n  }}\n}}\n\n.content-image {{\n  outline: {}px solid var(--image-outline);\n  outline-offset: {}px;\n}}\n",
        LIGHT_IMAGE_OUTLINE, DARK_IMAGE_OUTLINE, OUTLINE_WIDTH_PX, OUTLINE_OFFSET_PX
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_exposes_exact_oklch_and_geometry_values() {
        let s = spec();
        assert_eq!(s.id, "ux_04_image_outlines");
        assert_eq!(s.light_outline_color, "oklch(0 0 0 / 0.1)");
        assert_eq!(s.dark_outline_color, "oklch(1 0 0 / 0.1)");
        assert_eq!(s.outline_width_px, 1);
        assert_eq!(s.outline_offset_px, -1);
        assert_eq!(render_css_rules(), s.css_fragment);
        assert!(s
            .css_fragment
            .contains("outline: 1px solid var(--image-outline);"));
        assert!(s.css_fragment.contains("outline-offset: -1px;"));
    }

    #[test]
    fn skips_transparent_artwork_and_applies_to_content_images() {
        assert!(should_apply_outline(ImageKind::ContentPhoto));
        assert!(should_apply_outline(ImageKind::Thumbnail));
        assert!(!should_apply_outline(ImageKind::TransparentArtwork));
        assert!(!should_apply_outline(ImageKind::Logo));
        assert!(!should_apply_outline(ImageKind::Illustration));
    }

    #[test]
    fn rejects_tinted_neutrals_and_hex_colors() {
        assert!(validate_outline_color("oklch(0 0 0 / 0.1)").is_ok());
        assert!(validate_outline_color("oklch(1 0 0 / 0.1)").is_ok());
        assert!(validate_outline_color("#111827").is_err());
        assert!(validate_outline_color("outline-slate-900/10").is_err());
        assert!(validate_outline_color("oklch(0.2 0.02 250 / 0.1)").is_err());
    }
}
