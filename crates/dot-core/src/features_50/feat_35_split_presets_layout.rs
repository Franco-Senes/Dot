//! Split-view presets and layout tree resolution for Search Linux.
//!
//! Provides deterministic binary-split layout trees, built-in and custom
//! presets, minimum-dimension-aware viewport placement, and JSON persistence.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use thiserror::Error;

/// Maximum allowed nesting depth for a split layout tree.
pub const MAX_SPLIT_TREE_DEPTH: usize = 8;

/// Maximum allowed leaf panes in a single split preset.
pub const MAX_PANES_PER_PRESET: usize = 16;

/// Minimum allowed split ratio in basis points (0.01%).
pub const MIN_RATIO_BPS: u16 = 1;

/// Maximum allowed split ratio in basis points (99.99%).
pub const MAX_RATIO_BPS: u16 = 9_999;

/// Axis along which a split node divides its allocated rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitAxis {
    /// Divides space into left (`first`) and right (`second`) regions.
    Horizontal,
    /// Divides space into top (`first`) and bottom (`second`) regions.
    Vertical,
}

/// Recursive binary tree describing a split-view pane arrangement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SplitNode {
    /// Terminal pane occupying its resolved rectangle.
    Leaf {
        pane_id: String,
        #[serde(default)]
        min_width: u32,
        #[serde(default)]
        min_height: u32,
    },
    /// Internal split dividing space between two child subtrees.
    Split {
        axis: SplitAxis,
        /// Proportion allocated to `first` in basis points (`1..=9999`).
        ratio_bps: u16,
        first: Box<SplitNode>,
        second: Box<SplitNode>,
    },
}

/// Concrete pixel rectangle inside a browser viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ViewportRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl ViewportRect {
    /// Creates a new viewport rectangle.
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// Resolved placement of a single pane within a viewport.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanePlacement {
    pub pane_id: String,
    pub rect: ViewportRect,
}

/// Named split-view layout preset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitPreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub builtin: bool,
    pub gap_px: u16,
    pub root: SplitNode,
}

/// Errors produced when validating, resolving, or managing split presets.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SplitLayoutError {
    #[error("preset id '{0}' is invalid (must be non-empty ASCII alphanumeric, '-' or '_')")]
    InvalidPresetId(String),
    #[error("preset name must not be empty")]
    EmptyPresetName,
    #[error("preset '{0}' already exists")]
    DuplicatePresetId(String),
    #[error("preset '{0}' was not found")]
    PresetNotFound(String),
    #[error("built-in preset '{0}' cannot be removed")]
    CannotDeleteBuiltinPreset(String),
    #[error("pane id must not be empty")]
    EmptyPaneId,
    #[error("duplicate pane id '{0}' in preset")]
    DuplicatePaneId(String),
    #[error("split ratio {0} bps is out of range (expected 1..=9999)")]
    InvalidRatioBps(u16),
    #[error("split tree depth {actual} exceeds maximum {max}")]
    ExcessiveTreeDepth { max: usize, actual: usize },
    #[error("pane count {actual} exceeds maximum {max}")]
    TooManyPanes { max: usize, actual: usize },
    #[error(
        "viewport {available_width}x{available_height} is smaller than required minimum {required_width}x{required_height}"
    )]
    ViewportTooSmall {
        required_width: u32,
        required_height: u32,
        available_width: u32,
        available_height: u32,
    },
    #[error("split path does not point to an internal split node")]
    InvalidSplitPath,
    #[error("serialization error: {0}")]
    SerializationError(String),
}

impl SplitNode {
    /// Constructs a leaf node with zero minimum dimensions.
    pub fn leaf(pane_id: impl Into<String>) -> Self {
        Self::Leaf {
            pane_id: pane_id.into(),
            min_width: 0,
            min_height: 0,
        }
    }

    /// Constructs a leaf node with explicit minimum dimensions.
    pub fn leaf_with_min(pane_id: impl Into<String>, min_width: u32, min_height: u32) -> Self {
        Self::Leaf {
            pane_id: pane_id.into(),
            min_width,
            min_height,
        }
    }

    /// Constructs an internal split node.
    pub fn split(axis: SplitAxis, ratio_bps: u16, first: SplitNode, second: SplitNode) -> Self {
        Self::Split {
            axis,
            ratio_bps,
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    /// Returns the minimum `(width, height)` required to lay out this subtree
    /// without violating any leaf's minimum dimensions.
    pub fn minimum_size(&self, gap_px: u16) -> (u32, u32) {
        let gap = u32::from(gap_px);
        match self {
            Self::Leaf {
                min_width,
                min_height,
                ..
            } => (*min_width, *min_height),
            Self::Split {
                axis,
                first,
                second,
                ..
            } => {
                let (w1, h1) = first.minimum_size(gap_px);
                let (w2, h2) = second.minimum_size(gap_px);
                match axis {
                    SplitAxis::Horizontal => {
                        (w1.saturating_add(w2).saturating_add(gap), h1.max(h2))
                    }
                    SplitAxis::Vertical => (w1.max(w2), h1.saturating_add(h2).saturating_add(gap)),
                }
            }
        }
    }

    /// Updates `ratio_bps` on the split node addressed by `path` (`false` = first, `true` = second).
    pub fn set_ratio_at_path(
        &mut self,
        path: &[bool],
        new_ratio_bps: u16,
    ) -> Result<(), SplitLayoutError> {
        if !(MIN_RATIO_BPS..=MAX_RATIO_BPS).contains(&new_ratio_bps) {
            return Err(SplitLayoutError::InvalidRatioBps(new_ratio_bps));
        }

        let mut cursor = self;
        for &go_second in path {
            cursor = match cursor {
                Self::Split { first, second, .. } => {
                    if go_second {
                        second.as_mut()
                    } else {
                        first.as_mut()
                    }
                }
                Self::Leaf { .. } => return Err(SplitLayoutError::InvalidSplitPath),
            };
        }

        match cursor {
            Self::Split { ratio_bps, .. } => {
                *ratio_bps = new_ratio_bps;
                Ok(())
            }
            Self::Leaf { .. } => Err(SplitLayoutError::InvalidSplitPath),
        }
    }

    fn collect_pane_ids<'a>(&'a self, out: &mut Vec<&'a str>) {
        match self {
            Self::Leaf { pane_id, .. } => out.push(pane_id.as_str()),
            Self::Split { first, second, .. } => {
                first.collect_pane_ids(out);
                second.collect_pane_ids(out);
            }
        }
    }

    fn validate_recursive(
        &self,
        depth: usize,
        seen_panes: &mut HashSet<String>,
    ) -> Result<(), SplitLayoutError> {
        if depth > MAX_SPLIT_TREE_DEPTH {
            return Err(SplitLayoutError::ExcessiveTreeDepth {
                max: MAX_SPLIT_TREE_DEPTH,
                actual: depth,
            });
        }

        match self {
            Self::Leaf { pane_id, .. } => {
                if pane_id.trim().is_empty() {
                    return Err(SplitLayoutError::EmptyPaneId);
                }
                if !seen_panes.insert(pane_id.clone()) {
                    return Err(SplitLayoutError::DuplicatePaneId(pane_id.clone()));
                }
                if seen_panes.len() > MAX_PANES_PER_PRESET {
                    return Err(SplitLayoutError::TooManyPanes {
                        max: MAX_PANES_PER_PRESET,
                        actual: seen_panes.len(),
                    });
                }
                Ok(())
            }
            Self::Split {
                ratio_bps,
                first,
                second,
                ..
            } => {
                if !(MIN_RATIO_BPS..=MAX_RATIO_BPS).contains(ratio_bps) {
                    return Err(SplitLayoutError::InvalidRatioBps(*ratio_bps));
                }
                first.validate_recursive(depth + 1, seen_panes)?;
                second.validate_recursive(depth + 1, seen_panes)?;
                Ok(())
            }
        }
    }

    fn resolve_recursive(
        &self,
        rect: ViewportRect,
        gap_px: u16,
        out: &mut Vec<PanePlacement>,
    ) -> Result<(), SplitLayoutError> {
        match self {
            Self::Leaf {
                pane_id,
                min_width,
                min_height,
            } => {
                if rect.width < *min_width || rect.height < *min_height {
                    return Err(SplitLayoutError::ViewportTooSmall {
                        required_width: *min_width,
                        required_height: *min_height,
                        available_width: rect.width,
                        available_height: rect.height,
                    });
                }
                out.push(PanePlacement {
                    pane_id: pane_id.clone(),
                    rect,
                });
                Ok(())
            }
            Self::Split {
                axis,
                ratio_bps,
                first,
                second,
            } => {
                let gap = u32::from(gap_px);
                let (min_w1, min_h1) = first.minimum_size(gap_px);
                let (min_w2, min_h2) = second.minimum_size(gap_px);

                match axis {
                    SplitAxis::Horizontal => {
                        let req_w = min_w1.saturating_add(min_w2).saturating_add(gap);
                        let req_h = min_h1.max(min_h2);
                        if rect.width < req_w || rect.height < req_h {
                            return Err(SplitLayoutError::ViewportTooSmall {
                                required_width: req_w,
                                required_height: req_h,
                                available_width: rect.width,
                                available_height: rect.height,
                            });
                        }

                        let usable_w = rect.width.saturating_sub(gap);
                        let first_w = allocate_axis_span(usable_w, *ratio_bps, min_w1, min_w2);
                        let second_w = usable_w.saturating_sub(first_w);

                        let first_rect = ViewportRect::new(rect.x, rect.y, first_w, rect.height);
                        let second_rect = ViewportRect::new(
                            rect.x.saturating_add(first_w).saturating_add(gap),
                            rect.y,
                            second_w,
                            rect.height,
                        );

                        first.resolve_recursive(first_rect, gap_px, out)?;
                        second.resolve_recursive(second_rect, gap_px, out)?;
                        Ok(())
                    }
                    SplitAxis::Vertical => {
                        let req_w = min_w1.max(min_w2);
                        let req_h = min_h1.saturating_add(min_h2).saturating_add(gap);
                        if rect.width < req_w || rect.height < req_h {
                            return Err(SplitLayoutError::ViewportTooSmall {
                                required_width: req_w,
                                required_height: req_h,
                                available_width: rect.width,
                                available_height: rect.height,
                            });
                        }

                        let usable_h = rect.height.saturating_sub(gap);
                        let first_h = allocate_axis_span(usable_h, *ratio_bps, min_h1, min_h2);
                        let second_h = usable_h.saturating_sub(first_h);

                        let first_rect = ViewportRect::new(rect.x, rect.y, rect.width, first_h);
                        let second_rect = ViewportRect::new(
                            rect.x,
                            rect.y.saturating_add(first_h).saturating_add(gap),
                            rect.width,
                            second_h,
                        );

                        first.resolve_recursive(first_rect, gap_px, out)?;
                        second.resolve_recursive(second_rect, gap_px, out)?;
                        Ok(())
                    }
                }
            }
        }
    }
}

fn allocate_axis_span(usable: u32, ratio_bps: u16, min_first: u32, min_second: u32) -> u32 {
    let raw = ((u64::from(usable) * u64::from(ratio_bps)) / 10_000) as u32;
    let max_first = usable.saturating_sub(min_second);
    raw.clamp(min_first, max_first)
}

fn is_valid_preset_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl SplitPreset {
    /// Validates preset metadata, tree depth, pane uniqueness, and split ratios.
    pub fn validate(&self) -> Result<(), SplitLayoutError> {
        if !is_valid_preset_id(&self.id) {
            return Err(SplitLayoutError::InvalidPresetId(self.id.clone()));
        }
        if self.name.trim().is_empty() {
            return Err(SplitLayoutError::EmptyPresetName);
        }
        let mut seen_panes = HashSet::new();
        self.root.validate_recursive(1, &mut seen_panes)
    }

    /// Returns all pane IDs in depth-first left-to-right order.
    pub fn pane_ids(&self) -> Vec<&str> {
        let mut ids = Vec::new();
        self.root.collect_pane_ids(&mut ids);
        ids
    }

    /// Resolves the preset into concrete pane rectangles for the given `viewport`.
    pub fn resolve(&self, viewport: ViewportRect) -> Result<Vec<PanePlacement>, SplitLayoutError> {
        self.validate()?;
        let mut placements = Vec::new();
        self.root
            .resolve_recursive(viewport, self.gap_px, &mut placements)?;
        Ok(placements)
    }
}

/// Registry managing built-in and user-defined split presets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitPresetRegistry {
    presets: BTreeMap<String, SplitPreset>,
    active_preset_id: Option<String>,
}

impl Default for SplitPresetRegistry {
    fn default() -> Self {
        Self::with_builtins()
    }
}

impl SplitPresetRegistry {
    /// Creates an empty registry with no presets.
    pub fn empty() -> Self {
        Self {
            presets: BTreeMap::new(),
            active_preset_id: None,
        }
    }

    /// Creates a registry pre-populated with standard Search Linux split presets.
    pub fn with_builtins() -> Self {
        let mut registry = Self::empty();
        let builtins = [
            SplitPreset {
                id: "single".to_string(),
                name: "Single Pane".to_string(),
                description: "Full-viewport single pane".to_string(),
                builtin: true,
                gap_px: 0,
                root: SplitNode::leaf("main"),
            },
            SplitPreset {
                id: "split-50-50-h".to_string(),
                name: "Side by Side (50/50)".to_string(),
                description: "Two equal vertical columns".to_string(),
                builtin: true,
                gap_px: 4,
                root: SplitNode::split(
                    SplitAxis::Horizontal,
                    5_000,
                    SplitNode::leaf_with_min("left", 240, 200),
                    SplitNode::leaf_with_min("right", 240, 200),
                ),
            },
            SplitPreset {
                id: "split-50-50-v".to_string(),
                name: "Stacked (50/50)".to_string(),
                description: "Two equal horizontal rows".to_string(),
                builtin: true,
                gap_px: 4,
                root: SplitNode::split(
                    SplitAxis::Vertical,
                    5_000,
                    SplitNode::leaf_with_min("top", 240, 180),
                    SplitNode::leaf_with_min("bottom", 240, 180),
                ),
            },
            SplitPreset {
                id: "focus-left-70-30".to_string(),
                name: "Primary + Sidebar (70/30)".to_string(),
                description: "Wide primary pane on the left with a companion sidebar on the right"
                    .to_string(),
                builtin: true,
                gap_px: 4,
                root: SplitNode::split(
                    SplitAxis::Horizontal,
                    7_000,
                    SplitNode::leaf_with_min("primary", 320, 200),
                    SplitNode::leaf_with_min("sidebar", 200, 200),
                ),
            },
            SplitPreset {
                id: "three-columns".to_string(),
                name: "Three Columns".to_string(),
                description: "Three balanced vertical columns".to_string(),
                builtin: true,
                gap_px: 4,
                root: SplitNode::split(
                    SplitAxis::Horizontal,
                    3_333,
                    SplitNode::leaf_with_min("col-1", 200, 200),
                    SplitNode::split(
                        SplitAxis::Horizontal,
                        5_000,
                        SplitNode::leaf_with_min("col-2", 200, 200),
                        SplitNode::leaf_with_min("col-3", 200, 200),
                    ),
                ),
            },
            SplitPreset {
                id: "grid-2x2".to_string(),
                name: "Quad Grid (2x2)".to_string(),
                description: "Four panes arranged in a 2x2 grid".to_string(),
                builtin: true,
                gap_px: 4,
                root: SplitNode::split(
                    SplitAxis::Vertical,
                    5_000,
                    SplitNode::split(
                        SplitAxis::Horizontal,
                        5_000,
                        SplitNode::leaf_with_min("top-left", 200, 160),
                        SplitNode::leaf_with_min("top-right", 200, 160),
                    ),
                    SplitNode::split(
                        SplitAxis::Horizontal,
                        5_000,
                        SplitNode::leaf_with_min("bottom-left", 200, 160),
                        SplitNode::leaf_with_min("bottom-right", 200, 160),
                    ),
                ),
            },
        ];

        for preset in builtins {
            registry.presets.insert(preset.id.clone(), preset);
        }
        registry.active_preset_id = Some("single".to_string());
        registry
    }

    /// Registers a new preset after validating it and checking for duplicate IDs.
    pub fn register_preset(&mut self, preset: SplitPreset) -> Result<(), SplitLayoutError> {
        preset.validate()?;
        if self.presets.contains_key(&preset.id) {
            return Err(SplitLayoutError::DuplicatePresetId(preset.id));
        }
        self.presets.insert(preset.id.clone(), preset);
        Ok(())
    }

    /// Removes a custom preset by ID. Built-in presets cannot be removed.
    pub fn remove_preset(&mut self, id: &str) -> Result<SplitPreset, SplitLayoutError> {
        let Some(existing) = self.presets.get(id) else {
            return Err(SplitLayoutError::PresetNotFound(id.to_string()));
        };
        if existing.builtin {
            return Err(SplitLayoutError::CannotDeleteBuiltinPreset(id.to_string()));
        }
        let removed = self
            .presets
            .remove(id)
            .ok_or_else(|| SplitLayoutError::PresetNotFound(id.to_string()))?;
        if self.active_preset_id.as_deref() == Some(id) {
            self.active_preset_id = self.presets.keys().next().cloned();
        }
        Ok(removed)
    }

    /// Sets the active preset by ID.
    pub fn activate_preset(&mut self, id: &str) -> Result<&SplitPreset, SplitLayoutError> {
        if !self.presets.contains_key(id) {
            return Err(SplitLayoutError::PresetNotFound(id.to_string()));
        }
        self.active_preset_id = Some(id.to_string());
        self.presets
            .get(id)
            .ok_or_else(|| SplitLayoutError::PresetNotFound(id.to_string()))
    }

    /// Returns the currently active preset, if any.
    pub fn active_preset(&self) -> Option<&SplitPreset> {
        self.active_preset_id
            .as_deref()
            .and_then(|id| self.presets.get(id))
    }

    /// Returns a preset by ID.
    pub fn get(&self, id: &str) -> Option<&SplitPreset> {
        self.presets.get(id)
    }

    /// Lists all presets sorted deterministically by ID.
    pub fn list_presets(&self) -> Vec<&SplitPreset> {
        self.presets.values().collect()
    }

    /// Serializes the registry to pretty-printed JSON.
    pub fn export_json(&self) -> Result<String, SplitLayoutError> {
        serde_json::to_string_pretty(self)
            .map_err(|err| SplitLayoutError::SerializationError(err.to_string()))
    }

    /// Deserializes and validates a registry from JSON.
    pub fn import_json(json: &str) -> Result<Self, SplitLayoutError> {
        let registry: Self = serde_json::from_str(json)
            .map_err(|err| SplitLayoutError::SerializationError(err.to_string()))?;
        for (key, preset) in &registry.presets {
            if key != &preset.id {
                return Err(SplitLayoutError::InvalidPresetId(key.clone()));
            }
            preset.validate()?;
        }
        if let Some(active_id) = &registry.active_preset_id {
            if !registry.presets.contains_key(active_id) {
                return Err(SplitLayoutError::PresetNotFound(active_id.clone()));
            }
        }
        Ok(registry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_builtin_quad_grid_without_overlap() {
        let registry = SplitPresetRegistry::with_builtins();
        let quad = registry.get("grid-2x2").unwrap();
        let viewport = ViewportRect::new(0, 0, 1204, 804);
        let placements = quad.resolve(viewport).unwrap();

        assert_eq!(placements.len(), 4);
        assert_eq!(
            placements[0],
            PanePlacement {
                pane_id: "top-left".to_string(),
                rect: ViewportRect::new(0, 0, 600, 400),
            }
        );
        assert_eq!(
            placements[1],
            PanePlacement {
                pane_id: "top-right".to_string(),
                rect: ViewportRect::new(604, 0, 600, 400),
            }
        );
        assert_eq!(
            placements[2],
            PanePlacement {
                pane_id: "bottom-left".to_string(),
                rect: ViewportRect::new(0, 404, 600, 400),
            }
        );
        assert_eq!(
            placements[3],
            PanePlacement {
                pane_id: "bottom-right".to_string(),
                rect: ViewportRect::new(604, 404, 600, 400),
            }
        );
    }

    #[test]
    fn enforces_minimum_pane_dimensions_and_rejects_too_small_viewport() {
        let preset = SplitPreset {
            id: "clamped-split".to_string(),
            name: "Clamped Split".to_string(),
            description: "Tests minimum width clamping".to_string(),
            builtin: false,
            gap_px: 10,
            root: SplitNode::split(
                SplitAxis::Horizontal,
                1_000, // 10% of 1000 = 100, should clamp up to min_width 300
                SplitNode::leaf_with_min("left", 300, 100),
                SplitNode::leaf_with_min("right", 200, 100),
            ),
        };

        let placements = preset.resolve(ViewportRect::new(0, 0, 1010, 500)).unwrap();
        assert_eq!(placements[0].rect.width, 300);
        assert_eq!(placements[1].rect.x, 310);
        assert_eq!(placements[1].rect.width, 700);

        let err = preset
            .resolve(ViewportRect::new(0, 0, 500, 500))
            .unwrap_err();
        assert_eq!(
            err,
            SplitLayoutError::ViewportTooSmall {
                required_width: 510,
                required_height: 100,
                available_width: 500,
                available_height: 500,
            }
        );
    }

    #[test]
    fn registry_lifecycle_validation_and_json_roundtrip() {
        let mut registry = SplitPresetRegistry::with_builtins();
        assert_eq!(registry.active_preset().unwrap().id, "single");

        let mut custom = SplitPreset {
            id: "dev-layout".to_string(),
            name: "Developer Layout".to_string(),
            description: "Editor + terminal".to_string(),
            builtin: false,
            gap_px: 6,
            root: SplitNode::split(
                SplitAxis::Vertical,
                7_500,
                SplitNode::leaf("editor"),
                SplitNode::leaf("terminal"),
            ),
        };
        custom.root.set_ratio_at_path(&[], 8_000).unwrap();

        registry.register_preset(custom).unwrap();
        registry.activate_preset("dev-layout").unwrap();
        assert_eq!(registry.active_preset().unwrap().id, "dev-layout");

        let json = registry.export_json().unwrap();
        let restored = SplitPresetRegistry::import_json(&json).unwrap();
        assert_eq!(restored, registry);

        // Built-in presets cannot be removed
        assert_eq!(
            registry.remove_preset("single").unwrap_err(),
            SplitLayoutError::CannotDeleteBuiltinPreset("single".to_string())
        );

        // Custom preset can be removed and active preset falls back cleanly
        registry.remove_preset("dev-layout").unwrap();
        assert!(registry.get("dev-layout").is_none());
        assert!(registry.active_preset().is_some());
    }

    #[test]
    fn rejects_invalid_presets_and_duplicate_panes() {
        let dup_panes = SplitPreset {
            id: "bad-panes".to_string(),
            name: "Bad".to_string(),
            description: String::new(),
            builtin: false,
            gap_px: 0,
            root: SplitNode::split(
                SplitAxis::Horizontal,
                5_000,
                SplitNode::leaf("same"),
                SplitNode::leaf("same"),
            ),
        };
        assert_eq!(
            dup_panes.validate().unwrap_err(),
            SplitLayoutError::DuplicatePaneId("same".to_string())
        );

        let bad_ratio = SplitPreset {
            id: "bad-ratio".to_string(),
            name: "Bad Ratio".to_string(),
            description: String::new(),
            builtin: false,
            gap_px: 0,
            root: SplitNode::split(
                SplitAxis::Horizontal,
                10_000,
                SplitNode::leaf("a"),
                SplitNode::leaf("b"),
            ),
        };
        assert_eq!(
            bad_ratio.validate().unwrap_err(),
            SplitLayoutError::InvalidRatioBps(10_000)
        );
    }
}
