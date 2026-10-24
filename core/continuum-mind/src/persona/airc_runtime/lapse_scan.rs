//! Per-scan lapse recording (card 13d7e47a, gaps C/D of 1fc73f18). The
//! heartbeat's denial branch calls [`record_lapsed_holds`] at most once per
//! lease expiry crossing; this module owns the board scan, the episode-key
//! prune, the per-card on-disk re-check (off the async thread), and the one
//! probe row per lapse episode. Moved out of `persona_airc_runtime.rs` as a
//! pure move — same code, its own file.

use std::collections::HashSet;

use super::lapse_evidence::{lapse_evidence_recheck, LapseEvidence};
use super::*;

/// LAPSE-TIME OBSERVABILITY (card 13d7e47a, gaps C/D of 1fc73f18): denial is
/// what lapses her holds — record WHY at the moment a hold has in fact
/// lapsed. The pulse says idle; disk may disagree (a restart wipes the
/// stamp), so each lapsed checkout gets an on-disk re-check and its evidence
/// rides the row: one per lapse episode, keyed by lease generation.
///
/// Fetch discipline: while denied her holds only change by lapsing (no turns
/// → no claims), so the board is fetched at most once per expiry crossing —
/// `next_lapse_due` carries the earliest live lease from the last scan, and
/// ticks before it see nothing that can have lapsed. Returns the new
/// `next_lapse_due`: the earliest live lease expiry among her holds (None =
/// no live hold to lapse, or the fetch failed — in both cases the caller
/// simply retries on the next tick).
pub(super) async fn record_lapsed_holds(
    airc: &Airc,
    persona_id: Uuid,
    agent_name: &str,
    idle: Option<u64>,
    now_ms: u64,
    recorded_lapses: &mut HashSet<(Uuid, u64)>,
) -> Option<u64> {
    let Ok(board) = airc
        .work_board_complete(airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
        .await
    else {
        return None; // fetch failed — prune nothing (unreadable is not clean), retry next tick
    };
    let me = airc.peer_id();
    // Prune episode keys whose generation has ended: a card that left her live
    // holds (transferred, settled) or one with a new lease (different expiry) no
    // longer needs its old key. A lapsed hold still reads as her Claimed/
    // InProgress with an expired lease, so it keeps its key; and when the fetch
    // fails we prune nothing at all — unreadable is not clean.
    let live_generations: HashSet<(Uuid, u64)> = board
        .snapshot()
        .cards
        .iter()
        .filter(|c| {
            c.owner == Some(me)
                && matches!(
                    c.state,
                    airc_work::model::CardState::Claimed | airc_work::model::CardState::InProgress
                )
        })
        .filter_map(|c| c.claim_expires_at_ms.map(|e| (c.card_id.as_uuid(), e)))
        .collect();
    recorded_lapses.retain(|k| live_generations.contains(k));
    let mut next_due: Option<u64> = None;
    for c in board.snapshot().cards.iter() {
        if c.owner != Some(me)
            || !matches!(
                c.state,
                airc_work::model::CardState::Claimed | airc_work::model::CardState::InProgress
            )
        {
            continue;
        }
        let Some(expiry) = c.claim_expires_at_ms else {
            continue; // no lease to lapse
        };
        if expiry > now_ms {
            next_due = match next_due {
                Some(t) => Some(t.min(expiry)),
                None => Some(expiry),
            };
            continue; // still live — not lapsed yet
        }
        let key = (c.card_id.as_uuid(), expiry);
        if !recorded_lapses.insert(key) {
            continue; // this episode is already recorded
        }
        // The re-check is BLOCKING (two bounded git reads); run it off the async
        // thread so a slow checkout can never pin a tokio worker that also serves
        // her turns. `me` is this node's PeerId; the evidence fold keys by raw
        // uuid, so hand the closure its own copies of both.
        let peer = me.as_uuid();
        let card = c.clone();
        let evidence =
            match tokio::task::spawn_blocking(move || lapse_evidence_recheck(&peer, &card)).await {
                Ok(e) => e,
                Err(_) => {
                    // The check itself did not run: record it as unknown, never as an
                    // absent checkout.
                    LapseEvidence::unchecked()
                }
            };
        continuum_base::probe!(
            class = "persona.claim.lapse_recorded",
            persona_id = %persona_id,
            agent_name = %agent_name,
            card_id = %c.card_id.as_uuid(),
            // JUSTIFIED unwrap_or_else: `idle` is None only when she has never
            // thought in this process (no cognition stamp) — a real standing state,
            // not absent data; "never-thought" names it. Same idiom as the denial
            // probe at the call site.
            denial_reason = %idle
                .map(|ms| format!("idle_{ms}ms"))
                .unwrap_or_else(|| "never-thought".to_string()),
            recent_evidence = evidence.recent_within_ttl,
            checkout_present = ?evidence.checkout_present,
            head_commit_age_ms = ?evidence.head_commit_age_ms,
            dirty_files = ?evidence.dirty_files,
            "claim lapsed while renewal was denied — why the \
             denial stood and what work evidence disk still held"
        );
    }
    next_due
}
