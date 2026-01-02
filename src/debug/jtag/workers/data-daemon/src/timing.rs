//! Precision Timing Module for RustDataDaemon
//!
//! High-resolution timing instrumentation for identifying bottlenecks.
//! Designed for AR-style performance analysis where every microsecond matters.
//!
//! ARCHITECTURE:
//! - Nanosecond precision using std::time::Instant
//! - Lock-free metrics collection where possible
//! - Periodic aggregation (P50/P95/P99)
//! - Structured JSON output for analysis

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

// ============================================================================
// Timing Record - Captures all timing points for a single request
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimingRecord {
    // Identity
    pub request_id: String,
    pub timestamp_ms: u64,  // Unix timestamp for correlation

    // Request info
    pub request_type: String,      // "list", "create", "update", "delete", "ping", etc.
    pub collection: Option<String>,
    pub adapter_handle: Option<String>,

    // Timing breakdown (nanoseconds)
    pub socket_read_ns: u64,
    pub parse_ns: u64,
    pub route_ns: u64,
    pub query_build_ns: u64,
    pub lock_wait_ns: u64,
    pub execute_ns: u64,
    pub serialize_ns: u64,
    pub socket_write_ns: u64,

    // Derived totals
    pub total_ns: u64,
    pub handle_ns: u64,  // route + query_build + lock_wait + execute

    // Context
    pub concurrent_requests: usize,
    pub queue_depth: usize,
    pub result_count: Option<usize>,
    pub success: bool,
    pub error: Option<String>,
}

impl TimingRecord {
    pub fn new(request_type: &str) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();

        Self {
            request_id: Uuid::new_v4().to_string(),
            timestamp_ms: now.as_millis() as u64,
            request_type: request_type.to_string(),
            collection: None,
            adapter_handle: None,
            socket_read_ns: 0,
            parse_ns: 0,
            route_ns: 0,
            query_build_ns: 0,
            lock_wait_ns: 0,
            execute_ns: 0,
            serialize_ns: 0,
            socket_write_ns: 0,
            total_ns: 0,
            handle_ns: 0,
            concurrent_requests: 0,
            queue_depth: 0,
            result_count: None,
            success: true,
            error: None,
        }
    }

    pub fn finalize(&mut self) {
        self.handle_ns = self.route_ns + self.query_build_ns + self.lock_wait_ns + self.execute_ns;
        self.total_ns = self.socket_read_ns + self.parse_ns + self.handle_ns +
                        self.serialize_ns + self.socket_write_ns;
    }
}

// ============================================================================
// Request Timer - RAII-style timer for measuring request phases
// ============================================================================

pub struct RequestTimer {
    pub record: TimingRecord,
    phase_start: Instant,
    request_start: Instant,
}

impl RequestTimer {
    pub fn start(request_type: &str) -> Self {
        let now = Instant::now();
        Self {
            record: TimingRecord::new(request_type),
            phase_start: now,
            request_start: now,
        }
    }

    /// Mark end of socket read phase
    pub fn mark_socket_read(&mut self) {
        self.record.socket_read_ns = self.phase_start.elapsed().as_nanos() as u64;
        self.phase_start = Instant::now();
    }

    /// Mark end of parse phase
    pub fn mark_parse(&mut self) {
        self.record.parse_ns = self.phase_start.elapsed().as_nanos() as u64;
        self.phase_start = Instant::now();
    }

    /// Mark end of route phase
    pub fn mark_route(&mut self) {
        self.record.route_ns = self.phase_start.elapsed().as_nanos() as u64;
        self.phase_start = Instant::now();
    }

    /// Mark end of query build phase
    pub fn mark_query_build(&mut self) {
        self.record.query_build_ns = self.phase_start.elapsed().as_nanos() as u64;
        self.phase_start = Instant::now();
    }

    /// Mark end of lock wait phase
    pub fn mark_lock_acquired(&mut self) {
        self.record.lock_wait_ns = self.phase_start.elapsed().as_nanos() as u64;
        self.phase_start = Instant::now();
    }

    /// Mark end of execute phase
    pub fn mark_execute(&mut self) {
        self.record.execute_ns = self.phase_start.elapsed().as_nanos() as u64;
        self.phase_start = Instant::now();
    }

    /// Mark end of serialize phase
    pub fn mark_serialize(&mut self) {
        self.record.serialize_ns = self.phase_start.elapsed().as_nanos() as u64;
        self.phase_start = Instant::now();
    }

    /// Mark end of socket write phase
    pub fn mark_socket_write(&mut self) {
        self.record.socket_write_ns = self.phase_start.elapsed().as_nanos() as u64;
    }

    /// Set request metadata
    pub fn set_collection(&mut self, collection: &str) {
        self.record.collection = Some(collection.to_string());
    }

    pub fn set_adapter_handle(&mut self, handle: &str) {
        self.record.adapter_handle = Some(handle.to_string());
    }

    pub fn set_result_count(&mut self, count: usize) {
        self.record.result_count = Some(count);
    }

    pub fn set_concurrent(&mut self, count: usize) {
        self.record.concurrent_requests = count;
    }

    pub fn set_queue_depth(&mut self, depth: usize) {
        self.record.queue_depth = depth;
    }

    pub fn set_error(&mut self, error: &str) {
        self.record.success = false;
        self.record.error = Some(error.to_string());
    }

    /// Finalize and return the record
    pub fn finish(mut self) -> TimingRecord {
        self.record.finalize();
        self.record
    }
}

// ============================================================================
// Metrics Aggregator - Computes P50/P95/P99 percentiles
// ============================================================================

#[derive(Debug, Clone, Serialize)]
pub struct PercentileStats {
    pub count: usize,
    pub min_ns: u64,
    pub max_ns: u64,
    pub mean_ns: u64,
    pub p50_ns: u64,
    pub p95_ns: u64,
    pub p99_ns: u64,
}

impl PercentileStats {
    pub fn from_values(mut values: Vec<u64>) -> Self {
        if values.is_empty() {
            return Self {
                count: 0,
                min_ns: 0,
                max_ns: 0,
                mean_ns: 0,
                p50_ns: 0,
                p95_ns: 0,
                p99_ns: 0,
            };
        }

        values.sort();
        let count = values.len();
        let sum: u64 = values.iter().sum();

        Self {
            count,
            min_ns: values[0],
            max_ns: values[count - 1],
            mean_ns: sum / count as u64,
            p50_ns: values[count * 50 / 100],
            p95_ns: values[count * 95 / 100],
            p99_ns: values[count * 99 / 100],
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AggregatedMetrics {
    pub window_start_ms: u64,
    pub window_end_ms: u64,
    pub total_requests: usize,

    // By phase
    pub socket_read: PercentileStats,
    pub parse: PercentileStats,
    pub query_build: PercentileStats,
    pub lock_wait: PercentileStats,
    pub execute: PercentileStats,
    pub serialize: PercentileStats,
    pub socket_write: PercentileStats,
    pub total: PercentileStats,

    // By request type
    pub by_type: std::collections::HashMap<String, PercentileStats>,
}

// ============================================================================
// Metrics Collector - Thread-safe collection and aggregation
// ============================================================================

pub struct MetricsCollector {
    // Ring buffer of recent records (for percentile calculation)
    records: Mutex<VecDeque<TimingRecord>>,
    max_records: usize,

    // Atomic counters for real-time stats
    pub active_requests: AtomicUsize,
    total_requests: AtomicU64,

    // Log file
    log_file: Mutex<Option<File>>,
    log_path: String,
}

impl MetricsCollector {
    pub fn new(log_path: &str, max_records: usize) -> Self {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
            .ok();

        Self {
            records: Mutex::new(VecDeque::with_capacity(max_records)),
            max_records,
            active_requests: AtomicUsize::new(0),
            total_requests: AtomicU64::new(0),
            log_file: Mutex::new(file),
            log_path: log_path.to_string(),
        }
    }

    /// Record a completed request
    pub fn record(&self, mut timing: TimingRecord) {
        // Update concurrent count
        timing.concurrent_requests = self.active_requests.load(Ordering::Relaxed);

        // Increment total
        self.total_requests.fetch_add(1, Ordering::Relaxed);

        // Write to log file
        if let Ok(mut file_guard) = self.log_file.lock() {
            if let Some(ref mut file) = *file_guard {
                if let Ok(json) = serde_json::to_string(&timing) {
                    let _ = writeln!(file, "{}", json);
                }
            }
        }

        // Add to ring buffer
        if let Ok(mut records) = self.records.lock() {
            if records.len() >= self.max_records {
                records.pop_front();
            }
            records.push_back(timing);
        }
    }

    /// Increment active request count
    pub fn request_start(&self) {
        self.active_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement active request count
    pub fn request_end(&self) {
        self.active_requests.fetch_sub(1, Ordering::Relaxed);
    }

    /// Get current active request count
    pub fn get_active_count(&self) -> usize {
        self.active_requests.load(Ordering::Relaxed)
    }

    /// Compute aggregated metrics from recent records
    pub fn aggregate(&self) -> AggregatedMetrics {
        let records = self.records.lock().unwrap();

        if records.is_empty() {
            return AggregatedMetrics {
                window_start_ms: 0,
                window_end_ms: 0,
                total_requests: 0,
                socket_read: PercentileStats::from_values(vec![]),
                parse: PercentileStats::from_values(vec![]),
                query_build: PercentileStats::from_values(vec![]),
                lock_wait: PercentileStats::from_values(vec![]),
                execute: PercentileStats::from_values(vec![]),
                serialize: PercentileStats::from_values(vec![]),
                socket_write: PercentileStats::from_values(vec![]),
                total: PercentileStats::from_values(vec![]),
                by_type: std::collections::HashMap::new(),
            };
        }

        // Collect values by phase
        let socket_read: Vec<u64> = records.iter().map(|r| r.socket_read_ns).collect();
        let parse: Vec<u64> = records.iter().map(|r| r.parse_ns).collect();
        let query_build: Vec<u64> = records.iter().map(|r| r.query_build_ns).collect();
        let lock_wait: Vec<u64> = records.iter().map(|r| r.lock_wait_ns).collect();
        let execute: Vec<u64> = records.iter().map(|r| r.execute_ns).collect();
        let serialize: Vec<u64> = records.iter().map(|r| r.serialize_ns).collect();
        let socket_write: Vec<u64> = records.iter().map(|r| r.socket_write_ns).collect();
        let total: Vec<u64> = records.iter().map(|r| r.total_ns).collect();

        // Group by request type
        let mut by_type: std::collections::HashMap<String, Vec<u64>> = std::collections::HashMap::new();
        for r in records.iter() {
            by_type.entry(r.request_type.clone())
                .or_insert_with(Vec::new)
                .push(r.total_ns);
        }

        let by_type_stats: std::collections::HashMap<String, PercentileStats> = by_type
            .into_iter()
            .map(|(k, v)| (k, PercentileStats::from_values(v)))
            .collect();

        AggregatedMetrics {
            window_start_ms: records.front().map(|r| r.timestamp_ms).unwrap_or(0),
            window_end_ms: records.back().map(|r| r.timestamp_ms).unwrap_or(0),
            total_requests: records.len(),
            socket_read: PercentileStats::from_values(socket_read),
            parse: PercentileStats::from_values(parse),
            query_build: PercentileStats::from_values(query_build),
            lock_wait: PercentileStats::from_values(lock_wait),
            execute: PercentileStats::from_values(execute),
            serialize: PercentileStats::from_values(serialize),
            socket_write: PercentileStats::from_values(socket_write),
            total: PercentileStats::from_values(total),
            by_type: by_type_stats,
        }
    }

    /// Print summary to stdout
    pub fn print_summary(&self) {
        let metrics = self.aggregate();
        println!("\n📊 TIMING SUMMARY (last {} requests)", metrics.total_requests);
        println!("═══════════════════════════════════════════════════════");

        fn format_ns(ns: u64) -> String {
            if ns >= 1_000_000_000 {
                format!("{:.2}s", ns as f64 / 1_000_000_000.0)
            } else if ns >= 1_000_000 {
                format!("{:.2}ms", ns as f64 / 1_000_000.0)
            } else if ns >= 1_000 {
                format!("{:.2}µs", ns as f64 / 1_000.0)
            } else {
                format!("{}ns", ns)
            }
        }

        println!("Phase         │ P50        │ P95        │ P99        │");
        println!("──────────────┼────────────┼────────────┼────────────┤");
        println!("socket_read   │ {:>10} │ {:>10} │ {:>10} │",
            format_ns(metrics.socket_read.p50_ns),
            format_ns(metrics.socket_read.p95_ns),
            format_ns(metrics.socket_read.p99_ns));
        println!("parse         │ {:>10} │ {:>10} │ {:>10} │",
            format_ns(metrics.parse.p50_ns),
            format_ns(metrics.parse.p95_ns),
            format_ns(metrics.parse.p99_ns));
        println!("query_build   │ {:>10} │ {:>10} │ {:>10} │",
            format_ns(metrics.query_build.p50_ns),
            format_ns(metrics.query_build.p95_ns),
            format_ns(metrics.query_build.p99_ns));
        println!("lock_wait     │ {:>10} │ {:>10} │ {:>10} │",
            format_ns(metrics.lock_wait.p50_ns),
            format_ns(metrics.lock_wait.p95_ns),
            format_ns(metrics.lock_wait.p99_ns));
        println!("execute       │ {:>10} │ {:>10} │ {:>10} │",
            format_ns(metrics.execute.p50_ns),
            format_ns(metrics.execute.p95_ns),
            format_ns(metrics.execute.p99_ns));
        println!("serialize     │ {:>10} │ {:>10} │ {:>10} │",
            format_ns(metrics.serialize.p50_ns),
            format_ns(metrics.serialize.p95_ns),
            format_ns(metrics.serialize.p99_ns));
        println!("socket_write  │ {:>10} │ {:>10} │ {:>10} │",
            format_ns(metrics.socket_write.p50_ns),
            format_ns(metrics.socket_write.p95_ns),
            format_ns(metrics.socket_write.p99_ns));
        println!("──────────────┼────────────┼────────────┼────────────┤");
        println!("TOTAL         │ {:>10} │ {:>10} │ {:>10} │",
            format_ns(metrics.total.p50_ns),
            format_ns(metrics.total.p95_ns),
            format_ns(metrics.total.p99_ns));
        println!("═══════════════════════════════════════════════════════\n");

        if !metrics.by_type.is_empty() {
            println!("By Request Type:");
            for (req_type, stats) in &metrics.by_type {
                println!("  {:12} │ P50: {:>10} │ P95: {:>10} │ count: {}",
                    req_type,
                    format_ns(stats.p50_ns),
                    format_ns(stats.p95_ns),
                    stats.count);
            }
            println!();
        }
    }
}

// ============================================================================
// Global Metrics Instance
// ============================================================================

lazy_static::lazy_static! {
    pub static ref METRICS: MetricsCollector = {
        // Log to system log directory
        let log_path = std::env::var("JTAG_TIMING_LOG")
            .unwrap_or_else(|_| "/tmp/jtag-data-daemon-timing.jsonl".to_string());
        MetricsCollector::new(&log_path, 10000)  // Keep last 10k records
    };
}
