//! UDP Event Transport — fire-and-forget event streaming across the grid.
//!
//! Sits alongside the TCP transport (which handles reliable commands).
//! UDP is for high-throughput, low-latency events where dropping frames
//! is acceptable: sensor data, video frames, audio, telemetry, heartbeats.
//!
//! Design:
//!   - One UDP socket per node, bound to grid UDP port (7118)
//!   - Events are GridFrame with FrameType::Event, serialized as JSON
//!   - No acknowledgment, no retransmission, no ordering guarantees
//!   - MTU-aware: frames > 1400 bytes are silently dropped (use TCP for large data)
//!   - Received events are injected into the local Events.emit() bus
//!
//! Usage:
//!   - Node A: Events.emit('sensor:motion', data) → UDP to subscribed nodes
//!   - Node B: Events.subscribe('sensor:motion') → receives via UDP injection
//!   - Transparent: application code doesn't know events crossed the network

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use std::collections::HashMap;

use crate::runtime;
use super::super::frame::{GridFrame, FrameType, GridPayload};

/// Default UDP port for grid event streaming.
pub const DEFAULT_UDP_PORT: u16 = 7118;

/// Maximum UDP payload size (conservative for all networks).
const MAX_UDP_PAYLOAD: usize = 1400;

/// A subscription: which event patterns this remote node wants.
#[derive(Debug, Clone)]
struct RemoteSubscription {
    /// The remote node's UDP address
    addr: SocketAddr,
    /// Event name patterns (e.g., "sensor:*", "voice:*", "grid:heartbeat")
    patterns: Vec<String>,
    /// Last heartbeat received (ms since epoch)
    last_seen: u64,
}

/// UDP event transport for the grid.
pub struct UdpEventTransport {
    /// Our bound UDP socket
    socket: Option<Arc<UdpSocket>>,
    /// Our local identity (for source_node in frames)
    local_node_id: String,
    /// Remote nodes subscribed to our events
    subscribers: Arc<RwLock<HashMap<String, RemoteSubscription>>>,
    /// Port to bind on
    port: u16,
}

impl UdpEventTransport {
    pub fn new(local_node_id: String, port: u16) -> Self {
        Self {
            socket: None,
            local_node_id,
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            port,
        }
    }

    /// Start the UDP socket and begin receiving events.
    pub async fn start(&mut self) -> Result<(), String> {
        let log = runtime::logger("grid-udp");
        let bind_addr = format!("0.0.0.0:{}", self.port);

        let socket = UdpSocket::bind(&bind_addr)
            .await
            .map_err(|e| format!("UDP bind failed on {}: {}", bind_addr, e))?;

        log.info(&format!("UDP event transport listening on {}", bind_addr));
        self.socket = Some(Arc::new(socket));
        Ok(())
    }

    /// Send an event to all subscribed remote nodes.
    /// Fire-and-forget: errors are logged, not propagated.
    pub async fn broadcast_event(&self, event_name: &str, data: &serde_json::Value) {
        let socket = match &self.socket {
            Some(s) => s.clone(),
            None => return, // Not started
        };

        let frame = make_event_frame(&self.local_node_id, "broadcast", event_name, data);

        let payload = match serde_json::to_vec(&frame) {
            Ok(p) => p,
            Err(_) => return,
        };

        // Drop oversized frames (use TCP for large data)
        if payload.len() > MAX_UDP_PAYLOAD {
            return;
        }

        let subscribers = self.subscribers.read().await;
        for sub in subscribers.values() {
            if matches_pattern(&sub.patterns, event_name) {
                let _ = socket.send_to(&payload, sub.addr).await;
            }
        }
    }

    /// Send an event to a specific node.
    pub async fn send_event_to(
        &self,
        target_addr: SocketAddr,
        event_name: &str,
        data: &serde_json::Value,
    ) -> Result<(), String> {
        let socket = self.socket.as_ref().ok_or("UDP not started")?;

        let frame = make_event_frame(&self.local_node_id, &target_addr.to_string(), event_name, data);

        let payload = serde_json::to_vec(&frame)
            .map_err(|e| format!("serialize: {e}"))?;

        if payload.len() > MAX_UDP_PAYLOAD {
            return Err(format!("Frame too large for UDP: {} > {}", payload.len(), MAX_UDP_PAYLOAD));
        }

        socket.send_to(&payload, target_addr)
            .await
            .map_err(|e| format!("UDP send: {e}"))?;

        Ok(())
    }

    /// Register a remote node's event subscription.
    pub async fn add_subscriber(
        &self,
        node_id: String,
        addr: SocketAddr,
        patterns: Vec<String>,
    ) {
        let mut subs = self.subscribers.write().await;
        subs.insert(node_id, RemoteSubscription {
            addr,
            patterns,
            last_seen: now_millis(),
        });
    }

    /// Remove a subscriber.
    pub async fn remove_subscriber(&self, node_id: &str) {
        let mut subs = self.subscribers.write().await;
        subs.remove(node_id);
    }

    /// Start the receive loop — runs until shutdown.
    /// Received events are passed to the callback for injection into Events.emit().
    pub async fn recv_loop<F>(&self, on_event: F)
    where
        F: Fn(String, serde_json::Value, String) + Send + Sync + 'static,
    {
        let socket = match &self.socket {
            Some(s) => s.clone(),
            None => return,
        };
        let log = runtime::logger("grid-udp");

        let mut buf = vec![0u8; MAX_UDP_PAYLOAD + 100];
        loop {
            match socket.recv_from(&mut buf).await {
                Ok((len, src)) => {
                    if let Ok(frame) = serde_json::from_slice::<GridFrame>(&buf[..len]) {
                        match frame.payload {
                            GridPayload::Event { event, data } => {
                                on_event(event, data, frame.source_node);
                            }
                            // Subscription request: remote node wants our events
                            GridPayload::Command { ref command, ref params } if command == "grid/subscribe-events" => {
                                if let Some(patterns) = params.get("patterns").and_then(|p| {
                                    serde_json::from_value::<Vec<String>>(p.clone()).ok()
                                }) {
                                    let mut subs = self.subscribers.write().await;
                                    subs.insert(frame.source_node.clone(), RemoteSubscription {
                                        addr: src,
                                        patterns,
                                        last_seen: now_millis(),
                                    });
                                    log.debug(&format!("UDP: {} subscribed from {}", frame.source_node, src));
                                }
                            }
                            _ => {
                                // Non-event frames on UDP are unexpected but not fatal
                            }
                        }
                    }
                }
                Err(e) => {
                    // UDP recv errors are transient — log and continue
                    log.warn(&format!("UDP recv error: {}", e));
                }
            }
        }
    }

    /// Get the bound socket address.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.socket.as_ref().and_then(|s| s.local_addr().ok())
    }

    /// Shutdown the transport.
    pub fn shutdown(&mut self) {
        self.socket = None;
    }
}

/// Check if an event name matches any pattern.
/// Patterns support '*' wildcard at end: "sensor:*" matches "sensor:motion", "sensor:temp".
fn matches_pattern(patterns: &[String], event_name: &str) -> bool {
    for pattern in patterns {
        if pattern == "*" {
            return true;
        }
        if pattern.ends_with('*') {
            let prefix = &pattern[..pattern.len() - 1];
            if event_name.starts_with(prefix) {
                return true;
            }
        }
        if pattern == event_name {
            return true;
        }
    }
    false
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Create an event frame for UDP (uses existing GridFrame::event with empty correlation).
fn make_event_frame(source: &str, target: &str, event: &str, data: &serde_json::Value) -> GridFrame {
    GridFrame::event(
        String::new(), // No correlation for fire-and-forget events
        source.to_string(),
        target.to_string(),
        event.to_string(),
        data.clone(),
    )
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_matching() {
        assert!(matches_pattern(&["sensor:*".into()], "sensor:motion"));
        assert!(matches_pattern(&["sensor:*".into()], "sensor:temperature"));
        assert!(!matches_pattern(&["sensor:*".into()], "voice:transcription"));
        assert!(matches_pattern(&["*".into()], "anything"));
        assert!(matches_pattern(&["exact:match".into()], "exact:match"));
        assert!(!matches_pattern(&["exact:match".into()], "exact:other"));
        assert!(!matches_pattern(&[], "anything"));
    }

    #[tokio::test]
    async fn test_udp_transport_lifecycle() {
        let mut transport = UdpEventTransport::new("test-node".into(), 0); // 0 = OS picks port
        transport.start().await.unwrap();
        assert!(transport.local_addr().is_some());
        transport.shutdown();
        assert!(transport.socket.is_none());
    }

    #[tokio::test]
    async fn test_udp_send_receive() {
        // Sender
        let mut sender = UdpEventTransport::new("sender".into(), 0);
        sender.start().await.unwrap();

        // Receiver
        let receiver_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let receiver_addr = receiver_socket.local_addr().unwrap();

        // Send event
        sender.send_event_to(
            receiver_addr,
            "test:event",
            &serde_json::json!({"value": 42}),
        ).await.unwrap();

        // Receive
        let mut buf = vec![0u8; 2000];
        let (len, _src) = receiver_socket.recv_from(&mut buf).await.unwrap();
        let frame: GridFrame = serde_json::from_slice(&buf[..len]).unwrap();

        match frame.payload {
            GridPayload::Event { event, data } => {
                assert_eq!(event, "test:event");
                assert_eq!(data["value"], 42);
            }
            _ => panic!("Expected event payload"),
        }
    }
}
