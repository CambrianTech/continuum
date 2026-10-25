//! The inputs she admitted and has not answered: what a stop must hand to the next life
//! (docs/architecture/TURNS-RESUME-ACROSS-A-STOP.md). The turn and the loop's custody stamp
//! here as they go; the handoff carries it whole across the seam; the next life's custody is
//! seeded from it. Once the seam has written it, this life no longer answers for her: a turn
//! still running after the seal does not publish, so each input is answered by exactly one
//! life.

use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::cognition::conversation::IncomingMessage;

#[derive(Default)]
struct Ledger {
    /// The input the running turn is answering. Owned by that turn's [`InFlight`] guard.
    in_flight: Option<IncomingMessage>,
    /// What the loop's custody holds parked, in its order. Owned by custody (`set_parked`).
    parked: Vec<IncomingMessage>,
    /// The previous life's inputs, handed over by the handoff and taken once by custody.
    resumed: Vec<IncomingMessage>,
    /// The seam wrote the record; from here the next life answers.
    sealed: bool,
}

impl Ledger {
    fn owed(&self) -> Vec<IncomingMessage> {
        let mut owed: Vec<IncomingMessage> = self.in_flight.iter().cloned().collect();
        owed.extend(self.parked.iter().filter(|m| self.in_flight.as_ref() != Some(*m)).cloned());
        owed
    }
}

static LEDGERS: Mutex<Option<HashMap<Uuid, Ledger>>> = Mutex::new(None);

fn with<R>(persona: Uuid, f: impl FnOnce(&mut Ledger) -> R) -> R {
    let mut guard = LEDGERS.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: poisoned lock = read the last state, same policy as every lock in this crate
    f(guard.get_or_insert_with(HashMap::new).entry(persona).or_default())
}

/// Held by a turn for as long as it answers `message`; dropped on every exit of the turn.
#[must_use = "the input is owed only while the guard lives"]
pub struct InFlight {
    persona: Uuid,
    message: IncomingMessage,
}

/// A turn took `message`: it is owed an answer until the turn ends.
pub fn begin(persona: Uuid, message: &IncomingMessage) -> InFlight {
    with(persona, |l| l.in_flight = Some(message.clone()));
    InFlight { persona, message: message.clone() }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        with(self.persona, |l| {
            if l.in_flight.as_ref() == Some(&self.message) {
                l.in_flight = None;
            }
        });
    }
}

/// Custody's parked inputs, whole, each time they change.
pub fn set_parked(persona: Uuid, parked: Vec<IncomingMessage>) {
    with(persona, |l| l.parked = parked);
}

/// What the seam writes. Sealing is part of the read: the record and the refusal to publish
/// after it are one decision, so no answer can land between them.
pub fn seal(persona: Uuid) -> Vec<IncomingMessage> {
    with(persona, |l| {
        l.sealed = true;
        l.owed()
    })
}

/// What a periodic checkpoint writes: the same list, without sealing (the life goes on).
pub fn owed(persona: Uuid) -> Vec<IncomingMessage> {
    with(persona, |l| l.owed())
}

/// Whether this life may still publish an answer for her.
pub fn may_answer(persona: Uuid) -> bool {
    with(persona, |l| !l.sealed)
}

/// On wake: the previous life's unanswered inputs, for custody to take.
pub fn hand_over(persona: Uuid, previous: &[IncomingMessage]) {
    with(persona, |l| l.resumed = previous.to_vec());
}

/// Custody takes the handed-over inputs once, and parks them (then `set_parked` owns them).
pub fn take_resumed(persona: Uuid) -> Vec<IncomingMessage> {
    with(persona, |l| std::mem::take(&mut l.resumed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str) -> IncomingMessage {
        IncomingMessage {
            event_id: Uuid::new_v4(),
            lamport: 1,
            peer_id: Uuid::from_u128(1),
            text: text.into(),
            media: Vec::new(),
            room_id: Uuid::from_u128(2),
            work: None,
        }
    }

    // what this catches: a stop that cuts a turn loses the line it was answering (the
    // retracted #5397's defect). The open list is what the seam seals; sealing refuses this
    // life's late publish; the next life takes the list once and owes it again.
    #[test]
    fn an_unanswered_input_crosses_the_seam_once_and_is_answered_by_one_life() {
        let (me, next) = (Uuid::from_u128(0x5e41), Uuid::from_u128(0x5e42));
        let (asked, parked, done) = (line("can you look?"), line("and this"), line("thanks"));
        drop(begin(me, &done));
        assert!(owed(me).is_empty(), "a turn that ended owes nothing");
        set_parked(me, vec![parked.clone()]);
        let turn = begin(me, &asked);
        assert!(may_answer(me));
        let record = seal(me);
        assert_eq!(record, vec![asked.clone(), parked.clone()], "the running turn's input first, then custody's");
        assert!(!may_answer(me), "after the seal this life no longer answers");
        drop(turn);

        hand_over(next, &record);
        assert_eq!(take_resumed(next), record, "the next life takes them");
        assert!(take_resumed(next).is_empty(), "once");
        assert!(may_answer(next));
    }
}
