/// File Manager Module - Log File Caching and Auto-Recovery
///
/// This module handles all file operations for the logger:
/// - File handle caching (avoid repeated open/close)
/// - Header tracking (write once per category)
/// - Auto-recovery (recreate if deleted)
/// - Thread-safe shared access
///
/// KEY DESIGN: Files stay open across connections for performance.
/// Cache is shared via Arc<Mutex<>> for concurrent access.
use crate::messages::{LogLevel, WriteLogPayload};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// File handle with its own lock for concurrent access.
/// Each file can be written to independently without blocking others.
pub type LockedFile = Arc<Mutex<File>>;

/// File handle cache - keeps log files open across requests.
/// Key: category (e.g., "daemons/LoggerDaemonServer")
/// Value: LOCKED file handle (per-file locking for concurrency)
///
/// PERFORMANCE: With 160 log files, per-file locking eliminates contention.
/// Threads only block if writing to the SAME file.
pub type FileCache = Arc<Mutex<HashMap<String, LockedFile>>>;

/// Header tracking - ensures we only write header once per category.
/// Contains categories that have had headers written.
pub type HeaderTracker = Arc<Mutex<HashSet<String>>>;

/// Result of writing a log message (bytes written).
pub type WriteResult = std::io::Result<usize>;

// ============================================================================
// Public API
// ============================================================================

/// Create a new file cache.
pub fn create_file_cache() -> FileCache {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Create a new header tracker.
pub fn create_header_tracker() -> HeaderTracker {
    Arc::new(Mutex::new(HashSet::new()))
}

/// Write a log message to file, handling all caching and headers.
///
/// This is the main entry point for file operations.
/// Handles:
/// - Log file path resolution (daemon vs persona logs)
/// - Directory creation
/// - File handle caching
/// - Auto-recovery if file deleted
/// - Header writing (once per category)
/// - Actual log entry writing
pub fn write_log_message(
    payload: &WriteLogPayload,
    log_dir: &str,
    file_cache: &FileCache,
    headers_written: &HeaderTracker,
) -> WriteResult {
    let log_file_path = resolve_log_path(&payload.category, log_dir);
    let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    // Get or create file handle (with auto-recovery)
    ensure_file_handle(
        &payload.category,
        &log_file_path,
        file_cache,
        headers_written,
    )?;

    // Write header if needed
    let mut total_bytes = 0;
    let needs_header = {
        let headers = headers_written.lock().unwrap();
        !headers.contains(&payload.category)
    };

    if needs_header {
        total_bytes += write_header(
            &payload.component,
            &payload.category,
            &timestamp,
            file_cache,
            headers_written,
        )?;
    }

    // Write log entry
    let log_entry = format_log_entry(payload, &timestamp);
    total_bytes += write_entry(&payload.category, &log_entry, file_cache)?;

    Ok(total_bytes)
}

/// Get the count of active categories (open file handles).
pub fn active_category_count(file_cache: &FileCache) -> usize {
    file_cache.lock().unwrap().len()
}

// ============================================================================
// Internal Implementation
// ============================================================================

/// Resolve log file path based on category.
///
/// Rules:
/// - Persona logs: .continuum/personas/{id}/logs/{name}.log
/// - Daemon/system logs: {log_dir}/{category}.log
fn resolve_log_path(category: &str, log_dir: &str) -> PathBuf {
    if category.starts_with("personas/") {
        // Persona logs: .continuum/personas/{id}/logs/genome.log
        PathBuf::from(format!(".continuum/{}.log", category))
    } else {
        // Daemon/system logs: {log_dir}/daemons/LoggerDaemonServer.log
        PathBuf::from(log_dir).join(format!("{}.log", category))
    }
}

/// Ensure file handle exists in cache, creating/reopening if needed.
///
/// Auto-recovery: If cached file was deleted, remove from cache and reopen.
///
/// PERFORMANCE: Holds global cache lock ONLY during lookup/insertion.
/// Actual file I/O happens with per-file locks (no global contention).
fn ensure_file_handle(
    category: &str,
    log_file_path: &Path,
    file_cache: &FileCache,
    headers_written: &HeaderTracker,
) -> std::io::Result<()> {
    let mut cache = file_cache.lock().unwrap();

    // Check if cached file was deleted/moved
    if let Some(existing_locked_file) = cache.get(category) {
        // Try to get metadata (requires locking the file temporarily)
        let file_deleted = {
            let file = existing_locked_file.lock().unwrap();
            file.metadata().is_err()
        };

        if file_deleted {
            // File deleted - remove from cache and clear header flag
            cache.remove(category);
            let mut headers = headers_written.lock().unwrap();
            headers.remove(category);
        }
    }

    // Create file handle if not in cache
    if !cache.contains_key(category) {
        // Ensure directory exists
        if let Some(parent) = log_file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Open file in append mode
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(true)
            .open(log_file_path)?;

        // Wrap in Arc<Mutex<>> for per-file locking
        let locked_file = Arc::new(Mutex::new(file));
        cache.insert(category.to_string(), locked_file);
    }

    Ok(())
}

/// Write header to log file (once per category).
///
/// PERFORMANCE: Global cache lock held ONLY during lookup.
/// File write uses per-file lock (no contention).
fn write_header(
    component: &str,
    category: &str,
    timestamp: &str,
    file_cache: &FileCache,
    headers_written: &HeaderTracker,
) -> WriteResult {
    let header = generate_header(component, category, timestamp);
    let bytes = header.len();

    // Get locked file handle from cache (brief global lock)
    let locked_file = {
        let cache = file_cache.lock().unwrap();
        cache.get(category).unwrap().clone() // Clone Arc (cheap)
    }; // Global lock released here

    // Write header using per-file lock (no global contention)
    {
        let mut file = locked_file.lock().unwrap();
        file.write_all(header.as_bytes())?;
        file.flush()?;
    } // Per-file lock released here

    // Mark header as written
    let mut headers = headers_written.lock().unwrap();
    headers.insert(category.to_string());

    Ok(bytes)
}

/// Write log entry to file.
///
/// PERFORMANCE: Global cache lock held ONLY during lookup.
/// File write uses per-file lock (no contention).
fn write_entry(category: &str, log_entry: &str, file_cache: &FileCache) -> WriteResult {
    // Get locked file handle from cache (brief global lock)
    let locked_file = {
        let cache = file_cache.lock().unwrap();
        cache.get(category).unwrap().clone() // Clone Arc (cheap)
    }; // Global lock released here

    // Write entry using per-file lock (no global contention)
    {
        let mut file = locked_file.lock().unwrap();
        file.write_all(log_entry.as_bytes())?;
        file.flush()?;
    } // Per-file lock released here

    Ok(log_entry.len())
}

/// Format log entry with timestamp, level, component, message.
fn format_log_entry(payload: &WriteLogPayload, timestamp: &str) -> String {
    let base = format!(
        "[RUST] [{}] [{}] {}: {}",
        timestamp,
        payload.level.to_string().to_uppercase(),
        payload.component,
        payload.message
    );

    if let Some(args) = &payload.args {
        format!("{} {}\n", base, args)
    } else {
        format!("{}\n", base)
    }
}

/// Generate log file header.
fn generate_header(component: &str, category: &str, timestamp: &str) -> String {
    format!(
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
        chrono::Utc::now().timestamp_millis(),
        timestamp,
        std::process::id()
    )
}

// ============================================================================
// Display Trait for LogLevel
// ============================================================================

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
