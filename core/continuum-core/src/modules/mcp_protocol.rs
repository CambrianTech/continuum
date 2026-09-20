//! MCP JSON-RPC protocol handler — the headless-Rust MCP **server** brain.
//!
//! This is the Rust-native replacement for `src/mcp-server.ts` (871 lines of
//! Node + `@modelcontextprotocol/sdk`). The command path stays headless: an MCP
//! client (unsloth Studio, Claude Code) speaks JSON-RPC, and this handler turns
//! each call into a continuum command dispatch — no Node, no TS in the loop.
//!
//! ## Strongly typed protocol
//!
//! Every MCP message shape is a real serde struct (`JsonRpcRequest`,
//! `InitializeResult`, `ListToolsResult`, `CallToolParams`, `CallToolResult`,
//! `ContentBlock`, …), parsed in and constructed out. Raw `serde_json::Value`
//! appears ONLY at the two genuinely-dynamic seams: a tool call's `arguments`
//! and a command's result payload (both are per-command JSON the protocol layer
//! must pass through, not interpret). So the PROTOCOL is type-checked end to end;
//! only the command payload it ferries is dynamic.
//!
//! ## The MCP server IS a client ([[persona-is-a-client]])
//!
//! The handler holds nothing but a [`CommandDispatch`] — the same
//! request/response verb every continuum client uses. It does NOT reach into
//! module internals:
//!
//! - `tools/list` → `dispatch.execute("mcp/list-tools", {})` (the catalog in
//!   [`super::mcp`] — single source of truth for "commands ARE tools"), and the
//!   returned tools are validated into typed [`MCPTool`]s.
//! - `tools/call` → maps the MCP tool name back to a command path and
//!   `dispatch.execute(command, arguments)`.
//!
//! In production the `CommandDispatch` is a `continuum_client::Connection` over
//! the in-process or airc transport (gated like any caller); in tests a mock.
//! The transport + the stdio/HTTP bin that pumps bytes into
//! [`McpServer::handle_message`] is the next slice — this slice is the protocol,
//! pure and testable.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::mcp::MCPTool;

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

// ─── JSON-RPC envelope (typed) ──────────────────────────────────────────────

/// An inbound JSON-RPC 2.0 message. `id` absent ⇒ notification (no response).
/// `params` stays `Value` here because its shape is method-dependent; each
/// method deserializes it into its own typed params struct.
#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

/// A successful JSON-RPC response with a typed `result`.
#[derive(Debug, Serialize)]
struct JsonRpcSuccess<T: Serialize> {
    jsonrpc: &'static str,
    id: Value,
    result: T,
}

/// A JSON-RPC error response.
#[derive(Debug, Serialize)]
struct JsonRpcErrorResponse {
    jsonrpc: &'static str,
    id: Value,
    error: JsonRpcError,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

// ─── MCP method params / results (typed) ────────────────────────────────────

/// `initialize` result — protocol version + capabilities + serverInfo, the
/// handshake an MCP client requires before listing/calling tools.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InitializeResult {
    protocol_version: &'static str,
    capabilities: ServerCapabilities,
    server_info: ServerInfo,
}

/// Capabilities this server advertises. We expose `tools` (continuum commands
/// surfaced as MCP tools); the empty object is the MCP "supported, no sub-caps"
/// shape.
#[derive(Debug, Serialize)]
struct ServerCapabilities {
    tools: ToolsCapability,
}

#[derive(Debug, Serialize)]
struct ToolsCapability {}

#[derive(Debug, Serialize)]
struct ServerInfo {
    name: String,
    version: String,
}

/// `tools/list` result — the typed tool catalog.
#[derive(Debug, Serialize)]
struct ListToolsResult {
    tools: Vec<MCPTool>,
}

/// The catalog command (`mcp/list-tools`) result we deserialize the tools out of.
#[derive(Debug, Deserialize)]
struct CatalogResult {
    #[serde(default)]
    tools: Vec<MCPTool>,
}

/// `tools/call` params. `arguments` stays `Value`: it's the called command's
/// own params, per-tool dynamic JSON the protocol passes straight through.
#[derive(Debug, Deserialize)]
struct CallToolParams {
    name: String,
    #[serde(default)]
    arguments: Value,
}

/// `tools/call` result — content blocks + the tool-level error flag. A command
/// refusal is `is_error: true` content (the model reads it), NOT a protocol error.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CallToolResult {
    content: Vec<ContentBlock>,
    is_error: bool,
}

impl CallToolResult {
    /// One text block carrying a serialized JSON payload (a command result or an
    /// error object), with the tool-error flag.
    fn text(payload: &Value, is_error: bool) -> Self {
        Self {
            content: vec![ContentBlock::Text {
                text: payload.to_string(),
            }],
            is_error,
        }
    }
}

/// An MCP content block. Tagged by `type`; `text` today, extensible (image,
/// resource) as the server grows multi-modal tool results.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ContentBlock {
    Text { text: String },
}

// ─── dispatch seam ──────────────────────────────────────────────────────────

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
/// general rule.
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
    pub fn new(
        dispatch: D,
        server_name: impl Into<String>,
        server_version: impl Into<String>,
    ) -> Self {
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
        // Parse the raw envelope first so we can recover `id` for error replies
        // even when the typed deserialize below fails.
        let raw: Value = match serde_json::from_str(message) {
            Ok(v) => v,
            Err(e) => {
                return Some(error_response(
                    Value::Null,
                    codes::PARSE_ERROR,
                    &format!("parse error: {e}"),
                ));
            }
        };
        let id = raw.get("id").cloned();

        let req: JsonRpcRequest = match serde_json::from_value(raw) {
            Ok(r) => r,
            Err(e) => {
                // Valid JSON but not a valid request (e.g. missing `method`).
                return id.map(|id| {
                    error_response(id, codes::INVALID_REQUEST, &format!("invalid request: {e}"))
                });
            }
        };

        match req.method.as_str() {
            "initialize" => self.wrap(req.id, Ok(self.initialize_result())),
            "notifications/initialized" | "initialized" => None,
            "ping" => self.wrap(req.id, Ok(EmptyResult {})),
            "tools/list" => {
                let result = self.tools_list().await;
                self.wrap(req.id, result)
            }
            "tools/call" => {
                let result = self.tools_call(req.params).await;
                self.wrap(req.id, result)
            }
            // Unknown method: notifications stay silent; requests get an error.
            _ => req.id.map(|id| {
                error_response(
                    id,
                    codes::METHOD_NOT_FOUND,
                    &format!("method not found: {}", req.method),
                )
            }),
        }
    }

    /// Wrap a typed method result into a JSON-RPC success/error response, or
    /// `None` for a notification (no id).
    fn wrap<T: Serialize>(&self, id: Option<Value>, result: Result<T, McpError>) -> Option<String> {
        let id = id?;
        Some(match result {
            Ok(value) => serde_json::to_string(&JsonRpcSuccess {
                jsonrpc: "2.0",
                id,
                result: value,
            })
            .unwrap_or_else(|e| {
                error_response(
                    Value::Null,
                    codes::INTERNAL_ERROR,
                    &format!("serialize: {e}"),
                )
            }),
            Err(e) => error_response(id, e.code, &e.message),
        })
    }

    fn initialize_result(&self) -> InitializeResult {
        InitializeResult {
            protocol_version: MCP_PROTOCOL_VERSION,
            capabilities: ServerCapabilities {
                tools: ToolsCapability {},
            },
            server_info: ServerInfo {
                name: self.server_name.clone(),
                version: self.server_version.clone(),
            },
        }
    }

    /// `tools/list` → the catalog from `mcp/list-tools`, validated into typed
    /// [`MCPTool`]s. The catalog is the single source of truth (commands ARE
    /// tools); deserializing it here also proves the catalog conforms to the
    /// MCP tool shape.
    async fn tools_list(&self) -> Result<ListToolsResult, McpError> {
        let resp = self
            .dispatch
            .execute("mcp/list-tools", Value::Object(Default::default()))
            .await
            .map_err(McpError::internal)?;
        let catalog: CatalogResult = serde_json::from_value(resp)
            .map_err(|e| McpError::internal(format!("catalog shape: {e}")))?;
        Ok(ListToolsResult {
            tools: catalog.tools,
        })
    }

    /// `tools/call` → dispatch the underlying command, wrap its JSON result as a
    /// typed [`CallToolResult`]. A command refusal becomes `isError` content
    /// (the MCP convention — the model sees the failure rather than the call
    /// throwing at the protocol layer).
    async fn tools_call(&self, params: Value) -> Result<CallToolResult, McpError> {
        let params: CallToolParams = serde_json::from_value(params)
            .map_err(|e| McpError::new(codes::INVALID_PARAMS, format!("tools/call params: {e}")))?;

        let command = tool_name_to_command(&params.name);
        Ok(
            match self.dispatch.execute(&command, params.arguments).await {
                Ok(result) => CallToolResult::text(&result, false),
                Err(reason) => CallToolResult::text(&serde_json::json!({ "error": reason }), true),
            },
        )
    }
}

/// Empty `{}` result (e.g. `ping`).
#[derive(Debug, Serialize)]
struct EmptyResult {}

/// A JSON-RPC error response string.
fn error_response(id: Value, code: i64, message: &str) -> String {
    serde_json::to_string(&JsonRpcErrorResponse {
        jsonrpc: "2.0",
        id,
        error: JsonRpcError {
            code,
            message: message.to_string(),
        },
    })
    .unwrap_or_else(|_| {
        // Last-resort literal — serialization of this fixed shape can't realistically fail.
        format!(r#"{{"jsonrpc":"2.0","id":null,"error":{{"code":{code},"message":"error"}}}}"#)
    })
}

/// A protocol-level error (maps to a JSON-RPC `error` object).
struct McpError {
    code: i64,
    message: String,
}

impl McpError {
    fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    fn internal(message: impl Into<String>) -> Self {
        Self::new(codes::INTERNAL_ERROR, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
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
            self.calls
                .lock()
                .unwrap()
                .push((command.to_string(), params.clone()));
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
    // tools capability + serverInfo — the typed shape an MCP client (unsloth/
    // Claude Code) requires before it will list/call tools.
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
        assert!(
            v["result"]["capabilities"]["tools"].is_object(),
            "advertises tools capability"
        );
        assert_eq!(v["result"]["serverInfo"]["name"], "continuum-mcp");
    }

    // what this catches: tools/list pulls from the catalog command (single source
    // of truth — commands ARE tools), validates into typed MCPTool, and reshapes
    // to the MCP {tools:[...]} envelope.
    #[tokio::test]
    async fn tools_list_proxies_and_types_the_catalog() {
        let s = server(MockDispatch::new());
        let resp = s
            .handle_message(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#)
            .await
            .unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        let tools = v["result"]["tools"].as_array().expect("tools array");
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0]["name"], "ping");
        assert_eq!(
            tools[1]["inputSchema"]["type"], "object",
            "typed MCPTool round-trip"
        );
        assert_eq!(s.dispatch.calls.lock().unwrap()[0].0, "mcp/list-tools");
    }

    // what this catches: tools/call maps the MCP tool name back to the command
    // path and dispatches it with the given arguments; the JSON result is wrapped
    // as a typed CallToolResult text block.
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
        let (cmd, params) = s.dispatch.calls.lock().unwrap()[0].clone();
        assert_eq!(cmd, "interface/screenshot");
        assert_eq!(params["querySelector"], "body");
        let text = v["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("interface/screenshot"));
        assert_eq!(v["result"]["content"][0]["type"], "text");
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
        assert!(v["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("refused"));
    }

    // what this catches: missing `name` on tools/call is an INVALID_PARAMS
    // protocol error (the typed CallToolParams deserialize fails loudly).
    #[tokio::test]
    async fn tools_call_missing_name_is_invalid_params() {
        let s = server(MockDispatch::new());
        let resp = s
            .handle_message(
                r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"arguments":{}}}"#,
            )
            .await
            .unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["error"]["code"], codes::INVALID_PARAMS);
    }

    // what this catches: notifications (no id) get NO response; a request with an
    // unknown method gets a method-not-found error.
    #[tokio::test]
    async fn notifications_silent_and_unknown_method_errors() {
        let s = server(MockDispatch::new());
        assert!(s
            .handle_message(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
            .await
            .is_none());
        let resp = s
            .handle_message(r#"{"jsonrpc":"2.0","id":9,"method":"bogus/method"}"#)
            .await
            .unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["error"]["code"], codes::METHOD_NOT_FOUND);
    }

    // what this catches: malformed JSON → parse error (null id); valid JSON
    // missing `method` → invalid request. Neither panics — the transport stays
    // alive on garbage input.
    #[tokio::test]
    async fn malformed_and_invalid_requests_error_cleanly() {
        let s = server(MockDispatch::new());
        let parse = s.handle_message("{not json").await.unwrap();
        let pv: Value = serde_json::from_str(&parse).unwrap();
        assert_eq!(pv["error"]["code"], codes::PARSE_ERROR);
        assert!(pv["id"].is_null());

        let invalid = s
            .handle_message(r#"{"jsonrpc":"2.0","id":7,"params":{}}"#)
            .await
            .unwrap();
        let iv: Value = serde_json::from_str(&invalid).unwrap();
        assert_eq!(iv["error"]["code"], codes::INVALID_REQUEST);
        assert_eq!(iv["id"], 7);
    }
}
