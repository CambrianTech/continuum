//! Diff engine — unified diff computation using the `similar` crate.
//!
//! Generates forward and reverse diffs for file operations,
//! enabling the change graph's undo/redo capability.

use similar::{ChangeTag, DiffTag, TextDiff};

use super::types::{DiffHunk, FileDiff};

/// Compute a FileDiff between two strings.
/// Returns both the unified text representation and structured hunks.
pub fn compute_diff(old: &str, new: &str, file_path: &str) -> FileDiff {
    let diff = TextDiff::from_lines(old, new);

    // Generate unified diff text
    let unified = diff
        .unified_diff()
        .context_radius(3)
        .header(&format!("a/{}", file_path), &format!("b/{}", file_path))
        .to_string();

    // Build structured hunks
    let hunks = build_hunks(&diff);

    FileDiff { unified, hunks }
}

/// Compute forward (old→new) and reverse (new→old) diffs simultaneously.
pub fn compute_bidirectional_diff(
    old: &str,
    new: &str,
    file_path: &str,
) -> (FileDiff, FileDiff) {
    let forward = compute_diff(old, new, file_path);
    let reverse = compute_diff(new, old, file_path);
    (forward, reverse)
}

/// Build structured DiffHunks from a TextDiff.
fn build_hunks(diff: &TextDiff<'_, '_, '_, str>) -> Vec<DiffHunk> {
    let mut hunks = Vec::new();

    for group in diff.grouped_ops(3) {
        let mut old_start = 0u32;
        let mut new_start = 0u32;
        let mut old_count = 0u32;
        let mut new_count = 0u32;
        let mut content = String::new();

        for op in &group {
            if old_start == 0 && new_start == 0 {
                old_start = op.old_range().start as u32 + 1;
                new_start = op.new_range().start as u32 + 1;
            }

            match op.tag() {
                DiffTag::Equal => {
                    let count = op.old_range().len() as u32;
                    old_count += count;
                    new_count += count;
                    for value in diff.iter_changes(op) {
                        content.push(' ');
                        content.push_str(value.as_str().unwrap_or(""));
                    }
                }
                DiffTag::Delete => {
                    let count = op.old_range().len() as u32;
                    old_count += count;
                    for value in diff.iter_changes(op) {
                        content.push('-');
                        content.push_str(value.as_str().unwrap_or(""));
                    }
                }
                DiffTag::Insert => {
                    let count = op.new_range().len() as u32;
                    new_count += count;
                    for value in diff.iter_changes(op) {
                        content.push('+');
                        content.push_str(value.as_str().unwrap_or(""));
                    }
                }
                DiffTag::Replace => {
                    // Replace = Delete + Insert combined
                    old_count += op.old_range().len() as u32;
                    new_count += op.new_range().len() as u32;
                    for value in diff.iter_changes(op) {
                        match value.tag() {
                            ChangeTag::Delete => {
                                content.push('-');
                                content.push_str(value.as_str().unwrap_or(""));
                            }
                            ChangeTag::Insert => {
                                content.push('+');
                                content.push_str(value.as_str().unwrap_or(""));
                            }
                            ChangeTag::Equal => {
                                content.push(' ');
                                content.push_str(value.as_str().unwrap_or(""));
                            }
                        }
                    }
                }
            }
        }

        if !content.is_empty() {
            hunks.push(DiffHunk {
                old_start,
                old_count,
                new_start,
                new_count,
                content,
            });
        }
    }

    hunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_diff_no_changes() {
        let text = "line 1\nline 2\nline 3\n";
        let diff = compute_diff(text, text, "test.ts");
        assert!(diff.unified.is_empty() || !diff.unified.contains('-'));
        assert!(diff.hunks.is_empty());
    }

    #[test]
    fn test_compute_diff_simple_edit() {
        let old = "line 1\nline 2\nline 3\n";
        let new = "line 1\nline 2 modified\nline 3\n";
        let diff = compute_diff(old, new, "test.ts");
        assert!(diff.unified.contains("-line 2"));
        assert!(diff.unified.contains("+line 2 modified"));
        assert!(!diff.hunks.is_empty());
    }

    #[test]
    fn test_compute_diff_add_lines() {
        let old = "line 1\nline 3\n";
        let new = "line 1\nline 2\nline 3\n";
        let diff = compute_diff(old, new, "test.ts");
        assert!(diff.unified.contains("+line 2"));
    }

    #[test]
    fn test_compute_diff_delete_lines() {
        let old = "line 1\nline 2\nline 3\n";
        let new = "line 1\nline 3\n";
        let diff = compute_diff(old, new, "test.ts");
        assert!(diff.unified.contains("-line 2"));
    }

    #[test]
    fn test_bidirectional_diff() {
        let old = "hello\nworld\n";
        let new = "hello\nrust\n";
        let (forward, reverse) = compute_bidirectional_diff(old, new, "test.ts");
        assert!(forward.unified.contains("-world"));
        assert!(forward.unified.contains("+rust"));
        assert!(reverse.unified.contains("-rust"));
        assert!(reverse.unified.contains("+world"));
    }

    #[test]
    fn test_create_file_diff() {
        let (forward, reverse) = compute_bidirectional_diff("", "new content\n", "test.ts");
        assert!(forward.unified.contains("+new content"));
        assert!(reverse.unified.contains("-new content"));
    }
}
