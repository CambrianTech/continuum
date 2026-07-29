//! `node_content` — the grid's "who-has-what" registry: what each live node
//! HOLDS, distinct from `DeviceCapacity` (what it can FIT).
//!
//! `GridSnapshot` already answers "how many lanes fit where" (capacity). To make
//! the grid smarter than a pile of toy GPUs, the router also needs "who already
//! holds the thing this request needs" (content). That is the foundation for the
//! three grid-native serving wins ([[product-strategy-vs-exo]]):
//!   1. **cross-node expert sharding** — K3's 896 experts split across nodes;
//!      route each token's ~16 ACTIVE experts to whichever node holds them (sparse
//!      traffic, unlike exo's dense per-layer LAN hop).
//!   2. **prefix-aware routing** — send a multi-turn request to the node whose KV
//!      cache is already warm for its prefix → skip prefill.
//!   3. **model sharing** — route to a node that already has the model resident,
//!      instead of cold-loading it.
//!
//! Churn-safe by construction ([[restarts-are-commonplace]]): content is indexed
//! per live `PeerId` from a point-in-time view. A node that drops simply isn't in
//! the index — its content vanishes, queries fall back to another holder or to
//! local recompute. Nothing is held across the churn; every query re-derives.

use std::collections::HashMap;

use crate::identity::PeerId;

/// Which experts of a model a node holds — the unit of cross-node MoE sharding.
/// A contiguous `[first, last]` expert range (inclusive) per (model, layer);
/// ranges compose so a query can find the holder of any activated expert id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpertShard {
    pub model_id: String,
    /// Layer this shard's experts belong to (MoE experts are per-layer).
    pub layer: u32,
    /// Inclusive expert-id range this node holds for `(model_id, layer)`.
    pub first_expert: u32,
    pub last_expert: u32,
}

impl ExpertShard {
    #[inline]
    pub fn holds(&self, model_id: &str, layer: u32, expert: u32) -> bool {
        self.model_id == model_id
            && self.layer == layer
            && expert >= self.first_expert
            && expert <= self.last_expert
    }
}

/// A warm KV-cache prefix a node can continue without recomputing prefill.
/// `hash` is a rolling hash of the prompt token prefix; `token_len` lets the
/// router prefer the LONGEST warm prefix match (most prefill skipped).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarmPrefix {
    pub model_id: String,
    pub hash: u64,
    pub token_len: u32,
}

/// What ONE node advertises it holds this tick. Small + cheap to gossip alongside
/// `DeviceCapacity` on the existing snapshot cadence.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeContent {
    /// Model ids fully resident + serve-ready on this node.
    pub resident_models: Vec<String>,
    /// Expert shards this node holds (for models it serves as a shard, not whole).
    pub expert_shards: Vec<ExpertShard>,
    /// Warm KV prefixes this node can continue.
    pub warm_prefixes: Vec<WarmPrefix>,
}

impl NodeContent {
    pub fn holds_model(&self, model_id: &str) -> bool {
        self.resident_models.iter().any(|m| m == model_id)
    }
}

/// Where a given (model, layer, expert) lives on the grid, if anywhere live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpertLocation {
    /// The local node holds it — no network hop.
    Local,
    /// A peer holds it — route the sparse activation there.
    Peer(PeerId),
    /// No live node holds it — fault it in locally (page from disk) as the fallback.
    Fault,
}

/// The grid-level content query. Built per point-in-time view from the LOCAL
/// node's content + the CURRENTLY-LIVE peers' content. A dropped peer is absent,
/// so every answer names only reachable holders.
#[derive(Debug, Clone, Default)]
pub struct GridContentIndex {
    local: NodeContent,
    peers: HashMap<PeerId, NodeContent>,
}

impl GridContentIndex {
    pub fn new(local: NodeContent, peers: HashMap<PeerId, NodeContent>) -> Self {
        Self { local, peers }
    }

    /// Prefix-aware routing: the live node with the LONGEST warm prefix for this
    /// model whose hash matches — or None (do prefill locally). `None` peer means
    /// the local node is the best holder.
    pub fn best_prefix_holder(&self, model_id: &str, hash: u64) -> Option<(Option<PeerId>, u32)> {
        let mut best: Option<(Option<PeerId>, u32)> = None;
        let mut consider = |peer: Option<PeerId>, c: &NodeContent| {
            for p in &c.warm_prefixes {
                if p.model_id == model_id && p.hash == hash {
                    if best.as_ref().map(|(_, len)| p.token_len > *len).unwrap_or(true) {
                        best = Some((peer.clone(), p.token_len));
                    }
                }
            }
        };
        consider(None, &self.local);
        for (id, c) in &self.peers {
            consider(Some(id.clone()), c);
        }
        best
    }

    /// Cross-node expert sharding: where a token's activated expert lives. Local
    /// wins (no hop); else the first live peer holding it; else Fault (page local).
    pub fn locate_expert(&self, model_id: &str, layer: u32, expert: u32) -> ExpertLocation {
        if self.local.expert_shards.iter().any(|s| s.holds(model_id, layer, expert)) {
            return ExpertLocation::Local;
        }
        for (id, c) in &self.peers {
            if c.expert_shards.iter().any(|s| s.holds(model_id, layer, expert)) {
                return ExpertLocation::Peer(id.clone());
            }
        }
        ExpertLocation::Fault
    }

    /// Model sharing: live nodes with this model fully resident (local first).
    pub fn nodes_with_model(&self, model_id: &str) -> Vec<Option<PeerId>> {
        let mut out = Vec::new();
        if self.local.holds_model(model_id) {
            out.push(None);
        }
        for (id, c) in &self.peers {
            if c.holds_model(model_id) {
                out.push(Some(id.clone()));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(n: u128) -> PeerId {
        PeerId::from_u128(n)
    }

    // what this catches: expert-sharding routing must send an activated expert to
    // the node that actually holds its range, and Fault when nobody live does —
    // a wrong holder or a silent skip corrupts the MoE forward.
    #[test]
    fn locate_expert_routes_to_holder_and_faults_when_absent() {
        let local = NodeContent {
            expert_shards: vec![ExpertShard {
                model_id: "k3".into(), layer: 5, first_expert: 0, last_expert: 447,
            }],
            ..Default::default()
        };
        let mut peers = HashMap::new();
        peers.insert(peer(2), NodeContent {
            expert_shards: vec![ExpertShard {
                model_id: "k3".into(), layer: 5, first_expert: 448, last_expert: 895,
            }],
            ..Default::default()
        });
        let idx = GridContentIndex::new(local, peers);
        assert_eq!(idx.locate_expert("k3", 5, 10), ExpertLocation::Local);
        assert_eq!(idx.locate_expert("k3", 5, 500), ExpertLocation::Peer(peer(2)));
        // expert id out of every live shard's range -> fault in locally
        assert_eq!(idx.locate_expert("k3", 5, 900), ExpertLocation::Fault);
        // a DROPPED peer: rebuild index without it -> its experts now Fault
        let idx2 = GridContentIndex::new(
            NodeContent { expert_shards: vec![ExpertShard {
                model_id: "k3".into(), layer: 5, first_expert: 0, last_expert: 447 }], ..Default::default() },
            HashMap::new());
        assert_eq!(idx2.locate_expert("k3", 5, 500), ExpertLocation::Fault);
    }

    // what this catches: prefix-aware routing must pick the LONGEST warm match
    // (most prefill skipped), not just any match.
    #[test]
    fn best_prefix_holder_prefers_longest() {
        let local = NodeContent {
            warm_prefixes: vec![WarmPrefix { model_id: "m".into(), hash: 7, token_len: 100 }],
            ..Default::default()
        };
        let mut peers = HashMap::new();
        peers.insert(peer(3), NodeContent {
            warm_prefixes: vec![WarmPrefix { model_id: "m".into(), hash: 7, token_len: 500 }],
            ..Default::default()
        });
        let idx = GridContentIndex::new(local, peers);
        assert_eq!(idx.best_prefix_holder("m", 7), Some((Some(peer(3)), 500)));
        assert_eq!(idx.best_prefix_holder("m", 999), None); // no warm match -> prefill local
    }
}
