//! `discover()` — the aggregator that produces a typed `AircDiscovery`.
//!
//! Wraps the four existing discovery sub-steps
//! (`discover_airc_socket`, `discover_peer_id`,
//! `discover_default_room_name`, `discover_default_channel`) and
//! promotes each failure into the corresponding `AircDiscovery`
//! variant, carrying whatever partial state we did manage to
//! resolve.
//!
//! Critically, `discover_peer_id` IS the liveness probe — the Status
//! RPC round-trips against the socket. Before A.2, a failed
//! `discover_peer_id` soft-fell-back to `Uuid::nil()` so the module
//! still registered. Now it produces
//! `AircDiscovery::Degraded { reason: StaleSocket, .. }` which the
//! caller can act on per [[no-fallbacks-ever]].

use std::path::PathBuf;

use airc_core::RoomId;

use crate::airc::discovery::{
    discover_airc_socket, discover_default_channel, discover_default_room_name, discover_peer_id,
    DiscoveryError,
};
use crate::airc::discovery_state::{AircDiscovery, DiscoveryFailure, PartialDiscovery};

/// Discover the airc daemon's full state. Always returns a typed
/// `AircDiscovery` — never panics, never returns `Err`. The
/// substrate routes downstream behavior on the variant.
pub async fn discover() -> AircDiscovery {
    let mut partial = PartialDiscovery::default();

    let socket = match discover_airc_socket().await {
        Ok(path) => {
            partial.socket = Some(path.clone());
            path
        }
        Err(e) => return AircDiscovery::Unreachable { reason: e.into() },
    };

    // Liveness probe — Status RPC round-trip against the socket.
    // Before A.2 this could soft-fail to Uuid::nil() and the module
    // would still register. After A.2, a probe failure promotes
    // to AircDiscovery::Degraded { reason: StaleSocket } and
    // the substrate refuses persona hosting against this state.
    let peer_id = match discover_peer_id(&socket).await {
        Ok(p) => {
            partial.peer_id = Some(p);
            p
        }
        // A DEAD DAEMON IS RECOVERED HERE, ONCE — never printed as a remediation for a
        // human to type (2026-09-14: the airc daemon died on the M5 twice; each time the
        // core's own start printed "remove the stale socket and restart airc", refused,
        // and the node sat dark until an operator did exactly that — 03:33 and 06:53).
        Err(first) => match recover_stale_daemon(&socket).await {
            true => match discover_peer_id(&socket).await {
                Ok(p) => {
                    partial.peer_id = Some(p);
                    p
                }
                Err(e) => {
                    return AircDiscovery::Degraded {
                        reason: stale_socket_from_status_err(&socket, e),
                        partial,
                    };
                }
            },
            false => {
                return AircDiscovery::Degraded {
                    reason: stale_socket_from_status_err(&socket, first),
                    partial,
                };
            }
        },
    };

    let room_name = match discover_default_room_name().await {
        Ok(name) => {
            partial.room_name = Some(name.clone());
            name
        }
        Err(e) => {
            return AircDiscovery::Degraded {
                reason: room_failure(e),
                partial,
            };
        }
    };

    let default_room = match discover_default_channel().await {
        Ok(uuid) => {
            let room = RoomId::from_uuid(uuid);
            partial.default_room = Some(room);
            room
        }
        Err(e) => {
            return AircDiscovery::Degraded {
                reason: room_failure(e),
                partial,
            };
        }
    };

    AircDiscovery::Healthy {
        socket,
        default_room,
        room_name,
        peer_id,
    }
}

/// How long a boot waits for the daemon to come back before it accepts a
/// degraded verdict. An `airc update` (or the launcher's own re-ensure)
/// takes the daemon down for a few seconds; on 2026-09-07 07:0xZ one landed
/// between `ensure_airc_daemon` and this probe, the core booted degraded
/// (no persona hosting) on a socket that was live again ten seconds later,
/// and the deploy reported an empty verify. Sized to the slow case of a
/// daemon restart, not to a missing install — an absent binary is refused
/// on the first probe (see `is_transient`).
pub const DISCOVERY_PATIENCE: std::time::Duration = std::time::Duration::from_secs(45);
/// Cadence between probes while the daemon is coming back.
pub const DISCOVERY_RETRY_CADENCE: std::time::Duration = std::time::Duration::from_secs(2);

/// When a recovery last STARTED a daemon, process-wide. The patience budget is measured
/// from here rather than from the probe that found the socket dead.
///
/// A daemon the core starts itself needs time to answer — on this fleet as much as
/// 60–70 s, because the answer waits behind the event store it opens (2.4 GB on the
/// 5090, 2026-09-25). Charging that time against the window that decides whether to
/// BELIEVE the daemon is what left the node dark for four days: recovery started a
/// daemon at 19:28:40.940341, the 45 s budget was already spent on the probe that
/// preceded it, discovery settled `kind=degraded` 146 µs later, the core refused to
/// boot (`--mode=full-citizen requires AIRC Healthy`), and the daemon it had just
/// started died with it — every restart repeating the same pass, and no hand inside
/// the substrate able to break it (an operator had to start a daemon out-of-band).
static DAEMON_STARTED_AT: std::sync::Mutex<Option<std::time::Instant>> =
    std::sync::Mutex::new(None);

fn note_daemon_started() {
    if let Ok(mut at) = DAEMON_STARTED_AT.lock() {
        *at = Some(std::time::Instant::now());
    }
}

fn daemon_started_at() -> Option<std::time::Instant> {
    DAEMON_STARTED_AT.lock().ok().and_then(|at| *at)
}

/// PURE: how long the retry loop has effectively waited — from its own start, or from
/// the moment a recovery started a daemon, whichever is LATER. A started daemon resets
/// the budget once; its own start time is never charged against it.
pub fn effective_elapsed(
    started: std::time::Instant,
    daemon_started: Option<std::time::Instant>,
    now: std::time::Instant,
) -> std::time::Duration {
    let origin = match daemon_started {
        Some(d) if d > started => d,
        _ => started,
    };
    now.saturating_duration_since(origin)
}

/// PURE: the origin the patience budget is measured from, frozen at the FIRST daemon
/// start newer than the loop's own start. Once set it never moves, so a start from a
/// later retry cannot extend the budget; a start older than the loop is ignored.
pub fn frozen_reset_origin(
    frozen: Option<std::time::Instant>,
    loop_started: std::time::Instant,
    daemon_started: Option<std::time::Instant>,
) -> Option<std::time::Instant> {
    frozen.or(daemon_started.filter(|d| *d > loop_started))
}

/// A failure that a daemon restart explains — worth another probe. Install
/// and configuration failures are not: waiting cannot change them.
pub fn is_transient(d: &AircDiscovery) -> bool {
    match d {
        AircDiscovery::Healthy { .. } => false,
        AircDiscovery::Unreachable { reason } | AircDiscovery::Degraded { reason, .. } => {
            matches!(
                reason,
                DiscoveryFailure::StaleSocket(..)
                    | DiscoveryFailure::PeerStatusFailed(_)
                    | DiscoveryFailure::RoomCommandFailed(_)
                    | DiscoveryFailure::EndpointCommandFailed(_)
                    | DiscoveryFailure::EmptyPath
                    | DiscoveryFailure::AutoInstallInProgress
            )
        }
    }
}

/// Whether to probe again: the verdict is transient and the budget is not
/// spent. Pure so the policy is testable without a daemon.
pub fn should_retry(
    d: &AircDiscovery,
    elapsed: std::time::Duration,
    patience: std::time::Duration,
) -> bool {
    is_transient(d) && elapsed < patience
}

/// `discover()` with patience for a daemon that is restarting: probes at
/// `DISCOVERY_RETRY_CADENCE` until the verdict is `Healthy`, the failure is
/// one waiting cannot fix, or `DISCOVERY_PATIENCE` is spent. Every retry is
/// a probe row (`airc.discovery.retry`) so a slow boot names what it waited
/// for; the final verdict rides the caller's boot.status row as before.
pub async fn discover_with_patience() -> AircDiscovery {
    let started = std::time::Instant::now();
    let mut attempt: u32 = 0;
    let mut reset_origin: Option<std::time::Instant> = None;
    loop {
        let d = discover().await;
        attempt += 1;
        // The budget runs from the loop's start OR from the FIRST daemon this pass
        // started, whichever is later (see `DAEMON_STARTED_AT`). Frozen once seen: every
        // retry re-runs recovery when the socket is unheld, so a daemon that keeps dying
        // would otherwise move the origin forward each time and the budget would never
        // expire — boot looping on daemon starts, never settling a verdict.
        reset_origin = frozen_reset_origin(reset_origin, started, daemon_started_at());
        let waited = effective_elapsed(started, reset_origin, std::time::Instant::now());
        if !should_retry(&d, waited, DISCOVERY_PATIENCE) {
            if attempt > 1 {
                crate::probe!(
                    class = "airc.discovery.settled",
                    attempts = attempt,
                    waited_ms = started.elapsed().as_millis() as u64,
                    waited_since_daemon_start_ms = waited.as_millis() as u64,
                    kind = d.kind(),
                    "airc discovery settled after waiting for the daemon"
                );
            }
            return d;
        }
        crate::probe!(
            class = "airc.discovery.retry",
            attempt = attempt,
            waited_ms = started.elapsed().as_millis() as u64,
            waited_since_daemon_start_ms = waited.as_millis() as u64,
            kind = d.kind(),
            reason = ?d.reason(),
            "airc daemon not answering yet — probing again (a restart in progress, not a missing install)"
        );
        tokio::time::sleep(DISCOVERY_RETRY_CADENCE).await;
    }
}

impl From<DiscoveryError> for DiscoveryFailure {
    fn from(e: DiscoveryError) -> Self {
        match e {
            DiscoveryError::InstallFailed(msg) => DiscoveryFailure::InstallFailed(msg),
            DiscoveryError::AutoInstallDisabled => DiscoveryFailure::AutoInstallDisabled,
            DiscoveryError::AutoInstallInProgress => DiscoveryFailure::AutoInstallInProgress,
            DiscoveryError::EndpointCommandFailed(msg) => {
                DiscoveryFailure::EndpointCommandFailed(msg)
            }
            DiscoveryError::EmptyPath => DiscoveryFailure::EmptyPath,
            DiscoveryError::RoomCommandFailed(msg) => DiscoveryFailure::RoomCommandFailed(msg),
            DiscoveryError::UnparseableChannel(msg) => DiscoveryFailure::UnparseableRoomOutput(msg),
            DiscoveryError::PeerStatusFailed(msg) => DiscoveryFailure::PeerStatusFailed(msg),
            DiscoveryError::UnparseablePeerId(raw, err) => {
                DiscoveryFailure::UnparseablePeerId(raw, err.to_string())
            }
        }
    }
}

/// Recover a daemon whose socket nobody holds: remove the stale file, start a daemon by
/// the same path `airc status` uses (it starts one when none answers), and wait — bounded
/// — for the new socket to answer. Returns `true` when a retry is worth making. A socket
/// some process still holds is NOT stale (a wedged daemon is a different fault) and is
/// left alone. Every outcome is a probe: `airc.daemon.recovered`.
async fn recover_stale_daemon(socket: &std::path::Path) -> bool {
    use tokio::process::Command;
    let bound = std::time::Duration::from_secs(5);
    let held = tokio::time::timeout(bound, Command::new("lsof").arg("-t").arg(socket).output())
        .await
        .ok()
        .and_then(|r| r.ok())
        .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
        .unwrap_or(false); // JUSTIFIED unwrap_or: lsof absent or hung = cannot prove a holder; treat as unheld and try (the retry is bounded and harmless)
    if held {
        crate::probe!(
            class = "airc.daemon.recovered",
            socket = %socket.display(),
            outcome = "held_not_stale",
            "a process still holds the daemon socket — not a stale file; no recovery attempted"
        );
        return false;
    }
    let existed = socket.exists();
    if existed {
        let _ = std::fs::remove_file(socket);
    }
    let started = std::time::Instant::now();
    let start = tokio::time::timeout(std::time::Duration::from_secs(30), Command::new("airc").arg("status").output()).await;
    let start_ok = matches!(&start, Ok(Ok(o)) if o.status.success());
    if start_ok || socket.exists() {
        // A daemon was started. Whether it answers within THIS function's wait or not,
        // the patience loop now measures its budget from here — a slow daemon is a
        // daemon, and the verdict must not be the one its own start time bought.
        note_daemon_started();
    }
    let mut answered = false;
    while started.elapsed() < std::time::Duration::from_secs(25) {
        if socket.exists() && discover_peer_id(socket).await.is_ok() {
            answered = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    crate::probe!(
        class = "airc.daemon.recovered",
        socket = %socket.display(),
        outcome = if answered { "recovered" } else if start_ok { "started_not_answering" } else { "start_failed" },
        stale_file_removed = existed,
        waited_ms = started.elapsed().as_millis() as u64,
        "dead airc daemon: stale socket cleared and a daemon started by the core itself"
    );
    answered
}

/// Status RPC failure → typed `StaleSocket` carrying the path AND
/// the underlying error message. This is the structural fix for the
/// R2 hole: every Status failure path collapses to one variant the
/// caller MUST match exhaustively.
fn stale_socket_from_status_err(socket: &PathBuf, e: DiscoveryError) -> DiscoveryFailure {
    let underlying = match e {
        DiscoveryError::PeerStatusFailed(msg) => msg,
        other => other.to_string(),
    };
    DiscoveryFailure::StaleSocket(socket.clone(), underlying)
}

/// Room-side failures (no room set, command failed, unparseable
/// output) all promote to typed variants on `DiscoveryFailure`.
fn room_failure(e: DiscoveryError) -> DiscoveryFailure {
    match e {
        DiscoveryError::RoomCommandFailed(msg) => DiscoveryFailure::RoomCommandFailed(msg),
        DiscoveryError::UnparseableChannel(msg) => DiscoveryFailure::UnparseableRoomOutput(msg),
        other => DiscoveryFailure::RoomCommandFailed(other.to_string()),
    }
}

#[cfg(test)]
mod discovery_failure_mapping_tests {
    //! Lock in the `DiscoveryError → DiscoveryFailure` projection so
    //! a future refactor that re-routes one variant (e.g.
    //! `PeerStatusFailed → EndpointCommandFailed`) gets caught
    //! immediately — that class of silent mismatch would let the
    //! R2#1 BLOCK return without any test failing.
    //!
    //! The aggregator's typed `discover()` output drives operator-
    //! facing diagnostics; if a single variant maps wrong, the
    //! operator gets the wrong actionable repair message.

    use super::*;

    #[test]
    fn install_failed_preserves_message() {
        let f: DiscoveryFailure = DiscoveryError::InstallFailed("permission denied".into()).into();
        assert!(matches!(f, DiscoveryFailure::InstallFailed(m) if m == "permission denied"));
    }

    #[test]
    fn auto_install_disabled_maps_to_same() {
        let f: DiscoveryFailure = DiscoveryError::AutoInstallDisabled.into();
        assert!(matches!(f, DiscoveryFailure::AutoInstallDisabled));
    }

    #[test]
    fn endpoint_command_failed_preserves_message() {
        let f: DiscoveryFailure =
            DiscoveryError::EndpointCommandFailed("exit 2: unknown subcommand".into()).into();
        assert!(matches!(
            f,
            DiscoveryFailure::EndpointCommandFailed(m) if m.contains("exit 2")
        ));
    }

    #[test]
    fn empty_path_maps_to_empty_path() {
        let f: DiscoveryFailure = DiscoveryError::EmptyPath.into();
        assert!(matches!(f, DiscoveryFailure::EmptyPath));
    }

    #[test]
    fn room_command_failed_preserves_message() {
        let f: DiscoveryFailure =
            DiscoveryError::RoomCommandFailed("no current room".into()).into();
        assert!(matches!(
            f,
            DiscoveryFailure::RoomCommandFailed(m) if m.contains("no current")
        ));
    }

    /// `DiscoveryError::UnparseableChannel` → `DiscoveryFailure::UnparseableRoomOutput`.
    /// This is the variant most likely to be silently re-routed in a
    /// refactor (their names diverge for historical reasons) — pin it.
    #[test]
    fn unparseable_channel_maps_to_unparseable_room_output() {
        let f: DiscoveryFailure =
            DiscoveryError::UnparseableChannel("channel: <garbage>".into()).into();
        assert!(matches!(
            f,
            DiscoveryFailure::UnparseableRoomOutput(m) if m.contains("channel:")
        ));
    }

    #[test]
    fn peer_status_failed_preserves_message() {
        let f: DiscoveryFailure =
            DiscoveryError::PeerStatusFailed("connection refused".into()).into();
        assert!(matches!(
            f,
            DiscoveryFailure::PeerStatusFailed(m) if m == "connection refused"
        ));
    }

    #[test]
    fn unparseable_peer_id_preserves_raw_and_error() {
        let uuid_err = "not-a-uuid".parse::<uuid::Uuid>().unwrap_err();
        let f: DiscoveryFailure = DiscoveryError::UnparseablePeerId("xyz".into(), uuid_err).into();
        assert!(matches!(
            f,
            DiscoveryFailure::UnparseablePeerId(raw, err_msg)
            if raw == "xyz" && !err_msg.is_empty()
        ));
    }

    // what this catches (the 5090, 2026-09-25, four days dark): the core started a
    // daemon and then refused to boot on the verdict its own start time had bought —
    // the 45 s budget was spent by the probe that preceded the recovery, so discovery
    // settled `degraded` 146 µs after `airc.daemon.recovered` and the daemon died with
    // the core. A daemon this pass STARTED resets the budget once, so a daemon that
    // needs 60–70 s to answer (2.4 GB event store) is still waited for; a recovery
    // OLDER than the loop never extends it, and with no recovery nothing changes.
    #[test]
    fn a_started_daemon_resets_the_patience_budget_and_a_stale_one_never_extends_it() {
        use std::time::Duration;
        let t0 = std::time::Instant::now();
        let spent = t0 + DISCOVERY_PATIENCE + Duration::from_secs(8); // the incident: 53 s of a 45 s budget
        let restarting = AircDiscovery::Degraded {
            reason: DiscoveryFailure::StaleSocket(PathBuf::from("/tmp/x.sock"), "gone".into()),
            partial: PartialDiscovery::default(),
        };

        // No recovery: the budget is spent and the verdict stands (unchanged behavior).
        let none = effective_elapsed(t0, None, spent);
        assert!(!should_retry(&restarting, none, DISCOVERY_PATIENCE), "spent budget, nothing started");

        // A daemon started just before the verdict: the budget runs from THERE, so the
        // loop keeps probing instead of refusing the daemon it just started.
        let daemon_at = spent - Duration::from_millis(1);
        let after = effective_elapsed(t0, Some(daemon_at), spent);
        assert!(after < Duration::from_secs(1), "measured from the daemon's start, got {after:?}");
        assert!(should_retry(&restarting, after, DISCOVERY_PATIENCE), "a started daemon is waited for");

        // Still bounded: once the daemon's own window is spent, the verdict settles.
        let long = effective_elapsed(t0, Some(daemon_at), daemon_at + DISCOVERY_PATIENCE + Duration::from_secs(1));
        assert!(!should_retry(&restarting, long, DISCOVERY_PATIENCE), "the reset is once, not forever");

        // A recovery from BEFORE this loop cannot extend this loop's budget.
        let older = effective_elapsed(t0, Some(t0 - Duration::from_secs(300)), spent);
        assert_eq!(older, spent.saturating_duration_since(t0), "an older start is ignored");

        // Review of #4386: a daemon that keeps dying re-runs recovery on every retry, so
        // DAEMON_STARTED_AT moves forward each time. The origin freezes at the FIRST start
        // newer than the loop, so a second, later start cannot extend the budget.
        let first = frozen_reset_origin(None, t0, Some(daemon_at));
        assert_eq!(first, Some(daemon_at), "the first start in this pass sets the origin");
        let second_start = daemon_at + DISCOVERY_PATIENCE;
        let still = frozen_reset_origin(first, t0, Some(second_start));
        assert_eq!(still, Some(daemon_at), "a later start never moves a frozen origin");
        let end = daemon_at + DISCOVERY_PATIENCE + Duration::from_secs(1);
        assert!(
            !should_retry(&restarting, effective_elapsed(t0, still, end), DISCOVERY_PATIENCE),
            "with a daemon dying and restarting, the budget still expires once"
        );
        assert_eq!(frozen_reset_origin(None, t0, Some(t0 - Duration::from_secs(5))), None, "an older start is ignored");

        // A failure waiting cannot fix is still refused on the first probe.
        let fatal = AircDiscovery::Unreachable { reason: DiscoveryFailure::AutoInstallDisabled };
        assert!(!should_retry(&fatal, Duration::ZERO, DISCOVERY_PATIENCE), "not transient");
    }

    /// `stale_socket_from_status_err` MUST construct a `StaleSocket`
    /// variant carrying the path AND the underlying error message.
    /// This is the structural fix for R2#1: Status RPC failure
    /// against an env-var-supplied socket no longer collapses to
    /// `Uuid::nil()` soft-fallback; it produces a typed reason the
    /// substrate refuses to construct an attribution-less transport
    /// against (per the from_discovery test).
    #[test]
    fn stale_socket_carries_path_and_status_err_message() {
        let socket = PathBuf::from("/tmp/stale.sock");
        let underlying = "ECONNREFUSED (connection refused)";
        let f = stale_socket_from_status_err(
            &socket,
            DiscoveryError::PeerStatusFailed(underlying.into()),
        );
        match f {
            DiscoveryFailure::StaleSocket(p, msg) => {
                assert_eq!(p, socket);
                assert!(msg.contains("ECONNREFUSED"));
            }
            other => panic!("expected StaleSocket, got {other:?}"),
        }
    }

    /// `stale_socket_from_status_err` with a non-PeerStatusFailed
    /// error still produces `StaleSocket` (the function is named for
    /// its purpose — any error reaching it means the socket isn't
    /// alive). The underlying message gets the full Display of the
    /// non-Status variant.
    #[test]
    fn stale_socket_handles_non_status_errors() {
        let socket = PathBuf::from("/tmp/stale.sock");
        let f = stale_socket_from_status_err(&socket, DiscoveryError::EmptyPath);
        assert!(matches!(f, DiscoveryFailure::StaleSocket(p, _) if p == socket));
    }

    /// what this catches: a daemon restart (stale socket) is waited out inside the
    /// budget, while a missing/disabled install is refused on the first probe.
    #[test]
    fn a_daemon_restart_is_waited_out_but_a_missing_install_is_not() {
        use std::time::Duration;
        let restarting = AircDiscovery::Degraded {
            reason: DiscoveryFailure::StaleSocket(PathBuf::from("/tmp/x.sock"), "gone".into()),
            partial: PartialDiscovery::default(),
        };
        assert!(should_retry(&restarting, Duration::from_secs(1), DISCOVERY_PATIENCE));
        assert!(!should_retry(&restarting, DISCOVERY_PATIENCE, DISCOVERY_PATIENCE));
        let disabled = AircDiscovery::Unreachable { reason: DiscoveryFailure::AutoInstallDisabled };
        assert!(!should_retry(&disabled, Duration::from_secs(1), DISCOVERY_PATIENCE));
        let broken = AircDiscovery::Unreachable { reason: DiscoveryFailure::InstallFailed("x".into()) };
        assert!(!is_transient(&broken));
    }

}
