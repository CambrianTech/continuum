//! Local-substrate attachment — how a client process reaches the RUNNING
//! continuum-core through the machine's airc daemon.
//!
//! ## Why this module exists (the trap it closes)
//!
//! `Airc::open(home)` is **owner-mode**: it opens the scope's store and
//! identity but connects to NO daemon — peer-addressed frames sit in the
//! caller's own in-process node with no admissible route, and every
//! dispatch dies at the command deadline. `ctm grid-smoke` shipped exactly
//! that bug (0/3, 30s deadline per spec, 2026-08-27). The correct verb for
//! a client is `Airc::attach_as`: own agent identity + the daemon's live
//! routing, same as continuum-core's own `continuum-airc-interceptor`.
//!
//! ## Resolution (mirrors core/continuum-core/src/airc/discovery.rs)
//!
//! - Socket: `$AIRC_DAEMON_SOCKET` override, else ask `airc ipc-endpoint`
//!   (airc#1095). We ask airc rather than re-derive the path — the stale
//!   parallel derivation in `daemon_endpoint.rs` never matched what the
//!   daemon binds and broke headless boot; that lesson is load-bearing.
//! - Substrate peer: the daemon's typed `Status` RPC (`airc-ipc`
//!   `DaemonClient`), never stdout parsing.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use airc_lib::Airc;
use uuid::Uuid;

use crate::error::ClientError;

/// Explicit socket-path override. Honored unconditionally — same env var
/// continuum-core's discovery honors, so one override steers both.
pub const AIRC_DAEMON_SOCKET_ENV: &str = "AIRC_DAEMON_SOCKET";

/// Deadline for the `airc ipc-endpoint` subprocess and the Status RPC.
/// Matches airc-ipc's `DEFAULT_RPC_TIMEOUT`.
const DISCOVERY_DEADLINE: Duration = Duration::from_secs(5);

/// A live, daemon-routed connection to the local substrate.
pub struct SubstrateAttachment {
    /// Daemon-connected handle — hand to [`crate::AircIpcTransport::new`].
    pub airc: Arc<Airc>,
    /// The daemon's peer id: the UUID substrate commands are addressed to.
    pub substrate_peer: Uuid,
}

/// Discover the airc daemon's IPC socket path.
pub async fn discover_daemon_socket() -> Result<PathBuf, ClientError> {
    if let Some(path) = std::env::var_os(AIRC_DAEMON_SOCKET_ENV) {
        return Ok(PathBuf::from(path));
    }

    let call = tokio::process::Command::new("airc")
        .arg("ipc-endpoint")
        .output();
    let out = tokio::time::timeout(DISCOVERY_DEADLINE, call)
        .await
        .map_err(|_| {
            ClientError::Connect(format!(
                "`airc ipc-endpoint` did not exit within {DISCOVERY_DEADLINE:?}"
            ))
        })?
        .map_err(|e| {
            ClientError::Connect(format!(
                "`airc ipc-endpoint` failed to launch: {e} — is airc installed? \
                 (curl -fsSL https://raw.githubusercontent.com/CambrianTech/airc/main/install.sh | bash)"
            ))
        })?;
    if !out.status.success() {
        return Err(ClientError::Connect(format!(
            "`airc ipc-endpoint` exit {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if path.is_empty() {
        return Err(ClientError::Connect(
            "`airc ipc-endpoint` returned an empty path — airc predates #1095; upgrade airc"
                .to_string(),
        ));
    }
    Ok(PathBuf::from(path))
}

/// Ask the daemon who it is. This is the peer UUID a local client targets
/// to reach the substrate (continuum-core answers command frames through
/// its interceptor attached to this same daemon).
pub async fn discover_substrate_peer(socket: &std::path::Path) -> Result<Uuid, ClientError> {
    let client = airc_ipc::DaemonClient::new(socket.to_path_buf());
    let status = client
        .status_with_timeout(DISCOVERY_DEADLINE)
        .await
        .map_err(|e| {
            ClientError::Connect(format!(
                "daemon Status RPC failed at {}: {e} — is the airc daemon running? (`airc status`)",
                socket.display()
            ))
        })?;
    status.peer_id.parse::<Uuid>().map_err(|e| {
        ClientError::Connect(format!(
            "daemon Status returned an unparseable peer_id ({:?}): {e}",
            status.peer_id
        ))
    })
}

/// Attach to the local substrate: discover the daemon socket, attach with
/// an agent identity of our own (stored under `home`), and resolve the
/// substrate's peer UUID. `peer_override` skips Status discovery — for
/// targeting a REMOTE grid peer instead of the local daemon.
pub async fn attach_local_substrate(
    home: PathBuf,
    agent_name: &str,
    peer_override: Option<Uuid>,
) -> Result<SubstrateAttachment, ClientError> {
    let socket = discover_daemon_socket().await?;
    let substrate_peer = match peer_override {
        Some(p) => p,
        None => discover_substrate_peer(&socket).await?,
    };
    let airc = Airc::attach_as(home, agent_name, socket)
        .await
        .map_err(|e| ClientError::Connect(format!("airc attach_as failed: {e}")))?;
    Ok(SubstrateAttachment {
        airc: Arc::new(airc),
        substrate_peer,
    })
}
