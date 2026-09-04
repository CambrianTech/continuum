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
        let _ = sender.try_send(payload);
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

fn flush_all(file_cache: &FileCache) {
    let handles: Vec<LockedFile> = {
        let cache = file_cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.values().cloned().collect()
    };

    for locked_file in handles {
        let mut file = locked_file.lock().unwrap_or_else(|e| e.into_inner());
        let _ = file.flush();
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
        });
        (state, rx)
    }
}

impl LoggerModule {
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

        thread::spawn(move || {
            const FLUSH_INTERVAL: Duration = Duration::from_millis(250);
            const MAX_BATCH_BEFORE_FLUSH: usize = 200;

            let mut pending: usize = 0;
            let mut limiter = RateLimiter::new(100);

            let process_payload =
                |payload: &WriteLogPayload, limiter: &mut RateLimiter, pending: &mut usize| {
                    match limiter.check(&payload.category) {
                        RateDecision::Allow => {
                            if let Err(e) = write_log_message(
                                payload,
                                &writer_log_dir,
                                &writer_continuum_root,
                                &writer_file_cache,
                                &writer_headers,
                            ) {
                                eprintln!("❌ LoggerModule write error: {e}");
                            }
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
                            let _ = write_log_message(
                                &warning,
                                &writer_log_dir,
                                &writer_continuum_root,
                                &writer_file_cache,
                                &writer_headers,
                            );
                            if let Err(e) = write_log_message(
                                payload,
                                &writer_log_dir,
                                &writer_continuum_root,
                                &writer_file_cache,
                                &writer_headers,
                            ) {
                                eprintln!("❌ LoggerModule write error: {e}");
                            }
                            *pending += 2;
                        }
                    }
                };

            loop {
                match log_rx.recv_timeout(FLUSH_INTERVAL) {
                    Ok(payload) => {
                        process_payload(&payload, &mut limiter, &mut pending);

                        // Drain remaining messages non-blocking
                        while pending < MAX_BATCH_BEFORE_FLUSH {
                            match log_rx.try_recv() {
                                Ok(payload) => {
                                    process_payload(&payload, &mut limiter, &mut pending);
                                }
                                Err(_) => break,
                            }
                        }

                        if pending >= MAX_BATCH_BEFORE_FLUSH {
                            flush_all(&writer_file_cache);
                            writer_pending.store(0, Ordering::Relaxed);
                            pending = 0;
                        } else {
                            writer_pending.store(pending as u64, Ordering::Relaxed);
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if pending > 0 {
                            flush_all(&writer_file_cache);
                            writer_pending.store(0, Ordering::Relaxed);
                            pending = 0;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        if pending > 0 {
                            flush_all(&writer_file_cache);
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

    async fn shutdown(&self) -> Result<(), String> {
        // Flush any pending writes
        flush_all(&self.state.file_cache);
        Ok(())
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
}
