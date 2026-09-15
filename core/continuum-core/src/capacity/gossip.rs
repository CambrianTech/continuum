//! Grid capacity gossip (#56 step 4) — the bridge from the SIMULATED grid to the real one.
//!
//! Every node periodically broadcasts a [`CapacityOffer`] — its live, ever-changing usable
//! compute — over airc as an `EphemeralCoalesced` realtime envelope (presence-of-compute:
//! latest wins, never replayed; a stale capacity reading is a lie, so it must not outlive
//! its freshness). Every node also LISTENS, folding heard offers into the process-global
//! [`GridCapacityLedger`], whose [`GridCapacityLedger::snapshot`] projects the exact
//! [`GridSnapshot`] the simulator's placement policies are proven against
//! (`capacity::grid`, 40-peer BitTorrent churn, partition/join/death invariants). Sim == prod
//! at the WORLD seam now, not just the policy seam.
//!
//! ## Identity is the wire's, never the payload's
//!
//! The ledger keys offers on the transcript event's `peer_id` — airc's authenticated
//! transport identity — NOT on any id the payload declares. A peer cannot gossip capacity
//! on behalf of another peer; there is no self-declared identity to spoof. (The payload
//! carries only the capacity numbers.)
//!
//! ## Reachability = freshness
//!
//! The sim's `PeerCapacity.reachable` maps to offer AGE: a peer whose last offer is older
//! than the freshness window is present-but-unreachable (exactly the sim's mid-lease death
//! shape — its lanes reclaim on the next placement), and one silent past the eviction
//! window drops from the snapshot entirely. No connection state machine: liveness is
//! demonstrated by speaking, the same way BitTorrent peers prove themselves by serving.
//!
//! ## Loopback is the first proof
//!
//! A node's own offer round-trips through the daemon and lands in its own ledger — the
//! one-node grid. `snapshot()` recognizes the caller's own peer id and EXCLUDES it from the
//! peers list (the local device is passed in live; the echoed offer would double-count it).
//! Seeing your own row refresh IS the verification that the publish→hear pipeline works,
//! which makes adding the next machine an `airc join`, not a code change.

use std::sync::OnceLock;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::grid::{GridSnapshot, PeerCapacity};
use super::DeviceCapacity;
use crate::identity::PeerId;

/// One node's broadcast capacity reading — the wire payload (inline JSON in the
/// `grid_capacity` realtime envelope). Numbers only; identity comes from the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapacityOffer {
    /// Total GPU / UMA-serving-slice bytes on the offering device.
    pub gpu_total_bytes: u64,
    /// Free GPU bytes on the offering device RIGHT NOW (net of everything resident).
    pub gpu_free_bytes_live: u64,
    /// Free system RAM — the CPU-serve fallback budget.
    pub system_ram_free_bytes: u64,
    /// Sender's clock when the reading was taken (ms since epoch). Displayed, not
    /// trusted: freshness is judged by RECEIVER clock at hear-time.
    pub at_ms: u64,
    /// The sender's running build (the 9-hex git sha as an integer; 0 = unknown, a
    /// beacon from a core older than this field). FLEET DRIFT IS A RECEIPT
    /// (2026-09-14, Joel: "it has probably been days and you never noticed" — a node ran
    /// a nine-day-old core and nothing said so): every beacon carries the build, so
    /// every node knows who is behind tip without anyone reading a room.
    #[serde(default)]
    pub build: u64,
    /// The sender's monotonic build number (CONTINUUM_BUILD_NUMBER; 0 = unknown).
    /// "Behind" is a comparison of NUMBERS, never of shas: the first cross-node fold
    /// (2026-09-14 16:5xZ) had both nodes call each other "behind tip" because their
    /// shas differed, and the M5 was the one ahead.
    #[serde(default)]
    pub build_number: u64,
    /// WHAT THIS NODE SERVES (2026-09-15, automatic placement stage A): the served
    /// base model, the warm lanes, and the minds resident here. A starved node reads
    /// these to place its tail minds on a peer with lanes to spare — no ids typed.
    /// Absent on an older beacon (defaults): the peer offers nothing placeable.
    #[serde(default)]
    pub served_model: Option<String>,
    #[serde(default)]
    pub lanes: u32,
    #[serde(default)]
    pub residents: u32,
}

/// The 9-hex build sha prefix as the integer a beacon carries (0 when unparsable).
pub fn build_from_sha(sha: &str) -> u64 {
    let hex: String = sha.chars().take(9).collect();
    u64::from_str_radix(&hex, 16).unwrap_or(0) // JUSTIFIED unwrap_or: a non-hex build string beacons as "unknown", never a fake sha
}

/// The beacon's build back as the 9-hex prefix operators recognise.
pub fn build_hex(build: u64) -> Option<String> {
    (build != 0).then(|| format!("{build:09x}"))
}

impl CapacityOffer {
    pub fn capacity(&self) -> DeviceCapacity {
        DeviceCapacity {
            gpu_total_bytes: self.gpu_total_bytes,
            gpu_free_bytes_live: self.gpu_free_bytes_live,
            system_ram_free_bytes: self.system_ram_free_bytes,
        }
    }
}

/// A heard offer + the receiver-clock instant it arrived (the freshness anchor).
#[derive(Debug, Clone)]
struct HeardOffer {
    offer: CapacityOffer,
    heard_at_ms: u64,
}

/// Peers older than this are present-but-UNREACHABLE in the snapshot (the sim's
/// mid-lease-death shape): 3× the publish cadence, so one dropped gossip beat never
/// flaps reachability ([[never-thrash-sticky-hysteresis-on-every-lane]]).
pub const FRESHNESS_WINDOW_MS: u64 = 3 * PUBLISH_INTERVAL_MS;

/// Peers silent past this drop from the snapshot entirely (left the grid, not just
/// mid-hiccup). 12× cadence — two minutes of silence at the 10s beat.
pub const EVICTION_WINDOW_MS: u64 = 12 * PUBLISH_INTERVAL_MS;

/// The gossip heartbeat. Capacity is a live quantity; 10s tracks a game launching
/// within one hysteresis window of the prefill valve while staying far below any
/// pressure-relevant bandwidth (one tiny coalesced envelope per beat).
pub const PUBLISH_INTERVAL_MS: u64 = 10_000;

/// Minds seated on OTHER fresh nodes right now — [`GridCapacityLedger::residents_elsewhere`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RosterHeard {
    pub peers: usize,
    pub residents: u32,
    pub newest_heard_at_ms: u64,
}

/// Process-global ledger of heard capacity offers, keyed by the WIRE's peer id.
#[derive(Default)]
pub struct GridCapacityLedger {
    heard: DashMap<Uuid, HeardOffer>,
}

/// The one process-global ledger — the resource it mirrors (this node's view of the
/// grid) is process-global, same granularity argument as the admission gates.
pub fn global_ledger() -> &'static GridCapacityLedger {
    static LEDGER: OnceLock<GridCapacityLedger> = OnceLock::new();
    LEDGER.get_or_init(GridCapacityLedger::default)
}

impl GridCapacityLedger {
    /// Fold one heard offer in (latest per peer wins — capacity is a live reading).
    /// `from_peer` is the transcript event's transport identity, never payload-declared.
    /// Returns `true` when this peer is NEW to the ledger (first offer heard) — the
    /// probe-on-join surface; steady re-offers stay silent.
    pub fn hear(&self, from_peer: Uuid, offer: CapacityOffer, heard_at_ms: u64) -> bool {
        self.heard
            .insert(from_peer, HeardOffer { offer, heard_at_ms })
            .is_none()
    }

    /// Project the ledger onto the sim-proven [`GridSnapshot`]: the caller's LIVE local
    /// device + every heard peer, reachability derived from offer age. The caller's own
    /// echoed offer is excluded (its device arrives via `local`, live — fresher than any
    /// round-tripped gossip). Evicts peers silent past [`EVICTION_WINDOW_MS`] as it goes.
    pub fn snapshot(&self, own_peer: Uuid, local: DeviceCapacity, now_ms: u64) -> GridSnapshot {
        let mut peers = Vec::new();
        self.heard.retain(|peer, heard| {
            let age = now_ms.saturating_sub(heard.heard_at_ms);
            if age > EVICTION_WINDOW_MS {
                return false; // silent too long — left the grid
            }
            if *peer != own_peer {
                peers.push(PeerCapacity {
                    peer: PeerId::from_uuid(*peer),
                    capacity: heard.offer.capacity(),
                    reachable: age <= FRESHNESS_WINDOW_MS,
                });
            }
            true
        });
        // Deterministic order (DashMap iteration isn't): stable placement traces.
        peers.sort_by_key(|p| p.peer.as_uuid());
        GridSnapshot { local, peers }
    }

    /// What the grid says about minds seated ELSEWHERE, now: fresh peers (heard
    /// within [`FRESHNESS_WINDOW_MS`]) other than `own_peer` whose beacon reports
    /// residents. The one-roster rule's input (card b3b922c0, Joel 2026-09-15:
    /// "start on one computer and add more later … holistically like one machine"):
    /// an empty node that hears a team offers lanes and mints no team of its own.
    /// Own peer unknown (`None`) counts nobody as self — a caller that cannot name
    /// itself must not read its own echoed beacon as a team elsewhere, so it passes
    /// `None` only when the ledger cannot hold its echo.
    pub fn residents_elsewhere(&self, own_peer: Option<Uuid>, now_ms: u64) -> RosterHeard {
        let mut heard = RosterHeard::default();
        for r in self.heard.iter() {
            if Some(*r.key()) == own_peer {
                continue;
            }
            let at = r.value().heard_at_ms;
            if now_ms.saturating_sub(at) > FRESHNESS_WINDOW_MS || r.value().offer.residents == 0 {
                continue;
            }
            heard.peers += 1;
            heard.residents += r.value().offer.residents;
            heard.newest_heard_at_ms = heard.newest_heard_at_ms.max(at);
        }
        heard
    }

    /// Number of peers currently on the ledger (self included if echoed) — probe surface.
    pub fn heard_count(&self) -> usize {
        self.heard.len()
    }

    /// Every peer currently on the ledger with its latest offer — the auto-discovered,
    /// PeerId-keyed capacity a capacity beacon self-registers here. The grid folds these into
    /// the node registry so a beaconing peer becomes a routable node by its DURABLE identity,
    /// with no manual pairing (#2228). A read, not a sweep: staleness is applied by
    /// [`snapshot`](Self::snapshot)'s reachability window, so a briefly-silent peer is not
    /// deregistered here.
    /// Every heard offer with WHEN it was heard — the liveness fold's input.
    pub fn heard_offers_with_age(&self) -> Vec<(Uuid, CapacityOffer, u64)> {
        self.heard
            .iter()
            .map(|r| (*r.key(), r.value().offer.clone(), r.value().heard_at_ms))
            .collect()
    }

    pub fn heard_offers(&self) -> Vec<(Uuid, CapacityOffer)> {
        self.heard
            .iter()
            .map(|r| (*r.key(), r.value().offer.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a beacon from an older core (no served fields) still parses,
    // and reads as offering nothing placeable — a mixed fleet keeps folding.
    #[test]
    fn an_older_beacon_without_served_fields_still_parses() {
        let old = serde_json::json!({"gpuTotalBytes": 1, "gpuFreeBytesLive": 1, "systemRamFreeBytes": 1, "atMs": 5, "build": 7, "buildNumber": 9});
        let o: CapacityOffer = serde_json::from_value(old).expect("parses");
        assert_eq!((o.served_model, o.lanes, o.residents), (None, 0, 0));
    }

    const GB: u64 = 1024 * 1024 * 1024;

    fn offer(free_gb: u64, at_ms: u64) -> CapacityOffer {
        CapacityOffer {
            gpu_total_bytes: 32 * GB,
            gpu_free_bytes_live: free_gb * GB,
            system_ram_free_bytes: 16 * GB,
            at_ms,
                    build: 0,
                    build_number: 0,
                    served_model: None,
                    lanes: 0,
                    residents: 0,
        }
    }
    fn local() -> DeviceCapacity {
        DeviceCapacity {
            gpu_total_bytes: 55 * GB,
            gpu_free_bytes_live: 5 * GB,
            system_ram_free_bytes: 40 * GB,
        }
    }

    // what this catches: THE LOOPBACK CONTRACT — the node's own echoed offer must NOT
    // appear as a peer (the local device arrives live via `local`; counting the echo
    // would double-count this machine's GPU in every placement), while genuinely-other
    // peers DO appear with their heard capacity. This is the one-node-grid proof shape:
    // ledger hears self + one peer, snapshot shows exactly one peer.
    #[test]
    fn own_echoed_offer_is_excluded_and_real_peers_project() {
        let ledger = GridCapacityLedger::default();
        let me = Uuid::from_u128(1);
        let other = Uuid::from_u128(2);
        ledger.hear(me, offer(9, 1_000), 1_000); // our own gossip, round-tripped
        ledger.hear(other, offer(7, 1_000), 1_000);

        let snap = ledger.snapshot(me, local(), 2_000);
        assert_eq!(
            snap.local,
            local(),
            "local device is the LIVE reading, not the echo"
        );
        assert_eq!(
            snap.peers.len(),
            1,
            "self excluded; the real peer projected"
        );
        assert_eq!(snap.peers[0].peer.as_uuid(), other);
        assert_eq!(snap.peers[0].capacity.gpu_free_bytes_live, 7 * GB);
        assert!(snap.peers[0].reachable);
    }

    // what this catches: reachability-from-freshness — the gossip mapping of the sim's
    // three grid invariants. A peer inside the freshness window is reachable; past it,
    // present-but-UNREACHABLE (the sim's mid-lease death: its lanes reclaim on the next
    // placement, zero stranded); silent past the eviction window it leaves the snapshot
    // entirely; and a NEW offer from the same peer restores it instantly (the sim's
    // return/regrow). One dropped beat (age < 3× cadence) must NOT flap reachability.
    #[test]
    fn freshness_drives_reachability_death_and_return() {
        let ledger = GridCapacityLedger::default();
        let me = Uuid::from_u128(1);
        let peer = Uuid::from_u128(2);
        ledger.hear(peer, offer(7, 0), 0);

        // One missed beat: still reachable (no flap).
        let t1 = PUBLISH_INTERVAL_MS * 2;
        assert!(ledger.snapshot(me, local(), t1).peers[0].reachable);

        // Past the freshness window: present but unreachable — reclaim, don't forget.
        let t2 = FRESHNESS_WINDOW_MS + 1;
        let snap = ledger.snapshot(me, local(), t2);
        assert_eq!(snap.peers.len(), 1);
        assert!(
            !snap.peers[0].reachable,
            "stale peer is present-but-unreachable"
        );

        // Silent past eviction: gone from the snapshot.
        let t3 = EVICTION_WINDOW_MS + 1;
        assert!(
            ledger.snapshot(me, local(), t3).peers.is_empty(),
            "evicted after long silence"
        );

        // The peer speaks again: instantly back, reachable — grow is first-class.
        ledger.hear(peer, offer(9, t3), t3);
        let snap = ledger.snapshot(me, local(), t3 + 1);
        assert_eq!(snap.peers.len(), 1);
        assert!(
            snap.peers[0].reachable,
            "a returning peer is adopted on its first offer"
        );
        assert_eq!(snap.peers[0].capacity.gpu_free_bytes_live, 9 * GB);
    }

    // what this catches: the offer payload survives the wire byte-for-byte (serde
    // round-trip, camelCase like every realtime inline payload). If a field renames or
    // a u64 narrows, heard capacities would silently drift from published ones — the
    // grid would place lanes against numbers nobody offered.
    #[test]
    fn offer_round_trips_through_json() {
        let o = offer(7, 123_456);
        let json = serde_json::to_value(&o).unwrap();
        assert!(
            json.get("gpuFreeBytesLive").is_some(),
            "camelCase wire naming: {json}"
        );
        let back: CapacityOffer = serde_json::from_value(json).unwrap();
        assert_eq!(back, o);
    }

    // what this catches: the one-roster rule's input — a fresh peer with residents
    // counts, our own echoed beacon never does, a stale peer's team is not "elsewhere"
    // (its node may be rebooting; the durable memory covers that horizon), and a
    // fresh peer that only offers lanes (residents 0) is capacity, not a team.
    #[test]
    fn residents_elsewhere_counts_fresh_peers_with_minds_and_never_ourselves() {
        let ledger = GridCapacityLedger::default();
        let me = Uuid::from_u128(1);
        let m5 = Uuid::from_u128(2);
        let intel = Uuid::from_u128(3);
        let lanes_only = Uuid::from_u128(4);
        let now = 1_000_000;
        let with = |residents: u32, at: u64| {
            let mut o = offer(10, at);
            o.residents = residents;
            o
        };
        ledger.hear(me, with(16, now), now);
        ledger.hear(m5, with(16, now - 5_000), now - 5_000);
        ledger.hear(intel, with(2, now - FRESHNESS_WINDOW_MS - 1), now - FRESHNESS_WINDOW_MS - 1);
        ledger.hear(lanes_only, with(0, now), now);
        let heard = ledger.residents_elsewhere(Some(me), now);
        assert_eq!(heard, RosterHeard { peers: 1, residents: 16, newest_heard_at_ms: now - 5_000 });
        assert_eq!(
            ledger.residents_elsewhere(Some(me), now + FRESHNESS_WINDOW_MS + 1),
            RosterHeard::default(),
            "once every peer is silent past the window, nobody is elsewhere"
        );
    }
}
