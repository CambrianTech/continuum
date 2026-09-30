//! THE DAEMON THAT APPEARS LATER — attaching the core's airc handle is a RETRY, not a
//! boot-time verdict.
//!
//! Card e28a0340, measured on BigMama (5090) 2026-09-07: the core booted 06:28Z with no
//! daemon, took its one discovery verdict, and failed 1,216 times through 12:39Z against
//! a pipe that had been live since 07:39Z. Astra measured the same shape live on Windows
//! 2026-09-22 — `airc join` restored the daemon under a running core, and remote command
//! routing still refused with "AIRC handle has not attached yet" because
//! `Airc::attach_as` had been tried ONCE at boot (`ipc/mod.rs`). `discovery.rs` names the
//! gap in its own error text: "self-healing re-attach without restart is a follow-up".
//!
//! Two distinct one-shots, one law: **an absent dependency at boot must never be
//! permanent.** Before this, (a) no daemon socket at boot skipped the attach block
//! entirely — no task, no retry, ever; and (b) a socket that WAS found but failed to
//! attach logged an error whose own remedy was "a boot with a reachable airc daemon".
//! The substrate told the operator to reboot instead of healing.
//!
//! NOT a second copy of [`crate::airc::reresolving_client`] (#3850), which shares this
//! card's BigMama measurement and is easy to mistake for it. That one owns "the socket
//! MOVED under a daemon CALL": a live `DaemonClient` re-asks the authoritative source and
//! retries. This one owns the failure one step earlier — "there is no handle at all",
//! because `attach_as` ran once and lost. A core can have the re-resolving client wired
//! and still route nothing, which is exactly what Windows measured. One law, two seams.
//!
//! The shape is the concurrency guide's: an owned task, a bounded ladder, a probe that
//! says what it is waiting for, and a hand-off the moment the dependency appears. The
//! socket is RE-RESOLVED on every attempt (`discover_airc_socket`), so a daemon that is
//! installed, started, or moved after boot is found without a restart.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// The cadence card e28a0340 asks for: 30 s between attempts once the ladder is warm.
pub const REATTACH_CEILING: Duration = Duration::from_secs(30);
/// The first retry is quick — a daemon restarting under a running core is back in
/// seconds, and the node is degraded until it is.
const REATTACH_FLOOR: Duration = Duration::from_secs(1);
/// How often a still-unattached core says so. Every attempt would be a chatty floor;
/// silence would be worse (1,216 failures went unexplained for six hours).
pub const STILL_UNATTACHED_REPORT: Duration = Duration::from_secs(60);

/// PURE: the wait before attempt `attempt` (1-based). Doubles from [`REATTACH_FLOOR`]
/// to [`REATTACH_CEILING`] and stays there — a dependency that has not appeared in
/// thirty seconds is not one that appears faster for being asked more often, and the
/// node must keep asking for as long as it runs.
pub fn retry_delay(attempt: u32) -> Duration {
    // Clamped BEFORE the shift, not rescued after it: five doublings already pass the
    // ceiling (1→2→4→8→16→32 s), so the shift can never overflow and there is no failure
    // case to swallow. An arithmetic guard whose fallback is itself a policy number is
    // the shape that hides a bug; this has neither.
    let doublings = attempt.saturating_sub(1).min(5);
    let secs = REATTACH_FLOOR.as_secs().saturating_mul(1u64 << doublings);
    Duration::from_secs(secs).min(REATTACH_CEILING)
}

/// WHY an attach is not live yet — because the operator's next action differs, and a
/// retry ladder that says "still trying" for both is the one-value-two-meanings shape.
///
/// Measured on Windows 2026-09-22 (Astra): `attach_as` failed with an enrolled peer whose
/// stored pubkey did not match, needing a signed `TrustRotation` — while discovery was
/// HEALTHY. A ladder that reported that as "no daemon yet" would send an operator to
/// restart a daemon that was already up. Retrying stays right either way: a rotation
/// signed under a running core makes the next attempt succeed, which is the self-healing
/// this card exists for. What must not blur is the SENTENCE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachBlocker {
    /// The trust store refused: an enrolled peer id with a different pubkey
    /// (`PeersStoreError::PubkeyConflict` and its siblings). No amount of daemon
    /// restarting fixes it; a signed rotation does.
    Trust,
    /// Everything else — no daemon, no socket, a daemon still loading. Time fixes it.
    Transport,
}

impl AttachBlocker {
    pub const fn as_str(self) -> &'static str {
        match self {
            AttachBlocker::Trust => "trust",
            AttachBlocker::Transport => "transport",
        }
    }
}

/// PURE-ish: classify an attach failure by its TYPED variant, never by its message.
/// airc ships on its own cadence; a reworded error must not silently reclassify a trust
/// conflict as a missing daemon ([[strong-typing-across-boundaries]], the same rule
/// `reresolving_client` states for reachability).
pub fn blocker_of(error: &airc_lib::AircError) -> AttachBlocker {
    match error {
        airc_lib::AircError::PeersStore(_) => AttachBlocker::Trust,
        _ => AttachBlocker::Transport,
    }
}

/// PURE: whether an attempt that just failed should SAY so, given when the last report
/// went out. The first failure always speaks (the operator learns immediately that the
/// node is degraded); after that, once per [`STILL_UNATTACHED_REPORT`].
pub fn should_report(last_report_ms: Option<u64>, now_ms: u64) -> bool {
    match last_report_ms {
        None => true,
        Some(last) => now_ms.saturating_sub(last) >= STILL_UNATTACHED_REPORT.as_millis() as u64,
    }
}

/// Attach the core's own airc handle into `cell`, retrying until it succeeds. Runs for
/// the life of the process or until the cell is filled, whichever is first.
///
/// `boot_socket` is the socket discovery found at boot, used for the FIRST attempt so a
/// healthy boot pays no discovery cost; every later attempt re-resolves, which is the
/// half that heals a node whose daemon was absent, installing, or restarting.
pub async fn attach_until_live(
    root: PathBuf,
    agent_name: &'static str,
    boot_socket: Option<PathBuf>,
    cell: Arc<tokio::sync::OnceCell<Arc<airc_lib::Airc>>>,
) {
    let started = std::time::Instant::now();
    let mut last_report_ms: Option<u64> = None;
    let mut attempt: u32 = 0;
    loop {
        attempt = attempt.saturating_add(1);
        let socket = match (attempt, boot_socket.clone()) {
            (1, Some(socket)) => Ok(socket),
            _ => crate::airc::discovery::discover_airc_socket()
                .await
                .map_err(|e| e.to_string()),
        };
        // The typed error is kept until the probe renders it — classification happens on
        // the variant, and only the SENTENCE is a string.
        let outcome = match socket {
            Ok(socket) => match airc_lib::Airc::attach_as(root.clone(), agent_name, socket.clone())
                .await
            {
                Ok(airc) => Ok((airc, socket)),
                Err(error) => Err((blocker_of(&error), error.to_string())),
            },
            // Discovery failing is the transport case by construction: there is no store
            // to conflict with when there is no socket to attach to.
            Err(why) => Err((AttachBlocker::Transport, why)),
        };
        match outcome {
            Ok((airc, socket)) => {
                let peer = airc.peer_id().as_uuid();
                crate::persona::self_peer::register(peer);
                // `set` fails only if someone else won the race; either way the cell
                // holds a live handle, which is the whole postcondition.
                let _ = cell.set(Arc::new(airc));
                crate::probe!(
                    class = "airc.attach.live",
                    attempt = attempt as u64,
                    waited_ms = started.elapsed().as_millis() as u64,
                    socket = %socket.display(),
                    peer = %peer,
                    "the core's airc handle is attached — peer- and room-addressed commands route from here"
                );
                return;
            }
            Err((blocker, why)) => {
                let now_ms = now_ms();
                if should_report(last_report_ms, now_ms) {
                    last_report_ms = Some(now_ms);
                    crate::probe!(
                        class = "airc.attach.still_unattached",
                        attempt = attempt as u64,
                        waited_ms = started.elapsed().as_millis() as u64,
                        blocker = blocker.as_str(),
                        error = %why,
                        "no airc handle yet — remote command routing refuses until this clears; blocker=trust needs a signed rotation, blocker=transport needs the daemon; still trying either way"
                    );
                }
                tokio::time::sleep(retry_delay(attempt)).await;
            }
        }
    }
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0) // unwrap_or: a pre-epoch clock only affects report spacing, never the retry
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card e28a0340): a ladder that either hammers forever or walks
    // off to an interval no one waits through. BigMama's daemon appeared 71 minutes
    // after boot; the node must still be asking then, at the cadence the card names.
    #[test]
    fn the_ladder_warms_to_the_named_cadence_and_never_gives_up() {
        assert_eq!(retry_delay(1), REATTACH_FLOOR, "a restarting daemon is back in seconds");
        assert_eq!(retry_delay(2), Duration::from_secs(2));
        assert_eq!(retry_delay(3), Duration::from_secs(4));
        assert_eq!(retry_delay(6), REATTACH_CEILING, "warm at the card's 30 s cadence");
        // An hour of failures, then a day of them: still asking, still at the ceiling —
        // no overflow, no give-up branch.
        assert_eq!(retry_delay(120), REATTACH_CEILING);
        assert_eq!(retry_delay(u32::MAX), REATTACH_CEILING);
    }

    // what this catches (Astra, Windows 2026-09-22): the ladder reporting a TRUST
    // conflict as "no daemon yet", sending an operator to restart a daemon that was
    // already up — discovery was healthy and `attach_as` failed on an enrolled peer
    // whose stored pubkey did not match.
    //
    // The discriminating case is a TRANSPORT error whose MESSAGE carries the trust
    // words: it must classify as transport anyway. That is what fails if anyone
    // reimplements this by matching the sentence instead of the variant — the door
    // `reresolving_client` had to nail shut for reachability, on a crate that ships on
    // its own cadence. (The `PeersStore` arm itself is matched by variant; continuum-core
    // does not depend on `airc-trust`, so a witness for it cannot be built here without
    // pinning a second crate from that repo for one assertion.)
    #[test]
    fn the_blocker_comes_from_the_variant_never_from_the_message() {
        let liar = airc_lib::AircError::Transport(
            "pubkey mismatch, requires signed TrustRotation".into(),
        );
        assert_eq!(
            blocker_of(&liar),
            AttachBlocker::Transport,
            "a transport error that merely MENTIONS trust is still a transport error"
        );
        assert_eq!(AttachBlocker::Transport.as_str(), "transport");
        assert_eq!(AttachBlocker::Trust.as_str(), "trust");
        assert_ne!(AttachBlocker::Trust, AttachBlocker::Transport);
    }

    // what this catches: the two failure modes of saying it. Silence for six hours (the
    // measured case) and a line per attempt (the chatty floor that buries it).
    #[test]
    fn a_still_unattached_core_speaks_once_then_once_a_minute() {
        assert!(should_report(None, 10_000), "the first failure always speaks");
        assert!(!should_report(Some(10_000), 10_001), "not once per attempt");
        assert!(!should_report(Some(10_000), 69_999));
        assert!(should_report(Some(10_000), 70_000), "once a minute thereafter");
    }
}
