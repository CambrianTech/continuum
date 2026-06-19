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
//! ## Configuration (env, fail-loud — no fallbacks)
//!
//! - `CONTINUUM_HOME`   — the airc scope/home dir (e.g. `~/.airc` or a repo `.airc`).
//! - `CONTINUUM_SOCKET` — the core daemon's IPC socket path.
//! - `CONTINUUM_PEER`   — the core's peer id (UUID) this client targets.
//! - `CONTINUUM_AGENT`  — this client's agent name (default `continuum-mcp`).
//!
//! MCP clients pass these via their server config's `env` block, mirroring how
//! the old TS server received its connection config.
//!
//! ## stdout discipline
//!
//! MCP stdio framing means **only** JSON-RPC may go to stdout. All diagnostics
//! go to stderr. The runner writes solely to stdout; nothing else here prints
//! there.

use std::path::PathBuf;
use std::sync::Arc;

use continuum_client::Connection;
use continuum_core::modules::mcp_protocol::McpServer;
use continuum_core::modules::mcp_transport::{ConnectionDispatch, StdioRunner};
use uuid::Uuid;

const SERVER_NAME: &str = "continuum-mcp";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Read a required env var or fail loud (no silent fallback — a misconfigured
/// MCP server should refuse to start, not connect to the wrong place).
fn require_env(key: &str) -> Result<String, String> {
    std::env::var(key).map_err(|_| format!("missing required env var {key}"))
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let home = require_env("CONTINUUM_HOME")?;
    let socket = require_env("CONTINUUM_SOCKET")?;
    let peer_str = require_env("CONTINUUM_PEER")?;
    let agent = std::env::var("CONTINUUM_AGENT").unwrap_or_else(|_| SERVER_NAME.to_string());

    // Parse the peer id FIRST — a bad id fails cheaply before we touch the
    // daemon, and never fabricates a target (mirrors continuum-client-ffi).
    let peer = Uuid::parse_str(&peer_str)
        .map_err(|e| format!("CONTINUUM_PEER is not a valid UUID: {e}"))?;

    // Diagnostics → stderr only (stdout is the JSON-RPC channel).
    eprintln!("{SERVER_NAME} {SERVER_VERSION}: attaching to core at {socket} as '{agent}' (peer {peer})");

    let airc = airc_lib::Airc::attach_as(PathBuf::from(home), &agent, socket)
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
