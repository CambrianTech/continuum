//! Volume (scaffold): which drive holds a working set (THE-LANE-ON-SAGE.md, "Storage placement is a
//! use case too"; #6276 owns its enforcement). Rates are the resource owner's continuous observations,
//! each kind with its own semantics; unknown stays explicit.

use super::super::measured::Measured;
use super::super::use_case::{NonEmpty, Envelope, UseCase};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VolumeId(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub struct Volume {
    pub id: VolumeId,
    pub free_bytes: u64,
    /// Achieved bytes per second for this working set's access pattern (random reads, syncs, builds).
    pub rate: Measured<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Situation {
    pub bytes: u64,
    /// Hot: read and synced while work runs (a worktree, a cargo target, KV files). Cold: an archive.
    pub hot: bool,
    /// Never empty, by type: a node with no volume has nowhere to place anything and asks no decision.
    pub volumes: NonEmpty<Volume>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    On(VolumeId),
}

#[derive(Debug, Clone, PartialEq)]
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
    fn envelope(situation: &Situation) -> Envelope<Action> {
        let room: Vec<&Volume> = situation.volumes.iter().filter(|volume| volume.free_bytes >= situation.bytes).collect();
        let fastest = room.iter().filter_map(|volume| volume.rate.value()).fold(None, |best: Option<f64>, rate| Some(best.map_or(rate, |best| best.max(rate))));
        let mut fit = room.iter().filter(|volume| !situation.hot || match (volume.rate.value(), fastest) {
            (Some(rate), Some(fastest)) => rate >= fastest,
            _ => true,
        });
        // With no volume that has room, the one with the most free is forced: the disk-pressure owner frees it.
        match fit.next() {
            Some(first) => Envelope::Admissible(NonEmpty::new(Action::On(first.id), fit.map(|volume| Action::On(volume.id)))),
            None => {
                let most_free = situation.volumes.iter().fold(situation.volumes.first(), |best, volume| if volume.free_bytes > best.free_bytes { volume } else { best });
                Envelope::Forced(Action::On(most_free.id))
            }
        }
    }
}
