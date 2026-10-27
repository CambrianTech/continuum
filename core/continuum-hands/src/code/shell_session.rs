//! ShellSession — Shell execution state per workspace.
//!
//! Provides a handle-based shell execution model:
//!   1. Create session (bound to workspace directory)
//!   2. Execute command → get execution handle immediately
//!   3. Poll execution handle → get new stdout/stderr chunks
//!   4. Or: execute with wait=true → block until complete
//!   5. Kill execution if needed
//!   6. Destroy session on cleanup
//!
//! Supports BOTH quick commands (wait=true → immediate result) and
//! long-running commands (poll repeatedly → streaming output).
//!
//! Each command runs in its own process for isolation. The session retains
//! execution handles and the cwd/env configured through its methods. A child's
//! `cd`, variables, exports and shell options do not change later executions.
//! Files survive the child process. Exit status is the shell's status under its
//! normal command/pipeline rules, not an assertion that every child succeeded.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use regex::Regex;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as TokioCommand;
use tokio::sync::Notify;
use uuid::Uuid;

use super::shell_types::{
    ClassifiedLine, OutputClassification, SentinelAction, SentinelRule, ShellExecuteResponse,
    ShellExecutionStatus, ShellHistoryEntry, ShellPollResponse, ShellSessionInfo,
    ShellWatchResponse,
};
use crate::log_info;

mod max_output_lines_per_execution;
pub use max_output_lines_per_execution::*;
mod shell_session;
pub use shell_session::*;
mod run_shell_command;
pub use run_shell_command::*;

#[cfg(test)]
mod tests;
