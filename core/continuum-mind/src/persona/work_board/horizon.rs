//! The boards a citizen can see (one per subscribed room) and the walk that finds a card on
//! them, with read failures carried rather than reported as absence.

use std::sync::Arc;

use airc_lib::{Airc, WorkCardId};

use crate::sdk_codegen::CommandError;


/// The room whose board holds `card_id` — the ONE place that answers *which
/// activity does this card belong to*.
///
/// A card carries no room of its own (airc's `WorkCard` has no such field);
/// boards are PER-ROOM, so a card's activity IS the room whose board it appears
/// on. Two callers need that fact and used to derive it separately — badly: the
/// wrong-room claim retry inlined the scan, and the claim-fired solve dispatch
/// did not derive it at all and went roomless instead (#425, measured: 13,209
/// roomless turns, 35% of one citizen's cognition, invisible to every room).
///
/// Rooms are tried in subscription order and a card exists on exactly one board,
/// so the first hit is the only hit. A caller can only read boards of rooms it is
/// SUBSCRIBED to, so this widens no visibility — it only stops discarding what
/// the caller can already see.
pub async fn room_holding_card(
    airc: &Arc<Airc>,
    card_id: WorkCardId,
) -> Option<airc_lib::Room> {
    card_in_subscribed_rooms(airc, card_id)
        .await
        .map(|(room, _)| room)
}

/// The card AND the subscribed room whose board holds it — the one walk behind
/// [`room_holding_card`], kept whole for callers that need the card's own fields
/// (its title is what on-claim staging resolves the recipe from).
pub async fn card_in_subscribed_rooms(
    airc: &Arc<Airc>,
    card_id: WorkCardId,
) -> Option<(airc_lib::Room, airc_lib::WorkCard)> {
    board_horizon_for_card(airc, Some(card_id))
        .await
        .ok()?
        .boards
        .into_iter()
        .find_map(|(room, board)| {
            board
                .snapshot()
                .cards
                .iter()
                .find(|c| c.card_id == card_id)
                .map(|card| (room, card.clone()))
        })
}

/// Every board the caller can SEE: one fold per subscribed room, in subscription
/// order. This is the ONE definition of "a card I can address" for this scope.
///
/// The scope's current room is a cwd-derived default, not a horizon: a citizen
/// resident in the academy AND a run room addresses a card by prefix meaning
/// "a card on any board I am in". Resolving prefixes against the current room
/// alone answered Joaquin's `work/get 0c4317e1` with "available: the 12 bench
/// cards" — the run room's board — while her academy board held the card the
/// whole time (2026-09-05; the fold, the cursor and the page size were all
/// exonerated before the resolver was read). A room whose board fails to read
/// is skipped, never fatal: a stale run room must not hide the academy.
/// The board a READ verb looks at: `room`'s when named (a room name or channel id
/// this persona is subscribed to), else the CURRENT room's. Boards are per room —
/// #academy holds 176 cards, #continuum 231, a fresh activity room none — and a
/// citizen seated in a triage room saw an EMPTY board because `work/list` read the
/// room she was standing in (2026-09-17). The board she triages is the board she
/// names. Write verbs are unaffected: they resolve a card across every subscribed
/// board already.
pub async fn board_to_read(
    airc: &Arc<Airc>,
    room: Option<&str>,
) -> Result<airc_lib::BoardSnapshot, CommandError> {
    let projection = match room.map(str::trim).filter(|r| !r.is_empty()) {
        Some(name) => {
            let room = airc
                .room_by_name_or_channel(name, "read the work board of")
                .await
                .map_err(|e| CommandError::Invalid(format!("room {name:?}: {e}")))?;
            airc.project_room_work_board(&room, airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
                .await
                .map_err(|e| CommandError::Internal(format!("board read ({name}): {e}")))?
        }
        None => airc
            .work_board_complete(airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
            .await
            .map_err(|e| CommandError::Internal(format!("board read: {e}")))?,
    };
    Ok(projection.snapshot())
}

/// Every board the caller can see, AND every subscribed room whose board could not
/// be read this walk. A skipped room is not fatal (the doc on [`board_to_read`]:
/// a stale run room must not hide the academy) — but it is not SILENT either.
/// Kimi's `work/get 31c241e2` ran during a daemon outage (2026-09-21, card
/// cb4dea0f): every board read failed, the walk returned an empty set, and the
/// resolver told her "no cards exist … there are none to choose from" — a read
/// failure reported as an absence, and she reasoned from it. The horizon carries
/// the failures so a miss can say which it was.
pub struct BoardHorizon {
    pub boards: Vec<(airc_lib::Room, airc_lib::WorkBoardProjection)>,
    /// `(room name, error)` for each subscribed room skipped this walk.
    pub unreadable: Vec<(String, String)>,
}

impl BoardHorizon {
    pub(crate) fn readable_room_names(&self) -> Vec<String> {
        self.boards.iter().map(|(room, _)| room.name.clone()).collect()
    }

    /// The truthful "not found" for this walk — see [`not_found_in`]. `label` is the
    /// id kind the caller resolves (`"card"`, `"claim"`), the same word `id_resolve` gets.
    pub fn not_found(&self, label: &str, requested: &str) -> CommandError {
        not_found_in(
            &self.readable_room_names(),
            &self.unreadable,
            label,
            requested,
        )
    }
}

/// One sentence per way an id can fail to be found on the boards a citizen can see,
/// because they call for different next moves: a read failure means RETRY, an empty
/// subscription means JOIN the card's room, and a readable miss means the card lives
/// in a room she has not joined. Folding all three into "no cards exist" (the
/// resolver's empty-candidate line) sent Kimi looking for a fold bug during an outage.
pub fn not_found_in(
    readable_rooms: &[String],
    unreadable: &[(String, String)],
    label: &str,
    requested: &str,
) -> CommandError {
    if !unreadable.is_empty() {
        let failed = unreadable
            .iter()
            .map(|(room, err)| format!("{room} ({err})"))
            .collect::<Vec<_>>()
            .join("; ");
        let searched = if readable_rooms.is_empty() {
            "none".to_string()
        } else {
            readable_rooms.join(", ")
        };
        return CommandError::Internal(format!(
            "{label} {requested}: board read FAILED in {} of {} subscribed room(s) — {failed}; \
             readable boards searched: {searched}. This is a read failure, not an absence: \
             the {label} may be on an unreadable board — retry before concluding it does not exist",
            unreadable.len(),
            unreadable.len() + readable_rooms.len(),
        ));
    }
    if readable_rooms.is_empty() {
        return CommandError::NotFound(format!(
            "{label} {requested}: you are subscribed to no rooms, so there is no board to \
             search — join the room that holds the {label} and retry"
        ));
    }
    if label == "claim" {
        return CommandError::NotFound(format!(
            "claim {requested}: no claims at all on the boards of the rooms you are in ({}) — \
             a claim that is gone from the board has EXPIRED or been released; if it was \
             yours, re-claim the card (work/claim) and retry — YOUR WORK IS UNTOUCHED, a lapsed claim takes nothing from your checkout",
            readable_rooms.join(", ")
        ));
    }
    CommandError::NotFound(format!(
        "{label} {requested}: not on the board of any room you are in ({}) — it lives in a \
         room you have not joined",
        readable_rooms.join(", ")
    ))
}

pub async fn board_horizon(airc: &Arc<Airc>) -> Result<BoardHorizon, airc_lib::AircError> {
    board_horizon_for_card(airc, None).await
}

/// A fully identified card needs only its first containing board, as the existing
/// resolver's first match does. Prefixes need the complete horizon to detect ambiguity.
/// Keep one walk and one read-failure policy for both; unrelated later rooms must not
/// delay an already located card (44c5612e: Intel work/get took 121–161 seconds while
/// its containing board read in under one second).
pub async fn board_horizon_for_card(
    airc: &Arc<Airc>,
    card_id: Option<WorkCardId>,
) -> Result<BoardHorizon, airc_lib::AircError> {
    let set = airc.subscription_set().await?;
    let mut horizon = BoardHorizon {
        boards: Vec::new(),
        unreadable: Vec::new(),
    };
    for sub in set.all() {
        let room = sub.as_room();
        match airc.work_board_in(&room).await {
            Ok(board) => {
                let found = card_id.is_some_and(|id| board.card(id).is_some());
                horizon.boards.push((room, board));
                if found {
                    break;
                }
            }
            Err(e) => horizon.unreadable.push((room.name, e.to_string())),
        }
    }
    Ok(horizon)
}

/// Follow a card to the room that holds it: join that room (if it is not the
/// current one) so a by-id mutation can be retried there. `Some(room name)`
/// when the caller should retry; `None` when the card is placeable nowhere or
/// the caller already stands in its room. Accept-or-redirect, never
/// refuse-and-instruct — shared by claim, state and release.
pub async fn follow_card_room(
    airc: &Arc<Airc>,
    card_id: WorkCardId,
    verb: &'static str,
) -> Option<String> {
    // The card's room: the subscribed room whose board holds it. The round tracker's
    // node-wide fallback went with the detached runner (card c4f4b8cd step 2).
    let room = room_holding_card(airc, card_id).await?;
    let (room_name, room_channel) = (room.name, room.channel);
    let current = airc.current_room().await.ok();
    if current.as_ref().is_some_and(|c| c.channel == room_channel) {
        return None;
    }
    if airc.join(&room_name).await.is_err() {
        return None;
    }
    continuum_base::probe!(
        class = "work.followed_card_room",
        verb = verb,
        card_id = %airc_core::shown_id::shown_form(card_id.as_uuid()),
        room = %room_name,
        "a by-id mutation targeted a card outside the current room — switched to \
         the card's room and retried (accept-or-redirect, never refuse-and-instruct)"
    );
    Some(room_name)
}
