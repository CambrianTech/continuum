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
        Err(e) => {
            return AircDiscovery::Degraded {
                reason: stale_socket_from_status_err(&socket, e),
                partial,
            };
        }
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
    loop {
        let d = discover().await;
        attempt += 1;
        if !should_retry(&d, started.elapsed(), DISCOVERY_PATIENCE) {
            if attempt > 1 {
                crate::probe!(
                    class = "airc.discovery.settled",
                    attempts = attempt,
                    waited_ms = started.elapsed().as_millis() as u64,
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
