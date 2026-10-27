use super::*;
pub fn activity_rooms() -> Vec<(Uuid, String)> {
    rounds()
        .values()
        .filter(|r| r.stage != RoundStage::Done)
        .flat_map(|r| {
            let run = (!r.run_room_name.is_empty()).then(|| (r.round_id, r.run_room_name.clone()));
            run.into_iter().chain(
                r.card_activities
                    .values()
                    .filter(|a| !a.room_name.is_empty())
                    .map(|a| (a.solve_room, a.room_name.clone()))
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}

/// Is the round holding this card still WORKING? `true` for a card no round knows
/// (ordinary work).
pub fn card_round_is_working(card_id: Uuid) -> bool {
    match rounds().values().find(|r| r.cards.contains_key(&card_id)) {
        Some(r) => r.stage == RoundStage::Working,
        None => true,
    }
}
