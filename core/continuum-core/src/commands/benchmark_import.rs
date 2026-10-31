//! The recipe-facing benchmark adapter verbs (S4 of docs/planning/RECIPE-CONVERGENCE-PLAN.md).
//!
//! A benchmark is an adapter: it imports task + oracle into card rows, and the recipe
//! pipeline posts them into the activity's room with `work/create`. The room is the
//! runner; nothing here opens, tracks or drives a round
//! (docs/architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md).
//!
//! - `benchmark/import` — task + oracle only, projected into card rows by THE ONE
//!   writer ([`prepare_cards`]). No side effects: no room, no board, no env pre-warm.
//! - `coursework/import` — a coursework lesson set, projected the same way.

use crate::recipe::benchmark_catalog::known_benchmarks;
use crate::commands::benchmark::{
    prepare_cards, CardSelection, CardWork, PreparedCard,
};
use crate::recipe::bench_title::parse_card_title;
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
    /// Explicit instance ids (substring match). Empty = the seeded sample
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
    /// simply not work again (2026-09-07: re-offering re-staged an August checkout and
    /// overwrote a first citizen's outcome). Default true.
    #[serde(default = "default_true")]
    pub skip_already_resolved: bool,
    /// Board key for GYM cards (a gym has no per-task repo); SWE cards carry their own.
    #[serde(default)]
    pub repo: Option<String>,
    /// Cap on cards offered, applied AFTER the gate. `None`/0 = every card the selection
    /// drew. The gym suites do not sample (a suite IS its task list), so this is how a
    /// round takes the first N of `hard-rs`.
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

/// One card, ready to post — title and body from the ONE card writer, so the grader
/// (`benchmark_grade`, keyed on the title) and on-claim staging read what was posted.
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
    /// Instances drawn but NOT offered because THIS BOX cannot grade them: their own env
    /// refusal marker stands from an earlier attempt. Each entry names the instance and the wall, so the round's
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
            CardWork::Gym => "gym".to_string(),
        },
        repo: match &pc.work {
            CardWork::Swe { instance } => instance.repo.clone(),
            CardWork::Gym => gym_repo.to_string(),
        },
    })
}

#[async_trait]
impl ActionCommand for BenchmarkImport {
    const NAME: &'static str = "benchmark/import";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Import a benchmark suite's tasks as card rows (title + body from the one card \
         writer; task + oracle only, the answer key stays held out). No side effects — \
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
                continuum_base::probe!(
                    class = "bench.round.already_resolved",
                    instance = %row.task_id,
                    "a citizen's card already resolved this instance — it stays in the sample and its score, and is not offered as work again"
                );
                skipped_already_resolved.push(row.task_id);
                continue;
            }
            // THE DECK NEVER OFFERS WHAT THIS BOX CANNOT GRADE: the instance's own standing
            // env refusal.
            if let CardWork::Swe { .. } = &pc.work {
                let wall = crate::cognition::swe_verdict_sweep::standing_env_refusal(&row.task_id)
                    .map(|r| format!("env refusal stands from an earlier attempt: {r}"));
                if let Some(wall) = wall {
                    continuum_base::probe!(
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

// ─────────────────────────── coursework/import ───────────────────────────

/// Where a coursework round's lessons come from: the teach selector's sources, in its order
/// (`commands::genome::teach::lesson_tasks`). Only TESTED lessons become cards; a lesson's test
/// is its card's verdict (ONE-RESIDENT-MODEL §10.2).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/CourseworkImportParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct CourseworkImportParams {
    /// A citizen whose own objectively graded failures are the lessons (her experience
    /// stream, the #319 curriculum drain). Empty = not from experience.
    #[serde(default)]
    #[ts(optional)]
    pub from_experience: Option<String>,
    /// A lesson JSONL (`EvalTask` rows). Empty = the committed default teach set.
    #[serde(default)]
    #[ts(optional)]
    pub teach_set: Option<String>,
    /// The board key the cards land under (owner/name), as `benchmark/import`'s `repo`. The
    /// recipe always passes it (`$args.repo`).
    pub repo: String,
    /// Cap on cards offered; 0 or empty = every tested lesson.
    #[serde(default)]
    #[ts(optional)]
    pub limit: Option<u32>,
}

pub struct CourseworkImport;

#[async_trait]
impl ActionCommand for CourseworkImport {
    const NAME: &'static str = "coursework/import";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Import a coursework lesson set as card rows: the teach selector's TESTED lessons are \
         written as a content-addressed set (coursework-<sha12>, the set's task identity), then \
         projected as gym cards exactly as benchmark/import writes them. No side effects beyond \
         the set file; the coursework/round recipe fans the rows out into work/create.";
    type Params = CourseworkImportParams;
    type Output = BenchmarkImportResult;

    async fn run(&self, _ctx: &Ctx, p: CourseworkImportParams) -> Result<Self::Output, CommandError> {
        let from_experience = p.from_experience.filter(|s| !s.is_empty());
        let teach_set = p.teach_set.filter(|s| !s.is_empty());
        // The selector reads files (her experience stream, a teach set): off the async thread.
        let tasks = tokio::task::spawn_blocking(move || {
            crate::commands::genome::teach::lesson_tasks(
                None,
                from_experience.as_deref(),
                teach_set.as_deref(),
                true,
            )
        })
        .await
        .map_err(|e| CommandError::Internal(format!("lesson selection task failed: {e}")))??;
        let home = crate::paths::continuum_home()?;
        let set = crate::cognition::coursework::write_set(&home, &tasks).map_err(CommandError::Invalid)?;
        let reference = crate::cognition::eval_set::eval_set_reference(&set).ok_or_else(|| {
            CommandError::Internal(format!("coursework set {set} was written but does not resolve"))
        })?;
        let prepared = crate::commands::benchmark::prepare_gym_cards(&set, &reference)?;
        let limit = p.limit.filter(|n| *n > 0).map_or(usize::MAX, |n| n as usize);
        let cards = prepared
            .iter()
            .take(limit)
            .map(|pc| imported_from(pc, &p.repo))
            .collect::<Result<Vec<_>, _>>()?;
        continuum_base::probe!(
            class = "coursework.set_imported",
            set = %set,
            lessons = prepared.len() as u64,
            offered = cards.len() as u64,
            "a coursework lesson set was written under its content name and projected as cards"
        );
        Ok(BenchmarkImportResult {
            suite: set,
            cards,
            skipped_already_resolved: Vec::new(),
            skipped_ungradeable: Vec::new(),
        })
    }
}

crate::register_command!(BenchmarkImport);
crate::register_command!(CourseworkImport);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::bench_title::dispatch_card_title;
    use crate::commands::benchmark::dispatch_swe_card_body;

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
            work: CardWork::Swe { instance: Box::new(inst) },
        };
        let row = imported_from(&pc, "").expect("a writer's title parses");
        assert_eq!(row.task_id, "demo__repo-1");
        assert_eq!(row.kind, "swe");
        assert!(row.title.starts_with("[bench swe-bench-verified] demo__repo-1:"));
        assert!(!row.body.contains("GOLD_PATCH_MARKER"), "import never leaks the oracle");
    }
}
