//! Custody of the turns the last stop interrupted (stop-drain S3, card afaa0a3e).
//!
//! An interrupted turn whose row recorded no effect is handed back as a retained input, so
//! it is retried one beat later on the same path as a capacity retry, reconciling its own
//! row. One that recorded an effect is never retried: its notice is pinned, and its row is
//! held here until a turn on the cycle carrying that notice completes.

use uuid::Uuid;

use super::{PendingInput, RetainedInput};
use crate::cognition::act_observe::SettleStep;
use crate::cognition::conversation::IncomingMessage;
use crate::cognition::workspace::TurnAttention;

impl PendingInput {
    /// Hand an interrupted, untouched turn back: retried at the next beat with its original
    /// engram (memory already holds the trigger) and its own row id. Idempotent per row.
    pub(crate) fn readmit(&mut self, message: IncomingMessage, engram: Option<Uuid>, attention: TurnAttention, obligation: Uuid) {
        if self.inputs.iter().any(|input| input.obligation == Some(obligation)) {
            return;
        }
        self.inputs.push_back(RetainedInput {
            message,
            engram,
            obligation: Some(obligation),
            attention,
            capacity: None,
        });
    }

    /// An interrupted turn that recorded an effect: owed one completed turn in `room`'s cycle.
    pub(crate) fn hold_uncertain(&mut self, obligation: Uuid, room: Uuid) {
        if !self.uncertain.iter().any(|(row, _)| *row == obligation) {
            self.uncertain.push((obligation, room));
        }
    }

    /// The uncertain rows a completed turn has answered since the last call.
    pub(crate) fn take_answered(&mut self) -> Vec<Uuid> {
        std::mem::take(&mut self.answered)
    }

    /// A turn that reached a decision (not deferred, not refused for capacity) read her
    /// notices on its cycle: her mind cycle in her private room, her live cycle elsewhere.
    pub(super) fn turn_completed(&mut self, room: Uuid, step: &SettleStep) {
        if matches!(step, SettleStep::DeferredBeforeDispatch | SettleStep::CapacityRefused { .. }) {
            return;
        }
        let Some(persona) = self.persona else { return };
        let private = crate::cognition::mind_room::is_private_room(persona, room);
        let (answered, owed): (Vec<_>, Vec<_>) = std::mem::take(&mut self.uncertain)
            .into_iter()
            .partition(|(_, held)| crate::cognition::mind_room::is_private_room(persona, *held) == private);
        self.uncertain = owed;
        self.answered.extend(answered.into_iter().map(|(row, _)| row));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(room: Uuid) -> IncomingMessage {
        IncomingMessage {
            event_id: Uuid::new_v4(),
            lamport: 1,
            peer_id: Uuid::new_v4(),
            text: "owed".into(),
            media: Vec::new(),
            room_id: room,
            work: None,
        }
    }

    // what this catches: two starts' recoveries (or one start seeing a row twice) queueing the
    // same interrupted turn twice, so she would answer it twice.
    #[test]
    fn readmitting_a_row_twice_retains_it_once() {
        let mut pending = PendingInput::for_persona(Uuid::new_v4());
        let (msg, row) = (message(Uuid::new_v4()), Uuid::new_v4());
        pending.readmit(msg.clone(), None, TurnAttention::default(), row);
        pending.readmit(msg, None, TurnAttention::default(), row);
        assert_eq!(pending.inputs.len(), 1);
    }

    // what this catches: an uncertain row closed by a turn that could not have read its notice:
    // her private mind room runs another cycle with its own working memory.
    #[test]
    fn only_a_turn_on_the_cycle_carrying_the_notice_answers_it() {
        let persona = Uuid::new_v4();
        let mut pending = PendingInput::for_persona(persona);
        let (public, row) = (Uuid::new_v4(), Uuid::new_v4());
        pending.hold_uncertain(row, public);
        pending.hold_uncertain(row, public);
        let private = crate::cognition::mind_room::mind_room_id(persona);
        pending.settle_self(private, &SettleStep::Passed { reason: None });
        assert!(pending.take_answered().is_empty(), "her mind cycle never saw the live cycle's notice");
        pending.settle_self(Uuid::new_v4(), &SettleStep::Passed { reason: None });
        assert_eq!(pending.take_answered(), vec![row], "held once, answered once");
    }
}
