//! Per-scan lapse recording (card 13d7e47a, gaps C/D of 1fc73f18). The
//! heartbeat's denial branch calls [`record_lapsed_holds`] according to a
//! [`LapseScanSchedule`]: at most once per lease expiry crossing, and not at
//! all while nothing is left that can lapse. This module owns the board scan,
//! the episode-key prune (pure: [`plan_lapse_scan`], tested), the per-card
//! on-disk re-check (off the async thread), and the one probe row per lapse
//! episode. Moved out of `persona_airc_runtime.rs` as a pure move — same code,
//! its own file.

use std::collections::HashSet;

use super::lapse_evidence::{lapse_evidence_recheck, LapseEvidence};
use super::*;

/// What the next denial tick owes for lapse scanning (card 13d7e47a). One
/// value per state: a failed fetch reports [`LapseScanSchedule::DueNow`]
/// (retry on the next tick, prune nothing) — it must not read as "nothing left
/// to lapse" — while an exhaustive scan that found no live hold at all goes
/// quiescent until renewals resume. While denied her holds only change by
/// lapsing (no turns → no claims), so [`LapseScanSchedule::DueAt`] carries the
/// earliest live lease from the last scan and ticks before it fetch nothing:
/// one board read per expiry crossing, none in quiescence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LapseScanSchedule {
    /// Scan on this tick (first denial, or a failed fetch that must retry).
    DueNow,
    /// Earliest live lease expiry from the last scan (ms); scan once `now_ms`
    /// reaches it.
    DueAt(u64),
    /// Every hold has lapsed: nothing can newly lapse while she stays denied —
    /// no rescan until renewals resume.
    Quiescent,
}

impl LapseScanSchedule {
    /// Whether a scan is owed at `now_ms`. A failed fetch reports [`DueNow`], so
    /// it retries next tick; quiescence waits for the denial itself to end (the
    /// caller resets on resume).
    pub(super) fn is_due(self, now_ms: u64) -> bool {
        match self {
            LapseScanSchedule::DueNow => true,
            LapseScanSchedule::DueAt(t) => now_ms >= t,
            LapseScanSchedule::Quiescent => false,
        }
    }
}

/// The pure half of one lapse scan (testable without a board or git): prune the
/// recorded-episode keys against her live lease generations, then fold her
/// holds into which episodes to record this tick and what the next scan owes.
/// `recorded_lapses` is pruned IN PLACE — only on an authoritative read; a
/// failed fetch never reaches here (the caller reports DueNow instead).
pub(super) struct LapseScanPlan {
    /// Her holds whose lease has lapsed and whose episode is not yet recorded —
    /// each gets its re-check + probe row.
    pub to_record: Vec<airc_lib::WorkCard>,
    /// What the next denial tick owes after this scan.
    pub schedule: LapseScanSchedule,
}

pub(super) fn plan_lapse_scan(
    cards: &[airc_lib::WorkCard],
    me: &airc_core::PeerId,
    now_ms: u64,
    recorded_lapses: &mut HashSet<(Uuid, u64)>,
) -> LapseScanPlan {
    // Prune episode keys whose generation has ended: a card that left her live
    // holds (transferred, settled) or one with a new lease (different expiry) no
    // longer needs its old key. A lapsed hold still reads as her Claimed/
    // InProgress with an expired lease, so it keeps its key — and when the fetch
    // failed we prune nothing at all, because this function is never reached.
    let live_generations: HashSet<(Uuid, u64)> = cards
        .iter()
        .filter(|c| {
            c.owner == Some(*me)
                && matches!(
                    c.state,
                    airc_work::model::CardState::Claimed | airc_work::model::CardState::InProgress
                )
        })
        .filter_map(|c| c.claim_expires_at_ms.map(|e| (c.card_id.as_uuid(), e)))
        .collect();
    recorded_lapses.retain(|k| live_generations.contains(k));

    let mut plan = LapseScanPlan {
        to_record: Vec::new(),
        schedule: LapseScanSchedule::Quiescent, // no live hold seen yet — none left to lapse
    };
    for c in cards {
        if c.owner != Some(*me)
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
            plan.schedule = LapseScanSchedule::DueAt(match plan.schedule {
                LapseScanSchedule::DueAt(t) => t.min(expiry),
                _ => expiry,
            });
            continue; // still live — not lapsed yet
        }
        let key = (c.card_id.as_uuid(), expiry);
        if recorded_lapses.insert(key) {
            plan.to_record.push(c.clone());
        }
    }
    plan
}

/// LAPSE-TIME OBSERVABILITY (card 13d7e47a, gaps C/D of 1fc73f18): denial is
/// what lapses her holds — record WHY at the moment a hold has in fact
/// lapsed. The pulse says idle; disk may disagree (a restart wipes the
/// stamp), so each lapsed checkout gets an on-disk re-check and its evidence
/// rides the row: one per lapse episode, keyed by lease generation.
///
/// Fetch discipline lives in [`LapseScanSchedule`], which this returns for the
/// next tick; a failed fetch reports `DueNow` (retry next tick) exactly as it
/// always has. The blocking re-check runs off the async thread so a slow
/// checkout can never pin a tokio worker that also serves her turns.
pub(super) async fn record_lapsed_holds(
    airc: &Airc,
    persona_id: Uuid,
    agent_name: &str,
    idle: Option<u64>,
    now_ms: u64,
    recorded_lapses: &mut HashSet<(Uuid, u64)>,
) -> LapseScanSchedule {
    let Ok(board) = airc
        .work_board_complete(airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
        .await
    else {
        return LapseScanSchedule::DueNow; // fetch failed — prune nothing (unreadable is not clean), retry next tick
    };
    let me = airc.peer_id();
    let cards = board.snapshot().cards;
    let plan = plan_lapse_scan(&cards, &me, now_ms, recorded_lapses);
    for card in &plan.to_record {
        // The re-check is BLOCKING (two bounded git reads). `me` is this node's
        // PeerId; the evidence fold keys by raw uuid, so hand the closure its own
        // copies of both.
        let peer = me.as_uuid();
        let owned = card.clone();
        let evidence = match tokio::task::spawn_blocking(move || {
            lapse_evidence_recheck(&peer, &owned)
        })
        .await
        {
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
            card_id = %card.card_id.as_uuid(),
            idle_ms = ?idle,
            recent_evidence = evidence.recent_within_ttl,
            checkout_present = ?evidence.checkout_present,
            head_commit_age_ms = ?evidence.head_commit_age_ms,
            dirty_files = ?evidence.dirty_files,
            "claim lapsed while renewal was denied (idle_ms=None: she never \
             thought in this process) — why the denial stood and what work \
             evidence disk still held"
        );
    }
    plan.schedule
}

#[cfg(test)]
mod tests {
    use super::*;

    fn me() -> airc_core::PeerId {
        // Deterministic identity for the fold under test — the uuid newtype's
        // from_u128 exists exactly for this.
        airc_core::PeerId::from_u128(7)
    }

    fn card(
        title: &str,
        owner: Option<airc_core::PeerId>,
        state: airc_work::model::CardState,
        expiry: Option<u64>,
    ) -> airc_lib::WorkCard {
        airc_lib::WorkCard {
            card_id: airc_work::WorkCardId::new(),
            repo: airc_work::RepoId::new("acme/continuum").expect("valid repo id in fixture"),
            title: title.to_string(),
            body: None,
            priority: airc_work::model::Priority::P2,
            lane_id: None,
            state,
            owner,
            claim_provenance: None,
            claim_id: None,
            claim_expires_at_ms: expiry,
            last_heartbeat_at_ms: None,
            pull_request: None,
            created_by: airc_core::PeerId::new(),
            created_at_ms: 1_000_000,
            updated_at_ms: 1_000_000,
            reviews: None,
            submissions: Vec::new(),
            last_submission_rejection: None,
        }
    }

    #[test]
    // what this catches: the same lapsed (card, expiry) seen on several denial ticks
    // recorded one row per tick — dedup by lease generation is what keeps it to once.
    fn lapsed_episode_records_once_then_dedups() {
        let me = me();
        let c = card(
            "lapsed hold",
            Some(me),
            airc_work::model::CardState::Claimed,
            Some(1_000),
        );
        let mut recorded: HashSet<(Uuid, u64)> = HashSet::new();

        let first = plan_lapse_scan(&[c.clone()], &me, 50_000, &mut recorded);
        assert_eq!(
            first.to_record.len(),
            1,
            "the lapse episode is recorded once"
        );
        assert_eq!(recorded.len(), 1);

        // Same board on the next scan: the key says this generation was already
        // written — no second row.
        let again = plan_lapse_scan(&[c], &me, 60_000, &mut recorded);
        assert!(
            again.to_record.is_empty(),
            "dedup by (card_id, lease expiry)"
        );
        assert_eq!(recorded.len(), 1);
    }

    #[test]
    // what this catches: a re-claimed card under a fresh lease that lapses again would
    // be wrongly deduped as the old episode — new generation must record again.
    fn new_lease_generation_is_a_new_episode() {
        let me = me();
        let mut recorded: HashSet<(Uuid, u64)> = HashSet::new();

        // Generation one lapses and is recorded.
        let g1 = card(
            "hold",
            Some(me),
            airc_work::model::CardState::Claimed,
            Some(1_000),
        );
        let first = plan_lapse_scan(&[g1.clone()], &me, 50_000, &mut recorded);
        assert_eq!(first.to_record.len(), 1);

        // Re-claimed under a fresh lease that has itself now lapsed: same card,
        // new generation — a genuinely new episode.
        let mut g2 = card(
            "hold",
            Some(me),
            airc_work::model::CardState::Claimed,
            Some(9_000),
        );
        g2.card_id = g1.card_id;
        let second = plan_lapse_scan(&[g2], &me, 50_000, &mut recorded);
        assert_eq!(second.to_record.len(), 1, "new lease generation re-records");
    }

    #[test]
    // what this catches: recorded episode keys grow forever — a card that leaves her
    // live holds (transferred, settled) must have its stale key pruned per scan.
    fn keys_prune_when_the_hold_leaves() {
        let me = me();
        let other = airc_core::PeerId::from_u128(9);
        let mut recorded: HashSet<(Uuid, u64)> = HashSet::new();

        let c = card(
            "transferred hold",
            Some(me),
            airc_work::model::CardState::Claimed,
            Some(1_000),
        );
        plan_lapse_scan(&[c.clone()], &me, 50_000, &mut recorded);
        assert!(recorded.contains(&(c.card_id.as_uuid(), 1_000)));

        // The card leaves her holds: its generation has ended and the key is
        // pruned on the next authoritative read — the set cannot grow forever.
        let mut gone = c;
        gone.owner = Some(other);
        plan_lapse_scan(&[gone], &me, 60_000, &mut recorded);
        assert!(recorded.is_empty(), "stale episode key pruned");
    }

    #[test]
    // what this catches: the fetch discipline — one board read per expiry crossing, on
    // the EARLIEST live expiry, and none at all once every hold has lapsed (quiescent).
    fn schedule_tracks_earliest_live_expiry_and_quiescence() {
        let me = me();
        let mut recorded: HashSet<(Uuid, u64)> = HashSet::new();

        // Two live holds: the next fetch is owed at the EARLIEST expiry.
        let late = card(
            "live B",
            Some(me),
            airc_work::model::CardState::InProgress,
            Some(200_000),
        );
        let early = card(
            "live A",
            Some(me),
            airc_work::model::CardState::Claimed,
            Some(100_000),
        );
        let p = plan_lapse_scan(&[late, early], &me, 50_000, &mut recorded);
        assert_eq!(p.schedule, LapseScanSchedule::DueAt(100_000));

        // A hold with no lease cannot lapse: alone it is quiescent.
        let bare = card(
            "no lease",
            Some(me),
            airc_work::model::CardState::Claimed,
            None,
        );
        let q = plan_lapse_scan(&[bare], &me, 50_000, &mut recorded);
        assert_eq!(q.schedule, LapseScanSchedule::Quiescent);

        // Mixed: record the lapsed one now, schedule on what stays live.
        let mut fresh = HashSet::new();
        let lapsed = card(
            "lapsed",
            Some(me),
            airc_work::model::CardState::Claimed,
            Some(1_000),
        );
        let live = card(
            "live C",
            Some(me),
            airc_work::model::CardState::InProgress,
            Some(300_000),
        );
        let m = plan_lapse_scan(&[lapsed, live], &me, 50_000, &mut fresh);
        assert_eq!(m.to_record.len(), 1);
        assert_eq!(m.schedule, LapseScanSchedule::DueAt(300_000));
    }
}
