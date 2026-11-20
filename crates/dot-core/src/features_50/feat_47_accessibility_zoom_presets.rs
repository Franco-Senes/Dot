//! Accessibility zoom presets, per-origin overrides, and effective zoom resolution.
//!
//! Provides built-in and custom accessibility zoom presets (combining page zoom
//! percentage, minimum font size, font scaling, line-height, text reflow,
//! contrast boost, and focus-ring scaling), step-based zoom navigation,
//! per-origin overrides, and JSON persistence with strict validation.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

/// Minimum supported page zoom percentage (25%).
pub const MIN_ZOOM_PERCENT: u16 = 25;

/// Maximum supported page zoom percentage (500%).
pub const MAX_ZOOM_PERCENT: u16 = 500;

/// Default neutral page zoom percentage (100%).
pub const DEFAULT_ZOOM_PERCENT: u16 = 100;

/// Minimum supported font scale multiplier.
pub const MIN_FONT_SCALE: f64 = 0.5;

/// Maximum supported font scale multiplier.
pub const MAX_FONT_SCALE: f64 = 3.0;

/// Minimum supported line-height multiplier.
pub const MIN_LINE_HEIGHT: f64 = 1.0;

/// Maximum supported line-height multiplier.
pub const MAX_LINE_HEIGHT: f64 = 2.5;

/// Minimum supported focus-ring scale multiplier.
pub const MIN_FOCUS_RING_SCALE: f64 = 1.0;

/// Maximum supported focus-ring scale multiplier.
pub const MAX_FOCUS_RING_SCALE: f64 = 3.0;

/// Minimum supported minimum-font-size floor in pixels.
pub const MIN_FONT_SIZE_PX: u8 = 8;

/// Maximum supported minimum-font-size floor in pixels.
pub const MAX_FONT_SIZE_PX: u8 = 72;

/// Maximum allowed length for a preset identifier or label.
pub const MAX_PRESET_NAME_LEN: usize = 64;

/// Standard browser zoom steps in percent.
pub const DEFAULT_ZOOM_STEPS: &[u16] = &[
    25, 33, 50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300, 400, 500,
];

/// Identifier of the built-in standard (100%) preset.
pub const BUILTIN_PRESET_STANDARD: &str = "standard";

/// Errors returned by accessibility zoom preset validation and management.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum ZoomError {
    /// Zoom percentage is outside `[MIN_ZOOM_PERCENT, MAX_ZOOM_PERCENT]`.
    #[error("zoom percentage {value}% is out of range ({min}%..={max}%)")]
    ZoomOutOfRange { value: u16, min: u16, max: u16 },

    /// Preset identifier is empty, too long, or contains invalid characters.
    #[error("invalid preset id '{0}': must be 1..=64 ASCII alphanumeric, '-' or '_' characters")]
    InvalidPresetId(String),

    /// Preset label is empty or exceeds maximum length.
    #[error("invalid preset label '{0}': must be non-empty and <= 64 characters")]
    InvalidPresetLabel(String),

    /// A preset with the given identifier already exists.
    #[error("duplicate preset id '{0}'")]
    DuplicatePresetId(String),

    /// Requested preset identifier was not found.
    #[error("preset '{0}' not found")]
    PresetNotFound(String),

    /// Attempted to delete or mutate a protected built-in preset.
    #[error("cannot remove built-in preset '{0}'")]
    CannotRemoveBuiltin(String),

    /// Attempted to delete the currently active global preset.
    #[error("cannot remove active global preset '{0}'")]
    CannotRemoveActivePreset(String),

    /// Font scale is NaN, infinite, or outside `[MIN_FONT_SCALE, MAX_FONT_SCALE]`.
    #[error("font scale {0} is out of range ({MIN_FONT_SCALE}..={MAX_FONT_SCALE})")]
    InvalidFontScale(f64),

    /// Line-height multiplier is NaN, infinite, or outside `[MIN_LINE_HEIGHT, MAX_LINE_HEIGHT]`.
    #[error("line-height multiplier {0} is out of range ({MIN_LINE_HEIGHT}..={MAX_LINE_HEIGHT})")]
    InvalidLineHeight(f64),

    /// Focus-ring scale is NaN, infinite, or outside `[MIN_FOCUS_RING_SCALE, MAX_FOCUS_RING_SCALE]`.
    #[error(
        "focus ring scale {0} is out of range ({MIN_FOCUS_RING_SCALE}..={MAX_FOCUS_RING_SCALE})"
    )]
    InvalidFocusRingScale(f64),

    /// Minimum font size floor in pixels is outside `[MIN_FONT_SIZE_PX, MAX_FONT_SIZE_PX]`.
    #[error("minimum font size {0}px is out of range ({MIN_FONT_SIZE_PX}..={MAX_FONT_SIZE_PX}px)")]
    InvalidMinFontSize(u8),

    /// Origin string could not be parsed into a valid web origin.
    #[error("invalid origin '{0}': expected an http, https, or file URL/origin")]
    InvalidOrigin(String),

    /// Origin override must specify at least a preset id or a custom zoom percentage.
    #[error("origin override for '{0}' must specify a preset_id or custom_zoom_percent")]
    EmptyOriginOverride(String),

    /// Zoom step list is empty or contains out-of-range values.
    #[error("invalid zoom step policy: {0}")]
    InvalidStepPolicy(String),

    /// JSON serialization or deserialization failed.
    #[error("serialization error: {0}")]
    Serialization(String),
}

/// How text layout adapts when zoom is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TextReflowMode {
    /// Standard proportional CSS/page zoom.
    #[default]
    Standard,
    /// Reflows text blocks to fit the viewport width without horizontal scrolling.
    ReflowToViewport,
    /// Scales text elements only without enlarging images or chrome.
    FontOnly,
}

/// Optional contrast adjustment paired with a zoom preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContrastBoost {
    /// No contrast modification.
    #[default]
    None,
    /// Mild contrast enhancement for readability.
    Subtle,
    /// High-contrast foreground/background enforcement.
    High,
    /// Inverted lightness polarity for light-sensitive users.
    Inverted,
}

/// Source that determined an effective zoom configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoomSource {
    /// Determined by the active global preset.
    GlobalPreset,
    /// Determined by a per-origin override.
    OriginOverride,
}

/// A named accessibility zoom preset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZoomPreset {
    /// Unique slug identifier (ASCII alphanumeric, `-`, `_`).
    pub id: String,
    /// Human-readable display label.
    pub label: String,
    /// Brief description of the intended accessibility use case.
    pub description: String,
    /// Base page zoom percentage (`25..=500`).
    pub zoom_percent: u16,
    /// Optional minimum font size floor in CSS pixels (`8..=72`).
    pub min_font_size_px: Option<u8>,
    /// Additional font scale multiplier (`0.5..=3.0`).
    pub font_scale: f64,
    /// Optional minimum line-height multiplier (`1.0..=2.5`).
    pub line_height_multiplier: Option<f64>,
    /// Text reflow strategy when zoomed.
    pub reflow_mode: TextReflowMode,
    /// Contrast boost level associated with this preset.
    pub contrast_boost: ContrastBoost,
    /// Focus indicator scale multiplier (`1.0..=3.0`).
    pub focus_ring_scale: f64,
    /// Whether this preset is built-in (protected from removal).
    pub built_in: bool,
}

impl ZoomPreset {
    /// Validates all fields of the preset.
    pub fn validate(&self) -> Result<(), ZoomError> {
        validate_preset_id(&self.id)?;
        validate_preset_label(&self.label)?;
        validate_zoom_percent(self.zoom_percent)?;
        if let Some(min_px) = self.min_font_size_px {
            if !(MIN_FONT_SIZE_PX..=MAX_FONT_SIZE_PX).contains(&min_px) {
                return Err(ZoomError::InvalidMinFontSize(min_px));
            }
        }
        validate_finite_range(
            self.font_scale,
            MIN_FONT_SCALE,
            MAX_FONT_SCALE,
            ZoomError::InvalidFontScale,
        )?;
        if let Some(lh) = self.line_height_multiplier {
            validate_finite_range(
                lh,
                MIN_LINE_HEIGHT,
                MAX_LINE_HEIGHT,
                ZoomError::InvalidLineHeight,
            )?;
        }
        validate_finite_range(
            self.focus_ring_scale,
            MIN_FOCUS_RING_SCALE,
            MAX_FOCUS_RING_SCALE,
            ZoomError::InvalidFocusRingScale,
        )?;
        Ok(())
    }
}

/// Per-origin zoom override.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OriginZoomOverride {
    /// Canonicalized origin key (e.g. `https://example.com` or `https://example.com:8443`).
    pub origin: String,
    /// Optional preset identifier applied to this origin.
    pub preset_id: Option<String>,
    /// Optional custom zoom percentage overriding the preset's `zoom_percent`.
    pub custom_zoom_percent: Option<u16>,
    /// Unix timestamp (seconds) when this override was last updated.
    pub updated_at_unix: i64,
}

/// Ordered list of zoom steps used for zoom-in / zoom-out actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoomStepPolicy {
    /// Strictly increasing list of zoom percentages.
    pub steps_percent: Vec<u16>,
}

impl Default for ZoomStepPolicy {
    fn default() -> Self {
        Self {
            steps_percent: DEFAULT_ZOOM_STEPS.to_vec(),
        }
    }
}

impl ZoomStepPolicy {
    /// Creates a validated step policy from the provided zoom percentages.
    ///
    /// Duplicates are removed and steps are sorted ascending.
    pub fn new(mut steps_percent: Vec<u16>) -> Result<Self, ZoomError> {
        if steps_percent.is_empty() {
            return Err(ZoomError::InvalidStepPolicy(
                "steps list cannot be empty".to_string(),
            ));
        }
        for &step in &steps_percent {
            validate_zoom_percent(step)?;
        }
        steps_percent.sort_unstable();
        steps_percent.dedup();
        Ok(Self { steps_percent })
    }

    /// Validates that the step list is non-empty, strictly increasing, and in range.
    pub fn validate(&self) -> Result<(), ZoomError> {
        if self.steps_percent.is_empty() {
            return Err(ZoomError::InvalidStepPolicy(
                "steps list cannot be empty".to_string(),
            ));
        }
        let mut prev: Option<u16> = None;
        for &step in &self.steps_percent {
            validate_zoom_percent(step)?;
            if let Some(p) = prev {
                if step <= p {
                    return Err(ZoomError::InvalidStepPolicy(
                        "steps must be strictly increasing".to_string(),
                    ));
                }
            }
            prev = Some(step);
        }
        Ok(())
    }

    /// Returns the next higher zoom step above `current`, or the maximum step if already at/above it.
    pub fn step_up(&self, current: u16) -> u16 {
        for &step in &self.steps_percent {
            if step > current {
                return step;
            }
        }
        self.steps_percent
            .last()
            .copied()
            .unwrap_or(MAX_ZOOM_PERCENT)
    }

    /// Returns the next lower zoom step below `current`, or the minimum step if already at/below it.
    pub fn step_down(&self, current: u16) -> u16 {
        for &step in self.steps_percent.iter().rev() {
            if step < current {
                return step;
            }
        }
        self.steps_percent
            .first()
            .copied()
            .unwrap_or(MIN_ZOOM_PERCENT)
    }

    /// Returns the step closest to `target`. Ties prefer the higher step.
    pub fn nearest_step(&self, target: u16) -> u16 {
        let mut best = self
            .steps_percent
            .first()
            .copied()
            .unwrap_or(DEFAULT_ZOOM_PERCENT);
        let mut best_dist = best.abs_diff(target);
        for &step in &self.steps_percent {
            let dist = step.abs_diff(target);
            if dist < best_dist || (dist == best_dist && step > best) {
                best = step;
                best_dist = dist;
            }
        }
        best
    }
}

/// Resolved zoom configuration ready to apply to a browser tab or engine view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectiveZoomConfig {
    /// Preset identifier used (either from the origin override or global preset).
    pub preset_id: String,
    /// Effective page zoom percentage clamped to `[MIN_ZOOM_PERCENT, MAX_ZOOM_PERCENT]`.
    pub zoom_percent: u16,
    /// Linear zoom factor (`zoom_percent / 100.0`).
    pub zoom_factor: f64,
    /// Chromium/CEF logarithmic zoom level (`log_{1.2}(zoom_factor)`).
    pub cef_zoom_level: f64,
    /// Effective font scale multiplier after optional OS text scale composition.
    pub effective_font_scale: f64,
    /// Optional minimum font size floor in CSS pixels.
    pub min_font_size_px: Option<u8>,
    /// Optional minimum line-height multiplier.
    pub line_height_multiplier: Option<f64>,
    /// Text reflow strategy.
    pub reflow_mode: TextReflowMode,
    /// Contrast boost level.
    pub contrast_boost: ContrastBoost,
    /// Focus indicator scale multiplier.
    pub focus_ring_scale: f64,
    /// Whether this configuration came from a global preset or an origin override.
    pub source: ZoomSource,
    /// Canonical origin if an origin override matched.
    pub matched_origin: Option<String>,
}

/// Registry managing accessibility zoom presets, OS text scaling, and per-origin overrides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccessibilityZoomRegistry {
    /// Registered presets (built-in and user-defined).
    presets: Vec<ZoomPreset>,
    /// Identifier of the active global preset.
    active_global_preset_id: String,
    /// Per-origin overrides keyed by canonical origin string.
    origin_overrides: BTreeMap<String, OriginZoomOverride>,
    /// Whether the OS desktop text scale factor is composed into `effective_font_scale`.
    respect_os_text_scale: bool,
    /// OS text scale factor (e.g. GNOME `text-scaling-factor`, default `1.0`).
    os_text_scale_factor: f64,
    /// Step policy for zoom-in / zoom-out commands.
    step_policy: ZoomStepPolicy,
}

impl Default for AccessibilityZoomRegistry {
    fn default() -> Self {
        Self::with_builtin_presets()
    }
}

impl AccessibilityZoomRegistry {
    /// Creates a new registry pre-populated with built-in accessibility zoom presets.
    pub fn with_builtin_presets() -> Self {
        let presets = vec![
            ZoomPreset {
                id: "compact".to_string(),
                label: "Compact".to_string(),
                description: "Slightly denser layout at 85% zoom for high-information views."
                    .to_string(),
                zoom_percent: 85,
                min_font_size_px: None,
                font_scale: 1.0,
                line_height_multiplier: None,
                reflow_mode: TextReflowMode::Standard,
                contrast_boost: ContrastBoost::None,
                focus_ring_scale: 1.0,
                built_in: true,
            },
            ZoomPreset {
                id: BUILTIN_PRESET_STANDARD.to_string(),
                label: "Standard".to_string(),
                description: "Default 100% browser zoom with no overrides.".to_string(),
                zoom_percent: DEFAULT_ZOOM_PERCENT,
                min_font_size_px: None,
                font_scale: 1.0,
                line_height_multiplier: None,
                reflow_mode: TextReflowMode::Standard,
                contrast_boost: ContrastBoost::None,
                focus_ring_scale: 1.0,
                built_in: true,
            },
            ZoomPreset {
                id: "comfortable-reading".to_string(),
                label: "Comfortable Reading".to_string(),
                description: "125% zoom with 14px font floor, 1.4x line height, and text reflow."
                    .to_string(),
                zoom_percent: 125,
                min_font_size_px: Some(14),
                font_scale: 1.05,
                line_height_multiplier: Some(1.4),
                reflow_mode: TextReflowMode::ReflowToViewport,
                contrast_boost: ContrastBoost::None,
                focus_ring_scale: 1.15,
                built_in: true,
            },
            ZoomPreset {
                id: "large-text".to_string(),
                label: "Large Text".to_string(),
                description: "150% zoom with 16px font floor, subtle contrast boost, and reflow."
                    .to_string(),
                zoom_percent: 150,
                min_font_size_px: Some(16),
                font_scale: 1.25,
                line_height_multiplier: Some(1.5),
                reflow_mode: TextReflowMode::ReflowToViewport,
                contrast_boost: ContrastBoost::Subtle,
                focus_ring_scale: 1.25,
                built_in: true,
            },
            ZoomPreset {
                id: "low-vision".to_string(),
                label: "Low Vision".to_string(),
                description: "200% zoom with 20px font floor, high contrast, and prominent focus."
                    .to_string(),
                zoom_percent: 200,
                min_font_size_px: Some(20),
                font_scale: 1.5,
                line_height_multiplier: Some(1.6),
                reflow_mode: TextReflowMode::ReflowToViewport,
                contrast_boost: ContrastBoost::High,
                focus_ring_scale: 1.5,
                built_in: true,
            },
            ZoomPreset {
                id: "high-magnification".to_string(),
                label: "High Magnification".to_string(),
                description: "300% zoom with 24px font floor, high contrast, and 2x focus ring."
                    .to_string(),
                zoom_percent: 300,
                min_font_size_px: Some(24),
                font_scale: 1.75,
                line_height_multiplier: Some(1.8),
                reflow_mode: TextReflowMode::ReflowToViewport,
                contrast_boost: ContrastBoost::High,
                focus_ring_scale: 2.0,
                built_in: true,
            },
        ];

        Self {
            presets,
            active_global_preset_id: BUILTIN_PRESET_STANDARD.to_string(),
            origin_overrides: BTreeMap::new(),
            respect_os_text_scale: true,
            os_text_scale_factor: 1.0,
            step_policy: ZoomStepPolicy::default(),
        }
    }

    /// Returns all registered presets.
    pub fn presets(&self) -> &[ZoomPreset] {
        &self.presets
    }

    /// Looks up a preset by its identifier.
    pub fn get_preset(&self, id: &str) -> Option<&ZoomPreset> {
        self.presets.iter().find(|p| p.id == id)
    }

    /// Returns the identifier of the active global preset.
    pub fn active_global_preset_id(&self) -> &str {
        &self.active_global_preset_id
    }

    /// Returns all per-origin overrides keyed by canonical origin.
    pub fn origin_overrides(&self) -> &BTreeMap<String, OriginZoomOverride> {
        &self.origin_overrides
    }

    /// Returns the current zoom step policy.
    pub fn step_policy(&self) -> &ZoomStepPolicy {
        &self.step_policy
    }

    /// Replaces the zoom step policy after validation.
    pub fn set_step_policy(&mut self, policy: ZoomStepPolicy) -> Result<(), ZoomError> {
        policy.validate()?;
        self.step_policy = policy;
        Ok(())
    }

    /// Configures whether OS text scaling is composed into effective font scale.
    pub fn set_os_text_scale(&mut self, respect: bool, scale_factor: f64) -> Result<(), ZoomError> {
        validate_finite_range(
            scale_factor,
            MIN_FONT_SCALE,
            MAX_FONT_SCALE,
            ZoomError::InvalidFontScale,
        )?;
        self.respect_os_text_scale = respect;
        self.os_text_scale_factor = scale_factor;
        Ok(())
    }

    /// Registers a new custom preset.
    pub fn register_preset(&mut self, mut preset: ZoomPreset) -> Result<(), ZoomError> {
        preset.validate()?;
        if self.presets.iter().any(|p| p.id == preset.id) {
            return Err(ZoomError::DuplicatePresetId(preset.id));
        }
        preset.built_in = false;
        self.presets.push(preset);
        Ok(())
    }

    /// Updates an existing custom preset. Built-in presets cannot be overwritten.
    pub fn update_preset(&mut self, mut preset: ZoomPreset) -> Result<(), ZoomError> {
        preset.validate()?;
        let existing = self
            .presets
            .iter_mut()
            .find(|p| p.id == preset.id)
            .ok_or_else(|| ZoomError::PresetNotFound(preset.id.clone()))?;
        if existing.built_in {
            return Err(ZoomError::CannotRemoveBuiltin(existing.id.clone()));
        }
        preset.built_in = false;
        *existing = preset;
        Ok(())
    }

    /// Removes a custom preset by id.
    ///
    /// Built-in presets and the currently active global preset cannot be removed.
    /// Any origin overrides referencing the removed preset are cleared if they have
    /// no custom zoom percentage, or detached from `preset_id` otherwise.
    pub fn remove_preset(&mut self, id: &str) -> Result<ZoomPreset, ZoomError> {
        let idx = self
            .presets
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| ZoomError::PresetNotFound(id.to_string()))?;

        if self.presets[idx].built_in {
            return Err(ZoomError::CannotRemoveBuiltin(id.to_string()));
        }
        if self.active_global_preset_id == id {
            return Err(ZoomError::CannotRemoveActivePreset(id.to_string()));
        }

        let removed = self.presets.remove(idx);

        let mut to_remove = Vec::new();
        for (origin, ov) in &mut self.origin_overrides {
            if ov.preset_id.as_deref() == Some(id) {
                ov.preset_id = None;
                if ov.custom_zoom_percent.is_none() {
                    to_remove.push(origin.clone());
                }
            }
        }
        for origin in to_remove {
            self.origin_overrides.remove(&origin);
        }

        Ok(removed)
    }

    /// Sets the active global preset by identifier.
    pub fn set_active_global_preset(&mut self, id: &str) -> Result<(), ZoomError> {
        if self.get_preset(id).is_none() {
            return Err(ZoomError::PresetNotFound(id.to_string()));
        }
        self.active_global_preset_id = id.to_string();
        Ok(())
    }

    /// Sets or updates a per-origin zoom override.
    pub fn set_origin_override(
        &mut self,
        raw_url_or_origin: &str,
        preset_id: Option<&str>,
        custom_zoom_percent: Option<u16>,
        now_unix: i64,
    ) -> Result<OriginZoomOverride, ZoomError> {
        let origin = normalize_origin(raw_url_or_origin)?;
        if preset_id.is_none() && custom_zoom_percent.is_none() {
            return Err(ZoomError::EmptyOriginOverride(origin));
        }
        if let Some(pid) = preset_id {
            if self.get_preset(pid).is_none() {
                return Err(ZoomError::PresetNotFound(pid.to_string()));
            }
        }
        if let Some(zp) = custom_zoom_percent {
            validate_zoom_percent(zp)?;
        }

        let ov = OriginZoomOverride {
            origin: origin.clone(),
            preset_id: preset_id.map(str::to_string),
            custom_zoom_percent,
            updated_at_unix: now_unix,
        };
        self.origin_overrides.insert(origin, ov.clone());
        Ok(ov)
    }

    /// Clears a per-origin override. Returns `true` if an override was removed.
    pub fn clear_origin_override(&mut self, raw_url_or_origin: &str) -> Result<bool, ZoomError> {
        let origin = normalize_origin(raw_url_or_origin)?;
        Ok(self.origin_overrides.remove(&origin).is_some())
    }

    /// Steps the zoom up for `raw_url_or_origin` and stores it as a per-origin override.
    pub fn step_up_for_origin(
        &mut self,
        raw_url_or_origin: &str,
        now_unix: i64,
    ) -> Result<u16, ZoomError> {
        let origin = normalize_origin(raw_url_or_origin)?;
        let current = self.resolve_effective_zoom(Some(&origin))?;
        let next = self.step_policy.step_up(current.zoom_percent);
        let existing_preset = self
            .origin_overrides
            .get(&origin)
            .and_then(|ov| ov.preset_id.clone());
        self.set_origin_override(&origin, existing_preset.as_deref(), Some(next), now_unix)?;
        Ok(next)
    }

    /// Steps the zoom down for `raw_url_or_origin` and stores it as a per-origin override.
    pub fn step_down_for_origin(
        &mut self,
        raw_url_or_origin: &str,
        now_unix: i64,
    ) -> Result<u16, ZoomError> {
        let origin = normalize_origin(raw_url_or_origin)?;
        let current = self.resolve_effective_zoom(Some(&origin))?;
        let next = self.step_policy.step_down(current.zoom_percent);
        let existing_preset = self
            .origin_overrides
            .get(&origin)
            .and_then(|ov| ov.preset_id.clone());
        self.set_origin_override(&origin, existing_preset.as_deref(), Some(next), now_unix)?;
        Ok(next)
    }

    /// Resolves the effective zoom configuration for an optional URL or origin.
    pub fn resolve_effective_zoom(
        &self,
        raw_url_or_origin: Option<&str>,
    ) -> Result<EffectiveZoomConfig, ZoomError> {
        let matched_origin = match raw_url_or_origin {
            Some(raw) => Some(normalize_origin(raw)?),
            None => None,
        };

        let override_entry = matched_origin
            .as_ref()
            .and_then(|o| self.origin_overrides.get(o));

        let global_preset = self
            .get_preset(&self.active_global_preset_id)
            .ok_or_else(|| ZoomError::PresetNotFound(self.active_global_preset_id.clone()))?;

        let (base_preset, zoom_percent, source, resolved_origin) = match override_entry {
            Some(ov) => {
                let preset = match &ov.preset_id {
                    Some(pid) => self
                        .get_preset(pid)
                        .ok_or_else(|| ZoomError::PresetNotFound(pid.clone()))?,
                    None => global_preset,
                };
                let zp = ov.custom_zoom_percent.unwrap_or(preset.zoom_percent);
                (
                    preset,
                    zp,
                    ZoomSource::OriginOverride,
                    Some(ov.origin.clone()),
                )
            }
            None => (
                global_preset,
                global_preset.zoom_percent,
                ZoomSource::GlobalPreset,
                None,
            ),
        };

        let zoom_factor = f64::from(zoom_percent) / 100.0;
        let cef_zoom_level = zoom_factor_to_cef_level(zoom_factor);
        let os_scale = if self.respect_os_text_scale {
            self.os_text_scale_factor
        } else {
            1.0
        };
        let effective_font_scale =
            (base_preset.font_scale * os_scale).clamp(MIN_FONT_SCALE, MAX_FONT_SCALE);

        Ok(EffectiveZoomConfig {
            preset_id: base_preset.id.clone(),
            zoom_percent,
            zoom_factor,
            cef_zoom_level,
            effective_font_scale,
            min_font_size_px: base_preset.min_font_size_px,
            line_height_multiplier: base_preset.line_height_multiplier,
            reflow_mode: base_preset.reflow_mode,
            contrast_boost: base_preset.contrast_boost,
            focus_ring_scale: base_preset.focus_ring_scale,
            source,
            matched_origin: resolved_origin,
        })
    }

    /// Serializes the registry to pretty-printed JSON.
    pub fn to_json(&self) -> Result<String, ZoomError> {
        serde_json::to_string_pretty(self).map_err(|e| ZoomError::Serialization(e.to_string()))
    }

    /// Deserializes a registry from JSON and validates all invariants.
    pub fn from_json(json: &str) -> Result<Self, ZoomError> {
        let registry: Self =
            serde_json::from_str(json).map_err(|e| ZoomError::Serialization(e.to_string()))?;
        registry.validate()?;
        Ok(registry)
    }

    /// Validates all presets, active preset pointer, step policy, and origin overrides.
    pub fn validate(&self) -> Result<(), ZoomError> {
        let mut seen_ids = Vec::with_capacity(self.presets.len());
        for preset in &self.presets {
            preset.validate()?;
            if seen_ids.iter().any(|id| id == &preset.id) {
                return Err(ZoomError::DuplicatePresetId(preset.id.clone()));
            }
            seen_ids.push(preset.id.clone());
        }

        if self.get_preset(&self.active_global_preset_id).is_none() {
            return Err(ZoomError::PresetNotFound(
                self.active_global_preset_id.clone(),
            ));
        }

        validate_finite_range(
            self.os_text_scale_factor,
            MIN_FONT_SCALE,
            MAX_FONT_SCALE,
            ZoomError::InvalidFontScale,
        )?;
        self.step_policy.validate()?;

        for (key, ov) in &self.origin_overrides {
            let normalized = normalize_origin(key)?;
            if &normalized != key || normalized != ov.origin {
                return Err(ZoomError::InvalidOrigin(key.clone()));
            }
            if ov.preset_id.is_none() && ov.custom_zoom_percent.is_none() {
                return Err(ZoomError::EmptyOriginOverride(key.clone()));
            }
            if let Some(pid) = &ov.preset_id {
                if self.get_preset(pid).is_none() {
                    return Err(ZoomError::PresetNotFound(pid.clone()));
                }
            }
            if let Some(zp) = ov.custom_zoom_percent {
                validate_zoom_percent(zp)?;
            }
        }

        Ok(())
    }
}

/// Converts a linear zoom factor (e.g. `1.5`) to a Chromium/CEF zoom level (`log_{1.2}(factor)`).
pub fn zoom_factor_to_cef_level(factor: f64) -> f64 {
    if !factor.is_finite() || factor <= 0.0 {
        return 0.0;
    }
    factor.ln() / 1.2_f64.ln()
}

/// Converts a Chromium/CEF zoom level back to a clamped zoom percentage.
pub fn cef_level_to_zoom_percent(level: f64) -> u16 {
    if !level.is_finite() {
        return DEFAULT_ZOOM_PERCENT;
    }
    let factor = 1.2_f64.powf(level);
    let pct = (factor * 100.0).round();
    if pct <= f64::from(MIN_ZOOM_PERCENT) {
        MIN_ZOOM_PERCENT
    } else if pct >= f64::from(MAX_ZOOM_PERCENT) {
        MAX_ZOOM_PERCENT
    } else {
        pct as u16
    }
}

/// Normalizes a URL or origin string into its canonical origin (`scheme://host[:port]` or `file://`).
pub fn normalize_origin(raw: &str) -> Result<String, ZoomError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ZoomError::InvalidOrigin(raw.to_string()));
    }
    let parsed = Url::parse(trimmed).map_err(|_| ZoomError::InvalidOrigin(raw.to_string()))?;
    match parsed.scheme() {
        "http" | "https" => {
            if parsed.host_str().is_none() {
                return Err(ZoomError::InvalidOrigin(raw.to_string()));
            }
            Ok(parsed.origin().ascii_serialization())
        }
        "file" => Ok("file://".to_string()),
        _ => Err(ZoomError::InvalidOrigin(raw.to_string())),
    }
}

fn validate_zoom_percent(value: u16) -> Result<(), ZoomError> {
    if !(MIN_ZOOM_PERCENT..=MAX_ZOOM_PERCENT).contains(&value) {
        return Err(ZoomError::ZoomOutOfRange {
            value,
            min: MIN_ZOOM_PERCENT,
            max: MAX_ZOOM_PERCENT,
        });
    }
    Ok(())
}

fn validate_preset_id(id: &str) -> Result<(), ZoomError> {
    if id.is_empty() || id.len() > MAX_PRESET_NAME_LEN {
        return Err(ZoomError::InvalidPresetId(id.to_string()));
    }
    if !id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(ZoomError::InvalidPresetId(id.to_string()));
    }
    Ok(())
}

fn validate_preset_label(label: &str) -> Result<(), ZoomError> {
    let trimmed = label.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_PRESET_NAME_LEN {
        return Err(ZoomError::InvalidPresetLabel(label.to_string()));
    }
    Ok(())
}

fn validate_finite_range<F>(value: f64, min: f64, max: f64, make_err: F) -> Result<(), ZoomError>
where
    F: FnOnce(f64) -> ZoomError,
{
    if !value.is_finite() || value < min || value > max {
        return Err(make_err(value));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_presets_and_global_resolution() {
        let mut reg = AccessibilityZoomRegistry::with_builtin_presets();
        assert_eq!(reg.active_global_preset_id(), BUILTIN_PRESET_STANDARD);
        assert!(reg.presets().len() >= 6);

        let default_cfg = reg.resolve_effective_zoom(None).unwrap();
        assert_eq!(default_cfg.zoom_percent, 100);
        assert!((default_cfg.zoom_factor - 1.0).abs() < 1e-9);
        assert!(default_cfg.cef_zoom_level.abs() < 1e-9);
        assert_eq!(default_cfg.source, ZoomSource::GlobalPreset);

        reg.set_active_global_preset("low-vision").unwrap();
        reg.set_os_text_scale(true, 1.2).unwrap();
        let low_vision_cfg = reg
            .resolve_effective_zoom(Some("https://docs.example.org/guide?page=2"))
            .unwrap();
        assert_eq!(low_vision_cfg.preset_id, "low-vision");
        assert_eq!(low_vision_cfg.zoom_percent, 200);
        assert_eq!(low_vision_cfg.min_font_size_px, Some(20));
        assert_eq!(low_vision_cfg.reflow_mode, TextReflowMode::ReflowToViewport);
        assert_eq!(low_vision_cfg.contrast_boost, ContrastBoost::High);
        assert!((low_vision_cfg.effective_font_scale - 1.8).abs() < 1e-9);
    }

    #[test]
    fn per_origin_overrides_and_stepping() {
        let mut reg = AccessibilityZoomRegistry::with_builtin_presets();
        reg.set_active_global_preset("comfortable-reading").unwrap();

        let ov = reg
            .set_origin_override(
                "https://Example.COM:8443/article/123#top",
                Some("large-text"),
                None,
                1_700_000_000,
            )
            .unwrap();
        assert_eq!(ov.origin, "https://example.com:8443");

        let cfg = reg
            .resolve_effective_zoom(Some("https://example.com:8443/other"))
            .unwrap();
        assert_eq!(cfg.source, ZoomSource::OriginOverride);
        assert_eq!(cfg.preset_id, "large-text");
        assert_eq!(cfg.zoom_percent, 150);
        assert_eq!(
            cfg.matched_origin.as_deref(),
            Some("https://example.com:8443")
        );

        let up = reg
            .step_up_for_origin("https://example.com:8443/other", 1_700_000_010)
            .unwrap();
        assert_eq!(up, 175);

        let down = reg
            .step_down_for_origin("https://example.com:8443/other", 1_700_000_020)
            .unwrap();
        assert_eq!(down, 150);

        assert!(reg
            .clear_origin_override("https://example.com:8443")
            .unwrap());
        let after_clear = reg
            .resolve_effective_zoom(Some("https://example.com:8443"))
            .unwrap();
        assert_eq!(after_clear.source, ZoomSource::GlobalPreset);
        assert_eq!(after_clear.preset_id, "comfortable-reading");
    }

    #[test]
    fn custom_preset_lifecycle_and_validation_errors() {
        let mut reg = AccessibilityZoomRegistry::with_builtin_presets();

        let custom = ZoomPreset {
            id: "dyslexia-friendly".to_string(),
            label: "Dyslexia Friendly".to_string(),
            description: "135% zoom with wider line height and reflow.".to_string(),
            zoom_percent: 135,
            min_font_size_px: Some(16),
            font_scale: 1.1,
            line_height_multiplier: Some(1.75),
            reflow_mode: TextReflowMode::ReflowToViewport,
            contrast_boost: ContrastBoost::Subtle,
            focus_ring_scale: 1.25,
            built_in: true, // Should be forced to false on registration.
        };
        reg.register_preset(custom.clone()).unwrap();
        assert!(!reg.get_preset("dyslexia-friendly").unwrap().built_in);

        // Duplicate ID rejected.
        assert_eq!(
            reg.register_preset(custom),
            Err(ZoomError::DuplicatePresetId(
                "dyslexia-friendly".to_string()
            ))
        );

        // Cannot remove built-in preset.
        assert_eq!(
            reg.remove_preset("standard"),
            Err(ZoomError::CannotRemoveBuiltin("standard".to_string()))
        );

        // Cannot remove active custom preset.
        reg.set_active_global_preset("dyslexia-friendly").unwrap();
        assert_eq!(
            reg.remove_preset("dyslexia-friendly"),
            Err(ZoomError::CannotRemoveActivePreset(
                "dyslexia-friendly".to_string()
            ))
        );

        // Switch away, then remove cleanly.
        reg.set_active_global_preset("standard").unwrap();
        let removed = reg.remove_preset("dyslexia-friendly").unwrap();
        assert_eq!(removed.id, "dyslexia-friendly");

        // Invalid zoom percentage, NaN font scale, and invalid origin rejected.
        let mut bad_preset = reg.get_preset("standard").unwrap().clone();
        bad_preset.id = "bad-zoom".to_string();
        bad_preset.zoom_percent = 600;
        assert!(matches!(
            reg.register_preset(bad_preset),
            Err(ZoomError::ZoomOutOfRange { value: 600, .. })
        ));
        assert!(matches!(
            reg.set_os_text_scale(true, f64::NAN),
            Err(ZoomError::InvalidFontScale(_))
        ));
        assert!(matches!(
            reg.set_origin_override("javascript:alert(1)", None, Some(150), 0),
            Err(ZoomError::InvalidOrigin(_))
        ));
    }

    #[test]
    fn json_roundtrip_and_cef_level_conversion() {
        let mut reg = AccessibilityZoomRegistry::with_builtin_presets();
        reg.set_active_global_preset("large-text").unwrap();
        reg.set_origin_override("https://news.ycombinator.com", None, Some(175), 42)
            .unwrap();

        let json = reg.to_json().unwrap();
        let decoded = AccessibilityZoomRegistry::from_json(&json).unwrap();
        assert_eq!(decoded, reg);

        let level = zoom_factor_to_cef_level(1.5);
        assert_eq!(cef_level_to_zoom_percent(level), 150);
        assert_eq!(cef_level_to_zoom_percent(f64::NAN), DEFAULT_ZOOM_PERCENT);
    }
}
