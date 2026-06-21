//! `cu` — the pure-Rust Continuum CLI client. Replaces the legacy Node `./jtag`.
//!
//! `cu <command> [json-params]` connects to a running core's IPC socket and
//! dispatches the command through the SAME [`Connection`] every client uses
//! (CLI, persona, web, mobile) — `[[persona-is-a-client]]`, `[[lock-uniform-client-early]]`.
//! No Node, no `tsx`, no bundle: one Rust binary that speaks the core's IPC
//! protocol directly via [`CoreIpcTransport`] (the same transport the
//! `continuum-mcp` sidecar uses).
//!
//! ```text
//! cu ping
//! cu ping '{"message":"hi"}'
//! cu data/list '{"collection":"users"}'
//! CONTINUUM_CORE_SOCKET=/tmp/continuum-core.sock cu ping
//! ```
//!
//! Output: the command's JSON result on stdout (pretty), diagnostics on stderr.
//! A substrate refusal or transport error exits non-zero with the message on
//! stderr — so `cu` composes in shell pipelines and CI like any Unix tool.

use continuum_client::Connection;
use continuum_core::runtime::core_ipc_transport::CoreIpcTransport;
use serde_json::Value;

const DEFAULT_CORE_SOCKET: &str = "/tmp/continuum-core.sock";

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("cu: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let command = args.next().ok_or_else(usage)?;
    if command == "-h" || command == "--help" || command == "help" {
        eprintln!("{}", usage());
        return Ok(());
    }

    // Optional JSON params (default empty object). Parse loud — a typo in the
    // params shouldn't silently send `{}`.
    let params: Value = match args.next() {
        Some(raw) => serde_json::from_str(&raw)
            .map_err(|e| format!("invalid JSON params: {e}\n(got: {raw})"))?,
        None => Value::Object(Default::default()),
    };

    let socket = std::env::var("CONTINUUM_CORE_SOCKET")
        .unwrap_or_else(|_| DEFAULT_CORE_SOCKET.to_string());

    // The uniform client: same Connection + execute_value path the persona tool
    // executor and recipe walker take. Lazy connect — a dead core surfaces as a
    // typed error here, not a hang.
    let connection = Connection::new(CoreIpcTransport::new(socket));
    let result = connection
        .commands()
        .execute_value(&command, params)
        .await
        .map_err(|e| format!("{command}: {e}"))?;

    // Pretty JSON to stdout — the command's result is the deliverable.
    println!(
        "{}",
        serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
    );
    Ok(())
}

fn usage() -> String {
    "usage: cu <command> [json-params]\n\
     \n\
     Examples:\n  \
       cu ping\n  \
       cu ping '{\"message\":\"hi\"}'\n  \
       cu data/list '{\"collection\":\"users\"}'\n\
     \n\
     Env: CONTINUUM_CORE_SOCKET (default /tmp/continuum-core.sock)"
        .to_string()
}
