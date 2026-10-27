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

/// What one running process says about where it works: its command line, its cwd (`None` when
/// unreadable) and its environment.
pub struct GitProcess<'a> {
    pub cmd: &'a [String],
    pub cwd: Option<&'a Path>,
    pub env: &'a [String],
}

/// PURE: `path` joined onto `base` and normalized lexically (`.` dropped, `..` pops), so a
/// relative `--git-dir=.git/worktrees/tree` from cwd `/repo` compares as
/// `/repo/.git/worktrees/tree` without touching the filesystem.
fn resolve(base: &Path, path: &str) -> PathBuf {
    let mut out = PathBuf::new();
    for part in base.join(path).components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// PURE: whether this git might be working on the tree whose work root is `root` and whose
/// administrative gitdir (where the lock lives) is `gitdir`. A git is judged by every way it can
/// name the tree, each resolved the way git resolves it (Codex on #5464):
/// - its cwd, and each `-C <dir>` in order, inside either directory;
/// - `--git-dir` / `--work-tree` (`=value` or separate) resolved against the cwd after ALL the
///   `-C`s (git applies them first, wherever the option sits), and against the cwd where it sits;
/// - `GIT_DIR` / `GIT_WORK_TREE` in its environment, resolved against both the process cwd and the
///   cwd after the `-C`s (conservative: either reading counts).
/// Paths compare as given and canonical. An unreadable cwd is never evidence of absence.
pub fn may_work_on(git: &GitProcess<'_>, root: &Path, gitdir: &Path) -> bool {
    let canonical = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf()); // unwrap_or_else: an uncanonicalizable path still compares as given
    let dirs = [root.to_path_buf(), canonical(root), gitdir.to_path_buf(), canonical(gitdir)];
    let inside = |p: &Path| dirs.iter().any(|d| p.starts_with(d) || canonical(p).starts_with(d));
    let Some(cwd) = git.cwd else {
        return true; // unreadable: it may be working here
    };
    let mut base = cwd.to_path_buf();
    let mut named = Vec::new();
    let mut options = Vec::new();
    let mut args = git.cmd.iter().skip(1);
    while let Some(arg) = args.next() {
        if arg == "-C" {
            if let Some(dir) = args.next() {
                base = resolve(&base, dir);
                named.push(base.clone());
            }
        } else if let Some(option) = ["--git-dir", "--work-tree"].iter().find(|o| arg == **o || arg.starts_with(&format!("{o}="))) {
            let value = arg.strip_prefix(&format!("{option}=")).map(str::to_string).or_else(|| args.next().cloned());
            if let Some(value) = value {
                // git applies every -C before resolving this, so the FINAL directory decides;
                // the directory at this point is kept too, conservatively (Codex on #5464)
                named.push(resolve(&base, &value));
                options.push(value);
            }
        }
    }
    named.extend(options.iter().map(|value| resolve(&base, value)));
    for var in git.env {
        if let Some(value) = var.strip_prefix("GIT_DIR=").or_else(|| var.strip_prefix("GIT_WORK_TREE=")) {
            named.push(resolve(cwd, value));
            named.push(resolve(&base, value));
        }
    }
    inside(cwd) || inside(&base) || named.iter().any(|p| inside(p))
}

/// Whether any running `git` might be working on the tree (see [`may_work_on`]).
pub fn git_running_in(root: &Path, gitdir: &Path) -> bool {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_cwd(UpdateKind::Always)
            .with_cmd(UpdateKind::Always)
            .with_environ(UpdateKind::Always),
    );
    sys.processes().values().any(|p| {
        let name = p.name().to_string_lossy();
        if name != "git" && name != "git.exe" {
            return false;
        }
        let cmd: Vec<String> = p.cmd().iter().map(|a| a.to_string_lossy().into_owned()).collect();
        let env: Vec<String> = p.environ().iter().map(|v| v.to_string_lossy().into_owned()).collect();
        may_work_on(&GitProcess { cmd: &cmd, cwd: p.cwd(), env: &env }, root, gitdir)
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
    // The file judged, held open by identity: a lock removed and re-created by a live git between
    // the scan and the unlink is another file and is left alone. The handle stays open until the
    // unlink, so the judged file's identity cannot be reused by a new file meanwhile (Codex on
    // #5464). What remains is the instant between the last identity read and the unlink: a
    // replacement landing exactly there would be removed. Portable code cannot unlink by handle,
    // so this is a limitation, narrowed (a few syscalls) but not eliminated. On the zero-bound
    // path, right after this process ended a git, the lock may be seconds old; what protects a
    // live git there is the scan finding it, not the lock's age.
    let Some(judged) = open_identity(&lock) else {
        return Settled::Held { lock, age };
    };
    let gitdir = lock.parent().unwrap_or(root); // unwrap_or: a lock path always has a parent directory
    match verdict(Some(age), stale_after, age >= stale_after && git_running_in(root, gitdir)) {
        IndexLock::Absent => Settled::Absent,
        IndexLock::Held => Settled::Held { lock, age },
        IndexLock::Stale if open_identity(&lock).map(|(_, id)| id) != Some(judged.1) => Settled::Held { lock, age },
        IndexLock::Stale => match std::fs::remove_file(&lock) {
            Ok(()) => Settled::Cleared { lock, age },
            Err(error) => Settled::Failed { lock, age, error: error.to_string() },
        },
    }
}

/// The lock file, opened, with its identity on its volume. `None` when it cannot be opened (gone,
/// or unreadable: either way it is not removed on this pass). The handle is returned so the
/// caller keeps the judged file alive while it decides.
fn open_identity(lock: &Path) -> Option<(std::fs::File, continuum_host::file_identity::FileIdentity)> {
    let file = std::fs::File::open(lock).ok()?;
    let identity = continuum_host::file_identity::of(&file).ok()?;
    Some((file, identity))
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
        let _no_git_running = crate::GIT_PROCESSES.lock().unwrap_or_else(|poisoned| poisoned.into_inner()); // unwrap_or_else: a neighbour's failure must not fail this test too
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

        // the same lock re-created while the judged one is still held open is another file (the held
        // handle keeps its identity from being reused): left in place
        let (held, first) = open_identity(&gitdir.path().join("index.lock")).expect("judged lock");
        std::fs::remove_file(gitdir.path().join("index.lock")).expect("remove");
        std::fs::write(gitdir.path().join("index.lock"), b"").expect("re-create");
        let (_, second) = open_identity(&gitdir.path().join("index.lock")).expect("re-created lock");
        assert_ne!(first, second, "a lock re-created while the judged one is open is another file");
        drop(held);

        let plain = tempfile::tempdir().expect("plain");
        assert_eq!(lock_path(plain.path()), None, "not a git tree");
    }

    // what this catches (Codex on #5464): a live git working on a worktree from elsewhere, named
    // only by its administrative gitdir (--git-dir or GIT_DIR), read as absent so its lock went.
    #[test]
    fn a_git_naming_the_tree_any_way_may_hold_its_lock() {
        let root = Path::new("/w/tree");
        let gitdir = Path::new("/repo/.git/worktrees/tree");
        let elsewhere = Path::new("/somewhere/else");
        let git = |cmd: &[&str], cwd: Option<&Path>, env: &[&str]| {
            let cmd: Vec<String> = cmd.iter().map(|s| s.to_string()).collect();
            let env: Vec<String> = env.iter().map(|s| s.to_string()).collect();
            may_work_on(&GitProcess { cmd: &cmd, cwd, env: &env }, root, gitdir)
        };
        assert!(git(&["git", "--git-dir=/repo/.git/worktrees/tree", "status"], Some(elsewhere), &[]), "--git-dir names the gitdir");
        assert!(git(&["git", "status"], Some(elsewhere), &["GIT_DIR=/repo/.git/worktrees/tree"]), "GIT_DIR names the gitdir");
        assert!(git(&["git", "-C", "/w/tree", "add", "."], Some(elsewhere), &[]), "-C names the root");
        assert!(git(&["git", "commit"], Some(Path::new("/w/tree/src")), &[]), "cwd inside the root");
        assert!(git(&["git", "gc"], Some(Path::new("/repo/.git/worktrees/tree")), &[]), "cwd inside the gitdir");
        assert!(git(&["git", "status"], None, &[]), "unreadable cwd: may be working here");
        assert!(!git(&["git", "status"], Some(elsewhere), &["GIT_DIR=/other/.git"]), "a git on another tree");
        // relative paths, resolved as git resolves them (Codex on #5464)
        let repo = Path::new("/repo");
        assert!(git(&["git", "--git-dir=.git/worktrees/tree", "status"], Some(repo), &[]), "relative --git-dir=");
        assert!(git(&["git", "--git-dir", ".git/worktrees/tree", "status"], Some(repo), &[]), "relative --git-dir, separate value");
        assert!(git(&["git", "status"], Some(repo), &["GIT_DIR=.git/worktrees/tree"]), "relative GIT_DIR");
        assert!(git(&["git", "-C", "/repo", "--git-dir=.git/worktrees/tree", "add", "."], Some(elsewhere), &[]), "-C then relative --git-dir");
        assert!(git(&["git", "-C", "/w", "-C", "tree", "status"], Some(elsewhere), &[]), "ordered -C into the root");
        assert!(git(&["git", "--git-dir=.git/worktrees/tree", "-C", "/repo", "status"], Some(elsewhere), &[]), "a -C after --git-dir still applies to it");
        assert!(git(&["git", "--git-dir=../repo/.git/worktrees/tree", "log"], Some(Path::new("/w")), &[]), "a .. path normalizes");
        assert!(!git(&["git", "--git-dir=.git", "status"], Some(Path::new("/other")), &[]), "relative to another tree");
    }
}
