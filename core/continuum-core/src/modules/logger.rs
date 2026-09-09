//! LoggerModule — Absorbs the standalone logger worker into the unified runtime.
//!
//! High-performance log file management with:
//! - Batched flushing (every 250ms or 200 messages)
//! - Per-category rate limiting (100 msg/sec default)
//! - File handle caching (files stay open)
//! - Auto-recovery if log files deleted
//! - Per-file locking (no global contention)
//! - Global sender for clog_* macros (non-blocking)
//!
//! Commands:
//! - log/write: Write log entry to file
//! - log/ping: Health check with stats
//!
//! Usage from Rust code:
//! ```rust
//! use crate::clog_info;
//! clog_info!("Session started");  // Non-blocking, routes to modules/voice.log
//! ```
//!
//! Migration from: workers/logger (222 lines main.rs + 4 modules)

use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::sdk_codegen::DynCommand;
use async_trait::async_trait;
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use ts_rs::TS;

// ============================================================================
// Global Logger Sender — For clog_* Macros
// ============================================================================

/// Global sender for clog_* macros. Set by LoggerModule::new().
/// Uses SyncSender with try_send() for GUARANTEED non-blocking.
static GLOBAL_LOG_SENDER: OnceLock<mpsc::SyncSender<WriteLogPayload>> = OnceLock::new();

/// The log queue's admission gate: is the queue accepting, and how many entries are in
/// it, in ONE word.
///
/// Depth is distinct from `LoggerCommandState::pending_writes`, which counts entries the
/// writer has already WRITTEN and not yet flushed. Both are needed to answer "is this
/// module drained": one covers the channel, the other the file buffers, and a stop that
/// checked only the second would flush an empty buffer and report success while entries
/// were still queued behind it. `SyncSender` exposes no length, so depth is counted
/// rather than inferred.
///
/// This was a separate `ADMITTING` bool beside a separate counter, which is the race the
/// turn gate had just been fixed for — check admission, close lands, drain reads zero,
/// reservation increments a queue whose writer is being joined. It is the SAME type as
/// the turn gate now, so the invariant has one implementation instead of two that must be
/// kept agreeing.
static QUEUE: crate::runtime::AdmissionGate = crate::runtime::AdmissionGate::new();

/// Stop accepting new log entries. Returns whether THIS call closed admission.
pub fn close_log_admission() -> bool {
    QUEUE.close()
}

/// Entries sitting in the channel right now. Read by the drain phase of shutdown.
pub fn queued_log_entries() -> u64 {
    QUEUE.in_flight()
}

/// The WRITER's release: one entry has left the channel AND been written.
///
/// Paired with the `forget` in the enqueue path — the producer hands the count to the
/// writer, and the writer gives it back only after the write, so an entry being written
/// is still in flight and a drain waiting on zero cannot race the write it is waiting for.
fn release_queued_entry() {
    // Constructing a permit to drop it would be clearer, but `Permit` borrows the gate and
    // exists only to be RAII; the writer's release is a plain decrement of the same word.
    QUEUE.release_one();
}

/// THE ONLY WAY INTO THE LOG QUEUE. Returns whether the entry was accepted.
///
/// One choke point on purpose: the depth was first counted inside `queue_log`, and
/// `log/write` and `log/write-batch` send straight down `state.log_tx`, so two of the
/// three producers were invisible and the drain would call the module quiet with entries
/// still queued behind it. Anything that can enqueue must come through here, or the
/// counter is decoration.
pub fn enqueue_log_blocking(
    sender: &mpsc::SyncSender<WriteLogPayload>,
    payload: WriteLogPayload,
) -> Result<(), mpsc::SendError<WriteLogPayload>> {
    // The BLOCKING form, for `log/write`, whose caller is waiting on a result and must
    // not have its entry silently dropped when the queue is full. Same counter, same
    // rule — count only what the channel accepted.
    // ADMISSION AND RESERVATION ARE ONE STEP. Checking a flag and then incrementing a
    // separate counter lets a close land between them: the drain reads zero, declares the
    // module quiet, and this entry then joins a queue whose writer is being joined.
    //
    // Reserved BEFORE the send, too: the writer runs concurrently, so an entry published
    // before its count lands can be popped and decremented first, underflowing the depth
    // and making the drain wait forever.
    // ADMISSION AND RESERVATION ARE ONE STEP. Checking a flag and then incrementing a
    // separate counter lets a close land between them: the drain reads zero, declares the
    // module quiet, and this entry then joins a queue whose writer is being joined.
    let Some(permit) = QUEUE.admit() else {
        return Err(mpsc::SendError(payload));
    };
    if let Err(e) = sender.send(payload) {
        return Err(e); // permit drops here: the entry was never queued, so release it
    }
    // The entry is now the WRITER's to release, not ours — it stays counted until the
    // writer has written it. `forget` transfers that ownership; dropping here would
    // uncount an entry that is still in the channel.
    std::mem::forget(permit);
    Ok(())
}

pub fn enqueue_log(sender: &mpsc::SyncSender<WriteLogPayload>, payload: WriteLogPayload) -> bool {
    // Counted only on a SUCCESSFUL send: a dropped entry was never queued, and counting
    // it leaves a depth that never returns to zero and a drain that can never complete.
    // Same single-step admission+reservation as the blocking form, for the same reason.
    let Some(permit) = QUEUE.admit() else {
        return false;
    };
    if sender.try_send(payload).is_ok() {
        std::mem::forget(permit); // handed to the writer; see the blocking form
        true
    } else {
        false // permit drops: refused by the channel, so it was never queued
    }
}

/// Channel capacity - if full, new messages dropped (NEVER blocks)
const CLOG_CHANNEL_CAPACITY: usize = 4096;

/// Is the in-process file-log sink up? The clog_*/log_* macros consult this
/// BEFORE paying `format!` + category/component derivation — pre-gate, every
/// call site built its whole payload only for `queue_log` to drop it when the
/// LoggerModule wasn't initialized (and on the native server the fallback arm
/// is the ONLY arm, so early-boot call sites paid full formatting for nothing).
/// Same drop semantics, zero formatting cost.
#[inline]
pub fn log_sink_ready() -> bool {
    GLOBAL_LOG_SENDER.get().is_some()
}

/// Queue a log entry for async writing (called by clog_* macros).
/// GUARANTEED NON-BLOCKING: Uses try_send(), drops if channel full.
/// If LoggerModule not yet initialized, message is dropped.
#[inline]
pub fn queue_log(category: &str, level: LogLevel, component: &str, message: &str) {
    if let Some(sender) = GLOBAL_LOG_SENDER.get() {
        let payload = WriteLogPayload {
            category: category.to_string(),
            level,
            component: component.to_string(),
            message: message.to_string(),
            args: None,
        };
        // GUARANTEED NON-BLOCKING: try_send returns immediately
        // If channel full, message dropped - NEVER blocks caller
        let _ = enqueue_log(sender, payload);
    }
    // If GLOBAL_LOG_SENDER not set, silently drop (LoggerModule not initialized yet)
}

// ============================================================================
// Types (matches legacy worker's messages.rs)
// ============================================================================

/// Log levels matching TypeScript LogLevel type.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/logger/LogLevel.ts")]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Debug => write!(f, "debug"),
            LogLevel::Info => write!(f, "info"),
            LogLevel::Warn => write!(f, "warn"),
            LogLevel::Error => write!(f, "error"),
        }
    }
}

/// Payload for log/write requests. Shared between the `log/write` command
/// (params) and the in-process `queue_log`/`clog_*` macro path.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/logger/WriteLogPayload.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct WriteLogPayload {
    pub category: String,
    pub level: LogLevel,
    pub component: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(type = "any", optional)]
    pub args: Option<Value>,
}

// The `log/*` result types (WriteLogResult, WriteLogBatchPayload/Result,
// LoggerPingResult) live with their commands under `commands/log/` — they are
// pure command wire types with no other consumer in the module.

// ============================================================================
// Rate Limiter (from legacy rate_limiter.rs)
// ============================================================================

/// Per-category rate state
struct CategoryRate {
    count: u32,
    dropped: u32,
    window_start: Instant,
    limit: u32,
}

/// Result of checking rate limit
enum RateDecision {
    Allow,
    Drop,
    BurstEnded(u32),
}

/// Rate limiter for log categories
struct RateLimiter {
    categories: HashMap<String, CategoryRate>,
    default_limit: u32,
    window_duration: Duration,
}

impl RateLimiter {
    fn new(default_limit: u32) -> Self {
        Self {
            categories: HashMap::new(),
            default_limit,
            window_duration: Duration::from_secs(1),
        }
    }

    fn check(&mut self, category: &str) -> RateDecision {
        let now = Instant::now();
        let default_limit = self.default_limit;
        let window = self.window_duration;

        let state = self
            .categories
            .entry(category.to_string())
            .or_insert_with(|| CategoryRate {
                count: 0,
                dropped: 0,
                window_start: now,
                limit: default_limit,
            });

        // Check if window has elapsed
        if now.duration_since(state.window_start) >= window {
            let prev_dropped = state.dropped;
            state.count = 1;
            state.dropped = 0;
            state.window_start = now;

            if prev_dropped > 0 {
                return RateDecision::BurstEnded(prev_dropped);
            }
            return RateDecision::Allow;
        }

        if state.limit == 0 {
            state.count += 1;
            return RateDecision::Allow;
        }

        if state.count < state.limit {
            state.count += 1;
            RateDecision::Allow
        } else {
            state.dropped += 1;
            RateDecision::Drop
        }
    }
}

// ============================================================================
// File Manager (from legacy file_manager.rs)
// ============================================================================

type LockedFile = Arc<Mutex<File>>;
type FileCache = Arc<Mutex<HashMap<String, LockedFile>>>;
type HeaderTracker = Arc<Mutex<HashSet<String>>>;

/// Resolve category to proper log path based on concern hierarchy.
///
/// Categories follow a structured naming convention:
/// - `system/{component}` → .continuum/jtag/logs/system/{component}.log
/// - `modules/{module}` → .continuum/jtag/logs/modules/{module}.log
/// - `personas/{uniqueId}/{subsystem}` → .continuum/personas/{uniqueId}/logs/{subsystem}.log
/// - `sentinels/{handle}/{stream}` → .continuum/jtag/logs/system/sentinels/{handle}/{stream}.log
/// - `daemons/{name}` → .continuum/jtag/logs/system/daemons/{name}.log
/// - Anything else → .continuum/jtag/logs/system/{category}.log (legacy fallback)
fn resolve_log_path(category: &str, log_dir: &str, continuum_root: &str) -> PathBuf {
    let parts: Vec<&str> = category.split('/').collect();

    match parts.as_slice() {
        // personas/{uniqueId}/{subsystem} → $HOME/.continuum/personas/{uniqueId}/logs/{subsystem}.log
        ["personas", unique_id, subsystem] => {
            PathBuf::from(continuum_root).join(format!("personas/{unique_id}/logs/{subsystem}.log"))
        }
        // personas/{uniqueId} → $HOME/.continuum/personas/{uniqueId}/logs/general.log
        ["personas", unique_id] => {
            PathBuf::from(continuum_root).join(format!("personas/{unique_id}/logs/general.log"))
        }
        // sentinels/{handle}/{stream} → {log_dir}/sentinels/{handle}/{stream}.log
        ["sentinels", handle, stream] => {
            PathBuf::from(log_dir).join(format!("sentinels/{handle}/{stream}.log"))
        }
        // sentinels/{handle} → {log_dir}/sentinels/{handle}/execution.log
        ["sentinels", handle] => {
            PathBuf::from(log_dir).join(format!("sentinels/{handle}/execution.log"))
        }
        // modules/{module} → {log_dir}/modules/{module}.log
        ["modules", module] => PathBuf::from(log_dir).join(format!("modules/{module}.log")),
        // daemons/{name} → {log_dir}/daemons/{name}.log
        ["daemons", name] => PathBuf::from(log_dir).join(format!("daemons/{name}.log")),
        // system/{component} → {log_dir}/{component}.log
        ["system", component] => PathBuf::from(log_dir).join(format!("{component}.log")),
        // Legacy/fallback: put in system dir with category as filename
        _ => {
            // Replace slashes with underscores for legacy categories
            let safe_name = category.replace('/', "_");
            PathBuf::from(log_dir).join(format!("{safe_name}.log"))
        }
    }
}

/// Max log file size before rotation (10 MB). Prevents unbounded growth during long sessions.
const MAX_LOG_FILE_SIZE: u64 = 10 * 1024 * 1024;

fn ensure_file_handle(
    category: &str,
    log_file_path: &PathBuf,
    file_cache: &FileCache,
    headers_written: &HeaderTracker,
) -> std::io::Result<()> {
    let mut cache = file_cache.lock().unwrap_or_else(|e| e.into_inner());

    // Check if cached file was deleted or exceeded size limit
    if let Some(existing) = cache.get(category) {
        let needs_reopen = {
            let file = existing.lock().unwrap_or_else(|e| e.into_inner());
            match file.metadata() {
                Err(_) => true,                             // File deleted
                Ok(meta) => meta.len() > MAX_LOG_FILE_SIZE, // File too large
            }
        };
        if needs_reopen {
            cache.remove(category);
            headers_written
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(category);
            // Truncate the oversized file
            if log_file_path.exists() {
                let _ = fs::write(log_file_path, b"");
            }
        }
    }

    if !cache.contains_key(category) {
        if let Some(parent) = log_file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file_path)?;
        cache.insert(category.to_string(), Arc::new(Mutex::new(file)));
    }

    Ok(())
}

fn write_log_message(
    payload: &WriteLogPayload,
    log_dir: &str,
    continuum_root: &str,
    file_cache: &FileCache,
    headers_written: &HeaderTracker,
) -> std::io::Result<usize> {
    let log_file_path = resolve_log_path(&payload.category, log_dir, continuum_root);
    let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);

    ensure_file_handle(
        &payload.category,
        &log_file_path,
        file_cache,
        headers_written,
    )?;

    let mut total_bytes = 0;
    let needs_header = !headers_written
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&payload.category);

    if needs_header {
        total_bytes += write_header(
            &payload.component,
            &payload.category,
            &timestamp,
            file_cache,
            headers_written,
        )?;
    }

    let log_entry = format_log_entry(payload, &timestamp);
    total_bytes += write_entry(&payload.category, &log_entry, file_cache)?;

    Ok(total_bytes)
}

fn write_header(
    component: &str,
    category: &str,
    timestamp: &str,
    file_cache: &FileCache,
    headers_written: &HeaderTracker,
) -> std::io::Result<usize> {
    let header = format!(
        "================================================================================\n\
         COMPONENT: {}\n\
         CATEGORY: {}\n\
         SESSION: session-{}\n\
         STARTED: {}\n\
         PID: {}\n\
         ================================================================================\n\
         \n\
         LOG FORMAT:\n\
           [RUST] [timestamp] [LEVEL] Component: message [args]\n\
         \n\
         LOG LEVELS:\n\
           DEBUG - Detailed diagnostic information\n\
           INFO  - General informational messages\n\
           WARN  - Warning messages\n\
           ERROR - Error messages\n\
         \n\
         LOG ENTRIES BEGIN BELOW:\n\
         ================================================================================\n\
         \n",
        component,
        category,
        Utc::now().timestamp_millis(),
        timestamp,
        std::process::id()
    );
    let bytes = header.len();

    let locked_file = {
        let cache = file_cache.lock().unwrap_or_else(|e| e.into_inner());
        cache
            .get(category)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("No file handle for {category}"),
                )
            })?
            .clone()
    };

    {
        let mut file = locked_file.lock().unwrap_or_else(|e| e.into_inner());
        file.write_all(header.as_bytes())?;
    }

    headers_written
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(category.to_string());
    Ok(bytes)
}

fn write_entry(category: &str, log_entry: &str, file_cache: &FileCache) -> std::io::Result<usize> {
    let locked_file = {
        let cache = file_cache.lock().unwrap_or_else(|e| e.into_inner());
        cache
            .get(category)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("No file handle for {category}"),
                )
            })?
            .clone()
    };

    {
        let mut file = locked_file.lock().unwrap_or_else(|e| e.into_inner());
        file.write_all(log_entry.as_bytes())?;
    }

    Ok(log_entry.len())
}

fn format_log_entry(payload: &WriteLogPayload, timestamp: &str) -> String {
    let base = format!(
        "[RUST] [{}] [{}] {}: {}",
        timestamp,
        payload.level.to_string().to_uppercase(),
        payload.component,
        payload.message
    );

    if let Some(args) = &payload.args {
        format!("{base} {args}\n")
    } else {
        format!("{base}\n")
    }
}

/// Failures the writer could not report, counted so a STOP can.
///
/// The writer thread runs with nowhere to return an error to, so a failed write or flush
/// was printed to stderr and dropped. That is defensible while the process runs — a log
/// line is not worth killing a node over — and indefensible at shutdown, where the module
/// then returned `Ok(())` and the receipt said `Clean` over writes that never landed.
///
/// PER-INSTANCE, not a global. As a `static` it could not be tested: driving a real write
/// failure would have left the count non-zero for the whole test binary, so every later
/// `shutdown()` would report a dirty stop. That is the third time in this rail that a
/// process-wide global was the reason a test had to be fake rather than the reason it was
/// hard — see `AdmissionGate` and `ShutdownOperation`.
pub type WriteFailures = Arc<AtomicU64>;

/// Whether a stop may be reported clean, given what this module failed to write.
///
/// Pure, and separate from `shutdown` so it can be tested against a real failure count
/// without constructing a module around env vars. The decision is the load-bearing part:
/// with only a fully completed stop supporting durability, a module that cannot confirm
/// its content reached disk must fail the stop rather than let the CLI's exit code claim
/// the citizens' logs are safe.
/// WRITE AND LATCH: attempt one log write and record the failure if it does not land.
///
/// One operation, shared by the writer thread and by tests, because a test that performs
/// the latch ITSELF proves only that a counter can be incremented — not that production
/// increments it. Deleting the production increments would have left the earlier version
/// of the logger regression green. Astra's discriminator, and she was right.
fn write_and_latch(
    payload: &WriteLogPayload,
    log_dir: &str,
    continuum_root: &str,
    file_cache: &FileCache,
    headers_written: &HeaderTracker,
    failures: &WriteFailures,
) -> bool {
    match write_log_message(payload, log_dir, continuum_root, file_cache, headers_written) {
        Ok(_) => true,
        Err(e) => {
            failures.fetch_add(1, Ordering::Relaxed);
            eprintln!("❌ LoggerModule write error: {e}");
            false
        }
    }
}

fn stop_outcome(failures: u64) -> Result<(), String> {
    if failures > 0 {
        return Err(format!(
            "{failures} log write(s)/flush(es) failed in this process — some log content              did not reach disk"
        ));
    }
    Ok(())
}

fn flush_all(file_cache: &FileCache, failures: &WriteFailures) {
    let handles: Vec<LockedFile> = {
        let cache = file_cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.values().cloned().collect()
    };

    for locked_file in handles {
        let mut file = locked_file.lock().unwrap_or_else(|e| e.into_inner());
        // A flush that failed at shutdown is the whole reason `shutdown` exists for this
        // module. Swallowing it here and returning Ok() above made the receipt claim
        // durability for content still sitting in a buffer that never reached disk.
        if let Err(e) = file.flush() {
            failures.fetch_add(1, Ordering::Relaxed);
            eprintln!("❌ LoggerModule flush error: {e}");
        }
    }
}

// ============================================================================
// LoggerModule — ServiceModule Implementation
// ============================================================================

pub struct LoggerModule {
    log_dir: String,
    /// Shared state the `log/*` commands operate over (queue sender, open-file
    /// cache, lifetime counters). The writer thread holds its own clones captured
    /// at construction; this is the surface the commands read/write.
    state: Arc<LoggerCommandState>,
}

/// State shared by every `log/*` command — the queue sender (writes), the
/// open-file cache (ping's active-category count), and the lifetime counters.
/// Built once in [`LoggerModule::new`] and handed to each command via
/// [`crate::commands::log::command_objects`].
pub struct LoggerCommandState {
    pub log_tx: mpsc::SyncSender<WriteLogPayload>,
    pub file_cache: FileCache,
    pub started_at: Instant,
    pub requests_processed: AtomicU64,
    pub pending_writes: Arc<AtomicU64>,
    /// Writes and flushes this module could not complete. Non-zero means some log content
    /// is gone, and a stop must not be reported as clean.
    pub write_failures: WriteFailures,
}

#[cfg(test)]
impl LoggerCommandState {
    /// Build a standalone state backed by a throwaway bounded channel for
    /// command-level tests. Returns the receiver so the test can assert what was
    /// enqueued (and keeps it alive so `send` succeeds).
    pub(crate) fn new_for_test() -> (Arc<Self>, mpsc::Receiver<WriteLogPayload>) {
        let (log_tx, rx) = mpsc::sync_channel::<WriteLogPayload>(64);
        let state = Arc::new(Self {
            log_tx,
            file_cache: Arc::new(Mutex::new(HashMap::new())),
            started_at: Instant::now(),
            requests_processed: AtomicU64::new(0),
            pending_writes: Arc::new(AtomicU64::new(0)),
            write_failures: Arc::new(AtomicU64::new(0)),
        });
        (state, rx)
    }
}

impl LoggerModule {
    /// Build a module around an existing state, so a test can drive the REAL `shutdown`
    /// over a failure count it produced through the real write path. `new()` reads env
    /// vars and spawns a writer thread; a test needs neither, and needs the state it can
    /// see.
    #[cfg(test)]
    pub(crate) fn with_state(state: Arc<LoggerCommandState>) -> Self {
        Self {
            log_dir: String::new(),
            state,
        }
    }

    pub fn new() -> Self {
        let continuum_root = std::env::var("CONTINUUM_ROOT").unwrap_or_else(|_| {
            let home = dirs::home_dir().expect("Failed to resolve home directory");
            home.join(".continuum").to_string_lossy().to_string()
        });

        let log_dir = std::env::var("JTAG_LOG_DIR").unwrap_or_else(|_| {
            PathBuf::from(&continuum_root)
                .join("jtag")
                .join("logs")
                .join("system")
                .to_string_lossy()
                .to_string()
        });

        let file_cache = Arc::new(Mutex::new(HashMap::new()));
        let headers_written = Arc::new(Mutex::new(HashSet::new()));
        let pending_writes = Arc::new(AtomicU64::new(0));
        let write_failures: WriteFailures = Arc::new(AtomicU64::new(0));

        // Create BOUNDED sync_channel for GUARANTEED non-blocking
        // try_send() returns immediately - if full, message dropped (NEVER blocks)
        let (log_tx, log_rx) = mpsc::sync_channel::<WriteLogPayload>(CLOG_CHANNEL_CAPACITY);

        // Set global sender for clog_* macros (if not already set)
        let _ = GLOBAL_LOG_SENDER.set(log_tx.clone());

        // Spawn dedicated writer thread (same architecture as legacy worker)
        let writer_file_cache = file_cache.clone();
        let writer_headers = headers_written.clone();
        let writer_log_dir = log_dir.clone();
        let writer_continuum_root = continuum_root.clone();
        let writer_pending = pending_writes.clone();
        let writer_failures = write_failures.clone();

        thread::spawn(move || {
            const FLUSH_INTERVAL: Duration = Duration::from_millis(250);
            const MAX_BATCH_BEFORE_FLUSH: usize = 200;

            let mut pending: usize = 0;
            let mut limiter = RateLimiter::new(100);

            let process_payload =
                |payload: &WriteLogPayload, limiter: &mut RateLimiter, pending: &mut usize| {
                    match limiter.check(&payload.category) {
                        RateDecision::Allow => {
                            write_and_latch(
                                payload,
                                &writer_log_dir,
                                &writer_continuum_root,
                                &writer_file_cache,
                                &writer_headers,
                                &writer_failures,
                            );
                            *pending += 1;
                        }
                        RateDecision::Drop => {}
                        RateDecision::BurstEnded(dropped) => {
                            let warning = WriteLogPayload {
                                category: payload.category.clone(),
                                level: LogLevel::Warn,
                                component: "RateLimiter".to_string(),
                                message: format!(
                                    "Rate limit: dropped {} messages from '{}' (>100/sec)",
                                    dropped, payload.category
                                ),
                                args: None,
                            };
                            // Counted like every other write. It is the rate-limit
                            // warning — the line that explains why other lines are
                            // missing — so losing it silently is the one loss that also
                            // erases the evidence of the losses.
                            write_and_latch(
                                &warning,
                                &writer_log_dir,
                                &writer_continuum_root,
                                &writer_file_cache,
                                &writer_headers,
                                &writer_failures,
                            );
                            write_and_latch(
                                payload,
                                &writer_log_dir,
                                &writer_continuum_root,
                                &writer_file_cache,
                                &writer_headers,
                                &writer_failures,
                            );
                            *pending += 2;
                        }
                    }
                };

            loop {
                match log_rx.recv_timeout(FLUSH_INTERVAL) {
                    Ok(payload) => {
                        // Decremented AFTER the write, not on receipt: an entry popped
                        // off the channel and still being written is in flight, and a
                        // drain that stopped waiting at the pop would race the write it
                        // was waiting for.
                        process_payload(&payload, &mut limiter, &mut pending);
                        release_queued_entry();

                        // Drain remaining messages non-blocking
                        while pending < MAX_BATCH_BEFORE_FLUSH {
                            match log_rx.try_recv() {
                                Ok(payload) => {
                                    process_payload(&payload, &mut limiter, &mut pending);
                                    release_queued_entry();
                                }
                                Err(_) => break,
                            }
                        }

                        if pending >= MAX_BATCH_BEFORE_FLUSH {
                            flush_all(&writer_file_cache, &writer_failures);
                            writer_pending.store(0, Ordering::Relaxed);
                            pending = 0;
                        } else {
                            writer_pending.store(pending as u64, Ordering::Relaxed);
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if pending > 0 {
                            flush_all(&writer_file_cache, &writer_failures);
                            writer_pending.store(0, Ordering::Relaxed);
                            pending = 0;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        if pending > 0 {
                            flush_all(&writer_file_cache, &writer_failures);
                        }
                        break;
                    }
                }
            }
        });

        let state = Arc::new(LoggerCommandState {
            log_tx,
            file_cache,
            started_at: Instant::now(),
            requests_processed: AtomicU64::new(0),
            pending_writes,
            write_failures,
        });

        Self { log_dir, state }
    }
}

impl Default for LoggerModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for LoggerModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "logger",
            priority: ModulePriority::Background,
            command_prefixes: &["log/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false, // Writer thread is internal
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        // Ensure log directory exists
        fs::create_dir_all(&self.log_dir).map_err(|e| format!("Failed to create log dir: {e}"))?;
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // Migrated to the typed registry (`commands/log/{write,write_batch,ping}.rs`).
        // The legacy string-match surface is retired; fail loud rather than silently
        // route a stale name (per Joel's never-swallow rule).
        Err(format!(
            "logger command surface is migrated to the typed registry; '{command}' has no legacy handler"
        ))
    }

    fn commands(&self) -> Vec<Arc<dyn DynCommand>> {
        crate::commands::log::command_objects(self.state.clone())
    }

    /// Wait for the log queue to empty, bounded.
    ///
    /// The first real implementation of the drain contract, and it is here because the
    /// loss is concrete: `queue_log` hands entries to a writer THREAD, and `shutdown`
    /// only ever flushed the open files. Anything still in the channel when the core
    /// stopped was never written and never counted — including, on a bad stop, the log
    /// lines explaining why it was stopping.
    ///
    /// Polls rather than signals: the writer is a plain `std::thread` on a blocking
    /// `recv_timeout`, so there is no async completion to await, and its own flush
    /// interval is 250ms. Returning the residual count rather than an error is what lets
    /// the receipt say how many lines were lost instead of only that some were.
    async fn drain(&self) -> Result<u32, String> {
        // CLOSE ADMISSION FIRST. Waiting for the depth to reach zero while producers are
        // still free to enqueue measures a moment, not a drain — the count can be zero
        // and one `clog_*` later be one again, with the writer already joined.
        close_log_admission();
        const POLL: std::time::Duration = std::time::Duration::from_millis(25);
        // Bounded strictly inside the runtime's 2s phase bound, so the deadline that
        // fires is this one — with a count — rather than the outer timeout, which
        // produces no number at all.
        const BUDGET: std::time::Duration = std::time::Duration::from_millis(1_500);
        let deadline = std::time::Instant::now() + BUDGET;
        while std::time::Instant::now() < deadline {
            if queued_log_entries() == 0 && self.state.pending_writes.load(Ordering::Relaxed) == 0 {
                return Ok(0);
            }
            tokio::time::sleep(POLL).await;
        }
        // Both halves count: entries still in the channel, plus writes the writer has
        // made but not flushed. A drain that reported only the channel would call a
        // module drained while its file buffers still held lines.
        let left = queued_log_entries() + self.state.pending_writes.load(Ordering::Relaxed);
        Ok(left.min(u32::MAX as u64) as u32)
    }

    async fn shutdown(&self) -> Result<(), String> {
        // Flush any pending writes
        flush_all(&self.state.file_cache, &self.state.write_failures);
        // AND SAY SO IF ANY OF IT FAILED. Returning Ok() here made the receipt report
        // `Clean` for a module whose writes and flushes had been failing all along — the
        // errors went to stderr, which on a stopping node is nobody. A module that cannot
        // confirm its content reached disk must not let the stop be called durable.
        stop_outcome(self.state.write_failures.load(Ordering::Relaxed))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    //! The module now owns only config + the shared command state + the dep-holding
    //! family. The write/ping behavior contracts are pinned in
    //! `commands/log/{write,write_batch,ping}.rs`; these tests guard the module's
    //! wiring.
    use super::*;

    // what this catches: config exposes the canonical `log/` prefix + module name.
    // If either drifts, the registry routes the command elsewhere.
    #[test]
    fn config_reports_name_and_prefix() {
        let m = LoggerModule::new();
        let cfg = m.config();
        assert_eq!(cfg.name, "logger");
        assert_eq!(cfg.command_prefixes, &["log/"]);
    }

    // what this catches: the legacy string-match surface is retired — any
    // handle_command call fails loud naming the command (never silently swallows
    // or routes a stale name), per Joel's never-swallow rule.
    #[tokio::test]
    async fn legacy_handle_command_fails_loud() {
        let m = LoggerModule::new();
        let err = m
            .handle_command("log/ping", Value::Null)
            .await
            .expect_err("migrated surface must fail loud");
        assert!(err.contains("migrated to the typed registry"));
        assert!(err.contains("log/ping"));
    }

    // what this catches: the module contributes exactly the three dep-holding
    // log verbs (each carrying the shared state) to the typed registry.
    #[test]
    fn contributes_the_three_log_commands() {
        let m = LoggerModule::new();
        let names: Vec<&str> = m.commands().iter().map(|c| c.name()).collect();
        assert_eq!(names, vec!["log/write", "log/write-batch", "log/ping"]);
    }

    #[test]
    fn test_rate_limiter() {
        let mut rl = RateLimiter::new(3);
        assert!(matches!(rl.check("test"), RateDecision::Allow));
        assert!(matches!(rl.check("test"), RateDecision::Allow));
        assert!(matches!(rl.check("test"), RateDecision::Allow));
        assert!(matches!(rl.check("test"), RateDecision::Drop));
    }

    /// The stop must not be called clean when log content did not reach disk.
    mod write_failures_reach_the_receipt {
        use super::*;

        // what this catches: a write failure being swallowed while the stop still reports
        // clean. The writer thread has nowhere to return an error to, so failures went to
        // stderr — which on a stopping node is nobody — and `shutdown()` returned `Ok(())`
        // regardless. With only a fully completed stop supporting durability, that made
        // the CLI's exit code claim the citizens' logs were safe when they were not.
        //
        // Drives the REAL `write_log_message` against a real unwritable destination, a
        // real successful one after it, and the REAL decision `shutdown` delegates to.
        #[tokio::test]
        async fn a_failed_write_then_a_successful_one_still_ends_in_a_non_clean_stop() {
            let tmp = tempfile::tempdir().unwrap();
            let failures: WriteFailures = Arc::new(AtomicU64::new(0));
            let cache: FileCache = Default::default();
            let headers: HeaderTracker = Default::default();
            let payload = WriteLogPayload {
                category: "test".into(),
                level: LogLevel::Warn,
                component: "regression".into(),
                message: "this write cannot land".into(),
                args: None,
            };

            // A FILE where the log directory must be: the write genuinely cannot land.
            let blocked = tmp.path().join("blocked");
            std::fs::write(&blocked, b"in the way").unwrap();
            // THE SHARED OPERATION — the same `write_and_latch` the writer thread calls.
            // The earlier version called `write_log_message` and then did the
            // `fetch_add` ITSELF, which proved a counter can be incremented rather than
            // that production increments it: deleting the production latch would have
            // left it green.
            let landed = write_and_latch(
                &payload,
                &blocked.to_string_lossy(),
                &tmp.path().to_string_lossy(),
                &cache,
                &headers,
                &failures,
            );
            assert!(
                !landed,
                "a log directory that is a FILE must fail the write, not silently succeed"
            );
            assert_eq!(
                failures.load(Ordering::Relaxed),
                1,
                "the shared write+latch must record the failure — no manual increment here"
            );

            // A LATER write SUCCEEDS. This is the case that used to read as clean: the
            // last write worked and the buffers flushed, and nothing carried the earlier
            // loss forward.
            let ok_dir = tmp.path().join("writable");
            std::fs::create_dir_all(&ok_dir).unwrap();
            let good = write_and_latch(
                &payload,
                &ok_dir.to_string_lossy(),
                &tmp.path().to_string_lossy(),
                &cache,
                &headers,
                &failures,
            );
            assert!(good, "a writable destination must still work");
            assert_eq!(
                failures.load(Ordering::Relaxed),
                1,
                "a SUCCESSFUL write must not add a failure — the count is the earlier loss"
            );

            // THE POINT: the REAL `shutdown()` on a REAL module holding that count is not
            // clean. Reading `stop_outcome` directly, as the earlier version did, would
            // have stayed green if `shutdown` stopped delegating to it.
            let (log_tx, _rx) = mpsc::sync_channel::<WriteLogPayload>(8);
            let state = Arc::new(LoggerCommandState {
                log_tx,
                file_cache: cache.clone(),
                started_at: Instant::now(),
                requests_processed: AtomicU64::new(0),
                pending_writes: Arc::new(AtomicU64::new(0)),
                write_failures: failures.clone(),
            });
            let module = LoggerModule::with_state(state);
            let err = module
                .shutdown()
                .await
                .expect_err("a module that lost log content must not stop clean");
            assert!(
                err.contains('1') && err.contains("did not reach disk"),
                "the failure must be counted and named, got: {err}"
            );
        }

        // what this catches: the opposite error — a module with no failures refusing to
        // stop cleanly, which would make every ordinary shutdown report data loss and
        // train the operator to ignore the signal entirely.
        // what this catches: a module with no failures refusing to stop cleanly, which
        // would make every ordinary shutdown report data loss and train the operator to
        // ignore the signal. Also through the REAL `shutdown`, so it fails if the
        // delegation inverts.
        #[tokio::test]
        async fn a_module_that_wrote_everything_stops_clean() {
            let (log_tx, _rx) = mpsc::sync_channel::<WriteLogPayload>(8);
            let state = Arc::new(LoggerCommandState {
                log_tx,
                file_cache: Default::default(),
                started_at: Instant::now(),
                requests_processed: AtomicU64::new(0),
                pending_writes: Arc::new(AtomicU64::new(0)),
                write_failures: Arc::new(AtomicU64::new(0)),
            });
            assert!(
                LoggerModule::with_state(state).shutdown().await.is_ok(),
                "no failures means a clean stop"
            );
        }

        // what this catches: the counter being process-global again. Two modules must not
        // see each other's failures — which is exactly why this was moved off a `static`,
        // and why a test could not drive a real failure before.
        #[test]
        fn one_modules_failures_do_not_condemn_anothers_stop() {
            let mine: WriteFailures = Arc::new(AtomicU64::new(3));
            let theirs: WriteFailures = Arc::new(AtomicU64::new(0));
            assert!(stop_outcome(mine.load(Ordering::Relaxed)).is_err());
            assert!(stop_outcome(theirs.load(Ordering::Relaxed)).is_ok());
        }
    }
}
