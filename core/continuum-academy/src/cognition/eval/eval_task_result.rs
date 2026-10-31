use super::*;

#[derive(Debug, Clone, Serialize, TS)]
pub struct EvalTaskResult {
    pub id: String,
    /// Did the task pass (test exit 0, or substring matched)?
    pub ok: bool,
    /// Human-readable verdict: "tests passed" / a trimmed traceback / "timeout
    /// (10s)" for test tasks; "substring match" / "no match" for descriptive ones.
    pub grade: String,
    /// How many times she acted (ran code / read / searched) before settling.
    #[ts(type = "number")]
    pub acts: u32,
    /// What she SPOKE once settled — the COMPLETE generation (empty if she ran out of
    /// the act budget mid-action — an honest "did not finish", never fabricated). For a
    /// coder exam the answer IS the code, so this must carry the whole thing: a 200-char
    /// cap stored only the "Sure, I can help..." preamble and made every failure
    /// undiagnosable AND un-minable for a correction corpus (2026-07-21). Bounded by
    /// ANSWER_CAPTURE_CHARS against a pathological loop-to-length, which is itself a
    /// signal worth capturing up to the cap.
    pub answer: String,
    /// Wall-clock latency to SETTLE this task: the summed deliberation-generation
    /// time across every act→observe tick (the model's own measured request time,
    /// not the harness's). The latency axis of the scoreboard, per task.
    #[ts(type = "number")]
    pub latency_ms: u64,
    /// Completion tokens the model emitted across the task's deliberation turns —
    /// the work done, paired with `latency_ms` to derive throughput.
    #[ts(type = "number")]
    pub output_tokens: u32,
    /// Decode throughput for this task: `output_tokens / (latency_ms / 1000)`. The
    /// speed axis of the scoreboard, per task. 0 when the gateway omitted `usage`.
    /// NOTE: WALL-CLOCK tok/s — diluted by prefill + cognition overhead. See
    /// `decode_tokens_per_second` for the lane's undiluted rate.
    pub tokens_per_second: f64,
    /// REAL decode throughput from the lane clock (`output_tokens / decode_ms`),
    /// undiluted by prefill. The honest generation speed. 0 when the lane omitted
    /// timings (cloud / older endpoints).
    pub decode_tokens_per_second: f64,
    /// Fraction of prompt tokens served from KV cache across this task's acts —
    /// `cached / (cached + prefilled)`. Low = re-encoding the ~2000-token prompt
    /// every act (the dominant Metal inefficiency). 0 when no lane timings.
    pub cache_hit_rate: f64,
    /// Lane wall-ms spent PREFILLING across this task's acts (the re-rasterization
    /// tax). On Metal this dwarfs `decode_ms`; the lever to drive down.
    #[ts(type = "number")]
    pub prefill_ms: u64,
    /// Lane wall-ms spent DECODING across this task's acts (actual generation).
    #[ts(type = "number")]
    pub decode_ms: u64,
}

/// Why a run could NOT produce a trustworthy score — the serving lane never held a
/// verified, decode-capable context for the whole exam. This is NOT "she failed": the
/// harness never gave her a working lane, so the accuracy signal is meaningless. A run
/// carrying this is reported as infra-unavailable, and `score`/`pass_rate`/`results`
/// MUST be read as void, never as a real "she scored 0" — the exact fake-zero the
/// Proctored Exam Session exists to make impossible.
/// [[proctored-exam-session-dependable-benchmark]] [[benchmark-needs-its-own-serving-lane]]
#[derive(Debug, Clone, Serialize, TS)]
// Exported for the same reason as `EvalTask` above: this struct is a field of the
// exported result (`infra_unavailable`), so ts-rs emits a binding on every build
// regardless — a derive without an export left it an UNTRACKED orphan that
// regenerated forever (audit card #424). Canonical protocol root, sibling of
// `cognition/EvalTask.ts`.
#[ts(export, export_to = "../../../protocol/typescript/cognition/InfraUnavailable.ts")]
pub struct InfraUnavailable {
    /// Human cause: which axis broke (not-ready / connect-refused / not-the-served-model
    /// / compute-error / stream-idle timeout), naming the task it broke on. Display-only
    /// — the classification that produced it is [`SettleOutcome::failure`], never
    /// re-parsed from this string.
    pub reason: String,
    /// How many tasks were attempted before the run aborted. The exam is INCOMPLETE — a
    /// non-zero `score` over `tasks_attempted` is still void (some attempts phantom-failed
    /// on a dead lane), which is why the presence of this whole struct — not the count —
    /// is the void flag.
    #[ts(type = "number")]
    pub tasks_attempted: u32,
    /// How many attempted tasks hit an UNRECOVERABLE infra fault (the lane never returned
    /// to decode-ready within the bounded re-verify+retry budget). ≥1 ⇒ the lane died
    /// mid-exam and continuing would only accrue more phantom zeros.
    #[ts(type = "number")]
    pub infra_faults: u32,
}

#[derive(Debug, Clone, Serialize, TS, Default)]
pub struct CognitionEvalResult {
    /// The run handle (#86): present on a detached ack AND on the ledger row, so the
    /// two halves of fire-and-poll join on one id.
    pub run_id: Option<String>,
    /// Who the run is about.
    ///
    /// STILL `String`, and deliberately so pending the next slice: this struct
    /// derives `Default` across 21 fields, and a persona reference has NO sensible
    /// default — an empty one is a nonsense value that would read as a real answer.
    /// The fix is to split the fire-and-poll HANDLE from the completed RESULT (they
    /// are two different things wearing one struct: a handle knows only the
    /// requested `PersonaRef`, a result knows the resolved `PeerId`), which is a
    /// bigger change than this one. Fabricating a default to make the type check
    /// would be the same reflex as `unwrap_or` — it makes the compiler quiet and the
    /// runtime wrong.
    pub persona_id: String,
    /// True = this is a fire-and-poll JOB HANDLE (#86), NOT a completed run: the eval was
    /// spawned detached and its real result is in the progress ledger, not in these fields
    /// (which are all zero on a handle). Poll `~/.continuum/progress/<persona_id>.jsonl`,
    /// filtering by the `note` you passed. False/absent = a normal, complete result.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub detached: bool,
    /// Tasks passed.
    #[ts(type = "number")]
    pub score: u32,
    /// Tasks attempted.
    #[ts(type = "number")]
    pub total: u32,
    /// `score / total` — THE number a change is measured against. In A/B mode
    /// (a `gene` was given) this is the CANDIDATE (gene-paged-in) pass-rate.
    pub pass_rate: f64,
    /// Fraction of tasks where she ACTED at least once (ran code / read / searched)
    /// before settling — i.e. she verified instead of asserting. Memory flags
    /// `acts=0` (answers from the model's head, never runs it) as the #1 trainable
    /// lever; this is that lever as a tracked number, climbing toward 1.0 as the
    /// verify reflex takes hold. Reality-anchored alongside `pass_rate`: a high
    /// pass_rate with a low self_verify_rate means she's guessing right, not knowing.
    pub self_verify_rate: f64,
    /// Mean per-task settle latency across the set (ms). The headline LATENCY number
    /// — the branch's namesake — reported next to `pass_rate` so accuracy and speed
    /// move on the same scoreboard, never one traded silently for the other.
    pub mean_latency_ms: f64,
    /// 95th-percentile per-task settle latency (ms). The tail a mean hides: a model
    /// that's fast-on-average but occasionally stalls shows it here.
    #[ts(type = "number")]
    pub p95_latency_ms: u64,
    /// Mean WALL-CLOCK decode throughput across the set (tokens/sec) — diluted by
    /// prefill + cognition overhead. Kept as the gross headline; `mean_decode_tps`
    /// is the lane's honest rate.
    pub mean_tokens_per_second: f64,
    /// Mean REAL decode throughput across the set (lane clock, undiluted by
    /// prefill). This is the true generation speed — the number to push UP. The gap
    /// between this and `mean_tokens_per_second` IS the prefill+overhead tax.
    pub mean_decode_tokens_per_second: f64,
    /// Mean KV-cache hit-rate across the set (`cached / total prompt tokens`). Low =
    /// the ~2000-token prompt is re-prefilled every act — the dominant Metal sink.
    /// The number to push toward 1.0 via prompt-order / cache-reuse levers.
    pub mean_cache_hit_rate: f64,
    /// Total lane wall-ms spent PREFILLING across the whole set vs `total_decode_ms`.
    /// Measured 77% of eval time was here, not decode — the headline "where the time
    /// goes" split that makes speed iterable instead of one conflated tok/s.
    #[ts(type = "number")]
    pub total_prefill_ms: u64,
    /// Total lane wall-ms spent DECODING across the whole set.
    #[ts(type = "number")]
    pub total_decode_ms: u64,
    /// Total completion tokens emitted across the whole set — the gross work done,
    /// for cost/throughput accounting against wall-clock.
    #[ts(type = "number")]
    pub total_output_tokens: u32,
    pub results: Vec<EvalTaskResult>,
    /// The gene measured (A/B mode only) — its name, echoed so a run is traceable.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub gene_id: Option<String>,
    /// The BASELINE pass-rate (base model, gene paged out) — A/B mode only. `score`/
    /// `pass_rate`/`results` above are the candidate arm; this is what it's measured
    /// against.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub base_pass_rate: Option<f64>,
    /// `pass_rate - base_pass_rate` — the LIFT the gene produced (A/B mode only).
    /// Positive = the gene made her a better coder; the measure→decide gate adopts
    /// only `lift > 0`. Negative = an overfit/regressing gene, correctly rejected.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub lift: Option<f64>,
    /// Which device the measurement lane ran on: `"gpu"` or `"cpu"` (A/B mode), or
    /// `"gpu (live persona lane)"` in single-pass mode. GPU-FIRST is the policy — a
    /// `"cpu"` here means the GPU was genuinely full, and `lane_placement_reason`
    /// says why. Surfaced so the harness never hides a ~10×-slower CPU run behind a
    /// quiet `mean_tokens_per_second` ([[optimization-is-always-first]]).
    pub lane_placement: String,
    /// Why that device was chosen (the GPU-first decision, in words).
    pub lane_placement_reason: String,
    /// Live free VRAM (bytes) at lane spawn, net of the resident living lane — the
    /// headroom the GPU-first decision saw. `None` when no GPU monitor / single-pass.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub lane_free_vram_bytes: Option<u64>,
    /// Estimated weight+scratch footprint (bytes) of the measurement lane's base —
    /// what GPU-first weighed against free VRAM. `None` when unsized / single-pass.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub lane_estimated_footprint_bytes: Option<u64>,
    /// Set ONLY when infra prevented a trustworthy measurement (the shared serving lane
    /// flipped not-ready / refused / compute-errored and never recovered within the
    /// re-verify+retry budget). When present, `score`/`pass_rate`/`results` are VOID —
    /// the run never completed on a verified lane, and reading `pass_rate: 0.0` as "she
    /// scored 0" is precisely the fake-zero this guards against. Absent = a real, scored
    /// run whose number can be trusted. Every human-facing render site (ledger row,
    /// eval-status, benchmark/run) branches on this before showing a pass-rate.
    /// [[proctored-exam-session-dependable-benchmark]]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub infra_unavailable: Option<InfraUnavailable>,
}

/// The gym command. Stateless: it reaches the persona's live cognition through the
/// global [`WorkspaceCycle`](super::persona_workspace) registry, so it needs no host
/// module state.
/// Detached evals in flight, registered at DISPATCH time — not at first grade.
/// The idempotence gap this closes (measured live, 2026-08-24, runs 4ae8e14c +
/// 912de04f): a round's first task can grade 30+ minutes after dispatch, and until
/// then the run has no ledger rows and no RunMeta scope — so `benchmark/round`
/// re-invoked in that window happily minted a DUPLICATE fresh run. Entries are
/// (run_id, persona, eval_set reference); removed when the detached body returns
/// (either arm), so a crashed core clears it by process death — never a stale file.
pub(super) static DETACHED_IN_FLIGHT: std::sync::Mutex<Vec<(String, String, String)>> =
    std::sync::Mutex::new(Vec::new());

/// Snapshot of the detached evals dispatched and not yet finished — the dream
/// consolidation's quiet gate reads it.
pub(crate) fn detached_evals_in_flight() -> Vec<(String, String, String)> {
    DETACHED_IN_FLIGHT
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default() // poisoned lock = a panicked writer; empty = gate opens, worst case a duplicate run (the pre-fix behavior), never a wedge
}

#[derive(Default)]
pub struct CognitionEval;

#[async_trait]
impl ActionCommand for CognitionEval {
    const NAME: &'static str = "cognition/eval";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Put a persona through a test-graded coder gym using her LIVE cognition (same model, \
         faculties, tools). Pass persona_id (must be spawned); tasks come from inline `tasks`, an \
         `eval_set` JSONL path, or the default coder-eval set. Returns a pass-rate — the objective \
         number for whether a change made her a better coder.";
    type Params = CognitionEvalParams;
    type Output = CognitionEvalResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: CognitionEvalParams,
    ) -> Result<CognitionEvalResult, CommandError> {
        // Fire-and-poll (#86): a long ACTING eval runs many minutes — far past any IPC client
        // timeout — so `detach` spawns it on the runtime (the body owns its params and reaches
        // cognition via the global workspace registry, needing neither `self` nor `ctx`),
        // returns a job handle NOW, and lets the real result land in the progress ledger. The
        // run survives the client disconnecting; the caller polls
        // `~/.continuum/progress/<persona>.jsonl` by `note`.
        if p.detach.unwrap_or(false) {
            let persona_id = p.persona_id.clone();
            let note = p.note.clone().unwrap_or_default();
            let run_id = p
                .run_id
                .clone()
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let mut inner = p.clone();
            inner.detach = Some(false);
            inner.run_id = Some(run_id.clone());
            // Own copies for the spawned closure so `persona_id`/`run_id` remain available
            // for the ack's return value below (they're `String`, not `Copy`).
            let ledger_persona = persona_id.clone();
            let ledger_run = run_id.clone();
            if let Ok(mut g) = DETACHED_IN_FLIGHT.lock() {
                g.push((
                    run_id.clone(),
                    persona_id.as_str().to_string(),
                    inner
                        .eval_set
                        .clone()
                        .unwrap_or_else(|| DEFAULT_EVAL_SET.to_string()), // same default the task-source block resolves — one identity
                ));
            }
            tokio::spawn(async move {
                let outcome = CognitionEval::run_eval_restoring(inner).await;
                if let Ok(mut g) = DETACHED_IN_FLIGHT.lock() {
                    g.retain(|(rid, _, _)| rid != &ledger_run);
                }
                match outcome {
                    Ok(r) => tracing::info!(
                        note = %note,
                        score = r.score,
                        total = r.total,
                        "cognition/eval detached run complete — result in progress ledger"
                    ),
                    Err(e) => {
                        tracing::error!(note = %note, error = %e, "cognition/eval detached run failed");
                        // Fail loud on the POLL SURFACE, not only the log. A detached run
                        // that dies before `append_progress_ledger` leaves eval-status
                        // returning `complete:false, row:null` forever — indistinguishable
                        // from "still starting", so the poller waits on a corpse (cost me
                        // two cycles staring at `total:null` while the real error — a
                        // post-reboot "no workspace template" fork race — sat in the log).
                        // Write a FAILED row keyed on the SAME run_id so the poller sees the
                        // error and can retry. [[fallbacks-are-illegal-fail-loud]]
                        append_failed_ledger(&ledger_persona, &ledger_run, &note, &e.to_string());
                    }
                }
            });
            return Ok(CognitionEvalResult {
                detached: true,
                persona_id: persona_id.to_string(),
                run_id: Some(run_id),
                ..Default::default()
            });
        }
        CognitionEval::run_eval_restoring(p).await
    }
}

impl CognitionEval {
    /// The ONE entry both launch modes (inline and detached) go through.
    ///
    /// It used to restore the persona's hands here, because a measurement fork borrowed the
    /// LIVING persona's executor, so the eval's rooting moved her live hands and outlived the
    /// eval (#312). A fork now acts with its own hands in its own acting context
    /// (`persona_workspace::give_fork_its_own_hands`, docs/architecture/HER-HANDS.md): the
    /// eval never moves her live hands, so there's nothing of hers to put back. The fork's
    /// context ends when the fork's hands drop, on every exit path. That also removes the
    /// case this boundary missed: a default tool-bearing eval rooted at an ephemeral clone
    /// with no `workspace_root`, which was never restored and left her live hands at a deleted
    /// directory.
    pub(super) async fn run_eval_restoring(
        p: CognitionEvalParams,
    ) -> Result<CognitionEvalResult, CommandError> {
        CognitionEval::run_eval(p).await
    }
}
