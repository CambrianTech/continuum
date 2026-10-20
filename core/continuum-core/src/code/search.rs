//! Code Search — regex + glob file search with .gitignore awareness.
//!
//! Uses the `ignore` crate (from ripgrep) for .gitignore-aware file walking,
//! and standard regex for content matching.

use std::path::Path;

use super::types::{SearchMatch, SearchResult};

/// Search for a regex pattern across files in a directory.
///
/// Respects .gitignore rules and supports file glob filtering.
/// Returns EVERY match and the true `total_matches`: how many of them a caller may see is the
/// caller's share to decide (the command's explicit cap, then the executor's whole-or-handle
/// custody), never this scan's. It used to stop at a cap and report the count it had reached
/// as the total, a lower bound passed off as the answer.
pub fn search_files(root: &Path, pattern: &str, file_glob: Option<&str>) -> SearchResult {
    let regex = match regex::Regex::new(pattern) {
        Ok(r) => r,
        Err(e) => {
            return SearchResult {
                success: false,
                matches: Vec::new(),
                total_matches: 0,
                files_searched: 0,
                error: Some(format!("Invalid regex: {}", e)),
            };
        }
    };

    // Build the file walker with .gitignore awareness
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .hidden(true) // Skip hidden files
        .git_ignore(true) // Respect .gitignore
        .git_global(true) // Respect global gitignore
        .git_exclude(true); // Respect .git/info/exclude

    // Apply file glob filter if provided
    if let Some(glob) = file_glob {
        // The ignore crate uses overrides for glob filtering
        let mut overrides = ignore::overrides::OverrideBuilder::new(root);
        if let Err(e) = overrides.add(glob) {
            return SearchResult {
                success: false,
                matches: Vec::new(),
                total_matches: 0,
                files_searched: 0,
                error: Some(format!("Invalid glob pattern '{}': {}", glob, e)),
            };
        }
        match overrides.build() {
            Ok(ov) => {
                builder.overrides(ov);
            }
            Err(e) => {
                return SearchResult {
                    success: false,
                    matches: Vec::new(),
                    total_matches: 0,
                    files_searched: 0,
                    error: Some(format!("Invalid glob pattern: {}", e)),
                };
            }
        }
    }

    let mut matches = Vec::new();
    let mut files_searched = 0u32;
    let mut total_matches = 0u32;

    for entry in builder.build().flatten() {
        let path = entry.path();

        // Skip directories
        if path.is_dir() {
            continue;
        }

        // Skip binary files (simple heuristic: try reading as UTF-8)
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue, // Skip files we can't read as text
        };

        files_searched += 1;

        // Search for matches in each line
        for (line_idx, line) in content.lines().enumerate() {
            for mat in regex.find_iter(line) {
                total_matches += 1;

                let relative_path = path
                    .strip_prefix(root)
                    .unwrap_or(path)
                    .display()
                    .to_string();

                matches.push(SearchMatch {
                    file_path: relative_path,
                    line_number: (line_idx + 1) as u32,
                    line_content: line.to_string(),
                    match_start: mat.start() as u32,
                    match_end: mat.end() as u32,
                });
            }
        }
    }

    SearchResult {
        success: true,
        matches,
        total_matches,
        files_searched,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_search_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(
            dir.path().join("src/main.ts"),
            "function hello() {\n  console.log('world');\n}\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("src/utils.ts"),
            "export function greet(name: string) {\n  return `Hello ${name}`;\n}\n",
        )
        .unwrap();
        fs::write(dir.path().join("src/style.css"), "body { color: red; }\n").unwrap();
        fs::write(dir.path().join("readme.md"), "# Hello World\n").unwrap();
        dir
    }

    #[test]
    fn test_search_basic() {
        let dir = setup_search_dir();
        let result = search_files(dir.path(), "function", None);
        assert!(result.success);
        assert_eq!(result.total_matches, 2); // hello() and greet()
        assert!(result.files_searched >= 2);
    }

    #[test]
    fn test_search_with_glob() {
        let dir = setup_search_dir();
        let result = search_files(dir.path(), "function", Some("*.ts"));
        assert!(result.success);
        assert_eq!(result.total_matches, 2);
        // All matches should be .ts files
        for m in &result.matches {
            assert!(m.file_path.ends_with(".ts"));
        }
    }

    // what this catches: the scan stopped at the caller's cap and reported the count it had
    // reached as `total_matches` (a lower bound posing as a total). Every match is returned
    // and counted; the cap is applied by the command, after the true total is known.
    #[test]
    fn test_search_reports_the_true_total() {
        let dir = setup_search_dir();
        for i in 0..50 {
            fs::write(dir.path().join(format!("many{i}.txt")), "needle\nneedle\n").unwrap();
        }
        let result = search_files(dir.path(), "needle", None);
        assert!(result.success);
        assert_eq!(result.total_matches, 100);
        assert_eq!(result.matches.len(), 100);
    }

    #[test]
    fn test_search_no_matches() {
        let dir = setup_search_dir();
        let result = search_files(dir.path(), "zzz_nonexistent_zzz", None);
        assert!(result.success);
        assert_eq!(result.total_matches, 0);
        assert!(result.matches.is_empty());
    }

    #[test]
    fn test_search_regex() {
        let dir = setup_search_dir();
        let result = search_files(dir.path(), r"function\s+\w+", None);
        assert!(result.success);
        assert_eq!(result.total_matches, 2);
    }

    #[test]
    fn test_search_invalid_regex() {
        let dir = setup_search_dir();
        let result = search_files(dir.path(), "[invalid", None);
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_search_match_positions() {
        let dir = setup_search_dir();
        let result = search_files(dir.path(), "hello", None);
        assert!(result.success);
        for m in &result.matches {
            assert!(m.match_start < m.match_end);
            assert_eq!(
                &m.line_content[m.match_start as usize..m.match_end as usize],
                "hello"
            );
        }
    }
}
