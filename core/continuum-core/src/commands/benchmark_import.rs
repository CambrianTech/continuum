//! The recipe-facing benchmark verbs (S4 of docs/planning/RECIPE-CONVERGENCE-PLAN.md).
//!
//! `benchmark/dispatch` does everything a round needs in one Rust function: import,
//! room, cards, tracker, staging, kickoff. A recipe cannot call a function; it can only
//! call VERBS. These three are the pieces of dispatch a pipeline needs, each doing
//! exactly one thing and sharing dispatch's own writers — so a round posted by the
//! authored `benchmark/swe` recipe and one posted by dispatch are the same round.
//!
//! - `benchmark/import` — task + oracle only, projected into card rows by THE ONE
//!   writer ([`prepare_cards`]). No side effects: no room, no board, no env pre-warm.
//! - `benchmark/round-open` — tell the round tracker a round exists in a room.
//! - `benchmark/round-track` — tell it which board cards belong to that round.
//!
//! The two `round-*` verbs are DELIBERATELY THIN and deliberately temporary: the
//! tracker (`cognition::bench_round`) holds a second copy of card state the board
//! already owns, and the board-is-the-saved-state plan deletes it. When it goes,
//! these two verbs go with it and the recipe's pipeline gets two steps shorter.

use crate::commands::benchmark::{
    known_benchmarks, parse_card_title, prepare_cards, CardSelection, CardWork, PreparedCard,
};
use crate::cognition::bench_round::WorkDriver;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ─────────────────────────── benchmark/import ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkImportParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkImportParams {
    /// The suite (`benchmark/list` names them): `swe-bench-verified`, `hard-rs`, …
    pub suite: String,
    /// Explicit instance ids (substring match, as dispatch). Empty = the seeded sample
    /// or the suite head.
    #[serde(default)]
    #[ts(optional)]
    pub instances: Option<Vec<String>>,
    /// Seeded random sample size. `(suite, seed, sample)` is the replication contract.
    #[serde(default)]
    #[ts(optional)]
    pub sample: Option<u32>,
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub seed: Option<u64>,
    /// Do not OFFER an instance a citizen already resolved on this node (its verdict
    /// file says `resolved`). The instance stays in the sample and its score — it is
    /// simply not work again. Dispatch has done this since 2026-09-07 (re-offering
    /// re-staged an August checkout and overwrote a first citizen's outcome); the
    /// authored round must offer exactly what dispatch offers. Default true.
    #[serde(default = "default_true")]
    pub skip_already_resolved: bool,
    /// Board key for GYM cards (a gym has no per-task repo); SWE cards carry their own.
    #[serde(default)]
    pub repo: Option<String>,
    /// Cap on cards offered, applied AFTER the gate. `None`/0 = every card the selection
    /// drew. The gym suites do not sample (a suite IS its task list), so this is how a
    /// round takes the first N of `hard-rs` — dispatch's `limit`, kept.
    #[serde(default)]
    #[ts(optional)]
    pub limit: Option<u32>,
    /// Keep only instances whose harness speaks this language (`rust`, `go`, `php`,
    /// `ruby`, `javascript`, `java`, `c`; `python` for the SWE-bench family). The
    /// Multilingual suite mixes nine; a round is one language's board. Empty = all.
    #[serde(default)]
    #[ts(optional)]
    pub language: Option<String>,
}

fn default_true() -> bool {
    true
}

/// One card, ready to post — title and body exactly as dispatch writes them, so the
/// grader (`benchmark_grade`, keyed on the title) and on-claim staging see no
/// difference between a recipe-posted card and a dispatch-posted one.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/ImportedCard.ts")]
#[serde(rename_all = "camelCase")]
pub struct ImportedCard {
    pub title: String,
    pub body: String,
    /// The task's own id (`sympy__sympy-24152`, or a gym task id) — recovered from the
    /// title by the ONE parser, never re-derived.
    pub task_id: String,
    /// `"swe"` (a real-project instance) or `"gym"` (an eval-set task).
    pub kind: String,
    /// The board key the card is created under: the SWE instance's own repo (a seeded
    /// Verified round mixes repos — a round-wide repo was wrong for every card but one);
    /// a gym task's repo is the import's `repo` param.
    pub repo: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkImportResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkImportResult {
    pub suite: String,
    pub cards: Vec<ImportedCard>,
    /// Instances drawn by the selection but NOT offered because a citizen already
    /// resolved them here — named, so the round's receipt can say why 3 drew and 1 posted.
    pub skipped_already_resolved: Vec<String>,
    /// Instances drawn but NOT offered because THIS BOX cannot grade them: their env class
    /// is proven red by `benchmark/validate`, or their own env refusal marker stands from
    /// an earlier round. Each entry names the instance and the wall, so the round's
    /// receipt says what the deck withheld and why (2026-09-13: two of five seed-4 cards
    /// burned hours on pytest's pluggy and requests' 2013 pytest).
    pub skipped_ungradeable: Vec<String>,
}

pub struct BenchmarkImport;

/// The row an authored pipeline fans out over. Pure, so the projection is testable
/// without a dataset: the title's key is the task id (one parser, one writer).
pub(crate) fn imported_from(pc: &PreparedCard, gym_repo: &str) -> Result<ImportedCard, CommandError> {
    let (_, task_id) = parse_card_title(&pc.title).ok_or_else(|| {
        CommandError::Internal(format!(
            "prepared card title is not a benchmark title: {:?} — the ONE writer and the ONE \
             parser disagree",
            pc.title
        ))
    })?;
    Ok(ImportedCard {
        title: pc.title.clone(),
        body: pc.body.clone(),
        task_id,
        kind: match pc.work {
            CardWork::Swe { .. } => "swe".to_string(),
            CardWork::Gym { .. } => "gym".to_string(),
        },
        repo: match &pc.work {
            CardWork::Swe { instance } => instance.repo.clone(),
            CardWork::Gym { .. } => gym_repo.to_string(),
        },
    })
}

#[async_trait]
impl ActionCommand for BenchmarkImport {
    const NAME: &'static str = "benchmark/import";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Import a benchmark suite's tasks as card rows (title + body as benchmark/dispatch \
         writes them; task + oracle only, the answer key stays held out). No side effects — \
         the recipe pipeline fans these out into work/create.";
    type Params = BenchmarkImportParams;
    type Output = BenchmarkImportResult;

    async fn run(&self, _ctx: &Ctx, p: BenchmarkImportParams) -> Result<Self::Output, CommandError> {
        let spec = known_benchmarks()
            .iter()
            .find(|b| b.name == p.suite)
            .ok_or_else(|| {
                CommandError::Invalid(format!("unknown benchmark '{}' — see benchmark/list", p.suite))
            })?;
        let prepared = prepare_cards(
            spec,
            &CardSelection {
                instances: p.instances.filter(|w| !w.is_empty()),
                sample: p.sample.filter(|n| *n > 0),
                seed: p.seed,
                language: p.language.clone(),
            },
        )
        .await?;
        let mut cards = Vec::with_capacity(prepared.len());
        let mut skipped_already_resolved = Vec::new();
        let mut skipped_ungradeable = Vec::new();
        for pc in &prepared {
            let row = imported_from(pc, p.repo.as_deref().unwrap_or(""))?;
            if p.skip_already_resolved
                && row.kind == "swe"
                && crate::cognition::swe_bench::read_verdict(&row.task_id).is_some_and(|v| v.resolved)
            {
                crate::probe!(
                    class = "bench.round.already_resolved",
                    instance = %row.task_id,
                    "a citizen's card already resolved this instance — it stays in the sample and its score, and is not offered as work again"
                );
                skipped_already_resolved.push(row.task_id);
                continue;
            }
            // THE DECK NEVER OFFERS WHAT THIS BOX CANNOT GRADE — the same gate dispatch
            // applies (known_red_wall), plus the instance's own standing env refusal.
            if let CardWork::Swe { instance } = &pc.work {
                let wall = crate::commands::benchmark::known_red_wall(spec.name, &instance.repo, instance.year())
                    .map(|w| format!("env class {} {} proven red by benchmark/validate: {w}", instance.repo, instance.year()))
                    .or_else(|| crate::cognition::swe_verdict_sweep::standing_env_refusal(&row.task_id)
                        .map(|r| format!("env refusal stands from an earlier round: {r}")));
                if let Some(wall) = wall {
                    crate::probe!(
                        class = "bench.round.ungradeable_withheld",
                        instance = %row.task_id,
                        wall = %wall,
                        "an instance this box cannot grade is not offered as work"
                    );
                    skipped_ungradeable.push(format!("{}: {}", row.task_id, wall));
                    continue;
                }
            }
            cards.push(row);
        }
        if let Some(n) = p.limit.filter(|n| *n > 0) {
            cards.truncate(n as usize);
        }
        Ok(BenchmarkImportResult { suite: p.suite, cards, skipped_already_resolved, skipped_ungradeable })
    }
}

// ─────────────────────────── benchmark/round-open ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkRoundOpenParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkRoundOpenParams {
    /// The run room's id — the round IS the room (`$room.id` in a pipeline).
    #[ts(type = "string")]
    pub room_id: uuid::Uuid,
    pub room_name: String,
    pub suite: String,
    /// Who works the cards. Absent = `citizen` — resolved by the RECIPE's declared
    /// default on the authored path, which is what makes the declared default real.
    #[serde(default)]
    #[ts(optional)]
    pub driver: Option<WorkDriver>,
    #[serde(default)]
    #[ts(optional)]
    pub review_gate: Option<bool>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkRoundOpenResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkRoundOpenResult {
    #[ts(type = "string")]
    pub round_id: uuid::Uuid,
    pub driver: WorkDriver,
}

pub struct BenchmarkRoundOpen;

#[async_trait]
impl ActionCommand for BenchmarkRoundOpen {
    const NAME: &'static str = "benchmark/round-open";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Register a benchmark round in the round tracker for a room (a recipe pipeline step; \
         benchmark/dispatch does this inline). Temporary: goes away with the tracker.";
    type Params = BenchmarkRoundOpenParams;
    type Output = BenchmarkRoundOpenResult;

    async fn run(&self, _ctx: &Ctx, p: BenchmarkRoundOpenParams) -> Result<Self::Output, CommandError> {
        let room = p.room_id;
        let driver = p.driver.unwrap_or(WorkDriver::Citizen); // unwrap_or: the recipe's declared default is `citizen`; the authored path always passes it
        crate::cognition::bench_round::open_round(room, &p.suite, driver);
        crate::cognition::bench_round::set_run_room_name(room, &p.room_name);
        if p.review_gate.unwrap_or(false) { // unwrap_or: gate not named = off, the control arm
            crate::cognition::bench_round::set_review_gate(room, true);
        }
        Ok(BenchmarkRoundOpenResult { round_id: room, driver })
    }
}

// ─────────────────────────── benchmark/round-track ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkRoundTrackParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkRoundTrackParams {
    #[ts(type = "string")]
    pub room_id: uuid::Uuid,
    /// The `work/create` results, in the order the cards were imported (`$cards`).
    #[ts(type = "unknown")]
    pub cards: serde_json::Value,
    /// The imported rows, same order (`$imported.cards`) — zipped by index.
    #[ts(type = "unknown")]
    pub tasks: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkRoundTrackResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkRoundTrackResult {
    pub tracked: u32,
}

pub struct BenchmarkRoundTrack;

#[async_trait]
impl ActionCommand for BenchmarkRoundTrack {
    const NAME: &'static str = "benchmark/round-track";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Attach posted cards to a round in the tracker, by index against the imported rows \
         (a recipe pipeline step after work/create fans out). Temporary: goes with the tracker.";
    type Params = BenchmarkRoundTrackParams;
    type Output = BenchmarkRoundTrackResult;

    async fn run(&self, _ctx: &Ctx, p: BenchmarkRoundTrackParams) -> Result<Self::Output, CommandError> {
        let room = p.room_id;
        let cards = p.cards.as_array().ok_or_else(|| {
            CommandError::Invalid("cards must be the array of work/create results".into())
        })?;
        let tasks = p.tasks.as_array().ok_or_else(|| {
            CommandError::Invalid("tasks must be the array of imported rows".into())
        })?;
        if cards.len() != tasks.len() {
            return Err(CommandError::Invalid(format!(
                "cards ({}) and tasks ({}) differ in length — they zip by index",
                cards.len(),
                tasks.len()
            )));
        }
        let mut tracked = 0u32;
        for (card, task) in cards.iter().zip(tasks) {
            let card_id = card
                .get("card_id")
                .or_else(|| card.get("cardId"))
                .and_then(|v| v.as_str())
                .and_then(|s| uuid::Uuid::parse_str(s).ok())
                .ok_or_else(|| CommandError::Invalid(format!("a card row carries no card_id: {card}")))?;
            let task_id = task
                .get("taskId")
                .or_else(|| task.get("task_id"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| CommandError::Invalid(format!("a task row carries no taskId: {task}")))?;
            crate::cognition::bench_round::add_card(room, card_id);
            crate::cognition::bench_round::record_card_instance(card_id, task_id);
            tracked += 1;
        }
        Ok(BenchmarkRoundTrackResult { tracked })
    }
}

crate::register_command!(BenchmarkImport);
crate::register_command!(BenchmarkRoundOpen);
crate::register_command!(BenchmarkRoundTrack);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::benchmark::{dispatch_card_title, dispatch_swe_card_body};

    // what this catches: a recipe-posted card and a dispatch-posted card must be the
    // SAME card to the grader, which keys on the title. `imported_from` recovers the
    // task id through the one parser from the one writer's title; if either drifts,
    // the recipe path posts cards nothing ever grades.
    #[test]
    fn an_imported_row_carries_the_writers_title_and_the_parsers_task_id() {
        let inst = crate::cognition::swe_bench::SweInstance {
            instance_id: "demo__repo-1".into(),
            repo: "demo/repo".into(),
            base_commit: "abcdef0123456789".into(),
            patch: "GOLD_PATCH_MARKER".into(),
            test_patch: "TEST_PATCH_MARKER".into(),
            problem_statement: "Widget frobnicates twice.".into(),
            created_at: "2023-01-01".into(),
            fail_to_pass: "[\"tests/test_widget.py::test_single_frob\"]".into(),
            pass_to_pass: "[]".into(),
            eval_script: None,
            log_parser: None,
        };
        let pc = PreparedCard {
            title: dispatch_card_title("swe-bench-verified", &inst.instance_id, &inst.problem_statement),
            body: dispatch_swe_card_body("swe-bench-verified", &inst),
            needs_setup: false,
            setup_shell: None,
            work: CardWork::Swe { instance: Box::new(inst) },
        };
        let row = imported_from(&pc, "").expect("a writer's title parses");
        assert_eq!(row.task_id, "demo__repo-1");
        assert_eq!(row.kind, "swe");
        assert!(row.title.starts_with("[bench swe-bench-verified] demo__repo-1:"));
        assert!(!row.body.contains("GOLD_PATCH_MARKER"), "import never leaks the oracle");
    }

    // what this catches: the two tracker verbs are the temporary seam the plan names;
    // they must stay curation-level (a citizen must not be able to re-open a round).
    #[test]
    fn the_round_verbs_are_privileged_and_named() {
        assert_eq!(BenchmarkRoundOpen::NAME, "benchmark/round-open");
        assert_eq!(BenchmarkRoundTrack::NAME, "benchmark/round-track");
        assert!(matches!(BenchmarkRoundOpen::ACCESS, AccessLevel::Privileged));
        assert!(matches!(BenchmarkRoundTrack::ACCESS, AccessLevel::Privileged));
        assert_eq!(BenchmarkImport::NAME, "benchmark/import");
    }
}
