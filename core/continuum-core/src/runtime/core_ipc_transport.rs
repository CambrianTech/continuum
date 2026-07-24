//! `CoreIpcTransport` — a `continuum_client::Transport` that talks DIRECTLY to a
//! running core's IPC socket (the `ipc::start_server` Unix socket).
//!
//! ## Why (the local-sidecar path)
//!
//! [`AircIpcTransport`](continuum_client::AircIpcTransport) routes commands over
//! airc to a *peer* — the GRID/REMOTE path. A LOCAL sidecar (e.g. `continuum-mcp`
//! spawned beside the core) is the *same machine-account peer* as the core, so
//! addressing a command "to that peer" over airc doesn't route back to the core's
//! own handler (it timed out — live finding 2026-06-19). The local sidecar must
//! instead speak the core's IPC protocol on its Unix socket directly — which is
//! also simpler and airc-independent (more reliable).
//!
//! ## Wire protocol (mirrors `ipc/mod.rs`)
//!
//! - **Request**: one newline-terminated JSON object — the command params
//!   flattened with `command` + `requestId`. (The core's reader passes the whole
//!   object to `route_command`.)
//! - **Response**: a length-prefixed frame — `[u32 BE length][JSON payload]` where
//!   the payload is `{ success, result, error, requestId }`.
//!
//! Both shapes are TYPED structs ([`IpcRequest`], [`IpcResponse`]) — no `json!`
//! field-poking, so the wire contract is a struct, not magic.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
// The core's IPC socket is a Unix-domain socket. On Windows there is no Unix
// socket; alias to TcpStream so the field/signature types compile. The connect
// site is cfg-gated separately (a PathBuf is not a TCP address): on Windows it
// dials the core's TCP loopback listener (127.0.0.1:CONTINUUM_CORE_TCP, the
// PRIMARY IPC on Windows per ipc/mod.rs) — see the connect site below.
#[cfg(windows)]
use tokio::net::TcpStream as UnixStream;
#[cfg(unix)]
use tokio::net::UnixStream;
use tokio::sync::Mutex;

use continuum_client::event::EventStream;
use continuum_client::transport::{ServeHandler, Transport};
use continuum_client::ClientError;

/// Cap on a response frame, so a corrupt length prefix can't make us allocate
/// gigabytes. Generous — large tool results (search dumps, file reads) fit well
/// under this.
const MAX_FRAME_BYTES: u32 = 64 * 1024 * 1024;

/// Typed inbound request — the params object flattened with `command` +
/// `requestId`. `params` MUST be a JSON object (commands always pass one); the
/// transport normalizes a null/absent params to `{}` before constructing this.
#[derive(Debug, Serialize)]
struct IpcRequest<'a> {
    #[serde(flatten)]
    params: &'a Map<String, Value>,
    command: &'a str,
    #[serde(rename = "requestId")]
    request_id: u64,
}

/// Typed response frame payload — the core's wire response. Decoupled from the
/// core's internal `ipc::protocol::Response` (whose fields are crate-private):
/// this is the client-side contract.
#[derive(Debug, Deserialize)]
struct IpcResponse {
    success: bool,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<String>,
}

/// Direct connection to a core's IPC socket. Holds one lazily-opened stream
/// behind a `Mutex` — the MCP stdio loop is sequential (one command at a time),
/// so a single serialized connection is sufficient and keeps `requestId`
/// correlation trivial (one outstanding request).
pub struct CoreIpcTransport {
    socket_path: PathBuf,
    stream: Mutex<Option<UnixStream>>,
    next_id: AtomicU64,
}

impl CoreIpcTransport {
    /// Build a transport for the core listening at `socket_path` (e.g.
    /// `/tmp/continuum-core.sock`). Connection is lazy — opened on first
    /// `execute`, reopened if it drops.
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
            stream: Mutex::new(None),
            next_id: AtomicU64::new(1),
        }
    }

    /// One request/response round-trip over a connected stream. On any I/O error
    /// the caller drops the stream so the next call reconnects.
    async fn round_trip(
        stream: &mut UnixStream,
        command: &str,
        request_id: u64,
        params: &Map<String, Value>,
    ) -> Result<IpcResponse, ClientError> {
        // ── write the newline-terminated request ──
        let req = IpcRequest {
            params,
            command,
            request_id,
        };
        let mut line = serde_json::to_vec(&req).map_err(|e| ClientError::Codec(e.to_string()))?;
        line.push(b'\n');
        stream
            .write_all(&line)
            .await
            .map_err(|e| ClientError::Transport(format!("write request: {e}")))?;
        stream
            .flush()
            .await
            .map_err(|e| ClientError::Transport(format!("flush request: {e}")))?;

        // ── read the length-prefixed response frame ──
        let mut len_buf = [0u8; 4];
        stream
            .read_exact(&mut len_buf)
            .await
            .map_err(|e| ClientError::Transport(format!("read frame length: {e}")))?;
        let len = u32::from_be_bytes(len_buf);
        if len > MAX_FRAME_BYTES {
            return Err(ClientError::Transport(format!(
                "response frame too large ({len} bytes > {MAX_FRAME_BYTES} cap)"
            )));
        }
        let mut payload = vec![0u8; len as usize];
        stream
            .read_exact(&mut payload)
            .await
            .map_err(|e| ClientError::Transport(format!("read frame payload: {e}")))?;

        // JSON responses parse cleanly here. Binary frames (audio: JSON header +
        // \0 + bytes) don't — they're not part of the command/tool path the MCP
        // server uses, so surface a typed error rather than mis-parse.
        serde_json::from_slice::<IpcResponse>(&payload).map_err(|e| {
            ClientError::Codec(format!(
                "response frame was not a JSON command response (binary results \
                 aren't supported on the direct-IPC path): {e}"
            ))
        })
    }
}

#[async_trait]
impl Transport for CoreIpcTransport {
    async fn execute(&self, command: &str, params: Value) -> Result<Value, ClientError> {
        // Normalize params to an object (commands always carry one; null/absent → {}).
        let params_map = match params {
            Value::Object(m) => m,
            Value::Null => Map::new(),
            other => {
                return Err(ClientError::Codec(format!(
                    "command `{command}` params must be a JSON object, got {}",
                    kind_of(&other)
                )))
            }
        };
        let request_id = self.next_id.fetch_add(1, Ordering::Relaxed);

        let mut guard = self.stream.lock().await;
        // Lazy connect / reconnect if the stream is absent.
        if guard.is_none() {
            #[cfg(unix)]
            {
                let s = UnixStream::connect(&self.socket_path).await.map_err(|e| {
                    ClientError::Connect(format!(
                        "connect to core IPC socket {}: {e}",
                        self.socket_path.display()
                    ))
                })?;
                *guard = Some(s);
            }
            #[cfg(windows)]
            {
                // Windows has no Unix socket — the core's IPC is its TCP loopback
                // listener (see ipc/mod.rs; the TCP listener is PRIMARY on Windows).
                // Dial 127.0.0.1:<CONTINUUM_CORE_TCP, default 9100>; the socket_path
                // field is unused here. UnixStream is aliased to TcpStream on
                // windows, so this is a TCP connect.
                let port =
                    std::env::var("CONTINUUM_CORE_TCP").unwrap_or_else(|_| "9100".to_string());
                let addr = format!("127.0.0.1:{port}");
                let s = UnixStream::connect(&addr).await.map_err(|e| {
                    ClientError::Connect(format!("connect to core IPC (TCP {addr}): {e}"))
                })?;
                *guard = Some(s);
            }
        }

        let stream = guard.as_mut().expect("just ensured connected");
        let resp = match Self::round_trip(stream, command, request_id, &params_map).await {
            Ok(r) => r,
            Err(e) => {
                // Drop the (possibly broken) stream so the next call reconnects.
                *guard = None;
                return Err(e);
            }
        };

        if resp.success {
            Ok(resp.result.unwrap_or(Value::Null))
        } else {
            Err(ClientError::Refused {
                command: command.to_string(),
                reason: resp.error.unwrap_or_else(|| "command failed".to_string()),
            })
        }
    }

    async fn subscribe(&self, _class: &str) -> Result<EventStream, ClientError> {
        Err(ClientError::NotImplemented(
            "CoreIpcTransport::subscribe — event streaming over the direct-IPC path is a follow-up",
        ))
    }

    async fn emit(&self, _class: &str, _payload: Value) -> Result<(), ClientError> {
        Err(ClientError::NotImplemented(
            "CoreIpcTransport::emit — event publish over the direct-IPC path is a follow-up",
        ))
    }

    async fn provide(
        &self,
        _command: &str,
        _handler: std::sync::Arc<dyn ServeHandler>,
    ) -> Result<(), ClientError> {
        Err(ClientError::NotImplemented(
            "CoreIpcTransport::provide — serving a command over the direct-IPC path is a follow-up",
        ))
    }

    async fn revoke(&self, _command: &str) -> Result<(), ClientError> {
        Ok(()) // idempotent: nothing is provided over this transport
    }

    async fn close(&self) -> Result<(), ClientError> {
        *self.stream.lock().await = None;
        Ok(())
    }
}

/// Human-readable JSON kind for error messages.
fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::net::UnixListener;

    /// A minimal IPC server speaking the REAL wire protocol (newline JSON request
    /// in, length-prefixed JSON frame out) — so the transport is tested against
    /// the actual codec, not a mock of itself. Returns the temp socket path; the
    /// server handles one connection, echoing each request's command+params back
    /// as the result (or refusing a command named "refuse/me").
    async fn spawn_echo_ipc_server() -> PathBuf {
        // Unique per server instance — tests run in parallel in one binary, so a
        // shared (pid-only) path would collide. A per-call atomic counter isolates them.
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir();
        let path = dir.join(format!("cc-coreipc-test-{}-{n}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).expect("bind test socket");
        let path_ret = path.clone();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let (read_half, mut write_half) = stream.into_split();
            let mut lines = BufReader::new(read_half).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                let req: Value = serde_json::from_str(&line).expect("server: parse request");
                let command = req.get("command").and_then(|c| c.as_str()).unwrap_or("");
                let request_id = req.get("requestId").and_then(|r| r.as_u64());

                // Build the response payload (typed shape mirrored as json for the
                // server side — the SERVER is the core's role here).
                let payload = if command == "refuse/me" {
                    json!({ "success": false, "error": "refused by test server", "requestId": request_id })
                } else {
                    json!({ "success": true, "result": { "echoed": command, "saw": req }, "requestId": request_id })
                };
                let bytes = serde_json::to_vec(&payload).unwrap();
                let len = (bytes.len() as u32).to_be_bytes();
                write_half.write_all(&len).await.unwrap();
                write_half.write_all(&bytes).await.unwrap();
                write_half.flush().await.unwrap();
            }
        });

        // Give the listener a moment to be ready before the client connects.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        path_ret
    }

    // what this catches: the full direct-IPC round-trip over the REAL wire codec —
    // request is sent as a newline JSON object with command+requestId flattened
    // onto the params, the length-prefixed response frame is read + parsed, and the
    // inner result is returned. This is the path that fixes the airc self-peer
    // timeout (the live finding) and the coverage the unit MockTransport couldn't give.
    #[tokio::test]
    async fn execute_round_trips_over_the_real_wire_codec() {
        let socket = spawn_echo_ipc_server().await;
        let transport = CoreIpcTransport::new(&socket);

        let result = transport
            .execute("chat/send", json!({ "room": "general", "text": "hi" }))
            .await
            .expect("round-trip ok");

        // The server echoed our command + the full request it saw.
        assert_eq!(result["echoed"], "chat/send");
        // command + requestId were flattened onto the params on the wire.
        assert_eq!(result["saw"]["command"], "chat/send");
        assert_eq!(result["saw"]["room"], "general");
        assert_eq!(result["saw"]["text"], "hi");
        assert!(
            result["saw"]["requestId"].is_number(),
            "requestId stamped on the request"
        );

        let _ = std::fs::remove_file(&socket);
    }

    // what this catches: a core refusal (success:false + error) maps to
    // ClientError::Refused carrying the command + reason — so the MCP layer can
    // surface it as isError content rather than hanging or mis-reporting.
    #[tokio::test]
    async fn command_refusal_maps_to_refused_error() {
        let socket = spawn_echo_ipc_server().await;
        let transport = CoreIpcTransport::new(&socket);

        let err = transport
            .execute("refuse/me", json!({}))
            .await
            .expect_err("server refuses this command");
        match err {
            ClientError::Refused { command, reason } => {
                assert_eq!(command, "refuse/me");
                assert!(
                    reason.contains("refused by test server"),
                    "reason: {reason}"
                );
            }
            other => panic!("expected Refused, got {other:?}"),
        }
        let _ = std::fs::remove_file(&socket);
    }

    // what this catches: non-object params are rejected with a clear typed error
    // (commands always carry an object; a scalar would corrupt the flattened wire).
    #[tokio::test]
    async fn non_object_params_rejected_cleanly() {
        let transport = CoreIpcTransport::new("/nonexistent.sock");
        let err = transport
            .execute("x/y", json!(42))
            .await
            .expect_err("scalar params rejected");
        assert!(matches!(err, ClientError::Codec(_)), "got {err:?}");
    }
}
