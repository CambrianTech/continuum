//! The development worktrees under the continuum home (`<home>/worktrees`, one checkout per branch an
//! agent works on): a disk pool that gives back finished checkouts when their volume needs room.
//!
//! Untracked until 2026-11-22, when the M5's volume reached 92% with 40 GB of them (98 finished) and
//! no owner. Removing them by hand broke three other checkouts: a worktree's submodule object store
//! can be borrowed by another's (`objects/info/alternates`), and removing the lender takes objects the
//! borrower needs. So every fact that makes a removal safe is git's, read before each one:
//!
//! - clean: no uncommitted change (`git status --porcelain`);
//! - recreatable: its HEAD is on a remote branch (`git for-each-ref --contains HEAD refs/remotes`);
//! - not borrowed from: no other object store's alternates name a store inside its git dir;
//! - idle: not touched within [`IDLE`].
//!
//! Least recently touched first; removal goes through `git worktree remove`, so git's own records stay
//! consistent. A worktree that fails a fact is skipped and named, never forced.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use super::disk_reporters::TrackedDir;
use crate::paging::pool::{ResourcePool, ResourcePoolEntry};

/// How long a worktree must sit untouched before it is a candidate. An agent at work touches its
/// checkout (builds, edits, git) many times a day; a day untouched is a branch it has left.
const IDLE: Duration = Duration::from_secs(24 * 3600);

pub struct DevWorktreePool {
    tracked: Arc<TrackedDir>,
    tier: String,
}

impl DevWorktreePool {
    pub fn new(tracked: Arc<TrackedDir>) -> Self {
        let tier = format!("disk-{}", tracked.name());
        Self { tracked, tier }
    }
}

/// Why a worktree stays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kept {
    Recent,
    NotAWorktree,
    Dirty,
    Unpushed,
    BorrowedFrom(PathBuf),
    RemoveFailed(String),
}

/// The worktrees under `root`, least recently touched first, with their bytes.
fn candidates(root: &Path) -> Vec<(PathBuf, SystemTime, u64)> {
    let mut out: Vec<(PathBuf, SystemTime, u64)> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| {
            let path = entry.path();
            let touched = entry.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH); // unwrap_or: an unreadable time sorts first, and the idle check below still guards it
            let bytes = super::disk_reporters::dir_size_bytes(&path);
            (path, touched, bytes)
        })
        .collect();
    out.sort_by_key(|(_, touched, _)| *touched);
    out
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Every alternates file in the repository whose git dir is `common`: the main repo's submodule stores
/// and every worktree's.
fn alternates_files(common: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: u8, out: &mut Vec<PathBuf>) {
        if depth == 0 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.ends_with("objects/info/alternates") {
                out.push(path);
            } else if path.is_dir() && !path.ends_with("objects/pack") {
                walk(&path, depth - 1, out);
            }
        }
    }
    let mut out = Vec::new();
    for top in [common.join("modules"), common.join("worktrees")] {
        walk(&top, 12, &mut out);
    }
    out
}

/// Whether removing `worktree` is safe now, by git's facts; `Ok(admin dir)` when it is.
fn removable(worktree: &Path, now: SystemTime, touched: SystemTime) -> Result<PathBuf, Kept> {
    if now.duration_since(touched).unwrap_or_default() < IDLE { // unwrap_or_default: a time in the future is recent
        return Err(Kept::Recent);
    }
    let admin = git(worktree, &["rev-parse", "--absolute-git-dir"]).map(PathBuf::from).ok_or(Kept::NotAWorktree)?;
    let common = git(worktree, &["rev-parse", "--path-format=absolute", "--git-common-dir"]).map(PathBuf::from).ok_or(Kept::NotAWorktree)?;
    if admin == common {
        return Err(Kept::NotAWorktree); // a main checkout, never a pool's to remove
    }
    if !git(worktree, &["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| s.is_empty()) {
        return Err(Kept::Dirty);
    }
    if !git(worktree, &["for-each-ref", "--contains", "HEAD", "refs/remotes"]).is_some_and(|s| !s.is_empty()) {
        return Err(Kept::Unpushed);
    }
    for file in alternates_files(&common) {
        if file.starts_with(&admin) {
            continue; // its own stores borrowing elsewhere is no hazard
        }
        let lends = std::fs::read_to_string(&file).unwrap_or_default().lines().any(|line| Path::new(line.trim()).starts_with(&admin)); // unwrap_or_default: an unreadable file names no lender
        if lends {
            return Err(Kept::BorrowedFrom(file));
        }
    }
    Ok(admin)
}

impl ResourcePool for DevWorktreePool {
    fn root(&self) -> Option<PathBuf> {
        Some(self.tracked.path().to_path_buf())
    }

    fn tier_name(&self) -> &str {
        &self.tier
    }

    /// No budget of its own: relief asks it by its volume's pressure.
    fn capacity_bytes(&self) -> u64 {
        0
    }

    fn usage_bytes(&self) -> u64 {
        self.tracked.bytes()
    }

    fn evict_at_least(&self, want_bytes: u64) -> u64 {
        let now = SystemTime::now();
        let mut freed = 0u64;
        for (worktree, touched, bytes) in candidates(self.tracked.path()) {
            if freed >= want_bytes {
                break;
            }
            let kept = match removable(&worktree, now, touched) {
                Ok(_) => match git(&worktree, &["worktree", "remove", &worktree.to_string_lossy()]) {
                    Some(_) => {
                        freed = freed.saturating_add(bytes);
                        continuum_base::probe!(class = "disk.worktrees.removed", worktree = %worktree.display(), bytes, "a finished, clean, pushed worktree nobody borrows from was removed");
                        continue;
                    }
                    None => Kept::RemoveFailed("git worktree remove refused".into()),
                },
                Err(kept) => kept,
            };
            if kept != Kept::Recent {
                continuum_base::probe!(class = "disk.worktrees.kept", worktree = %worktree.display(), why = ?kept, "a worktree stays: removing it is not safe by git's facts");
            }
        }
        freed
    }

    fn snapshot(&self) -> Vec<ResourcePoolEntry> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(dir: &Path, args: &[&str]) {
        let ok = Command::new("git").arg("-C").arg(dir).args(args).output().expect("git runs").status.success(); // expect: the test needs git
        assert!(ok, "git {args:?}");
    }

    // what this catches: the hand cleanup that broke three checkouts (2026-11-22). A clean, pushed
    // worktree nobody borrows from is removable; one with an uncommitted change, one with an unpushed
    // commit, one another object store borrows from, and one touched recently all stay.
    #[test]
    fn only_a_finished_clean_pushed_unborrowed_worktree_is_removable() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let origin = tmp.path().join("origin.git");
        let main = tmp.path().join("main");
        run(tmp.path(), &["init", "-q", "--bare", origin.to_str().unwrap()]);
        run(tmp.path(), &["clone", "-q", origin.to_str().unwrap(), main.to_str().unwrap()]);
        run(&main, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "one"]);
        run(&main, &["push", "-q", "origin", "HEAD:main"]);
        let wt = |name: &str| {
            let path = tmp.path().join("worktrees").join(name);
            run(&main, &["worktree", "add", "-q", "--detach", path.to_str().unwrap(), "HEAD"]);
            path
        };
        let old = SystemTime::now() - IDLE - Duration::from_secs(60);
        let (done, dirty, unpushed, lender) = (wt("done"), wt("dirty"), wt("unpushed"), wt("lender"));

        let admin = removable(&done, SystemTime::now(), old).expect("a finished worktree is removable");
        assert!(admin.ends_with("worktrees/done"));
        assert_eq!(removable(&done, SystemTime::now(), SystemTime::now()), Err(Kept::Recent));

        std::fs::write(dirty.join("tracked.txt"), "a").expect("write");
        run(&dirty, &["add", "tracked.txt"]);
        assert_eq!(removable(&dirty, SystemTime::now(), old), Err(Kept::Dirty));

        run(&unpushed, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "local"]);
        assert_eq!(removable(&unpushed, SystemTime::now(), old), Err(Kept::Unpushed));

        // Another store borrows from the lender's (a submodule store chained into it).
        let lender_admin = PathBuf::from(git(&lender, &["rev-parse", "--absolute-git-dir"]).expect("admin dir"));
        let borrower = main.join(".git/modules/sub/objects/info");
        std::fs::create_dir_all(&borrower).expect("dirs");
        std::fs::write(borrower.join("alternates"), format!("{}\n", lender_admin.join("modules/sub/objects").display())).expect("write");
        assert!(matches!(removable(&lender, SystemTime::now(), old), Err(Kept::BorrowedFrom(_))));
    }
}
