//! GridRouter — decides whether a command executes locally or on a remote node.
//!
//! Routing logic:
//! 1. Explicit nodeId param → route to that node
//! 2. routingHint param → match hint to capability
//! 3. Local execution impossible → find capable remote node
//! 4. Default → execute locally

use super::node::{GridNode, NodeCapability, TrustLevel};
use super::registry::NodeRegistry;

/// Routing decision for a command.
#[derive(Debug, Clone)]
pub enum RouteDecision {
    /// Execute locally on this node.
    Local,
    /// Forward to a specific remote node.
    Remote {
        node: GridNode,
        reason: &'static str,
    },
}

/// Routing hints that can be passed as params.
pub const HINT_PREFER_GPU: &str = "prefer-gpu";
pub const HINT_MAX_COMPUTE: &str = "max-compute";
pub const HINT_LOCAL_ONLY: &str = "local-only";

/// The Grid router.
pub struct GridRouter {
    /// Whether this node has a GPU.
    pub local_has_gpu: bool,
    /// Local GPU VRAM in MB (0 if no GPU).
    pub local_vram_mb: u64,
}

impl GridRouter {
    pub fn new(local_has_gpu: bool, local_vram_mb: u64) -> Self {
        Self {
            local_has_gpu,
            local_vram_mb,
        }
    }

    /// Decide where to route a command.
    pub fn route(
        &self,
        command: &str,
        params: &serde_json::Value,
        registry: &NodeRegistry,
    ) -> RouteDecision {
        // 1. Explicit nodeId targeting
        if let Some(node_id) = params.get("nodeId").and_then(|v| v.as_str()) {
            if let Some(node) = registry.get(node_id) {
                if node.trust_level >= TrustLevel::Provisional {
                    return RouteDecision::Remote {
                        node,
                        reason: "explicit nodeId",
                    };
                }
            }
            // If nodeId specified but not found/trusted, fall through to local
            return RouteDecision::Local;
        }

        // 2. Routing hints
        if let Some(hint) = params.get("routingHint").and_then(|v| v.as_str()) {
            match hint {
                HINT_LOCAL_ONLY => return RouteDecision::Local,

                HINT_PREFER_GPU => {
                    if self.local_has_gpu {
                        return RouteDecision::Local;
                    }
                    if let Some(node) = find_gpu_node(registry) {
                        return RouteDecision::Remote {
                            node,
                            reason: "prefer-gpu hint",
                        };
                    }
                }

                HINT_MAX_COMPUTE => {
                    if let Some(node) = find_max_compute_node(registry, self.local_vram_mb) {
                        return RouteDecision::Remote {
                            node,
                            reason: "max-compute hint",
                        };
                    }
                }

                // node:<name> — route to a named node
                _ if hint.starts_with("node:") => {
                    let name = &hint[5..];
                    let nodes = registry.all_nodes();
                    if let Some(node) = nodes.into_iter().find(|n| {
                        n.node_name.as_deref() == Some(name)
                            && n.trust_level >= TrustLevel::Provisional
                    }) {
                        return RouteDecision::Remote {
                            node,
                            reason: "named node hint",
                        };
                    }
                }

                _ => {} // Unknown hint, fall through
            }
        }

        // 3. Capability-based routing (can't run locally?)
        if requires_gpu(command) && !self.local_has_gpu {
            if let Some(node) = find_gpu_node(registry) {
                return RouteDecision::Remote {
                    node,
                    reason: "no local GPU",
                };
            }
        }

        // 4. Default: local
        RouteDecision::Local
    }
}

/// Check if a command requires GPU hardware.
fn requires_gpu(command: &str) -> bool {
    command.starts_with("genome/train")
        || command.starts_with("plasticity/")
        || (command.starts_with("ai/") && command != "ai/report")
}

/// Find an online, trusted node with GPU capability.
fn find_gpu_node(registry: &NodeRegistry) -> Option<GridNode> {
    let mut candidates: Vec<GridNode> = registry
        .nodes_with_capability("compute")
        .into_iter()
        .filter(|n| n.trust_level >= TrustLevel::Trusted)
        .filter(|n| is_online(n))
        .collect();

    // Sort by VRAM descending, then latency ascending
    candidates.sort_by(|a, b| {
        let vram_a = gpu_vram(a);
        let vram_b = gpu_vram(b);
        vram_b.cmp(&vram_a)
            .then_with(|| a.latency_ms.unwrap_or(u64::MAX).cmp(&b.latency_ms.unwrap_or(u64::MAX)))
    });

    candidates.into_iter().next()
}

/// Find the node with the most compute power (including local).
/// Returns None if local node is the most powerful.
fn find_max_compute_node(registry: &NodeRegistry, local_vram: u64) -> Option<GridNode> {
    let best_remote = find_gpu_node(registry)?;
    let remote_vram = gpu_vram(&best_remote);
    if remote_vram > local_vram {
        Some(best_remote)
    } else {
        None // Local is best
    }
}

/// Extract GPU VRAM from a node's capabilities.
fn gpu_vram(node: &GridNode) -> u64 {
    node.capabilities
        .iter()
        .filter_map(|c| match c {
            NodeCapability::Compute { vram_mb, .. } => *vram_mb,
            _ => None,
        })
        .max()
        .unwrap_or(0)
}

/// Check if a node was seen recently (within 5 minutes).
fn is_online(node: &GridNode) -> bool {
    let cutoff = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    node.last_seen >= cutoff.saturating_sub(5 * 60 * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::node::TransportAddress;

    fn test_registry_with_gpu_node() -> (NodeRegistry, String) {
        let dir = std::env::temp_dir().join("grid-test-router");
        let registry = NodeRegistry::new(&dir);

        let node = GridNode {
            node_id: "100.1.2.3".into(),
            node_name: Some("home-5090".into()),
            addresses: vec![TransportAddress::Tailscale {
                ip: "100.1.2.3".into(),
                port: 7117,
                machine_name: Some("bigmama".into()),
            }],
            capabilities: vec![NodeCapability::Compute {
                gpu: Some("RTX 5090".into()),
                vram_mb: Some(32768),
            }],
            trust_level: TrustLevel::Owner,
            last_seen: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            latency_ms: Some(47),
        };
        registry.register_node(node);
        (registry, dir.to_string_lossy().into())
    }

    #[test]
    fn test_explicit_node_id() {
        let (registry, _dir) = test_registry_with_gpu_node();
        let router = GridRouter::new(false, 0);

        let decision = router.route(
            "genome/train",
            &serde_json::json!({"nodeId": "100.1.2.3"}),
            &registry,
        );

        match decision {
            RouteDecision::Remote { node, reason } => {
                assert_eq!(node.node_id, "100.1.2.3");
                assert_eq!(reason, "explicit nodeId");
            }
            RouteDecision::Local => panic!("Expected remote routing"),
        }
    }

    #[test]
    fn test_gpu_command_without_local_gpu() {
        let (registry, _dir) = test_registry_with_gpu_node();
        let router = GridRouter::new(false, 0);

        let decision = router.route(
            "genome/train",
            &serde_json::json!({}),
            &registry,
        );

        match decision {
            RouteDecision::Remote { node, reason } => {
                assert_eq!(node.node_id, "100.1.2.3");
                assert_eq!(reason, "no local GPU");
            }
            RouteDecision::Local => panic!("Expected remote routing"),
        }
    }

    #[test]
    fn test_local_only_hint() {
        let (registry, _dir) = test_registry_with_gpu_node();
        let router = GridRouter::new(false, 0);

        let decision = router.route(
            "genome/train",
            &serde_json::json!({"routingHint": "local-only"}),
            &registry,
        );

        assert!(matches!(decision, RouteDecision::Local));
    }

    #[test]
    fn test_default_is_local() {
        let (registry, _dir) = test_registry_with_gpu_node();
        let router = GridRouter::new(true, 8192); // Has GPU

        let decision = router.route(
            "genome/train",
            &serde_json::json!({}),
            &registry,
        );

        assert!(matches!(decision, RouteDecision::Local));
    }
}
