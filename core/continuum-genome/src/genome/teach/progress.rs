//! Live teach progress: the watch cell, the live run-ledger row, and the bus milestone events
//! a curriculum synthesis emits. The terminal result row is the `genome/teach` command's.

use continuum_base::time::EpochMs;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Live teach progress — the queryable snapshot `genome/teach-status` returns, so a
/// long corpus-gen run is OBSERVABLE (poll `done/total/currentTask`) instead of a
/// black-box wait or an autopsy of CPU%. Mirrors eval's `EvalPassProgress` watch cell —
/// bus events are fire-and-forget (a UI subscribes), but a STATUS you can query is what
/// lets an operator or a poller see where a run actually is. Clean harness = you know
/// what's going on. [[self-test-via-command-feedback-surface-never-blind]]
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema, Default)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/TeachProgress.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct TeachProgress {
    /// `started` (denominator set, no work yet) | `task` (one graded) | `completed`.
    pub phase: String,
    /// Tasks graded so far.
    #[ts(type = "number")]
    pub done: usize,
    /// Total tasks in the set (the progress-bar denominator, known at `started`).
    #[ts(type = "number")]
    pub total: usize,
    /// Validated (test-passing) trajectories so far — the corpus yield.
    #[ts(type = "number")]
    pub solved: usize,
    /// The task just graded (None at `started`/`completed`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub current_task: Option<String>,
    /// Wall-clock stamp — a poller checks this for staleness on a long-dead run.
    #[ts(type = "number")]
    pub updated_at_ms: u64,
}

static TEACH_PROGRESS: std::sync::OnceLock<tokio::sync::watch::Sender<Option<TeachProgress>>> =
    std::sync::OnceLock::new();

fn teach_progress_tx() -> &'static tokio::sync::watch::Sender<Option<TeachProgress>> {
    TEACH_PROGRESS.get_or_init(|| tokio::sync::watch::channel(None).0)
}

/// Subscribe to live teach progress — the watch `genome/teach-status` reads.
pub fn subscribe_teach_progress() -> tokio::sync::watch::Receiver<Option<TeachProgress>> {
    teach_progress_tx().subscribe()
}

fn set_teach_progress(p: TeachProgress) {
    let _ = teach_progress_tx().send(Some(p));
}

/// The run ledger for a detached teach job — one JSON row per `run_id`, the DURABLE half of
/// fire-and-stream (#86). The watch cell + bus carry PROGRESS (push, live); this file carries
/// the terminal RESULT, cross-process and surviving the run, so `genome/teach-status --run_id`
/// resolves to complete+result (or a failed row) regardless of who's watching or when.
pub fn teach_ledger_path(run_id: &str) -> Option<std::path::PathBuf> {
    progress_class().ok()?.path(&ledger_name(run_id)).ok()
}

/// Run ledgers live in the `progress` disk class (`disk_reporters::standard`): measured, with an
/// eviction decision, written only through the class.
const PROGRESS_CLASS: &str = "progress";

fn progress_class() -> std::io::Result<continuum_kernel::system_resources::disk_reporters::ClassWriter> {
    continuum_kernel::system_resources::disk_reporters::ClassWriter::of(PROGRESS_CLASS)
}

fn ledger_name(run_id: &str) -> std::path::PathBuf {
    std::path::Path::new("teach").join(format!("{run_id}.json"))
}

/// Replace `run_id`'s ledger row whole (staged, synced, placed through the class): the one writer
/// of a teach run's ledger, for its live progress rows and its terminal row alike.
pub fn write_teach_ledger(run_id: &str, row: &serde_json::Value) -> std::io::Result<()> {
    progress_class()?.replace(&ledger_name(run_id), serde_json::to_string(row)?.as_bytes())
}

/// The run_id of the teach pass executing NOW — set at `run_teach` entry so the emit
/// helpers can write LIVE progress to that run's ledger file WITHOUT threading run_id
/// through synthesize_remediation. Cross-process observability needs the file (the
/// in-process watch cell is invisible to a client-side `teach-status`); this is how the
/// progress bar advances for any poller/widget. Mirrors eval's CURRENT_RUN_ID.
static CURRENT_TEACH_RUN: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub fn set_current_teach_run(run_id: Option<String>) {
    if let Ok(mut g) = CURRENT_TEACH_RUN.lock() {
        *g = run_id;
    }
}

/// Write a LIVE progress row (complete:false) to the current run's ledger — overwritten
/// each milestone/task, then replaced by the terminal row on completion (last write wins).
fn write_teach_progress_ledger(prog: &TeachProgress) {
    let run_id = match CURRENT_TEACH_RUN.lock() {
        Ok(g) => match g.clone() {
            Some(r) => r,
            None => return,
        },
        Err(_) => return,
    };
    let row = serde_json::json!({ "runId": run_id, "complete": false, "progress": prog });
    if let Err(error) = write_teach_ledger(&run_id, &row) {
        tracing::warn!(%run_id, %error, "teach progress row not written: a status poller sees the previous row");
    }
}

/// Emit ONE teach progress event to the message bus, so a UI / the operator can WATCH
/// corpus generation live instead of black-box-waiting for the final result (the whole
/// point of long jobs being observable — the same `MessageBus` seam `emit_eval_phase`
/// uses for `eval:phase`). Feedback is a first-class cross-modality dimension: this
/// long job streams its own progress. [[feedback-is-a-first-class-cross-modality-dimension-jtag-cu]]
pub(crate) fn emit_teach_progress(
    done: usize,
    total: usize,
    task_id: &str,
    solved: bool,
    solved_count: usize,
    with_correction: usize,
) {
    let prog = TeachProgress {
        phase: "task".to_string(),
        done,
        total,
        solved: solved_count,
        current_task: Some(task_id.to_string()),
        updated_at_ms: EpochMs::now().millis(),
    };
    write_teach_progress_ledger(&prog); // cross-process (file) so teach-status shows it
    set_teach_progress(prog); // in-process watch (same-process subscribers)
    if let Some(bus) = crate::runtime::MessageBus::global() {
        bus.publish_async_only(
            "genome:teach:progress",
            serde_json::json!({
                "phase": "task",
                "done": done,
                "total": total,
                "task": task_id,
                "solved": solved,
                "withCorrection": with_correction,
                "atMs": EpochMs::now().millis(),
            }),
        );
    }
    tracing::info!(
        target: "genome::teach",
        done, total, task = task_id, solved, with_correction,
        "teach task graded"
    );
}

/// A lifecycle MILESTONE event — `started` (carries `total`, the progress bar's
/// denominator, before any work) and `completed` (the terminal fill). Every milestone
/// emits an event so a UI can render a real progress bar: `started` sizes it, the
/// per-task `emit_teach_progress` increments it, `completed` closes it. Same seam as
/// the per-task events. [[feedback-is-a-first-class-cross-modality-dimension-jtag-cu]]
pub(crate) fn emit_teach_milestone(phase: &str, done: usize, total: usize, solved: usize) {
    let prog = TeachProgress {
        phase: phase.to_string(),
        done,
        total,
        solved,
        current_task: None,
        updated_at_ms: EpochMs::now().millis(),
    };
    write_teach_progress_ledger(&prog);
    set_teach_progress(prog);
    if let Some(bus) = crate::runtime::MessageBus::global() {
        bus.publish_async_only(
            "genome:teach:progress",
            serde_json::json!({
                "phase": phase,
                "done": done,
                "total": total,
                "solved": solved,
                "atMs": EpochMs::now().millis(),
            }),
        );
    }
    tracing::info!(target: "genome::teach", phase, done, total, solved, "teach milestone");
}
