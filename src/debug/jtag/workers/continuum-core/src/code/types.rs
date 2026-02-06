//! Shared types for the code module.
//!
//! **Single source of truth** — TypeScript types are generated via `ts-rs`.
//! These are the wire types for IPC communication between TS and Rust.
//!
//! Re-generate TypeScript bindings:
//!   cargo test --package continuum-core export_bindings
//!
//! Output: shared/generated/code/*.ts

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// Every file operation creates a ChangeNode in the DAG.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/code/ChangeNode.ts")]
pub struct ChangeNode {
    #[ts(type = "string")]
    pub id: Uuid,
    /// Parent node IDs. Empty for root operations. Multiple for merges.
    #[ts(type = "Array<string>")]
    pub parent_ids: Vec<Uuid>,
    /// Who performed this operation (persona UUID string).
    pub author_id: String,
    /// When the operation occurred (unix millis).
    #[ts(type = "number")]
    pub timestamp: u64,
    /// The file affected (relative to workspace root).
    pub file_path: String,
    /// The operation type.
    pub operation: FileOperation,
    /// Forward diff (apply to go forward in time).
    pub forward_diff: FileDiff,
    /// Reverse diff (apply to go backward in time — undo).
    pub reverse_diff: FileDiff,
    /// Optional description from the AI about what this change does.
    #[ts(optional)]
    pub description: Option<String>,
    /// Workspace ID this change belongs to.
    pub workspace_id: String,
}

/// File operation types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../shared/generated/code/FileOperation.ts")]
pub enum FileOperation {
    Create,
    Write,
    Edit,
    Delete,
    Rename {
        from: String,
        to: String,
    },
    /// An undo operation that reversed a previous change.
    Undo {
        #[ts(type = "string")]
        reverted_id: Uuid,
    },
}

/// A file diff consisting of hunks.
#[derive(Debug, Clone, Serialize, Deserialize, Default, TS)]
#[ts(export, export_to = "../../../shared/generated/code/FileDiff.ts")]
pub struct FileDiff {
    /// Unified diff text (compatible with standard tooling).
    pub unified: String,
    /// Structured hunks for programmatic application.
    pub hunks: Vec<DiffHunk>,
}

/// A single hunk in a unified diff.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/code/DiffHunk.ts")]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    /// The hunk content (with +/- prefixes on each line).
    pub content: String,
}

/// How to edit a file (four modes).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export, export_to = "../../../shared/generated/code/EditMode.ts")]
pub enum EditMode {
    /// Replace content between line numbers (1-indexed, inclusive).
    LineRange {
        start_line: u32,
        end_line: u32,
        new_content: String,
    },
    /// Find text and replace it.
    SearchReplace {
        search: String,
        replace: String,
        #[serde(default)]
        all: bool,
    },
    /// Insert content at a specific line (pushes existing lines down).
    InsertAt {
        line: u32,
        content: String,
    },
    /// Append content to end of file.
    Append {
        content: String,
    },
}

/// Result of a file write/edit/delete operation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/code/WriteResult.ts")]
pub struct WriteResult {
    pub success: bool,
    /// UUID of the ChangeNode created.
    #[ts(optional)]
    pub change_id: Option<String>,
    pub file_path: String,
    #[ts(type = "number")]
    pub bytes_written: u64,
    #[ts(optional)]
    pub error: Option<String>,
}

/// Result of a file read operation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/code/ReadResult.ts")]
pub struct ReadResult {
    pub success: bool,
    #[ts(optional)]
    pub content: Option<String>,
    pub file_path: String,
    pub total_lines: u32,
    pub lines_returned: u32,
    pub start_line: u32,
    pub end_line: u32,
    #[ts(type = "number")]
    pub size_bytes: u64,
    #[ts(optional)]
    pub error: Option<String>,
}

/// A single search match.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/code/SearchMatch.ts")]
pub struct SearchMatch {
    pub file_path: String,
    pub line_number: u32,
    pub line_content: String,
    pub match_start: u32,
    pub match_end: u32,
}

/// Result of a code search operation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/code/SearchResult.ts")]
pub struct SearchResult {
    pub success: bool,
    pub matches: Vec<SearchMatch>,
    pub total_matches: u32,
    pub files_searched: u32,
    #[ts(optional)]
    pub error: Option<String>,
}

/// A node in a directory tree.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/code/TreeNode.ts")]
pub struct TreeNode {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    #[ts(optional, type = "number")]
    pub size_bytes: Option<u64>,
    pub children: Vec<TreeNode>,
}

/// Result of a tree operation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/code/TreeResult.ts")]
pub struct TreeResult {
    pub success: bool,
    #[ts(optional)]
    pub root: Option<TreeNode>,
    pub total_files: u32,
    pub total_directories: u32,
    #[ts(optional)]
    pub error: Option<String>,
}

/// Result of an undo operation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/code/UndoResult.ts")]
pub struct UndoResult {
    pub success: bool,
    pub changes_undone: Vec<WriteResult>,
    #[ts(optional)]
    pub error: Option<String>,
}

/// History query result.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/code/HistoryResult.ts")]
pub struct HistoryResult {
    pub success: bool,
    pub nodes: Vec<ChangeNode>,
    pub total_count: u32,
    #[ts(optional)]
    pub error: Option<String>,
}

/// Git status information.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/code/GitStatusInfo.ts")]
pub struct GitStatusInfo {
    pub success: bool,
    #[ts(optional)]
    pub branch: Option<String>,
    pub modified: Vec<String>,
    pub added: Vec<String>,
    pub deleted: Vec<String>,
    pub untracked: Vec<String>,
    #[ts(optional)]
    pub error: Option<String>,
}

/// Allowed file extensions for write operations.
pub const ALLOWED_EXTENSIONS: &[&str] = &[
    "ts", "tsx", "js", "jsx", "json", "md", "css", "html",
    "rs", "toml", "yaml", "yml", "txt", "sh", "py",
];

/// Maximum file size for write operations (1MB).
pub const MAX_WRITE_SIZE: u64 = 1_048_576;
