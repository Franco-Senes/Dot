//! Registro estructurado ligero controlado por la variable de entorno `DOT_LOG`
//! (con respaldo obsoleto `SEARCH_LOG`).

use once_cell::sync::OnceCell;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
            Self::Trace => "TRACE",
        }
    }
}

#[derive(Debug)]
struct LogFilter {
    default_level: Option<Level>,
    module_overrides: Vec<(String, Option<Level>)>,
}

impl LogFilter {
    fn parse(spec: &str) -> Self {
        let mut default_level = Some(Level::Info);
        let mut module_overrides = Vec::new();

        for part in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            if let Some((target, lvl_str)) = part.split_once('=') {
                module_overrides.push((target.trim().to_string(), parse_level(lvl_str.trim())));
            } else {
                default_level = parse_level(part);
            }
        }

        Self {
            default_level,
            module_overrides,
        }
    }

    fn enabled(&self, target: &str, level: Level) -> bool {
        let mut active = self.default_level;
        let mut best_len = 0usize;
        for (prefix, override_lvl) in &self.module_overrides {
            if target.starts_with(prefix.as_str()) && prefix.len() >= best_len {
                best_len = prefix.len();
                active = *override_lvl;
            }
        }
        active.is_some_and(|max| level <= max)
    }
}

fn parse_level(raw: &str) -> Option<Level> {
    match raw.to_ascii_lowercase().as_str() {
        "off" | "none" | "0" => None,
        "error" => Some(Level::Error),
        "warn" | "warning" => Some(Level::Warn),
        "info" => Some(Level::Info),
        "debug" => Some(Level::Debug),
        "trace" => Some(Level::Trace),
        _ => Some(Level::Info),
    }
}

static FILTER: OnceCell<LogFilter> = OnceCell::new();

/// Inicializa el filtro global de logs a partir de `DOT_LOG` (respaldo obsoleto `SEARCH_LOG`, por defecto `info`).
pub fn init_logging() {
    let mut used_legacy = false;
    let _ = FILTER.get_or_init(|| {
        let spec = match crate::env::lookup_with("DOT_LOG", "SEARCH_LOG", |k| std::env::var(k).ok())
        {
            Some(found) => {
                used_legacy = found.used_legacy;
                found.value
            }
            None => "info".to_string(),
        };
        LogFilter::parse(&spec)
    });
    if used_legacy {
        emit(
            Level::Warn,
            module_path!(),
            format_args!(
                "La variable de entorno 'SEARCH_LOG' está obsoleta; usa 'DOT_LOG' en su lugar"
            ),
        );
    }
}

pub fn is_enabled(target: &str, level: Level) -> bool {
    let filter = FILTER.get_or_init(|| {
        let spec = crate::env::lookup_with("DOT_LOG", "SEARCH_LOG", |k| std::env::var(k).ok())
            .map(|f| f.value)
            .unwrap_or_else(|| "info".to_string());
        LogFilter::parse(&spec)
    });
    filter.enabled(target, level)
}

pub fn emit(level: Level, target: &str, args: std::fmt::Arguments<'_>) {
    if !is_enabled(target, level) {
        return;
    }
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let secs = millis / 1000;
    let rem_ms = millis % 1000;
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(
        stderr,
        "[{secs}.{rem_ms:03} {:5} {target}] {args}",
        level.as_str()
    );
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)+) => {
        $crate::trace::emit($crate::trace::Level::Error, module_path!(), format_args!($($arg)+))
    };
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)+) => {
        $crate::trace::emit($crate::trace::Level::Warn, module_path!(), format_args!($($arg)+))
    };
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)+) => {
        $crate::trace::emit($crate::trace::Level::Info, module_path!(), format_args!($($arg)+))
    };
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)+) => {
        $crate::trace::emit($crate::trace::Level::Debug, module_path!(), format_args!($($arg)+))
    };
}

#[macro_export]
macro_rules! trace {
    ($($arg:tt)+) => {
        $crate::trace::emit($crate::trace::Level::Trace, module_path!(), format_args!($($arg)+))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_respects_levels_and_module_overrides() {
        let filter = LogFilter::parse("warn,dot_core::chromium_flags=debug");
        assert!(filter.enabled("dot::window", Level::Error));
        assert!(filter.enabled("dot::window", Level::Warn));
        assert!(!filter.enabled("dot::window", Level::Info));
        assert!(filter.enabled("dot_core::chromium_flags", Level::Debug));
        assert!(!filter.enabled("dot_core::chromium_flags", Level::Trace));
    }

    #[test]
    fn filter_supports_off() {
        let filter = LogFilter::parse("off");
        assert!(!filter.enabled("dot", Level::Error));
    }
}
