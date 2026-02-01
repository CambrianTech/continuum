//! Git Bridge — git status, diff, and branch operations.
//!
//! Shells out to `git` CLI for operations. This keeps the implementation
//! simple and avoids pulling in libgit2 as a dependency.

use std::path::Path;
use std::process::Command;

use super::types::GitStatusInfo;

/// Get git status for a workspace directory.
///
/// Returns branch name, modified/added/deleted/untracked files.
pub fn git_status(workspace_root: &Path) -> GitStatusInfo {
    // Get current branch
    let branch = run_git(workspace_root, &["rev-parse", "--abbrev-ref", "HEAD"])
        .ok()
        .map(|s| s.trim().to_string());

    // Get porcelain status (machine-readable)
    let status_output = match run_git(workspace_root, &["status", "--porcelain=v1"]) {
        Ok(output) => output,
        Err(e) => {
            return GitStatusInfo {
                success: false,
                branch,
                modified: Vec::new(),
                added: Vec::new(),
                deleted: Vec::new(),
                untracked: Vec::new(),
                error: Some(format!("git status failed: {}", e)),
            };
        }
    };

    let mut modified = Vec::new();
    let mut added = Vec::new();
    let mut deleted = Vec::new();
    let mut untracked = Vec::new();

    for line in status_output.lines() {
        if line.len() < 3 {
            continue;
        }

        let status_code = &line[..2];
        let file_path = line[3..].trim().to_string();

        // Parse porcelain v1 status codes
        // First char = index status, second char = work tree status
        match status_code {
            "??" => untracked.push(file_path),
            s if s.starts_with('A') || s.ends_with('A') => added.push(file_path),
            s if s.starts_with('D') || s.ends_with('D') => deleted.push(file_path),
            s if s.starts_with('M') || s.ends_with('M')
                || s.starts_with('R') || s.ends_with('R')
                || s.starts_with('C') || s.ends_with('C') => modified.push(file_path),
            _ => {
                // Catch-all: treat as modified
                if !file_path.is_empty() {
                    modified.push(file_path);
                }
            }
        }
    }

    GitStatusInfo {
        success: true,
        branch,
        modified,
        added,
        deleted,
        untracked,
        error: None,
    }
}

/// Get git diff output for the workspace.
///
/// If `staged` is true, shows staged changes (--cached).
/// Otherwise shows unstaged working tree changes.
pub fn git_diff(workspace_root: &Path, staged: bool) -> Result<String, String> {
    let mut args = vec!["diff"];
    if staged {
        args.push("--cached");
    }
    run_git(workspace_root, &args)
}

/// Get git diff against a specific ref (branch, commit, etc.).
pub fn git_diff_ref(workspace_root: &Path, reference: &str) -> Result<String, String> {
    run_git(workspace_root, &["diff", reference])
}

/// Get git log (last N commits, one-line format).
pub fn git_log(workspace_root: &Path, count: u32) -> Result<String, String> {
    run_git(
        workspace_root,
        &["log", &format!("-{}", count), "--oneline", "--no-decorate"],
    )
}

/// Run a git command in the workspace directory.
fn run_git(workspace_root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(workspace_root)
        .output()
        .map_err(|e| format!("Failed to run git: {}", e))?;

    if output.status.success() {
        String::from_utf8(output.stdout)
            .map_err(|e| format!("Invalid UTF-8 in git output: {}", e))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("git {} failed: {}", args.join(" "), stderr.trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_git_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();

        // Initialize a git repo
        run_git(dir.path(), &["init"]).expect("git init should work");
        run_git(
            dir.path(),
            &["config", "user.email", "test@test.com"],
        )
        .expect("git config email");
        run_git(
            dir.path(),
            &["config", "user.name", "Test"],
        )
        .expect("git config name");

        // Create an initial commit
        fs::write(dir.path().join("initial.txt"), "hello\n").unwrap();
        run_git(dir.path(), &["add", "."]).expect("git add");
        run_git(dir.path(), &["commit", "-m", "Initial"]).expect("git commit");

        dir
    }

    #[test]
    fn test_git_status_clean() {
        let dir = setup_git_repo();
        let status = git_status(dir.path());
        assert!(status.success);
        assert!(status.branch.is_some());
        assert!(status.modified.is_empty());
        assert!(status.untracked.is_empty());
    }

    #[test]
    fn test_git_status_modified() {
        let dir = setup_git_repo();
        fs::write(dir.path().join("initial.txt"), "modified\n").unwrap();

        let status = git_status(dir.path());
        assert!(status.success);
        assert!(status.modified.contains(&"initial.txt".to_string()));
    }

    #[test]
    fn test_git_status_untracked() {
        let dir = setup_git_repo();
        fs::write(dir.path().join("new_file.txt"), "new\n").unwrap();

        let status = git_status(dir.path());
        assert!(status.success);
        assert!(status.untracked.contains(&"new_file.txt".to_string()));
    }

    #[test]
    fn test_git_diff() {
        let dir = setup_git_repo();
        fs::write(dir.path().join("initial.txt"), "modified\n").unwrap();

        let diff = git_diff(dir.path(), false).unwrap();
        assert!(diff.contains("-hello"));
        assert!(diff.contains("+modified"));
    }

    #[test]
    fn test_git_log() {
        let dir = setup_git_repo();
        let log = git_log(dir.path(), 5).unwrap();
        assert!(log.contains("Initial"));
    }

    #[test]
    fn test_git_status_not_a_repo() {
        let dir = tempfile::tempdir().unwrap();
        let status = git_status(dir.path());
        // Should still return a result (possibly with error)
        // git status in non-repo returns error
        assert!(!status.success || status.branch.is_none());
    }
}
