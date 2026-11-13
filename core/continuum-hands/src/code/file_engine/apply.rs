//! Applying an edit, and reversing one.
use super::*;

/// Apply an EditMode to file content, producing the new content.
pub(super) fn apply_edit(content: &str, edit_mode: &EditMode) -> Result<String, FileEngineError> {
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
                // Indentation-tolerant fallback: the dominant miss cause is a model
                // reproducing lines from a numbered read with the indent dropped or
                // normalised (see `search_miss_report`). When the search block matches
                // exactly ONE contiguous run of file lines once per-line leading
                // whitespace is ignored, the intent is unambiguous — apply it, carrying
                // the FILE's real indentation onto the replacement. Glass-boxed live
                // 2026-08-08 (benchy-sympy-22840-n4): a citizen found the exact fix
                // site, wrote the right 3-line anchor with 0-indent, and the edit died
                // "nothing resembles it" — she then burned her act budget recovering.
                // Safety: ambiguous (2+) normalized matches still refuse with line
                // numbers, and the parse-refusal gate downstream still rejects an edit
                // that would break a parsing file.
                if let Some(result) = indent_tolerant_replace(content, search, replace)? {
                    return Ok(result);
                }
                return Err(FileEngineError::EditFailed(search_miss_report(
                    content, search,
                )));
            }

            // AN AMBIGUOUS EXACT ANCHOR IS A GUESS TOO (2026-09-21, Sahar on
            // pydata__xarray-7393): `replacen(.., 1)` took the FIRST of several verbatim
            // occurrences, so a 7-line hunk meant for `stack` landed inside
            // `reset_index`, where `product_vars` is undefined — a correct edit at the
            // wrong site, applied silently. The indent-tolerant path below already
            // refuses 2+ candidates with their line numbers; the exact path now holds
            // the same law. `all: true` is the explicit "every site" and stays.
            if !*all {
                let sites: Vec<usize> = content
                    .match_indices(search.as_str())
                    .map(|(at, _)| content[..at].matches('\n').count() + 1)
                    .collect();
                if sites.len() > 1 {
                    let shown: Vec<String> = sites.iter().take(5).map(|l| l.to_string()).collect();
                    return Err(FileEngineError::EditFailed(format!(
                        "SEARCH TEXT matches {} locations verbatim (starting at lines {}). \
                         Applying to the first would be a guess — Sahar's stack hunk landed in \
                         reset_index this way. Include more surrounding lines so the anchor is \
                         unique, address the site by number with a line_range edit, or pass \
                         all=true to change every occurrence on purpose.",
                        sites.len(),
                        shown.join(", ")
                    )));
                }
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
pub(super) fn apply_reverse_simple(current: &str, reverse_diff: &FileDiff) -> Option<String> {
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
pub(super) fn extract_added_content(diff: &FileDiff) -> String {
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
