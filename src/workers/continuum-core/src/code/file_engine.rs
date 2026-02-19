//! File Engine — per-persona file operations with change tracking.
//!
//! Every write/edit/delete operation creates a ChangeNode in the change graph,
//! enabling undo at any point. Reads are side-effect-free.
//!
//! Thread safety: Each persona gets their own FileEngine instance.
//! The underlying ChangeGraph and PathSecurity handle concurrency.

use std::fs;
use std::path::PathBuf;

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
        Self::Security(e)
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
        self.security.validate_size(relative_path, content.len() as u64)?;

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

        self.security.validate_size(relative_path, new_content.len() as u64)?;

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
        let (reverse_diff, file_path) = self
            .graph
            .reverse_diff_for(change_id)
            .ok_or_else(|| FileEngineError::EditFailed(format!("Change {} not found", change_id)))?;

        // Read current file content
        let abs_path = self.security.validate_write(&file_path)?;
        let current_content = if abs_path.exists() {
            fs::read_to_string(&abs_path)?
        } else {
            String::new()
        };

        // The reverse diff's unified text tells us what to apply.
        // For a proper undo, we use the stored old content from the original node.
        let original_node = self
            .graph
            .get(change_id)
            .ok_or_else(|| FileEngineError::EditFailed(format!("Change {} not found", change_id)))?;

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
            .ok_or_else(|| FileEngineError::EditFailed(format!("Change {} not found for undo", change_id)))?;

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
            error: if !is_latest {
                Some("Warning: undone change was not the latest; result may have conflicts".to_string())
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

    /// Get the latest parent ID for a file (for DAG edges).
    fn latest_parent(&self, file_path: &str) -> Vec<Uuid> {
        self.graph
            .latest_for_file(file_path)
            .map(|n| vec![n.id])
            .unwrap_or_default()
    }
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
            if *end_line < *start_line || *end_line > total {
                return Err(FileEngineError::EditFailed(format!(
                    "end_line {} out of range ({}-{})",
                    end_line, start_line, total
                )));
            }

            let start_idx = (*start_line - 1) as usize;
            let end_idx = *end_line as usize;

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

        EditMode::InsertAt { line, content: new_content } => {
            let lines: Vec<&str> = content.lines().collect();
            let total = lines.len() as u32;

            if *line == 0 || *line > total + 1 {
                return Err(FileEngineError::EditFailed(format!(
                    "Insert line {} out of range (1-{})",
                    line,
                    total + 1
                )));
            }

            let insert_idx = (*line - 1) as usize;
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

        EditMode::Append { content: new_content } => {
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

    fn setup_engine() -> (tempfile::TempDir, FileEngine) {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(
            dir.path().join("src/main.ts"),
            "line 1\nline 2\nline 3\n",
        )
        .unwrap();

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
            .write("src/new.ts", "export const x = 1;\n", Some("Create new file"))
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

    #[test]
    fn test_write_blocked_extension() {
        let (_dir, engine) = setup_engine();
        let result = engine.write("src/malware.exe", "bad", None);
        assert!(result.is_err());
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
}
