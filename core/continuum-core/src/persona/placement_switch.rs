//! PlacementSwitch — a persona's brain follows the fleet.
//!
//! A persona bound to a REMOTE seat (`PersonaModelOverride.remote_peer`) used to be
//! bound for the life of her adapter: when the tower went dark her breaker went cold
//! and she sat refused for the window, and when the tower came back nothing re-offered
//! her. Both moves were done BY HAND on 2026-09-07 (Kira + Mathis, the 5090). Joel,
//! 2026-09-14: "towers all going on and offline all the time while we maintain
//! persistent intelligence and ideally reestablish the dynamic grid when internet is
//! restored or tower is rebooted."
//!
//! The switch is the adapter she runs on. It holds her REMOTE lane and, built on first
//! need, her HOME adapter from the same local factory every resident uses; the seat she
//! is on is one atomic. [`decide`] is the pure rule (unit-tested);
//! `modules::grid::placement::follow_the_fleet` owns the Grid orchestration and
//! applies it once per grid tick from the capacity ledger's beacon ages and the
//! breaker's cold state. Every move is a probe AND a line for the org room — this is a
//! decided, receipted, reversible move, not the silent substitution
//! [[fallbacks-are-illegal-fail-loud]] forbids. A node with no local lane PARKS her
//! (probe, no move) instead of downgrading.
//!
//! Return to the remote seat needs no synthetic probe: the beacon being fresh and the
//! breaker warm is the admission; her next real turn is the proof, and if it dies at
//! the deadline three times the breaker trips and she falls home again after the
//! cooldown. Hysteresis: one move per persona per [`MOVE_COOLDOWN_MS`].
//!
//! STAGE B (2026-09-15, Joel via Astra: "placement must work automatically through
//! the airc design, not manual binding IDs"): EVERY persona runs on a switch, Home
//! first. Once per grid tick the pass reads the peers' beacons (what each serves, its
//! lanes, its residents) and, when this node has more minds than lanes, moves the
//! least-served minds onto a peer's free lanes whose served model ranks at least as
//! high ([`choose_offloads`], pure). The move is a bound remote lane + a persisted
//! override, so a reboot keeps it; the fall-home/return rule above then owns it.

use dashmap::DashMap;
use std::sync::{Arc, LazyLock};
use uuid::Uuid;

mod adapter;
mod decision;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod preparation_tests;

pub use adapter::PlacementSwitch;
pub use decision::{
    choose_offloads, decide, moves_between_machines, seat_queued, LocalShape,
    PeerOffer, PlacementInputs, PlacementMove, Seat, MOVE_COOLDOWN_MS,
    REMOTE_SEAT_FRESH_MS, REMOTE_SEAT_SILENT_MS, SEAT_QUEUED_REASON,
    SEAT_QUEUE_BOUND_MS, SEAT_STARVES_REASON,
};
pub(crate) use decision::{
    decide_opportunity, opportunity_cooldown_ms, seat_starves, OpportunityInputs,
    SeatFit, SELF_SEAT_REASON,
};

/// Every live switch on this node, by persona. The factory registers at bind; the grid
/// tick walks it. A despawned persona's switch stays until her id is re-bound (a stale
/// entry only ever costs one ledger read per tick).
static SWITCHES: LazyLock<DashMap<Uuid, Arc<PlacementSwitch>>> = LazyLock::new(DashMap::new);

pub fn register(switch: Arc<PlacementSwitch>) {
    SWITCHES.insert(switch.persona_id(), switch);
}

pub fn switches() -> Vec<Arc<PlacementSwitch>> {
    SWITCHES.iter().map(|r| Arc::clone(r.value())).collect()
}
