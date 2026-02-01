//! Code module — file operations, change tracking, and code intelligence.
//!
//! Provides the Rust foundation for the coding agent system:
//! - `types` — Shared wire types for IPC (ChangeNode, FileDiff, EditMode, etc.)
//! - `diff_engine` — Unified diff computation using the `similar` crate
//! - `change_graph` — Per-workspace DAG of file operations with undo/redo
//! - `path_security` — Workspace-scoped path validation and traversal guard
//! - `file_engine` — Per-persona file operations (read/write/edit/delete)
//! - `search` — Regex + glob code search with .gitignore awareness
//! - `tree` — Directory tree generation
//! - `git_bridge` — Git status, diff, and branch operations

pub mod types;
pub mod diff_engine;
pub mod change_graph;
pub mod path_security;
pub mod file_engine;
pub mod search;
pub mod tree;
pub mod git_bridge;

// Re-export key types for convenience
pub use types::*;
pub use change_graph::ChangeGraph;
pub use diff_engine::{compute_diff, compute_bidirectional_diff};
pub use path_security::PathSecurity;
pub use file_engine::FileEngine;
