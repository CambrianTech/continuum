//! The durable per-persona progress ledger: where it lives, finding a run's row, the graded task
//! ids a resume continues from, and appending the failed, per-task and run-summary rows.

use super::*;

/// Per-task detail retained in the DURABLE run ledger row (`append_progress_ledger`).
/// The ledger stored only the summary (score/total), so a detached run's per-task
/// verdicts were unrecoverable — a webdev pass/fail could not be glass-boxed after the
/// fact (glass-boxed 2026-07-21: could not tell whether an `acts=0` webdev miss was the
/// mouth-or-hands capture not firing or the spoken page failing a strict check). We now
/// persist a compact per-task array; the diagnostic `grade` verdict + answer head are kept
/// ONLY for FAILED tasks, so a 164-task humaneval row stays lean while a failing 6-task
/// webdev row is fully inspectable. [[self-test-via-command-feedback-surface-never-blind]]
// context-budget-exempt: observability: ledger rendering width for a failed grade, human-facing only
const LEDGER_FAIL_GRADE_CHARS: usize = 800;
// context-budget-exempt: observability: ledger rendering width for a failed answer, human-facing only
const LEDGER_FAIL_ANSWER_CHARS: usize = 1_200;

/// The progress-ledger directory (`~/.continuum/progress`), the ONE place
/// [`append_progress_ledger`] writes and `cognition/eval-status` reads.
pub(crate) fn progress_ledger_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|home| home.join(".continuum/progress"))
}

/// The terminal ledger row for `run_id` in a KNOWN persona's ledger, newest-first.
/// `None` = no such row yet (the run is still in flight, or the id is wrong).
pub(super) fn find_run_row_for_persona(persona_id: &str, run_id: &str) -> Option<serde_json::Value> {
    let path = progress_ledger_dir()?.join(format!("{persona_id}.jsonl"));
    let text = std::fs::read_to_string(&path).ok()?;
    row_with_run_id(&text, run_id)
}

/// The terminal ledger row for `run_id` across EVERY persona's ledger — the poll
/// path when the caller holds only the (globally-unique) run handle. Scans each
/// `*.jsonl` in the progress dir; returns the first match (a run_id lives in exactly
/// one persona's ledger). Keeps run_id a sufficient key so a matrix/CI poller never
/// hangs on a false pending just because it didn't also thread the persona_id.
pub(crate) fn find_run_row_any_persona(run_id: &str) -> Option<serde_json::Value> {
    let dir = progress_ledger_dir()?;
    for entry in std::fs::read_dir(&dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Some(row) = row_with_run_id(&text, run_id) {
                return Some(row);
            }
        }
    }
    None
}

/// Scan a jsonl ledger body newest-first for the row whose `runId` matches. Pure
/// (text in, Value out) so the match logic is unit-testable without the filesystem.
pub(super) fn row_with_run_id(text: &str, run_id: &str) -> Option<serde_json::Value> {
    for line in text.lines().rev() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            // Per-task streaming rows (`kind:"task"`) share the run_id but are NOT the
            // terminal run row — skipping them keeps eval-status meaning "the run
            // finished", while `graded_task_ids_for_run` reads them for resume.
            if v.get("kind").and_then(|k| k.as_str()) == Some("task") {
                continue;
            }
            if v.get("runId").and_then(|r| r.as_str()) == Some(run_id) {
                return Some(v);
            }
        }
    }
    None
}

/// Every task id already GRADED under `run_id`, from the streamed `kind:"task"` rows —
/// the read half of the resume seam. Pure over the ledger text; ordering preserved.
pub fn graded_task_ids_from(text: &str, run_id: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v.get("kind").and_then(|k| k.as_str()) == Some("task"))
        .filter(|v| v.get("runId").and_then(|r| r.as_str()) == Some(run_id))
        .filter_map(|v| {
            v.get("taskId")
                .and_then(|t| t.as_str())
                .map(|t| t.to_string())
        })
        .collect()
}

/// Write a FAILED run row to the progress ledger so `cognition/eval-status` surfaces
/// the error (keyed on `run_id`) instead of returning `null` forever. The poll surface
/// must be able to tell "died" from "still starting" — a detached run that errors before
/// [`append_progress_ledger`] otherwise reads as an eternal pending. `error` + `failed:true`
/// mark it; `total:0` keeps the numeric shape valid for consumers.
pub(super) fn append_failed_ledger(
    persona: &crate::identity::PersonaRef,
    run_id: &str,
    note: &str,
    error: &str,
) {
    let persona_id = persona.as_str();
    let Some(home) = std::env::var("HOME").ok() else {
        return;
    };
    let dir = std::path::PathBuf::from(home).join(".continuum/progress");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join(format!("{persona_id}.jsonl"));
    let row = serde_json::json!({
        "capturedAtMs": EpochMs::now().millis(),
        "personaId": persona_id,
        "runId": run_id,
        "note": note,
        "failed": true,
        "error": error,
        "score": 0,
        "total": 0,
    });
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(f, "{row}");
    }
}

/// One graded task, durably, at grade time — the resume seam. Same file as the run
/// summary rows (`~/.continuum/progress/<persona>.jsonl`), discriminated by
/// `kind:"task"` so every existing run-row reader skips it. Best-effort like the
/// summary writer: a ledger IO failure degrades resume, never the round.
pub(super) fn append_task_ledger_row(
    persona_id: &str,
    run_id: &str,
    eval_set: &str,
    task_id: &str,
    ok: bool,
    acts: u32,
) {
    let Some(dir) = progress_ledger_dir() else {
        return;
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let row = serde_json::json!({
        "kind": "task",
        "capturedAtMs": EpochMs::now().millis(),
        "runId": run_id,
        "personaId": persona_id,
        "evalSet": eval_set,
        "taskId": task_id,
        "ok": ok,
        "acts": acts,
    });
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(format!("{persona_id}.jsonl")))
    {
        let _ = writeln!(f, "{row}");
    }
}

pub(super) fn append_progress_ledger(
    result: &CognitionEvalResult,
    note: Option<&str>,
    eval_set: &str,
    clean_lane: Option<bool>,
    measured_gene: Option<&crate::forge::adapter_manifest::TrainedAdapter>,
) {
    let Some(dir) = progress_ledger_dir() else {
        return;
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join(format!("{}.jsonl", result.persona_id));
    // A cancelled run's terminal row says so — resume reads it as PAUSED, never done.
    let cancelled = result
        .run_id
        .as_deref()
        .is_some_and(eval_run_cancelled);
    let row = serde_json::json!({
        "cancelled": cancelled,
        "capturedAtMs": EpochMs::now().millis(),
        "personaId": result.persona_id,
        "evalSet": eval_set,
        "cleanLane": clean_lane,
        "score": result.score,
        "total": result.total,
        "passRate": result.pass_rate,
        "selfVerifyRate": result.self_verify_rate,
        "meanLatencyMs": result.mean_latency_ms,
        "p95LatencyMs": result.p95_latency_ms,
        "meanTokensPerSecond": result.mean_tokens_per_second,
        "meanDecodeTokensPerSecond": result.mean_decode_tokens_per_second,
        "meanCacheHitRate": result.mean_cache_hit_rate,
        "totalPrefillMs": result.total_prefill_ms,
        "totalDecodeMs": result.total_decode_ms,
        "totalOutputTokens": result.total_output_tokens,
        "geneId": result.gene_id,
        "baseModelId": measured_gene.map(|gene| &gene.base_model_id),
        "adapterPath": measured_gene.map(|gene| &gene.path),
        "basePassRate": result.base_pass_rate,
        "lift": result.lift,
        "note": note,
        "runId": result.run_id,
        // The Proctored Exam Session void-flag: when present, the row's score/passRate are
        // VOID (the lane died mid-exam), and every reader MUST report infra-unavailable
        // instead of a real pass-rate. [[proctored-exam-session-dependable-benchmark]]
        "infraUnavailable": result.infra_unavailable,
        // Per-task detail for post-hoc glass-boxing (why did THIS task fail?). Compact by
        // design: every task carries id/ok/acts; only FAILED tasks carry the grade verdict
        // + answer head, so the row stays lean on a mostly-passing large benchmark.
        "tasks": result.results.iter().map(|r| {
            // `outputTokens` disambiguates a FAILED task with an empty answer: >0 means she
            // GENERATED (and the empty answer is a Pass/silence cognition decision or an
            // unparseable turn); 0 means the LANE returned nothing (empty generation / wedge).
            // Without it the two read identical in the ledger and get hand-diagnosed each time.
            let mut t = serde_json::json!({ "id": r.id, "ok": r.ok, "acts": r.acts, "outputTokens": r.output_tokens });
            if !r.ok {
                t["grade"] = serde_json::json!(r.grade.chars().take(LEDGER_FAIL_GRADE_CHARS).collect::<String>());
                t["answerHead"] = serde_json::json!(r.answer.chars().take(LEDGER_FAIL_ANSWER_CHARS).collect::<String>());
            }
            t
        }).collect::<Vec<_>>(),
    });
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(f, "{row}");
    }
    // TERMINAL completion event (the fire-and-stream close, #86). Every finished eval —
    // gene A/B, base-model, single-pass — funnels through here, so this ONE publish gives a
    // monitoring widget/persona "done + final score + per-task detail" as a single push
    // instead of inferring completion from `done==total` on the last `eval:progress`. Same
    // payload as the durable ledger row: score/total/passRate/runId/geneId/lift/
    // infraUnavailable + the per-task array carrying `outputTokens`, so the CLEAN / SUSPECT /
    // VOID classification a tile renders comes straight off the wire, no post-hoc ledger read.
    // [[long-runs-are-handle-plus-events-never-poll-or-timeout]]
    if let Some(bus) = crate::runtime::MessageBus::global() {
        bus.publish_async_only("eval:complete", row);
    }
}
