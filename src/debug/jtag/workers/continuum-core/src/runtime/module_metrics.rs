/// ModuleMetrics — Built-in IPC performance monitoring.
///
/// Automatic timing capture for every command. Rolling window stats.
/// Exposed via runtime/metrics/* commands for dashboards and UI.
/// TypeScript types generated via ts-rs for Ares (RTOS controller) integration.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use ts_rs::TS;

const TIMING_WINDOW_SIZE: usize = 1000;
const SLOW_THRESHOLD_MS: u64 = 50;

/// Individual command timing record
#[derive(Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/runtime/CommandTiming.ts")]
#[serde(rename_all = "camelCase")]
pub struct CommandTiming {
    pub command: String,
    pub queue_time_ms: u64,
    pub execute_time_ms: u64,
    pub total_time_ms: u64,
    pub success: bool,
}

pub struct ModuleMetrics {
    module_name: &'static str,
    command_timings: DashMap<String, VecDeque<CommandTiming>>,
    total_commands: AtomicU64,
    total_time_ms: AtomicU64,
    slow_commands: AtomicU64,
}

/// Aggregate statistics for a module
#[derive(Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/runtime/ModuleStats.ts")]
#[serde(rename_all = "camelCase")]
pub struct ModuleStats {
    pub module_name: String,
    pub total_commands: u64,
    pub avg_time_ms: u64,
    pub slow_command_count: u64,
    pub p50_ms: u64,
    pub p95_ms: u64,
    pub p99_ms: u64,
}

/// Tracker returned by start_command(), call finish() when done
pub struct CommandTracker {
    command: String,
    started_at: Instant,
    queued_at: Instant,
}

impl CommandTracker {
    pub fn finish(self, success: bool) -> CommandTiming {
        let now = Instant::now();
        let total_ms = now.duration_since(self.queued_at).as_millis() as u64;
        let execute_ms = now.duration_since(self.started_at).as_millis() as u64;
        let queue_ms = total_ms.saturating_sub(execute_ms);

        CommandTiming {
            command: self.command,
            queue_time_ms: queue_ms,
            execute_time_ms: execute_ms,
            total_time_ms: total_ms,
            success,
        }
    }
}

impl ModuleMetrics {
    pub fn new(module_name: &'static str) -> Self {
        Self {
            module_name,
            command_timings: DashMap::new(),
            total_commands: AtomicU64::new(0),
            total_time_ms: AtomicU64::new(0),
            slow_commands: AtomicU64::new(0),
        }
    }

    /// Called by runtime BEFORE dispatching to module
    pub fn start_command(&self, command: &str, queued_at: Instant) -> CommandTracker {
        CommandTracker {
            command: command.to_string(),
            started_at: Instant::now(),
            queued_at,
        }
    }

    /// Record completed command timing
    pub fn record(&self, timing: CommandTiming) {
        self.total_commands.fetch_add(1, Ordering::Relaxed);
        self.total_time_ms.fetch_add(timing.total_time_ms, Ordering::Relaxed);

        if timing.total_time_ms > SLOW_THRESHOLD_MS {
            self.slow_commands.fetch_add(1, Ordering::Relaxed);
        }

        // Add to rolling window
        let mut timings = self.command_timings
            .entry(timing.command.clone())
            .or_insert_with(VecDeque::new);

        timings.push_back(timing);
        while timings.len() > TIMING_WINDOW_SIZE {
            timings.pop_front();
        }
    }

    /// Get aggregate stats
    pub fn stats(&self) -> ModuleStats {
        let total = self.total_commands.load(Ordering::Relaxed);
        let time = self.total_time_ms.load(Ordering::Relaxed);

        // Collect all timings for percentile calculation
        // Must clone VecDeques first to avoid borrowing DashMap entry across iteration
        let mut all_times: Vec<u64> = Vec::new();
        for entry in self.command_timings.iter() {
            for timing in entry.value().iter() {
                all_times.push(timing.total_time_ms);
            }
        }
        all_times.sort_unstable();

        ModuleStats {
            module_name: self.module_name.to_string(),
            total_commands: total,
            avg_time_ms: if total > 0 { time / total } else { 0 },
            slow_command_count: self.slow_commands.load(Ordering::Relaxed),
            p50_ms: percentile(&all_times, 50),
            p95_ms: percentile(&all_times, 95),
            p99_ms: percentile(&all_times, 99),
        }
    }

    /// Get recent slow commands for debugging
    pub fn slow_commands(&self) -> Vec<CommandTiming> {
        self.command_timings
            .iter()
            .flat_map(|entry| {
                entry.value()
                    .iter()
                    .filter(|t| t.total_time_ms > SLOW_THRESHOLD_MS)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    pub fn module_name(&self) -> &'static str {
        self.module_name
    }
}

fn percentile(sorted: &[u64], p: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = (sorted.len() * p / 100).min(sorted.len() - 1);
    sorted[idx]
}
