//! What a work turn is handed to begin with: the held-card burst, her own last
//! thoughts on the work (resume, never re-orient), and the board anchor the
//! self-cycle speaks from. Pure text builders — no I/O, fully testable — carved
//! out of `service_loop` 2026-09-05.

use uuid::Uuid;

pub(crate) fn held_work_burst(held: &[&airc_lib::WorkCard], last_state: &[String]) -> String {
    use std::fmt::Write as _;
    let mut s = String::from(
        "[work turn] The room is quiet and your speak-turn is settled. This \
         turn is for your claimed work:\n",
    );
    for card in held {
        let id8: String = card.card_id.as_uuid().to_string().chars().take(8).collect();
        let _ = writeln!(s, "- card {id8} \"{}\"", card.title);
    }
    // HER LAST STATE LEADS (2026-09-04, measured on Freya: at 11:01 she had the
    // bug located — "lines 107-152, I can see it clearly" — and at 11:16 the
    // next work turn opened with "let me recall what I know"; twelve checkouts,
    // zero diffs after fourteen hours). The turn resumes from her own newest
    // thoughts on this work, oldest first, instead of re-orienting from the
    // room. Her words, unedited: state, not steering.
    if !last_state.is_empty() {
        s.push_str(
            "Your own last thoughts on this work, oldest first — resume from them; \
             do not re-orient:\n",
        );
        for line in last_state {
            let _ = writeln!(s, "  · {line}");
        }
    }
    s.push_str(
        "Your workspace holds the staged checkout (see [workspace-map] and \
         [active-work]). Continue the work with your tools — read, run, edit, \
         test. When this card is finished, or you can go no further, conclude by \
         passing with a reason on ONE line: 'PASS: done' (the work is complete \
         and in the workspace), 'PASS: blocked — <one line why>', or \
         'PASS: nothing' (nothing to contribute). 'PASS: done' concludes the \
         card, so use it only when the deliverable is really written. Speak only \
         to report a result or blocker to the room.",
    );
    s
}

/// Her own newest thoughts in the room (💭 lines), oldest first, each clipped —
/// the raw material of the resume block in [`held_work_burst`]. Pure: the
/// caller pages the durable store.
pub(crate) fn own_recent_thoughts(
    rows: &[crate::persona::durable_history::RoomRow],
    me: Uuid,
    keep: usize,
    max_chars: usize,
) -> Vec<String> {
    let mut mine: Vec<&crate::persona::durable_history::RoomRow> = rows
        .iter()
        .filter(|r| r.sender == me && r.text.starts_with('💭'))
        .collect();
    mine.sort_by_key(|r| r.occurred_at_ms);
    let start = mine.len().saturating_sub(keep);
    mine[start..]
        .iter()
        .map(|r| {
            let one_line = r.text.split_whitespace().collect::<Vec<_>>().join(" ");
            if one_line.chars().count() > max_chars {
                let cut: String = one_line.chars().take(max_chars).collect();
                format!("{cut}…")
            } else {
                one_line
            }
        })
        .collect()
}

pub(crate) fn work_board_anchor(deliveries: &[crate::persona::rag_budget::RagDelivery]) -> String {
    // Did the board source SPEAK this turn? "The board is empty" and "I never read the
    // board" are different facts about the world, and only one of them is knowable from an
    // absent delivery. Glass-boxed 2026-08-06 from Benchy's live capture: `room-kanban`
    // delivered NOTHING (grounding is last in the budget queue), the anchor rendered that
    // as "No open cards are visible", and she then said exactly that in-room for six turns
    // — while `work/list()` in her OWN working memory listed a full board in the same
    // prompt. She trusted the authoritative-sounding anchor over her own receipt.
    //
    // Never assert a fact about the world on behalf of a source that did not speak.
    // [[grounding-is-last-in-the-budget-queue-so-she-goes-blind-one-turn-in-ten]]
    let board_spoke = deliveries.iter().any(|d| d.source_id == "room-kanban");
    if !board_spoke {
        // Say nothing rather than something false. A silent anchor leaves her own
        // `work/list` receipt as the only board claim in the prompt — which is the truthful
        // one. An anchor that invents emptiness actively overrides it.
        return String::new();
    }
    let cards: Vec<&crate::persona::rag_budget::RagItem> = deliveries
        .iter()
        .filter(|d| d.source_id == "room-kanban")
        .flat_map(|d| d.items.iter())
        .filter(|i| i.metadata.get("card_id").is_some())
        .collect();
    /// The card's state as the TYPE, never as a string to be spelled correctly.
    ///
    /// `None` for an item whose metadata carries no parseable state — which is a real
    /// possibility (a future variant this build doesn't know) and must read as "unknown",
    /// never as a silent mismatch against a hardcoded spelling.
    fn state(i: &crate::persona::rag_budget::RagItem) -> Option<airc_work::CardState> {
        i.metadata
            .get("state")
            .and_then(|s| serde_json::from_value(s.clone()).ok())
    }
    /// Is this card's hold still good? Read as the structural fact the board source
    /// carries, never re-derived here — `claim_is_live` is the ONE definition and
    /// `room_board_source` already applied it. Absent (an older projection) reads as
    /// LIVE, so a missing field can never invent availability that isn't there.
    fn claim_live(i: &crate::persona::rag_budget::RagItem) -> bool {
        i.metadata
            .get("claim_live")
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
    }
    // AVAILABLE work is not just `Open` — it is anything nobody currently holds. A card
    // stuck in `Claimed` with a LAPSED lease is free to take, and treating it as taken is
    // what emptied this anchor while 19 takeable cards sat on the board (2026-08-06: every
    // resident read "nothing available" off their own expired claims and passed, for hours).
    // `state == Open` and "unheld" are different questions; ask the second one.
    use airc_work::CardState;
    let unclaimed: Vec<&str> = cards
        .iter()
        .filter(|i| {
            let unowned_open = state(i) == Some(CardState::Open)
                && i.metadata.get("owner").is_none_or(|o| o.is_null());
            // A lapsed hold on ANY non-terminal card is available work, whoever held it.
            let lapsed = !claim_live(i)
                && matches!(
                    state(i),
                    Some(CardState::Claimed | CardState::InProgress | CardState::Review)
                );
            unowned_open || lapsed
        })
        .map(|i| i.content.trim())
        .take(2)
        .collect();
    // Exhaustive over the enum, so ADDING a variant to `CardState` forces a decision here
    // instead of silently falling through as "not in flight". That is the whole point of
    // matching the type rather than a string.
    let in_flight: Vec<&str> = cards
        .iter()
        // Genuinely in flight = claimed AND the hold is still live. Without the liveness
        // term a lapsed card counts as both available and in-flight, and the anchor would
        // tell her the same card is free and busy in one breath.
        .filter(|i| claim_live(i))
        .filter(|i| match state(i) {
            Some(CardState::Claimed | CardState::InProgress | CardState::Review) => true,
            Some(CardState::Open | CardState::Blocked | CardState::Merged | CardState::Closed) => {
                false
            }
            None => false,
        })
        .map(|i| i.content.trim())
        .take(1)
        .collect();
    if unclaimed.is_empty() && in_flight.is_empty() {
        // Honest empty: no cards visible (empty board, unreadable board, or a
        // context whose board source abstained). Never invent work.
        "[anchor] No open cards are visible on this room's board right now — \
         proposing one (work/create) would add something new; restating prior \
         messages adds nothing."
            .to_string()
    } else {
        let facts: Vec<&str> = unclaimed.into_iter().chain(in_flight).collect();
        format!(
            "[anchor] Open work exists on this room's board right now: {}. \
             Restating prior messages adds nothing; acting on a card would.",
            facts.join("; ")
        )
    }
}
