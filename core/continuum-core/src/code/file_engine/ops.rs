//! Reads, writes, edits, deletes, diffs, undo and history.
use super::*;

impl FileEngine {
    /// Read a file, optionally a range of lines (1-indexed, inclusive), inside the sandbox.
    pub fn read(
        &self,
        relative_path: &str,
        start_line: Option<u32>,
        end_line: Option<u32>,
    ) -> Result<ReadResult, FileEngineError> {
        self.read_in(relative_path, start_line, end_line, ReadScope::Sandbox)
    }

    /// [`Self::read`] with the reach `scope` grants (see [`ReadScope`]).
    pub fn read_in(
        &self,
        relative_path: &str,
        start_line: Option<u32>,
        end_line: Option<u32>,
        scope: ReadScope,
    ) -> Result<ReadResult, FileEngineError> {
        let cleaned = self.path_arg(relative_path)?;
        let relative_path = cleaned.path.as_str();
        let abs_path = self.security.validate_read_in(relative_path, scope)?;

        if !abs_path.exists() {
            return Err(self.not_found(relative_path));
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
            .map(|(i, line)| numbered_line(i as u32 + 1, line))
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
        self.write_in(relative_path, content, description, &crate::code::path_security::ReadScope::Sandbox)
    }

    /// [`Self::write`] under the caller's scope: an unconfined caller may write an absolute
    /// path outside the workspace ([`PathSecurity::validate_write_in`]).
    pub fn write_in(
        &self,
        relative_path: &str,
        content: &str,
        description: Option<&str>,
        scope: &crate::code::path_security::ReadScope,
    ) -> Result<WriteResult, FileEngineError> {
        let cleaned = self.path_arg(relative_path)?;
        let relative_path = cleaned.path.as_str();
        let abs_path = self.security.validate_write_in(relative_path, scope)?;
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
        // The workspace changed: tell its observers from the write itself (card f860e59c).
        crate::code::workspace_events::note_written(&abs_path);

        // Record in change graph
        let node = ChangeNode {
            id: Uuid::new_v4(),
            parent_ids: self.latest_parent(relative_path),
            author_id: self.persona_id.clone(),
            timestamp: EpochMs::now().millis(),
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
                // The cleaning receipt LEADS: a write that landed somewhere other than the
                // literal argument must say so before anything about the content.
                let mut out = cleaned.note();
                out.push_str(&numbered_paste_notice(content));
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
        self.edit_in(relative_path, edit_mode, description, &crate::code::path_security::ReadScope::Sandbox)
    }

    /// [`Self::edit`] under the caller's scope (see [`Self::write_in`]).
    pub fn edit_in(
        &self,
        relative_path: &str,
        edit_mode: &EditMode,
        description: Option<&str>,
        scope: &crate::code::path_security::ReadScope,
    ) -> Result<WriteResult, FileEngineError> {
        let cleaned = self.path_arg(relative_path)?;
        let relative_path = cleaned.path.as_str();
        let abs_path = self.security.validate_write_in(relative_path, scope)?;

        if !abs_path.exists() {
            return Err(self.not_found(relative_path));
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
            // SCALE fact (glass-boxed on sympy-22840 n7b, 2026-08-08): Benchy twice proposed
            // replacing ~700 lines of cse_main.py with a 16-line rewrite of `cse()` that
            // referenced helpers the file doesn't have — the gate refused (correctly), but
            // nothing told her the SHAPE was wrong, so she tried the same wholesale rewrite
            // again. The tool can PROVE the removal size, so state it: a bug fix is rarely
            // a mass deletion. A fact about the edit, not a steer — same doctrine as the
            // repair hint below.
            let removed = old_content
                .lines()
                .count()
                .saturating_sub(new_content.lines().count());
            let scale_note = if removed >= 50 {
                format!(
                    "\nSCALE: this edit REMOVES ~{removed} lines of {relative_path}. Bug fixes \
                     are usually a few lines — replace only the exact function or lines at \
                     fault (read them first, then use a search_replace on that block), not a \
                     large region.\n"
                )
            } else {
                String::new()
            };
            return Ok(WriteResult {
                success: false,
                change_id: None,
                file_path: relative_path.to_string(),
                bytes_written: 0,
                error: Some(format!(
                    "EDIT REFUSED — it would have made {relative_path} unparseable. The file \
                     is UNCHANGED on disk; nothing was written and there is nothing to undo.\n\
                     {}{}\n{}",
                    scale_note,
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
        crate::code::workspace_events::note_written(&abs_path);

        // Record in change graph
        let node = ChangeNode {
            id: Uuid::new_v4(),
            parent_ids: self.latest_parent(relative_path),
            author_id: self.persona_id.clone(),
            timestamp: EpochMs::now().millis(),
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
            applied_context: {
                // The cleaning receipt LEADS (card 1daffbaf); the landing-site report follows
                // only when the edit has an anchor line to report from.
                let mut out = cleaned.note();
                if let Some(anchor) = edit_anchor_line(edit_mode, &new_content) {
                    out.push_str(&syntax_error_after_edit(&abs_path).unwrap_or_default());
                    // A displaced docstring parses clean, so nothing else here would mention it.
                    out.push_str(
                        &displaced_docstrings(&abs_path, &old_content, &new_content)
                            .unwrap_or_default(),
                    );
                    // Neither does code written INTO a literal — the quietest failure of all (#317).
                    out.push_str(
                        &inert_insertions(&abs_path, &old_content, &new_content)
                            .unwrap_or_default(),
                    );
                    out.push_str(&line_shift_notice(&old_content, &new_content, anchor));
                    out.push_str(&numbered_neighborhood(
                        &new_content,
                        anchor,
                        APPLIED_CONTEXT_RADIUS,
                    ));
                }
                (!out.is_empty()).then_some(out)
            },
        })
    }

    /// Delete a file. Records a ChangeNode with the full content as reverse diff.
    pub fn delete(
        &self,
        relative_path: &str,
        description: Option<&str>,
    ) -> Result<WriteResult, FileEngineError> {
        let cleaned = self.path_arg(relative_path)?;
        let relative_path = cleaned.path.as_str();
        let abs_path = self.security.validate_write(relative_path)?;

        if !abs_path.exists() {
            return Err(self.not_found(relative_path));
        }

        let old_content = fs::read_to_string(&abs_path)?;

        // Compute diffs (new content is empty for delete)
        let (forward_diff, reverse_diff) =
            compute_bidirectional_diff(&old_content, "", relative_path);

        // Delete the file
        fs::remove_file(&abs_path)?;
        crate::code::workspace_events::note_written(&abs_path);

        // Record in change graph
        let node = ChangeNode {
            id: Uuid::new_v4(),
            parent_ids: self.latest_parent(relative_path),
            author_id: self.persona_id.clone(),
            timestamp: EpochMs::now().millis(),
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
                "{}DELETED {} — {} line(s), {} byte(s) removed from disk. `code/undo` with \
                 change_id={} restores the file exactly.\n",
                cleaned.note(),
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
            return Err(self.not_found(relative_path));
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
        // Every undo arm rewrote (or removed) the one restored file. `undo_last` folds
        // through here, so each of its undos announces itself too.
        crate::code::workspace_events::note_written(&abs_path);

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

}
