//! Directory Tree — recursive directory structure generation.
//!
//! Generates a tree representation of a directory, respecting .gitignore
//! rules and supporting depth limits.

use std::fs;
use std::path::Path;

use super::types::{TreeNode, TreeResult};

/// Generate a directory tree starting from `root`.
///
/// Respects .gitignore, skips hidden files by default,
/// and limits depth to prevent runaway recursion.
pub fn generate_tree(root: &Path, max_depth: u32, include_hidden: bool) -> TreeResult {
    if !root.exists() || !root.is_dir() {
        // HONEST, ACTIONABLE error — the flat "Not a directory" read as "the whole
        // workspace is unexplorable" and a persona GAVE UP (glass-boxed 2026-07-14:
        // Anwen). Distinguish the two real cases and point at what DOES work, so the
        // mind re-orients instead of concluding it is stuck. [[fallbacks-are-illegal-fail-loud]]
        let error = if !root.exists() {
            format!(
                "path not found: {} — nothing exists at that path. This is about the \
                 PATH, not the workspace: the workspace root itself IS explorable — call \
                 code/tree with NO `path` for the whole tree, then pass a path you see in \
                 that output. Paths are relative to the workspace root (do not prefix them \
                 with 'workspace/').",
                root.display()
            )
        } else {
            format!(
                "{} is a FILE, not a directory — code/tree lists directories. To read this \
                 file use code/read; to see what's around it, code/tree on its parent \
                 directory.",
                root.display()
            )
        };
        return TreeResult {
            success: false,
            root: None,
            total_files: 0,
            total_directories: 0,
            error: Some(error),
        };
    }

    let mut total_files = 0u32;
    let mut total_directories = 0u32;

    let tree = build_tree_node(
        root,
        root,
        0,
        max_depth,
        include_hidden,
        &mut total_files,
        &mut total_directories,
    );

    TreeResult {
        success: true,
        root: tree,
        total_files,
        total_directories,
        error: None,
    }
}

/// Recursively build a TreeNode for a directory entry.
fn build_tree_node(
    entry_path: &Path,
    root: &Path,
    current_depth: u32,
    max_depth: u32,
    include_hidden: bool,
    total_files: &mut u32,
    total_directories: &mut u32,
) -> Option<TreeNode> {
    let name = entry_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| entry_path.display().to_string());

    // Skip hidden files/directories unless requested
    if !include_hidden && name.starts_with('.') && entry_path != root {
        return None;
    }

    let relative_path = entry_path
        .strip_prefix(root)
        .unwrap_or(entry_path)
        .display()
        .to_string();

    if entry_path.is_dir() {
        *total_directories += 1;

        let mut children = Vec::new();

        if current_depth < max_depth {
            // Read directory entries
            if let Ok(entries) = fs::read_dir(entry_path) {
                let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
                // Sort entries: directories first, then alphabetically
                entries.sort_by(|a, b| {
                    let a_is_dir = a.path().is_dir();
                    let b_is_dir = b.path().is_dir();
                    match (a_is_dir, b_is_dir) {
                        (true, false) => std::cmp::Ordering::Less,
                        (false, true) => std::cmp::Ordering::Greater,
                        _ => a.file_name().cmp(&b.file_name()),
                    }
                });

                for entry in entries {
                    let entry_name = entry.file_name().to_string_lossy().to_string();

                    // Skip common ignored directories
                    if entry.path().is_dir() && is_ignored_dir(&entry_name) {
                        continue;
                    }

                    if let Some(child) = build_tree_node(
                        &entry.path(),
                        root,
                        current_depth + 1,
                        max_depth,
                        include_hidden,
                        total_files,
                        total_directories,
                    ) {
                        children.push(child);
                    }
                }
            }
        }

        Some(TreeNode {
            name,
            path: if relative_path.is_empty() {
                ".".to_string()
            } else {
                relative_path
            },
            is_directory: true,
            size_bytes: None,
            children,
        })
    } else {
        *total_files += 1;

        let size_bytes = fs::metadata(entry_path).map(|m| m.len()).ok();

        Some(TreeNode {
            name,
            path: relative_path,
            is_directory: false,
            size_bytes,
            children: Vec::new(),
        })
    }
}

/// Common directories to skip in tree generation.
fn is_ignored_dir(name: &str) -> bool {
    matches!(
        name,
        "node_modules"
            | ".git"
            | "target"
            | "dist"
            | "build"
            | ".next"
            | ".nuxt"
            | ".cache"
            | "__pycache__"
            | ".tsbuildinfo"
            | "coverage"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_tree_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src/components")).unwrap();
        fs::create_dir_all(dir.path().join("src/utils")).unwrap();
        fs::create_dir_all(dir.path().join("tests")).unwrap();
        fs::write(dir.path().join("src/main.ts"), "main").unwrap();
        fs::write(dir.path().join("src/components/App.tsx"), "app").unwrap();
        fs::write(dir.path().join("src/utils/helpers.ts"), "helpers").unwrap();
        fs::write(dir.path().join("tests/main.test.ts"), "test").unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        dir
    }

    #[test]
    fn test_generate_tree() {
        let dir = setup_tree_dir();
        let result = generate_tree(dir.path(), 10, false);
        assert!(result.success);
        assert!(result.root.is_some());
        assert!(result.total_files > 0);
        assert!(result.total_directories > 0);
    }

    #[test]
    fn test_tree_depth_limit() {
        let dir = setup_tree_dir();

        // Depth 0: only root, no children explored
        let shallow = generate_tree(dir.path(), 0, false);
        assert!(shallow.success);
        let root = shallow.root.unwrap();
        assert!(root.children.is_empty());

        // Depth 1: root's immediate children
        let one_deep = generate_tree(dir.path(), 1, false);
        assert!(one_deep.success);
        let root = one_deep.root.unwrap();
        assert!(!root.children.is_empty());
        // Subdirectories at depth 1 should have no children
        for child in &root.children {
            if child.is_directory {
                assert!(child.children.is_empty());
            }
        }
    }

    #[test]
    fn test_tree_sorted() {
        let dir = setup_tree_dir();
        let result = generate_tree(dir.path(), 10, false);
        let root = result.root.unwrap();

        // Directories should come before files
        let mut saw_file = false;
        for child in &root.children {
            if child.is_directory {
                assert!(!saw_file, "Directory after file — sorting broken");
            } else {
                saw_file = true;
            }
        }
    }

    #[test]
    fn test_tree_skips_node_modules() {
        let dir = setup_tree_dir();
        fs::create_dir_all(dir.path().join("node_modules/foo")).unwrap();
        fs::write(dir.path().join("node_modules/foo/index.js"), "x").unwrap();

        let result = generate_tree(dir.path(), 10, false);
        let root = result.root.unwrap();

        // node_modules should not appear
        for child in &root.children {
            assert_ne!(child.name, "node_modules");
        }
    }

    #[test]
    fn test_tree_skips_hidden() {
        let dir = setup_tree_dir();
        fs::create_dir_all(dir.path().join(".hidden")).unwrap();
        fs::write(dir.path().join(".hidden/secret"), "s").unwrap();

        let result = generate_tree(dir.path(), 10, false);
        let root = result.root.unwrap();

        for child in &root.children {
            assert!(!child.name.starts_with('.'));
        }
    }

    #[test]
    fn test_tree_includes_hidden() {
        let dir = setup_tree_dir();
        fs::create_dir_all(dir.path().join(".config")).unwrap();
        fs::write(dir.path().join(".config/settings.json"), "{}").unwrap();

        let result = generate_tree(dir.path(), 10, true);
        let root = result.root.unwrap();

        let has_hidden = root.children.iter().any(|c| c.name == ".config");
        assert!(has_hidden, "Hidden directory should be included");
    }

    #[test]
    fn test_tree_nonexistent() {
        let result = generate_tree(Path::new("/nonexistent/path"), 10, false);
        assert!(!result.success);
        // Actionable: names it a PATH problem, not a workspace problem, and points at
        // the no-`path` call that works (regression for Anwen's 2026-07-14 give-up).
        let err = result.error.unwrap();
        assert!(err.contains("path not found"), "{err}");
        assert!(err.contains("code/tree with NO"), "names the recovery: {err}");
    }

    // what this catches: a FILE path (exists, not a dir) gets a DISTINCT, honest error
    // that points to code/read — not the old flat "Not a directory" a persona misread
    // as "the whole workspace is unexplorable".
    #[test]
    fn tree_on_a_file_names_it_a_file_and_points_to_read() {
        let dir = setup_tree_dir();
        let file = dir.path().join("README.md");
        std::fs::write(&file, "hi").unwrap();
        let result = generate_tree(&file, 10, false);
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("is a FILE"), "{err}");
        assert!(err.contains("code/read"), "points at the right tool: {err}");
    }

    #[test]
    fn test_tree_file_sizes() {
        let dir = setup_tree_dir();
        let result = generate_tree(dir.path(), 10, false);
        let root = result.root.unwrap();

        // Find a file and check it has size
        fn find_file(node: &TreeNode) -> Option<&TreeNode> {
            if !node.is_directory {
                return Some(node);
            }
            for child in &node.children {
                if let Some(f) = find_file(child) {
                    return Some(f);
                }
            }
            None
        }

        let file = find_file(&root).expect("Should have at least one file");
        assert!(file.size_bytes.is_some());
        assert!(file.size_bytes.unwrap() > 0);
    }
}
