//! ONE card per turn. A citizen may hold several cards (WIP = lanes lets her
//! pull a second while the first is in review), but a work turn is about ONE:
//! her freshest live claim — last heartbeat, else last board change. Before
//! this rule two held cards made the staging resolution ambiguous (hands stayed
//! at home) and the active-work list read as two jobs at once ("my working
//! memory has stale references to <the other card>", three citizens,
//! 2026-09-04). The other cards stay held and take their turn when freshest.

use airc_lib::WorkCard;

/// The card this turn is about, among the cards she holds.
pub fn focus_card<'a>(held: impl IntoIterator<Item = &'a WorkCard>) -> Option<&'a WorkCard> {
    held.into_iter()
        .max_by_key(|c| c.last_heartbeat_at_ms.unwrap_or(c.updated_at_ms)) // unwrap_or: never heartbeated = its last board change
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
            claim_id: None,
            claim_expires_at_ms: None,
            last_heartbeat_at_ms: heartbeat,
            pull_request: None,
            created_by: airc_core::PeerId::new(),
            created_at_ms: 1,
            updated_at_ms: updated,
            reviews: None,
        }
    }

    // what this catches: the focus drifting to the OLDER card (e.g. by board
    // order) — the turn must follow her freshest live claim, heartbeat first.
    #[test]
    fn the_focus_is_the_freshest_live_claim_heartbeat_before_board_change() {
        let held = [card(Some(100), 900), card(None, 500), card(Some(400), 50)];
        let f = focus_card(held.iter()).expect("some");
        assert_eq!(f.updated_at_ms, 500, "the second card's board change (500) is freshest: heartbeat 100 and 400 lose");
        assert!(focus_card(std::iter::empty()).is_none());
    }
}
