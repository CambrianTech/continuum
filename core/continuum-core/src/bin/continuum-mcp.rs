//! `continuum-mcp` — the headless-Rust MCP server entrypoint.
//!
//! An MCP client (unsloth Studio, Claude Code, …) spawns this as a stdio
//! subprocess; it connects to the running local core over its IPC socket and
//! exposes continuum's commands as MCP tools. Rust-native replacement for
//! `src/mcp-server.ts` — no Node in the loop.
//!
//! It is a thin glue bin: all logic lives in [`continuum_core`] —
//! [`McpServer`] (typed JSON-RPC protocol), [`StdioRunner`] (the byte loop),
//! [`ConnectionDispatch`] (forwards each tool call to the core), and
//! [`CoreIpcTransport`] (the direct-IPC connection).
//!
//! ## Transport: direct to the local core's IPC socket
//!
//! A local sidecar is the *same machine-account peer* as the core, so routing a
//! command "to that peer" over airc doesn't reach the core's handler (it times
//! out — live finding 2026-06-19). So `continuum-mcp` talks to the core's IPC
//! socket DIRECTLY via [`CoreIpcTransport`] — simpler and airc-independent.
//! (Cross-grid MCP — addressing a *remote* core over airc — is a later path; a
//! remote core is a distinct peer, so it doesn't hit the self-peer issue.)
//!
//! ## Configuration
//!
//! - `CONTINUUM_CORE_SOCKET` — the core's IPC socket. Default
//!   `/tmp/continuum-core.sock` (matches `start-server.sh`). The MCP client
//!   config can be just `command: "continuum-mcp"`.
//!
//! ## stdout discipline
//!
//! MCP stdio framing means **only** JSON-RPC may go to stdout. All diagnostics
//! go to stderr. The runner writes solely to stdout; nothing else here prints
//! there.

use continuum_client::Connection;
use continuum_core::modules::mcp_protocol::McpServer;
use continuum_core::modules::mcp_transport::{ConnectionDispatch, StdioRunner};
use continuum_core::runtime::core_ipc_transport::CoreIpcTransport;

const SERVER_NAME: &str = "continuum-mcp";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_CORE_SOCKET: &str = "/tmp/continuum-core.sock";

#[tokio::main]
async fn main() -> Result<(), String> {
    let socket =
        std::env::var("CONTINUUM_CORE_SOCKET").unwrap_or_else(|_| DEFAULT_CORE_SOCKET.to_string());

    // Diagnostics → stderr only (stdout is the JSON-RPC channel). The connection
    // is lazy: a missing/dead core surfaces as a typed error on the first tool
    // call (and the MCP client sees it), not a startup hang.
    eprintln!("{SERVER_NAME} {SERVER_VERSION}: serving MCP over stdio; core IPC socket = {socket}");

    let transport = CoreIpcTransport::new(socket);
    let connection = Connection::new(transport);
    let server = McpServer::new(
        ConnectionDispatch::new(connection),
        SERVER_NAME,
        SERVER_VERSION,
    );
    let runner = StdioRunner::new(server);

    runner
        .run(tokio::io::stdin(), tokio::io::stdout())
        .await
        .map_err(|e| format!("stdio loop error: {e}"))?;
    Ok(())
}
