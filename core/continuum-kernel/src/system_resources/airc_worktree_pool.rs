//! airc's card checkouts (`~/.airc/worktrees`, one per claimed card): a disk pool whose every removal
//! is airc's decision. 81 GB of them sat in no pool on the IntelMac (2026-11-24) while relief wanted
//! 74 GB and freed 67 KB.
//!
//! Whether a checkout is finished is a fact only airc holds: a clean, pushed checkout can still be a
//! claimed card or one in review (Codex, card 186ee967), and git's facts cannot tell. So the pool
//! asks airc's own drain, `airc work cleanup --force`, which removes only checkouts of Merged or
//! Closed cards and never a dirty, unpushed or unknown one. A card is known only to the scope that
//! subscribes to its room, the `.airc` of the checkout the worktree was cut from (with the account
//! home, every #continuum-dev card read as unknown and nothing was removable), so the drain runs once
//! per main checkout the worktrees name. Continuum never removes one itself.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::bounded_command::{capture, Captured};
use super::dev_worktree_pool::git;
use super::disk_reporters::TrackedDir;
use super::drives::{detect_drives, drive_holding};
use crate::paging::pool::{ResourcePool, ResourcePoolEntry};

// event-exempt: airc's drain reads each card's PR from GitHub, which may never answer; the drain's
// exit is the event waited on, and an elapsed drain is reported as timed_out and frees nothing.
// derived-or-floor: one drain classified 4 checkouts in 0.8 s on the IntelMac (2026-11-24); a few
// minutes covers hundreds.
const DRAIN_BOUND: Duration = Duration::from_secs(300);

pub struct AircWorktreePool {
    tracked: Arc<TrackedDir>,
    tier: String,
}

impl AircWorktreePool {
    pub fn new(tracked: Arc<TrackedDir>) -> Self {
        let tier = format!("disk-{}", tracked.name());
        Self { tracked, tier }
    }
}

/// The airc home of every main checkout the worktrees under `root` were cut from: `<checkout>/.airc`,
/// the scope that subscribes to the rooms their cards live in.
fn scopes(root: &Path) -> BTreeSet<PathBuf> {
    std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            let worktree = entry.path();
            let common = PathBuf::from(git(&worktree, &["rev-parse", "--path-format=absolute", "--git-common-dir"])?);
            Some(common.parent()?.join(".airc"))
        })
        .filter(|home| home.is_dir())
        .collect()
}

/// The free bytes of the volume holding `path`, read once.
fn available(path: &Path) -> Option<u64> {
    drive_holding(&detect_drives(), path).map(|drive| drive.available_bytes)
}

impl ResourcePool for AircWorktreePool {
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

    /// One drain per scope; what it freed is what the volume gained across the drains (a removed
    /// checkout is not sized first: sizing every candidate was most of a 20 s pass on the M5).
    fn evict_at_least(&self, _want_bytes: u64) -> u64 {
        let root = self.tracked.path().to_path_buf();
        let Some(before) = available(&root) else {
            continuum_base::probe!(class = "disk.airc_worktrees.no_volume", root = %root.display(), "no volume holds airc's worktrees; nothing is drained");
            return 0;
        };
        for home in scopes(&root) {
            let Some(home_arg) = home.to_str() else { continue }; // a non-UTF-8 home is never handed to airc
            let drained = capture("airc", &["work", "cleanup", "--force", "--home", home_arg], DRAIN_BOUND);
            let outcome = match &drained {
                Captured::Exited { code: Some(0), .. } => "ok",
                Captured::Exited { .. } => "failed",
                Captured::TimedOut => "timed_out",
                Captured::Unstartable { .. } => "unstartable",
            };
            continuum_base::probe!(class = "disk.airc_worktrees.drained", home = %home.display(), outcome, "airc drained the checkouts of its finished cards");
        }
        let freed = available(&root).map_or(0, |after| after.saturating_sub(before));
        if freed > 0 {
            self.tracked.record_freed(freed);
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
        let ok = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().expect("git runs").status.success(); // expect: the test needs git
        assert!(ok, "git {args:?}");
    }

    // what this catches: the drain run under the account home, which knows no project's cards, so
    // every checkout read as an unknown card and nothing was ever removable (the IntelMac,
    // 2026-11-24). Each worktree names the scope of the checkout it was cut from; a checkout with no
    // `.airc` is no scope, and a plain directory is skipped.
    #[test]
    fn each_worktree_names_the_scope_of_the_checkout_it_was_cut_from() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let root = tmp.path().join("worktrees");
        let cut = |project: &str, scoped: bool| {
            let main = tmp.path().join(project);
            std::fs::create_dir_all(&main).expect("dir");
            run(&main, &["init", "-q"]);
            run(&main, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "one"]);
            if scoped {
                std::fs::create_dir_all(main.join(".airc")).expect("scope");
            }
            for card in ["a", "b"] {
                let at = root.join(format!("{project}-{card}"));
                run(&main, &["worktree", "add", "-q", "--detach", at.to_str().expect("utf-8"), "HEAD"]);
            }
            main.canonicalize().expect("main").join(".airc")
        };
        let continuum = cut("continuum", true);
        let airc = cut("airc", true);
        cut("unscoped", false);
        std::fs::create_dir_all(root.join("not-a-worktree")).expect("dir");
        assert_eq!(scopes(&root), BTreeSet::from([continuum, airc]), "one scope per project, each once");
    }
}
