#![allow(dead_code)]
//! Shared IPC client for integration tests.
//!
//! Defines the length-prefixed binary framing protocol in ONE place.
//! All integration tests that communicate with the continuum-core IPC server
//! MUST use this module instead of hand-rolling socket reads.
//!
//! ## Protocol (server → client)
//!
//! Requests:  newline-delimited JSON (client → server)
//! Responses: length-prefixed binary framing (server → client)
//!
//! ```text
//! [4 bytes u32 BE: payload length][payload bytes]
//! ```
//!
//! Payload variants:
//! - **JSON**: entire payload is valid UTF-8 JSON
//! - **Binary**: `[JSON header bytes][\0 separator][raw binary data]`
//!
//! The `\0` separator is unambiguous — serde_json encodes null chars as `\u0000`.

use serde::{Deserialize, Serialize};
use std::io::{Read as IoRead, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

pub const IPC_SOCKET: &str = "/tmp/continuum-core.sock";

// ============================================================================
// Response Types
// ============================================================================

/// Parsed JSON fields from an IPC response.
#[derive(Deserialize, Debug)]
pub struct IpcResponse {
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

/// Full IPC result — either pure JSON or JSON header + binary payload.
pub enum IpcResult {
    Json(IpcResponse),
    Binary { header: IpcResponse, data: Vec<u8> },
}

impl IpcResult {
    /// Get the response (header for binary, response for JSON).
    #[allow(dead_code)]
    pub fn response(&self) -> &IpcResponse {
        match self {
            IpcResult::Json(r) => r,
            IpcResult::Binary { header, .. } => header,
        }
    }

    /// Unwrap as JSON response, panicking if binary.
    #[allow(dead_code)]
    pub fn into_json(self) -> IpcResponse {
        match self {
            IpcResult::Json(r) => r,
            IpcResult::Binary { header, .. } => header,
        }
    }

    /// Unwrap as binary response, panicking if JSON-only.
    #[allow(dead_code)]
    pub fn into_binary(self) -> (IpcResponse, Vec<u8>) {
        match self {
            IpcResult::Binary { header, data } => (header, data),
            IpcResult::Json(r) => panic!(
                "Expected binary IPC response, got JSON-only: success={}, error={:?}",
                r.success, r.error
            ),
        }
    }
}

// ============================================================================
// IPC Client
// ============================================================================

/// Connect to the IPC socket with timeouts.
/// Returns `None` (and prints skip message) if the server isn't running.
pub fn ipc_connect() -> Option<UnixStream> {
    ipc_connect_with_timeout(Duration::from_secs(30))
}

/// Connect with a custom read timeout.
pub fn ipc_connect_with_timeout(read_timeout: Duration) -> Option<UnixStream> {
    match UnixStream::connect(IPC_SOCKET) {
        Ok(s) => {
            s.set_read_timeout(Some(read_timeout)).ok();
            s.set_write_timeout(Some(Duration::from_secs(5))).ok();
            Some(s)
        }
        Err(e) => {
            println!("Cannot connect to {IPC_SOCKET}: {e}");
            println!("   Make sure server is running: npm start");
            println!("   Skipping test.\n");
            None
        }
    }
}

/// Send a JSON request and read the length-prefixed response.
///
/// This is the ONLY correct way to read from the IPC server.
/// DO NOT use `read_line()` or `BufReader` — the server uses binary framing.
pub fn ipc_request<T: Serialize>(stream: &mut UnixStream, request: &T) -> Result<IpcResult, String> {
    // Send newline-delimited JSON request
    let json = serde_json::to_string(request).map_err(|e| format!("Serialize: {e}"))?;
    stream.write_all(json.as_bytes()).map_err(|e| format!("Write: {e}"))?;
    stream.write_all(b"\n").map_err(|e| format!("Write newline: {e}"))?;
    stream.flush().map_err(|e| format!("Flush: {e}"))?;

    // Read 4-byte length prefix (u32 big-endian)
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).map_err(|e| format!("Read length prefix: {e}"))?;
    let length = u32::from_be_bytes(len_buf) as usize;

    if length == 0 {
        return Err("Empty response (length=0)".into());
    }

    // Read the full payload
    let mut payload = vec![0u8; length];
    stream.read_exact(&mut payload).map_err(|e| format!("Read payload ({length} bytes): {e}"))?;

    // Detect binary frame: JSON header + \0 separator + raw bytes
    if let Some(sep_idx) = payload.iter().position(|&b| b == 0) {
        let json_bytes = &payload[..sep_idx];
        let binary_data = payload[sep_idx + 1..].to_vec();
        let header: IpcResponse = serde_json::from_slice(json_bytes)
            .map_err(|e| format!("Parse binary header: {e}"))?;
        Ok(IpcResult::Binary { header, data: binary_data })
    } else {
        let response: IpcResponse = serde_json::from_slice(&payload)
            .map_err(|e| format!("Parse JSON response: {e}"))?;
        Ok(IpcResult::Json(response))
    }
}

/// Check if the IPC server is reachable (non-blocking probe).
#[allow(dead_code)]
pub fn server_is_running() -> bool {
    UnixStream::connect(IPC_SOCKET).is_ok()
}
