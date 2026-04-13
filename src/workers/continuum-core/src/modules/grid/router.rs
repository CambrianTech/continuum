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

                // best-model-for:<domain> — Many-Worlds grid routing
                // Routes inference to the node with the best model for the task domain
                _ if hint.starts_with(HINT_BEST_MODEL_PREFIX) => {
                    let domain = &hint[HINT_BEST_MODEL_PREFIX.len()..];
                    // Gather local models from Inference capabilities
                    let local_models: Vec<String> = vec![]; // TODO: populate from local capabilities
                    if let Some(node) = find_best_inference_node(registry, domain, &local_models) {
                        return RouteDecision::Remote {
                            node,
                            reason: "best-model-for domain routing",
                        };
                    }
                }

                // node:<name> — route to a named node
                _ if hint.starts_with("node:") => {
                    let name = hint[5..].to_lowercase();
                    let nodes = registry.all_nodes();
                    if let Some(node) = nodes.into_iter().find(|n| {
                        n.node_name.as_deref().map(|s| s.to_lowercase()) == Some(name.clone())
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

        // 3b. Inference routing: if this is an inference command, check if a remote
        // node has a better model. Many-Worlds: the population is the grid.
        if is_inference(command) {
            let domain = params.get("domain")
                .and_then(|v| v.as_str())
                .unwrap_or("general");
            let local_models: Vec<String> = vec![]; // TODO: populate from local Inference capability
            if let Some(node) = find_best_inference_node(registry, domain, &local_models) {
                return RouteDecision::Remote {
                    node,
                    reason: "better model available on grid",
                };
            }
        }

        // 4. Default: local
        RouteDecision::Local
    }
}

/// Routing hint prefix for model-aware inference routing.
/// Usage: routingHint = "best-model-for:code" or "best-model-for:general"
pub const HINT_BEST_MODEL_PREFIX: &str = "best-model-for:";

/// Check if a command requires GPU hardware.
fn requires_gpu(command: &str) -> bool {
    command.starts_with("genome/train")
        || command.starts_with("plasticity/")
        || (command.starts_with("ai/") && command != "ai/report")
}

/// Check if a command is an inference request.
fn is_inference(command: &str) -> bool {
    command == "ai/generate"
        || command == "cognition/generate"
        || command.starts_with("ai/agent")
}

/// Find the best node for inference based on model quality.
/// Ranks by: model size (larger = better), then latency (lower = better).
/// Returns None if local node has the best or only model.
fn find_best_inference_node(
    registry: &NodeRegistry,
    domain: &str,
    local_models: &[String],
) -> Option<GridNode> {
    let candidates: Vec<GridNode> = registry
        .all_nodes()
        .into_iter()
        .filter(|n| n.trust_level >= TrustLevel::Trusted)
        .filter(|n| is_online(n))
        .filter(|n| {
            // Node must have inference capability with at least one model
            n.capabilities.iter().any(|c| matches!(c, NodeCapability::Inference { models } if !models.is_empty()))
        })
        .collect();

    if candidates.is_empty() {
        return None;
    }

    // Score each node: larger model name containing domain keywords = better
    // Model naming convention: "continuum-ai/qwen3.5-27b-code-forged" — the "27b" indicates size
    let score_model = |model: &str, domain: &str| -> u64 {
        let mut score: u64 = 0;
        // Extract parameter count from model name (e.g., "27b" → 27, "4b" → 4)
        for part in model.split('-') {
            if part.ends_with('b') || part.ends_with('B') {
                if let Ok(size) = part[..part.len()-1].parse::<u64>() {
                    score += size * 1000; // Larger model = higher score
                }
            }
        }
        // Bonus if model name contains the domain keyword
        let model_lower = model.to_lowercase();
        if model_lower.contains(domain) {
            score += 500; // Domain match bonus
        }
        // Bonus for "forged" models (our optimized variants)
        if model_lower.contains("forged") {
            score += 200;
        }
        score
    };

    let best_local_score: u64 = local_models
        .iter()
        .map(|m| score_model(m, domain))
        .max()
        .unwrap_or(0);

    // Find best remote node
    let mut scored_nodes: Vec<(GridNode, u64)> = candidates
        .into_iter()
        .map(|node| {
            let best_model_score = node.capabilities
                .iter()
                .filter_map(|c| match c {
                    NodeCapability::Inference { models } => {
                        models.iter().map(|m| score_model(m, domain)).max()
                    }
                    _ => None,
                })
                .max()
                .unwrap_or(0);
            (node, best_model_score)
        })
        .collect();

    // Sort by score descending, then latency ascending
    scored_nodes.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| {
                a.0.latency_ms.unwrap_or(u64::MAX)
                    .cmp(&b.0.latency_ms.unwrap_or(u64::MAX))
            })
    });

    // Only route remotely if remote has a significantly better model
    if let Some((node, score)) = scored_nodes.into_iter().next() {
        if score > best_local_score + 500 {
            // Remote has meaningfully better model
            return Some(node);
        }
    }

    None // Local is good enough
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
