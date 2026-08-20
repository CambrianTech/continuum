//! Typed `AircDiscovery` state — the substrate's single answer to
//! "what is the airc daemon doing right now?"
//!
//! ## Why a typed enum
//!
//! Before A.2, AIRC discovery returned `Option<(PathBuf, RoomId)>` —
//! a tuple where presence meant "all four sub-discoveries succeeded"
//! and absence collapsed every failure mode into one. The
//! [[no-fallbacks-ever]] doctrine demands the substrate name its
//! degraded states explicitly so the operator can act on them. A
//! typed enum makes the variants exhaustive: every `match` against
//! `AircDiscovery` is forced to consider `Healthy`, `Degraded { reason }`,
//! and `Unreachable { reason }`.
//!
//! ## Why a liveness probe
//!
//! Slice A shipped a hard-fail check ("if persona seeds exist + AIRC
//! degraded → refuse boot") but the env-var override path
//! (`AIRC_DAEMON_SOCKET=/path/to/dead.sock`) bypassed every check —
//! discovery returned the path with no liveness verification, the
//! module registered, the operator saw "✅ All N modules registered"
//! and "🌐 The Grid hosts citizen Paige," and every IPC call
//! ECONNREFUSED. This is the bug R2 found in the Slice A review.
//!
//! `AircDiscovery::Healthy` is now only producible AFTER a successful
//! Status RPC round-trip against the socket. Stale-socket promotes to
//! `Degraded { reason: StaleSocket }`, not soft-fallback-to-nil-peer.
//!
//! ## Threaded through `Context`
//!
//! Slices 1–4 of #142 established `Context` as the universal actor
//! handle. A.2 extends it with `discovery(&self) -> &AircDiscovery`
//! so every actor (persona, agent, jtag, human, web) carries the
//! discovery state that was true at the moment they were created.
//! No more bare paths floating without provenance. B' will generalize
//! this from one axis (airc) to N axes (renderer, voice, inference,
//! foundry) via the category-handle pattern.

use std::path::PathBuf;

use airc_core::RoomId;
use uuid::Uuid;

/// The substrate's typed answer to "what is the airc daemon doing
/// right now?"
///
/// Exhaustive — every match is forced to handle each variant. The
/// three variants are ordered by user-actionability:
/// `Healthy` is the success path, `Degraded` means the daemon is
/// reachable but cannot fully serve persona hosting (operator
/// remediates), `Unreachable` means the daemon is not running or
/// not on the operator's machine (operator installs / runs airc).
#[derive(Debug, Clone)]
pub enum AircDiscovery {
    /// All four sub-discoveries succeeded AND a Status RPC round-trip
    /// against the socket confirmed the daemon responds. This is the
    /// only state from which `PersonaInstanceManagerModule` can be
    /// registered.
    Healthy {
        socket: PathBuf,
        default_room: RoomId,
        room_name: String,
        peer_id: Uuid,
    },
    /// Socket was discovered but at least one downstream check
    /// failed — the daemon is reachable enough to expose a path,
    /// but cannot fully serve persona hosting. `partial` carries
    /// whatever we DID resolve so observability can pinpoint the
    /// remaining gap.
    Degraded {
        reason: DiscoveryFailure,
        partial: PartialDiscovery,
    },
    /// Socket itself could not be discovered. airc not on PATH,
    /// auto-install disabled, `airc ipc-endpoint` failed, empty
    /// path returned. The operator cannot reach the substrate's
    /// expected airc surface.
    Unreachable { reason: DiscoveryFailure },
}

impl AircDiscovery {
    /// `true` iff persona hosting can proceed against this state.
    /// Equivalent to `matches!(self, AircDiscovery::Healthy { .. })`
    /// but named for the question the caller is actually asking.
    pub fn can_host_personas(&self) -> bool {
        matches!(self, AircDiscovery::Healthy { .. })
    }

    /// Short human-readable kind for log lines + boot banner.
    pub fn kind(&self) -> &'static str {
        match self {
            AircDiscovery::Healthy { .. } => "healthy",
            AircDiscovery::Degraded { .. } => "degraded",
            AircDiscovery::Unreachable { .. } => "unreachable",
        }
    }

    /// Borrow the typed failure (if any) for structured logging.
    pub fn reason(&self) -> Option<&DiscoveryFailure> {
        match self {
            AircDiscovery::Healthy { .. } => None,
            AircDiscovery::Degraded { reason, .. } | AircDiscovery::Unreachable { reason } => {
                Some(reason)
            }
        }
    }
}

/// Everything the substrate WAS able to resolve before the failure
/// that produced `AircDiscovery::Degraded`. Used for observability —
/// the operator's remediation depends on knowing whether we got
/// the socket but lost the room, vs. got the socket but the daemon
/// is dead, etc.
#[derive(Debug, Clone, Default)]
pub struct PartialDiscovery {
    pub socket: Option<PathBuf>,
    pub default_room: Option<RoomId>,
    pub room_name: Option<String>,
    pub peer_id: Option<Uuid>,
}

/// Typed failure reasons. Every error path the substrate can take
/// during AIRC discovery maps to exactly one variant; the operator's
/// remediation depends on which.
#[derive(Debug, Clone, thiserror::Error)]
pub enum DiscoveryFailure {
    #[error("airc binary not on PATH and auto-install was disabled (CONTINUUM_NO_AUTOINSTALL=1)")]
    AutoInstallDisabled,

    #[error("airc not on PATH — installing in the background; node is UP (local commands work), restart the core once the install completes to join airc as a grid peer")]
    AutoInstallInProgress,

    #[error("airc binary install failed: {0}")]
    InstallFailed(String),

    #[error(
        "`airc ipc-endpoint` failed: {0}\n\
         remediation: ensure airc is installed (curl -fsSL https://airc.sh | bash) \
         OR set AIRC_DAEMON_SOCKET=<path>"
    )]
    EndpointCommandFailed(String),

    #[error(
        "`airc ipc-endpoint` returned an empty path — airc binary may be from before \
         the ipc-endpoint subcommand (task #79); upgrade airc"
    )]
    EmptyPath,

    #[error(
        "daemon socket at {0} is unreachable: {1}\n\
         most likely cause: a stale socket file from a daemon that has exited. \
         remediation: remove the stale socket and restart airc, OR point \
         AIRC_DAEMON_SOCKET at a live daemon's socket"
    )]
    StaleSocket(PathBuf, String),

    #[error("daemon Status RPC failed: {0}")]
    PeerStatusFailed(String),

    #[error("daemon Status returned unparseable peer_id ({0:?}): {1}")]
    UnparseablePeerId(String, String),

    #[error(
        "`airc room` failed: {0}\n\
         remediation: run `airc room <name>` to subscribe the scope to a room"
    )]
    RoomCommandFailed(String),

    #[error(
        "`airc room` output did not contain a parseable channel: {0}\n\
         remediation: upgrade airc OR set AIRC_DEFAULT_CHANNEL=<uuid>"
    )]
    UnparseableRoomOutput(String),

    #[error("no default room set — run `airc room <name>` to subscribe the scope to a room")]
    NoDefaultRoom,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn healthy_can_host_personas() {
        let d = AircDiscovery::Healthy {
            socket: PathBuf::from("/tmp/x.sock"),
            default_room: RoomId::from_uuid(Uuid::new_v4()),
            room_name: "general".into(),
            peer_id: Uuid::new_v4(),
        };
        assert!(d.can_host_personas());
        assert_eq!(d.kind(), "healthy");
        assert!(d.reason().is_none());
    }

    #[test]
    fn degraded_cannot_host_personas() {
        let d = AircDiscovery::Degraded {
            reason: DiscoveryFailure::NoDefaultRoom,
            partial: PartialDiscovery {
                socket: Some(PathBuf::from("/tmp/x.sock")),
                ..Default::default()
            },
        };
        assert!(!d.can_host_personas());
        assert_eq!(d.kind(), "degraded");
        assert!(matches!(d.reason(), Some(DiscoveryFailure::NoDefaultRoom)));
    }

    #[test]
    fn unreachable_cannot_host_personas() {
        let d = AircDiscovery::Unreachable {
            reason: DiscoveryFailure::AutoInstallDisabled,
        };
        assert!(!d.can_host_personas());
        assert_eq!(d.kind(), "unreachable");
        assert!(matches!(
            d.reason(),
            Some(DiscoveryFailure::AutoInstallDisabled)
        ));
    }

    /// StaleSocket — the bug R2 caught — is now a first-class
    /// variant carrying both the socket path AND the underlying
    /// IO error so the operator knows whether it was ECONNREFUSED,
    /// EACCES, or "file exists but not a socket."
    #[test]
    fn stale_socket_carries_path_and_io_reason() {
        let reason =
            DiscoveryFailure::StaleSocket(PathBuf::from("/tmp/dead.sock"), "ECONNREFUSED".into());
        let display = format!("{reason}");
        assert!(display.contains("/tmp/dead.sock"));
        assert!(display.contains("ECONNREFUSED"));
        assert!(display.contains("stale socket"));
    }
}
