//! A git `index.lock` no live git holds is a dead process's, and it blocks every later git write
//! in that tree ("Unable to create '…/index.lock': File exists") until someone removes it.
//!
//! Two owners hit this and each fixed it by hand until it had one rule:
//! - the deploy consumer (card 677437fa: the M5 on 2026-09-27 failed every checkout for ~3.4 h,
//!   twice, each time cleared by hand);
//! - a citizen's hands (card d7765b90: on enrico 6 residents' workspaces held locks dated
//!   2026-10-14..16 with no holder, and none of them could commit for ~10 days until Eitan
//!   reported it).
//!
//! The rule: a lock older than its caller's bound, with no git that might be working in the
//! tree, is removed and the removal is reported. A lock a running git might own is never touched.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// How old a lock must be, with no git in the tree, before it is a dead process's. Git holds
/// `index.lock` only while it writes the index, so seconds; a full checkout of the deploy tree
/// on the slowest node takes well under a minute.
// derived-or-floor: a floor, ten times the slowest checkout seen here, far under the hours (and days) a stale lock sat.
pub const STALE_INDEX_LOCK: Duration = Duration::from_secs(10 * 60);

/// The verdict on a tree's lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexLock {
    /// No lock.
    Absent,
    /// A lock a live git operation may still hold (younger than the bound, or a git process
    /// may be working in the tree): leave it.
    Held,
    /// A lock no git operation holds: remove it, with a receipt.
    Stale,
}

/// PURE: the verdict from the lock's age, the age past which no live write holds it, and
/// whether any git might be working in the tree. Never calls a lock stale while a git might own it.
pub fn verdict(age: Option<Duration>, stale_after: Duration, git_in_tree: bool) -> IndexLock {
    match age {
        None => IndexLock::Absent,
        Some(age) if age >= stale_after && !git_in_tree => IndexLock::Stale,
        Some(_) => IndexLock::Held,
    }
}

/// Where `root`'s index lock lives, without running git: `<root>/.git/index.lock` for a clone,
/// or `<gitdir>/index.lock` for a worktree whose `.git` file names its gitdir. `None` when
/// `root` is not a git work tree.
pub fn lock_path(root: &Path) -> Option<PathBuf> {
    let dot_git = root.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git.join("index.lock"));
    }
    let text = std::fs::read_to_string(&dot_git).ok()?;
    let gitdir = text.lines().find_map(|line| line.strip_prefix("gitdir:"))?.trim();
    let gitdir = Path::new(gitdir);
    Some(if gitdir.is_absolute() { gitdir.join("index.lock") } else { root.join(gitdir).join("index.lock") })
}

/// Whether any running `git` might be working on `root`, judged conservatively: a git whose cwd
/// is inside the tree, whose command line names the tree (`--git-dir` / `--work-tree` given from
/// elsewhere), or whose cwd cannot be read at all. Only a git positively seen working elsewhere
/// is ruled out: an unreadable cwd is not evidence of absence (Codex on #4477).
pub fn git_running_in(root: &Path) -> bool {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cwd(UpdateKind::Always).with_cmd(UpdateKind::Always),
    );
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf()); // unwrap_or_else: an uncanonicalizable root still compares as given
    let spellings = [root.to_string_lossy().into_owned(), canonical.to_string_lossy().into_owned()];
    sys.processes().values().any(|p| {
        let name = p.name().to_string_lossy();
        if name != "git" && name != "git.exe" {
            return false;
        }
        let names_tree = p.cmd().iter().any(|a| spellings.iter().any(|r| a.to_string_lossy().contains(r.as_str())));
        match p.cwd() {
            Some(cwd) => names_tree || cwd.starts_with(&canonical) || cwd.starts_with(root),
            None => true, // unreadable: it may be working here
        }
    })
}

/// What [`settle`] found and did.
#[derive(Debug)]
pub enum Settled {
    /// No lock (or not a git tree).
    Absent,
    /// A lock that may be live was left in place.
    Held { lock: PathBuf, age: Duration },
    /// A dead process's lock was removed.
    Cleared { lock: PathBuf, age: Duration },
    /// A stale lock could not be removed.
    Failed { lock: PathBuf, age: Duration, error: String },
}

/// Judge `root`'s index lock and remove it when no git holds it. The process scan runs only
/// when a lock exists, so a clean tree costs one `stat`. `stale_after` is the caller's bound:
/// [`STALE_INDEX_LOCK`] for a lock of unknown origin, zero right after this process ended a
/// git itself (its lock is that git's, unless another git now works in the tree).
pub fn settle(root: &Path, stale_after: Duration) -> Settled {
    let Some(lock) = lock_path(root) else {
        return Settled::Absent;
    };
    let Some(age) = std::fs::metadata(&lock).ok().and_then(|m| m.modified().ok()).map(|t| t.elapsed().unwrap_or_default()) // unwrap_or_default: an mtime in the future reads as brand new, never as stale
    else {
        return Settled::Absent;
    };
    match verdict(Some(age), stale_after, age >= stale_after && git_running_in(root)) {
        IndexLock::Absent => Settled::Absent,
        IndexLock::Held => Settled::Held { lock, age },
        IndexLock::Stale => match std::fs::remove_file(&lock) {
            Ok(()) => Settled::Cleared { lock, age },
            Err(error) => Settled::Failed { lock, age, error: error.to_string() },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card 677437fa): an old lock with no git in the tree is cleared; a
    // young one, or one a git process may own, is only named.
    #[test]
    fn a_stale_index_lock_is_cleared_and_a_live_one_is_left() {
        let old = Some(STALE_INDEX_LOCK + Duration::from_secs(1));
        let young = Some(Duration::from_secs(5));
        assert_eq!(verdict(None, STALE_INDEX_LOCK, false), IndexLock::Absent);
        assert_eq!(verdict(old, STALE_INDEX_LOCK, false), IndexLock::Stale, "abandoned: remove it");
        assert_eq!(verdict(old, STALE_INDEX_LOCK, true), IndexLock::Held, "a git that may own it: leave it");
        assert_eq!(verdict(young, STALE_INDEX_LOCK, false), IndexLock::Held, "young: a live write");
        assert_eq!(verdict(Some(STALE_INDEX_LOCK), STALE_INDEX_LOCK, false), IndexLock::Stale, "the bound itself is stale");
        assert_eq!(verdict(young, Duration::ZERO, false), IndexLock::Stale, "a lock of a git this process just ended");
        assert_eq!(verdict(young, Duration::ZERO, true), IndexLock::Held, "unless another git now works in the tree");
    }

    // what this catches (card d7765b90): a dead git's lock in a clone, and in a worktree whose
    // `.git` is a file, is found without running git and removed; a tree without one is untouched.
    #[test]
    fn a_dead_gits_lock_is_found_in_a_clone_and_a_worktree_and_removed() {
        let clone = tempfile::tempdir().expect("clone");
        std::fs::create_dir(clone.path().join(".git")).expect(".git");
        assert!(matches!(settle(clone.path(), Duration::ZERO), Settled::Absent), "no lock");
        let lock = clone.path().join(".git/index.lock");
        std::fs::write(&lock, b"").expect("lock");
        assert!(matches!(settle(clone.path(), Duration::ZERO), Settled::Cleared { .. }), "no git runs in a fresh tempdir");
        assert!(!lock.exists());

        let worktree = tempfile::tempdir().expect("worktree");
        let gitdir = tempfile::tempdir().expect("gitdir");
        std::fs::write(worktree.path().join(".git"), format!("gitdir: {}\n", gitdir.path().display())).expect(".git file");
        assert_eq!(lock_path(worktree.path()), Some(gitdir.path().join("index.lock")));
        std::fs::write(gitdir.path().join("index.lock"), b"").expect("lock");
        assert!(matches!(settle(worktree.path(), STALE_INDEX_LOCK), Settled::Held { .. }), "young under the default bound: left");
        assert!(gitdir.path().join("index.lock").exists());

        let plain = tempfile::tempdir().expect("plain");
        assert_eq!(lock_path(plain.path()), None, "not a git tree");
    }
}
