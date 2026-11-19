use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fmt;
use thiserror::Error;

/// Errors returned by download speed and ETA estimation operations.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DownloadSpeedError {
    #[error("invalid estimator configuration: {0}")]
    InvalidConfig(String),

    #[error("timestamp regression: previous={previous_ms}ms, current={current_ms}ms")]
    TimestampRegression { previous_ms: u64, current_ms: u64 },

    #[error("bytes received regression: previous={previous_bytes}, current={current_bytes}")]
    BytesRegression {
        previous_bytes: u64,
        current_bytes: u64,
    },

    #[error("invalid total bytes: received={received}, total={total}")]
    InvalidTotalBytes { received: u64, total: u64 },

    #[error("invalid state transition from {from} to {to}")]
    InvalidStateTransition {
        from: DownloadStatus,
        to: DownloadStatus,
    },
}

/// Unit system used for byte and speed formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UnitSystem {
    /// Base-1000 units (B, KB, MB, GB, TB).
    #[default]
    Decimal,
    /// Base-1024 units (B, KiB, MiB, GiB, TiB).
    Binary,
}

/// Current lifecycle status of a tracked download.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    #[default]
    Pending,
    Active,
    Stalled,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl fmt::Display for DownloadStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Stalled => "stalled",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        };
        f.write_str(label)
    }
}

impl DownloadStatus {
    /// Returns true if the download is in a terminal state.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// Configuration for [`DownloadSpeedEstimator`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EstimatorConfig {
    /// Sliding window duration in milliseconds used for windowed speed calculation.
    pub window_ms: u64,
    /// Exponential moving average smoothing factor in `(0.0, 1.0]`.
    pub ema_alpha: f64,
    /// Duration without byte progress in milliseconds before marking a download stalled.
    pub stall_timeout_ms: u64,
    /// Minimum number of progress samples required before estimating an ETA.
    pub min_samples_for_eta: usize,
    /// Maximum number of samples retained in the sliding window buffer.
    pub max_samples: usize,
    /// Unit system for human-readable formatting.
    pub unit_system: UnitSystem,
}

impl Default for EstimatorConfig {
    fn default() -> Self {
        Self {
            window_ms: 5_000,
            ema_alpha: 0.3,
            stall_timeout_ms: 5_000,
            min_samples_for_eta: 2,
            max_samples: 128,
            unit_system: UnitSystem::Decimal,
        }
    }
}

impl EstimatorConfig {
    /// Validates all configuration fields.
    pub fn validate(&self) -> Result<(), DownloadSpeedError> {
        if self.window_ms == 0 {
            return Err(DownloadSpeedError::InvalidConfig(
                "window_ms must be greater than 0".to_string(),
            ));
        }
        if !self.ema_alpha.is_finite() || self.ema_alpha <= 0.0 || self.ema_alpha > 1.0 {
            return Err(DownloadSpeedError::InvalidConfig(
                "ema_alpha must be finite and in (0.0, 1.0]".to_string(),
            ));
        }
        if self.stall_timeout_ms == 0 {
            return Err(DownloadSpeedError::InvalidConfig(
                "stall_timeout_ms must be greater than 0".to_string(),
            ));
        }
        if self.min_samples_for_eta < 2 {
            return Err(DownloadSpeedError::InvalidConfig(
                "min_samples_for_eta must be at least 2".to_string(),
            ));
        }
        if self.max_samples < self.min_samples_for_eta {
            return Err(DownloadSpeedError::InvalidConfig(
                "max_samples must be at least min_samples_for_eta".to_string(),
            ));
        }
        Ok(())
    }
}

/// A single monotonic progress observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressSample {
    pub timestamp_ms: u64,
    pub bytes_received: u64,
}

/// Snapshot of computed download speed, ETA, and formatted display strings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DownloadMetrics {
    pub download_id: String,
    pub status: DownloadStatus,
    pub bytes_received: u64,
    pub total_bytes: Option<u64>,
    pub remaining_bytes: Option<u64>,
    pub progress_percent: Option<f64>,
    pub instantaneous_bps: f64,
    pub smoothed_bps: f64,
    pub average_bps: f64,
    pub eta_seconds: Option<u64>,
    pub elapsed_ms: u64,
    pub formatted_speed: String,
    pub formatted_eta: String,
    pub formatted_progress: String,
}

/// Tracks download byte progress over monotonic timestamps to estimate speed and ETA.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DownloadSpeedEstimator {
    download_id: String,
    config: EstimatorConfig,
    status: DownloadStatus,
    total_bytes: Option<u64>,
    started_at_ms: Option<u64>,
    last_progress_ms: Option<u64>,
    last_sample_ms: Option<u64>,
    bytes_received: u64,
    samples: VecDeque<ProgressSample>,
    ema_bps: Option<f64>,
    instantaneous_bps: f64,
}

impl DownloadSpeedEstimator {
    /// Creates a new estimator with validated configuration.
    pub fn new(
        download_id: impl Into<String>,
        total_bytes: Option<u64>,
        config: EstimatorConfig,
    ) -> Result<Self, DownloadSpeedError> {
        let id = download_id.into();
        if id.trim().is_empty() {
            return Err(DownloadSpeedError::InvalidConfig(
                "download_id must not be empty".to_string(),
            ));
        }
        config.validate()?;

        Ok(Self {
            download_id: id,
            config,
            status: DownloadStatus::Pending,
            total_bytes,
            started_at_ms: None,
            last_progress_ms: None,
            last_sample_ms: None,
            bytes_received: 0,
            samples: VecDeque::new(),
            ema_bps: None,
            instantaneous_bps: 0.0,
        })
    }

    /// Creates a new estimator using [`EstimatorConfig::default()`].
    pub fn with_default_config(
        download_id: impl Into<String>,
        total_bytes: Option<u64>,
    ) -> Result<Self, DownloadSpeedError> {
        Self::new(download_id, total_bytes, EstimatorConfig::default())
    }

    /// Returns the download identifier.
    pub fn download_id(&self) -> &str {
        &self.download_id
    }

    /// Returns the current download status.
    pub fn status(&self) -> DownloadStatus {
        self.status
    }

    /// Returns the estimator configuration.
    pub fn config(&self) -> &EstimatorConfig {
        &self.config
    }

    /// Updates the known total size of the download.
    pub fn set_total_bytes(&mut self, total_bytes: Option<u64>) -> Result<(), DownloadSpeedError> {
        if let Some(total) = total_bytes {
            if total < self.bytes_received {
                return Err(DownloadSpeedError::InvalidTotalBytes {
                    received: self.bytes_received,
                    total,
                });
            }
        }
        self.total_bytes = total_bytes;
        if let Some(total) = total_bytes {
            if self.bytes_received == total && !self.status.is_terminal() {
                self.status = DownloadStatus::Completed;
                self.instantaneous_bps = 0.0;
            }
        }
        Ok(())
    }

    /// Records a progress observation at `timestamp_ms` (monotonic milliseconds).
    pub fn record_progress(
        &mut self,
        timestamp_ms: u64,
        bytes_received: u64,
    ) -> Result<DownloadMetrics, DownloadSpeedError> {
        if self.status.is_terminal() {
            return Err(DownloadSpeedError::InvalidStateTransition {
                from: self.status,
                to: DownloadStatus::Active,
            });
        }
        if let Some(prev_ms) = self.last_sample_ms {
            if timestamp_ms < prev_ms {
                return Err(DownloadSpeedError::TimestampRegression {
                    previous_ms: prev_ms,
                    current_ms: timestamp_ms,
                });
            }
        }
        if bytes_received < self.bytes_received {
            return Err(DownloadSpeedError::BytesRegression {
                previous_bytes: self.bytes_received,
                current_bytes: bytes_received,
            });
        }
        if let Some(total) = self.total_bytes {
            if bytes_received > total {
                return Err(DownloadSpeedError::InvalidTotalBytes {
                    received: bytes_received,
                    total,
                });
            }
        }

        if self.started_at_ms.is_none() {
            self.started_at_ms = Some(timestamp_ms);
        }

        let bytes_delta = bytes_received - self.bytes_received;
        if bytes_delta > 0 || self.last_progress_ms.is_none() {
            self.last_progress_ms = Some(timestamp_ms);
        }

        if let Some(prev_sample) = self.samples.back().copied() {
            let dt_ms = timestamp_ms.saturating_sub(prev_sample.timestamp_ms);
            if dt_ms > 0 {
                let inst = (bytes_delta as f64) * 1000.0 / (dt_ms as f64);
                self.instantaneous_bps = inst;
                self.ema_bps = Some(match self.ema_bps {
                    Some(prev_ema) => {
                        self.config.ema_alpha * inst + (1.0 - self.config.ema_alpha) * prev_ema
                    }
                    None => inst,
                });
            }
        }

        self.bytes_received = bytes_received;
        self.last_sample_ms = Some(timestamp_ms);
        self.push_sample(ProgressSample {
            timestamp_ms,
            bytes_received,
        });
        self.prune_samples(timestamp_ms);

        if let Some(total) = self.total_bytes {
            if bytes_received >= total {
                self.status = DownloadStatus::Completed;
                self.instantaneous_bps = 0.0;
                return Ok(self.metrics(timestamp_ms));
            }
        }

        if bytes_delta > 0 {
            self.status = DownloadStatus::Active;
        } else if self.is_stalled_at(timestamp_ms) {
            self.status = DownloadStatus::Stalled;
            self.instantaneous_bps = 0.0;
        } else if self.status == DownloadStatus::Pending {
            self.status = DownloadStatus::Active;
        }

        Ok(self.metrics(timestamp_ms))
    }

    /// Advances time without new bytes to detect stalls and refresh metrics.
    pub fn tick(&mut self, now_ms: u64) -> Result<DownloadMetrics, DownloadSpeedError> {
        if let Some(prev_ms) = self.last_sample_ms {
            if now_ms < prev_ms {
                return Err(DownloadSpeedError::TimestampRegression {
                    previous_ms: prev_ms,
                    current_ms: now_ms,
                });
            }
        }

        self.prune_samples(now_ms);
        if matches!(self.status, DownloadStatus::Active) && self.is_stalled_at(now_ms) {
            self.status = DownloadStatus::Stalled;
            self.instantaneous_bps = 0.0;
            self.ema_bps = Some(0.0);
        }

        Ok(self.metrics(now_ms))
    }

    /// Pauses an active, stalled, or pending download.
    pub fn pause(&mut self) -> Result<(), DownloadSpeedError> {
        if self.status.is_terminal() {
            return Err(DownloadSpeedError::InvalidStateTransition {
                from: self.status,
                to: DownloadStatus::Paused,
            });
        }
        self.status = DownloadStatus::Paused;
        self.instantaneous_bps = 0.0;
        Ok(())
    }

    /// Resumes a paused download at `now_ms`, resetting the stall timer and sample window.
    pub fn resume(&mut self, now_ms: u64) -> Result<(), DownloadSpeedError> {
        if self.status != DownloadStatus::Paused {
            return Err(DownloadSpeedError::InvalidStateTransition {
                from: self.status,
                to: DownloadStatus::Active,
            });
        }
        if let Some(prev_ms) = self.last_sample_ms {
            if now_ms < prev_ms {
                return Err(DownloadSpeedError::TimestampRegression {
                    previous_ms: prev_ms,
                    current_ms: now_ms,
                });
            }
        }
        self.status = DownloadStatus::Active;
        self.last_progress_ms = Some(now_ms);
        self.last_sample_ms = Some(now_ms);
        self.samples.clear();
        self.samples.push_back(ProgressSample {
            timestamp_ms: now_ms,
            bytes_received: self.bytes_received,
        });
        self.ema_bps = None;
        self.instantaneous_bps = 0.0;
        Ok(())
    }

    /// Marks the download as completed.
    pub fn mark_completed(&mut self) -> Result<(), DownloadSpeedError> {
        if self.status.is_terminal() && self.status != DownloadStatus::Completed {
            return Err(DownloadSpeedError::InvalidStateTransition {
                from: self.status,
                to: DownloadStatus::Completed,
            });
        }
        self.status = DownloadStatus::Completed;
        self.instantaneous_bps = 0.0;
        Ok(())
    }

    /// Marks the download as failed.
    pub fn mark_failed(&mut self) -> Result<(), DownloadSpeedError> {
        if self.status.is_terminal() && self.status != DownloadStatus::Failed {
            return Err(DownloadSpeedError::InvalidStateTransition {
                from: self.status,
                to: DownloadStatus::Failed,
            });
        }
        self.status = DownloadStatus::Failed;
        self.instantaneous_bps = 0.0;
        Ok(())
    }

    /// Marks the download as cancelled.
    pub fn mark_cancelled(&mut self) -> Result<(), DownloadSpeedError> {
        if self.status.is_terminal() && self.status != DownloadStatus::Cancelled {
            return Err(DownloadSpeedError::InvalidStateTransition {
                from: self.status,
                to: DownloadStatus::Cancelled,
            });
        }
        self.status = DownloadStatus::Cancelled;
        self.instantaneous_bps = 0.0;
        Ok(())
    }

    /// Computes a full [`DownloadMetrics`] snapshot at `now_ms` without mutating state.
    pub fn metrics(&self, now_ms: u64) -> DownloadMetrics {
        let elapsed_ms = self
            .started_at_ms
            .map(|start| now_ms.saturating_sub(start))
            .unwrap_or(0);

        let average_bps = if elapsed_ms > 0 {
            (self.bytes_received as f64) * 1000.0 / (elapsed_ms as f64)
        } else {
            0.0
        };

        let smoothed_bps = match self.status {
            DownloadStatus::Active => self.compute_smoothed_bps(),
            _ => 0.0,
        };

        let remaining_bytes = self
            .total_bytes
            .map(|total| total.saturating_sub(self.bytes_received));

        let progress_percent = self.total_bytes.map(|total| {
            if total == 0 {
                100.0
            } else {
                ((self.bytes_received as f64) * 100.0 / (total as f64)).clamp(0.0, 100.0)
            }
        });

        let eta_seconds = self.compute_eta_seconds(remaining_bytes, smoothed_bps);

        let unit = self.config.unit_system;
        let formatted_speed = match self.status {
            DownloadStatus::Active => format_speed(smoothed_bps, unit),
            DownloadStatus::Stalled => "0 B/s (stalled)".to_string(),
            DownloadStatus::Paused => "paused".to_string(),
            DownloadStatus::Completed => "completed".to_string(),
            DownloadStatus::Failed => "failed".to_string(),
            DownloadStatus::Cancelled => "cancelled".to_string(),
            DownloadStatus::Pending => "--".to_string(),
        };

        let formatted_eta = match self.status {
            DownloadStatus::Completed => "0s".to_string(),
            DownloadStatus::Active => format_eta(eta_seconds),
            _ => "--".to_string(),
        };

        let formatted_progress = format_progress(self.bytes_received, self.total_bytes, unit);

        DownloadMetrics {
            download_id: self.download_id.clone(),
            status: self.status,
            bytes_received: self.bytes_received,
            total_bytes: self.total_bytes,
            remaining_bytes,
            progress_percent,
            instantaneous_bps: self.instantaneous_bps,
            smoothed_bps,
            average_bps,
            eta_seconds,
            elapsed_ms,
            formatted_speed,
            formatted_eta,
            formatted_progress,
        }
    }

    fn push_sample(&mut self, sample: ProgressSample) {
        if self.samples.len() >= self.config.max_samples {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    fn prune_samples(&mut self, now_ms: u64) {
        let cutoff = now_ms.saturating_sub(self.config.window_ms);
        while self.samples.len() > self.config.min_samples_for_eta {
            match self.samples.front() {
                Some(front) if front.timestamp_ms < cutoff => {
                    self.samples.pop_front();
                }
                _ => break,
            }
        }
    }

    fn is_stalled_at(&self, now_ms: u64) -> bool {
        match self.last_progress_ms {
            Some(last_ms) => now_ms.saturating_sub(last_ms) >= self.config.stall_timeout_ms,
            None => false,
        }
    }

    fn compute_smoothed_bps(&self) -> f64 {
        let windowed_bps = if self.samples.len() >= 2 {
            match (self.samples.front(), self.samples.back()) {
                (Some(first), Some(last)) if last.timestamp_ms > first.timestamp_ms => {
                    let dt = (last.timestamp_ms - first.timestamp_ms) as f64;
                    let db = last.bytes_received.saturating_sub(first.bytes_received) as f64;
                    Some(db * 1000.0 / dt)
                }
                _ => None,
            }
        } else {
            None
        };

        match (self.ema_bps, windowed_bps) {
            (Some(ema), Some(win)) => (ema + win) * 0.5,
            (Some(ema), None) => ema,
            (None, Some(win)) => win,
            (None, None) => 0.0,
        }
    }

    fn compute_eta_seconds(&self, remaining_bytes: Option<u64>, smoothed_bps: f64) -> Option<u64> {
        if self.status == DownloadStatus::Completed {
            return Some(0);
        }
        if self.status != DownloadStatus::Active {
            return None;
        }
        if self.samples.len() < self.config.min_samples_for_eta {
            return None;
        }
        let remaining = remaining_bytes?;
        if remaining == 0 {
            return Some(0);
        }
        if !smoothed_bps.is_finite() || smoothed_bps <= 0.0 {
            return None;
        }
        let secs = (remaining as f64 / smoothed_bps).ceil();
        if !secs.is_finite() || secs < 0.0 {
            return None;
        }
        if secs >= u64::MAX as f64 {
            return Some(u64::MAX);
        }
        Some(secs as u64)
    }
}

/// Formats a byte count using the specified [`UnitSystem`].
pub fn format_bytes(bytes: u64, unit_system: UnitSystem) -> String {
    let (base, units): (f64, [&str; 5]) = match unit_system {
        UnitSystem::Decimal => (1000.0, ["B", "KB", "MB", "GB", "TB"]),
        UnitSystem::Binary => (1024.0, ["B", "KiB", "MiB", "GiB", "TiB"]),
    };

    if bytes < base as u64 {
        return format!("{bytes} B");
    }

    let mut value = bytes as f64;
    let mut idx = 0;
    while value >= base && idx < units.len() - 1 {
        value /= base;
        idx += 1;
    }

    if value >= 100.0 {
        format!("{value:.0} {}", units[idx])
    } else if value >= 10.0 {
        format!("{value:.1} {}", units[idx])
    } else {
        format!("{value:.2} {}", units[idx])
    }
}

/// Formats a transfer rate in bytes per second using the specified [`UnitSystem`].
pub fn format_speed(bytes_per_sec: f64, unit_system: UnitSystem) -> String {
    if !bytes_per_sec.is_finite() || bytes_per_sec <= 0.0 {
        return "0 B/s".to_string();
    }
    let rounded = if bytes_per_sec >= u64::MAX as f64 {
        u64::MAX
    } else {
        bytes_per_sec.round() as u64
    };
    format!("{}/s", format_bytes(rounded, unit_system))
}

/// Formats an optional ETA duration in seconds into a compact human-readable string.
pub fn format_eta(eta_seconds: Option<u64>) -> String {
    let Some(secs) = eta_seconds else {
        return "--".to_string();
    };
    if secs == 0 {
        return "0s".to_string();
    }
    if secs < 60 {
        return format!("{secs}s");
    }
    let minutes = secs / 60;
    let rem_secs = secs % 60;
    if minutes < 60 {
        return format!("{minutes}m {rem_secs:02}s");
    }
    let hours = minutes / 60;
    let rem_mins = minutes % 60;
    if hours < 24 {
        return format!("{hours}h {rem_mins:02}m");
    }
    let days = hours / 24;
    let rem_hours = hours % 24;
    format!("{days}d {rem_hours:02}h")
}

/// Formats received and optional total bytes as `"X / Y (Z%)"` or `"X / ?"`.
pub fn format_progress(received: u64, total: Option<u64>, unit_system: UnitSystem) -> String {
    let recv_str = format_bytes(received, unit_system);
    match total {
        Some(total_bytes) => {
            let total_str = format_bytes(total_bytes, unit_system);
            let pct = if total_bytes == 0 {
                100.0
            } else {
                ((received as f64) * 100.0 / (total_bytes as f64)).clamp(0.0, 100.0)
            };
            format!("{recv_str} / {total_str} ({pct:.1}%)")
        }
        None => format!("{recv_str} / ?"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_speed_eta_and_completion() {
        let mut est =
            DownloadSpeedEstimator::with_default_config("dl-ubuntu-iso", Some(10_000_000))
                .expect("valid estimator");

        let m1 = est.record_progress(1_000, 0).expect("sample 1");
        assert_eq!(m1.status, DownloadStatus::Active);
        assert_eq!(m1.eta_seconds, None);

        // 2 MB in 1 second => 2_000_000 B/s
        let m2 = est.record_progress(2_000, 2_000_000).expect("sample 2");
        assert_eq!(m2.status, DownloadStatus::Active);
        assert!((m2.instantaneous_bps - 2_000_000.0).abs() < 1e-6);
        assert!((m2.smoothed_bps - 2_000_000.0).abs() < 1e-6);
        assert_eq!(m2.remaining_bytes, Some(8_000_000));
        assert_eq!(m2.eta_seconds, Some(4));
        assert_eq!(m2.formatted_eta, "4s");
        assert_eq!(m2.formatted_speed, "2.00 MB/s");

        // Complete the download
        let m3 = est.record_progress(6_000, 10_000_000).expect("sample 3");
        assert_eq!(m3.status, DownloadStatus::Completed);
        assert_eq!(m3.remaining_bytes, Some(0));
        assert_eq!(m3.progress_percent, Some(100.0));
        assert_eq!(m3.eta_seconds, Some(0));
        assert_eq!(m3.formatted_eta, "0s");
    }

    #[test]
    fn stall_detection_and_pause_resume() {
        let config = EstimatorConfig {
            stall_timeout_ms: 3_000,
            ..EstimatorConfig::default()
        };
        let mut est = DownloadSpeedEstimator::new("dl-1", Some(1_000), config).expect("estimator");

        est.record_progress(1_000, 200).expect("s1");
        est.record_progress(2_000, 400).expect("s2");
        assert_eq!(est.status(), DownloadStatus::Active);

        // Tick before stall timeout
        let m_before = est.tick(4_500).expect("tick before stall");
        assert_eq!(m_before.status, DownloadStatus::Active);

        // Tick past stall timeout (2_000 + 3_000 = 5_000)
        let m_stalled = est.tick(5_000).expect("tick at stall");
        assert_eq!(m_stalled.status, DownloadStatus::Stalled);
        assert_eq!(m_stalled.eta_seconds, None);
        assert_eq!(m_stalled.formatted_speed, "0 B/s (stalled)");

        // Pause and resume
        est.pause().expect("pause");
        assert_eq!(est.status(), DownloadStatus::Paused);
        est.resume(10_000).expect("resume");
        assert_eq!(est.status(), DownloadStatus::Active);

        let m_resumed = est.record_progress(11_000, 700).expect("resumed sample");
        assert_eq!(m_resumed.status, DownloadStatus::Active);
        assert_eq!(m_resumed.remaining_bytes, Some(300));
        assert_eq!(m_resumed.eta_seconds, Some(1));
    }

    #[test]
    fn validation_and_regression_errors() {
        let bad_cfg = EstimatorConfig {
            ema_alpha: 0.0,
            ..EstimatorConfig::default()
        };
        assert!(matches!(
            DownloadSpeedEstimator::new("dl-bad", None, bad_cfg),
            Err(DownloadSpeedError::InvalidConfig(_))
        ));

        let mut est =
            DownloadSpeedEstimator::with_default_config("dl-err", Some(500)).expect("estimator");
        est.record_progress(2_000, 100).expect("s1");

        assert_eq!(
            est.record_progress(1_500, 150),
            Err(DownloadSpeedError::TimestampRegression {
                previous_ms: 2_000,
                current_ms: 1_500,
            })
        );

        assert_eq!(
            est.record_progress(2_500, 50),
            Err(DownloadSpeedError::BytesRegression {
                previous_bytes: 100,
                current_bytes: 50,
            })
        );

        assert_eq!(
            est.record_progress(3_000, 600),
            Err(DownloadSpeedError::InvalidTotalBytes {
                received: 600,
                total: 500,
            })
        );
    }

    #[test]
    fn formatting_and_serde_roundtrip() {
        assert_eq!(format_bytes(512, UnitSystem::Decimal), "512 B");
        assert_eq!(format_bytes(1_500_000, UnitSystem::Decimal), "1.50 MB");
        assert_eq!(format_bytes(1_048_576, UnitSystem::Binary), "1.00 MiB");
        assert_eq!(format_eta(None), "--");
        assert_eq!(format_eta(Some(0)), "0s");
        assert_eq!(format_eta(Some(125)), "2m 05s");
        assert_eq!(format_eta(Some(3_665)), "1h 01m");
        assert_eq!(format_eta(Some(90_000)), "1d 01h");
        assert_eq!(format_progress(500, None, UnitSystem::Decimal), "500 B / ?");

        let mut est =
            DownloadSpeedEstimator::with_default_config("dl-serde", Some(2_000)).expect("est");
        let metrics = est.record_progress(1_000, 500).expect("s1");

        let est_json = serde_json::to_string(&est).expect("serialize estimator");
        let est_back: DownloadSpeedEstimator =
            serde_json::from_str(&est_json).expect("deserialize estimator");
        assert_eq!(est, est_back);

        let metrics_json = serde_json::to_string(&metrics).expect("serialize metrics");
        let metrics_back: DownloadMetrics =
            serde_json::from_str(&metrics_json).expect("deserialize metrics");
        assert_eq!(metrics, metrics_back);
    }
}
