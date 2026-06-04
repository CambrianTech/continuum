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
    discover_airc_socket, discover_default_channel, discover_default_room_name,
    discover_peer_id, DiscoveryError,
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

impl From<DiscoveryError> for DiscoveryFailure {
    fn from(e: DiscoveryError) -> Self {
        match e {
            DiscoveryError::InstallFailed(msg) => DiscoveryFailure::InstallFailed(msg),
            DiscoveryError::AutoInstallDisabled => DiscoveryFailure::AutoInstallDisabled,
            DiscoveryError::EndpointCommandFailed(msg) => {
                DiscoveryFailure::EndpointCommandFailed(msg)
            }
            DiscoveryError::EmptyPath => DiscoveryFailure::EmptyPath,
            DiscoveryError::RoomCommandFailed(msg) => DiscoveryFailure::RoomCommandFailed(msg),
            DiscoveryError::UnparseableChannel(msg) => {
                DiscoveryFailure::UnparseableRoomOutput(msg)
            }
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
