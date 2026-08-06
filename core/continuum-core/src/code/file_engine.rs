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
    write_policy: WritePolicy,
}

/// What an edit that lands INSIDE a string literal should cost (#317).
///
/// Writing code as TEXT is a first-class thing a citizen does — a docstring example, a quoted
/// snippet, a fixture, a doc block. Talking about code is not a defect, and the gate must never
/// make her unable to do it. What she needs is to KNOW the text is not executing, which is why
/// the warning is UNCONDITIONAL and lives on every path (Joel, 2026-08-06: "They need to be able
/// to talk about code like any other first class citizen, just know that this isn't doing").
///
/// [`RefuseInert`](Self::RefuseInert) exists for the one context where that ambiguity does not
/// exist: a SCORED run, where the deliverable IS a patch that has to execute, graded by applying
/// the diff and running the tests. There, an inert insertion is never what she meant, the run
/// cannot recover from it (the verdict reads as a capability zero), and the file is the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WritePolicy {
    /// Let the edit land and TELL her it is inert. Every live citizen path.
    #[default]
    Warn,
    /// Reject the edit, leave the file untouched, and say why — so the act→observe circuit
    /// turns it into a retry instead of a destroyed patch. Scored/benchmark paths only.
    RefuseInert,
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
            write_policy: WritePolicy::default(),
        }
    }

    /// Harden this engine for a SCORED run: an edit whose inserted code lands inside a string
    /// literal is refused rather than warned. See [`WritePolicy`] for why this is scoped to
    /// measurement and never applied to a living citizen's hands.
    pub fn with_write_policy(mut self, policy: WritePolicy) -> Self {
        self.write_policy = policy;
        self
    }

    /// The inert-edit stance this engine enforces.
    pub fn write_policy(&self) -> WritePolicy {
        self.write_policy
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

        // NUMBERED, because `code/edit` addresses lines by NUMBER and this is the only
        // tool that shows the file. Returning bare text asked the caller to count lines by
        // hand and then bet an edit on the count. Glass-boxed on SWE-bench flask-4045 (M5,
        // 2026-08-04): she read the whole 128-line file, asked for `insert_at` line 35, and
        // landed 4 lines off — inside `super().__init__(`'s argument list, where a statement
        // is a SyntaxError. Same shape in runs 1 and 3. A read tool and an edit tool that
        // disagree about whether lines have numbers is a defect in the PAIR, not in the
        // caller. `{n:>6} | {line}` matches `applied_context`'s neighborhood, so the file
        // reads identically before an edit and after one.
        let selected: String = content
            .lines()
            .enumerate()
            .filter(|(i, _)| {
                let line_num = *i as u32 + 1;
                line_num >= start && line_num <= end
            })
            .map(|(i, line)| format!("{:>6} | {}", i as u32 + 1, line))
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
            // A whole-file write has no landing site — but when it lands ON existing
            // content it has something more consequential to report: what it destroyed,
            // and whether the bytes it wrote were ever meant to be file content at all.
            applied_context: {
                let mut out = numbered_paste_notice(content);
                if let Some(magnitude) = overwrite_magnitude(&old_content, content) {
                    out.push_str(&magnitude);
                }
                (!out.is_empty()).then_some(out)
            },
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

        // AN EDIT MAY NOT BREAK A FILE THAT PARSED. Refuse, restore, explain.
        //
        // Glass-boxed on sympy-22005 (M5, 2026-08-04). She had the right idea and asked for
        // `line_range` 240..247 — but the block she meant to replace ends at 249, because
        // lines 248-249 are the tail of a triple-quoted string. Her replacement carried its
        // own `'''))`, so the original's last two lines survived as an orphaned tail and the
        // module stopped parsing. Semantics right, extent wrong, score zero.
        //
        // The syntax receipt below already caught this and named the `code/undo` handle. It
        // was not enough: across every run measured this session she repaired FORWARD and
        // never once reverted, so the wreckage stayed under the later edits
        // ([[the-edit-defect-is-grammar-not-text-bracket-balance-is-not-a-proxy]]). Reporting
        // a break she does not act on leaves a broken file either way.
        //
        // So the check moves from reporting to GATING. Same information, same moment, but the
        // file stays valid and there is nothing to stack on. This is not a heuristic steering
        // her cognition — it is the tool refusing to perform an operation it can prove is
        // destructive, and the ONLY case it fires is "parsed before, does not parse after",
        // which no correct single edit produces. A whole-file `code/write` is deliberately NOT
        // gated: when she genuinely intends content that does not parse yet, that is the verb.
        // A NameError the edit INTRODUCED is provable and fatal at runtime, and the syntax
        // gate cannot see it. Refuse the same way, for the same reason: the tool can prove
        // this specific harm. Measured on sympy-21379 — a 2-line edit calling an unimported
        // `clear_cache` reported success and destroyed 14 passing tests.
        if let Some(names) = introduced_undefined_calls(&abs_path, &old_content, &new_content) {
            let list = names.join(", ");
            let one = names.first().cloned().unwrap_or_default();
            // A refusal is a load-bearing decision and it was INVISIBLE: the run verdict carries
            // acts + filesChanged, so a solve that burns its whole act budget on refused edits
            // looks identical to one that never tried to edit at all (glass-boxed on
            // sympy-21379 v9: 30 acts, 0 files, no way to tell which). Probe both gates.
            crate::probe!(
                class = "code.edit.refused",
                reason = "undefined_name",
                path = %relative_path,
                names = %list,
                "edit refused — it would introduce a NameError; file unchanged on disk"
            );
            return Ok(WriteResult {
                success: false,
                change_id: None,
                file_path: relative_path.to_string(),
                bytes_written: 0,
                error: Some(format!(
                    "EDIT REFUSED — it calls {list}, which {} defined or imported anywhere in                      {relative_path}. The file is UNCHANGED on disk.\n\nTHE FIX: this is a                      missing import, not a syntax problem — the file parses fine and would                      raise `NameError: name '{one}' is not defined` the first time this code                      runs. Add the import for {one} (find where it lives with `code/search`),                      or call something this module already has.",
                    if names.len() == 1 { "is not" } else { "are not" }
                )),
                applied_context: None,
            });
        }
        if !parses_clean(&abs_path, &new_content) && parses_clean(&abs_path, &old_content) {
            crate::probe!(
                class = "code.edit.refused",
                reason = "parse_break",
                path = %relative_path,
                "edit refused — it would break the file's parse; file unchanged on disk"
            );
            return Ok(WriteResult {
                success: false,
                change_id: None,
                file_path: relative_path.to_string(),
                bytes_written: 0,
                error: Some(format!(
                    "EDIT REFUSED — it would have made {relative_path} unparseable. The file \
                     is UNCHANGED on disk; nothing was written and there is nothing to undo.\n\
                     {}\n{}",
                    syntax_error_detail(&abs_path, &new_content).unwrap_or_default(),
                    // Don't just diagnose — AIM HER. The refusal used to say "widen the range",
                    // which is advice she has to act on blind. The same parser that proved the
                    // edit destructive can find the end line that works, so say the number.
                    //
                    // But FIRST check the far more embarrassing cause, because the line-range
                    // advice is actively misleading when it applies: content still carrying the
                    // `   12 | ` gutter that `code/read` prints. `numbered_paste_notice` has
                    // detected exactly this since it was written — but it was only ever consulted
                    // on the write() SUCCESS path, as `applied_context`. The refusal path, the one
                    // place the diagnosis decides whether she recovers, never asked.
                    //
                    // Glass-boxed on sympy-21379: she read `basic.py`, pasted the numbered output
                    // straight back as `content`, and every line was indented by the gutter →
                    // `IndentationError: unexpected indent`. The refusal correctly saved the file,
                    // then told her to widen a line range she wasn't using. She spent the
                    // remaining 16 acts of a 30-act budget chasing that wrong lead and never
                    // landed an edit. [[a-probe-that-can-only-fail-is-worse-than-no-probe]]
                    {
                        let gutter = numbered_paste_refusal(&new_content);
                        if gutter.is_empty() {
                            repair_hint(&abs_path, &old_content, edit_mode)
                        } else {
                            gutter
                        }
                    }
                )),
                applied_context: None,
            });
        }

        // SCORED RUNS ONLY (#317): the edit parses, so the gate above passed it — but the code
        // she inserted landed inside a string literal, where it is text. On a live path that is
        // her business (a docstring example, a fixture, a quoted snippet) and the warning on the
        // success path tells her it will not execute. On a MEASURED run the deliverable IS a
        // patch that has to execute, the grader reads only the diff, and a run cannot recover
        // from a file it believes it fixed — so refuse and let the act→observe circuit retry.
        //
        // Measured three times on pallets__flask-4045: the model derived the correct guard every
        // time and the write destroyed it every time — unparseable, then inside the class
        // docstring twice. The third run applied cleanly with all 51 PASS_TO_PASS green and both
        // FAIL_TO_PASS still failing, which is what an inert guard looks like from the outside.
        if self.write_policy == WritePolicy::RefuseInert {
            if let Some(where_) = inert_insertion_sites(&abs_path, &old_content, &new_content) {
                return Ok(WriteResult {
                    success: false,
                    change_id: None,
                    file_path: relative_path.to_string(),
                    bytes_written: 0,
                    error: Some(format!(
                        "EDIT REFUSED — the code you inserted would land INSIDE A STRING \
                         LITERAL, so it would be TEXT, not code. The file is UNCHANGED on disk; \
                         nothing was written and there is nothing to undo.\n{where_}\n\
                         The file would have parsed and the tests would have run, which is why \
                         this is worth stopping: nothing would have failed and nothing would have \
                         worked, and the score would have read as though you were wrong.\n\
                         \n\
                         Your fix is probably right — it just needs a different anchor:\n{}",
                        inert_edit_recovery()
                    )),
                    applied_context: None,
                });
            }
        }

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
            // Cloned because `applied_context` below hands the SAME id back as the undo
            // handle for a break this edit introduced — one id, two places it must appear.
            change_id: Some(change_id.clone()),
            file_path: relative_path.to_string(),
            bytes_written,
            error: None,
            // Read back from the content just written, not from what was requested — the
            // whole point is to report where the edit ACTUALLY landed.
            // The parse verdict LEADS: broken code reads as plausible in a six-line
            // window, and "SyntaxError: line 21" does not.
            applied_context: edit_anchor_line(edit_mode, &new_content).map(|anchor| {
                let mut out = syntax_error_after_edit(&abs_path).unwrap_or_default();
                // A displaced docstring parses clean, so nothing else here would mention it.
                out.push_str(
                    &displaced_docstrings(&abs_path, &old_content, &new_content)
                        .unwrap_or_default(),
                );
                // Neither does code written INTO a literal — the quietest failure of all (#317).
                out.push_str(
                    &inert_insertions(&abs_path, &old_content, &new_content).unwrap_or_default(),
                );
                out.push_str(&line_shift_notice(&old_content, &new_content, anchor));
                out.push_str(&numbered_neighborhood(
                    &new_content,
                    anchor,
                    APPLIED_CONTEXT_RADIUS,
                ));
                out
            }),
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
            change_id: Some(change_id.clone()),
            file_path: relative_path.to_string(),
            bytes_written: 0,
            error: None,
            // "The file is gone, so there is no neighborhood to show" was true and beside the
            // point — the same reasoning that left `code/write` silent about what it
            // overwrote until a persona replaced a 128-line module with a 5-line stub behind
            // `success: true` (4e74d93ce). Delete is the most destructive verb in the set and
            // reported the LEAST: `success: true, bytes_written: 0`, which reads like a
            // no-op. State the size of what went and the handle that brings it back.
            applied_context: Some(format!(
                "DELETED {} — {} line(s), {} byte(s) removed from disk. `code/undo` with \
                 change_id={} restores the file exactly.\n",
                relative_path,
                old_content.lines().count(),
                old_content.len(),
                change_id,
            )),
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
            let is_file = entry.file_type().map(|ft| ft.is_file()).unwrap_or(false);
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
// context-budget-exempt: lines of surrounding source shown around an applied edit so she can SEE what landed — a diff display shape in lines, not a size budget
const APPLIED_CONTEXT_RADIUS: u32 = 6;

/// A file's own language checker, if this file type has a cheap one. `None` means "no
/// opinion" — most extensions, and that is correct: silence must be the default so the
/// warning carries weight when it appears.
///
/// Deliberately NOT a build. `py_compile` parses one file in milliseconds and answers the
/// only question asked here — does this still parse. Running the project's real test suite
/// is the persona's own move via `code/shell`, not something a file write does behind her.
/// This path's in-process validator, if we have a parser for its language.
///
/// Was `syntax_checker_for`, which returned an argv for `python3 -m py_compile`. Core is
/// entirely Rust — see `code::syntax` for why the interpreter had to go. `None` is still
/// the honest default for a language we cannot parse.
fn validator_for(
    path: &std::path::Path,
) -> Option<&'static dyn crate::code::syntax::SyntaxValidator> {
    crate::code::syntax::validator_for(path)
}

/// Does the file still parse after this edit? `Some(error)` only when a checker exists,
/// ran, and REJECTED the file. A missing interpreter, an unknown extension, or a spawn
/// failure all yield `None`: a tool that cannot verify must not render a verdict.
///
/// This exists because no lexical proxy works. Measured on the M5, 2026-08-04 — three
/// SWE-bench runs, every failure the same shape: a guard clause placed inside an open
/// `def __init__(`. The closing paren is still present, so delimiters stay balanced while
/// the file is unparseable (a bracket-balance check was built, falsified by its own test,
/// and thrown away). "Valid text at this location" and "valid program" are different
/// predicates. A real `SyntaxError: line 21` is unambiguous where six numbered lines of
/// plausible-looking Python are not.
///
/// NOTE the narrowed scope since the refusal gate landed. An edit that would break a file
/// that parsed is now rejected before it is written, so reaching this function means the file
/// was ALREADY broken and this edit did not fix it. There is no longer a "your edit caused
/// this" case to report here — the gate reports that, at the moment it refuses, and the file
/// never enters the broken state. The only remaining way in is damage from a whole-file
/// `code/write` (deliberately ungated) or a change made outside the engine.
fn syntax_error_after_edit(abs_path: &std::path::Path) -> Option<String> {
    let validator = validator_for(abs_path)?;
    let content = fs::read_to_string(abs_path).ok()?;
    let fault = validator.parse_check(&content).err()?;
    Some(format!(
        "STILL BROKEN — this file does not parse, and the damage was already here before \
         this edit (an edit that would BREAK a parsing file is refused outright, so it \
         cannot be from this one):\n{fault}\n\
         Fixing only the error above will leave the earlier damage in place. `code/undo` \
         walks changes back one at a time.\n"
    ))
}

/// Would `content` have parsed, at this path's language? Writes it to a sibling temp file so
/// the check never disturbs what is on disk. `false` whenever we cannot tell — the caller
/// uses this only to decide WHICH true statement to make, and "we could not verify" must
/// never masquerade as "it was already broken".
fn parses_clean(abs_path: &std::path::Path, content: &str) -> bool {
    probe_parse(abs_path, content)
        .map(|o| o.is_none())
        .unwrap_or(false)
}

/// A `search_replace` whose text isn't in the file — reported as a LEAD, not a dead end.
///
/// "Search text not found: 'if len(univariate) == 1:'" is a true statement that helps with
/// nothing. The tool is holding the file; it can say WHY the match missed. And this error is
/// on the path the edit-refusal deliberately steers her onto ("anchor on the text itself with
/// a search_replace edit") — a redirect that lands on a dead end is worse than no redirect.
///
/// The dominant cause by far is invisible: leading whitespace. A model reproducing a line from
/// a numbered read routinely drops or normalises the indent, and the diff is unseeable in a
/// quoted string. So when the trimmed forms match, SAY that, and give the exact line with its
/// indent spelled out.
fn search_miss_report(content: &str, search: &str) -> String {
    let head: String = if search.len() > 60 {
        format!("{}…", &search[..60])
    } else {
        search.to_string()
    };

    // Single-line searches are the common case and the one we can localise precisely.
    let needle = search.trim();
    if !needle.is_empty() && !needle.contains('\n') {
        for (i, line) in content.lines().enumerate() {
            if line.trim() == needle {
                let n = i + 1;
                let indent = line.len() - line.trim_start().len();
                return format!(
                    "SEARCH TEXT NOT FOUND — but line {n} matches once whitespace is ignored, so \
                     this is an INDENT mismatch, not a missing line.\n\
                     the file has : {line:?}\n\
                     you searched : {search:?}\n\
                     Line {n} starts with {indent} space(s). Copy it exactly — including leading \
                     whitespace — or use a line_range edit on line {n}, which does not depend on \
                     reproducing the indent."
                );
            }
        }
        // No whitespace-equal line: offer the nearest neighbour so she can see how far off she
        // is, rather than guessing whether the file changed under her.
        if let Some((n, line)) = nearest_line(content, needle) {
            return format!(
                "SEARCH TEXT NOT FOUND. The closest line is {n}:\n\
                 the file has : {line:?}\n\
                 you searched : {search:?}\n\
                 If that is the line you meant, copy it verbatim from a fresh `code/read` — the \
                 text you searched for does not appear anywhere in the file, so it is either \
                 mis-remembered or from a different file."
            );
        }
    }

    format!(
        "SEARCH TEXT NOT FOUND: '{head}'. Nothing in this file resembles it. Re-read the region \
         with `code/read` and copy the anchor verbatim, or address the lines by number with a \
         line_range edit."
    )
}

/// The line most similar to `needle`, by shared-word overlap. Deliberately crude: its only job
/// is to point at something she can LOOK at, and a wrong-but-close pointer still orients her
/// better than "not found". `None` when nothing shares meaningful content.
fn nearest_line(content: &str, needle: &str) -> Option<(usize, String)> {
    let want: Vec<&str> = needle.split_whitespace().collect();
    if want.is_empty() {
        return None;
    }
    let mut best: Option<(usize, usize, String)> = None;
    for (i, line) in content.lines().enumerate() {
        let hits = want.iter().filter(|w| line.contains(**w)).count();
        if hits == 0 {
            continue;
        }
        if best.as_ref().map(|(h, _, _)| hits > *h).unwrap_or(true) {
            best = Some((hits, i + 1, line.to_string()));
        }
    }
    // Demand at least half the tokens so an accidental `self`/`)` overlap is not "closest".
    best.filter(|(hits, _, _)| *hits * 2 >= want.len())
        .map(|(_, n, line)| (n, line))
}

/// How far past a mis-bounded `end_line` to look for the line that closes the construct.
///
/// Sized for real code, not for search: a triple-quoted docstring, an argument list, a nested
/// literal — the shapes that actually swallow an edit — close within a couple of dozen lines.
/// A wider window would mostly buy the ability to "repair" a range so wrong that widening it
/// silently eats unrelated code, which is worse than saying nothing.
const REPAIR_SEARCH_LINES: u32 = 40;

/// Given a refused edit, find the concrete change that would have worked — and say it.
///
/// Glass-boxed on sympy-22005: she asked for `line_range` 240..247 when the block ends at 249,
/// because 248-249 are the tail of a triple-quoted string. The old refusal told her to "widen
/// the range to cover the whole construct", which is exactly the judgement she had already got
/// wrong once. The parser that proved the edit destructive can also prove which end line is
/// right, so it does the counting and hands her the number.
///
/// This is not a heuristic steering her cognition — nothing is guessed. Each candidate is
/// APPLIED and PARSED; the first that parses is reported as fact, and if none do she gets the
/// general guidance instead of a confident wrong answer.
fn repair_hint(abs_path: &std::path::Path, old_content: &str, edit_mode: &EditMode) -> String {
    const GENERAL: &str = "The most common cause is a line range that ends INSIDE a construct \
         it does not close — a triple-quoted string, an open paren, a bracket. Widen the range \
         to cover the whole construct, or anchor on the text itself with a search_replace edit \
         instead of counting lines. Re-read the region first: `code/read` numbers lines by \
         absolute position, so the numbers it shows are the numbers this tool expects.";

    let EditMode::LineRange {
        start_line,
        end_line,
        new_content,
    } = edit_mode
    else {
        return GENERAL.to_string();
    };

    let total = old_content.lines().count() as u32;
    for candidate in (end_line + 1)..=(end_line + REPAIR_SEARCH_LINES).min(total) {
        let widened = EditMode::LineRange {
            start_line: *start_line,
            end_line: candidate,
            new_content: new_content.clone(),
        };
        let Ok(text) = apply_edit(old_content, &widened) else {
            continue;
        };
        if parses_clean(abs_path, &text) {
            let extra = candidate - end_line;
            return format!(
                "THE FIX: use end_line={candidate} instead of {end_line}. Your range stopped \
                 {extra} line(s) short — lines {}..{candidate} are the tail of a construct your \
                 replacement re-opens and closes itself, so the original's tail survived after \
                 it. The SAME new_content with end_line={candidate} parses clean; re-issue the \
                 edit with that one number changed.",
                end_line + 1
            );
        }
    }
    GENERAL.to_string()
}

/// The syntax error `content` WOULD produce at this path's language, formatted for a persona.
/// `None` when it parses, or when this language has no checker — a refusal still stands on the
/// `parses_clean` pair, so an unavailable detail costs an explanation, never a wrong verdict.
fn syntax_error_detail(abs_path: &std::path::Path, content: &str) -> Option<String> {
    let detail = probe_parse(abs_path, content).ok()??;
    let head = detail.lines().take(6).collect::<Vec<_>>().join("\n");
    // The checker names the temp probe it actually read. Handing a persona a path that does
    // not exist sends her to look for a file she cannot find — say the real one.
    let real = abs_path.file_name()?.to_str()?;
    let probe = format!(
        "{}.continuum-parse-probe.{}",
        abs_path.file_stem()?.to_str()?,
        abs_path.extension()?.to_str()?
    );
    Some(head.replace(&probe, real))
}

/// Run the language's syntax checker over `content` WITHOUT touching what is on disk.
///
/// `Ok(None)` parsed clean, `Ok(Some(stderr))` did not, `Err(())` we could not tell (no
/// extension, no checker for this language, probe unwritable). The three-way return is the
/// point: callers must never be able to read "could not verify" as "already broken".
fn probe_parse(abs_path: &std::path::Path, content: &str) -> Result<Option<String>, ()> {
    let validator = validator_for(abs_path).ok_or(())?;
    match validator.parse_check(content) {
        Ok(()) => Ok(None),
        Err(fault) => Ok(Some(fault.to_string())),
    }
}

/// Names the edit CALLS that exist nowhere in the module — a guaranteed `NameError`.
///
/// `parses_clean` is a syntax gate, and a missing import is perfectly valid syntax. So the
/// most destructive edit a model makes routinely sails straight through it.
///
/// Glass-boxed on sympy-21379 (v7 — her first edit to real library source all session). The
/// entire library change was two lines:
///     @cacheit
///     def _subs(self, old, new, **hints):
///   +     # Clear cache to avoid issues with assumptions
///   +     clear_cache()
/// `clear_cache` is imported nowhere in `sympy/core/basic.py` — line 957 is its ONLY
/// occurrence in the file. Every substitution in sympy now raises NameError. Graded result:
/// passToPass 26/40, fourteen previously-passing tests destroyed, and the edit reported
/// `success: true` with no hint of what it had done.
///
/// DELIBERATELY NARROW, because a false refusal here is worse than none
/// ([[a-probe-that-can-only-fail-is-worse-than-no-probe]]). Python scoping is genuinely hard
/// — globals, builtins, star-imports, comprehensions, class bodies, late binding. So this
/// flags a name only when EVERY one of these holds:
///   • it is CALLED in the new content,
///   • it is bound nowhere in the new module (no import, def, class, assignment, param,
///     comprehension target, except-as, with-as, global/nonlocal declaration),
///   • it is not a builtin,
///   • and it appears NOWHERE in the OLD file — so the edit INTRODUCED it.
/// That last clause is what makes it safe: a name the file never mentioned, now called and
/// unbound, is a missing import essentially every time. Anything the file already knew about
/// is left alone, star-imports included.
///
/// Returns the offending names, or `None` when the analysis cannot run at all (no python, a
/// non-python file) — an inconclusive probe must never read as a verdict.
fn introduced_undefined_calls(
    abs_path: &std::path::Path,
    old_content: &str,
    new_content: &str,
) -> Option<Vec<String>> {
    let validator = validator_for(abs_path)?;
    let unbound = validator.unbound_calls(new_content)?;
    // The safety clause: only names the edit INTRODUCED. Anything the old file already
    // mentioned (star-imported, conditionally defined, whatever) is none of our business.
    let introduced: Vec<String> = unbound
        .into_iter()
        .filter(|n| !old_content.contains(n.as_str()))
        .collect();
    (!introduced.is_empty()).then_some(introduced)
}

/// Functions whose docstring the edit DISPLACED — code inserted between `def` and the
/// docstring, so the string is now a bare expression statement and no longer a docstring.
///
/// Same 2-line sympy-21379 edit that produced the missing import also did this:
///     def _subs(self, old, new, **hints):
///   +     clear_cache()
///         """Substitutes an expression old -> new. ..."""
/// `ast.get_docstring(_subs)` returns None afterwards. `help()` loses it, and in a project
/// like sympy — where doctests ARE part of the suite — the doctests in that string stop being
/// collected at all. Valid syntax, so the parse gate cannot see it.
///
/// A WARNING, not a refusal, and the distinction is deliberate: a missing import is a
/// guaranteed `NameError` and earns a refusal; a displaced docstring is real degradation but
/// the code still runs, and she may have meant it. Severity should match force — the gate that
/// cries wolf is the gate that gets ignored.
///
/// Reports only functions whose docstring the edit ACTUALLY MOVED: present before, absent
/// after. A function that never had one is untouched.
fn displaced_docstrings(
    abs_path: &std::path::Path,
    old_content: &str,
    new_content: &str,
) -> Option<String> {
    let validator = validator_for(abs_path)?;
    let lost = validator.displaced_docstrings(old_content, new_content)?;
    if lost.is_empty() {
        return None;
    }
    Some(format!(
        "WARNING: {} no longer has a docstring — your insert went BETWEEN the `def` line and \
         the docstring, so that string is now an ordinary expression statement. `help()` loses \
         it, and any doctests inside it stop being collected (in this repo those may be part of \
         the test suite). Move your code to AFTER the closing quotes.\n",
        lost.join(", ")
    ))
}

/// Code the edit put INSIDE a string literal — the write that parses and does nothing (#317).
///
/// The loudest possible wording, because this failure is the quietest one there is. A file that
/// will not parse screams: `syntax_error_after_edit` names the line, the tests fail immediately,
/// and the cause is obvious. This one leaves a green parse, an unchanged docstring, and a test
/// suite that fails for reasons that look like the model was simply wrong.
///
/// Measured on `pallets__flask-4045` (2026-08-06): she derived the correct guard and wrote it
/// into the middle of the class docstring, deleting ~17 lines of API docs. Every gate we had was
/// silent and the zero was charged to her intelligence.
fn inert_insertion_sites(
    abs_path: &std::path::Path,
    old_content: &str,
    new_content: &str,
) -> Option<String> {
    let validator = validator_for(abs_path)?;
    let inert = validator.inert_insertions(old_content, new_content)?;
    if inert.is_empty() {
        return None;
    }
    Some(inert.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("; "))
}

/// The recovery path, shared by the warning and the refusal so the advice cannot drift between
/// them. Numbered and concrete: naming the verbs beats "widen the range", which is advice she
/// has to act on blind (the sympy-21379 lesson — a correct refusal plus a vague hint burned 16
/// of her 30 acts).
fn inert_edit_recovery() -> &'static str {
    "\x20 1. `code/read` that region and find where the docstring's closing `\"\"\"` is.\n\
     \x20 2. Choose an anchor INSIDE the function or method body — a real statement you can \
     see, not a line of prose.\n\
     \x20 3. Re-apply with `code/edit`, matching on that statement's TEXT rather than a line \
     number (line numbers shift as you edit; text does not).\n\
     \x20 4. Run it (`code/shell`) and confirm the behavior actually changed — a passing parse \
     is not proof that anything executes.\n"
}

/// The LIVE stance: the edit landed, and she is told what it will and will not do. Writing code
/// as text is first-class (a docstring example, a fixture, a snippet she is quoting), so this
/// opens by saying that outright before offering the way out — it must never read as a scolding
/// for something she may have meant.
fn inert_insertions(
    abs_path: &std::path::Path,
    old_content: &str,
    new_content: &str,
) -> Option<String> {
    let where_ = inert_insertion_sites(abs_path, old_content, new_content)?;
    Some(format!(
        "HEADS UP: your edit landed INSIDE A STRING LITERAL ({where_}) — a docstring or quoted \
         block. The file parses and the tests will run, but that code is TEXT: it never executes, \
         so nothing you intended actually changed. This is exactly how a run looks like a wrong \
         answer when the reasoning was right.\n\
         \n\
         If you MEANT to write about code — a docstring example, a snippet you are quoting, a \
         fixture — this is already correct and there is nothing to fix.\n\
         \n\
         If you meant it to RUN:\n{}",
        inert_edit_recovery()
    ))
}

/// What a whole-file write REPLACED, when it replaced anything. `None` for a genuinely
/// new file — creating one destroys nothing and needs no warning.
///
/// Glass-boxed on the M5, 2026-08-04. A persona had just produced a correct 2-line fix to
/// `src/flask/blueprints.py` (128 lines). Two acts later she called `code/write` with a
/// 5-line stub reconstructed from memory — no imports, no class hierarchy, the real module
/// gone — and the receipt told her `success: true, bytes_written: 214`. She read the file
/// afterward and still did not repair it. The task ended with the file destroyed and the
/// correct fix erased.
///
/// The `applied_context` doc says a whole-file write has no landing site to report, and
/// that is true. It does not follow that it has nothing to report. Replacing 128 lines
/// with 5 is the single most consequential thing a file tool can do, and it was the ONE
/// path reporting nothing — so the verb most able to destroy work was the verb with the
/// least feedback. The shrink ratio is the fact; what she does with it stays hers
/// ([[no-hardcoded-heuristics-to-steer-cognition]] — this refuses nothing and gates
/// nothing).
fn overwrite_magnitude(old_content: &str, new_content: &str) -> Option<String> {
    if old_content.is_empty() {
        return None; // a new file: nothing was there to lose
    }
    let before = old_content.lines().count();
    let after = new_content.lines().count();
    let mut out = format!(
        "OVERWROTE an existing file: {before} line(s) replaced by {after}. The previous \
         content is gone from disk (recoverable via code/undo).\n"
    );
    // A large shrink is the destructive-clobber shape specifically: a file reconstructed
    // from memory instead of edited in place. Name it, and show what survives, so the next
    // turn perceives the loss instead of reading "success".
    if after * 2 < before {
        out.push_str(
            "This REMOVED most of the file. If you meant to change part of it, code/undo \
             restores it and code/edit changes a region without rewriting the whole file.\n",
        );
    }
    out.push_str("The file now reads:\n");
    out.push_str(&numbered_neighborhood(
        new_content,
        1,
        APPLIED_CONTEXT_RADIUS * 2,
    ));
    Some(out)
}

/// Do these bytes look like `code/read` OUTPUT rather than file CONTENT?
///
/// The hazard that arrives with numbered reads: read a file, hand the numbered text
/// straight back to `code/write`, and every line is now prefixed with a gutter that was
/// never in the source. The write succeeds, the file is corrupt, and the receipt would
/// otherwise say `success: true`. This is the read→write round trip, and it is a
/// predictable consequence of the numbering — so it ships WITH the numbering, not after
/// the first time it destroys a file.
///
/// Reports; never refuses. A file that genuinely contains a numbered gutter (a captured
/// listing, a diff fixture) is legitimate content, and the caller is the one who knows
/// which this is. Requires a strong majority so ordinary code with a few `123 | x` lines
/// stays quiet, and a floor of 3 lines so a two-line snippet can't trip it.
fn looks_like_numbered_read(content: &str) -> bool {
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() < 3 {
        return false;
    }
    let gutters = lines
        .iter()
        .filter(|l| {
            // `   123 | text` — digits, then " | ", exactly the shape read/edit emit.
            let t = l.trim_start();
            let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
            !digits.is_empty() && t[digits.len()..].starts_with(" | ")
        })
        .count();
    gutters * 10 >= lines.len() * 9
}

/// The REFUSAL-path sibling of [`numbered_paste_notice`]. Same detection, opposite
/// consequence: here the gate caught it BEFORE writing, so the file is intact and there is
/// nothing to undo. Saying "this file is now corrupt" on this path would be a plain lie, and
/// a diagnostic that lies about the state of the world is worse than none
/// ([[a-probe-that-can-only-fail-is-worse-than-no-probe]]).
fn numbered_paste_refusal(content: &str) -> String {
    if !looks_like_numbered_read(content) {
        return String::new();
    }
    "THE FIX: nearly every line you sent begins with a `NNN | ` line-number gutter. That      gutter is how `code/read` DISPLAYS a file — it is NOT part of the file, and prefixing      every line with it is what made this unparseable (each line ends up indented). Nothing      was written, so the file is fine. Re-send the SAME content with the `NNN | ` prefix      stripped from every line."
        .to_string()
}

/// The notice `looks_like_numbered_read` earns, or empty when the content is clean.
fn numbered_paste_notice(content: &str) -> String {
    if !looks_like_numbered_read(content) {
        return String::new();
    }
    "WARNING: nearly every line of what you wrote begins with a `NNN | ` line-number \
     gutter. That gutter is how code/read DISPLAYS a file — it is not part of the file. \
     If you pasted read output back, this file is now corrupt; code/undo restores it, and \
     writing the lines WITHOUT the `NNN | ` prefixes fixes it.\n"
        .to_string()
}

/// What this edit did to every line number BELOW it — the fact that makes a second
/// line-addressed edit safe.
///
/// A line-addressed edit that changes the line COUNT silently invalidates every line
/// number the caller is holding from an earlier read. Glass-boxed on SWE-bench flask-4045
/// (run 4, 2026-08-04): read the file → `line_range 16..17` replacing 2 lines with 4 →
/// `line_range 14..23` computed against the numbers from the READ. The second edit was a
/// correct, complete replacement block aimed exactly 2 lines short, so it left the tail of
/// the old parameter list orphaned below it and broke the parse. Nothing in the receipt
/// said the map had moved.
///
/// So the receipt says it. The delta is arithmetic, not a guess, and it states a fact about
/// the file rather than an instruction about what to do next
/// ([[no-hardcoded-heuristics-to-steer-cognition]]) — a caller who re-reads and a caller who
/// adjusts by `delta` are both acting on the same true statement. Empty when the line count
/// did not change: an in-place replacement invalidates nothing, and a notice that fires on
/// every edit is a notice nobody reads.
fn line_shift_notice(old_content: &str, new_content: &str, anchor: u32) -> String {
    let before = old_content.lines().count() as i64;
    let after = new_content.lines().count() as i64;
    let delta = after - before;
    if delta == 0 {
        return String::new();
    }
    format!(
        "LINE NUMBERS SHIFTED: this file went from {before} to {after} lines ({delta:+}). \
         Every line below {anchor} now sits {delta:+} from where it was, so any line number \
         taken from an earlier read of this file is stale. Re-read before addressing lines \
         by number again.\n"
    )
}

/// The line an edit landed on, resolved against the content that was ACTUALLY written.
///
/// Every `EditMode` puts text somewhere, so every one of them has a landing site worth
/// showing — not just the ones addressed by number. A content-anchored `SearchReplace`
/// is the safer idiom precisely because the caller does not have to know a line number,
/// but "safer" is not "always right": the search text can match in a place the caller did
/// not mean, and silently. Locating the replacement in the new content answers that.
fn edit_anchor_line(edit_mode: &EditMode, new_content: &str) -> Option<u32> {
    match edit_mode {
        EditMode::LineRange { start_line, .. } => Some(*start_line),
        EditMode::InsertAt { line, .. } => Some(*line),
        // Report the FIRST replacement site. With `all: true` there may be several; the
        // first is the honest representative, and the caller asked for a sweep so the
        // count is not the surprise — placement is.
        EditMode::SearchReplace { replace, .. } => {
            line_of_first_occurrence(new_content, replace.lines().next().unwrap_or(replace))
        }
        // An append always lands at the end; showing the tail proves it joined the file
        // cleanly rather than fusing onto a last line that had no trailing newline.
        EditMode::Append { .. } => {
            let total = new_content.lines().count() as u32;
            (total > 0).then_some(total)
        }
    }
}

/// 1-indexed line of the first line that contains `needle`, or `None` when it does not
/// appear (a replacement that cannot be located is not one we should invent a site for).
fn line_of_first_occurrence(content: &str, needle: &str) -> Option<u32> {
    if needle.is_empty() {
        return None;
    }
    content
        .lines()
        .position(|line| line.contains(needle))
        .map(|idx| idx as u32 + 1)
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
                return Err(FileEngineError::EditFailed(search_miss_report(
                    content, search,
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
        let written = engine.write("src/other.ts", "a\nb\n", None).expect("write");
        assert!(
            written.applied_context.is_none(),
            "creating a NEW file destroys nothing and needs no warning"
        );
    }

    // what this catches: THE measured failure, identical in three SWE-bench runs on the M5,
    // 2026-08-04 — a guard clause placed inside an open `def __init__(`, via insert_at and
    // via search/replace, six edits across twelve acts, with the file read back in between.
    // The numbered-line receipt (59efe1ecd) could not surface it: the closing paren is still
    // there so brackets stay balanced, and six lines of plausible Python say nothing about
    // the grammar. Only a parser does.
    //
    // Each assertion gets its OWN file. The first version of this test appended valid code
    // to the file the first assertion had just broken, then asserted no warning — it failed
    // for that reason and nearly cost a correct implementation.
    #[test]
    fn an_edit_that_would_break_the_parse_is_refused_and_changes_nothing() {
        if std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_err()
        {
            return; // no interpreter here; degrade-silent is the contract
        }
        let (dir, engine) = setup_engine();
        let signature = "def f(\n    a,\n    b,\n):\n    return a\n";
        fs::write(dir.path().join("src/broken.py"), signature).unwrap();
        fs::write(dir.path().join("src/clean.py"), signature).unwrap();

        // The exact shape from the runs: a statement inside the parameter list.
        let broken = engine
            .edit(
                "src/broken.py",
                &EditMode::InsertAt {
                    line: 2,
                    content: "    if '.' in a:\n        raise ValueError(\"no dots\")\n"
                        .to_string(),
                },
                None,
            )
            .expect("a refusal is a RESULT, not a transport error");
        assert!(
            !broken.success,
            "an edit that makes a parsing file unparseable must be refused, not applied"
        );
        let err = broken.error.expect("a refusal must say why");
        assert!(
            err.contains("EDIT REFUSED") && err.contains("UNCHANGED"),
            "the refusal must name itself and state the file was left alone, got:\n{err}"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("src/broken.py")).unwrap(),
            signature,
            "REFUSED means byte-for-byte unchanged — the whole point is that no wreckage \
             is left behind for a later edit to stack on"
        );
        assert!(
            broken.change_id.is_none(),
            "nothing was written, so there must be no change to undo"
        );

        // A valid edit on an INTACT file must stay quiet, or the warning is noise.
        let clean = engine
            .edit(
                "src/clean.py",
                &EditMode::Append {
                    content: "\ndef g():\n    return 1\n".to_string(),
                },
                None,
            )
            .expect("clean edit");
        let ctx = clean.applied_context.expect("append reports its tail");
        assert!(
            !ctx.contains("SYNTAX ERROR"),
            "valid Python must not warn, got:\n{ctx}"
        );
    }

    // what this catches: repair-forward-on-rubble. flask-4045 run 5 (M5, 2026-08-04): an
    // early edit dropped a guard clause into `super().__init__(`'s argument list; four acts
    // later she worked out the right idiom and applied a CORRECT content-anchored fix — on
    // top of the still-present wreckage, so the file never parsed and the task failed. Across
    // five runs she repaired forward every time and never reverted once. Pins the two facts
    // she cannot compute from a SyntaxError alone: whether THIS edit is what broke a
    // previously-working file, and the change_id that reverses it.
    #[test]
    fn an_edit_on_an_already_broken_file_says_the_damage_predates_it() {
        if std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_err()
        {
            return; // no interpreter here; degrade-silent is the contract
        }
        let (dir, engine) = setup_engine();
        // Damage that did NOT come from an edit — the engine refuses those now. This is what a
        // whole-file `code/write` (deliberately ungated) or an outside change leaves behind.
        fs::write(
            dir.path().join("src/first.py"),
            "def f(\n    a,\n    if a:\n):\n    return a\n",
        )
        .unwrap();

        // An edit that neither causes nor fixes the break must say the damage PREDATES it.
        // Claiming otherwise would send her to undo her own innocent change and leave the real
        // damage in place — the trap run 5 fell into.
        let again = engine
            .edit(
                "src/first.py",
                &EditMode::Append {
                    content: "\ndef g():\n    return 1\n".to_string(),
                },
                None,
            )
            .expect("an edit on an already-broken file still applies");
        assert!(
            again.success,
            "the gate only fires when the file PARSED before"
        );
        let ctx = again.applied_context.expect("landing site");
        assert!(
            ctx.contains("STILL BROKEN") && ctx.contains("already here before"),
            "an edit on an already-broken file must say the damage predates it, got:\n{ctx}"
        );
    }

    // what this catches: THE upstream defect behind the whole "statement inside an open
    // paren" family (SWE-bench flask-4045 runs 1, 3, 5 on the M5, 2026-08-04). code/edit
    // addresses lines by NUMBER; code/read returned bare text. She read the whole 128-line
    // file, asked for insert_at line 35, and landed 4 lines off — inside
    // `super().__init__(`'s argument list — because she had to count by hand. Pins that a
    // what this catches: a dead-end error on the path the refusal STEERS her onto. The edit
    // gate tells her to "anchor on the text itself with a search_replace edit"; if that then
    // says only "Search text not found", the redirect lands nowhere. The dominant real cause is
    // invisible in a quoted string — leading whitespace — so the miss must NAME it.
    #[test]
    fn a_search_miss_names_the_indent_mismatch_and_the_line() {
        let content = "def f():\n    if len(univariate) == 1:\n        return 1\n";
        // She reproduced the line without its indent — the classic miss.
        let report = search_miss_report(content, "if len(univariate) == 1:");
        assert!(
            report.contains("INDENT mismatch") && report.contains("line 2"),
            "must localise the line and name the cause, got:\n{report}"
        );
        assert!(
            report.contains("4 space(s)"),
            "must state the actual indent so she can reproduce it, got:\n{report}"
        );
        assert!(
            report.contains("line_range edit on line 2"),
            "and offer the escape that does not depend on reproducing whitespace, got:\n{report}"
        );
    }

    // what this catches: a genuinely absent anchor must still orient her — pointing at the
    // nearest real line is what distinguishes "you mis-remembered" from "the file changed".
    #[test]
    fn a_search_miss_with_no_whitespace_match_points_at_the_nearest_line() {
        let content = "def solve(gens, basis):\n    if len(univariate) == 2:\n        pass\n";
        let report = search_miss_report(content, "if len(univariate) == 99:");
        assert!(
            report.contains("closest line is 2"),
            "must point at something she can look at, got:\n{report}"
        );
        // And an anchor sharing nothing must NOT invent a neighbour.
        let unrelated = search_miss_report(content, "completely unrelated banana text");
        assert!(
            unrelated.contains("Nothing in this file resembles it"),
            "a bogus 'closest' is worse than admitting no match, got:\n{unrelated}"
        );
    }

    // what this catches: the ORPHANED TAIL — a line_range whose end falls inside a construct
    // it does not close. sympy-22005 (M5, 2026-08-04, agent/solve): she asked for lines
    // 240..247 of `polysys.py`, but the block she meant ends at 249 because 248-249 are the
    // tail of a triple-quoted string. Her replacement carried its own `'''))`, so the
    // original's last two lines survived after it and the module stopped parsing. The FIX
    // itself was reasonable — semantics right, extent wrong, score zero.
    //
    // Bracket balance cannot catch this and was falsified for the insert case; only a parser
    // can. The gate is the parser, and refusing keeps the correct-ish attempt recoverable
    // instead of burying it under a syntax error she never reverts.
    // what this catches: the refusal pointing at the WRONG cause. Live on sympy-21379 —
    // she read basic.py, pasted the numbered output straight back as whole-file content, so
    // every line carried the `   12 | ` gutter and Python saw `IndentationError: unexpected
    // indent`. The gate correctly saved the file, then handed her the line-range advice
    // ("widen the range to cover the whole construct") for an edit that used no line range.
    // She spent the remaining 16 of 30 acts chasing that wrong lead and never landed an edit.
    // The gutter detector already existed; it was only wired to the write SUCCESS path.
    // A refusal must name the cause it can actually prove, and must not lie about damage it
    // prevented. [[a-probe-that-can-only-fail-is-worse-than-no-probe]]
    // what this catches: the missing import — valid syntax, guaranteed NameError, invisible
    // to the parse gate. Live on sympy-21379, her FIRST edit to real library source all
    // session, and the entire library change was two lines:
    //     def _subs(self, old, new, **hints):
    //   +     clear_cache()
    // `clear_cache` is imported nowhere in basic.py. Every substitution in sympy raised
    // NameError; passToPass went 40 -> 26, fourteen passing tests destroyed, and the edit
    // reported success: true.
    //
    // Also pins the SAFETY clause, which matters more than the catch: only names the edit
    // INTRODUCED are flagged. A name the old file already mentions is left alone — that is
    // what keeps star-imports and conditional definitions from producing false refusals.
    // what this catches: the docstring that silently stops being one. Same 2-line
    // sympy-21379 edit as the missing-import test — she inserted between `def` and the
    // docstring, so ast.get_docstring(_subs) went None. Valid syntax; the parse gate is blind
    // to it; help() loses the text and any doctests inside it stop being collected (in sympy
    // those run as part of the suite).
    //
    // WARNING not refusal, on purpose: a NameError is a guaranteed crash and earns a refusal,
    // a displaced docstring is degradation the code survives. Severity matches force — a gate
    // that cries wolf is a gate that gets ignored.
    #[test]
    fn an_insert_above_a_docstring_is_reported_as_displacing_it() {
        let dir = tempfile::tempdir().expect("tmp");
        let f = dir.path().join("m.py");
        let old = "def a():\n    \"\"\"Docs.\"\"\"\n    return 1\n";
        let displaced = "def a():\n    x = 1\n    \"\"\"Docs.\"\"\"\n    return 1\n";

        match displaced_docstrings(&f, old, displaced) {
            None => return, // no python3 — inconclusive is not a verdict
            Some(w) => {
                assert!(w.contains("a"), "names the function: {w}");
                assert!(w.contains("docstring"), "says what was lost: {w}");
                assert!(
                    w.contains("AFTER the closing quotes"),
                    "says exactly what to do about it: {w}"
                );
            }
        }

        // Inserting BELOW the docstring is fine — the common, correct edit must stay silent.
        assert!(
            displaced_docstrings(
                &f,
                old,
                "def a():\n    \"\"\"Docs.\"\"\"\n    x = 1\n    return 1\n"
            )
            .is_none(),
            "a correct insert must not warn"
        );
        // A function that never had a docstring is not 'displaced'.
        assert!(
            displaced_docstrings(
                &f,
                "def b():\n    return 1\n",
                "def b():\n    x=1\n    return 1\n"
            )
            .is_none(),
            "no docstring to lose"
        );
    }

    #[test]
    fn an_edit_that_calls_an_unimported_name_is_refused_not_silently_broken() {
        let dir = tempfile::tempdir().expect("tmp");
        let f = dir.path().join("m.py");
        let old = "def a():\n    return 1\n";

        // introduces a call to a name the file has never heard of
        let bad = "def a():\n    clear_cache()\n    return 1\n";
        let hit = introduced_undefined_calls(&f, old, bad);
        match hit {
            None => return, // no python3 here — an inconclusive probe is not a verdict
            Some(names) => assert!(
                names.iter().any(|n| n == "clear_cache"),
                "must name the undefined call: {names:?}"
            ),
        }

        // builtins are fine
        assert!(
            introduced_undefined_calls(&f, old, "def a():\n    return len([1])\n").is_none(),
            "builtins are not undefined names"
        );
        // imported in the same edit → fine
        assert!(
            introduced_undefined_calls(
                &f,
                old,
                "from x import clear_cache\ndef a():\n    clear_cache()\n"
            )
            .is_none(),
            "a name imported by the same edit is bound"
        );
        // ALREADY MENTIONED in the old file → never flagged, even if unbound in the new text
        assert!(
            introduced_undefined_calls(
                &f,
                "# clear_cache lives in sympy.core.cache\ndef a():\n    return 1\n",
                bad
            )
            .is_none(),
            "a name the old file already mentions is out of scope — this is the anti-false-refusal clause"
        );
    }

    #[test]
    fn a_refused_gutter_paste_is_told_about_the_gutter_not_a_line_range() {
        let pasted = "     1 | def f():\n     2 |     return 1\n     3 | \n     4 | x = f()\n";
        let hint = numbered_paste_refusal(pasted);
        assert!(hint.contains("NNN | "), "names the gutter itself: {hint}");
        assert!(hint.contains("strip"), "says what to DO about it: {hint}");
        assert!(
            !hint.contains("undo") && !hint.to_lowercase().contains("now corrupt"),
            "the refusal path SAVED the file — claiming damage would be a lie: {hint}"
        );
        // Ordinary code with a bit-or must not trip it.
        assert!(
            numbered_paste_refusal("fn a() {}\nlet t = 1 | 2;\nfn b() {}\n").is_empty(),
            "real code is not a numbered paste"
        );
    }

    #[test]
    fn a_line_range_ending_inside_a_triple_quoted_string_is_refused() {
        if std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_err()
        {
            return; // no interpreter here; degrade-silent is the contract
        }
        let (dir, engine) = setup_engine();
        // Lines 1..8, with the string closing on 8 — the sympy shape, minus the distance.
        let original = "def solve(gens, basis):\n\
                        \x20   univariate = [b for b in basis]\n\
                        \x20   if len(univariate) == 1:\n\
                        \x20       f = univariate.pop()\n\
                        \x20   else:\n\
                        \x20       raise NotImplementedError('''\n\
                        \x20           only zero-dimensional systems supported\n\
                        \x20           (finite number of solutions)\n\
                        \x20           ''')\n";
        fs::write(dir.path().join("src/polysys.py"), original).unwrap();

        // End at 7 — one line INTO the string. Her replacement closes the quote itself, so
        // lines 8-9 are left dangling exactly as they were on the real run.
        let refused = engine
            .edit(
                "src/polysys.py",
                &EditMode::LineRange {
                    start_line: 3,
                    end_line: 7,
                    new_content: "    if len(univariate) == 1 and len(gens) == 1:\n\
                                  \x20       f = univariate.pop()\n\
                                  \x20   else:\n\
                                  \x20       raise NotImplementedError('''\n\
                                  \x20           only zero-dimensional systems supported\n\
                                  \x20           ''')"
                        .to_string(),
                },
                None,
            )
            .expect("a refusal is a RESULT, not a transport error");

        assert!(!refused.success, "the orphaned tail must not be written");
        assert_eq!(
            fs::read_to_string(dir.path().join("src/polysys.py")).unwrap(),
            original,
            "the file must survive byte-for-byte so her next attempt starts from working code"
        );
        let err = refused.error.expect("a refusal must say why");
        // THE AIM ASSIST: not "widen the range" — the NUMBER. She already made this judgement
        // once and got it wrong; repeating the instruction is not help. The construct closes at
        // line 9, so the refusal must say end_line=9 and confirm the same content then parses.
        assert!(
            err.contains("THE FIX: use end_line=9 instead of 7"),
            "the refusal must compute and name the end line that works, got:\n{err}"
        );
        assert!(
            err.contains("stopped 2 line(s) short"),
            "and quantify how far off she was, got:\n{err}"
        );

        // And the SAME edit with the range widened to cover the whole construct must APPLY —
        // otherwise the gate would just be blocking the fix rather than correcting the aim.
        let ok = engine
            .edit(
                "src/polysys.py",
                &EditMode::LineRange {
                    start_line: 3,
                    end_line: 9,
                    new_content: "    if len(univariate) == 1 and len(gens) == 1:\n\
                                  \x20       f = univariate.pop()\n\
                                  \x20   else:\n\
                                  \x20       raise NotImplementedError('''\n\
                                  \x20           only zero-dimensional systems supported\n\
                                  \x20           ''')"
                        .to_string(),
                },
                None,
            )
            .expect("edit");
        assert!(
            ok.success,
            "a correctly-bounded edit must still apply — the gate rejects breakage, not intent"
        );
        assert!(
            fs::read_to_string(dir.path().join("src/polysys.py"))
                .unwrap()
                .contains("len(gens) == 1"),
            "the widened edit lands the fix"
        );
    }

    // read shows the coordinate system an edit requires, and that the gutter matches
    // `applied_context`'s (`{n:>6} | `), so one file reads the same before and after.
    #[test]
    fn a_read_shows_the_line_numbers_an_edit_addresses() {
        let (dir, engine) = setup_engine();
        fs::write(
            dir.path().join("src/counted.txt"),
            "alpha\nbravo\ncharlie\n",
        )
        .unwrap();

        let whole = engine.read("src/counted.txt", None, None).expect("read");
        let content = whole.content.expect("content");
        assert!(
            content.contains("     1 | alpha") && content.contains("     3 | charlie"),
            "a full read numbers every line, got:\n{content}"
        );

        // A WINDOWED read must number by ABSOLUTE file position, not 1..n of the slice —
        // a relative number is worse than none, because it looks addressable and is not.
        let window = engine
            .read("src/counted.txt", Some(2), Some(3))
            .expect("read");
        let content = window.content.expect("content");
        assert!(
            content.contains("     2 | bravo") && !content.contains("     1 | bravo"),
            "a windowed read numbers from the file's start, not the window's, got:\n{content}"
        );
    }

    // what this catches: the hazard the numbering itself creates. Read a file, hand the
    // numbered text back to code/write, and every line carries a gutter that was never in
    // the source — a corrupt file behind `success: true`. Pins that the write receipt names
    // it, and that ordinary code (which has no gutter) never trips the warning.
    #[test]
    fn writing_read_output_back_verbatim_is_named_in_the_receipt() {
        let (dir, engine) = setup_engine();
        fs::write(dir.path().join("src/round.py"), "a = 1\nb = 2\nc = 3\n").unwrap();
        let numbered = engine
            .read("src/round.py", None, None)
            .expect("read")
            .content
            .expect("content");

        let pasted = engine
            .write("src/round.py", &numbered, None)
            .expect("the write still applies — we report, never refuse");
        let ctx = pasted
            .applied_context
            .expect("an overwrite always reports something");
        assert!(
            ctx.contains("line-number gutter"),
            "pasting read output back must be named as such, got:\n{ctx}"
        );

        // Real source has no gutter — the warning must stay silent or it is noise.
        let clean = engine
            .write("src/round.py", "a = 1\nb = 2\nc = 3\n", None)
            .expect("clean write");
        let ctx = clean.applied_context.expect("overwrite reports magnitude");
        assert!(
            !ctx.contains("line-number gutter"),
            "ordinary source must not trip the gutter warning, got:\n{ctx}"
        );
    }

    // what this catches: the stale-line-number defect that produced the orphaned parameter
    // tail on SWE-bench flask-4045, run 4 (M5, 2026-08-04). She read the file, edited
    // lines 16..17 replacing 2 lines with 4, then addressed lines 14..23 using the numbers
    // from the READ — two short, because the first edit had pushed everything down by 2.
    // Her second block was correct; it just landed against a map that had moved and nothing
    // told her. Pins both directions: a growing edit says how far the lines below it slid,
    // and an edit that does NOT change the line count stays silent so the notice keeps
    // meaning something.
    #[test]
    fn an_edit_that_changes_the_line_count_says_the_numbers_below_it_moved() {
        let (dir, engine) = setup_engine();
        let five = "one\ntwo\nthree\nfour\nfive\n";
        fs::write(dir.path().join("src/grew.txt"), five).unwrap();
        fs::write(dir.path().join("src/same.txt"), five).unwrap();

        let grew = engine
            .edit(
                "src/grew.txt",
                &EditMode::LineRange {
                    start_line: 2,
                    end_line: 3,
                    new_content: "a\nb\nc\nd".to_string(),
                },
                None,
            )
            .expect("edit applies");
        let ctx = grew
            .applied_context
            .expect("a line-addressed edit reports its landing site");
        assert!(
            ctx.contains("LINE NUMBERS SHIFTED") && ctx.contains("+2"),
            "replacing 2 lines with 4 must state the +2 slide of everything below, got:\n{ctx}"
        );

        // Same line count in, same line count out — nothing downstream moved, so a notice
        // here would be noise on every ordinary in-place edit.
        let same = engine
            .edit(
                "src/same.txt",
                &EditMode::LineRange {
                    start_line: 2,
                    end_line: 3,
                    new_content: "a\nb".to_string(),
                },
                None,
            )
            .expect("edit applies");
        let ctx = same.applied_context.expect("landing site");
        assert!(
            !ctx.contains("LINE NUMBERS SHIFTED"),
            "an edit that preserves the line count invalidates nothing, got:\n{ctx}"
        );
    }

    // what this catches: the destructive clobber. On the M5, 2026-08-04, a persona had a
    // CORRECT 2-line fix in src/flask/blueprints.py (128 lines), then called code/write
    // with a 5-line stub reconstructed from memory — imports gone, class hierarchy gone,
    // her own fix gone — and the receipt said `success: true, bytes_written: 214`. She read
    // the file afterward and never repaired it. Of every file verb, whole-file write can
    // destroy the most, and it was the one reporting the least. Pins: an overwrite states
    // what it replaced, a large shrink is called out by name, and creating a new file stays
    // quiet (asserted above) so the warning means something when it appears.
    #[test]
    fn overwriting_an_existing_file_reports_what_it_destroyed() {
        let (_dir, engine) = setup_engine();

        // 3 lines -> 1: the shape that erased a 128-line module.
        let clobbered = engine
            .write("src/main.ts", "just this\n", None)
            .expect("overwrite");
        let ctx = clobbered
            .applied_context
            .expect("an overwrite of real content must report the loss");
        assert!(
            ctx.contains("3 line(s) replaced by 1"),
            "the magnitude of the loss is the fact, got:\n{ctx}"
        );
        assert!(
            ctx.contains("REMOVED most of the file"),
            "a large shrink must be named, not left for the reader to compute, got:\n{ctx}"
        );
        assert!(
            ctx.contains("code/undo"),
            "the receipt must point at the recovery path, got:\n{ctx}"
        );
    }

    /// The flask-4045 shape, minimised: a class whose docstring is long enough that an edit
    /// anchored on prose lands inside the literal. `guard` is the fix she actually derived.
    fn blueprint_module() -> &'static str {
        "class Blueprint:\n    \"\"\"Represents a blueprint.\n\n    :param name: The name of the blueprint.\n    :param import_name: The package name.\n    \"\"\"\n\n    def __init__(self, name):\n        self.name = name\n"
    }

    fn guard() -> &'static str {
        "        if \".\" in name:\n            raise ValueError(\"names may not contain dots\")\n"
    }

    // what this catches (#317, measured 3× on pallets__flask-4045): the write that parses and
    // does nothing. The model derives the correct guard and anchors it on a docstring line, so
    // the code lands INSIDE the class docstring. `ast.parse` is happy, the test suite runs, the
    // guard is prose. The last measured run applied cleanly with all 51 PASS_TO_PASS green and
    // both FAIL_TO_PASS still failing — indistinguishable from the model simply being wrong.
    //
    // Pins the LIVE stance: the edit LANDS (a citizen writes code as text whenever she means to
    // — docstring examples, fixtures, quoted snippets — and that is first-class), and she is
    // TOLD it will not execute. Warning, never a block.
    #[test]
    fn code_written_into_a_docstring_lands_but_says_it_will_not_execute() {
        let (dir, engine) = setup_engine();
        fs::write(dir.path().join("src/bp.py"), blueprint_module()).unwrap();

        let r = engine
            .edit(
                "src/bp.py",
                &EditMode::InsertAt { line: 5, content: guard().to_string() },
                None,
            )
            .expect("a live citizen's edit is never refused for landing in a literal");
        assert!(r.success, "the live path writes it: {:?}", r.error);
        let ctx = r
            .applied_context
            .expect("an edit into a literal must carry a receipt");
        assert!(
            ctx.contains("INSIDE A STRING LITERAL") && ctx.contains("never executes"),
            "she must be told the text is inert, in those words, got:\n{ctx}"
        );
        // It must AFFIRM the legitimate case first — writing about code is first-class, and a
        // notice that reads as a scolding for something she may have meant is a PX defect.
        assert!(
            ctx.contains("nothing to fix"),
            "a citizen who MEANT to write a docstring example must be told she is already \
             correct, got:\n{ctx}"
        );
        // And it must teach the way out with real verbs, not "widen the range".
        for must in ["code/read", "code/edit", "method body"] {
            assert!(
                ctx.contains(must),
                "the notice must walk her through it — missing `{must}`, got:\n{ctx}"
            );
        }
        // And it really is on disk — this is a warning, not a silent refusal.
        assert!(fs::read_to_string(dir.path().join("src/bp.py"))
            .unwrap()
            .contains("raise ValueError"));
    }

    // what this catches: the same edit on a SCORED run, where the deliverable IS an executing
    // patch and she cannot recover from a file she believes she fixed. Refusal is what gives the
    // act→observe circuit something to retry. Pins that the file is left UNTOUCHED (a refusal
    // that half-wrote would be worse than the warning) and that the message AIMS her at the body
    // instead of only diagnosing — the lesson from the sympy-21379 refusal that sent her chasing
    // a line range she wasn't using.
    #[test]
    fn a_scored_run_refuses_the_same_edit_and_leaves_the_file_intact() {
        let (dir, engine) = setup_engine();
        fs::write(dir.path().join("src/bp.py"), blueprint_module()).unwrap();
        let engine = engine.with_write_policy(WritePolicy::RefuseInert);

        let r = engine
            .edit(
                "src/bp.py",
                &EditMode::InsertAt { line: 5, content: guard().to_string() },
                None,
            )
            .expect("a refusal is a result, not an Err");
        assert!(!r.success, "a scored run must not accept an inert patch");
        let err = r.error.expect("a refusal must say why");
        assert!(
            err.contains("EDIT REFUSED") && err.contains("UNCHANGED"),
            "the refusal must state that nothing was written, got:\n{err}"
        );
        // Diagnosing is not enough — it has to TEACH: name the recovery verbs and the anchor
        // that works. The sympy-21379 refusal was correct and still burned 16 of her 30 acts
        // because the advice ("widen the range") was something she had to act on blind.
        for must in ["code/read", "code/edit", "method body", "confirm the behavior"] {
            assert!(
                err.contains(must),
                "the refusal must walk her through it — missing `{must}`, got:\n{err}"
            );
        }
        assert_eq!(
            fs::read_to_string(dir.path().join("src/bp.py")).unwrap(),
            blueprint_module(),
            "a refused edit must leave the file byte-identical"
        );
    }

    // what this catches: the refusal staying NARROW. Hardening a scored run must not turn into
    // "you may not edit Python" — a real fix anchored in the method body is exactly what the
    // benchmark wants, and it must sail through the same strict engine untouched.
    #[test]
    fn the_scored_gate_passes_the_same_guard_when_it_lands_in_the_body() {
        let (dir, engine) = setup_engine();
        fs::write(dir.path().join("src/bp.py"), blueprint_module()).unwrap();
        let engine = engine.with_write_policy(WritePolicy::RefuseInert);

        // Line 8 is `def __init__(self, name):` — inserting after it is the real fix.
        let r = engine
            .edit(
                "src/bp.py",
                &EditMode::InsertAt { line: 9, content: guard().to_string() },
                None,
            )
            .expect("edit");
        assert!(
            r.success,
            "the correct fix must not be collateral damage: {:?}",
            r.error
        );
        assert!(fs::read_to_string(dir.path().join("src/bp.py"))
            .unwrap()
            .contains("raise ValueError"));
    }

    // what this catches: the content-anchored blind spot. A search/replace is the SAFER
    // idiom — the caller never has to know a line number — but the search text can still
    // match somewhere they did not mean, silently. Reporting only line-addressed edits
    // would leave exactly the idiom a careful model reaches for as the one with no
    // feedback. Pins: a replacement is located in the written content, and an append
    // reports the tail (proving it joined the file rather than fusing onto the last line).
    #[test]
    fn content_anchored_and_append_edits_report_their_landing_site_too() {
        let (_dir, engine) = setup_engine();

        let replaced = engine
            .edit(
                "src/main.ts",
                &EditMode::SearchReplace {
                    search: "line 2".to_string(),
                    replace: "REPLACED".to_string(),
                    all: false,
                },
                None,
            )
            .expect("search/replace");
        let ctx = replaced
            .applied_context
            .expect("a replacement has a location even without a line number");
        assert!(
            ctx.contains(">    2 | REPLACED"),
            "the replacement must be located and marked in the written content, got:\n{ctx}"
        );

        let appended = engine
            .edit(
                "src/main.ts",
                &EditMode::Append {
                    content: "TAIL\n".to_string(),
                },
                None,
            )
            .expect("append");
        let ctx = appended
            .applied_context
            .expect("an append lands at the end");
        assert!(
            ctx.contains("TAIL"),
            "an append must show the tail it produced, got:\n{ctx}"
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
        // Numbered by ABSOLUTE file position — a read must show the coordinate system
        // code/edit addresses, and a windowed read must not renumber from 1.
        assert_eq!(result.content.unwrap(), "     2 | line 2");
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

        let content = engine
            .read("src/main.ts", None, None)
            .unwrap()
            .content
            .unwrap();
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

        let content = engine
            .read("src/main.ts", None, None)
            .unwrap()
            .content
            .unwrap();
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

        // Assert on the FILE, not on code/read's rendering — this is an edit test, and
        // coupling it to how the read tool displays lines is what made it fail when the
        // display gained line numbers.
        let content = fs::read_to_string(_dir.path().join("src/main.ts")).unwrap();
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

        // Assert on the FILE, not on code/read's rendering — see the sibling note in
        // insert_at_past_eof_appends.
        let content = fs::read_to_string(_dir.path().join("src/main.ts")).unwrap();
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

        // what this catches: the most destructive verb reporting the least. Delete returned
        // `success: true, bytes_written: 0` and an empty applied_context — indistinguishable
        // from a no-op in a receipt. Same reasoning error that left code/write silent about
        // overwrites (4e74d93ce). Pins that a delete states the size of what it removed and
        // carries the undo handle.
        let ctx = result
            .applied_context
            .expect("a delete reports what it destroyed");
        let change_id = result.change_id.expect("a delete records a change");
        assert!(
            ctx.contains("DELETED") && ctx.contains("line(s)") && ctx.contains(&change_id),
            "a delete must name the file, its size, and the undo handle, got:\n{ctx}"
        );
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
        let names: Vec<&str> = with_hidden
            .entries
            .iter()
            .map(|e| e.name.as_str())
            .collect();
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
        let miss = engine
            .resolve_dir("does/not/exist")
            .unwrap_err()
            .to_string();
        assert!(miss.contains("path not found"), "{miss}");
        assert!(
            miss.contains("workspace root itself IS explorable"),
            "{miss}"
        );
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
            assert_eq!(
                list.total_count, 10,
                "list result must be stable across concurrent reads"
            );
            // glob: always returns the 10 src files
            assert_eq!(
                glob.total_matches, 10,
                "glob must return all 10 matches regardless of concurrent siblings"
            );
        }
    }
}
