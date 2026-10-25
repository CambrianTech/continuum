//! A message turn takes in its trigger when it STARTS (her cursor passes the line,
//! `service_loop::turn_context::announce_turn_start`) but answers it only when it ends. A stop
//! that cuts the turn between the two left the line read and never answered, and after the
//! restart nothing woke her for it (the M5, 2026-10-24: three of four stops cut 2 turns each).
//!
//! So while a message turn runs it holds its room's cursor from before it took the line in. The
//! hold is released however the turn ends on its own (a reply, silence, an error: the guard
//! drops). If a stop cuts the turn, the guard is still alive when the stop's settlers run, and
//! [`TriggerCustodySettle`] gives the line back before her state saves, so the same line wakes
//! the same turn after the restart (on warm KV when the engine was kept, `stop --keep-engine`).

use std::collections::HashMap;
use std::sync::LazyLock;

use parking_lot::Mutex;
use uuid::Uuid;

use crate::persona::mind_state::ActivityCursor;

struct Held {
    room: Uuid,
    before: Option<ActivityCursor>,
}

/// One message turn per mind at a time, so the hold is keyed by her.
static HELD: LazyLock<Mutex<HashMap<Uuid, Held>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// A running turn's custody of its trigger. Dropping it (the turn ended on its own) releases it.
pub(crate) struct TriggerHold {
    persona: Uuid,
}

impl Drop for TriggerHold {
    fn drop(&mut self) {
        HELD.lock().remove(&self.persona);
    }
}

/// Record `room`'s cursor before her turn takes in its trigger. Call BEFORE `mark_perceived`.
pub(crate) fn hold(persona: Uuid, room: Uuid) -> TriggerHold {
    let before = crate::persona::perception_feed::with_region(persona, |region| region.cursor(room).cloned()).flatten();
    HELD.lock().insert(persona, Held { room, before });
    TriggerHold { persona }
}

/// Give back every trigger a cut turn still holds; how many.
pub(crate) fn give_back_all(now_ms: u64) -> usize {
    let held: Vec<(Uuid, Held)> = HELD.lock().drain().collect();
    let returned = held.len();
    for (persona, held) in held {
        crate::persona::perception_feed::with_region(persona, |region| region.give_back(held.room, held.before, now_ms));
        continuum_base::probe!(
            class = "persona.turn.trigger_returned",
            persona = %persona,
            room = %held.room,
            "a stop cut her turn after it took in its trigger and before it answered; the line is unread again, for the same turn after the restart"
        );
    }
    returned
}

/// The stop participant: runs after the admitted turns settle (installed after them), before
/// any module saves, and gives back the triggers of the turns the settle cut.
pub struct TriggerCustodySettle;

impl crate::runtime::extensions::SettleBeforeStop for TriggerCustodySettle {
    fn name(&self) -> &'static str {
        "turn_trigger_custody"
    }

    fn settle(&self, _cap: std::time::Duration) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            let returned = give_back_all(continuum_base::time::EpochMs::now().millis());
            continuum_base::probe!(class = "persona.turn.triggers_returned", returned, "triggers of cut turns given back before the stop saves");
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a turn a stop cuts after it took in its trigger leaving the line read
    // and never answered (three of four stops on the M5, 2026-10-24), and a turn that ended on
    // its own having its line handed back (she would answer it twice).
    #[test]
    fn a_cut_turn_gives_its_trigger_back_and_an_ended_one_does_not() {
        let (cut, ended, room) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let _running = hold(cut, room);
        drop(hold(ended, room));
        let held: Vec<Uuid> = HELD.lock().keys().copied().collect();
        assert!(held.contains(&cut), "the running turn holds its trigger");
        assert!(!held.contains(&ended), "a turn that ended released its hold");
        // No region is registered for these test minds, so the give-back only drains the holds;
        // the region's own give_back is covered in mind_state's tests.
        let returned = give_back_all(1);
        assert!(returned >= 1);
        assert!(!HELD.lock().contains_key(&cut), "given back once");
    }
}
