//! File Engine — per-persona file operations with change tracking.
//!
//! Every write/edit/delete operation creates a ChangeNode in the change graph,
//! enabling undo at any point. Reads are side-effect-free.
//!
//! Thread safety: Each persona gets their own FileEngine instance.
//! The underlying ChangeGraph and PathSecurity handle concurrency.

use std::fs;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use super::change_graph::ChangeGraph;
use super::diff_engine::compute_bidirectional_diff;
use super::path_security::{PathSecurity, PathSecurityError};
use super::types::*;

/// Per-persona file engine with workspace scoping and change tracking.
pub struct FileEngine {
    persona_id: String,
    security: PathSecurity,
    graph: ChangeGraph,
}

/// Errors from file engine operations.
#[derive(Debug)]
pub enum FileEngineError {
    Security(PathSecurityError),
    Io(std::io::Error),
    NotFound(String),
    EditFailed(String),
}

impl std::fmt::Display for FileEngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Security(e) => write!(f, "Security: {}", e),
            Self::Io(e) => write!(f, "I/O: {}", e),
            Self::NotFound(path) => write!(f, "File not found: {}", path),
            Self::EditFailed(msg) => write!(f, "Edit failed: {}", msg),
        }
    }
}

impl std::error::Error for FileEngineError {}

impl From<PathSecurityError> for FileEngineError {
    fn from(e: PathSecurityError) -> Self {
        match e {
            // In-sandbox ENOENT is not a security event and must never render with
            // the "Security:" prefix — that framing reads as FORBIDDEN and stops a
            // persona from correcting the path (the bitflags-exam 58-dead-reads bug).
            PathSecurityError::NotFound { .. } => Self::NotFound(e.to_string()),
            other => Self::Security(other),
        }
    }
}

impl From<std::io::Error> for FileEngineError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl FileEngine {
    /// Create a new FileEngine for a persona.
    pub fn new(persona_id: &str, security: PathSecurity) -> Self {
        let workspace_id = format!("workspace-{}", persona_id);
        Self {
            persona_id: persona_id.to_string(),
            security,
            graph: ChangeGraph::new(&workspace_id),
        }
    }

    /// Read a file, optionally a range of lines (1-indexed, inclusive).
    pub fn read(
        &self,
        relative_path: &str,
        start_line: Option<u32>,
        end_line: Option<u32>,
    ) -> Result<ReadResult, FileEngineError> {
        let abs_path = self.security.validate_read(relative_path)?;

        if !abs_path.exists() {
            return Err(FileEngineError::NotFound(relative_path.to_string()));
        }

        let content = fs::read_to_string(&abs_path)?;
        let total_lines = content.lines().count() as u32;
        let size_bytes = content.len() as u64;

        let start = start_line.unwrap_or(1).max(1);
        let end = end_line.unwrap_or(total_lines).min(total_lines);

        let selected: String = content
            .lines()
            .enumerate()
            .filter(|(i, _)| {
                let line_num = *i as u32 + 1;
                line_num >= start && line_num <= end
            })
            .map(|(_, line)| line)
            .collect::<Vec<_>>()
            .join("\n");

        let lines_returned = if end >= start { end - start + 1 } else { 0 };

        Ok(ReadResult {
            success: true,
            content: Some(if selected.is_empty() && total_lines > 0 {
                // If the file has content but selection is empty, return empty
                String::new()
            } else {
                selected
            }),
            file_path: relative_path.to_string(),
            total_lines,
            lines_returned,
            start_line: start,
            end_line: end,
            size_bytes,
            error: None,
        })
    }

    /// Write (create or overwrite) a file. Records a ChangeNode.
    pub fn write(
        &self,
        relative_path: &str,
        content: &str,
        description: Option<&str>,
    ) -> Result<WriteResult, FileEngineError> {
        let abs_path = self.security.validate_write(relative_path)?;
        self.security
            .validate_size(relative_path, content.len() as u64)?;

        // Read old content (empty string for new files)
        let old_content = if abs_path.exists() {
            fs::read_to_string(&abs_path).unwrap_or_default()
        } else {
            String::new()
        };

        let operation = if abs_path.exists() {
            FileOperation::Write
        } else {
            FileOperation::Create
        };

        // Compute diffs
        let (forward_diff, reverse_diff) =
            compute_bidirectional_diff(&old_content, content, relative_path);

        // Create parent directories if needed
        if let Some(parent) = abs_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        // Write the file
        fs::write(&abs_path, content)?;

        // Record in change graph
        let node = ChangeNode {
            id: Uuid::new_v4(),
            parent_ids: self.latest_parent(relative_path),
            author_id: self.persona_id.clone(),
            timestamp: now_millis(),
            file_path: relative_path.to_string(),
            operation,
            forward_diff,
            reverse_diff,
            description: description.map(String::from),
            workspace_id: self.graph.workspace_id().to_string(),
        };

        let change_id = node.id.to_string();
        self.graph.record(node);

        Ok(WriteResult {
            success: true,
            change_id: Some(change_id),
            file_path: relative_path.to_string(),
            bytes_written: content.len() as u64,
            error: None,
            // Whole-file write — the caller supplied the entire content, so there is no
            // "where did it land" question to answer.
            applied_context: None,
        })
    }

    /// Edit a file using an EditMode. Records a ChangeNode.
    pub fn edit(
        &self,
        relative_path: &str,
        edit_mode: &EditMode,
        description: Option<&str>,
    ) -> Result<WriteResult, FileEngineError> {
        let abs_path = self.security.validate_write(relative_path)?;

        if !abs_path.exists() {
            return Err(FileEngineError::NotFound(relative_path.to_string()));
        }

        let old_content = fs::read_to_string(&abs_path)?;
        let new_content = apply_edit(&old_content, edit_mode)?;

        self.security
            .validate_size(relative_path, new_content.len() as u64)?;

        // Compute diffs
        let (forward_diff, reverse_diff) =
            compute_bidirectional_diff(&old_content, &new_content, relative_path);

        // Write the modified file
        fs::write(&abs_path, &new_content)?;

        // Record in change graph
        let node = ChangeNode {
            id: Uuid::new_v4(),
            parent_ids: self.latest_parent(relative_path),
            author_id: self.persona_id.clone(),
            timestamp: now_millis(),
            file_path: relative_path.to_string(),
            operation: FileOperation::Edit,
            forward_diff,
            reverse_diff,
            description: description.map(String::from),
            workspace_id: self.graph.workspace_id().to_string(),
        };

        let change_id = node.id.to_string();
        let bytes_written = new_content.len() as u64;
        self.graph.record(node);

        Ok(WriteResult {
            success: true,
            change_id: Some(change_id),
            file_path: relative_path.to_string(),
            bytes_written,
            error: None,
            // Read back from the content just written, not from what was requested — the
            // whole point is to report where the edit ACTUALLY landed.
            applied_context: edit_anchor_line(edit_mode)
                .map(|anchor| numbered_neighborhood(&new_content, anchor, APPLIED_CONTEXT_RADIUS)),
        })
    }

    /// Delete a file. Records a ChangeNode with the full content as reverse diff.
    pub fn delete(
        &self,
        relative_path: &str,
        description: Option<&str>,
    ) -> Result<WriteResult, FileEngineError> {
        let abs_path = self.security.validate_write(relative_path)?;

        if !abs_path.exists() {
            return Err(FileEngineError::NotFound(relative_path.to_string()));
        }

        let old_content = fs::read_to_string(&abs_path)?;

        // Compute diffs (new content is empty for delete)
        let (forward_diff, reverse_diff) =
            compute_bidirectional_diff(&old_content, "", relative_path);

        // Delete the file
        fs::remove_file(&abs_path)?;

        // Record in change graph
        let node = ChangeNode {
            id: Uuid::new_v4(),
            parent_ids: self.latest_parent(relative_path),
            author_id: self.persona_id.clone(),
            timestamp: now_millis(),
            file_path: relative_path.to_string(),
            operation: FileOperation::Delete,
            forward_diff,
            reverse_diff,
            description: description.map(String::from),
            workspace_id: self.graph.workspace_id().to_string(),
        };

        let change_id = node.id.to_string();
        self.graph.record(node);

        Ok(WriteResult {
            success: true,
            change_id: Some(change_id),
            file_path: relative_path.to_string(),
            bytes_written: 0,
            error: None,
            // The file is gone; there is no neighborhood left to show.
            applied_context: None,
        })
    }

    /// Preview what an edit would produce (unified diff) without applying it.
    pub fn preview_diff(
        &self,
        relative_path: &str,
        edit_mode: &EditMode,
    ) -> Result<FileDiff, FileEngineError> {
        let abs_path = self.security.validate_read(relative_path)?;

        if !abs_path.exists() {
            return Err(FileEngineError::NotFound(relative_path.to_string()));
        }

        let old_content = fs::read_to_string(&abs_path)?;
        let new_content = apply_edit(&old_content, edit_mode)?;

        let (forward_diff, _) =
            compute_bidirectional_diff(&old_content, &new_content, relative_path);

        Ok(forward_diff)
    }

    /// Undo a specific change by applying its reverse diff.
    pub fn undo(&self, change_id: &Uuid) -> Result<WriteResult, FileEngineError> {
        let (reverse_diff, file_path) =
            self.graph.reverse_diff_for(change_id).ok_or_else(|| {
                FileEngineError::EditFailed(format!("Change {} not found", change_id))
            })?;

        // Read current file content
        let abs_path = self.security.validate_write(&file_path)?;
        let current_content = if abs_path.exists() {
            fs::read_to_string(&abs_path)?
        } else {
            String::new()
        };

        // The reverse diff's unified text tells us what to apply.
        // For a proper undo, we use the stored old content from the original node.
        let original_node = self.graph.get(change_id).ok_or_else(|| {
            FileEngineError::EditFailed(format!("Change {} not found", change_id))
        })?;

        // Reconstruct: the original node's reverse_diff goes old→new when applied backward.
        // We apply the reverse_diff to the current content. Since we stored the complete
        // forward and reverse diffs, we can reconstruct by computing what the content
        // should be by using the reverse operation's forward diff.
        //
        // For simple cases (create→undo = delete, write→undo = restore old):
        // The undo node created by ChangeGraph has the correct forward_diff.
        let undo_node = self
            .graph
            .record_undo(*change_id, &self.persona_id)
            .ok_or_else(|| {
                FileEngineError::EditFailed(format!("Change {} not found for undo", change_id))
            })?;

        // For the undo, we need to apply the reverse diff to the file.
        // The simplest correct approach: re-read the original diff to determine
        // what the file should look like after undo.
        //
        // Since the reverse diff might not apply cleanly if other changes happened,
        // we do a best-effort: if the change was the latest for this file, apply the
        // reverse content directly; otherwise, warn about conflicts.
        let latest = self.graph.latest_for_file(&file_path);
        let is_latest = latest
            .as_ref()
            .map(|n| n.id == undo_node.id)
            .unwrap_or(false);

        // Apply the reverse diff content — use the unified diff text
        // For now, use a simple heuristic: if we can identify the old content,
        // reconstruct it from the diff hunks.
        let _restored_content = if !reverse_diff.unified.is_empty() {
            // The reverse diff exists, attempt to apply
            apply_reverse_simple(&current_content, &reverse_diff)
                .unwrap_or_else(|| current_content.clone())
        } else {
            current_content.clone()
        };

        // Write the restored content
        if original_node.operation == FileOperation::Create {
            // Undoing a create = delete the file
            if abs_path.exists() {
                fs::remove_file(&abs_path)?;
            }
        } else if matches!(original_node.operation, FileOperation::Delete) {
            // Undoing a delete = recreate the file with reverse diff content
            // The reverse_diff for a delete contains the original content
            let content = extract_added_content(&reverse_diff);
            if let Some(parent) = abs_path.parent() {
                if !parent.exists() {
                    fs::create_dir_all(parent)?;
                }
            }
            fs::write(&abs_path, content)?;
        } else {
            // Undoing a write/edit = apply reverse diff
            let restored = apply_reverse_simple(&current_content, &reverse_diff)
                .unwrap_or_else(|| current_content.clone());
            fs::write(&abs_path, &restored)?;
        }

        Ok(WriteResult {
            success: true,
            change_id: Some(undo_node.id.to_string()),
            file_path,
            bytes_written: 0,
            // An undo restores a prior whole-file state rather than editing at a line.
            applied_context: None,
            error: if !is_latest {
                Some(
                    "Warning: undone change was not the latest; result may have conflicts"
                        .to_string(),
                )
            } else {
                None
            },
        })
    }

    /// Undo the last N non-undo operations.
    pub fn undo_last(&self, count: usize) -> Result<UndoResult, FileEngineError> {
        let ids = self.graph.last_n_undoable(count);
        let mut changes_undone = Vec::new();

        for id in ids {
            match self.undo(&id) {
                Ok(result) => changes_undone.push(result),
                Err(e) => {
                    return Ok(UndoResult {
                        success: false,
                        changes_undone,
                        error: Some(format!("Failed to undo {}: {}", id, e)),
                    });
                }
            }
        }

        Ok(UndoResult {
            success: true,
            changes_undone,
            error: None,
        })
    }

    /// Get change history for a specific file.
    pub fn file_history(&self, file_path: &str, limit: usize) -> HistoryResult {
        let nodes = self.graph.file_history(file_path, limit);
        let total_count = nodes.len() as u32;
        HistoryResult {
            success: true,
            nodes,
            total_count,
            error: None,
        }
    }

    /// Get all change history for the workspace.
    pub fn workspace_history(&self, limit: usize) -> HistoryResult {
        let nodes = self.graph.workspace_history(limit);
        let total_count = nodes.len() as u32;
        HistoryResult {
            success: true,
            nodes,
            total_count,
            error: None,
        }
    }

    /// Get the underlying PathSecurity (for search/tree operations that need it).
    pub fn security(&self) -> &PathSecurity {
        &self.security
    }

    /// Get the workspace root path.
    pub fn workspace_root(&self) -> PathBuf {
        self.security.workspace_root().to_path_buf()
    }

    /// Get all searchable roots: workspace root + read-only roots.
    /// Used by code/search and code/tree to search the full project, not just the worktree.
    pub fn searchable_roots(&self) -> Vec<PathBuf> {
        let mut roots = vec![self.security.workspace_root().to_path_buf()];
        roots.extend(self.security.read_roots().iter().cloned());
        roots
    }

    /// Resolve a workspace-relative path for INTROSPECTION queries
    /// (`exists`, `list_dir`, `glob_match`) where the path is allowed
    /// to NOT exist yet — `exists()` returning false isn't an error.
    ///
    /// `validate_read` rejects non-existent paths (TraversalBlocked)
    /// because it canonicalizes, which fails on missing entries.
    /// That's correct for read/write/edit which require the file —
    /// but wrong for introspection where the whole point is to
    /// answer "does this exist?". Hence this separate validator:
    /// string-level traversal check + join, no existence requirement.
    /// The ONE traversal-guarded join primitive: join `relative` under `root`,
    /// rejecting an absolute path or any `..` segment (the string-level escape
    /// vectors once existence isn't required). Every workspace-path resolver that
    /// does NOT require the target to already exist ([`validate_introspect_path`],
    /// [`resolve_dir`]) goes through here, so the sandbox boundary lives in ONE
    /// place instead of being hand-rolled per call site (a manual `root.join(rel)`
    /// in a command handler silently skips this check — the bypass this consolidates
    /// away). Symlink-escape on EXISTING paths is caught by PathSecurity's
    /// canonicalize check ([`validate_read`]); this is the floor for maybe-missing
    /// paths.
    fn secure_join(&self, root: &Path, relative: &str) -> Result<PathBuf, FileEngineError> {
        let blocked = || {
            FileEngineError::Security(PathSecurityError::TraversalBlocked {
                path: relative.to_string(),
                workspace: root.display().to_string(),
            })
        };
        if relative.starts_with('/') || relative.starts_with('\\') {
            return Err(blocked());
        }
        if relative.split(['/', '\\']).any(|seg| seg == "..") {
            return Err(blocked());
        }
        Ok(root.join(relative))
    }

    /// Resolve a workspace-relative path for INTROSPECTION queries (`exists`,
    /// `list_dir`, `glob_match`) where the path is allowed to NOT exist yet.
    /// Thin wrapper over [`secure_join`] against the workspace root.
    fn validate_introspect_path(&self, relative: &str) -> Result<PathBuf, FileEngineError> {
        self.secure_join(self.security.workspace_root(), relative)
    }

    /// Resolve a persona-supplied path to an existing DIRECTORY within the sandbox —
    /// the ONE resolver every directory-oriented command (`code/tree`, `code/list`,
    /// the `code/search`/`code/glob` root) shares. Path-idiom forgiveness and honest
    /// errors thus live in ONE place instead of drifting per handler (they used to:
    /// only `code/tree` tolerated the prefix; `code/list` and `code/search` hand-rolled
    /// `searchable_roots().join(rel)`, which ALSO skipped the sandbox check).
    ///
    /// Forgives the idioms a persona reaches for from what it sees rendered: a leading
    /// `/`, and a redundant leading `workspace/` segment (the root already IS the
    /// workspace, so that prefix doubles and misses — glass-boxed 2026-07-14). Tries
    /// each searchable root (workspace + read-only roots). HONEST, actionable errors:
    /// a FILE says "use code/read", a miss says "the workspace root itself IS
    /// explorable" — so a mind re-orients instead of concluding it is stuck.
    pub fn resolve_dir(&self, relative: &str) -> Result<PathBuf, FileEngineError> {
        let rel = relative.trim().trim_start_matches('/');
        if rel.is_empty() || rel == "." {
            return Ok(self.security.workspace_root().to_path_buf());
        }
        let mut candidates = vec![rel];
        if let Some(stripped) = rel.strip_prefix("workspace/") {
            candidates.push(stripped);
        }
        let mut file_hit: Option<PathBuf> = None;
        for root in self.searchable_roots() {
            for cand in &candidates {
                // `..`/absolute are rejected here; a rejected candidate is skipped, not
                // fatal, so a later candidate/root can still resolve. A genuinely
                // malicious `..` resolves to no directory and surfaces the miss error.
                if let Ok(abs) = self.secure_join(&root, cand) {
                    if abs.is_dir() {
                        return Ok(abs);
                    }
                    if abs.is_file() {
                        file_hit.get_or_insert(abs);
                    }
                }
            }
        }
        if let Some(f) = file_hit {
            return Err(FileEngineError::NotFound(format!(
                "{} is a FILE, not a directory — use code/read to read it, or run this on \
                 its parent directory.",
                f.display()
            )));
        }
        Err(FileEngineError::NotFound(format!(
            "path not found: {relative} — nothing exists at that path. This is about the \
             PATH, not the workspace: the workspace root itself IS explorable — call this \
             command with NO path (or \".\") for the whole tree, then use a path you see \
             in that output. Paths are relative to the workspace root; do not prefix them \
             with 'workspace/'."
        )))
    }

    /// Check whether a path exists, and if so what kind of entry it is.
    ///
    /// Closes the "is this path safe to write to / scaffold into?"
    /// question in one call. Per
    /// [PERSONA-AS-DEVELOPER-GAP.md](../../../../../../../docs/planning/PERSONA-AS-DEVELOPER-GAP.md),
    /// this is the top-priority filesystem-introspection seam: a
    /// persona running `generate/module` needs to probe before
    /// scaffolding to avoid clobbering.
    ///
    /// Uses `validate_introspect_path` so non-existent paths report
    /// `exists: false` rather than failing with a security error.
    /// Symlinks report as `Symlink` without following — callers that
    /// want follow-the-link semantics can `code/read` and observe the
    /// `NotFound` error if the target is broken.
    pub fn exists(&self, relative_path: &str) -> Result<ExistsResult, FileEngineError> {
        let abs_path = self.validate_introspect_path(relative_path)?;

        // symlink_metadata so we don't follow links transparently.
        let meta = fs::symlink_metadata(&abs_path);
        match meta {
            Ok(m) => {
                let kind = if m.is_symlink() {
                    FsEntryKind::Symlink
                } else if m.is_file() {
                    FsEntryKind::File
                } else if m.is_dir() {
                    FsEntryKind::Directory
                } else {
                    FsEntryKind::Other
                };
                let size_bytes = if matches!(kind, FsEntryKind::File) {
                    Some(m.len())
                } else {
                    None
                };
                Ok(ExistsResult {
                    success: true,
                    exists: true,
                    file_path: relative_path.to_string(),
                    kind: Some(kind),
                    size_bytes,
                    error: None,
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ExistsResult {
                success: true,
                exists: false,
                file_path: relative_path.to_string(),
                kind: None,
                size_bytes: None,
                error: None,
            }),
            Err(e) => Err(FileEngineError::Io(e)),
        }
    }

    /// Flat directory listing (no recursion). Hidden entries (names
    /// starting with `.`) excluded unless `include_hidden` is true.
    ///
    /// Sorted: directories first, then files, both alphabetical.
    /// Predictable order matters for persona reproducibility (a
    /// generator that picks "first available name" must get the
    /// same answer every run).
    ///
    /// For recursive output, callers use `code/tree` instead — this
    /// is intentionally O(N) in directory size, not O(N) in subtree
    /// size, so cheap-by-design.
    pub fn list_dir(
        &self,
        relative_path: &str,
        include_hidden: bool,
    ) -> Result<ListResult, FileEngineError> {
        // ONE resolver (resolve_dir): idiom-forgiveness + the not-found / is-a-FILE
        // honest errors live there, shared with code/tree — not re-checked here.
        let abs_path = self.resolve_dir(relative_path)?;

        let workspace_root = self.security.workspace_root();
        let mut entries: Vec<DirEntry> = Vec::new();

        for raw in fs::read_dir(&abs_path)? {
            let raw = match raw {
                Ok(e) => e,
                Err(_) => continue, // single bad entry shouldn't kill the listing
            };
            let name = raw.file_name().to_string_lossy().to_string();
            if !include_hidden && name.starts_with('.') {
                continue;
            }
            // Stat each entry so we can report kind + size. Errors on
            // individual entries surface as `Other` rather than
            // failing the whole listing — partial info beats none.
            let entry_meta = fs::symlink_metadata(raw.path()).ok();
            let kind = match entry_meta.as_ref() {
                Some(m) if m.is_symlink() => FsEntryKind::Symlink,
                Some(m) if m.is_file() => FsEntryKind::File,
                Some(m) if m.is_dir() => FsEntryKind::Directory,
                _ => FsEntryKind::Other,
            };
            let size_bytes = match (entry_meta.as_ref(), kind) {
                (Some(m), FsEntryKind::File) => Some(m.len()),
                _ => None,
            };
            let path = raw
                .path()
                .strip_prefix(workspace_root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| raw.path().to_string_lossy().to_string());
            entries.push(DirEntry {
                name,
                path,
                kind,
                size_bytes,
            });
        }

        // Directories first, then files; alphabetical within each.
        // Symlinks + Other sort as directories (uncommon enough that
        // their ordering doesn't justify a third bucket).
        entries.sort_by(|a, b| {
            let a_is_file = matches!(a.kind, FsEntryKind::File);
            let b_is_file = matches!(b.kind, FsEntryKind::File);
            a_is_file.cmp(&b_is_file).then(a.name.cmp(&b.name))
        });

        let total_count = entries.len() as u32;
        Ok(ListResult {
            success: true,
            directory_path: relative_path.to_string(),
            entries,
            total_count,
            error: None,
        })
    }

    /// Glob expansion scoped to the workspace (or a `root`
    /// subdirectory of it). Uses the `ignore` crate's overrides for
    /// `.gitignore`-respecting walks, same as `code/search`.
    ///
    /// Patterns are workspace-relative globs like `**/*.rs` or
    /// `core/**/Cargo.toml`. Output is workspace-relative
    /// paths, sorted alphabetically. Capped at `GLOB_MAX_MATCHES`
    /// (5000) so a runaway pattern doesn't OOM the caller —
    /// `truncated: true` flags the cap.
    pub fn glob_match(
        &self,
        pattern: &str,
        root: Option<&str>,
    ) -> Result<GlobResult, FileEngineError> {
        // Root may not exist; use introspect validator. For the actual
        // walk, the directory MUST exist — error if not.
        let scan_root = match root {
            Some(r) => {
                let p = self.validate_introspect_path(r)?;
                if !p.is_dir() {
                    return Err(FileEngineError::NotFound(format!(
                        "code/glob: root is not a directory: {r}"
                    )));
                }
                p
            }
            None => self.security.workspace_root().to_path_buf(),
        };

        // Build the override as a whitelist match for the pattern.
        // OverrideBuilder treats non-`!` patterns as whitelist; we
        // explicitly check `is_whitelist()` per entry so only matched
        // files are emitted.
        let mut overrides = ignore::overrides::OverrideBuilder::new(&scan_root);
        overrides
            .add(pattern)
            .map_err(|e| FileEngineError::EditFailed(format!("code/glob: bad pattern: {e}")))?;
        let overrides = overrides
            .build()
            .map_err(|e| FileEngineError::EditFailed(format!("code/glob: overrides build: {e}")))?;

        // standard_filters=true ⇒ respects .gitignore, .ignore, AND
        // hides hidden files by default. Persona-as-developer
        // contract: glob does NOT see dotfiles unless the pattern
        // explicitly starts with `.` (matches Unix shell intuition).
        let walker = ignore::WalkBuilder::new(&scan_root)
            .standard_filters(true)
            .hidden(true)
            .build();

        let workspace_root = self.security.workspace_root();
        let mut matches: Vec<String> = Vec::new();
        let mut truncated = false;

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = entry.path();

            // Skip the scan root itself (the walker yields it).
            if path == scan_root {
                continue;
            }

            // FILES only — directories are not glob matches per the
            // contract. (A persona that wants to enumerate directories
            // uses `code/list`.) `file_type` returns Some when the
            // walker stat'd it; treat None as "skip" (rare).
            let is_file = entry
                .file_type()
                .map(|ft| ft.is_file())
                .unwrap_or(false);
            if !is_file {
                continue;
            }

            // Explicit whitelist check — only emit when the pattern
            // matched this specific path. `Override::matched(path,
            // is_dir)` returns Match::None / Ignore / Whitelist; we
            // want Whitelist only.
            let m = overrides.matched(path, false);
            if !m.is_whitelist() {
                continue;
            }

            let rel = path
                .strip_prefix(workspace_root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| path.to_string_lossy().to_string());

            if matches.len() >= GLOB_MAX_MATCHES {
                truncated = true;
                break;
            }
            matches.push(rel);
        }

        matches.sort();
        let total_matches = matches.len() as u32;

        Ok(GlobResult {
            success: true,
            pattern: pattern.to_string(),
            matches,
            total_matches,
            truncated,
            error: None,
        })
    }

    /// Get the latest parent ID for a file (for DAG edges).
    fn latest_parent(&self, file_path: &str) -> Vec<Uuid> {
        self.graph
            .latest_for_file(file_path)
            .map(|n| vec![n.id])
            .unwrap_or_default()
    }
}

/// How many lines on each side of an edit the receipt shows. Enough to see whether the
/// insert landed inside a signature, a parameter list, or a docstring — the misplacements
/// actually observed — without turning every edit receipt into a file dump that crowds a
/// small working-memory window.
const APPLIED_CONTEXT_RADIUS: u32 = 6;

/// The line a line-addressed edit is anchored at, or `None` for modes that rewrite the
/// whole file (where "where did it land" has no answer worth rendering).
fn edit_anchor_line(edit_mode: &EditMode) -> Option<u32> {
    match edit_mode {
        EditMode::LineRange { start_line, .. } => Some(*start_line),
        EditMode::InsertAt { line, .. } => Some(*line),
        _ => None,
    }
}

/// Numbered lines around `anchor` in `content` — the same shape `code/read` returns, so a
/// persona reads one surface for "what is in this file", never two. 1-indexed, clamped to
/// the file, and marked with `>` on the anchor line so the landing site is unmissable.
fn numbered_neighborhood(content: &str, anchor: u32, radius: u32) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len() as u32;
    if total == 0 {
        return "(file is now empty)".to_string();
    }
    let anchor = anchor.clamp(1, total);
    let first = anchor.saturating_sub(radius).max(1);
    let last = (anchor + radius).min(total);
    let mut out = String::new();
    for n in first..=last {
        let marker = if n == anchor { '>' } else { ' ' };
        out.push_str(&format!("{marker}{n:>5} | {}\n", lines[(n - 1) as usize]));
    }
    out
}

/// Apply an EditMode to file content, producing the new content.
fn apply_edit(content: &str, edit_mode: &EditMode) -> Result<String, FileEngineError> {
    match edit_mode {
        EditMode::LineRange {
            start_line,
            end_line,
            new_content,
        } => {
            let lines: Vec<&str> = content.lines().collect();
            let total = lines.len() as u32;

            if *start_line == 0 || *start_line > total + 1 {
                return Err(FileEngineError::EditFailed(format!(
                    "start_line {} out of range (1-{})",
                    start_line,
                    total + 1
                )));
            }
            // end_line past EOF is the universal "to end of file" idiom (sed, ed,
            // every editor) — and 65535 (u16 max) is the sentinel a tool-trained
            // model reflexively reaches for to mean "replace through the end".
            // CLAMP to the last line instead of rejecting: the reflexive whole-file
            // / to-EOF edit must LAND, not loop forever on "end_line out of range".
            // That reject was the live stall — personas re-emitted the identical
            // out-of-range edit turn after turn, never reached compile/run, and
            // confabulated success in prose. [[tool-ergonomics-meet-the-idiom]]
            let end_line = (*end_line).min(total);
            // end_line == start_line - 1 is a valid EMPTY range (pure insert before
            // start — e.g. start=1,end=0 clamped on an empty file). Only a range
            // that is genuinely inverted below that is an error.
            if end_line + 1 < *start_line {
                return Err(FileEngineError::EditFailed(format!(
                    "end_line {} is below start_line {} (inverted range)",
                    end_line, start_line
                )));
            }

            let start_idx = (*start_line - 1) as usize;
            let end_idx = end_line as usize;

            let mut result = String::new();

            // Lines before the range
            for line in &lines[..start_idx] {
                result.push_str(line);
                result.push('\n');
            }

            // Insert new content
            if !new_content.is_empty() {
                result.push_str(new_content);
                if !new_content.ends_with('\n') {
                    result.push('\n');
                }
            }

            // Lines after the range
            for line in &lines[end_idx..] {
                result.push_str(line);
                result.push('\n');
            }

            // Preserve trailing newline behavior
            if !content.ends_with('\n') && result.ends_with('\n') {
                result.pop();
            }

            Ok(result)
        }

        EditMode::SearchReplace {
            search,
            replace,
            all,
        } => {
            if !content.contains(search.as_str()) {
                return Err(FileEngineError::EditFailed(format!(
                    "Search text not found: '{}'",
                    if search.len() > 50 {
                        format!("{}...", &search[..50])
                    } else {
                        search.clone()
                    }
                )));
            }

            let result = if *all {
                content.replace(search.as_str(), replace.as_str())
            } else {
                content.replacen(search.as_str(), replace.as_str(), 1)
            };

            Ok(result)
        }

        EditMode::InsertAt {
            line,
            content: new_content,
        } => {
            let lines: Vec<&str> = content.lines().collect();
            let total = lines.len() as u32;

            if *line == 0 {
                return Err(FileEngineError::EditFailed(
                    "insert line 0 is invalid — lines are 1-based".to_string(),
                ));
            }
            // A line past EOF means "append at the end" (same to-EOF idiom as
            // LineRange above) — clamp instead of rejecting so the reflexive
            // append lands.
            let line = (*line).min(total + 1);
            let insert_idx = (line - 1) as usize;
            let mut result = String::new();

            for line_str in &lines[..insert_idx] {
                result.push_str(line_str);
                result.push('\n');
            }

            result.push_str(new_content);
            if !new_content.ends_with('\n') {
                result.push('\n');
            }

            for line_str in &lines[insert_idx..] {
                result.push_str(line_str);
                result.push('\n');
            }

            if !content.ends_with('\n') && result.ends_with('\n') {
                result.pop();
            }

            Ok(result)
        }

        EditMode::Append {
            content: new_content,
        } => {
            let mut result = content.to_string();
            if !result.ends_with('\n') && !result.is_empty() {
                result.push('\n');
            }
            result.push_str(new_content);
            Ok(result)
        }
    }
}

/// Simple reverse diff application.
///
/// Extracts removed lines from the diff and added lines from the original,
/// reconstructing the previous content. This handles the common case where
/// the undo target was the most recent change.
fn apply_reverse_simple(current: &str, reverse_diff: &FileDiff) -> Option<String> {
    if reverse_diff.hunks.is_empty() {
        return None;
    }

    // Simple approach: use the unified diff lines.
    // Lines starting with '-' in the reverse diff are what to remove from current.
    // Lines starting with '+' in the reverse diff are what to add.
    // Lines starting with ' ' are context (unchanged).
    let mut result_lines: Vec<String> = Vec::new();
    let current_lines: Vec<&str> = current.lines().collect();
    let mut current_idx = 0;

    for hunk in &reverse_diff.hunks {
        let hunk_start = (hunk.old_start as usize).saturating_sub(1);

        // Copy lines before this hunk
        while current_idx < hunk_start && current_idx < current_lines.len() {
            result_lines.push(current_lines[current_idx].to_string());
            current_idx += 1;
        }

        // Process hunk content
        for line in hunk.content.lines() {
            if let Some(stripped) = line.strip_prefix('+') {
                // Add this line (it's being added by the reverse)
                result_lines.push(stripped.to_string());
            } else if let Some(_stripped) = line.strip_prefix('-') {
                // Skip this line (it's being removed by the reverse)
                current_idx += 1;
            } else if let Some(stripped) = line.strip_prefix(' ') {
                // Context line
                result_lines.push(stripped.to_string());
                current_idx += 1;
            }
        }
    }

    // Copy remaining lines
    while current_idx < current_lines.len() {
        result_lines.push(current_lines[current_idx].to_string());
        current_idx += 1;
    }

    let mut result = result_lines.join("\n");
    if current.ends_with('\n') && !result.ends_with('\n') {
        result.push('\n');
    }

    Some(result)
}

/// Extract added content from a diff (lines starting with '+').
/// Used for reconstructing files on undo of delete.
fn extract_added_content(diff: &FileDiff) -> String {
    let mut lines = Vec::new();
    for hunk in &diff.hunks {
        for line in hunk.content.lines() {
            if let Some(stripped) = line.strip_prefix('+') {
                lines.push(stripped);
            }
        }
    }
    let mut result = lines.join("\n");
    if !result.is_empty() && !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// Get current time in milliseconds since epoch.
fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // what this catches: the silent misplaced edit. A persona addressed a line she had
    // never seen numbered (`code/shell cat` has no line numbers) and dropped a guard clause
    // into a function's parameter list; the receipt said `success: true` and nothing else,
    // so her next turn had no way to know (M5 glass-box, 2026-08-04, SWE-bench flask-4045).
    // The receipt must carry the numbered landing site so a wrong line becomes a visible
    // fact in working memory. Pins: line-addressed edits report the neighborhood with the
    // anchor marked, and a whole-file write reports None (no landing site to speak of).
    #[test]
    fn a_line_addressed_edit_reports_where_it_actually_landed() {
        let (_dir, engine) = setup_engine();

        let inserted = engine
            .edit(
                "src/main.ts",
                &EditMode::InsertAt {
                    line: 2,
                    content: "INSERTED\n".to_string(),
                },
                None,
            )
            .expect("insert");
        let ctx = inserted
            .applied_context
            .expect("a line-addressed edit must report its landing site");
        assert!(
            ctx.contains("INSERTED"),
            "the receipt must show the text that landed, got:\n{ctx}"
        );
        assert!(
            ctx.contains(">    2 |"),
            "the anchor line must be marked so the landing site is unmissable, got:\n{ctx}"
        );
        assert!(
            ctx.contains("line 1") && ctx.contains("line 3"),
            "surrounding lines are the whole point — they are what reveals a guard clause \
             dropped into a parameter list, got:\n{ctx}"
        );

        // A whole-file write replaces everything; there is no landing site to report, and
        // inventing one would be noise in a small working-memory window.
        let written = engine
            .write("src/other.ts", "a\nb\n", None)
            .expect("write");
        assert!(
            written.applied_context.is_none(),
            "a whole-file write has no line to locate"
        );
    }

    fn setup_engine() -> (tempfile::TempDir, FileEngine) {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/main.ts"), "line 1\nline 2\nline 3\n").unwrap();

        let security = PathSecurity::new(dir.path()).unwrap();
        let engine = FileEngine::new("test-persona", security);
        (dir, engine)
    }

    #[test]
    fn test_read_full_file() {
        let (_dir, engine) = setup_engine();
        let result = engine.read("src/main.ts", None, None).unwrap();
        assert!(result.success);
        assert_eq!(result.total_lines, 3);
        assert!(result.content.unwrap().contains("line 1"));
    }

    #[test]
    fn test_read_line_range() {
        let (_dir, engine) = setup_engine();
        let result = engine.read("src/main.ts", Some(2), Some(2)).unwrap();
        assert!(result.success);
        assert_eq!(result.lines_returned, 1);
        assert_eq!(result.content.unwrap(), "line 2");
    }

    #[test]
    fn test_read_nonexistent() {
        let (_dir, engine) = setup_engine();
        let result = engine.read("src/nonexistent.ts", None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_write_new_file() {
        let (_dir, engine) = setup_engine();
        let result = engine
            .write(
                "src/new.ts",
                "export const x = 1;\n",
                Some("Create new file"),
            )
            .unwrap();
        assert!(result.success);
        assert!(result.change_id.is_some());
        assert_eq!(result.bytes_written, 20);

        // Verify content
        let read = engine.read("src/new.ts", None, None).unwrap();
        assert!(read.content.unwrap().contains("export const x = 1;"));
    }

    #[test]
    fn test_write_overwrite_creates_diff() {
        let (_dir, engine) = setup_engine();

        // Overwrite existing file
        let result = engine
            .write("src/main.ts", "modified\n", Some("Overwrite"))
            .unwrap();
        assert!(result.success);

        // Check history
        let history = engine.file_history("src/main.ts", 10);
        assert_eq!(history.nodes.len(), 1);
        assert!(history.nodes[0].forward_diff.unified.contains("-line 1"));
        assert!(history.nodes[0].forward_diff.unified.contains("+modified"));
    }

    #[test]
    fn test_edit_search_replace() {
        let (_dir, engine) = setup_engine();

        let result = engine
            .edit(
                "src/main.ts",
                &EditMode::SearchReplace {
                    search: "line 2".to_string(),
                    replace: "line 2 modified".to_string(),
                    all: false,
                },
                Some("Modify line 2"),
            )
            .unwrap();
        assert!(result.success);

        let read = engine.read("src/main.ts", None, None).unwrap();
        assert!(read.content.unwrap().contains("line 2 modified"));
    }

    #[test]
    fn test_edit_line_range() {
        let (_dir, engine) = setup_engine();

        let result = engine
            .edit(
                "src/main.ts",
                &EditMode::LineRange {
                    start_line: 2,
                    end_line: 2,
                    new_content: "replaced line".to_string(),
                },
                Some("Replace line 2"),
            )
            .unwrap();
        assert!(result.success);

        let read = engine.read("src/main.ts", None, None).unwrap();
        let content = read.content.unwrap();
        assert!(content.contains("line 1"));
        assert!(content.contains("replaced line"));
        assert!(content.contains("line 3"));
        assert!(!content.contains("line 2\n"));
    }

    // what this catches: the live stall (2026-07-14) — Devstral personas emitted
    // edit_file with end_line: 65535 (the u16 "to EOF" sentinel) to replace a
    // whole file; the engine rejected it as "out of range", they re-emitted the
    // identical broken edit turn after turn, never compiled/ran, and confabulated
    // success. A past-EOF end_line must CLAMP to the last line and land the edit.
    #[test]
    fn line_range_end_past_eof_clamps_and_replaces_to_end() {
        let (_dir, engine) = setup_engine(); // 3-line file: "line 1\nline 2\nline 3\n"

        let result = engine
            .edit(
                "src/main.ts",
                &EditMode::LineRange {
                    start_line: 1,
                    end_line: 65535, // reflexive "replace whole file" sentinel
                    new_content: "brand new body".to_string(),
                },
                Some("whole-file replace via to-EOF sentinel"),
            )
            .unwrap();
        assert!(result.success);

        let content = engine.read("src/main.ts", None, None).unwrap().content.unwrap();
        assert!(content.contains("brand new body"));
        assert!(!content.contains("line 1"));
        assert!(!content.contains("line 3"));
    }

    // what this catches: a partial to-EOF edit — start mid-file, end past EOF —
    // keeps the head and replaces the tail, rather than erroring.
    #[test]
    fn line_range_from_middle_to_past_eof_keeps_head_replaces_tail() {
        let (_dir, engine) = setup_engine();

        let result = engine
            .edit(
                "src/main.ts",
                &EditMode::LineRange {
                    start_line: 2,
                    end_line: 9999,
                    new_content: "tail rewritten".to_string(),
                },
                Some("replace from line 2 to end"),
            )
            .unwrap();
        assert!(result.success);

        let content = engine.read("src/main.ts", None, None).unwrap().content.unwrap();
        assert!(content.contains("line 1"));
        assert!(content.contains("tail rewritten"));
        assert!(!content.contains("line 2"));
        assert!(!content.contains("line 3"));
    }

    // what this catches: a genuinely inverted range (end below start-1) is still
    // an error — the clamp widens intent, it doesn't swallow contradictions.
    #[test]
    fn line_range_inverted_still_errors() {
        let (_dir, engine) = setup_engine();

        let result = engine.edit(
            "src/main.ts",
            &EditMode::LineRange {
                start_line: 3,
                end_line: 1,
                new_content: "x".to_string(),
            },
            Some("inverted range"),
        );
        assert!(result.is_err());
    }

    // what this catches: InsertAt past EOF is the "append" idiom — clamp to
    // total+1 and append instead of rejecting.
    #[test]
    fn insert_at_past_eof_appends() {
        let (_dir, engine) = setup_engine();

        let result = engine
            .edit(
                "src/main.ts",
                &EditMode::InsertAt {
                    line: 65535,
                    content: "appended tail".to_string(),
                },
                Some("append via past-EOF insert"),
            )
            .unwrap();
        assert!(result.success);

        let content = engine.read("src/main.ts", None, None).unwrap().content.unwrap();
        assert!(content.contains("line 3\nappended tail"));
    }

    #[test]
    fn test_edit_insert_at() {
        let (_dir, engine) = setup_engine();

        let result = engine
            .edit(
                "src/main.ts",
                &EditMode::InsertAt {
                    line: 2,
                    content: "inserted line".to_string(),
                },
                Some("Insert before line 2"),
            )
            .unwrap();
        assert!(result.success);

        let read = engine.read("src/main.ts", None, None).unwrap();
        let content = read.content.unwrap();
        assert!(content.contains("line 1\ninserted line\nline 2"));
    }

    #[test]
    fn test_edit_append() {
        let (_dir, engine) = setup_engine();

        let result = engine
            .edit(
                "src/main.ts",
                &EditMode::Append {
                    content: "line 4".to_string(),
                },
                Some("Append line 4"),
            )
            .unwrap();
        assert!(result.success);

        let read = engine.read("src/main.ts", None, None).unwrap();
        assert!(read.content.unwrap().contains("line 4"));
    }

    #[test]
    fn test_delete_file() {
        let (_dir, engine) = setup_engine();

        let result = engine.delete("src/main.ts", Some("Remove main")).unwrap();
        assert!(result.success);

        let read = engine.read("src/main.ts", None, None);
        assert!(read.is_err()); // File should not exist
    }

    // what this catches: the write boundary is the SANDBOX, not a file-extension
    // allowlist — a822d6337 (#1908) deleted the extension list deliberately, and
    // this pins the new contract so the list doesn't creep back. (This test
    // previously asserted `.exe` writes fail; it was stale against that commit.)
    #[test]
    fn test_write_any_extension_inside_sandbox() {
        let (_dir, engine) = setup_engine();
        let result = engine.write("src/tool.exe", "bytes", None);
        assert!(
            result.is_ok(),
            "extension is not a boundary; the sandbox is: {result:?}"
        );
    }

    #[test]
    fn test_preview_diff() {
        let (_dir, engine) = setup_engine();
        let diff = engine
            .preview_diff(
                "src/main.ts",
                &EditMode::SearchReplace {
                    search: "line 2".to_string(),
                    replace: "LINE TWO".to_string(),
                    all: false,
                },
            )
            .unwrap();
        assert!(diff.unified.contains("-line 2"));
        assert!(diff.unified.contains("+LINE TWO"));
    }

    #[test]
    fn test_workspace_history() {
        let (_dir, engine) = setup_engine();

        engine.write("src/a.ts", "a", Some("Write a")).unwrap();
        engine.write("src/b.ts", "b", Some("Write b")).unwrap();

        let history = engine.workspace_history(10);
        assert_eq!(history.nodes.len(), 2);
        assert_eq!(history.nodes[0].description.as_deref(), Some("Write b"));
        assert_eq!(history.nodes[1].description.as_deref(), Some("Write a"));
    }

    #[test]
    fn test_edit_search_not_found() {
        let (_dir, engine) = setup_engine();
        let result = engine.edit(
            "src/main.ts",
            &EditMode::SearchReplace {
                search: "nonexistent text".to_string(),
                replace: "replacement".to_string(),
                all: false,
            },
            None,
        );
        assert!(result.is_err());
    }

    // ════════════════════════════════════════════════════════════════
    // Filesystem introspection — persona-as-developer cluster
    // ════════════════════════════════════════════════════════════════
    //
    // Tests for exists / list_dir / glob_match per
    // docs/planning/PERSONA-AS-DEVELOPER-GAP.md priority 1 (the
    // safe-self-scaffolding seam).

    fn setup_engine_with_tree() -> (tempfile::TempDir, FileEngine) {
        let dir = tempfile::tempdir().unwrap();
        // Mini tree:
        //   src/main.ts                              file
        //   src/utils/helpers.ts                     file
        //   src/utils/.private.ts                    hidden file
        //   src/empty_dir/                           empty dir
        //   docs/README.md                           file in sibling
        fs::create_dir_all(dir.path().join("src/utils")).unwrap();
        fs::create_dir_all(dir.path().join("src/empty_dir")).unwrap();
        fs::create_dir_all(dir.path().join("docs")).unwrap();
        fs::write(dir.path().join("src/main.ts"), "x").unwrap();
        fs::write(dir.path().join("src/utils/helpers.ts"), "y").unwrap();
        fs::write(dir.path().join("src/utils/.private.ts"), "z").unwrap();
        fs::write(dir.path().join("docs/README.md"), "w").unwrap();
        let security = PathSecurity::new(dir.path()).unwrap();
        let engine = FileEngine::new("test-persona", security);
        (dir, engine)
    }

    // ── exists ──────────────────────────────────────────────────────

    #[test]
    fn exists_reports_file_with_size() {
        let (_dir, engine) = setup_engine_with_tree();
        let r = engine.exists("src/main.ts").expect("exists must succeed");
        assert!(r.exists);
        assert_eq!(r.kind, Some(FsEntryKind::File));
        assert_eq!(r.size_bytes, Some(1));
        assert!(r.error.is_none());
    }

    #[test]
    fn exists_reports_directory_without_size() {
        let (_dir, engine) = setup_engine_with_tree();
        let r = engine.exists("src/utils").expect("exists must succeed");
        assert!(r.exists);
        assert_eq!(r.kind, Some(FsEntryKind::Directory));
        assert_eq!(r.size_bytes, None, "directories don't report size");
    }

    #[test]
    fn exists_reports_false_for_missing_with_no_error() {
        let (_dir, engine) = setup_engine_with_tree();
        let r = engine
            .exists("src/nonexistent.ts")
            .expect("missing path is NOT an error — exists=false");
        assert!(!r.exists);
        assert_eq!(r.kind, None);
        assert_eq!(r.size_bytes, None);
        assert!(r.error.is_none(), "missing != error per the contract");
    }

    #[test]
    fn exists_rejects_path_outside_workspace_via_path_security() {
        let (_dir, engine) = setup_engine_with_tree();
        let err = engine
            .exists("../escape.ts")
            .expect_err("workspace escape must fail loud via PathSecurity");
        let msg = err.to_string();
        assert!(
            msg.contains("Security") || msg.contains("escape"),
            "error must surface PathSecurity layer: {msg}"
        );
    }

    // ── list_dir ────────────────────────────────────────────────────

    #[test]
    fn list_dir_returns_flat_listing_directories_first() {
        let (_dir, engine) = setup_engine_with_tree();
        let r = engine.list_dir("src", false).expect("list must succeed");
        assert!(r.success);
        // src has: main.ts (file), utils (dir), empty_dir (dir)
        // Sorted: directories first (alphabetical: empty_dir, utils),
        // then files (main.ts).
        let names: Vec<&str> = r.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["empty_dir", "utils", "main.ts"],
            "directories must come before files; each group alphabetical"
        );
        assert_eq!(r.total_count, 3);
    }

    #[test]
    fn list_dir_excludes_hidden_by_default_includes_when_asked() {
        let (_dir, engine) = setup_engine_with_tree();

        let default = engine.list_dir("src/utils", false).expect("default");
        let names: Vec<&str> = default.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["helpers.ts"],
            ".private.ts must be excluded by default"
        );

        let with_hidden = engine
            .list_dir("src/utils", true)
            .expect("include_hidden=true");
        let names: Vec<&str> = with_hidden.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec![".private.ts", "helpers.ts"],
            "include_hidden=true surfaces dotfiles, still alphabetical"
        );
    }

    #[test]
    fn list_dir_reports_file_size_only_for_files() {
        let (_dir, engine) = setup_engine_with_tree();
        let r = engine.list_dir("src", false).expect("list");
        for entry in &r.entries {
            match entry.kind {
                FsEntryKind::File => assert!(
                    entry.size_bytes.is_some(),
                    "{}: file must report size_bytes",
                    entry.name
                ),
                FsEntryKind::Directory => assert!(
                    entry.size_bytes.is_none(),
                    "{}: directory must NOT report size_bytes",
                    entry.name
                ),
                _ => {}
            }
        }
    }

    #[test]
    fn list_dir_rejects_non_directory_path_loud() {
        let (_dir, engine) = setup_engine_with_tree();
        let err = engine
            .list_dir("src/main.ts", false)
            .expect_err("listing a file (not a dir) must fail loud");
        assert!(err.to_string().contains("not a directory"));
    }

    #[test]
    fn list_dir_for_missing_path_returns_not_found() {
        let (_dir, engine) = setup_engine_with_tree();
        let err = engine
            .list_dir("src/nonexistent", false)
            .expect_err("missing directory must fail loud");
        assert!(err.to_string().contains("not found"));
    }

    // what this catches: resolve_dir is the ONE directory resolver — idiom-forgiveness
    // + honest errors + sandbox boundary in one place (2026-07-14 consolidation, and
    // the fix for Anwen's "workspace/ prefix → workspace is not a directory → give up").
    #[test]
    fn resolve_dir_is_the_one_forgiving_honest_secure_resolver() {
        let (_dir, engine) = setup_engine_with_tree(); // has src/ (dir), src/main.ts (file)

        // plain dir resolves
        assert!(engine.resolve_dir("src").unwrap().is_dir());
        // "." / empty → workspace root
        assert_eq!(engine.resolve_dir(".").unwrap(), engine.workspace_root());
        assert_eq!(engine.resolve_dir("").unwrap(), engine.workspace_root());
        // FORGIVENESS: a redundant leading "workspace/" is stripped (the root already
        // IS the workspace) — this is the exact live give-up shape.
        assert_eq!(
            engine.resolve_dir("workspace/src").unwrap(),
            engine.resolve_dir("src").unwrap()
        );
        // a leading slash is tolerated
        assert_eq!(
            engine.resolve_dir("/src").unwrap(),
            engine.resolve_dir("src").unwrap()
        );
        // HONEST: a FILE says so and points to code/read
        let file_err = engine.resolve_dir("src/main.ts").unwrap_err().to_string();
        assert!(file_err.contains("is a FILE"), "{file_err}");
        assert!(file_err.contains("code/read"), "{file_err}");
        // HONEST: a miss re-orients instead of reading as terminal
        let miss = engine.resolve_dir("does/not/exist").unwrap_err().to_string();
        assert!(miss.contains("path not found"), "{miss}");
        assert!(miss.contains("workspace root itself IS explorable"), "{miss}");
        // SECURE: `..` traversal is blocked (the bypass the old handler joins skipped)
        assert!(engine.resolve_dir("../../etc").is_err());
    }

    #[test]
    fn list_dir_handles_empty_directory_cleanly() {
        let (_dir, engine) = setup_engine_with_tree();
        let r = engine
            .list_dir("src/empty_dir", false)
            .expect("empty dir lists cleanly");
        assert_eq!(r.entries.len(), 0);
        assert_eq!(r.total_count, 0);
    }

    // ── glob_match ──────────────────────────────────────────────────

    #[test]
    fn glob_matches_files_by_extension_recursively() {
        let (_dir, engine) = setup_engine_with_tree();
        let r = engine
            .glob_match("**/*.ts", None)
            .expect("glob must succeed");
        assert!(r.success);
        // Should match main.ts + helpers.ts (NOT .private.ts —
        // hidden files excluded by ignore's standard filters).
        assert!(
            r.matches.iter().any(|p| p == "src/main.ts"),
            "expected src/main.ts in matches: {:?}",
            r.matches
        );
        assert!(
            r.matches.iter().any(|p| p == "src/utils/helpers.ts"),
            "expected src/utils/helpers.ts in matches: {:?}",
            r.matches
        );
        // Matches are sorted for determinism.
        let mut sorted = r.matches.clone();
        sorted.sort();
        assert_eq!(r.matches, sorted, "matches must be sorted alphabetically");
        assert!(!r.truncated);
    }

    #[test]
    fn glob_scoped_to_subdirectory_via_root_param() {
        let (_dir, engine) = setup_engine_with_tree();
        let r = engine
            .glob_match("**/*.ts", Some("src/utils"))
            .expect("scoped glob must succeed");
        // Only helpers.ts should match — main.ts is outside src/utils.
        assert_eq!(
            r.matches,
            vec!["src/utils/helpers.ts".to_string()],
            "root param must scope the walk: {:?}",
            r.matches
        );
    }

    #[test]
    fn glob_with_no_matches_returns_empty_not_error() {
        let (_dir, engine) = setup_engine_with_tree();
        let r = engine
            .glob_match("**/*.nope", None)
            .expect("no matches != error");
        assert!(r.success);
        assert!(r.matches.is_empty());
        assert_eq!(r.total_matches, 0);
        assert!(!r.truncated);
    }

    #[test]
    fn glob_rejects_bad_pattern_loud() {
        let (_dir, engine) = setup_engine_with_tree();
        let err = engine
            .glob_match("[invalid", None)
            .expect_err("malformed glob must fail loud");
        assert!(err.to_string().contains("bad pattern"));
    }

    #[test]
    fn glob_rejects_root_outside_workspace_via_path_security() {
        let (_dir, engine) = setup_engine_with_tree();
        let err = engine
            .glob_match("**/*", Some("../escape"))
            .expect_err("workspace escape must fail loud");
        let msg = err.to_string();
        assert!(
            msg.contains("Security") || msg.contains("escape"),
            "PathSecurity layer must surface: {msg}"
        );
    }

    // ── concurrency stress test ─────────────────────────────────────
    //
    // Per [field manual §4.2](docs/architecture/COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md):
    // multi-thread tokio for any handler that holds state across
    // calls. FileEngine is &self read-only here, but workspaces are
    // shared across personas — N concurrent reads must NOT interfere.
    //
    // The test fires 32 concurrent exists/list/glob ops and verifies
    // every result is internally consistent.

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn introspection_under_concurrent_load_returns_consistent_results() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        for i in 0..10 {
            fs::write(dir.path().join(format!("src/file_{i}.ts")), "x").unwrap();
        }
        let security = PathSecurity::new(dir.path()).unwrap();
        let engine = std::sync::Arc::new(FileEngine::new("test-persona", security));

        const PARALLEL: usize = 32;
        let mut tasks = Vec::with_capacity(PARALLEL);
        for i in 0..PARALLEL {
            let engine = engine.clone();
            tasks.push(tokio::spawn(async move {
                // Each task does the trio: exists + list + glob.
                let target = format!("src/file_{}.ts", i % 10);
                let exists = engine.exists(&target).expect("exists");
                let list = engine.list_dir("src", false).expect("list");
                let glob = engine.glob_match("**/*.ts", None).expect("glob");
                (exists, list, glob)
            }));
        }
        let results: Vec<_> = futures::future::join_all(tasks)
            .await
            .into_iter()
            .map(|r| r.expect("task must not panic"))
            .collect();

        for (exists, list, glob) in &results {
            // exists: always finds something (we round-robin file_0..9)
            assert!(exists.exists);
            assert_eq!(exists.kind, Some(FsEntryKind::File));
            // list: always returns the 10 src files
            assert_eq!(list.total_count, 10, "list result must be stable across concurrent reads");
            // glob: always returns the 10 src files
            assert_eq!(
                glob.total_matches, 10,
                "glob must return all 10 matches regardless of concurrent siblings"
            );
        }
    }
}
