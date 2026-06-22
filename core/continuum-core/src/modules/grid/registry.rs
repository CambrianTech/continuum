//! Grid node registry — tracks known nodes, their capabilities, and trust levels.
//!
//! Persisted to .continuum/grid/known_nodes.json.
//! Updated by discovery, manual pairing, and periodic health checks.

use super::node::{DiscoveredNode, GridNode, TransportAddress, TrustLevel};
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
            });
    }

    /// Register a node manually (e.g., from pairing or config).
    pub fn register_node(&self, node: GridNode) {
        self.nodes.insert(node.node_id.clone(), node);
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

// NOTE: NodeRegistry intentionally does NOT implement
// `routing::PeerTrustSource`. It is keyed by transport ADDRESS
// (`address_to_node_id` → Tailscale IP / Reticulum hash), NOT by the airc
// `peer_id` that a `CallerIdentity` carries — so resolving an airc caller's trust
// through this registry would silently miss every time. The airc↔grid trust
// bridge needs a peer_id-keyed trust source (airc-side enrollment/trust), which is
// the airc↔grid identity unification (task #38). Until that exists, the auth gate
// uses its flat Provisional ceiling. See routing/grid_trust_policy.rs.

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
