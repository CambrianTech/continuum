//! Grid node identity, capability advertisement, and transport addressing.
//!
//! A GridNode represents a Continuum instance on the mesh.
//! Each node has a unique identity, a set of capabilities it advertises,
//! and one or more transport addresses through which it can be reached.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::forge::endpoint::ForgeEndpoint;
use crate::identity::PeerId;

/// Trust level for a remote node.
/// Determines what commands the node is allowed to execute on us,
/// and what commands we're willing to send to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/grid/TrustLevel.ts")]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    /// Unknown node — not in our known_nodes list.
    /// No remote commands allowed.
    Blocked = 0,
    /// First contact — limited to read-only info commands.
    Provisional = 1,
    /// Established history of successful interactions.
    /// Can run compute commands (inference, training).
    Trusted = 2,
    /// Our own nodes (e.g., laptop + home tower).
    /// Full access to all non-local-only commands.
    Owner = 3,
}

impl Default for TrustLevel {
    fn default() -> Self {
        Self::Blocked
    }
}

/// Transport-specific address for reaching a node.
/// Each variant corresponds to a GridTransport implementation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/grid/TransportAddress.ts"
)]
#[serde(tag = "transport")]
pub enum TransportAddress {
    /// TCP over Tailscale mesh (WireGuard-encrypted).
    #[serde(rename = "tailscale")]
    Tailscale {
        /// Tailscale IP (e.g., "100.124.122.107")
        ip: String,
        /// Grid service port (default: 7117)
        port: u16,
        /// Tailscale machine name (e.g., "bigmama")
        #[serde(skip_serializing_if = "Option::is_none")]
        machine_name: Option<String>,
    },

    /// Reticulum encrypted mesh (identity-based addressing).
    #[serde(rename = "reticulum")]
    Reticulum {
        /// Reticulum destination hash (hex-encoded Ed25519 public key derivative).
        destination_hash: String,
    },
}

impl TransportAddress {
    /// Which transport type this address belongs to.
    pub fn transport_name(&self) -> &'static str {
        match self {
            Self::Tailscale { .. } => "tailscale",
            Self::Reticulum { .. } => "reticulum",
        }
    }

    /// Human-readable display string.
    pub fn display_address(&self) -> String {
        match self {
            Self::Tailscale {
                ip,
                port,
                machine_name,
            } => {
                if let Some(name) = machine_name {
                    format!("{name} ({ip}:{port})")
                } else {
                    format!("{ip}:{port}")
                }
            }
            Self::Reticulum { destination_hash } => {
                // Show first 8 chars of hash for brevity. UTF-8 safe even
                // though destination_hash is in practice ASCII-hex — the
                // safe primitive removes the latent panic by construction
                // per [[every-error-is-an-opportunity-to-battle-harden]].
                let short =
                    crate::utils::str_truncate::truncate_at_char_boundary(destination_hash, 8);
                format!("ret:{short}...")
            }
        }
    }

    /// Create a Tailscale address with default grid port.
    pub fn tailscale(ip: impl Into<String>, machine_name: Option<String>) -> Self {
        Self::Tailscale {
            ip: ip.into(),
            port: DEFAULT_GRID_PORT,
            machine_name,
        }
    }
}

/// Default port for Grid TCP connections (over Tailscale or direct).
pub const DEFAULT_GRID_PORT: u16 = 7117;

/// A capability that a node advertises to the mesh.
/// Used by the GridRouter to decide where to send commands.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/grid/NodeCapability.ts"
)]
#[serde(tag = "type")]
pub enum NodeCapability {
    /// GPU compute available.
    #[serde(rename = "compute")]
    Compute {
        /// GPU model name (e.g., "RTX 5090", "M3 Pro")
        #[serde(skip_serializing_if = "Option::is_none")]
        gpu: Option<String>,
        /// Available VRAM in megabytes
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(type = "number")]
        vram_mb: Option<u64>,
    },

    /// Storage available for datasets, models, etc.
    #[serde(rename = "storage")]
    Storage {
        #[ts(type = "number")]
        available_mb: u64,
    },

    /// AI inference capability — which models can run here.
    #[serde(rename = "inference")]
    Inference { models: Vec<String> },

    /// Training capability — LoRA fine-tuning.
    #[serde(rename = "training")]
    Training {
        /// Maximum LoRA rank supported
        #[ts(type = "number")]
        max_rank: u32,
        /// Maximum training epochs
        #[ts(type = "number")]
        max_epochs: u32,
    },

    /// Forge custodian capability — turns a trained checkpoint into a GGUF gene
    /// (FORGE-CUSTODIAN-CONTRACT.md §5, Pass 5b). The endpoint row is DISCOVERED by
    /// probing the local custodian's `/health`, never declared by config; a node
    /// only advertises this when a custodian actually answered (see
    /// [`ForgeEndpoint::probe_local`](crate::forge::endpoint::ForgeEndpoint::probe_local)).
    #[serde(rename = "forge")]
    Forge {
        /// The routable forge endpoint (locator, capabilities, health, capacity,
        /// trust). The fabric re-probes health for the live reading; this is the
        /// discovery snapshot, exactly as `Compute` carries a VRAM snapshot.
        endpoint: ForgeEndpoint,
    },
}

/// A known node on the Grid mesh.
/// Stored in the node registry with all known addresses and capabilities.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/grid/GridNode.ts")]
pub struct GridNode {
    /// Unique node identifier — derived from the first transport identity.
    /// For Tailscale: the Tailscale IP. For Reticulum: the destination hash.
    pub node_id: String,

    /// Human-readable name (user-assigned, e.g., "home-5090", "school-laptop").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_name: Option<String>,

    /// All transport addresses through which this node can be reached.
    /// A node might be reachable via both Tailscale and Reticulum.
    pub addresses: Vec<TransportAddress>,

    /// Capabilities this node has advertised.
    pub capabilities: Vec<NodeCapability>,

    /// Trust level we've assigned to this node.
    pub trust_level: TrustLevel,

    /// Last time we heard from this node (ms since epoch).
    #[ts(type = "number")]
    pub last_seen: u64,

    /// Last measured round-trip latency in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | undefined")]
    pub latency_ms: Option<u64>,

    /// The node's DURABLE airc identity — its `PeerId`, the SAME key the capacity
    /// gossip (`CapacityOffer`) and the settlement `Reputation` use. `node_id` above is
    /// a TRANSPORT-derived address (a Tailscale IP) that changes with location; THIS is
    /// the being's identity that moves with it. Optional because a node found by a
    /// transport-level scan alone has no `PeerId` until the pairing/gossip correlation
    /// supplies it (`set_peer_id`). Once set it is the ONE key that joins routing ↔
    /// capacity ↔ reputation — #2228, the node sibling of the enforced
    /// `persona_id == peer_id` (`airc_runtime.rs:390`). See GRID-ELASTIC-CAPABILITY §3d.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "string | undefined")]
    pub peer_id: Option<PeerId>,
}

/// A node discovered during transport-level discovery (before trust assignment).
#[derive(Debug, Clone)]
pub struct DiscoveredNode {
    /// Transport address where this node was found.
    pub address: TransportAddress,
    /// Capabilities announced during discovery (may be empty).
    pub capabilities: Vec<NodeCapability>,
    /// Machine/node name if provided during discovery.
    pub name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_address_serde() {
        let addr = TransportAddress::Tailscale {
            ip: "100.124.122.107".into(),
            port: 7117,
            machine_name: Some("bigmama".into()),
        };

        let json = serde_json::to_value(&addr).unwrap();
        assert_eq!(json["transport"], "tailscale");
        assert_eq!(json["ip"], "100.124.122.107");
        assert_eq!(json["port"], 7117);
        assert_eq!(json["machine_name"], "bigmama");

        let roundtrip: TransportAddress = serde_json::from_value(json).unwrap();
        assert_eq!(roundtrip, addr);
    }

    #[test]
    fn test_capability_serde() {
        let cap = NodeCapability::Compute {
            gpu: Some("RTX 5090".into()),
            vram_mb: Some(32768),
        };

        let json = serde_json::to_value(&cap).unwrap();
        assert_eq!(json["type"], "compute");
        assert_eq!(json["gpu"], "RTX 5090");
        assert_eq!(json["vram_mb"], 32768);
    }

    #[test]
    fn test_trust_level_ordering() {
        assert!(TrustLevel::Owner > TrustLevel::Trusted);
        assert!(TrustLevel::Trusted > TrustLevel::Provisional);
        assert!(TrustLevel::Provisional > TrustLevel::Blocked);
    }

    // what this catches: the Forge capability variant survives the grid-bus round
    // trip (it IS announced over `GridTransport::announce`, Pass 5b). The endpoint
    // nests under the internal "type":"forge" tag; a serde shape that failed to
    // (de)serialize the nested ForgeEndpoint would silently drop forge discovery on
    // the hop — peers would never learn the node can forge.
    #[test]
    fn test_forge_capability_serde() {
        use crate::forge::endpoint::{ForgeEndpoint, ForgeHealth, ForgeLocator};

        let cap = NodeCapability::Forge {
            endpoint: ForgeEndpoint {
                locator: ForgeLocator::Local {
                    base_url: "http://127.0.0.1:8899".into(),
                },
                capabilities: vec!["gguf-lora".into()],
                contract_version: 1,
                health: ForgeHealth::Healthy,
                capacity: 2,
                trust_scope: TrustLevel::Owner,
            },
        };

        let json = serde_json::to_value(&cap).unwrap();
        assert_eq!(json["type"], "forge");
        assert_eq!(json["endpoint"]["health"], "healthy");
        assert_eq!(json["endpoint"]["capacity"], 2);

        let back: NodeCapability = serde_json::from_value(json).unwrap();
        match back {
            NodeCapability::Forge { endpoint } => {
                assert_eq!(endpoint.capacity, 2);
                assert!(endpoint.supports("gguf-lora"));
            }
            other => panic!("expected Forge, got {other:?}"),
        }
    }
}
