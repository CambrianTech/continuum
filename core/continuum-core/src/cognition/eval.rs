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
use crate::inference::llama_server::LanePlacement;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

/// The committed, discriminating coder set used when the caller passes neither
/// inline `tasks` nor an explicit `eval_set` path. Resolved (not read raw)
/// through [`super::gym::resolve_gym`], so the default — like every committed
/// gym — comes from the embedded registry, CWD- and deployment-independent. A
/// *custom* set still uses `--eval_set <path>` (an existing on-disk file wins);
/// only changing a *committed* gym needs a rebuild — the right trade, since the
/// committed gyms must be reliable. The whole class of "core launched from a
/// different cwd → file-not-found → silent degrade" is killed by going through
/// the resolver instead of `std::fs::read_to_string`.
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

/// Host-fit PHYSICAL launch window (`-c`) for the EPHEMERAL measurement lane —
/// derived from the LIVE serving budget through the SAME [`plan_serving`] classifier
/// the autonomic serving daemon uses, NEVER a baked constant (task #124; the old
/// `EVAL_LANE_CONTEXT: u32 = 16_384` was the exact "clamp a 128k window to a magic
/// number → the model is starved" anti-pattern [[no-hardcoded-context-numbers-derive-from-the-live-window]]).
///
/// The fork's COGNITION window is still read back from the lane's real `/props` after
/// spawn ([`EphemeralServingLane::served_context_window`]) — the SAME served-truth pin
/// the supervisor applies to the living persona, so a measurement copy budgets against
/// exactly what its lane serves ([[dreaming-mind-eval-must-match-live-cognition]], task
/// #50). This value only sizes the launch `-c` + KV allocation. Because it now comes
/// from `plan_serving` against live FREE VRAM (already net of the resident living lane,
/// the same figure [`decide_eval_lane_placement`] reads), the throwaway lane's geometry
/// mirrors what production would serve THIS base on THIS host RIGHT NOW: roomy host →
/// big window up to the model's trained ceiling; contended host → honestly smaller;
/// always floored at [`MIN_SERVE_CTX`](crate::cognition::serving_plan::MIN_SERVE_CTX)
/// and capped at the base's own trained window.
///
/// Degrades honestly (never a fresh magic cap): no GPU monitor on the node (CPU-only
/// host) or a base whose GGUF can't be sized → fall back to the model's own trained
/// window. Short coder-eval prompts dominate, so an over-large `-c` on a CPU host costs
/// KV allocation but never starves cognition; the point is to stop clamping a capable
/// GPU host's window to a constant below what it could serve. Remaining edge: when
/// placement spills this lane to CPU, the window is still sized off the GPU free-VRAM
/// budget (under-sizes for the rare CPU-spilled eval) — acceptable for short prompts;
/// the two-phase device-aware sizing rides with #56's `ResourceGovernor`.
fn plan_eval_lane_ctx(base: &crate::model_registry::Model) -> u32 {
    use crate::cognition::serving_plan::{plan_serving, MIN_SERVE_CTX};
    use crate::modules::serving_daemon::{footprint_for, host_budget_from, perf_cores, HostBudgetInputs};

    let plan = match (crate::gpu::monitor::detect(), footprint_for(base)) {
        (Some(mon), Some(footprint)) => {
            // Live free VRAM (net of the resident living lane) against physical VRAM,
            // scaled by the serving headroom fraction — the coexistence-safe budget.
            // One measurement stream, no batching → demand_lanes = 1.
            let budget = host_budget_from(&HostBudgetInputs {
                available_bytes: mon.free_bytes(),
                total_vram_bytes: mon.total_bytes(),
                perf_cores: perf_cores(),
            });
            plan_serving(budget, std::slice::from_ref(&footprint), 1)
        }
        // CPU-only host or unsizable base: the model's own trained window, floored —
        // never a fresh invented cap.
        _ => None,
    };
    plan.map(|p| p.served_context_window)
        .unwrap_or_else(|| base.context_window.max(MIN_SERVE_CTX))
}

/// A stood-up ephemeral measurement lane plus everything the eval loop needs to fork
/// a cognition copy onto it. Named fields, NOT a positional 4-tuple, so a new piece of
/// lane state threads as ONE field instead of a fifth positional slot every caller
/// must re-destructure in the right order ([[structs-by-reference-not-massive-param-lists]]).
struct EvalLane {
    /// The throwaway server; kills its process on drop (#59).
    lane: crate::inference::llama_server::EphemeralServingLane,
    /// Adapter pinned to THIS lane (never the global serving root).
    adapter: std::sync::Arc<dyn crate::ai::adapter::AIProviderAdapter>,
    /// The lane's REAL served `/props` window — what the fork's cognition budgets against.
    served_ctx: u32,
    /// Where + why the lane landed (GPU/CPU), surfaced on the eval result.
    placement: PlacementEvidence,
}

/// Base port the ephemeral eval lane scans up from for a free one. Deliberately
/// ABOVE the default serving port (58057) so the scan never lands on — or has to
/// step over — the living persona's lane.
const EVAL_LANE_BASE_PORT: u16 = 58_200;

/// Small headroom (bytes) kept free on the GPU so a lane placed right at the edge
/// can't trip Metal's decode-time command-buffer OOM. Deliberately SMALL: the
/// policy is GPU-FIRST — fill the accelerator, aim for ~100% GPU utilization
/// ([[optimization-is-always-first]]; Joel: "near 100% GPU utilization if you're
/// doing it right. Fill in GPU lanes first"). This keeps us off the exact cliff;
/// it is NOT a conservative reserve that idles the GPU.
const GPU_PLACEMENT_MARGIN_BYTES: u64 = 256 * 1024 * 1024;

/// Where a coexisting eval lane runs + WHY. Rides out on the eval result so a CPU
/// placement is VISIBLE in the harness (a CPU-pinned lane is ~10× slower; surfacing
/// the device + the headroom numbers means a slow run is explained by data, never a
/// silent degrade). The benchmark self-reports which device it measured on.
#[derive(Debug, Clone)]
struct PlacementEvidence {
    placement: LanePlacement,
    device: String,
    reason: String,
    free_vram_bytes: Option<u64>,
    footprint_bytes: Option<u64>,
}

/// GPU-FIRST placement policy: pack the accelerator, spill to CPU ONLY when the GPU
/// genuinely can't hold this lane alongside what's already resident. Pure (no I/O)
/// so the policy is unit-testable; the caller supplies the live `free` VRAM (from
/// the GPU monitor — already net of the living persona's resident lane) and this
/// lane's estimated `footprint`.
///
/// - no GPU on the node (`free=None`)   → CPU is the only device (the honest truth,
///   not a fallback — there is nothing to fall back FROM)
/// - footprint unknown (`free=Some`)    → GPU (optimistic: pack it; llama.cpp
///   offloads what fits — never idle the accelerator on a sizing miss)
/// - footprint fits in free (− margin)  → GPU
/// - footprint exceeds free             → CPU spill (the GPU is full; SAY so, loud)
///
/// The seam #56's `ResourceGovernor` will later own this — and extend it to PARTIAL
/// `--n-gpu-layers N` offload (fill the GPU to the brim, spill only the residue to
/// CPU) for true ~100% utilization. Today's enum is binary GPU/CPU; this is the
/// GPU-first bias on that binary.
fn choose_lane_placement(
    free: Option<u64>,
    footprint: Option<u64>,
) -> (LanePlacement, &'static str) {
    match (free, footprint) {
        (None, _) => (
            LanePlacement::Cpu,
            "no GPU backend on this node — CPU is the only device",
        ),
        (Some(_), None) => (
            LanePlacement::Gpu,
            "GPU-first: lane footprint unknown, offloading to GPU",
        ),
        (Some(f), Some(fp)) if f >= fp.saturating_add(GPU_PLACEMENT_MARGIN_BYTES) => (
            LanePlacement::Gpu,
            "GPU-first: lane fits in free VRAM alongside resident lanes",
        ),
        (Some(_), Some(_)) => (
            LanePlacement::Cpu,
            "GPU full: free VRAM below lane footprint + margin — spilling this lane to CPU",
        ),
    }
}

/// GPU-FIRST placement decision for the coexisting eval lane: probe live free VRAM
/// (net of the living persona's resident lane) + size this base from its GGUF on
/// disk, then [`choose_lane_placement`]. Emits the decision into the placement glass
/// box (`~/.continuum/fixtures/placement-decisions/decisions.jsonl`) and packages the
/// evidence for the harness. `context` labels what asked for the lane; `served_ctx`
/// is the KV-sizing window — both land in the structured log.
fn decide_eval_lane_placement(
    base: &crate::model_registry::Model,
    served_ctx: u32,
    context: &str,
) -> PlacementEvidence {
    // Q4_K_M GGUF file bytes ≈ resident weight bytes; ×1.25 covers KV cache +
    // scratch at the bounded eval ctx. None → couldn't size (treated optimistically
    // as GPU-first below).
    let footprint = crate::model_registry::artifacts::resolve_gguf_for_model(base)
        .and_then(|p| std::fs::metadata(&p).ok())
        .map(|m| (m.len() as f64 * 1.25) as u64);
    // Live "what's free RIGHT NOW" — already accounts for the resident living lane.
    // None → no GPU monitor on this node.
    let free_vram = crate::gpu::monitor::detect().map(|m| m.free_bytes());
    let (placement, reason) = choose_lane_placement(free_vram, footprint);
    let device = match placement {
        LanePlacement::Gpu => "gpu",
        LanePlacement::Cpu => "cpu",
    };
    // Formal capture — a load-bearing decision gets a structured record, not just a
    // log line ([[observability-is-half-the-architecture]]). Best-effort: a sink
    // open/write failure never blocks the lane.
    use crate::inference::placement_capture::{JsonlPlacementCaptureSink, PlacementDecisionRecord};
    JsonlPlacementCaptureSink::glass_box().record(&PlacementDecisionRecord {
        schema_version: 1,
        captured_at_ms: JsonlPlacementCaptureSink::now_ms(),
        context: context.to_string(),
        model: base.id.clone(),
        served_ctx,
        device: device.to_string(),
        reason: reason.to_string(),
        free_vram_bytes: free_vram,
        footprint_bytes: footprint,
        margin_bytes: GPU_PLACEMENT_MARGIN_BYTES,
    });
    // One concise operator line for live tailing; the JSONL above is the audit trail.
    tracing::info!(
        target: "cognition::eval",
        device, reason, context,
        free_vram_bytes = ?free_vram,
        footprint_bytes = ?footprint,
        "eval-lane placement"
    );
    PlacementEvidence {
        placement,
        device: device.to_string(),
        reason: reason.to_string(),
        free_vram_bytes: free_vram,
        footprint_bytes: footprint,
    }
}

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
    EvalLane,
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

    // 3. Host-fit PHYSICAL window (`-c`) for the throwaway lane, derived from the
    //    live serving budget (see `plan_eval_lane_ctx`). One lane: a single
    //    measurement stream, no batching. This sizes the lane's launch `-c` + its
    //    placement; the fork's COGNITION window is read back from the lane's real
    //    `/props` after spawn (below), the same served-truth pin the supervisor
    //    applies to the living persona — so a measurement copy budgets against
    //    exactly what its lane serves, never this planned value
    //    ([[dreaming-mind-eval-must-match-live-cognition]], task #50).
    let lane_ctx = plan_eval_lane_ctx(&base);

    // 3b. GPU-FIRST placement (Joel: fill GPU lanes first, ~100% utilization; CPU is
    //     spillover of last resort, never the default for a coexisting lane). Probe
    //     live free VRAM (net of the living persona's resident lane) + size this base,
    //     then pack the GPU unless it genuinely can't hold this lane. EVIDENCED — the
    //     chosen device + headroom ride out on the result so a CPU spill is VISIBLE in
    //     the harness, not a silent slow path.
    let placement_evidence =
        decide_eval_lane_placement(&base, lane_ctx, &format!("eval-lane gene:{}", gene.name));

    // 4. Bring the lane up: forged base + the gene loaded via `--lora` (loadable;
    //    the per-request `lora` field decides per turn whether it actually pages in,
    //    which is exactly the base-vs-gene A/B below).
    let target = ServingTarget {
        model: base.clone(),
        context_window: lane_ctx,
        lanes: 1,
        adapters: vec![AdapterEntry {
            alias: gene.name.clone(),
            path: entry.path.clone(),
        }],
        placement: placement_evidence.placement,
    };
    emit_eval_phase("loading_lane", &format!("cold-loading gene eval lane ({})", gene.name));
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

    // The lane was launched with the gene loaded via `--lora`; probe the catalog
    // NOW so (a) the BASE arm can neutralize it — an empty genome must serve true
    // base, but llama.cpp applies a loaded adapter at 1.0 for any request that
    // omits the `lora` field, so the base arm must emit an explicit 0.0 and that
    // needs the catalog populated before the first (base) pass — and (b) a gene
    // that failed to load fails loud HERE, not as a silent no-op mid-measurement.
    adapter
        .probe_lora_catalog()
        .await
        .map_err(|e| CommandError::Internal(format!("eval-lane LoRA catalog probe failed: {e}")))?;

    // Pin the fork's cognition window to the lane's REAL served `/props` slot — the
    // SAME served-truth discipline the supervisor applies to the living persona
    // (`profile.context_length = snap.served_context_window`). The planned `lane_ctx`
    // sized the launch `-c`; the SERVED value is what her cognition must budget
    // against, so a measurement copy plans against exactly the window it runs on and
    // training can never silently diverge from the served reality
    // ([[dreaming-mind-eval-must-match-live-cognition]], task #50). Fail loud if the
    // lane is up but its `/props` is unreadable — never size cognition off the
    // planned value and diverge unseen.
    let served_ctx = lane.served_context_window().await.map_err(|e| {
        CommandError::Internal(format!(
            "eval lane for gene '{}' is up but its /props served window is unreadable ({e}) — \
             refusing to size the fork's cognition from the planned lane `-c` and silently diverge \
             from what the lane actually serves",
            gene.name
        ))
    })?;

    Ok(EvalLane {
        lane,
        adapter: std::sync::Arc::new(adapter),
        served_ctx,
        placement: placement_evidence,
    })
}

/// Stand up an ephemeral measurement lane for a BARE base model (no gene, no LoRA) — the
/// same #59 discipline as `spawn_gene_eval_lane`, minus the adapter. This is the clean
/// "measure THIS model through our full loop" control: `benchmark/run --base_model_id X`
/// forks the cognition copy onto a throwaway server for X, sized against free VRAM, WITHOUT
/// re-homing or disturbing the living persona (whose lane stays whatever she's served on).
/// It's what makes the same-model matrix — hold the harness fixed, vary the model — fall
/// out of one command, with no serving-pin race. Fails loud (never a substitute base) if
/// the id isn't in the registry or the lane won't come up.
async fn spawn_base_eval_lane(
    base_id: &str,
) -> Result<EvalLane, CommandError> {
    use crate::ai::adapter::AIProviderAdapter;
    use crate::inference::llama_server::{EphemeralServingLane, ServingTarget, PROVIDER_ID};

    let base = crate::model_registry::try_global()
        .and_then(|r| r.model(base_id).cloned())
        .ok_or_else(|| {
            CommandError::NotFound(format!(
                "base_model_id '{base_id}' is not in the model registry — cannot stand up a measurement lane for it. Call ai/inference/models for loadable ids."
            ))
        })?;
    let lane_ctx = plan_eval_lane_ctx(&base);
    let placement_evidence =
        decide_eval_lane_placement(&base, lane_ctx, &format!("eval-lane base:{base_id}"));
    let target = ServingTarget {
        model: base.clone(),
        context_window: lane_ctx,
        lanes: 1,
        adapters: vec![], // bare base — no gene
        placement: placement_evidence.placement,
    };
    // The ephemeral lane cold-loads the base model (can be minutes for a 14B+); emit
    // the phase so positronic layers show "loading <model>…", not a frozen bar.
    emit_eval_phase("loading_lane", &format!("cold-loading eval lane for {base_id}"));
    let lane = EphemeralServingLane::spawn(&target, EVAL_LANE_BASE_PORT)
        .await
        .map_err(|e| {
            CommandError::Internal(format!(
                "could not bring up the ephemeral eval lane for base '{base_id}': {e}"
            ))
        })?;
    let mut adapter =
        crate::ai::openai_adapter::OpenAICompatibleAdapter::from_registry(PROVIDER_ID)
            .with_runtime_base_url(lane.root().to_string())
            .with_default_model(base.id.clone())
            .with_dedicated_lane();
    adapter
        .initialize()
        .await
        .map_err(|e| CommandError::Internal(format!("eval-lane adapter failed to initialize: {e}")))?;
    let served_ctx = lane.served_context_window().await.map_err(|e| {
        CommandError::Internal(format!(
            "eval lane for base '{base_id}' is up but its /props served window is unreadable ({e})"
        ))
    })?;
    Ok(EvalLane {
        lane,
        adapter: std::sync::Arc::new(adapter),
        served_ctx,
        placement: placement_evidence,
    })
}

/// One eval task. Both the JSONL rows and inline `tasks` deserialize into this;
/// every field is optional so an authoring typo degrades to a benign empty rather
/// than failing the whole run. A task is TEST-GRADED when it carries `test`, else
/// substring-graded against `expect`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
// `#[ts(export)]` so the binding at `bindings/EvalTask.ts` (imported by
// GenomeTeachParams / MinedTask / RedactMemoryParams) REGENERATES with this struct
// instead of drifting stale — the file was orphaned (a derive without export) and a
// new field silently rotted it. Default export path = `bindings/EvalTask.ts`, exactly
// where the parents already import it from.
#[ts(export)]
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
    /// A REAL definition-of-done: a shell command run in the persona's workspace AFTER she
    /// acts (edits files with her tools). Pass = exit 0. This is what makes a task REAL vs
    /// a HumanEval toy — the grade checks the actual repo state her edits produced (e.g.
    /// `cargo test --test foo`), not code extracted from her chat answer. Supersedes
    /// `test`/`expect`; the recovery loop feeds its stdout+stderr back on failure so she
    /// iterates against the real compiler/test output until it goes green.
    #[serde(default, alias = "dodShell", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub dod_shell: Option<String>,
    /// ARTIFACT grade: a relative in-workspace path she is told to write her solution to. When
    /// set alongside `test`, the grade reads HER FILE (her hands) instead of extracting a code
    /// block from her spoken answer (her mouth), then runs the SAME harness (strip her `main`,
    /// append `test`, compile, run). This is how an ACTING persona is measured — the act→verify
    /// loop is only visible if we grade what she actually wrote + compiled, not what she narrated.
    /// The file lands in the workspace root (= core cwd, where `code/write` sandboxes writes).
    #[serde(default, alias = "solutionFile", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub solution_file: Option<String>,
    /// Task-state SETUP: a shell command run BEFORE the prompt is posed, restoring the
    /// task's initial workspace state so runs are repeatable (a `gym/mine` task re-breaks
    /// its checkout: `git checkout <commit>^ -- src/lib.rs`). Setup failure is a named
    /// infra grade, never a silent broken workspace ([[fallbacks-are-illegal-fail-loud]]).
    #[serde(default, alias = "setupShell", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub setup_shell: Option<String>,
    /// FUNCTIONAL WEB-DEV grade: structural acceptance criteria checked against what the
    /// persona's UI ACTUALLY RENDERED. When non-empty, the grade OBSERVES `target` through
    /// HER OWN eyes (the `perception/observe` eye-node path — the same purity as every other
    /// tool) and scores the element tree with `perception::scoring::grade_ui`. This measures a
    /// persona on building a UI that WORKS, on equal footing for every model: the structure tree
    /// is plain text a non-visual model reads exactly like a VLM ([[built-to-teach-lesser-tuned-intelligences-win]]).
    /// The SAME `UiScore` is her self-check, her training label, and this benchmark score.
    #[serde(default, alias = "uiChecks", skip_serializing_if = "Vec::is_empty")]
    pub ui_checks: Vec<crate::perception::scoring::UiCheck>,
    /// What to observe for a web-dev task: a workspace-relative path (absolutized to a `file://`
    /// URL against her workspace root — where `code/write` sandboxes her files) or an explicit
    /// URL (e.g. an `http://localhost` dev server she started). Defaults to `index.html`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub target: Option<String>,
    /// Fraction of `ui_checks` that must hold to PASS (`1.0` = every criterion; the fractional
    /// score always rides along in the grade line). Defaults to `1.0` — "the UI works".
    #[serde(default, alias = "uiPassThreshold", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ui_pass_threshold: Option<f32>,
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
    #[ts(optional, type = "number")]
    pub reviewers: Option<u32>,
    /// Max act→observe cycles per task before it counts as unfinished. Default 8.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub max_acts: Option<u32>,
    /// Agentic-recovery budget: how many times a FAILED test-graded task is handed its
    /// compiler/test output to fix before it scores a miss. Default [`MAX_FAIL_RETRIES`].
    /// Set `0` for the ONE-SHOT baseline (what plain inference / unsloth gets on the same
    /// weights) — so a `0` vs `N` A/B on the identical model+benchmark measures exactly the
    /// edge our agentic loop adds, repeatably, from one param.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub max_retries: Option<u32>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub learn: Option<bool>,
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

#[derive(Debug, Clone, Serialize, TS, Default)]
pub struct CognitionEvalResult {
    /// The run handle (#86): present on a detached ack AND on the ledger row, so the
    /// two halves of fire-and-poll join on one id.
    pub run_id: Option<String>,
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
    #[ts(optional, type = "number")]
    pub base_pass_rate: Option<f64>,
    /// `pass_rate - base_pass_rate` — the LIFT the gene produced (A/B mode only).
    /// Positive = the gene made her a better coder; the measure→decide gate adopts
    /// only `lift > 0`. Negative = an overfit/regressing gene, correctly rejected.
    #[serde(skip_serializing_if = "Option::is_none")]
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
    #[ts(optional, type = "number")]
    pub lane_free_vram_bytes: Option<u64>,
    /// Estimated weight+scratch footprint (bytes) of the measurement lane's base —
    /// what GPU-first weighed against free VRAM. `None` when unsized / single-pass.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub lane_estimated_footprint_bytes: Option<u64>,
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
            tokio::spawn(async move {
                match CognitionEval::run_eval(inner).await {
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
                persona_id,
                run_id: Some(run_id),
                ..Default::default()
            });
        }
        CognitionEval::run_eval(p).await
    }
}

impl CognitionEval {
    /// The eval body — deliberately ctx-free (reaches the persona's live cognition through
    /// the global workspace registry, owns its params), so it runs inline from `run` OR is
    /// spawned detached for fire-and-poll (#86). One code path, two launch modes: the test
    /// and prod paths stay identical [[validate-via-pure-rust-not-npm-jtag]].
    async fn run_eval(p: CognitionEvalParams) -> Result<CognitionEvalResult, CommandError> {
        // Eval-preemption lease: suspend the WHOLE live fleet's autonomic self-tick
        // for the duration of this measurement, so it runs on an uncontended GPU. The
        // personas stay online and still answer THIS eval's directed cognition turns —
        // they just stop wandering, so their self-directed generation can't fight the
        // measurement for the single GPU. The lease RESTORES every persona on drop,
        // including early-return and panic (Drop rides the unwind). Held for the whole
        // body. `None` (no live fleet — tools/tests) → nothing to quiesce; measure as-is.
        // [[benchmark-is-a-governor-preemption-lease]] [[first-class-citizens-even-during-benchmarks]]
        // Observable via `probe!`, NOT `tracing::info!`: the eval runs across the
        // concurrent tokio fleet where tracing lines don't survive to a readable sink
        // ([[jtag-probes-are-rtos-debugger]]). Whether the fleet actually quiesced is
        // load-bearing for trusting a benchmark number (contended → depressed score), so
        // it MUST be verifiable — this probe is how we confirm the clean lane fired.
        let _fleet_lease = match crate::persona::PersonaAircRuntimeRegistry::try_global() {
            Some(r) => {
                let lease = r.quiesce_all();
                crate::probe!(
                    class = "eval.quiesce",
                    personas = lease.count(),
                    "eval-preemption lease: fleet quiesced for the measurement"
                );
                Some(lease)
            }
            None => {
                crate::probe!(
                    class = "eval.quiesce.absent",
                    "eval-preemption: no global roster published — fleet NOT quiesced; measurement may be GPU-contended"
                );
                None
            }
        };

        // Stamp this pass's run_id onto the live progress snapshot for the whole body
        // (RAII-cleared on any exit) so a persona-only poll can tell it's reading THIS
        // run, not a prior one's finished numbers (the stale-progress trap).
        let _run_scope = RunIdScope::enter(p.run_id.clone());

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

        // Task source: inline → a gym reference resolved through `gym::resolve_gym`
        // (an existing on-disk file wins for a custom set; otherwise a committed gym
        // resolves from the embedded registry — CWD- and deployment-independent;
        // a typo'd / vanished gym FAILS LOUD naming every candidate, no silent
        // degrade) → when no `eval_set` is given, the committed default through the
        // SAME resolver. One JSONL line = one task. A malformed line FAILS LOUD with
        // its line number — never silently dropped. A vanished task would shrink the
        // gym and report a clean score over fewer tasks than intended: the same
        // invisible-degraded-mode as a fallback, which the resolver's fail-loud kills.
        let parse_jsonl = |text: &str, origin: &str| -> Result<Vec<EvalTask>, CommandError> {
            text.lines()
                .enumerate()
                .map(|(i, l)| (i + 1, l.trim()))
                .filter(|(_, l)| !l.is_empty())
                .map(|(n, l)| {
                    serde_json::from_str::<EvalTask>(l).map_err(|e| {
                        CommandError::Invalid(format!("{origin} line {n}: malformed EvalTask: {e}"))
                    })
                })
                .collect()
        };
        let tasks: Vec<EvalTask> = if let Some(inline) = p.tasks {
            inline
        } else {
            let reference = p.eval_set.as_deref().unwrap_or(DEFAULT_EVAL_SET);
            let (origin, text) =
                crate::cognition::gym::resolve_gym(reference).map_err(CommandError::Invalid)?;
            parse_jsonl(&text, &origin)?
        };

        // Does this exam grade her HANDS or her MOUTH? A task graded from a file she
        // writes (`solution_file`), a workspace DoD she must satisfy (`dod_shell`), a
        // pinned repo she edits (`workspace_root`), or a UI she BUILDS and we OBSERVE
        // (`ui_checks`) needs tools — she must `code/write` the file, then can
        // `perception/observe` her own render and iterate (the image-feedback loop).
        // A purely spoken-graded task (`test`/`expect`) does not — and offering tools
        // there is a net TAX: a native-tool-call model loops on the discovery pair
        // (`commands/help`) and never speaks (the isolator's Devstral 100%→0%). Match
        // the surface to the modality.
        let needs_tools = p.workspace_root.is_some()
            || tasks.iter().any(|t| {
                t.solution_file.is_some() || t.dod_shell.is_some() || !t.ui_checks.is_empty()
            });

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
        // GPU-first placement evidence for the gene's measurement lane — surfaced on
        // the result so the harness shows which device the A/B was scored on. None in
        // single-pass mode (that forks onto her LIVE lane, which is already GPU).
        let mut placement_evidence: Option<PlacementEvidence> = None;
        let cycle = match (&p.gene, p.base_model_id.as_deref()) {
            // A gene names its own forged base → ephemeral lane + the gene as `--lora`.
            (Some(gene), _) => {
                let EvalLane {
                    lane,
                    adapter,
                    served_ctx,
                    placement,
                } = spawn_gene_eval_lane(gene).await?;
                placement_evidence = Some(placement);
                let cycle = fork_eval_cycle_waiting(&persona_uuid, || {
                    crate::cognition::persona_workspace::global()
                        .fork_eval_cycle_with_adapter(&persona_uuid, adapter.clone(), served_ctx, needs_tools, p.workspace_root.as_deref())
                })
                .await
                .ok_or_else(|| CommandError::NotFound(format!(
                    "no workspace template for persona {persona_uuid} after waiting {WORKSPACE_TEMPLATE_WAIT_TRIES}s — its mind was not assembled at spawn (register_from_cfg), so eval cannot fork a measurement copy"
                )))?;
                _eval_lane = Some(lane);
                cycle
            }
            // A bare base_model_id → ephemeral lane for THAT model, no gene. The clean
            // same-model control: measure the full loop on a chosen model in its own
            // throwaway server, living persona untouched (#59).
            (None, Some(base_id)) => {
                let EvalLane {
                    lane,
                    adapter,
                    served_ctx,
                    placement,
                } = spawn_base_eval_lane(base_id).await?;
                placement_evidence = Some(placement);
                let cycle = fork_eval_cycle_waiting(&persona_uuid, || {
                    crate::cognition::persona_workspace::global()
                        .fork_eval_cycle_with_adapter(&persona_uuid, adapter.clone(), served_ctx, needs_tools, p.workspace_root.as_deref())
                })
                .await
                .ok_or_else(|| CommandError::NotFound(format!(
                    "no workspace template for persona {persona_uuid} after waiting {WORKSPACE_TEMPLATE_WAIT_TRIES}s — its mind was not assembled at spawn (register_from_cfg), so eval cannot fork a measurement copy"
                )))?;
                _eval_lane = Some(lane);
                cycle
            }
            // Neither → fork onto her LIVE lane (a plain number on whatever she's served on).
            // Same bounded wait-for-template as the lane branches above (fork_eval_cycle_waiting):
            // the post-reboot register_from_cfg race hits every fork path identically.
            (None, None) => fork_eval_cycle_waiting(&persona_uuid, || {
                crate::cognition::persona_workspace::global()
                    .fork_eval_cycle(&persona_uuid, needs_tools, p.workspace_root.as_deref())
            })
            .await
            .ok_or_else(|| CommandError::NotFound(format!(
                "no workspace template for persona {persona_uuid} after waiting {WORKSPACE_TEMPLATE_WAIT_TRIES}s — its mind was not assembled at spawn (register_from_cfg), so eval cannot fork a measurement copy without measuring her live mind"
            )))?,
        };

        // GLASS-BOX (task #14): if a capture_dir is pinned, wrap the fork's cognition in the
        // JSONL capture sink so every tick's bids + DECISION + timings append to
        // <dir>/<persona>.jsonl. This is what makes a MEASURED run inspectable — reading the
        // `decision` field tells us whether she chose Act (and which tool) or Respond, which is
        // exactly the fork needed to diagnose a 0-edit. Fork-only, never her live mind. Opt-in.
        let cycle = match &p.capture_dir {
            Some(dir) => cycle.with_capture(std::sync::Arc::new(
                crate::cognition::workspace_capture::JsonlWorkspaceCaptureSink::open(
                    std::path::Path::new(dir),
                    persona_uuid,
                )
                .map_err(|e| {
                    CommandError::Internal(format!("failed to open eval capture_dir '{dir}': {e}"))
                })?,
            )),
            None => cycle,
        };

        // WARM-GATE — never measure a COLD model. A just-loaded or just-swapped lane 500s
        // until its weights + KV are resident; firing the gym into it scores "inference
        // failed" on every task — a false ZERO that would LIE about how far behind (or
        // ahead) we are. A measurement that can't trust its own model is no measurement.
        // Poll with a tiny throwaway deliberation until the lane answers, then run; if it
        // never warms, fail LOUD rather than emit a bogus 0. (Cost is one cheap probe on a
        // warm lane — the common case returns on the first try.)
        {
            let warm_room = Uuid::new_v4();
            let probe_delivery = crate::persona::rag_budget::RagDelivery {
                source_id: "airc".to_string(),
                items: vec![crate::persona::rag_budget::RagItem {
                    content: "ready check".to_string(),
                    tokens: 0,
                    metadata: serde_json::json!({ "peer_id": "peer", "occurred_at_ms": EVAL_EPOCH_MS }),
                }],
                tokens_used: 0,
                continuation: None,
                resolution_used: crate::persona::rag_budget::ResolutionPreference::Raw,
            };
            let mut warm = false;
            for _ in 0..15u32 {
                let burst = crate::cognition::workspace::Burst::from_turns(
                    warm_room,
                    crate::persona::service_loop::build_workspace_turns(
                        std::slice::from_ref(&probe_delivery),
                        "",
                        "",
                        None,
                    ),
                );
                let probe = crate::cognition::act_observe::drive_to_settle(
                    &cycle,
                    burst,
                    warm_room,
                    0, // no acts — a bare deliberation is enough to prove the lane answers
                    crate::cognition::workspace::TurnFraming::directed(),
                )
                .await;
                if probe.inference_error.is_none() {
                    warm = true;
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_secs(4)).await;
            }
            // Reset the volatile scratch the probe touched so task #1 starts from the same
            // clean frame every other task does.
            cycle.reset_working_memory();
            if !warm {
                return Err(CommandError::Internal(
                    "serving lane not ready after ~60s of warm probes — refusing to run an \
                     eval on a cold model (every task would score a false 'inference failed' \
                     zero). Wait for the model to load, then retry."
                        .to_string(),
                ));
            }
        }

        // WORKSPACE-ROOT SEAM (#49) — if the caller pinned a workspace_root (SWE-bench: the target
        // repo clone), root the persona's file engine THERE before any task runs, by invoking
        // code/create-workspace through HER OWN identity-bearing executor. create-workspace keys on
        // the caller identity (not a spoofable param), so this reroots the eval-fork's hands at the
        // repo — her subsequent edits land in the clone, not the core cwd. Deterministic: no reliance
        // on the model choosing to call create-workspace itself. Fail LOUD if requested but unroutable
        // — a silent no-root scores a false ZERO (0-byte diff) that would LIE about the solver
        // ([[fallbacks-are-illegal-fail-loud]]).
        if let Some(root) = &p.workspace_root {
            let acting = cycle.acting().ok_or_else(|| {
                CommandError::Internal(
                    "workspace_root requested but this eval cycle has no acting body (no hands) — \
                     cannot root a workspace for a pure-cognition persona"
                        .to_string(),
                )
            })?;
            let ws_ctx = crate::cognition::tool_executor::ToolExecutionContext {
                persona_id: acting.persona_id,
                persona_name: acting.persona_name.clone(),
                session_id: Uuid::new_v4(),
                context_id: Uuid::new_v4(),
                caller_context: serde_json::Value::Null,
                persona_config: crate::cognition::tool_executor::PersonaMediaConfigLite {
                    auto_load_media: false,
                    supported_media_types: vec![],
                },
            };
            let ws_call = crate::ai::types::ToolCall {
                id: "eval-workspace-root".to_string(),
                name: "code/create-workspace".to_string(),
                input: serde_json::json!({ "workspace_root": root }),
            };
            let ws_out = acting
                .executor
                .execute_native_batch(std::slice::from_ref(&ws_call), &ws_ctx, 8000)
                .await
                .map_err(|e| {
                    CommandError::Internal(format!("failed to root eval workspace at '{root}': {e}"))
                })?;
            if let Some(r) = ws_out.results.first() {
                if r.is_error.is_some() {
                    return Err(CommandError::Internal(format!(
                        "code/create-workspace rejected workspace_root '{root}': {} — refusing to \
                         run the eval with the persona's hands rooted at the wrong directory (would \
                         score a false zero).",
                        r.content
                    )));
                }
            }
            crate::probe!(
                class = "eval.workspace.rooted",
                persona = %acting.persona_name,
                root = %root,
                "eval persona's file engine rooted at the target repo before her cycle"
            );
        }

        let max_acts = p.max_acts.unwrap_or(DEFAULT_MAX_ACTS) as usize;
        let max_retries = p.max_retries.unwrap_or(MAX_FAIL_RETRIES);
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
            let (base_score, _) = run_pass(&cycle, &isolation, &tasks, room, max_acts, max_retries, p.workspace_root.as_deref()).await;

            // Both arms start each task from the pre-eval memory frame — `run_pass`
            // rewinds the admission frame before EVERY task (per-task isolation), so
            // the candidate arm inherits none of the engrams the base arm admitted.
            // The only difference the lift measures is the genome, paged in here.
            cycle.page_in(vec![crate::ai::types::ActiveAdapterRequest {
                name: gene.name.clone(),
                path: gene.path.clone(),
                domain: String::new(),
                scale: gene.scale.unwrap_or(1.0),
            }]);
            let (gene_score, gene_results) =
                run_pass(&cycle, &isolation, &tasks, room, max_acts, max_retries, p.workspace_root.as_deref()).await;
            cycle.page_out();

            // Guard drops here: her memory frame + real persistence sink restored.
            let verify = self_verify_rate(&gene_results);
            let agg = speed_latency_aggregates(&gene_results);
            let mut result = CognitionEvalResult {
                run_id: None,
                detached: false,
                persona_id: persona_uuid.to_string(),
                score: gene_score,
                total,
                pass_rate: rate(gene_score),
                self_verify_rate: verify,
                mean_latency_ms: agg.mean_latency_ms,
                p95_latency_ms: agg.p95_latency_ms,
                mean_tokens_per_second: agg.mean_tokens_per_second,
                mean_decode_tokens_per_second: agg.mean_decode_tokens_per_second,
                mean_cache_hit_rate: agg.mean_cache_hit_rate,
                total_prefill_ms: agg.total_prefill_ms,
                total_decode_ms: agg.total_decode_ms,
                total_output_tokens: agg.total_output_tokens,
                results: gene_results,
                gene_id: Some(gene.name.clone()),
                base_pass_rate: Some(rate(base_score)),
                lift: Some(rate(gene_score) - rate(base_score)),
                lane_placement: placement_evidence
                    .as_ref()
                    .map(|e| e.device.clone())
                    .unwrap_or_default(),
                lane_placement_reason: placement_evidence
                    .as_ref()
                    .map(|e| e.reason.clone())
                    .unwrap_or_default(),
                lane_free_vram_bytes: placement_evidence.as_ref().and_then(|e| e.free_vram_bytes),
                lane_estimated_footprint_bytes: placement_evidence
                    .as_ref()
                    .and_then(|e| e.footprint_bytes),
            };
            result.run_id = p.run_id.clone();
            result.run_id = p.run_id.clone();
        append_progress_ledger(
            &result,
            p.note.as_deref(),
            &eval_set_label,
            _fleet_lease.as_ref().map(|_| true),
        );
            return Ok(result);
        }

        // Readiness gate (single-pass on her LIVE lane): refuse to grade a COLD
        // serving lane. After a core/llama-server relaunch the model is not resident
        // for ~tens of seconds; firing tasks at it returns empty generations that the
        // grader would silently record as 0-token "no match" failures — a phantom
        // score produced in an invisible degraded mode. Wait (bounded) for the lane to
        // actually be able to generate; fail loud if it never is, naming the cause.
        // (The gene A/B path above stands up its own EphemeralServingLane and waits on
        // spawn, so this guards only the live-lane single pass.)
        {
            const LANE_WARMUP: std::time::Duration = std::time::Duration::from_secs(90);
            if crate::inference::llama_server::await_ready_serving(LANE_WARMUP)
                .await
                .is_none()
            {
                return Err(CommandError::Invalid(format!(
                    "inference lane not serving-ready after {}s — refusing to grade a cold lane \
                     (would record phantom 0-token failures). The lane warms after a \
                     core/llama-server relaunch; retry once `/health` returns 200.",
                    LANE_WARMUP.as_secs()
                )));
            }
        }

        // Single pass: measure whatever genome is currently paged in (base by
        // default) — the plain coder number, no A/B. Still isolated, so a plain
        // baseline run is reproducible and leaves her memory untouched. TEAM mode
        // (reviewers>=1, live single-pass only) forks a second copy of the same persona
        // as a reviewer and grades the reviewed answer — same model, +1 teammate.
        let want_team = p.reviewers.unwrap_or(0) >= 1 && p.gene.is_none() && p.base_model_id.is_none();
        let (score, results) = if want_team {
            let reviewer = crate::cognition::persona_workspace::global()
                .fork_eval_cycle(&persona_uuid, needs_tools, p.workspace_root.as_deref())
                .ok_or_else(|| CommandError::NotFound(format!(
                    "no workspace template for persona {persona_uuid} — cannot fork a reviewer teammate"
                )))?;
            let reviewer_iso = reviewer.isolate_for_eval();
            let out =
                run_pass_team(&cycle, &isolation, &reviewer, &reviewer_iso, &tasks, room, max_acts).await;
            drop(reviewer_iso);
            out
        } else {
            run_pass(&cycle, &isolation, &tasks, room, max_acts, max_retries, p.workspace_root.as_deref()).await
        };
        drop(isolation);

        // LEARN mode: the exam just taught her — carry the redacted lesson back to the
        // LIVING self. She keeps the experience of having been asked and how she did; the
        // held-out answer key is scrubbed so she can never memorize it (redaction, not
        // forget-context: keep the memory, excise the crib sheet). The exam ran on the fork
        // (#59 intact); only the clean lesson crosses back. Single-pass only in this slice.
        if p.learn.unwrap_or(false) && p.gene.is_none() {
            let transferred = transfer_redacted_lessons(&persona_uuid, room, &tasks, &results);
            tracing::info!(
                persona = %persona_uuid,
                transferred,
                tasks = tasks.len(),
                "learn mode: redacted exam lessons admitted to the living self"
            );
        }

        let verify = self_verify_rate(&results);
        let agg = speed_latency_aggregates(&results);
        let mut result = CognitionEvalResult {
            run_id: None,
            detached: false,
            persona_id: persona_uuid.to_string(),
            score,
            total,
            pass_rate: rate(score),
            self_verify_rate: verify,
            mean_latency_ms: agg.mean_latency_ms,
            p95_latency_ms: agg.p95_latency_ms,
            mean_tokens_per_second: agg.mean_tokens_per_second,
            mean_decode_tokens_per_second: agg.mean_decode_tokens_per_second,
            mean_cache_hit_rate: agg.mean_cache_hit_rate,
            total_prefill_ms: agg.total_prefill_ms,
            total_decode_ms: agg.total_decode_ms,
            total_output_tokens: agg.total_output_tokens,
            results,
            gene_id: None,
            base_pass_rate: None,
            lift: None,
            // Single-pass forks onto her LIVE lane, which serves on the GPU
            // (LanePlacement::Gpu default). No throwaway lane was placed, so there's
            // no free/footprint decision to report — the device is hers.
            lane_placement: "gpu (live persona lane)".to_string(),
            lane_placement_reason: "single-pass: measured on the living persona's GPU lane"
                .to_string(),
            lane_free_vram_bytes: None,
            lane_estimated_footprint_bytes: None,
        };
        result.run_id = p.run_id.clone();
        append_progress_ledger(
            &result,
            p.note.as_deref(),
            &eval_set_label,
            _fleet_lease.as_ref().map(|_| true),
        );
        Ok(result)
    }
}

/// Build the durable LESSON string for one graded exam task — what she was asked
/// and how she did. Pure and answer-key-agnostic (the caller's redaction policy
/// scrubs the crib sheet); kept separate so it's unit-testable.
fn format_exam_lesson(task: &EvalTask, result: &EvalTaskResult) -> String {
    let outcome = if result.ok { "I solved it" } else { "I did NOT solve it" };
    format!(
        "Exam task '{}'. I was asked: {} {} (grade: {}).",
        task.id.trim(),
        task.prompt.trim(),
        outcome,
        result.grade.trim()
    )
}

/// LEARN mode's transfer step: admit each task's REDACTED lesson into the LIVING
/// persona so the exam becomes a real teacher without leaking the answer key. The
/// exam already ran on the fork (#59 untouched); this is the ONLY thing that
/// reaches her durable memory, and only after the held-out answers are scrubbed.
/// Returns how many FRESH lessons were admitted (an identical lesson from a
/// re-take dedups idempotently via `admit_reflection` and is not counted).
fn transfer_redacted_lessons(
    persona_uuid: &uuid::Uuid,
    room: uuid::Uuid,
    tasks: &[EvalTask],
    results: &[EvalTaskResult],
) -> usize {
    // Policy: scrub every held-out answer key (each task's `expect`).
    let answers: Vec<String> = tasks
        .iter()
        .map(|t| t.expect.clone())
        .filter(|a| !a.trim().is_empty())
        .collect();
    let policy = crate::persona::redaction::RedactionPolicy::new(vec![Box::new(
        crate::persona::redaction::ExamKeyDetector::new(
            answers,
            crate::persona::redaction::ExamKeyDetector::DEFAULT_MIN_LEN,
        ),
    )]);

    // The LIVING persona's admission — never the fork.
    let Some(admission) = crate::cognition::persona_workspace::global()
        .get(persona_uuid)
        .and_then(|cycle| cycle.acting().map(|a| a.admission.clone()))
    else {
        tracing::warn!(
            persona = %persona_uuid,
            "learn mode: no live admission for persona — lesson not transferred \
             (she was measured, but the living self is not resident to teach)"
        );
        return 0;
    };

    let mut admitted = 0usize;
    for (task, result) in tasks.iter().zip(results.iter()) {
        if task.prompt.trim().is_empty() {
            continue;
        }
        let (lesson, _report) = policy.redact(&format_exam_lesson(task, result));
        let engram = crate::persona::engram::Engram {
            id: uuid::Uuid::new_v4(),
            context_id: Some(room),
            kind: crate::persona::engram::EngramKind::Episodic,
            content: lesson,
            origin: crate::persona::engram::EngramOrigin::SelfReflection {
                parent_engram_id: uuid::Uuid::nil(),
            },
            recall_keys: vec!["exam".to_string(), task.id.clone()],
            admitted_at_ms: crate::persona::trace::now_ms(),
            trust_state_at_admission: crate::persona::engram::TrustState::SelfTrust,
            admission_trace_id: None,
        };
        if matches!(
            admission.admit_reflection(engram),
            Ok(crate::persona::engram::AdmissionDecision::Admit { .. })
        ) {
            admitted += 1;
        }
    }
    admitted
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

/// Speed + latency aggregates over the per-task results — the SPEED and LATENCY
/// axes of the scoreboard. P95 is the honest tail (a mean hides an occasional
/// stall); wall-clock throughput is averaged per-task (not total_tokens/total_ms)
/// so one slow task can't dominate. The PREFILL-vs-DECODE breakdown (real decode
/// tok/s, cache-hit-rate, the prefill/decode ms split) is the lever data that
/// makes speed iterable instead of one conflated number — measured 77% of eval
/// time was prefill, not decode. Empty set → all zero.
#[derive(Debug, PartialEq)]
struct SpeedAggregates {
    mean_latency_ms: f64,
    p95_latency_ms: u64,
    /// WALL-CLOCK mean tok/s — diluted by prefill + cognition overhead.
    mean_tokens_per_second: f64,
    /// REAL mean decode tok/s off the lane clock — the honest generation rate.
    mean_decode_tokens_per_second: f64,
    /// Mean KV-cache hit-rate (cached / total prompt tokens).
    mean_cache_hit_rate: f64,
    total_prefill_ms: u64,
    total_decode_ms: u64,
    total_output_tokens: u32,
}

fn speed_latency_aggregates(results: &[EvalTaskResult]) -> SpeedAggregates {
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
    let mean_decode_tps = results.iter().map(|r| r.decode_tokens_per_second).sum::<f64>() / n;
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
        let (Some(persona_id), Some(run_id)) = (p.persona_id, p.run_id) else {
            // No run handle → live progress only (the "how's it going" poll).
            return Ok(CognitionEvalStatusResult { complete: false, row: None, progress });
        };
        let home = std::env::var("HOME")
            .map_err(|_| CommandError::Internal("HOME unset — no progress ledger".into()))?;
        let path = std::path::PathBuf::from(home)
            .join(".continuum/progress")
            .join(format!("{persona_id}.jsonl"));
        let Ok(text) = std::fs::read_to_string(&path) else {
            // No ledger yet = no completed runs for this persona = pending.
            return Ok(CognitionEvalStatusResult { complete: false, row: None, progress });
        };
        for line in text.lines().rev() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                if v.get("runId").and_then(|r| r.as_str()) == Some(run_id.as_str()) {
                    return Ok(CognitionEvalStatusResult { complete: true, row: Some(v), progress });
                }
            }
        }
        Ok(CognitionEvalStatusResult { complete: false, row: None, progress })
    }
}

crate::register_stateless_command!(CognitionEvalStatus);

/// Write a FAILED run row to the progress ledger so `cognition/eval-status` surfaces
/// the error (keyed on `run_id`) instead of returning `null` forever. The poll surface
/// must be able to tell "died" from "still starting" — a detached run that errors before
/// [`append_progress_ledger`] otherwise reads as an eternal pending. `error` + `failed:true`
/// mark it; `total:0` keeps the numeric shape valid for consumers.
fn append_failed_ledger(persona_id: &str, run_id: &str, note: &str, error: &str) {
    let Some(home) = std::env::var("HOME").ok() else {
        return;
    };
    let dir = std::path::PathBuf::from(home).join(".continuum/progress");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join(format!("{persona_id}.jsonl"));
    let row = serde_json::json!({
        "capturedAtMs": crate::persona::trace::now_ms(),
        "personaId": persona_id,
        "runId": run_id,
        "note": note,
        "failed": true,
        "error": error,
        "score": 0,
        "total": 0,
    });
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{row}");
    }
}

/// `clean_lane`: whether an eval-preemption quiesce lease was held for this run
/// (`Some(true)` = the live fleet was suspended, the fork measured on a clean GPU
/// lane; `None` = no lease acquired — no live roster, so the provenance is UNKNOWN,
/// never falsely claimed clean). This is the honesty stamp: a number carries whether
/// it was measured contended, on the durable row, so `cognition/observe` can light a
/// CLEAN/UNKNOWN chip instead of anyone inferring it. [[benchmark-numbers-carry-gpu-provenance]]
fn append_progress_ledger(
    result: &CognitionEvalResult,
    note: Option<&str>,
    eval_set: &str,
    clean_lane: Option<bool>,
) {
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
        "basePassRate": result.base_pass_rate,
        "lift": result.lift,
        "note": note,
        "runId": result.run_id,
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
/// How many times a FAILED (test-graded) task is handed its compiler/test output to fix
/// before we score it a miss. This is the agentic-recovery budget — the edge over one-shot
/// inference. 3 keeps a full eval bounded while giving real iteration room.
const MAX_FAIL_RETRIES: u32 = 3;

/// Seconds to wait (1s/try) for a persona's fork-template to appear before failing the
/// eval. Covers the post-reboot window where the supervisor's `register_from_cfg` is
/// still assembling the mind when an eval fires (the fork race, 2026-07-17).
const WORKSPACE_TEMPLATE_WAIT_TRIES: u32 = 10;

/// Fork a measurement copy, WAITING out the post-reboot template race. After a reboot
/// `register_from_cfg` assembles each persona's fork-template asynchronously, so an eval
/// fired seconds after boot can race ahead of it and see `None`. This race is NOT specific
/// to one lane — it hits the live-lane, base-model-lane, AND gene-lane forks identically —
/// so ALL of them retry through here (previously only the live-lane branch did, and a
/// tool-using eval on a base_model_id fired right after `cu reboot` failed loud instead of
/// waiting). `fork` is the per-lane fork call; retried up to
/// [`WORKSPACE_TEMPLATE_WAIT_TRIES`] times, 1s apart, then `None` (caller fails loud).
async fn fork_eval_cycle_waiting(
    persona_uuid: &Uuid,
    mut fork: impl FnMut() -> Option<crate::cognition::workspace::WorkspaceCycle>,
) -> Option<crate::cognition::workspace::WorkspaceCycle> {
    for attempt in 0..WORKSPACE_TEMPLATE_WAIT_TRIES {
        if let Some(cycle) = fork() {
            return Some(cycle);
        }
        if attempt == 0 {
            tracing::info!(
                %persona_uuid,
                "eval: workspace template not ready (post-spawn register_from_cfg race) — waiting"
            );
        }
        // Emit the wait as an EVENT so positronic layers show "preparing…" instead of a
        // dead spinner — this null-progress window otherwise looked identical to a hang.
        emit_eval_phase("preparing", &format!("waiting for workspace template ({}s)", attempt + 1));
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    None
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
                "atMs": crate::persona::trace::now_ms(),
            }),
        );
    }
    crate::probe!(class = "eval.phase", phase = phase, detail = detail, "eval lifecycle phase");
}

/// Run a REAL definition-of-done: a shell command in the persona's workspace (cwd). Pass =
/// exit 0. Returns `(ok, verdict)`; on failure the verdict carries the real stdout+stderr
/// (tail-bounded) so the recovery loop hands the model the actual error to fix against.
/// This is how a persona works on REAL things — her file edits are checked by a real build/
/// test, not by grading text she typed into chat.
async fn run_dod(cmd: &str) -> (bool, String) {
    match tokio::process::Command::new("bash")
        .arg("-lc")
        .arg(cmd)
        .output()
        .await
    {
        Ok(o) => {
            let ok = o.status.success();
            let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
            s.push_str(&String::from_utf8_lossy(&o.stderr));
            let s = s.trim();
            let tail: String = {
                let n = s.chars().count();
                if n > 2000 {
                    format!("…{}", s.chars().skip(n - 2000).collect::<String>())
                } else {
                    s.to_string()
                }
            };
            if ok {
                (true, format!("DoD passed: `{cmd}`"))
            } else {
                (false, format!("DoD `{cmd}` FAILED:\n{tail}"))
            }
        }
        Err(e) => (false, format!("DoD `{cmd}` could not run: {e}")),
    }
}

/// Grade a FUNCTIONAL WEB-DEV task by OBSERVING what the persona's UI actually rendered, then
/// scoring the element tree against the task's `ui_checks`. Returns `(ok, verdict)` in the same
/// shape as [`run_dod`].
///
/// # Purity — she is graded through HER OWN eyes
///
/// The observation routes `perception/observe` through the persona's OWN identity-bearing
/// executor (`cycle.acting().executor`), exactly like the workspace-root seam roots her file
/// engine — NOT a separate grader-only command path. The persona sees her work via the same
/// eye-node the benchmark scores it with; there is one path, so the first-class citizen stays one
/// ([[dispatch-path-purity-and-load-harness]]). The core is headless and cannot render, so a
/// perception task requires a connected eye-node adapter; with none, the observe call fails LOUD
/// and the task grades an honest infra-fail, never a fabricated pass ([[fallbacks-are-illegal-fail-loud]]).
async fn perception_grade(
    cycle: &crate::cognition::workspace::WorkspaceCycle,
    target: &str,
    checks: &[crate::perception::scoring::UiCheck],
    threshold: f32,
    workspace_root: Option<&str>,
) -> (bool, String) {
    let acting = match cycle.acting() {
        Some(a) => a,
        None => {
            return (
                false,
                "web-dev grade needs hands (no acting body) to route perception/observe — \
                 cannot observe a pure-cognition persona's UI"
                    .to_string(),
            )
        }
    };
    // Absolutize a workspace-relative target to a `file://` URL. She wrote her UI through
    // `code/write`, whose hands were rooted at `workspace_root` when the caller pinned one
    // (the from-scratch build path: a clean per-benchmark dir) — so grade the file THERE,
    // not at the core cwd. Without a pin she uses the default root (core cwd) exactly as the
    // hands do. Writer and grader MUST agree on the root or a correct artifact scores a false
    // zero (the #49 dual-root gap). An explicit URL passes through untouched.
    let target_url = if target.contains("://") {
        target.to_string()
    } else {
        let base = match workspace_root {
            Some(root) => std::path::PathBuf::from(root),
            None => match std::env::current_dir() {
                Ok(cwd) => cwd,
                Err(e) => {
                    return (
                        false,
                        format!("could not resolve workspace cwd for target '{target}': {e}"),
                    )
                }
            },
        };
        format!(
            "file://{}/{}",
            base.display(),
            target.trim_start_matches('/')
        )
    };

    // Local static artifact → the STATIC-HTML eye grades it headless. The eval core has
    // no browser eye-node connected, so routing `perception/observe` for a file:// target
    // fails loud ("no adapter to fulfil it") and a correct render scores a false zero
    // (#206). Her `ui_checks` are structural (tags/roles/text/counts) — a pure html5ever
    // parse answers them the way a browser's a11y tree would. A remote URL / live surface
    // still routes to the browser eye below (a persona's real seeing loop).
    if let Some(path) = target_url.strip_prefix("file://") {
        let obs = crate::perception::static_html::observe_file(std::path::Path::new(path));
        let grade = crate::perception::scoring::grade_ui(&obs, checks, threshold);
        return (grade.passed, grade.summary);
    }

    let params = crate::perception::ObserveParams {
        target: target_url.clone(),
        viewport: None,
        selector: None,
    };
    let ws_ctx = crate::cognition::tool_executor::ToolExecutionContext {
        persona_id: acting.persona_id,
        persona_name: acting.persona_name.clone(),
        session_id: Uuid::new_v4(),
        context_id: Uuid::new_v4(),
        caller_context: serde_json::Value::Null,
        persona_config: crate::cognition::tool_executor::PersonaMediaConfigLite {
            auto_load_media: false,
            supported_media_types: vec![],
        },
    };
    let observe_call = crate::ai::types::ToolCall {
        id: "eval-perception-observe".to_string(),
        name: "perception/observe".to_string(),
        input: serde_json::json!(params),
    };
    // Generous char bound: the ObserveResult carries the structure tree + (inline) the rendered
    // PNG, which the model-facing path would fold. Grading is not latency-critical and needs the
    // WHOLE JSON, so we don't fold it here.
    const OBSERVE_RESULT_BOUND: usize = 16_000_000;
    let out = match acting
        .executor
        .execute_native_batch(std::slice::from_ref(&observe_call), &ws_ctx, OBSERVE_RESULT_BOUND)
        .await
    {
        Ok(o) => o,
        Err(e) => {
            return (
                false,
                format!("perception/observe could not run for '{target_url}' (no eye-node adapter connected?): {e}"),
            )
        }
    };
    let content = match out.results.first() {
        Some(r) if r.is_error.is_none() => &r.content,
        Some(r) => return (false, format!("perception/observe failed for '{target_url}': {}", r.content)),
        None => return (false, format!("perception/observe returned no result for '{target_url}'")),
    };
    let obs: crate::perception::ObserveResult = match serde_json::from_str(content) {
        Ok(o) => o,
        Err(e) => {
            let preview: String = content.chars().take(400).collect();
            return (false, format!("could not parse observation for '{target_url}': {e} — got: {preview}"));
        }
    };
    let grade = crate::perception::scoring::grade_ui(&obs, checks, threshold);
    (grade.passed, grade.summary)
}

// ── Live eval progress (#123/#141, Joel: "widgets and others like yourself should
// subscribe to these events, current totals, by command, and also display the
// process — things like this take forever so you really need to provide feedback").
//
// ONE source of truth (each graded task), THREE surfaces:
//  1. watch snapshot — the substrate-canonical live surface; `cognition/eval-status`
//     with no run_id returns it (poll, works for agents/scripts today);
//  2. `eval:progress` bus event — widgets / positron (#141) / persona observers
//     subscribe (published via the global bus, same set_global precedent as the
//     persona registry);
//  3. `eval.task` probe — the greppable log pulse.
//
// ONE progress row is correct by construction: the eval-preemption lease serializes
// evals on the GPU, so at most one pass is grading at any instant.

#[derive(Debug, Clone, Serialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/cognition/EvalPassProgress.ts")]
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

/// The run_id of the pass currently grading — set at [`run_eval`] entry (RAII), read
/// by [`report_task_graded`] into the live snapshot. Correct-by-construction because
/// the eval-preemption lease serializes evals: at most one pass grades at any instant
/// (the same invariant that makes ONE progress row correct).
static CURRENT_RUN_ID: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// RAII: stamp the current pass's run_id for the live snapshot; clear on drop so it
/// rides early-return AND panic (Drop runs on unwind) — never a stale run_id bleeding
/// into the next pass or an idle board.
struct RunIdScope;
impl RunIdScope {
    fn enter(run_id: Option<String>) -> Self {
        if let Ok(mut g) = CURRENT_RUN_ID.lock() {
            *g = run_id;
        }
        RunIdScope
    }
}
impl Drop for RunIdScope {
    fn drop(&mut self) {
        if let Ok(mut g) = CURRENT_RUN_ID.lock() {
            *g = None;
        }
    }
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
fn report_task_graded(task_id: &str, ok: bool, acts: u32, latency_ms: u64, pass: u32, done: usize, total: usize) {
    // Efficiency axis: sample the live board (lock-free watch read) so every graded
    // task carries the VRAM state it ran under — the tuning signal for knobs like
    // max_acts / context budget / lane count ([[self-improvement-is-a-control-loop]]:
    // the reward is only as trustworthy as the metric, and a score without its
    // resource cost is half a metric).
    let vram_free_gb = crate::resources::ResourceDaemon::global().and_then(|d| {
        d.board()
            .kinds
            .iter()
            .find(|k| k.kind == crate::resources::ResourceKind::Vram)
            .map(|k| k.available_bytes / 1_000_000_000)
    });
    let snap = EvalPassProgress {
        done: done as u32,
        total: total as u32,
        pass,
        current_task: task_id.to_string(),
        last_ok: ok,
        updated_at_ms: crate::persona::trace::now_ms(),
        vram_free_gb,
        run_id: CURRENT_RUN_ID.lock().ok().and_then(|g| g.clone()),
    };
    let _ = eval_progress_tx().send_replace(Some(snap.clone()));
    if let Some(bus) = crate::runtime::MessageBus::global() {
        if let Ok(v) = serde_json::to_value(&snap) {
            bus.publish_async_only("eval:progress", v);
        }
    }
    crate::probe!(
        class = "eval.task",
        task = task_id,
        ok = ok,
        acts = acts,
        done = done,
        total = total,
        pass = pass,
        running_rate = format!("{:.3}", pass as f64 / done.max(1) as f64).as_str(),
        latency_ms = latency_ms,
        vram_free_gb = vram_free_gb.unwrap_or(0),
        "task graded",
    );
}

async fn run_pass(
    cycle: &crate::cognition::workspace::WorkspaceCycle,
    isolation: &crate::cognition::workspace::EvalIsolation,
    tasks: &[EvalTask],
    room: Uuid,
    max_acts: usize,
    max_retries: u32,
    // The pinned workspace root the persona's hands were rooted at (from-scratch build /
    // SWE-bench). UI grading MUST observe the artifact HERE, where she wrote it — not the
    // core cwd — or a correct render scores a false zero (#49 dual-root gap). `None` = the
    // hands used the default root (core cwd) and grading follows.
    workspace_root: Option<&str>,
) -> (u32, Vec<EvalTaskResult>) {
    let mut pass = 0u32;
    let mut results = Vec::with_capacity(tasks.len());
    // THE NATURAL PROCTORED EXAM (Joel: "we use our own persona as naturally as possible, as
    // if these are proctored exams... natural personas even in tests, with memories intact").
    // She sits down ONCE: rewind to the pre-eval frame at PASS start (each A/B arm still
    // starts from the identical memory — arm fairness preserved), clear the volatile
    // mid-thought scratch (she walks into the exam room attending to the exam), and then
    // she works the whole task sheet CONTINUOUSLY — carrying what she learns on task 3 into
    // task 4, exactly like a real student. The old per-task reset ("exam hygiene") measured
    // a rigged amnesiac who could never accumulate — the very cognition+learning the thesis
    // claims helps was structurally excluded from the measurement. If cross-task memory
    // helps, that's the system WORKING; if it distracts, that's a real cognition gap the
    // honest number should show. The exam EPISODE is dropped afterward (the fork is
    // discarded; `AdmissionState::forget_context` is the amnesia flash for live-self runs),
    // so the answer key never leaks into training.
    // [[benchmarks-are-proctored-exams-of-the-natural-living-persona]]
    cycle.reset_working_memory();
    isolation.rewind();
    for t in tasks {
        // Task-state SETUP (gym/mine tasks re-break their checkout so runs are
        // repeatable). A failed setup is a NAMED infra grade — the persona is
        // never examined against a workspace in an unknown state.
        if let Some(setup) = &t.setup_shell {
            let (setup_ok, setup_out) = run_dod(setup).await;
            if !setup_ok {
                results.push(EvalTaskResult {
                    id: t.id.clone(),
                    ok: false,
                    grade: format!("setup failed (infra, not capability): {setup_out}"),
                    acts: 0,
                    answer: String::new(),
                    latency_ms: 0,
                    output_tokens: 0,
                    tokens_per_second: 0.0,
                    decode_tokens_per_second: 0.0,
                    cache_hit_rate: 0.0,
                    prefill_ms: 0,
                    decode_ms: 0,
                });
                continue;
            }
        }
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
        // Pose the task as an EXAMINER'S REQUEST TO HER, not a bare imperative. The
        // shared [Conversational Presence] block answers "a task → answer it now" only
        // when the model RECOGNIZES the message as asking something OF IT; a terse gym
        // imperative ("Implement `fn eval`…") reads to a coder model as ambient text
        // "about" a task, so it takes the pleasantry-rest PASS hatch and scores 0
        // WITHOUT ATTEMPTING (glass-boxed 2026-07-19: hard-rs settled to Decision::Pass
        // twice with the task fully in view; humaneval — whose prompt already says
        // "Give your final answer as…" — engaged and scored 10/10). `directed=true`
        // alone no longer withholds the hatch since the directed block was softened for
        // live spiral-breaking, so the examiner must pose the ask itself. This is the
        // examiner doing its job — stating the question clearly so the exam measures
        // CAPABILITY, not whether the model happened to read a terse line as directed —
        // and it standardizes at ONE seam the imperative framing the stronger gym prompts
        // already carry inline. Verbatim task preserved; only a directed preamble added.
        let framed_prompt = format!(
            "This is a task for you to complete now. Provide your complete solution:\n\n{}",
            t.prompt.trim()
        );
        let task_delivery = crate::persona::rag_budget::RagDelivery {
            source_id: "airc".to_string(),
            items: vec![crate::persona::rag_budget::RagItem {
                content: framed_prompt,
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
        let burst = crate::cognition::workspace::Burst::from_turns(
            room,
            crate::persona::service_loop::build_workspace_turns(
                std::slice::from_ref(&task_delivery),
                "",
                "",
                // A single-task exam IS the stimulus — the task delivery is the
                // whole thread; there is no out-of-band trigger to anchor.
                None,
            ),
        );
        // `directed = true`: an exam question is put TO the persona — it is not
        // ambient room chatter she may let pass. This withholds the bare-PASS silence
        // escape (the [Conversational Presence] block) for the eval turn, the same kind of
        // measurement control as the greedy-temperature pin: it isolates the coding
        // *capability* signal from the *participation* decision. Without it a coder
        // model takes the "reply PASS, nothing reaches the room" exit on a directed
        // question (reproduced via glass-box replay — 0/13 on the gym). She can still
        // decline in her own words; she just isn't handed the silent hatch. The live
        // path computes directedness from real addressing (TODO #9); pinning it here
        // is the eval's exam-is-directed control. See `Workspace::directed_at_self`.
        // Per-task wedge watchdog (#123 hang): `drive_to_settle` awaits the persona's
        // real cognition, which POSTs to the serving gateway. If the gateway wedges
        // mid-generation (connection open, zero tokens) that await NEVER returns and the
        // WHOLE eval hangs on this one task — observed live 2026-07-17: humaneval-rs
        // stalled at task 6/20, no progress for >20 min, holding the fleet-quiesce lease
        // the entire time. A hung task MUST degrade to a graded infra-fail (identical to
        // an inference_error) and let the run proceed — never block the measurement. The
        // deadline is a generous backstop vs real per-task latency (16–50s observed on
        // humaneval; minutes across act cycles on the harder tiers), so slow-but-valid
        // work is never cut; it fires only on a true wedge. Not a latency policy.
        const PER_TASK_DEADLINE: std::time::Duration = std::time::Duration::from_secs(600);
        let settled = match tokio::time::timeout(
            PER_TASK_DEADLINE,
            crate::cognition::act_observe::drive_to_settle(
                cycle,
                burst,
                room,
                max_acts,
                crate::cognition::workspace::TurnFraming::directed(),
            ),
        )
        .await
        {
            Ok(s) => s,
            Err(_) => {
                tracing::warn!(
                    probe_class = "eval.task.timeout",
                    deadline_s = PER_TASK_DEADLINE.as_secs(),
                    "eval task exceeded the per-task deadline — grading infra-fail and \
                     advancing, NOT hanging the run (serving wedged mid-generation)"
                );
                crate::cognition::act_observe::SettleOutcome::infra_failure(format!(
                    "per-task deadline exceeded ({}s) — serving wedged mid-generation",
                    PER_TASK_DEADLINE.as_secs()
                ))
            }
        };
        let answer = settled.spoken.clone().unwrap_or_default();
        let (ok, grade) = if let Some(cause) = &settled.inference_error {
            // The model call FAILED (timeout, 5xx, a serving lane refusing a model it
            // isn't hosting) — NOT a wrong answer. Grade it a named infra failure so a
            // serving hiccup never masquerades as a capability miss and corrupts the
            // accuracy signal ([[self-improvement-is-a-control-loop]]: the reward is
            // only as trustworthy as the metric). `ok = false`, but the grade tells the
            // truth instead of a misleading "no match".
            (false, format!("inference failed: {cause}"))
        } else if !t.ui_checks.is_empty() {
            // FUNCTIONAL WEB-DEV: grade what her UI ACTUALLY RENDERED. Observe `target` through
            // her own eyes (the eye-node path) and score the element tree — the money signal for
            // "can a persona build a UI that works", judged on the structure a non-visual model
            // reads too. Default target `index.html`; default threshold 1.0 ("it works").
            let target = t.target.as_deref().unwrap_or("index.html");
            let threshold = t.ui_pass_threshold.unwrap_or(1.0);
            perception_grade(cycle, target, &t.ui_checks, threshold, workspace_root).await
        } else if let (Some(file), Some(test)) = (&t.solution_file, &t.test) {
            // ARTIFACT-graded: she was told to WRITE her solution to `file` and verify it with her
            // own tools. Grade her HANDS (the file she wrote + compiled), not her MOUTH (spoken
            // text) — the only way the act→verify loop shows up in the score. Same harness as
            // test_grade (strip her main, append test, compile, run).
            let lang = t.lang.as_deref().unwrap_or("rust");
            crate::cognition::gym_grader::test_grade_file(file, lang, test).await
        } else if let Some(dod) = &t.dod_shell {
            // REAL task: run the definition-of-done against the repo state her edits produced.
            run_dod(dod).await
        } else if let Some(test) = &t.test {
            let lang = t.lang.as_deref().unwrap_or("rust");
            test_grade(&answer, lang, test).await
        } else {
            let m =
                !t.expect.is_empty() && answer.to_lowercase().contains(&t.expect.to_lowercase());
            (m, if m { "substring match".into() } else { "no match".into() })
        };
        // NO test-only recovery wrapper here. Iterating on a failure is a PRODUCTION
        // persona behavior — she runs her own verification (a shell/compile tool) inside
        // `drive_to_settle`, sees the failure as an ordinary tool observation (threaded back
        // by the full-result fix), and fixes, all in the ONE shared act→observe loop. The
        // eval must exercise that identical path — a grader-fed retry loop that only exists
        // at eval time would measure a fiction (Joel: "you can't code the test and prod path
        // differently, or you never get a valid test"). So: one settle, then grade the
        // artifact. `max_retries` no longer drives a harness loop; `max_acts` is the real
        // knob — it bounds the persona's OWN act→observe iterations, in test and prod alike.
        let _ = max_retries;
        let total_acts = settled.acts as u32;
        if ok {
            pass += 1;
        }
        // Speed/latency of THIS task — the accumulated deliberation cost the settle
        // loop folded across every act→observe tick (the model's own measured time
        // + tokens). Reported next to the grade so accuracy and speed sit on one row.
        let m = settled.metrics;
        results.push(EvalTaskResult {
            id: t.id.clone(),
            ok,
            grade,
            acts: total_acts,
            answer: answer.chars().take(200).collect(),
            latency_ms: m.latency_ms,
            output_tokens: m.output_tokens,
            tokens_per_second: m.tokens_per_second(),
            decode_tokens_per_second: m.decode_tokens_per_second(),
            cache_hit_rate: m.cache_hit_rate(),
            prefill_ms: m.prefill_ms,
            decode_ms: m.decode_ms,
        });
        report_task_graded(&t.id, ok, total_acts, m.latency_ms, pass, results.len(), tasks.len());
    }
    (pass, results)
}

/// One deliberation on a forked cycle: deliver `prompt` through the SAME live burst formatter
/// the heartbeat uses, then drive to settlement (directed — an exam question put TO her). The
/// shared motion behind each teammate's turn in `run_pass_team`; mirrors the solo `run_pass`
/// delivery byte-for-byte so a team-vs-solo delta is coordination, not a framing difference.
/// Caller does the exam-hygiene reset/rewind (isolation is per-cycle).
async fn eval_settle(
    cycle: &crate::cognition::workspace::WorkspaceCycle,
    room: Uuid,
    prompt: &str,
    max_acts: usize,
) -> crate::cognition::act_observe::SettleOutcome {
    let delivery = crate::persona::rag_budget::RagDelivery {
        source_id: "airc".to_string(),
        items: vec![crate::persona::rag_budget::RagItem {
            content: prompt.to_string(),
            tokens: 0,
            metadata: serde_json::json!({ "peer_id": "peer", "occurred_at_ms": EVAL_EPOCH_MS }),
        }],
        tokens_used: 0,
        continuation: None,
        resolution_used: crate::persona::rag_budget::ResolutionPreference::Raw,
    };
    let burst = crate::cognition::workspace::Burst::from_turns(
        room,
        crate::persona::service_loop::build_workspace_turns(
            std::slice::from_ref(&delivery),
            "",
            "",
            None,
        ),
    );
    crate::cognition::act_observe::drive_to_settle(
        cycle,
        burst,
        room,
        max_acts,
        crate::cognition::workspace::TurnFraming::directed(),
    )
    .await
}

/// TEAM pass — the Continuum's molecule: two forks of the SAME persona (same base model) as
/// WRITER + REVIEWER. The writer solves each task; a FRESH reviewer (clean context — a genuine
/// second set of eyes, which is exactly what a solo who can't catch its own bug lacks) reviews
/// the writer's solution, finds defects, and produces the FINAL code. The reviewer's answer is
/// graded — same tasks, same grader, same model as the solo `run_pass`, so any team-vs-solo
/// delta is PURE coordination value, not model-fit. This is the undeniable proof of teams:
/// hold everything fixed but add a teammate. [[coordination-learning-flywheel]]
async fn run_pass_team(
    writer: &crate::cognition::workspace::WorkspaceCycle,
    writer_iso: &crate::cognition::workspace::EvalIsolation,
    reviewer: &crate::cognition::workspace::WorkspaceCycle,
    reviewer_iso: &crate::cognition::workspace::EvalIsolation,
    tasks: &[EvalTask],
    room: Uuid,
    max_acts: usize,
) -> (u32, Vec<EvalTaskResult>) {
    let mut pass = 0u32;
    let mut results = Vec::with_capacity(tasks.len());
    // Natural proctored exam, team edition: both teammates sit down ONCE (rewind to the
    // pre-eval frame at pass start) and then work the sheet CONTINUOUSLY — the writer carries
    // what she learned on earlier tasks; the reviewer builds a sense of the writer's habits.
    // That accumulated familiarity IS part of what a team is. Same rationale as run_pass —
    // [[benchmarks-are-proctored-exams-of-the-natural-living-persona]].
    writer.reset_working_memory();
    writer_iso.rewind();
    reviewer.reset_working_memory();
    reviewer_iso.rewind();
    for t in tasks {
        // WRITER solves (same delivery as solo run_pass).
        let w = eval_settle(writer, room, &t.prompt, max_acts).await;
        let writer_answer = w.spoken.clone().unwrap_or_default();

        // REVIEWER reviews the writer's solution and produces the FINAL code.
        // The reviewer's job is QUALITY CONTROL, not rewriting. The failure mode a glass-box
        // trace exposed: the reviewer eyeballs (acts=0), narrates a review, and "optimizes"
        // WORKING code — regressing tasks the writer got right. So the mandate is explicit:
        // verify by ACTUALLY compiling/running; leave passing code UNTOUCHED; change only a
        // proven defect; output only code. This is the coordination discipline a good human
        // reviewer has (don't break what works), made explicit for the model.
        let review_prompt = format!(
            "You are the QUALITY-CONTROL reviewer on a teammate's solution below — not a \
             rewriter. FIRST, actually COMPILE AND RUN it against the task using your tools \
             (do not just read it). If it compiles and passes, return it EXACTLY as-is — do \
             NOT optimize, refactor, or 'improve' working code; that only introduces bugs. \
             ONLY if it fails to compile or a case is wrong, fix the SPECIFIC defect and \
             nothing else. Output ONLY the final complete code — no commentary.\n\n\
             TASK:\n{}\n\nTEAMMATE'S SOLUTION:\n{}",
            t.prompt, writer_answer
        );
        let r = eval_settle(reviewer, room, &review_prompt, max_acts).await;
        let answer = r.spoken.clone().unwrap_or_default();

        // Grade the FINAL (reviewer's) answer — SAME branches as solo run_pass.
        let (ok, grade) = if let Some(cause) =
            r.inference_error.clone().or_else(|| w.inference_error.clone())
        {
            (false, format!("inference failed: {cause}"))
        } else if let Some(dod) = &t.dod_shell {
            run_dod(dod).await
        } else if let Some(test) = &t.test {
            test_grade(&answer, t.lang.as_deref().unwrap_or("rust"), test).await
        } else {
            let m = !t.expect.is_empty() && answer.to_lowercase().contains(&t.expect.to_lowercase());
            (m, if m { "substring match".into() } else { "no match".into() })
        };
        if ok {
            pass += 1;
        }
        let acts = w.acts as u32 + r.acts as u32;
        let m = r.metrics;
        results.push(EvalTaskResult {
            id: t.id.clone(),
            ok,
            grade,
            acts,
            answer: answer.chars().take(200).collect(),
            latency_ms: m.latency_ms,
            output_tokens: m.output_tokens,
            tokens_per_second: m.tokens_per_second(),
            decode_tokens_per_second: m.decode_tokens_per_second(),
            cache_hit_rate: m.cache_hit_rate(),
            prefill_ms: m.prefill_ms,
            decode_ms: m.decode_ms,
        });
        report_task_graded(&t.id, ok, acts, m.latency_ms, pass, results.len(), tasks.len());
    }
    (pass, results)
}

// Stateless → self-register onto the ONE registry (descriptor + runtime object).
crate::register_stateless_command!(CognitionEval);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the mid-run scoreboard's poll surface (#123/#141). One
    // report_task_graded call must land on the watch every reader shares — done/total/
    // pass/current task — so cognition/eval-status (no run_id) and widget bridges see
    // live totals instead of a multi-hour void. If the watch write is dropped, long
    // runs go dark again (the exact gap Joel called out mid-run 2026-07-16).
    #[test]
    fn report_task_graded_lands_on_the_progress_watch() {
        report_task_graded("HumanEval/7", true, 2, 45_000, 6, 7, 164);
        let snap = subscribe_eval_progress()
            .borrow()
            .clone()
            .expect("a graded task must publish progress");
        assert_eq!(snap.done, 7);
        assert_eq!(snap.total, 164);
        assert_eq!(snap.pass, 6);
        assert_eq!(snap.current_task, "HumanEval/7");
        assert!(snap.last_ok);
        assert!(snap.updated_at_ms > 0);
        // Bare unit test: no global authority published → honest None, never a
        // fabricated 0 ([[fallbacks-are-illegal-fail-loud]] applied to metrics).
        assert!(snap.vram_free_gb.is_none());
    }

    fn task_result(latency_ms: u64, output_tokens: u32, tps: f64) -> EvalTaskResult {
        EvalTaskResult {
            id: "t".into(),
            ok: true,
            grade: "tests passed".into(),
            acts: 0,
            answer: String::new(),
            latency_ms,
            output_tokens,
            tokens_per_second: tps,
            decode_tokens_per_second: 0.0,
            cache_hit_rate: 0.0,
            prefill_ms: 0,
            decode_ms: 0,
        }
    }

    // what this catches: LEARN mode's lesson keeps the EXPERIENCE (what she was
    // asked + how she did) while the redaction policy scrubs the held-out answer
    // key — so the exam teaches her without ever letting her memorize the crib
    // sheet. If the format changed to drop the experience, or redaction stopped
    // catching the key, "learn from exams" would either teach nothing or cheat.
    #[test]
    fn learn_mode_lesson_keeps_experience_but_scrubs_the_answer_key() {
        use crate::persona::redaction::{ExamKeyDetector, RedactionClass, RedactionPolicy};
        let task = EvalTask {
            id: "loop-file".into(),
            prompt: "Which file holds the service loop, service_loop.rs perhaps?".into(),
            expect: "service_loop.rs".into(),
            ..Default::default()
        };
        let mut result = task_result(1000, 10, 5.0);
        result.grade = "answer contained service_loop.rs".into();

        let lesson = format_exam_lesson(&task, &result);
        assert!(lesson.contains("I was asked"));
        assert!(lesson.contains("I solved it"));

        let policy = RedactionPolicy::new(vec![Box::new(ExamKeyDetector::new(
            [task.expect.clone()],
            ExamKeyDetector::DEFAULT_MIN_LEN,
        ))]);
        let (redacted, report) = policy.redact(&lesson);
        assert!(!redacted.contains("service_loop.rs"), "answer key must be scrubbed");
        assert!(report.count(RedactionClass::ExamKey) >= 1);
        assert!(redacted.contains("I was asked"), "the experience survives");
        assert!(redacted.contains("I solved it"), "the outcome survives");
    }

    // what this catches: the speed/latency aggregate math behind the scoreboard —
    // specifically the P95 index (ceil(0.95*n)-1, clamped), which is the easy place
    // to land off-by-one. For n=4, P95 is the max (idx 3); means average per-task
    // (not total/total); total_output sums. An empty set must be all-zero, never a
    // divide-by-zero. If this drifts, the latency/speed numbers a run reports — and
    // the trend ledger — silently lie.
    #[test]
    fn speed_latency_aggregates_compute_mean_p95_and_totals() {
        let empty = speed_latency_aggregates(&[]);
        assert_eq!(empty.mean_latency_ms, 0.0, "empty set → zero, never NaN");
        assert_eq!(empty.total_output_tokens, 0);
        assert_eq!(empty.total_prefill_ms, 0);

        let results = vec![
            task_result(100, 10, 50.0),
            task_result(200, 20, 40.0),
            task_result(300, 30, 30.0),
            task_result(400, 40, 20.0),
        ];
        let agg = speed_latency_aggregates(&results);
        assert_eq!(agg.mean_latency_ms, 250.0, "mean latency = (100+200+300+400)/4");
        assert_eq!(
            agg.p95_latency_ms, 400,
            "P95 of 4 tasks is the slowest (idx ceil(3.8)-1=3)"
        );
        assert_eq!(
            agg.mean_tokens_per_second, 35.0,
            "mean throughput averages per-task, not total/total"
        );
        assert_eq!(agg.total_output_tokens, 100, "total output tokens sum across the set");
    }

    // what this catches: the GPU-FIRST placement policy for the coexisting eval lane
    // (Joel: "fill GPU lanes first, ~100% utilization; CPU is spillover of last
    // resort"). The lane goes on the GPU whenever it fits in live free VRAM, and on a
    // 64GB unified-memory box a 4B base (~3GB) alongside a resident 14B (~9GB) MUST
    // land on GPU — the regression this guards is the old hardcoded `Cpu` pin that ran
    // the gym at 4 tok/s with the accelerator idle. CPU is chosen ONLY when there's no
    // GPU at all, or the GPU genuinely can't hold the lane (then it's a visible spill,
    // never a silent default).
    #[test]
    fn placement_is_gpu_first_cpu_only_when_full_or_absent() {
        const GB: u64 = 1024 * 1024 * 1024;
        // 64GB box, 14B resident → ~50GB free; 4B base ~3GB footprint → GPU.
        let (p, why) = choose_lane_placement(Some(50 * GB), Some(3 * GB));
        assert_eq!(p, LanePlacement::Gpu, "fits with room to spare: {why}");
        // GPU genuinely full (free below footprint+margin) → CPU spill, and SAID so.
        let (p, why) = choose_lane_placement(Some(2 * GB), Some(3 * GB));
        assert_eq!(p, LanePlacement::Cpu, "no headroom must spill to CPU");
        assert!(why.contains("GPU full"), "the spill must name the reason: {why}");
        // No GPU monitor on the node → CPU is the only device (honest, not a fallback).
        let (p, _) = choose_lane_placement(None, Some(3 * GB));
        assert_eq!(p, LanePlacement::Cpu, "no GPU backend → CPU is the only device");
        // Couldn't size the base → GPU-first optimism, never idle the accelerator.
        let (p, _) = choose_lane_placement(Some(50 * GB), None);
        assert_eq!(p, LanePlacement::Gpu, "unknown footprint defaults to GPU-first");
        // Exactly at the margin edge counts as fitting (>= margin).
        let (p, _) = choose_lane_placement(Some(3 * GB + GPU_PLACEMENT_MARGIN_BYTES), Some(3 * GB));
        assert_eq!(p, LanePlacement::Gpu, "free == footprint+margin fits on GPU");
    }

    // what this catches: TurnMetrics throughput + accumulation, the per-turn cost
    // the settle loop folds. tokens_per_second guards div-by-zero (0ms → 0.0, never
    // NaN/inf that would poison the mean); accumulate sums each field so a multi-act
    // task reports the TOTAL generation cost, not just the last turn's.
    #[test]
    fn turn_metrics_throughput_and_accumulation() {
        use crate::cognition::workspace::TurnMetrics;
        let zero = TurnMetrics::default();
        assert_eq!(zero.tokens_per_second(), 0.0, "0ms must not divide-by-zero");

        let mut acc = TurnMetrics {
            input_tokens: 5,
            output_tokens: 20,
            latency_ms: 1000,
            ..Default::default()
        };
        assert_eq!(acc.tokens_per_second(), 20.0, "20 tokens / 1s = 20 tok/s");
        acc.accumulate(TurnMetrics {
            input_tokens: 3,
            output_tokens: 10,
            latency_ms: 1000,
            ..Default::default()
        });
        assert_eq!(acc.output_tokens, 30, "accumulate sums completion tokens");
        assert_eq!(acc.latency_ms, 2000, "accumulate sums latency across turns");
        assert_eq!(acc.tokens_per_second(), 15.0, "30 tokens / 2s = 15 tok/s");
    }

    // what this catches: the lane prefill/decode split folds correctly and the
    // derived rates (real decode tok/s, cache-hit-rate) come off the LANE clock,
    // not the diluted wall-clock — the speed-harness numbers must not lie.
    #[test]
    fn turn_metrics_prefill_decode_split_and_rates() {
        use crate::cognition::workspace::TurnMetrics;
        let mut acc = TurnMetrics {
            output_tokens: 20,
            latency_ms: 5000, // wall-clock diluted by prefill + cognition overhead
            cached_tokens: 1500,
            prefill_tokens: 500,
            prefill_ms: 2000,
            decode_ms: 1000, // lane decoded 20 tok in 1s → 20 tok/s real
            ..Default::default()
        };
        assert_eq!(
            acc.decode_tokens_per_second(),
            20.0,
            "real decode tok/s comes off decode_ms (1s), not the 5s wall-clock"
        );
        assert!(
            (acc.cache_hit_rate() - 0.75).abs() < 1e-9,
            "1500 cached / 2000 prompt = 0.75 hit-rate"
        );
        // wall-clock tok/s is the DILUTED number (20 tok / 5s = 4) — the gap vs the
        // 20 tok/s real decode IS the prefill+overhead tax the harness surfaces.
        assert_eq!(acc.tokens_per_second(), 4.0, "wall-clock tok/s stays diluted");

        acc.accumulate(TurnMetrics {
            output_tokens: 10,
            cached_tokens: 1900,
            prefill_tokens: 100,
            prefill_ms: 500,
            decode_ms: 500,
            ..Default::default()
        });
        assert_eq!(acc.prefill_ms, 2500, "prefill_ms sums across acts");
        assert_eq!(acc.decode_ms, 1500, "decode_ms sums across acts");
        assert_eq!(acc.cached_tokens, 3400, "cached_tokens sums across acts");
        assert_eq!(acc.prefill_tokens, 600, "prefill_tokens sums across acts");
        assert_eq!(
            acc.decode_tokens_per_second(),
            20.0,
            "30 tok / 1.5s lane decode = 20 tok/s real"
        );
    }
}
