//! Custody of the turns the last stop interrupted (stop-drain S3, card afaa0a3e).
//!
//! An interrupted turn whose row recorded no effect is handed back as a retained input, so
//! it is retried one beat later on the same path as a capacity retry, reconciling its own
//! row. One that recorded an effect is never retried: its notice is pinned, and its row is
//! held here until a turn on the cycle carrying that notice completes.

use uuid::Uuid;

use super::{PendingInput, RetainedInput};
use crate::ai::provenance::{GenerationOutcome, GenerationReceipt};
use crate::cognition::conversation::IncomingMessage;
use crate::cognition::workspace::TurnAttention;

impl PendingInput {
    /// Apply a completed generation to input and notice custody together. A receipt
    /// checkpoint failure leaves both with their existing owner for restart recovery.
    pub(crate) fn observe_settlement(
        &mut self,
        turn_room: Uuid,
        message: &IncomingMessage,
        wake_engram: Option<Uuid>,
        attention: TurnAttention,
        step: &crate::cognition::act_observe::SettleStep,
        receipts: &[GenerationReceipt],
    ) {
        if matches!(
            step,
            crate::cognition::act_observe::SettleStep::ReceiptPersistenceFailed { .. }
        ) {
            return;
        }
        self.settle(message, wake_engram, attention, step);
        self.notices_read(turn_room, receipts);
    }

    /// Hand an interrupted, untouched turn back: retried at the next beat with its original
    /// engram (memory already holds the trigger) and its own row id. Idempotent per row.
    pub(crate) fn readmit(
        &mut self,
        message: IncomingMessage,
        engram: Option<Uuid>,
        attention: TurnAttention,
        obligation: Uuid,
    ) {
        if self
            .inputs
            .iter()
            .any(|input| input.obligation == Some(obligation))
        {
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

    /// A turn that SERVED a generation read her notices: they are required prompt input on the
    /// cycle that carries them (her mind cycle in her private room, her live cycle elsewhere).
    /// The step's kind is not evidence the model read anything; a served receipt is (Codex on
    /// #5497: a failed inference must never answer a row she was never shown).
    pub(crate) fn notices_read(&mut self, room: Uuid, receipts: &[GenerationReceipt]) {
        if !receipts
            .iter()
            .any(|r| matches!(r.outcome, GenerationOutcome::Served { .. }))
        {
            return;
        }
        let Some(persona) = self.persona else { return };
        let private = crate::cognition::mind_room::is_private_room(persona, room);
        let (answered, owed): (Vec<_>, Vec<_>) = std::mem::take(&mut self.uncertain)
            .into_iter()
            .partition(|(_, held)| {
                crate::cognition::mind_room::is_private_room(persona, *held) == private
            });
        self.uncertain = owed;
        self.answered
            .extend(answered.into_iter().map(|(row, _)| row));
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
    // one that served nothing (a failed inference, a pass with no generation; Codex on #5497),
    // or one on her private mind room's cycle, which has its own working memory.
    #[test]
    fn only_a_served_generation_on_the_cycle_carrying_the_notice_answers_it() {
        use crate::persona::training_producer::test_support::{receipt, served_receipt};
        let persona = Uuid::new_v4();
        let mut pending = PendingInput::for_persona(persona);
        let (public, row) = (Uuid::new_v4(), Uuid::new_v4());
        pending.hold_uncertain(row, public);
        pending.hold_uncertain(row, public);
        pending.notices_read(public, &[receipt("faulted")]);
        pending.notices_read(public, &[]);
        assert!(
            pending.take_answered().is_empty(),
            "nothing served: the model read nothing"
        );
        let private = crate::cognition::mind_room::mind_room_id(persona);
        pending.notices_read(private, &[served_receipt("mind", "m")]);
        assert!(
            pending.take_answered().is_empty(),
            "her mind cycle never saw the live cycle's notice"
        );
        pending.notices_read(
            Uuid::new_v4(),
            &[receipt("faulted"), served_receipt("live", "m")],
        );
        assert_eq!(
            pending.take_answered(),
            vec![row],
            "held once, answered once"
        );
    }
    // A recovered trigger's original room need not be the cycle selected for this turn.
    #[test]
    fn settlement_uses_the_selected_cycle_room_for_notice_receipts() {
        use crate::cognition::act_observe::SettleStep;
        use crate::persona::training_producer::test_support::served_receipt;
        let persona = Uuid::new_v4();
        let public = Uuid::new_v4();
        let private = crate::cognition::mind_room::mind_room_id(persona);
        let public_row = Uuid::new_v4();
        let private_row = Uuid::new_v4();
        let mut pending = PendingInput::for_persona(persona);
        pending.hold_uncertain(public_row, public);
        pending.hold_uncertain(private_row, private);
        pending.observe_settlement(
            private,
            &message(public),
            None,
            TurnAttention::default(),
            &SettleStep::Spoke("settled".into()),
            &[served_receipt("mind", "m")],
        );
        assert_eq!(pending.take_answered(), vec![private_row]);
        pending.notices_read(public, &[served_receipt("live", "m")]);
        assert_eq!(pending.take_answered(), vec![public_row]);
    }
}
