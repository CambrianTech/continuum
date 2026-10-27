//! Grid node registry — tracks known nodes, their capabilities, and trust levels.
//!
//! Persisted to .continuum/grid/known_nodes.json.
//! Updated by discovery, manual pairing, and periodic health checks.

use continuum_base::time::EpochMs;
use super::node::{DiscoveredNode, GridNode, TransportAddress};
use crate::routing::trust_level::TrustLevel;
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

/// One fleet liveness transition, for a probe and one room line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetTransition {
    pub node: String,
    pub kind: FleetChange,
    pub silent_secs: u64,
    pub build_sha: Option<String>,
    /// The peer's beaconed build number (0 = never beaconed one) — the value the
    /// behind/caught-up verdict was computed from, so the line can say it.
    pub build_number: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FleetChange {
    WentStale,
    BackFresh,
    FellBehind,
    CaughtUp,
}

impl FleetChange {
    pub fn as_str(self) -> &'static str {
        match self {
            FleetChange::WentStale => "went_stale",
            FleetChange::BackFresh => "back_fresh",
            FleetChange::FellBehind => "fell_behind",
            FleetChange::CaughtUp => "caught_up",
        }
    }
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
        let cutoff = EpochMs::now().millis().saturating_sub(5 * 60 * 1000);
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
                existing.last_seen = EpochMs::now().millis();
            })
            .or_insert_with(|| GridNode {
                node_id,
                node_name: discovered.name,
                addresses: vec![discovered.address],
                capabilities: discovered.capabilities,
                trust_level: TrustLevel::default(), // Blocked until manually trusted
                last_seen: EpochMs::now().millis(),
                latency_ms: None,
                peer_id: None, // learned later via set_peer_id (pairing/gossip correlation, #2228)
                build_sha: None,
                build_number: 0,
                served_model: None,
                lanes: 0,
                residents: 0,
                resident_alarms: Vec::new(),
                silent_secs: 0,
                stale: false,
                behind: false,
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

    /// Record the build a peer's beacon reported (and that it was just heard).
    pub fn note_peer_build(&self, peer: &PeerId, build_sha: Option<String>, build_number: u64, heard_at_ms: u64) {
        self.fold_peer(peer, build_sha, build_number, None, heard_at_ms);
    }

    /// Fold one heard beacon into the node: its build, what it serves, and the residents it
    /// judged stalled or regressed (card 1c157668), so `grid/nodes` on any node names them.
    pub fn note_peer_beacon(&self, peer: &PeerId, offer: &crate::capacity::gossip::CapacityOffer, heard_at_ms: u64) {
        let build_sha = crate::capacity::gossip::build_hex(offer.build);
        self.fold_peer(peer, build_sha, offer.build_number, Some(offer), heard_at_ms);
    }

    fn fold_peer(
        &self,
        peer: &PeerId,
        build_sha: Option<String>,
        build_number: u64,
        offer: Option<&crate::capacity::gossip::CapacityOffer>,
        heard_at_ms: u64,
    ) {
        for mut r in self.nodes.iter_mut() {
            if r.value().peer_id.as_ref() == Some(peer) {
                let n = r.value_mut();
                if build_sha.is_some() {
                    n.build_sha = build_sha.clone();
                }
                if build_number != 0 {
                    n.build_number = build_number;
                }
                if let Some(offer) = offer {
                    if offer.served_model.is_some() {
                        n.served_model = offer.served_model.clone();
                        n.lanes = offer.lanes;
                        n.residents = offer.residents;
                    }
                    // Every beacon is the node's current judgement: an empty list clears the
                    // last one (her stall ended, or the node no longer hosts her).
                    n.resident_alarms = offer.resident_alarms.clone();
                }
                n.last_seen = n.last_seen.max(heard_at_ms);
            }
        }
    }

    /// THE FLEET FOLD: refresh every node's `silent_secs` / `stale` / `behind` from
    /// `now` and this node's own build; returns the transitions (a node going stale
    /// or fresh, falling behind or catching up) so the caller probes and says each
    /// ONCE, never per tick. `stale` is relative to `silent_after_ms`.
    pub fn fold_liveness(&self, now_ms: u64, silent_after_ms: u64, local_build_number: u64) -> Vec<FleetTransition> {
        let mut out = Vec::new();
        for mut r in self.nodes.iter_mut() {
            let n = r.value_mut();
            // Only a node that has ever BEACONED a build or has a transport address is
            // a fleet member the fold may judge. A bare registry row (a seat registered
            // by hand, an id with no addresses) is not a node that can go silent —
            // 2026-09-14 the first live fold flagged this node's OWN operator seat as
            // "silent 828 h — treat as DOWN" in the org room.
            if n.build_sha.is_none() && n.addresses.is_empty() {
                continue;
            }
            let silent = now_ms.saturating_sub(n.last_seen);
            n.silent_secs = silent / 1000;
            let stale = silent > silent_after_ms;
            // BEHIND is a comparison of build NUMBERS (monotonic), never of shas: a
            // different sha may be AHEAD. Unknown (0) on either side = not judged.
            let behind = n.build_number != 0 && local_build_number != 0 && n.build_number < local_build_number;
            let label = n.node_name.clone().unwrap_or_else(|| n.node_id.clone()); // JUSTIFIED unwrap_or_else: an unnamed node is named by its id
            if stale != n.stale {
                out.push(FleetTransition { node: label.clone(), kind: if stale { FleetChange::WentStale } else { FleetChange::BackFresh }, silent_secs: n.silent_secs, build_sha: n.build_sha.clone(), build_number: n.build_number });
                n.stale = stale;
            }
            if behind != n.behind {
                out.push(FleetTransition { node: label, kind: if behind { FleetChange::FellBehind } else { FleetChange::CaughtUp }, silent_secs: n.silent_secs, build_sha: n.build_sha.clone(), build_number: n.build_number });
                n.behind = behind;
            }
        }
        out
    }

    /// Ensure a grid node exists for a peer known ONLY by its durable airc `PeerId` — the
    /// automatic path: a capacity beacon arrives over airc carrying the authenticated PeerId
    /// (no transport address), and the grid FIGURES OUT the node identity from it rather than
    /// waiting for a manual `grid/pair` (#2228). If a node already carries this `peer_id` this
    /// is a no-op; otherwise a node is self-registered keyed by the PeerId (a beaconing peer
    /// IS a grid node, identity-first). Trust is left at the DEFAULT — discovery is not
    /// authorization, so the node is visible to `get_by_peer`/pricing but is not sent work
    /// until the trust bridge (#38) elevates it. `vram_mb` seeds a Compute capability from the
    /// offer so the router can weigh it. Returns true when a node was newly created.
    pub fn ensure_peer_node(&self, peer: PeerId, vram_mb: Option<u64>) -> bool {
        if self.get_by_peer(&peer).is_some() {
            return false;
        }
        let node_id = peer.to_string();
        let capabilities = match vram_mb {
            Some(mb) => vec![super::node::NodeCapability::Compute {
                gpu: None,
                vram_mb: Some(mb),
            }],
            None => vec![],
        };
        let mut created = false;
        self.nodes.entry(node_id.clone()).or_insert_with(|| {
            created = true;
            GridNode {
                node_id,
                node_name: None,
                addresses: vec![],
                capabilities,
                trust_level: TrustLevel::default(), // discovery ≠ authorization (#38)
                last_seen: EpochMs::now().millis(),
                latency_ms: None,
                peer_id: Some(peer),
                build_sha: None,
                build_number: 0,
                served_model: None,
                lanes: 0,
                residents: 0,
                resident_alarms: Vec::new(),
                silent_secs: 0,
                stale: false,
                behind: false,
            }
        });
        created
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
                r.value_mut().last_seen = EpochMs::now().millis();
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
            r.value_mut().last_seen = EpochMs::now().millis();
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
// matters). Legacy address-routing trust is not authenticated AIRC authority.
// The production AIRC gate reads PeerId decisions from access_decision; grid/nodes
// exposes that separate local policy projection. Discovery defaults are never
// imported as explicit access decisions.

/// Derive a canonical node_id from a transport address.
fn address_to_node_id(address: &TransportAddress) -> String {
    match address {
        TransportAddress::Tailscale { ip, .. } => ip.clone(),
        TransportAddress::Reticulum { destination_hash } => destination_hash.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::node::NodeCapability;
    use super::*;


    /// what this catches: a node nine days silent (or on an old build) reading as a
    /// live, current peer — the fold must flag it ONCE and unflag it once when it
    /// returns, never a transition per tick.
    #[test]
    fn a_bare_registry_row_is_never_judged_by_the_fold() {
        let dir = std::env::temp_dir().join(format!("grid-fleet-bare-{}", uuid::Uuid::new_v4()));
        let reg = NodeRegistry::new(&dir);
        let peer = PeerId::from_uuid(uuid::Uuid::from_u128(11));
        assert!(reg.ensure_peer_node(peer, None));
        let t0 = EpochMs::now().millis();
        let six_h = 6 * 3600 * 1000;
        assert!(reg.fold_liveness(t0 + six_h * 200, six_h, 5309).is_empty(), "no beacon, no address: not a fleet member");
        reg.note_peer_build(&peer, Some("8d1282271".into()), 5249, t0);
        assert_eq!(reg.fold_liveness(t0 + 1000, six_h, 5309).len(), 1, "once it beacons, it is judged");
    }

    // what this catches: automatic placement's input — a beacon's served model, lanes and
    // residents land on the node (grid/nodes shows them), and an older beacon without
    // them leaves the last known values alone.
    #[test]
    fn a_beacon_reports_what_the_node_serves() {
        let dir = std::env::temp_dir().join(format!("grid-serves-{}", uuid::Uuid::new_v4()));
        let reg = NodeRegistry::new(&dir);
        let peer = PeerId::from_uuid(uuid::Uuid::from_u128(21));
        assert!(reg.ensure_peer_node(peer, None));
        let t0 = EpochMs::now().millis();
        // Kimi on the 5090, 2026-10-26: refused before inference every turn, on the beacon.
        let kimi = serde_json::json!({
            "persona_id": uuid::Uuid::from_u128(0x5090), "name": "Kimi", "state": { "kind": "stalled" },
            "window_start_ms": t0, "window_end_ms": t0 + 3_600_000, "demand": 12, "completions": 0,
            "refusals": 12, "parked": true,
            "refusal": { "class": "prompt_capacity", "required_tokens": 26_101, "budget_tokens": 17_829, "context_window": 49_152 },
        });
        let beacon = |alarms: serde_json::Value| -> crate::capacity::gossip::CapacityOffer {
            serde_json::from_value(serde_json::json!({
                "gpuTotalBytes": 1, "gpuFreeBytesLive": 0, "systemRamFreeBytes": 0, "atMs": t0,
                "build": 0x269cefdec_u64, "buildNumber": 5322, "servedModel": "Qwen3.8-27B", "lanes": 2,
                "residents": 0, "residentAlarms": alarms,
            }))
            .expect("the wire shape of a seat's beacon")
        };
        reg.note_peer_beacon(&peer, &beacon(serde_json::json!([kimi])), t0);
        let n = reg.all_nodes().into_iter().find(|n| n.peer_id == Some(peer)).expect("node");
        assert_eq!((n.served_model.as_deref(), n.lanes, n.residents), (Some("Qwen3.8-27B"), 2, 0));
        assert_eq!(n.build_sha.as_deref(), Some("269cefdec"));
        // what this catches (card 1c157668): another node's stalled resident shows in grid/nodes.
        assert_eq!(n.resident_alarms.iter().map(|a| a.line()).collect::<Vec<_>>(),
            vec!["Kimi stalled: 12 asked, 0 completed, prompt capacity 26101 > 17829 at context 49152, parked".to_string()]);
        reg.note_peer_build(&peer, Some("269cefdec".into()), 5322, t0 + 1000);
        let n = reg.all_nodes().into_iter().find(|n| n.peer_id == Some(peer)).expect("node");
        assert_eq!(n.lanes, 2, "an older beacon shape leaves the served facts alone");
        assert_eq!(n.resident_alarms.len(), 1, "a build-only fold leaves the judgement alone");
        reg.note_peer_beacon(&peer, &beacon(serde_json::json!([])), t0 + 2000);
        let n = reg.all_nodes().into_iter().find(|n| n.peer_id == Some(peer)).expect("node");
        assert!(n.resident_alarms.is_empty(), "the next beacon's judgement replaces the last");
    }

    /// what this catches: a peer on a NEWER build called "behind" because its sha
    /// differs (both nodes told each other to reboot, 2026-09-14).
    #[test]
    fn a_peer_on_a_higher_build_number_is_not_behind() {
        let dir = std::env::temp_dir().join(format!("grid-fleet-ahead-{}", uuid::Uuid::new_v4()));
        let reg = NodeRegistry::new(&dir);
        let peer = PeerId::from_uuid(uuid::Uuid::from_u128(13));
        assert!(reg.ensure_peer_node(peer, None));
        let t0 = EpochMs::now().millis();
        reg.note_peer_build(&peer, Some("5109f2bba".into()), 5310, t0);
        assert!(reg.fold_liveness(t0 + 1000, 6 * 3600 * 1000, 5309).is_empty(), "a higher number is ahead, not behind");
        reg.note_peer_build(&peer, Some("ce3f6cebc".into()), 5308, t0 + 2000);
        assert_eq!(reg.fold_liveness(t0 + 3000, 6 * 3600 * 1000, 5309).len(), 1, "a lower number is behind");
    }

    #[test]
    fn the_fleet_fold_flags_silence_and_drift_once_each_way() {
        let dir = std::env::temp_dir().join(format!("grid-fleet-fold-{}", uuid::Uuid::new_v4()));
        let reg = NodeRegistry::new(&dir);
        let peer = PeerId::from_uuid(uuid::Uuid::from_u128(7));
        assert!(reg.ensure_peer_node(peer, None));
        // The registry stamps last_seen with the real clock on creation; the fold is
        // relative to that, so the test's clock starts there.
        let t0 = EpochMs::now().millis();
        reg.note_peer_build(&peer, Some("8d1282271".into()), 5249, t0);
        let six_h = 6 * 3600 * 1000;
        let first = reg.fold_liveness(t0 + 1000, six_h, 5309);
        assert_eq!(first.len(), 1, "{first:?}");
        assert_eq!(first[0].kind, FleetChange::FellBehind);
        assert!(reg.fold_liveness(t0 + 2000, six_h, 5309).is_empty(), "no second transition while nothing changed");
        let later = reg.fold_liveness(t0 + six_h + 1, six_h, 5309);
        assert_eq!(later.len(), 1);
        assert_eq!(later[0].kind, FleetChange::WentStale);
        assert!(later[0].silent_secs >= 6 * 3600);
        reg.note_peer_build(&peer, Some("740055750".into()), 5309, t0 + six_h + 5000);
        let back: Vec<FleetChange> = reg.fold_liveness(t0 + six_h + 6000, six_h, 5309).into_iter().map(|t| t.kind).collect();
        assert!(back.contains(&FleetChange::BackFresh) && back.contains(&FleetChange::CaughtUp), "{back:?}");
        let node = reg.get_by_peer(&peer).expect("registered"); // JUSTIFIED: the test registered it
        assert_eq!(node.build_sha.as_deref(), Some("740055750"));
        assert!(!node.stale && !node.behind);
    }
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
        let found = registry
            .get_by_peer(&peer)
            .expect("node resolves by PeerId");
        assert_eq!(
            found.node_id, "100.9.9.9",
            "same node, now reachable by its durable id"
        );
        assert_eq!(
            found.peer_id,
            Some(peer),
            "and it carries the durable identity"
        );

        // Address trust stays untouched; correlated aliases share ONE access decision.
        use super::super::handlers::nodes::GridNodeView;
        use crate::grid::access_decision::{AccessDecision, AccessPolicy, LocalInboundAircAuthorization};
        let mut policy = AccessPolicy::default();
        let unrecorded = GridNodeView::from_policy(found.clone(), &policy);
        assert_eq!(unrecorded.node.trust_level, TrustLevel::Blocked);
        assert_eq!(unrecorded.local_inbound_airc_authorization,
            LocalInboundAircAuthorization::Unrecorded { effective_level: TrustLevel::Provisional });
        for level in [TrustLevel::Trusted, TrustLevel::Blocked] {
            policy.decisions = vec![AccessDecision { peer: peer.as_uuid(), level,
                reason: "fixture decision".into(), decided_by: None, decided_ms: 1 }];
            let first = GridNodeView::from_policy(found.clone(), &policy);
            let mut alias = found.clone();
            alias.node_id = peer.to_string();
            let second = GridNodeView::from_policy(alias, &policy);
            assert_eq!(first.local_inbound_airc_authorization, second.local_inbound_airc_authorization);
            assert_eq!(first.local_inbound_airc_authorization,
                LocalInboundAircAuthorization::Recorded { effective_level: level, decision: policy.decisions[0].clone() });
            let mut uncorrelated = found.clone();
            uncorrelated.peer_id = None;
            assert_eq!(GridNodeView::from_policy(uncorrelated, &policy).local_inbound_airc_authorization,
                LocalInboundAircAuthorization::Uncorrelated);
            let before = serde_json::to_value(&found).expect("legacy node");
            let after = serde_json::to_value(&first).expect("node view");
            for (key, value) in before.as_object().expect("node object") {
                assert_eq!(after.get(key), Some(value), "legacy field {key} must not change");
            }
            assert!(after.get("local_inbound_airc_authorization").is_some());
        }

        // Correlating an UNKNOWN node fails loud — never silently invents a node.
        let ghost = PeerId::from_uuid(uuid::Uuid::from_u128(0xdead));
        assert!(
            registry.set_peer_id("100.0.0.0", ghost).is_err(),
            "correlating an unknown node fails loud"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: THE AUTOMATIC PATH — a beaconing peer known ONLY by its PeerId becomes
    // a routable grid node with no manual pairing (#2228). ensure_peer_node self-registers it
    // (identity-first), is idempotent (a re-beacon is a no-op), carries the beacon's advertised
    // compute, and leaves trust at the default so discovery does not grant authorization (#38).
    #[test]
    fn ensure_peer_node_auto_registers_a_beaconing_peer_by_identity() {
        let dir = std::env::temp_dir().join("grid-test-ensure-peer");
        let _ = std::fs::remove_dir_all(&dir);
        let registry = NodeRegistry::new(&dir);
        let peer = PeerId::from_uuid(uuid::Uuid::from_u128(0xbeac04));

        assert!(
            registry.get_by_peer(&peer).is_none(),
            "unknown before any beacon"
        );
        assert!(
            registry.ensure_peer_node(peer, Some(32768)),
            "first beacon self-registers the peer"
        );

        let node = registry
            .get_by_peer(&peer)
            .expect("now routable by durable identity");
        assert_eq!(node.peer_id, Some(peer));
        // NOT `!= Owner`. That also passes at Trusted, which is the exact bar
        // `router::find_gpu_node` admits compute candidates on — so the weaker form
        // would stay green while every beaconing stranger became routable. The
        // danger is the second rung from the top, not the top.
        assert!(
            node.trust_level < TrustLevel::Trusted,
            "a beaconing stranger must not clear the router's admission bar — \
             discovery is not authorization"
        );
        assert!(
            !node.capabilities.is_empty(),
            "carries the beacon's advertised compute"
        );

        assert!(
            !registry.ensure_peer_node(peer, Some(32768)),
            "a re-beacon from the same peer is a no-op"
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
