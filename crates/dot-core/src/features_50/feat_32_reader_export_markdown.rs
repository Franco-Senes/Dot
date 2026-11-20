//! Reader Mode Markdown export: converts extracted reader HTML and article metadata
//! into clean CommonMark/GFM Markdown with optional YAML/TOML frontmatter, safe URL
//! resolution, reference-style links, fenced code blocks, and GFM tables.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use thiserror::Error;
use url::Url;

/// Default maximum HTML input size (4 MiB).
pub const DEFAULT_MAX_INPUT_BYTES: usize = 4 * 1024 * 1024;

/// Maximum heading offset allowed when shifting heading levels.
pub const MAX_HEADING_OFFSET: u8 = 4;

/// Maximum length of the generated filename stem (excluding `.md`).
const MAX_FILENAME_STEM_LEN: usize = 80;

/// Errors returned during validation or Markdown export.
#[derive(Debug, Error, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkdownExportError {
    #[error("article HTML content is empty")]
    EmptyContent,
    #[error("article title is required when frontmatter or title heading is enabled")]
    TitleRequired,
    #[error("invalid base URL '{url}': {reason}")]
    InvalidBaseUrl { url: String, reason: String },
    #[error("invalid heading level offset {0}; must be between 0 and {MAX_HEADING_OFFSET}")]
    InvalidHeadingOffset(u8),
    #[error("input HTML exceeds maximum allowed size ({len} bytes > {max} bytes)")]
    ContentTooLarge { len: usize, max: usize },
}

/// Frontmatter metadata format prepended to the exported Markdown document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrontmatterFormat {
    None,
    #[default]
    Yaml,
    Toml,
}

/// How hyperlinks are rendered in the exported Markdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkStyle {
    #[default]
    Inline,
    Reference,
    Strip,
}

/// Configuration options controlling Markdown export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownExportOptions {
    #[serde(default)]
    pub frontmatter: FrontmatterFormat,
    #[serde(default = "default_true")]
    pub include_title_heading: bool,
    #[serde(default)]
    pub heading_level_offset: u8,
    #[serde(default)]
    pub link_style: LinkStyle,
    #[serde(default = "default_true")]
    pub include_images: bool,
    #[serde(default = "default_true")]
    pub resolve_relative_urls: bool,
    #[serde(default = "default_true")]
    pub preserve_code_languages: bool,
    #[serde(default = "default_true")]
    pub include_reading_stats: bool,
    #[serde(default = "default_max_input_bytes")]
    pub max_input_bytes: usize,
}

fn default_true() -> bool {
    true
}

fn default_max_input_bytes() -> usize {
    DEFAULT_MAX_INPUT_BYTES
}

impl Default for MarkdownExportOptions {
    fn default() -> Self {
        Self {
            frontmatter: FrontmatterFormat::Yaml,
            include_title_heading: true,
            heading_level_offset: 0,
            link_style: LinkStyle::Inline,
            include_images: true,
            resolve_relative_urls: true,
            preserve_code_languages: true,
            include_reading_stats: true,
            max_input_bytes: DEFAULT_MAX_INPUT_BYTES,
        }
    }
}

impl MarkdownExportOptions {
    /// Validates export options before processing an article.
    pub fn validate(&self) -> Result<(), MarkdownExportError> {
        if self.heading_level_offset > MAX_HEADING_OFFSET {
            return Err(MarkdownExportError::InvalidHeadingOffset(
                self.heading_level_offset,
            ));
        }
        Ok(())
    }
}

/// Input article payload to export to Markdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArticleInput {
    pub title: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub site_name: Option<String>,
    #[serde(default)]
    pub byline: Option<String>,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub excerpt: Option<String>,
    pub content_html: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl ArticleInput {
    /// Validates the article input against the provided export options.
    pub fn validate(&self, options: &MarkdownExportOptions) -> Result<(), MarkdownExportError> {
        options.validate()?;

        if self.content_html.len() > options.max_input_bytes {
            return Err(MarkdownExportError::ContentTooLarge {
                len: self.content_html.len(),
                max: options.max_input_bytes,
            });
        }

        if self.content_html.trim().is_empty() {
            return Err(MarkdownExportError::EmptyContent);
        }

        if (options.frontmatter != FrontmatterFormat::None || options.include_title_heading)
            && self.title.trim().is_empty()
        {
            return Err(MarkdownExportError::TitleRequired);
        }

        if let Some(raw_url) = self.url.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Url::parse(raw_url).map_err(|err| MarkdownExportError::InvalidBaseUrl {
                url: raw_url.to_string(),
                reason: err.to_string(),
            })?;
        }

        Ok(())
    }
}

/// Result of exporting a reader article to Markdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownExportResult {
    pub markdown: String,
    pub suggested_filename: String,
    pub word_count: usize,
    pub estimated_minutes: u32,
    pub link_count: usize,
    pub image_count: usize,
    pub heading_count: usize,
}

/// Converts an `ArticleInput` to Markdown using the given `MarkdownExportOptions`.
pub fn export_article_to_markdown(
    article: &ArticleInput,
    options: &MarkdownExportOptions,
) -> Result<MarkdownExportResult, MarkdownExportError> {
    article.validate(options)?;

    let base_url = if options.resolve_relative_urls {
        article
            .url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .and_then(|s| Url::parse(s).ok())
    } else {
        None
    };

    let mut converter = HtmlToMarkdown::new(options, base_url);
    converter.process(&article.content_html);
    let body = converter.finish_body();

    if body.trim().is_empty() {
        return Err(MarkdownExportError::EmptyContent);
    }

    let word_count = count_words(&body);
    let estimated_minutes = ((word_count as f64) / 220.0).ceil().max(1.0) as u32;

    let mut doc = String::new();
    match options.frontmatter {
        FrontmatterFormat::None => {}
        FrontmatterFormat::Yaml => {
            write_yaml_frontmatter(&mut doc, article, word_count, estimated_minutes, options);
        }
        FrontmatterFormat::Toml => {
            write_toml_frontmatter(&mut doc, article, word_count, estimated_minutes, options);
        }
    }

    if options.include_title_heading {
        let level = (1u8.saturating_add(options.heading_level_offset)).min(6) as usize;
        for _ in 0..level {
            doc.push('#');
        }
        doc.push(' ');
        doc.push_str(article.title.trim());
        doc.push_str("\n\n");
    }

    doc.push_str(&body);
    if !doc.ends_with('\n') {
        doc.push('\n');
    }

    Ok(MarkdownExportResult {
        markdown: doc,
        suggested_filename: suggest_markdown_filename(&article.title),
        word_count,
        estimated_minutes,
        link_count: converter.link_count,
        image_count: converter.image_count,
        heading_count: converter.heading_count,
    })
}

/// Generates a filesystem-safe `.md` filename from an article title.
pub fn suggest_markdown_filename(title: &str) -> String {
    let mut stem = String::new();
    let mut prev_dash = false;

    for ch in title.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            stem.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !stem.is_empty() {
            stem.push('-');
            prev_dash = true;
        }
        if stem.len() >= MAX_FILENAME_STEM_LEN {
            break;
        }
    }

    while stem.ends_with('-') {
        stem.pop();
    }

    if stem.is_empty() {
        stem.push_str("article");
    }
    stem.push_str(".md");
    stem
}

fn count_words(text: &str) -> usize {
    text.split_whitespace().count()
}

fn write_yaml_frontmatter(
    out: &mut String,
    article: &ArticleInput,
    word_count: usize,
    estimated_minutes: u32,
    options: &MarkdownExportOptions,
) {
    out.push_str("---\n");
    let _ = writeln!(out, "title: {}", yaml_quote(article.title.trim()));
    if let Some(url) = non_empty(article.url.as_deref()) {
        let _ = writeln!(out, "source_url: {}", yaml_quote(url));
    }
    if let Some(site) = non_empty(article.site_name.as_deref()) {
        let _ = writeln!(out, "site_name: {}", yaml_quote(site));
    }
    if let Some(byline) = non_empty(article.byline.as_deref()) {
        let _ = writeln!(out, "author: {}", yaml_quote(byline));
    }
    if let Some(published) = non_empty(article.published_at.as_deref()) {
        let _ = writeln!(out, "published_at: {}", yaml_quote(published));
    }
    if let Some(excerpt) = non_empty(article.excerpt.as_deref()) {
        let _ = writeln!(out, "excerpt: {}", yaml_quote(excerpt));
    }
    if !article.tags.is_empty() {
        out.push_str("tags:\n");
        for tag in &article.tags {
            let trimmed = tag.trim();
            if !trimmed.is_empty() {
                let _ = writeln!(out, "  - {}", yaml_quote(trimmed));
            }
        }
    }
    if options.include_reading_stats {
        let _ = writeln!(out, "word_count: {word_count}");
        let _ = writeln!(out, "reading_time_minutes: {estimated_minutes}");
    }
    out.push_str("---\n\n");
}

fn write_toml_frontmatter(
    out: &mut String,
    article: &ArticleInput,
    word_count: usize,
    estimated_minutes: u32,
    options: &MarkdownExportOptions,
) {
    out.push_str("+++\n");
    let _ = writeln!(out, "title = {}", toml_quote(article.title.trim()));
    if let Some(url) = non_empty(article.url.as_deref()) {
        let _ = writeln!(out, "source_url = {}", toml_quote(url));
    }
    if let Some(site) = non_empty(article.site_name.as_deref()) {
        let _ = writeln!(out, "site_name = {}", toml_quote(site));
    }
    if let Some(byline) = non_empty(article.byline.as_deref()) {
        let _ = writeln!(out, "author = {}", toml_quote(byline));
    }
    if let Some(published) = non_empty(article.published_at.as_deref()) {
        let _ = writeln!(out, "published_at = {}", toml_quote(published));
    }
    if let Some(excerpt) = non_empty(article.excerpt.as_deref()) {
        let _ = writeln!(out, "excerpt = {}", toml_quote(excerpt));
    }
    let clean_tags: Vec<String> = article
        .tags
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .map(toml_quote)
        .collect();
    if !clean_tags.is_empty() {
        let _ = writeln!(out, "tags = [{}]", clean_tags.join(", "));
    }
    if options.include_reading_stats {
        let _ = writeln!(out, "word_count = {word_count}");
        let _ = writeln!(out, "reading_time_minutes = {estimated_minutes}");
    }
    out.push_str("+++\n\n");
}

fn non_empty(opt: Option<&str>) -> Option<&str> {
    opt.map(str::trim).filter(|s| !s.is_empty())
}

fn yaml_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn toml_quote(s: &str) -> String {
    yaml_quote(s)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListKind {
    Unordered,
    Ordered(usize),
}

#[derive(Debug, Clone)]
struct LinkContext {
    href: String,
    text_buf: String,
}

#[derive(Debug, Clone)]
struct TableState {
    rows: Vec<Vec<String>>,
    current_row: Vec<String>,
    current_cell: String,
    in_cell: bool,
}

impl TableState {
    fn new() -> Self {
        Self {
            rows: Vec::new(),
            current_row: Vec::new(),
            current_cell: String::new(),
            in_cell: false,
        }
    }
}

struct HtmlToMarkdown<'a> {
    options: &'a MarkdownExportOptions,
    base_url: Option<Url>,
    out: String,
    skip_depth: usize,
    skip_tag: Option<String>,
    in_pre: bool,
    pre_buf: String,
    pre_lang: Option<String>,
    in_inline_code: bool,
    inline_code_buf: String,
    blockquote_depth: usize,
    list_stack: Vec<ListKind>,
    active_link: Option<LinkContext>,
    ref_links: Vec<(usize, String)>,
    ref_map: BTreeMap<String, usize>,
    table: Option<TableState>,
    pending_heading_level: Option<u8>,
    link_count: usize,
    image_count: usize,
    heading_count: usize,
}

impl<'a> HtmlToMarkdown<'a> {
    fn new(options: &'a MarkdownExportOptions, base_url: Option<Url>) -> Self {
        Self {
            options,
            base_url,
            out: String::new(),
            skip_depth: 0,
            skip_tag: None,
            in_pre: false,
            pre_buf: String::new(),
            pre_lang: None,
            in_inline_code: false,
            inline_code_buf: String::new(),
            blockquote_depth: 0,
            list_stack: Vec::new(),
            active_link: None,
            ref_links: Vec::new(),
            ref_map: BTreeMap::new(),
            table: None,
            pending_heading_level: None,
            link_count: 0,
            image_count: 0,
            heading_count: 0,
        }
    }

    fn process(&mut self, html: &str) {
        let bytes = html.as_bytes();
        let mut i = 0;

        while i < bytes.len() {
            if bytes[i] == b'<' {
                if html[i..].starts_with("<!--") {
                    if let Some(end) = html[i + 4..].find("-->") {
                        i = i + 4 + end + 3;
                        continue;
                    } else {
                        break;
                    }
                }
                if let Some(close_rel) = find_tag_end(&html[i + 1..]) {
                    let tag_raw = &html[i + 1..i + 1 + close_rel];
                    self.handle_tag(tag_raw);
                    i = i + 1 + close_rel + 1;
                    continue;
                }
            }

            let next_lt = html[i..].find('<').map(|rel| i + rel).unwrap_or(html.len());
            let text_slice = &html[i..next_lt];
            self.handle_text(text_slice);
            i = next_lt;
        }
    }

    fn handle_tag(&mut self, raw: &str) {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('!') || trimmed.starts_with('?') {
            return;
        }

        let is_closing = trimmed.starts_with('/');
        let is_self_closing = !is_closing && trimmed.ends_with('/');
        let inner = trimmed.trim_start_matches('/').trim_end_matches('/').trim();

        let name_end = inner
            .find(|c: char| c.is_ascii_whitespace())
            .unwrap_or(inner.len());
        let tag_name = inner[..name_end].to_ascii_lowercase();
        let attrs_str = &inner[name_end..];

        if let Some(skip) = &self.skip_tag {
            if is_closing && &tag_name == skip {
                self.skip_depth = self.skip_depth.saturating_sub(1);
                if self.skip_depth == 0 {
                    self.skip_tag = None;
                }
            } else if !is_closing && !is_self_closing && &tag_name == skip {
                self.skip_depth += 1;
            }
            return;
        }

        if !is_closing
            && !is_self_closing
            && matches!(
                tag_name.as_str(),
                "script" | "style" | "noscript" | "iframe" | "svg"
            )
        {
            self.skip_tag = Some(tag_name);
            self.skip_depth = 1;
            return;
        }

        if is_closing {
            self.handle_close_tag(&tag_name);
        } else {
            self.handle_open_tag(&tag_name, attrs_str, is_self_closing);
        }
    }

    fn handle_open_tag(&mut self, tag: &str, attrs_str: &str, self_closing: bool) {
        match tag {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let base_level = tag[1..].parse::<u8>().unwrap_or(1);
                let level = (base_level.saturating_add(self.options.heading_level_offset)).min(6);
                self.ensure_blank_line();
                self.pending_heading_level = Some(level);
                self.heading_count += 1;
                for _ in 0..level {
                    self.push_output("#");
                }
                self.push_output(" ");
            }
            "p" | "div" | "section" | "article" | "header" | "footer" | "main" => {
                if self.table.is_none() && !self.in_pre {
                    self.ensure_blank_line();
                }
            }
            "br" => {
                if self.in_pre {
                    self.pre_buf.push('\n');
                } else if self.table.as_ref().is_some_and(|t| t.in_cell) {
                    self.push_output(" ");
                } else {
                    self.push_output("  \n");
                    self.write_blockquote_prefix_if_line_start();
                }
            }
            "hr" => {
                self.ensure_blank_line();
                self.push_output("---\n\n");
            }
            "blockquote" => {
                self.ensure_blank_line();
                self.blockquote_depth += 1;
                self.push_output("> ".repeat(self.blockquote_depth).as_str());
            }
            "pre" => {
                self.ensure_blank_line();
                self.in_pre = true;
                self.pre_buf.clear();
                self.pre_lang = extract_code_language(attrs_str);
            }
            "code" => {
                if self.in_pre {
                    if self.pre_lang.is_none() {
                        self.pre_lang = extract_code_language(attrs_str);
                    }
                } else {
                    self.in_inline_code = true;
                    self.inline_code_buf.clear();
                }
            }
            "strong" | "b" => {
                self.push_output("**");
            }
            "em" | "i" | "figcaption" => {
                if tag == "figcaption" {
                    self.ensure_blank_line();
                }
                self.push_output("*");
            }
            "del" | "s" | "strike" => {
                self.push_output("~~");
            }
            "ul" => {
                if self.list_stack.is_empty() {
                    self.ensure_blank_line();
                } else {
                    self.ensure_newline();
                }
                self.list_stack.push(ListKind::Unordered);
            }
            "ol" => {
                if self.list_stack.is_empty() {
                    self.ensure_blank_line();
                } else {
                    self.ensure_newline();
                }
                let start = parse_attr(attrs_str, "start")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(1);
                self.list_stack.push(ListKind::Ordered(start));
            }
            "li" => {
                self.ensure_newline();
                let indent_levels = self.list_stack.len().saturating_sub(1);
                for _ in 0..indent_levels {
                    self.push_output("  ");
                }
                if let Some(top) = self.list_stack.last_mut() {
                    match top {
                        ListKind::Unordered => self.push_output("- "),
                        ListKind::Ordered(n) => {
                            let marker = format!("{n}. ");
                            *n += 1;
                            self.push_output(&marker);
                        }
                    }
                } else {
                    self.push_output("- ");
                }
            }
            "a" => {
                if !self_closing {
                    let raw_href = parse_attr(attrs_str, "href").unwrap_or_default();
                    let resolved = self.resolve_safe_url(&raw_href);
                    self.active_link = Some(LinkContext {
                        href: resolved.unwrap_or_default(),
                        text_buf: String::new(),
                    });
                }
            }
            "img" => {
                if self.options.include_images {
                    let raw_src = parse_attr(attrs_str, "src").unwrap_or_default();
                    if let Some(resolved) = self.resolve_safe_url(&raw_src) {
                        let alt = parse_attr(attrs_str, "alt")
                            .map(|a| decode_html_entities(&a))
                            .unwrap_or_default();
                        let clean_alt = alt.replace(['\r', '\n'], " ");
                        let rendered = format!("![{}]({})", clean_alt.trim(), resolved);
                        self.push_output(&rendered);
                        self.image_count += 1;
                    }
                }
            }
            "table" => {
                self.ensure_blank_line();
                self.table = Some(TableState::new());
            }
            "tr" => {
                if let Some(table) = self.table.as_mut() {
                    table.current_row.clear();
                }
            }
            "th" | "td" => {
                if let Some(table) = self.table.as_mut() {
                    table.current_cell.clear();
                    table.in_cell = true;
                }
            }
            _ => {}
        }
    }

    fn handle_close_tag(&mut self, tag: &str) {
        match tag {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.pending_heading_level = None;
                self.ensure_blank_line();
            }
            "p" | "div" | "section" | "article" | "header" | "footer" | "main" => {
                if self.table.is_none() && !self.in_pre {
                    self.ensure_blank_line();
                }
            }
            "blockquote" => {
                self.blockquote_depth = self.blockquote_depth.saturating_sub(1);
                self.ensure_blank_line();
            }
            "pre" => {
                self.in_pre = false;
                let pre_buf = std::mem::take(&mut self.pre_buf);
                let code = pre_buf.trim_matches('\n');
                let fence = if code.contains("```") { "````" } else { "```" };
                let lang = if self.options.preserve_code_languages {
                    self.pre_lang.take().unwrap_or_default()
                } else {
                    self.pre_lang = None;
                    String::new()
                };
                self.push_output(fence);
                self.push_output(&lang);
                self.push_output("\n");
                self.push_output(code);
                self.push_output("\n");
                self.push_output(fence);
                self.push_output("\n\n");
            }
            "code" => {
                if !self.in_pre && self.in_inline_code {
                    self.in_inline_code = false;
                    let inline_buf = std::mem::take(&mut self.inline_code_buf);
                    let code = inline_buf.trim();
                    if !code.is_empty() {
                        if code.contains('`') {
                            self.push_output("`` ");
                            self.push_output(code);
                            self.push_output(" ``");
                        } else {
                            self.push_output("`");
                            self.push_output(code);
                            self.push_output("`");
                        }
                    }
                }
            }
            "strong" | "b" => {
                self.push_output("**");
            }
            "em" | "i" | "figcaption" => {
                self.push_output("*");
                if tag == "figcaption" {
                    self.ensure_blank_line();
                }
            }
            "del" | "s" | "strike" => {
                self.push_output("~~");
            }
            "ul" | "ol" => {
                self.list_stack.pop();
                if self.list_stack.is_empty() {
                    self.ensure_blank_line();
                } else {
                    self.ensure_newline();
                }
            }
            "li" => {
                self.ensure_newline();
            }
            "a" => {
                if let Some(link) = self.active_link.take() {
                    let label = link.text_buf.trim();
                    if link.href.is_empty() || self.options.link_style == LinkStyle::Strip {
                        self.push_output(label);
                    } else if !label.is_empty() {
                        self.link_count += 1;
                        match self.options.link_style {
                            LinkStyle::Inline => {
                                let md = format!("[{label}]({})", link.href);
                                self.push_output(&md);
                            }
                            LinkStyle::Reference => {
                                let ref_id = self.get_or_insert_ref(&link.href);
                                let md = format!("[{label}][{ref_id}]");
                                self.push_output(&md);
                            }
                            LinkStyle::Strip => {
                                self.push_output(label);
                            }
                        }
                    }
                }
            }
            "th" | "td" => {
                if let Some(table) = self.table.as_mut() {
                    table.in_cell = false;
                    let cell = table
                        .current_cell
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .replace('|', "\\|");
                    table.current_row.push(cell);
                    table.current_cell.clear();
                }
            }
            "tr" => {
                if let Some(table) = self.table.as_mut() {
                    if !table.current_row.is_empty() {
                        let row = std::mem::take(&mut table.current_row);
                        table.rows.push(row);
                    }
                }
            }
            "table" => {
                if let Some(table) = self.table.take() {
                    self.render_table(table);
                }
            }
            _ => {}
        }
    }

    fn handle_text(&mut self, raw: &str) {
        if self.skip_tag.is_some() {
            return;
        }
        let decoded = decode_html_entities(raw);
        if self.in_pre {
            self.pre_buf.push_str(&decoded);
            return;
        }
        if self.in_inline_code {
            self.inline_code_buf.push_str(&decoded);
            return;
        }
        let collapsed = collapse_inline_whitespace(&decoded);
        if collapsed.is_empty() {
            return;
        }
        if collapsed == " "
            && (self.out.ends_with('\n') || self.out.ends_with(' ') || self.out.is_empty())
            && self.active_link.is_none()
            && !self.table.as_ref().is_some_and(|t| t.in_cell)
        {
            return;
        }
        self.push_output(&collapsed);
    }

    fn push_output(&mut self, s: &str) {
        if let Some(link) = self.active_link.as_mut() {
            link.text_buf.push_str(s);
            return;
        }
        if let Some(table) = self.table.as_mut() {
            if table.in_cell {
                table.current_cell.push_str(s);
                return;
            }
        }
        self.out.push_str(s);
    }

    fn ensure_newline(&mut self) {
        if self.table.is_some() || self.active_link.is_some() {
            return;
        }
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.out.push('\n');
            self.write_blockquote_prefix_if_line_start();
        }
    }

    fn ensure_blank_line(&mut self) {
        if self.table.is_some() || self.active_link.is_some() || self.out.is_empty() {
            return;
        }
        if self.out.ends_with("\n\n") {
            return;
        }
        if self.out.ends_with('\n') {
            self.out.push('\n');
        } else {
            self.out.push_str("\n\n");
        }
        self.write_blockquote_prefix_if_line_start();
    }

    fn write_blockquote_prefix_if_line_start(&mut self) {
        if self.blockquote_depth > 0 && self.out.ends_with('\n') {
            for _ in 0..self.blockquote_depth {
                self.out.push_str("> ");
            }
        }
    }

    fn resolve_safe_url(&self, raw: &str) -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }

        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("javascript:")
            || lower.starts_with("vbscript:")
            || lower.starts_with("data:")
        {
            return None;
        }

        if let Ok(parsed) = Url::parse(trimmed) {
            return Some(parsed.to_string());
        }

        if let Some(base) = &self.base_url {
            if let Ok(joined) = base.join(trimmed) {
                return Some(joined.to_string());
            }
        }

        Some(trimmed.to_string())
    }

    fn get_or_insert_ref(&mut self, url: &str) -> usize {
        if let Some(&id) = self.ref_map.get(url) {
            return id;
        }
        let id = self.ref_links.len() + 1;
        self.ref_map.insert(url.to_string(), id);
        self.ref_links.push((id, url.to_string()));
        id
    }

    fn render_table(&mut self, table: TableState) {
        if table.rows.is_empty() {
            return;
        }
        let col_count = table.rows.iter().map(Vec::len).max().unwrap_or(0);
        if col_count == 0 {
            return;
        }

        for (row_idx, row) in table.rows.iter().enumerate() {
            self.out.push('|');
            for col_idx in 0..col_count {
                let cell = row.get(col_idx).map(String::as_str).unwrap_or("");
                let _ = write!(self.out, " {cell} |");
            }
            self.out.push('\n');

            if row_idx == 0 {
                self.out.push('|');
                for _ in 0..col_count {
                    self.out.push_str(" --- |");
                }
                self.out.push('\n');
            }
        }
        self.out.push('\n');
    }

    fn finish_body(&mut self) -> String {
        let mut cleaned = normalize_markdown_whitespace(&self.out);
        if !self.ref_links.is_empty() {
            if !cleaned.is_empty() {
                cleaned.push_str("\n\n");
            }
            for (id, url) in &self.ref_links {
                let _ = writeln!(cleaned, "[{id}]: {url}");
            }
        }
        cleaned.trim().to_string()
    }
}

fn find_tag_end(s: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (idx, ch) in s.char_indices() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => {}
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch == '>' => return Some(idx),
            None => {}
        }
    }
    None
}

fn parse_attr(attrs: &str, target: &str) -> Option<String> {
    let bytes = attrs.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let name_start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' {
            i += 1;
        }
        let name = attrs[name_start..i].to_ascii_lowercase();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'=' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let val = if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                let q = bytes[i];
                i += 1;
                let v_start = i;
                while i < bytes.len() && bytes[i] != q {
                    i += 1;
                }
                let slice = &attrs[v_start..i];
                if i < bytes.len() {
                    i += 1;
                }
                slice
            } else {
                let v_start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                &attrs[v_start..i]
            };
            if name == target {
                return Some(val.to_string());
            }
        }
    }
    None
}

fn extract_code_language(attrs: &str) -> Option<String> {
    let class_val = parse_attr(attrs, "class")?;
    for token in class_val.split_whitespace() {
        if let Some(lang) = token
            .strip_prefix("language-")
            .or_else(|| token.strip_prefix("lang-"))
        {
            let clean: String = lang
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '+')
                .collect();
            if !clean.is_empty() {
                return Some(clean);
            }
        }
    }
    None
}

fn collapse_inline_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            out.push(ch);
            in_ws = false;
        }
    }
    out
}

fn normalize_markdown_whitespace(raw: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut blank_run = 0usize;
    let mut in_fence = false;

    for line in raw.lines() {
        let trimmed_start = line.trim_start();
        if trimmed_start.starts_with("```") {
            in_fence = !in_fence;
            blank_run = 0;
            lines.push(line.trim_end().to_string());
            continue;
        }

        if in_fence {
            lines.push(line.to_string());
            continue;
        }

        let trimmed_end = line.trim_end();
        if trimmed_end.is_empty() {
            blank_run += 1;
            if blank_run <= 1 {
                lines.push(String::new());
            }
        } else {
            blank_run = 0;
            lines.push(trimmed_end.to_string());
        }
    }

    lines.join("\n")
}

fn decode_html_entities(input: &str) -> String {
    if !input.contains('&') {
        return input.to_string();
    }

    let mut out = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(amp_idx) = rest.find('&') {
        out.push_str(&rest[..amp_idx]);
        let after_amp = &rest[amp_idx + 1..];
        if let Some(semi_idx) = after_amp.find(';') {
            if semi_idx > 0 && semi_idx <= 12 {
                let entity = &after_amp[..semi_idx];
                if let Some(decoded) = decode_single_entity(entity) {
                    out.push(decoded);
                    rest = &after_amp[semi_idx + 1..];
                    continue;
                }
            }
        }
        out.push('&');
        rest = after_amp;
    }
    out.push_str(rest);
    out
}

fn decode_single_entity(entity: &str) -> Option<char> {
    if let Some(hex) = entity
        .strip_prefix("#x")
        .or_else(|| entity.strip_prefix("#X"))
    {
        return u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
    }
    if let Some(dec) = entity.strip_prefix('#') {
        return dec.parse::<u32>().ok().and_then(char::from_u32);
    }
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        "ndash" => Some('\u{2013}'),
        "mdash" => Some('\u{2014}'),
        "hellip" => Some('\u{2026}'),
        "lsquo" => Some('\u{2018}'),
        "rsquo" => Some('\u{2019}'),
        "ldquo" => Some('\u{201C}'),
        "rdquo" => Some('\u{201D}'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_article() -> ArticleInput {
        ArticleInput {
            title: "Rust & Linux: Reader Export".to_string(),
            url: Some("https://example.com/posts/rust-linux".to_string()),
            site_name: Some("Example Blog".to_string()),
            byline: Some("Ada Lovelace".to_string()),
            published_at: Some("2026-11-19".to_string()),
            excerpt: Some("Clean Markdown export.".to_string()),
            content_html: r#"
                <script>alert("xss")</script>
                <p>Hello <strong>world</strong> &amp; <em>readers</em>!</p>
                <h2>Code Example</h2>
                <pre><code class="language-rust">fn main() {
    println!("hi");
}</code></pre>
                <ul>
                    <li>First item with <a href="/docs/guide">relative link</a></li>
                    <li>Second item with <img src="/img/diagram.png" alt="Arch"></li>
                </ul>
            "#
            .to_string(),
            tags: vec!["rust".to_string(), "linux".to_string()],
        }
    }

    #[test]
    fn happy_path_yaml_frontmatter_and_elements() {
        let article = sample_article();
        let opts = MarkdownExportOptions::default();
        let res = export_article_to_markdown(&article, &opts).unwrap();

        assert_eq!(res.suggested_filename, "rust-linux-reader-export.md");
        assert!(res.markdown.starts_with("---\n"));
        assert!(res
            .markdown
            .contains("title: \"Rust & Linux: Reader Export\""));
        assert!(res.markdown.contains("# Rust & Linux: Reader Export"));
        assert!(res.markdown.contains("Hello **world** & *readers*!"));
        assert!(res.markdown.contains("## Code Example"));
        assert!(res.markdown.contains("```rust\nfn main() {"));
        assert!(res
            .markdown
            .contains("- First item with [relative link](https://example.com/docs/guide)"));
        assert!(res
            .markdown
            .contains("- Second item with ![Arch](https://example.com/img/diagram.png)"));
        assert!(!res.markdown.contains("alert(\"xss\")"));
        assert_eq!(res.link_count, 1);
        assert_eq!(res.image_count, 1);
        assert_eq!(res.heading_count, 1);
        assert!(res.word_count > 10);
        assert!(res.estimated_minutes >= 1);
    }

    #[test]
    fn reference_links_toml_frontmatter_and_table() {
        let article = ArticleInput {
            title: "Comparison".to_string(),
            url: Some("https://example.com/compare".to_string()),
            site_name: None,
            byline: None,
            published_at: None,
            excerpt: None,
            content_html: r#"
                <p>See <a href="https://a.example">Site A</a> and <a href="https://b.example">Site B</a>.</p>
                <table>
                    <tr><th>Engine</th><th>Score</th></tr>
                    <tr><td>Search</td><td>100</td></tr>
                </table>
            "#
            .to_string(),
            tags: vec!["bench".to_string()],
        };
        let opts = MarkdownExportOptions {
            frontmatter: FrontmatterFormat::Toml,
            link_style: LinkStyle::Reference,
            include_title_heading: false,
            include_images: false,
            heading_level_offset: 1,
            ..Default::default()
        };

        let res = export_article_to_markdown(&article, &opts).unwrap();
        assert!(res.markdown.starts_with("+++\n"));
        assert!(res.markdown.contains("title = \"Comparison\""));
        assert!(res.markdown.contains("tags = [\"bench\"]"));
        assert!(res.markdown.contains("[Site A][1]"));
        assert!(res.markdown.contains("[Site B][2]"));
        assert!(res.markdown.contains("[1]: https://a.example/"));
        assert!(res.markdown.contains("[2]: https://b.example/"));
        assert!(res
            .markdown
            .contains("| Engine | Score |\n| --- | --- |\n| Search | 100 |"));
        assert!(!res.markdown.contains("# Comparison"));
    }

    #[test]
    fn validation_and_unsafe_urls_edge_cases() {
        let empty = ArticleInput {
            title: "Empty".to_string(),
            url: None,
            site_name: None,
            byline: None,
            published_at: None,
            excerpt: None,
            content_html: "   ".to_string(),
            tags: vec![],
        };
        assert_eq!(
            export_article_to_markdown(&empty, &MarkdownExportOptions::default()),
            Err(MarkdownExportError::EmptyContent)
        );

        let bad_offset = MarkdownExportOptions {
            heading_level_offset: 5,
            ..Default::default()
        };
        assert_eq!(
            bad_offset.validate(),
            Err(MarkdownExportError::InvalidHeadingOffset(5))
        );

        let xss_article = ArticleInput {
            title: "Security".to_string(),
            url: None,
            site_name: None,
            byline: None,
            published_at: None,
            excerpt: None,
            content_html: r#"<p><a href="javascript:alert(1)">Click</a> <img src="data:text/html,hi" alt="bad"></p>"#.to_string(),
            tags: vec![],
        };
        let res =
            export_article_to_markdown(&xss_article, &MarkdownExportOptions::default()).unwrap();
        assert!(!res.markdown.contains("javascript:"));
        assert!(!res.markdown.contains("data:text/html"));
        assert!(res.markdown.contains("Click"));
        assert_eq!(res.link_count, 0);
        assert_eq!(res.image_count, 0);
    }

    #[test]
    fn filename_slug_and_serde_roundtrip() {
        assert_eq!(
            suggest_markdown_filename("  Hello, World! --- 2026  "),
            "hello-world-2026.md"
        );
        assert_eq!(suggest_markdown_filename("..."), "article.md");

        let opts = MarkdownExportOptions::default();
        let json = serde_json::to_string(&opts).unwrap();
        let decoded: MarkdownExportOptions = serde_json::from_str(&json).unwrap();
        assert_eq!(opts, decoded);
    }
}
