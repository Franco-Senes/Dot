use serde::{Deserialize, Serialize};
use thiserror::Error;

const MIN_FOCUS_SECS: u64 = 60;
const MAX_FOCUS_SECS: u64 = 7200;
const MIN_SHORT_BREAK_SECS: u64 = 30;
const MAX_SHORT_BREAK_SECS: u64 = 1800;
const MIN_LONG_BREAK_SECS: u64 = 60;
const MAX_LONG_BREAK_SECS: u64 = 3600;
const MIN_LONG_BREAK_INTERVAL: u32 = 1;
const MAX_LONG_BREAK_INTERVAL: u32 = 12;
const MAX_LABEL_LEN: usize = 120;
const MAX_HISTORY_RECORDS: usize = 500;

/// Errors produced by Pomodoro configuration, state transitions, and serialization.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PomodoroError {
    #[error("{field} must be between {min} and {max} seconds, got {value}")]
    InvalidDuration {
        field: &'static str,
        value: u64,
        min: u64,
        max: u64,
    },

    #[error("long_break_interval must be between {MIN_LONG_BREAK_INTERVAL} and {MAX_LONG_BREAK_INTERVAL}, got {0}")]
    InvalidLongBreakInterval(u32),

    #[error("blocked domain must be a valid non-empty hostname without whitespace: '{0}'")]
    InvalidBlockedDomain(String),

    #[error("cannot perform '{action}' while timer is in {from:?} state")]
    InvalidStateTransition {
        from: TimerState,
        action: &'static str,
    },

    #[error("session label cannot be empty or whitespace-only")]
    EmptySessionLabel,

    #[error("session label length {len} exceeds maximum of {max} characters")]
    LabelTooLong { len: usize, max: usize },

    #[error("timestamp {now} precedes last update timestamp {last}")]
    TimestampRegression { now: u64, last: u64 },

    #[error("serialization error: {0}")]
    Serialization(String),
}

impl From<serde_json::Error> for PomodoroError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}

/// Active phase in a Pomodoro cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PomodoroPhase {
    Focus,
    ShortBreak,
    LongBreak,
}

impl PomodoroPhase {
    /// Human-readable label for UI status bars and notifications.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Focus => "Focus",
            Self::ShortBreak => "Short Break",
            Self::LongBreak => "Long Break",
        }
    }
}

/// Execution state of the timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimerState {
    Idle,
    Running,
    Paused,
    Completed,
}

/// Events emitted by timer transitions and ticks for UI or browser hooks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TimerEvent {
    PhaseStarted {
        phase: PomodoroPhase,
        duration_secs: u64,
    },
    PhaseCompleted {
        phase: PomodoroPhase,
        completed_focus_sessions: u32,
    },
    Paused {
        remaining_secs: u64,
    },
    Resumed {
        remaining_secs: u64,
    },
    Skipped {
        from: PomodoroPhase,
        to: PomodoroPhase,
    },
    Reset,
}

/// Configuration for focus/break durations, auto-start behavior, and site blocking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PomodoroConfig {
    pub focus_secs: u64,
    pub short_break_secs: u64,
    pub long_break_secs: u64,
    pub long_break_interval: u32,
    pub auto_start_breaks: bool,
    pub auto_start_focus: bool,
    pub block_distracting_sites: bool,
    pub blocked_domains: Vec<String>,
}

impl Default for PomodoroConfig {
    fn default() -> Self {
        Self::preset_classic()
    }
}

impl PomodoroConfig {
    /// Classic 25/5/15 Pomodoro preset with a 4-session cycle.
    pub fn preset_classic() -> Self {
        Self {
            focus_secs: 25 * 60,
            short_break_secs: 5 * 60,
            long_break_secs: 15 * 60,
            long_break_interval: 4,
            auto_start_breaks: true,
            auto_start_focus: false,
            block_distracting_sites: true,
            blocked_domains: Vec::new(),
        }
    }

    /// Deep work 50/10/30 preset with a 2-session cycle.
    pub fn preset_deep_work() -> Self {
        Self {
            focus_secs: 50 * 60,
            short_break_secs: 10 * 60,
            long_break_secs: 30 * 60,
            long_break_interval: 2,
            auto_start_breaks: true,
            auto_start_focus: false,
            block_distracting_sites: true,
            blocked_domains: Vec::new(),
        }
    }

    /// Short sprint 15/3/10 preset with a 4-session cycle.
    pub fn preset_sprint() -> Self {
        Self {
            focus_secs: 15 * 60,
            short_break_secs: 3 * 60,
            long_break_secs: 10 * 60,
            long_break_interval: 4,
            auto_start_breaks: true,
            auto_start_focus: false,
            block_distracting_sites: true,
            blocked_domains: Vec::new(),
        }
    }

    /// Returns the configured duration in seconds for the given phase.
    pub fn duration_for(&self, phase: PomodoroPhase) -> u64 {
        match phase {
            PomodoroPhase::Focus => self.focus_secs,
            PomodoroPhase::ShortBreak => self.short_break_secs,
            PomodoroPhase::LongBreak => self.long_break_secs,
        }
    }

    /// Validates all durations, cycle intervals, and blocked domain patterns.
    pub fn validate(&self) -> Result<(), PomodoroError> {
        if !(MIN_FOCUS_SECS..=MAX_FOCUS_SECS).contains(&self.focus_secs) {
            return Err(PomodoroError::InvalidDuration {
                field: "focus_secs",
                value: self.focus_secs,
                min: MIN_FOCUS_SECS,
                max: MAX_FOCUS_SECS,
            });
        }
        if !(MIN_SHORT_BREAK_SECS..=MAX_SHORT_BREAK_SECS).contains(&self.short_break_secs) {
            return Err(PomodoroError::InvalidDuration {
                field: "short_break_secs",
                value: self.short_break_secs,
                min: MIN_SHORT_BREAK_SECS,
                max: MAX_SHORT_BREAK_SECS,
            });
        }
        if !(MIN_LONG_BREAK_SECS..=MAX_LONG_BREAK_SECS).contains(&self.long_break_secs) {
            return Err(PomodoroError::InvalidDuration {
                field: "long_break_secs",
                value: self.long_break_secs,
                min: MIN_LONG_BREAK_SECS,
                max: MAX_LONG_BREAK_SECS,
            });
        }
        if !(MIN_LONG_BREAK_INTERVAL..=MAX_LONG_BREAK_INTERVAL).contains(&self.long_break_interval)
        {
            return Err(PomodoroError::InvalidLongBreakInterval(
                self.long_break_interval,
            ));
        }
        for domain in &self.blocked_domains {
            if !is_valid_domain(domain) {
                return Err(PomodoroError::InvalidBlockedDomain(domain.clone()));
            }
        }
        Ok(())
    }

    /// Normalizes blocked domain entries to lowercase and strips leading/trailing dots.
    pub fn normalized(mut self) -> Result<Self, PomodoroError> {
        self.blocked_domains = self
            .blocked_domains
            .into_iter()
            .map(|d| d.trim().trim_matches('.').to_ascii_lowercase())
            .collect();
        self.validate()?;
        Ok(self)
    }
}

/// Completed or skipped session entry stored in statistics history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub phase: PomodoroPhase,
    pub label: Option<String>,
    pub planned_secs: u64,
    pub actual_secs: u64,
    pub completed: bool,
    pub started_at_unix: u64,
    pub ended_at_unix: u64,
}

/// Aggregated productivity metrics across Pomodoro sessions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PomodoroStats {
    pub total_focus_secs: u64,
    pub total_break_secs: u64,
    pub completed_focus_sessions: u32,
    pub completed_cycles: u32,
    pub current_streak: u32,
    pub best_streak: u32,
    pub history: Vec<SessionRecord>,
}

impl PomodoroStats {
    fn record_session(&mut self, record: SessionRecord) {
        match record.phase {
            PomodoroPhase::Focus => {
                self.total_focus_secs = self.total_focus_secs.saturating_add(record.actual_secs);
                if record.completed {
                    self.completed_focus_sessions = self.completed_focus_sessions.saturating_add(1);
                    self.current_streak = self.current_streak.saturating_add(1);
                    if self.current_streak > self.best_streak {
                        self.best_streak = self.current_streak;
                    }
                } else {
                    self.current_streak = 0;
                }
            }
            PomodoroPhase::ShortBreak | PomodoroPhase::LongBreak => {
                self.total_break_secs = self.total_break_secs.saturating_add(record.actual_secs);
                if record.completed && record.phase == PomodoroPhase::LongBreak {
                    self.completed_cycles = self.completed_cycles.saturating_add(1);
                }
            }
        }

        if self.history.len() >= MAX_HISTORY_RECORDS {
            let overflow = self.history.len().saturating_sub(MAX_HISTORY_RECORDS) + 1;
            self.history.drain(0..overflow);
        }
        self.history.push(record);
    }
}

/// Stateful Pomodoro focus timer engine with deterministic time progression.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PomodoroTimer {
    config: PomodoroConfig,
    phase: PomodoroPhase,
    state: TimerState,
    label: Option<String>,
    remaining_secs: u64,
    phase_elapsed_secs: u64,
    focus_sessions_in_cycle: u32,
    phase_started_at_unix: Option<u64>,
    last_updated_at_unix: Option<u64>,
    stats: PomodoroStats,
}

impl PomodoroTimer {
    /// Creates a new idle timer in the `Focus` phase using the validated config.
    pub fn new(config: PomodoroConfig) -> Result<Self, PomodoroError> {
        let config = config.normalized()?;
        let remaining_secs = config.focus_secs;
        Ok(Self {
            config,
            phase: PomodoroPhase::Focus,
            state: TimerState::Idle,
            label: None,
            remaining_secs,
            phase_elapsed_secs: 0,
            focus_sessions_in_cycle: 0,
            phase_started_at_unix: None,
            last_updated_at_unix: None,
            stats: PomodoroStats::default(),
        })
    }

    pub fn config(&self) -> &PomodoroConfig {
        &self.config
    }

    pub fn phase(&self) -> PomodoroPhase {
        self.phase
    }

    pub fn state(&self) -> TimerState {
        self.state
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn remaining_secs(&self) -> u64 {
        self.remaining_secs
    }

    pub fn focus_sessions_in_cycle(&self) -> u32 {
        self.focus_sessions_in_cycle
    }

    pub fn stats(&self) -> &PomodoroStats {
        &self.stats
    }

    /// Sets or clears the current focus session label.
    pub fn set_label(&mut self, label: Option<String>) -> Result<(), PomodoroError> {
        match label {
            None => {
                self.label = None;
                Ok(())
            }
            Some(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    return Err(PomodoroError::EmptySessionLabel);
                }
                let char_count = trimmed.chars().count();
                if char_count > MAX_LABEL_LEN {
                    return Err(PomodoroError::LabelTooLong {
                        len: char_count,
                        max: MAX_LABEL_LEN,
                    });
                }
                self.label = Some(trimmed.to_string());
                Ok(())
            }
        }
    }

    /// Updates timer configuration. If idle, resets `remaining_secs` to match the new duration.
    pub fn update_config(&mut self, config: PomodoroConfig) -> Result<(), PomodoroError> {
        let config = config.normalized()?;
        if self.state == TimerState::Idle {
            self.remaining_secs = config.duration_for(self.phase);
            self.phase_elapsed_secs = 0;
        }
        self.config = config;
        Ok(())
    }

    /// Starts the current phase from `Idle` or `Completed` state.
    pub fn start(&mut self, now_unix: u64) -> Result<Vec<TimerEvent>, PomodoroError> {
        if !matches!(self.state, TimerState::Idle | TimerState::Completed) {
            return Err(PomodoroError::InvalidStateTransition {
                from: self.state,
                action: "start",
            });
        }
        self.validate_timestamp(now_unix)?;

        if self.state == TimerState::Completed {
            self.remaining_secs = self.config.duration_for(self.phase);
            self.phase_elapsed_secs = 0;
        }

        self.state = TimerState::Running;
        self.phase_started_at_unix = Some(now_unix);
        self.last_updated_at_unix = Some(now_unix);

        Ok(vec![TimerEvent::PhaseStarted {
            phase: self.phase,
            duration_secs: self.remaining_secs,
        }])
    }

    /// Pauses a running timer after advancing up to `now_unix`.
    pub fn pause(&mut self, now_unix: u64) -> Result<Vec<TimerEvent>, PomodoroError> {
        if self.state != TimerState::Running {
            return Err(PomodoroError::InvalidStateTransition {
                from: self.state,
                action: "pause",
            });
        }
        let mut events = self.tick(now_unix)?;
        if self.state == TimerState::Running {
            self.state = TimerState::Paused;
            events.push(TimerEvent::Paused {
                remaining_secs: self.remaining_secs,
            });
        }
        Ok(events)
    }

    /// Resumes a paused timer.
    pub fn resume(&mut self, now_unix: u64) -> Result<TimerEvent, PomodoroError> {
        if self.state != TimerState::Paused {
            return Err(PomodoroError::InvalidStateTransition {
                from: self.state,
                action: "resume",
            });
        }
        self.validate_timestamp(now_unix)?;
        self.state = TimerState::Running;
        self.last_updated_at_unix = Some(now_unix);
        Ok(TimerEvent::Resumed {
            remaining_secs: self.remaining_secs,
        })
    }

    /// Resets the current phase back to `Idle` without recording completion.
    pub fn reset(&mut self) -> TimerEvent {
        self.state = TimerState::Idle;
        self.remaining_secs = self.config.duration_for(self.phase);
        self.phase_elapsed_secs = 0;
        self.phase_started_at_unix = None;
        self.last_updated_at_unix = None;
        TimerEvent::Reset
    }

    /// Skips the current phase, records partial elapsed time if started, and advances to the next phase.
    pub fn skip_phase(&mut self, now_unix: u64) -> Result<Vec<TimerEvent>, PomodoroError> {
        self.validate_timestamp(now_unix)?;
        if self.state == TimerState::Running {
            self.advance_elapsed_only(now_unix);
        }

        let from = self.phase;
        if self.phase_elapsed_secs > 0 {
            let started_at = self.phase_started_at_unix.unwrap_or(now_unix);
            self.stats.record_session(SessionRecord {
                phase: from,
                label: if from == PomodoroPhase::Focus {
                    self.label.clone()
                } else {
                    None
                },
                planned_secs: self.config.duration_for(from),
                actual_secs: self.phase_elapsed_secs,
                completed: false,
                started_at_unix: started_at,
                ended_at_unix: now_unix,
            });
        }

        let next = self.next_phase(false);
        self.phase = next;
        self.remaining_secs = self.config.duration_for(next);
        self.phase_elapsed_secs = 0;
        self.state = TimerState::Idle;
        self.phase_started_at_unix = None;
        self.last_updated_at_unix = Some(now_unix);

        Ok(vec![TimerEvent::Skipped { from, to: next }])
    }

    /// Advances the timer to `now_unix`, completing phases and auto-starting if configured.
    pub fn tick(&mut self, now_unix: u64) -> Result<Vec<TimerEvent>, PomodoroError> {
        self.validate_timestamp(now_unix)?;
        if self.state != TimerState::Running {
            return Ok(Vec::new());
        }

        let last = self.last_updated_at_unix.unwrap_or(now_unix);
        let mut delta = now_unix.saturating_sub(last);
        let mut cursor = last;
        let mut events = Vec::new();

        while delta > 0 && self.state == TimerState::Running {
            if delta < self.remaining_secs {
                self.remaining_secs = self.remaining_secs.saturating_sub(delta);
                self.phase_elapsed_secs = self.phase_elapsed_secs.saturating_add(delta);
                cursor = cursor.saturating_add(delta);
                delta = 0;
            } else {
                let step = self.remaining_secs;
                delta = delta.saturating_sub(step);
                cursor = cursor.saturating_add(step);
                self.phase_elapsed_secs = self.phase_elapsed_secs.saturating_add(step);
                self.remaining_secs = 0;

                let completed_phase = self.phase;
                let started_at = self.phase_started_at_unix.unwrap_or(cursor);
                self.stats.record_session(SessionRecord {
                    phase: completed_phase,
                    label: if completed_phase == PomodoroPhase::Focus {
                        self.label.clone()
                    } else {
                        None
                    },
                    planned_secs: self.config.duration_for(completed_phase),
                    actual_secs: self.phase_elapsed_secs,
                    completed: true,
                    started_at_unix: started_at,
                    ended_at_unix: cursor,
                });

                events.push(TimerEvent::PhaseCompleted {
                    phase: completed_phase,
                    completed_focus_sessions: self.stats.completed_focus_sessions,
                });

                let next = self.next_phase(true);
                self.phase = next;
                self.remaining_secs = self.config.duration_for(next);
                self.phase_elapsed_secs = 0;

                let should_auto_start = match next {
                    PomodoroPhase::Focus => self.config.auto_start_focus,
                    PomodoroPhase::ShortBreak | PomodoroPhase::LongBreak => {
                        self.config.auto_start_breaks
                    }
                };

                if should_auto_start {
                    self.state = TimerState::Running;
                    self.phase_started_at_unix = Some(cursor);
                    events.push(TimerEvent::PhaseStarted {
                        phase: next,
                        duration_secs: self.remaining_secs,
                    });
                } else {
                    self.state = TimerState::Idle;
                    self.phase_started_at_unix = None;
                }
            }
        }

        self.last_updated_at_unix = Some(now_unix);
        Ok(events)
    }

    /// Returns completion progress for the current phase in `[0.0, 1.0]`.
    pub fn progress_ratio(&self) -> f64 {
        let total = self.config.duration_for(self.phase);
        if total == 0 {
            return 1.0;
        }
        let elapsed = total.saturating_sub(self.remaining_secs);
        (elapsed as f64 / total as f64).clamp(0.0, 1.0)
    }

    /// Formats remaining time as `MM:SS` or `HH:MM:SS` when >= 1 hour.
    pub fn formatted_remaining(&self) -> String {
        let secs = self.remaining_secs;
        let hours = secs / 3600;
        let minutes = (secs % 3600) / 60;
        let rem_secs = secs % 60;
        if hours > 0 {
            format!("{hours:02}:{minutes:02}:{rem_secs:02}")
        } else {
            format!("{minutes:02}:{rem_secs:02}")
        }
    }

    /// Returns `true` if the given host should be blocked during an active focus session.
    pub fn should_block_host(&self, host: &str) -> bool {
        if !self.config.block_distracting_sites
            || self.phase != PomodoroPhase::Focus
            || self.state != TimerState::Running
        {
            return false;
        }
        let candidate = host.trim().trim_matches('.').to_ascii_lowercase();
        if candidate.is_empty() {
            return false;
        }
        self.config.blocked_domains.iter().any(|blocked| {
            candidate == *blocked
                || candidate
                    .strip_suffix(blocked.as_str())
                    .is_some_and(|prefix| prefix.ends_with('.'))
        })
    }

    /// Serializes the full timer state and history to JSON.
    pub fn to_json(&self) -> Result<String, PomodoroError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Deserializes timer state from JSON and validates the embedded configuration.
    pub fn from_json(raw: &str) -> Result<Self, PomodoroError> {
        let mut timer: Self = serde_json::from_str(raw)?;
        timer.config = timer.config.normalized()?;
        Ok(timer)
    }

    fn validate_timestamp(&self, now_unix: u64) -> Result<(), PomodoroError> {
        if let Some(last) = self.last_updated_at_unix {
            if now_unix < last {
                return Err(PomodoroError::TimestampRegression {
                    now: now_unix,
                    last,
                });
            }
        }
        Ok(())
    }

    fn advance_elapsed_only(&mut self, now_unix: u64) {
        if let Some(last) = self.last_updated_at_unix {
            let delta = now_unix.saturating_sub(last);
            let applied = delta.min(self.remaining_secs);
            self.remaining_secs = self.remaining_secs.saturating_sub(applied);
            self.phase_elapsed_secs = self.phase_elapsed_secs.saturating_add(applied);
        }
    }

    fn next_phase(&mut self, completed_normally: bool) -> PomodoroPhase {
        match self.phase {
            PomodoroPhase::Focus => {
                if completed_normally {
                    self.focus_sessions_in_cycle = self.focus_sessions_in_cycle.saturating_add(1);
                    if self.focus_sessions_in_cycle >= self.config.long_break_interval {
                        self.focus_sessions_in_cycle = 0;
                        PomodoroPhase::LongBreak
                    } else {
                        PomodoroPhase::ShortBreak
                    }
                } else {
                    PomodoroPhase::ShortBreak
                }
            }
            PomodoroPhase::ShortBreak | PomodoroPhase::LongBreak => PomodoroPhase::Focus,
        }
    }
}

fn is_valid_domain(raw: &str) -> bool {
    let trimmed = raw.trim().trim_matches('.');
    !trimmed.is_empty()
        && !trimmed.chars().any(char::is_whitespace)
        && trimmed.contains('.')
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_full_pomodoro_cycle_and_stats() {
        let mut config = PomodoroConfig::preset_classic();
        config.focus_secs = 120;
        config.short_break_secs = 30;
        config.long_break_secs = 60;
        config.long_break_interval = 2;
        config.auto_start_breaks = true;
        config.auto_start_focus = true;

        let mut timer = PomodoroTimer::new(config).expect("valid config");
        timer
            .set_label(Some("  Write search-core tests  ".to_string()))
            .expect("valid label");

        let start_events = timer.start(1000).expect("start focus");
        assert_eq!(
            start_events,
            vec![TimerEvent::PhaseStarted {
                phase: PomodoroPhase::Focus,
                duration_secs: 120,
            }]
        );
        assert_eq!(timer.formatted_remaining(), "02:00");

        // Advance halfway through first focus session.
        let mid_events = timer.tick(1060).expect("mid tick");
        assert!(mid_events.is_empty());
        assert_eq!(timer.remaining_secs(), 60);
        assert!((timer.progress_ratio() - 0.5).abs() < 1e-6);

        // Complete first focus (60s) + short break (30s) in one jump.
        let jump_events = timer.tick(1150).expect("jump tick");
        assert_eq!(
            jump_events,
            vec![
                TimerEvent::PhaseCompleted {
                    phase: PomodoroPhase::Focus,
                    completed_focus_sessions: 1,
                },
                TimerEvent::PhaseStarted {
                    phase: PomodoroPhase::ShortBreak,
                    duration_secs: 30,
                },
                TimerEvent::PhaseCompleted {
                    phase: PomodoroPhase::ShortBreak,
                    completed_focus_sessions: 1,
                },
                TimerEvent::PhaseStarted {
                    phase: PomodoroPhase::Focus,
                    duration_secs: 120,
                },
            ]
        );
        assert_eq!(timer.phase(), PomodoroPhase::Focus);

        // Complete second focus session -> should trigger LongBreak.
        let second_focus = timer.tick(1270).expect("second focus complete");
        assert_eq!(
            second_focus,
            vec![
                TimerEvent::PhaseCompleted {
                    phase: PomodoroPhase::Focus,
                    completed_focus_sessions: 2,
                },
                TimerEvent::PhaseStarted {
                    phase: PomodoroPhase::LongBreak,
                    duration_secs: 60,
                },
            ]
        );

        // Complete long break -> cycle count increments.
        timer.tick(1330).expect("long break complete");
        assert_eq!(timer.stats().completed_focus_sessions, 2);
        assert_eq!(timer.stats().completed_cycles, 1);
        assert_eq!(timer.stats().total_focus_secs, 240);
        assert_eq!(timer.stats().total_break_secs, 90);
        assert_eq!(timer.stats().current_streak, 2);
        assert_eq!(timer.stats().best_streak, 2);
        assert_eq!(timer.stats().history.len(), 4);
        assert_eq!(
            timer.stats().history[0].label.as_deref(),
            Some("Write search-core tests")
        );
    }

    #[test]
    fn pause_resume_skip_and_domain_blocking() {
        let mut config = PomodoroConfig::preset_sprint();
        config.blocked_domains = vec!["  YouTube.com ".to_string(), "reddit.com".to_string()];

        let mut timer = PomodoroTimer::new(config).expect("valid sprint config");
        assert!(!timer.should_block_host("youtube.com"));

        timer.start(2000).expect("start");
        assert!(timer.should_block_host("youtube.com"));
        assert!(timer.should_block_host("www.youtube.com"));
        assert!(!timer.should_block_host("notyoutube.com"));
        assert!(!timer.should_block_host("docs.rs"));

        let pause_events = timer.pause(2045).expect("pause");
        assert_eq!(
            pause_events,
            vec![TimerEvent::Paused {
                remaining_secs: 855,
            }]
        );
        assert!(!timer.should_block_host("youtube.com"));

        let resumed = timer.resume(2100).expect("resume");
        assert_eq!(
            resumed,
            TimerEvent::Resumed {
                remaining_secs: 855
            }
        );

        let skip_events = timer.skip_phase(2110).expect("skip");
        assert_eq!(
            skip_events,
            vec![TimerEvent::Skipped {
                from: PomodoroPhase::Focus,
                to: PomodoroPhase::ShortBreak,
            }]
        );
        assert_eq!(timer.state(), TimerState::Idle);
        assert_eq!(timer.phase(), PomodoroPhase::ShortBreak);
        assert_eq!(timer.stats().current_streak, 0);
        assert_eq!(timer.stats().total_focus_secs, 55);
    }

    #[test]
    fn validation_errors_and_json_roundtrip() {
        let bad_cfg = PomodoroConfig {
            focus_secs: 10,
            ..Default::default()
        };
        assert!(matches!(
            PomodoroTimer::new(bad_cfg),
            Err(PomodoroError::InvalidDuration {
                field: "focus_secs",
                ..
            })
        ));

        let bad_domain_cfg = PomodoroConfig {
            blocked_domains: vec!["invalid domain".to_string()],
            ..Default::default()
        };
        assert!(matches!(
            PomodoroTimer::new(bad_domain_cfg),
            Err(PomodoroError::InvalidBlockedDomain(_))
        ));

        let mut timer = PomodoroTimer::new(PomodoroConfig::preset_deep_work()).expect("valid");
        assert_eq!(
            timer.set_label(Some("   ".to_string())),
            Err(PomodoroError::EmptySessionLabel)
        );
        assert_eq!(
            timer.pause(100),
            Err(PomodoroError::InvalidStateTransition {
                from: TimerState::Idle,
                action: "pause",
            })
        );

        timer.start(500).expect("start");
        assert_eq!(
            timer.tick(499),
            Err(PomodoroError::TimestampRegression {
                now: 499,
                last: 500
            })
        );

        let json = timer.to_json().expect("serialize");
        let restored = PomodoroTimer::from_json(&json).expect("deserialize");
        assert_eq!(timer, restored);
    }
}
