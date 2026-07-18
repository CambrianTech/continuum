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
            s if s.starts_with('M')
                || s.ends_with('M')
                || s.starts_with('R')
                || s.ends_with('R')
                || s.starts_with('C')
                || s.ends_with('C') =>
            {
                modified.push(file_path)
            }
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

/// Stage files for commit.
///
/// Pass specific file paths, or `&["--all"]` / `&["."]` to stage everything.
pub fn git_add(workspace_root: &Path, paths: &[&str]) -> Result<String, String> {
    let mut args = vec!["add"];
    args.extend_from_slice(paths);
    run_git(workspace_root, &args)
}

/// Create a commit with the given message.
///
/// Returns the full commit hash on success.
pub fn git_commit(workspace_root: &Path, message: &str) -> Result<String, String> {
    // Commit through the repository's normal hook path. AI-authored commits
    // must fail loudly when validation fails; callers surface the git stderr.
    run_git(workspace_root, &["commit", "-m", message])?;

    // Return the commit hash
    run_git(workspace_root, &["rev-parse", "HEAD"]).map(|s| s.trim().to_string())
}

/// Push the current branch to a remote.
///
/// Defaults to `origin` if remote is empty.
/// Apply a unified diff to the working tree (`git apply`). `check_only` runs
/// `git apply --check` — validate without touching files. The patch arrives on
/// stdin so peer-shared diffs of any size apply without temp files. This is the
/// RECEIVING end of diffs-over-the-room: one citizen `code/git/diff`s, posts the
/// patch, another applies it — the consolidation verb the Conway team asked for
/// (2026-07-11) before the full branch/merge rails exist.
pub fn git_apply(workspace_root: &Path, patch: &str, check_only: bool) -> Result<String, String> {
    use std::io::Write;
    let mut args = vec!["apply", "--whitespace=nowarn"];
    if check_only {
        args.push("--check");
    }
    let mut child = Command::new("git")
        .args(&args)
        .current_dir(workspace_root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to run git apply: {e}"))?;
    child
        .stdin
        .as_mut()
        .ok_or("git apply stdin unavailable")?
        .write_all(patch.as_bytes())
        .map_err(|e| format!("failed to write patch to git apply: {e}"))?;
    let out = child
        .wait_with_output()
        .map_err(|e| format!("git apply did not exit: {e}"))?;
    if out.status.success() {
        Ok(if check_only {
            "patch applies cleanly (checked, not applied)".to_string()
        } else {
            "patch applied".to_string()
        })
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Ensure `workspace_root` is a git repository: no-op when `.git` exists;
/// otherwise `git init` + an initial commit of whatever is present, so every
/// citizen workspace is diff-able/apply-able from birth
/// ([[workspace-is-a-cow-diff-from-shared-always-git]]). Loud Err if git
/// itself is unavailable — a workspace that silently can't version work is
/// the kind of quiet defect the jobs-ledger taught us to refuse.
pub fn git_init_if_needed(workspace_root: &Path) -> Result<bool, String> {
    if workspace_root.join(".git").exists() {
        return Ok(false);
    }
    run_git(workspace_root, &["init"]).map_err(|e| format!("git init failed: {e}"))?;
    // Identity for the initial commit: repo-local, never touching global config.
    let _ = run_git(workspace_root, &["config", "user.email", "citizen@continuum.local"]);
    let _ = run_git(workspace_root, &["config", "user.name", "continuum-citizen"]);
    let _ = run_git(workspace_root, &["add", "-A"]);
    // An empty dir still gets a root commit so diffs have a base.
    let _ = run_git(
        workspace_root,
        &["commit", "--allow-empty", "-m", "workspace: initial state"],
    );
    Ok(true)
}

pub fn git_push(workspace_root: &Path, remote: &str, branch: &str) -> Result<String, String> {
    let remote = if remote.is_empty() { "origin" } else { remote };
    let mut args = vec!["push", remote];
    if !branch.is_empty() {
        args.push(branch);
    }
    run_git(workspace_root, &args)
}

/// Outcome of a [`git_sync_from_shared`] refresh — drives the "notify" the caller
/// surfaces to the persona (Joel: "preserve and notify… eventually just learn this").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitSyncReport {
    /// True when new shared commits were actually merged in (workspace changed).
    pub synced: bool,
    /// Short human/persona-readable summary of what happened.
    pub summary: String,
}

/// Bring a citizen workspace CURRENT with the shared checkout WITHOUT destroying
/// the persona's own work — the self-heal for
/// [[citizen-workspaces-are-stale-one-time-clones]]. Citizen workspaces are
/// `cp -cR` clones of the shared checkout (including its `.git`), so they share
/// history with shared; but `ensure_citizen_layer` never refreshed them, so they
/// drifted stale (one froze before the `workers/→core/` restructure). Because the
/// histories are related, a fetch + merge brings the framework current while
/// KEEPING the persona's commits.
///
/// Preserve-first (Joel: "preserve and notify"):
///  1. autocommit any uncommitted persona work, so a merge can't lose it and it
///     survives on the persona's own history.
///  2. `git fetch <shared> HEAD` — shared's current HEAD into FETCH_HEAD.
///  3. `git merge -X theirs --no-edit --allow-unrelated-histories FETCH_HEAD` —
///     shared wins framework-file conflicts (the persona shouldn't diverge the
///     framework); the persona's OWN new files are untouched; their commits stay
///     in history. `-X theirs` is deterministic — never drops to an interactive
///     conflict. On a merge that truly can't complete we `--abort` so the
///     workspace is never left half-merged, and surface the cause LOUD.
///
/// Returns whether anything changed (+ a summary) so the caller can drop the
/// teaching note only when there was actually a refresh.
pub fn git_sync_from_shared(
    workspace_root: &Path,
    shared_checkout: &Path,
) -> Result<GitSyncReport, String> {
    // 1. Preserve: commit in-flight persona work before merging.
    let porcelain = run_git(workspace_root, &["status", "--porcelain"]).unwrap_or_default();
    if !porcelain.trim().is_empty() {
        let _ = run_git(workspace_root, &["add", "-A"]);
        let _ = run_git(
            workspace_root,
            &["commit", "-m", "workspace: autosave persona work before shared sync"],
        );
    }
    let before = run_git(workspace_root, &["rev-parse", "HEAD"])
        .unwrap_or_default()
        .trim()
        .to_string();

    // 2. Fetch shared's current HEAD (a filesystem path is a valid git remote).
    let shared = shared_checkout.to_string_lossy();
    run_git(workspace_root, &["fetch", "--no-tags", shared.as_ref(), "HEAD"])
        .map_err(|e| format!("fetch from shared checkout '{shared}' failed: {e}"))?;

    // 3. Merge shared in — shared wins framework conflicts, persona files kept.
    if let Err(e) = run_git(
        workspace_root,
        &[
            "merge",
            "--no-edit",
            "--allow-unrelated-histories",
            "-X",
            "theirs",
            "FETCH_HEAD",
        ],
    ) {
        // Never leave a half-merge — abort so the workspace stays usable, fail loud.
        let _ = run_git(workspace_root, &["merge", "--abort"]);
        return Err(format!(
            "shared merge failed (aborted; workspace left intact): {e}"
        ));
    }

    let after = run_git(workspace_root, &["rev-parse", "HEAD"])
        .unwrap_or_default()
        .trim()
        .to_string();
    let synced = !before.is_empty() && before != after;
    let summary = if synced {
        let count = run_git(
            workspace_root,
            &["rev-list", "--count", &format!("{before}..{after}")],
        )
        .unwrap_or_default()
        .trim()
        .to_string();
        format!("synced {count} shared commit(s) in; your work was preserved")
    } else {
        "already current with the shared checkout".to_string()
    };
    Ok(GitSyncReport { synced, summary })
}

/// Run a git command in the workspace directory.
fn run_git(workspace_root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(workspace_root)
        // Strip git-context env vars that would otherwise pin git to
        // the parent repo regardless of cwd. Without this, when
        // run_git is invoked from a process that itself was launched
        // by git (the most common case: pre-push / pre-commit hooks
        // invoking `cargo test`), git sets GIT_DIR/GIT_PREFIX/etc and
        // those propagate to every child. Concrete failure:
        // git_bridge::tests' tempdir `git commit` inherited GIT_DIR
        // pointing at the parent worktree's .git, then ran the
        // worktree's pre-commit hook (whose paths don't exist in the
        // tempdir context) and panicked. Caught 2026-05-02 wedging the
        // whole git_bridge::tests cluster every time the pre-push hook
        // ran them. Stripping these makes run_git context-clean — git
        // discovers from current_dir(workspace_root) only, no parent
        // contamination.
        // GIT_CEILING_DIRECTORIES caps any residual upward discovery
        // at workspace_root (defense in depth — env_remove handles the
        // documented vars; ceiling handles anything new git might add
        // in future versions).
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_PREFIX")
        .env("GIT_CEILING_DIRECTORIES", workspace_root)
        .output()
        .map_err(|e| format!("Failed to run git: {}", e))?;

    if output.status.success() {
        String::from_utf8(output.stdout).map_err(|e| format!("Invalid UTF-8 in git output: {}", e))
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
        run_git(dir.path(), &["config", "user.email", "test@test.com"]).expect("git config email");
        run_git(dir.path(), &["config", "user.name", "Test"]).expect("git config name");

        // Create an initial commit
        fs::write(dir.path().join("initial.txt"), "hello\n").unwrap();
        run_git(dir.path(), &["add", "."]).expect("git add");
        run_git(dir.path(), &["commit", "-m", "Initial"]).expect("git commit");

        dir
    }

    // what this catches: the self-heal for stale citizen clones must bring the
    // workspace CURRENT with shared AND preserve the persona's own work. A
    // regression here silently either strands a persona on stale code (Casper's
    // pre-restructure clone) or clobbers her work on refresh.
    #[test]
    fn sync_from_shared_brings_current_and_preserves_persona_work() {
        let shared = setup_git_repo();

        // citizen: a cp-clone of shared incl. .git — mirrors the `cp -cR` CoW clone
        // (related history, the whole reason a fetch+merge works).
        let citizen = tempfile::tempdir().unwrap();
        let out = std::process::Command::new("cp")
            .arg("-R")
            .arg(format!("{}/.", shared.path().display()))
            .arg(citizen.path())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "cp clone failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        run_git(citizen.path(), &["config", "user.email", "citizen@test"]).unwrap();
        run_git(citizen.path(), &["config", "user.name", "Citizen"]).unwrap();

        // shared advances (a new framework file), citizen has its OWN uncommitted work.
        fs::write(shared.path().join("framework.rs"), "// shared code\n").unwrap();
        run_git(shared.path(), &["add", "."]).unwrap();
        run_git(shared.path(), &["commit", "-m", "shared: add framework.rs"]).unwrap();
        fs::write(citizen.path().join("my_work.rs"), "// persona code\n").unwrap();

        let report = git_sync_from_shared(citizen.path(), shared.path()).expect("sync ok");
        assert!(report.synced, "should report a sync: {}", report.summary);

        // BOTH survive: shared's new file arrives, the persona's work is preserved.
        assert!(citizen.path().join("framework.rs").exists(), "shared file must arrive");
        assert!(citizen.path().join("my_work.rs").exists(), "persona work must survive");
        assert!(citizen.path().join("initial.txt").exists(), "base file still present");

        // Idempotent: a second sync is a clean no-op.
        let again = git_sync_from_shared(citizen.path(), shared.path()).expect("2nd sync ok");
        assert!(!again.synced, "already current: {}", again.summary);
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

    #[test]
    fn test_git_add_and_commit() {
        let dir = setup_git_repo();

        // Create a new file
        fs::write(dir.path().join("feature.txt"), "new feature\n").unwrap();

        // Stage it
        git_add(dir.path(), &["feature.txt"]).expect("git add should work");

        // Status should show it as added
        let status = git_status(dir.path());
        assert!(status.added.contains(&"feature.txt".to_string()));

        // Commit it
        let hash = git_commit(dir.path(), "Add feature").expect("git commit should work");
        assert!(!hash.is_empty());
        assert!(hash.len() >= 7); // At least a short hash

        // Status should be clean now
        let status_after = git_status(dir.path());
        assert!(status_after.modified.is_empty());
        assert!(status_after.added.is_empty());
        assert!(status_after.untracked.is_empty());
    }

    #[test]
    fn test_git_commit_empty_fails() {
        let dir = setup_git_repo();
        // Nothing staged — commit should fail
        let result = git_commit(dir.path(), "Empty commit");
        assert!(result.is_err());
    }

    #[test]
    fn test_git_add_all() {
        let dir = setup_git_repo();

        fs::write(dir.path().join("a.txt"), "aaa\n").unwrap();
        fs::write(dir.path().join("b.txt"), "bbb\n").unwrap();

        git_add(dir.path(), &["."]).expect("git add . should work");

        let status = git_status(dir.path());
        // Both files should be staged (added)
        assert!(status.added.contains(&"a.txt".to_string()));
        assert!(status.added.contains(&"b.txt".to_string()));
    }
}
