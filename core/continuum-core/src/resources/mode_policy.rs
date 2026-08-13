//! `mode_policy` — operating MODES as pure lease policy (#395).
//!
//! An operating mode ("maximize benchmark capacity", "dev sim", "balanced") is NOT
//! new plumbing on top of the [`ResourceGovernor`](crate::resources::ResourceGovernor).
//! It is a CONFIGURATION of the lease primitives that already exist: per-consumer
//! reservation floors ([`ResourceDaemon::reserve`](crate::resources::ResourceDaemon::reserve)),
//! `serving_fraction`, and [`ReclaimPolicy`](crate::resources::ReclaimPolicy). This module
//! is the pure decision function that turns a mode + a LOCAL view of the resident
//! consumers into the floors the governor should hold. Nothing here touches the daemon,
//! the board, or any I/O — the caller applies the result via `reserve(...)`.
//!
//! # Coordinator-free by construction (the box → grid → P2P constraint)
//!
//! [`GovernorMode::floors`] is `fn(&[ConsumerDemand]) -> Vec<PolicyFloor>` — a pure
//! function of a LOCAL view. It never reads a global authority, so the SAME object runs
//! per-node on one box today, composes into the grid market next, and drops into the P2P
//! "wireless" arbiter (radios sharing a finite medium, no central controller) with no
//! rewrite. A local peer that decides from what it can see is exactly what a coordinator-
//! free protocol needs. Do NOT introduce a "the governor knows everything" dependency
//! here — that is the thing that would force a rewrite at the grid/P2P boundary.
//!
//! # Cost is always present, even when donated
//!
//! Every [`PolicyFloor`] carries a [`Price`]. Compute has a real cost basis (energy, wear,
//! opportunity) and every allocation is, in principle, an exchange. "Free" / "donated" is
//! NOT the absence of cost — it is a transaction at [`Price::FREE`] where the owner bears
//! (gifts) the cost. So the cost term exists from day one (the market / alt-coin settlement
//! layer slots in later as a pricing policy, no rewrite); the initial modes just price at
//! zero among trusted peers, and a [`ConsumerDemand::gift`] is always free regardless of
//! mode. You can always answer "what did this cost, and who paid" — a gift answers "the
//! owner". This is honest accounting, not the economy.
//!
//! # Roles, not id string-matching
//!
//! Policy switches on a consumer's [`ConsumerRole`], never on its id string (the `#70`
//! smell). A new consumer joins by declaring its role; the modes keep working with no edit.

use crate::resources::ResourceKind;

/// What KIND of consumer this is, for policy purposes — the axis a mode reasons over.
/// Distinct from the consumer's string id (which is identity, not policy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConsumerRole {
    /// The base-model serving lane(s) — the persona brain AND the benchmark lane share
    /// this role on the VRAM axis (they are one VRAM consumer; benchmark-vs-persona is
    /// differentiated by the throughput-lease layer, not by VRAM floors).
    Serving,
    /// The recall embedding lane — cognition needs it every turn.
    Embedding,
    /// A vision / VL lane (image understanding). Optional for text-only workloads.
    Vision,
    /// A realtime media consumer — a live call, an avatar render (latency-critical).
    Realtime,
    /// Anything else that leases (a game, a batch render) — squeezed first under pressure.
    Other,
}

/// The price the arbiter attaches to an allocation. Always present; `FREE` means the
/// owner gifted the cost (the transaction is still real, its price is just zero).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Price {
    /// Units of account per byte-second (or whatever the settlement layer later fixes).
    /// `0.0` = free / donated. The market / alt-coin layer changes only THIS number.
    pub per_unit: f64,
}

impl Price {
    /// A gifted / donated allocation — the cost is borne by the owner, the price is zero.
    pub const FREE: Price = Price { per_unit: 0.0 };
}

/// A LOCAL view of one resident consumer, as this node sees it right now — the input the
/// policy decides from. Deliberately carries no handle to any global authority.
#[derive(Debug, Clone)]
pub struct ConsumerDemand {
    /// The consumer's stable id (`"serving"`, `"embed"`, a VL lane id, …) — used only to
    /// key the resulting floor back onto `reserve(...)`, never to decide policy.
    pub id: String,
    /// The policy axis this consumer sits on.
    pub role: ConsumerRole,
    /// The resource this demand is for (VRAM today; the same policy generalizes to RAM/disk).
    pub kind: ResourceKind,
    /// Its measured resident footprint in bytes — the floor a mode grants when it protects
    /// this consumer. Measured, never a magic constant (that is the honest-accounting rule).
    pub footprint_bytes: u64,
    /// This consumer's capacity is donated: its owner bears the cost, so its price is always
    /// [`Price::FREE`] regardless of mode.
    pub gift: bool,
}

/// The policy's decision for one consumer: the VRAM floor the governor should hold for it
/// under the active mode, plus the price. Maps 1:1 onto
/// `ResourceDaemon::reserve(consumer_id, kind, floor_bytes)`.
#[derive(Debug, Clone, PartialEq)]
pub struct PolicyFloor {
    pub consumer_id: String,
    pub kind: ResourceKind,
    /// The reservation floor to hold. `0` means "protect nothing — this consumer's bytes
    /// are reclaimable for whoever the mode favors" (its `Graceful` lease can be evicted).
    pub floor_bytes: u64,
    pub price: Price,
}

/// The operating mode — a named lease policy. This is the dial a human, a `PowerMode`, or
/// (later) a learned/market policy turns; the governor cannot tell them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GovernorMode {
    /// Everyone keeps their measured footprint — no consumer is starved. The friendly
    /// default: the box serves its whole population fairly.
    #[default]
    Balanced,
    /// Maximize benchmark capacity: protect the serving lane's full footprint, drop every
    /// OTHER consumer's floor to zero so their `Graceful` leases reclaim and the GPU frees
    /// for the run. Vision + embedding + media all yield to the benchmark.
    BenchmarkMax,
    /// Development simulation: protect serving AND embedding (recall is load-bearing for a
    /// coding persona's cognition every turn), but yield vision + realtime + other.
    DevSim,
}

impl GovernorMode {
    /// The pure decision. Given a LOCAL view of the resident consumers, return the floor +
    /// price the governor should hold for each under this mode. Order-preserving; total
    /// (every input gets exactly one output). No global state, no I/O.
    pub fn floors(self, demands: &[ConsumerDemand]) -> Vec<PolicyFloor> {
        demands
            .iter()
            .map(|d| PolicyFloor {
                consumer_id: d.id.clone(),
                kind: d.kind,
                floor_bytes: if self.protects(d.role) { d.footprint_bytes } else { 0 },
                // Cost is always present; a gifted consumer is free regardless of mode, and
                // the initial modes price everything at zero (free among trusted peers).
                // The market / alt-coin settlement layer replaces ONLY this value.
                price: if d.gift { Price::FREE } else { self.price_for(d.role) },
            })
            .collect()
    }

    /// Does this mode PROTECT (hold the full footprint of) a consumer with this role, or
    /// yield it (floor 0, reclaimable)?
    fn protects(self, role: ConsumerRole) -> bool {
        match self {
            // Everyone protected — nobody starved.
            GovernorMode::Balanced => true,
            // Only the serving lane; everything else yields to the benchmark.
            GovernorMode::BenchmarkMax => role == ConsumerRole::Serving,
            // Serving + recall embedding; vision / realtime / other yield.
            GovernorMode::DevSim => matches!(role, ConsumerRole::Serving | ConsumerRole::Embedding),
        }
    }

    /// The price this mode attaches to a (non-gifted) consumer's allocation. All current
    /// modes are FREE — the cost term is present and honest, but nothing charges yet; the
    /// market / alt-coin layer is a later pricing policy that changes only this.
    fn price_for(self, _role: ConsumerRole) -> Price {
        Price::FREE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demand(id: &str, role: ConsumerRole, bytes: u64, gift: bool) -> ConsumerDemand {
        ConsumerDemand { id: id.into(), role, kind: ResourceKind::Vram, footprint_bytes: bytes, gift }
    }

    /// A representative LAN-party box: a serving lane, the recall embed lane, a VL vision
    /// lane, and a game — the exact co-residency that wedged Devstral live (#385/#395).
    fn box_demands() -> Vec<ConsumerDemand> {
        vec![
            demand("serving", ConsumerRole::Serving, 22_000, false),
            demand("embed", ConsumerRole::Embedding, 1_800, false),
            demand("vl", ConsumerRole::Vision, 2_800, false),
            demand("game", ConsumerRole::Other, 4_000, false),
        ]
    }

    fn floor_of<'a>(fs: &'a [PolicyFloor], id: &str) -> &'a PolicyFloor {
        fs.iter().find(|f| f.consumer_id == id).expect("consumer present in output")
    }

    // what this catches (#395): benchmark-max must protect ONLY the serving lane and drop
    // every other consumer's floor to 0 so their Graceful leases reclaim — that is how the
    // mode frees the GPU for the run. A regression that kept VL/embed/game floors would
    // leave the run oversubscribed (the live wedge this targets).
    #[test]
    fn benchmark_max_protects_only_serving_and_yields_the_rest() {
        let fs = GovernorMode::BenchmarkMax.floors(&box_demands());
        assert_eq!(floor_of(&fs, "serving").floor_bytes, 22_000, "serving keeps its full footprint");
        assert_eq!(floor_of(&fs, "embed").floor_bytes, 0, "embed yields to the benchmark");
        assert_eq!(floor_of(&fs, "vl").floor_bytes, 0, "vision yields to the benchmark");
        assert_eq!(floor_of(&fs, "game").floor_bytes, 0, "the game yields to the benchmark");
    }

    // what this catches: dev-sim protects serving AND recall embedding (a coding persona
    // needs recall every turn) but still yields vision + other — the distinction from
    // benchmark-max lives in which NON-serving consumers survive.
    #[test]
    fn dev_sim_protects_serving_and_embedding_but_yields_vision_and_other() {
        let fs = GovernorMode::DevSim.floors(&box_demands());
        assert_eq!(floor_of(&fs, "serving").floor_bytes, 22_000);
        assert_eq!(floor_of(&fs, "embed").floor_bytes, 1_800, "recall survives in dev-sim");
        assert_eq!(floor_of(&fs, "vl").floor_bytes, 0, "no vision in a coding sim");
        assert_eq!(floor_of(&fs, "game").floor_bytes, 0);
    }

    // what this catches: the default mode starves nobody — every consumer keeps its
    // measured footprint (the friendly whole-population default).
    #[test]
    fn balanced_protects_every_consumers_full_footprint() {
        let fs = GovernorMode::default().floors(&box_demands());
        assert_eq!(GovernorMode::default(), GovernorMode::Balanced);
        for d in box_demands() {
            assert_eq!(floor_of(&fs, &d.id).floor_bytes, d.footprint_bytes, "{} kept whole", d.id);
        }
    }

    // what this catches: the floor is exactly the MEASURED footprint, never a magic
    // constant — protect a consumer and its floor equals what it actually holds (honest
    // accounting; the thing that would drift if someone hardcoded a number).
    #[test]
    fn protected_floor_equals_measured_footprint_not_a_constant() {
        let one = vec![demand("serving", ConsumerRole::Serving, 13_579, false)];
        assert_eq!(GovernorMode::Balanced.floors(&one)[0].floor_bytes, 13_579);
    }

    // what this catches (cost-is-always-present, even donated): every floor carries a
    // Price; the current modes price at FREE; and a GIFTED consumer is FREE regardless of
    // mode — the exchange is real, its price is just zero and the owner bears the cost.
    #[test]
    fn cost_is_present_and_a_gift_is_free_regardless_of_mode() {
        let donated = vec![
            demand("serving", ConsumerRole::Serving, 22_000, false),
            demand("friends-gpu", ConsumerRole::Serving, 8_000, true), // a friend lent their box
        ];
        for mode in [GovernorMode::Balanced, GovernorMode::BenchmarkMax, GovernorMode::DevSim] {
            let fs = mode.floors(&donated);
            assert_eq!(floor_of(&fs, "friends-gpu").price, Price::FREE, "a gift is always free ({mode:?})");
            assert_eq!(floor_of(&fs, "serving").price, Price::FREE, "current modes price free ({mode:?})");
        }
    }

    // what this catches (coordinator-free, box→grid→P2P): the decision is a pure function
    // of the LOCAL view — same input always yields the same output, no hidden global
    // state. A regression that reached for a global authority would make this
    // non-deterministic across processes and break the P2P wireless-arbiter reuse.
    #[test]
    fn decision_is_a_pure_function_of_the_local_view() {
        let d = box_demands();
        assert_eq!(GovernorMode::BenchmarkMax.floors(&d), GovernorMode::BenchmarkMax.floors(&d));
        assert_eq!(GovernorMode::BenchmarkMax.floors(&[]), Vec::<PolicyFloor>::new());
    }
}
