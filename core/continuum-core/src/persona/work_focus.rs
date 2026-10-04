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

/// Work eligible to execute, from the already ownership/lease-filtered claim set.
/// Review retains a claim but is not a new implementation turn.
pub fn actionable(card: &WorkCard) -> bool {
    matches!(card.state, airc_work::CardState::Claimed | airc_work::CardState::InProgress)
}

/// Selection for scheduling new implementation work.
pub fn focus_actionable_card<'a>(
    held: impl IntoIterator<Item = &'a WorkCard>,
) -> Option<&'a WorkCard> {
    focus_card(held.into_iter().filter(|card| actionable(card)))
}

/// Keep a retained review checkout available for conversation and repairs after
/// restart, without scheduling review as new implementation work. Active work
/// always takes precedence. The caller supplies ownership/lease-filtered claims.
pub fn focus_workspace_card<'a>(
    held: impl IntoIterator<Item = &'a WorkCard>,
) -> Option<&'a WorkCard> {
    let mut active = Vec::new();
    let mut review = Vec::new();
    for card in held {
        if actionable(card) {
            active.push(card);
        } else if card.state == airc_work::CardState::Review {
            review.push(card);
        }
    }
    focus_card(active).or_else(|| focus_card(review))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HeldFocus {
    pub room_id: uuid::Uuid,
    pub card_id: airc_work::WorkCardId,
    pub changed: bool,
}

/// An unread room fact about a held card is a changed world, irrespective of
/// column. Use the shared digest and durable reader bookmark; no wake ledger.
pub(crate) async fn focus_room(
    citizen: &dyn super::airc_citizen::AircCitizen,
) -> Result<Option<HeldFocus>, airc_lib::AircError> {
    use crate::runtime::ready_buffer::ReadyBuffer;
    let held = citizen.active_claims().await?;
    if held.is_empty() {
        return Ok(None);
    }
    let rooms = citizen.subscribed_rooms().await?;
    let buffer = crate::cognition::channel_substrate::global_channel_digest_buffer();
    let builder = crate::cognition::channel_substrate::global_channel_digest_builder();
    let mut changed = None;
    for room in &rooms {
        let key = (citizen.peer_id(), *room);
        let previous = buffer.peek(&key);
        let digest = std::sync::Arc::new(builder.build_with_progress(
            citizen.peer_id(), *room, citizen,
            super::airc_source::FETCH_LIMIT,
            crate::cognition::channel_digest::DEFAULT_GROUNDING,
            previous.as_deref(),
        ).await?);
        for element in digest.unread() {
            let Some(subject) = element.content_kind().and_then(|kind| kind.subject_card()) else {
                continue;
            };
            if held.iter().any(|card| card.card_id == subject) {
                let event = element.event();
                let cursor = airc_core::TranscriptCursor { lamport: event.lamport, event_id: event.event_id };
                if changed.as_ref().is_none_or(|(previous, _)| super::airc_source::cursor_key(&cursor) > super::airc_source::cursor_key(previous)) {
                    changed = Some((cursor, HeldFocus { room_id: *room, card_id: subject, changed: true }));
                }
            }
        }
        buffer.publish(key, digest);
    }
    if let Some((_, focus)) = changed {
        return Ok(Some(focus));
    }
    let Some(card) = focus_actionable_card(&held) else {
        return Ok(None);
    };
    let mut unreadable = None;
    for room in rooms {
        let board = match citizen.work_board(Some(room)).await {
            Ok(board) => board,
            Err(error) => {
                unreadable = Some(error);
                continue;
            }
        };
        if board.cards.iter().any(|c| c.card_id == card.card_id) {
            return Ok(Some(HeldFocus { room_id: room, card_id: card.card_id, changed: false }));
        }
    }
    if let Some(error) = unreadable {
        return Err(error);
    }
    Err(airc_lib::AircError::NotSubscribed(format!(
        "selected work card {} has no readable subscribed board", card.card_id
    )))
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

    // what this catches: a renewed benchmark lease selecting a different room
    // from the explicit ordinary project choice used by the hands/work gate.
    #[tokio::test]
    async fn self_cycle_uses_project_board_and_explicit_choice() {
        use super::super::airc_citizen::StubAircCitizen;
        let project_room = uuid::Uuid::new_v4();
        let bench_room = uuid::Uuid::new_v4();
        let mut project = card(Some(10), 10);
        project.claim_provenance = Some(airc_work::model::ClaimProvenance {
            origin: airc_work::ClaimOrigin::Explicit,
            selected_at_ms: 10,
        });
        let benchmark = card(Some(999), 999);
        let citizen = StubAircCitizen::new(uuid::Uuid::new_v4())
            .with_rooms(vec![bench_room, project_room])
            .with_claims(vec![benchmark.clone(), project.clone()])
            .with_board(bench_room, vec![benchmark])
            .with_board(project_room, vec![project]);
        assert_eq!(focus_room(&citizen).await.expect("board readable").map(|focus| focus.room_id), Some(project_room));
        let citizen = citizen.with_rooms(vec![bench_room]);
        assert!(matches!(focus_room(&citizen).await, Err(airc_lib::AircError::NotSubscribed(_))));
        let citizen = citizen.with_claims(vec![]);
        assert_eq!(focus_room(&citizen).await.expect("idle"), None);
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
        // A newer retained review claim must not move tools away from the
        // implementation card selected by the follow-on work gate.
        held[1].state = airc_work::CardState::Review;
        assert_eq!(focus_actionable_card(held.iter()).map(|c| c.card_id), Some(held[0].card_id));
        assert_eq!(focus_workspace_card(held.iter()).map(|c| c.card_id), Some(held[0].card_id));
        held[0].state = airc_work::CardState::Closed;
        assert!(focus_actionable_card(held.iter()).is_none());
        // Review must retain native hands on restart without becoming scheduled work.
        assert_eq!(focus_workspace_card(held.iter()).map(|c| c.card_id), Some(held[1].card_id));
        held[1].state = airc_work::CardState::Closed;
        assert!(focus_workspace_card(held.iter()).is_none());

    }
}
