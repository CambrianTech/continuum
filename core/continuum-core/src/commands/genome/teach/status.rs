//! `genome/teach-status` — the poll half of a detached teach run.

use super::*;

/// `genome/teach-status` — the poll half of a long corpus-gen run. `genome/teach` runs
/// for many minutes (write→grade→fix over a whole task set); this returns the LIVE
/// `{done, total, currentTask, solved}` snapshot so an operator, a poller, or a UI
/// progress bar can SEE where it is — instead of guessing from CPU%. Read-only, ai-safe,
/// no params: the substrate serializes teach runs, so there's one live board to read.
/// The queryable companion to the fire-and-forget `genome:teach:progress` bus events —
/// a status you can ask beats a light you can only hope someone's watching.
/// [[self-test-via-command-feedback-surface-never-blind]]
#[derive(Default)]
pub struct GenomeTeachStatus;

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema, Default)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/GenomeTeachStatusParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct GenomeTeachStatusParams {
    /// The detached run's handle. With it, the terminal RESULT resolves from the run ledger
    /// (complete + result/error). Omit for LIVE progress only (the "how's it going" poll).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS, JsonSchema, Default)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/GenomeTeachStatusResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct GenomeTeachStatusResult {
    /// True once the run's ledger row exists (the detached run finished — check
    /// `result.ok`). Always false when polled without a run_id (live-progress only).
    pub complete: bool,
    /// The terminal ledger row `{ok, result|error}` when complete, else null.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "unknown")]
    pub result: Option<serde_json::Value>,
    /// Live progress of the currently-running teach — the mid-run scoreboard a progress
    /// bar renders (done/total/currentTask/solved). Null until the first milestone fires;
    /// check `updatedAtMs` for staleness on a long-finished run.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub progress: Option<TeachProgress>,
}

#[async_trait]
impl ActionCommand for GenomeTeachStatus {
    const NAME: &'static str = "genome/teach-status";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Poll a detached genome/teach by run_id: {complete, result} from the run ledger + LIVE \
         {done, total, currentTask, solved} progress. The observable half of fire-and-stream — a \
         UI renders it as a progress bar, a poller knows alive-vs-done, without holding a \
         connection open across the many-minute run.";
    type Params = GenomeTeachStatusParams;
    type Output = GenomeTeachStatusResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: GenomeTeachStatusParams,
    ) -> Result<GenomeTeachStatusResult, CommandError> {
        // The ledger FILE is the cross-process truth (the in-process watch cell is
        // invisible to a client-side status). A row is either LIVE (complete:false, carries
        // `progress`) or TERMINAL (complete:true, carries `result`/`error`).
        let row = p.run_id.as_deref().and_then(read_teach_ledger);
        let complete = row
            .as_ref()
            .and_then(|r| r.get("complete"))
            .and_then(|c| c.as_bool())
            .unwrap_or(false);
        // Live progress: prefer the ledger row (cross-process), fall back to the in-process
        // watch (same-process subscribers, or a handleless "how's it going" poll).
        let progress = row
            .as_ref()
            .and_then(|r| r.get("progress"))
            .and_then(|pr| serde_json::from_value::<TeachProgress>(pr.clone()).ok())
            .or_else(|| subscribe_teach_progress().borrow().clone());
        Ok(GenomeTeachStatusResult {
            complete,
            result: if complete { row } else { None },
            progress,
        })
    }
}

crate::register_stateless_command!(GenomeTeachStatus);
