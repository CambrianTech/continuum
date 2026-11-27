//! What the trainer owes, said on `learning/owed` (UNIFIED-PAGING.md "learning is PWM"): every seat
//! with a lesson owed, from the seats the trainer holds, said whenever one of them changes. The lane
//! sets learning's duty from it; the trainer still decides what she learns next.

use std::collections::{BTreeMap, HashMap};

use uuid::Uuid;

use super::super::durable::Arrival;
use super::super::in_place::start;
use super::super::TrainingTriggerState;
use crate::inference::learning::{LearnerOwed, Owed, PersonaId};

/// The examples `rows` still owe past the newest committed generation (`checkpointed`) and the seat's
/// own `progress`.
pub(super) fn examples_owed(rows: &[Arrival], checkpointed: &HashMap<Uuid, u32>, progress: &HashMap<Uuid, u32>) -> usize {
    rows.iter()
        .map(|row| {
            let learned = start(row.learned, checkpointed.get(&row.id).copied(), progress.get(&row.id).copied());
            row.examples.len().saturating_sub(learned as usize) // as: a u32 fits a usize on every target this builds for
        })
        .fold(0, usize::saturating_add)
}

impl TrainingTriggerState {
    /// Say what every seat owes now. A seat stays while it has examples to learn or progress not yet
    /// committed; one with neither is left out (its Done).
    pub(super) fn say_owed(&self) {
        let owed = {
            let seats = self.on_engine.seats.lock();
            let owing = seats.iter().filter(|(_, held)| held.examples_owed > 0 || !held.run.progress().is_empty());
            Owed(owing.map(|(seat, held)| (*seat, LearnerOwed { mind: PersonaId(held.key.persona_id), examples: held.examples_owed, next: held.next, since: held.since })).collect::<BTreeMap<_, _>>())
        };
        self.say(|out| out.owe(owed));
    }
}
