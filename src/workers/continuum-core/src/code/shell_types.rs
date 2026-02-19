//! Shell wire types — IPC protocol for shell session management.
//!
//! TypeScript types generated via ts-rs.
//! Re-generate: `cargo test --package continuum-core export_bindings`

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Status of a shell command execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../shared/generated/code/ShellExecutionStatus.ts")]
pub enum ShellExecutionStatus {
    Running,
    Completed,
    Failed,
    TimedOut,
    Killed,
}

/// Response from `code/shell-execute`.
///
/// Always returns immediately with the execution handle.
/// If `wait: true` was specified, also includes the completed result.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/code/ShellExecuteResponse.ts")]
pub struct ShellExecuteResponse {
    pub execution_id: String,
    pub status: ShellExecutionStatus,
    /// Full stdout (only present when `wait: true` and execution completed).
    #[ts(optional)]
    pub stdout: Option<String>,
    /// Full stderr (only present when `wait: true` and execution completed).
    #[ts(optional)]
    pub stderr: Option<String>,
    /// Exit code (only present when execution completed).
    #[ts(optional)]
    pub exit_code: Option<i32>,
}

/// Response from `code/shell-poll`.
///
/// Returns new output since the last poll (cursor-based).
/// Call repeatedly until `finished` is true.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/code/ShellPollResponse.ts")]
pub struct ShellPollResponse {
    pub execution_id: String,
    pub status: ShellExecutionStatus,
    /// New stdout lines since last poll.
    pub new_stdout: Vec<String>,
    /// New stderr lines since last poll.
    pub new_stderr: Vec<String>,
    /// Exit code (present when finished).
    #[ts(optional)]
    pub exit_code: Option<i32>,
    /// True when the execution is no longer running.
    pub finished: bool,
}

/// Response from `code/shell-status` — session metadata.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/code/ShellSessionInfo.ts")]
pub struct ShellSessionInfo {
    pub session_id: String,
    pub persona_id: String,
    pub cwd: String,
    pub workspace_root: String,
    pub active_executions: u32,
    pub total_executions: u32,
}

/// A history entry for a completed execution.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/code/ShellHistoryEntry.ts")]
pub struct ShellHistoryEntry {
    pub execution_id: String,
    pub command: String,
    #[ts(optional)]
    pub exit_code: Option<i32>,
    #[ts(type = "number")]
    pub started_at: u64,
    #[ts(optional, type = "number")]
    pub finished_at: Option<u64>,
}

// ============================================================================
// Sentinel Types — Output classification and filtering
// ============================================================================

/// Classification level for a line of shell output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/code/OutputClassification.ts")]
pub enum OutputClassification {
    Error,
    Warning,
    Info,
    Success,
    Verbose,
}

/// What to do with a line that matches a sentinel rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/code/SentinelAction.ts")]
pub enum SentinelAction {
    /// Include the line in watch results.
    Emit,
    /// Filter the line out silently.
    Suppress,
}

/// A sentinel filter rule: regex pattern → classification + action.
///
/// Wire type for IPC. Patterns are compiled to `regex::Regex` on the Rust side
/// when `set_sentinel()` is called.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/code/SentinelRule.ts")]
pub struct SentinelRule {
    /// Regex pattern to match against each output line.
    pub pattern: String,
    /// Classification to assign when this rule matches.
    pub classification: OutputClassification,
    /// Whether to include or suppress the matched line.
    pub action: SentinelAction,
}

/// A single line of classified shell output.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/code/ClassifiedLine.ts")]
pub struct ClassifiedLine {
    /// The raw text content of the line.
    pub text: String,
    /// Classification assigned by sentinel rules.
    pub classification: OutputClassification,
    /// Line number within the stream (0-indexed from execution start).
    #[ts(type = "number")]
    pub line_number: u64,
    /// Which stream this line came from: "stdout" or "stderr".
    pub stream: String,
    /// Unix timestamp in milliseconds when the line was classified.
    #[ts(type = "number")]
    pub timestamp: u64,
}

/// Response from `code/shell-watch`.
///
/// Returns classified output lines since the last watch call.
/// Blocks until output is available (no timeout, no polling).
/// Call in a loop until `finished` is true.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/code/ShellWatchResponse.ts")]
pub struct ShellWatchResponse {
    pub execution_id: String,
    /// Classified output lines (filtered through sentinel rules).
    pub lines: Vec<ClassifiedLine>,
    /// True when the execution is no longer running.
    pub finished: bool,
    /// Exit code (present when finished).
    #[ts(optional)]
    pub exit_code: Option<i32>,
}
