use super::*;

/// Build the durable LESSON string for one graded exam task — what she was asked
/// and how she did. Pure and answer-key-agnostic (the caller's redaction policy
/// scrubs the crib sheet); kept separate so it's unit-testable.
pub(super) fn format_exam_lesson(task: &EvalTask, result: &EvalTaskResult) -> String {
    // BELIEF HYGIENE (2026-08-24, the false-failure lesson): a PASS is strong
    // evidence — say it plainly. A MISS is WEAK, noisy evidence — a broken env
    // (the bare-`python` exit-127 that produced "5 failed"), a one-off, or a real
    // gap, indistinguishable at grade time. So a miss is phrased as a PROVISIONAL
    // single attempt, explicitly not a verdict on ability and explicitly
    // retryable, so it can never harden into "I am bad at X" self-belief and the
    // dream reviewer can SUPERSEDE it when a later attempt on the same task passes
    // (both share the `task.id` recall key). Redaction still strips the answer key.
    if result.ok {
        format!(
            "Exam task '{}'. I was asked: {} — I solved it (grade: {}).",
            task.id.trim(),
            task.prompt.trim(),
            result.grade.trim()
        )
    } else {
        format!(
            "Exam task '{}'. I was asked: {} — this attempt did not pass yet (grade: {}).              One attempt, not a settled verdict on my ability: the cause may be my approach              OR the environment, and it is worth retrying. If a later attempt passes, that              supersedes this.",
            task.id.trim(),
            task.prompt.trim(),
            result.grade.trim()
        )
    }
}

/// PER-TASK lesson streaming (2026-08-23, Joel: "she going to learn from
/// this?" — and the honest answer exposed that end-of-run transfer meant
/// every KILLED run taught NOTHING: eight attempts that day, zero lessons
/// delivered, because the teacher waited for the final bell). The sink is
/// built ONCE per learning run (policy over the WHOLE set's answer keys, so
/// a lesson about task A can never leak task B's key either; the living
/// self's admission + experience dir resolved once) and each task's lesson
/// streams AT GRADE TIME — an interrupted round keeps every lesson it
/// earned. Resume-is-recall applied to teaching.
pub(crate) struct LessonSink {
    pub(super) policy: crate::persona::redaction::RedactionPolicy,
    pub(super) persona_uuid: uuid::Uuid,
    pub(super) room: uuid::Uuid,
    pub(super) peer_dir: std::path::PathBuf,
    pub(super) admission: std::sync::Arc<crate::persona::admission_state::AdmissionState>,
    pub(super) admitted: std::sync::atomic::AtomicUsize,
}

impl LessonSink {
    /// `None` when the living self is not resident to teach (measured but
    /// unteachable — the caller logs it once).
    pub(crate) fn open(
        persona_uuid: &uuid::Uuid,
        room: uuid::Uuid,
        tasks: &[EvalTask],
    ) -> Option<Self> {
        let answers: Vec<String> = tasks
            .iter()
            .map(|t| t.expect.clone())
            .filter(|a| !a.trim().is_empty())
            .collect();
        let policy = crate::persona::redaction::RedactionPolicy::new(vec![Box::new(
            crate::persona::redaction::HeldOutAnswerDetector::new(
                answers,
                crate::persona::redaction::HeldOutAnswerDetector::DEFAULT_MIN_LEN,
            ),
        )]);
        let peer_dir = crate::identity::citizen_peer_dir(
            &crate::paths::continuum_root(),
            crate::identity::PeerId::from_uuid(*persona_uuid),
        );
        if let Err(e) = std::fs::create_dir_all(&peer_dir) {
            tracing::warn!(error = %e, "citizen dir for the experience stream could not be created — exam episodes stay out of curriculum this run");
        }
        let admission = crate::cognition::persona_workspace::global()
            .get(persona_uuid)
            .and_then(|cycle| cycle.acting().map(|a| a.admission.clone()))?;
        Some(Self {
            policy,
            persona_uuid: *persona_uuid,
            room,
            peer_dir,
            admission,
            admitted: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    /// Stream ONE graded task's lesson + experience episode, at grade time.
    /// Infra-graded tasks are skipped — a dead lane is substrate evidence,
    /// never her lesson.
    pub(crate) fn teach(&self, task: &EvalTask, result: &EvalTaskResult) {
        if task.prompt.trim().is_empty() || result.grade.trim_start().starts_with("infra") {
            return;
        }
        let (lesson, _report) = self.policy.redact(&format_exam_lesson(task, result));
        let engram = crate::persona::engram::Engram {
            id: uuid::Uuid::new_v4(),
            context_id: Some(self.room),
            kind: crate::persona::engram::EngramKind::Episodic,
            content: lesson,
            origin: crate::persona::engram::EngramOrigin::SelfReflection {
                parent_engram_id: uuid::Uuid::nil(),
            },
            recall_keys: vec!["exam".to_string(), task.id.clone()],
            admitted_at_ms: EpochMs::now().millis(),
            trust_state_at_admission: crate::persona::engram::TrustState::SelfTrust,
            admission_trace_id: None,
        };
        if matches!(
            self.admission.admit_reflection(engram),
            Ok(crate::persona::engram::AdmissionDecision::Admit { .. })
        ) {
            self.admitted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            continuum_base::probe!(
                class = "eval.lesson.streamed",
                task = %task.id,
                ok = result.ok,
                "redacted lesson admitted at GRADE time — an interrupted round keeps every lesson it earned"
            );
        }
        let mut episode =
            crate::cognition::experience::ExperienceRecord::from_eval_result(task, result);
        // The sink KNOWS the activity — carry it (A6), and when a tracked round
        // runs a team solve in this room, stamp the team + this persona's seat.
        // A reviewer's episode and a solver's episode from the same room teach
        // different skills; unattributed rows can never become that corpus.
        episode.room = Some(self.room);
        if let Some(team) = crate::cognition::bench_round::team_for_room(self.room) {
            episode.team_role = Some(if team.assignee == self.persona_uuid {
                crate::cognition::experience::TeamRole::Solver
            } else {
                crate::cognition::experience::TeamRole::Reviewer
            });
            episode.teammates = std::iter::once(team.assignee)
                .chain(team.teammates.iter().copied())
                .filter(|p| *p != self.persona_uuid)
                .collect();
        }
        if let Err(e) = crate::cognition::experience::append_experience(&self.peer_dir, &episode) {
            tracing::warn!(
                persona = %self.persona_uuid,
                task = %task.id,
                error = %e,
                "exam episode could not join the experience stream — lesson admitted, curriculum blind to it"
            );
        }
    }

    pub(crate) fn admitted(&self) -> usize {
        self.admitted.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// SEAM 3a: she REMEMBERS BECOMING BETTER. The progress ledger records
/// passRate durably, but no engram ever carried it — a persona's beliefs
/// formed with no awareness of her own measured capability. One Semantic
/// summary per learned exam, keyed ["progress", <eval_set>], so the dream
/// can cluster successive exams into a trajectory belief. (The per-task
/// lesson half of the old transfer step now STREAMS at grade time — see
/// [`LessonSink`]; this run-shape summary is the one piece that genuinely
/// needs the whole run.)
pub(super) fn admit_progress_engram(
    admission: &crate::persona::admission_state::AdmissionState,
    room: uuid::Uuid,
    eval_set: &str,
    results: &[EvalTaskResult],
) {
    let solved = results.iter().filter(|r| r.ok).count();
    let graded = results.len();
    if graded > 0 {
        let rate = solved as f32 / graded as f32;
        let summary = crate::persona::engram::Engram {
            id: uuid::Uuid::new_v4(),
            context_id: Some(room),
            kind: crate::persona::engram::EngramKind::Semantic,
            content: format!(
                "Exam '{eval_set}': I solved {solved} of {graded} tasks (rate {rate:.2})."
            ),
            origin: crate::persona::engram::EngramOrigin::SelfReflection {
                parent_engram_id: uuid::Uuid::nil(),
            },
            recall_keys: vec!["progress".to_string(), eval_set.to_string()],
            admitted_at_ms: EpochMs::now().millis(),
            trust_state_at_admission: crate::persona::engram::TrustState::SelfTrust,
            admission_trace_id: None,
        };
        let _ = admission.admit_reflection(summary); // duplicate-content dedup is the store's job; a repeat exam at the same rate is legitimately the same belief
    }
}

/// Fraction of tasks where she ACTED at least once before settling — the
/// verify-don't-assert reflex as a number. `acts=0` everywhere = she answered
/// from the model's head and never ran her own code (the #1 trainable lever per
/// [[asha-coder-baseline-and-spawn-race]]).
pub(super) fn self_verify_rate(results: &[EvalTaskResult]) -> f64 {
    if results.is_empty() {
        return 0.0;
    }
    let acted = results.iter().filter(|r| r.acts > 0).count();
    acted as f64 / results.len() as f64
}

/// Speed + latency aggregates over the per-task results — the SPEED and LATENCY
/// axes of the scoreboard. P95 is the honest tail (a mean hides an occasional
/// stall); wall-clock throughput is averaged per-task (not total_tokens/total_ms)
/// so one slow task can't dominate. The PREFILL-vs-DECODE breakdown (real decode
/// tok/s, cache-hit-rate, the prefill/decode ms split) is the lever data that
/// makes speed iterable instead of one conflated number — measured 77% of eval
/// time was prefill, not decode. Empty set → all zero.
#[derive(Debug, PartialEq)]
pub(super) struct SpeedAggregates {
    pub(super) mean_latency_ms: f64,
    pub(super) p95_latency_ms: u64,
    /// WALL-CLOCK mean tok/s — diluted by prefill + cognition overhead.
    pub(super) mean_tokens_per_second: f64,
    /// REAL mean decode tok/s off the lane clock — the honest generation rate.
    pub(super) mean_decode_tokens_per_second: f64,
    /// Mean KV-cache hit-rate (cached / total prompt tokens).
    pub(super) mean_cache_hit_rate: f64,
    pub(super) total_prefill_ms: u64,
    pub(super) total_decode_ms: u64,
    pub(super) total_output_tokens: u32,
}

pub(super) fn speed_latency_aggregates(results: &[EvalTaskResult]) -> SpeedAggregates {
    if results.is_empty() {
        return SpeedAggregates {
            mean_latency_ms: 0.0,
            p95_latency_ms: 0,
            mean_tokens_per_second: 0.0,
            mean_decode_tokens_per_second: 0.0,
            mean_cache_hit_rate: 0.0,
            total_prefill_ms: 0,
            total_decode_ms: 0,
            total_output_tokens: 0,
        };
    }
    let n = results.len() as f64;
    let mean_latency = results.iter().map(|r| r.latency_ms as f64).sum::<f64>() / n;
    let mean_tps = results.iter().map(|r| r.tokens_per_second).sum::<f64>() / n;
    let mean_decode_tps = results
        .iter()
        .map(|r| r.decode_tokens_per_second)
        .sum::<f64>()
        / n;
    let mean_cache_hit = results.iter().map(|r| r.cache_hit_rate).sum::<f64>() / n;
    let total_out = results
        .iter()
        .fold(0u32, |acc, r| acc.saturating_add(r.output_tokens));
    let total_prefill_ms = results
        .iter()
        .fold(0u64, |acc, r| acc.saturating_add(r.prefill_ms));
    let total_decode_ms = results
        .iter()
        .fold(0u64, |acc, r| acc.saturating_add(r.decode_ms));
    // P95: sort latencies, take the ceil(0.95 * n)-th (1-indexed) — the value at or
    // below which 95% of tasks settled.
    let mut lat: Vec<u64> = results.iter().map(|r| r.latency_ms).collect();
    lat.sort_unstable();
    let idx = (((lat.len() as f64) * 0.95).ceil() as usize)
        .saturating_sub(1)
        .min(lat.len() - 1);
    SpeedAggregates {
        mean_latency_ms: mean_latency,
        p95_latency_ms: lat[idx],
        mean_tokens_per_second: mean_tps,
        mean_decode_tokens_per_second: mean_decode_tps,
        mean_cache_hit_rate: mean_cache_hit,
        total_prefill_ms,
        total_decode_ms,
        total_output_tokens: total_out,
    }
}

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

/// `clean_lane`: whether an eval-preemption quiesce lease was held for this run
/// (`Some(true)` = the live fleet was suspended, the fork measured on a clean GPU
/// lane; `None` = no lease acquired — no live roster, so the provenance is UNKNOWN,
/// never falsely claimed clean). This is the honesty stamp: a number carries whether
/// it was measured contended, on the durable row, so `cognition/observe` can light a
/// CLEAN/UNKNOWN chip instead of anyone inferring it. [[benchmark-numbers-carry-gpu-provenance]]
/// Build the Proctored Exam Session's serving context for the living-persona benchmark:
/// read the live serving snapshot + the governed budget, construct the resident set and the
/// exam's `LaneDemand`, and run the strategic [`ExamServingContext::acquire`] admission
/// decision. For the living-persona exam she is served on this base already, so the verdict
/// is `SharedLane` — she is measured as a co-tenant slot on her real lane (no second weight
/// copy) and the lane is held steady for the run. Making it an EXPLICIT decision (not the
/// old blind steady-hold) is the strategic layer plugged in at the exam seam
/// ([[lane-admission-planner-scenario-driven]]). When the live inputs aren't resolvable
/// (ungoverned host, model row missing, lane not yet ready) it falls back to holding the
/// lane steady on the historical share assumption — behavior never regresses, but the gap
/// is marked so it's visible. [[proctored-exam-session-dependable-benchmark]]
pub(super) async fn acquire_exam_serving_context() -> crate::cognition::exam_serving::ExamServingContext {
    use crate::cognition::exam_serving::ExamServingContext;
    use crate::resources::placement::{DemandTier, LaneDemand, ResidentLane};

    let snap = crate::inference::llama_server::current_serving();
    let (Some(active), true) = (snap.active_model.as_deref(), snap.ready) else {
        return ExamServingContext::ludicrous_fallback().await;
    };
    let Some(model) = crate::model_registry::try_global().and_then(|r| r.model(active).cloned())
    else {
        return ExamServingContext::ludicrous_fallback().await;
    };
    let Some(fp) = crate::cognition::serving_plan::footprint::footprint_for(&model) else {
        return ExamServingContext::ludicrous_fallback().await;
    };
    // Capacity = the ONE memory authority's governed budget (VRAM netted over every measured
    // consumer + external pressure), the same source `plan_serving` sizes against — never a
    // raw GPU probe. No authority (ungoverned host) or a zero budget ⇒ fall back.
    let Some(capacity) = crate::resources::ResourceDaemon::global()
        .map(|d| crate::modules::serving_daemon::governed_host_budget(&d).usable_bytes)
        .filter(|c| *c > 0)
    else {
        return ExamServingContext::ludicrous_fallback().await;
    };
    let window = snap.served_context_window.max(1);
    let compute_buffer = fp.compute_buffer_per_lane();
    let resident = ResidentLane {
        lane_id: "live".to_string(),
        base_model_id: active.to_string(),
        weights_bytes: fp.weights_bytes,
        slots: snap.lanes.max(1),
        window,
        kv_per_token: fp.kv_per_token,
        compute_buffer,
        tier: DemandTier::Live,
        pinned: true,
    };
    // The exam demand: her SAME base, one measured decode slot, at the live served window, at
    // Eval tier (preemptible by live work, never preempts it) — and ISOLATED. An exam is a
    // hard, disruption-intolerant task: a co-tenant slot on the live persona lane starves it
    // behind their turns (glass-boxed 2026-07-21 — looked like the model "spinning" but was the
    // lane taken away mid-thought). `isolate: true` makes the planner give it its OWN dedicated
    // lane when a fresh copy fits (autonomic — no `base_model_id` hand-holding), falling back to
    // a co-tenant share only under real memory pressure. See `LaneDemand::isolate`.
    // Size the DEDICATED exam lane's window to what actually fits beside the resident live
    // lane, instead of mirroring the live window. A fresh copy's KV at the full served window
    // (e.g. 47872 × 1 slot ≈ 8 GB) on top of the resident lane (2 slots at that window ≈ 30 GB)
    // can exceed the free budget — which forced the isolate demand back to a co-tenant SHARE
    // that then STARVED the exam behind live turns (glass-boxed 2026-07-21: dedicated lane never
    // spawned, 0/3 graded in 12 min). Compute the largest window whose fresh copy fits the free
    // space; a self-contained coder task needs far less than the live chat window, so this keeps
    // its OWN lane (the isolation is the point) at a window matched to the budget. If not even a
    // floor-sized copy fits, `isolate` still degrades to SHARE in plan_placement (never an OOM).
    let free = capacity.saturating_sub(resident.footprint());
    let kv_budget = free.saturating_sub(fp.weights_bytes.saturating_add(compute_buffer));
    let max_fit_window = if fp.kv_per_token > 0 {
        (kv_budget / fp.kv_per_token).min(u32::MAX as u64) as u32
    } else {
        window
    };
    // Capped at the live window (the exam never asks for MORE context than the persona actually
    // has) and at what genuinely fits beside the resident lane. The old `.max(EXAM_WINDOW_FLOOR
    // = 8192)` here was a magic floor that could exceed the real fit — asking for KV the host
    // could not hold, on the theory that 8k is "enough for a task". Honest fit is the floor: if
    // even a floor-sized copy won't fit, `isolate` degrades to SHARE in plan_placement rather
    // than inventing headroom. [[never-hardcode-a-context-window-4k-defaults-destroy-the-moe-thesis]]
    let exam_window = window
        .min(max_fit_window)
        .max(crate::inference::planning::MIN_SERVE_CTX);
    let demand = LaneDemand {
        base_model_id: active.to_string(),
        weights_bytes: fp.weights_bytes,
        slots: 1,
        window: exam_window,
        kv_per_token: fp.kv_per_token,
        compute_buffer,
        tier: DemandTier::Eval,
        isolate: true,
    };
    ExamServingContext::acquire(capacity, std::slice::from_ref(&resident), &demand).await
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
    let row = serde_json::json!({
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

/// Run the full task set once through the persona's LIVE cognition, returning the
/// pass count and per-task results. Operates over whatever genome is currently
/// paged into `cycle` — the A/B pages a gene in/out around two calls to this, so
/// the base and candidate arms run the IDENTICAL motion (the only difference is
/// the genome). That sameness is what makes the lift a fair measurement.
/// How many times a FAILED (test-graded) task is handed its compiler/test output to fix
/// before we score it a miss. This is the agentic-recovery budget — the edge over one-shot
/// inference. 3 keeps a full eval bounded while giving real iteration room.
pub(super) const MAX_FAIL_RETRIES: u32 = 3;

/// Seconds to wait (1s/try) for a persona's fork-template to appear before failing the
/// eval. Covers the post-reboot window where the supervisor's `register_from_cfg` is
/// still assembling the mind when an eval fires (the fork race, 2026-07-17).
pub(super) const WORKSPACE_TEMPLATE_WAIT_TRIES: u32 = 10;
