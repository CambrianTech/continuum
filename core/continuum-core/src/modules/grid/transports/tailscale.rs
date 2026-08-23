//! TailscaleTransport — TCP over Tailscale WireGuard mesh.
//!
//! The working transport. Tailscale handles:
//! - WireGuard encryption (we don't need to encrypt)
//! - NAT traversal (connections just work)
//! - Key management (Tailscale coordinates)
//! - Discovery (via `tailscale status --json`)
//!
//! We just do TCP with length-prefixed JSON frames over Tailscale IPs.
//! Port 7117 is the default Grid service port.

use crate::modules::grid::frame::GridFrame;
use crate::modules::grid::node::{
    DiscoveredNode, NodeCapability, TransportAddress, DEFAULT_GRID_PORT,
};
use crate::modules::grid::transport::{GridConnection, GridTransport, TransportError};
use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

/// TCP connection to a remote Grid node over Tailscale.
pub struct TailscaleConnection {
    reader: Mutex<tokio::io::ReadHalf<TcpStream>>,
    writer: Mutex<tokio::io::WriteHalf<TcpStream>>,
    remote: TransportAddress,
    connected: std::sync::atomic::AtomicBool,
}

#[async_trait]
impl GridConnection for TailscaleConnection {
    async fn send_frame(&self, frame: &GridFrame) -> Result<(), TransportError> {
        let bytes = frame
            .to_wire_bytes()
            .map_err(|e| TransportError::IoError(e))?;

        let mut writer = self.writer.lock().await;
        writer
            .write_all(&bytes)
            .await
            .map_err(|e| TransportError::IoError(format!("TCP write failed: {e}")))?;
        writer
            .flush()
            .await
            .map_err(|e| TransportError::IoError(format!("TCP flush failed: {e}")))?;
        Ok(())
    }

    async fn recv_frame(&self) -> Result<GridFrame, TransportError> {
        let mut reader = self.reader.lock().await;

        // Read 4-byte length prefix
        let mut len_buf = [0u8; 4];
        reader
            .read_exact(&mut len_buf)
            .await
            .map_err(|e| TransportError::IoError(format!("TCP read length failed: {e}")))?;
        let len = u32::from_be_bytes(len_buf) as usize;

        // Sanity check frame size (max 64 MB)
        if len > 64 * 1024 * 1024 {
            return Err(TransportError::IoError(format!(
                "Frame too large: {len} bytes (max 64 MB)"
            )));
        }

        // Read JSON payload
        let mut payload = vec![0u8; len];
        reader
            .read_exact(&mut payload)
            .await
            .map_err(|e| TransportError::IoError(format!("TCP read payload failed: {e}")))?;

        GridFrame::from_json_bytes(&payload).map_err(|e| TransportError::IoError(e))
    }

    async fn close(&self) -> Result<(), TransportError> {
        self.connected
            .store(false, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    fn remote_address(&self) -> &TransportAddress {
        &self.remote
    }

    fn is_connected(&self) -> bool {
        self.connected.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl TailscaleConnection {
    fn from_stream(stream: TcpStream, remote: TransportAddress) -> Self {
        let (reader, writer) = tokio::io::split(stream);
        Self {
            reader: Mutex::new(reader),
            writer: Mutex::new(writer),
            remote,
            connected: std::sync::atomic::AtomicBool::new(true),
        }
    }
}

/// Tailscale mesh transport.
///
/// Uses TCP connections over Tailscale WireGuard mesh.
/// Discovery via `tailscale status --json` CLI.
pub struct TailscaleTransport {
    /// Port to listen on / connect to. `0` = OS-assigned: the REAL port is
    /// stored back here after bind, so `local_address()`/`bound_port()` always
    /// report the truth. This is what kills the probe-then-rebind TOCTOU race
    /// (a "free" port can be taken between probe and bind — CI, 2026-08-23:
    /// `Failed to bind 0.0.0.0:46243: Address already in use`).
    port: std::sync::atomic::AtomicU16,
    /// TCP listener (set after start()).
    listener: Mutex<Option<Arc<TcpListener>>>,
    /// Our Tailscale IP (discovered at start time).
    local_ip: Mutex<Option<String>>,
}

impl TailscaleTransport {
    pub fn new(port: u16) -> Self {
        Self {
            port: std::sync::atomic::AtomicU16::new(port),
            listener: Mutex::new(None),
            local_ip: Mutex::new(None),
        }
    }

    /// The port actually bound (differs from the constructor arg when it was 0).
    pub fn bound_port(&self) -> u16 {
        self.port.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn with_default_port() -> Self {
        Self::new(DEFAULT_GRID_PORT)
    }

    /// Start with a known IP, bypassing `tailscale status` query.
    /// Used for testing (localhost) and when the IP is already known.
    pub async fn start_with_ip(&self, ip: &str) -> Result<(), TransportError> {
        *self.local_ip.lock().await = Some(ip.to_string());

        let bind_addr = format!("0.0.0.0:{}", self.port.load(std::sync::atomic::Ordering::Relaxed));
        let listener = TcpListener::bind(&bind_addr).await.map_err(|e| {
            TransportError::ConnectionFailed(format!("Failed to bind {bind_addr}: {e}"))
        })?;
        if let Ok(addr) = listener.local_addr() {
            // Adopt the OS-assigned port so every later report speaks the truth.
            self.port.store(addr.port(), std::sync::atomic::Ordering::Relaxed);
        }

        *self.listener.lock().await = Some(Arc::new(listener));
        Ok(())
    }
}

#[async_trait]
impl GridTransport for TailscaleTransport {
    fn name(&self) -> &'static str {
        "tailscale"
    }

    fn local_address(&self) -> Option<TransportAddress> {
        // We can't block here, so check if we've cached it.
        // This is set during start().
        let ip = self.local_ip.try_lock().ok()?.clone()?;
        Some(TransportAddress::Tailscale {
            ip,
            port: self.port.load(std::sync::atomic::Ordering::Relaxed),
            machine_name: None,
        })
    }

    fn provides_encryption(&self) -> bool {
        true // WireGuard handles encryption
    }

    async fn start(&self) -> Result<(), TransportError> {
        // Discover our own Tailscale IP
        let status = query_tailscale_status().await?;
        let self_ip = status
            .tailscale_ips
            .first()
            .ok_or_else(|| {
                TransportError::NotReady("No Tailscale IP found — is Tailscale running?".into())
            })?
            .clone();

        *self.local_ip.lock().await = Some(self_ip.clone());

        // Bind TCP listener on all interfaces (Tailscale handles routing)
        let bind_addr = format!("0.0.0.0:{}", self.port.load(std::sync::atomic::Ordering::Relaxed));
        let listener = TcpListener::bind(&bind_addr).await.map_err(|e| {
            TransportError::ConnectionFailed(format!("Failed to bind {bind_addr}: {e}"))
        })?;
        if let Ok(addr) = listener.local_addr() {
            // Adopt the OS-assigned port so every later report speaks the truth.
            self.port.store(addr.port(), std::sync::atomic::Ordering::Relaxed);
        }

        *self.listener.lock().await = Some(Arc::new(listener));

        Ok(())
    }

    async fn connect(
        &self,
        address: &TransportAddress,
    ) -> Result<Box<dyn GridConnection>, TransportError> {
        let (ip, port) = match address {
            TransportAddress::Tailscale { ip, port, .. } => (ip.clone(), *port),
            other => {
                return Err(TransportError::InvalidAddress(format!(
                    "TailscaleTransport cannot connect to {}: wrong transport type",
                    other.display_address()
                )));
            }
        };

        let addr = format!("{ip}:{port}");
        let stream = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            TcpStream::connect(&addr),
        )
        .await
        .map_err(|_| TransportError::Timeout(format!("Connect to {addr} timed out (10s)")))?
        .map_err(|e| {
            TransportError::ConnectionFailed(format!("TCP connect to {addr} failed: {e}"))
        })?;

        // Disable Nagle's algorithm for low-latency frame exchange
        stream
            .set_nodelay(true)
            .map_err(|e| TransportError::IoError(format!("set_nodelay failed: {e}")))?;

        Ok(Box::new(TailscaleConnection::from_stream(
            stream,
            address.clone(),
        )))
    }

    async fn accept(&self) -> Result<Box<dyn GridConnection>, TransportError> {
        let listener = self.listener.lock().await;
        let listener = listener
            .as_ref()
            .ok_or_else(|| TransportError::NotReady("Listener not started".into()))?;

        let (stream, peer_addr) = listener
            .accept()
            .await
            .map_err(|e| TransportError::IoError(format!("TCP accept failed: {e}")))?;

        stream
            .set_nodelay(true)
            .map_err(|e| TransportError::IoError(format!("set_nodelay failed: {e}")))?;

        let remote = TransportAddress::Tailscale {
            ip: peer_addr.ip().to_string(),
            port: peer_addr.port(),
            machine_name: None,
        };

        Ok(Box::new(TailscaleConnection::from_stream(stream, remote)))
    }

    async fn discover(&self) -> Result<Vec<DiscoveredNode>, TransportError> {
        let status = query_tailscale_status().await?;

        let nodes: Vec<DiscoveredNode> = status
            .peers
            .into_iter()
            .filter(|p| p.online)
            .map(|peer| {
                let ip = peer.tailscale_ips.first().cloned().unwrap_or_default();
                DiscoveredNode {
                    address: TransportAddress::Tailscale {
                        ip,
                        port: self.port.load(std::sync::atomic::Ordering::Relaxed),
                        machine_name: Some(peer.host_name.clone()),
                    },
                    capabilities: vec![], // We don't know capabilities until we connect
                    name: Some(peer.host_name),
                }
            })
            .collect();

        Ok(nodes)
    }

    async fn announce(&self, _capabilities: &[NodeCapability]) -> Result<(), TransportError> {
        // Tailscale handles presence automatically via its coordinator.
        // No additional announcement needed.
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), TransportError> {
        // Drop the listener — this will stop accepting connections.
        *self.listener.lock().await = None;
        Ok(())
    }
}

// ============================================================================
// Tailscale CLI integration
// ============================================================================

/// Parsed output of `tailscale status --json`.
#[derive(Debug)]
struct TailscaleStatus {
    tailscale_ips: Vec<String>,
    peers: Vec<TailscalePeer>,
}

#[derive(Debug)]
struct TailscalePeer {
    host_name: String,
    tailscale_ips: Vec<String>,
    online: bool,
}

/// Raw JSON structures from `tailscale status --json`.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawTailscaleStatus {
    #[serde(default, rename = "TailscaleIPs")]
    tailscale_ips: Vec<String>,
    #[serde(default)]
    peer: std::collections::HashMap<String, RawTailscalePeer>,
    #[serde(default, rename = "Self")]
    self_node: Option<RawSelfNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawSelfNode {
    #[serde(default, rename = "TailscaleIPs")]
    tailscale_ips: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawTailscalePeer {
    host_name: String,
    #[serde(default, rename = "TailscaleIPs")]
    tailscale_ips: Vec<String>,
    online: bool,
}

/// Query Tailscale status — tries CLI first, falls back to cached file.
///
/// In Docker, the `tailscale` CLI isn't available. The host writes
/// `~/.continuum/grid/tailscale-status.json` (via setup.sh / continuum CLI),
/// which is mounted into the container. The Rust code reads it as fallback.
/// Tailscale IPs are reachable from Docker via the host's network stack.
async fn query_tailscale_status() -> Result<TailscaleStatus, TransportError> {
    // Try CLI first (works on host, fails in Docker)
    if let Ok(status) = query_tailscale_cli().await {
        return Ok(status);
    }

    // Fallback: read cached status file from host (Docker mount)
    query_tailscale_file().await
}

/// Query via `tailscale status --json` CLI.
async fn query_tailscale_cli() -> Result<TailscaleStatus, TransportError> {
    let output = tokio::process::Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        .await
        .map_err(|e| TransportError::NotReady(format!("tailscale CLI not available: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(TransportError::NotReady(format!(
            "Tailscale CLI failed: {stderr}"
        )));
    }

    parse_tailscale_json(&output.stdout)
}

/// Read cached `tailscale-status.json` written by host.
async fn query_tailscale_file() -> Result<TailscaleStatus, TransportError> {
    // Check standard locations
    let paths = [
        std::path::PathBuf::from("/root/.continuum/grid/tailscale-status.json"),
        dirs::home_dir()
            .unwrap_or_default()
            .join(".continuum/grid/tailscale-status.json"),
    ];

    for path in &paths {
        if let Ok(bytes) = tokio::fs::read(path).await {
            match parse_tailscale_json(&bytes) {
                Ok(status) => {
                    eprintln!(
                        "[grid/tailscale] Using cached status from {}",
                        path.display()
                    );
                    return Ok(status);
                }
                Err(e) => {
                    eprintln!("[grid/tailscale] Failed to parse {}: {e}", path.display());
                }
            }
        }
    }

    Err(TransportError::NotReady(
        "No tailscale CLI and no cached tailscale-status.json from host".into(),
    ))
}

/// Parse raw Tailscale status JSON bytes into our TailscaleStatus.
fn parse_tailscale_json(bytes: &[u8]) -> Result<TailscaleStatus, TransportError> {
    let raw: RawTailscaleStatus = serde_json::from_slice(bytes).map_err(|e| {
        TransportError::IoError(format!("Failed to parse tailscale status JSON: {e}"))
    })?;

    // Self IPs may be at top level or in the Self node
    let self_ips = if !raw.tailscale_ips.is_empty() {
        raw.tailscale_ips
    } else if let Some(self_node) = raw.self_node {
        self_node.tailscale_ips
    } else {
        vec![]
    };

    let peers: Vec<TailscalePeer> = raw
        .peer
        .into_values()
        .map(|p| TailscalePeer {
            host_name: p.host_name,
            tailscale_ips: p.tailscale_ips,
            online: p.online,
        })
        .collect();

    Ok(TailscaleStatus {
        tailscale_ips: self_ips,
        peers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tailscale_status() {
        let json = r#"{
            "Self": {
                "TailscaleIPs": ["100.64.0.1"],
                "HostName": "my-laptop"
            },
            "Peer": {
                "key:abc123": {
                    "HostName": "bigmama",
                    "TailscaleIPs": ["100.124.122.107"],
                    "Online": true
                },
                "key:def456": {
                    "HostName": "work-mac",
                    "TailscaleIPs": ["100.64.0.5"],
                    "Online": false
                }
            }
        }"#;

        let raw: RawTailscaleStatus = serde_json::from_str(json).unwrap();

        let self_ips = if let Some(self_node) = raw.self_node {
            self_node.tailscale_ips
        } else {
            vec![]
        };
        assert_eq!(self_ips, vec!["100.64.0.1"]);
        assert_eq!(raw.peer.len(), 2);

        let bigmama = raw
            .peer
            .values()
            .find(|p| p.host_name == "bigmama")
            .unwrap();
        assert!(bigmama.online);
        assert_eq!(bigmama.tailscale_ips[0], "100.124.122.107");
    }
}
