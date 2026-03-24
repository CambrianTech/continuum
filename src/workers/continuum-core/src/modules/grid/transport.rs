//! GridTransport — the polymorphic transport trait.
//!
//! This is the OpenCV-style interface: one trait, multiple implementations.
//! If the interface fits both Tailscale (managed mesh, IP-based, WireGuard)
//! and Reticulum (cryptographic identity, infrastructure-free, transport-agnostic),
//! it handles anything.
//!
//! Transport implementations:
//! - TailscaleTransport: TCP over Tailscale mesh (working NOW)
//! - ReticulumTransport: Reticulum encrypted mesh (future, validates interface)

use super::frame::GridFrame;
use super::node::{DiscoveredNode, NodeCapability, TransportAddress};
use async_trait::async_trait;
use std::fmt;

/// Errors from transport operations.
#[derive(Debug, Clone)]
pub enum TransportError {
    /// Connection failed (unreachable, refused, timeout).
    ConnectionFailed(String),
    /// Send/receive failed on an established connection.
    IoError(String),
    /// The address format is invalid for this transport.
    InvalidAddress(String),
    /// Transport is not initialized or has been shut down.
    NotReady(String),
    /// The remote node rejected our request (auth, ACL, etc.).
    Rejected(String),
    /// Operation timed out.
    Timeout(String),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConnectionFailed(msg) => write!(f, "connection failed: {msg}"),
            Self::IoError(msg) => write!(f, "I/O error: {msg}"),
            Self::InvalidAddress(msg) => write!(f, "invalid address: {msg}"),
            Self::NotReady(msg) => write!(f, "transport not ready: {msg}"),
            Self::Rejected(msg) => write!(f, "rejected: {msg}"),
            Self::Timeout(msg) => write!(f, "timeout: {msg}"),
        }
    }
}

impl std::error::Error for TransportError {}

impl From<TransportError> for String {
    fn from(e: TransportError) -> String {
        e.to_string()
    }
}

/// A connection to a remote Grid node.
///
/// Connections are bidirectional — you can send frames and receive responses.
/// The connection owns the underlying transport resource (TCP stream, Reticulum link, etc.).
#[async_trait]
pub trait GridConnection: Send + Sync {
    /// Send a frame over this connection.
    async fn send_frame(&self, frame: &GridFrame) -> Result<(), TransportError>;

    /// Receive the next frame from this connection.
    /// Blocks until a frame is available or the connection is closed.
    async fn recv_frame(&self) -> Result<GridFrame, TransportError>;

    /// Close this connection gracefully.
    async fn close(&self) -> Result<(), TransportError>;

    /// The remote node's transport address.
    fn remote_address(&self) -> &TransportAddress;

    /// Whether this connection is still alive.
    fn is_connected(&self) -> bool;
}

/// The polymorphic transport trait.
///
/// Each transport implementation handles:
/// - Connection establishment (connect to a known address)
/// - Listening for incoming connections (accept from remote nodes)
/// - Discovery (find other nodes on this transport)
/// - Announcement (advertise our presence and capabilities)
///
/// The GridModule owns one or more transports and routes through them.
#[async_trait]
pub trait GridTransport: Send + Sync {
    /// Human-readable transport name (e.g., "tailscale", "reticulum").
    fn name(&self) -> &'static str;

    /// Our local address on this transport.
    /// Returns None if the transport hasn't been initialized yet.
    fn local_address(&self) -> Option<TransportAddress>;

    /// Whether this transport provides its own encryption layer.
    /// Tailscale: true (WireGuard). Reticulum: true (X25519+AES-GCM).
    /// If false, the GridModule must add encryption on top.
    fn provides_encryption(&self) -> bool;

    /// Start the transport — bind listener, initialize identity, etc.
    /// Called once during GridModule::initialize().
    async fn start(&self) -> Result<(), TransportError>;

    /// Connect to a remote node at the given address.
    /// Returns a bidirectional connection for sending/receiving frames.
    async fn connect(
        &self,
        address: &TransportAddress,
    ) -> Result<Box<dyn GridConnection>, TransportError>;

    /// Accept an incoming connection from a remote node.
    /// Blocks until a connection arrives.
    /// Returns the connection and the remote node's address string.
    async fn accept(&self) -> Result<Box<dyn GridConnection>, TransportError>;

    /// Discover other Continuum nodes reachable via this transport.
    /// For Tailscale: query `tailscale status --json` for peers.
    /// For Reticulum: listen for announce packets on the mesh.
    async fn discover(&self) -> Result<Vec<DiscoveredNode>, TransportError>;

    /// Announce our presence and capabilities on this transport.
    /// For Tailscale: no-op (Tailscale handles presence via its coordinator).
    /// For Reticulum: broadcast an announce packet with capability app_data.
    async fn announce(
        &self,
        capabilities: &[NodeCapability],
    ) -> Result<(), TransportError>;

    /// Gracefully shut down this transport.
    /// Close listener, drop connections, clean up resources.
    async fn shutdown(&self) -> Result<(), TransportError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_error_display() {
        let err = TransportError::ConnectionFailed("node unreachable".into());
        assert_eq!(err.to_string(), "connection failed: node unreachable");

        let s: String = err.into();
        assert!(s.contains("node unreachable"));
    }
}
