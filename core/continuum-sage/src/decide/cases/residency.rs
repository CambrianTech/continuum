//! Residency (scaffold): which parked minds stay warm and which are demoted to flash. Its owner (the
//! block layer) builds its policy; the types are here now. Prefix reuse and restore time are what a
//! policy weighs; continuity after a restore is a constraint, never traded.

use std::fmt::Debug;
use std::marker::PhantomData;
use std::time::Duration;

use super::super::measured::Measured;
use super::super::use_case::{Gate, UseCase, Violation};

/// A parked mind's index in [`Situation::parked`].
pub type Slot = u16;

/// One parked mind as residency sees her.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parked<K> {
    pub mind: K,
    pub resident_bytes: u64,
    pub cold_bytes: u64,
    pub idle: Duration,
    /// How much of her next turn her warm prefix is predicted to serve.
    pub prefix_hit: Measured<f64>,
    /// When she is predicted back (a tool's measured duration, her activity's cadence).
    pub returns_in: Measured<Duration>,
    pub restore: Measured<Duration>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Situation<K> {
    pub parked: Vec<Parked<K>>,
    /// Memory the ledger says is free now.
    pub free_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Keep,
    /// Her pages written back and released; her file keeps every byte.
    Demote(Slot),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outcome {
    pub bytes_freed: u64,
    pub restore: Measured<Duration>,
    /// Her next tokens after the restore equal an uninterrupted mind's.
    pub continuous: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Residency<K>(PhantomData<K>);

impl<K: Copy + Debug + PartialEq + Send + 'static> UseCase for Residency<K> {
    type Situation = Situation<K>;
    type Action = Action;
    type Outcome = Outcome;
    const NAME: &'static str = "residency";

    fn envelope(situation: &Situation<K>, admissible: &mut Vec<Action>) -> Gate<Action> {
        admissible.push(Action::Keep);
        let named = situation.parked.len().min(usize::from(Slot::MAX));
        admissible.extend((0..named).map(|slot| Action::Demote(slot as Slot)));
        Gate::Choose
    }

    fn constraint(outcome: &Outcome) -> Option<Violation> {
        (!outcome.continuous).then_some(Violation { constraint: "next-token continuity after a restore", measured: 0.0, limit: 1.0 })
    }

    fn retain(slot: &mut Situation<K>, from: &Situation<K>) {
        slot.parked.clone_from(&from.parked);
        slot.free_bytes = from.free_bytes;
    }

    fn empty() -> Situation<K> {
        Situation { parked: Vec::new(), free_bytes: 0 }
    }
}
