//! Where a persona id she types back resolves (airc's `Shown::resolve_in`, ONE-ACT stage 6): the
//! personas this process knows by their registered cards, or the personas online in the runtime
//! registry. A persona is an airc peer, so her id is a `PeerId`; `Shown` is the one reading (a full
//! id exact, a prefix keeping every digit, a UUID one digit off read by its short form).

use airc_core::shown_id::{Shown, ShownIdError, ShownScope};
use airc_core::PeerId;
use uuid::Uuid;

/// The personas this process knows by their cards (`persona::card::ids`): registered at spawn
/// and on every identity edit.
pub struct PersonaCards;

impl ShownScope<PeerId> for PersonaCards {
    fn shown_ids(&self) -> Vec<PeerId> {
        super::card::ids().into_iter().map(PeerId::from_uuid).collect()
    }
}

/// The personas online now (`PersonaAircRuntimeRegistry`), for the runtime verbs.
impl ShownScope<PeerId> for super::PersonaAircRuntimeRegistry {
    fn shown_ids(&self) -> Vec<PeerId> {
        self.ids().into_iter().map(PeerId::from_uuid).collect()
    }
}

/// The persona `raw` names in `scope`, as the uuid the persona verbs key on.
pub fn persona_in(raw: &str, scope: &impl ShownScope<PeerId>) -> Result<Uuid, ShownIdError> {
    Shown::<PeerId>::try_from(raw)?.resolve_in(scope).map(|peer| peer.as_uuid())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::card::{register, remove, PersonaCard};

    // what this catches (#164, ONE-ACT stage 6): a persona verb takes back the short id her
    // roster shows, resolved against the personas this process knows by their cards; a word
    // around an id is not mined for hex (the old repairing reading took `card 08ece9e8` as an id);
    // an id no card names is refused, never guessed. Membership only: the card registry is
    // process-global, shared with other tests.
    #[test]
    fn a_shown_persona_id_resolves_against_her_card_and_a_wrapped_one_is_refused() {
        let asha = Uuid::new_v4();
        register(PersonaCard::genesis(asha, "Asha", 1, None));
        let shown = airc_core::shown_id::shown_form(asha);
        assert_eq!(persona_in(&shown, &PersonaCards), Ok(asha), "her short form names her");
        assert_eq!(persona_in(&asha.to_string(), &PersonaCards), Ok(asha), "a full id is exact");
        assert!(
            matches!(persona_in(&format!("persona {shown}"), &PersonaCards), Err(ShownIdError::NotAnId { .. })),
            "words around an id are not an id"
        );
        remove(&asha.to_string());
        assert!(persona_in(&shown, &PersonaCards).is_err(), "a persona with no card is refused");
    }
}
