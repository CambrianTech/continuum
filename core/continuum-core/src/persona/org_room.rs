//! THE ORG ROOM IS A FACT THE CORE HOLDS — never the launcher's working directory.
//!
//! The org room (`#cambriantech` for `github.com/CambrianTech/continuum`) is the room the
//! fleet speaks in: the hourly `[health]` line, the fleet transitions, the operator peer's
//! project tree. Two call sites derived it from the core PROCESS'S cwd
//! (`airc_lib::JoinContext::from_cwd(current_dir())` → the git remote's owner). That is
//! right only when the launcher happens to run the core from the checkout — the script
//! path. A supervised core does not: launchd runs it with `WorkingDirectory ~/.continuum`
//! (the #4228 plist), the Windows S4U task from its own directory — no remote there, so
//! the org was `None`, `say_in_org_room` returned silently, and the two supervised nodes
//! judged their hour every hour for weeks and never said it (card 11b66313: the 5090
//! invisible since 06:40Z; IntelMac's `citizen.health.hour` rows at 06:08/07:08/09:08Z,
//! none in the room). The M5 posted because its launcher's cwd was the checkout.
//!
//! One resolver: the org is read off the TRACKED CHECKOUT — the durable clone the deploy
//! decision already stands on ([`crate::runtime::tracked_checkout`]) — through the same
//! `JoinContext` rule `airc join` applies, so a core launched from `/`, `~/.continuum`, or
//! a lease worktree names the same room as one launched from the repo root. No checkout
//! at all (an installed product with no clone) is a NAMED absence, never a silent one.

use std::path::Path;

use crate::runtime::tracked_checkout::durable_checkout_of;

/// Why no org room could be named — each variant is what the probe says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrgRoomAbsence {
    /// Nothing names a checkout on this node (`CONTINUUM_TRACK_REPO_DIR` unset, and the
    /// build root is not a candidate).
    NoCheckout,
    /// A checkout was named but cannot be resolved (gone, or not a repo).
    Checkout(String),
    /// The checkout resolves but its origin remote names no owner (no remote, a local
    /// path remote, an unparseable URL).
    NoRemoteOwner(std::path::PathBuf),
}

impl std::fmt::Display for OrgRoomAbsence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoCheckout => write!(f, "no checkout is tracked on this node (set CONTINUUM_TRACK_REPO_DIR)"),
            Self::Checkout(why) => write!(f, "the tracked checkout cannot be resolved: {why}"),
            Self::NoRemoteOwner(p) => write!(f, "the checkout at {} has no origin remote owner to name an org room", p.display()),
        }
    }
}

/// PURE over a directory: the org channel the checkout at `checkout` names — its origin
/// remote's owner, by the rule `airc join` applies (`JoinContext`). `None` when the
/// directory names no remote owner. Takes the checkout explicitly so a test can hand it a
/// temp repo while the process's cwd is somewhere else entirely.
///
/// Resolved through the DURABLE checkout first ([`durable_checkout_of`]): a lease
/// worktree's `.git` is a `gitdir:` pointer into `<main>/.git/worktrees/<name>/`, which
/// carries no `config`, so `JoinContext` read straight off a worktree names no org at all
/// (`airc join` from a lease lands in the lobby — airc-lib's gap, named here). The main
/// working tree carries the remote; every form of the same clone names the same room.
pub fn org_channel_of(checkout: &Path) -> Option<airc_lib::ChannelName> {
    let durable = durable_checkout_of(checkout).ok()?;
    airc_lib::JoinContext::from_cwd(&durable)
        .channels
        .into_iter()
        .find(|c| c.as_str() != airc_lib::GENERAL_CHANNEL)
}

/// The org room this node speaks in, as a fact resolved now from the tracked checkout.
pub fn org_channel() -> Result<airc_lib::ChannelName, OrgRoomAbsence> {
    let checkout = match crate::runtime::tracked_checkout::tracked_checkout() {
        Ok(Some(dir)) => dir,
        Ok(None) => return Err(OrgRoomAbsence::NoCheckout),
        Err(e) => return Err(OrgRoomAbsence::Checkout(e.to_string())),
    };
    org_channel_of(&checkout).ok_or(OrgRoomAbsence::NoRemoteOwner(checkout))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().expect("git runs");
        assert!(out.status.success(), "git {args:?} in {}: {}", dir.display(), String::from_utf8_lossy(&out.stderr));
    }

    // what this catches (card 11b66313): the org room is read off the CHECKOUT handed in,
    // not off wherever the process happens to be running — a core launched from a
    // directory that is no checkout (launchd's ~/.continuum, the S4U task's dir) names
    // the same room as one launched from the repo root; a checkout with no remote owner
    // is a named absence.
    #[test]
    fn the_org_room_is_the_checkouts_remote_owner_wherever_the_process_runs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let checkout = tmp.path().join("clone");
        std::fs::create_dir(&checkout).unwrap();
        git(&checkout, &["init", "-q"]);
        git(&checkout, &["remote", "add", "origin", "https://github.com/AcmeOrg/product.git"]);
        // The process's cwd is NOT the checkout (the test binary's cwd is the crate dir);
        // the resolver never consults it.
        assert_ne!(std::env::current_dir().unwrap(), checkout);
        let org = org_channel_of(&checkout).expect("the remote names an owner");
        assert_eq!(org.as_str(), "acmeorg", "the org room is the remote owner, lowercased as airc join does");
        // A worktree of that clone names the same room (the durable resolver hands the
        // main checkout in; this pins that the rule holds on the worktree path too).
        git(&checkout, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "root"]);
        let lease = tmp.path().join("lease");
        git(&checkout, &["worktree", "add", "-q", lease.to_str().unwrap()]);
        assert_eq!(org_channel_of(&lease).map(|c| c.as_str().to_string()), Some("acmeorg".to_string()));
        // No remote owner: a named absence, not a room.
        let bare = tmp.path().join("bare");
        std::fs::create_dir(&bare).unwrap();
        git(&bare, &["init", "-q"]);
        assert_eq!(org_channel_of(&bare), None, "no origin remote → no org room");
        let nowhere = tmp.path().join("nowhere");
        std::fs::create_dir(&nowhere).unwrap();
        assert_eq!(org_channel_of(&nowhere), None, "not a checkout → no org room");
        // The airc-lib gap this resolver closes: read straight off the worktree, the
        // JoinContext names nothing (a worktree gitdir carries no config).
        assert!(
            airc_lib::JoinContext::from_cwd(&lease).channels.iter().all(|c| c.as_str() == airc_lib::GENERAL_CHANNEL),
            "airc-lib reads no org off a worktree; if this starts passing, the resolver's detour is no longer needed"
        );
    }
}
