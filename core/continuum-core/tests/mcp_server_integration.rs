//! Integration test: the MCP server protocol path against a LIVE core over the
//! REAL IPC socket transport.
//!
//! This rebuilds the exact composition `continuum-mcp` ships
//! (`CoreIpcTransport` → `Connection` → `ConnectionDispatch` → `McpServer`) and
//! round-trips `initialize` → `tools/list` → `tools/call`. It exists because the
//! unit tests in `mcp_protocol` / `mcp_transport` mock `CommandDispatch`, so they
//! never exercise the socket transport reaching a real core — exactly the gap
//! that let the airc self-peer timeout (continuum-mcp #21) slip past green unit
//! tests. This test pins the wiring unit tests can't reach.
//!
//! Convention: skip-if-no-live-core, like the other `tests/` integration suites
//! (`call_server_integration`, `ipc_voice_tests`). With a core running:
//!   npm start   # then, in another shell:
//!   cargo test -p continuum-core --features metal,accelerate --test mcp_server_integration
//!
//! Without a live core it skips cleanly (prints SKIP, returns) so plain
//! `cargo test` stays green. The hermetic in-process-serve variant (boot a
//! minimal runtime on a temp socket so this runs in CI without npm start) is the
//! tracked depth follow-up (#22).

// unix-only integration target (#304): dials the core UNIX IPC socket /
// sends unix signals. Windows checks compile it to empty; the lib +
// unit tests are the windows-supported surface today.
#![cfg(unix)]

mod common;

use continuum_client::Connection;
use continuum_core::modules::mcp_protocol::McpServer;
use continuum_core::modules::mcp_transport::ConnectionDispatch;
use continuum_core::runtime::core_ipc_transport::CoreIpcTransport;

/// Build the same `McpServer` the `continuum-mcp` bin builds, pointed at the
/// live core's IPC socket. Returns `None` (after printing SKIP) when no core is
/// reachable — matching the repo's integration-test convention.
fn mcp_server_against_live_core() -> Option<McpServer<ConnectionDispatch<CoreIpcTransport>>> {
    if !common::server_is_running() {
        println!(
            "SKIP: no live continuum-core IPC socket at {} — start one with `npm start`.",
            common::ipc_socket_path()
        );
        return None;
    }
    let transport = CoreIpcTransport::new(common::ipc_socket_path());
    let connection = Connection::new(transport);
    Some(McpServer::new(
        ConnectionDispatch::new(connection),
        "continuum-mcp-itest",
        "0.0.0",
    ))
}

// what this catches: the `initialize` handshake survives the real transport —
// a response comes back, ids line up, and serverInfo is populated. A broken
// transport (the self-peer timeout class) would hang or error here, not on a
// mock.
#[tokio::test]
async fn mcp_initialize_handshake_against_live_core() {
    let Some(server) = mcp_server_against_live_core() else {
        return;
    };
    let resp = server
        .handle_message(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#)
        .await
        .expect("initialize is a request → must respond");

    let v: serde_json::Value = serde_json::from_str(&resp).expect("response is JSON");
    assert_eq!(v["jsonrpc"], "2.0");
    assert_eq!(v["id"].as_i64(), Some(1), "id echoed: {v}");
    assert!(v["error"].is_null(), "no protocol error on initialize: {v}");
    assert!(
        v["result"]["serverInfo"]["name"].is_string(),
        "serverInfo present in handshake: {v}"
    );
}

// what this catches: `tools/list` actually round-trips to the core's command
// catalog over the socket (not a mocked list) and comes back non-empty with the
// always-present meta tools. This is the leg that proves transport → real
// dispatch → real catalog.
#[tokio::test]
async fn mcp_tools_list_round_trips_to_core_catalog() {
    let Some(server) = mcp_server_against_live_core() else {
        return;
    };
    let resp = server
        .handle_message(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#)
        .await
        .expect("tools/list is a request → must respond");

    let v: serde_json::Value = serde_json::from_str(&resp).expect("response is JSON");
    assert_eq!(v["id"].as_i64(), Some(2), "id echoed: {v}");
    assert!(v["error"].is_null(), "tools/list reached the core cleanly: {v}");

    let tools = v["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("tools array present: {v}"));
    assert!(!tools.is_empty(), "live core exposes a non-empty tool catalog");

    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert!(
        names.iter().any(|n| n.contains("search_tools")),
        "catalog includes the mcp search meta-tool: {names:?}"
    );
}

// what this catches: a `tools/call` is mapped (mcp_search_tools → mcp/search-tools,
// the hyphen-exception path) and dispatched through the socket to the core, and
// the reply is a well-formed CallToolResult — no PROTOCOL error (tool-level
// errors, if any, ride in result.isError content, which is still a valid call).
#[tokio::test]
async fn mcp_tools_call_dispatches_through_the_socket() {
    let Some(server) = mcp_server_against_live_core() else {
        return;
    };
    let req = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call",
        "params":{"name":"mcp_search_tools","arguments":{"query":"data"}}}"#;
    let resp = server
        .handle_message(req)
        .await
        .expect("tools/call is a request → must respond");

    let v: serde_json::Value = serde_json::from_str(&resp).expect("response is JSON");
    assert_eq!(v["id"].as_i64(), Some(3), "id echoed: {v}");
    assert!(
        v["error"].is_null(),
        "tools/call dispatched without a PROTOCOL error: {v}"
    );
    assert!(
        v["result"]["content"].is_array(),
        "CallToolResult carries content blocks: {v}"
    );
}
