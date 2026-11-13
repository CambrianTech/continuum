//! The clog_*/log_* macros' file sink: the LoggerModule's side of
//! `logging::clog_layer`, which names nothing above the kernel.

use super::logger::{log_sink_ready, queue_log, LogLevel};
use crate::logging::clog_layer::ClogSink;

/// The clog_*/log_* macros' file sink: `logging::clog_layer::ClogLayer` hands every routed
/// line to this queue, so the per-category files, batching and rate limiting are the
/// LoggerModule's. Lines fired before the LoggerModule exists are dropped, as they always were.
pub struct LoggerQueueSink;

impl ClogSink for LoggerQueueSink {
    fn ready(&self) -> bool {
        log_sink_ready()
    }

    fn write(&self, category: &str, level: tracing::Level, component: &str, message: &str) {
        queue_log(category, LogLevel::from(level), component, message);
    }
}

/// The macros emit DEBUG through ERROR only; TRACE has no file level and shares DEBUG's.
impl From<tracing::Level> for LogLevel {
    fn from(level: tracing::Level) -> Self {
        if level == tracing::Level::ERROR {
            LogLevel::Error
        } else if level == tracing::Level::WARN {
            LogLevel::Warn
        } else if level == tracing::Level::INFO {
            LogLevel::Info
        } else {
            LogLevel::Debug // DEBUG and TRACE: Level is a struct, its five values are not an enum to match
        }
    }
}
