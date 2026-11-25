//! Residency (scaffold): which parked minds stay warm, which go to flash, which are let go. Its owner
//! (the block layer) builds its policy; the types are here now. Prefix reuse and restore time are what
//! a policy weighs; continuity after a restore is a constraint, never traded.

use std::fmt::Debug;
use std::hash::Hash;
use std::marker::PhantomData;
use std::time::Duration;

use super::super::measured::Measured;
use super::super::use_case::{NonEmpty, Envelope, UseCase, Violation};

/// One parked mind as residency sees her.
#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action<K> {
    Keep,
    /// Her pages written back and released; her file keeps every byte.
    Demote(K),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub bytes_freed: u64,
    pub restore: Measured<Duration>,
    /// Her next tokens after the restore equal an uninterrupted mind's.
    pub continuous: bool,
}

pub struct Residency<K>(PhantomData<K>);

impl<K: Clone + Debug + PartialEq + Eq + Hash + Send + 'static> UseCase for Residency<K> {
    type Situation = Situation<K>;
    type Action = Action<K>;
    type Outcome = Outcome;
    const NAME: &'static str = "residency";

    fn envelope(situation: &Situation<K>) -> Envelope<Action<K>> {
        Envelope::Admissible(NonEmpty::new(Action::Keep, situation.parked.iter().map(|mind| Action::Demote(mind.mind.clone()))))
    }

    fn constraint(outcome: &Outcome) -> Option<Violation> {
        (!outcome.continuous).then_some(Violation { constraint: "next-token continuity after a restore", measured: 0.0, limit: 1.0 })
    }
}
