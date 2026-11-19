use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors returned when validating, updating, or serializing gesture and mouse action configs.
#[derive(Debug, Error)]
pub enum GestureError {
    #[error("invalid gesture configuration: {0}")]
    InvalidConfig(String),
    #[error("binding id must not be empty")]
    EmptyBindingId,
    #[error("binding name must not be empty")]
    EmptyBindingName,
    #[error("duplicate binding id: {0}")]
    DuplicateBindingId(String),
    #[error("stroke gesture sequence must not be empty")]
    EmptyStrokeSequence,
    #[error("stroke sequence length {len} exceeds maximum {max}")]
    StrokeTooLong { len: usize, max: usize },
    #[error("rocker gesture requires distinct held and clicked mouse buttons")]
    InvalidRockerButtons,
    #[error("invalid hex trail color: {0}")]
    InvalidColorHex(String),
    #[error("custom command id must not be empty")]
    InvalidCustomCommand,
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Quantized directional segment of a mouse stroke gesture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GestureDirection {
    Up,
    Down,
    Left,
    Right,
    UpLeft,
    UpRight,
    DownLeft,
    DownRight,
}

impl GestureDirection {
    /// Returns a compact single/two-letter code for display in gesture HUDs.
    pub fn short_code(self) -> &'static str {
        match self {
            Self::Up => "U",
            Self::Down => "D",
            Self::Left => "L",
            Self::Right => "R",
            Self::UpLeft => "UL",
            Self::UpRight => "UR",
            Self::DownLeft => "DL",
            Self::DownRight => "DR",
        }
    }
}

/// Physical mouse button used to initiate or complete a gesture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    Back,
    Forward,
}

/// Keyboard modifier that can be combined with wheel or edge mouse actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModifierKey {
    Ctrl,
    Alt,
    Shift,
    Super,
}

/// Scroll wheel direction for wheel-based mouse actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WheelDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Viewport or screen edge region for edge-click mouse actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreenEdge {
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Input trigger that activates a bound mouse action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MouseTrigger {
    Stroke {
        button: MouseButton,
        sequence: Vec<GestureDirection>,
    },
    Rocker {
        held: MouseButton,
        clicked: MouseButton,
    },
    Wheel {
        held: Option<MouseButton>,
        modifiers: Vec<ModifierKey>,
        direction: WheelDirection,
    },
    EdgeClick {
        edge: ScreenEdge,
        button: MouseButton,
        modifiers: Vec<ModifierKey>,
    },
}

/// Browser or application action executed when a mouse trigger matches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MouseAction {
    NavigateBack,
    NavigateForward,
    Reload { bypass_cache: bool },
    StopLoading,
    NewTab { background: bool },
    CloseTab,
    ReopenClosedTab,
    NextTab,
    PreviousTab,
    ScrollTop,
    ScrollBottom,
    DuplicateTab,
    ToggleFullscreen,
    OpenSearchBar,
    CustomCommand { command_id: String },
}

/// Named binding between a mouse trigger and an action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GestureBinding {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub trigger: MouseTrigger,
    pub action: MouseAction,
    pub priority: i32,
}

/// 2D pointer coordinate in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point2D {
    pub x: f64,
    pub y: f64,
}

impl Point2D {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance_to(self, other: Self) -> f64 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        (dx * dx + dy * dy).sqrt()
    }
}

/// Result of matching a pointer stroke against configured bindings.
#[derive(Debug, Clone, PartialEq)]
pub struct RecognitionMatch {
    pub binding_id: String,
    pub action: MouseAction,
    pub sequence: Vec<GestureDirection>,
    pub total_distance_px: f64,
}

/// Complete configuration for mouse gestures, rocker gestures, wheel, and edge actions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GestureConfig {
    pub enabled: bool,
    pub min_segment_px: f64,
    pub jitter_tolerance_px: f64,
    pub diagonal_mode: bool,
    pub max_stroke_len: usize,
    pub rocker_enabled: bool,
    pub wheel_enabled: bool,
    pub trail_enabled: bool,
    pub trail_color_hex: String,
    pub trail_width_px: f32,
    pub bindings: Vec<GestureBinding>,
}

impl Default for GestureConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_segment_px: 24.0,
            jitter_tolerance_px: 8.0,
            diagonal_mode: false,
            max_stroke_len: 12,
            rocker_enabled: true,
            wheel_enabled: true,
            trail_enabled: true,
            trail_color_hex: "#3584E4".to_string(),
            trail_width_px: 3.0,
            bindings: vec![
                GestureBinding {
                    id: "nav_back".to_string(),
                    name: "Navigate Back".to_string(),
                    enabled: true,
                    trigger: MouseTrigger::Stroke {
                        button: MouseButton::Right,
                        sequence: vec![GestureDirection::Left],
                    },
                    action: MouseAction::NavigateBack,
                    priority: 10,
                },
                GestureBinding {
                    id: "nav_forward".to_string(),
                    name: "Navigate Forward".to_string(),
                    enabled: true,
                    trigger: MouseTrigger::Stroke {
                        button: MouseButton::Right,
                        sequence: vec![GestureDirection::Right],
                    },
                    action: MouseAction::NavigateForward,
                    priority: 10,
                },
                GestureBinding {
                    id: "new_tab".to_string(),
                    name: "New Tab".to_string(),
                    enabled: true,
                    trigger: MouseTrigger::Stroke {
                        button: MouseButton::Right,
                        sequence: vec![GestureDirection::Up],
                    },
                    action: MouseAction::NewTab { background: false },
                    priority: 10,
                },
                GestureBinding {
                    id: "close_tab".to_string(),
                    name: "Close Tab".to_string(),
                    enabled: true,
                    trigger: MouseTrigger::Stroke {
                        button: MouseButton::Right,
                        sequence: vec![GestureDirection::Down, GestureDirection::Right],
                    },
                    action: MouseAction::CloseTab,
                    priority: 10,
                },
                GestureBinding {
                    id: "rocker_back".to_string(),
                    name: "Rocker Back".to_string(),
                    enabled: true,
                    trigger: MouseTrigger::Rocker {
                        held: MouseButton::Right,
                        clicked: MouseButton::Left,
                    },
                    action: MouseAction::NavigateBack,
                    priority: 5,
                },
                GestureBinding {
                    id: "rocker_forward".to_string(),
                    name: "Rocker Forward".to_string(),
                    enabled: true,
                    trigger: MouseTrigger::Rocker {
                        held: MouseButton::Left,
                        clicked: MouseButton::Right,
                    },
                    action: MouseAction::NavigateForward,
                    priority: 5,
                },
            ],
        }
    }
}

impl GestureConfig {
    /// Validates all configuration parameters and every registered binding.
    pub fn validate(&self) -> Result<(), GestureError> {
        if !self.min_segment_px.is_finite() || self.min_segment_px <= 0.0 {
            return Err(GestureError::InvalidConfig(
                "min_segment_px must be a finite positive number".to_string(),
            ));
        }
        if !self.jitter_tolerance_px.is_finite() || self.jitter_tolerance_px < 0.0 {
            return Err(GestureError::InvalidConfig(
                "jitter_tolerance_px must be a finite non-negative number".to_string(),
            ));
        }
        if self.max_stroke_len == 0 {
            return Err(GestureError::InvalidConfig(
                "max_stroke_len must be at least 1".to_string(),
            ));
        }
        if !self.trail_width_px.is_finite() || self.trail_width_px <= 0.0 {
            return Err(GestureError::InvalidConfig(
                "trail_width_px must be a finite positive number".to_string(),
            ));
        }
        validate_hex_color(&self.trail_color_hex)?;

        let mut seen_ids = HashSet::with_capacity(self.bindings.len());
        for binding in &self.bindings {
            self.validate_binding(binding)?;
            if !seen_ids.insert(binding.id.as_str()) {
                return Err(GestureError::DuplicateBindingId(binding.id.clone()));
            }
        }
        Ok(())
    }

    fn validate_binding(&self, binding: &GestureBinding) -> Result<(), GestureError> {
        if binding.id.trim().is_empty() {
            return Err(GestureError::EmptyBindingId);
        }
        if binding.name.trim().is_empty() {
            return Err(GestureError::EmptyBindingName);
        }
        match &binding.trigger {
            MouseTrigger::Stroke { sequence, .. } => {
                if sequence.is_empty() {
                    return Err(GestureError::EmptyStrokeSequence);
                }
                if sequence.len() > self.max_stroke_len {
                    return Err(GestureError::StrokeTooLong {
                        len: sequence.len(),
                        max: self.max_stroke_len,
                    });
                }
            }
            MouseTrigger::Rocker { held, clicked } => {
                if held == clicked {
                    return Err(GestureError::InvalidRockerButtons);
                }
            }
            MouseTrigger::Wheel { .. } | MouseTrigger::EdgeClick { .. } => {}
        }
        if let MouseAction::CustomCommand { command_id } = &binding.action {
            if command_id.trim().is_empty() {
                return Err(GestureError::InvalidCustomCommand);
            }
        }
        Ok(())
    }

    /// Adds a new binding after validating it and checking for duplicate IDs.
    pub fn add_binding(&mut self, binding: GestureBinding) -> Result<(), GestureError> {
        self.validate_binding(&binding)?;
        if self.bindings.iter().any(|b| b.id == binding.id) {
            return Err(GestureError::DuplicateBindingId(binding.id));
        }
        self.bindings.push(binding);
        Ok(())
    }

    /// Removes a binding by ID, returning `true` if a binding was removed.
    pub fn remove_binding(&mut self, id: &str) -> bool {
        let before = self.bindings.len();
        self.bindings.retain(|b| b.id != id);
        self.bindings.len() != before
    }

    /// Enables or disables a binding by ID, returning `true` if found.
    pub fn set_binding_enabled(&mut self, id: &str, enabled: bool) -> bool {
        if let Some(binding) = self.bindings.iter_mut().find(|b| b.id == id) {
            binding.enabled = enabled;
            true
        } else {
            false
        }
    }

    /// Serializes the configuration to pretty-printed JSON after validation.
    pub fn to_json(&self) -> Result<String, GestureError> {
        self.validate()?;
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Deserializes and validates a configuration from JSON.
    pub fn from_json(raw: &str) -> Result<Self, GestureError> {
        let config: Self = serde_json::from_str(raw)?;
        config.validate()?;
        Ok(config)
    }
}

/// Evaluates pointer strokes, rocker clicks, wheel events, and edge clicks against a config.
#[derive(Debug, Clone)]
pub struct GestureRecognizer {
    config: GestureConfig,
}

impl GestureRecognizer {
    /// Creates a new recognizer after validating the provided configuration.
    pub fn new(config: GestureConfig) -> Result<Self, GestureError> {
        config.validate()?;
        Ok(Self { config })
    }

    /// Returns a reference to the active configuration.
    pub fn config(&self) -> &GestureConfig {
        &self.config
    }

    /// Quantizes a pointer path into directional segments and finds the highest-priority match.
    pub fn match_stroke(
        &self,
        button: MouseButton,
        points: &[Point2D],
    ) -> Option<RecognitionMatch> {
        if !self.config.enabled || points.len() < 2 {
            return None;
        }

        let (sequence, total_distance_px) = quantize_points(
            points,
            self.config.min_segment_px,
            self.config.jitter_tolerance_px,
            self.config.diagonal_mode,
        );

        if sequence.is_empty() || sequence.len() > self.config.max_stroke_len {
            return None;
        }

        let matched = self
            .config
            .bindings
            .iter()
            .filter(|b| b.enabled)
            .filter(|b| match &b.trigger {
                MouseTrigger::Stroke {
                    button: b_btn,
                    sequence: b_seq,
                } => *b_btn == button && *b_seq == sequence,
                _ => false,
            })
            .max_by_key(|b| b.priority)?;

        Some(RecognitionMatch {
            binding_id: matched.id.clone(),
            action: matched.action.clone(),
            sequence,
            total_distance_px,
        })
    }

    /// Matches a rocker gesture (holding one mouse button while clicking another).
    pub fn match_rocker(&self, held: MouseButton, clicked: MouseButton) -> Option<&GestureBinding> {
        if !self.config.enabled || !self.config.rocker_enabled || held == clicked {
            return None;
        }
        self.config
            .bindings
            .iter()
            .filter(|b| b.enabled)
            .filter(|b| {
                matches!(
                    b.trigger,
                    MouseTrigger::Rocker {
                        held: h,
                        clicked: c
                    } if h == held && c == clicked
                )
            })
            .max_by_key(|b| b.priority)
    }

    /// Matches a scroll wheel gesture with optional held button and modifier keys.
    pub fn match_wheel(
        &self,
        held: Option<MouseButton>,
        modifiers: &[ModifierKey],
        direction: WheelDirection,
    ) -> Option<&GestureBinding> {
        if !self.config.enabled || !self.config.wheel_enabled {
            return None;
        }
        self.config
            .bindings
            .iter()
            .filter(|b| b.enabled)
            .filter(|b| match &b.trigger {
                MouseTrigger::Wheel {
                    held: b_held,
                    modifiers: b_mods,
                    direction: b_dir,
                } => *b_held == held && *b_dir == direction && modifiers_match(b_mods, modifiers),
                _ => false,
            })
            .max_by_key(|b| b.priority)
    }

    /// Matches a click on a screen/viewport edge with optional modifier keys.
    pub fn match_edge(
        &self,
        edge: ScreenEdge,
        button: MouseButton,
        modifiers: &[ModifierKey],
    ) -> Option<&GestureBinding> {
        if !self.config.enabled {
            return None;
        }
        self.config
            .bindings
            .iter()
            .filter(|b| b.enabled)
            .filter(|b| match &b.trigger {
                MouseTrigger::EdgeClick {
                    edge: b_edge,
                    button: b_btn,
                    modifiers: b_mods,
                } => *b_edge == edge && *b_btn == button && modifiers_match(b_mods, modifiers),
                _ => false,
            })
            .max_by_key(|b| b.priority)
    }
}

/// Quantizes raw pointer coordinates into collapsed `GestureDirection` segments and total path distance.
pub fn quantize_points(
    points: &[Point2D],
    min_segment_px: f64,
    jitter_tolerance_px: f64,
    diagonal_mode: bool,
) -> (Vec<GestureDirection>, f64) {
    if points.len() < 2 || !min_segment_px.is_finite() || min_segment_px <= 0.0 {
        return (Vec::new(), 0.0);
    }

    let mut sequence: Vec<GestureDirection> = Vec::new();
    let mut total_distance = 0.0;
    let mut anchor = points[0];
    let mut prev = points[0];
    let mut past_jitter = jitter_tolerance_px <= 0.0;

    for &pt in &points[1..] {
        if !pt.x.is_finite() || !pt.y.is_finite() {
            continue;
        }
        total_distance += prev.distance_to(pt);
        prev = pt;

        let dist_from_anchor = anchor.distance_to(pt);
        if !past_jitter {
            if dist_from_anchor < jitter_tolerance_px {
                continue;
            }
            past_jitter = true;
        }

        if dist_from_anchor >= min_segment_px {
            let dir = classify_vector(pt.x - anchor.x, pt.y - anchor.y, diagonal_mode);
            if sequence.last().copied() != Some(dir) {
                sequence.push(dir);
            }
            anchor = pt;
        }
    }

    (sequence, total_distance)
}

fn classify_vector(dx: f64, dy: f64, diagonal_mode: bool) -> GestureDirection {
    if !diagonal_mode {
        if dx.abs() >= dy.abs() {
            if dx >= 0.0 {
                GestureDirection::Right
            } else {
                GestureDirection::Left
            }
        } else if dy >= 0.0 {
            GestureDirection::Down
        } else {
            GestureDirection::Up
        }
    } else {
        // Screen coordinates: +x is right, +y is down.
        let angle = dy.atan2(dx).to_degrees();
        if (-22.5..22.5).contains(&angle) {
            GestureDirection::Right
        } else if (22.5..67.5).contains(&angle) {
            GestureDirection::DownRight
        } else if (67.5..112.5).contains(&angle) {
            GestureDirection::Down
        } else if (112.5..157.5).contains(&angle) {
            GestureDirection::DownLeft
        } else if !(-157.5..157.5).contains(&angle) {
            GestureDirection::Left
        } else if (-157.5..-112.5).contains(&angle) {
            GestureDirection::UpLeft
        } else if (-112.5..-67.5).contains(&angle) {
            GestureDirection::Up
        } else {
            GestureDirection::UpRight
        }
    }
}

fn modifiers_match(expected: &[ModifierKey], actual: &[ModifierKey]) -> bool {
    let expected_set: HashSet<ModifierKey> = expected.iter().copied().collect();
    let actual_set: HashSet<ModifierKey> = actual.iter().copied().collect();
    expected_set == actual_set
}

fn validate_hex_color(color: &str) -> Result<(), GestureError> {
    let bytes = color.as_bytes();
    let valid_len = bytes.len() == 7 || bytes.len() == 9;
    if !valid_len || bytes.first() != Some(&b'#') || !bytes[1..].iter().all(u8::is_ascii_hexdigit) {
        return Err(GestureError::InvalidColorHex(color.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stroke_recognition_and_quantization_happy_path() {
        let config = GestureConfig::default();
        let recognizer = GestureRecognizer::new(config).expect("default config should be valid");

        // Down then Right -> CloseTab
        let points = vec![
            Point2D::new(100.0, 100.0),
            Point2D::new(102.0, 103.0), // within jitter
            Point2D::new(101.0, 140.0), // Down
            Point2D::new(101.0, 180.0), // Still Down (collapsed)
            Point2D::new(150.0, 182.0), // Right
        ];

        let matched = recognizer
            .match_stroke(MouseButton::Right, &points)
            .expect("should match close_tab");

        assert_eq!(matched.binding_id, "close_tab");
        assert_eq!(matched.action, MouseAction::CloseTab);
        assert_eq!(
            matched.sequence,
            vec![GestureDirection::Down, GestureDirection::Right]
        );
        assert!(matched.total_distance_px > 120.0);
    }

    #[test]
    fn diagonal_mode_and_priority_resolution() {
        let mut config = GestureConfig {
            diagonal_mode: true,
            ..GestureConfig::default()
        };
        config
            .add_binding(GestureBinding {
                id: "diag_low".to_string(),
                name: "Diag Low".to_string(),
                enabled: true,
                trigger: MouseTrigger::Stroke {
                    button: MouseButton::Right,
                    sequence: vec![GestureDirection::DownRight],
                },
                action: MouseAction::ScrollBottom,
                priority: 1,
            })
            .expect("add low");
        config
            .add_binding(GestureBinding {
                id: "diag_high".to_string(),
                name: "Diag High".to_string(),
                enabled: true,
                trigger: MouseTrigger::Stroke {
                    button: MouseButton::Right,
                    sequence: vec![GestureDirection::DownRight],
                },
                action: MouseAction::ToggleFullscreen,
                priority: 50,
            })
            .expect("add high");

        let recognizer = GestureRecognizer::new(config).expect("valid config");
        let points = vec![Point2D::new(0.0, 0.0), Point2D::new(40.0, 40.0)];
        let matched = recognizer
            .match_stroke(MouseButton::Right, &points)
            .expect("should match high priority diagonal");
        assert_eq!(matched.binding_id, "diag_high");
        assert_eq!(matched.action, MouseAction::ToggleFullscreen);
    }

    #[test]
    fn rocker_wheel_and_edge_matching_with_modifiers() {
        let mut config = GestureConfig::default();
        config
            .add_binding(GestureBinding {
                id: "wheel_next_tab".to_string(),
                name: "Wheel Next Tab".to_string(),
                enabled: true,
                trigger: MouseTrigger::Wheel {
                    held: Some(MouseButton::Right),
                    modifiers: vec![ModifierKey::Ctrl, ModifierKey::Shift],
                    direction: WheelDirection::Down,
                },
                action: MouseAction::NextTab,
                priority: 10,
            })
            .expect("add wheel");
        config
            .add_binding(GestureBinding {
                id: "edge_search".to_string(),
                name: "Edge Search".to_string(),
                enabled: true,
                trigger: MouseTrigger::EdgeClick {
                    edge: ScreenEdge::TopLeft,
                    button: MouseButton::Left,
                    modifiers: vec![ModifierKey::Alt],
                },
                action: MouseAction::OpenSearchBar,
                priority: 10,
            })
            .expect("add edge");

        let recognizer = GestureRecognizer::new(config).expect("valid config");

        let rocker = recognizer
            .match_rocker(MouseButton::Right, MouseButton::Left)
            .expect("rocker back");
        assert_eq!(rocker.action, MouseAction::NavigateBack);
        assert!(recognizer
            .match_rocker(MouseButton::Left, MouseButton::Left)
            .is_none());

        // Order-independent modifier matching
        let wheel = recognizer
            .match_wheel(
                Some(MouseButton::Right),
                &[ModifierKey::Shift, ModifierKey::Ctrl],
                WheelDirection::Down,
            )
            .expect("wheel next tab");
        assert_eq!(wheel.action, MouseAction::NextTab);

        let edge = recognizer
            .match_edge(ScreenEdge::TopLeft, MouseButton::Left, &[ModifierKey::Alt])
            .expect("edge search");
        assert_eq!(edge.action, MouseAction::OpenSearchBar);
    }

    #[test]
    fn validation_and_json_roundtrip_edge_cases() {
        let config = GestureConfig::default();
        let json = config.to_json().expect("serialize default");
        let parsed = GestureConfig::from_json(&json).expect("deserialize default");
        assert_eq!(config, parsed);

        // Invalid color hex
        let bad_color = GestureConfig {
            trail_color_hex: "blue".to_string(),
            ..Default::default()
        };
        assert!(matches!(
            bad_color.validate(),
            Err(GestureError::InvalidColorHex(_))
        ));

        // Duplicate binding id
        let mut dup = GestureConfig::default();
        let err = dup.add_binding(dup.bindings[0].clone()).unwrap_err();
        assert!(matches!(err, GestureError::DuplicateBindingId(_)));

        // Invalid rocker (same held and clicked)
        let mut same_rocker = GestureConfig::default();
        let err = same_rocker
            .add_binding(GestureBinding {
                id: "bad_rocker".to_string(),
                name: "Bad Rocker".to_string(),
                enabled: true,
                trigger: MouseTrigger::Rocker {
                    held: MouseButton::Left,
                    clicked: MouseButton::Left,
                },
                action: MouseAction::Reload {
                    bypass_cache: false,
                },
                priority: 1,
            })
            .unwrap_err();
        assert!(matches!(err, GestureError::InvalidRockerButtons));

        // Remove and toggle binding
        assert!(dup.set_binding_enabled("nav_back", false));
        assert!(!dup.bindings[0].enabled);
        assert!(dup.remove_binding("nav_back"));
        assert!(!dup.remove_binding("nonexistent"));
    }
}
