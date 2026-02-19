//! JTAG MCP Server
//!
//! MCP (Model Context Protocol) server that exposes JTAG commands as tools.
//! Runs as a stdio transport server - reads JSON-RPC from stdin, writes to stdout.
//!
//! Architecture:
//! - Reads JSON-RPC messages from stdin
//! - Connects to continuum-core via Unix socket
//! - Routes MCP protocol messages to JTAG commands
//! - Single source of truth: tools discovered from registry at runtime
//! - Context injection: persona_id, db_path, workspace_root auto-added to commands
//!
//! Usage:
//!   jtag-mcp <socket-path> [options]
//!
//! Options:
//!   --persona-id=<id>       Default persona ID for code/* commands
//!   --db-path=<path>        Default database path for data/* commands
//!   --workspace-root=<path> Default workspace root for code/* commands
//!
//! Claude Desktop config:
//!   {
//!     "mcpServers": {
//!       "jtag": {
//!         "command": "/path/to/jtag-mcp",
//!         "args": [
//!           ".continuum/sockets/continuum-core.sock",
//!           "--persona-id=mcp-user",
//!           "--db-path=.continuum/jtag/data/database.sqlite",
//!           "--workspace-root=/path/to/project"
//!         ]
//!       }
//!     }
//!   }

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

// ============================================================================
// Utility Functions
// ============================================================================

/// Convert camelCase to snake_case
/// e.g., "filePath" -> "file_path", "startLine" -> "start_line"
fn camel_to_snake(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 5);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}

// ============================================================================
// MCP Protocol Types
// ============================================================================

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

impl JsonRpcResponse {
    fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<Value>, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: None,
            }),
        }
    }
}

// ============================================================================
// MCP Context - Injected into commands that need it
// ============================================================================

#[derive(Debug, Clone, Default)]
struct McpContext {
    /// Default persona ID for code/* commands
    persona_id: Option<String>,
    /// Default database path for data/* commands
    db_path: Option<String>,
    /// Default workspace root for code/* commands
    workspace_root: Option<String>,
}

impl McpContext {
    fn from_args(args: &[String]) -> Self {
        let mut ctx = Self::default();

        for arg in args {
            if let Some(value) = arg.strip_prefix("--persona-id=") {
                ctx.persona_id = Some(value.to_string());
            } else if let Some(value) = arg.strip_prefix("--db-path=") {
                ctx.db_path = Some(value.to_string());
            } else if let Some(value) = arg.strip_prefix("--workspace-root=") {
                ctx.workspace_root = Some(value.to_string());
            }
        }

        ctx
    }

    /// Inject context into command arguments based on command prefix
    fn inject(&self, command: &str, mut args: Map<String, Value>) -> Map<String, Value> {
        // Code commands need persona_id
        if command.starts_with("code/") {
            if !args.contains_key("persona_id") {
                if let Some(ref pid) = self.persona_id {
                    args.insert("persona_id".to_string(), json!(pid));
                }
            }
            // Also inject workspace_root for code/create-workspace
            if command == "code/create-workspace" && !args.contains_key("workspace_root") {
                if let Some(ref root) = self.workspace_root {
                    args.insert("workspace_root".to_string(), json!(root));
                }
            }
        }

        // Data commands need dbPath
        if command.starts_with("data/") || command.starts_with("vector/") {
            if !args.contains_key("dbPath") {
                if let Some(ref path) = self.db_path {
                    args.insert("dbPath".to_string(), json!(path));
                }
            }
        }

        args
    }
}

// ============================================================================
// JTAG Client (Unix socket IPC)
// ============================================================================

struct JtagClient {
    socket_path: PathBuf,
}

impl JtagClient {
    fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    fn execute(&self, command: &str, params: Value) -> Result<Value, String> {
        // Connect to Unix socket with timeout
        let stream = UnixStream::connect(&self.socket_path)
            .map_err(|e| format!("Failed to connect to continuum-core: {}. Is it running?", e))?;

        // Set read/write timeout to 60 seconds for large responses
        let timeout = std::time::Duration::from_secs(60);
        stream.set_read_timeout(Some(timeout)).ok();
        stream.set_write_timeout(Some(timeout)).ok();

        let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
        let mut writer = BufWriter::new(stream);

        // Build request - merge params at top level (not nested)
        // Protocol: {"command": "...", "field1": value, "field2": value, ...}
        let mut request = params.as_object().cloned().unwrap_or_default();
        request.insert("command".to_string(), json!(command));
        let request = Value::Object(request);

        let request_str = serde_json::to_string(&request).map_err(|e| e.to_string())?;

        // Send line-delimited JSON (server reads with BufReader::lines())
        writeln!(writer, "{}", request_str).map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;

        // Read response length
        let mut length_bytes = [0u8; 4];
        std::io::Read::read_exact(&mut reader, &mut length_bytes).map_err(|e| e.to_string())?;
        let response_length = u32::from_be_bytes(length_bytes) as usize;

        // Read response
        let mut response_bytes = vec![0u8; response_length];
        std::io::Read::read_exact(&mut reader, &mut response_bytes).map_err(|e| e.to_string())?;

        let response_str = String::from_utf8(response_bytes).map_err(|e| e.to_string())?;
        let response: Value = serde_json::from_str(&response_str).map_err(|e| e.to_string())?;

        // Check for error
        if let Some(error) = response.get("error").and_then(|e| e.as_str()) {
            return Err(error.to_string());
        }

        // Return result
        Ok(response.get("result").cloned().unwrap_or(json!(null)))
    }
}

// ============================================================================
// MCP Server
// ============================================================================

struct McpServer {
    client: JtagClient,
    context: McpContext,
    #[allow(dead_code)]
    tools_cache: Option<Vec<Value>>,
}

impl McpServer {
    fn new(socket_path: PathBuf, context: McpContext) -> Self {
        Self {
            client: JtagClient::new(socket_path),
            context,
            tools_cache: None,
        }
    }

    fn handle_request(&mut self, request: JsonRpcRequest) -> JsonRpcResponse {
        match request.method.as_str() {
            "initialize" => self.handle_initialize(request.id, request.params),
            "notifications/initialized" => {
                // No response needed for notifications
                JsonRpcResponse::success(request.id, json!({}))
            }
            "tools/list" => self.handle_list_tools(request.id),
            "tools/call" => self.handle_call_tool(request.id, request.params),
            _ => JsonRpcResponse::error(
                request.id,
                -32601,
                format!("Method not found: {}", request.method),
            ),
        }
    }

    fn handle_initialize(&self, id: Option<Value>, _params: Option<Value>) -> JsonRpcResponse {
        JsonRpcResponse::success(id, json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "jtag-mcp-server",
                "version": "2.0.0"
            }
        }))
    }

    fn handle_list_tools(&mut self, id: Option<Value>) -> JsonRpcResponse {
        // Fetch tools from continuum-core
        match self.client.execute("mcp/list-tools", json!({})) {
            Ok(result) => {
                let tools = result.get("tools").cloned().unwrap_or(json!([]));
                JsonRpcResponse::success(id, json!({
                    "tools": tools
                }))
            }
            Err(e) => {
                JsonRpcResponse::error(id, -32000, format!("Failed to list tools: {}", e))
            }
        }
    }

    fn handle_call_tool(&self, id: Option<Value>, params: Option<Value>) -> JsonRpcResponse {
        let params = match params {
            Some(p) => p,
            None => {
                return JsonRpcResponse::error(id, -32602, "Missing params".to_string());
            }
        };

        let tool_name = match params.get("name").and_then(|n| n.as_str()) {
            Some(n) => n,
            None => {
                return JsonRpcResponse::error(id, -32602, "Missing tool name".to_string());
            }
        };

        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

        // Handle MCP meta-tools
        if tool_name == "mcp_search_tools" {
            return self.call_jtag_command(id, "mcp/search-tools", arguments);
        }
        if tool_name == "mcp_tool_help" {
            return self.call_jtag_command(id, "mcp/tool-help", arguments);
        }

        // Convert MCP tool name back to JTAG command
        // MCP names have _ instead of / and -
        // Try the direct underscore-to-slash conversion first
        let command_name = tool_name.replace('_', "/");

        // For shell commands, TypeScript uses code/shell/execute but Rust uses code/shell-execute
        // Handle this by also trying the hyphen variant for code/shell/* commands
        let command_name = if command_name.starts_with("code/shell/") {
            command_name.replacen("code/shell/", "code/shell-", 1)
        } else {
            command_name
        };

        self.call_jtag_command(id, &command_name, arguments)
    }

    fn call_jtag_command(&self, id: Option<Value>, command: &str, args: Value) -> JsonRpcResponse {
        // Normalize parameter names: camelCase → snake_case
        // TypeScript uses camelCase (filePath) but Rust uses snake_case (file_path)
        let args_map = args.as_object().cloned().unwrap_or_default();
        let args_normalized: Map<String, Value> = args_map.into_iter()
            .map(|(k, v)| (camel_to_snake(&k), v))
            .collect();

        // Inject context (persona_id, db_path, etc.) based on command type
        let args_with_context = self.context.inject(command, args_normalized);
        let args = Value::Object(args_with_context);

        match self.client.execute(command, args) {
            Ok(result) => {
                // Format result for MCP
                let content = vec![json!({
                    "type": "text",
                    "text": serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
                })];

                JsonRpcResponse::success(id, json!({
                    "content": content
                }))
            }
            Err(e) => {
                let content = vec![json!({
                    "type": "text",
                    "text": format!("Error: {}", e)
                })];

                JsonRpcResponse::success(id, json!({
                    "content": content,
                    "isError": true
                }))
            }
        }
    }
}

// ============================================================================
// Main
// ============================================================================

fn main() {
    // Set up tracing to stderr (stdout is for JSON-RPC)
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <continuum-core-socket-path> [options]", args[0]);
        eprintln!();
        eprintln!("Options:");
        eprintln!("  --persona-id=<id>       Default persona ID for code/* commands");
        eprintln!("  --db-path=<path>        Default database path for data/* commands");
        eprintln!("  --workspace-root=<path> Default workspace root for code/* commands");
        eprintln!();
        eprintln!("Example:");
        eprintln!("  {} .continuum/sockets/continuum-core.sock \\", args[0]);
        eprintln!("    --persona-id=mcp-user \\");
        eprintln!("    --db-path=.continuum/jtag/data/database.sqlite");
        std::process::exit(1);
    }

    let socket_path = PathBuf::from(&args[1]);
    let context = McpContext::from_args(&args[2..]);

    tracing::info!("JTAG MCP Server starting");
    tracing::info!("Socket: {:?}", socket_path);
    if let Some(ref pid) = context.persona_id {
        tracing::info!("Persona ID: {}", pid);
    }
    if let Some(ref db) = context.db_path {
        tracing::info!("DB Path: {}", db);
    }
    if let Some(ref ws) = context.workspace_root {
        tracing::info!("Workspace Root: {}", ws);
    }

    let mut server = McpServer::new(socket_path, context);
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut stdout_lock = stdout.lock();

    // Read lines from stdin (JSON-RPC over stdio)
    for line in stdin.lock().lines() {
        match line {
            Ok(line) if line.trim().is_empty() => continue,
            Ok(line) => {
                match serde_json::from_str::<JsonRpcRequest>(&line) {
                    Ok(request) => {
                        tracing::debug!("Request: {:?}", request.method);
                        let response = server.handle_request(request);

                        // Write response
                        let response_str = serde_json::to_string(&response).unwrap();
                        writeln!(stdout_lock, "{}", response_str).ok();
                        stdout_lock.flush().ok();
                    }
                    Err(e) => {
                        tracing::error!("Failed to parse request: {}", e);
                        let response = JsonRpcResponse::error(
                            None,
                            -32700,
                            format!("Parse error: {}", e),
                        );
                        let response_str = serde_json::to_string(&response).unwrap();
                        writeln!(stdout_lock, "{}", response_str).ok();
                        stdout_lock.flush().ok();
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to read line: {}", e);
                break;
            }
        }
    }

    tracing::info!("JTAG MCP Server shutting down");
}
