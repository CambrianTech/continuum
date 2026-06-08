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

/// Resolve the IPC socket path.
/// Checks the project socket dir first (npm start), falls back to /tmp (manual cargo run).
pub fn ipc_socket_path() -> String {
    // Walk up from the cargo test working directory to find the project root
    let project_socket = ".continuum/sockets/continuum-core.sock";
    let mut dir = std::env::current_dir().unwrap_or_default();
    for _ in 0..5 {
        let candidate = dir.join(project_socket);
        if candidate.exists() {
            return candidate.to_string_lossy().to_string();
        }
        if !dir.pop() {
            break;
        }
    }
    // Fallback for manual `cargo run` outside project
    "/tmp/continuum-core.sock".to_string()
}

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
    let socket_path = ipc_socket_path();
    match UnixStream::connect(&socket_path) {
        Ok(s) => {
            s.set_read_timeout(Some(read_timeout)).ok();
            s.set_write_timeout(Some(Duration::from_secs(5))).ok();
            Some(s)
        }
        Err(e) => {
            println!("Cannot connect to {socket_path}: {e}");
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
pub fn ipc_request<T: Serialize>(
    stream: &mut UnixStream,
    request: &T,
) -> Result<IpcResult, String> {
    // Send newline-delimited JSON request
    let json = serde_json::to_string(request).map_err(|e| format!("Serialize: {e}"))?;
    stream
        .write_all(json.as_bytes())
        .map_err(|e| format!("Write: {e}"))?;
    stream
        .write_all(b"\n")
        .map_err(|e| format!("Write newline: {e}"))?;
    stream.flush().map_err(|e| format!("Flush: {e}"))?;

    // Read 4-byte length prefix (u32 big-endian)
    let mut len_buf = [0u8; 4];
    stream
        .read_exact(&mut len_buf)
        .map_err(|e| format!("Read length prefix: {e}"))?;
    let length = u32::from_be_bytes(len_buf) as usize;

    if length == 0 {
        return Err("Empty response (length=0)".into());
    }

    // Read the full payload
    let mut payload = vec![0u8; length];
    stream
        .read_exact(&mut payload)
        .map_err(|e| format!("Read payload ({length} bytes): {e}"))?;

    // Detect binary frame: JSON header + \0 separator + raw bytes
    if let Some(sep_idx) = payload.iter().position(|&b| b == 0) {
        let json_bytes = &payload[..sep_idx];
        let binary_data = payload[sep_idx + 1..].to_vec();
        let header: IpcResponse =
            serde_json::from_slice(json_bytes).map_err(|e| format!("Parse binary header: {e}"))?;
        Ok(IpcResult::Binary {
            header,
            data: binary_data,
        })
    } else {
        let response: IpcResponse =
            serde_json::from_slice(&payload).map_err(|e| format!("Parse JSON response: {e}"))?;
        Ok(IpcResult::Json(response))
    }
}

/// Check if the IPC server is reachable (non-blocking probe).
#[allow(dead_code)]
pub fn server_is_running() -> bool {
    UnixStream::connect(ipc_socket_path()).is_ok()
}

// ============================================================================
// Docker Model Runner (DMR) bundle resolution + auto-pull
// ============================================================================
//
// Tests that need a specific model on disk MUST resolve through this helper
// instead of hardcoding paths or SHA hashes. Hardcoded paths assume one
// developer's HOME and break for everyone else; hardcoded SHAs go stale the
// next time the model is reforged.
//
// Resolution flow:
//   1. If `$TEST_MODEL_PATH_<NAME>` is set and points to a real file, use it.
//   2. Otherwise, ask `docker model ls` for the matching MODEL ID and resolve
//      to ~/.docker/models/bundles/sha256/<full-hash>/model/model.gguf.
//   3. If the model isn't installed yet, `docker model pull <name>` it now
//      (one-time cost, cached forever after) — so tests that need it just
//      work on a fresh checkout, no separate manual step.
//   4. Return None only if Docker/DMR isn't available at all (test should
//      then skip with a clear error message naming the install).

#[allow(dead_code)]
pub fn dmr_model_gguf(model_name: &str) -> Option<std::path::PathBuf> {
    let env_override_var = format!(
        "TEST_MODEL_PATH_{}",
        model_name.to_uppercase().replace(['/', '.', '-', ':'], "_")
    );
    if let Ok(p) = std::env::var(&env_override_var) {
        let pb = std::path::PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
    }

    // First lookup pass — does DMR already have it?
    if let Some(p) = lookup_dmr_bundle(model_name) {
        return Some(p);
    }

    // Auto-pull. This is the "no one has to remember" path. The pull is
    // idempotent (DMR no-ops if the bundle is already content-addressed
    // present), and cached forever. We surface stderr so a real failure
    // (no internet, model 404'd) is diagnosable.
    eprintln!(
        "→ {model_name} not found in DMR; auto-pulling via `docker model pull` (cached after this run)"
    );
    let pull = std::process::Command::new("docker")
        .args(["model", "pull", model_name])
        .status()
        .ok()?;
    if !pull.success() {
        eprintln!(
            "✗ `docker model pull {model_name}` failed (exit {pull:?}). \
             Verify Docker Desktop is running and Model Runner is enabled."
        );
        return None;
    }

    // Re-lookup after pull
    lookup_dmr_bundle(model_name)
}

fn lookup_dmr_bundle(model_name: &str) -> Option<std::path::PathBuf> {
    let output = std::process::Command::new("docker")
        .args(["model", "ls", "--format", "{{.Name}}\t{{.ID}}"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let id_prefix = stdout.lines().find_map(|line| {
        let mut parts = line.splitn(2, '\t');
        let name = parts.next()?.trim();
        let id = parts.next()?.trim();
        if name.eq_ignore_ascii_case(model_name) {
            Some(id.to_string())
        } else {
            None
        }
    })?;

    let home = std::env::var("HOME").ok()?;
    let bundles = std::path::PathBuf::from(home).join(".docker/models/bundles/sha256");
    for entry in std::fs::read_dir(&bundles).ok()?.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if name.starts_with(&id_prefix) {
                let gguf = entry.path().join("model").join("model.gguf");
                if gguf.exists() {
                    return Some(gguf);
                }
            }
        }
    }
    None
}

/// Convenience for tests that need the qwen3.5-4b-code-forged GGUF. Resolves
/// (and auto-pulls if missing) via DMR. Returns None only when Docker/DMR
/// itself is unreachable, in which case the test should skip with a clear
/// install hint.
#[allow(dead_code)]
pub fn qwen35_4b_code_gguf() -> Option<std::path::PathBuf> {
    if let Ok(path) = std::env::var("QWEN35_4B_GGUF") {
        let path = std::path::PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }

    for name in [
        "huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf",
        "hf.co/continuum-ai/qwen3.5-4b-code-forged-gguf",
        "continuum-ai/qwen3.5-4b-code-forged-gguf",
    ] {
        if let Some(p) = dmr_model_gguf(name) {
            return Some(p);
        }
    }
    None
}
