//! The `cognition/eval-status` command: poll a detached run's ledger row by persona and run id.

use super::*;

/// Append one row to the per-persona progress ledger so a trend line accrues
/// across runs — the scoreboard for "mark improvement as you go". Best-effort
/// like the recorder ([[cognition-half-the-work-is-harnesses]]): a ledger
/// failure must NEVER fail an eval. Every row is reality-anchored — `passRate`
/// is test-graded, `selfVerifyRate` is whether she actually ran her own code —
/// so the trend can't be gamed by prose. One JSONL line per run at
/// `~/.continuum/progress/<persona_id>.jsonl`, labelled by `note`.
/// `cognition/eval-status` — the poll half of fire-and-poll (#86). A detached
/// `cognition/eval`/`benchmark/run` returns a `run_id` immediately; this command reads the
/// persona's progress ledger and reports whether that run's row has landed (complete, with
/// the full result row) or not yet (pending). Read-only, ai-safe: personas can watch their
/// OWN runs ([[first-class-citizens]] — self-monitoring rides the same registry), clients
/// poll instead of holding a connection open across a many-minute exam.
#[derive(Default)]
pub struct CognitionEvalStatus;

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CognitionEvalStatusParams {
    /// The examinee persona (whose ledger holds the row). Optional: omit (with
    /// run_id) to poll just the LIVE progress of whatever pass is running now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub persona_id: Option<String>,
    /// The handle returned by the detached run. Optional: omit for live progress only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS, Default)]
pub struct CognitionEvalStatusResult {
    /// True once the run's ledger row exists.
    pub complete: bool,
    /// The full ledger row when complete (score/total/passRate/lift/...), else null.
    pub row: Option<serde_json::Value>,
    /// LIVE progress of the currently-grading pass (done/total/pass/current task) —
    /// the mid-run scoreboard (#123/#141). `null` when nothing has graded yet this
    /// process; check `updated_at_ms` for staleness on long-dead passes.
    pub progress: Option<EvalPassProgress>,
}

#[async_trait]
impl ActionCommand for CognitionEvalStatus {
    const NAME: &'static str = "cognition/eval-status";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Poll a detached cognition/eval / benchmark/run by its run_id: returns {complete, row} \
         from the persona's progress ledger. The poll half of fire-and-poll — clients and \
         personas alike watch long runs without holding a connection open.";

    type Params = CognitionEvalStatusParams;
    type Output = CognitionEvalStatusResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: CognitionEvalStatusParams,
    ) -> Result<CognitionEvalStatusResult, CommandError> {
        // The live scoreboard rides on EVERY poll — with or without a run_id.
        let progress = subscribe_eval_progress().borrow().clone();
        let Some(run_id) = p.run_id else {
            // No run handle → live progress only (the "how's it going" poll).
            return Ok(CognitionEvalStatusResult {
                complete: false,
                row: None,
                progress,
            });
        };
        // run_id is a globally-unique UUID, so it is a SUFFICIENT key on its own. With a
        // persona_id we read that persona's ledger directly (the fast path); WITHOUT one
        // we scan every persona's ledger for the row rather than silently degrading to
        // "live progress only" — a poller that holds only the run handle (a benchmark/matrix
        // driver) must still resolve to the terminal row, never hang forever on a false
        // `complete:false` (the exact footgun a load-test poll hit 2026-07-20).
        let row = match &p.persona_id {
            Some(pid) => find_run_row_for_persona(pid, &run_id),
            None => find_run_row_any_persona(&run_id),
        };
        match row {
            Some(v) => Ok(CognitionEvalStatusResult {
                complete: true,
                row: Some(v),
                progress,
            }),
            None => Ok(CognitionEvalStatusResult {
                complete: false,
                row: None,
                progress,
            }),
        }
    }
}

crate::register_stateless_command!(CognitionEvalStatus);
