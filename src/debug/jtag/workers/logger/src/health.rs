/// Health Module - Worker Statistics and Monitoring
///
/// This module tracks worker health metrics for monitoring:
/// - Uptime tracking
/// - Connection counting
/// - Request throughput
/// - Active file count (via external query)
///
/// The TypeScript LoggerDaemonCore polls these stats via ping messages
/// to detect frozen/unresponsive workers.
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Worker statistics for health monitoring.
///
/// THREAD-SAFE: Wrapped in Arc<Mutex<>> for concurrent access.
pub struct WorkerStats {
    /// When the worker started (for uptime calculation)
    start_time: Instant,
    /// Total connections accepted (lifetime)
    connections_total: u64,
    /// Total requests processed (lifetime)
    requests_processed: u64,
}

/// Thread-safe handle to worker stats.
pub type StatsHandle = Arc<Mutex<WorkerStats>>;

impl WorkerStats {
    /// Create new stats tracker.
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            connections_total: 0,
            requests_processed: 0,
        }
    }

    /// Record a new connection.
    pub fn record_connection(&mut self) {
        self.connections_total += 1;
    }

    /// Record a processed request.
    pub fn record_request(&mut self) {
        self.requests_processed += 1;
    }

    /// Get uptime in milliseconds.
    pub fn uptime_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    /// Get total connections count.
    pub fn connections_total(&self) -> u64 {
        self.connections_total
    }

    /// Get total requests processed.
    pub fn requests_processed(&self) -> u64 {
        self.requests_processed
    }
}

impl Default for WorkerStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a new thread-safe stats handle.
pub fn create_stats() -> StatsHandle {
    Arc::new(Mutex::new(WorkerStats::new()))
}
