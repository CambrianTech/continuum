//! The `cognition/eval` command's wire contract: what a caller sends (`CognitionEvalParams`,
//! the `EvalGene` under test) and what it gets back (`CognitionEvalResult`, its per-task
//! `EvalTaskResult`, `InfraUnavailable`). Moved whole out of `cognition/eval.rs` (card
//! 9f0ba854) so the command's contract is one file its callers can read, apart from how the
//! eval runs.

use super::*;

/// A gene to page in for the candidate arm of an A/B. The persona runs the eval
/// once on the base model (gene paged OUT), then again with this gene paged IN;
/// the result carries both pass-rates and their `lift`. The gene must already be
/// REGISTERED with the serving endpoint (the adapter resolves `name`/`path` → the
/// server's LoRA load-index); continuum holds the handle, the custodian owns the
/// bytes ([[model-endpoint-fabric-adapter-router]]).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct EvalGene {
    /// The gene's name — how the serving endpoint catalogs it (`/lora-adapters`).
    pub name: String,
    /// The gene's on-disk path (the custodian-owned GGUF/safetensors LoRA). The
    /// adapter matches this against the registered catalog when `name` doesn't.
    #[serde(default)]
    pub path: String,
    /// Influence dial in [0,1+] — 0 = base, 1 = full gene. The page-in is analog:
    /// the per-request scale rides into `"lora":[{id,scale}]`. Defaults to 1.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub scale: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CognitionEvalParams {
    /// Which persona to put through the gym — a full UUID, an 8-char short-id, or a
    /// name. Resolved against the live roster before anything runs, so a garbage or
    /// unknown reference fails loud here instead of dying later as a misleading
    /// "not assembled at spawn". Must be spawned (have a live `WorkspaceCycle`) —
    /// the eval drives her real cognition, not a stand-in.
    pub persona_id: crate::identity::PersonaRef,
    /// Optional gene to MEASURE: when set, the eval runs base vs gene as an A/B and
    /// reports the `lift`. When omitted, a single pass on whatever genome is
    /// currently paged in (base, by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub gene: Option<EvalGene>,
    /// Room context the eval turns are scoped to. Omit for the nil room.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub room_id: Option<String>,
    /// Inline tasks. When set, takes precedence over `eval_set`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tasks: Option<Vec<EvalTask>>,
    /// Path to a JSONL eval set (one `EvalTask` per line). Defaults to the committed
    /// `docs/genome/coder-eval.jsonl` when neither this nor `tasks` is given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub eval_set: Option<String>,
    /// Run the A/B only on the tasks of the gym `eval_set` that the `gene`'s whole training
    /// lineage never trained on (`cognition::held_out::for_eval`): its job is found from the
    /// gene's own artifact, never supplied beside it. Refused, with the reason, when that
    /// cannot be known, when tasks are inline, or when `gene` or `eval_set` is missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub held_out: Option<bool>,
    /// Measure THIS base model through the full loop, in its OWN ephemeral lane, without
    /// re-homing or disturbing the living persona (#59). The clean same-model control:
    /// hold the harness fixed, vary the model. Ignored when a `gene` is set (a gene names
    /// its own forged base). None → fork onto her live lane as before. Must be a loadable
    /// id from `ai/inference/models`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base_model_id: Option<String>,
    /// Team mode: with `Some(n>=1)`, add a REVIEWER teammate (a fresh fork of the SAME
    /// persona/model) that reviews + corrects the writer's answer before grading — the
    /// undeniable team proof (same model, same tasks, +1 teammate → does coordination lift?).
    /// None/0 = solo. First slice supports one reviewer; live single-pass path only (no gene,
    /// no base_model_id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub reviewers: Option<u32>,
    /// Max act→observe cycles per task before it counts as unfinished. Default 32
    /// (see DEFAULT_MAX_ACTS); a task row's own max_acts overrides per task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub max_acts: Option<u32>,
    /// Agentic-recovery budget: how many times a FAILED test-graded task is handed its
    /// compiler/test output to fix before it scores a miss. Default [`MAX_FAIL_RETRIES`].
    /// Set `0` for the ONE-SHOT baseline (what plain inference / unsloth gets on the same
    /// weights) — so a `0` vs `N` A/B on the identical model+benchmark measures exactly the
    /// edge our agentic loop adds, repeatably, from one param.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub max_retries: Option<u32>,
    /// Decoding temperature pin for the WHOLE run. **Omitted → her LIVED
    /// sampling** (the engineerable form — Joel 2026-08-23: "the old form is
    /// useless"): greedy 0.0 measured pathological on long-context agentic
    /// work (16k-token think spirals hitting the reply ceiling exactly, 4+min
    /// decodes) while every serious agentic harness samples. Pass
    /// `Some(0.0)` to opt INTO the old byte-reproducible greedy pin. A run's
    /// number is only comparable to runs at the same pin either way.
    #[serde(default)]
    #[ts(optional)]
    pub temperature: Option<f32>,
    /// Free-text label for THIS run, written to the progress ledger so a trend
    /// line is readable: "baseline", "taught show-output", "genome v2", etc.
    /// The ledger is how you "mark improvement as you go" — every eval leaves a
    /// dated, labelled, test-anchored mark at `~/.continuum/progress/<id>.jsonl`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note: Option<String>,
    /// Fire-and-poll (#86): when true, the eval is spawned DETACHED — `run` returns a job
    /// handle immediately (so it survives the IPC client disconnecting on a long acting run)
    /// and the REAL result lands in the progress ledger (`~/.continuum/progress/<persona>.jsonl`),
    /// which the caller polls by `note`. Default/false = run inline and block as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detach: Option<bool>,
    /// Handle for a detached run (#86 fire-and-poll). Minted by the command when omitted;
    /// the progress-ledger row carries it, and `cognition/eval-status` polls by it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
    /// Repo-work seam (#49): pin the persona's file engine at this directory (e.g. a SWE-bench
    /// target-repo clone) BEFORE her cycle, by invoking `code/create-workspace` through HER OWN
    /// identity-bearing executor. Deterministic rooting — no reliance on the model choosing to
    /// call create-workspace itself. `None` → she uses the default root (core cwd) exactly as
    /// before, so this is opt-in with zero change to every existing eval path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub workspace_root: Option<String>,
    /// Glass-box seam (task #14): if set, wrap the eval-fork's cognition in the JSONL
    /// [`JsonlWorkspaceCaptureSink`] so every tick's bids + DECISION + timings land in
    /// `<capture_dir>/<persona_id>.jsonl` — makes a MEASURED run inspectable (did she Act or
    /// Respond? which tool? why 0 edits?) without touching her live mind. `None` → Noop capture
    /// exactly as before. Opt-in: zero change to every existing eval path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub capture_dir: Option<String>,
    /// LEARN mode (the exam as a legitimate teacher). When true, after the exam the
    /// redacted lesson of each task — "I was asked X; I solved it / did not (grade Y)" with
    /// the held-out answer key scrubbed — is admitted into the LIVING persona via
    /// `admit_reflection`, so she carries the *experience* forward and gets better across
    /// retakes WITHOUT ever memorizing the crib sheet. The exam still runs on the fork
    /// (#59: living persona never frozen/degraded); only the redacted lesson crosses back.
    /// This is what makes "learn from the exam" honest — provably clean, encouraged. Default
    /// false = pure measurement (the discarded fork teaches nothing, as before). Single-pass
    /// only in this slice (ignored under a `gene` A/B). [[redaction-makes-exam-learning-honest-so-encourage-it]]
    /// No `Default` on [`LearningPolicy`] — every Rust caller must state which this run is.
    #[serde(default = "LearningPolicy::wire_default")]
    #[ts(type = "boolean", optional)]
    pub learn: LearningPolicy,
    /// REPRODUCIBLE-ABSOLUTE mode (#207): suppress the fork's episodic recall so a
    /// self-contained proctored exam scores the SAME absolute number run-to-run. Each eval
    /// re-forks from her LIVING durable engram store, which grows as she lives between runs;
    /// injecting it into a self-contained task (HumanEval, a repo-nav question, a from-scratch
    /// UI build) recalls unrelated room chatter that drifts the prompt — noise AND
    /// nondeterminism, unrelated to sampling (greedy is already pinned) or serving. `true`
    /// omits the recall faculty from the fork; system + task + grounding (roster/doctrine/
    /// workspace-map) remain. The LIFT (base vs gene in one fork) is reproducible either way;
    /// this pins the ABSOLUTE baseline so today's number compares to last week's. `None`/false =
    /// memories intact (the natural persona), unchanged for every existing path. NOT a life
    /// knob — a benchmark control, sibling of the greedy-temperature and directed-turn pins.
    /// [[eval-reproducibility-is-two-tier-lift-controlled-absolute-drifts]]
    #[serde(
        default,
        alias = "suppressRecall",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub suppress_recall: Option<bool>,
    /// HELP ARM (rung one of room collaboration, 2026-08-24): when true and the
    /// eval is room-scoped, the exam condition DECLARES that asking peers is
    /// legal — the task prompts say so, and a listener folds live room speech
    /// from OTHERS into the fork's working memory so she perceives replies
    /// between acts. Exchanges are receipts in the transcript; score under this
    /// arm is reported as OURS+help, never conflated with solo. Default off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub help: Option<bool>,
}

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
    /// — the classification that produced it is [`SettleOutcome::inference_error`], never
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
