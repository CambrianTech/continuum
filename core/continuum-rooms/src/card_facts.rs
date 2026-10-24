//! The board's plain facts: the default claim lease, a state's wire spelling, which states
//! conclude a card, and the board's 8-char short id.

use airc_lib::CardState;

/// Default claim lease (ms) — 30 min — when the caller doesn't set one. The claim
/// is heartbeat-extendable; this is just the initial TTL.
///
/// Shared with the persona runtime's heartbeat pump
/// (`persona::airc_runtime`), which renews a citizen's live claims on the
/// presence cadence. ONE lease length, one place — a renewal that used a different
/// TTL than the claim would make "how long is my hold good for" answerable two ways.
pub const DEFAULT_CLAIM_TTL_MS: u64 = 30 * 60 * 1000;

/// Inverse of [`parse_state`] — the wire spelling of a card state.
pub fn state_str(s: &CardState) -> &'static str {
    match s {
        CardState::Open => "open",
        CardState::Claimed => "claimed",
        CardState::InProgress => "in_progress",
        CardState::Blocked => "blocked",
        CardState::Review => "review",
        CardState::Merged => "merged",
        CardState::Closed => "closed",
    }
}

/// Is this card CONCLUDED: merged or closed, its lane over. Narrower than airc's
/// `CardState::is_settled`, which also counts Review (past claiming, but a card in review
/// is still live work: its holder's ledger is her resumption state). The one place that
/// says which states end a card's lane here (the transition gate's `finishing`, and which
/// card ledgers leave a persona's context).
pub fn is_concluded(s: &CardState) -> bool {
    matches!(s, CardState::Merged | CardState::Closed)
}

