#![cfg(test)]

use super::*;

/// A citizen must never be silenced by her own last tool result.
mod an_action_result_cannot_mute_a_citizen;

/// A citizen must never be silenced by her ENRICHMENT not fitting.
mod grounding_never_mutes;

fn history_with_stimulus(stimulus: &str) -> Workspace {
    use crate::cognition::workspace::Burst;
    let room = crate::identity::ActivityRoom::from_uuid(Uuid::new_v4()).unwrap();
    Workspace::new(Burst::from_turns(
        room,
        vec![
            BurstTurn::attributed(
                false,
                "Peer",
                "old chatter line\n".repeat(2000),
                Some(1),
            ),
            BurstTurn::attributed(false, "Peer", stimulus, Some(2)),
        ],
    ))
}

/// A grounding block built from a LIST of units, sized like the live work
/// board (leads first, then one line per card).
fn board_like(units: usize) -> Contribution {
    let mut parts = vec![
        "[your work] you HOLD 1 card — 0dd1123c \"wire the gather fallback\"".to_string(),
        "[available work] 60 card(s) are claimable — pick one up with work/claim"
            .to_string(),
    ];
    for i in 0..units {
        parts.push(format!(
            "card {i:08x} [Open] \"a card whose title is about as long as a real one\" \
             (Normal, unclaimed)"
        ));
    }
    let content = parts.join("\n");
    Contribution::context(
        FacultyId::Custom("room-kanban".to_string()),
        content,
        0.9,
        "board",
    )
    .with_parts(parts)
}

/// Card dec1a7ff: #3829 made the tool surface a per-turn decision but left the
/// ACCOUNTING pricing the full registry, so a work turn's message budget was
/// short by (full − hands) and trimmed conversation to fit room that was never
/// occupied. These pin what is COUNTED against what is SENT.
mod the_budget_prices_the_surface_it_actually_sends;

// ─── Stable prefix first (card c119ace7) ───────────────────────────────
//
// The prompt cache reuses only a common PREFIX. These pin the whole-prompt
// order at the live boundary (`prompt_view`): conversation first, then the
// standing grounding, held in her epoch so a re-ranked bid moves nothing; the
// clock and the presence framing in the volatile tail, after the conversation;
// nothing dated or counted in the system message.
mod stable_prefix_first;

mod turn_window;

mod reply_reserve;

mod prefix_and_grounding;

mod tool_surface_and_fit;

mod binding_and_history;
