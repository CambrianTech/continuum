//! Grid audit trail — logs all remote command executions.
//!
//! Every command received from or sent to a remote node is logged.
//! Stored in .continuum/grid/audit.jsonl (append-only, one JSON object per line).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

/// Direction of a remote command.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditDirection {
    /// We received this command from a remote node.
    Inbound,
    /// We sent this command to a remote node.
    Outbound,
}

/// Outcome of a remote command execution.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditOutcome {
    Success,
    Denied,
    Error,
    Timeout,
}

/// A single audit trail entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Timestamp in milliseconds since epoch.
    pub timestamp: u64,
    /// Direction: inbound (we received) or outbound (we sent).
    pub direction: AuditDirection,
    /// Remote node identifier.
    pub remote_node: String,
    /// Command name (e.g., "genome/train").
    pub command: String,
    /// Correlation ID for matching request/response.
    pub correlation_id: String,
    /// Outcome of the command.
    pub outcome: AuditOutcome,
    /// Duration in milliseconds (0 if denied before execution).
    pub duration_ms: u64,
}

/// Thread-safe audit log writer.
pub struct AuditLog {
    path: PathBuf,
    writer: Mutex<Option<tokio::fs::File>>,
}

impl AuditLog {
    pub fn new(grid_dir: &Path) -> Self {
        Self {
            path: grid_dir.join("audit.jsonl"),
            writer: Mutex::new(None),
        }
    }

    /// Ensure the log file is open for appending.
    async fn ensure_open(&self) -> Result<(), String> {
        let mut guard = self.writer.lock().await;
        if guard.is_none() {
            if let Some(parent) = self.path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| format!("Failed to create audit dir: {e}"))?;
            }
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
                .await
                .map_err(|e| format!("Failed to open audit log: {e}"))?;
            *guard = Some(file);
        }
        Ok(())
    }

    /// Append an audit entry to the log.
    pub async fn log(&self, entry: &AuditEntry) -> Result<(), String> {
        self.ensure_open().await?;
        let mut line =
            serde_json::to_string(entry).map_err(|e| format!("Audit serialization failed: {e}"))?;
        line.push('\n');

        let mut guard = self.writer.lock().await;
        if let Some(file) = guard.as_mut() {
            file.write_all(line.as_bytes())
                .await
                .map_err(|e| format!("Audit write failed: {e}"))?;
            file.flush()
                .await
                .map_err(|e| format!("Audit flush failed: {e}"))?;
        }
        Ok(())
    }

    /// Read the last N entries from the audit log.
    pub async fn recent(&self, limit: usize) -> Result<Vec<AuditEntry>, String> {
        let contents = tokio::fs::read_to_string(&self.path)
            .await
            .unwrap_or_default();

        let entries: Vec<AuditEntry> = contents
            .lines()
            .rev()
            .take(limit)
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();

        Ok(entries)
    }
}
