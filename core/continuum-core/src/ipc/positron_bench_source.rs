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

use continuum_positron::bench::{BenchRunRow, BenchViewState};
use continuum_positron::{StateBuilder, Substrate};

use crate::commands::benchmark::{scan_run_cards, BenchRunCard};

/// Emit cadence. The board is a minutes-scale instrument (acts land every
/// 2-6 min); 5s keeps a verdict visible within a beat of landing without
/// rescanning the ledger dir at gauge rates.
const SAMPLE_INTERVAL: Duration = Duration::from_secs(5);

/// Rows the board carries — the newest N runs (matches the command default).
const BOARD_LIMIT: usize = 20;

fn row_of(card: BenchRunCard) -> BenchRunRow {
    BenchRunRow {
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

/// Spawn the bench-board emitter: scan → fold → publish `kind="bench"`.
pub fn spawn_bench_emitter(rt: &tokio::runtime::Handle, substrate: Substrate) {
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
            let cards = tokio::task::spawn_blocking(|| {
                scan_run_cards(None, BOARD_LIMIT).unwrap_or_default()
            })
            .await
            .unwrap_or_default();
            let view = BenchViewState {
                runs: cards.into_iter().map(row_of).collect(),
                sample_interval_ms: SAMPLE_INTERVAL.as_millis() as u64,
            };
            // age_secs ticks every scan, which would defeat store-on-change;
            // compare with ages zeroed so only REAL row changes publish.
            let comparable = |v: &BenchViewState| BenchViewState {
                runs: v
                    .runs
                    .iter()
                    .map(|r| BenchRunRow {
                        age_secs: 0,
                        ..r.clone()
                    })
                    .collect(),
                sample_interval_ms: v.sample_interval_ms,
            };
            if last.as_ref().map(&comparable) == Some(comparable(&view)) {
                continue;
            }
            last = Some(view.clone());
            substrate.store(builder.session(view));
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
