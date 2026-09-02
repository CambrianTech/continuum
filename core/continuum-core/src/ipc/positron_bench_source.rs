//! Benchmark-board emitter — the ACADEMY right-rail's source (#329: a
//! benchmark IS a live room; the run rows ARE the panel).
//!
//! Own task + `tokio::time::interval` + a store into the served [`Substrate`]
//! — the same emit shape as [`crate::ipc::positron_serving_source`]. ONE feed,
//! already owned by the process: the run-ledger scan behind `benchmark/runs`
//! ([`crate::commands::benchmark::scan_run_cards`] — the one projection every
//! consumer folds, never a parallel file scrape). The scan is synchronous fs
//! I/O, so it runs on `spawn_blocking` per the concurrency guide; a failed
//! scan (no progress dir yet) publishes the honest EMPTY board, never a
//! fabricated row.
//!
//! Store-on-change: run ledgers move on act cadence (minutes), so identical
//! consecutive scans are the common case — publishing them would re-render
//! every subscribed client per tick for nothing. The serving source publishes
//! unconditionally because its series genuinely changes every sample; this
//! one compares and skips.

use std::time::Duration;

use continuum_positron::bench::{BenchRoundCardRow, BenchRoundRow, BenchRunRow, BenchViewState};
use continuum_positron::{StateBuilder, Substrate};

use crate::cognition::bench_round::RoundSnapshot;
use crate::commands::benchmark::{card_run_facts, scan_run_cards, BenchRunCard};

/// Emit cadence. The board is a minutes-scale instrument (acts land every
/// 2-6 min); 5s keeps a verdict visible within a beat of landing without
/// rescanning the ledger dir at gauge rates.
const SAMPLE_INTERVAL: Duration = Duration::from_secs(5);

/// Rows the board carries — the newest N runs (matches the command default).
const BOARD_LIMIT: usize = 20;

/// Scan depth for round enrichment — deep enough to cover every in-flight
/// round's instances (same bound as the `benchmark/rounds` command; the
/// display rail still shows only [`BOARD_LIMIT`]).
const ROUND_ENRICH_SCAN_LIMIT: usize = 200;

fn row_of(card: BenchRunCard) -> BenchRunRow {
    // A `claim-<uuid>` run id CARRIES its card — the round (run room) and the
    // recorded solve room derive from the tracker, making every board row a
    // navigable door instead of a dead readout (Joel, 2026-08-31: "see
    // current runs and navigate to them").
    let card_uuid = card
        .run_id
        .strip_prefix("claim-")
        .and_then(|t| uuid::Uuid::parse_str(t).ok());
    let round_id = card_uuid.and_then(crate::cognition::bench_round::room_for_card);
    let activity = card_uuid.and_then(crate::cognition::bench_round::card_activity);
    let solve_room = activity.as_ref().map(|a| a.solve_room);
    let solve_room_name = activity
        .as_ref()
        .filter(|a| !a.room_name.is_empty())
        .map(|a| a.room_name.clone());
    BenchRunRow {
        round_id: round_id.map(|u| u.to_string()),
        solve_room: solve_room.map(|u| u.to_string()),
        solve_room_name,
        run_id: card.run_id,
        instance: card.instance,
        solver: card.solver,
        phase: card.phase,
        stalled: card.stalled,
        attempt: card.attempt,
        max_attempts: card.max_attempts,
        age_secs: card.age_secs,
        acts: card.acts,
        patch_bytes: card.patch_bytes,
        resolved: card.resolved,
        fail_to_pass: card.fail_to_pass,
        pass_to_pass: card.pass_to_pass,
        failed_tests: card.failed_tests,
        infra_error: card.infra_error,
    }
}

/// The LIVE exam as a run row — `cognition/eval`'s in-flight pass, folded from
/// its watch snapshot into the same wire every ledger-scanned run rides. Joel
/// (2026-08-23): "monitors need to monitor the work itself … not be blind and
/// polling" — the exam publishes per-graded-task events; this is the rail's
/// subscription. `None` when no pass is grading. A snapshot from a PRIOR run
/// (run_id mismatch) contributes only the bare active row — a ghost grade must
/// never render as this run's progress.
fn live_exam_row() -> Option<BenchRunRow> {
    let run_id = crate::cognition::eval::live_eval_run_id()?;
    let snap = crate::cognition::eval::subscribe_eval_progress()
        .borrow()
        .clone()
        .filter(|s| s.run_id.as_deref() == Some(run_id.as_str()));
    let now = crate::persona::trace::now_ms();
    Some(match snap {
        Some(s) => BenchRunRow {
            round_id: None,
            solve_room: None,
            solve_room_name: None,
            run_id,
            instance: Some(format!("exam · last: {}", s.current_task)),
            solver: Some("cognition/eval".to_string()),
            phase: if s.last_ok { "active · last pass".into() } else { "active · last fail".into() },
            stalled: false,
            attempt: Some(s.done),
            max_attempts: Some(s.total),
            age_secs: now.saturating_sub(s.updated_at_ms) / 1000,
            acts: None,
            patch_bytes: None,
            resolved: None,
            fail_to_pass: None,
            pass_to_pass: None,
            failed_tests: Vec::new(),
            infra_error: None,
        },
        None => BenchRunRow {
            round_id: None,
            solve_room: None,
            solve_room_name: None,
            run_id,
            instance: Some("exam · provisioning".to_string()),
            solver: Some("cognition/eval".to_string()),
            phase: "active".into(),
            stalled: false,
            attempt: None,
            max_attempts: None,
            age_secs: 0,
            acts: None,
            patch_bytes: None,
            resolved: None,
            fail_to_pass: None,
            pass_to_pass: None,
            failed_tests: Vec::new(),
            infra_error: None,
        },
    })
}

/// Core round snapshot → wire round row. Lossless: a field dropped here is a
/// scoreboard that lies by omission (same contract as `row_of`).
fn round_row_of(s: RoundSnapshot) -> BenchRoundRow {
    BenchRoundRow {
        round_id: s.round_id,
        benchmark: s.benchmark,
        stage: s.stage,
        dispatched: s.dispatched as u32,
        settled: s.settled as u32,
        remaining: s.remaining as u32,
        driver: s.driver,
        cards: s
            .cards
            .into_iter()
            .map(|c| BenchRoundCardRow {
                card_id: c.card_id,
                instance: c.instance,
                assignee: c.assignee,
                solve_room_name: c.solve_room_name,
                state: c.state,
                acts: c.acts,
                patch_bytes: c.patch_bytes,
                last_act_secs: c.last_act_secs,
                resolved: c.resolved,
            })
            .collect(),
        verdict: s.verdict,
        idle_secs: s.idle_secs,
    }
}

/// Spawn the bench-board emitter: scan → fold → publish `kind="bench"`.
///
/// Dual render targets from ONE fold (#426): `substrate` is the websocket
/// store human eyes read; `mind_substrate` is `global_bench_substrate()`,
/// the store a citizen's `ViewStateRagSource::<BenchViewState>` reads. The
/// SAME `builder.session(view)` revision lands in both, so a mind and a
/// screen can never disagree about the board — the roster repair's
/// one-definition-two-targets contract applied to the bench outlier.
pub fn spawn_bench_emitter(
    rt: &tokio::runtime::Handle,
    substrate: Substrate,
    mind_substrate: Substrate,
) {
    rt.spawn(async move {
        // Sole writer of the "bench" kind → its own standalone Revisions well.
        let builder = StateBuilder::standalone();
        let mut ticker = tokio::time::interval(SAMPLE_INTERVAL);
        let mut last: Option<BenchViewState> = None;
        loop {
            ticker.tick().await;
            // Ledger scan is blocking fs I/O — off the async worker per the
            // concurrency guide. A scan error (progress dir absent on a fresh
            // node) folds as the honest empty board.
            // `.cards` only: the board renders a fixed-height rail, so `BOARD_LIMIT` is a
            // DISPLAY bound, not a claim about how many runs exist. The scan's `matched`
            // total belongs on the `benchmark/runs` receipt, where a caller is asking
            // "how is the benchmark going" and a silent truncation would answer wrongly.
            // If the rail ever grows a "+N more" affordance, that is what it reads.
            // One scan serves BOTH consumers: the display rail takes the
            // newest BOARD_LIMIT; round enrichment folds over the whole
            // window (a round's runs are recent by definition, so the same
            // depth the `benchmark/rounds` command scans suffices).
            let cards = tokio::task::spawn_blocking(|| {
                scan_run_cards(None, ROUND_ENRICH_SCAN_LIMIT)
                    .map(|s| s.cards)
                    // safe: a scan error means the progress dir is absent (fresh node) or
                    // unreadable. Empty is the HONEST board for that — the module header
                    // states the rule: never a fabricated row. An emitter that panicked
                    // here would take the whole board down for every viewer instead.
                    .unwrap_or_default() // safe: see the 4 lines above
            })
            .await
            // safe: JoinError only if the blocking task panicked or was cancelled at
            // shutdown. Same answer for the same reason — publish the empty board, do
            // not propagate a panic into the emitter's own tick loop.
            .unwrap_or_default(); // safe: see the 3 lines above
            // The round tracker's own truth (#371), with run-ledger facts
            // merged so the scoreboard can say `unstarted`/`grinding`/
            // `stalled` instead of an undifferentiated `working` (the SAME
            // fold the `benchmark/rounds` command runs — one projection, two
            // consumers, no drift).
            let mut rounds = crate::cognition::bench_round::live_rounds();
            let facts: Vec<crate::cognition::bench_round::CardRunFacts> =
                cards.iter().map(card_run_facts).collect();
            crate::cognition::bench_round::enrich_rounds(
                &mut rounds,
                &facts,
                crate::persona::trace::now_ms(),
            );
            let view = BenchViewState {
                // The live exam leads the rail (it is the work happening NOW);
                // ledger-scanned rows follow.
                runs: live_exam_row()
                    .into_iter()
                    .chain(cards.into_iter().take(BOARD_LIMIT).map(row_of))
                    .collect(),
                rounds: rounds.into_iter().map(round_row_of).collect(),
                sample_interval_ms: SAMPLE_INTERVAL.as_millis() as u64,
            };
            // age_secs / last_act_secs / idle_secs tick every scan, which
            // would defeat store-on-change; compare with ages zeroed so only
            // REAL row changes publish.
            let comparable = |v: &BenchViewState| BenchViewState {
                runs: v
                    .runs
                    .iter()
                    .map(|r| BenchRunRow {
                        age_secs: 0,
                        ..r.clone()
                    })
                    .collect(),
                rounds: v
                    .rounds
                    .iter()
                    .map(|r| BenchRoundRow {
                        idle_secs: None,
                        cards: r
                            .cards
                            .iter()
                            .map(|c| BenchRoundCardRow {
                                last_act_secs: None,
                                ..c.clone()
                            })
                            .collect(),
                        // verdict flips (grinding→stalled) are REAL changes —
                        // they ride the compare and publish.
                        ..r.clone()
                    })
                    .collect(),
                sample_interval_ms: v.sample_interval_ms,
            };
            if last.as_ref().map(&comparable) == Some(comparable(&view)) {
                continue;
            }
            last = Some(view.clone());
            let envelope = std::sync::Arc::new(builder.session(view));
            // One allocation, two sinks (2026-08-23 audit): the by-value clone
            // deep-copied the whole board per publish for the second target.
            substrate.store_shared(std::sync::Arc::clone(&envelope));
            mind_substrate.store_shared(envelope);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the card→row fold is LOSSLESS for the board facts —
    // a field silently dropped here is a board that lies by omission (the
    // instance/attempt columns were the whole point of #2266).
    #[test]
    fn card_folds_to_row_losslessly() {
        let card = BenchRunCard {
            run_id: "r1".into(),
            instance: Some("sympy__sympy-21055".into()),
            attempt: Some(2),
            max_attempts: Some(3),
            solver: Some("anon".into()),
            phase: "active".into(),
            stalled: false,
            last_activity_ms: 1,
            age_secs: 42,
            acts: Some(10),
            files_changed: vec!["a.py".into()],
            files_examined: vec![],
            resolved: Some(false),
            fail_to_pass: Some("0/1".into()),
            pass_to_pass: Some("13/13".into()),
            patch_bytes: Some(1295),
            failed_tests: vec!["test_x".into()],
            infra_error: None,
        };
        let row = row_of(card);
        assert_eq!(row.instance.as_deref(), Some("sympy__sympy-21055"));
        assert_eq!(row.attempt, Some(2));
        assert_eq!(row.max_attempts, Some(3));
        assert_eq!(row.patch_bytes, Some(1295));
        assert_eq!(row.pass_to_pass.as_deref(), Some("13/13"));
        assert_eq!(row.failed_tests, vec!["test_x".to_string()]);
    }
}
