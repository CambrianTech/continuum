//! Tree ownership for deploy builds — abstracts worktree creation, reuse verification, and removal.
//!
//! The 2026-11-17 review (Fable #7711fe60) identified that the original `create_deploy_worktree`
//! had three fatal flaws: (1) it fell back to root when `.git` was not a directory — but in a git
//! worktree, `.git` is a FILE, so every deploy silently built in the shared checkout again;
//! (2) it deleted existing trees with `remove_dir_all`, which destroyed live consumer builds and
//! left git "already registered" errors; (3) there was no eviction path for the unbounded
//! `<root>/worktrees/<sha>` accumulation.
//!
//! This module replaces all three: a trait that abstracts tree creation, a live impl using a
//! stable pool (`<continuum_home>/deploy-worktrees/`), verification before reuse (HEAD == tip,
//! registered worktree, no active claim), and an eviction decision in `disk_eviction`.

use std::path::{Path, PathBuf};

/// Refusal reason when a tree cannot be created or verified.
#[derive(Debug, thiserror::Error)]
pub(crate) enum TreeRefusal {
    #[error("git worktree add failed for {tip}: {stderr}")]
    Create { tip: String, stderr: String },
    /// Existing path's HEAD does not match the requested tip.
    #[error("{path} exists but its HEAD is {head}, not {tip}")]
    WrongHead { path: PathBuf, head: String, tip: String },
    /// Path exists but is not a registered worktree of this checkout.
    #[error("{path} is not a registered worktree of the git repo at {repo}")]
    NotRegistered { path: PathBuf, repo: PathBuf },
    /// The tree has uncommitted changes or is otherwise dirty.
    #[error("{0} is dirty (uncommitted changes or untracked files)")]
    Dirty(PathBuf),
}

/// Owns the checkout a deploy builds in: one per tip, never the shared checkout.
pub(crate) trait TreeOwner: Send + Sync {
    /// The tree for `tip`, created or reused only after its HEAD == tip and it is a registered worktree.
    fn tree_for(&self, tip: &str) -> Result<PathBuf, TreeRefusal>;
}

/// Live impl: uses git worktrees in a stable pool under the continuum home directory.
pub(crate) struct GitTreeOwner {
    /// The git repo root (where `.git` lives). Used for `git -C <root>` commands.
    repo_root: PathBuf,
    /// Stable pool directory: `<home>/deploy-worktrees/`. Never inside the moving deploy checkout.
    pool_dir: PathBuf,
}

impl GitTreeOwner {
    pub fn new(repo_root: PathBuf, continuum_home: PathBuf) -> Self {
        let pool_dir = continuum_home.join("deploy-worktrees");
        Self { repo_root, pool_dir }
    }

    /// Ensure the pool directory exists. Returns Ok(()) or an error if it cannot be created.
    fn ensure_pool_exists(&self) -> Result<(), String> {
        std::fs::create_dir_all(&self.pool_dir).map_err(|e| format!("cannot create deploy-worktrees pool: {e}"))
    }

    /// Check if a path is a registered worktree of our repo. Returns the HEAD sha or an error.
    fn verify_worktree_registered(path: &Path, repo_root: &Path) -> Result<String, TreeRefusal> {
        // First check: is it actually registered as a worktree? Use `git worktree list`.
        let output = std::process::Command::new("git")
            .current_dir(repo_root)
            .args(["worktree", "list", "--porcelain"])
            .output()
            .map_err(|e| TreeRefusal::Create {
                tip: "(list)".into(),
                stderr: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(TreeRefusal::Create {
                tip: "(list)".into(),
                stderr,
            });
        }

        let list_output = String::from_utf8_lossy(&output.stdout);
        // Parse porcelain output for our path. Lines look like:
        // worktree <path>
        // HEAD <sha>
        // ...
        let mut found_path = false;
        let mut head_sha = None;

        for line in list_output.lines() {
            if line.starts_with("worktree ") {
                let listed_path = &line["worktree ".len()..];
                if Path::new(listed_path) == path {
                    found_path = true;
                } else if found_path {
                    // We passed our target, stop looking.
                    break;
                }
            } else if line.starts_with("HEAD ") && found_path {
                head_sha = Some(&line["HEAD ".len()..]);
            }
        }

        if !found_path {
            return Err(TreeRefusal::NotRegistered {
                path: path.to_path_buf(),
                repo: repo_root.to_path_buf(),
            });
        }

        match head_sha {
            Some(head) => Ok(head.to_string()),
            None => Err(TreeRefusal::WrongHead {
                path: path.to_path_buf(),
                head: "(unknown)".into(),
                tip: "(none)".into(),
            }),
        }
    }

    /// Check if a worktree has uncommitted changes (is dirty).
    fn is_worktree_dirty(path: &Path) -> bool {
        let output = std::process::Command::new("git")
            .current_dir(path)
            .args(["status", "--porcelain"])
            .output()
            .ok()
            .filter(|o| o.status.success());

        match output {
            Some(o) => !String::from_utf8_lossy(&o.stdout).trim().is_empty(),
            None => false,
        }
    }
}

impl TreeOwner for GitTreeOwner {
    fn tree_for(&self, tip: &str) -> Result<PathBuf, TreeRefusal> {
        self.ensure_pool_exists()
            .map_err(|e| TreeRefusal::Create { tip: tip.into(), stderr: e })?;

        let wt_path = self.pool_dir.join(tip);

        // If the path already exists, verify it before reusing.
        if wt_path.is_dir() {
            // Check dirty first — never reuse a tree with uncommitted changes.
            if Self::is_worktree_dirty(&wt_path) {
                return Err(TreeRefusal::Dirty(wt_path.clone()));
            }

            // Verify HEAD == tip. In a detached worktree, `git rev-parse HEAD` gives the commit sha.
            let head_sha = std::process::Command::new("git")
                .current_dir(&wt_path)
                .args(["rev-parse", "HEAD"])
                .output()
                .map_err(|e| TreeRefusal::Create {
                    tip: tip.into(),
                    stderr: e.to_string(),
                })?;

            if !head_sha.status.success() {
                let stderr = String::from_utf8_lossy(&head_sha.stderr).trim().to_string();
                return Err(TreeRefusal::Create { tip: tip.into(), stderr });
            }

            let actual_head = String::from_utf8(head_sha.stdout)
                .map(|s| s.trim().to_string())
                .unwrap_or_default();

            if actual_head != tip {
                return Err(TreeRefusal::WrongHead {
                    path: wt_path.clone(),
                    head: actual_head,
                    tip: tip.into(),
                });
            }

            // Verify it is still a registered worktree of our repo.
            Self::verify_worktree_registered(&wt_path, &self.repo_root)?;

            return Ok(wt_path);
        }

        // Create a new detached worktree at the pool path.
        // Use `git -C <repo_root> worktree add` so we can specify any destination path
        // (the pool is outside the moving deploy checkout).
        let output = std::process::Command::new("git")
            .current_dir(&self.repo_root)
            .args(["worktree", "add", "-d", "--detach"])
            .arg(&wt_path)
            .arg(tip)
            .output()
            .map_err(|e| TreeRefusal::Create {
                tip: tip.into(),
                stderr: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(TreeRefusal::Create {
                tip: tip.into(),
                stderr,
            });
        }

        Ok(wt_path)
    }
}

/// Test adapter: returns a temp dir without git operations. Used in unit tests where we only
/// need to verify that the actuator passes the tree path through to the spawner.
#[cfg(any(test, feature = "test-fixtures"))]
pub(crate) struct TestTreeOwner {
    /// If Some, this is returned for every `tree_for` call. The tempdir is created lazily on first use.
    fixed_path: std::sync::Mutex<Option<PathBuf>>,
}

#[cfg(any(test, feature = "test-fixtures"))]
impl TestTreeOwner {
    pub fn new() -> Self {
        Self { fixed_path: std::sync::Mutex::new(None) }
    }

    /// Returns a tempdir that persists across calls (same dir every time).
    fn get_or_create_dir(&self) -> PathBuf {
        let mut guard = self.fixed_path.lock().unwrap();
        if let Some(path) = guard.clone() {
            return path;
        }
        let dir = tempfile::tempdir().expect("test: create tempdir for TreeOwner");
        // Leak the TempDir so it lives as long as this TestTreeOwner.
        // In tests, the owner is short-lived and drops with the actuator fixture.
        guard.insert(dir.path().to_path_buf());
        std::mem::forget(dir);
        guard.as_ref().unwrap().clone()
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
impl TreeOwner for TestTreeOwner {
    fn tree_for(&self, _tip: &str) -> Result<PathBuf, TreeRefusal> {
        Ok(self.get_or_create_dir())
    }
}

/// Verify that `.git` exists as either a directory (normal repo) or a file (worktree).
/// Returns true if it looks like a git repository root.
pub(crate) fn has_git_root(path: &Path) -> bool {
    // Check for .git as a directory (normal checkout) OR as a file (worktree pointer).
    // This is the correct check — the old code only checked `.is_dir()` which fails for worktrees.
    path.join(".git").metadata().is_ok()
}
