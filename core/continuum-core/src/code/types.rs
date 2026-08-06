//! Shared types for the code module.
//!
//! **Single source of truth** — TypeScript types are generated via `ts-rs`.
//! These are the wire types for IPC communication between TS and Rust.
//!
//! Re-generate TypeScript bindings:
//!   cargo test --package continuum-core export_bindings
//!
//! Output: protocol/typescript/code/*.ts

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// Every file operation creates a ChangeNode in the DAG.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/ChangeNode.ts")]
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
#[ts(export, export_to = "../../../protocol/typescript/code/FileOperation.ts")]
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
#[ts(export, export_to = "../../../protocol/typescript/code/FileDiff.ts")]
pub struct FileDiff {
    /// Unified diff text (compatible with standard tooling).
    pub unified: String,
    /// Structured hunks for programmatic application.
    pub hunks: Vec<DiffHunk>,
}

/// A single hunk in a unified diff.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/DiffHunk.ts")]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    /// The hunk content (with +/- prefixes on each line).
    pub content: String,
}

/// How to edit a file (four modes).
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/code/EditMode.ts")]
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
    InsertAt { line: u32, content: String },
    /// Append content to end of file.
    Append { content: String },
}

/// Result of a file write/edit/delete operation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/WriteResult.ts")]
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
    /// The NUMBERED lines around where a line-addressed edit actually landed, read back
    /// from the file AFTER writing. `None` for whole-file writes (nothing to locate).
    ///
    /// This exists because "success: true, bytes_written: N" is not feedback — it confirms
    /// the write happened, never that it landed where the caller meant. Glass-boxed on the
    /// M5, 2026-08-04: a persona ran `code/shell cat file` (output has NO line numbers),
    /// then `insert_at line 28` — a number she had never seen — and dropped a guard clause
    /// into the middle of a function's parameter list. Next act she replaced lines 62-65 of
    /// a second file after reading from line 119, a region she had never looked at. Both
    /// returned `success: true`. She spent her remaining acts flailing through discovery
    /// tools because nothing in her working memory said the edits were wrong.
    ///
    /// A human editor shows you the result; the act→observe circuit only closes if the
    /// receipt carries what a screen would ([[errors-as-data]], [[px-personas-experience-tools-as-good-ux]]).
    /// So the receipt now carries the neighborhood, numbered — the same surface `code/read`
    /// gives — and a misplaced edit becomes a visible fact on the next turn instead of a
    /// silent success.
    #[ts(optional)]
    pub applied_context: Option<String>,
}

/// Result of a file read operation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/ReadResult.ts")]
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
#[ts(export, export_to = "../../../protocol/typescript/code/SearchMatch.ts")]
pub struct SearchMatch {
    pub file_path: String,
    pub line_number: u32,
    pub line_content: String,
    pub match_start: u32,
    pub match_end: u32,
}

/// Result of a code search operation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/SearchResult.ts")]
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
#[ts(export, export_to = "../../../protocol/typescript/code/TreeNode.ts")]
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
#[ts(export, export_to = "../../../protocol/typescript/code/TreeResult.ts")]
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
#[ts(export, export_to = "../../../protocol/typescript/code/UndoResult.ts")]
pub struct UndoResult {
    pub success: bool,
    pub changes_undone: Vec<WriteResult>,
    #[ts(optional)]
    pub error: Option<String>,
}

/// History query result.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/HistoryResult.ts")]
pub struct HistoryResult {
    pub success: bool,
    pub nodes: Vec<ChangeNode>,
    pub total_count: u32,
    #[ts(optional)]
    pub error: Option<String>,
}

/// Git status information.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GitStatusInfo.ts")]
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

/// Kind of filesystem entry reported by `code/exists` and `code/list`.
/// Coalesced into one enum so a single value covers presence + type,
/// avoiding two round trips for the common "does this exist and is
/// it a file or a directory?" question.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/FsEntryKind.ts")]
#[serde(rename_all = "snake_case")]
pub enum FsEntryKind {
    /// Regular file (`is_file`).
    File,
    /// Directory (`is_dir`).
    Directory,
    /// Symbolic link (`is_symlink`). `code/list` follows symlinks by
    /// default when reporting size; `code/exists` reports the link
    /// itself without following.
    Symlink,
    /// Anything else (block device, fifo, etc.) — preserved so the
    /// substrate doesn't lie about presence even for exotic entries.
    Other,
}

/// Result of `code/exists`. Presence + kind in one value so a caller
/// can decide whether to overwrite vs. create vs. bail in a single
/// roundtrip.
///
/// `exists: false` always means no entry at the path; `kind` is
/// `None` in that case. When `exists: true`, `kind` is always set
/// (never `None`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/ExistsResult.ts")]
pub struct ExistsResult {
    pub success: bool,
    pub exists: bool,
    pub file_path: String,
    #[ts(optional)]
    pub kind: Option<FsEntryKind>,
    /// File size in bytes when `kind == File`; `None` for directories,
    /// symlinks, or missing entries.
    #[ts(optional, type = "number")]
    pub size_bytes: Option<u64>,
    #[ts(optional)]
    pub error: Option<String>,
}

/// One entry in a `code/list` response — a flat directory listing.
/// Compact: just enough info for a persona to decide whether to
/// recurse, edit, or skip. For richer recursive output, callers use
/// `code/tree` instead.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/DirEntry.ts")]
pub struct DirEntry {
    /// Bare entry name (no path separators).
    pub name: String,
    /// Path relative to the workspace root.
    pub path: String,
    pub kind: FsEntryKind,
    /// File size in bytes when `kind == File`; `None` otherwise.
    #[ts(optional, type = "number")]
    pub size_bytes: Option<u64>,
}

/// Result of `code/list`. Flat — no recursion. Hidden entries
/// (`.git`, `.continuum`, dotfiles) are excluded by default; callers
/// pass `include_hidden: true` to see them.
///
/// Sorted: directories first (alphabetical), then files
/// (alphabetical). Predictable ordering matters for persona
/// reproducibility — a generator that picks "first available name"
/// gets the same answer every run.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/ListResult.ts")]
pub struct ListResult {
    pub success: bool,
    pub directory_path: String,
    pub entries: Vec<DirEntry>,
    pub total_count: u32,
    #[ts(optional)]
    pub error: Option<String>,
}

/// Result of `code/glob`. Matches are workspace-relative paths,
/// sorted alphabetically for determinism.
///
/// The glob runs scoped to the workspace root unless `root` is set
/// on the input — `PathSecurity::validate_read` enforces both
/// boundaries.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GlobResult.ts")]
pub struct GlobResult {
    pub success: bool,
    pub pattern: String,
    /// Workspace-relative paths of matching entries, sorted.
    pub matches: Vec<String>,
    pub total_matches: u32,
    /// True when the result was truncated to `GLOB_MAX_MATCHES`. The
    /// substrate caps glob output so a runaway recursive pattern
    /// (double-star slash star) doesn't OOM the caller — partial
    /// results are still useful.
    ///
    /// Pattern is intentionally spelled in words rather than glyphs:
    /// the literal sequence round-trips through ts-rs into a JSDoc
    /// block on the TS side, where the comment-close glyph
    /// prematurely terminates the doc comment and breaks the
    /// TypeScript build. See task #62 ("ts-rs binding drift CI
    /// guard") for the proper substrate-level fix.
    pub truncated: bool,
    #[ts(optional)]
    pub error: Option<String>,
}

/// Maximum number of paths a single `code/glob` response returns.
/// Beyond this, the result is truncated with `truncated: true`. Set
/// generously enough to cover typical "find all rust files in a
/// module tree" use cases without enabling unbounded memory on a
/// recursive everything pattern.
pub const GLOB_MAX_MATCHES: usize = 5_000;

/// Allowed file extensions for write operations.

/// Maximum file size for write operations (1MB).
pub const MAX_WRITE_SIZE: u64 = 1_048_576;

// NOTE: `code/*` commands are deliberately NOT registered via `CommandSpec` yet.
// There is no Rust `code` ServiceModule — `code/read` et al. are served today by
// the legacy TS mixin (`bindings/modules/code.ts`), which carries a `persona_id`
// param and a nested `{success, result}` IPC envelope. Declaring a CommandSpec
// here would describe a wire that doesn't exist on the new transport. When the
// code surface is ported to a real Rust ServiceModule on continuum-client, add
// the CommandSpec then (with the params the handler actually parses) — not before.
