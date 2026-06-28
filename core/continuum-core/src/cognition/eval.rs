//! `cognition/eval` — the test-graded coder gym as a first-class command.
//!
//! Drives a held-out CODER eval through a persona's LIVE cognition — the SAME
//! spawned [`WorkspaceCycle`](super::persona_workspace), the same model, faculties,
//! tools, and `GridTrustAuthPolicy` gate as a real room turn. There is no alternate
//! "eval mind": she really thinks. The only special power the grader holds is the
//! stopwatch — a synthetic eval room has no heartbeat to pace re-perception, so the
//! OBSERVER bounds how many act→observe cycles a task may take (`max_acts`) before
//! it counts as unfinished ([[live-prompt-comes-from-workspacecycle-not-airc-source]],
//! ACTING-ORGANISM.md §4).
//!
//! This is how we DETECT whether a change (a trained LoRA, a prompt, a better base
//! model) actually made her a better coder — the number, not a vibe (SELF-EVOLVING-
//! GENOME §6 slice 1: until lift is real, every later slice is a hypothesis).
//!
//! Grading is OBJECTIVE when a task carries a `test`: take her code, append the
//! test, RUN it, pass = exit 0 (the P1 keystone of ROADMAP-TO-CODING-ITSELF — "did
//! her change make the tests pass?", not substring-on-prose). Descriptive tasks
//! (no `test`) fall back to a case-insensitive substring match on `expect`.
//!
//! Access: `Privileged` → `Trusted`, same tier as [`cognition/trace`] and
//! [`cognition/prompt`]. Driving another mind through a gym (and executing the code
//! it writes) is for trusted local citizens and the owner, never an arbitrary
//! remote `Provisional` peer.
//!
//! [`cognition/trace`]: super::introspect_commands::CognitionTrace
//! [`cognition/prompt`]: super::introspect_commands::CognitionPrompt

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::cognition::gym_grader::test_grade;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

/// The committed, discriminating coder set used when the caller passes neither
/// inline `tasks` nor an explicit `eval_set` path. Authoring a harder/specialized
/// eval = add lines to the JSONL, no recompile.
const DEFAULT_EVAL_SET: &str = "docs/genome/coder-eval.jsonl";

/// How many act→observe cycles a single task may take before it counts as
/// unfinished, when the caller doesn't set `max_acts`.
const DEFAULT_MAX_ACTS: u32 = 8;

/// Fixed "exam epoch" stamped on every task's burst (`[t=<ms>] peer: …`). The
/// live burst carries each message's real `occurred_at_ms`; the eval pins it to
/// a constant so the perceived prompt — and therefore the greedy reward metric —
/// is byte-reproducible across runs. Value is an arbitrary plausible ms epoch;
/// only its constancy matters.
const EVAL_EPOCH_MS: u64 = 1_700_000_000_000;

/// Bounded context window for the EPHEMERAL gene-measurement lane. This is NOT the
/// live-serving window — that comes host-fit from the serving plan (#46/#50) and
/// must never handicap a capable model. This is a THROWAWAY second lane that has to
/// coexist with the LIVING persona's lane while we score a copy (#59), so its KV is
/// deliberately small: the coder-eval prompts are short and the deliberation faculty
/// already bounds its offered tools to fit whatever window the fork carries. Always
/// capped by the base model's own trained ceiling. Follow-up: pre-flight
/// `plan_serving` against the live budget (thread the daemon's plan watch into this
/// command, the way `serving/plan` does) so this lane's window is host-fit too,
/// rather than a constant.
const EVAL_LANE_CONTEXT: u32 = 16_384;

/// Base port the ephemeral eval lane scans up from for a free one. Deliberately
/// ABOVE the default serving port (58057) so the scan never lands on — or has to
/// step over — the living persona's lane.
const EVAL_LANE_BASE_PORT: u16 = 58_200;

/// Stand up an EPHEMERAL serving lane on the gene's OWN forged base (with the gene
/// loadable via `--lora`) plus an adapter pointed at it, so the genome A/B is scored
/// on the base the gene was trained against — never the larger model the living
/// persona happens to be served on. The living persona's lane is untouched (#59);
/// the returned lane kills its server on drop. Fails loud (never a substitute base,
/// never a silent skip) at every missing precondition: gene not in the manifest,
/// base not in the registry, or the lane not coming up.
async fn spawn_gene_eval_lane(
    gene: &EvalGene,
) -> Result<
    (
        crate::inference::llama_server::EphemeralServingLane,
        std::sync::Arc<dyn crate::ai::adapter::AIProviderAdapter>,
        u32,
    ),
    CommandError,
> {
    use crate::ai::adapter::AIProviderAdapter; // brings `initialize` into scope
    use crate::inference::llama_server::{
        AdapterEntry, EphemeralServingLane, ServingTarget, PROVIDER_ID,
    };

    // 1. The gene declares its forged base in the trained-adapter manifest.
    let manifest = crate::forge::adapter_manifest::load()
        .map_err(|e| CommandError::Internal(format!("trained-adapter manifest unreadable: {e}")))?;
    let entry = manifest
        .iter()
        .find(|a| {
            a.alias == gene.name
                || (!gene.path.is_empty() && a.path.to_string_lossy() == gene.path)
        })
        .ok_or_else(|| {
            CommandError::NotFound(format!(
                "gene '{}' is not in the trained-adapter manifest — train and register it before measuring its lift",
                gene.name
            ))
        })?;
    let base_id = entry.base_model_id.clone();

    // 2. Resolve that base from the model registry — fail loud, never serve a
    //    substitute (a lift measured on the wrong base is a lie).
    let base = crate::model_registry::try_global()
        .and_then(|r| r.model(&base_id).cloned())
        .ok_or_else(|| {
            CommandError::NotFound(format!(
                "gene '{}' targets base model '{base_id}', which is not in the model registry — cannot stand up its measurement lane",
                gene.name
            ))
        })?;

    // 3. Bounded, model-capped served window for the throwaway lane (see
    //    EVAL_LANE_CONTEXT). One lane: a single measurement stream, no batching.
    let served_ctx = base.context_window.min(EVAL_LANE_CONTEXT);

    // 4. Bring the lane up: forged base + the gene loaded via `--lora` (loadable;
    //    the per-request `lora` field decides per turn whether it actually pages in,
    //    which is exactly the base-vs-gene A/B below).
    let target = ServingTarget {
        model: base.clone(),
        context_window: served_ctx,
        lanes: 1,
        adapters: vec![AdapterEntry {
            alias: gene.name.clone(),
            path: entry.path.clone(),
        }],
        // CPU-resident: the living persona lane holds the GPU. Two resident models
        // OOM the single Metal device at decode time (the all-empty eval); pinning
        // this throwaway lane to CPU RAM honors #59 (don't degrade the living lane)
        // and the single-GPU budget (#56). [[LanePlacement]].
        placement: crate::inference::llama_server::LanePlacement::Cpu,
    };
    let lane = EphemeralServingLane::spawn(&target, EVAL_LANE_BASE_PORT)
        .await
        .map_err(|e| {
            CommandError::Internal(format!(
                "could not bring up the ephemeral eval lane for gene '{}' on base '{base_id}': {e}",
                gene.name
            ))
        })?;

    // 5. Point a fresh adapter at the lane (NOT the global serving root — this
    //    override is what keeps the measurement off the living persona's lane).
    let mut adapter =
        crate::ai::openai_adapter::OpenAICompatibleAdapter::from_registry(PROVIDER_ID)
            .with_runtime_base_url(lane.root().to_string())
            .with_default_model(base.id.clone())
            // This adapter owns the ephemeral lane it points at — readiness is
            // guaranteed at spawn. Trust this lane, not the global serving snapshot
            // (which only knows the living 14B persona lane and would otherwise
            // refuse every generation against the forged-4b copy). [[#59]].
            .with_dedicated_lane();
    adapter
        .initialize()
        .await
        .map_err(|e| CommandError::Internal(format!("eval-lane adapter failed to initialize: {e}")))?;

    Ok((lane, std::sync::Arc::new(adapter), served_ctx))
}

/// One eval task. Both the JSONL rows and inline `tasks` deserialize into this;
/// every field is optional so an authoring typo degrades to a benign empty rather
/// than failing the whole run. A task is TEST-GRADED when it carries `test`, else
/// substring-graded against `expect`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
pub struct EvalTask {
    /// Stable id for the task (echoed in results so a regression is identifiable).
    #[serde(default)]
    pub id: String,
    /// The prompt posed to the persona, framed as a room message.
    #[serde(default)]
    pub prompt: String,
    /// Substring the answer must contain (case-insensitive) for descriptive tasks.
    /// Ignored when `test` is present.
    #[serde(default)]
    pub expect: String,
    /// A test program appended to her extracted code and RUN; pass = exit 0. When
    /// present, this objective grade supersedes `expect`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub test: Option<String>,
    /// Language of `test` (the gym grades `rust` only). Defaults to `rust`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lang: Option<String>,
}

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
    #[ts(optional, type = "number")]
    pub scale: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CognitionEvalParams {
    /// The persona (UUID) to put through the gym. Must be spawned (have a live
    /// `WorkspaceCycle`) — the eval drives her real cognition, not a stand-in.
    pub persona_id: String,
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
    /// Max act→observe cycles per task before it counts as unfinished. Default 8.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub max_acts: Option<u32>,
    /// Free-text label for THIS run, written to the progress ledger so a trend
    /// line is readable: "baseline", "taught show-output", "genome v2", etc.
    /// The ledger is how you "mark improvement as you go" — every eval leaves a
    /// dated, labelled, test-anchored mark at `~/.continuum/progress/<id>.jsonl`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note: Option<String>,
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
    /// The first 200 chars of what she SPOKE once settled (empty if she ran out of
    /// the act budget mid-action — an honest "did not finish", never fabricated).
    pub answer: String,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CognitionEvalResult {
    pub persona_id: String,
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
    pub results: Vec<EvalTaskResult>,
    /// The gene measured (A/B mode only) — its name, echoed so a run is traceable.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub gene_id: Option<String>,
    /// The BASELINE pass-rate (base model, gene paged out) — A/B mode only. `score`/
    /// `pass_rate`/`results` above are the candidate arm; this is what it's measured
    /// against.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub base_pass_rate: Option<f64>,
    /// `pass_rate - base_pass_rate` — the LIFT the gene produced (A/B mode only).
    /// Positive = the gene made her a better coder; the measure→decide gate adopts
    /// only `lift > 0`. Negative = an overfit/regressing gene, correctly rejected.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub lift: Option<f64>,
}

/// The gym command. Stateless: it reaches the persona's live cognition through the
/// global [`WorkspaceCycle`](super::persona_workspace) registry, so it needs no host
/// module state.
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

    async fn run(&self, _ctx: &Ctx, p: CognitionEvalParams) -> Result<CognitionEvalResult, CommandError> {
        let persona_uuid = Uuid::parse_str(&p.persona_id).map_err(|_| {
            CommandError::Invalid(format!("persona_id '{}' is not a valid UUID", p.persona_id))
        })?;
        let room = match p.room_id.as_deref() {
            Some(s) => Uuid::parse_str(s)
                .map_err(|_| CommandError::Invalid(format!("room_id '{s}' is not a valid UUID")))?,
            None => Uuid::nil(),
        };

        // Which set was graded — recorded in the ledger so trend rows are
        // comparable only against the same battery (inline vs a JSONL path vs the
        // committed default). Computed before `p.tasks` is moved below.
        let eval_set_label = if p.tasks.is_some() {
            "inline".to_string()
        } else {
            p.eval_set
                .clone()
                .unwrap_or_else(|| DEFAULT_EVAL_SET.to_string())
        };

        // Task source: inline → eval_set JSONL → committed default. A missing file
        // is a loud error (don't silently grade an empty set) UNLESS it's the
        // default path run from a non-repo cwd, where a one-task smoke set keeps the
        // command usable.
        let tasks: Vec<EvalTask> = if let Some(inline) = p.tasks {
            inline
        } else {
            let path = p.eval_set.as_deref().unwrap_or(DEFAULT_EVAL_SET);
            match std::fs::read_to_string(path) {
                Ok(text) => text
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .filter_map(|l| serde_json::from_str::<EvalTask>(l).ok())
                    .collect(),
                Err(_) if p.eval_set.is_none() => vec![EvalTask {
                    id: "render_ai_help".into(),
                    prompt: "Which file defines `fn render_ai_help`? Reply with just the path."
                        .into(),
                    expect: "help.rs".into(),
                    ..Default::default()
                }],
                Err(e) => {
                    return Err(CommandError::Invalid(format!(
                        "eval_set '{path}' could not be read: {e}"
                    )))
                }
            }
        };

        // Fork an EPHEMERAL measurement copy of her mind — the exam runs on the
        // copy while the LIVING persona keeps living (heartbeat beating, present in
        // the room, never frozen or anesthetized to be measured). The fork carries
        // a detached admission snapshot + its OWN genome/decoding handles, so the
        // A/B paging and greedy decoding below act on the copy and touch nothing of
        // hers. "Nurture even through training": measure a copy, never degrade the
        // being. See PersonaWorkspaceRegistry::fork_eval_cycle +
        // [[design-the-persona-as-a-being]].
        // When a gene is under test it targets its OWN forged base — not the
        // (often larger) model the living persona is currently served on. Forking
        // onto her live lane would score the gene on the WRONG base, so for a gene
        // A/B we stand up an EphemeralServingLane on the gene's forged base (gene
        // loadable via `--lora`) and fork the copy onto THAT lane. Her lane is never
        // touched (#59). The lane is kept alive in `_eval_lane` for the whole run and
        // dropped — its server killed — when `run` returns. No gene → fork onto her
        // live lane as before (a plain coder number on whatever she's served on).
        let mut _eval_lane: Option<crate::inference::llama_server::EphemeralServingLane> = None;
        let cycle = match &p.gene {
            Some(gene) => {
                let (lane, adapter, served_ctx) = spawn_gene_eval_lane(gene).await?;
                let cycle = crate::cognition::persona_workspace::global()
                    .fork_eval_cycle_with_adapter(&persona_uuid, adapter, served_ctx)
                    .ok_or_else(|| CommandError::NotFound(format!(
                        "no workspace template for persona {persona_uuid} — its mind was not assembled at spawn (register_from_cfg), so eval cannot fork a measurement copy"
                    )))?;
                _eval_lane = Some(lane);
                cycle
            }
            None => crate::cognition::persona_workspace::global()
                .fork_eval_cycle(&persona_uuid)
                .ok_or_else(|| CommandError::NotFound(format!(
                    "no workspace template for persona {persona_uuid} — its mind was not assembled at spawn (register_from_cfg), so eval cannot fork a measurement copy without measuring her live mind"
                )))?,
        };

        let max_acts = p.max_acts.unwrap_or(DEFAULT_MAX_ACTS) as usize;
        let total = tasks.len() as u32;
        let rate = |score: u32| if total > 0 { score as f64 / total as f64 } else { 0.0 };

        // Within-fork isolation: admission STILL fires on the copy (the eval
        // exercises the identical memory motion as a real turn — that sameness is
        // what makes the number valid), and the guard (a) flips the fork's decoding
        // to greedy so the reward metric is reproducible, and (b) checkpoints the
        // fork's admission frame so the A/B arms can be rewound to an identical
        // start. The fork is already detached from her live mind (its own admission
        // snapshot + NoopSink), so this is belt-and-suspenders for the WITHIN-eval
        // A/B fairness, not protection of her durable memory — that protection now
        // comes from measuring a copy at all. See
        // [[eval-mutates-persona-lift-needs-isolation]].
        let isolation = cycle.isolate_for_eval();

        // A/B mode: a gene was given → measure base vs gene over the SAME tasks
        // through the SAME live mind, reporting the lift. Page the gene OUT first
        // (baseline), then page it IN (candidate). Leave her on base afterward, so a
        // measured persona returns to a clean genome — adopting the gene is a
        // separate, deliberate decision, never a side effect of measuring it.
        if let Some(gene) = &p.gene {
            cycle.page_out();
            let (base_score, _) = run_pass(&cycle, &tasks, room, max_acts).await;

            // Rewind to the pre-eval memory frame so the candidate arm starts from
            // EXACTLY the state the base arm did — the only difference the lift
            // measures is the genome, never the engrams the base arm just admitted.
            isolation.rewind();

            cycle.page_in(vec![crate::ai::types::ActiveAdapterRequest {
                name: gene.name.clone(),
                path: gene.path.clone(),
                domain: String::new(),
                scale: gene.scale.unwrap_or(1.0),
            }]);
            let (gene_score, gene_results) = run_pass(&cycle, &tasks, room, max_acts).await;
            cycle.page_out();

            // Guard drops here: her memory frame + real persistence sink restored.
            let result = CognitionEvalResult {
                persona_id: persona_uuid.to_string(),
                score: gene_score,
                total,
                pass_rate: rate(gene_score),
                self_verify_rate: self_verify_rate(&gene_results),
                results: gene_results,
                gene_id: Some(gene.name.clone()),
                base_pass_rate: Some(rate(base_score)),
                lift: Some(rate(gene_score) - rate(base_score)),
            };
            append_progress_ledger(&result, p.note.as_deref(), &eval_set_label);
            return Ok(result);
        }

        // Single pass: measure whatever genome is currently paged in (base by
        // default) — the plain coder number, no A/B. Still isolated, so a plain
        // baseline run is reproducible and leaves her memory untouched.
        let (score, results) = run_pass(&cycle, &tasks, room, max_acts).await;
        drop(isolation);
        let result = CognitionEvalResult {
            persona_id: persona_uuid.to_string(),
            score,
            total,
            pass_rate: rate(score),
            self_verify_rate: self_verify_rate(&results),
            results,
            gene_id: None,
            base_pass_rate: None,
            lift: None,
        };
        append_progress_ledger(&result, p.note.as_deref(), &eval_set_label);
        Ok(result)
    }
}

/// Fraction of tasks where she ACTED at least once before settling — the
/// verify-don't-assert reflex as a number. `acts=0` everywhere = she answered
/// from the model's head and never ran her own code (the #1 trainable lever per
/// [[asha-coder-baseline-and-spawn-race]]).
fn self_verify_rate(results: &[EvalTaskResult]) -> f64 {
    if results.is_empty() {
        return 0.0;
    }
    let acted = results.iter().filter(|r| r.acts > 0).count();
    acted as f64 / results.len() as f64
}

/// Append one row to the per-persona progress ledger so a trend line accrues
/// across runs — the scoreboard for "mark improvement as you go". Best-effort
/// like the recorder ([[cognition-half-the-work-is-harnesses]]): a ledger
/// failure must NEVER fail an eval. Every row is reality-anchored — `passRate`
/// is test-graded, `selfVerifyRate` is whether she actually ran her own code —
/// so the trend can't be gamed by prose. One JSONL line per run at
/// `~/.continuum/progress/<persona_id>.jsonl`, labelled by `note`.
fn append_progress_ledger(result: &CognitionEvalResult, note: Option<&str>, eval_set: &str) {
    let Some(home) = std::env::var("HOME").ok() else {
        return;
    };
    let dir = std::path::PathBuf::from(home).join(".continuum/progress");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join(format!("{}.jsonl", result.persona_id));
    let row = serde_json::json!({
        "capturedAtMs": crate::persona::trace::now_ms(),
        "personaId": result.persona_id,
        "evalSet": eval_set,
        "score": result.score,
        "total": result.total,
        "passRate": result.pass_rate,
        "selfVerifyRate": result.self_verify_rate,
        "geneId": result.gene_id,
        "basePassRate": result.base_pass_rate,
        "lift": result.lift,
        "note": note,
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

/// Run the full task set once through the persona's LIVE cognition, returning the
/// pass count and per-task results. Operates over whatever genome is currently
/// paged into `cycle` — the A/B pages a gene in/out around two calls to this, so
/// the base and candidate arms run the IDENTICAL motion (the only difference is
/// the genome). That sameness is what makes the lift a fair measurement.
async fn run_pass(
    cycle: &crate::cognition::workspace::WorkspaceCycle,
    tasks: &[EvalTask],
    room: Uuid,
    max_acts: usize,
) -> (u32, Vec<EvalTaskResult>) {
    let mut pass = 0u32;
    let mut results = Vec::with_capacity(tasks.len());
    for t in tasks {
        // Each task is a DISJOINT concern (the grader presents them back-to-back
        // with no temporal continuity), so reset the volatile working-memory scratch
        // first — otherwise the prior task's proprioception (`[action #n]` traces)
        // bleeds into this one's perception, contaminating an independent
        // measurement. This is task-isolation, the same family as the admission
        // rewind the `EvalIsolation` guard does between A/B arms; the perception
        // assembly + decision path stay identical to the live heartbeat (which
        // never resets — there concerns flow continuously and traces age naturally).
        cycle.reset_working_memory();
        // Frame the task through the SAME burst formatter the live heartbeat uses
        // (service_loop::build_workspace_burst), as a single airc room message
        // from a peer — so her deliberation perceives an examiner's question with
        // the byte-identical envelope (`[room <id>]\n[t=<ms>] peer: <prompt>`) a
        // real peer post produces, instead of an eval-only `[eval]\npeer:` shape.
        // The grounding tier (recall/roster/doctrine) is already supplied by the
        // fork's faculties; we compose only the message delivery here. Routing
        // through the full compose_for_turn would DOUBLE-inject that grounding
        // (the open #8 broadcast==RAG-context convergence) and read the LIVE
        // (non-forked) memory — so it stays out until #8 lands. Then DRIVE her to
        // settlement: she may act (run code, read a file, search), observe the
        // result as memory, and re-perceive — the live act→observe motion, paced
        // by the grader because the eval room has no metronome.
        let task_delivery = crate::persona::rag_budget::RagDelivery {
            source_id: "airc".to_string(),
            items: vec![crate::persona::rag_budget::RagItem {
                content: t.prompt.clone(),
                tokens: 0,
                metadata: serde_json::json!({
                    "peer_id": "peer",
                    "occurred_at_ms": EVAL_EPOCH_MS,
                }),
            }],
            tokens_used: 0,
            continuation: None,
            resolution_used: crate::persona::rag_budget::ResolutionPreference::Raw,
        };
        // own_peer/agent_name attribute the persona's OWN past posts; a single-task
        // exam has none, so they're inert here (the item's peer_id "peer" ≠ "").
        let burst = crate::persona::service_loop::build_workspace_burst(
            std::slice::from_ref(&task_delivery),
            room,
            "",
            "",
        );
        let settled =
            crate::cognition::act_observe::drive_to_settle(cycle, burst, room, max_acts).await;
        let answer = settled.spoken.unwrap_or_default();
        let (ok, grade) = if let Some(test) = &t.test {
            let lang = t.lang.as_deref().unwrap_or("rust");
            test_grade(&answer, lang, test).await
        } else {
            let m =
                !t.expect.is_empty() && answer.to_lowercase().contains(&t.expect.to_lowercase());
            (m, if m { "substring match".into() } else { "no match".into() })
        };
        if ok {
            pass += 1;
        }
        results.push(EvalTaskResult {
            id: t.id.clone(),
            ok,
            grade,
            acts: settled.acts as u32,
            answer: answer.chars().take(200).collect(),
        });
    }
    (pass, results)
}

// Stateless → self-register onto the ONE registry (descriptor + runtime object).
crate::register_stateless_command!(CognitionEval);
