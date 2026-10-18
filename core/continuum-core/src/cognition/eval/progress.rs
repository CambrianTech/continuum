//! The live state of eval runs: detached runs in flight, cancellation requests, lifecycle phase
//! events, the current run's identity scope, the progress watch every reader shares, and the one
//! reporter that publishes each graded task on every surface.

use super::*;

/// The gym command. Stateless: it reaches the persona's live cognition through the
/// global [`WorkspaceCycle`](crate::cognition::persona_workspace) registry, so it needs no host
/// module state.
/// Detached evals in flight, registered at DISPATCH time — not at first grade.
/// The idempotence gap this closes (measured live, 2026-08-24, runs 4ae8e14c +
/// 912de04f): a round's first task can grade 30+ minutes after dispatch, and until
/// then the run has no ledger rows and no RunMeta scope — so `benchmark/round`
/// re-invoked in that window happily minted a DUPLICATE fresh run. Entries are
/// (run_id, persona, eval_set reference); removed when the detached body returns
/// (either arm), so a crashed core clears it by process death — never a stale file.
pub(super) static DETACHED_IN_FLIGHT: parking_lot::Mutex<Vec<(String, String, String)>> =
    parking_lot::Mutex::new(Vec::new());

/// Cancel requests by run_id — the iteration primitive a reboot used to stand in
/// for (2026-08-24, Joel: "can't iterate to put the new one in place"). The task
/// loop checks between tasks; a cancelled run stops cleanly, keeps every grade
/// row already streamed, and writes its terminal row with `cancelled:true` so
/// resume treats it as PAUSED (the same command continues it later).
static CANCELLED_RUNS: parking_lot::Mutex<Vec<String>> = parking_lot::Mutex::new(Vec::new());

/// Request cancellation of a run. Effective at the next task boundary.
pub(crate) fn cancel_eval_run(run_id: &str) {
    let mut g = CANCELLED_RUNS.lock();
    if !g.iter().any(|r| r == run_id) {
        g.push(run_id.to_string());
    }
}

/// Has this run been asked to stop? Checked between tasks (and cleared when the
/// run exits).
pub(crate) fn eval_run_cancelled(run_id: &str) -> bool {
    CANCELLED_RUNS.lock().iter().any(|r| r == run_id)
}

pub(crate) fn clear_eval_cancel(run_id: &str) {
    CANCELLED_RUNS.lock().retain(|r| r != run_id);
}

/// Snapshot of the detached evals dispatched and not yet finished. For
/// `benchmark/round`'s idempotence gate.
pub(crate) fn detached_evals_in_flight() -> Vec<(String, String, String)> {
    DETACHED_IN_FLIGHT.lock().clone()
}

/// Emit an eval LIFECYCLE phase on the bus (+ a probe) so every positronic layer can
/// show live feedback during the windows where `eval:progress` is silent — lane
/// spawn, model cold-load, template wait, teardown. Fire-and-forget, Noop when the
/// bus is unwired (tools/tests): observability at zero hot-path cost
/// [[observability-as-substrate]]. `eval:progress` carries per-task grading; this
/// carries the BEFORE/AROUND-generation states that were previously dark.
pub(crate) fn emit_eval_phase(phase: &str, detail: &str) {
    if let Some(bus) = crate::runtime::MessageBus::global() {
        bus.publish_async_only(
            "eval:phase",
            serde_json::json!({
                "phase": phase,
                "detail": detail,
                "atMs": EpochMs::now().millis(),
            }),
        );
    }
    crate::probe!(
        class = "eval.phase",
        phase = phase,
        detail = detail,
        "eval lifecycle phase"
    );
}

#[derive(Debug, Clone, Serialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/EvalPassProgress.ts"
)]
pub struct EvalPassProgress {
    /// Tasks graded so far in the CURRENT pass.
    #[ts(type = "number")]
    pub done: u32,
    /// Tasks in the current pass.
    #[ts(type = "number")]
    pub total: u32,
    /// Passes so far — `pass / done` is the running rate.
    #[ts(type = "number")]
    pub pass: u32,
    /// The task just graded.
    pub current_task: String,
    /// Whether it passed.
    pub last_ok: bool,
    /// Output tokens the model produced for this task. A near-zero value on a FAILED
    /// task is the decline/wedge signature (a bare "PASS" is ~2 tokens) — this is the
    /// LIVE signal a monitoring widget/persona needs to light a cell SUSPECT (harness
    /// noise) vs a real wrong answer, in real time instead of post-hoc from the ledger.
    #[ts(type = "number")]
    pub output_tokens: u32,
    /// Receiver-clock ms — staleness signal for readers (a row older than a task's
    /// typical latency means the pass ended or died; the ledger row is the truth).
    #[ts(type = "number")]
    pub updated_at_ms: u64,
    /// Live VRAM available (GB) at grade time — the resource-efficiency axis next to
    /// accuracy and latency, sampled from the ONE per-machine authority. `null` when
    /// VRAM is ungoverned (bare tests) — absence is honest, never a fabricated 0.
    #[ts(optional, type = "number")]
    pub vram_free_gb: Option<u64>,
    /// The run_id producing THIS snapshot, when the pass carries a handle. Lets a
    /// persona-only poll (no run_id arg) tell it's reading ITS run vs a prior one —
    /// the stale-progress trap where a fresh detached run reads as instantly-complete
    /// with the last run's numbers. `null` for a sync/handleless run.
    #[ts(optional)]
    pub run_id: Option<String>,
}

/// Identity of the pass currently grading — set at [`run_eval`] entry (RAII), read
/// by [`report_task_graded`] into the live snapshot AND the durable per-task ledger
/// row. Correct-by-construction because the eval-preemption lease serializes evals:
/// at most one pass grades at any instant (the same invariant that makes ONE
/// progress row correct). Grew from a bare run_id (2026-08-24): the per-task row
/// needs the persona (which ledger file) and the eval set (which gym) at grade time,
/// and threading them through both pass loops would be the param-hell the scope
/// exists to avoid.
struct RunMeta {
    run_id: Option<String>,
    persona_id: String,
    eval_set: String,
}
static CURRENT_RUN_META: parking_lot::Mutex<Option<RunMeta>> = parking_lot::Mutex::new(None);

/// RAII: stamp the current pass's identity for the live snapshot; clear on drop so it
/// rides early-return AND panic (Drop runs on unwind) — never a stale run_id bleeding
/// into the next pass or an idle board.
pub(super) struct RunIdScope;
impl RunIdScope {
    pub(super) fn enter(run_id: Option<String>, persona_id: String, eval_set: String) -> Self {
        *CURRENT_RUN_META.lock() = Some(RunMeta {
            run_id,
            persona_id,
            eval_set,
        });
        RunIdScope
    }
}
impl Drop for RunIdScope {
    fn drop(&mut self) {
        *CURRENT_RUN_META.lock() = None;
    }
}

/// The run_id of the exam pass grading RIGHT NOW, `None` when idle. Public for
/// the positron bench emitter (#141's third surface made real): a live exam is
/// WORK the room's rail must show, through the same pipe every other run row
/// rides — never a separate poller.
pub fn live_eval_run_id() -> Option<String> {
    CURRENT_RUN_META.lock().as_ref().and_then(|m| m.run_id.clone())
}

static EVAL_PROGRESS: std::sync::OnceLock<tokio::sync::watch::Sender<Option<EvalPassProgress>>> =
    std::sync::OnceLock::new();

fn eval_progress_tx() -> &'static tokio::sync::watch::Sender<Option<EvalPassProgress>> {
    EVAL_PROGRESS.get_or_init(|| tokio::sync::watch::channel(None).0)
}

/// Subscribe to live eval progress — the watch every reader shares.
pub fn subscribe_eval_progress() -> tokio::sync::watch::Receiver<Option<EvalPassProgress>> {
    eval_progress_tx().subscribe()
}

/// Report one graded task on all three surfaces. Called by BOTH pass loops (solo +
/// team) — one reporter, no drift.
pub(super) fn report_task_graded(
    task_id: &str,
    ok: bool,
    acts: u32,
    latency_ms: u64,
    output_tokens: u32,
    pass: u32,
    done: usize,
    total: usize,
) {
    // Efficiency axis: sample the live board (lock-free watch read) so every graded
    // task carries the VRAM state it ran under — the tuning signal for knobs like
    // max_acts / context budget / lane count ([[self-improvement-is-a-control-loop]]:
    // the reward is only as trustworthy as the metric, and a score without its
    // resource cost is half a metric).
    let vram_free_gb = governed_vram_available().map(|bytes| bytes / 1_000_000_000);
    let run_id = live_eval_run_id();
    let snap = EvalPassProgress {
        done: done as u32,
        total: total as u32,
        pass,
        current_task: task_id.to_string(),
        last_ok: ok,
        output_tokens,
        updated_at_ms: EpochMs::now().millis(),
        vram_free_gb,
        run_id: run_id.clone(),
    };
    // DURABLE per-task grade row (the one-command-round resume seam): the run summary
    // row only lands when the whole pass completes, so a killed/rebooted round left NO
    // record of which tasks already graded — resume meant re-running everything. Stream
    // one `kind:"task"` row into the SAME per-persona ledger at grade time (the file
    // eval-status already owns; `row_with_run_id` skips task rows) so
    // `benchmark/round` can continue from the first ungraded task.
    if let Some(RunMeta { run_id: Some(rid), persona_id, eval_set }) = CURRENT_RUN_META.lock().as_ref() {
        append_task_ledger_row(persona_id, rid, eval_set, task_id, ok, acts);
    }
    let _ = eval_progress_tx().send_replace(Some(snap.clone()));
    if let Some(bus) = crate::runtime::MessageBus::global() {
        if let Ok(v) = serde_json::to_value(&snap) {
            bus.publish_async_only("eval:progress", v);
        }
    }
    // run_id ON the row (2026-08-24, "not good to be so dumb about what's graded"):
    // an unscoped consumer counting eval.task matches summed every round, sentinel
    // A/B, and smoke eval ever run — "51 graded" while the live round had zero. The
    // row itself must carry which round it belongs to.
    crate::probe!(
        class = "eval.task",
        task = task_id,
        run_id = ?run_id,
        ok = ok,
        acts = acts,
        done = done,
        total = total,
        pass = pass,
        running_rate = format!("{:.3}", pass as f64 / done.max(1) as f64).as_str(),
        latency_ms = latency_ms,
        vram_free_gb = ?vram_free_gb,
        "task graded",
    );
}
