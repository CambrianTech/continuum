//! In-memory registry of per-node inference capabilities.
//!
//! `NodeCapabilityRegistry` is the data structure PR-2 (claude-tab-1)'s
//! GridCapabilityAnnouncer feeds — local node's own capability set + peer
//! announcements arriving from the tailscale mesh. PR-3 (codex)'s
//! `GridInferenceRouter` queries it to pick the best node per job.
//!
//! This file ships ONLY the data structure + pure CRUD. No grid wiring,
//! no broadcast, no announcement logic — those are PR-2's. Keeping it
//! pure means PR-3 can compose against a stable shape that's
//! independently testable.

use crate::inference_capability::types::{InferenceKind, NodeCapability};
use std::collections::HashMap;

/// Live view of every node currently on the mesh + their capabilities.
/// Keyed by `node_id`. Single-threaded — PR-2 wraps in a parking_lot
/// RwLock when wiring the announcer.
#[derive(Debug, Clone, Default)]
pub struct NodeCapabilityRegistry {
    nodes: HashMap<String, NodeCapability>,
}

impl NodeCapabilityRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// How many nodes are tracked. Includes the local node when registered.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Insert or replace a node's full capability advertisement. PR-2's
    /// announcer calls this on every peer message + every local refresh.
    /// `last_updated_ms` on the NodeCapability sets the freshness; PR-3's
    /// router pairs this with a TTL to evict stale entries.
    pub fn upsert(&mut self, node: NodeCapability) {
        self.nodes.insert(node.node_id.clone(), node);
    }

    /// Remove a node (e.g. peer disappeared from the mesh). Returns the
    /// removed advertisement if present, useful for "node left" telemetry.
    pub fn remove(&mut self, node_id: &str) -> Option<NodeCapability> {
        self.nodes.remove(node_id)
    }

    /// Get one node's full advertisement.
    pub fn get(&self, node_id: &str) -> Option<&NodeCapability> {
        self.nodes.get(node_id)
    }

    /// List every known node. PR-3's router walks this for scoring; PR-2's
    /// announcer walks it for digest broadcasts.
    pub fn list(&self) -> impl Iterator<Item = &NodeCapability> {
        self.nodes.values()
    }

    /// Find all nodes that advertise the given `kind` with at least
    /// `min_free_vram_bytes` available. PR-3 calls this first, then
    /// scores the result subset on latency + lease count + RTT.
    ///
    /// Returns ALL viable candidates, not a "best" pick — scoring is
    /// PR-3's concern, not the registry's. Keeps the registry pure
    /// data-access; routing policy stays in the router module.
    pub fn find_capable<'a>(
        &'a self,
        kind: &'a InferenceKind,
        min_free_vram_bytes: u64,
    ) -> impl Iterator<Item = &'a NodeCapability> + 'a {
        self.nodes.values().filter(move |node| {
            node.capabilities
                .iter()
                .any(|cap| cap.kind == *kind && cap.free_vram_bytes >= min_free_vram_bytes)
        })
    }

    /// Like [`find_capable`], but ALSO requires the node to have a GPU
    /// backend (Metal/CUDA/Vulkan). This is the cross-node GPU-first
    /// guardrail (Joel's "GPU-first ALWAYS", promoted from per-node to
    /// ACROSS-node; no_cpu_fallback contract, the OFFLOAD path): a weak
    /// node OFFLOADS heavy inference to a GPU peer, never a CPU-only peer.
    /// When this returns empty, the routing mechanism must REFUSE the
    /// offload (deny loud) rather than fall back to a CPU peer that
    /// `find_capable` would still surface.
    ///
    /// `find_capable` stays GPU-agnostic on purpose — the routing
    /// mechanism owns peer *selection*; this method is the *invariant* on
    /// top: the contract guarantees the picked offload peer is a GPU peer.
    pub fn find_gpu_capable<'a>(
        &'a self,
        kind: &'a InferenceKind,
        min_free_vram_bytes: u64,
    ) -> impl Iterator<Item = &'a NodeCapability> + 'a {
        self.find_capable(kind, min_free_vram_bytes)
            .filter(|node| node.hardware.has_gpu())
    }

    /// Evict every node whose `last_updated_ms` is older than `cutoff_ms`.
    /// Returns the count of evicted nodes. PR-2's announcer ticks the TTL
    /// on broker cadence; this is the helper it calls.
    pub fn evict_stale(&mut self, cutoff_ms: u64) -> usize {
        let before = self.nodes.len();
        self.nodes.retain(|_, n| n.last_updated_ms >= cutoff_ms);
        before - self.nodes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference_capability::types::{
        kinds, HardwareProfile, InferenceCapability, LatencyClass,
    };

    fn mk_node(
        node_id: &str,
        kind: &str,
        free_vram_bytes: u64,
        last_updated_ms: u64,
    ) -> NodeCapability {
        NodeCapability {
            node_id: node_id.into(),
            hardware: HardwareProfile {
                platform: "test".into(),
                has_metal: true,
                has_cuda: false,
                has_vulkan: false,
                free_vram_bytes,
                total_vram_bytes: free_vram_bytes,
                cpu_cores: 8,
                system_ram_bytes: 16 * 1024 * 1024 * 1024,
            },
            capabilities: vec![InferenceCapability {
                kind: InferenceKind::from(kind),
                free_vram_bytes,
                current_lease_count: 0,
                latency_class: LatencyClass::Local,
            }],
            last_updated_ms,
        }
    }

    /// What this catches: fresh registry has zero nodes; insertion goes
    /// from 0 → 1; lookup by id returns the inserted node. Core CRUD
    /// happy path.
    #[test]
    fn upsert_then_get_round_trips() {
        let mut r = NodeCapabilityRegistry::new();
        assert_eq!(r.node_count(), 0);
        let n = mk_node("node-a", kinds::LLAMACPP, 8_000_000_000, 1000);
        r.upsert(n.clone());
        assert_eq!(r.node_count(), 1);
        assert_eq!(r.get("node-a"), Some(&n));
    }

    /// What this catches: upsert REPLACES, not appends. A peer's
    /// repeated announcements over the wire update the live view rather
    /// than accumulating duplicates.
    #[test]
    fn upsert_with_same_id_replaces_not_appends() {
        let mut r = NodeCapabilityRegistry::new();
        r.upsert(mk_node("node-a", kinds::LLAMACPP, 1_000_000_000, 100));
        r.upsert(mk_node("node-a", kinds::LLAMACPP, 5_000_000_000, 200));
        assert_eq!(r.node_count(), 1);
        let got = r.get("node-a").unwrap();
        assert_eq!(got.last_updated_ms, 200);
        assert_eq!(got.capabilities[0].free_vram_bytes, 5_000_000_000);
    }

    /// What this catches: remove returns the previous value, signaling
    /// "node-a was here before". PR-2's announcer uses this for "node
    /// left" telemetry; if the API silently dropped the value, the
    /// telemetry would lose what node disappeared.
    #[test]
    fn remove_returns_previous_value() {
        let mut r = NodeCapabilityRegistry::new();
        let n = mk_node("node-a", kinds::LLAMACPP, 1_000_000_000, 100);
        r.upsert(n.clone());
        let removed = r.remove("node-a");
        assert_eq!(removed, Some(n));
        assert_eq!(r.node_count(), 0);
        assert_eq!(r.remove("node-a"), None, "second remove is a no-op");
    }

    /// What this catches: find_capable returns only nodes with BOTH the
    /// matching kind AND adequate free VRAM. The two-clause filter is
    /// load-bearing — a node with the right kind but no VRAM, or vice
    /// versa, must be excluded.
    #[test]
    fn find_capable_filters_on_kind_and_vram() {
        let mut r = NodeCapabilityRegistry::new();
        r.upsert(mk_node(
            "big-llamacpp",
            kinds::LLAMACPP,
            24_000_000_000,
            100,
        ));
        r.upsert(mk_node(
            "small-llamacpp",
            kinds::LLAMACPP,
            2_000_000_000,
            100,
        ));
        r.upsert(mk_node("big-candle", kinds::CANDLE, 24_000_000_000, 100));

        let llamacpp = InferenceKind::from(kinds::LLAMACPP);
        let want_5gb: Vec<&str> = r
            .find_capable(&llamacpp, 5_000_000_000)
            .map(|n| n.node_id.as_str())
            .collect();
        assert_eq!(want_5gb, vec!["big-llamacpp"], "small-llamacpp lacks VRAM");

        let want_any: Vec<&str> = {
            let mut v: Vec<&str> = r
                .find_capable(&llamacpp, 0)
                .map(|n| n.node_id.as_str())
                .collect();
            v.sort();
            v
        };
        assert_eq!(want_any, vec!["big-llamacpp", "small-llamacpp"]);
    }

    /// What this catches: the cross-node GPU-first guardrail (no_cpu_fallback
    /// offload path). A CPU-only peer (no Metal/CUDA/Vulkan) that advertises
    /// the right kind + VRAM is still surfaced by `find_capable` (GPU-agnostic,
    /// the mechanism's concern) but MUST be excluded by `find_gpu_capable`.
    /// If it weren't, a weak node could offload a heavy job onto a CPU peer —
    /// the exact "GPU-first ALWAYS" violation this guardrail forbids. Empty
    /// result = the routing mechanism must DENY, not CPU-serve.
    #[test]
    fn find_gpu_capable_excludes_cpu_only_peers() {
        let mut r = NodeCapabilityRegistry::new();
        let mut cpu_only = mk_node("cpu-box", kinds::LLAMACPP, 32_000_000_000, 100);
        cpu_only.hardware.has_metal = false;
        cpu_only.hardware.has_cuda = false;
        cpu_only.hardware.has_vulkan = false;
        r.upsert(cpu_only);

        let llamacpp = InferenceKind::from(kinds::LLAMACPP);
        // GPU-agnostic find still surfaces it (mechanism's job to pick)...
        assert_eq!(r.find_capable(&llamacpp, 1).count(), 1);
        // ...but the GPU-first guardrail leaves NO eligible offload target.
        assert_eq!(
            r.find_gpu_capable(&llamacpp, 1).count(),
            0,
            "CPU-only peer must NOT be an eligible offload target — offload must be DENIED, not CPU-served"
        );
    }

    /// What this catches: with a real GPU peer present, find_gpu_capable
    /// returns ONLY it (the CPU-only peer is filtered out). Pairs with the
    /// exclusion test to pin the full invariant: GPU peer => route there;
    /// only CPU peers => empty => deny.
    #[test]
    fn find_gpu_capable_returns_gpu_peer_not_cpu_peer() {
        let mut r = NodeCapabilityRegistry::new();
        // mk_node defaults has_metal: true => a GPU (Apple Silicon) peer.
        r.upsert(mk_node("gpu-mac", kinds::LLAMACPP, 24_000_000_000, 100));
        let mut cpu_only = mk_node("cpu-box", kinds::LLAMACPP, 64_000_000_000, 100);
        cpu_only.hardware.has_metal = false;
        r.upsert(cpu_only);

        let llamacpp = InferenceKind::from(kinds::LLAMACPP);
        let gpu: Vec<&str> = r
            .find_gpu_capable(&llamacpp, 1)
            .map(|n| n.node_id.as_str())
            .collect();
        assert_eq!(
            gpu,
            vec!["gpu-mac"],
            "only the GPU peer is an eligible offload target, even though cpu-box has more VRAM"
        );
    }

    /// What this catches: a CUDA node (no metal) and a Vulkan node both
    /// count as GPU peers — the guardrail is backend-agnostic, it just
    /// requires SOME GPU. Guards against a future regression that hardcodes
    /// has_metal only and silently drops CUDA/Vulkan offload targets.
    #[test]
    fn find_gpu_capable_accepts_cuda_and_vulkan_not_just_metal() {
        let mut r = NodeCapabilityRegistry::new();
        let mut cuda = mk_node("cuda-5090", kinds::LLAMACPP, 32_000_000_000, 100);
        cuda.hardware.has_metal = false;
        cuda.hardware.has_cuda = true;
        r.upsert(cuda);
        let mut vulkan = mk_node("vulkan-amd", kinds::LLAMACPP, 24_000_000_000, 100);
        vulkan.hardware.has_metal = false;
        vulkan.hardware.has_vulkan = true;
        r.upsert(vulkan);

        let llamacpp = InferenceKind::from(kinds::LLAMACPP);
        let mut ids: Vec<&str> = r
            .find_gpu_capable(&llamacpp, 1)
            .map(|n| n.node_id.as_str())
            .collect();
        ids.sort();
        assert_eq!(ids, vec!["cuda-5090", "vulkan-amd"]);
    }

    /// What this catches: find_capable on a kind no node advertises
    /// returns empty (not panic, not partial match). PR-3's router needs
    /// "nobody can take this job" to be a clean signal.
    #[test]
    fn find_capable_returns_empty_when_kind_not_advertised() {
        let mut r = NodeCapabilityRegistry::new();
        r.upsert(mk_node(
            "llamacpp-only",
            kinds::LLAMACPP,
            8_000_000_000,
            100,
        ));
        let ort_vision = InferenceKind::from(kinds::ORT_VISION);
        let got: Vec<_> = r.find_capable(&ort_vision, 0).collect();
        assert!(got.is_empty());
    }

    /// What this catches: list iterates all nodes. PR-2's broadcast +
    /// PR-3's full-walk scoring both depend on this returning every
    /// entry, not a paginated subset.
    #[test]
    fn list_iterates_all_nodes() {
        let mut r = NodeCapabilityRegistry::new();
        for i in 0..5 {
            r.upsert(mk_node(
                &format!("node-{i}"),
                kinds::LLAMACPP,
                4_000_000_000,
                100,
            ));
        }
        let mut ids: Vec<&str> = r.list().map(|n| n.node_id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, vec!["node-0", "node-1", "node-2", "node-3", "node-4"]);
    }

    /// What this catches: evict_stale removes only nodes older than the
    /// cutoff; fresh nodes stay. Returns the count of evictions for
    /// telemetry.
    #[test]
    fn evict_stale_removes_only_old_nodes() {
        let mut r = NodeCapabilityRegistry::new();
        r.upsert(mk_node("old-a", kinds::LLAMACPP, 4_000_000_000, 100));
        r.upsert(mk_node("old-b", kinds::LLAMACPP, 4_000_000_000, 200));
        r.upsert(mk_node("fresh", kinds::LLAMACPP, 4_000_000_000, 1000));

        let evicted = r.evict_stale(500);
        assert_eq!(evicted, 2);
        assert_eq!(r.node_count(), 1);
        assert!(r.get("fresh").is_some());
        assert!(r.get("old-a").is_none());
        assert!(r.get("old-b").is_none());
    }

    /// What this catches: evict_stale with no stale entries returns 0
    /// and doesn't touch any node. PR-2 calls this on every tick; a
    /// no-op tick must be free.
    #[test]
    fn evict_stale_no_op_when_all_fresh() {
        let mut r = NodeCapabilityRegistry::new();
        r.upsert(mk_node("fresh-a", kinds::LLAMACPP, 4_000_000_000, 1000));
        r.upsert(mk_node("fresh-b", kinds::LLAMACPP, 4_000_000_000, 2000));
        let evicted = r.evict_stale(500);
        assert_eq!(evicted, 0);
        assert_eq!(r.node_count(), 2);
    }

    /// What this catches: empty registry's list iterator yields nothing
    /// and node_count is zero. PR-2's announcer + PR-3's router both walk
    /// `list()`; an empty registry must be a clean "no nodes" signal,
    /// not a panic and not stray ghost entries.
    #[test]
    fn empty_registry_list_is_empty() {
        let r = NodeCapabilityRegistry::new();
        assert_eq!(r.list().count(), 0);
        assert_eq!(r.node_count(), 0);
    }

    /// What this catches: get on a node_id that was never inserted
    /// returns None (not panic, not stale value). PR-3's router uses
    /// `get` to look up a node it scored; if the node was evicted in
    /// between, None is the correct "rescore needed" signal.
    #[test]
    fn get_returns_none_for_unknown_id() {
        let mut r = NodeCapabilityRegistry::new();
        r.upsert(mk_node("node-a", kinds::LLAMACPP, 4_000_000_000, 100));
        assert!(r.get("node-z").is_none());
    }

    /// What this catches: find_capable matches when free_vram_bytes is
    /// EXACTLY the requested minimum, not just strictly greater. The
    /// router asks "can you take >=X bytes"; the boundary is inclusive.
    /// Symmetric with `evict_stale_keeps_node_at_exact_cutoff`.
    #[test]
    fn find_capable_matches_on_exact_vram_boundary() {
        let mut r = NodeCapabilityRegistry::new();
        r.upsert(mk_node("exact", kinds::LLAMACPP, 5_000_000_000, 100));
        let llamacpp = InferenceKind::from(kinds::LLAMACPP);
        let got: Vec<&str> = r
            .find_capable(&llamacpp, 5_000_000_000)
            .map(|n| n.node_id.as_str())
            .collect();
        assert_eq!(got, vec!["exact"], "exact-match VRAM must qualify");
    }

    /// What this catches: evict_stale keeps a node whose `last_updated_ms`
    /// is EXACTLY at the cutoff (inclusive). The TTL boundary is the most
    /// recent timestamp still "fresh." Symmetric with the find_capable
    /// VRAM-boundary test — both establish inclusive-min semantics.
    #[test]
    fn evict_stale_keeps_node_at_exact_cutoff() {
        let mut r = NodeCapabilityRegistry::new();
        r.upsert(mk_node("at-cutoff", kinds::LLAMACPP, 4_000_000_000, 500));
        r.upsert(mk_node("one-ms-stale", kinds::LLAMACPP, 4_000_000_000, 499));
        let evicted = r.evict_stale(500);
        assert_eq!(evicted, 1);
        assert!(r.get("at-cutoff").is_some(), "exact-cutoff must NOT evict");
        assert!(r.get("one-ms-stale").is_none());
    }

    /// What this catches: clearing the registry by removing every node
    /// leaves node_count at 0 and list empty. Sanity check that remove
    /// returns to the empty state — important for PR-2 teardown paths
    /// (mesh teardown, scope shutdown) that drain peer state.
    #[test]
    fn remove_all_nodes_returns_to_empty() {
        let mut r = NodeCapabilityRegistry::new();
        for i in 0..3 {
            r.upsert(mk_node(
                &format!("n-{i}"),
                kinds::LLAMACPP,
                4_000_000_000,
                100,
            ));
        }
        assert_eq!(r.node_count(), 3);
        for i in 0..3 {
            assert!(r.remove(&format!("n-{i}")).is_some());
        }
        assert_eq!(r.node_count(), 0);
        assert_eq!(r.list().count(), 0);
    }

    /// What this catches: find_capable with a dynamic (registry-unknown)
    /// kind returns empty rather than panicking. Future backends added
    /// via `InferenceKind::from("tflite")` must not break the lookup
    /// path before any nodes advertise them.
    #[test]
    fn find_capable_handles_dynamic_unknown_kind() {
        let mut r = NodeCapabilityRegistry::new();
        r.upsert(mk_node("known", kinds::LLAMACPP, 4_000_000_000, 100));
        let mlx = InferenceKind::from("mlx-future");
        assert_eq!(r.find_capable(&mlx, 0).count(), 0);
    }

    /// What this catches: a node with multiple capabilities (e.g. a Mac
    /// with llamacpp + candle + 4 ort kinds) shows up in find_capable
    /// for each matching kind, not duplicated within one kind. Sanity
    /// check on the multi-cap shape.
    #[test]
    fn multi_capability_node_appears_per_kind() {
        let mut r = NodeCapabilityRegistry::new();
        let multi_cap = NodeCapability {
            node_id: "m5-pro".into(),
            hardware: HardwareProfile {
                platform: "macos-arm64-m5pro".into(),
                has_metal: true,
                has_cuda: false,
                has_vulkan: false,
                free_vram_bytes: 32_000_000_000,
                total_vram_bytes: 48_000_000_000,
                cpu_cores: 16,
                system_ram_bytes: 64_000_000_000,
            },
            capabilities: vec![
                InferenceCapability {
                    kind: InferenceKind::from(kinds::LLAMACPP),
                    free_vram_bytes: 32_000_000_000,
                    current_lease_count: 0,
                    latency_class: LatencyClass::Local,
                },
                InferenceCapability {
                    kind: InferenceKind::from(kinds::CANDLE),
                    free_vram_bytes: 32_000_000_000,
                    current_lease_count: 0,
                    latency_class: LatencyClass::Local,
                },
                InferenceCapability {
                    kind: InferenceKind::from(kinds::ORT_VISION),
                    free_vram_bytes: 32_000_000_000,
                    current_lease_count: 0,
                    latency_class: LatencyClass::Local,
                },
            ],
            last_updated_ms: 1000,
        };
        r.upsert(multi_cap);

        let llamacpp = InferenceKind::from(kinds::LLAMACPP);
        let candle = InferenceKind::from(kinds::CANDLE);
        let vision = InferenceKind::from(kinds::ORT_VISION);
        let stt = InferenceKind::from(kinds::ORT_STT);

        assert_eq!(r.find_capable(&llamacpp, 0).count(), 1);
        assert_eq!(r.find_capable(&candle, 0).count(), 1);
        assert_eq!(r.find_capable(&vision, 0).count(), 1);
        assert_eq!(
            r.find_capable(&stt, 0).count(),
            0,
            "STT not advertised by this node"
        );
    }
}
