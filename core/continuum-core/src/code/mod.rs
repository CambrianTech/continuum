//! Code module — file operations, change tracking, shell sessions, and code intelligence.
//!
//! Provides the Rust foundation for the coding agent system:
//! - `types` — Shared wire types for IPC (ChangeNode, FileDiff, EditMode, etc.)
//! - `shell_types` — Wire types for shell session IPC
//! - `shell_session` — Persistent shell sessions per workspace (handle + poll)
//! - `diff_engine` — Unified diff computation using the `similar` crate
//! - `change_graph` — Per-workspace DAG of file operations with undo/redo
//! - `path_security` — Workspace-scoped path validation and traversal guard
//! - `file_engine` — Per-persona file operations (read/write/edit/delete)
//! - `search` — Regex + glob code search with .gitignore awareness
//! - `tree` — Directory tree generation
//! - `git_bridge` — Git status, diff, and branch operations

pub mod change_graph;
pub mod diff_engine;
pub mod file_engine;
pub mod git_bridge;
pub mod path_security;
pub mod search;
pub mod shell_session;
pub mod shell_types;
pub mod tree;
pub mod types;

// Re-export key types for convenience
pub use change_graph::ChangeGraph;
pub use diff_engine::{compute_bidirectional_diff, compute_diff};
pub use file_engine::FileEngine;
pub use path_security::PathSecurity;
pub use shell_session::{watch_execution, ShellSession};
pub use types::*;
