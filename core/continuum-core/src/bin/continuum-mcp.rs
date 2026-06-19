//! `continuum-mcp` — the headless-Rust MCP server entrypoint.
//!
//! An MCP client (unsloth Studio, Claude Code, …) spawns this as a stdio
//! subprocess; it attaches to the running continuum core over airc IPC and
//! exposes continuum's commands as MCP tools. This is the Rust-native
//! replacement for `src/mcp-server.ts` — no Node in the loop.
//!
//! It is a thin glue bin: all logic lives in [`continuum_core::modules`] —
//! [`McpServer`] (typed JSON-RPC protocol), [`StdioRunner`] (the byte loop), and
//! [`ConnectionDispatch`] (forwards each tool call to the core over a
//! `continuum_client::Connection`, gated like any caller).
//!
//! ## Configuration — turnkey by default (auto-discovers the local core)
//!
//! With **no config at all** the server auto-discovers the running local core
//! via airc ([`continuum_core::airc::discover`]) — the same liveness probe the
//! core uses — so the MCP client config can be just `command: "continuum-mcp"`.
//! No manual peer-id/socket lookup (that lookup was the friction that bred
//! unreliability). Each value is overridable via env when needed:
//!
//! - `CONTINUUM_SOCKET` — airc daemon socket. Default: discovered.
//! - `CONTINUUM_PEER`   — the core's airc peer id (UUID). Default: discovered.
//! - `CONTINUUM_HOME`   — airc scope/home dir. Default: `$AIRC_HOME`, else
//!   `$HOME/.airc`.
//! - `CONTINUUM_AGENT`  — this client's agent name. Default: `continuum-mcp`.
//!
//! ## stdout discipline
//!
//! MCP stdio framing means **only** JSON-RPC may go to stdout. All diagnostics
//! go to stderr. The runner writes solely to stdout; nothing else here prints
//! there.

use std::path::PathBuf;
use std::sync::Arc;

use continuum_client::Connection;
use continuum_core::airc::{discover, AircDiscovery};
use continuum_core::modules::mcp_protocol::McpServer;
use continuum_core::modules::mcp_transport::{ConnectionDispatch, StdioRunner};
use uuid::Uuid;

const SERVER_NAME: &str = "continuum-mcp";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Resolve socket + peer: env override wins, else auto-discover the local core.
/// Discovery is the canonical liveness probe — if the core isn't up (or the
/// socket is stale) it fails loud here rather than hanging on a bad target.
async fn resolve_target() -> Result<(String, Uuid), String> {
    let env_socket = std::env::var("CONTINUUM_SOCKET").ok();
    let env_peer = std::env::var("CONTINUUM_PEER").ok();

    // Both supplied → use them (no discovery round-trip needed).
    if let (Some(socket), Some(peer_str)) = (&env_socket, &env_peer) {
        let peer = Uuid::parse_str(peer_str)
            .map_err(|e| format!("CONTINUUM_PEER is not a valid UUID: {e}"))?;
        return Ok((socket.clone(), peer));
    }

    // Otherwise discover the local core, then let any env value override.
    match discover().await {
        AircDiscovery::Healthy { socket, peer_id, .. } => {
            let socket = env_socket.unwrap_or_else(|| socket.to_string_lossy().into_owned());
            let peer = match env_peer {
                Some(p) => Uuid::parse_str(&p)
                    .map_err(|e| format!("CONTINUUM_PEER is not a valid UUID: {e}"))?,
                None => peer_id,
            };
            Ok((socket, peer))
        }
        other => Err(format!(
            "could not discover a healthy local core ({}); is `npm start` running? \
             Override with CONTINUUM_SOCKET + CONTINUUM_PEER.",
            other.kind()
        )),
    }
}

/// Resolve the airc scope/home: `CONTINUUM_HOME`, else `AIRC_HOME`, else
/// `$HOME/.airc` (the machine-account scope the core attaches under).
fn resolve_home() -> Result<PathBuf, String> {
    if let Ok(h) = std::env::var("CONTINUUM_HOME") {
        return Ok(PathBuf::from(h));
    }
    if let Ok(h) = std::env::var("AIRC_HOME") {
        return Ok(PathBuf::from(h));
    }
    let home = std::env::var("HOME").map_err(|_| {
        "cannot resolve airc home: set CONTINUUM_HOME (or HOME/AIRC_HOME)".to_string()
    })?;
    Ok(PathBuf::from(home).join(".airc"))
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let agent = std::env::var("CONTINUUM_AGENT").unwrap_or_else(|_| SERVER_NAME.to_string());
    let home = resolve_home()?;
    let (socket, peer) = resolve_target().await?;

    // Diagnostics → stderr only (stdout is the JSON-RPC channel).
    eprintln!(
        "{SERVER_NAME} {SERVER_VERSION}: attaching to core at {socket} as '{agent}' (peer {peer})"
    );

    let airc = airc_lib::Airc::attach_as(home, &agent, socket)
        .await
        .map_err(|e| format!("airc attach failed: {e}"))?;
    let connection = Connection::connect(Arc::new(airc), peer);

    let server = McpServer::new(
        ConnectionDispatch::new(connection),
        SERVER_NAME,
        SERVER_VERSION,
    );
    let runner = StdioRunner::new(server);

    eprintln!("{SERVER_NAME}: ready — serving MCP over stdio");
    runner
        .run(tokio::io::stdin(), tokio::io::stdout())
        .await
        .map_err(|e| format!("stdio loop error: {e}"))?;
    Ok(())
}
