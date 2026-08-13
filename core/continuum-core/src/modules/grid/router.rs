//! GridRouter — decides whether a command executes locally or on a remote node.
//!
//! Routing logic:
//! 1. Explicit nodeId param → route to that node
//! 2. routingHint param → match hint to capability
//! 3. Local execution impossible → find capable remote node
//! 4. Default → execute locally
//!
//! # Eligibility gates ranking (why candidates are filtered before sorting)
//!
//! Every ranking below used to be "sort by advertised VRAM, take the top". That
//! makes the biggest *claim* win, not the biggest card — a node advertising
//! 999999 MB is picked for every job forever, and nothing here notices, because
//! the number is only ever compared against other numbers.
//!
//! So a declared requirement ([`crate::capacity::eligibility::Requirement`], read
//! from `requiresVramMb`) is now applied as a **hard filter before the sort**, via
//! [`crate::capacity::eligibility::eligible`]. Ineligible nodes are not
//! low-ranked; they are not candidates. The ordering is the point: fold the check
//! into the comparator and "big enough" becomes a tiebreak that a large enough
//! claim can outrank.
//!
//! The local node is gated by the same call on the same axis — `prefer-gpu` no
//! longer means "we have *a* GPU" but "we have a GPU that can hold this job".
//!
//! **Honest limit.** The registry's `vram_mb` is a self-reported advertisement,
//! and it is the *only* number a node publishes here — so
//! [`crate::capacity::eligibility::Eligibility::VoidClaim`] (claiming past your
//! own ceiling) is structurally unreachable from this path: with one number,
//! claim and ceiling are the same value. Catching that requires the live capacity
//! ledger's measured free bytes as an independent ceiling, which needs the
//! `PeerId` join key (#2228). Until then this filter refuses nodes that *admit*
//! they are too small, which is what collapses the cross-class hiding place; it
//! does not yet catch a node that lies about its size.

use super::node::{GridNode, NodeCapability, TrustLevel};
use super::registry::NodeRegistry;
use crate::capacity::eligibility::{eligible, Offer, Requirement};

/// Params key by which a caller declares the floor a job cannot run below.
///
/// Megabytes, to match the unit the node registry already advertises in.
pub const PARAM_REQUIRES_VRAM_MB: &str = "requiresVramMb";

fn mb_to_bytes(mb: u64) -> u64 {
    mb.saturating_mul(1024 * 1024)
}

/// Read the declared requirement out of command params.
///
/// Absent ⇒ a floor of zero: an undeclared job is not silently assumed to be
/// huge (which would strand it) nor assumed to fit a specific node (which is the
/// bug this module is closing). It is simply unconstrained, and every node stays
/// eligible exactly as before. Replacing this with a real per-command declaration
/// is M5's requirements lane; this reads whatever the caller states today.
fn requirement_from(params: &serde_json::Value) -> Requirement {
    let mb = params
        .get(PARAM_REQUIRES_VRAM_MB)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    Requirement {
        min_bytes: mb_to_bytes(mb),
    }
}

/// A node's advertised capability, expressed as an eligibility offer.
///
/// `node_total_bytes` mirrors `advertised_bytes` because the registry publishes a
/// single number — see the module doc's honest-limit note. The moment a measured
/// ceiling is available, this is the one place that changes.
fn offer_of(node: &GridNode) -> Offer {
    let bytes = mb_to_bytes(gpu_vram(node));
    Offer {
        advertised_bytes: bytes,
        node_total_bytes: bytes,
    }
}

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

        let requirement = requirement_from(params);

        // 2. Routing hints
        if let Some(hint) = params.get("routingHint").and_then(|v| v.as_str()) {
            match hint {
                HINT_LOCAL_ONLY => return RouteDecision::Local,

                HINT_PREFER_GPU => {
                    // "We have a GPU" is not the question — "we have a GPU that
                    // can hold this job" is. Same gate a peer faces.
                    if self.local_has_gpu && self.local_is_eligible(&requirement) {
                        return RouteDecision::Local;
                    }
                    if let Some(node) = find_gpu_node(registry, &requirement) {
                        return RouteDecision::Remote {
                            node,
                            reason: "prefer-gpu hint",
                        };
                    }
                }

                HINT_MAX_COMPUTE => {
                    if let Some(node) =
                        find_max_compute_node(registry, self.local_vram_mb, &requirement)
                    {
                        return RouteDecision::Remote {
                            node,
                            reason: "max-compute hint",
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
            if let Some(node) = find_gpu_node(registry, &requirement) {
                return RouteDecision::Remote {
                    node,
                    reason: "no local GPU",
                };
            }
        }

        // 4. Default: local
        RouteDecision::Local
    }

    /// Does this box itself clear the declared floor? Judged by the same
    /// [`eligible`] call a peer gets — there are no node types here, and a gate
    /// that exempts the local node is a gate with a hole shaped like us.
    fn local_is_eligible(&self, requirement: &Requirement) -> bool {
        let bytes = mb_to_bytes(self.local_vram_mb);
        eligible(
            requirement,
            &Offer {
                advertised_bytes: bytes,
                node_total_bytes: bytes,
            },
        )
        .is_eligible()
    }
}

/// Check if a command requires GPU hardware.
fn requires_gpu(command: &str) -> bool {
    command.starts_with("genome/train")
        || command.starts_with("plasticity/")
        || (command.starts_with("ai/") && command != "ai/report")
}

/// Find an online, trusted node with GPU capability that can actually hold the
/// job.
///
/// The eligibility filter runs **before** the sort, deliberately. Ranking answers
/// "which of these is best"; it cannot answer "is any of these adequate", and a
/// sort with no floor under it will always return its largest element however
/// small that element is.
fn find_gpu_node(registry: &NodeRegistry, requirement: &Requirement) -> Option<GridNode> {
    let mut candidates: Vec<GridNode> = registry
        .nodes_with_capability("compute")
        .into_iter()
        .filter(|n| n.trust_level >= TrustLevel::Trusted)
        .filter(|n| is_online(n))
        .filter(|n| eligible(requirement, &offer_of(n)).is_eligible())
        .collect();

    // Sort by VRAM descending, then latency ascending
    candidates.sort_by(|a, b| {
        let vram_a = gpu_vram(a);
        let vram_b = gpu_vram(b);
        vram_b.cmp(&vram_a).then_with(|| {
            a.latency_ms
                .unwrap_or(u64::MAX)
                .cmp(&b.latency_ms.unwrap_or(u64::MAX))
        })
    });

    candidates.into_iter().next()
}

/// Find the node with the most compute power (including local).
/// Returns None if local node is the most powerful — or if no remote node clears
/// the declared floor, in which case there is nothing to hop to and the caller
/// stays home.
fn find_max_compute_node(
    registry: &NodeRegistry,
    local_vram: u64,
    requirement: &Requirement,
) -> Option<GridNode> {
    let best_remote = find_gpu_node(registry, requirement)?;
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
    use super::super::node::TransportAddress;
    use super::*;

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
            peer_id: None,
        };
        registry.register_node(node);
        (registry, dir.to_string_lossy().into())
    }

    /// Eligibility gating at the routing seam. These use their own registry dirs
    /// because the fixture above shares one path across tests, and a shell node
    /// leaking into an unrelated case would be a confusing failure.
    mod eligibility_gate {
        use super::*;

        fn node(id: &str, vram_mb: u64, latency_ms: u64) -> GridNode {
            GridNode {
                node_id: id.into(),
                node_name: Some(id.into()),
                addresses: vec![TransportAddress::Tailscale {
                    ip: id.into(),
                    port: 7117,
                    machine_name: None,
                }],
                capabilities: vec![NodeCapability::Compute {
                    gpu: Some("test".into()),
                    vram_mb: Some(vram_mb),
                }],
                trust_level: TrustLevel::Owner,
                last_seen: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                latency_ms: Some(latency_ms),
                // These fixtures predate the #2228 join key and describe nodes known
                // only by transport identity — which is the honest state for a node
                // the registry learned before any beacon correlated it.
                peer_id: None,
            }
        }

        fn registry_of(name: &str, nodes: Vec<GridNode>) -> NodeRegistry {
            let dir = std::env::temp_dir().join(format!("grid-router-elig-{name}"));
            let _ = std::fs::remove_dir_all(&dir);
            let registry = NodeRegistry::new(&dir);
            for n in nodes {
                registry.register_node(n);
            }
            registry
        }

        /// what this catches: THE LIVE CROSS-CLASS HOLE. Before the filter,
        /// `find_gpu_node` sorted by advertised VRAM and took the top with no
        /// floor under it, so the ONLY candidate won even at 1 GB for a 24 GB
        /// job — the caller got a remote hop that could not possibly serve.
        /// Losing on rank was never the issue; being a candidate was.
        #[test]
        fn a_node_that_cannot_hold_the_job_is_never_routed_to() {
            let registry = registry_of("too-small", vec![node("10.0.0.1", 1024, 5)]);
            let router = GridRouter::new(false, 0);

            let decision = router.route(
                "genome/train",
                &serde_json::json!({ PARAM_REQUIRES_VRAM_MB: 24576 }),
                &registry,
            );

            assert!(
                matches!(decision, RouteDecision::Local),
                "a 1GB node must not be handed a 24GB job: {decision:?}"
            );
        }

        /// what this catches: the gate becoming a tiebreak. A shell advertising
        /// tiny specs sits at the FRONT of the latency order and the BACK of the
        /// VRAM order — if eligibility were folded into the comparator it could
        /// still surface. It must be excluded from the candidate set entirely,
        /// leaving the one node that actually fits.
        #[test]
        fn a_cheap_shell_cannot_outrank_its_way_into_a_job_it_cannot_hold() {
            let registry = registry_of(
                "shell-vs-real",
                vec![
                    node("10.0.0.9", 512, 1),     // shell: fastest, useless
                    node("10.0.0.2", 32768, 200), // real: slow but adequate
                ],
            );
            let router = GridRouter::new(false, 0);

            let decision = router.route(
                "ai/generate",
                &serde_json::json!({ PARAM_REQUIRES_VRAM_MB: 24576 }),
                &registry,
            );

            match decision {
                RouteDecision::Remote { node, .. } => assert_eq!(
                    node.node_id, "10.0.0.2",
                    "the adequate node must win over the cheap shell"
                ),
                RouteDecision::Local => panic!("expected the adequate node, got Local"),
            }
        }

        /// what this catches: a silent behaviour change for every existing
        /// caller. No declared requirement means no floor — the gate must be
        /// invisible until someone states a need, or landing it would strand
        /// jobs that route fine today.
        #[test]
        fn an_undeclared_requirement_routes_exactly_as_before() {
            let registry = registry_of("undeclared", vec![node("10.0.0.3", 512, 5)]);
            let router = GridRouter::new(false, 0);

            match router.route("genome/train", &serde_json::json!({}), &registry) {
                RouteDecision::Remote { node, reason } => {
                    assert_eq!(node.node_id, "10.0.0.3");
                    assert_eq!(reason, "no local GPU");
                }
                RouteDecision::Local => panic!("undeclared jobs must route as they always did"),
            }
        }

        /// what this catches: exempting ourselves. `prefer-gpu` used to mean "we
        /// own a GPU", which sent a 24GB job to an 8GB local card while an
        /// adequate peer sat idle. The local node is judged on the same axis by
        /// the same call.
        #[test]
        fn the_local_gpu_is_gated_too_and_yields_to_an_adequate_peer() {
            let registry = registry_of("local-too-small", vec![node("10.0.0.4", 32768, 50)]);
            let router = GridRouter::new(true, 8192);

            let decision = router.route(
                "ai/generate",
                &serde_json::json!({
                    "routingHint": HINT_PREFER_GPU,
                    PARAM_REQUIRES_VRAM_MB: 24576,
                }),
                &registry,
            );

            match decision {
                RouteDecision::Remote { node, reason } => {
                    assert_eq!(node.node_id, "10.0.0.4");
                    assert_eq!(reason, "prefer-gpu hint");
                }
                RouteDecision::Local => {
                    panic!("an 8GB local card must not keep a 24GB job just because it is ours")
                }
            }
        }

        /// what this catches: the inverse — the gate must not evict a local node
        /// that DOES fit. A filter that is merely strict is as wrong as one that
        /// is merely permissive; `prefer-gpu` still prefers a capable local GPU.
        #[test]
        fn an_adequate_local_gpu_still_keeps_the_job() {
            let registry = registry_of("local-fits", vec![node("10.0.0.5", 32768, 50)]);
            let router = GridRouter::new(true, 49152);

            assert!(
                matches!(
                    router.route(
                        "ai/generate",
                        &serde_json::json!({
                            "routingHint": HINT_PREFER_GPU,
                            PARAM_REQUIRES_VRAM_MB: 24576,
                        }),
                        &registry,
                    ),
                    RouteDecision::Local
                ),
                "a local card that clears the floor keeps the job"
            );
        }

        /// what this catches: the #2228 auto-correlation turning discovery into
        /// authorization. `GridModule::tick` now folds every beaconing peer into
        /// the registry automatically, so a stranger with a big advertised card
        /// appears as a candidate-shaped node with no human in the loop. This pins
        /// the BEHAVIOUR rather than the constant: `registry.rs` asserts the trust
        /// default sits below `Trusted`, and this asserts the router does not route
        /// to such a node — so moving the bar cannot silently open the door.
        #[test]
        fn an_auto_registered_beaconing_peer_is_discovered_but_not_routed_to() {
            let dir = std::env::temp_dir().join("grid-router-elig-beacon");
            let _ = std::fs::remove_dir_all(&dir);
            let registry = NodeRegistry::new(&dir);
            let peer = airc_core::PeerId(uuid::Uuid::from_u128(0xbeac04));

            // A beacon self-registers it, advertising a card big enough for the job.
            assert!(registry.ensure_peer_node(peer, Some(32768)));
            assert!(
                registry.get_by_peer(&peer).is_some(),
                "the peer IS discovered — that half must keep working"
            );

            let router = GridRouter::new(false, 0);
            let decision = router.route(
                "ai/generate",
                &serde_json::json!({ PARAM_REQUIRES_VRAM_MB: 24576 }),
                &registry,
            );

            assert!(
                matches!(decision, RouteDecision::Local),
                "an unauthorized beaconing peer must not receive compute: {decision:?}"
            );

            let _ = std::fs::remove_dir_all(&dir);
        }

        /// what this catches: max-compute hopping to a bigger-but-still-too-small
        /// node. "Biggest available" and "big enough" are different questions,
        /// and only the second one makes the hop worth taking.
        #[test]
        fn max_compute_does_not_hop_to_a_node_that_still_cannot_hold_it() {
            let registry = registry_of("max-compute-short", vec![node("10.0.0.6", 16384, 5)]);
            let router = GridRouter::new(true, 8192);

            assert!(
                matches!(
                    router.route(
                        "ai/generate",
                        &serde_json::json!({
                            "routingHint": HINT_MAX_COMPUTE,
                            PARAM_REQUIRES_VRAM_MB: 24576,
                        }),
                        &registry,
                    ),
                    RouteDecision::Local
                ),
                "16GB beats our 8GB but still cannot hold 24GB — the hop buys nothing"
            );
        }
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

        let decision = router.route("genome/train", &serde_json::json!({}), &registry);

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

        let decision = router.route("genome/train", &serde_json::json!({}), &registry);

        assert!(matches!(decision, RouteDecision::Local));
    }
}
