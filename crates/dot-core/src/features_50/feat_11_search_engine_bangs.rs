//! Search engine bangs (`!g`, `!w`, `!gh`, etc.) for quick query routing.
//!
//! Provides domain models, validation, query parsing/expansion, prefix
//! autocomplete suggestions, usage tracking, and JSON import/export.

use std::collections::HashMap;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

const MAX_TRIGGER_LEN: usize = 32;
const QUERY_PLACEHOLDER: &str = "{query}";

/// Errors returned by bang validation, registration, and serialization.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum BangError {
    #[error("bang trigger cannot be empty")]
    EmptyTrigger,

    #[error("invalid bang trigger '{trigger}': {reason}")]
    InvalidTrigger {
        trigger: String,
        reason: &'static str,
    },

    #[error("bang name cannot be empty")]
    EmptyName,

    #[error("invalid URL template '{template}': {reason}")]
    InvalidUrlTemplate { template: String, reason: String },

    #[error("invalid home URL '{url}': {reason}")]
    InvalidHomeUrl { url: String, reason: String },

    #[error("duplicate bang trigger or alias '{trigger}'")]
    DuplicateTrigger { trigger: String },

    #[error("bang '{trigger}' not found")]
    BangNotFound { trigger: String },

    #[error("cannot remove built-in bang '{trigger}'")]
    BuiltinProtected { trigger: String },

    #[error("JSON serialization error: {0}")]
    Serialization(String),
}

impl From<serde_json::Error> for BangError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}

/// Logical category for organizing and filtering search engine bangs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BangCategory {
    WebSearch,
    Code,
    Documentation,
    Encyclopedia,
    Media,
    Shopping,
    Maps,
    Translation,
    PackageRegistry,
    Custom,
}

/// Definition of a single search engine bang.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BangDefinition {
    /// Canonical lowercase trigger without leading `!` (e.g., `"g"`).
    pub trigger: String,
    /// Optional alternative triggers without leading `!` (e.g., `["google"]`).
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Human-readable engine name (e.g., `"Google"`).
    pub name: String,
    /// URL template containing `{query}` (e.g., `"https://www.google.com/search?q={query}"`).
    pub url_template: String,
    /// Optional fallback URL when the search query after the bang is empty.
    #[serde(default)]
    pub home_url: Option<String>,
    /// Category for UI grouping and filtering.
    pub category: BangCategory,
    /// Encode spaces as `+` instead of `%20` when true.
    #[serde(default)]
    pub encode_plus: bool,
    /// Whether this bang is active during query resolution.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Whether this bang is a built-in default (protected from deletion).
    #[serde(default)]
    pub builtin: bool,
    /// Number of times this bang has been resolved.
    #[serde(default)]
    pub usage_count: u64,
}

fn default_true() -> bool {
    true
}

impl BangDefinition {
    /// Creates a new custom `BangDefinition` with sensible defaults.
    pub fn custom(
        trigger: impl Into<String>,
        name: impl Into<String>,
        url_template: impl Into<String>,
        category: BangCategory,
    ) -> Self {
        Self {
            trigger: trigger.into(),
            aliases: Vec::new(),
            name: name.into(),
            url_template: url_template.into(),
            home_url: None,
            category,
            encode_plus: false,
            enabled: true,
            builtin: false,
            usage_count: 0,
        }
    }

    /// Validates and normalizes all fields of this definition in place.
    pub fn validate_and_normalize(&mut self) -> Result<(), BangError> {
        self.trigger = validate_trigger(&self.trigger)?;

        let mut normalized_aliases = Vec::with_capacity(self.aliases.len());
        for alias in &self.aliases {
            let norm = validate_trigger(alias)?;
            if norm == self.trigger || normalized_aliases.contains(&norm) {
                return Err(BangError::DuplicateTrigger { trigger: norm });
            }
            normalized_aliases.push(norm);
        }
        self.aliases = normalized_aliases;

        let trimmed_name = self.name.trim();
        if trimmed_name.is_empty() {
            return Err(BangError::EmptyName);
        }
        self.name = trimmed_name.to_string();

        validate_url_template(&self.url_template)?;

        if let Some(ref home) = self.home_url {
            let trimmed = home.trim();
            if trimmed.is_empty() {
                self.home_url = None;
            } else {
                validate_http_url(trimmed).map_err(|reason| BangError::InvalidHomeUrl {
                    url: trimmed.to_string(),
                    reason,
                })?;
                self.home_url = Some(trimmed.to_string());
            }
        }

        Ok(())
    }
}

/// Options controlling how bang tokens are detected in user input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BangParseOptions {
    /// Prefix character introducing a bang token (default `'!'`).
    pub prefix_char: char,
    /// When true, only the first token in the input is checked for a bang.
    pub prefix_only: bool,
    /// When true, trigger matching is case-sensitive.
    pub case_sensitive: bool,
}

impl Default for BangParseOptions {
    fn default() -> Self {
        Self {
            prefix_char: '!',
            prefix_only: false,
            case_sensitive: false,
        }
    }
}

/// Result of resolving a bang inside a search query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BangMatch {
    /// The trigger or alias token that matched in the input (without prefix char).
    pub matched_trigger: String,
    /// The canonical trigger of the matched bang definition.
    pub canonical_trigger: String,
    /// Human-readable engine name.
    pub bang_name: String,
    /// Remaining search query with the bang token removed.
    pub remaining_query: String,
    /// Final expanded URL ready for navigation.
    pub resolved_url: String,
    /// True when `remaining_query` was empty and `home_url` was used instead.
    pub used_home_fallback: bool,
}

/// Autocomplete suggestion for a bang prefix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BangSuggestion {
    pub trigger: String,
    pub name: String,
    pub category: BangCategory,
    pub matched_alias: Option<String>,
    pub usage_count: u64,
}

/// Registry managing built-in and user-defined search engine bangs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BangRegistry {
    /// Canonical trigger -> definition.
    bangs: HashMap<String, BangDefinition>,
    /// Lowercase alias -> canonical trigger.
    alias_index: HashMap<String, String>,
}

impl BangRegistry {
    /// Creates an empty bang registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a registry pre-populated with common built-in search bangs.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        for def in default_builtin_bangs() {
            // Built-in definitions are static and valid; ignore any unexpected error defensively.
            let _ = registry.register(def);
        }
        registry
    }

    /// Returns the number of registered bang definitions.
    pub fn len(&self) -> usize {
        self.bangs.len()
    }

    /// Returns true if no bangs are registered.
    pub fn is_empty(&self) -> bool {
        self.bangs.is_empty()
    }

    /// Registers a new bang definition after validating uniqueness across triggers and aliases.
    pub fn register(&mut self, mut def: BangDefinition) -> Result<(), BangError> {
        def.validate_and_normalize()?;

        if self.bangs.contains_key(&def.trigger) || self.alias_index.contains_key(&def.trigger) {
            return Err(BangError::DuplicateTrigger {
                trigger: def.trigger,
            });
        }
        for alias in &def.aliases {
            if self.bangs.contains_key(alias) || self.alias_index.contains_key(alias) {
                return Err(BangError::DuplicateTrigger {
                    trigger: alias.clone(),
                });
            }
        }

        for alias in &def.aliases {
            self.alias_index.insert(alias.clone(), def.trigger.clone());
        }
        self.bangs.insert(def.trigger.clone(), def);
        Ok(())
    }

    /// Inserts or replaces a custom (non-builtin) bang definition.
    pub fn upsert_custom(&mut self, mut def: BangDefinition) -> Result<(), BangError> {
        def.builtin = false;
        def.validate_and_normalize()?;

        if let Some(existing) = self.bangs.get(&def.trigger) {
            if existing.builtin {
                return Err(BangError::BuiltinProtected {
                    trigger: def.trigger,
                });
            }
        }
        if let Some(owner) = self.alias_index.get(&def.trigger) {
            if owner != &def.trigger {
                return Err(BangError::DuplicateTrigger {
                    trigger: def.trigger,
                });
            }
        }

        for alias in &def.aliases {
            if let Some(existing_owner) = self.alias_index.get(alias) {
                if existing_owner != &def.trigger {
                    return Err(BangError::DuplicateTrigger {
                        trigger: alias.clone(),
                    });
                }
            }
            if self.bangs.contains_key(alias) {
                return Err(BangError::DuplicateTrigger {
                    trigger: alias.clone(),
                });
            }
        }

        // Remove old aliases if updating an existing custom bang.
        if let Some(old) = self.bangs.remove(&def.trigger) {
            for old_alias in old.aliases {
                self.alias_index.remove(&old_alias);
            }
        }

        for alias in &def.aliases {
            self.alias_index.insert(alias.clone(), def.trigger.clone());
        }
        self.bangs.insert(def.trigger.clone(), def);
        Ok(())
    }

    /// Removes a custom bang by trigger. Built-in bangs cannot be removed.
    pub fn remove(&mut self, trigger: &str) -> Result<BangDefinition, BangError> {
        let norm = validate_trigger(trigger)?;
        let canonical = self
            .alias_index
            .get(&norm)
            .cloned()
            .unwrap_or_else(|| norm.clone());

        match self.bangs.get(&canonical) {
            None => Err(BangError::BangNotFound { trigger: norm }),
            Some(def) if def.builtin => Err(BangError::BuiltinProtected { trigger: canonical }),
            Some(_) => {
                let removed =
                    self.bangs
                        .remove(&canonical)
                        .ok_or_else(|| BangError::BangNotFound {
                            trigger: canonical.clone(),
                        })?;
                for alias in &removed.aliases {
                    self.alias_index.remove(alias);
                }
                Ok(removed)
            }
        }
    }

    /// Enables or disables a bang by trigger or alias.
    pub fn set_enabled(&mut self, trigger: &str, enabled: bool) -> Result<(), BangError> {
        let norm = validate_trigger(trigger)?;
        let canonical = self
            .alias_index
            .get(&norm)
            .cloned()
            .unwrap_or_else(|| norm.clone());
        let def = self
            .bangs
            .get_mut(&canonical)
            .ok_or(BangError::BangNotFound { trigger: norm })?;
        def.enabled = enabled;
        Ok(())
    }

    /// Looks up a bang definition by canonical trigger or alias.
    pub fn get(&self, trigger: &str) -> Option<&BangDefinition> {
        let norm = validate_trigger(trigger).ok()?;
        if let Some(def) = self.bangs.get(&norm) {
            return Some(def);
        }
        let canonical = self.alias_index.get(&norm)?;
        self.bangs.get(canonical)
    }

    /// Lists all enabled bangs in a given category, sorted by trigger.
    pub fn list_by_category(&self, category: BangCategory) -> Vec<&BangDefinition> {
        let mut items: Vec<&BangDefinition> = self
            .bangs
            .values()
            .filter(|def| def.enabled && def.category == category)
            .collect();
        items.sort_by(|a, b| a.trigger.cmp(&b.trigger));
        items
    }

    /// Resolves a query string against registered enabled bangs without mutating usage counts.
    pub fn resolve(&self, input: &str, options: &BangParseOptions) -> Option<BangMatch> {
        let tokens: Vec<&str> = input.split_whitespace().collect();
        if tokens.is_empty() {
            return None;
        }

        let max_idx = if options.prefix_only { 1 } else { tokens.len() };
        for (idx, token) in tokens.iter().enumerate().take(max_idx) {
            let Some(raw_trigger) = token.strip_prefix(options.prefix_char) else {
                continue;
            };
            if raw_trigger.is_empty() {
                continue;
            }

            let lookup = if options.case_sensitive {
                raw_trigger.to_string()
            } else {
                raw_trigger.to_ascii_lowercase()
            };

            let Some(def) = self.lookup_enabled(&lookup, options.case_sensitive) else {
                continue;
            };

            let mut remaining_parts = Vec::with_capacity(tokens.len().saturating_sub(1));
            for (other_idx, other_tok) in tokens.iter().enumerate() {
                if other_idx != idx {
                    remaining_parts.push(*other_tok);
                }
            }
            let remaining_query = remaining_parts.join(" ");

            let (resolved_url, used_home_fallback) =
                if remaining_query.is_empty() && def.home_url.is_some() {
                    (def.home_url.clone().unwrap_or_default(), true)
                } else {
                    let encoded = encode_query_component(&remaining_query, def.encode_plus);
                    (def.url_template.replace(QUERY_PLACEHOLDER, &encoded), false)
                };

            return Some(BangMatch {
                matched_trigger: raw_trigger.to_string(),
                canonical_trigger: def.trigger.clone(),
                bang_name: def.name.clone(),
                remaining_query,
                resolved_url,
                used_home_fallback,
            });
        }

        None
    }

    /// Resolves a query string and increments the matched bang's `usage_count`.
    pub fn resolve_and_record(
        &mut self,
        input: &str,
        options: &BangParseOptions,
    ) -> Option<BangMatch> {
        let matched = self.resolve(input, options)?;
        if let Some(def) = self.bangs.get_mut(&matched.canonical_trigger) {
            def.usage_count = def.usage_count.saturating_add(1);
        }
        Some(matched)
    }

    /// Suggests enabled bangs matching `prefix` (with or without leading `!`).
    pub fn suggest(&self, prefix: &str, limit: usize) -> Vec<BangSuggestion> {
        if limit == 0 {
            return Vec::new();
        }
        let cleaned = prefix
            .trim()
            .strip_prefix('!')
            .unwrap_or(prefix.trim())
            .to_ascii_lowercase();

        let mut matches: Vec<(bool, &BangDefinition, Option<String>)> = Vec::new();
        for def in self.bangs.values() {
            if !def.enabled {
                continue;
            }
            if cleaned.is_empty() || def.trigger.starts_with(&cleaned) {
                let exact = def.trigger == cleaned;
                matches.push((exact, def, None));
            } else if let Some(alias) = def.aliases.iter().find(|a| a.starts_with(&cleaned)) {
                let exact = alias == &cleaned;
                matches.push((exact, def, Some(alias.clone())));
            }
        }

        matches.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| b.1.usage_count.cmp(&a.1.usage_count))
                .then_with(|| a.1.trigger.cmp(&b.1.trigger))
        });

        matches
            .into_iter()
            .take(limit)
            .map(|(_, def, matched_alias)| BangSuggestion {
                trigger: def.trigger.clone(),
                name: def.name.clone(),
                category: def.category,
                matched_alias,
                usage_count: def.usage_count,
            })
            .collect()
    }

    /// Exports all registered bangs as pretty-printed JSON sorted by trigger.
    pub fn export_json(&self) -> Result<String, BangError> {
        let mut defs: Vec<&BangDefinition> = self.bangs.values().collect();
        defs.sort_by(|a, b| a.trigger.cmp(&b.trigger));
        Ok(serde_json::to_string_pretty(&defs)?)
    }

    /// Imports bang definitions from a JSON array, returning how many were applied.
    pub fn import_json(&mut self, json: &str, overwrite_custom: bool) -> Result<usize, BangError> {
        let defs: Vec<BangDefinition> = serde_json::from_str(json)?;
        let mut applied = 0usize;
        for def in defs {
            if overwrite_custom && !def.builtin {
                self.upsert_custom(def)?;
                applied += 1;
            } else if self.get(&def.trigger).is_none() {
                self.register(def)?;
                applied += 1;
            }
        }
        Ok(applied)
    }

    fn lookup_enabled(&self, key: &str, case_sensitive: bool) -> Option<&BangDefinition> {
        let _ = case_sensitive;
        if let Some(def) = self.bangs.get(key) {
            return def.enabled.then_some(def);
        }
        let canonical = self.alias_index.get(key)?;
        self.bangs.get(canonical).filter(|def| def.enabled)
    }
}

/// Validates and normalizes a bang trigger or alias.
pub fn validate_trigger(raw: &str) -> Result<String, BangError> {
    let trimmed = raw.trim();
    let stripped = trimmed.strip_prefix('!').unwrap_or(trimmed);
    if stripped.is_empty() {
        return Err(BangError::EmptyTrigger);
    }
    if stripped.len() > MAX_TRIGGER_LEN {
        return Err(BangError::InvalidTrigger {
            trigger: stripped.to_string(),
            reason: "exceeds maximum length of 32 characters",
        });
    }
    if !stripped
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(BangError::InvalidTrigger {
            trigger: stripped.to_string(),
            reason: "only ASCII alphanumeric, '-', '_', and '.' are allowed",
        });
    }
    Ok(stripped.to_ascii_lowercase())
}

/// Validates that a URL template contains `{query}` and produces a valid HTTP(S) URL.
pub fn validate_url_template(template: &str) -> Result<(), BangError> {
    let trimmed = template.trim();
    if trimmed.is_empty() {
        return Err(BangError::InvalidUrlTemplate {
            template: template.to_string(),
            reason: "template cannot be empty".to_string(),
        });
    }
    if !trimmed.contains(QUERY_PLACEHOLDER) {
        return Err(BangError::InvalidUrlTemplate {
            template: template.to_string(),
            reason: "template must contain {query} placeholder".to_string(),
        });
    }
    let sample = trimmed.replace(QUERY_PLACEHOLDER, "search");
    validate_http_url(&sample).map_err(|reason| BangError::InvalidUrlTemplate {
        template: template.to_string(),
        reason,
    })
}

fn validate_http_url(raw: &str) -> Result<(), String> {
    let parsed = Url::parse(raw).map_err(|e| e.to_string())?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => return Err(format!("unsupported URL scheme '{other}'")),
    }
    if parsed.host_str().is_none() {
        return Err("URL must include a host".to_string());
    }
    Ok(())
}

/// Percent-encodes a search query component according to RFC 3986 (or `+` for spaces).
pub fn encode_query_component(query: &str, encode_plus: bool) -> String {
    let mut out = String::with_capacity(query.len());
    for byte in query.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            b' ' if encode_plus => out.push('+'),
            other => {
                let _ = write!(out, "%{other:02X}");
            }
        }
    }
    out
}

fn builtin(
    trigger: &str,
    aliases: &[&str],
    name: &str,
    url_template: &str,
    home_url: &str,
    category: BangCategory,
    encode_plus: bool,
) -> BangDefinition {
    BangDefinition {
        trigger: trigger.to_string(),
        aliases: aliases.iter().map(|s| (*s).to_string()).collect(),
        name: name.to_string(),
        url_template: url_template.to_string(),
        home_url: Some(home_url.to_string()),
        category,
        encode_plus,
        enabled: true,
        builtin: true,
        usage_count: 0,
    }
}

fn default_builtin_bangs() -> Vec<BangDefinition> {
    vec![
        builtin(
            "g",
            &["google"],
            "Google",
            "https://www.google.com/search?q={query}",
            "https://www.google.com/",
            BangCategory::WebSearch,
            true,
        ),
        builtin(
            "ddg",
            &["duck"],
            "DuckDuckGo",
            "https://duckduckgo.com/?q={query}",
            "https://duckduckgo.com/",
            BangCategory::WebSearch,
            true,
        ),
        builtin(
            "w",
            &["wiki", "wikipedia"],
            "Wikipedia",
            "https://en.wikipedia.org/w/index.php?search={query}",
            "https://en.wikipedia.org/",
            BangCategory::Encyclopedia,
            true,
        ),
        builtin(
            "gh",
            &["github"],
            "GitHub",
            "https://github.com/search?q={query}",
            "https://github.com/",
            BangCategory::Code,
            true,
        ),
        builtin(
            "rs",
            &["docsrs"],
            "docs.rs",
            "https://docs.rs/releases/search?query={query}",
            "https://docs.rs/",
            BangCategory::Documentation,
            true,
        ),
        builtin(
            "crates",
            &["cargo"],
            "crates.io",
            "https://crates.io/search?q={query}",
            "https://crates.io/",
            BangCategory::PackageRegistry,
            false,
        ),
        builtin(
            "yt",
            &["youtube"],
            "YouTube",
            "https://www.youtube.com/results?search_query={query}",
            "https://www.youtube.com/",
            BangCategory::Media,
            true,
        ),
        builtin(
            "mdn",
            &[],
            "MDN Web Docs",
            "https://developer.mozilla.org/en-US/search?q={query}",
            "https://developer.mozilla.org/",
            BangCategory::Documentation,
            true,
        ),
        builtin(
            "arch",
            &["archwiki"],
            "ArchWiki",
            "https://wiki.archlinux.org/index.php?search={query}",
            "https://wiki.archlinux.org/",
            BangCategory::Documentation,
            true,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_prefix_suffix_and_middle_bangs() {
        let mut registry = BangRegistry::with_defaults();
        let opts = BangParseOptions::default();

        let prefix = registry
            .resolve_and_record("!g rust async traits", &opts)
            .expect("prefix bang should resolve");
        assert_eq!(prefix.canonical_trigger, "g");
        assert_eq!(prefix.remaining_query, "rust async traits");
        assert_eq!(
            prefix.resolved_url,
            "https://www.google.com/search?q=rust+async+traits"
        );
        assert!(!prefix.used_home_fallback);

        let suffix = registry
            .resolve("tokio runtime !crates", &opts)
            .expect("suffix bang should resolve");
        assert_eq!(suffix.canonical_trigger, "crates");
        assert_eq!(suffix.remaining_query, "tokio runtime");
        assert_eq!(
            suffix.resolved_url,
            "https://crates.io/search?q=tokio%20runtime"
        );

        let alias_mid = registry
            .resolve("linux !wiki kernel", &opts)
            .expect("middle alias bang should resolve");
        assert_eq!(alias_mid.canonical_trigger, "w");
        assert_eq!(alias_mid.matched_trigger, "wiki");
        assert_eq!(alias_mid.remaining_query, "linux kernel");

        assert_eq!(registry.get("g").expect("g exists").usage_count, 1);
    }

    #[test]
    fn empty_query_uses_home_fallback_and_prefix_only_respected() {
        let registry = BangRegistry::with_defaults();

        let home = registry
            .resolve("!gh", &BangParseOptions::default())
            .expect("bare bang should resolve to home URL");
        assert!(home.used_home_fallback);
        assert_eq!(home.resolved_url, "https://github.com/");
        assert!(home.remaining_query.is_empty());

        let prefix_only = BangParseOptions {
            prefix_only: true,
            ..BangParseOptions::default()
        };
        assert!(registry.resolve("rust !gh", &prefix_only).is_none());
        assert!(registry.resolve("!gh rust", &prefix_only).is_some());
    }

    #[test]
    fn custom_bang_validation_protection_and_suggestions() {
        let mut registry = BangRegistry::with_defaults();

        let bad_template = BangDefinition::custom(
            "bad",
            "Bad Engine",
            "https://example.com/search",
            BangCategory::Custom,
        );
        assert!(matches!(
            registry.register(bad_template),
            Err(BangError::InvalidUrlTemplate { .. })
        ));

        let bad_trigger = BangDefinition::custom(
            "bad bang",
            "Bad Trigger",
            "https://example.com/?q={query}",
            BangCategory::Custom,
        );
        assert!(matches!(
            registry.register(bad_trigger),
            Err(BangError::InvalidTrigger { .. })
        ));

        let mut custom = BangDefinition::custom(
            "aur",
            "Arch User Repository",
            "https://aur.archlinux.org/packages?K={query}",
            BangCategory::PackageRegistry,
        );
        custom.aliases = vec!["aurpkg".to_string()];
        registry.register(custom).expect("custom bang registers");

        assert!(matches!(
            registry.remove("g"),
            Err(BangError::BuiltinProtected { .. })
        ));

        let suggestions = registry.suggest("!ar", 5);
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].trigger, "arch");

        let removed = registry.remove("aurpkg").expect("remove via alias");
        assert_eq!(removed.trigger, "aur");
        assert!(registry.get("aur").is_none());
    }

    #[test]
    fn json_roundtrip_and_disable_toggle() {
        let mut registry = BangRegistry::with_defaults();
        let custom = BangDefinition::custom(
            "lob",
            "Lobsters",
            "https://lobste.rs/search?q={query}",
            BangCategory::Code,
        );
        registry.register(custom).expect("register custom");
        registry.set_enabled("lob", false).expect("disable lob");
        assert!(registry
            .resolve("!lob rust", &BangParseOptions::default())
            .is_none());

        let exported = registry.export_json().expect("export json");
        let mut fresh = BangRegistry::new();
        let count = fresh.import_json(&exported, true).expect("import json");
        assert_eq!(count, registry.len());
        assert!(fresh.get("lob").is_some());
    }
}
