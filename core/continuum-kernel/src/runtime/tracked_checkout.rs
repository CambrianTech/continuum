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
//! 2. The BUILD-TIME candidate resolves to its DURABLE checkout: the main working tree of the
//!    repo it belongs to (`git rev-parse --git-common-dir`'s parent). A lease worktree is temp
//!    by definition; the main checkout outlives every lease. A plain checkout resolves to
//!    itself.
//! 3. A RECORDED checkout is the deploy checkout as recorded, worktree or not. The deploy
//!    checks tips out in it (`deploy-consume`: `git checkout --detach <tip>`) and refuses it
//!    while it holds tracked edits, so it must be a tree nobody works in. On a development
//!    node the main working tree is where agents work: the M5's recorded deploy worktree was
//!    collapsed to it, the consumer moved that tree to a detached 8e38ada8d on 2026-10-25,
//!    and from then on every published tip met an agent's edits there (`RefuseDirty`), so
//!    every deploy was done by hand from `wt/deploy-latest` (2026-11-14, three in a few
//!    hours). A recorded tree that is gone is named GONE by path, never re-resolved.
//! 4. Resolved on EVERY read (one `exists()` and one bounded git call), never cached: a
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

/// The candidate before resolution, by where it came from: the two resolve differently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Candidate {
    /// `CONTINUUM_TRACK_REPO_DIR`: the deploy checkout someone declared. Taken as recorded.
    Recorded(PathBuf),
    /// The source root this binary was built from: possibly a lease, so resolved to the main tree.
    BuildRoot(PathBuf),
}

/// The candidate before resolution: the recorded fact, else the build-time source root.
/// `None` only when neither names a path (a binary built outside any checkout with nothing
/// recorded) — the tracker then has no source, honestly.
pub fn candidate() -> Option<Candidate> {
    continuum_base::config_env::read(TRACK_REPO_DIR_KEY)
        .map(|dir| Candidate::Recorded(PathBuf::from(dir)))
        .or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(2)
                .map(|p| Candidate::BuildRoot(p.to_path_buf()))
        })
}

/// A recorded checkout's own working tree (`--show-toplevel`): a worktree stays itself.
pub fn recorded_checkout_of(recorded: &Path) -> Result<PathBuf, CheckoutError> {
    if !recorded.is_dir() {
        return Err(CheckoutError::Gone(recorded.to_path_buf()));
    }
    let dir = recorded.to_string_lossy().into_owned();
    let out = crate::system_resources::bounded_command::probe(
        "git",
        &["-C", &dir, "rev-parse", "--path-format=absolute", "--show-toplevel"],
        GIT_TIMEOUT,
    );
    match out.stdout_if_ok().map(str::trim).filter(|top| !top.is_empty()) {
        Some(top) => Ok(PathBuf::from(top)),
        None => Err(CheckoutError::NotARepo(recorded.to_path_buf(), out.outcome().to_string())),
    }
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
        Some(Candidate::Recorded(dir)) => recorded_checkout_of(&dir).map(Some),
        Some(Candidate::BuildRoot(dir)) => durable_checkout_of(&dir).map(Some),
        None => Ok(None),
    }
}

/// Record the durable checkout as the fact every owner reads from now on. Called by
/// `continuum install`'s core arm with the checkout it just deployed from, so a core built
/// in a lease tracks the clone the lease came from, not the lease.
pub fn record(durable: &Path) -> Result<(), String> {
    continuum_base::config_env::upsert(TRACK_REPO_DIR_KEY, &durable.to_string_lossy())
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

    // what this catches (the M5, 2026-10-25 to 2026-11-14): a RECORDED deploy worktree collapsed
    // to the main working tree agents edit, so the consumer detached that tree and every
    // published tip met their edits (RefuseDirty); every deploy was then done by hand. A
    // recorded worktree is the deploy checkout itself; one that is gone is named GONE.
    #[test]
    fn a_recorded_deploy_worktree_is_tracked_as_itself_never_the_main_tree() {
        let Ok(tmp) = tempfile::tempdir() else { panic!("a temp dir") };
        let main = tmp.path().join("main");
        assert!(std::fs::create_dir(&main).is_ok());
        git(&main, &["init", "-q", "-b", "canary"]);
        git(&main, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "root"]);
        let deploy = tmp.path().join("deploy-latest");
        git(&main, &["worktree", "add", "-q", "--detach", &deploy.to_string_lossy()]);
        let canon = |p: &Path| p.canonicalize().ok();
        let Ok(tracked) = recorded_checkout_of(&deploy) else { panic!("a recorded worktree resolves") };
        assert_eq!(canon(&tracked), canon(&deploy), "the recorded deploy worktree is tracked as itself");
        assert_ne!(canon(&tracked), canon(&main), "never the main working tree agents edit");
        git(&main, &["worktree", "remove", "--force", &deploy.to_string_lossy()]);
        assert_eq!(recorded_checkout_of(&deploy), Err(CheckoutError::Gone(deploy.clone())), "a recorded tree that is gone is named GONE");
    }
}
