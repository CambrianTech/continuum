//! MCP server transport — the byte layer that drives [`McpServer`] (slice 2).
//!
//! Slice 1 ([`super::mcp_protocol`]) is the pure protocol brain: a JSON-RPC
//! message string in, an optional response string out. This slice gives it:
//!
//! - [`StdioRunner`] — the newline-delimited-JSON stdio loop MCP clients
//!   (unsloth Studio, Claude Code) speak. Generic over any async reader/writer
//!   so it's unit-tested with in-memory pipes, not a real process.
//! - [`ConnectionDispatch`] — the production [`CommandDispatch`]: forwards each
//!   MCP `tools/call` to the headless core over a `continuum_client::Connection`
//!   (gated like any caller). This is what makes the MCP server *a client*
//!   ([[persona-is-a-client]]) rather than a privileged backdoor.
//!
//! The thin `continuum-mcp` bin wires these together: attach to the core over
//! airc IPC → `ConnectionDispatch` → `McpServer` → `StdioRunner::run(stdin,
//! stdout)`. No Node anywhere — this replaces `src/mcp-server.ts`.

use async_trait::async_trait;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};

use continuum_client::transport::Transport;
use continuum_client::Connection;

use super::mcp_protocol::{CommandDispatch, McpServer};

/// Drives an [`McpServer`] over a newline-delimited-JSON byte stream — the MCP
/// stdio transport. Each inbound line is one JSON-RPC message; each response is
/// written as one line. (MCP stdio framing is newline-delimited, not LSP
/// Content-Length; `serde_json` emits no embedded newlines, so one-message-per-
/// line holds.)
pub struct StdioRunner<D: CommandDispatch> {
    server: McpServer<D>,
}

impl<D: CommandDispatch> StdioRunner<D> {
    pub fn new(server: McpServer<D>) -> Self {
        Self { server }
    }

    /// Read JSON-RPC messages from `reader` line by line, dispatch each through
    /// the server, and write each non-empty response (notifications produce
    /// none) to `writer`, newline-terminated and flushed. Returns when the
    /// reader hits EOF (client disconnected). Blank lines are skipped; a
    /// malformed line still produces a JSON-RPC parse-error response (the loop
    /// never dies on bad input — that's [`McpServer::handle_message`]'s job).
    pub async fn run<R, W>(&self, reader: R, mut writer: W) -> std::io::Result<()>
    where
        R: tokio::io::AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let mut lines = BufReader::new(reader).lines();
        while let Some(line) = lines.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }
            if let Some(response) = self.server.handle_message(&line).await {
                writer.write_all(response.as_bytes()).await?;
                writer.write_all(b"\n").await?;
                writer.flush().await?;
            }
        }
        Ok(())
    }
}

/// The production [`CommandDispatch`]: forwards a command to the headless core
/// over a `continuum_client::Connection`. The MCP server holds one of these, so
/// every `tools/call` runs through the SAME gated command path any client uses —
/// no Node, no privileged bypass.
pub struct ConnectionDispatch<T: Transport> {
    connection: Connection<T>,
}

impl<T: Transport> ConnectionDispatch<T> {
    pub fn new(connection: Connection<T>) -> Self {
        Self { connection }
    }
}

#[async_trait]
impl<T: Transport> CommandDispatch for ConnectionDispatch<T> {
    async fn execute(&self, command: &str, params: Value) -> Result<Value, String> {
        // Value→Value: the MCP layer ferries dynamic command JSON; the typed
        // command surface is the SDK's concern, not the protocol bridge's.
        self.connection
            .commands()
            .execute::<Value, Value>(command, params)
            .await
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    /// Minimal dispatch for driving the stdio loop end-to-end.
    struct EchoDispatch {
        calls: Mutex<Vec<String>>,
    }
    impl EchoDispatch {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
            }
        }
    }
    #[async_trait]
    impl CommandDispatch for EchoDispatch {
        async fn execute(&self, command: &str, _params: Value) -> Result<Value, String> {
            self.calls.lock().unwrap().push(command.to_string());
            match command {
                "mcp/list-tools" => Ok(json!({ "tools": [] })),
                _ => Ok(json!({ "ran": command })),
            }
        }
    }

    // what this catches: the stdio transport drives the protocol end-to-end over
    // a byte stream — two newline-delimited requests in, two newline-delimited
    // responses out, in order. This is the real wire shape MCP clients speak.
    #[tokio::test]
    async fn stdio_runner_pumps_newline_delimited_requests() {
        let server = McpServer::new(EchoDispatch::new(), "continuum-mcp", "0.1.0");
        let runner = StdioRunner::new(server);

        let input = concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            "\n",
        );
        let mut output: Vec<u8> = Vec::new();
        runner
            .run(input.as_bytes(), &mut output)
            .await
            .expect("run to EOF");

        let out = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2, "one response line per request:\n{out}");
        let r1: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(r1["id"], 1);
        assert_eq!(r1["result"]["protocolVersion"], "2024-11-05");
        let r2: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(r2["id"], 2);
        assert!(r2["result"]["tools"].is_array());
    }

    // what this catches: a notification (no id) produces NO output line, so the
    // transport doesn't emit phantom responses; a following request still answers.
    #[tokio::test]
    async fn stdio_runner_skips_notifications_and_blank_lines() {
        let server = McpServer::new(EchoDispatch::new(), "continuum-mcp", "0.1.0");
        let runner = StdioRunner::new(server);

        let input = concat!(
            "\n", // blank line
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":7,"method":"ping"}"#,
            "\n",
        );
        let mut output: Vec<u8> = Vec::new();
        runner.run(input.as_bytes(), &mut output).await.unwrap();

        let out = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(
            lines.len(),
            1,
            "only the ping request gets a response:\n{out}"
        );
        let v: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(v["id"], 7);
    }

    // what this catches: ConnectionDispatch forwards through a real
    // continuum_client::Connection — a tools/call reaches the command path with
    // its arguments and the command result comes back as tool content. Uses the
    // client's MockTransport so it's the genuine Connection→CommandClient path,
    // not a hand mock of the dispatch.
    #[tokio::test]
    async fn connection_dispatch_forwards_through_a_real_connection() {
        use continuum_client::mock::MockTransport;

        let mock = MockTransport::new();
        mock.respond_with(
            "interface/screenshot",
            json!({ "success": true, "dataUrl": "x" }),
        );
        let conn = Connection::new(mock);

        let server = McpServer::new(ConnectionDispatch::new(conn), "continuum-mcp", "0.1.0");
        let resp = server
            .handle_message(
                r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"interface_screenshot","arguments":{"querySelector":"body"}}}"#,
            )
            .await
            .unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["result"]["isError"], false);
        let text = v["result"]["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("dataUrl"),
            "command result ferried back: {text}"
        );
    }
}
