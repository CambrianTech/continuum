//! The deploy claim — "a swap is in flight; do NOT mint a core from the installed image."
//!
//! # The incident this exists for (2026-08-17, measured on this box)
//!
//! `continuum reboot` reported `DEPLOY MISMATCH (#194)`: it shipped `6229b3762`, the core
//! answering was `a87f7c871`. The installed image on disk was verified to contain the NEW
//! sha, so the build and the install were both correct. The core that answered simply was
//! not the one the deploy launched.
//!
//! The mechanism, and it is not a subtle race:
//!
//! 1. `reboot` stops the old core and runs `start-server.sh`, which BUILDS (measured 292s
//!    and 530s on this box; #422 records 772s on another) and only near the END copies the
//!    fresh artifact over `~/.continuum/bin/continuum-core-server`.
//! 2. For that entire multi-minute window, no core answers the socket AND the installed
//!    path still holds the PREVIOUS build.
//! 3. Every `continuum <verb>` calls `ensure_core_running`, whose whole job is to autostart
//!    a core when none answers. In that window it launches the STALE installed image.
//! 4. That stale core binds the socket and answers. The deploy's own freshly-built binary
//!    then loses, and `deploy-verify` correctly reports the old sha.
//!
//! Any client command can do this — a UI poll, `npm start`, a cron, a persona, an operator
//! typing `continuum ping` to see whether the reboot finished. In the observed incident the
//! trigger was a 120-second monitor loop running `persona/roster`. That also positively
//! settles the open question on task #421 ("something respawns a core within seconds of
//! every kill — external supervisor, or me?"): **it was the CLI's own autostart.** There is
//! no mystery daemon.
//!
//! # Why the existing guards could not catch it
//!
//! [`crate::runtime::core_bind_guard::decide`] is a function of (ping answered, core pids
//! running). Mid-build BOTH are false — the honest reading of that state really is "no core
//! is running, and starting one is safe." It is safe in general; it is wrong DURING A
//! DEPLOY, and "a deploy is in flight" is a fact the bind guard cannot observe because it
//! lives in another process. So this is not a stricter bind guard — it is the missing
//! observation, published by the only party that knows it.
//!
//! # The shape, and the failure mode it must not become
//!
//! A claim that outlives its owner would wedge the machine permanently — exactly the
//! `airc daemon start GIVES UP on a contended lock` defect (#355). So the claim is
//! ADVISORY and self-healing: it blocks only while its owner process is demonstrably
//! alive AND the claim is younger than [`CLAIM_MAX_AGE_MS`]. A killed `reboot`, a reused
//! pid, or a hung build all decay back to Clear on their own, and the decision says which
//! so the caller can report it instead of silently ignoring a file.
//!
//! The decision is a pure function of (claim, owner_alive, now) so the `--lib` CI gate
//! covers every row without a filesystem or a process — the same split as
//! [`crate::runtime::core_bind_guard`], for the same reason.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// How long a claim may block before it is treated as abandoned.
///
/// It must comfortably exceed a COLD full build, because blocking is the correct behaviour
/// for the whole of one: measured 292s and 530s here, 772s on BIGMAMA (#422). One hour is
/// far above any of those and still bounded, so the worst case of a `kill -9`'d reboot on a
/// machine that later reuses its pid is a one-hour degradation, never a permanent wedge.
pub const CLAIM_MAX_AGE_MS: u64 = 60 * 60 * 1000;

/// What one deploying process published about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeployClaim {
    /// The pid of the process performing the deploy (the `continuum reboot` invocation).
    pub pid: i32,
    /// Epoch-ms when the claim was taken. Ages the claim; see [`CLAIM_MAX_AGE_MS`].
    pub started_ms: u64,
    /// The build the deploy intends to ship, for a legible refusal message.
    pub target_sha: String,
}

/// The gate an implicit launcher must pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeployGate {
    /// No live deploy. Launching is as safe as the bind guard says it is.
    Clear,
    /// A deploy is genuinely in flight — launching now would mint a core from the
    /// pre-swap installed image and defeat it.
    InProgress {
        pid: i32,
        age_ms: u64,
        target_sha: String,
    },
    /// A claim exists but no longer binds (owner gone, or older than the cap). Treated as
    /// Clear by every caller — but named so the caller can SAY it swept a stale claim
    /// rather than pretending the file was never there.
    Abandoned { pid: i32, age_ms: u64, why: AbandonReason },
}

/// Why an existing claim stopped binding. Kept as data so the message names the cause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbandonReason {
    /// The deploying process is gone (crashed, killed, or simply finished without
    /// clearing — the RAII release should prevent the last one, but never assume it).
    OwnerDead,
    /// Older than [`CLAIM_MAX_AGE_MS`]. Covers a hung build and a recycled pid.
    Expired,
}

impl DeployGate {
    /// True only when a launcher must refuse. `Abandoned` deliberately does NOT block.
    pub fn blocks(&self) -> bool {
        matches!(self, DeployGate::InProgress { .. })
    }
}

/// The whole policy, pure and total. Every row is a real case, not a defensive branch.
pub fn decide(claim: Option<&DeployClaim>, owner_alive: bool, now_ms: u64) -> DeployGate {
    let Some(claim) = claim else {
        return DeployGate::Clear;
    };
    // Saturating: a claim stamped in the future (clock skew, a restored snapshot) reads as
    // age 0 and therefore blocks for its full window rather than being instantly expired.
    // Blocking a little too long is recoverable; launching a stale core is the bug.
    let age_ms = now_ms.saturating_sub(claim.started_ms);
    if !owner_alive {
        return DeployGate::Abandoned {
            pid: claim.pid,
            age_ms,
            why: AbandonReason::OwnerDead,
        };
    }
    if age_ms >= CLAIM_MAX_AGE_MS {
        return DeployGate::Abandoned {
            pid: claim.pid,
            age_ms,
            why: AbandonReason::Expired,
        };
    }
    DeployGate::InProgress {
        pid: claim.pid,
        age_ms,
        target_sha: claim.target_sha.clone(),
    }
}

/// Where the claim lives, given the continuum root (`~/.continuum`).
pub fn claim_path(root: &Path) -> PathBuf {
    root.join("run").join("deploy.claim")
}

/// Publish a claim. Best-effort by design: if the file cannot be written the deploy still
/// proceeds — losing the guard degrades to today's behaviour (a possible stale autostart,
/// which `deploy-verify` still catches), whereas failing the deploy over an unwritable
/// advisory file would turn a hint into an outage.
pub fn write(root: &Path, claim: &DeployClaim) -> std::io::Result<()> {
    let path = claim_path(root);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let body = serde_json::to_string_pretty(claim)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    // Write-then-rename so a reader never observes a half-written claim.
    let tmp = path.with_extension(format!("claim.tmp.{}", claim.pid));
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, &path)
}

/// Read the current claim, if any. A malformed file reads as None: an unparseable advisory
/// note must not block launches forever, and the next deploy overwrites it.
pub fn read(root: &Path) -> Option<DeployClaim> {
    let body = std::fs::read_to_string(claim_path(root)).ok()?;
    serde_json::from_str(&body).ok()
}

/// Drop the claim. Idempotent; a missing file is success.
pub fn clear(root: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(claim_path(root)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim(pid: i32, started_ms: u64) -> DeployClaim {
        DeployClaim {
            pid,
            started_ms,
            target_sha: "deadbeef".into(),
        }
    }

    // what this catches: THE regression — an implicit autostart minting a core from the
    // pre-swap installed image while a deploy is mid-build. Measured 2026-08-17: a 120s
    // monitor loop autostarted the OLD binary during a 530s build, it won the socket, and
    // `deploy-verify` reported build a87f7c871 for a deploy that shipped 6229b3762.
    #[test]
    fn a_live_deploy_blocks_an_implicit_launch_for_the_whole_build() {
        // Well inside a long build: still blocking.
        let g = decide(Some(&claim(4242, 1_000)), true, 1_000 + 530_000);
        assert!(g.blocks(), "a 530s-old live deploy must still block: {g:?}");
        match g {
            DeployGate::InProgress { pid, target_sha, .. } => {
                assert_eq!(pid, 4242);
                assert_eq!(target_sha, "deadbeef", "the refusal names the build it protects");
            }
            other => panic!("expected InProgress, got {other:?}"),
        }
        // No claim at all is the overwhelmingly common case and must be free.
        assert_eq!(decide(None, false, 9_999), DeployGate::Clear);
    }

    // what this catches: the OPPOSITE failure — a claim that outlives its owner wedging the
    // machine, which is the airc `start gives up on a contended lock` defect (#355). A
    // killed reboot, a hung build, and a recycled pid must ALL decay back to launchable.
    #[test]
    fn an_abandoned_claim_never_wedges_the_machine() {
        // Owner died mid-deploy (kill -9, crash, power loss).
        let dead = decide(Some(&claim(4242, 1_000)), false, 2_000);
        assert!(!dead.blocks(), "a dead owner must not block: {dead:?}");
        assert!(matches!(
            dead,
            DeployGate::Abandoned { why: AbandonReason::OwnerDead, pid: 4242, .. }
        ));

        // Owner alive but the claim is older than any real build — hung, or a reused pid.
        let old = decide(Some(&claim(4242, 0)), true, CLAIM_MAX_AGE_MS);
        assert!(!old.blocks(), "an expired claim must not block: {old:?}");
        assert!(matches!(
            old,
            DeployGate::Abandoned { why: AbandonReason::Expired, .. }
        ));

        // One millisecond under the cap still blocks — the boundary is not off by one.
        assert!(decide(Some(&claim(4242, 0)), true, CLAIM_MAX_AGE_MS - 1).blocks());
    }

    // what this catches: clock skew making a claim instantly "expired". A claim stamped in
    // the future must read as brand new (block), not as maximally old (launch anyway) —
    // an underflow here would silently disable the guard on any box with a skewed clock.
    #[test]
    fn a_future_stamped_claim_reads_as_new_not_as_ancient() {
        let g = decide(Some(&claim(7, 10_000)), true, 1_000);
        assert!(g.blocks(), "future-stamped claim must still block: {g:?}");
    }

    // what this catches: round-trip through the real files, including that clear() is
    // idempotent (the RAII release runs on paths where the claim may already be gone) and
    // that a corrupt file cannot block launches forever.
    #[test]
    fn claims_round_trip_and_degrade_safely_on_disk() {
        let dir = std::env::temp_dir().join(format!("continuum-claim-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(read(&dir), None, "no claim on a fresh root");
        let c = claim(1234, 5_000);
        write(&dir, &c).expect("write a claim");
        assert_eq!(read(&dir).as_ref(), Some(&c), "claims round-trip verbatim");

        std::fs::write(claim_path(&dir), "{not json").expect("corrupt the claim");
        assert_eq!(read(&dir), None, "a corrupt claim reads as absent, never as a block");

        clear(&dir).expect("clear");
        clear(&dir).expect("clear is idempotent — the release path may run twice");
        assert_eq!(read(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
