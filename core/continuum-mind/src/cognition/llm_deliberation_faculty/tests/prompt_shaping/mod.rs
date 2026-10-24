#![cfg(test)]

use super::*;
use super::super::test_support::*;

mod turn_window;

/// A citizen must never be silenced by her own last tool result.
mod an_action_result_cannot_mute_a_citizen;

/// A citizen must never be silenced by her ENRICHMENT not fitting.
mod grounding_never_mutes;

mod reply_reserve;

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

mod prefix_and_grounding;

mod tool_surface_and_fit;

mod binding_and_history;

// ─── Stable prefix first (card c119ace7) ───────────────────────────────
//
// The prompt cache reuses only a common PREFIX. These pin the whole-prompt
// order at the live boundary (`prompt_view`): the settled grounding first, in ONE
// canonical order, then what grows (her actions, the conversation); the clock and
// the presence framing in the volatile tail, after the conversation; nothing
// dated or counted in the system message; and an act's prompt holds the whole
// previous view as its head.
mod stable_prefix_first;

/// Her view's turnover leaves a growth reserve, so a sliding room appends between turnovers.
mod a_busy_room_keeps_its_prefix;
