//! Typed benchmark-board payload — `BenchViewState`, the substrate-shaped
//! view of the node's LIVE benchmark runs (#329: a benchmark IS a live room;
//! the run rows ARE the panel). Joel, 2026-08-12: the board doubles as the
//! efficiency instrument — "it will also help us really see what's going on".
//!
//! Same define-once discipline as `serving.rs`: the core emitter folds the ONE
//! run-ledger projection (`benchmark/runs`' own scan — never a parallel file
//! scrape) into rows, so the web rail widget, a TUI board, and a teacher
//! persona's grounding all render the SAME facts, and reconnect resyncs the
//! board instead of starting blank.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One benchmark run — one board row. Mirrors the fields `BenchRunCard`
/// (the command projection) carries; every field here is REAL ledger data,
/// absent when the ledger hasn't written it yet (honest absence — a queued
/// run renders as queued, never dressed as work).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/positron/BenchRunRow.ts")]
pub struct BenchRunRow {
    pub run_id: String,
    /// Instance under test ("sympy__sympy-24066"); absent on non-SWE runs.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub instance: Option<String>,
    /// Solver persona id; absent while attempt 1 hasn't journaled.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub solver: Option<String>,
    /// `resolved` | `failed` | `active` | `quiet` — the projection's phases.
    pub phase: String,
    /// True exactly when `phase == "quiet"` (stall-window silence).
    pub stalled: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub attempt: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub max_attempts: Option<u32>,
    /// Seconds since the newest artifact write — the pulse.
    #[ts(type = "number")]
    pub age_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub acts: Option<u32>,
    /// Graded diff bytes when a grade exists; the result's own live diff
    /// length before that (the "patch is forming" leading indicator).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub patch_bytes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub resolved: Option<bool>,
    /// "passed/total" strings, render-ready ("1/1", "38/40").
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub fail_to_pass: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub pass_to_pass: Option<String>,
    /// Failed test NAMES (capped upstream) — a verdict that can teach.
    pub failed_tests: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub infra_error: Option<String>,
    /// The ROUND this run belongs to (== its run room's UUID) — the board
    /// groups runs under their round. Absent for unrounded/legacy rows.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub round_id: Option<String>,
    /// The run's SOLVE ROOM — the DOOR: a renderer navigates here to stand
    /// in the activity (transcript + act receipts). Absent before the mint.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub solve_room: Option<String>,
    /// The solve room's airc NAME — joins are by name, and standing in the
    /// room requires joining it first. Absent for rooms minted before names
    /// were recorded; such a door stays closed rather than half-opening.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub solve_room_name: Option<String>,
}

/// One IN-FLIGHT round — the real lifecycle object behind the board's
/// scoreboard region (#371: rounds are recipe-owned state, reboot-durable).
/// Mirrors the core's `RoundSnapshot`; before this row existed the client
/// derived a "round scoreboard" by counting run rows, which is a guess — the
/// recipe's scoreboard region renders THIS, the round tracker's own truth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/positron/BenchRoundRow.ts")]
pub struct BenchRoundRow {
    /// The run room's airc name — lets a navigator label the rail tab for this
    /// round by what it is (`verified · working · 9/12 in hands`) instead of
    /// its raw name. `#[serde(default)]` so an older core folds as empty.
    #[serde(default)]
    pub run_room: String,
    /// The round id — which IS its run room's id (a round is its room's activity).
    pub round_id: String,
    /// Suite name as catalogued ("swe-bench-lite", "ds-1000").
    pub benchmark: String,
    /// `working` | `done`. Present on the wire means in flight.
    pub stage: String,
    #[ts(type = "number")]
    pub dispatched: u32,
    #[ts(type = "number")]
    pub settled: u32,
    #[ts(type = "number")]
    pub remaining: u32,
    /// `citizen` | `detached_solve` — who works the cards.
    pub driver: String,
    /// Per-card rows the board renders under the round — WHAT, WHO, and how
    /// it is going, including cards that never started (2026-09-01: those
    /// rendered as NOTHING, making `working 0/8` for three hours of thrash
    /// pixel-identical to a healthy grind). `default` for pre-cards wires.
    #[serde(default)]
    pub cards: Vec<BenchRoundCardRow>,
    /// Glanceable health, pronounced core-side (never client arithmetic):
    /// `unstarted` | `grinding` | `stalled` | `paused` | `done`.
    #[serde(default)]
    pub verdict: String,
    /// Seconds since the newest work artifact on an unsettled card.
    /// Absent = no artifacts yet — an absence, never `0`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional, type = "number")]
    pub idle_secs: Option<u64>,
}

/// One card of a round, as the board renders it. Mirrors the core's
/// `RoundCardSnapshot` (same lossless-fold contract as run rows).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/positron/BenchRoundCardRow.ts"
)]
pub struct BenchRoundCardRow {
    pub card_id: String,
    /// Instance under test; empty until the solve activity is minted.
    pub instance: String,
    /// Solver name once a run names one, else the staged assignee's uuid.
    pub assignee: String,
    /// The solve activity's airc name — the navigable door. Empty until minted.
    pub solve_room_name: String,
    /// `unstarted` | run phase (`active`, `quiet`, `ungraded`, …) | terminal state.
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional, type = "number")]
    pub acts: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional, type = "number")]
    pub patch_bytes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional, type = "number")]
    pub last_act_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[ts(optional)]
    pub resolved: Option<bool>,
    /// BOARD truth: who holds the card right now (display name; empty = nobody).
    #[serde(default)]
    pub owner: String,
    /// BOARD truth: the card's column (`open|claimed|in_progress|review|closed|…`);
    /// empty when the board could not be read.
    #[serde(default)]
    pub board_state: String,
    /// When the verdict was recorded — the settle clock for time-to-resolve.
    #[ts(optional, type = "number")]
    #[serde(default)]
    pub graded_at_ms: Option<u64>,
}

/// The benchmark board — what the ACADEMY right-rail widget draws.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/positron/BenchViewState.ts")]
pub struct BenchViewState {
    /// Rows, most recently active first, bounded at the emitter. EMPTY =
    /// no runs on this node — the awaiting frame, never a fabricated row.
    pub runs: Vec<BenchRunRow>,
    /// In-flight rounds from the round tracker (#371) — the scoreboard's
    /// truth. `default` so a pre-rounds wire still deserializes (empty =
    /// honest "no rounds", same contract as `runs`).
    #[serde(default)]
    pub rounds: Vec<BenchRoundRow>,
    /// Emitter cadence in ms so renderers label freshness from data.
    #[ts(type = "number")]
    pub sample_interval_ms: u64,
    /// The ONE room this view describes, when it is a room's view: a round's
    /// room (its round row + the runs under it) or a solve room (its runs + the
    /// parent round). `None` is the node-wide fold the human rail renders.
    ///
    /// A citizen reads the board of the activity she is standing in and nothing
    /// else, like the roster (HER-LOOP-IS-HER-OWN.md rule 5): the node-wide
    /// fold is never pushed into a mind. `Option` + `default` so the wire the
    /// rail already reads is unchanged.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room_id: Option<String>,
}

impl BenchViewState {
    /// The on-wire `kind` this view is published under (open
    /// self-registration, not a central enum).
    pub const KIND: &'static str = "bench";

    /// Split the node-wide fold into one view per room it describes, keyed by
    /// the room's id: each round's room gets that round and the runs whose
    /// `round_id` is it; each solve room gets its runs and their parent round.
    /// Rows that name no room (verdict and artifact rows, the live exam row)
    /// belong to no room's view: a citizen in a room sees that room's work.
    pub fn per_room(&self) -> Vec<BenchViewState> {
        let mut by_room: std::collections::BTreeMap<String, BenchViewState> =
            std::collections::BTreeMap::new();
        let blank = |room: &str| BenchViewState {
            runs: Vec::new(),
            rounds: Vec::new(),
            sample_interval_ms: self.sample_interval_ms,
            room_id: Some(room.to_string()),
        };
        for round in &self.rounds {
            by_room
                .entry(round.round_id.clone())
                .or_insert_with(|| blank(&round.round_id))
                .rounds
                .push(round.clone());
        }
        for run in &self.runs {
            if let Some(round_room) = run.round_id.as_deref() {
                by_room
                    .entry(round_room.to_string())
                    .or_insert_with(|| blank(round_room))
                    .runs
                    .push(run.clone());
            }
            if let Some(solve_room) = run.solve_room.as_deref() {
                let view = by_room
                    .entry(solve_room.to_string())
                    .or_insert_with(|| blank(solve_room));
                view.runs.push(run.clone());
                let parent = run
                    .round_id
                    .as_deref()
                    .and_then(|r| self.rounds.iter().find(|round| round.round_id == r));
                if let Some(parent) = parent {
                    if !view.rounds.iter().any(|r| r.round_id == parent.round_id) {
                        view.rounds.push(parent.clone());
                    }
                }
            }
        }
        by_room.into_values().collect()
    }
}

impl positron_core::ViewState for BenchViewState {
    fn kind(&self) -> &'static str {
        Self::KIND
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the "bench" kind string never drifts from the trait,
    // and the empty view is honest (no rows) — a widget rendering it shows the
    // awaiting frame, never a fabricated run.
    #[test]
    fn kind_is_stable_and_empty_view_is_honest() {
        use positron_core::ViewState;
        let view = BenchViewState { room_id: None, runs: vec![], rounds: vec![], sample_interval_ms: 5000 };
        assert_eq!(view.kind(), "bench");
        assert_eq!(BenchViewState::KIND, "bench");
        assert!(view.runs.is_empty());
        // Optional fields elide from the wire — the TS optional contract.
        let row = BenchRunRow {
            run_id: "r1".into(),
            round_id: None,
            solve_room: None,
            solve_room_name: None,
            instance: None,
            solver: None,
            phase: "active".into(),
            stalled: false,
            attempt: None,
            max_attempts: None,
            age_secs: 3,
            acts: None,
            patch_bytes: None,
            resolved: None,
            fail_to_pass: None,
            pass_to_pass: None,
            failed_tests: vec![],
            infra_error: None,
        };
        let wire = serde_json::to_value(&row).expect("serialize");
        assert!(wire.get("instance").is_none(), "absent facts elide, never null-fabricate");
    }

    fn run(id: &str, round: Option<&str>, solve: Option<&str>) -> BenchRunRow {
        BenchRunRow {
            run_id: id.into(), instance: None, solver: None, phase: "solving".into(), stalled: false,
            attempt: None, max_attempts: None, age_secs: 0, acts: None, patch_bytes: None,
            resolved: None, fail_to_pass: None, pass_to_pass: None, failed_tests: vec![],
            infra_error: None, round_id: round.map(str::to_string),
            solve_room: solve.map(str::to_string), solve_room_name: None,
        }
    }

    fn round(id: &str) -> BenchRoundRow {
        BenchRoundRow {
            run_room: format!("room-{id}"), round_id: id.into(), benchmark: "swe".into(),
            stage: "working".into(), dispatched: 2, settled: 0, remaining: 2, driver: "citizen".into(),
            cards: vec![], verdict: "grinding".into(), idle_secs: None,
        }
    }

    // what this catches: HER-LOOP-IS-HER-OWN.md rule 5. The node-wide fold is never
    // what a mind is handed; a room's view holds ONLY that room's rows: a round room
    // its round and its runs, a solve room its runs and the parent round, and a row
    // that names no room (a verdict/artifact row, the live exam) lands in no room.
    // If the split ever leaks another round's runs into a room, a citizen in one
    // activity is back to carrying every run on the node, and only this fails.
    #[test]
    fn per_room_views_hold_only_their_own_rooms_rows() {
        let fold = BenchViewState {
            room_id: None,
            sample_interval_ms: 5000,
            rounds: vec![round("r1"), round("r2")],
            runs: vec![
                run("a", Some("r1"), Some("solve-a")),
                run("b", Some("r1"), None),
                run("c", Some("r2"), None),
                run("exam", None, None),
            ],
        };
        let views = fold.per_room();
        let by_room = |id: &str| views.iter().find(|v| v.room_id.as_deref() == Some(id)).expect(id);
        let ids = |v: &BenchViewState| v.runs.iter().map(|r| r.run_id.clone()).collect::<Vec<_>>();

        let r1 = by_room("r1");
        assert_eq!(ids(r1), vec!["a", "b"], "a round room holds its own runs only");
        assert_eq!(r1.rounds.iter().map(|r| r.round_id.as_str()).collect::<Vec<_>>(), vec!["r1"]);
        assert_eq!(ids(by_room("r2")), vec!["c"]);

        let solve = by_room("solve-a");
        assert_eq!(ids(solve), vec!["a"], "a solve room holds its own run");
        assert_eq!(solve.rounds.len(), 1, "and the parent round, once");
        assert_eq!(solve.rounds[0].round_id, "r1");

        assert_eq!(views.len(), 3, "a run naming no room is in no room's view: {views:?}");
        assert!(views.iter().all(|v| v.room_id.is_some() && v.sample_interval_ms == 5000));
    }
}
