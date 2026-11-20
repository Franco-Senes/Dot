//! Custom Search Engines registry, validation, omnibox keyword resolution,
//! OpenSearch XML import, and JSON bundle serialization for Search Linux.

use regex::Regex;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use url::form_urlencoded;
use url::Url;

/// Placeholder token defined by the OpenSearch specification.
const OPENSEARCH_TERMS_TOKEN: &str = "{searchTerms}";
/// Legacy Chromium/Firefox omnibox placeholder token.
const LEGACY_TERMS_TOKEN: &str = "%s";
/// Optional OpenSearch encoding placeholder.
const OPENSEARCH_ENCODING_TOKEN: &str = "{inputEncoding}";
/// Optional OpenSearch language placeholder.
const OPENSEARCH_LANGUAGE_TOKEN: &str = "{language}";

/// Current schema version for exported search engine bundles.
pub const BUNDLE_SCHEMA_VERSION: u32 = 1;

/// HTTP method used when submitting a search query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum SearchMethod {
    #[default]
    Get,
    Post,
}

/// Logical category for organizing custom search engines in settings and UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SearchEngineCategory {
    #[default]
    General,
    Code,
    Academic,
    Media,
    Shopping,
    Maps,
    Reference,
    Custom(String),
}

/// Key/value pair sent in the body of a `POST` search request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostParam {
    pub key: String,
    pub value: String,
}

/// Definition of a custom or built-in search engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomSearchEngine {
    pub id: String,
    pub name: String,
    pub keyword: String,
    pub url_template: String,
    #[serde(default)]
    pub suggest_url: Option<String>,
    #[serde(default)]
    pub favicon_url: Option<String>,
    #[serde(default)]
    pub method: SearchMethod,
    #[serde(default)]
    pub post_params: Vec<PostParam>,
    #[serde(default = "default_encoding")]
    pub encoding: String,
    #[serde(default)]
    pub category: SearchEngineCategory,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub is_builtin: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub usage_count: u64,
    #[serde(default)]
    pub created_at_unix: i64,
    #[serde(default)]
    pub updated_at_unix: i64,
}

fn default_encoding() -> String {
    "UTF-8".to_string()
}

fn default_true() -> bool {
    true
}

/// Partial update payload for modifying an existing [`CustomSearchEngine`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchEnginePatch {
    pub name: Option<String>,
    pub keyword: Option<String>,
    pub url_template: Option<String>,
    pub suggest_url: Option<Option<String>>,
    pub favicon_url: Option<Option<String>>,
    pub method: Option<SearchMethod>,
    pub post_params: Option<Vec<PostParam>>,
    pub encoding: Option<String>,
    pub category: Option<SearchEngineCategory>,
    pub enabled: Option<bool>,
}

/// Result of resolving an omnibox input string against the registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryResolution {
    pub engine_id: String,
    pub engine_name: String,
    pub keyword: String,
    pub raw_query: String,
    pub target_url: String,
    pub method: SearchMethod,
    pub post_body: Option<Vec<(String, String)>>,
    pub suggest_url: Option<String>,
}

/// Versioned JSON export/import bundle for custom search engines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchEngineBundle {
    pub version: u32,
    pub exported_at_unix: i64,
    pub engines: Vec<CustomSearchEngine>,
}

/// Summary returned after importing a [`SearchEngineBundle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportReport {
    pub added: usize,
    pub updated: usize,
    pub skipped: usize,
}

/// Errors produced by search engine validation, resolution, or persistence.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SearchEngineError {
    #[error("search engine id must not be empty")]
    EmptyId,
    #[error("search engine name must not be empty")]
    EmptyName,
    #[error("invalid keyword '{0}': must be 1..=32 ASCII alphanumeric, '-' or '_' characters starting with alphanumeric")]
    InvalidKeyword(String),
    #[error("duplicate search engine id: '{0}'")]
    DuplicateId(String),
    #[error("duplicate search engine keyword: '{0}'")]
    DuplicateKeyword(String),
    #[error("search engine not found: '{0}'")]
    EngineNotFound(String),
    #[error("invalid URL template: {0}")]
    InvalidUrlTemplate(String),
    #[error("invalid suggest URL template: {0}")]
    InvalidSuggestUrl(String),
    #[error("cannot remove the default search engine '{0}'")]
    CannotDeleteDefaultEngine(String),
    #[error("cannot remove a built-in search engine '{0}'; disable it instead")]
    CannotDeleteBuiltinEngine(String),
    #[error("cannot disable the default search engine '{0}'")]
    CannotDisableDefaultEngine(String),
    #[error("cannot set disabled search engine '{0}' as default")]
    CannotSetDisabledAsDefault(String),
    #[error("no default search engine is configured")]
    NoDefaultEngine,
    #[error("search query must not be empty")]
    EmptyQuery,
    #[error("invalid OpenSearch XML: {0}")]
    InvalidOpenSearchXml(String),
    #[error("serialization error: {0}")]
    SerializationError(String),
    #[error("unsupported bundle version: {0}")]
    UnsupportedBundleVersion(u32),
}

/// In-memory registry managing built-in and user-defined custom search engines.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchEngineRegistry {
    engines: Vec<CustomSearchEngine>,
}

impl SearchEngineRegistry {
    /// Creates an empty search engine registry.
    pub fn new() -> Self {
        Self {
            engines: Vec::new(),
        }
    }

    /// Creates a registry pre-populated with privacy-focused and developer built-in engines.
    pub fn with_builtins() -> Self {
        let now = now_unix();
        let mut reg = Self::new();
        let builtins = [
            (
                "duckduckgo",
                "DuckDuckGo",
                "ddg",
                "https://duckduckgo.com/?q={searchTerms}",
                Some("https://duckduckgo.com/ac/?q={searchTerms}&type=list"),
                SearchEngineCategory::General,
                true,
            ),
            (
                "startpage",
                "Startpage",
                "sp",
                "https://www.startpage.com/sp/search?query={searchTerms}",
                None,
                SearchEngineCategory::General,
                false,
            ),
            (
                "wikipedia",
                "Wikipedia",
                "wiki",
                "https://en.wikipedia.org/w/index.php?search={searchTerms}",
                Some("https://en.wikipedia.org/w/api.php?action=opensearch&search={searchTerms}"),
                SearchEngineCategory::Reference,
                false,
            ),
            (
                "github",
                "GitHub",
                "gh",
                "https://github.com/search?q={searchTerms}",
                None,
                SearchEngineCategory::Code,
                false,
            ),
            (
                "crates_io",
                "crates.io",
                "crates",
                "https://crates.io/search?q={searchTerms}",
                None,
                SearchEngineCategory::Code,
                false,
            ),
            (
                "mdn",
                "MDN Web Docs",
                "mdn",
                "https://developer.mozilla.org/en-US/search?q={searchTerms}",
                None,
                SearchEngineCategory::Reference,
                false,
            ),
        ];

        for (id, name, kw, tmpl, suggest, category, is_default) in builtins {
            let engine = CustomSearchEngine {
                id: id.to_string(),
                name: name.to_string(),
                keyword: kw.to_string(),
                url_template: tmpl.to_string(),
                suggest_url: suggest.map(str::to_string),
                favicon_url: None,
                method: SearchMethod::Get,
                post_params: Vec::new(),
                encoding: default_encoding(),
                category,
                is_default,
                is_builtin: true,
                enabled: true,
                usage_count: 0,
                created_at_unix: now,
                updated_at_unix: now,
            };
            reg.engines.push(engine);
        }

        reg
    }

    /// Adds a new custom search engine after validating all fields and uniqueness.
    pub fn add_engine(&mut self, mut engine: CustomSearchEngine) -> Result<(), SearchEngineError> {
        let now = now_unix();
        engine.id = engine.id.trim().to_string();
        engine.name = engine.name.trim().to_string();
        engine.keyword = normalize_keyword(&engine.keyword)?;

        if engine.id.is_empty() {
            return Err(SearchEngineError::EmptyId);
        }
        if engine.name.is_empty() {
            return Err(SearchEngineError::EmptyName);
        }
        validate_url_template(&engine.url_template, false)?;
        if let Some(ref suggest) = engine.suggest_url {
            validate_url_template(suggest, true)?;
        }
        if let Some(ref favicon) = engine.favicon_url {
            validate_http_url(favicon).map_err(SearchEngineError::InvalidUrlTemplate)?;
        }
        if engine.encoding.trim().is_empty() {
            engine.encoding = default_encoding();
        }

        if self.engines.iter().any(|e| e.id == engine.id) {
            return Err(SearchEngineError::DuplicateId(engine.id));
        }
        if self.engines.iter().any(|e| e.keyword == engine.keyword) {
            return Err(SearchEngineError::DuplicateKeyword(engine.keyword));
        }

        if engine.created_at_unix == 0 {
            engine.created_at_unix = now;
        }
        engine.updated_at_unix = now;

        let wants_default = engine.is_default;
        if wants_default && !engine.enabled {
            return Err(SearchEngineError::CannotSetDisabledAsDefault(engine.id));
        }
        if wants_default {
            for existing in &mut self.engines {
                existing.is_default = false;
            }
        }

        self.engines.push(engine);
        Ok(())
    }

    /// Updates an existing search engine using a partial [`SearchEnginePatch`].
    pub fn update_engine(
        &mut self,
        id: &str,
        patch: SearchEnginePatch,
    ) -> Result<&CustomSearchEngine, SearchEngineError> {
        let idx = self
            .engines
            .iter()
            .position(|e| e.id == id)
            .ok_or_else(|| SearchEngineError::EngineNotFound(id.to_string()))?;

        if let Some(ref name) = patch.name {
            if name.trim().is_empty() {
                return Err(SearchEngineError::EmptyName);
            }
        }

        let normalized_kw = match patch.keyword {
            Some(ref kw) => {
                let norm = normalize_keyword(kw)?;
                if self
                    .engines
                    .iter()
                    .enumerate()
                    .any(|(i, e)| i != idx && e.keyword == norm)
                {
                    return Err(SearchEngineError::DuplicateKeyword(norm));
                }
                Some(norm)
            }
            None => None,
        };

        if let Some(ref tmpl) = patch.url_template {
            validate_url_template(tmpl, false)?;
        }
        if let Some(Some(ref suggest)) = patch.suggest_url {
            validate_url_template(suggest, true)?;
        }
        if let Some(Some(ref favicon)) = patch.favicon_url {
            validate_http_url(favicon).map_err(SearchEngineError::InvalidUrlTemplate)?;
        }
        if let Some(false) = patch.enabled {
            if self.engines[idx].is_default {
                return Err(SearchEngineError::CannotDisableDefaultEngine(
                    id.to_string(),
                ));
            }
        }

        let engine = &mut self.engines[idx];
        if let Some(name) = patch.name {
            engine.name = name.trim().to_string();
        }
        if let Some(kw) = normalized_kw {
            engine.keyword = kw;
        }
        if let Some(tmpl) = patch.url_template {
            engine.url_template = tmpl;
        }
        if let Some(suggest) = patch.suggest_url {
            engine.suggest_url = suggest;
        }
        if let Some(favicon) = patch.favicon_url {
            engine.favicon_url = favicon;
        }
        if let Some(method) = patch.method {
            engine.method = method;
        }
        if let Some(params) = patch.post_params {
            engine.post_params = params;
        }
        if let Some(encoding) = patch.encoding {
            if !encoding.trim().is_empty() {
                engine.encoding = encoding.trim().to_string();
            }
        }
        if let Some(category) = patch.category {
            engine.category = category;
        }
        if let Some(enabled) = patch.enabled {
            engine.enabled = enabled;
        }
        engine.updated_at_unix = now_unix();

        Ok(&self.engines[idx])
    }

    /// Removes a user-defined search engine by ID.
    /// Built-in and default engines cannot be removed.
    pub fn remove_engine(&mut self, id: &str) -> Result<CustomSearchEngine, SearchEngineError> {
        let idx = self
            .engines
            .iter()
            .position(|e| e.id == id)
            .ok_or_else(|| SearchEngineError::EngineNotFound(id.to_string()))?;

        if self.engines[idx].is_default {
            return Err(SearchEngineError::CannotDeleteDefaultEngine(id.to_string()));
        }
        if self.engines[idx].is_builtin {
            return Err(SearchEngineError::CannotDeleteBuiltinEngine(id.to_string()));
        }

        Ok(self.engines.remove(idx))
    }

    /// Enables or disables a search engine.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), SearchEngineError> {
        let idx = self
            .engines
            .iter()
            .position(|e| e.id == id)
            .ok_or_else(|| SearchEngineError::EngineNotFound(id.to_string()))?;

        if !enabled && self.engines[idx].is_default {
            return Err(SearchEngineError::CannotDisableDefaultEngine(
                id.to_string(),
            ));
        }

        self.engines[idx].enabled = enabled;
        self.engines[idx].updated_at_unix = now_unix();
        Ok(())
    }

    /// Marks an enabled search engine as the default engine.
    pub fn set_default(&mut self, id: &str) -> Result<(), SearchEngineError> {
        let idx = self
            .engines
            .iter()
            .position(|e| e.id == id)
            .ok_or_else(|| SearchEngineError::EngineNotFound(id.to_string()))?;

        if !self.engines[idx].enabled {
            return Err(SearchEngineError::CannotSetDisabledAsDefault(
                id.to_string(),
            ));
        }

        let now = now_unix();
        for (i, engine) in self.engines.iter_mut().enumerate() {
            let new_default = i == idx;
            if engine.is_default != new_default {
                engine.is_default = new_default;
                engine.updated_at_unix = now;
            }
        }
        Ok(())
    }

    /// Returns the currently active default search engine, if any.
    pub fn default_engine(&self) -> Option<&CustomSearchEngine> {
        self.engines.iter().find(|e| e.is_default && e.enabled)
    }

    /// Looks up an engine by its unique identifier.
    pub fn get_by_id(&self, id: &str) -> Option<&CustomSearchEngine> {
        self.engines.iter().find(|e| e.id == id)
    }

    /// Looks up an engine by its omnibox keyword (case-insensitive).
    pub fn get_by_keyword(&self, keyword: &str) -> Option<&CustomSearchEngine> {
        let kw = keyword.trim().to_ascii_lowercase();
        self.engines.iter().find(|e| e.keyword == kw)
    }

    /// Lists all registered engines.
    pub fn list_engines(&self) -> &[CustomSearchEngine] {
        &self.engines
    }

    /// Lists all enabled engines.
    pub fn list_enabled(&self) -> Vec<&CustomSearchEngine> {
        self.engines.iter().filter(|e| e.enabled).collect()
    }

    /// Lists all engines matching the requested category.
    pub fn list_by_category(&self, category: &SearchEngineCategory) -> Vec<&CustomSearchEngine> {
        self.engines
            .iter()
            .filter(|e| &e.category == category)
            .collect()
    }

    /// Checks whether `input` starts with an enabled engine keyword (`kw query`,
    /// `@kw query`, or `!kw query`), returning the matched engine and remaining query.
    pub fn match_omnibox_input<'a>(
        &self,
        input: &'a str,
    ) -> Option<(&CustomSearchEngine, &'a str)> {
        let trimmed = input.trim();
        let (first_token, rest) = split_first_token(trimmed)?;
        let candidate_kw = first_token
            .strip_prefix('@')
            .or_else(|| first_token.strip_prefix('!'))
            .unwrap_or(first_token)
            .to_ascii_lowercase();

        let engine = self
            .engines
            .iter()
            .find(|e| e.enabled && e.keyword == candidate_kw)?;

        let query = rest.trim();
        if query.is_empty() {
            return None;
        }
        Some((engine, query))
    }

    /// Resolves an omnibox input string into a concrete [`QueryResolution`].
    ///
    /// If the input begins with a matching enabled keyword and a non-empty query,
    /// that engine is used; otherwise the default engine is used for the full input.
    /// Increments `usage_count` on the chosen engine.
    pub fn resolve_omnibox(&mut self, input: &str) -> Result<QueryResolution, SearchEngineError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(SearchEngineError::EmptyQuery);
        }

        let (idx, raw_query) = if let Some((engine, query)) = self.match_omnibox_input(trimmed) {
            let id = engine.id.clone();
            let idx = self
                .engines
                .iter()
                .position(|e| e.id == id)
                .ok_or(SearchEngineError::NoDefaultEngine)?;
            (idx, query.to_string())
        } else {
            let idx = self
                .engines
                .iter()
                .position(|e| e.is_default && e.enabled)
                .ok_or(SearchEngineError::NoDefaultEngine)?;
            (idx, trimmed.to_string())
        };

        self.engines[idx].usage_count = self.engines[idx].usage_count.saturating_add(1);
        let engine = &self.engines[idx];

        let target_url = build_search_url(engine, &raw_query)?;
        let post_body = build_post_body(engine, &raw_query);
        let suggest_url = build_suggest_url(engine, &raw_query)?;

        Ok(QueryResolution {
            engine_id: engine.id.clone(),
            engine_name: engine.name.clone(),
            keyword: engine.keyword.clone(),
            raw_query,
            target_url,
            method: engine.method,
            post_body,
            suggest_url,
        })
    }

    /// Exports all engines in the registry as a versioned JSON bundle string.
    pub fn export_json(&self) -> Result<String, SearchEngineError> {
        let bundle = SearchEngineBundle {
            version: BUNDLE_SCHEMA_VERSION,
            exported_at_unix: now_unix(),
            engines: self.engines.clone(),
        };
        serde_json::to_string_pretty(&bundle)
            .map_err(|e| SearchEngineError::SerializationError(e.to_string()))
    }

    /// Imports engines from a JSON bundle string.
    ///
    /// When `overwrite_existing` is `true`, engines with matching IDs are updated;
    /// otherwise existing IDs are skipped.
    pub fn import_json(
        &mut self,
        json: &str,
        overwrite_existing: bool,
    ) -> Result<ImportReport, SearchEngineError> {
        let bundle: SearchEngineBundle = serde_json::from_str(json)
            .map_err(|e| SearchEngineError::SerializationError(e.to_string()))?;

        if bundle.version != BUNDLE_SCHEMA_VERSION {
            return Err(SearchEngineError::UnsupportedBundleVersion(bundle.version));
        }

        let mut report = ImportReport {
            added: 0,
            updated: 0,
            skipped: 0,
        };

        for mut engine in bundle.engines {
            if let Some(existing) = self.engines.iter().find(|e| e.id == engine.id) {
                if !overwrite_existing {
                    report.skipped += 1;
                    continue;
                }
                let was_default = existing.is_default;
                let patch = SearchEnginePatch {
                    name: Some(engine.name),
                    keyword: Some(engine.keyword),
                    url_template: Some(engine.url_template),
                    suggest_url: Some(engine.suggest_url),
                    favicon_url: Some(engine.favicon_url),
                    method: Some(engine.method),
                    post_params: Some(engine.post_params),
                    encoding: Some(engine.encoding),
                    category: Some(engine.category),
                    enabled: Some(if was_default { true } else { engine.enabled }),
                };
                self.update_engine(&engine.id, patch)?;
                if engine.is_default && !was_default {
                    self.set_default(&engine.id)?;
                }
                report.updated += 1;
            } else {
                engine.is_builtin = false;
                self.add_engine(engine)?;
                report.added += 1;
            }
        }

        Ok(report)
    }
}

/// Substitutes query placeholders into `engine.url_template` and validates the resulting URL.
pub fn build_search_url(
    engine: &CustomSearchEngine,
    query: &str,
) -> Result<String, SearchEngineError> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(SearchEngineError::EmptyQuery);
    }
    substitute_url_template(&engine.url_template, trimmed, &engine.encoding)
        .map_err(SearchEngineError::InvalidUrlTemplate)
}

/// Builds the `POST` body parameters for `engine`, if `engine.method == SearchMethod::Post`.
pub fn build_post_body(engine: &CustomSearchEngine, query: &str) -> Option<Vec<(String, String)>> {
    if engine.method != SearchMethod::Post {
        return None;
    }
    let trimmed = query.trim();
    let pairs = engine
        .post_params
        .iter()
        .map(|p| {
            let value = p
                .value
                .replace(OPENSEARCH_TERMS_TOKEN, trimmed)
                .replace(LEGACY_TERMS_TOKEN, trimmed)
                .replace(OPENSEARCH_ENCODING_TOKEN, &engine.encoding)
                .replace(OPENSEARCH_LANGUAGE_TOKEN, "en");
            (p.key.clone(), value)
        })
        .collect();
    Some(pairs)
}

/// Substitutes query placeholders into `engine.suggest_url` if present.
pub fn build_suggest_url(
    engine: &CustomSearchEngine,
    prefix: &str,
) -> Result<Option<String>, SearchEngineError> {
    let Some(ref suggest_tmpl) = engine.suggest_url else {
        return Ok(None);
    };
    let trimmed = prefix.trim();
    if trimmed.is_empty() {
        return Err(SearchEngineError::EmptyQuery);
    }
    let url = substitute_url_template(suggest_tmpl, trimmed, &engine.encoding)
        .map_err(SearchEngineError::InvalidSuggestUrl)?;
    Ok(Some(url))
}

/// Parses a minimal OpenSearch XML description document into a [`CustomSearchEngine`].
pub fn parse_opensearch_xml(
    xml: &str,
    id: &str,
    keyword: &str,
) -> Result<CustomSearchEngine, SearchEngineError> {
    let short_name_re = Regex::new(r"(?is)<ShortName>\s*([^<]+?)\s*</ShortName>")
        .map_err(|e| SearchEngineError::InvalidOpenSearchXml(e.to_string()))?;
    let url_tag_re = Regex::new(r#"(?is)<Url\b([^>]*)/?>"#)
        .map_err(|e| SearchEngineError::InvalidOpenSearchXml(e.to_string()))?;
    let type_re = Regex::new(r#"(?is)\btype\s*=\s*["']([^"']+)["']"#)
        .map_err(|e| SearchEngineError::InvalidOpenSearchXml(e.to_string()))?;
    let tmpl_re = Regex::new(r#"(?is)\btemplate\s*=\s*["']([^"']+)["']"#)
        .map_err(|e| SearchEngineError::InvalidOpenSearchXml(e.to_string()))?;

    let name = short_name_re
        .captures(xml)
        .and_then(|c| c.get(1))
        .map(|m| decode_xml_entities(m.as_str().trim()))
        .filter(|s| !s.is_empty())
        .ok_or_else(|| SearchEngineError::InvalidOpenSearchXml("missing <ShortName>".into()))?;

    let mut html_template: Option<String> = None;
    let mut suggest_template: Option<String> = None;

    for cap in url_tag_re.captures_iter(xml) {
        let attrs = cap.get(1).map_or("", |m| m.as_str());
        let url_type = type_re
            .captures(attrs)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_ascii_lowercase());
        let template = tmpl_re
            .captures(attrs)
            .and_then(|c| c.get(1))
            .map(|m| decode_xml_entities(m.as_str().trim()));

        if let (Some(t), Some(tmpl)) = (url_type, template) {
            if t == "text/html" && html_template.is_none() {
                html_template = Some(tmpl);
            } else if t == "application/x-suggestions+json" && suggest_template.is_none() {
                suggest_template = Some(tmpl);
            }
        }
    }

    let url_template = html_template.ok_or_else(|| {
        SearchEngineError::InvalidOpenSearchXml(
            "missing <Url type=\"text/html\" template=\"...\">".into(),
        )
    })?;

    validate_url_template(&url_template, false)?;
    if let Some(ref suggest) = suggest_template {
        validate_url_template(suggest, true)?;
    }

    let norm_kw = normalize_keyword(keyword)?;
    let trimmed_id = id.trim();
    if trimmed_id.is_empty() {
        return Err(SearchEngineError::EmptyId);
    }

    let now = now_unix();
    Ok(CustomSearchEngine {
        id: trimmed_id.to_string(),
        name,
        keyword: norm_kw,
        url_template,
        suggest_url: suggest_template,
        favicon_url: None,
        method: SearchMethod::Get,
        post_params: Vec::new(),
        encoding: default_encoding(),
        category: SearchEngineCategory::General,
        is_default: false,
        is_builtin: false,
        enabled: true,
        usage_count: 0,
        created_at_unix: now,
        updated_at_unix: now,
    })
}

fn normalize_keyword(raw: &str) -> Result<String, SearchEngineError> {
    let kw = raw.trim().to_ascii_lowercase();
    if kw.is_empty() || kw.len() > 32 {
        return Err(SearchEngineError::InvalidKeyword(raw.to_string()));
    }
    let mut chars = kw.chars();
    let Some(first) = chars.next() else {
        return Err(SearchEngineError::InvalidKeyword(raw.to_string()));
    };
    if !first.is_ascii_alphanumeric() {
        return Err(SearchEngineError::InvalidKeyword(raw.to_string()));
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(SearchEngineError::InvalidKeyword(raw.to_string()));
    }
    Ok(kw)
}

fn validate_url_template(template: &str, is_suggest: bool) -> Result<(), SearchEngineError> {
    let make_err = |msg: String| {
        if is_suggest {
            SearchEngineError::InvalidSuggestUrl(msg)
        } else {
            SearchEngineError::InvalidUrlTemplate(msg)
        }
    };

    let trimmed = template.trim();
    if !trimmed.contains(OPENSEARCH_TERMS_TOKEN) && !trimmed.contains(LEGACY_TERMS_TOKEN) {
        return Err(make_err(
            "template must contain '{searchTerms}' or '%s'".to_string(),
        ));
    }

    substitute_url_template(trimmed, "test", "UTF-8").map_err(make_err)?;
    Ok(())
}

fn validate_http_url(raw: &str) -> Result<Url, String> {
    let parsed = Url::parse(raw.trim()).map_err(|e| e.to_string())?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed),
        other => Err(format!("unsupported URL scheme '{other}'")),
    }
}

fn substitute_url_template(template: &str, query: &str, encoding: &str) -> Result<String, String> {
    let encoded_query: String = form_urlencoded::byte_serialize(query.as_bytes()).collect();
    let encoded_encoding: String = form_urlencoded::byte_serialize(encoding.as_bytes()).collect();

    let filled = template
        .trim()
        .replace(OPENSEARCH_TERMS_TOKEN, &encoded_query)
        .replace(LEGACY_TERMS_TOKEN, &encoded_query)
        .replace(OPENSEARCH_ENCODING_TOKEN, &encoded_encoding)
        .replace(OPENSEARCH_LANGUAGE_TOKEN, "en");

    let parsed = validate_http_url(&filled)?;
    Ok(parsed.to_string())
}

fn split_first_token(input: &str) -> Option<(&str, &str)> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.find(char::is_whitespace) {
        Some(idx) => Some((&trimmed[..idx], &trimmed[idx..])),
        None => Some((trimmed, "")),
    }
}

fn decode_xml_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

fn now_unix() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_custom_engine(id: &str, keyword: &str) -> CustomSearchEngine {
        CustomSearchEngine {
            id: id.to_string(),
            name: format!("Custom {id}"),
            keyword: keyword.to_string(),
            url_template: "https://example.org/search?q={searchTerms}".to_string(),
            suggest_url: Some("https://example.org/suggest?q={searchTerms}".to_string()),
            favicon_url: None,
            method: SearchMethod::Get,
            post_params: Vec::new(),
            encoding: "UTF-8".to_string(),
            category: SearchEngineCategory::Custom("dev".to_string()),
            is_default: false,
            is_builtin: false,
            enabled: true,
            usage_count: 0,
            created_at_unix: 1000,
            updated_at_unix: 1000,
        }
    }

    #[test]
    fn resolves_default_and_keyword_omnibox_queries() {
        let mut reg = SearchEngineRegistry::with_builtins();

        // Default engine (DuckDuckGo) handles plain queries.
        let res_default = reg
            .resolve_omnibox("linux kernel")
            .expect("default resolve");
        assert_eq!(res_default.engine_id, "duckduckgo");
        assert_eq!(
            res_default.target_url,
            "https://duckduckgo.com/?q=linux+kernel"
        );
        assert_eq!(res_default.raw_query, "linux kernel");

        // Keyword prefix `gh`, `@gh`, and `!gh` route to GitHub.
        let res_kw = reg.resolve_omnibox("gh rust async").expect("kw resolve");
        assert_eq!(res_kw.engine_id, "github");
        assert_eq!(res_kw.target_url, "https://github.com/search?q=rust+async");

        let res_at = reg.resolve_omnibox("@crates serde").expect("@kw resolve");
        assert_eq!(res_at.engine_id, "crates_io");
        assert_eq!(res_at.target_url, "https://crates.io/search?q=serde");

        let res_bang = reg.resolve_omnibox("!wiki Rust").expect("!kw resolve");
        assert_eq!(res_bang.engine_id, "wikipedia");

        // Usage count should be tracked.
        assert_eq!(reg.get_by_id("duckduckgo").unwrap().usage_count, 1);
        assert_eq!(reg.get_by_id("github").unwrap().usage_count, 1);
    }

    #[test]
    fn validates_crud_and_default_constraints() {
        let mut reg = SearchEngineRegistry::with_builtins();

        // Reject duplicate keyword (case-insensitive).
        let dup_kw = sample_custom_engine("gh2", "GH");
        assert_eq!(
            reg.add_engine(dup_kw),
            Err(SearchEngineError::DuplicateKeyword("gh".to_string()))
        );

        // Reject invalid URL template missing placeholder.
        let mut bad_url = sample_custom_engine("bad", "bad");
        bad_url.url_template = "https://example.org/search".to_string();
        assert!(matches!(
            reg.add_engine(bad_url),
            Err(SearchEngineError::InvalidUrlTemplate(_))
        ));

        // Reject invalid keyword.
        let bad_kw = sample_custom_engine("badkw", "-invalid");
        assert!(matches!(
            reg.add_engine(bad_kw),
            Err(SearchEngineError::InvalidKeyword(_))
        ));

        // Add valid custom engine, make it default, then verify removal rules.
        let custom = sample_custom_engine("archwiki", "aw");
        reg.add_engine(custom).expect("add custom");
        reg.set_default("archwiki").expect("set default");
        assert_eq!(reg.default_engine().unwrap().id, "archwiki");

        // Cannot disable or delete the default engine.
        assert_eq!(
            reg.set_enabled("archwiki", false),
            Err(SearchEngineError::CannotDisableDefaultEngine(
                "archwiki".to_string()
            ))
        );
        assert_eq!(
            reg.remove_engine("archwiki"),
            Err(SearchEngineError::CannotDeleteDefaultEngine(
                "archwiki".to_string()
            ))
        );

        // Cannot delete built-in engine, but can disable it.
        assert_eq!(
            reg.remove_engine("github"),
            Err(SearchEngineError::CannotDeleteBuiltinEngine(
                "github".to_string()
            ))
        );
        reg.set_enabled("github", false).expect("disable builtin");

        // Switch default back and remove custom engine cleanly.
        reg.set_default("duckduckgo").expect("restore default");
        let removed = reg.remove_engine("archwiki").expect("remove custom");
        assert_eq!(removed.id, "archwiki");
    }

    #[test]
    fn encodes_special_characters_and_post_bodies() {
        let mut reg = SearchEngineRegistry::new();
        let mut post_engine = sample_custom_engine("post_search", "ps");
        post_engine.url_template = "https://example.org/post?fmt=%s".to_string();
        post_engine.method = SearchMethod::Post;
        post_engine.post_params = vec![
            PostParam {
                key: "q".to_string(),
                value: "{searchTerms}".to_string(),
            },
            PostParam {
                key: "enc".to_string(),
                value: "{inputEncoding}".to_string(),
            },
        ];
        post_engine.is_default = true;

        reg.add_engine(post_engine).expect("add post engine");
        let res = reg
            .resolve_omnibox("a&b=c?")
            .expect("resolve special chars");
        assert_eq!(res.target_url, "https://example.org/post?fmt=a%26b%3Dc%3F");
        assert_eq!(
            res.post_body,
            Some(vec![
                ("q".to_string(), "a&b=c?".to_string()),
                ("enc".to_string(), "UTF-8".to_string()),
            ])
        );
        assert_eq!(
            res.suggest_url,
            Some("https://example.org/suggest?q=a%26b%3Dc%3F".to_string())
        );
    }

    #[test]
    fn roundtrips_json_bundle_and_parses_opensearch_xml() {
        let mut reg = SearchEngineRegistry::with_builtins();
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<OpenSearchDescription xmlns="http://a9.com/-/spec/opensearch/1.1/">
  <ShortName>Docs &amp; Guides</ShortName>
  <Url type="text/html" template="https://docs.example.com/search?q={searchTerms}&amp;lang={language}"/>
  <Url type="application/x-suggestions+json" template="https://docs.example.com/suggest?q={searchTerms}"/>
</OpenSearchDescription>"#;

        let parsed = parse_opensearch_xml(xml, "docs_ex", "docs").expect("parse opensearch");
        assert_eq!(parsed.name, "Docs & Guides");
        assert_eq!(
            parsed.url_template,
            "https://docs.example.com/search?q={searchTerms}&lang={language}"
        );
        assert!(parsed.suggest_url.is_some());

        reg.add_engine(parsed).expect("add parsed engine");
        let exported = reg.export_json().expect("export json");

        let mut target = SearchEngineRegistry::with_builtins();
        let report_skip = target
            .import_json(&exported, false)
            .expect("import skip existing");
        assert_eq!(report_skip.added, 1);
        assert_eq!(report_skip.skipped, 6);
        assert_eq!(report_skip.updated, 0);

        let report_overwrite = target
            .import_json(&exported, true)
            .expect("import overwrite");
        assert_eq!(report_overwrite.added, 0);
        assert_eq!(report_overwrite.updated, 7);
    }
}
