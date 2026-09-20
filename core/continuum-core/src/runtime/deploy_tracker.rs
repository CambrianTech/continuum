//! THE DEPLOY TRACKER — the fleet follows the deployable tip on cadence, in Rust, on
//! every platform.
//!
//! Joel, 2026-09-17: "It's supposed to be rust for infrastructure not shell jacks" /
//! "How's this gonna work on windows." The tracking DECISION lived in
//! `tools/scripts/track-canary.sh` — `#!/usr/bin/env bash` installed as a launchd
//! agent, so it ran on macOS/Linux and DID NOT EXIST on the Windows 5090, which had no
//! deploy owner at all and paid a manual-reboot tax. The deploy PRIMITIVES were already
//! cross-platform Rust ([`super::deploy_claim`], [`super::deploy_provenance`], the
//! warm-build reboot); only the decision was shell. This module is that decision, in the
//! core: one binary, identical on Windows/Mac/Linux (the CBAR/SubstrateGovernor promise —
//! same code, different governor policy).
//!
//! This file is the LOAD-BEARING CORE (slice 1): a pure [`decide`] over the guards, the
//! [`Hold`] fact, the [`DeployRequest`] seam, and a [`DeploySource`] trait so the tip +
//! its check-state come from behind an adapter (git remote / gh, with the
//! gh-is-never-on-the-liveness-path fallbacks — a source outage DEGRADES, it never
//! wedges). Nothing here touches a live fleet: the `ServiceModule` tick and the reboot
//! ACTION wire onto this in later slices, so the decision is proven in tests first.
//!
//! THE SEAM (Joel's split, 2026-09-17): this module OWNS THE DECISION and, on a deploy,
//! records a [`DeployRequest`] — it never spawns a process or reboots. A SUPERVISOR
//! (BigMama's lane, card 82af11f5 — supervised core via launchd / systemd / a Windows
//! service) consumes the request and performs the cross-platform build + swap + re-exec.
//! Decoupling the portable decision from the OS-integration action is what makes the
//! whole thing work on Windows.
//!
//! The [`Hold`] carries an EXPIRY — the fix for the unbounded-hold class (a 3-day-stale
//! `deploy-hold` file paused the whole fleet, card ee76c0df; `ServingSteadyHold` held a
//! re-home 255 ticks, same shape). A hold that names "until X is fixed" with no way to
//! detect X is fixed outlives its cause forever. Here a hold is a substrate fact with a
//! reason and an optional TTL; [`Hold::is_active`] answers per tick, and a hold older
//! than [`STALE_HOLD_WARN`] is reported as a WARNING, never a silent skip.

use std::time::Duration;

/// A held deploy: an operator (or a subsystem, e.g. a benchmark round asking for quiet)
/// paused tracking, with a reason and an optional time-to-live. A hold with no TTL is
/// honored but flagged stale after [`STALE_HOLD_WARN`] so it can never silently pause the
/// fleet for days.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Hold {
    pub reason: String,
    /// Unix ms the hold was written.
    pub created_ms: u64,
    /// Optional lifetime; `None` = until cleared by hand (and flagged stale meanwhile).
    #[serde(default)]
    pub ttl_ms: Option<u64>,
}

/// A hold not cleared or expired within this long is surfaced as a warning: the fleet
/// has stopped tracking and a human should confirm the hold is still real.
pub const STALE_HOLD_WARN: Duration = Duration::from_secs(6 * 3600);

impl Hold {
    /// Whether the hold still pauses deploys at `now_ms`. A TTL'd hold expires on its
    /// own (the cause was time-bounded); a TTL-less hold stays active until cleared.
    pub fn is_active(&self, now_ms: u64) -> bool {
        match self.ttl_ms {
            Some(ttl) => now_ms.saturating_sub(self.created_ms) < ttl,
            None => true,
        }
    }

    /// Whether the hold is old enough to warn about (a TTL-less hold left standing too
    /// long, or a TTL'd one that somehow outlived its window without being cleared).
    pub fn is_stale(&self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.created_ms) >= STALE_HOLD_WARN.as_millis() as u64
    }
}

/// The check-state of the tip, from the deploy source. `Unknown` is distinct from `Red`:
/// gh unreachable or no runs yet must WAIT, never deploy and never be read as failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checks {
    Green,
    Pending,
    Red,
    Unknown,
}

/// The seam between the DECISION (this module) and the ACTION (the supervisor, card
/// 82af11f5). When [`decide`] returns [`DeployVerdict::Deploy`] the `ServiceModule`
/// records one of these; the supervisor consumes it and performs the build + swap +
/// re-exec cross-platform. Keyed by `tip_sha` so re-recording the same tip is idempotent
/// (the module does not re-emit an identical pending request every tick).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeployRequest {
    /// The verified-green tip the supervisor should build and serve.
    pub tip_sha: String,
    /// Unix ms the request was recorded (for staleness / audit; the supervisor may
    /// refuse a request older than its own bound).
    pub requested_ms: u64,
}

impl DeployRequest {
    pub fn new(tip_sha: impl Into<String>, now_ms: u64) -> Self {
        Self { tip_sha: tip_sha.into(), requested_ms: now_ms }
    }
}

/// What one tick of the tracker decides. Every non-deploy outcome names WHY, so the
/// probe stream reads the whole decision history (never a silent skip — the class that
/// paused the fleet for three days).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeployVerdict {
    /// The running build already matches the tip — nothing to do.
    UpToDate,
    /// Deploy: the running build differs from a green tip and every guard is clear. The
    /// module turns this into a [`DeployRequest`] for the supervisor.
    Deploy { tip_sha: String },
    /// The source could not be read (offline/degraded); the running build stands.
    SourceUnavailable { why: String },
    /// The core is not answering — starting it is a different owner's job, not a deploy.
    CoreNotAnswering,
    /// A hold is active; carries the reason and whether it is stale (warn).
    Held { reason: String, stale: bool },
    /// A build/reboot is already in flight (a deploy claim blocks).
    BuildInFlight,
    /// The tip's checks are not green yet.
    ChecksPending,
    /// The tip is red — a red tip is never deployed.
    RefuseRed,
    /// The tip's checks are unknown (source reachable but no verdict) — wait.
    ChecksUnknown,
    /// The deploy tree has uncommitted changes — the deploy tree is the deploy tree.
    RefuseDirty,
}

/// Everything one tick knows, already gathered off the hot path (the `ServiceModule`
/// tick stages these via `spawn_blocking` + a bounded timeout; `decide` is pure).
#[derive(Debug, Clone)]
pub struct TickInputs {
    /// The running core's build SHA, or `None` if the core is not answering.
    pub running_sha: Option<String>,
    /// The deployable tip's SHA, or `None` if the source could not be read.
    pub tip_sha: Option<String>,
    /// Why the source was unreadable (only meaningful when `tip_sha` is `None`).
    pub source_error: Option<String>,
    pub checks: Checks,
    pub hold: Option<Hold>,
    pub build_in_flight: bool,
    pub tree_dirty: bool,
    pub now_ms: u64,
}

/// SHAs compare on their first [`SHA_PREFIX`] hex chars — a short SHA from `ping`
/// matches a full SHA from `git rev-parse`.
pub const SHA_PREFIX: usize = 9;

fn sha_eq(a: &str, b: &str) -> bool {
    let n = SHA_PREFIX.min(a.len()).min(b.len());
    n > 0 && a.as_bytes()[..n] == b.as_bytes()[..n]
}

/// The pure decision — the guards of `track-canary.sh`, in the same order, as one
/// testable function. The order is deliberate: source/liveness before hold before
/// in-flight before checks before tree, so the probe names the FIRST reason that
/// applies, and a red tip is refused before a dirty tree is even consulted.
pub fn decide(inp: &TickInputs) -> DeployVerdict {
    let Some(tip) = inp.tip_sha.as_deref() else {
        return DeployVerdict::SourceUnavailable {
            why: inp.source_error.clone().unwrap_or_else(|| "source unreadable".into()),
        };
    };
    let Some(running) = inp.running_sha.as_deref() else {
        return DeployVerdict::CoreNotAnswering;
    };
    if sha_eq(running, tip) {
        return DeployVerdict::UpToDate;
    }
    if let Some(hold) = &inp.hold {
        if hold.is_active(inp.now_ms) {
            return DeployVerdict::Held {
                reason: hold.reason.clone(),
                stale: hold.is_stale(inp.now_ms),
            };
        }
    }
    if inp.build_in_flight {
        return DeployVerdict::BuildInFlight;
    }
    match inp.checks {
        Checks::Pending => return DeployVerdict::ChecksPending,
        Checks::Red => return DeployVerdict::RefuseRed,
        Checks::Unknown => return DeployVerdict::ChecksUnknown,
        Checks::Green => {}
    }
    if inp.tree_dirty {
        return DeployVerdict::RefuseDirty;
    }
    DeployVerdict::Deploy { tip_sha: tip.to_string() }
}

/// What a recorded [`DeployRequest`] MEANS once the running build is known — the other
/// half of the seam.
///
/// [`decide`] writes a request and, until now, nothing ever read one back: the only
/// non-test references to [`DeployRequest`] in the tree were the struct and the write
/// site. So "the deploy I asked for has arrived" was not a fact the substrate held, and
/// the only way to learn it was to poll `continuum ping` until the sha changed — measured
/// 2026-09-17, two hours of an operator re-deriving by hand a comparison the node could
/// make every tick from two values it already has.
///
/// Worse than the polling: a request that never took is INDISTINGUISHABLE from one still
/// working. `continuum reboot` returns exit 1 while its build continues detached (card
/// 79faf35d), so "recorded a request, nothing happened" and "recorded a request, it is
/// compiling" looked the same from outside. [`Stranded`](Self::Stranded) is that
/// difference made readable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestOutcome {
    /// No request on record — nothing was asked for, nothing is owed.
    Nothing,
    /// The running build IS the requested tip: the deploy ARRIVED. Carries how long it
    /// took, so the cost of a deploy on this tier is a measured number rather than an
    /// impression (this box: ~40-120 min; the M5: ~75).
    Settled { tip_sha: String, waited_ms: u64 },
    /// Requested, not yet running, and a build claim is active — working. Expected.
    InFlight { tip_sha: String, elapsed_ms: u64 },
    /// Requested, not yet running, and NOTHING is building. The request did not take:
    /// the supervisor never ran it, or its build died. This is the state that used to be
    /// silent, and it is the one worth waking someone for.
    Stranded { tip_sha: String, elapsed_ms: u64 },
}

/// Do two git shas name the same commit when one may be abbreviated?
///
/// NOT `==`. `running_sha()` is the short form the build stamps (`90194bead`) while a
/// [`DeployRequest`] records the full tip (`90194beadf4fbf9d8e08a55c9262a3860c37a76e`) —
/// an equality check would never fire, [`reconcile_request`] would never return
/// [`RequestOutcome::Settled`], and the whole reconcile would be correct code on a branch
/// nothing reaches. Compare as a prefix, shorter against longer, and require enough
/// characters that a coincidence is not a match.
pub fn same_commit(a: &str, b: &str) -> bool {
    const MIN_ABBREV: usize = 7; // git's own floor for an unambiguous short sha
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    short.len() >= MIN_ABBREV && long.starts_with(short)
}

/// How long a request with NO observable build is carried before it is called stranded.
///
/// `build_in_flight` is the deploy CLAIM, and a claim does not cover a deploy's whole life:
/// it is dropped when the build ends, while the running sha only changes once the new core
/// answers. Everything in between — stop the old core, hand off the artifact, the new
/// core's own startup compile and boot — is a healthy deploy with no claim and the old sha,
/// which is bit-for-bit the state a genuinely dead reboot leaves behind. Only DURATION
/// separates them, and they are orders of magnitude apart, so the grace is not delicate.
///
/// Sized from the measurement that exposed this (Intel tier, 2026-09-18): the probe fired
/// one 300 s tick after the request was written, on a deploy that came up healthy. The old
/// code had no grace at all — the first tick that could not see a build condemned the
/// deploy. Twenty minutes comfortably covers the observed claim-drop-to-core-up window on
/// the slowest tier and still names a reboot that died within the hour.
///
/// Same shape, and the same reasoning, as `COLD_BOOT_BELOW_FLOOR_GRACE` in the serving
/// planner: a transient and a real fault present identically in one sample, so wait rather
/// than invent a second signal.
pub const STRANDED_GRACE_MS: u64 = 20 * 60 * 1000;

/// Close the deploy loop: given the request on record and the build actually running,
/// say whether what was asked for has arrived. Pure — the module supplies the two facts
/// it already gathers every tick, so this costs nothing and can be asserted with no
/// filesystem, no git and no clock.
pub fn reconcile_request(
    request: Option<&DeployRequest>,
    running_sha: &str,
    build_in_flight: bool,
    now_ms: u64,
) -> RequestOutcome {
    let Some(req) = request else {
        return RequestOutcome::Nothing;
    };
    let elapsed_ms = now_ms.saturating_sub(req.requested_ms);
    if same_commit(&req.tip_sha, running_sha) {
        return RequestOutcome::Settled { tip_sha: req.tip_sha.clone(), waited_ms: elapsed_ms };
    }
    if build_in_flight || elapsed_ms < STRANDED_GRACE_MS {
        RequestOutcome::InFlight { tip_sha: req.tip_sha.clone(), elapsed_ms }
    } else {
        RequestOutcome::Stranded { tip_sha: req.tip_sha.clone(), elapsed_ms }
    }
}

/// The source of the deployable tip and its check-state, behind a trait so the decision
/// never touches gh directly and a LAN/gist/Reticulum fallback can slot in (gh is never
/// on the liveness path). The `ServiceModule` calls this via `spawn_blocking` + a
/// bounded timeout; a failure returns `Err`, which the caller maps to `tip: None` and
/// `decide` reads as `SourceUnavailable` — degrade, never wedge.
#[async_trait::async_trait]
pub trait DeploySource: Send + Sync {
    /// The tracked branch's tip SHA and its check-state. `Ok(None)` = reachable but no
    /// tip (empty branch); `Err` = unreachable (offline) → the running build stands.
    async fn tip(&self) -> Result<Option<(String, Checks)>, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_000_000_000_000;

    fn base() -> TickInputs {
        TickInputs {
            running_sha: Some("aaaaaaaaa111".into()),
            tip_sha: Some("bbbbbbbbb222".into()),
            source_error: None,
            checks: Checks::Green,
            hold: None,
            build_in_flight: false,
            tree_dirty: false,
            now_ms: NOW,
        }
    }

    // what this catches: the full guard ladder of track-canary.sh, ported so the Rust
    // owner makes the identical decision on every platform (the shell owner did not exist
    // on Windows at all). Order matters — a red tip is refused before a dirty tree, and a
    // hold beats an in-flight build.
    #[test]
    fn the_guard_ladder_matches_the_shell_owner_in_order() {
        assert_eq!(decide(&base()), DeployVerdict::Deploy { tip_sha: "bbbbbbbbb222".into() });
        // Source down → the running build stands, never a deploy.
        let mut i = base(); i.tip_sha = None; i.source_error = Some("fetch failed".into());
        assert_eq!(decide(&i), DeployVerdict::SourceUnavailable { why: "fetch failed".into() });
        // Core not answering → not a deploy trigger.
        let mut i = base(); i.running_sha = None;
        assert_eq!(decide(&i), DeployVerdict::CoreNotAnswering);
        // Already at the tip (short vs full SHA) → up to date.
        let mut i = base(); i.tip_sha = Some("aaaaaaaaa111deadbeef".into());
        assert_eq!(decide(&i), DeployVerdict::UpToDate);
        // A red tip is refused BEFORE a dirty tree is consulted.
        let mut i = base(); i.checks = Checks::Red; i.tree_dirty = true;
        assert_eq!(decide(&i), DeployVerdict::RefuseRed);
        // Pending / unknown wait (unknown != red — gh unreachable never reads as failure).
        let mut i = base(); i.checks = Checks::Pending;
        assert_eq!(decide(&i), DeployVerdict::ChecksPending);
        let mut i = base(); i.checks = Checks::Unknown;
        assert_eq!(decide(&i), DeployVerdict::ChecksUnknown);
        // A build in flight blocks (before checks).
        let mut i = base(); i.build_in_flight = true;
        assert_eq!(decide(&i), DeployVerdict::BuildInFlight);
        // Green but dirty tree → refuse (the deploy tree is the deploy tree).
        let mut i = base(); i.tree_dirty = true;
        assert_eq!(decide(&i), DeployVerdict::RefuseDirty);
    }

    // what this catches (card ee76c0df / the unbounded-hold class): a hold expires on its
    // TTL, a TTL-less hold stays active but is flagged stale after STALE_HOLD_WARN so it
    // can never silently pause the fleet for days, and an active hold beats the deploy.
    #[test]
    fn a_hold_expires_and_a_stale_one_is_flagged_never_silent() {
        // A TTL-less hold holds, and once past STALE_HOLD_WARN it is flagged stale.
        let mut i = base();
        i.hold = Some(Hold { reason: "sqlite-14".into(), created_ms: NOW, ttl_ms: None });
        assert_eq!(decide(&i), DeployVerdict::Held { reason: "sqlite-14".into(), stale: false });
        i.now_ms = NOW + STALE_HOLD_WARN.as_millis() as u64;
        assert_eq!(decide(&i), DeployVerdict::Held { reason: "sqlite-14".into(), stale: true },
            "a hold standing longer than the warn window is flagged, not skipped in silence");
        // A TTL'd hold expires and the deploy proceeds — the 3-day-stale-hold bug cannot recur.
        let mut i = base();
        i.hold = Some(Hold { reason: "quiet round".into(), created_ms: NOW, ttl_ms: Some(3_600_000) });
        assert!(matches!(decide(&i), DeployVerdict::Held { .. }), "inside its TTL the hold holds");
        i.now_ms = NOW + 3_600_001;
        assert_eq!(decide(&i), DeployVerdict::Deploy { tip_sha: "bbbbbbbbb222".into() },
            "past its TTL the hold is gone and tracking resumes");
    }

    // what this catches: the seam struct the supervisor consumes is idempotent by tip —
    // the same green tip yields the same request, so a module re-deciding each tick does
    // not re-signal a deploy already in flight.
    #[test]
    fn a_deploy_request_is_keyed_by_tip() {
        let a = DeployRequest::new("bbbbbbbbb222", NOW);
        let b = DeployRequest::new("bbbbbbbbb222", NOW + 5_000);
        assert_eq!(a.tip_sha, b.tip_sha);
        assert_ne!(DeployRequest::new("ccc", NOW).tip_sha, a.tip_sha);
    }

    // what this catches, and it is the whole reason this reconcile is not dead code:
    // a DeployRequest records the FULL tip sha while `running_sha()` is the SHORT form the
    // build stamps. An `==` comparison never fires, `Settled` is never returned, and the
    // loop stays open while looking closed. These are the real values from this tier on
    // 2026-09-17. Against a `==` implementation the first assertion fails.
    #[test]
    fn a_short_running_sha_settles_a_full_requested_tip() {
        let req = DeployRequest::new("c2344d758225d87911d1ee2934b4e7e42673c26e", NOW);
        assert_eq!(
            reconcile_request(Some(&req), "c2344d758", false, NOW + 90_000),
            RequestOutcome::Settled {
                tip_sha: "c2344d758225d87911d1ee2934b4e7e42673c26e".into(),
                waited_ms: 90_000
            },
            "the short stamped sha IS the requested tip — abbreviation is not a difference"
        );
        // and the reverse orientation, so the comparison is not accidentally one-sided
        let req_short = DeployRequest::new("c2344d758", NOW);
        assert!(matches!(
            reconcile_request(Some(&req_short), "c2344d758225d87911d1ee2934b4e7e42673c26e", false, NOW),
            RequestOutcome::Settled { .. }
        ));
    }

    // what this catches: a sha too short to be unambiguous must NOT match, or any request
    // could be "settled" by coincidence. git's own floor is 7.
    #[test]
    fn an_abbreviation_too_short_to_be_unambiguous_is_not_a_match() {
        let req = DeployRequest::new("c2344d758225d87911d1ee2934b4e7e42673c26e", NOW);
        assert!(
            !matches!(reconcile_request(Some(&req), "c2344", false, NOW), RequestOutcome::Settled { .. }),
            "five characters is a coincidence, not a commit"
        );
    }

    // what this catches (the silent half, measured 2026-09-17): a request that never took
    // and a request still building were INDISTINGUISHABLE — `continuum reboot` exits 1
    // while its build continues detached (card 79faf35d), so both looked like nothing.
    // The build claim is what separates them, and Stranded is the state worth surfacing.
    #[test]
    fn a_request_that_did_not_take_is_stranded_not_in_flight() {
        let req = DeployRequest::new("aaaaaaaaa111bbbb", NOW);
        assert_eq!(
            reconcile_request(Some(&req), "999999999", true, NOW + 60_000),
            RequestOutcome::InFlight { tip_sha: "aaaaaaaaa111bbbb".into(), elapsed_ms: 60_000 },
            "a build claim is active — this is working, not broken"
        );
        let late = STRANDED_GRACE_MS + 1;
        assert_eq!(
            reconcile_request(Some(&req), "999999999", false, NOW + late),
            RequestOutcome::Stranded { tip_sha: "aaaaaaaaa111bbbb".into(), elapsed_ms: late },
            "past the grace with nothing building and the tip not running — it did not take"
        );
    }

    // what this catches: the live falsification of this module's own prediction, Intel
    // tier 2026-09-18. deploy.stranded fired at elapsed_ms=300004 — ONE tick after the
    // request was written — for a deploy that came up healthy 25 minutes later. Two causes
    // in one chain: the deploy claim had expired mid-build (a 4,197 s build under a 1 h
    // ceiling, fixed in deploy_claim.rs), and this function had no grace of its own, so the
    // first tick that could not see a build condemned the deploy. A claim does not cover a
    // deploy's whole life — stop, handoff, startup compile and boot all run with no claim
    // and the OLD sha, which is exactly what a dead reboot looks like. Only duration tells
    // them apart. Calling a live deploy stranded is not cosmetic: stranded is the signal to
    // relaunch, and a second build races the first on the shared CARGO_TARGET_DIR.
    #[test]
    fn a_handoff_window_with_no_claim_is_not_yet_stranded() {
        let req = DeployRequest::new("aaaaaaaaa111bbbb", NOW);
        // The exact measured moment, replayed: one 300 s tick, no claim, old sha running.
        assert_eq!(
            reconcile_request(Some(&req), "38be2e1a9", false, NOW + 300_004),
            RequestOutcome::InFlight { tip_sha: "aaaaaaaaa111bbbb".into(), elapsed_ms: 300_004 },
            "a deploy mid-handoff has no claim and the old sha — that is not a strand"
        );
        // And the boundary is not off by one in the forgiving direction either.
        assert!(matches!(
            reconcile_request(Some(&req), "38be2e1a9", false, NOW + STRANDED_GRACE_MS),
            RequestOutcome::Stranded { .. }
        ));
        assert!(matches!(
            reconcile_request(Some(&req), "38be2e1a9", false, NOW + STRANDED_GRACE_MS - 1),
            RequestOutcome::InFlight { .. }
        ));
    }

    // what this catches: no request on record must be quiet, not a false Stranded every
    // tick on a node that is simply up to date.
    #[test]
    fn no_request_on_record_is_nothing_owed() {
        assert_eq!(reconcile_request(None, "c2344d758", false, NOW), RequestOutcome::Nothing);
    }
}
