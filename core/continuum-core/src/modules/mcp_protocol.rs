//! MCP JSON-RPC protocol handler — the headless-Rust MCP **server** brain.
//!
//! This is the Rust-native replacement for `src/mcp-server.ts` (871 lines of
//! Node + `@modelcontextprotocol/sdk`). The command path stays headless: an MCP
//! client (unsloth Studio, Claude Code) speaks JSON-RPC, and this handler turns
//! each call into a continuum command dispatch — no Node, no TS in the loop.
//!
//! ## The MCP server IS a client ([[persona-is-a-client]])
//!
//! The handler holds nothing but a [`CommandDispatch`] — the same
//! request/response verb every continuum client uses. It does NOT reach into
//! module internals:
//!
//! - `tools/list` → `dispatch.execute("mcp/list-tools", {})` (the catalog in
//!   [`super::mcp`] — single source of truth for "commands ARE tools").
//! - `tools/call` → maps the MCP tool name back to a command path and
//!   `dispatch.execute(command, arguments)`.
//!
//! So this layer is pure protocol framing over the command verb. In production
//! the `CommandDispatch` is a `continuum_client::Connection` over the in-process
//! or airc transport (gated like any caller); in tests it's a mock. The
//! transport + the stdio/HTTP bin that pumps bytes into [`McpServer::handle_message`]
//! is the next slice — this slice is the protocol, pure and testable.
//!
//! ## Scope (slice 1)
//!
//! `initialize`, `notifications/initialized`, `tools/list`, `tools/call`, plus
//! JSON-RPC error framing. Pure: a JSON-RPC message string in, an optional
//! response string out (None for notifications). No transport, no process.

use async_trait::async_trait;
use serde_json::{json, Value};

/// The MCP protocol revision this server advertises. Bump as the spec moves;
/// `2024-11-05` is the broadly-supported baseline (Claude Code, unsloth, etc.).
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// JSON-RPC error codes used by the handler (subset of the spec).
mod codes {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
}

/// The one verb the MCP server needs: dispatch a continuum command and get JSON
/// back (or a refusal string). Implemented in production by a
/// `continuum_client::Connection` (the MCP server is a client); in tests by a
/// mock. This is the seam that keeps the command path headless — no Node, and no
/// direct `CommandExecutor`/module coupling in the protocol layer.
#[async_trait]
pub trait CommandDispatch: Send + Sync {
    async fn execute(&self, command: &str, params: Value) -> Result<Value, String>;
}

/// Translate an MCP tool name back to a continuum command path.
///
/// The catalog ([`super::mcp`]) mints tool names by `command.replace('/', "_")`,
/// so the inverse is `replace('_', "/")` — `interface_screenshot` →
/// `interface/screenshot`, `collaboration_chat_send` → `collaboration/chat/send`.
///
/// The `mcp_*` meta-tools are the documented exception: the catalog hardcodes
/// underscore names (`mcp_search_tools`, `mcp_tool_help`) whose real commands use
/// a hyphen (`mcp/search-tools`, `mcp/tool-help`), so a blind `_`→`/` would
/// produce `mcp/search/tools`. Map those explicitly; everything else is the
/// general rule. (Tool names already use `_` for `/`, so a hyphen in a non-meta
/// command name round-trips fine — only the meta-tools collide.)
fn tool_name_to_command(tool_name: &str) -> String {
    match tool_name {
        "mcp_search_tools" => "mcp/search-tools".to_string(),
        "mcp_tool_help" => "mcp/tool-help".to_string(),
        other => other.replace('_', "/"),
    }
}

/// The MCP JSON-RPC protocol handler. Holds only the command dispatch — it's a
/// client of the headless core, nothing more.
pub struct McpServer<D: CommandDispatch> {
    dispatch: D,
    server_name: String,
    server_version: String,
}

impl<D: CommandDispatch> McpServer<D> {
    /// Build a server over `dispatch`. `server_name`/`server_version` are
    /// surfaced in the `initialize` handshake's `serverInfo`.
    pub fn new(dispatch: D, server_name: impl Into<String>, server_version: impl Into<String>) -> Self {
        Self {
            dispatch,
            server_name: server_name.into(),
            server_version: server_version.into(),
        }
    }

    /// Handle one JSON-RPC message. Returns `Some(response_json)` for requests
    /// (success or error), `None` for notifications (no `id`) — the transport
    /// writes `Some` back to the client and drops `None`.
    pub async fn handle_message(&self, message: &str) -> Option<String> {
        let req: Value = match serde_json::from_str(message) {
            Ok(v) => v,
            Err(e) => {
                // Parse error: no id available, per JSON-RPC reply with null id.
                return Some(error_response(Value::Null, codes::PARSE_ERROR, &format!("parse error: {e}")));
            }
        };

        let id = req.get("id").cloned();
        let method = req.get("method").and_then(|m| m.as_str());

        let Some(method) = method else {
            return id.map(|id| error_response(id, codes::INVALID_REQUEST, "missing `method`"));
        };

        // Notifications (no id) get no response regardless of method.
        let is_notification = id.is_none();

        match method {
            "initialize" => self.wrap(id, self.initialize_result()),
            "notifications/initialized" | "initialized" => None,
            "ping" => self.wrap(id, Ok(json!({}))),
            "tools/list" => {
                let result = self.tools_list().await;
                self.wrap(id, result)
            }
            "tools/call" => {
                let params = req.get("params").cloned().unwrap_or(Value::Null);
                let result = self.tools_call(params).await;
                self.wrap(id, result)
            }
            _ if is_notification => None,
            _ => id.map(|id| {
                error_response(id, codes::METHOD_NOT_FOUND, &format!("method not found: {method}"))
            }),
        }
    }

    /// Wrap a method result into a JSON-RPC response, or `None` for a
    /// notification (no id). An `Err` becomes a JSON-RPC error response.
    fn wrap(&self, id: Option<Value>, result: Result<Value, McpError>) -> Option<String> {
        let id = id?;
        Some(match result {
            Ok(value) => json!({ "jsonrpc": "2.0", "id": id, "result": value }).to_string(),
            Err(e) => error_response(id, e.code, &e.message),
        })
    }

    /// The `initialize` handshake result — protocol version + capabilities +
    /// serverInfo. We advertise the `tools` capability (this server exposes
    /// continuum commands as tools).
    fn initialize_result(&self) -> Result<Value, McpError> {
        Ok(json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": self.server_name, "version": self.server_version },
        }))
    }

    /// `tools/list` → the catalog from `mcp/list-tools`, reshaped to the MCP
    /// `{ tools: [...] }` envelope. The catalog's `MCPTool` shape already matches
    /// MCP (`name`, `description`, `inputSchema`), so we pass the array through.
    async fn tools_list(&self) -> Result<Value, McpError> {
        let resp = self
            .dispatch
            .execute("mcp/list-tools", json!({}))
            .await
            .map_err(McpError::internal)?;
        let tools = resp
            .get("tools")
            .cloned()
            .unwrap_or_else(|| json!([]));
        Ok(json!({ "tools": tools }))
    }

    /// `tools/call` → dispatch the underlying command, wrap its JSON result as an
    /// MCP `CallToolResult` (`content: [{ type: "text", text }]`). A command
    /// refusal becomes `isError: true` content (the MCP convention — the model
    /// sees the failure rather than the call throwing at the protocol layer).
    async fn tools_call(&self, params: Value) -> Result<Value, McpError> {
        let name = params
            .get("name")
            .and_then(|n| n.as_str())
            .ok_or_else(|| McpError::new(codes::INVALID_PARAMS, "tools/call: missing `name`"))?;
        let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));

        let command = tool_name_to_command(name);
        match self.dispatch.execute(&command, arguments).await {
            Ok(result) => Ok(tool_content(&result, false)),
            // A refused command is a TOOL error, not a protocol error: surface it
            // in the result content so the calling model can read + react.
            Err(reason) => Ok(tool_content(&json!({ "error": reason }), true)),
        }
    }
}

/// Build an MCP `CallToolResult` from a JSON payload: one text content block
/// carrying the serialized JSON, plus the `isError` flag.
fn tool_content(payload: &Value, is_error: bool) -> Value {
    json!({
        "content": [ { "type": "text", "text": payload.to_string() } ],
        "isError": is_error,
    })
}

/// A JSON-RPC error response string.
fn error_response(id: Value, code: i64, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }).to_string()
}

/// A protocol-level error (maps to a JSON-RPC `error` object).
struct McpError {
    code: i64,
    message: String,
}

impl McpError {
    fn new(code: i64, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
    fn internal(message: impl Into<String>) -> Self {
        Self::new(codes::INTERNAL_ERROR, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Mock dispatch: records the (command, params) it saw and returns a canned
    /// result per command — so the protocol layer is tested without a core.
    struct MockDispatch {
        calls: Mutex<Vec<(String, Value)>>,
        list_tools_result: Value,
        fail_on: Option<String>,
    }
    impl MockDispatch {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                list_tools_result: json!({
                    "success": true,
                    "tools": [
                        { "name": "ping", "description": "[JTAG] health", "inputSchema": { "type": "object", "properties": {} } },
                        { "name": "interface_screenshot", "description": "[JTAG] shot", "inputSchema": { "type": "object", "properties": {} } }
                    ],
                    "count": 2
                }),
                fail_on: None,
            }
        }
        fn failing(command: &str) -> Self {
            let mut s = Self::new();
            s.fail_on = Some(command.to_string());
            s
        }
    }
    #[async_trait]
    impl CommandDispatch for MockDispatch {
        async fn execute(&self, command: &str, params: Value) -> Result<Value, String> {
            self.calls.lock().unwrap().push((command.to_string(), params.clone()));
            if self.fail_on.as_deref() == Some(command) {
                return Err(format!("{command}: refused by mock"));
            }
            match command {
                "mcp/list-tools" => Ok(self.list_tools_result.clone()),
                _ => Ok(json!({ "ok": true, "echoed": command })),
            }
        }
    }

    fn server(d: MockDispatch) -> McpServer<MockDispatch> {
        McpServer::new(d, "continuum-mcp", "0.1.0")
    }

    // what this catches: the initialize handshake returns the protocol version +
    // tools capability + serverInfo — the shape an MCP client (unsloth/Claude
    // Code) requires before it will list/call tools.
    #[tokio::test]
    async fn initialize_returns_capabilities_and_server_info() {
        let s = server(MockDispatch::new());
        let resp = s
            .handle_message(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#)
            .await
            .expect("initialize is a request, must respond");
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["id"], 1);
        assert_eq!(v["result"]["protocolVersion"], MCP_PROTOCOL_VERSION);
        assert!(v["result"]["capabilities"]["tools"].is_object(), "advertises tools capability");
        assert_eq!(v["result"]["serverInfo"]["name"], "continuum-mcp");
    }

    // what this catches: tools/list pulls from the catalog command (single source
    // of truth — commands ARE tools) and reshapes to the MCP {tools:[...]} envelope.
    #[tokio::test]
    async fn tools_list_proxies_the_catalog_command() {
        let d = MockDispatch::new();
        let s = server(d);
        let resp = s
            .handle_message(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#)
            .await
            .unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        let tools = v["result"]["tools"].as_array().expect("tools array");
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0]["name"], "ping");
        // It dispatched the catalog command, not reached into module internals.
        assert_eq!(s.dispatch.calls.lock().unwrap()[0].0, "mcp/list-tools");
    }

    // what this catches: tools/call maps the MCP tool name back to the command
    // path and dispatches it with the given arguments; the JSON result is wrapped
    // as MCP text content. This is the core "MCP call → headless command" bridge.
    #[tokio::test]
    async fn tools_call_maps_name_and_dispatches() {
        let s = server(MockDispatch::new());
        let resp = s
            .handle_message(
                r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"interface_screenshot","arguments":{"querySelector":"body"}}}"#,
            )
            .await
            .unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["result"]["isError"], false);
        // dispatched the slashed command with the arguments verbatim
        let (cmd, params) = s.dispatch.calls.lock().unwrap()[0].clone();
        assert_eq!(cmd, "interface/screenshot");
        assert_eq!(params["querySelector"], "body");
        // result wrapped as text content carrying the command's JSON
        let text = v["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("interface/screenshot"));
    }

    // what this catches: the meta-tool name exception (underscore tool name →
    // hyphen command) — a blind _→/ would wrongly produce mcp/search/tools.
    #[tokio::test]
    async fn tools_call_maps_meta_tool_hyphen_exception() {
        let s = server(MockDispatch::new());
        s.handle_message(
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"mcp_search_tools","arguments":{"query":"chat"}}}"#,
        )
        .await
        .unwrap();
        assert_eq!(s.dispatch.calls.lock().unwrap()[0].0, "mcp/search-tools");
    }

    // what this catches: a refused command surfaces as isError content (the model
    // sees the failure), NOT a JSON-RPC protocol error — the MCP convention.
    #[tokio::test]
    async fn tools_call_refusal_is_iserror_content_not_protocol_error() {
        let s = server(MockDispatch::failing("data/wipe"));
        let resp = s
            .handle_message(
                r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"data_wipe","arguments":{}}}"#,
            )
            .await
            .unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert!(v.get("error").is_none(), "not a protocol error");
        assert_eq!(v["result"]["isError"], true);
        assert!(v["result"]["content"][0]["text"].as_str().unwrap().contains("refused"));
    }

    // what this catches: notifications (no id) get NO response; a request with an
    // unknown method gets a method-not-found error.
    #[tokio::test]
    async fn notifications_silent_and_unknown_method_errors() {
        let s = server(MockDispatch::new());
        // notifications/initialized is a notification → no response
        assert!(s
            .handle_message(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
            .await
            .is_none());
        // unknown method WITH id → method-not-found error
        let resp = s
            .handle_message(r#"{"jsonrpc":"2.0","id":9,"method":"bogus/method"}"#)
            .await
            .unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["error"]["code"], codes::METHOD_NOT_FOUND);
    }

    // what this catches: malformed JSON yields a JSON-RPC parse error (null id),
    // never a panic — the transport stays alive on garbage input.
    #[tokio::test]
    async fn malformed_json_is_parse_error_not_panic() {
        let s = server(MockDispatch::new());
        let resp = s.handle_message("{not json").await.unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["error"]["code"], codes::PARSE_ERROR);
        assert!(v["id"].is_null());
    }
}
