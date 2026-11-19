//! PDF export options, validation, page range resolution, and layout calculation.

use serde::{Deserialize, Serialize};
use thiserror::Error;

const MM_PER_INCH: f64 = 25.4;
const POINTS_PER_INCH: f64 = 72.0;
const MIN_SCALE: f64 = 0.1;
const MAX_SCALE: f64 = 2.0;
const MIN_PAPER_MM: f64 = 25.0;
const MAX_PAPER_MM: f64 = 5000.0;

/// Errors returned when validating or resolving PDF export options.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum PdfExportError {
    #[error("invalid custom paper dimensions: {width_mm}mm x {height_mm}mm")]
    InvalidPaperDimensions { width_mm: f64, height_mm: f64 },

    #[error("scale {scale} is out of valid range ({MIN_SCALE}..={MAX_SCALE})")]
    InvalidScale { scale: f64 },

    #[error("margin '{side}' cannot be negative or non-finite: {value}mm")]
    NegativeMargin { side: &'static str, value: f64 },

    #[error(
        "margins leave no printable area (width: {content_width_mm}mm, height: {content_height_mm}mm)"
    )]
    MarginsExceedPage {
        content_width_mm: f64,
        content_height_mm: f64,
    },

    #[error("invalid page range expression: {0}")]
    InvalidPageRange(String),

    #[error("page {page} exceeds total page count {total_pages}")]
    PageOutOfBounds { page: u32, total_pages: u32 },

    #[error("invalid output filename: {0}")]
    InvalidOutputFilename(String),
}

/// Standard and custom paper sizes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[derive(Default)]
pub enum PaperSize {
    #[default]
    A4,
    A3,
    A5,
    Letter,
    Legal,
    Tabloid,
    Custom {
        width_mm: f64,
        height_mm: f64,
    },
}

impl PaperSize {
    /// Returns the portrait `(width, height)` in millimeters.
    pub fn dimensions_mm(&self) -> (f64, f64) {
        match *self {
            Self::A4 => (210.0, 297.0),
            Self::A3 => (297.0, 420.0),
            Self::A5 => (148.0, 210.0),
            Self::Letter => (215.9, 279.4),
            Self::Legal => (215.9, 355.6),
            Self::Tabloid => (279.4, 431.8),
            Self::Custom {
                width_mm,
                height_mm,
            } => (width_mm, height_mm),
        }
    }

    /// Returns the portrait `(width, height)` in inches.
    pub fn dimensions_inches(&self) -> (f64, f64) {
        let (w, h) = self.dimensions_mm();
        (w / MM_PER_INCH, h / MM_PER_INCH)
    }

    /// Returns the portrait `(width, height)` in PostScript points (72 pt/inch).
    pub fn dimensions_points(&self) -> (f64, f64) {
        let (w_in, h_in) = self.dimensions_inches();
        (w_in * POINTS_PER_INCH, h_in * POINTS_PER_INCH)
    }
}

/// Page orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    #[default]
    Portrait,
    Landscape,
}

/// Page margins in millimeters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PageMarginsMm {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl Default for PageMarginsMm {
    fn default() -> Self {
        Self::uniform(10.0)
    }
}

impl PageMarginsMm {
    /// Creates uniform margins on all four sides.
    pub fn uniform(mm: f64) -> Self {
        Self {
            top: mm,
            right: mm,
            bottom: mm,
            left: mm,
        }
    }

    /// Zero margins on all sides.
    pub fn none() -> Self {
        Self::uniform(0.0)
    }

    /// Narrow margins (5mm).
    pub fn narrow() -> Self {
        Self::uniform(5.0)
    }

    /// Wide margins (25.4mm / 1 inch).
    pub fn wide() -> Self {
        Self::uniform(25.4)
    }
}

/// Color rendering mode for PDF output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PdfColorMode {
    #[default]
    Color,
    Grayscale,
}

/// Specification of which pages to export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum PageRangeSpec {
    #[default]
    All,
    Range {
        start: u32,
        end: u32,
    },
    Pages(Vec<u32>),
    Expression(String),
}

impl PageRangeSpec {
    /// Resolves the page range into a sorted, deduplicated list of 1-based page numbers.
    pub fn resolve(&self, total_pages: u32) -> Result<Vec<u32>, PdfExportError> {
        if total_pages == 0 {
            return Err(PdfExportError::InvalidPageRange(
                "document has zero pages".to_string(),
            ));
        }

        let mut pages = match self {
            Self::All => (1..=total_pages).collect::<Vec<_>>(),
            Self::Range { start, end } => {
                if *start == 0 || *start > *end {
                    return Err(PdfExportError::InvalidPageRange(format!("{start}-{end}")));
                }
                if *end > total_pages {
                    return Err(PdfExportError::PageOutOfBounds {
                        page: *end,
                        total_pages,
                    });
                }
                (*start..=*end).collect()
            }
            Self::Pages(list) => {
                if list.is_empty() {
                    return Err(PdfExportError::InvalidPageRange(
                        "explicit page list is empty".to_string(),
                    ));
                }
                for &p in list {
                    if p == 0 {
                        return Err(PdfExportError::InvalidPageRange(
                            "page numbers are 1-based; 0 is invalid".to_string(),
                        ));
                    }
                    if p > total_pages {
                        return Err(PdfExportError::PageOutOfBounds {
                            page: p,
                            total_pages,
                        });
                    }
                }
                list.clone()
            }
            Self::Expression(expr) => parse_page_expression(expr, total_pages)?,
        };

        pages.sort_unstable();
        pages.dedup();
        Ok(pages)
    }
}

fn parse_page_expression(expr: &str, total_pages: u32) -> Result<Vec<u32>, PdfExportError> {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return Err(PdfExportError::InvalidPageRange(
            "empty page expression".to_string(),
        ));
    }

    let mut result = Vec::new();
    for part in trimmed.split(',') {
        let token = part.trim();
        if token.is_empty() {
            return Err(PdfExportError::InvalidPageRange(expr.to_string()));
        }
        if let Some((a, b)) = token.split_once('-') {
            let start: u32 = a
                .trim()
                .parse()
                .map_err(|_| PdfExportError::InvalidPageRange(token.to_string()))?;
            let end: u32 = b
                .trim()
                .parse()
                .map_err(|_| PdfExportError::InvalidPageRange(token.to_string()))?;
            if start == 0 || start > end {
                return Err(PdfExportError::InvalidPageRange(token.to_string()));
            }
            if end > total_pages {
                return Err(PdfExportError::PageOutOfBounds {
                    page: end,
                    total_pages,
                });
            }
            result.extend(start..=end);
        } else {
            let page: u32 = token
                .parse()
                .map_err(|_| PdfExportError::InvalidPageRange(token.to_string()))?;
            if page == 0 {
                return Err(PdfExportError::InvalidPageRange(
                    "page numbers are 1-based; 0 is invalid".to_string(),
                ));
            }
            if page > total_pages {
                return Err(PdfExportError::PageOutOfBounds { page, total_pages });
            }
            result.push(page);
        }
    }

    Ok(result)
}

/// Template strings for PDF running headers or footers.
///
/// Supported placeholders: `{title}`, `{url}`, `{date}`, `{page}`, `{pages}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderFooterTemplate {
    pub left: String,
    pub center: String,
    pub right: String,
}

impl HeaderFooterTemplate {
    /// Default browser-style header: title on left, date on right.
    pub fn default_header() -> Self {
        Self {
            left: "{title}".to_string(),
            center: String::new(),
            right: "{date}".to_string(),
        }
    }

    /// Default browser-style footer: URL on left, page counter on right.
    pub fn default_footer() -> Self {
        Self {
            left: "{url}".to_string(),
            center: String::new(),
            right: "{page} / {pages}".to_string(),
        }
    }

    /// Renders all three slots by substituting template placeholders.
    pub fn render(&self, ctx: &HeaderFooterContext<'_>) -> RenderedHeaderFooter {
        RenderedHeaderFooter {
            left: expand_template(&self.left, ctx),
            center: expand_template(&self.center, ctx),
            right: expand_template(&self.right, ctx),
        }
    }
}

/// Context values substituted into header and footer templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaderFooterContext<'a> {
    pub title: &'a str,
    pub url: &'a str,
    pub date: &'a str,
    pub page: u32,
    pub total_pages: u32,
}

/// Rendered header or footer strings for a single page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedHeaderFooter {
    pub left: String,
    pub center: String,
    pub right: String,
}

fn expand_template(template: &str, ctx: &HeaderFooterContext<'_>) -> String {
    template
        .replace("{title}", ctx.title)
        .replace("{url}", ctx.url)
        .replace("{date}", ctx.date)
        .replace("{page}", &ctx.page.to_string())
        .replace("{pages}", &ctx.total_pages.to_string())
}

/// Named export presets for common workflows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PdfExportPreset {
    Standard,
    Compact,
    Presentation,
    PrintReady,
}

/// Complete configuration for exporting a web page or document to PDF.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PdfExportOptions {
    pub paper_size: PaperSize,
    pub orientation: Orientation,
    pub margins_mm: PageMarginsMm,
    pub scale: f64,
    pub page_range: PageRangeSpec,
    pub print_backgrounds: bool,
    pub display_header_footer: bool,
    pub header: HeaderFooterTemplate,
    pub footer: HeaderFooterTemplate,
    pub color_mode: PdfColorMode,
    pub prefer_css_page_size: bool,
    pub generate_tagged_pdf: bool,
    pub generate_outline: bool,
    pub document_title: Option<String>,
    pub author: Option<String>,
    pub output_filename: Option<String>,
}

impl Default for PdfExportOptions {
    fn default() -> Self {
        Self {
            paper_size: PaperSize::A4,
            orientation: Orientation::Portrait,
            margins_mm: PageMarginsMm::default(),
            scale: 1.0,
            page_range: PageRangeSpec::All,
            print_backgrounds: true,
            display_header_footer: false,
            header: HeaderFooterTemplate::default_header(),
            footer: HeaderFooterTemplate::default_footer(),
            color_mode: PdfColorMode::Color,
            prefer_css_page_size: false,
            generate_tagged_pdf: true,
            generate_outline: true,
            document_title: None,
            author: None,
            output_filename: None,
        }
    }
}

/// Computed physical dimensions for a validated `PdfExportOptions`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedPdfLayout {
    pub page_width_mm: f64,
    pub page_height_mm: f64,
    pub printable_width_mm: f64,
    pub printable_height_mm: f64,
    pub page_width_pt: f64,
    pub page_height_pt: f64,
}

impl PdfExportOptions {
    /// Builds options from a named preset.
    pub fn from_preset(preset: PdfExportPreset) -> Self {
        match preset {
            PdfExportPreset::Standard => Self::default(),
            PdfExportPreset::Compact => Self {
                margins_mm: PageMarginsMm::narrow(),
                scale: 0.85,
                print_backgrounds: false,
                ..Self::default()
            },
            PdfExportPreset::Presentation => Self {
                orientation: Orientation::Landscape,
                margins_mm: PageMarginsMm::none(),
                print_backgrounds: true,
                display_header_footer: false,
                ..Self::default()
            },
            PdfExportPreset::PrintReady => Self {
                margins_mm: PageMarginsMm::wide(),
                display_header_footer: true,
                color_mode: PdfColorMode::Grayscale,
                prefer_css_page_size: true,
                ..Self::default()
            },
        }
    }

    /// Validates all numeric bounds, margins, and optional output filename.
    pub fn validate(&self) -> Result<(), PdfExportError> {
        let (raw_w, raw_h) = self.paper_size.dimensions_mm();
        if !raw_w.is_finite()
            || !raw_h.is_finite()
            || raw_w < MIN_PAPER_MM
            || raw_h < MIN_PAPER_MM
            || raw_w > MAX_PAPER_MM
            || raw_h > MAX_PAPER_MM
        {
            return Err(PdfExportError::InvalidPaperDimensions {
                width_mm: raw_w,
                height_mm: raw_h,
            });
        }

        if !self.scale.is_finite() || self.scale < MIN_SCALE || self.scale > MAX_SCALE {
            return Err(PdfExportError::InvalidScale { scale: self.scale });
        }

        let sides = [
            ("top", self.margins_mm.top),
            ("right", self.margins_mm.right),
            ("bottom", self.margins_mm.bottom),
            ("left", self.margins_mm.left),
        ];
        for (side, value) in sides {
            if !value.is_finite() || value < 0.0 {
                return Err(PdfExportError::NegativeMargin { side, value });
            }
        }

        let (page_w, page_h) = self.effective_dimensions_mm()?;
        let content_w = page_w - self.margins_mm.left - self.margins_mm.right;
        let content_h = page_h - self.margins_mm.top - self.margins_mm.bottom;
        if content_w <= 0.0 || content_h <= 0.0 {
            return Err(PdfExportError::MarginsExceedPage {
                content_width_mm: content_w,
                content_height_mm: content_h,
            });
        }

        if let Some(ref name) = self.output_filename {
            if name.trim().is_empty() || name.contains('/') || name.contains('\\') {
                return Err(PdfExportError::InvalidOutputFilename(name.clone()));
            }
        }

        Ok(())
    }

    /// Returns `(width_mm, height_mm)` after applying `orientation`.
    pub fn effective_dimensions_mm(&self) -> Result<(f64, f64), PdfExportError> {
        let (w, h) = self.paper_size.dimensions_mm();
        if !w.is_finite() || !h.is_finite() || w < MIN_PAPER_MM || h < MIN_PAPER_MM {
            return Err(PdfExportError::InvalidPaperDimensions {
                width_mm: w,
                height_mm: h,
            });
        }
        Ok(match self.orientation {
            Orientation::Portrait => (w, h),
            Orientation::Landscape => (h, w),
        })
    }

    /// Validates options and computes the resolved physical layout.
    pub fn resolve_layout(&self) -> Result<ResolvedPdfLayout, PdfExportError> {
        self.validate()?;
        let (page_width_mm, page_height_mm) = self.effective_dimensions_mm()?;
        let printable_width_mm = page_width_mm - self.margins_mm.left - self.margins_mm.right;
        let printable_height_mm = page_height_mm - self.margins_mm.top - self.margins_mm.bottom;
        Ok(ResolvedPdfLayout {
            page_width_mm,
            page_height_mm,
            printable_width_mm,
            printable_height_mm,
            page_width_pt: (page_width_mm / MM_PER_INCH) * POINTS_PER_INCH,
            page_height_pt: (page_height_mm / MM_PER_INCH) * POINTS_PER_INCH,
        })
    }

    /// Resolves `page_range` against `total_pages`.
    pub fn resolve_pages(&self, total_pages: u32) -> Result<Vec<u32>, PdfExportError> {
        self.page_range.resolve(total_pages)
    }

    /// Produces a safe `.pdf` filename from `output_filename` or `fallback_title`.
    pub fn sanitize_filename(&self, fallback_title: &str) -> String {
        let raw = self
            .output_filename
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(fallback_title);

        let cleaned: String = raw
            .trim()
            .chars()
            .map(|c| match c {
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                c if c.is_control() => '_',
                c => c,
            })
            .collect();

        let base = if cleaned.is_empty() {
            "document".to_string()
        } else {
            cleaned
        };

        if base.to_ascii_lowercase().ends_with(".pdf") {
            base
        } else {
            format!("{base}.pdf")
        }
    }

    /// Serializes options to JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserializes options from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_layout_and_json_roundtrip() {
        let opts = PdfExportOptions::default();
        let layout = opts
            .resolve_layout()
            .expect("default options should be valid");
        assert!((layout.page_width_mm - 210.0).abs() < 1e-6);
        assert!((layout.page_height_mm - 297.0).abs() < 1e-6);
        assert!((layout.printable_width_mm - 190.0).abs() < 1e-6);
        assert!((layout.printable_height_mm - 277.0).abs() < 1e-6);

        let json = opts.to_json().expect("serialize");
        let decoded = PdfExportOptions::from_json(&json).expect("deserialize");
        assert_eq!(opts, decoded);
    }

    #[test]
    fn landscape_swaps_dimensions_and_presets_validate() {
        for preset in [
            PdfExportPreset::Standard,
            PdfExportPreset::Compact,
            PdfExportPreset::Presentation,
            PdfExportPreset::PrintReady,
        ] {
            let opts = PdfExportOptions::from_preset(preset);
            assert!(
                opts.validate().is_ok(),
                "preset {:?} should validate",
                preset
            );
        }

        let pres = PdfExportOptions::from_preset(PdfExportPreset::Presentation);
        let layout = pres.resolve_layout().expect("presentation layout");
        assert!((layout.page_width_mm - 297.0).abs() < 1e-6);
        assert!((layout.page_height_mm - 210.0).abs() < 1e-6);
    }

    #[test]
    fn validation_catches_invalid_scale_margins_and_paper() {
        let bad_scale = PdfExportOptions {
            scale: 2.5,
            ..PdfExportOptions::default()
        };
        assert!(matches!(
            bad_scale.validate(),
            Err(PdfExportError::InvalidScale { .. })
        ));

        let neg_margin = PdfExportOptions {
            margins_mm: PageMarginsMm {
                top: -1.0,
                ..PageMarginsMm::default()
            },
            ..PdfExportOptions::default()
        };
        assert!(matches!(
            neg_margin.validate(),
            Err(PdfExportError::NegativeMargin { side: "top", .. })
        ));

        let huge_margins = PdfExportOptions {
            margins_mm: PageMarginsMm::uniform(120.0),
            ..PdfExportOptions::default()
        };
        assert!(matches!(
            huge_margins.validate(),
            Err(PdfExportError::MarginsExceedPage { .. })
        ));

        let tiny_paper = PdfExportOptions {
            paper_size: PaperSize::Custom {
                width_mm: 10.0,
                height_mm: 10.0,
            },
            ..PdfExportOptions::default()
        };
        assert!(matches!(
            tiny_paper.validate(),
            Err(PdfExportError::InvalidPaperDimensions { .. })
        ));
    }

    #[test]
    fn page_range_resolution_and_errors() {
        let expr = PageRangeSpec::Expression("5, 1-3, 2, 7-8".to_string());
        assert_eq!(expr.resolve(10).expect("valid"), vec![1, 2, 3, 5, 7, 8]);

        let out_of_bounds = PageRangeSpec::Expression("1-12".to_string());
        assert!(matches!(
            out_of_bounds.resolve(10),
            Err(PdfExportError::PageOutOfBounds {
                page: 12,
                total_pages: 10
            })
        ));

        let inverted = PageRangeSpec::Range { start: 5, end: 2 };
        assert!(matches!(
            inverted.resolve(10),
            Err(PdfExportError::InvalidPageRange(_))
        ));

        let zero_page = PageRangeSpec::Pages(vec![0, 1]);
        assert!(matches!(
            zero_page.resolve(10),
            Err(PdfExportError::InvalidPageRange(_))
        ));
    }

    #[test]
    fn header_footer_rendering_and_filename_sanitization() {
        let opts = PdfExportOptions::from_preset(PdfExportPreset::PrintReady);
        let ctx = HeaderFooterContext {
            title: "Search Linux Docs",
            url: "https://example.org/docs",
            date: "2026-11-19",
            page: 3,
            total_pages: 12,
        };
        let rendered_header = opts.header.render(&ctx);
        assert_eq!(rendered_header.left, "Search Linux Docs");
        assert_eq!(rendered_header.right, "2026-11-19");

        let rendered_footer = opts.footer.render(&ctx);
        assert_eq!(rendered_footer.left, "https://example.org/docs");
        assert_eq!(rendered_footer.right, "3 / 12");

        assert_eq!(
            opts.sanitize_filename("Report: Q4/2026?"),
            "Report_ Q4_2026_.pdf"
        );
        assert_eq!(opts.sanitize_filename("already.PDF"), "already.PDF");
    }
}
