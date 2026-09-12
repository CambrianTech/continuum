//! The `[ledger]` fact a work turn opens with — the card's ledger read from the run room's
//! wall (board-true, never a process note). For a review card the OWNER's ledger is what
//! the reviewer needs; for a held work card, its own.
use crate::experience::ledger::{render_absent, render_for_turn, LedgerStore, WallLedgerStore};

/// The fact for this turn. Reads through the holder's own airc runtime; an unreadable
/// wall reads as absence, named.
pub async fn ledger_fact_for(peer: uuid::Uuid, held: &[&airc_lib::WorkCard]) -> String {
    let Some(registry) = crate::persona::PersonaAircRuntimeRegistry::try_global() else {
        return render_absent();
    };
    let Some(rt) = registry.get(peer) else { return render_absent() };
    // The card whose ledger matters: a review card reads its parent's, else the first work
    // card held (WIP = 1 makes that the card).
    let target = held.iter().find_map(|c| {
        crate::commands::benchmark::parse_review_title(&c.title)
            .and_then(|_| crate::cognition::bench_round::review_parent(c.card_id.as_uuid()))
    });
    let (card, room) = match target {
        Some(parent) => (parent, room_of_card(rt.airc(), parent).await),
        None => match held.first() {
            Some(c) => (c.card_id.as_uuid(), room_of_card(rt.airc(), c.card_id.as_uuid()).await),
            None => return render_absent(),
        },
    };
    let Some(room) = room else { return render_absent() };
    match WallLedgerStore::new(rt.airc().clone()).read(&room, card).await {
        Ok(Some(l)) => render_for_turn(&l),
        Ok(None) => render_absent(),
        Err(e) => {
            crate::probe!(
                class = "card.ledger.unreadable_wall",
                card = %card,
                error = %e,
                "the run room's ledger records could not be read this turn"
            );
            render_absent()
        }
    }
}

async fn room_of_card(airc: &std::sync::Arc<airc_lib::Airc>, card: uuid::Uuid) -> Option<airc_lib::Room> {
    crate::modules::work::card_in_subscribed_rooms(airc, airc_lib::WorkCardId::from_uuid(card))
        .await
        .map(|(room, _)| room)
}
