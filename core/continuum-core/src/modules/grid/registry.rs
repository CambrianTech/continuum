//! Grid node registry — tracks known nodes, their capabilities, and trust levels.
//!
//! Persisted to .continuum/grid/known_nodes.json.
//! Updated by discovery, manual pairing, and periodic health checks.

use super::node::{DiscoveredNode, GridNode, TransportAddress, TrustLevel};
use crate::identity::PeerId;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Persistent registry of known Grid nodes.
pub struct NodeRegistry {
    /// Nodes indexed by node_id.
    nodes: DashMap<String, GridNode>,
    /// Path to the persisted registry file.
    persist_path: PathBuf,
}

/// Serialization wrapper for the registry file.
#[derive(Serialize, Deserialize)]
struct PersistedRegistry {
    nodes: Vec<GridNode>,
}

impl NodeRegistry {
    /// Create a new registry, loading from disk if available.
    pub fn new(grid_dir: &Path) -> Self {
        let persist_path = grid_dir.join("known_nodes.json");
        let registry = Self {
            nodes: DashMap::new(),
            persist_path,
        };
        registry.load_from_disk();
        registry
    }

    /// Get a node by ID.
    pub fn get(&self, node_id: &str) -> Option<GridNode> {
        self.nodes.get(node_id).map(|r| r.value().clone())
    }

    /// Get all known nodes.
    pub fn all_nodes(&self) -> Vec<GridNode> {
        self.nodes.iter().map(|r| r.value().clone()).collect()
    }

    /// Get online nodes (seen within the last 5 minutes).
    pub fn online_nodes(&self) -> Vec<GridNode> {
        let cutoff = now_millis().saturating_sub(5 * 60 * 1000);
        self.nodes
            .iter()
            .filter(|r| r.value().last_seen >= cutoff)
            .map(|r| r.value().clone())
            .collect()
    }

    /// Add or update a node from discovery.
    /// If the node already exists, merges addresses and updates last_seen.
    /// Does NOT change trust level (that's a manual operation).
    pub fn upsert_discovered(&self, discovered: DiscoveredNode) {
        let node_id = address_to_node_id(&discovered.address);

        self.nodes
            .entry(node_id.clone())
            .and_modify(|existing| {
                // Merge address if not already present
                if !existing.addresses.contains(&discovered.address) {
                    existing.addresses.push(discovered.address.clone());
                }
                // Update capabilities if provided
                if !discovered.capabilities.is_empty() {
                    existing.capabilities = discovered.capabilities.clone();
                }
                // Update name if provided
                if discovered.name.is_some() {
                    existing.node_name = discovered.name.clone();
                }
                existing.last_seen = now_millis();
            })
            .or_insert_with(|| GridNode {
                node_id,
                node_name: discovered.name,
                addresses: vec![discovered.address],
                capabilities: discovered.capabilities,
                trust_level: TrustLevel::default(), // Blocked until manually trusted
                last_seen: now_millis(),
                latency_ms: None,
                peer_id: None, // learned later via set_peer_id (pairing/gossip correlation, #2228)
            });
    }

    /// Register a node manually (e.g., from pairing or config).
    pub fn register_node(&self, node: GridNode) {
        self.nodes.insert(node.node_id.clone(), node);
    }

    /// Get a node by its DURABLE airc `PeerId` — the #2228 join key. This is the lookup
    /// the router needs to price a node against the `PeerId`-keyed capacity gossip and
    /// settlement reputation, instead of against its transport-derived `node_id`. Returns
    /// the first node carrying this `PeerId` (a node has at most one durable identity).
    pub fn get_by_peer(&self, peer_id: &PeerId) -> Option<GridNode> {
        self.nodes
            .iter()
            .find(|r| r.value().peer_id.as_ref() == Some(peer_id))
            .map(|r| r.value().clone())
    }

    /// Correlate a known node (keyed by its transport-derived `node_id`) with its durable
    /// airc `PeerId` once discovery/gossip/pairing learns it. The airc pairing already
    /// learns the transport route AND the `PeerId` together; this is where the grid
    /// CONSUMES that correlation instead of running a parallel transport-only scan — the
    /// one move that closes the three-key split (GRID-ELASTIC-CAPABILITY §3d, #2228).
    pub fn set_peer_id(&self, node_id: &str, peer_id: PeerId) -> Result<(), String> {
        self.nodes
            .get_mut(node_id)
            .map(|mut r| {
                r.value_mut().peer_id = Some(peer_id);
                r.value_mut().last_seen = now_millis();
            })
            .ok_or_else(|| format!("Unknown node: {node_id}"))
    }

    /// Update a node's trust level.
    pub fn set_trust(&self, node_id: &str, trust: TrustLevel) -> Result<(), String> {
        self.nodes
            .get_mut(node_id)
            .map(|mut r| {
                r.value_mut().trust_level = trust;
            })
            .ok_or_else(|| format!("Unknown node: {node_id}"))
    }

    /// Update a node's latency measurement.
    pub fn update_latency(&self, node_id: &str, latency_ms: u64) {
        if let Some(mut r) = self.nodes.get_mut(node_id) {
            r.value_mut().latency_ms = Some(latency_ms);
            r.value_mut().last_seen = now_millis();
        }
    }

    /// Find nodes that have a specific capability.
    pub fn nodes_with_capability(&self, cap_type: &str) -> Vec<GridNode> {
        self.nodes
            .iter()
            .filter(|r| {
                r.value().capabilities.iter().any(|c| {
                    let name = match c {
                        super::node::NodeCapability::Compute { .. } => "compute",
                        super::node::NodeCapability::Storage { .. } => "storage",
                        super::node::NodeCapability::Inference { .. } => "inference",
                        super::node::NodeCapability::Training { .. } => "training",
                        super::node::NodeCapability::Forge { .. } => "forge",
                    };
                    name == cap_type
                })
            })
            .map(|r| r.value().clone())
            .collect()
    }

    /// Find the best node for a given address (for routing).
    /// Prefers: lower latency, higher trust, more recently seen.
    pub fn find_by_address(&self, address: &TransportAddress) -> Option<GridNode> {
        self.nodes
            .iter()
            .find(|r| r.value().addresses.contains(address))
            .map(|r| r.value().clone())
    }

    /// Persist the registry to disk.
    pub fn save_to_disk(&self) -> Result<(), String> {
        let nodes = self.all_nodes();
        let registry = PersistedRegistry { nodes };
        let json = serde_json::to_string_pretty(&registry)
            .map_err(|e| format!("Registry serialization failed: {e}"))?;

        if let Some(parent) = self.persist_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create grid dir: {e}"))?;
        }
        std::fs::write(&self.persist_path, json)
            .map_err(|e| format!("Failed to write registry: {e}"))?;
        Ok(())
    }

    /// Load the registry from disk.
    fn load_from_disk(&self) {
        if let Ok(json) = std::fs::read_to_string(&self.persist_path) {
            if let Ok(registry) = serde_json::from_str::<PersistedRegistry>(&json) {
                for node in registry.nodes {
                    self.nodes.insert(node.node_id.clone(), node);
                }
            }
        }
    }

    /// Remove a node from the registry.
    pub fn remove(&self, node_id: &str) -> Option<GridNode> {
        self.nodes.remove(node_id).map(|(_, v)| v)
    }
}

// NOTE: NodeRegistry is INDEXED by transport ADDRESS (`address_to_node_id` →
// Tailscale IP / Reticulum hash), not by the airc `peer_id`. As of #2228 each
// `GridNode` now CARRIES its durable `peer_id` (set at the pairing/gossip
// correlation via `set_peer_id`), and `get_by_peer` resolves a node by that durable
// identity — the join key the router needs to reach the PeerId-keyed capacity gossip
// and settlement reputation (GRID-ELASTIC-CAPABILITY §3d). What is still NOT wired:
// (a) the DashMap's primary index is still the address, so `get_by_peer` is a linear
// scan (fine at fleet scale; a secondary PeerId index is the follow-up when it
// matters); (b) `routing::PeerTrustSource` — resolving an airc caller's TRUST through
// this registry — still needs the airc-side enrollment/trust bridge (task #38); until
// then the auth gate uses its flat Provisional ceiling. See routing/grid_trust_policy.rs.

/// Derive a canonical node_id from a transport address.
fn address_to_node_id(address: &TransportAddress) -> String {
    match address {
        TransportAddress::Tailscale { ip, .. } => ip.clone(),
        TransportAddress::Reticulum { destination_hash } => destination_hash.clone(),
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::super::node::NodeCapability;
    use super::*;

    #[test]
    fn test_upsert_new_node() {
        let dir = std::env::temp_dir().join("grid-test-registry");
        let registry = NodeRegistry::new(&dir);

        registry.upsert_discovered(DiscoveredNode {
            address: TransportAddress::Tailscale {
                ip: "100.1.2.3".into(),
                port: 7117,
                machine_name: Some("test-box".into()),
            },
            capabilities: vec![NodeCapability::Compute {
                gpu: Some("RTX 5090".into()),
                vram_mb: Some(32768),
            }],
            name: Some("test-box".into()),
        });

        let node = registry.get("100.1.2.3").unwrap();
        assert_eq!(node.node_name, Some("test-box".into()));
        assert_eq!(node.trust_level, TrustLevel::Blocked); // Default
        assert_eq!(node.capabilities.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: THE #2228 JOIN. A node carries its durable airc `PeerId`, and the
    // registry can resolve it BY that PeerId — not only by its transport-derived `node_id`.
    // This is the one lookup the router needs to price a node against the PeerId-keyed capacity
    // gossip + settlement reputation. `set_peer_id` is the correlation point (pairing/gossip
    // learns route + PeerId together); `get_by_peer` is the join. Without both, routing (node_id),
    // capacity (PeerId), and reputation (PeerId) stay three key-spaces that never meet
    // (GRID-ELASTIC-CAPABILITY §3d).
    #[test]
    fn peer_id_join_resolves_a_node_by_its_durable_identity() {
        let dir = std::env::temp_dir().join("grid-test-peer-join");
        let _ = std::fs::remove_dir_all(&dir);
        let registry = NodeRegistry::new(&dir);

        let peer = PeerId::from_uuid(uuid::Uuid::from_u128(0x5090));
        registry.upsert_discovered(DiscoveredNode {
            address: TransportAddress::Tailscale {
                ip: "100.9.9.9".into(),
                port: 7117,
                machine_name: Some("home-5090".into()),
            },
            capabilities: vec![],
            name: Some("home-5090".into()),
        });

        // Fresh from a transport scan: no durable identity yet, so the join can't resolve it.
        assert!(
            registry.get_by_peer(&peer).is_none(),
            "a transport-only node has no PeerId to join on"
        );

        // The correlation point: pairing/gossip learns this node IS that airc peer.
        registry.set_peer_id("100.9.9.9", peer).expect("known node");

        // Now the router can find the node by its DURABLE identity — the #2228 join.
        let found = registry.get_by_peer(&peer).expect("node resolves by PeerId");
        assert_eq!(found.node_id, "100.9.9.9", "same node, now reachable by its durable id");
        assert_eq!(found.peer_id, Some(peer), "and it carries the durable identity");

        // Correlating an UNKNOWN node fails loud — never silently invents a node.
        let ghost = PeerId::from_uuid(uuid::Uuid::from_u128(0xdead));
        assert!(
            registry.set_peer_id("100.0.0.0", ghost).is_err(),
            "correlating an unknown node fails loud"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_upsert_merges_addresses() {
        let dir = std::env::temp_dir().join("grid-test-merge");
        let registry = NodeRegistry::new(&dir);

        // First discovery via Tailscale
        registry.upsert_discovered(DiscoveredNode {
            address: TransportAddress::Tailscale {
                ip: "100.1.2.3".into(),
                port: 7117,
                machine_name: None,
            },
            capabilities: vec![],
            name: Some("box".into()),
        });

        // Manually add the same node with a different transport
        let mut node = registry.get("100.1.2.3").unwrap();
        node.addresses.push(TransportAddress::Reticulum {
            destination_hash: "abcd1234".into(),
        });
        registry.register_node(node);

        let updated = registry.get("100.1.2.3").unwrap();
        assert_eq!(updated.addresses.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
