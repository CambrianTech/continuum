use serde::{Deserialize, Serialize};
/// Health Module - Universal Worker Protocol Implementation
///
/// Implements the three required interfaces:
/// 1. ping() - Health check with metrics
/// 2. shutdown() - Graceful shutdown with queue draining
/// 3. status() - Detailed diagnostics
///
/// This module is the reference implementation of the universal protocol
/// that all workers must implement.
use std::sync::{Arc, Mutex};
use std::time::Instant;
use ts_rs::TS;

// ============================================================================
// Shared Stats Handle
// ============================================================================

/// Thread-safe handle to worker statistics
pub type StatsHandle = Arc<Mutex<WorkerStats>>;

/// Create a new stats handle with initial values
pub fn create_stats() -> StatsHandle {
    Arc::new(Mutex::new(WorkerStats::new()))
}

// ============================================================================
// Worker Statistics
// ============================================================================

/// Worker statistics tracking
pub struct WorkerStats {
    start_time: Instant,
    connections_total: u64,
    requests_processed: u64,
    errors_total: u64,
    queue_depth: usize, // Current queue size
}

impl WorkerStats {
    pub fn new() -> Self {
        WorkerStats {
            start_time: Instant::now(),
            connections_total: 0,
            requests_processed: 0,
            errors_total: 0,
            queue_depth: 0,
        }
    }

    /// Record a new connection
    pub fn record_connection(&mut self) {
        self.connections_total += 1;
    }

    /// Record a processed request
    pub fn record_request(&mut self) {
        self.requests_processed += 1;
    }

    /// Record an error
    pub fn record_error(&mut self) {
        self.errors_total += 1;
    }

    /// Update queue depth
    pub fn set_queue_depth(&mut self, depth: usize) {
        self.queue_depth = depth;
    }

    /// Get uptime in milliseconds
    pub fn uptime_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    /// Get total connections
    #[allow(dead_code)]
    pub fn connections_total(&self) -> u64 {
        self.connections_total
    }

    /// Get total requests processed
    pub fn requests_processed(&self) -> u64 {
        self.requests_processed
    }

    /// Get total errors
    pub fn errors_total(&self) -> u64 {
        self.errors_total
    }

    /// Get error rate (0.0 - 1.0)
    pub fn error_rate(&self) -> f64 {
        if self.requests_processed == 0 {
            return 0.0;
        }
        self.errors_total as f64 / self.requests_processed as f64
    }

    /// Get current queue depth
    pub fn queue_depth(&self) -> usize {
        self.queue_depth
    }

    /// Determine worker status based on metrics
    pub fn status(&self) -> WorkerStatus {
        // Failing: High error rate
        if self.error_rate() > 0.05 {
            return WorkerStatus::Failing;
        }

        // Degraded: High queue depth (arbitrary threshold)
        if self.queue_depth > 800 {
            return WorkerStatus::Degraded;
        }

        WorkerStatus::Healthy
    }
}

// ============================================================================
// Protocol Types (Universal Worker Protocol)
// ============================================================================

/// Worker status enum
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum WorkerStatus {
    Healthy,
    Degraded,
    Failing,
}

/// Ping result - basic health check
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct PingResult {
    pub uptime_ms: u64,
    pub queue_depth: usize,
    pub processed_total: u64,
    pub errors_total: u64,
    pub memory_mb: f64,
    pub status: WorkerStatus,
}

/// Shutdown request payload
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ShutdownPayload {
    pub timeout_ms: u64,
    pub force: bool,
}

/// Shutdown result
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ShutdownResult {
    pub queue_drained: usize,
    pub shutdown_time_ms: u64,
}

/// Status request payload
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct StatusPayload {
    pub verbose: bool,
}

/// Metrics breakdown
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct StatusMetrics {
    pub queue_depth: usize,
    pub queue_capacity: usize,
    pub processed_total: u64,
    pub errors_total: u64,
    pub error_rate: f64,
}

/// Resource usage
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct StatusResources {
    pub memory_mb: f64,
    pub memory_limit_mb: f64,
    pub threads: usize,
}

/// Status result - detailed diagnostics
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct StatusResult {
    pub worker_type: String,
    pub version: String,
    pub pid: u32,
    pub uptime_ms: u64,
    pub status: WorkerStatus,
    pub metrics: StatusMetrics,
    pub resources: StatusResources,
}

// ============================================================================
// Protocol Implementation
// ============================================================================

/// Get memory usage in MB (placeholder - would use OS-specific API)
fn get_memory_mb() -> f64 {
    // TODO: Implement actual memory usage tracking
    // For now, return a placeholder
    18.5
}

/// Generate ping result from stats
pub fn generate_ping_result(stats: &WorkerStats) -> PingResult {
    PingResult {
        uptime_ms: stats.uptime_ms(),
        queue_depth: stats.queue_depth(),
        processed_total: stats.requests_processed(),
        errors_total: stats.errors_total(),
        memory_mb: get_memory_mb(),
        status: stats.status(),
    }
}

/// Generate detailed status result
pub fn generate_status_result(stats: &WorkerStats, _verbose: bool) -> StatusResult {
    StatusResult {
        worker_type: "data".to_string(),
        version: "1.0.0".to_string(),
        pid: std::process::id(),
        uptime_ms: stats.uptime_ms(),
        status: stats.status(),
        metrics: StatusMetrics {
            queue_depth: stats.queue_depth(),
            queue_capacity: 1000, // Unbounded, but report a "soft" limit
            processed_total: stats.requests_processed(),
            errors_total: stats.errors_total(),
            error_rate: stats.error_rate(),
        },
        resources: StatusResources {
            memory_mb: get_memory_mb(),
            memory_limit_mb: 512.0,
            threads: 4, // Main + processor + N connections
        },
    }
}

// ============================================================================
// TypeScript Export Test
// ============================================================================

#[cfg(test)]
mod export_typescript {
    use super::*;

    #[test]
    fn export_bindings() {
        WorkerStatus::export().expect("Failed to export WorkerStatus");
        PingResult::export().expect("Failed to export PingResult");
        ShutdownPayload::export().expect("Failed to export ShutdownPayload");
        ShutdownResult::export().expect("Failed to export ShutdownResult");
        StatusPayload::export().expect("Failed to export StatusPayload");
        StatusMetrics::export().expect("Failed to export StatusMetrics");
        StatusResources::export().expect("Failed to export StatusResources");
        StatusResult::export().expect("Failed to export StatusResult");
        println!("✅ TypeScript bindings exported to bindings/");
    }
}
