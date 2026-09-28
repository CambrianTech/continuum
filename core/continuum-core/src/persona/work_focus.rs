//! One card per turn: honor the latest explicit choice recorded with a claim.
//! Heartbeats establish liveness, not new intent. Legacy/automatic claims retain
//! their freshness fallback. Reconciliation uses the same explicit-choice key.

use airc_lib::WorkCard;

/// The card this turn is about, among the cards she holds.
pub fn focus_card<'a>(held: impl IntoIterator<Item = &'a WorkCard>) -> Option<&'a WorkCard> {
    held.into_iter().max_by_key(|c| {
        (
            explicit_choice_key(c.card_id.as_uuid(), c.claim_provenance.as_ref()),
            c.last_heartbeat_at_ms.unwrap_or(c.updated_at_ms), // unwrap_or: no heartbeat uses the board change
        )
    })
}

/// The card THIS turn's activity is about. Focus is per activity (Joel, 2026-09-28): among
/// the cards held on the board of `room`, use the usual choice. No matching card means no
/// card binding for this room. An unrelated or unknown room cannot authorize a checkout:
/// the caller must resolve an explicit activity binding instead of borrowing another task.
pub fn focus_card_for_room(held: &[(Option<uuid::Uuid>, WorkCard)], room: uuid::Uuid) -> Option<&WorkCard> {
    focus_card(held.iter().filter(|(r, _)| *r == Some(room)).map(|(_, card)| card))
}

/// Shared intent ordering for turn focus and surplus-claim reconciliation.
/// Missing/unknown history never becomes an inferred explicit choice.
pub(crate) fn explicit_choice_key(
    card_id: uuid::Uuid,
    provenance: Option<&airc_work::model::ClaimProvenance>,
) -> Option<(u64, uuid::Uuid)> {
    let provenance = provenance?;
    (provenance.origin == airc_work::ClaimOrigin::Explicit)
        .then_some((provenance.selected_at_ms, card_id))
}

/// A lapsed explicit choice can supersede live automatic work, but recovery
/// cannot manufacture a choice or override a newer live explicit selection.
pub(crate) fn recovery_preferred_over_live<'a>(
    candidate: &WorkCard,
    live: impl IntoIterator<Item = &'a WorkCard>,
) -> bool {
    let candidate_key = explicit_choice_key(
        candidate.card_id.as_uuid(),
        candidate.claim_provenance.as_ref(),
    );
    let mut any_live = false;
    let mut live_choice = None;
    for card in live {
        any_live = true;
        live_choice = live_choice.max(explicit_choice_key(
            card.card_id.as_uuid(),
            card.claim_provenance.as_ref(),
        ));
    }
    !any_live || candidate_key.is_some_and(|key| Some(key) > live_choice)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(heartbeat: Option<u64>, updated: u64) -> WorkCard {
        WorkCard {
            card_id: airc_work::WorkCardId::new(),
            repo: airc_work::RepoId::new("acme/continuum").expect("valid repo id in fixture"),
            title: "t".to_string(),
            body: None,
            priority: airc_work::Priority::P2,
            lane_id: None,
            state: airc_work::CardState::Claimed,
            owner: None,
            claim_provenance: None,
            claim_id: None,
            claim_expires_at_ms: None,
            last_heartbeat_at_ms: heartbeat,
            pull_request: None,
            created_by: airc_core::PeerId::new(),
            created_at_ms: 1,
            updated_at_ms: updated,
            reviews: None,
            submissions: Vec::new(),
            last_submission_rejection: None,
        }
    }

    // what this catches: the focus drifting to the OLDER card (e.g. by board
    // order) — the turn must follow her freshest live claim, heartbeat first.
    #[test]
    fn the_focus_is_the_freshest_live_claim_heartbeat_before_board_change() {
        let held = [card(Some(100), 900), card(None, 500), card(Some(400), 50)];
        let f = focus_card(held.iter()).expect("some");
        assert_eq!(
            f.updated_at_ms, 500,
            "the second card's board change (500) is freshest: heartbeat 100 and 400 lose"
        );
        assert!(focus_card(std::iter::empty()).is_none());
        let mut held = [card(Some(2000), 2000), card(Some(30), 30)];
        held[0].claim_provenance = Some(airc_work::model::ClaimProvenance {
            origin: airc_work::ClaimOrigin::Automatic,
            selected_at_ms: 10,
        });
        held[1].claim_provenance = Some(airc_work::model::ClaimProvenance {
            origin: airc_work::ClaimOrigin::Explicit,
            selected_at_ms: 30,
        });
        assert_eq!(
            focus_card(held.iter()).map(|c| c.card_id),
            Some(held[1].card_id),
            "a heartbeat on automatically pulled work must not redirect her explicit choice"
        );
        assert!(recovery_preferred_over_live(&held[1], [&held[0]]));
        assert!(!recovery_preferred_over_live(&held[0], [&held[1]]));
        held[0]
            .claim_provenance
            .as_mut()
            .expect("fixture provenance")
            .origin = airc_work::ClaimOrigin::Explicit;
        assert_eq!(
            focus_card(held.iter()).map(|c| c.card_id),
            Some(held[1].card_id),
            "renewing an older explicit claim is not a new selection"
        );
        assert!(recovery_preferred_over_live(&held[1], [&held[0]]));
        assert!(!recovery_preferred_over_live(&held[0], [&held[1]]));
        assert!(recovery_preferred_over_live(&held[0], std::iter::empty()));
    }

    // what this catches (Joel, 2026-09-28: focus is per activity): a turn in one activity
    // rooting her hands at another activity's card because that card is the freshest
    // whole-mind choice. An unrelated room and an unscoped legacy claim must not select
    // another activity's checkout. Explicit choice ordering still applies within the room.
    #[test]
    fn a_turn_focuses_the_card_of_its_own_room_before_the_freshest_elsewhere() {
        let (project, benchmark, lounge) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        let here = card(Some(100), 100);
        let elsewhere_fresher = card(Some(9_000), 9_000);
        let held = vec![(Some(project), here.clone()), (Some(benchmark), elsewhere_fresher.clone()), (None, card(Some(50), 50))];
        assert_eq!(focus_card_for_room(&held, project).map(|c| c.card_id), Some(here.card_id), "her project turn works the project card");
        assert_eq!(focus_card_for_room(&held, benchmark).map(|c| c.card_id), Some(elsewhere_fresher.card_id));
        assert_eq!(
            focus_card_for_room(&held, lounge).map(|c| c.card_id),
            None,
            "a room with no card must not borrow another activity's checkout"
        );
        assert!(focus_card_for_room(&[(None, elsewhere_fresher.clone())], project).is_none(),
            "unknown claim scope is not a binding for this activity");
        let same_room = vec![(Some(project), here), (Some(project), elsewhere_fresher.clone())];
        assert_eq!(focus_card_for_room(&same_room, project).map(|c| c.card_id), Some(elsewhere_fresher.card_id),
            "existing explicit choice ordering is preserved within the activity");
        assert!(focus_card_for_room(&[], project).is_none());
    }
}
