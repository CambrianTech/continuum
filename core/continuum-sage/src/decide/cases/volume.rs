//! Volume (scaffold): which drive holds a working set (THE-LANE-ON-SAGE.md, "Storage placement is a
//! use case too"; #6276 owns its enforcement). Rates are the resource owner's continuous observations,
//! each kind with its own semantics; unknown stays explicit.

use super::super::measured::Measured;
use super::super::use_case::{Deferred, Gate, UseCase};

/// A volume's index in [`Situation::volumes`].
pub type Slot = u16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Volume {
    pub id: u32,
    pub free_bytes: u64,
    /// Achieved bytes per second for this working set's access pattern (random reads, syncs, builds).
    pub rate: Measured<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Situation {
    pub bytes: u64,
    /// Hot: read and synced while work runs (a worktree, a cargo target, KV files). Cold: an archive.
    pub hot: bool,
    pub volumes: Vec<Volume>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    On(Slot),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outcome {
    /// The work's measured time on the volume chosen (a build, a restore).
    pub seconds: f64,
}

pub struct Placement;

impl UseCase for Placement {
    type Situation = Situation;
    type Action = Action;
    type Outcome = Outcome;
    const NAME: &'static str = "volume";

    /// A hot set never goes to a volume measured slower than the fastest one with room; a volume not yet
    /// measured is a candidate (its rate becomes known by use). A cold set may go anywhere with room.
    /// With no volume that has room, the decision defers: the disk-pressure owner reclaims, and it is
    /// asked again on fresh observations. A full volume is never chosen.
    fn envelope(situation: &Situation, admissible: &mut Vec<Action>) -> Gate<Action> {
        let named = &situation.volumes[..situation.volumes.len().min(usize::from(Slot::MAX))];
        let room = |volume: &&Volume| volume.free_bytes >= situation.bytes;
        let fastest = named.iter().filter(room).filter_map(|volume| volume.rate.value()).reduce(f64::max);
        for (slot, volume) in named.iter().enumerate() {
            let fast_enough = !situation.hot || matches!((volume.rate.value(), fastest), (Some(rate), Some(fastest)) if rate >= fastest) || volume.rate.value().is_none();
            if room(&volume) && fast_enough {
                admissible.push(Action::On(slot as Slot));
            }
        }
        if admissible.is_empty() {
            let Some(roomiest) = named.iter().map(|volume| volume.free_bytes).max() else {
                return Gate::Defer(Deferred { condition: "a volume to place on", have: 0.0, need: 1.0 });
            };
            return Gate::Defer(Deferred { condition: "free bytes on the roomiest volume", have: roomiest as f64, need: situation.bytes as f64 });
        }
        Gate::Choose
    }

    fn retain(slot: &mut Situation, from: &Situation) {
        slot.bytes = from.bytes;
        slot.hot = from.hot;
        slot.volumes.clone_from(&from.volumes);
    }

    fn empty() -> Situation {
        Situation { bytes: 0, hot: false, volumes: Vec::new() }
    }
}
