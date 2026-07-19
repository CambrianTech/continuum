//! `cognition/observe` — the AGENTIC observe surface for a benchmark activity.
//!
//! The agent-first / information-first half of "an activity is a room": one call
//! returns the benchmark's live state as a structured, UI-free **information model**
//! — three subscriptions (feed / central / scoreboard) that a positron widget, this
//! observe skill, and a proctor-persona's perception all render in their own idiom.
//! ONE truth, three consumers. See `docs/architecture/BENCHMARK-AS-ROOM-AND-OBSERVE.md`.
//!
//! This is the SAFE first slice: it only READS surfaces that already exist — the
//! live progress watch (`subscribe_eval_progress`) and the durable progress ledger
//! (`~/.continuum/progress/<persona>.jsonl`). It touches no cognition hot-path and no
//! identity. It is, concretely, the thing that replaces operator log-grep: instead of
//! tailing a log (and conflating two runs, as happened live 2026-07-17), an agent
//! gets `this run`, cleanly, run-disambiguated, as data.
//!
//! Output regions map 1:1 onto the positron tab layout:
//!   - `scoreboard` → right-hand region  (done/total/pass, provenance chip, vram)
//!   - `central`    → central region     (current task + last outcome)
//!   - `feed`       → event-feed region  (recent runs from the ledger; per-TASK
//!                     streaming lands when eval publishes per-task events — slice 2)
//!
//! `access: AiSafe` — read-only; a persona may observe its own (or a peer's) run,
//! same as `cognition/eval-status`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::cognition::eval::{subscribe_eval_progress, EvalPassProgress};
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

/// Whether the measurement lane was clean (nobody else on the GPU) or contended.
/// The honesty chip: "is this number a lie?" is a first-class field, never inferred
/// by a viewer watching who else is running. `Unknown` until eval stamps the
/// quiesce state onto the run (slice 2 — the quiesce-verify fix); the field exists
/// now so the model + the UI layout reserve the chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/BenchmarkProvenance.ts")]
pub enum BenchmarkProvenance {
    Clean,
    Contended,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/BenchmarkObserveParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkObserveParams {
    /// The examinee persona whose ledger holds the run history. Omit for live
    /// scoreboard only (no feed history).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub persona_id: Option<String>,
    /// Focus a specific run (its ledger row → scoreboard.complete + pass_rate).
    /// Omit for the live pass + latest history.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
    /// Max feed events (recent runs, newest first). Default 10.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub feed_limit: Option<usize>,
}

/// The whole activity as data — the three regions + meta. This struct IS the tab
/// layout: a widget renders these fields, this skill returns them, a persona
/// perceives them.
#[derive(Debug, Clone, Serialize, TS, Default)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/BenchmarkObserveResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkObserveResult {
    pub scoreboard: Scoreboard,
    pub central: Central,
    pub feed: Vec<FeedEvent>,
    pub meta: Meta,
}

/// Right-hand region: the at-a-glance number + how much to trust it.
#[derive(Debug, Clone, Serialize, TS, Default)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/BenchmarkScoreboard.ts")]
#[serde(rename_all = "camelCase")]
pub struct Scoreboard {
    /// Tasks graded / total in the running (or just-finished) pass.
    #[ts(type = "number")]
    pub done: u32,
    #[ts(type = "number")]
    pub total: u32,
    #[ts(type = "number")]
    pub pass: u32,
    /// `pass / done` live, or the ledger's final `passRate` when the run completed.
    pub pass_rate: f64,
    /// Is this number CLEAN or CONTENDED — surfaced, not inferred.
    pub provenance: BenchmarkProvenance,
    /// Live free VRAM (GB) at last grade — the resource axis. `null` when ungoverned.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub vram_free_gb: Option<u64>,
    /// Receiver-clock ms of the last progress tick — staleness signal.
    #[ts(type = "number")]
    pub updated_at_ms: u64,
    /// Every task in the pass has been graded (`done == total > 0`). The live
    /// "it's finished" signal a watcher needs WITHOUT a run_id — distinct from
    /// `complete` (which waits for the durable ledger row). Dogfood: `done=3/3`
    /// with `complete=false` read as "still going?"; this removes that ambiguity.
    pub pass_finished: bool,
    /// True once the focused run's ledger row exists (durable final number is in).
    /// Requires a `run_id`. `pass_finished` is the live precursor.
    pub complete: bool,
}

/// Central region: what she's working on right now.
#[derive(Debug, Clone, Serialize, TS, Default)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/BenchmarkCentral.ts")]
#[serde(rename_all = "camelCase")]
pub struct Central {
    /// The task currently being (or just) graded.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub current_task: Option<String>,
    /// Whether that task passed.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_ok: Option<bool>,
}

/// One event-feed entry — a completed run from the ledger (run history). The live
/// per-TASK stream (task_graded, turn) is the next slice, when eval publishes
/// per-task events onto the bus/room.
#[derive(Debug, Clone, Serialize, TS, Default)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/BenchmarkFeedEvent.ts")]
#[serde(rename_all = "camelCase")]
pub struct FeedEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
    /// The eval set / benchmark name (from the ledger's `evalSet`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub benchmark: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub pass_rate: Option<f64>,
    /// Was this run measured on a CLEAN (quiesced) GPU lane? The honesty chip per run.
    pub provenance: BenchmarkProvenance,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub captured_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, TS, Default)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/BenchmarkMeta.ts")]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub persona_id: Option<String>,
    /// True when there is no live pass AND no focused-run row — nothing to watch.
    pub idle: bool,
}

/// Clean benchmark name for the feed. `benchmark/run` stamps the note as
/// `benchmark/run <name>[ on <model>]`; prefer that, else the explicit `evalSet`.
fn benchmark_name(note: Option<&str>, eval_set: Option<&str>) -> Option<String> {
    note.and_then(|n| n.strip_prefix("benchmark/run "))
        .map(|rest| rest.split(" on ").next().unwrap_or(rest).trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| eval_set.map(str::to_string))
}

#[derive(Default)]
pub struct CognitionObserve;

impl BenchmarkObserveResult {
    /// Fold the live watch snapshot + the ledger into the three-region model. Pure
    /// (given the progress snapshot + ledger text) so it is unit-testable without
    /// the global watch or the filesystem.
    fn assemble(
        progress: Option<EvalPassProgress>,
        ledger_text: Option<&str>,
        run_id: Option<&str>,
        persona_id: Option<String>,
        feed_limit: usize,
    ) -> Self {
        let mut scoreboard = Scoreboard::default();
        let mut central = Central::default();

        if let Some(p) = &progress {
            scoreboard.done = p.done;
            scoreboard.total = p.total;
            scoreboard.pass = p.pass;
            scoreboard.pass_rate = if p.done > 0 { p.pass as f64 / p.done as f64 } else { 0.0 };
            scoreboard.vram_free_gb = p.vram_free_gb;
            scoreboard.updated_at_ms = p.updated_at_ms;
            central.current_task = Some(p.current_task.clone());
            central.last_ok = Some(p.last_ok);
            scoreboard.pass_finished = p.done > 0 && p.done == p.total;
        }
        // Provenance is not stamped on the watch/ledger yet — honest Unknown until
        // the quiesce-verify fix stamps it (slice 2). Never fabricate Clean.
        scoreboard.provenance = BenchmarkProvenance::Unknown;

        // Ledger: newest-first feed of completed runs, and (if a run_id is focused)
        // its final row → complete + final pass_rate.
        let mut feed: Vec<FeedEvent> = Vec::new();
        if let Some(text) = ledger_text {
            for line in text.lines().rev() {
                let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                    continue;
                };
                let row_run = v.get("runId").and_then(|r| r.as_str());
                // The honesty stamp eval now writes: cleanLane=true → measured on a
                // quiesced lane → CLEAN; absent/null → UNKNOWN (never falsely Clean).
                let prov = if v.get("cleanLane").and_then(|x| x.as_bool()) == Some(true) {
                    BenchmarkProvenance::Clean
                } else {
                    BenchmarkProvenance::Unknown
                };
                if let Some(want) = run_id {
                    if row_run == Some(want) {
                        scoreboard.complete = true;
                        scoreboard.provenance = prov;
                        if let Some(pr) = v.get("passRate").and_then(|x| x.as_f64()) {
                            scoreboard.pass_rate = pr;
                        }
                    }
                }
                if feed.len() < feed_limit {
                    let note = v.get("note").and_then(|x| x.as_str());
                    feed.push(FeedEvent {
                        run_id: row_run.map(str::to_string),
                        provenance: prov,
                        // The clean benchmark NAME for the feed. `benchmark/run` records
                        // `evalSet="inline"` (it passes tasks inline) but stamps the name into
                        // the note as `benchmark/run <name>[ on <model>]`; parse it so the feed
                        // reads `hard-rs`, not `inline`. Fall back to `evalSet` for a raw
                        // `cognition/eval` run that used a named set directly.
                        benchmark: benchmark_name(note, v.get("evalSet").and_then(|x| x.as_str())),
                        pass_rate: v.get("passRate").and_then(|x| x.as_f64()),
                        note: note.map(str::to_string),
                        captured_at_ms: v.get("capturedAtMs").and_then(|x| x.as_u64()),
                    });
                }
            }
        }

        let idle = progress.is_none() && !scoreboard.complete;
        BenchmarkObserveResult {
            scoreboard,
            central,
            feed,
            meta: Meta { persona_id, idle },
        }
    }
}

#[async_trait]
impl ActionCommand for CognitionObserve {
    const NAME: &'static str = "cognition/observe";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Observe a benchmark activity as a structured information model (scoreboard / \
         central / feed) — the agent-native view of a run that a positron widget, this \
         skill, and a proctor-persona all render. Reads the live progress watch + the \
         persona's progress ledger; replaces operator log-grep.";

    type Params = BenchmarkObserveParams;
    type Output = BenchmarkObserveResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: BenchmarkObserveParams,
    ) -> Result<BenchmarkObserveResult, CommandError> {
        let progress = subscribe_eval_progress().borrow().clone();
        let feed_limit = p.feed_limit.unwrap_or(10);

        // Read the persona's ledger, if a persona was named.
        let ledger_text = p.persona_id.as_ref().and_then(|pid| {
            let home = std::env::var("HOME").ok()?;
            let path = std::path::PathBuf::from(home)
                .join(".continuum/progress")
                .join(format!("{pid}.jsonl"));
            std::fs::read_to_string(path).ok()
        });

        Ok(BenchmarkObserveResult::assemble(
            progress,
            ledger_text.as_deref(),
            p.run_id.as_deref(),
            p.persona_id,
            feed_limit,
        ))
    }
}

crate::register_stateless_command!(CognitionObserve);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the name + access contract. observe is the AiSafe agentic
    // read surface — a persona can watch its own or a peer's run.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(CognitionObserve::NAME, "cognition/observe");
        assert_eq!(CognitionObserve::ACCESS, AccessLevel::AiSafe);
    }

    // what this catches: the assemble fold — the live watch becomes the scoreboard +
    // central, the ledger becomes a newest-first feed, a focused run_id flips
    // `complete` and overrides pass_rate with the final number. This is the whole
    // information model; if it drifts, every renderer (widget/agent/persona) drifts.
    #[test]
    fn assemble_folds_watch_and_ledger_into_the_three_regions() {
        let progress = Some(EvalPassProgress {
            done: 6,
            total: 8,
            pass: 2,
            current_task: "lru_cache".to_string(),
            last_ok: true,
            updated_at_ms: 1_000,
            vram_free_gb: Some(36),
            run_id: Some("r2".to_string()),
        });
        // Two ledger rows, oldest first on disk; newest = the focused run.
        let ledger = "{\"runId\":\"old\",\"evalSet\":\"humaneval-rs\",\"passRate\":0.25,\"note\":\"a\",\"capturedAtMs\":10}\n\
                      {\"runId\":\"r2\",\"evalSet\":\"hard-rs\",\"passRate\":0.375,\"note\":\"rebaseline\",\"capturedAtMs\":20,\"cleanLane\":true}";

        let out = BenchmarkObserveResult::assemble(
            progress,
            Some(ledger),
            Some("r2"),
            Some("asha".to_string()),
            10,
        );

        // scoreboard: live counts from the watch, but pass_rate overridden to the
        // focused run's final 0.375, and complete flipped true.
        assert_eq!(out.scoreboard.done, 6);
        assert_eq!(out.scoreboard.total, 8);
        assert!(!out.scoreboard.pass_finished, "6/8 is mid-pass, not finished");
        assert!(out.scoreboard.complete, "the focused run's ledger row exists");
        assert!((out.scoreboard.pass_rate - 0.375).abs() < 1e-9);
        // r2 was stamped cleanLane=true → the focused run's chip is CLEAN.
        assert_eq!(out.scoreboard.provenance, BenchmarkProvenance::Clean);
        assert_eq!(out.scoreboard.vram_free_gb, Some(36));
        // central: current focus from the watch.
        assert_eq!(out.central.current_task.as_deref(), Some("lru_cache"));
        assert_eq!(out.central.last_ok, Some(true));
        // feed: newest-first (r2 before old).
        assert_eq!(out.feed.len(), 2);
        assert_eq!(out.feed[0].run_id.as_deref(), Some("r2"));
        assert_eq!(out.feed[0].benchmark.as_deref(), Some("hard-rs"));
        assert_eq!(out.feed[0].provenance, BenchmarkProvenance::Clean, "r2 stamped clean");
        assert_eq!(out.feed[1].run_id.as_deref(), Some("old"));
        assert_eq!(out.feed[1].provenance, BenchmarkProvenance::Unknown, "unstamped row → unknown");
        assert!(!out.meta.idle);
    }

    // what this catches: the feed's benchmark name reads clean — parsed from the
    // `benchmark/run <name>` note (the ledger's evalSet is the unhelpful "inline"),
    // with an ` on <model>` suffix stripped, and a raw eval run falling back to evalSet.
    #[test]
    fn benchmark_name_reads_clean_from_the_run_note() {
        assert_eq!(benchmark_name(Some("benchmark/run hard-rs"), Some("inline")).as_deref(), Some("hard-rs"));
        assert_eq!(
            benchmark_name(Some("benchmark/run humaneval-rs on qwen2.5"), Some("inline")).as_deref(),
            Some("humaneval-rs")
        );
        // Raw cognition/eval with a named set and no benchmark note → fall back to evalSet.
        assert_eq!(benchmark_name(Some("baseline"), Some("coder-eval")).as_deref(), Some("coder-eval"));
        assert_eq!(benchmark_name(None, None), None);
    }

    // what this catches: pass_finished flips true when every task is graded
    // (done==total>0) — the live "it's done" signal without a run_id.
    #[test]
    fn pass_finished_flips_when_all_tasks_graded() {
        let progress = Some(EvalPassProgress {
            done: 3, total: 3, pass: 0, current_task: "rle_roundtrip".to_string(),
            last_ok: false, updated_at_ms: 1, vram_free_gb: None, run_id: None,
        });
        let out = BenchmarkObserveResult::assemble(progress, None, None, None, 10);
        assert!(out.scoreboard.pass_finished);
        assert!(!out.scoreboard.complete, "no run_id → no durable-row completion");
    }

    // what this catches: nothing running + no run row = honest idle, not a fabricated
    // zero-score board.
    #[test]
    fn assemble_reports_idle_when_nothing_is_happening() {
        let out = BenchmarkObserveResult::assemble(None, None, None, None, 10);
        assert!(out.meta.idle);
        assert!(!out.scoreboard.complete);
        assert!(out.feed.is_empty());
    }
}
