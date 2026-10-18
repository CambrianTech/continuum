//! `benchmark/rounds`: round projection enriched from the board and verdicts.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};
use super::*;

// ---------------------------------------------------------------------------
// benchmark/rounds — the ROUND lifecycle, askable (#371)
// ---------------------------------------------------------------------------

/// No parameters. Rounds in flight are few (usually one) and each is a handful of
/// fields, so paging and filtering would be ceremony over a list you always want whole.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRoundsParams.ts"
)]
pub struct BenchmarkRoundsParams {}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRoundsResult.ts"
)]
pub struct BenchmarkRoundsResult {
    /// Rounds currently in flight. EMPTY is a real, unambiguous answer — "no round is
    /// running" — and never "the question could not be reached". A round is removed the
    /// instant its last card settles, so a round that finished is absent by design and
    /// its END is on the `bench.round.done` probe.
    pub rounds: Vec<crate::cognition::bench_round::RoundSnapshot>,
    /// How many are in flight, so a reader that only needs the yes/no does not have to
    /// interpret an array's length.
    pub in_flight: usize,
}

#[derive(Default)]
pub struct BenchmarkRounds;

#[async_trait]
impl ActionCommand for BenchmarkRounds {
    const NAME: &'static str = "benchmark/rounds";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Every benchmark ROUND in flight, with its stage — the lifecycle question answered \
         by a QUERY instead of by probe archaeology (#371). A round is the card set one \
         `benchmark/dispatch` posted; its id IS its run room's id. Each row carries stage \
         (working|done), dispatched/settled/remaining, and the work DRIVER (citizen — works \
         in the room and feeds the curriculum — vs detached_solve). An EMPTY list is a real \
         answer meaning no round is running, never a failure to reach the question; a round \
         is dropped the moment its last card settles, and that END is the `bench.round.done` \
         probe. This is what a fresh driver reads to answer 'has it started, is it stuck, is \
         it done' with zero log reads.";
    type Params = BenchmarkRoundsParams;
    type Output = BenchmarkRoundsResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        _p: BenchmarkRoundsParams,
    ) -> Result<BenchmarkRoundsResult, CommandError> {
        let mut rounds = crate::cognition::bench_round::live_rounds();
        // Merge run-ledger facts so the row answers "is it stuck" itself
        // (2026-09-01: `working 0/8` rendered identically for three hours of
        // thrash and for a healthy grind — the description above promised
        // "is it stuck" and the row could not say). Blocking fs scan off the
        // async worker; a scan failure degrades to tracker-only rows, which
        // is the honest board on a node with no ledger yet.
        let runs = tokio::task::spawn_blocking(|| {
            scan_run_cards(None, ROUNDS_ENRICH_SCAN_LIMIT)
                .map(|s| s.cards)
                .unwrap_or_default()
        })
        .await
        .unwrap_or_default();  // unwrap_or: unreadable = empty, the report shows the tracker's view
        let facts: Vec<crate::cognition::bench_round::CardRunFacts> =
            runs.iter().map(card_run_facts).collect();
        crate::cognition::bench_round::enrich_rounds(
            &mut rounds,
            &facts,
            crate::time::EpochMs::now().millis(),
        );
        enrich_rounds_from_board_and_verdicts(&mut rounds).await;
        Ok(BenchmarkRoundsResult {
            in_flight: rounds.len(),
            rounds,
        })
    }
}

/// BOARD TRUTH + VERDICT FILES onto the round report — the durable records, read at
/// report time (the round tracker knows the deck; the board knows who holds what and
/// when; the verdict file knows the grade and when). This is what makes a round's
/// time-to-claim, time-to-settle and resolve rate readable by a stranger after any
/// number of reboots. Best-effort: an unreadable board leaves the tracker's view.
pub(crate) async fn enrich_rounds_from_board_and_verdicts(
    rounds: &mut [crate::cognition::bench_round::RoundSnapshot],
) {
    let registry = crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global();
    let reader = registry.as_ref().and_then(|reg| reg.any_live_citizen());
    for round in rounds.iter_mut() {
        // Verdicts: keyed by instance, independent of the board.
        for card in round.cards.iter_mut() {
            if card.instance.is_empty() {
                continue;
            }
            if let Some(v) = crate::cognition::swe_bench::read_verdict(&card.instance) {
                card.resolved = Some(v.resolved);
                card.graded_at_ms = (v.graded_at_ms > 0).then_some(v.graded_at_ms);
            }
        }
        let Some(rt) = reader.as_ref() else {
            continue;
        };
        let Ok(room_id) = round.round_id.parse::<uuid::Uuid>() else {
            continue;
        };
        let Ok(set) = rt.airc().subscription_set().await else {
            continue;
        };
        let Some(room) = set
            .all()
            .map(|sub| sub.as_room())
            .find(|r| r.channel.as_uuid() == room_id)
        else {
            continue; // the reader is not resident in this run room — tracker view stands
        };
        let Ok(board) = rt.airc().work_board_in(&room).await else {
            continue;
        };
        let board = board.snapshot();
        for card in round.cards.iter_mut() {
            let Some(bc) = board
                .cards
                .iter()
                .find(|c| c.card_id.as_uuid().to_string() == card.card_id)
            else {
                continue;
            };
            card.board_state = serde_json::to_value(bc.state)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default();  // unwrap_or: unreadable = empty, the report shows the tracker's view
            card.owner = bc
                .owner
                .map(|o| {
                    registry
                        .as_ref()
                        .and_then(|reg| reg.get(o.as_uuid()))
                        .map(|rt| rt.agent_name().to_string())
                        .unwrap_or_else(|| o.as_uuid().to_string()[..8].to_string())  // unwrap_or: no live runtime for the owner = her short id, still addressable
                })
                .unwrap_or_default();  // unwrap_or: unreadable = empty, the report shows the tracker's view
            // An owner is a CLAIM attribute: the board keeps the last holder's id on a
            // card set back to open (2026-09-05: seven reopened cards read as "owned" and
            // the academy board — and the operator — counted 12 in hands with 5 held).
            if matches!(card.board_state.as_str(), "open" | "closed" | "merged") {
                card.owner.clear();
            }
            card.created_at_ms = Some(bc.created_at_ms);
            card.updated_at_ms = Some(bc.updated_at_ms);
        }
    }
}

/// How deep the rounds projection scans the run ledger for enrichment. Bounded
/// so a node with hundreds of banked verdicts pays a bounded scan; deep enough
/// that every in-flight round's instances are covered (a round is ≤ dozens of
/// cards and its runs are by definition recent).
const ROUNDS_ENRICH_SCAN_LIMIT: usize = 200;

/// `BenchRunCard` → the minimal facts `bench_round::enrich_rounds` folds.
/// ONE mapping, shared by the command above and the positron bench emitter —
/// two hand-rolled copies would drift ([[the-same-bug-at-two-sites-is-a-missing-constraint]]).
pub(crate) fn card_run_facts(card: &BenchRunCard) -> crate::cognition::bench_round::CardRunFacts {
    crate::cognition::bench_round::CardRunFacts {
        card_id: card
            .run_id
            .strip_prefix("claim-")
            .and_then(|rest| rest.get(..36))
            .filter(|id| uuid::Uuid::parse_str(id).is_ok())
            .map(str::to_string),
        instance: card.instance.clone().unwrap_or_default(),
        solver: card.solver.clone(),
        phase: card.phase.clone(),
        acts: card.acts,
        patch_bytes: card.patch_bytes,
        last_activity_ms: card.last_activity_ms,
        resolved: card.resolved,
    }
}
