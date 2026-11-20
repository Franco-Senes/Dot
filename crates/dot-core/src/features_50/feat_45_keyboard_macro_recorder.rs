use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Maximum allowed length for a macro name.
pub const MAX_MACRO_NAME_LEN: usize = 64;

/// Maximum allowed length for a key identifier.
pub const MAX_KEY_NAME_LEN: usize = 32;

/// Minimum allowed playback speed multiplier.
pub const MIN_PLAYBACK_SPEED: f64 = 0.1;

/// Maximum allowed playback speed multiplier.
pub const MAX_PLAYBACK_SPEED: f64 = 10.0;

/// Errors returned by keyboard macro recording, validation, storage, and playback.
#[derive(Debug, Error, PartialEq)]
pub enum MacroError {
    #[error("a recording session is already active")]
    AlreadyRecording,

    #[error("no recording session is currently active")]
    NotRecording,

    #[error("macro name cannot be empty")]
    EmptyMacroName,

    #[error("invalid macro name: {0}")]
    InvalidMacroName(String),

    #[error("macro '{0}' was not found")]
    MacroNotFound(String),

    #[error("a macro named '{0}' already exists")]
    DuplicateMacroName(String),

    #[error("shortcut '{0}' is already bound to macro '{1}'")]
    ShortcutConflict(String, String),

    #[error("recording exceeded maximum step limit of {max}")]
    MaxStepsExceeded { max: usize },

    #[error("macro store exceeded maximum capacity of {max}")]
    MaxMacrosExceeded { max: usize },

    #[error("cannot save or play a macro with no steps")]
    EmptyMacro,

    #[error("invalid playback speed {0}; must be finite and within {MIN_PLAYBACK_SPEED}..={MAX_PLAYBACK_SPEED}")]
    InvalidPlaybackSpeed(f64),

    #[error("repeat count must be at least 1")]
    InvalidRepeatCount,

    #[error("invalid key or shortcut specification: {0}")]
    InvalidShortcut(String),

    #[error("invalid command action: {0}")]
    InvalidCommand(String),

    #[error("serialization error: {0}")]
    Serialization(String),
}

impl From<serde_json::Error> for MacroError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}

/// Modifier keys that can accompany a primary key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyModifier {
    Ctrl,
    Alt,
    Shift,
    Super,
}

impl KeyModifier {
    /// Parses a modifier token case-insensitively.
    pub fn parse(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Some(Self::Ctrl),
            "alt" | "option" => Some(Self::Alt),
            "shift" => Some(Self::Shift),
            "super" | "meta" | "win" | "cmd" | "command" => Some(Self::Super),
            _ => None,
        }
    }

    /// Returns the canonical display label for the modifier.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ctrl => "Ctrl",
            Self::Alt => "Alt",
            Self::Shift => "Shift",
            Self::Super => "Super",
        }
    }
}

impl fmt::Display for KeyModifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Physical or logical state transition of a key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyEventKind {
    Press,
    Release,
    Repeat,
}

/// Normalized representation of a key combined with zero or more modifiers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyChord {
    pub key: String,
    pub modifiers: Vec<KeyModifier>,
}

impl KeyChord {
    /// Creates a validated, canonicalized `KeyChord`.
    pub fn new(
        key: impl Into<String>,
        modifiers: impl IntoIterator<Item = KeyModifier>,
    ) -> Result<Self, MacroError> {
        let normalized_key = normalize_key_name(&key.into())?;
        let unique: BTreeSet<KeyModifier> = modifiers.into_iter().collect();
        Ok(Self {
            key: normalized_key,
            modifiers: unique.into_iter().collect(),
        })
    }

    /// Parses a shortcut string such as `"Ctrl+Shift+P"` or `"Enter"`.
    pub fn parse(spec: &str) -> Result<Self, MacroError> {
        let trimmed = spec.trim();
        if trimmed.is_empty() {
            return Err(MacroError::InvalidShortcut("empty shortcut".to_string()));
        }

        let parts: Vec<&str> = trimmed.split('+').map(str::trim).collect();
        if parts.iter().any(|p| p.is_empty()) {
            return Err(MacroError::InvalidShortcut(spec.to_string()));
        }

        let (key_part, mod_parts) = match parts.split_last() {
            Some(pair) => pair,
            None => return Err(MacroError::InvalidShortcut(spec.to_string())),
        };

        let mut modifiers = Vec::with_capacity(mod_parts.len());
        for part in mod_parts {
            let modifier = KeyModifier::parse(part)
                .ok_or_else(|| MacroError::InvalidShortcut(format!("unknown modifier '{part}'")))?;
            modifiers.push(modifier);
        }

        Self::new(*key_part, modifiers)
    }

    /// Formats the chord in canonical `Modifier+Key` order.
    pub fn format_canonical(&self) -> String {
        let mut out = String::new();
        for modifier in &self.modifiers {
            out.push_str(modifier.as_str());
            out.push('+');
        }
        out.push_str(&self.key);
        out
    }

    /// Returns true when the primary key itself is only a modifier key.
    pub fn is_modifier_only(&self) -> bool {
        KeyModifier::parse(&self.key).is_some()
    }
}

impl fmt::Display for KeyChord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format_canonical())
    }
}

/// A single recordable user action within a keyboard macro.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MacroAction {
    KeyEvent {
        chord: KeyChord,
        kind: KeyEventKind,
    },
    TextInput {
        text: String,
    },
    Command {
        command_id: String,
        args: Vec<String>,
    },
    Wait {
        duration_ms: u64,
    },
}

/// One step in a macro sequence with relative timing from the previous step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroStep {
    pub index: usize,
    pub action: MacroAction,
    pub delay_from_prev_ms: u64,
}

/// Optional invocation bindings for a saved macro.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroTrigger {
    pub shortcut: Option<KeyChord>,
    pub alias: Option<String>,
}

/// Complete persisted keyboard macro definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardMacro {
    pub id: String,
    pub name: String,
    pub description: String,
    pub steps: Vec<MacroStep>,
    pub trigger: MacroTrigger,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
    pub play_count: u64,
}

impl KeyboardMacro {
    /// Computes the unscaled total duration of the macro in milliseconds.
    pub fn total_duration_ms(&self) -> u64 {
        self.steps.iter().fold(0u64, |acc, step| {
            let action_wait = match &step.action {
                MacroAction::Wait { duration_ms } => *duration_ms,
                _ => 0,
            };
            acc.saturating_add(step.delay_from_prev_ms)
                .saturating_add(action_wait)
        })
    }

    /// Validates macro invariants before persistence or playback.
    pub fn validate(&self) -> Result<(), MacroError> {
        validate_macro_name(&self.name)?;
        if self.steps.is_empty() {
            return Err(MacroError::EmptyMacro);
        }
        for (expected_idx, step) in self.steps.iter().enumerate() {
            if step.index != expected_idx {
                return Err(MacroError::InvalidCommand(format!(
                    "step index mismatch at {expected_idx}: found {}",
                    step.index
                )));
            }
            if let MacroAction::Command { command_id, .. } = &step.action {
                if command_id.trim().is_empty() {
                    return Err(MacroError::InvalidCommand(
                        "command_id cannot be empty".to_string(),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Playback configuration applied when scheduling a macro run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaybackOptions {
    pub speed: f64,
    pub repeat_count: u32,
    pub ignore_delays: bool,
    pub max_step_delay_ms: Option<u64>,
}

impl Default for PlaybackOptions {
    fn default() -> Self {
        Self {
            speed: 1.0,
            repeat_count: 1,
            ignore_delays: false,
            max_step_delay_ms: None,
        }
    }
}

impl PlaybackOptions {
    /// Validates playback parameters.
    pub fn validate(&self) -> Result<(), MacroError> {
        if !self.speed.is_finite()
            || self.speed < MIN_PLAYBACK_SPEED
            || self.speed > MAX_PLAYBACK_SPEED
        {
            return Err(MacroError::InvalidPlaybackSpeed(self.speed));
        }
        if self.repeat_count == 0 {
            return Err(MacroError::InvalidRepeatCount);
        }
        Ok(())
    }
}

/// A concrete scheduled action produced for a playback runner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledPlaybackStep {
    pub iteration: u32,
    pub step_index: usize,
    pub at_offset_ms: u64,
    pub action: MacroAction,
}

/// Deterministic timeline of actions generated from a macro and playback options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybackPlan {
    pub macro_id: String,
    pub macro_name: String,
    pub total_duration_ms: u64,
    pub steps: Vec<ScheduledPlaybackStep>,
}

/// Configuration for the `KeyboardMacroRecorder` engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecorderConfig {
    pub max_steps_per_macro: usize,
    pub max_saved_macros: usize,
    pub coalesce_text_input: bool,
    pub max_recorded_delay_ms: u64,
    pub ignore_modifier_only_events: bool,
}

impl Default for RecorderConfig {
    fn default() -> Self {
        Self {
            max_steps_per_macro: 500,
            max_saved_macros: 100,
            coalesce_text_input: true,
            max_recorded_delay_ms: 30_000,
            ignore_modifier_only_events: true,
        }
    }
}

/// Active in-progress recording state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordingSession {
    pub name: String,
    pub description: String,
    pub started_at_unix_ms: u64,
    pub last_event_unix_ms: u64,
    pub steps: Vec<MacroStep>,
}

/// Stateful keyboard macro recorder, library, and playback planner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyboardMacroRecorder {
    config: RecorderConfig,
    active_session: Option<RecordingSession>,
    macros: BTreeMap<String, KeyboardMacro>,
    next_id_seq: u64,
}

impl Default for KeyboardMacroRecorder {
    fn default() -> Self {
        Self::new(RecorderConfig::default())
    }
}

impl KeyboardMacroRecorder {
    /// Creates a new recorder with the supplied configuration.
    pub fn new(config: RecorderConfig) -> Self {
        Self {
            config,
            active_session: None,
            macros: BTreeMap::new(),
            next_id_seq: 1,
        }
    }

    /// Returns true if a recording session is currently active.
    pub fn is_recording(&self) -> bool {
        self.active_session.is_some()
    }

    /// Returns the active recording session, if any.
    pub fn active_session(&self) -> Option<&RecordingSession> {
        self.active_session.as_ref()
    }

    /// Begins a new recording session with the given macro name and description.
    pub fn start_recording(
        &mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        now_unix_ms: u64,
    ) -> Result<(), MacroError> {
        if self.active_session.is_some() {
            return Err(MacroError::AlreadyRecording);
        }

        let clean_name = validate_macro_name(&name.into())?;
        if self.find_by_name(&clean_name).is_some() {
            return Err(MacroError::DuplicateMacroName(clean_name));
        }

        self.active_session = Some(RecordingSession {
            name: clean_name,
            description: description.into().trim().to_string(),
            started_at_unix_ms: now_unix_ms,
            last_event_unix_ms: now_unix_ms,
            steps: Vec::new(),
        });
        Ok(())
    }

    /// Records a key press, release, or repeat event into the active session.
    pub fn record_key_event(
        &mut self,
        chord: KeyChord,
        kind: KeyEventKind,
        now_unix_ms: u64,
    ) -> Result<(), MacroError> {
        if self.config.ignore_modifier_only_events && chord.is_modifier_only() {
            if self.active_session.is_none() {
                return Err(MacroError::NotRecording);
            }
            return Ok(());
        }

        self.push_action(MacroAction::KeyEvent { chord, kind }, now_unix_ms)
    }

    /// Records printable text input, coalescing consecutive text steps when configured.
    pub fn record_text_input(&mut self, text: &str, now_unix_ms: u64) -> Result<(), MacroError> {
        if text.is_empty() {
            if self.active_session.is_none() {
                return Err(MacroError::NotRecording);
            }
            return Ok(());
        }

        let max_steps = self.config.max_steps_per_macro;
        let max_delay = self.config.max_recorded_delay_ms;
        let coalesce = self.config.coalesce_text_input;

        let session = self
            .active_session
            .as_mut()
            .ok_or(MacroError::NotRecording)?;

        if coalesce {
            if let Some(last) = session.steps.last_mut() {
                if let MacroAction::TextInput { text: existing } = &mut last.action {
                    existing.push_str(text);
                    session.last_event_unix_ms = now_unix_ms.max(session.last_event_unix_ms);
                    return Ok(());
                }
            }
        }

        append_step(
            session,
            MacroAction::TextInput {
                text: text.to_string(),
            },
            now_unix_ms,
            max_steps,
            max_delay,
        )
    }

    /// Records a high-level application command (e.g. `"focus-omnibox"`).
    pub fn record_command(
        &mut self,
        command_id: &str,
        args: Vec<String>,
        now_unix_ms: u64,
    ) -> Result<(), MacroError> {
        let trimmed = command_id.trim();
        if trimmed.is_empty() {
            return Err(MacroError::InvalidCommand(
                "command_id cannot be empty".to_string(),
            ));
        }
        self.push_action(
            MacroAction::Command {
                command_id: trimmed.to_string(),
                args,
            },
            now_unix_ms,
        )
    }

    /// Cancels and discards the active recording session.
    pub fn cancel_recording(&mut self) -> Result<RecordingSession, MacroError> {
        self.active_session.take().ok_or(MacroError::NotRecording)
    }

    /// Finalizes the active recording session, validates it, and stores the macro.
    pub fn stop_and_save(
        &mut self,
        trigger: MacroTrigger,
        now_unix_ms: u64,
    ) -> Result<KeyboardMacro, MacroError> {
        let session = self
            .active_session
            .as_ref()
            .ok_or(MacroError::NotRecording)?;

        if session.steps.is_empty() {
            return Err(MacroError::EmptyMacro);
        }
        if self.macros.len() >= self.config.max_saved_macros {
            return Err(MacroError::MaxMacrosExceeded {
                max: self.config.max_saved_macros,
            });
        }
        if let Some(shortcut) = &trigger.shortcut {
            if let Some(existing) = self.find_by_shortcut(shortcut) {
                return Err(MacroError::ShortcutConflict(
                    shortcut.format_canonical(),
                    existing.name.clone(),
                ));
            }
        }

        let session = self.active_session.take().ok_or(MacroError::NotRecording)?;

        let id = self.allocate_id(&session.name);
        let saved = KeyboardMacro {
            id: id.clone(),
            name: session.name,
            description: session.description,
            steps: session.steps,
            trigger,
            created_at_unix_ms: session.started_at_unix_ms,
            updated_at_unix_ms: now_unix_ms.max(session.started_at_unix_ms),
            play_count: 0,
        };
        saved.validate()?;
        self.macros.insert(id, saved.clone());
        Ok(saved)
    }

    /// Looks up a macro by its unique ID.
    pub fn get_macro(&self, id: &str) -> Option<&KeyboardMacro> {
        self.macros.get(id)
    }

    /// Looks up a macro by its human-readable name (case-insensitive).
    pub fn find_by_name(&self, name: &str) -> Option<&KeyboardMacro> {
        let target = name.trim();
        self.macros
            .values()
            .find(|m| m.name.eq_ignore_ascii_case(target))
    }

    /// Looks up a macro bound to the given canonical shortcut.
    pub fn find_by_shortcut(&self, chord: &KeyChord) -> Option<&KeyboardMacro> {
        self.macros
            .values()
            .find(|m| m.trigger.shortcut.as_ref() == Some(chord))
    }

    /// Returns all saved macros ordered by ID.
    pub fn list_macros(&self) -> Vec<&KeyboardMacro> {
        self.macros.values().collect()
    }

    /// Removes a macro by ID and returns it.
    pub fn delete_macro(&mut self, id: &str) -> Result<KeyboardMacro, MacroError> {
        self.macros
            .remove(id)
            .ok_or_else(|| MacroError::MacroNotFound(id.to_string()))
    }

    /// Builds a deterministic playback schedule and increments the macro's play counter.
    pub fn build_playback_plan(
        &mut self,
        id: &str,
        options: &PlaybackOptions,
    ) -> Result<PlaybackPlan, MacroError> {
        options.validate()?;

        let entry = self
            .macros
            .get_mut(id)
            .ok_or_else(|| MacroError::MacroNotFound(id.to_string()))?;
        entry.validate()?;

        let mut scheduled = Vec::with_capacity(entry.steps.len() * options.repeat_count as usize);
        let mut cursor_ms = 0u64;

        for iteration in 0..options.repeat_count {
            for step in &entry.steps {
                let raw_delay = if options.ignore_delays {
                    0
                } else if let Some(cap) = options.max_step_delay_ms {
                    step.delay_from_prev_ms.min(cap)
                } else {
                    step.delay_from_prev_ms
                };

                let scaled_delay = scale_duration_ms(raw_delay, options.speed);
                cursor_ms = cursor_ms.saturating_add(scaled_delay);

                let action = match &step.action {
                    MacroAction::Wait { duration_ms } => {
                        let effective = if options.ignore_delays {
                            0
                        } else {
                            scale_duration_ms(*duration_ms, options.speed)
                        };
                        MacroAction::Wait {
                            duration_ms: effective,
                        }
                    }
                    other => other.clone(),
                };

                scheduled.push(ScheduledPlaybackStep {
                    iteration,
                    step_index: step.index,
                    at_offset_ms: cursor_ms,
                    action: action.clone(),
                });

                if let MacroAction::Wait { duration_ms } = action {
                    cursor_ms = cursor_ms.saturating_add(duration_ms);
                }
            }
        }

        entry.play_count = entry.play_count.saturating_add(1);

        Ok(PlaybackPlan {
            macro_id: entry.id.clone(),
            macro_name: entry.name.clone(),
            total_duration_ms: cursor_ms,
            steps: scheduled,
        })
    }

    /// Serializes all saved macros to a JSON array string.
    pub fn export_json(&self) -> Result<String, MacroError> {
        let list: Vec<&KeyboardMacro> = self.macros.values().collect();
        Ok(serde_json::to_string_pretty(&list)?)
    }

    /// Imports macros from a JSON array string, validating every entry.
    pub fn import_json(&mut self, json: &str, overwrite: bool) -> Result<usize, MacroError> {
        let incoming: Vec<KeyboardMacro> = serde_json::from_str(json)?;
        let mut imported = 0usize;

        for mut item in incoming {
            item.validate()?;

            if self.macros.contains_key(&item.id) {
                if !overwrite {
                    continue;
                }
            } else if self.macros.len() >= self.config.max_saved_macros {
                return Err(MacroError::MaxMacrosExceeded {
                    max: self.config.max_saved_macros,
                });
            }

            if let Some(existing) = self.find_by_name(&item.name) {
                if existing.id != item.id && !overwrite {
                    return Err(MacroError::DuplicateMacroName(item.name));
                }
            }

            if item.id.trim().is_empty() {
                item.id = self.allocate_id(&item.name);
            }
            self.macros.insert(item.id.clone(), item);
            imported += 1;
        }

        Ok(imported)
    }

    fn push_action(&mut self, action: MacroAction, now_unix_ms: u64) -> Result<(), MacroError> {
        let max_steps = self.config.max_steps_per_macro;
        let max_delay = self.config.max_recorded_delay_ms;
        let session = self
            .active_session
            .as_mut()
            .ok_or(MacroError::NotRecording)?;
        append_step(session, action, now_unix_ms, max_steps, max_delay)
    }

    fn allocate_id(&mut self, name: &str) -> String {
        let slug: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        let trimmed_slug = slug.trim_matches('-');
        let base = if trimmed_slug.is_empty() {
            "macro"
        } else {
            trimmed_slug
        };

        loop {
            let candidate = format!("{base}-{}", self.next_id_seq);
            self.next_id_seq = self.next_id_seq.saturating_add(1);
            if !self.macros.contains_key(&candidate) {
                return candidate;
            }
        }
    }
}

fn append_step(
    session: &mut RecordingSession,
    action: MacroAction,
    now_unix_ms: u64,
    max_steps: usize,
    max_delay_ms: u64,
) -> Result<(), MacroError> {
    if session.steps.len() >= max_steps {
        return Err(MacroError::MaxStepsExceeded { max: max_steps });
    }

    let raw_delay = if session.steps.is_empty() {
        0
    } else {
        now_unix_ms.saturating_sub(session.last_event_unix_ms)
    };
    let delay_from_prev_ms = raw_delay.min(max_delay_ms);
    let index = session.steps.len();

    session.steps.push(MacroStep {
        index,
        action,
        delay_from_prev_ms,
    });
    session.last_event_unix_ms = now_unix_ms.max(session.last_event_unix_ms);
    Ok(())
}

fn scale_duration_ms(duration_ms: u64, speed: f64) -> u64 {
    if duration_ms == 0 {
        return 0;
    }
    let scaled = (duration_ms as f64 / speed).round();
    if scaled <= 0.0 {
        0
    } else if scaled >= u64::MAX as f64 {
        u64::MAX
    } else {
        scaled as u64
    }
}

fn validate_macro_name(raw: &str) -> Result<String, MacroError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(MacroError::EmptyMacroName);
    }
    if trimmed.chars().count() > MAX_MACRO_NAME_LEN {
        return Err(MacroError::InvalidMacroName(format!(
            "exceeds {MAX_MACRO_NAME_LEN} characters"
        )));
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(MacroError::InvalidMacroName(
            "control characters are not allowed".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

fn normalize_key_name(raw: &str) -> Result<String, MacroError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_KEY_NAME_LEN {
        return Err(MacroError::InvalidShortcut(raw.to_string()));
    }
    if trimmed.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(MacroError::InvalidShortcut(raw.to_string()));
    }
    if trimmed.chars().count() == 1 {
        Ok(trimmed.to_ascii_uppercase())
    } else {
        Ok(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_coalesce_save_and_playback_plan_happy_path() {
        let mut recorder = KeyboardMacroRecorder::default();

        recorder
            .start_recording("Quick Search", "Focus omnibox and type query", 1_000)
            .unwrap();
        assert!(recorder.is_recording());

        let focus_chord = KeyChord::parse("Shift+Ctrl+L").unwrap();
        assert_eq!(focus_chord.format_canonical(), "Ctrl+Shift+L");

        recorder
            .record_key_event(focus_chord.clone(), KeyEventKind::Press, 1_050)
            .unwrap();
        recorder.record_text_input("rust ", 1_150).unwrap();
        recorder.record_text_input("macros", 1_200).unwrap();
        recorder
            .record_command("submit-omnibox", vec!["current-tab".to_string()], 1_400)
            .unwrap();

        let saved = recorder
            .stop_and_save(
                MacroTrigger {
                    shortcut: Some(KeyChord::parse("Alt+M").unwrap()),
                    alias: Some(":qs".to_string()),
                },
                1_500,
            )
            .unwrap();

        assert!(!recorder.is_recording());
        assert_eq!(saved.steps.len(), 3);
        assert_eq!(
            saved.steps[1].action,
            MacroAction::TextInput {
                text: "rust macros".to_string()
            }
        );
        // First step delay is 0, second step is 1150-1050=100, third is 1400-1200=200.
        assert_eq!(saved.total_duration_ms(), 300);

        let plan = recorder
            .build_playback_plan(
                &saved.id,
                &PlaybackOptions {
                    speed: 2.0,
                    repeat_count: 2,
                    ignore_delays: false,
                    max_step_delay_ms: None,
                },
            )
            .unwrap();

        assert_eq!(plan.steps.len(), 6);
        // Per iteration at 2.0x speed: 0 + 50 + 100 = 150ms, repeated twice = 300ms.
        assert_eq!(plan.total_duration_ms, 300);
        assert_eq!(recorder.get_macro(&saved.id).unwrap().play_count, 1);
    }

    #[test]
    fn edge_cases_validation_and_limits() {
        let config = RecorderConfig {
            max_steps_per_macro: 2,
            max_saved_macros: 1,
            coalesce_text_input: false,
            max_recorded_delay_ms: 500,
            ignore_modifier_only_events: true,
        };
        let mut recorder = KeyboardMacroRecorder::new(config);

        assert_eq!(
            recorder.start_recording("   ", "desc", 0),
            Err(MacroError::EmptyMacroName)
        );
        assert_eq!(
            recorder.record_text_input("a", 10),
            Err(MacroError::NotRecording)
        );

        recorder.start_recording("One", "", 100).unwrap();
        assert_eq!(
            recorder.stop_and_save(MacroTrigger::default(), 200),
            Err(MacroError::EmptyMacro)
        );

        // Modifier-only event should be ignored without consuming step budget.
        let ctrl_only = KeyChord::new("Ctrl", []).unwrap();
        recorder
            .record_key_event(ctrl_only, KeyEventKind::Press, 120)
            .unwrap();
        assert_eq!(recorder.active_session().unwrap().steps.len(), 0);

        recorder.record_text_input("a", 150).unwrap();
        // Delay 1000ms should be clamped to max_recorded_delay_ms (500ms).
        recorder.record_text_input("b", 1_150).unwrap();
        assert_eq!(
            recorder.active_session().unwrap().steps[1].delay_from_prev_ms,
            500
        );

        // Third step exceeds max_steps_per_macro (2).
        assert_eq!(
            recorder.record_text_input("c", 1_200),
            Err(MacroError::MaxStepsExceeded { max: 2 })
        );

        let first = recorder
            .stop_and_save(
                MacroTrigger {
                    shortcut: Some(KeyChord::parse("Ctrl+K").unwrap()),
                    alias: None,
                },
                1_300,
            )
            .unwrap();

        // Playback option validation errors.
        assert_eq!(
            recorder.build_playback_plan(
                &first.id,
                &PlaybackOptions {
                    speed: 0.0,
                    ..PlaybackOptions::default()
                }
            ),
            Err(MacroError::InvalidPlaybackSpeed(0.0))
        );
        assert_eq!(
            recorder.build_playback_plan(
                &first.id,
                &PlaybackOptions {
                    repeat_count: 0,
                    ..PlaybackOptions::default()
                }
            ),
            Err(MacroError::InvalidRepeatCount)
        );

        // Capacity limit reached.
        recorder.start_recording("Two", "", 2_000).unwrap();
        recorder.record_text_input("x", 2_050).unwrap();
        assert_eq!(
            recorder.stop_and_save(MacroTrigger::default(), 2_100),
            Err(MacroError::MaxMacrosExceeded { max: 1 })
        );
    }

    #[test]
    fn serialization_roundtrip_and_shortcut_lookup() {
        let mut recorder = KeyboardMacroRecorder::default();
        recorder
            .start_recording("Reload Tab", "Hard reload", 500)
            .unwrap();
        recorder
            .record_key_event(
                KeyChord::parse("Ctrl+Shift+R").unwrap(),
                KeyEventKind::Press,
                520,
            )
            .unwrap();
        let saved = recorder
            .stop_and_save(
                MacroTrigger {
                    shortcut: Some(KeyChord::parse("F9").unwrap()),
                    alias: None,
                },
                550,
            )
            .unwrap();

        let json = recorder.export_json().unwrap();

        let mut restored = KeyboardMacroRecorder::default();
        let count = restored.import_json(&json, false).unwrap();
        assert_eq!(count, 1);

        let found = restored
            .find_by_shortcut(&KeyChord::parse("F9").unwrap())
            .unwrap();
        assert_eq!(found.id, saved.id);
        assert_eq!(found.name, "Reload Tab");
        assert_eq!(found.steps, saved.steps);

        // Duplicate import with overwrite=false skips existing ID.
        let skipped = restored.import_json(&json, false).unwrap();
        assert_eq!(skipped, 0);

        // Deleting removes it cleanly.
        let removed = restored.delete_macro(&saved.id).unwrap();
        assert_eq!(removed.id, saved.id);
        assert!(restored.list_macros().is_empty());
    }
}
