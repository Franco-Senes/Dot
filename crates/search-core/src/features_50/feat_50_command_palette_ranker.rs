//! Command palette fuzzy ranker with frecency decay, contextual boosting,
//! match-index highlighting, validation, and JSON serialization.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors returned by command palette registration, configuration, and ranking operations.
#[derive(Debug, Error)]
pub enum RankerError {
    #[error("command id must not be empty")]
    EmptyCommandId,

    #[error("command title must not be empty")]
    EmptyCommandTitle,

    #[error("duplicate command id: {0}")]
    DuplicateCommandId(String),

    #[error("command not found: {0}")]
    CommandNotFound(String),

    #[error("invalid ranker configuration: {0}")]
    InvalidConfig(String),

    #[error("invalid unix timestamp: {0}")]
    InvalidTimestamp(i64),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// High-level category for grouping and contextual boosting of palette commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandCategory {
    Navigation,
    Tab,
    Bookmark,
    History,
    Settings,
    Extension,
    Developer,
    Window,
    Search,
    Action,
}

/// Runtime application context used to filter and boost commands.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandContext {
    pub active_category: Option<CommandCategory>,
    pub is_incognito: bool,
    pub has_selection: bool,
    pub active_tab_domain: Option<String>,
    pub open_tab_count: usize,
}

/// Definition of a command available in the command palette.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandItem {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub category: CommandCategory,
    pub keywords: Vec<String>,
    pub shortcut: Option<String>,
    pub enabled: bool,
    pub requires_selection: bool,
    pub hidden_in_incognito: bool,
    pub base_priority: f64,
}

impl CommandItem {
    /// Creates a new enabled command with default optional fields.
    pub fn new(id: impl Into<String>, title: impl Into<String>, category: CommandCategory) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            subtitle: None,
            category,
            keywords: Vec::new(),
            shortcut: None,
            enabled: true,
            requires_selection: false,
            hidden_in_incognito: false,
            base_priority: 0.0,
        }
    }

    /// Validates that required fields are non-empty and numeric priority is finite.
    pub fn validate(&self) -> Result<(), RankerError> {
        if self.id.trim().is_empty() {
            return Err(RankerError::EmptyCommandId);
        }
        if self.title.trim().is_empty() {
            return Err(RankerError::EmptyCommandTitle);
        }
        if !self.base_priority.is_finite() {
            return Err(RankerError::InvalidConfig(
                "command base_priority must be finite".to_string(),
            ));
        }
        Ok(())
    }
}

/// Persisted usage statistics for frecency scoring and pinning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageRecord {
    pub command_id: String,
    pub use_count: u64,
    pub last_used_unix_secs: i64,
    pub pinned: bool,
}

/// Tunable weights and thresholds for the command palette ranker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RankerConfig {
    pub max_results: usize,
    pub title_weight: f64,
    pub keyword_weight: f64,
    pub subtitle_weight: f64,
    pub id_weight: f64,
    pub frecency_weight: f64,
    pub context_boost: f64,
    pub pinned_boost: f64,
    pub half_life_secs: f64,
    pub include_disabled: bool,
    pub min_score_threshold: f64,
}

impl Default for RankerConfig {
    fn default() -> Self {
        Self {
            max_results: 25,
            title_weight: 100.0,
            keyword_weight: 65.0,
            subtitle_weight: 45.0,
            id_weight: 25.0,
            frecency_weight: 20.0,
            context_boost: 15.0,
            pinned_boost: 35.0,
            half_life_secs: 7.0 * 24.0 * 3600.0,
            include_disabled: false,
            min_score_threshold: 0.0,
        }
    }
}

impl RankerConfig {
    /// Validates that weights are finite and limits/half-life are positive.
    pub fn validate(&self) -> Result<(), RankerError> {
        if self.max_results == 0 {
            return Err(RankerError::InvalidConfig(
                "max_results must be greater than zero".to_string(),
            ));
        }
        if !self.half_life_secs.is_finite() || self.half_life_secs <= 0.0 {
            return Err(RankerError::InvalidConfig(
                "half_life_secs must be finite and positive".to_string(),
            ));
        }
        let weights = [
            ("title_weight", self.title_weight),
            ("keyword_weight", self.keyword_weight),
            ("subtitle_weight", self.subtitle_weight),
            ("id_weight", self.id_weight),
            ("frecency_weight", self.frecency_weight),
            ("context_boost", self.context_boost),
            ("pinned_boost", self.pinned_boost),
            ("min_score_threshold", self.min_score_threshold),
        ];
        for (name, val) in weights {
            if !val.is_finite() {
                return Err(RankerError::InvalidConfig(format!("{name} must be finite")));
            }
        }
        Ok(())
    }
}

/// Primary field where the query matched a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchField {
    Title,
    Keyword,
    Subtitle,
    Id,
    None,
}

/// A ranked command returned by a palette query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RankedCommand {
    pub command_id: String,
    pub title: String,
    pub category: CommandCategory,
    pub score: f64,
    pub matched_field: MatchField,
    pub title_match_indices: Vec<usize>,
    pub enabled: bool,
    pub pinned: bool,
}

/// Stateful command palette registry and ranker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandPaletteRanker {
    config: RankerConfig,
    commands: HashMap<String, CommandItem>,
    usage: HashMap<String, UsageRecord>,
}

impl CommandPaletteRanker {
    /// Creates a new ranker with the given configuration.
    pub fn new(config: RankerConfig) -> Result<Self, RankerError> {
        config.validate()?;
        Ok(Self {
            config,
            commands: HashMap::new(),
            usage: HashMap::new(),
        })
    }

    /// Returns the active configuration.
    pub fn config(&self) -> &RankerConfig {
        &self.config
    }

    /// Updates the ranker configuration after validation.
    pub fn set_config(&mut self, config: RankerConfig) -> Result<(), RankerError> {
        config.validate()?;
        self.config = config;
        Ok(())
    }

    /// Registers a new command, failing if a command with the same ID already exists.
    pub fn register_command(&mut self, item: CommandItem) -> Result<(), RankerError> {
        item.validate()?;
        if self.commands.contains_key(&item.id) {
            return Err(RankerError::DuplicateCommandId(item.id));
        }
        self.commands.insert(item.id.clone(), item);
        Ok(())
    }

    /// Inserts or replaces a command definition.
    pub fn upsert_command(&mut self, item: CommandItem) -> Result<(), RankerError> {
        item.validate()?;
        self.commands.insert(item.id.clone(), item);
        Ok(())
    }

    /// Removes a command and its associated usage record.
    pub fn remove_command(&mut self, command_id: &str) -> Result<CommandItem, RankerError> {
        let removed = self
            .commands
            .remove(command_id)
            .ok_or_else(|| RankerError::CommandNotFound(command_id.to_string()))?;
        self.usage.remove(command_id);
        Ok(removed)
    }

    /// Looks up a registered command by ID.
    pub fn get_command(&self, command_id: &str) -> Option<&CommandItem> {
        self.commands.get(command_id)
    }

    /// Looks up usage statistics for a command by ID.
    pub fn get_usage(&self, command_id: &str) -> Option<&UsageRecord> {
        self.usage.get(command_id)
    }

    /// Records a command execution at `timestamp_unix_secs` for frecency scoring.
    pub fn record_usage(
        &mut self,
        command_id: &str,
        timestamp_unix_secs: i64,
    ) -> Result<(), RankerError> {
        if !self.commands.contains_key(command_id) {
            return Err(RankerError::CommandNotFound(command_id.to_string()));
        }
        if timestamp_unix_secs < 0 {
            return Err(RankerError::InvalidTimestamp(timestamp_unix_secs));
        }

        let entry = self
            .usage
            .entry(command_id.to_string())
            .or_insert_with(|| UsageRecord {
                command_id: command_id.to_string(),
                use_count: 0,
                last_used_unix_secs: timestamp_unix_secs,
                pinned: false,
            });
        entry.use_count = entry.use_count.saturating_add(1);
        if timestamp_unix_secs > entry.last_used_unix_secs {
            entry.last_used_unix_secs = timestamp_unix_secs;
        }
        Ok(())
    }

    /// Pins or unpins a registered command.
    pub fn set_pinned(&mut self, command_id: &str, pinned: bool) -> Result<(), RankerError> {
        if !self.commands.contains_key(command_id) {
            return Err(RankerError::CommandNotFound(command_id.to_string()));
        }
        let entry = self
            .usage
            .entry(command_id.to_string())
            .or_insert_with(|| UsageRecord {
                command_id: command_id.to_string(),
                use_count: 0,
                last_used_unix_secs: 0,
                pinned,
            });
        entry.pinned = pinned;
        Ok(())
    }

    /// Serializes the ranker state (config, commands, usage) to JSON.
    pub fn to_json(&self) -> Result<String, RankerError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Restores a ranker from JSON and validates its configuration and commands.
    pub fn from_json(json: &str) -> Result<Self, RankerError> {
        let ranker: Self = serde_json::from_str(json)?;
        ranker.config.validate()?;
        for item in ranker.commands.values() {
            item.validate()?;
        }
        Ok(ranker)
    }

    /// Ranks registered commands against `query` in the given `context` at `now_unix_secs`.
    pub fn rank(
        &self,
        query: &str,
        context: &CommandContext,
        now_unix_secs: i64,
    ) -> Vec<RankedCommand> {
        let tokens: Vec<String> = query
            .split_whitespace()
            .map(|t| t.to_lowercase())
            .filter(|t| !t.is_empty())
            .collect();

        let mut results = Vec::new();

        for item in self.commands.values() {
            if context.is_incognito && item.hidden_in_incognito {
                continue;
            }

            let effective_enabled =
                item.enabled && (!item.requires_selection || context.has_selection);
            if !effective_enabled && !self.config.include_disabled {
                continue;
            }

            let usage = self.usage.get(&item.id);
            let pinned = usage.map(|u| u.pinned).unwrap_or(false);

            let (match_score, matched_field, title_match_indices) = if tokens.is_empty() {
                (0.0, MatchField::None, Vec::new())
            } else {
                match self.score_tokens(item, &tokens) {
                    Some(scored) => scored,
                    None => continue,
                }
            };

            let frecency_score = usage
                .map(|u| self.compute_frecency(u, now_unix_secs))
                .unwrap_or(0.0);

            let context_score = if context.active_category == Some(item.category) {
                self.config.context_boost
            } else {
                0.0
            };

            let pinned_score = if pinned {
                self.config.pinned_boost
            } else {
                0.0
            };

            let total_score =
                match_score + frecency_score + context_score + pinned_score + item.base_priority;

            if total_score < self.config.min_score_threshold {
                continue;
            }

            results.push(RankedCommand {
                command_id: item.id.clone(),
                title: item.title.clone(),
                category: item.category,
                score: total_score,
                matched_field,
                title_match_indices,
                enabled: effective_enabled,
                pinned,
            });
        }

        results.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| b.pinned.cmp(&a.pinned))
                .then_with(|| b.enabled.cmp(&a.enabled))
                .then_with(|| a.title.cmp(&b.title))
                .then_with(|| a.command_id.cmp(&b.command_id))
        });

        results.truncate(self.config.max_results);
        results
    }

    fn compute_frecency(&self, record: &UsageRecord, now_unix_secs: i64) -> f64 {
        if record.use_count == 0 {
            return 0.0;
        }
        let age_secs = now_unix_secs
            .saturating_sub(record.last_used_unix_secs)
            .max(0) as f64;
        let decay = 0.5_f64.powf(age_secs / self.config.half_life_secs);
        let freq = (1.0 + record.use_count as f64).ln();
        freq * decay * self.config.frecency_weight
    }

    fn score_tokens(
        &self,
        item: &CommandItem,
        tokens: &[String],
    ) -> Option<(f64, MatchField, Vec<usize>)> {
        let mut total = 0.0;
        let mut best_field = MatchField::None;
        let mut best_field_score = -1.0_f64;
        let mut title_indices: Vec<usize> = Vec::new();

        for token in tokens {
            let mut token_best = None::<(f64, MatchField, Vec<usize>)>;

            if let Some(m) = fuzzy_match(&item.title, token) {
                let weighted = m.score * self.config.title_weight;
                token_best = Some((weighted, MatchField::Title, m.indices));
            }

            for kw in &item.keywords {
                if let Some(m) = fuzzy_match(kw, token) {
                    let weighted = m.score * self.config.keyword_weight;
                    let replace = token_best
                        .as_ref()
                        .map(|(best, _, _)| weighted > *best)
                        .unwrap_or(true);
                    if replace {
                        token_best = Some((weighted, MatchField::Keyword, Vec::new()));
                    }
                }
            }

            if let Some(subtitle) = &item.subtitle {
                if let Some(m) = fuzzy_match(subtitle, token) {
                    let weighted = m.score * self.config.subtitle_weight;
                    let replace = token_best
                        .as_ref()
                        .map(|(best, _, _)| weighted > *best)
                        .unwrap_or(true);
                    if replace {
                        token_best = Some((weighted, MatchField::Subtitle, Vec::new()));
                    }
                }
            }

            if let Some(m) = fuzzy_match(&item.id, token) {
                let weighted = m.score * self.config.id_weight;
                let replace = token_best
                    .as_ref()
                    .map(|(best, _, _)| weighted > *best)
                    .unwrap_or(true);
                if replace {
                    token_best = Some((weighted, MatchField::Id, Vec::new()));
                }
            }

            let (score, field, indices) = token_best?;
            total += score;
            if score > best_field_score {
                best_field_score = score;
                best_field = field;
            }
            if field == MatchField::Title {
                for idx in indices {
                    if !title_indices.contains(&idx) {
                        title_indices.push(idx);
                    }
                }
            }
        }

        title_indices.sort_unstable();
        let avg_score = total / tokens.len() as f64;
        Some((avg_score, best_field, title_indices))
    }
}

struct FuzzyMatch {
    score: f64,
    indices: Vec<usize>,
}

/// Case-insensitive subsequence fuzzy matcher returning normalized score in `[0.0, 1.5]`
/// and character indices of matched characters in `target`.
fn fuzzy_match(target: &str, query_lower: &str) -> Option<FuzzyMatch> {
    if query_lower.is_empty() {
        return Some(FuzzyMatch {
            score: 0.0,
            indices: Vec::new(),
        });
    }

    let target_chars: Vec<char> = target.chars().collect();
    let target_lower: Vec<char> = target.chars().flat_map(|c| c.to_lowercase()).collect();
    let query_chars: Vec<char> = query_lower.chars().collect();

    if target_lower.len() != target_chars.len() {
        // Fallback for rare multi-char Unicode lowercase expansions: compare char-by-char on
        // lowercase strings directly.
        return fuzzy_match_chars(&target_lower, &query_chars);
    }

    fuzzy_match_with_boundaries(&target_chars, &target_lower, &query_chars)
}

fn fuzzy_match_chars(target_lower: &[char], query_chars: &[char]) -> Option<FuzzyMatch> {
    let mut indices = Vec::with_capacity(query_chars.len());
    let mut qi = 0;
    for (ti, &tc) in target_lower.iter().enumerate() {
        if qi < query_chars.len() && tc == query_chars[qi] {
            indices.push(ti);
            qi += 1;
        }
    }
    if qi < query_chars.len() {
        return None;
    }
    let ratio = query_chars.len() as f64 / target_lower.len().max(1) as f64;
    Some(FuzzyMatch {
        score: 0.5 + 0.5 * ratio,
        indices,
    })
}

fn fuzzy_match_with_boundaries(
    target_orig: &[char],
    target_lower: &[char],
    query_chars: &[char],
) -> Option<FuzzyMatch> {
    if query_chars.len() > target_lower.len() {
        return None;
    }

    let mut indices = Vec::with_capacity(query_chars.len());
    let mut qi = 0;
    for (ti, &tc) in target_lower.iter().enumerate() {
        if qi < query_chars.len() && tc == query_chars[qi] {
            indices.push(ti);
            qi += 1;
        }
    }

    if qi < query_chars.len() {
        return None;
    }

    // Exact match
    if target_lower == query_chars {
        return Some(FuzzyMatch {
            score: 1.5,
            indices,
        });
    }

    let mut score = 0.4;

    // Prefix match bonus
    if target_lower.starts_with(query_chars) {
        score += 0.45;
    }

    // Boundary and consecutive match bonuses
    let mut boundary_hits = 0usize;
    let mut consecutive_hits = 0usize;

    for (pos, &ti) in indices.iter().enumerate() {
        if is_word_boundary(target_orig, ti) {
            boundary_hits += 1;
        }
        if pos > 0 && ti == indices[pos - 1] + 1 {
            consecutive_hits += 1;
        }
    }

    let qlen = query_chars.len() as f64;
    score += 0.25 * (boundary_hits as f64 / qlen);
    if query_chars.len() > 1 {
        score += 0.2 * (consecutive_hits as f64 / (qlen - 1.0));
    }

    // Compactness bonus: tighter span and shorter target rank higher
    let span = indices
        .last()
        .and_then(|last| indices.first().map(|first| last - first + 1))
        .unwrap_or(1);
    let span_ratio = qlen / span as f64;
    let len_ratio = qlen / target_lower.len() as f64;
    score += 0.1 * span_ratio + 0.1 * len_ratio;

    Some(FuzzyMatch { score, indices })
}

fn is_word_boundary(chars: &[char], idx: usize) -> bool {
    if idx == 0 {
        return true;
    }
    let prev = chars[idx - 1];
    let curr = chars[idx];
    if matches!(prev, ' ' | '-' | '_' | ':' | '/' | '.') {
        return true;
    }
    prev.is_lowercase() && curr.is_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_ranker() -> CommandPaletteRanker {
        let mut ranker = CommandPaletteRanker::new(RankerConfig::default()).unwrap();

        let mut new_tab = CommandItem::new("tab.new", "New Tab", CommandCategory::Tab);
        new_tab.keywords = vec!["open".into(), "create".into()];
        new_tab.shortcut = Some("Ctrl+T".into());
        ranker.register_command(new_tab).unwrap();

        let mut close_tab = CommandItem::new("tab.close", "Close Tab", CommandCategory::Tab);
        close_tab.keywords = vec!["remove".into(), "exit".into()];
        ranker.register_command(close_tab).unwrap();

        let mut dev_tools = CommandItem::new(
            "dev.inspector",
            "Toggle Developer Tools",
            CommandCategory::Developer,
        );
        dev_tools.subtitle = Some("Inspect DOM and network requests".into());
        dev_tools.keywords = vec!["devtools".into(), "console".into()];
        ranker.register_command(dev_tools).unwrap();

        let mut copy_sel = CommandItem::new(
            "edit.copy_selection",
            "Copy Selection",
            CommandCategory::Action,
        );
        copy_sel.requires_selection = true;
        ranker.register_command(copy_sel).unwrap();

        let mut history = CommandItem::new(
            "history.clear",
            "Clear Browsing History",
            CommandCategory::History,
        );
        history.hidden_in_incognito = true;
        ranker.register_command(history).unwrap();

        ranker
    }

    #[test]
    fn fuzzy_matching_and_highlight_indices_happy_path() {
        let ranker = sample_ranker();
        let ctx = CommandContext::default();

        let results = ranker.rank("nt", &ctx, 1_000);
        assert!(!results.is_empty());
        assert_eq!(results[0].command_id, "tab.new");
        assert_eq!(results[0].matched_field, MatchField::Title);
        // 'N' at 0, 'T' at 4 in "New Tab"
        assert_eq!(results[0].title_match_indices, vec![0, 4]);
    }

    #[test]
    fn multi_token_query_matches_across_title_and_keywords() {
        let ranker = sample_ranker();
        let ctx = CommandContext::default();

        let results = ranker.rank("tab create", &ctx, 1_000);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].command_id, "tab.new");

        let sub_results = ranker.rank("inspector network", &ctx, 1_000);
        assert_eq!(sub_results.len(), 1);
        assert_eq!(sub_results[0].command_id, "dev.inspector");
    }

    #[test]
    fn frecency_decay_and_pinning_affect_empty_query_order() {
        let mut ranker = sample_ranker();
        let ctx = CommandContext::default();

        // Older frequent command vs recent command
        ranker.record_usage("tab.close", 1_000).unwrap();
        ranker.record_usage("tab.close", 1_000).unwrap();
        // 14 days later (2 half-lives)
        let now = 1_000 + 14 * 24 * 3600;
        ranker.record_usage("dev.inspector", now - 10).unwrap();
        ranker.set_pinned("history.clear", true).unwrap();

        let results = ranker.rank("", &ctx, now);
        assert_eq!(results[0].command_id, "history.clear");
        assert!(results[0].pinned);
        // Recent dev.inspector outranks 2-week-old tab.close
        let dev_pos = results
            .iter()
            .position(|r| r.command_id == "dev.inspector")
            .unwrap();
        let close_pos = results
            .iter()
            .position(|r| r.command_id == "tab.close")
            .unwrap();
        assert!(dev_pos < close_pos);
    }

    #[test]
    fn context_filtering_and_category_boosting() {
        let ranker = sample_ranker();

        // Without selection and in incognito: copy_selection & history.clear excluded
        let incognito_ctx = CommandContext {
            active_category: Some(CommandCategory::Developer),
            is_incognito: true,
            has_selection: false,
            ..Default::default()
        };
        let results = ranker.rank("", &incognito_ctx, 1_000);
        assert!(!results.iter().any(|r| r.command_id == "history.clear"));
        assert!(!results
            .iter()
            .any(|r| r.command_id == "edit.copy_selection"));
        assert_eq!(results[0].command_id, "dev.inspector");

        // With selection enabled, copy_selection appears
        let sel_ctx = CommandContext {
            has_selection: true,
            ..Default::default()
        };
        let sel_results = ranker.rank("copy", &sel_ctx, 1_000);
        assert_eq!(sel_results.len(), 1);
        assert_eq!(sel_results[0].command_id, "edit.copy_selection");
    }

    #[test]
    fn validation_and_error_cases() {
        let bad_cfg = RankerConfig {
            max_results: 0,
            ..Default::default()
        };
        assert!(matches!(
            CommandPaletteRanker::new(bad_cfg),
            Err(RankerError::InvalidConfig(_))
        ));

        let mut ranker = CommandPaletteRanker::new(RankerConfig::default()).unwrap();
        assert!(matches!(
            ranker.register_command(CommandItem::new("", "Title", CommandCategory::Action)),
            Err(RankerError::EmptyCommandId)
        ));
        assert!(matches!(
            ranker.register_command(CommandItem::new("id.1", "   ", CommandCategory::Action)),
            Err(RankerError::EmptyCommandTitle)
        ));

        ranker
            .register_command(CommandItem::new("id.1", "One", CommandCategory::Action))
            .unwrap();
        assert!(matches!(
            ranker.register_command(CommandItem::new("id.1", "Dup", CommandCategory::Action)),
            Err(RankerError::DuplicateCommandId(_))
        ));
        assert!(matches!(
            ranker.record_usage("missing", 100),
            Err(RankerError::CommandNotFound(_))
        ));
        assert!(matches!(
            ranker.record_usage("id.1", -5),
            Err(RankerError::InvalidTimestamp(-5))
        ));
    }

    #[test]
    fn json_roundtrip_preserves_state() {
        let mut ranker = sample_ranker();
        ranker.record_usage("tab.new", 5_000).unwrap();
        ranker.set_pinned("tab.new", true).unwrap();

        let json = ranker.to_json().unwrap();
        let restored = CommandPaletteRanker::from_json(&json).unwrap();

        assert_eq!(restored.get_usage("tab.new").unwrap().use_count, 1);
        assert!(restored.get_usage("tab.new").unwrap().pinned);
        let results = restored.rank("new", &CommandContext::default(), 5_100);
        assert_eq!(results[0].command_id, "tab.new");
    }
}
