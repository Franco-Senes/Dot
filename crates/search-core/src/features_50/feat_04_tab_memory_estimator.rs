use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;

const BYTES_PER_MIB: f64 = 1024.0 * 1024.0;
const KIB: u64 = 1024;
const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * 1024 * 1024;

/// Errors produced by tab memory estimation, configuration validation, or serialization.
#[derive(Debug, Error, PartialEq)]
pub enum TabMemoryError {
    #[error("tab_id must not be empty")]
    EmptyTabId,
    #[error("duplicate tab_id in session: {0}")]
    DuplicateTabId(String),
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("invalid tab input for '{tab_id}': {reason}")]
    InvalidInput { tab_id: String, reason: String },
    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Lifecycle state of a browser tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TabLifecycleState {
    Active,
    Background,
    Throttled,
    Frozen,
    Discarded,
}

/// Coarse complexity profile of the document loaded in a tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentComplexity {
    Minimal,
    TextHeavy,
    RichApp,
    MediaStreaming,
    DevTools,
}

/// Classification of an individual tab's estimated memory footprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryTier {
    Lightweight,
    Moderate,
    Heavy,
    Critical,
}

/// Aggregate session memory pressure relative to the configured budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPressureLevel {
    Normal,
    Elevated,
    High,
    Critical,
}

/// Multipliers applied to heuristic memory based on tab lifecycle state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateMultipliers {
    pub active: f64,
    pub background: f64,
    pub throttled: f64,
    pub frozen: f64,
    pub discarded: f64,
}

impl Default for StateMultipliers {
    fn default() -> Self {
        Self {
            active: 1.0,
            background: 0.92,
            throttled: 0.78,
            frozen: 0.55,
            discarded: 0.0,
        }
    }
}

impl StateMultipliers {
    fn for_state(&self, state: TabLifecycleState) -> f64 {
        match state {
            TabLifecycleState::Active => self.active,
            TabLifecycleState::Background => self.background,
            TabLifecycleState::Throttled => self.throttled,
            TabLifecycleState::Frozen => self.frozen,
            TabLifecycleState::Discarded => self.discarded,
        }
    }

    fn validate(&self) -> Result<(), TabMemoryError> {
        let pairs = [
            ("active", self.active),
            ("background", self.background),
            ("throttled", self.throttled),
            ("frozen", self.frozen),
            ("discarded", self.discarded),
        ];
        for (name, val) in pairs {
            if !val.is_finite() || val < 0.0 {
                return Err(TabMemoryError::InvalidConfig(format!(
                    "state multiplier '{name}' must be finite and >= 0.0"
                )));
            }
        }
        Ok(())
    }
}

/// Base payload estimates (in bytes) for each content complexity profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComplexityBaselines {
    pub minimal_bytes: u64,
    pub text_heavy_bytes: u64,
    pub rich_app_bytes: u64,
    pub media_streaming_bytes: u64,
    pub devtools_bytes: u64,
}

impl Default for ComplexityBaselines {
    fn default() -> Self {
        Self {
            minimal_bytes: 6 * MIB,
            text_heavy_bytes: 22 * MIB,
            rich_app_bytes: 68 * MIB,
            media_streaming_bytes: 96 * MIB,
            devtools_bytes: 84 * MIB,
        }
    }
}

impl ComplexityBaselines {
    fn for_complexity(&self, complexity: ContentComplexity) -> u64 {
        match complexity {
            ContentComplexity::Minimal => self.minimal_bytes,
            ContentComplexity::TextHeavy => self.text_heavy_bytes,
            ContentComplexity::RichApp => self.rich_app_bytes,
            ContentComplexity::MediaStreaming => self.media_streaming_bytes,
            ContentComplexity::DevTools => self.devtools_bytes,
        }
    }
}

/// Thresholds (in bytes) separating `Lightweight`, `Moderate`, `Heavy`, and `Critical` tabs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TierThresholds {
    pub moderate_bytes: u64,
    pub heavy_bytes: u64,
    pub critical_bytes: u64,
}

impl Default for TierThresholds {
    fn default() -> Self {
        Self {
            moderate_bytes: 96 * MIB,
            heavy_bytes: 256 * MIB,
            critical_bytes: 512 * MIB,
        }
    }
}

impl TierThresholds {
    fn validate(&self) -> Result<(), TabMemoryError> {
        if self.moderate_bytes == 0 {
            return Err(TabMemoryError::InvalidConfig(
                "moderate_bytes must be > 0".to_string(),
            ));
        }
        if !(self.moderate_bytes <= self.heavy_bytes && self.heavy_bytes <= self.critical_bytes) {
            return Err(TabMemoryError::InvalidConfig(
                "tier thresholds must satisfy moderate <= heavy <= critical".to_string(),
            ));
        }
        Ok(())
    }

    fn classify(&self, bytes: u64) -> MemoryTier {
        if bytes >= self.critical_bytes {
            MemoryTier::Critical
        } else if bytes >= self.heavy_bytes {
            MemoryTier::Heavy
        } else if bytes >= self.moderate_bytes {
            MemoryTier::Moderate
        } else {
            MemoryTier::Lightweight
        }
    }
}

/// Configuration parameters for [`TabMemoryEstimator`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EstimatorConfig {
    pub base_tab_bytes: u64,
    pub discarded_stub_bytes: u64,
    pub dom_node_bytes: u64,
    pub iframe_overhead_bytes: u64,
    pub extension_hook_bytes: u64,
    pub media_buffer_bytes: u64,
    pub js_heap_multiplier: f64,
    pub observed_blend_weight: f64,
    pub system_budget_bytes: u64,
    pub state_multipliers: StateMultipliers,
    pub complexity_baselines: ComplexityBaselines,
    pub tier_thresholds: TierThresholds,
}

impl Default for EstimatorConfig {
    fn default() -> Self {
        Self {
            base_tab_bytes: 28 * MIB,
            discarded_stub_bytes: 512 * KIB,
            dom_node_bytes: 1_200,
            iframe_overhead_bytes: 6 * MIB,
            extension_hook_bytes: 2 * MIB,
            media_buffer_bytes: 32 * MIB,
            js_heap_multiplier: 1.35,
            observed_blend_weight: 0.75,
            system_budget_bytes: 4 * GIB,
            state_multipliers: StateMultipliers::default(),
            complexity_baselines: ComplexityBaselines::default(),
            tier_thresholds: TierThresholds::default(),
        }
    }
}

impl EstimatorConfig {
    /// Validates all configuration invariants.
    pub fn validate(&self) -> Result<(), TabMemoryError> {
        if self.base_tab_bytes == 0 {
            return Err(TabMemoryError::InvalidConfig(
                "base_tab_bytes must be > 0".to_string(),
            ));
        }
        if self.system_budget_bytes == 0 {
            return Err(TabMemoryError::InvalidConfig(
                "system_budget_bytes must be > 0".to_string(),
            ));
        }
        if !self.js_heap_multiplier.is_finite() || self.js_heap_multiplier < 1.0 {
            return Err(TabMemoryError::InvalidConfig(
                "js_heap_multiplier must be finite and >= 1.0".to_string(),
            ));
        }
        if !self.observed_blend_weight.is_finite()
            || !(0.0..=1.0).contains(&self.observed_blend_weight)
        {
            return Err(TabMemoryError::InvalidConfig(
                "observed_blend_weight must be within [0.0, 1.0]".to_string(),
            ));
        }
        self.state_multipliers.validate()?;
        self.tier_thresholds.validate()?;
        Ok(())
    }
}

/// Per-tab metrics used to estimate memory footprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabMemoryInput {
    pub tab_id: String,
    pub title: Option<String>,
    pub origin: Option<String>,
    pub state: TabLifecycleState,
    pub complexity: ContentComplexity,
    pub dom_nodes: u32,
    pub iframe_count: u16,
    pub active_media_streams: u16,
    pub extension_hooks: u16,
    pub js_heap_bytes: Option<u64>,
    pub observed_rss_bytes: Option<u64>,
    pub idle_seconds: u64,
    pub pinned: bool,
    pub audible: bool,
}

impl TabMemoryInput {
    /// Validates tab input fields.
    pub fn validate(&self) -> Result<(), TabMemoryError> {
        if self.tab_id.trim().is_empty() {
            return Err(TabMemoryError::EmptyTabId);
        }
        if let Some(rss) = self.observed_rss_bytes {
            if rss == 0 && self.state != TabLifecycleState::Discarded {
                return Err(TabMemoryError::InvalidInput {
                    tab_id: self.tab_id.clone(),
                    reason: "observed_rss_bytes cannot be 0 for a live tab".to_string(),
                });
            }
        }
        Ok(())
    }
}

/// Component-level breakdown of a tab's heuristic memory estimate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryBreakdown {
    pub base_bytes: u64,
    pub complexity_bytes: u64,
    pub dom_bytes: u64,
    pub iframe_bytes: u64,
    pub media_bytes: u64,
    pub extension_bytes: u64,
    pub js_overhead_bytes: u64,
}

/// Estimated memory profile and discard recommendation metadata for a single tab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabMemoryEstimate {
    pub tab_id: String,
    pub estimated_bytes: u64,
    pub estimated_mib: f64,
    pub calibrated_from_observed: bool,
    pub tier: MemoryTier,
    pub potential_savings_bytes: u64,
    pub discard_priority_score: f64,
    pub breakdown: MemoryBreakdown,
}

/// Candidate tab recommended for discarding to reclaim memory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscardRecommendation {
    pub tab_id: String,
    pub reclaimable_bytes: u64,
    pub priority_score: f64,
    pub tier: MemoryTier,
}

/// Aggregate memory report across all tabs in a browser session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionMemoryReport {
    pub total_estimated_bytes: u64,
    pub total_estimated_mib: f64,
    pub total_potential_savings_bytes: u64,
    pub tab_count: usize,
    pub live_tab_count: usize,
    pub discarded_tab_count: usize,
    pub pressure_level: MemoryPressureLevel,
    pub budget_utilization_ratio: f64,
    pub estimates: Vec<TabMemoryEstimate>,
}

/// Estimates per-tab and session-wide memory consumption for Search Linux.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TabMemoryEstimator {
    config: EstimatorConfig,
}

impl TabMemoryEstimator {
    /// Creates an estimator with a validated custom configuration.
    pub fn new(config: EstimatorConfig) -> Result<Self, TabMemoryError> {
        config.validate()?;
        Ok(Self { config })
    }

    /// Returns a reference to the active configuration.
    pub fn config(&self) -> &EstimatorConfig {
        &self.config
    }

    /// Estimates memory usage for a single tab.
    pub fn estimate_tab(
        &self,
        input: &TabMemoryInput,
    ) -> Result<TabMemoryEstimate, TabMemoryError> {
        input.validate()?;

        if input.state == TabLifecycleState::Discarded {
            let estimated = self.config.discarded_stub_bytes;
            return Ok(TabMemoryEstimate {
                tab_id: input.tab_id.clone(),
                estimated_bytes: estimated,
                estimated_mib: bytes_to_mib(estimated),
                calibrated_from_observed: false,
                tier: self.config.tier_thresholds.classify(estimated),
                potential_savings_bytes: 0,
                discard_priority_score: 0.0,
                breakdown: MemoryBreakdown {
                    base_bytes: estimated,
                    complexity_bytes: 0,
                    dom_bytes: 0,
                    iframe_bytes: 0,
                    media_bytes: 0,
                    extension_bytes: 0,
                    js_overhead_bytes: 0,
                },
            });
        }

        let base_bytes = self.config.base_tab_bytes;
        let complexity_bytes = self
            .config
            .complexity_baselines
            .for_complexity(input.complexity);
        let dom_bytes = u64::from(input.dom_nodes).saturating_mul(self.config.dom_node_bytes);
        let iframe_bytes =
            u64::from(input.iframe_count).saturating_mul(self.config.iframe_overhead_bytes);
        let media_bytes =
            u64::from(input.active_media_streams).saturating_mul(self.config.media_buffer_bytes);
        let extension_bytes =
            u64::from(input.extension_hooks).saturating_mul(self.config.extension_hook_bytes);

        let js_overhead_bytes = match input.js_heap_bytes {
            Some(heap) => ((heap as f64) * self.config.js_heap_multiplier).round() as u64,
            None => 0,
        };

        let raw_heuristic = base_bytes
            .saturating_add(complexity_bytes)
            .saturating_add(dom_bytes)
            .saturating_add(iframe_bytes)
            .saturating_add(media_bytes)
            .saturating_add(extension_bytes)
            .saturating_add(js_overhead_bytes);

        let multiplier = self.config.state_multipliers.for_state(input.state);
        let scaled_heuristic = ((raw_heuristic as f64) * multiplier).round() as u64;

        let (estimated_bytes, calibrated_from_observed) = match input.observed_rss_bytes {
            Some(observed) => {
                let w = self.config.observed_blend_weight;
                let blended = (observed as f64) * w + (scaled_heuristic as f64) * (1.0 - w);
                (blended.round() as u64, true)
            }
            None => (scaled_heuristic, false),
        };

        let potential_savings_bytes =
            estimated_bytes.saturating_sub(self.config.discarded_stub_bytes);
        let tier = self.config.tier_thresholds.classify(estimated_bytes);
        let discard_priority_score = compute_discard_priority(input, potential_savings_bytes);

        Ok(TabMemoryEstimate {
            tab_id: input.tab_id.clone(),
            estimated_bytes,
            estimated_mib: bytes_to_mib(estimated_bytes),
            calibrated_from_observed,
            tier,
            potential_savings_bytes,
            discard_priority_score,
            breakdown: MemoryBreakdown {
                base_bytes,
                complexity_bytes,
                dom_bytes,
                iframe_bytes,
                media_bytes,
                extension_bytes,
                js_overhead_bytes,
            },
        })
    }

    /// Estimates memory usage across a collection of tabs in a session.
    pub fn estimate_session(
        &self,
        tabs: &[TabMemoryInput],
    ) -> Result<SessionMemoryReport, TabMemoryError> {
        let mut seen_ids = HashSet::with_capacity(tabs.len());
        let mut estimates = Vec::with_capacity(tabs.len());
        let mut total_estimated_bytes: u64 = 0;
        let mut total_potential_savings_bytes: u64 = 0;
        let mut live_tab_count: usize = 0;
        let mut discarded_tab_count: usize = 0;

        for tab in tabs {
            if !seen_ids.insert(tab.tab_id.as_str()) {
                return Err(TabMemoryError::DuplicateTabId(tab.tab_id.clone()));
            }
            let estimate = self.estimate_tab(tab)?;
            total_estimated_bytes = total_estimated_bytes.saturating_add(estimate.estimated_bytes);
            total_potential_savings_bytes =
                total_potential_savings_bytes.saturating_add(estimate.potential_savings_bytes);
            if tab.state == TabLifecycleState::Discarded {
                discarded_tab_count += 1;
            } else {
                live_tab_count += 1;
            }
            estimates.push(estimate);
        }

        let budget_utilization_ratio =
            (total_estimated_bytes as f64) / (self.config.system_budget_bytes as f64);
        let pressure_level = classify_pressure(budget_utilization_ratio);

        Ok(SessionMemoryReport {
            total_estimated_bytes,
            total_estimated_mib: bytes_to_mib(total_estimated_bytes),
            total_potential_savings_bytes,
            tab_count: tabs.len(),
            live_tab_count,
            discarded_tab_count,
            pressure_level,
            budget_utilization_ratio,
            estimates,
        })
    }

    /// Selects tabs to discard in descending priority order until `target_reclaim_bytes` is met.
    pub fn recommend_discard_candidates(
        &self,
        tabs: &[TabMemoryInput],
        target_reclaim_bytes: u64,
    ) -> Result<Vec<DiscardRecommendation>, TabMemoryError> {
        if target_reclaim_bytes == 0 {
            return Ok(Vec::new());
        }

        let report = self.estimate_session(tabs)?;
        let mut candidates: Vec<DiscardRecommendation> = report
            .estimates
            .into_iter()
            .filter(|est| est.discard_priority_score > 0.0 && est.potential_savings_bytes > 0)
            .map(|est| DiscardRecommendation {
                tab_id: est.tab_id,
                reclaimable_bytes: est.potential_savings_bytes,
                priority_score: est.discard_priority_score,
                tier: est.tier,
            })
            .collect();

        candidates.sort_by(|a, b| {
            b.priority_score
                .total_cmp(&a.priority_score)
                .then_with(|| b.reclaimable_bytes.cmp(&a.reclaimable_bytes))
                .then_with(|| a.tab_id.cmp(&b.tab_id))
        });

        let mut selected = Vec::new();
        let mut reclaimed: u64 = 0;
        for candidate in candidates {
            reclaimed = reclaimed.saturating_add(candidate.reclaimable_bytes);
            selected.push(candidate);
            if reclaimed >= target_reclaim_bytes {
                break;
            }
        }

        Ok(selected)
    }

    /// Serializes a [`SessionMemoryReport`] to pretty JSON.
    pub fn report_to_json(&self, report: &SessionMemoryReport) -> Result<String, TabMemoryError> {
        serde_json::to_string_pretty(report)
            .map_err(|err| TabMemoryError::Serialization(err.to_string()))
    }

    /// Deserializes a [`SessionMemoryReport`] from JSON.
    pub fn report_from_json(json: &str) -> Result<SessionMemoryReport, TabMemoryError> {
        serde_json::from_str(json).map_err(|err| TabMemoryError::Serialization(err.to_string()))
    }
}

fn bytes_to_mib(bytes: u64) -> f64 {
    (bytes as f64) / BYTES_PER_MIB
}

fn classify_pressure(utilization_ratio: f64) -> MemoryPressureLevel {
    if utilization_ratio >= 0.90 {
        MemoryPressureLevel::Critical
    } else if utilization_ratio >= 0.75 {
        MemoryPressureLevel::High
    } else if utilization_ratio >= 0.55 {
        MemoryPressureLevel::Elevated
    } else {
        MemoryPressureLevel::Normal
    }
}

fn compute_discard_priority(input: &TabMemoryInput, potential_savings_bytes: u64) -> f64 {
    if input.pinned
        || input.audible
        || matches!(
            input.state,
            TabLifecycleState::Active | TabLifecycleState::Discarded
        )
        || potential_savings_bytes == 0
    {
        return 0.0;
    }

    let savings_mib = bytes_to_mib(potential_savings_bytes);
    let idle_minutes = (input.idle_seconds as f64) / 60.0;
    let state_bonus = match input.state {
        TabLifecycleState::Frozen => 1.30,
        TabLifecycleState::Throttled => 1.15,
        TabLifecycleState::Background => 1.0,
        TabLifecycleState::Active | TabLifecycleState::Discarded => 0.0,
    };

    (savings_mib * 0.6 + idle_minutes.min(240.0) * 0.4) * state_bonus
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tab(tab_id: &str, state: TabLifecycleState) -> TabMemoryInput {
        TabMemoryInput {
            tab_id: tab_id.to_string(),
            title: Some("Example".to_string()),
            origin: Some("https://example.org".to_string()),
            state,
            complexity: ContentComplexity::RichApp,
            dom_nodes: 2_500,
            iframe_count: 2,
            active_media_streams: 0,
            extension_hooks: 1,
            js_heap_bytes: Some(24 * MIB),
            observed_rss_bytes: None,
            idle_seconds: 600,
            pinned: false,
            audible: false,
        }
    }

    #[test]
    fn estimates_live_and_discarded_tabs_accurately() {
        let estimator = TabMemoryEstimator::default();

        let live = sample_tab("tab-1", TabLifecycleState::Active);
        let live_est = estimator.estimate_tab(&live).unwrap();
        assert!(!live_est.calibrated_from_observed);
        assert!(live_est.estimated_bytes > 100 * MIB);
        assert_eq!(live_est.discard_priority_score, 0.0);
        assert_eq!(live_est.tier, MemoryTier::Moderate);

        let discarded = sample_tab("tab-2", TabLifecycleState::Discarded);
        let disc_est = estimator.estimate_tab(&discarded).unwrap();
        assert_eq!(
            disc_est.estimated_bytes,
            estimator.config().discarded_stub_bytes
        );
        assert_eq!(disc_est.potential_savings_bytes, 0);
        assert_eq!(disc_est.tier, MemoryTier::Lightweight);
    }

    #[test]
    fn blends_observed_rss_when_provided() {
        let estimator = TabMemoryEstimator::default();
        let mut tab = sample_tab("tab-observed", TabLifecycleState::Background);
        let uncalibrated = estimator.estimate_tab(&tab).unwrap();

        tab.observed_rss_bytes = Some(400 * MIB);
        let calibrated = estimator.estimate_tab(&tab).unwrap();

        assert!(calibrated.calibrated_from_observed);
        assert!(calibrated.estimated_bytes > uncalibrated.estimated_bytes);
        assert!(calibrated.estimated_bytes < 400 * MIB);
        assert_eq!(calibrated.tier, MemoryTier::Heavy);
    }

    #[test]
    fn validates_config_and_inputs() {
        let mut bad_cfg = EstimatorConfig::default();
        bad_cfg.tier_thresholds.heavy_bytes = 32 * MIB;
        let err = TabMemoryEstimator::new(bad_cfg).unwrap_err();
        assert!(matches!(err, TabMemoryError::InvalidConfig(_)));

        let estimator = TabMemoryEstimator::default();
        let mut empty_id = sample_tab("   ", TabLifecycleState::Background);
        assert_eq!(
            estimator.estimate_tab(&empty_id),
            Err(TabMemoryError::EmptyTabId)
        );

        empty_id.tab_id = "ok-id".to_string();
        empty_id.observed_rss_bytes = Some(0);
        assert!(matches!(
            estimator.estimate_tab(&empty_id),
            Err(TabMemoryError::InvalidInput { .. })
        ));
    }

    #[test]
    fn session_report_and_discard_recommendations_respect_protections() {
        let estimator = TabMemoryEstimator::default();

        let active = sample_tab("tab-active", TabLifecycleState::Active);

        let mut pinned = sample_tab("tab-pinned", TabLifecycleState::Background);
        pinned.pinned = true;
        pinned.observed_rss_bytes = Some(600 * MIB);

        let mut audible = sample_tab("tab-audible", TabLifecycleState::Background);
        audible.audible = true;
        audible.observed_rss_bytes = Some(450 * MIB);

        let mut heavy_idle = sample_tab("tab-idle-heavy", TabLifecycleState::Frozen);
        heavy_idle.observed_rss_bytes = Some(350 * MIB);
        heavy_idle.idle_seconds = 3_600;

        let mut light_idle = sample_tab("tab-idle-light", TabLifecycleState::Background);
        light_idle.complexity = ContentComplexity::Minimal;
        light_idle.dom_nodes = 100;
        light_idle.iframe_count = 0;
        light_idle.extension_hooks = 0;
        light_idle.js_heap_bytes = None;
        light_idle.idle_seconds = 120;

        let tabs = vec![active, pinned, audible, heavy_idle, light_idle];
        let report = estimator.estimate_session(&tabs).unwrap();
        assert_eq!(report.tab_count, 5);
        assert_eq!(report.live_tab_count, 5);
        assert_eq!(report.discarded_tab_count, 0);

        let recs = estimator
            .recommend_discard_candidates(&tabs, 200 * MIB)
            .unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].tab_id, "tab-idle-heavy");

        let json = estimator.report_to_json(&report).unwrap();
        let roundtrip = TabMemoryEstimator::report_from_json(&json).unwrap();
        assert_eq!(
            roundtrip.total_estimated_bytes,
            report.total_estimated_bytes
        );
        assert_eq!(roundtrip.tab_count, report.tab_count);
        assert_eq!(roundtrip.estimates.len(), report.estimates.len());
        for (a, b) in roundtrip.estimates.iter().zip(report.estimates.iter()) {
            assert_eq!(a.tab_id, b.tab_id);
            assert_eq!(a.estimated_bytes, b.estimated_bytes);
            assert!((a.discard_priority_score - b.discard_priority_score).abs() < 1e-9);
        }
    }

    #[test]
    fn rejects_duplicate_tab_ids_in_session() {
        let estimator = TabMemoryEstimator::default();
        let t1 = sample_tab("dup-1", TabLifecycleState::Active);
        let t2 = sample_tab("dup-1", TabLifecycleState::Background);
        let err = estimator.estimate_session(&[t1, t2]).unwrap_err();
        assert_eq!(err, TabMemoryError::DuplicateTabId("dup-1".to_string()));
    }
}
