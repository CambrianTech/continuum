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
use super::disk_reporters::{dir_size_bytes, TrackedDir};
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

/// The checkouts under `root`.
fn checkouts(root: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(root).into_iter().flatten().flatten().map(|entry| entry.path()).filter(|path| path.is_dir()).collect()
}

/// The airc home of every main checkout `worktrees` were cut from: `<checkout>/.airc`, the scope
/// that subscribes to the rooms their cards live in.
fn scopes(worktrees: &[PathBuf]) -> BTreeSet<PathBuf> {
    worktrees
        .iter()
        .filter_map(|worktree| {
            let common = PathBuf::from(git(worktree, &["rev-parse", "--path-format=absolute", "--git-common-dir"])?);
            Some(common.parent()?.join(".airc"))
        })
        .filter(|home| home.is_dir())
        .collect()
}

/// This pool's receipt: the bytes of the checkouts measured before the drain that are gone after it.
/// Never the volume's gain, which another pool's removals move too (Codex on 186ee967).
fn removed(sized: &[(PathBuf, u64)]) -> u64 {
    sized.iter().filter(|(checkout, _)| !checkout.exists()).map(|(_, bytes)| bytes).sum()
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

    /// One drain per scope. Each checkout is sized before the drains (a walk per relief pass with a
    /// scope to drain; relief asks again only on news), and the receipt is the ones that went.
    fn evict_at_least(&self, _want_bytes: u64) -> u64 {
        let checkouts = checkouts(self.tracked.path());
        let homes = scopes(&checkouts);
        if homes.is_empty() {
            return 0;
        }
        let sized: Vec<(PathBuf, u64)> = checkouts.into_iter().map(|checkout| {
            let bytes = dir_size_bytes(&checkout);
            (checkout, bytes)
        }).collect();
        let mut drained = false;
        for home in homes {
            let Some(home_arg) = home.to_str() else { continue }; // a non-UTF-8 home is never handed to airc
            let outcome = match capture("airc", &["work", "cleanup", "--force", "--home", home_arg], DRAIN_BOUND) {
                Captured::Exited { code: Some(0), .. } => "ok",
                Captured::Exited { .. } => "failed",
                Captured::TimedOut => "timed_out",
                Captured::Unstartable { .. } => "unstartable",
            };
            drained |= outcome == "ok";
            continuum_base::probe!(class = "disk.airc_worktrees.drained", home = %home.display(), outcome, "airc drained the checkouts of its finished cards");
        }
        if !drained {
            return 0;
        }
        let freed = removed(&sized);
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
            main.join(".airc")
        };
        let continuum = cut("continuum", true).canonicalize().expect("scope");
        let airc = cut("airc", true).canonicalize().expect("scope");
        cut("unscoped", false);
        std::fs::create_dir_all(root.join("not-a-worktree")).expect("dir");
        // git names a path its own way (on Windows `C:/...`, never the `\\?\` form canonicalize
        // gives), so both sides are compared as the filesystem resolves them (Codex on #6253).
        let found: BTreeSet<PathBuf> = scopes(&checkouts(&root)).iter().map(|home| home.canonicalize().expect("scope")).collect();
        assert_eq!(found, BTreeSet::from([continuum, airc]), "one scope per project, each once");
    }

    // what this catches (Codex on 186ee967): the pool credited itself with the volume's whole gain,
    // so another pool's removals during its pass read as worktrees relieved while every checkout
    // stayed. The receipt is the measured bytes of exactly the checkouts that went.
    #[test]
    fn the_receipt_is_the_checkouts_that_went_not_the_volumes_gain() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let checkout = |name: &str, bytes: usize| {
            let at = tmp.path().join(name);
            std::fs::create_dir_all(&at).expect("dir");
            std::fs::write(at.join("f"), vec![0u8; bytes]).expect("write");
            (at.clone(), dir_size_bytes(&at))
        };
        let sized = vec![checkout("finished", 3000), checkout("in-review", 5000)];
        let elsewhere = tmp.path().join("another-pools-cache");
        std::fs::write(&elsewhere, vec![0u8; 9000]).expect("write");
        assert_eq!(removed(&sized), 0, "nothing went: no receipt");

        std::fs::remove_dir_all(&sized[0].0).expect("airc removes the finished checkout");
        std::fs::remove_file(&elsewhere).expect("another pool frees room on the same volume");
        assert_eq!(removed(&sized), sized[0].1, "only the finished checkout's bytes");
        assert!(sized[0].1 >= 3000);
    }
}
