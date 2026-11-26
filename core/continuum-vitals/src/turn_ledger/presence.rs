//! Her presence, said on her own bus: `mind/acting` is true while a turn of hers is in flight and
//! false once her last one ends. The turn ledger is its one owner (Fable on d56dc734): her turn is
//! begun and ended there ([`super::begin_turn`], its guard's drop), so what her parts
//! hear is the ledger's own fact, never a second reading of it.
//!
//! Her bus is hers (`senses::start`), so the topic is claimed there and handed to the ledger with
//! [`say_on`]; a mind with no bus here (a test, a mind not resident on this core) has nothing to
//! hear it, and her turns are still ledgered.

use std::collections::HashMap;
use std::sync::LazyLock;

use continuum_sage::component::{Bus, Key, Source, Taken};
use continuum_sage::dataflow::TopicName;
use parking_lot::Mutex;
use uuid::Uuid;

/// Whether she is acting a turn now (watched: her parts read the newest).
pub const ACTING: TopicName = TopicName("mind/acting");

/// She is acting a turn (`true`) or resting between them (`false`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Acting(pub bool);

static SAID: LazyLock<Mutex<HashMap<Uuid, Source<Acting>>>> = LazyLock::new(Default::default);

/// Say `persona`'s presence on `bus` from now on, starting with what her ledger holds. Her bus's
/// presence topic has one writer: a second claim is refused.
pub fn say_on(persona: Uuid, bus: &Bus) -> Result<(), Taken> {
    let mut source = Source::on(bus, ACTING, Key::ONE, "turn-ledger")?;
    let mut said = SAID.lock();
    source.say(Acting(super::turn_in_flight(persona)));
    said.insert(persona, source);
    Ok(())
}

/// Stop saying `persona`'s presence: her bus is gone with her.
pub fn forget(persona: Uuid) {
    SAID.lock().remove(&persona);
}

/// Say what her ledger now holds; a mind whose bus is not here says nothing. The ledger is read
/// under this owner's lock, so a turn's begin and another's end say in the order they read: never
/// a stale `true` after her last turn ended. Nothing takes this lock while holding the ledger's.
pub(crate) fn say(persona: Uuid) {
    let mut said = SAID.lock();
    if let Some(source) = said.get_mut(&persona) {
        source.say(Acting(super::turn_in_flight(persona)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn_ledger::begin_turn;

    fn acting(bus: &Bus) -> Option<bool> {
        bus.reader::<Acting>(ACTING, Key::ONE).latest().map(|said| said.value.0)
    }

    // what this catches: presence that says resting while another of her turns still runs, a
    // stale `true` after her last turn ends, or a bus that reads nothing until her first turn.
    #[test]
    fn she_is_acting_from_her_first_turn_until_her_last_ends() {
        let persona = Uuid::new_v4();
        let bus = Bus::new();
        say_on(persona, &bus).expect("test: the first claim of her presence");
        assert_eq!(acting(&bus), Some(false), "resting before any turn, read at once");
        let first = begin_turn(persona, 1);
        let second = begin_turn(persona, 2);
        assert_eq!(acting(&bus), Some(true));
        drop(first);
        assert_eq!(acting(&bus), Some(true), "another of her turns still runs");
        drop(second);
        assert_eq!(acting(&bus), Some(false));
        assert!(say_on(persona, &bus).is_err(), "her presence has one writer");
        forget(persona);
    }
}
