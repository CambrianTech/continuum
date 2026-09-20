//! THE CHECKOUT THE DEPLOY DECISION TRACKS — one resolver for every owner.
//!
//! Two copies of this decision existed (`modules/deploy_tracker.rs::from_env`,
//! `bin/continuum.rs::tracked_repo_dir`), both reading `CONTINUUM_TRACK_REPO_DIR` else the
//! source root this binary was BUILT from (`CARGO_MANIFEST_DIR`), and the tracker's copy
//! resolved it once at construction. Card 790c6bcb (IntelMac, 2026-09-19 21:38Z → 02:0xZ):
//! the running core had been built in a lease worktree, the merge pruned the worktree, and
//! for 4.5 hours every tick read `SourceUnavailable { "git fetch canary absent" }` — a git
//! failure, not the truth, which was "the checkout I track no longer exists". Every lease
//! build on every node carries the same dead pointer (the 5090's `uu install` from a lease
//! is the same shape).
//!
//! The rule now:
//! 1. The CANDIDATE is `CONTINUUM_TRACK_REPO_DIR` (a recorded fact — `continuum install`
//!    records it, see [`record`]) else the build-time source root — a convenience for a
//!    fresh clone that has never installed, never the fact itself.
//! 2. The candidate resolves to its DURABLE checkout: the main working tree of the repo it
//!    belongs to (`git rev-parse --git-common-dir`'s parent). A worktree is temp by
//!    definition; the main checkout outlives every lease. A plain checkout resolves to
//!    itself.
//! 3. Resolved on EVERY read (one `exists()` and one bounded git call), never cached: a
//!    checkout that vanishes is named by path on the next tick, not mislabeled as a fetch
//!    failure forever.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// The config.env key `continuum install` records and every owner reads.
pub const TRACK_REPO_DIR_KEY: &str = "CONTINUUM_TRACK_REPO_DIR";
const GIT_TIMEOUT: Duration = Duration::from_secs(30);

/// Why there is no checkout to track — each variant names what an operator would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckoutError {
    /// The candidate path no longer exists (a pruned lease worktree, a moved clone).
    Gone(PathBuf),
    /// The candidate exists but git cannot name its common dir: not a checkout.
    NotARepo(PathBuf, String),
}

impl std::fmt::Display for CheckoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gone(p) => write!(f, "checkout {} is gone (set {TRACK_REPO_DIR_KEY} to a durable clone)", p.display()),
            Self::NotARepo(p, why) => write!(f, "{} is not a git checkout ({why}); set {TRACK_REPO_DIR_KEY}", p.display()),
        }
    }
}

/// The candidate before resolution: the recorded fact, else the build-time source root.
/// `None` only when neither names a path (a binary built outside any checkout with nothing
/// recorded) — the tracker then has no source, honestly.
pub fn candidate() -> Option<PathBuf> {
    crate::config_env::read(TRACK_REPO_DIR_KEY)
        .map(PathBuf::from)
        .or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(2)
                .map(|p| p.to_path_buf())
        })
}

/// The durable checkout a candidate belongs to: the repo's MAIN working tree. Pure over
/// the filesystem + one bounded git call, so a test can build a repo, add a worktree and
/// assert both resolve to the main tree.
pub fn durable_checkout_of(candidate: &Path) -> Result<PathBuf, CheckoutError> {
    if !candidate.is_dir() {
        return Err(CheckoutError::Gone(candidate.to_path_buf()));
    }
    let dir = candidate.to_string_lossy().into_owned();
    let out = crate::system_resources::bounded_command::probe(
        "git",
        &["-C", &dir, "rev-parse", "--path-format=absolute", "--git-common-dir"],
        GIT_TIMEOUT,
    );
    let Some(common) = out.stdout_if_ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) else {
        return Err(CheckoutError::NotARepo(candidate.to_path_buf(), out.outcome().to_string()));
    };
    // `<main>/.git` for a checkout or any of its worktrees; its parent is the main tree.
    let common = PathBuf::from(common);
    match common.parent() {
        Some(main) if main.is_dir() => Ok(main.to_path_buf()),
        _ => Err(CheckoutError::NotARepo(candidate.to_path_buf(), format!("common dir {} has no parent tree", common.display()))),
    }
}

/// The checkout to track, resolved now. `Ok(None)` = nothing names a candidate.
pub fn tracked_checkout() -> Result<Option<PathBuf>, CheckoutError> {
    match candidate() {
        Some(c) => durable_checkout_of(&c).map(Some),
        None => Ok(None),
    }
}

/// Record the durable checkout as the fact every owner reads from now on. Called by
/// `continuum install`'s core arm with the checkout it just deployed from, so a core built
/// in a lease tracks the clone the lease came from, not the lease.
pub fn record(durable: &Path) -> Result<(), String> {
    crate::config_env::upsert(TRACK_REPO_DIR_KEY, &durable.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().expect("git runs");
        assert!(out.status.success(), "git {args:?} in {}: {}", dir.display(), String::from_utf8_lossy(&out.stderr));
    }

    // what this catches (card 790c6bcb): a core built in a lease worktree tracking the lease.
    // A worktree resolves to the MAIN checkout it belongs to; the main checkout resolves to
    // itself; a pruned worktree is named as GONE by path (not as a git failure); a plain
    // directory is named as not a repo.
    #[test]
    fn a_worktree_resolves_to_the_durable_main_checkout_and_a_pruned_one_is_named_gone() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let main = tmp.path().join("main");
        std::fs::create_dir(&main).unwrap();
        git(&main, &["init", "-q", "-b", "canary"]);
        git(&main, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "root"]);
        let lease = tmp.path().join("lease");
        git(&main, &["worktree", "add", "-q", lease.to_str().unwrap()]);

        let main_canon = main.canonicalize().unwrap();
        assert_eq!(durable_checkout_of(&lease).unwrap().canonicalize().unwrap(), main_canon, "a lease resolves to its main checkout");
        assert_eq!(durable_checkout_of(&main).unwrap().canonicalize().unwrap(), main_canon, "the main checkout resolves to itself");

        git(&main, &["worktree", "remove", "--force", lease.to_str().unwrap()]);
        assert_eq!(durable_checkout_of(&lease), Err(CheckoutError::Gone(lease.clone())), "a pruned lease is GONE by path");
        assert!(durable_checkout_of(&lease).unwrap_err().to_string().contains(TRACK_REPO_DIR_KEY), "the error names the fix");

        let plain = tmp.path().join("plain");
        std::fs::create_dir(&plain).unwrap();
        assert!(matches!(durable_checkout_of(&plain), Err(CheckoutError::NotARepo(p, _)) if p == plain), "a directory that is no checkout is named as such");
    }
}
