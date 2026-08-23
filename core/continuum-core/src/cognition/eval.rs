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

use crate::cognition::learning_policy::LearningPolicy;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

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
/// unfinished, when neither the caller nor the task row sets `max_acts`.
///
/// Raised 8 → 32 on 2026-08-23 after the MirrorCode baseline showed what 8
/// buys on a frontier task: Atlas spent the whole budget on a genuinely
/// competent case-bucketing analysis and was graded with an EMPTY src/ — the
/// cap measured our patience, not the model ("repeated engineering process
/// till finished … who cares if it has to cycle to do it" — the doctrine the
/// old default contradicted). Tasks that need fewer acts settle on their own;
/// the cap only exists to bound a runaway, so it must sit ABOVE what honest
/// engineering needs. Per-task rows override via `EvalTask::max_acts`.
const DEFAULT_MAX_ACTS: u32 = 32;

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
/// The eval lane's `-c` must be BOTH reproducible and honest to the model. Two failures bracket
/// this seam and the fix has to clear both:
///
/// 1. Sizing off LIVE free VRAM made `-c` swing run-to-run (placement captures 2872 ↔ 99371).
///    A different `n_ctx` changes llama.cpp's KV/tensor shapes enough that even GREEDY argmax
///    flips on borderline tokens, so the SAME task passed one run and failed the next and the
///    ABSOLUTE score meant nothing (glass-boxed 2026-07-21).
/// 2. The "fix" for (1) was `const EXAM_LANE_CTX: u32 = 16384` — which re-introduced the exact
///    anti-pattern task #124 had already deleted from this file (`EVAL_LANE_CONTEXT: u32 =
///    16_384`, see the doc block above, still describing the removal). It scored a 1M-context
///    MoE at 16k. That is not a measurement of the model; it is a measurement of the constant,
///    and it deletes the reason to run MoE at all (Joel, 2026-08-03: "these 4k or smaller
///    windows turn 1M context models into stupid numbers... eliminating all reason to have an
///    MoE"). [[never-hardcode-a-context-window-4k-defaults-destroy-the-moe-thesis]]
///
/// The resolution: the instability in (1) came from the SOURCE being live free VRAM, not from
/// deriving at all. So derive from the model's own trained window, capped by what the GOVERNED
/// host budget can actually hold — and quantize a capped result down to a power-of-two rung so
/// ordinary budget flutter cannot move it. On a host that can hold the model's full trained
/// window the exam runs at that window EXACTLY (deterministic, and as large as the model
/// deserves — 1M stays 1M). On a host that cannot, it runs at a stable rung below the fit,
/// identical across runs on that machine. No invented ceiling at any point.
///
/// Degrades honestly: ungoverned host, or a base whose GGUF can't be sized ⇒ the model's own
/// trained window (never a fresh magic cap). Fit is still decided downstream by
/// `decide_eval_lane_placement`; if the window won't fit, the lane spawn fails and the caller
/// degrades to a co-tenant SHARE rather than OOM.
fn plan_eval_lane_ctx(base: &crate::model_registry::Model) -> u32 {
    use crate::cognition::serving_plan::MIN_SERVE_CTX;
    let trained = base.context_window.max(MIN_SERVE_CTX);
    match exam_fit_window(base) {
        // The host can hold the model's whole trained window — use it exactly.
        Some(fit) if trained <= fit => trained,
        // It cannot: largest stable rung under the real fit.
        Some(fit) => stable_window_rung(fit).max(MIN_SERVE_CTX),
        None => trained,
    }
}

/// Largest window whose KV + weights + compute buffer fit the GOVERNED host budget — the same
/// memory authority `plan_serving` sizes against, never a raw GPU probe. `None` when the host is
/// ungoverned or the model can't be sized, which the caller reads as "no cap".
fn exam_fit_window(base: &crate::model_registry::Model) -> Option<u32> {
    let fp = crate::modules::serving_daemon::footprint_for(base)?;
    if fp.kv_per_token == 0 {
        return None;
    }
    let budget = crate::resources::ResourceDaemon::global()
        .map(|d| crate::modules::serving_daemon::governed_host_budget(&d).usable_bytes)
        .filter(|b| *b > 0)?;
    let kv_budget = budget
        .checked_sub(fp.weights_bytes)?
        .checked_sub(fp.compute_buffer_per_lane())?;
    Some((kv_budget / fp.kv_per_token).min(u32::MAX as u64) as u32)
}

/// Round DOWN to the nearest power of two. A quantization for run-to-run stability — a few
/// hundred MB of budget flutter must not change `n_ctx` and thus the greedy token path. It is
/// deliberately NOT a cap: it only ever applies to a value already limited by real fit.
fn stable_window_rung(window: u32) -> u32 {
    if window == 0 {
        0
    } else {
        1u32 << (u32::BITS - 1 - window.leading_zeros())
    }
}

/// The stood-up ephemeral measurement lane and everything the eval loop needs to fork a
/// cognition copy onto it. This is the SHARED inner — owned behind an `Arc` by every
/// [`EvalLane`] handle that references it, so N concurrent eval / `agent/solve` tasks on
/// the SAME base model share ONE lane instead of each cold-spawning a competitor that
/// fights the others for the GPU. The llama-server process + governed VRAM lease tear
/// down by RAII the instant the LAST handle drops — one authority, lanes lease, nothing
/// fights ([[resource-authority-is-a-system-concern]], #56).
///
/// Named fields, NOT a positional tuple, so new lane state threads as ONE field
/// ([[structs-by-reference-not-massive-param-lists]]).
pub(crate) struct EvalLaneInner {
    /// The throwaway server; kills its process on drop (#59). Declared FIRST so it drops
    /// (kills the process, frees the physical VRAM) BEFORE `_vram_lease` releases the
    /// accounting — same release order the pre-share struct guaranteed. `None` for a
    /// gateway-routed lane on an EXTERNAL provider (#310): the model is served by someone
    /// else's process (ds4 sidecar, cloud row) — there is nothing to spawn, hold, or kill.
    lane: Option<crate::inference::llama_server::EphemeralServingLane>,
    /// Adapter pinned to THIS lane (never the global serving root). `pub(crate)` so the
    /// teacher path (`genome/teach`) can generate against a DEDICATED bare-base lane —
    /// the same isolation that makes the eval score trustworthy — instead of the live
    /// multi-LoRA serving lane that OOMs the Metal backend on a real generation (#175).
    pub(crate) adapter: std::sync::Arc<dyn crate::ai::adapter::AIProviderAdapter>,
    /// The lane's REAL served `/props` window — what the fork's cognition budgets against.
    pub(crate) served_ctx: u32,
    /// Where + why the lane landed (GPU/CPU), surfaced on the eval result.
    placement: PlacementEvidence,
    /// The governed VRAM reservation this lane holds while it runs (#56/G1). RAII:
    /// released when the last handle drops, AFTER `lane`'s process is killed. `None` on a
    /// CPU-spilled lane or an ungoverned node. Held so a concurrent serving tick sees the
    /// eval's bytes as taken and won't tier up into them.
    _vram_lease: Option<crate::resources::LeaseGuard>,
}

/// A cheap, cloneable handle to a (possibly shared) [`EvalLaneInner`]. Field reads
/// (`lane.adapter`, `lane.served_ctx`, `lane.placement`) work transparently through
/// `Deref`. Dropping a handle only tears the lane down if it was the last one.
#[derive(Clone)]
pub(crate) struct EvalLane {
    inner: std::sync::Arc<EvalLaneInner>,
}

impl std::ops::Deref for EvalLane {
    type Target = EvalLaneInner;
    fn deref(&self) -> &EvalLaneInner {
        &self.inner
    }
}

/// Warm eval-lane pool: lane key (base model id / gene id) → a `Weak` to the live shared
/// lane. A concurrent caller for the same key UPGRADES the weak and shares the lane
/// instead of cold-spawning a rival. `Weak`, not strong, so the lane tears down by RAII
/// the moment its last real user drops — no lingering VRAM, no reaper task, no eviction
/// policy to get wrong. Dangling entries are pruned lazily on the next miss.
static WARM_EVAL_LANES: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, std::sync::Weak<EvalLaneInner>>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// Serializes eval-lane COLD-SPAWNS process-wide: only one llama-server cold-loads at a
/// time, so two different bases can't thrash the GPU against each other and same-base
/// racers collapse onto the first spawn's result. A `tokio` mutex because it is held
/// across the spawn `.await` — never a `std` lock across await
/// (docs/architecture/CONCURRENCY-STYLE-GUIDE.md).
static EVAL_LANE_SPAWN_GATE: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

/// Return a handle to a live warm lane for `key`, pruning a dangling entry on a miss.
fn lookup_warm_eval_lane(key: &str) -> Option<EvalLane> {
    let mut map = WARM_EVAL_LANES.lock().unwrap();
    if let Some(weak) = map.get(key) {
        if let Some(inner) = weak.upgrade() {
            return Some(EvalLane { inner });
        }
        map.remove(key); // last user already dropped it — prune the tombstone
    }
    None
}

/// Reuse a warm lane for `key` if one is live; otherwise cold-spawn it under the
/// single-flight gate and register it. `build` performs the actual cold-load and is
/// invoked ONLY on a real miss (never speculatively), so a shared lane costs zero extra
/// llama-servers. This is the seam that makes `agent/solve` + `cognition/eval` composable
/// at scale: fire N tasks on one model and they share ONE lane instead of N fighting.
async fn warm_or_spawn_eval_lane<F, Fut>(key: &str, build: F) -> Result<EvalLane, CommandError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<EvalLaneInner, CommandError>>,
{
    // Fast path: a lane is already live for this key → share it, no gate, no spawn.
    if let Some(existing) = lookup_warm_eval_lane(key) {
        return Ok(existing);
    }
    // Slow path: serialize cold-spawns so bringups never fight for the GPU.
    // The gate wait is PROBED on both sides (n8/n11 stall, 2026-08-08): a fork
    // queued behind a wedged bringup was indistinguishable from one that never
    // asked — gate_wait with no matching gate_held IS the "parked on the mutex"
    // receipt.
    crate::probe!(class = "eval.lane.acquire", step = "gate_wait", key = %key, "cold-spawn gate: waiting");
    let _gate = EVAL_LANE_SPAWN_GATE.lock().await;
    crate::probe!(class = "eval.lane.acquire", step = "gate_held", key = %key, "cold-spawn gate: held");
    // Re-check under the gate — a racer for the same key may have spawned it while we
    // waited, in which case we share theirs and skip a redundant cold-load.
    if let Some(existing) = lookup_warm_eval_lane(key) {
        return Ok(existing);
    }
    let inner = std::sync::Arc::new(build().await?);
    WARM_EVAL_LANES
        .lock()
        .unwrap()
        .insert(key.to_string(), std::sync::Arc::downgrade(&inner));
    Ok(EvalLane { inner })
}

/// Base port the ephemeral eval lane scans up from for a free one. Deliberately
/// ABOVE the default serving port (58057) so the scan never lands on — or has to
/// step over — the living persona's lane.
const EVAL_LANE_BASE_PORT: u16 = 58_200;

/// How much of a graded answer to persist in the result. A coder answer (code + reasoning)
/// fits far under this; the generous bound exists ONLY to cap a pathological loop-to-length
/// generation (which is captured up to here, still a diagnosable signal). The old 200-char
/// cap stored only the preamble and blinded every failure diagnosis + correction-corpus mine.
// context-budget-exempt: observability: how much of an answer is CAPTURED into the run ledger for humans, never text sent to a model
const ANSWER_CAPTURE_CHARS: usize = 24_000;

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

/// Small headroom (bytes) kept free on the GPU so a lane placed right at the edge
/// can't trip Metal's decode-time command-buffer OOM. Deliberately SMALL: the
/// policy is GPU-FIRST — fill the accelerator, aim for ~100% GPU utilization
/// ([[optimization-is-always-first]]; Joel: "near 100% GPU utilization if you're
/// doing it right. Fill in GPU lanes first"). This keeps us off the exact cliff;
/// it is NOT a conservative reserve that idles the GPU.
const GPU_PLACEMENT_MARGIN_BYTES: u64 = 256 * 1024 * 1024;

/// Wall-clock TTL for the eval lane's governed VRAM lease (#56/G1). Generous — a
/// cold 14B+ load plus a full benchmark run. RAII ([`crate::resources::LeaseGuard`])
/// releases the bytes the instant the lane drops; this TTL is ONLY the self-healing
/// backstop that returns the reservation to the board if the whole PROCESS is
/// SIGKILLed mid-eval without ever running `Drop` — no stranded reservation, nothing
/// static ([[memory-system-is-fully-dynamic-nothing-static]]).
const EVAL_LANE_LEASE_TTL_MS: u64 = 30 * 60 * 1000;

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
) -> (PlacementEvidence, Option<crate::resources::LeaseGuard>) {
    // ONE sizing, shared with the pre-spawn RAM gate (compression). None → couldn't
    // size (treated optimistically as GPU-first below).
    let footprint = eval_lane_footprint(base);

    // #56/G1 — ASK THE ONE GOVERNOR FOR THE SLOT, don't read raw free and race.
    // The eval lane is the top concurrent-OOM source: it stands up a SECOND
    // llama-server, and it used to place GPU-vs-CPU off `gpu::monitor::detect()`'s
    // raw free bytes — which count serving's RESERVED-but-not-yet-allocated prefill
    // buffer as "free", so the eval placed on GPU and serving's next prefill OOM'd.
    // Now it `acquire`s a real VRAM lease of its footprint from the resource
    // authority: an atomic check-and-reserve against the GOVERNED available (physical
    // free minus every other consumer's guarantee, now peak-accurate after G5).
    //   granted  → GPU, and HOLD the lease so a concurrent serving tick sees these
    //              bytes as taken and won't tier up into them
    //   refused  → the governed GPU is full → spill to CPU (VISIBLE, ~10× slower,
    //              never an OOM) — "pressure, not OOM"
    //   ungoverned node (no daemon) → the ORIGINAL raw-free probe, unchanged
    let (placement, reason, lease, free_vram) = acquire_eval_lane_slot(footprint);
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
    (
        PlacementEvidence {
            placement,
            device: device.to_string(),
            reason: reason.clone(),
            free_vram_bytes: free_vram,
            footprint_bytes: footprint,
        },
        lease,
    )
}

/// Refuse to stand up a measurement lane while the MACHINE is already drowning.
///
/// The VRAM ledger (#56/G1) accounts what the governor GRANTED — but on unified
/// memory the machine can be compressor-deep in swap from everything the ledger
/// doesn't meter (builds, apps, page compression), and the arithmetic still says
/// yes. That exact gap kernel-panicked the dev machine (2026-07-22: watchdog
/// timeout, compressed-pages 100%, 5 swapfiles) while a benchmark battery stood
/// up an eval lane. `PressureLevel`'s own contract already names the rule —
/// High: "aggressive eviction", Critical: "refuse new allocations" — and Whisper
/// honors it before loading its model; a multi-GB ELECTIVE lane must too. Veto at
/// >= High or while the sustained-pressure gate is closed. Fail-loud, never a
/// silent degrade: the detached ledger carries this exact string, and the harness
/// retries when pressure clears. Live serving is untouched — it has its own
/// shedding machinery; this gates only the deferrable measurement load.
fn refuse_eval_lane_under_memory_pressure(footprint: Option<u64>) -> Result<(), CommandError> {
    eval_lane_memory_veto(
        crate::system_resources::MemoryPressureMonitor::current_level(),
        crate::system_resources::is_memory_gate_closed(),
    )?;
    // The pressure LEVEL passed — but on unified memory a macOS "Normal" level can
    // still sit atop only a few GB of real free RAM (it counts compressible/cached
    // pages as available). Standing up a SECOND llama-server of a known footprint into
    // that is a jetsam-SIGKILL wall the level never sees (glass-boxed 2026-07-27: a 24B
    // eval lane co-resident with a 24B live lane, 9.6 GB free → exit 137, zero-byte
    // log). Size against the honest free-bytes number so we REFUSE cleanly (deferrable
    // load, retried by `await_eval_lane_memory_headroom`) instead of OOM-crashing.
    eval_lane_ram_veto(
        crate::system_resources::current_available_bytes(),
        footprint,
        EVAL_LANE_RAM_HEADROOM_BYTES,
    )
}

/// Weight-file bytes ≈ resident weight bytes; ×1.25 covers KV cache + scratch at the
/// bounded eval ctx. `None` → couldn't size (no GGUF on disk / unstatable). ONE place
/// so the pre-spawn RAM gate and the GPU/CPU placement decision size identically.
fn eval_lane_footprint(base: &crate::model_registry::Model) -> Option<u64> {
    crate::model_registry::artifacts::resolve_gguf_for_model(base)
        .and_then(|p| std::fs::metadata(&p).ok())
        .map(|m| (m.len() as f64 * 1.25) as u64)
}

/// Free physical memory kept in reserve beyond the lane's own footprint — the OS, the
/// live persona lane's in-flight buffers, and page-cache churn all need slack, and a
/// lane placed at the exact edge is the one jetsam picks. If a bigger cushion is ever
/// wanted it belongs here, one named constant, not scattered magic.
const EVAL_LANE_RAM_HEADROOM_BYTES: u64 = 2 * 1024 * 1024 * 1024; // 2 GiB

/// Whether a lane of `footprint` bytes fits in `available` free physical memory with
/// `headroom` to spare. Pure so the load-bearing "would this OOM the machine" decision
/// is unit-tested without process-global memory state. Refuses ONLY when BOTH numbers
/// are known and it genuinely won't fit — an unknown footprint or an unread free-bytes
/// reading is NOT a veto (the pressure-level gate and the placement lease remain the
/// backstops; we never refuse a lane we couldn't size, to avoid starving a node whose
/// probe is momentarily blind).
fn eval_lane_ram_veto(
    available: Option<u64>,
    footprint: Option<u64>,
    headroom: u64,
) -> Result<(), CommandError> {
    if let (Some(avail), Some(fp)) = (available, footprint) {
        let need = fp.saturating_add(headroom);
        if avail < need {
            return Err(CommandError::Internal(format!(
                "refusing to stand up an eval lane: only {avail} B free physical memory, but this \
                 lane needs ~{fp} B (+{headroom} B headroom = {need} B) — standing it up would be \
                 OOM-killed by the OS. A measurement lane is deferrable; free memory (e.g. unload a \
                 resident model) and retry."
            )));
        }
    }
    Ok(())
}

/// How long a lane bringup will WAIT for a transient pressure spike to clear
/// before failing loud. Sized for the real spikes observed live (2026-07-22): a
/// `continuum reboot` cargo build or a sibling model cold-load pushes used/total past
/// 0.90 for a couple of minutes, then the machine returns to Normal. Deferring
/// briefly turns "battery wiped out by a build" into "battery ran 3 minutes
/// later"; a SUSTAINED squeeze still fails loud at the deadline.
const EVAL_LANE_PRESSURE_WAIT: std::time::Duration = std::time::Duration::from_secs(180);

/// The deferring form of the memory veto: poll until pressure clears or the
/// bounded deadline lapses (then fail loud with the veto's own error). The
/// veto's error message says "retry when pressure clears" — for a DETACHED
/// solve/eval there is no operator watching to retry, so the system honors its
/// own advice for one bounded window. Never a silent wait: each deferred tick
/// probes, so a stall is visible in the trace, not a mystery hang.
async fn await_eval_lane_memory_headroom(footprint: Option<u64>) -> Result<(), CommandError> {
    let deadline = tokio::time::Instant::now() + EVAL_LANE_PRESSURE_WAIT;
    loop {
        let verdict = refuse_eval_lane_under_memory_pressure(footprint);
        match verdict {
            Ok(()) => return Ok(()),
            Err(e) => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(e);
                }
                // Log the REFUSAL, not just the pressure level. These are two different
                // vetoes — a High/sustained pressure gate, and a hard free-RAM shortfall
                // — and only one of them is a spike that clears on its own. Printing
                // `level=Normal` beside "waiting for a transient memory spike" while the
                // real cause was a lane that could never fit sent a reader looking at the
                // pressure monitor for 3 minutes (M5, 2026-08-04) while the answer was in
                // `e` the whole time. Whatever we are waiting OUT is the thing to name.
                crate::probe!(
                    class = "eval.lane.pressure_defer",
                    level = ?crate::system_resources::MemoryPressureMonitor::current_level(),
                    refusal = %e,
                    "eval lane bringup deferred — retrying the refusal below until it clears \
                     or the bounded window lapses"
                );
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }
}

/// The pure veto decision — split from the static reads so it's unit-testable
/// without mutating process-global pressure state (same split rationale as
/// [`acquire_eval_lane_slot`]).
fn eval_lane_memory_veto(
    level: crate::system_resources::PressureLevel,
    gate_closed: bool,
) -> Result<(), CommandError> {
    use crate::system_resources::PressureLevel;
    if level >= PressureLevel::High || gate_closed {
        return Err(CommandError::Internal(format!(
            "refusing to stand up an eval lane under system memory pressure \
             (level={level:?}, sustained_gate_closed={gate_closed}) — a measurement \
             lane is deferrable load; retry when pressure clears"
        )));
    }
    Ok(())
}

/// Ask the ONE resource authority for this eval lane's VRAM slot (#56/G1) and
/// decide GPU/CPU from the answer. Returns `(placement, reason, held-lease,
/// observed-free-vram)`. Split out so the acquire/refuse/ungoverned branching is
/// unit-testable and `decide_eval_lane_placement` stays about capturing evidence.
///
/// - governed + sized → `acquire_guarded`: Ok ⇒ GPU + hold the lease; refused ⇒ CPU
/// - governed + unsized (GGUF unstatable) → optimistic GPU, ungoverned (rare; the
///   model likely won't load anyway) — surfaced in the reason
/// - ungoverned node (no `ResourceDaemon::global()`) → the original raw-free probe
fn acquire_eval_lane_slot(
    footprint: Option<u64>,
) -> (
    LanePlacement,
    String,
    Option<crate::resources::LeaseGuard>,
    Option<u64>,
) {
    use crate::resources::{LeaseError, LeaseRequest, ReclaimPolicy, ResourceDaemon, ResourceKind};
    match (ResourceDaemon::global(), footprint) {
        (Some(daemon), Some(fp)) => {
            let req = LeaseRequest {
                consumer_id: "eval-lane".to_string(),
                kind: ResourceKind::Vram,
                bytes: fp,
                ttl_ms: EVAL_LANE_LEASE_TTL_MS,
                // A bounded, first-class measurement lane is not yanked mid-eval
                // ([[first-class-citizens-even-during-benchmarks]]); the RAII guard
                // returns the bytes the moment the lane finishes. Graceful yield-
                // under-pressure (the eval negotiating early release) is the piece-3
                // follow-up, not this slice.
                reclaim_policy: ReclaimPolicy::Pinned,
            };
            match daemon.acquire_guarded(&req) {
                Ok(guard) => {
                    let remaining = governed_vram_available();
                    (
                        LanePlacement::Gpu,
                        "GPU-first: governor granted a VRAM lease for this lane".to_string(),
                        Some(guard),
                        remaining,
                    )
                }
                Err(LeaseError::InsufficientCapacity { available, .. }) => (
                    LanePlacement::Cpu,
                    format!(
                        "GPU full (governed): available {available}B < lane footprint {fp}B \
                         — spilling this lane to CPU",
                    ),
                    None,
                    Some(available),
                ),
                Err(e) => (
                    // Any non-capacity lease error is unexpected. Fail SAFE to CPU
                    // (never place on GPU ungoverned behind the governor's back) and
                    // say why — a slow CPU lane is recoverable, a blind GPU OOM is not.
                    LanePlacement::Cpu,
                    format!("governor lease error ({e:?}) — spilling to CPU rather than placing ungoverned"),
                    None,
                    None,
                ),
            }
        }
        (Some(_), None) => (
            LanePlacement::Gpu,
            "GPU-first: lane footprint unknown, offloading to GPU (ungoverned — could not size to lease)"
                .to_string(),
            None,
            None,
        ),
        (None, _) => {
            // Ungoverned node: the ORIGINAL raw-free probe, behavior unchanged.
            // `and_then`, not `map`: "no GPU" and "GPU with no reading yet" are
            // the same answer to a placement question — unknown — and
            // `choose_lane_placement` already models unknown as `None`.
            let free_vram = crate::gpu::monitor::detect().and_then(|m| m.free_bytes());
            let (placement, reason) = choose_lane_placement(free_vram, footprint);
            (placement, reason.to_string(), None, free_vram)
        }
    }
}

/// The governed VRAM `available` right now (the board's Vram lease-headroom), or
/// `None` when VRAM is ungoverned. Local read for placement evidence; mirrors the
/// serving daemon's `governed_vram_ceiling` (same board field, read-only).
fn governed_vram_available() -> Option<u64> {
    crate::resources::ResourceDaemon::global().and_then(|d| {
        d.board()
            .kinds
            .iter()
            .find(|k| k.kind == crate::resources::ResourceKind::Vram)
            .map(|k| k.available_bytes)
    })
}

/// Stand up an EPHEMERAL serving lane on the gene's OWN forged base (with the gene
/// loadable via `--lora`) plus an adapter pointed at it, so the genome A/B is scored
/// on the base the gene was trained against — never the larger model the living
/// persona happens to be served on. The living persona's lane is untouched (#59);
/// the returned lane kills its server on drop. Fails loud (never a substitute base,
/// never a silent skip) at every missing precondition: gene not in the manifest,
/// base not in the registry, or the lane not coming up.
async fn spawn_gene_eval_lane(gene: &EvalGene) -> Result<EvalLane, CommandError> {
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

    // Size THIS lane, then gate on real free RAM — refuse cleanly if it wouldn't fit
    // (a jetsam SIGKILL otherwise), before paying the cold-load.
    await_eval_lane_memory_headroom(eval_lane_footprint(&base)).await?;

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
    let (placement_evidence, vram_lease) =
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
        expert_placement: None, // eval lanes run the whole model; no K3 expert paging
        resident_override: None, // eval lanes serve resident as-shipped; no device-fit override
    };
    emit_eval_phase(
        "loading_lane",
        &format!("cold-loading gene eval lane ({})", gene.name),
    );
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
    adapter.initialize().await.map_err(|e| {
        CommandError::Internal(format!("eval-lane adapter failed to initialize: {e}"))
    })?;

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

    // A gene lane is per-forged-adapter; it stays a fresh dedicated lane (no pooling) —
    // the shared inner just makes it a handle so both lane kinds are one type.
    Ok(EvalLane {
        inner: std::sync::Arc::new(EvalLaneInner {
            lane: Some(lane),
            adapter: std::sync::Arc::new(adapter),
            served_ctx,
            placement: placement_evidence,
            _vram_lease: vram_lease,
        }),
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
/// Stand up (or REUSE) an ephemeral base-model measurement lane. Concurrent callers for
/// the same `base_id` share ONE warm lane via [`warm_or_spawn_eval_lane`] instead of
/// cold-spawning rivals that fight for the GPU — the fix that makes an at-scale benchmark
/// battery (`agent/solve` × N tasks) reliable instead of a lane-thrash lottery.
pub(crate) async fn spawn_base_eval_lane(base_id: &str) -> Result<EvalLane, CommandError> {
    warm_or_spawn_eval_lane(base_id, || build_base_eval_lane_inner(base_id)).await
}

/// The actual cold-load for a bare base lane — invoked ONLY on a warm-pool miss.
async fn build_base_eval_lane_inner(base_id: &str) -> Result<EvalLaneInner, CommandError> {
    use crate::ai::adapter::AIProviderAdapter;
    use crate::inference::llama_server::{EphemeralServingLane, ServingTarget, PROVIDER_ID};

    let base = crate::model_registry::try_global()
        .and_then(|r| r.model(base_id).cloned())
        .ok_or_else(|| {
            CommandError::NotFound(format!(
                "base_model_id '{base_id}' is not in the model registry — cannot stand up a measurement lane for it. Call ai/inference/models for loadable ids."
            ))
        })?;
    // External-provider rows (ds4 sidecar, cloud models) are served by someone else's
    // process — they are NEVER lane-spawnable here. Route the measurement through the
    // registered provider adapter instead of cold-loading a llama-server for weights
    // this box may not even hold. (#310: the live failure was this path trying to stand
    // up an ~81GB ephemeral lane for deepseek-v4-flash while the ds4 sidecar was already
    // answering on :8901 — the memory veto then rightly refused a lane that should never
    // have been asked for.)
    if base.provider != PROVIDER_ID {
        return build_external_eval_lane_inner(&base).await;
    }
    // The LOCAL live lane may already hold exactly these weights. Same principle as the
    // external route above and the same failure when missed: a model that is already
    // being served must never be cold-loaded a second time.
    if let Some(shared) = share_live_serving_lane(&base).await {
        return Ok(shared);
    }
    // Size THIS lane, then gate on real free RAM — refuse cleanly if it wouldn't fit
    // (a jetsam SIGKILL otherwise), before paying the cold-load.
    await_eval_lane_memory_headroom(eval_lane_footprint(&base)).await?;
    let lane_ctx = plan_eval_lane_ctx(&base);
    let (placement_evidence, vram_lease) =
        decide_eval_lane_placement(&base, lane_ctx, &format!("eval-lane base:{base_id}"));
    let target = ServingTarget {
        model: base.clone(),
        context_window: lane_ctx,
        lanes: 1,
        adapters: vec![], // bare base — no gene
        placement: placement_evidence.placement,
        expert_placement: None, // eval lanes run the whole model; no K3 expert paging
        resident_override: None, // eval lanes serve resident as-shipped; no device-fit override
    };
    // The ephemeral lane cold-loads the base model (can be minutes for a 14B+); emit
    // the phase so positronic layers show "loading <model>…", not a frozen bar.
    emit_eval_phase(
        "loading_lane",
        &format!("cold-loading eval lane for {base_id}"),
    );
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
    adapter.initialize().await.map_err(|e| {
        CommandError::Internal(format!("eval-lane adapter failed to initialize: {e}"))
    })?;
    let served_ctx = lane.served_context_window().await.map_err(|e| {
        CommandError::Internal(format!(
            "eval lane for base '{base_id}' is up but its /props served window is unreadable ({e})"
        ))
    })?;
    Ok(EvalLaneInner {
        lane: Some(lane),
        adapter: std::sync::Arc::new(adapter),
        served_ctx,
        placement: placement_evidence,
        _vram_lease: vram_lease,
    })
}

/// Measurement "lane" for a base the LIVE serving lane is ALREADY holding — the local
/// sibling of the external-provider route below (#310). Returns `None`, falling through
/// to a dedicated cold-load, whenever sharing would not be honest.
///
/// Why this exists (glass-boxed on the M5, 2026-08-04). A SWE-bench `agent/solve` run
/// asked for `unsloth/Devstral-Small-2507-GGUF` — the exact model the live lane was
/// already serving, 27.2 GB resident, answering on its port. This path tried to
/// cold-load a SECOND copy: ~17.9 GB + 2 GiB headroom against 15.0 GB free on a 64 GB
/// box. It can never fit, so `await_eval_lane_memory_headroom` deferred every 5 s for
/// its full 180 s window and then failed loud. The harness wrote a zero-byte diff and
/// the benchmark reported RESOLVED=0 — a number that measured a harness which never
/// started, not a model that could not solve. Duplicating resident weights is not a
/// transient spike to wait out; it is a request that should never have been made.
///
/// Isolation (#59/#312) is preserved, not traded away. What a measurement needs
/// isolated is the GENOME and the WINDOW, never the weights — identical weights are
/// identical weights. So the share is refused unless every resident adapter is at
/// scale 0, asked of the running server rather than assumed (the M5 lane carries six
/// loaded-but-unapplied genome layers — inert here, but that is a fact to VERIFY, and
/// a lane serving an applied genome must never be mistaken for a bare base). The
/// returned handle owns nothing (`lane: None`, no lease), so dropping a measurement can
/// never tear down the living persona's lane.
async fn share_live_serving_lane(base: &crate::model_registry::Model) -> Option<EvalLaneInner> {
    use crate::ai::adapter::AIProviderAdapter;
    use crate::inference::llama_server::PROVIDER_ID;

    let snap = crate::inference::llama_server::current_serving();
    // `served_context_window == 0` only ever appears on the empty/not-yet-served
    // snapshot; a lane with no known window cannot be budgeted against honestly.
    if !snap.ready
        || snap.active_model.as_deref() != Some(base.id.as_str())
        || snap.served_context_window == 0
    {
        return None;
    }
    // NO genome probe here, deliberately. Genome activation on this lane is PER REQUEST:
    // the daemon loads the `--lora` catalog and immediately zeroes every global scale
    // (`llama_server::zero_adapter_scales` — "catalog LOADED, scales DORMANT (0.0),
    // per-request activation only"), and a turn pages its gene in through the request
    // body's `lora: [{id, scale}]` field. This adapter is built without that field, so
    // what it measures is the bare base — whatever else is in the catalog and whoever else
    // is using the lane.
    //
    // The probe that used to be here asked `/lora-adapters` for the GLOBAL scales, which
    // by that same design are always 0.0. It could therefore only ever answer "inert" or
    // fail — it carried no information. What it did do is fail: `/lora-adapters` BLOCKS
    // while the lane is generating (measured on the M5, 2026-08-04: `/health` 200 in
    // milliseconds, `/lora-adapters` no response in 5s on a busy lane), and the timeout
    // was rendered as the definite claim "has an APPLIED genome layer". So the share was
    // refused exactly when the lane was in use — i.e. whenever sharing mattered — and the
    // fallback cold-loaded a SECOND 17.9 GB copy that missed the GPU by 126 MB and spilled
    // to CPU, turning a measurement into a run that could never finish.
    //
    // Blind must not mean "assume clean" — that principle was right. The error was
    // treating an unanswered HTTP call as evidence when the invariant was already
    // guaranteed upstream by the code that applies the adapters.
    let mut adapter =
        crate::ai::openai_adapter::OpenAICompatibleAdapter::from_registry(PROVIDER_ID)
            .with_runtime_base_url(snap.base_url.clone())
            .with_default_model(base.id.clone());
    // BOUNDED: this initialize is an HTTP round-trip against the LIVE lane, and this
    // server is documented (the /lora-adapters lesson above) to block non-completion
    // endpoints while generating. A share CHECK must never park acquisition — if the
    // busy lane can't answer briefly, fall through to the cold path LOUDLY instead of
    // hanging the solve prelude (n8/n11, 2026-08-08: 2h+ parked with three candidate
    // parks and this round-trip among them).
    const SHARE_CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
    match tokio::time::timeout(SHARE_CHECK_TIMEOUT, adapter.initialize()).await {
        Ok(Ok(())) => {}
        Ok(Err(_)) => return None,
        Err(_) => {
            crate::probe!(
                class = "eval.lane.acquire",
                step = "share_check_timeout",
                model = %base.id,
                "live-lane share check blocked past its bound (busy lane) — falling to cold path"
            );
            return None;
        }
    }

    emit_eval_phase(
        "loading_lane",
        &format!(
            "sharing the live serving lane for {} — weights already resident",
            base.id
        ),
    );
    crate::probe!(
        class = "eval.lane.shared",
        model = %base.id,
        base_url = %snap.base_url,
        served_ctx = snap.served_context_window,
        "measuring through the live lane — no second copy of these weights cold-loaded"
    );
    Some(EvalLaneInner {
        // Nothing spawned here, so nothing to kill on drop — the living persona's lane
        // outlives every measurement that borrows it.
        lane: None,
        adapter: std::sync::Arc::new(adapter),
        // The live lane's own `/props` truth, the same authority the living persona
        // budgets against (task #50) — never a recomputed plan value.
        served_ctx: snap.served_context_window,
        placement: PlacementEvidence {
            placement: crate::inference::llama_server::LanePlacement::Cpu,
            device: "shared:live-serving-lane".to_string(),
            reason: format!(
                "model '{}' is already resident on the live lane at {} — shared, no lane \
                 spawned and no VRAM leased",
                base.id, snap.base_url
            ),
            free_vram_bytes: None,
            footprint_bytes: None,
        },
        _vram_lease: None,
    })
}

/// Measurement "lane" for a model served by an EXTERNAL provider (#310): the ds4
/// sidecar, a cloud row — any registry model whose `provider` is not the local
/// llama-server. Nothing is spawned and no VRAM is leased; the fork's generation is
/// routed through an adapter built from the provider row, exactly how the inference
/// gateway reaches the same provider. The memory-headroom gate is deliberately NOT
/// consulted — it prices a local cold-load this path never pays.
async fn build_external_eval_lane_inner(
    base: &crate::model_registry::Model,
) -> Result<EvalLaneInner, CommandError> {
    use crate::ai::adapter::AIProviderAdapter;

    let base_url = crate::model_registry::try_global()
        .and_then(|r| r.provider(&base.provider).map(|p| p.base_url.clone()))
        .ok_or_else(|| {
            CommandError::Internal(format!(
                "model '{}' names provider '{}' but the registry has no such provider row",
                base.id, base.provider
            ))
        })?;
    emit_eval_phase(
        "loading_lane",
        &format!(
            "routing eval through external provider '{}' for {}",
            base.provider, base.id
        ),
    );
    let mut adapter =
        crate::ai::openai_adapter::OpenAICompatibleAdapter::from_registry(&base.provider)
            .with_default_model(base.id.clone());
    adapter.initialize().await.map_err(|e| {
        CommandError::Internal(format!(
            "external provider '{}' failed to initialize for eval (is it running at {base_url}?): {e}",
            base.provider
        ))
    })?;
    // The provider's live /v1/models context_length is the serving truth (the ds4
    // sidecar serves 8192 while the catalog row states the model's 1M capability);
    // budget the fork against what the server will actually accept. Row ctx is the
    // fallback when the provider doesn't report one.
    let served_ctx = external_served_context_window(&base_url, &base.id)
        .await
        .unwrap_or(base.context_window);
    Ok(EvalLaneInner {
        lane: None,
        adapter: std::sync::Arc::new(adapter),
        served_ctx,
        // `LanePlacement` drives llama-server spawn flags, which never apply here;
        // `Cpu` carries the honest zero-VRAM-contention semantics, and the device
        // string names the real host.
        placement: PlacementEvidence {
            placement: crate::inference::llama_server::LanePlacement::Cpu,
            device: format!("external:{}", base.provider),
            reason: format!(
                "model is served by external provider '{}' at {base_url} — no lane spawned, no VRAM leased",
                base.provider
            ),
            free_vram_bytes: None,
            footprint_bytes: None,
        },
        _vram_lease: None,
    })
}

/// #312 — the exam's disposable world: a CoW clone of the shared checkout under the
/// system temp dir, one per run. clonefile on APFS / reflink on btrfs-XFS (instant,
/// block-sharing), plain recursive copy where the FS lacks CoW — the same platform
/// split as citizen-layer provisioning (`ensure_citizen_layer_from_base`). `cfg!`
/// (not `#[cfg]`) so both arms type-check on every platform.
fn provision_ephemeral_eval_root(tag: &str) -> Result<std::path::PathBuf, String> {
    let base = std::env::current_dir().map_err(|e| format!("cwd: {e}"))?;
    let root = std::env::temp_dir().join("continuum-eval").join(tag);
    let parent = root
        .parent()
        .ok_or_else(|| "eval root has no parent".to_string())?
        .to_path_buf();
    sweep_orphan_eval_roots(&parent, &root);
    if root.is_dir() {
        write_eval_run_marker(&root, tag); // resume re-stamps the owner pid (a new core owns it now)
        return Ok(root); // resume of the same run reuses its world
    }
    std::fs::create_dir_all(&parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    let mut clone = std::process::Command::new("cp");
    if cfg!(target_os = "macos") {
        clone.arg("-cR");
    } else {
        clone.args(["--reflink=auto", "-R"]);
    }
    let out = clone
        .arg(&base)
        .arg(&root)
        .output()
        .map_err(|e| format!("cp spawn: {e}"))?;
    if !out.status.success() {
        let _ = std::fs::remove_dir_all(&root); // never leave a half-clone
        return Err(format!(
            "CoW clone {} -> {} failed: {}",
            base.display(),
            root.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    crate::probe!(
        class = "eval.workspace.provisioned",
        root = root.display().to_string().as_str(),
        "exam world provisioned — CoW clone of the shared checkout, removed when the run ends"
    );
    write_eval_run_marker(&root, tag);
    Ok(root)
}

/// Marker filename inside each eval world naming the run and its owning core
/// pid — the cross-process evidence `continuum reboot`'s guard reads. Solve
/// runs got this day one (the grade-ledger `state: running` rows); eval runs
/// had NO on-disk in-flight signal, so a reboot killed them without ever being
/// asked (measured 2026-08-23: the guard named zero runs while a MirrorCode
/// battery was mid-task). The world dir dies with the run (Drop) and orphans
/// are swept at next provision, so marker lifetime is exactly run lifetime;
/// the guard pid-checks to ignore debris from a killed core.
pub const EVAL_RUN_MARKER: &str = "eval-run.json";

fn write_eval_run_marker(root: &std::path::Path, run_id: &str) {
    let marker = serde_json::json!({
        "runId": run_id,
        "corePid": std::process::id(),
        "startedAtMs": crate::persona::trace::now_ms(),
    });
    // Marker write is best-effort: the run must not die because its guard
    // breadcrumb could not be written; the reboot guard just loses this run.
    if let Err(e) = std::fs::write(
        root.join(EVAL_RUN_MARKER),
        marker.to_string(), // disk marker read by the CLI process — a true process boundary
    ) {
        tracing::warn!(error = %e, "eval run marker not written — reboot guard cannot see this run");
    }
}

/// Every eval run some world dir claims is IN FLIGHT: `(run_id, core_pid)`.
/// The CALLER owns pid-liveness (the CLI has its own pid_alive; this crate-side
/// scan stays pure file reading) — a marker whose core died is debris the next
/// provision sweeps, not an in-flight run.
pub fn in_flight_eval_runs() -> Vec<(String, u32)> {
    let base = std::env::temp_dir().join("continuum-eval");
    let Ok(entries) = std::fs::read_dir(&base) else {
        return Vec::new();
    };
    let mut live = Vec::new();
    for entry in entries.flatten() {
        let marker = entry.path().join(EVAL_RUN_MARKER);
        let Ok(text) = std::fs::read_to_string(&marker) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let (Some(run), Some(pid)) = (
            v.get("runId").and_then(|r| r.as_str()),
            v.get("corePid").and_then(|p| p.as_u64()),
        ) else {
            continue;
        };
        live.push((run.to_string(), pid as u32));
    }
    live.sort();
    live
}

/// Eval worlds LIVE in this process. `Drop` covers every in-process return path,
/// but not a killed process: `continuum reboot` SIGTERMs the core, `Drop` never
/// runs, and the run's world outlives it (measured 2026-08-23 — a reboot mid-run
/// left a 31 GB clone under `$TMPDIR/continuum-eval`, found by hand during the
/// Ornith battery). The sweep below deletes those orphans at the next provision;
/// this set is what stops it deleting a CONCURRENT run's live world.
static LIVE_EVAL_ROOTS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashSet<std::path::PathBuf>>,
> = std::sync::OnceLock::new();

fn live_eval_roots() -> &'static std::sync::Mutex<std::collections::HashSet<std::path::PathBuf>> {
    LIVE_EVAL_ROOTS.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
}

/// Remove sibling eval worlds that belong to no live run in this process. An
/// eval run cannot survive its process (the mind fork, the lane lease, and the
/// progress reporter all die with it), so any world under `continuum-eval/`
/// that is neither the run being provisioned nor in [`live_eval_roots`] is an
/// orphan from a killed process — re-creatable debris by construction (#312:
/// the world is a CoW clone of the checkout; nothing in it is the only copy).
/// Event-driven at the point of use — no boot hook, no background tick.
fn sweep_orphan_eval_roots(parent: &std::path::Path, keep: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return; // parent not created yet — nothing to sweep
    };
    // Snapshot the live set and release the lock BEFORE deleting: a multi-GB
    // clone takes seconds to remove, and a concurrent provision must not queue
    // behind filesystem work.
    let live = match live_eval_roots().lock() {
        Ok(guard) => guard.clone(),
        Err(_) => return, // poisoned registry: skip the sweep, never guess at liveness
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || path == keep || live.contains(&path) {
            continue;
        }
        match std::fs::remove_dir_all(&path) {
            Ok(()) => crate::probe!(
                class = "eval.workspace.orphan_swept",
                root = path.display().to_string().as_str(),
                "orphaned exam world from a killed process removed — Drop cannot run \
                 through SIGTERM, so provision-time sweep is the crash-path owner"
            ),
            Err(e) => tracing::warn!(
                root = %path.display(),
                error = %e,
                "orphaned eval world could not be removed — temp-dir debris persists"
            ),
        }
    }
}

/// RAII holder for the run's disposable world — `Drop` removes it on EVERY return
/// path (score, infra-abort, error), so a crashed exam never leaks fixtures into
/// the temp dir long-term and NEVER touches the shared checkout at all. The
/// crash path Drop cannot cover (SIGKILL/SIGTERM) is owned by
/// [`sweep_orphan_eval_roots`] at the next provision.
struct EphemeralEvalRoot(std::path::PathBuf);

impl EphemeralEvalRoot {
    fn new(root: std::path::PathBuf) -> Self {
        if let Ok(mut live) = live_eval_roots().lock() {
            live.insert(root.clone());
        }
        Self(root)
    }
}

impl Drop for EphemeralEvalRoot {
    fn drop(&mut self) {
        if let Ok(mut live) = live_eval_roots().lock() {
            live.remove(&self.0);
        }
        if let Err(e) = std::fs::remove_dir_all(&self.0) {
            tracing::warn!(
                root = %self.0.display(),
                error = %e,
                "ephemeral eval workspace cleanup failed — temp-dir debris only, shared checkout unaffected"
            );
        }
    }
}

/// Ask an OpenAI-compatible provider what context window it ACTUALLY serves for
/// `model_id` (`/v1/models` `context_length`). `None` when unreachable or unreported —
/// callers fall back to the catalog row.
async fn external_served_context_window(base_url: &str, model_id: &str) -> Option<u32> {
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let resp = reqwest::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
        .ok()?;
    let body: serde_json::Value = resp.json().await.ok()?;
    parse_provider_context_length(&body, model_id)
}

/// Pure `/v1/models` → `context_length` extraction, split from the fetch so the
/// provider-shape contract is unit-testable without a live sidecar.
fn parse_provider_context_length(body: &serde_json::Value, model_id: &str) -> Option<u32> {
    body.get("data")?
        .as_array()?
        .iter()
        .find(|m| m.get("id").and_then(|i| i.as_str()) == Some(model_id))?
        .get("context_length")?
        .as_u64()
        .map(|n| n as u32)
}

/// One eval task. Both the JSONL rows and inline `tasks` deserialize into this;
/// every field is optional so an authoring typo degrades to a benign empty rather
/// than failing the whole run. A task is TEST-GRADED when it carries `test`, else
/// substring-graded against `expect`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
// Exported to the canonical protocol root (#247/#80) so the parents that embed it
// (GenomeTeachParams / MinedTask / RedactMemoryParams — all already emitting under
// protocol/typescript/) import a SIBLING instead of reaching back into the legacy
// crate bindings/ dir. A derive without an export once left this file a silently
// rotting orphan; the explicit export is the fix, the canonical path is #424.
#[ts(export, export_to = "../../../protocol/typescript/cognition/EvalTask.ts")]
pub struct EvalTask {
    /// Stable id for the task (echoed in results so a regression is identifiable).
    #[serde(default)]
    pub id: String,
    /// Per-task act→observe budget override — budget-as-data, so a gym can size
    /// patience to its task class (a mirror task needs analysis + implementation
    /// + verification; a one-liner doesn't). `None` inherits the run's budget.
    #[serde(default)]
    #[ts(optional)]
    pub max_acts: Option<u32>,
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
    #[serde(
        default,
        alias = "solutionFile",
        skip_serializing_if = "Option::is_none"
    )]
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
    #[serde(
        default,
        alias = "uiPassThreshold",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub ui_pass_threshold: Option<f32>,
    /// Does answering this task REQUIRE tools — regardless of how it's graded? The derived
    /// default (see [`needs_tools`](Self::needs_tools)) keys off the GRADING modality: a task
    /// whose grade reads a file she wrote (`solution_file`), a workspace DoD (`dod_shell`), a
    /// pinned repo (`workspace_root`), or a rendered UI (`ui_checks`) obviously needs hands.
    /// But a MOUTH-graded task (`expect`/`test`) can ALSO require tools to PRODUCE the answer —
    /// a repo-navigation exam ("Which file defines `fn build_workspace_burst`?") is graded by a
    /// spoken substring yet is unanswerable without `code/search`/`code/read`. The derived
    /// signal misses that, so the whole gym runs speak-only and scores a SILENT 0 for every
    /// model (#208, coder-eval). This EXPLICIT declaration overrides the default: `Some(true)`
    /// forces tools on for the run; `Some(false)` pins a task speak-only even if it looks
    /// hands-graded. `None` (the common case) falls back to the grading-modality derivation.
    #[serde(default, alias = "needsTools", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub needs_tools: Option<bool>,
    /// The checkout THIS task lives in — her hands get rooted here before the task runs.
    ///
    /// The run-level `workspace_root` param pins ONE repo for a whole exam, which is right for a
    /// SWE-bench instance (one clone, one task). It is wrong for a MINED gym: `gym/mine` gives
    /// every task its own git worktree so tasks are independent and repeatable, so the root is a
    /// property of the TASK. Without this the evaluator hands her one root for all N tasks — the
    /// wrong one for every task — and she is told to fix a bug in a directory her sandboxed tools
    /// cannot reach. That is not a hard exam, it is an unrunnable one, and it is why mined gyms
    /// had never produced a number.
    ///
    /// It is DECLARED here, not applied mid-run. Rooting a persona is two operations — her hands
    /// (`root_acting_workspace`, callable any time) and her eyes (`repoint_workspace_map_if_pinned`,
    /// fork-time only, because the cycle's faculties are immutable once built). Applying just the
    /// first splits her: hands in one tree, map of another, which is the #206 shape and produces a
    /// zero that lies about the solver with no probe to show it. So a task whose root differs from
    /// the run's is REFUSED with a named infra grade telling the caller to run one eval per root —
    /// the run-level pin does both halves at fork, so that path is correct today. When the map
    /// derives its root from her hands (one source of truth), this becomes a live re-root and the
    /// refusal is deleted.
    /// [[re-rooting-a-persona-is-two-operations-moving-one-is-worse-than-moving-neither]]
    #[serde(
        default,
        alias = "workspaceRoot",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub workspace_root: Option<String>,
}

impl EvalTask {
    /// Is this task GRADED ON THE WORKSPACE (a DoD shell reads files, a
    /// solution file is collected, UI checks screenshot artifacts) rather than
    /// on her spoken answer? Decides the turn's [`TurnFraming::on_workspace`]
    /// contract — which in turn arms the discovery-saturation gate, the
    /// no-deliverable nudge, and the act-budget proprioception. Before this
    /// predicate the eval framed EVERY task as directed-speech, so none of
    /// those protections ever applied to the tasks they were built for
    /// (glass-boxed 2026-08-23: MirrorCode graded an empty src/ after a
    /// discovery-only turn the saturation gate should have interrupted).
    pub fn workspace_deliverable(&self) -> bool {
        self.dod_shell.is_some() || self.solution_file.is_some() || !self.ui_checks.is_empty()
    }
}

impl EvalTask {
    /// Whether answering THIS task requires the persona's hands. An explicit
    /// [`needs_tools`](Self::needs_tools) declaration wins; otherwise it's derived from the
    /// GRADING modality — a grade that reads a written file / DoD / rendered UI needs hands.
    /// The run-level decision is `any task needs tools` (tools are offered to the whole run),
    /// so a mixed gym with one repo-nav task correctly arms tools for all. See #208.
    pub fn needs_tools(&self) -> bool {
        self.needs_tools.unwrap_or_else(|| {
            self.solution_file.is_some()
                || self.dod_shell.is_some()
                || self.workspace_root.is_some()
                || !self.ui_checks.is_empty()
        })
    }

    /// CODE IS BUILT, NEVER SPOKEN. A task carrying a compile-and-run `test` grades a PROGRAM,
    /// and a program's deliverable is a FILE. If the task didn't name one, name it here — the
    /// grade then reads what her hands wrote, and the spoken-code payout stops existing.
    ///
    /// Joel, 2026-08-04, on why this is not merely a measurement fix: *"any benchmark, at least
    /// for code, that has them speak code is actually more of a problem for learning persona with
    /// minds. It's going to train them to think speak code (or anything that needs an action) is
    /// building projects. That's exactly the opposite of our goals for first class citizens."*
    ///
    /// That is the load-bearing argument. Our graded episodes do not stop at a scoreboard — they
    /// enter the SAME experience loop as lived ones (engram → sentinel → curriculum → genome,
    /// [[one-experience-loop-benchmark-lessons-are-engrams-dream-sentinels-train-them]]). So a
    /// grader that pays out for a fence does not just mis-measure the fence→act gap, it TEACHES
    /// it, and the layer promoted on that reward carries "describing is doing" into every room —
    /// not just the exam. 404 of 458 gym tasks paid out that way when this was written.
    ///
    /// Doing it at LOAD is the compression: the alternative already existed as
    /// `hard-rs-acted.jsonl`, a hand-forked twin of `hard-rs.jsonl` differing only by this
    /// preamble and this filename. One rule in the loader replaces N forked corpora that would
    /// each drift. A gym author still WINS by naming `solution_file` (and its prompt) explicitly;
    /// this only ensures no code task can be graded by mouth because someone forgot.
    ///
    /// `expect`-graded knowledge tasks are untouched — answering a question IS speaking, and
    /// nothing is confused about that. The rule keys on `test`: only a task that compiles and
    /// runs code has a program as its deliverable.
    ///
    /// `pub(crate)` because THIS is the one derivation of a code task's artifact name:
    /// `benchmark/dispatch` must run the same rule before composing a work card, or the card
    /// tells the citizen nothing about WHERE to write while the grade reads the derived path
    /// (glass-boxed 2026-08-15: a frontier-rs claimer inventing `swe/benchmarks/.../lib.rs`).
    pub(crate) fn require_hands_for_code(&mut self) {
        if self.test.is_none()
            || self.solution_file.is_some()
            || self.dod_shell.is_some()
            || self.workspace_root.is_some()
        {
            return;
        }
        let ext = match self.lang.as_deref().unwrap_or("rust") {
            "rust" | "rs" => "rs",
            "python" | "py" => "py",
            "typescript" | "ts" => "ts",
            "javascript" | "js" => "js",
            "go" => "go",
            "c" => "c",
            "cpp" | "c++" => "cpp",
            other => other,
        };
        let safe: String = self
            .id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let file = format!("sol_{safe}.{ext}");
        // The prompt must ASK for what the grade READS — a derived file behind an unchanged
        // "return the complete function" prompt would score an honest 0 for the wrong reason
        // (she answered the question she was asked). Same wording the hand-authored acted gym
        // used, so migrated tasks read identically to ones an author wrote by hand.
        self.prompt = format!(
            "Implement the following, and VERIFY it before finishing. Write your solution to \
             the file `{file}` (a relative path in your workspace) using your write tool, then \
             compile and run it with your shell tool against a few checks you devise; if it \
             fails, read the compiler/test output and fix it. Only finish once it actually \
             compiles and runs. Leave the final, working code in `{file}`.\n\n{}",
            self.prompt
        );
        self.solution_file = Some(file);
    }
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
            tokio::spawn(async move {
                match CognitionEval::run_eval_restoring(inner).await {
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
    /// [`run_eval`](Self::run_eval) plus the HANDS restore — the ONE entry both launch modes
    /// (inline and detached) go through, so neither can forget it.
    ///
    /// `--workspace_root` re-roots the persona's file engine, and that is PROCESS-GLOBAL:
    /// `code/create-workspace` keys the engine on the caller identity, the engines live in one
    /// map for the whole runtime, and a measurement fork borrows the LIVING persona's executor
    /// and her id. So the eval's rooting outlives the eval, and the living persona keeps
    /// acting in the exam repo (#312 — measured on `agent/solve`, identical mechanism here).
    ///
    /// Restoring HERE rather than inside the body is deliberate. The body is ~230 lines with
    /// many fallible steps; the invariant is not "unwind whatever the body did" but "when the
    /// eval is over, her hands are her own", and that is exactly what this boundary knows.
    /// It also covers the error paths without wrapping them.
    async fn run_eval_restoring(
        p: CognitionEvalParams,
    ) -> Result<CognitionEvalResult, CommandError> {
        // Only an eval that re-rooted has anything to put back — an ordinary eval never
        // touched her hands and must not provoke a citizen-layer provision for nothing.
        let rooted = p.workspace_root.is_some();
        let persona_id = p.persona_id.clone();
        let out = CognitionEval::run_eval(p).await;
        if rooted {
            if let Err(e) =
                crate::cognition::persona_workspace::restore_persona_workspace(&persona_id).await
            {
                tracing::error!(
                    persona = %persona_id,
                    error = %e,
                    "cognition/eval could NOT return the persona's hands to her own workspace — \
                     she is still rooted at the eval's workspace_root and her live turns will act \
                     there (#312)"
                );
            }
        }
        out
    }

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

        // Resolve the persona reference at ONE formal boundary (#396): a full UUID,
        // an 8-char short-id, or a persona name — normalized to the typed id against
        // the forkable set. A full UUID passes through (race-safe for a persona whose
        // template is still assembling; the fork wait below absorbs that), while a
        // garbage or unknown reference fails LOUD HERE naming the online personas,
        // instead of parsing fine and dying at the fork wait with a misleading
        // "not assembled at spawn" (the #396 fiasco: a loose-String id fed a dead
        // reference to a doomed eval).
        let persona_uuid = crate::cognition::persona_workspace::global()
            .resolve_persona(&p.persona_id)
            .map_err(|e| CommandError::Invalid(format!("{e} Or call persona/instances/list.")))?
            // The workspace fork machinery below is keyed by bare `Uuid`; unwrap the
            // resolved identity ONCE, here, rather than threading two types through it.
            .as_uuid();
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
        let mut tasks: Vec<EvalTask> = if let Some(inline) = p.tasks {
            inline
        } else {
            let reference = p.eval_set.as_deref().unwrap_or(DEFAULT_EVAL_SET);
            let (origin, text) =
                crate::cognition::gym::resolve_gym(reference).map_err(CommandError::Invalid)?;
            parse_jsonl(&text, &origin)?
        };
        // Every code task grades hands. See `require_hands_for_code` — applied HERE, after both
        // load paths converge, so an inline task from a command payload obeys the same rule as a
        // committed gym line. There is no loader path to a mouth-graded code task.
        for t in tasks.iter_mut() {
            t.require_hands_for_code();
        }

        // Does this exam grade her HANDS or her MOUTH? A task graded from a file she
        // writes (`solution_file`), a workspace DoD she must satisfy (`dod_shell`), a
        // pinned repo she edits (`workspace_root`), or a UI she BUILDS and we OBSERVE
        // (`ui_checks`) needs tools — she must `code/write` the file, then can
        // `perception/observe` her own render and iterate (the image-feedback loop).
        // A purely spoken KNOWLEDGE task (`test`/`expect`, answerable from the model's own
        // weights) does not — and offering tools there is a net TAX: a native-tool-call model
        // loops on the discovery pair (`commands/help`) and never speaks (the isolator's
        // Devstral 100%→0%). Match the surface to the need. But "graded by mouth" ≠ "needs no
        // tools": a repo-nav exam is spoken-graded yet unanswerable without `code/search`, so
        // each task decides via `EvalTask::needs_tools()` (explicit declaration, else derived
        // from the grading modality) — closing the #208 silent-0 where coder-eval ran the whole
        // repo-nav gym speak-only. Run-level = any task needs tools (offered to the whole run).
        let needs_tools = p.workspace_root.is_some() || tasks.iter().any(EvalTask::needs_tools);

        // #312 — THE EXAM'S WORLD IS A COPY, NEVER THE SHARED CHECKOUT. With no pinned
        // workspace_root the fork's hands rooted at the CORE CWD (the real repo): fixture
        // writes + per-task wipes (#209) landed in the shared world every persona
        // perceives, and gym debris (conway_game_of_life, work-*) accumulated as the
        // personas' "own" projects — the exam↔live contamination the room degraded under.
        // The mind side was already isolated (fork_detached + NoopSink + volatile-tier
        // exclusion); the WORLD side wasn't. Repo-nav exams still need the repo, so the
        // default is a per-run CoW clone (APFS clonefile / reflink; plain copy where the
        // FS lacks CoW), removed on every return path via Drop. An explicit
        // workspace_root pin still wins (SWE-bench checkouts own their state).
        let ephemeral_root = if p.workspace_root.is_none() && needs_tools {
            let tag = p
                .run_id
                .clone()
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            match provision_ephemeral_eval_root(&tag) {
                Ok(root) => Some(EphemeralEvalRoot::new(root)),
                Err(e) => {
                    // Fail LOUD and refuse to contaminate — no silent fallback to cwd.
                    return Err(CommandError::Internal(format!(
                        "eval workspace provisioning failed ({e}) — refusing to run a \
                         tool-bearing exam in the shared checkout (#312)"
                    )));
                }
            }
        } else {
            None
        };
        let eval_workspace_root: Option<String> = p
            .workspace_root
            .clone()
            .or_else(|| ephemeral_root.as_ref().map(|r| r.0.display().to_string()));

        // #207: suppress the fork's drifting episodic recall for a reproducible ABSOLUTE
        // baseline. Opt-in; default false = memories intact (the natural persona).
        let suppress_recall = p.suppress_recall.unwrap_or(false);

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
        // The governed VRAM reservation for the eval lane (#56/G1). Held for the WHOLE
        // run so the board sees the eval's bytes as taken; released (RAII) when `run`
        // returns. Declared BEFORE `_eval_lane` so — locals drop in REVERSE order — it
        // drops SECOND: the lane's server is killed first (physical VRAM freed), THEN the
        // accounting reservation is released. Releasing the lease while the process still
        // holds the bytes would flash them as "available" and re-open the grab race.
        // Holds the (possibly shared) measurement lane alive for the whole run. The
        // handle's Arc carries the llama-server process AND the governed VRAM lease, so
        // one holder keeps both — no separate `_eval_vram_lease` juggling.
        let mut _eval_lane: Option<EvalLane> = None;
        // The Proctored Exam Session's ACQUIRE for the LIVE-lane case (the `(None, None)`
        // living-persona benchmark): an EXPLICIT `plan_placement` decision (Slice A), not a
        // blind steady-hold. Her base is already resident ⇒ the verdict is `ShareLane` — she
        // is measured as a co-tenant decode slot on her real lane (NO second weight copy),
        // and while the context is held the daemon skips the grow-back re-home that would
        // relaunch the lane and connection-refuse the exam mid-flight (hard-rs 0/8,
        // 2026-07-20). None for the ephemeral-lane branches — those own an isolated lane the
        // serving daemon never touches. Dropped when `run` returns (RAII → grow-back resumes).
        let mut _exam_serving: Option<crate::cognition::exam_serving::ExamServingContext> = None;
        // GPU-first placement evidence for the gene's measurement lane — surfaced on
        // the result so the harness shows which device the A/B was scored on. None in
        // single-pass mode (that forks onto her LIVE lane, which is already GPU).
        let mut placement_evidence: Option<PlacementEvidence> = None;
        let cycle = match (&p.gene, p.base_model_id.as_deref()) {
            // A gene names its own forged base → ephemeral lane + the gene as `--lora`.
            (Some(gene), _) => {
                let el = spawn_gene_eval_lane(gene).await?;
                placement_evidence = Some(el.placement.clone());
                let adapter = el.adapter.clone();
                let served_ctx = el.served_ctx;
                let cycle = fork_eval_cycle_waiting(&persona_uuid, || {
                    crate::cognition::persona_workspace::global()
                        .fork_eval_cycle_with_adapter(&persona_uuid, adapter.clone(), served_ctx, needs_tools, eval_workspace_root.as_deref(), suppress_recall, Vec::new())
                })
                .await
                .ok_or_else(|| CommandError::NotFound(format!(
                    "persona {persona_uuid} is not online — no workspace template after {WORKSPACE_TEMPLATE_WAIT_TRIES}s, so eval cannot fork a measurement copy of her mind. She either isn't running or the id is wrong; call persona/instances/list for the personas online right now."
                )))?;
                _eval_lane = Some(el); // holds the lane + its VRAM lease alive for the run
                cycle
            }
            // A bare base_model_id → ephemeral lane for THAT model, no gene. The clean
            // same-model control: measure the full loop on a chosen model in its own
            // throwaway server, living persona untouched (#59).
            (None, Some(base_id)) => {
                let el = spawn_base_eval_lane(base_id).await?;
                placement_evidence = Some(el.placement.clone());
                let adapter = el.adapter.clone();
                let served_ctx = el.served_ctx;
                let cycle = fork_eval_cycle_waiting(&persona_uuid, || {
                    crate::cognition::persona_workspace::global()
                        .fork_eval_cycle_with_adapter(&persona_uuid, adapter.clone(), served_ctx, needs_tools, eval_workspace_root.as_deref(), suppress_recall, Vec::new())
                })
                .await
                .ok_or_else(|| CommandError::NotFound(format!(
                    "persona {persona_uuid} is not online — no workspace template after {WORKSPACE_TEMPLATE_WAIT_TRIES}s, so eval cannot fork a measurement copy of her mind. She either isn't running or the id is wrong; call persona/instances/list for the personas online right now."
                )))?;
                _eval_lane = Some(el); // holds the lane + its VRAM lease alive for the run
                cycle
            }
            // Neither → the living-persona benchmark: she is served on THIS base already,
            // so the admission decision is `placement::Placement::ShareLane` — measure her
            // as a co-tenant decode slot on her REAL lane (her genome, her window), no
            // second weight copy. Hold the lane STEADY for the run so the grow-back re-home
            // can't relaunch it and connection-refuse the exam mid-flight (the hard-rs 0/8
            // bounce, 2026-07-20). Same bounded wait-for-template as the lane branches above;
            // the post-reboot register_from_cfg race hits every fork path identically.
            (None, None) => {
                // The default benchmark. Prefer a DEDICATED throwaway lane on her own base — the
                // exam must not co-tenant the live persona lane, where it STARVES behind live
                // turns (glass-boxed 2026-07-21: 0/3 graded in 12 min, the isolate verdict was
                // computed but never acted on — the branch forked onto the live lane anyway).
                // Reuse the SAME governor-leased ephemeral-lane spawn `base_model_id` rides
                // (proven: 17.9 GB footprint fits beside the live lane per the placement
                // captures). On ANY failure (won't fit, no active model, template not ready)
                // fall back to the co-tenant SHARE hold — degrade, never OOM or hard-fail.
                // WAIT for a READY served model, don't snapshot instantaneously. The
                // instantaneous `current_serving().active_model` is None in the window right
                // after a reboot while the live lane is still cold-loading its base + LoRAs —
                // so the dedicated-lane decision raced the boot, saw None, and fell to a SHARED
                // lane that ALSO wasn't ready yet → wedge ("lane never recovered: per-task
                // deadline", glass-boxed 2026-07-21: the same 3-task set scored 2/3 when the
                // dedicated lane spawned and 0/3 when this race dropped it to share). Awaiting
                // readiness makes active_model reliably Some, so the isolated lane spawns every
                // run and the score is trustworthy. Bounded by the spawner's own load budget;
                // on timeout (genuinely no served model) active stays None and we degrade loud.
                let active = crate::inference::llama_server::await_ready_serving(
                    crate::inference::llama_server::DEFAULT_SERVING_WAIT,
                )
                .await
                .and_then(|s| s.active_model);
                let dedicated = match &active {
                    Some(base) => match spawn_base_eval_lane(base).await {
                        Ok(lane) => Some(lane),
                        // NEVER swallow the spawn failure — a silent `.ok()` degrade to the
                        // co-tenant SHARE path is a fail-loud violation ([[fallbacks-are-illegal-fail-loud]])
                        // AND it hides the exact reason the isolated lane didn't come up. The
                        // co-tenant path is unreliable (it STARVES behind live turns → per-task
                        // deadline → dropped tasks → a meaningless low score, glass-boxed
                        // 2026-07-21 run B). We still degrade rather than hard-fail (a share
                        // score beats no score), but the reason is now VISIBLE in the eval phase
                        // stream and the log, so we can actually fix the lane instead of guessing.
                        Err(e) => {
                            tracing::warn!(
                                target: "cognition::eval",
                                base = %base,
                                error = %e,
                                "dedicated eval lane failed to come up — DEGRADING to co-tenant share (unreliable: may starve behind live turns). Fix the lane; do not trust a shared-lane score."
                            );
                            emit_eval_phase(
                                "dedicated_lane_failed",
                                &format!("isolated eval lane failed ({e}) — falling back to shared live lane"),
                            );
                            None
                        }
                    },
                    None => {
                        tracing::warn!(
                            target: "cognition::eval",
                            "no active served model in the serving snapshot — cannot stand up a dedicated eval lane, DEGRADING to co-tenant share"
                        );
                        emit_eval_phase(
                            "no_active_model",
                            "serving snapshot has no active model — falling back to shared live lane",
                        );
                        None
                    }
                };
                match dedicated {
                    Some(el) => {
                        placement_evidence = Some(el.placement.clone());
                        let adapter = el.adapter.clone();
                        let served_ctx = el.served_ctx;
                        let cycle = fork_eval_cycle_waiting(&persona_uuid, || {
                            crate::cognition::persona_workspace::global()
                                .fork_eval_cycle_with_adapter(&persona_uuid, adapter.clone(), served_ctx, needs_tools, eval_workspace_root.as_deref(), suppress_recall, Vec::new())
                        })
                        .await
                        .ok_or_else(|| CommandError::NotFound(format!(
                            "persona {persona_uuid} is not online — no workspace template after {WORKSPACE_TEMPLATE_WAIT_TRIES}s, so eval cannot fork a measurement copy of her mind. She either isn't running or the id is wrong; call persona/instances/list for the personas online right now."
                        )))?;
                        _eval_lane = Some(el); // holds the lane + its VRAM lease alive for the run
                        cycle
                    }
                    None => {
                        // A dedicated lane won't fit / isn't available → co-tenant SHARE on her
                        // live lane, held steady for the run (the historical behavior).
                        _exam_serving = Some(acquire_exam_serving_context().await);
                        fork_eval_cycle_waiting(&persona_uuid, || {
                            crate::cognition::persona_workspace::global()
                                .fork_eval_cycle(&persona_uuid, needs_tools, eval_workspace_root.as_deref(), suppress_recall)
                        })
                        .await
                        .ok_or_else(|| CommandError::NotFound(format!(
                            "persona {persona_uuid} is not online — no workspace template after {WORKSPACE_TEMPLATE_WAIT_TRIES}s, so eval cannot fork a measurement copy without measuring her live mind. She either isn't running or the id is wrong; call persona/instances/list for the personas online right now."
                        )))?
                    }
                }
            }
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
        // Root her hands at the target repo — the canonical mechanism, now shared with
        // `agent/solve` (drives `code/create-workspace` through her executor; fails LOUD rather
        // than let a silent no-root score a false 0-byte diff, [[fallbacks-are-illegal-fail-loud]]).
        // ROOT AT THE RESOLVED ROOT — explicit pin OR the #312 ephemeral clone. This
        // was gated on `p.workspace_root` alone, so every DEFAULT gym eval since the
        // ephemeral-clone isolation landed had its grader reading the clone while her
        // hands wrote to her HOME workspace: uniform false zeros on correct code
        // (glass-boxed 2026-08-23 on the Ornith showcase battery — Atlas's
        // roman_to_int was textbook-correct in home, the clone empty, 0/3 graded).
        // The exact "silent no-root scores a false ZERO that would LIE about the
        // solver" this comment block always warned about, one variable to the left.
        if let Some(root) = eval_workspace_root.as_deref() {
            // A gym run IS the measurement — the grader reads the artifact, never her prose,
            // so an inert edit is unrecoverable here for the same reason as `benchmark/swe-solve`.
            crate::cognition::persona_workspace::root_acting_workspace(&cycle, root, &[], true)
                .await?;
            crate::probe!(
                class = "eval.hands_rooted",
                root = %root,
                explicit = %p.workspace_root.is_some(),
                "eval fork's hands rooted where the grader reads"
            );
        }

        let max_acts = p.max_acts.unwrap_or(DEFAULT_MAX_ACTS) as usize;
        let max_retries = p.max_retries.unwrap_or(MAX_FAIL_RETRIES);
        let total = tasks.len() as u32;
        let rate = |score: u32| {
            if total > 0 {
                score as f64 / total as f64
            } else {
                0.0
            }
        };

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
            // A/B LIFT arms consume `.pass`/`.results`; the infra-fault accounting rides
            // on the EphemeralServingLane path (decode-verified at spawn) as a scoped
            // follow-up — the shared-lane single-pass below is what Slice B makes honest.
            let base_score = run_pass(
                &cycle,
                &isolation,
                &tasks,
                room,
                max_acts,
                max_retries,
                eval_workspace_root.as_deref(),
            )
            .await
            .pass;

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
            let gene_outcome = run_pass(
                &cycle,
                &isolation,
                &tasks,
                room,
                max_acts,
                max_retries,
                eval_workspace_root.as_deref(),
            )
            .await;
            let (gene_score, gene_results) = (gene_outcome.pass, gene_outcome.results);
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
                // A/B runs on its OWN EphemeralServingLane (gene/base copy), which
                // decode-verifies at spawn — a different failure surface than the shared
                // live lane. Threading the infra-fault classification through the two-arm
                // LIFT assembly is a scoped follow-up; the shared-lane single-pass (the
                // fake-zero incident) is what Slice B makes trustworthy first.
                infra_unavailable: None,
            };
            result.run_id = p.run_id.clone();
            append_progress_ledger(
                &result,
                p.note.as_deref(),
                &eval_set_label,
                _fleet_lease.as_ref().map(|_| true),
            );
            return Ok(result);
        }

        // Readiness gate — ONLY for a true LIVE-lane single pass. Refuse to grade a COLD
        // serving lane: after a core/llama-server relaunch the model is not resident for
        // ~tens of seconds; firing tasks at it returns empty generations the grader would
        // silently record as 0-token "no match" failures — a phantom score in an invisible
        // degraded mode. Wait (bounded) for the live lane to be able to generate; fail loud
        // if it never is.
        //
        // This gate guards the LIVE lane (`await_ready_serving` reads the live serving
        // snapshot). BOTH the `gene` A/B path (returned above) AND a `base_model_id` run
        // stand up their OWN `EphemeralServingLane`, which decode-verifies at spawn
        // (`spawn_base_eval_lane` → `wait_ready` with the ephemeral budget) — they never
        // touch the live lane, so gating them on live-lane readiness is spurious: a
        // base_model_id run flakily aborted whenever the UNRELATED live lane happened not
        // to be snapshot-ready within 90s (glass-boxed 2026-07-21: Qwen-7B webdev 0/0 while
        // its own dedicated lane was fine; Hermes passed the same gate only by live-lane
        // luck). Skip the gate when a dedicated lane owns the run.
        if p.base_model_id.is_none() {
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
        let want_team =
            p.reviewers.unwrap_or(0) >= 1 && p.gene.is_none() && p.base_model_id.is_none();
        let outcome = if want_team {
            let reviewer = crate::cognition::persona_workspace::global()
                .fork_eval_cycle(&persona_uuid, needs_tools, eval_workspace_root.as_deref(), suppress_recall)
                .ok_or_else(|| CommandError::NotFound(format!(
                    "no workspace template for persona {persona_uuid} — cannot fork a reviewer teammate"
                )))?;
            let reviewer_iso = reviewer.isolate_for_eval();
            let out = run_pass_team(
                &cycle,
                &isolation,
                &reviewer,
                &reviewer_iso,
                &tasks,
                room,
                max_acts,
                eval_workspace_root.as_deref(),
            )
            .await;
            drop(reviewer_iso);
            out
        } else {
            run_pass(
                &cycle,
                &isolation,
                &tasks,
                room,
                max_acts,
                max_retries,
                eval_workspace_root.as_deref(),
            )
            .await
        };
        drop(isolation);

        // The Proctored Exam Session verdict (Slice B): ≥1 UNRECOVERABLE infra fault ⇒ the
        // shared serving lane died mid-exam and this score is VOID. Build the marker BEFORE
        // moving `results`; when set, every render site reports InfraUnavailable instead of a
        // fake `pass_rate: 0.0`. [[proctored-exam-session-dependable-benchmark]]
        let infra_unavailable = infra_verdict(&outcome);
        let (score, results) = (outcome.pass, outcome.results);

        // LEARN mode: the exam just taught her — carry the redacted lesson back to the
        // LIVING self. She keeps the experience of having been asked and how she did; the
        // held-out answer key is scrubbed so she can never memorize it (redaction, not
        // forget-context: keep the memory, excise the crib sheet). The exam ran on the fork
        // (#59 intact); only the clean lesson crosses back. Single-pass only in this slice.
        // NEVER learn from an infra-unavailable run — a dead-lane "failure" is not a lesson.
        if p.learn.learns() && p.gene.is_none() && infra_unavailable.is_none() {
            let transferred = transfer_redacted_lessons(
                &persona_uuid,
                room,
                p.eval_set.as_deref().unwrap_or(DEFAULT_EVAL_SET), // the run's real set: None means the default set genuinely ran
                &tasks,
                &results,
            );
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
            // TRUTHFUL placement report — thread the actual decision, don't hardcode a
            // lie. The (None,None) default benchmark tries to stand up a DEDICATED
            // throwaway lane (isolated window + slot → reliable, reproducible); only on
            // spawn failure does it co-tenant the live persona lane (unreliable — starves
            // behind live turns). A hardcoded "live persona lane" string made the two
            // indistinguishable in the result, so a wedge-prone shared-lane score looked
            // identical to a trusted isolated one and glass-boxing was impossible
            // (2026-07-21). Now the report says which lane actually ran the exam, so a
            // shared-lane score is visibly flagged as not-to-be-trusted.
            // [[dedicated-eval-lane-must-keep-its-own-window]]
            lane_placement: match &placement_evidence {
                Some(ev) => format!("{} (dedicated eval lane)", ev.device),
                None => "gpu (SHARED live persona lane — co-tenant, unreliable)".to_string(),
            },
            lane_placement_reason: match &placement_evidence {
                Some(ev) => ev.reason.clone(),
                None => "no dedicated lane could be placed — measured as a co-tenant on the living persona's GPU lane (may starve behind live turns)".to_string(),
            },
            lane_free_vram_bytes: placement_evidence.as_ref().and_then(|e| e.free_vram_bytes),
            lane_estimated_footprint_bytes: placement_evidence.as_ref().and_then(|e| e.footprint_bytes),
            infra_unavailable,
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
    let outcome = if result.ok {
        "I solved it"
    } else {
        "I did NOT solve it"
    };
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
    eval_set: &str,
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
    let peer_dir = crate::identity::citizen_peer_dir(
        &crate::modules::persona_instance_manager::resolve_continuum_root(),
        crate::identity::PeerId::from_uuid(*persona_uuid),
    );
    if let Err(e) = std::fs::create_dir_all(&peer_dir) {
        tracing::warn!(error = %e, "citizen dir for the experience stream could not be created — exam episodes stay out of curriculum this run");
    }
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
        // SEAM 1 of the academy convergence (archaeology 2026-08-23): the same
        // graded pair also lands in the EXPERIENCE STREAM, so
        // `genome/teach --from_experience` can remediate the exact failures
        // learn-mode just committed to memory as lessons. Before this, an eval
        // run wrote lessons into the MIND but nothing into the stream — the
        // salience→curriculum drain could never see an exam failure.
        // [[lived-and-eval-experience-are-one-stream-one-being]]
        let episode =
            crate::cognition::experience::ExperienceRecord::from_eval_result(task, result);
        if let Err(e) = crate::cognition::experience::append_experience(&peer_dir, &episode) {
            tracing::warn!(
                persona = %persona_uuid,
                task = %task.id,
                error = %e,
                "exam episode could not join the experience stream — lesson admitted, curriculum blind to it"
            );
        }
    }
    // SEAM 3a: she REMEMBERS BECOMING BETTER. The progress ledger records
    // passRate durably, but no engram ever carried it — a persona's beliefs
    // formed with no awareness of her own measured capability. One Semantic
    // summary per learned exam, keyed ["progress", <eval_set>], so the dream
    // can cluster successive exams into a trajectory belief.
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
            admitted_at_ms: crate::persona::trace::now_ms(),
            trust_state_at_admission: crate::persona::engram::TrustState::SelfTrust,
            admission_trace_id: None,
        };
        let _ = admission.admit_reflection(summary); // duplicate-content dedup is the store's job; a repeat exam at the same rate is legitimately the same belief
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
fn progress_ledger_dir() -> Option<std::path::PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|h| std::path::PathBuf::from(h).join(".continuum/progress"))
}

/// The terminal ledger row for `run_id` in a KNOWN persona's ledger, newest-first.
/// `None` = no such row yet (the run is still in flight, or the id is wrong).
fn find_run_row_for_persona(persona_id: &str, run_id: &str) -> Option<serde_json::Value> {
    let path = progress_ledger_dir()?.join(format!("{persona_id}.jsonl"));
    let text = std::fs::read_to_string(&path).ok()?;
    row_with_run_id(&text, run_id)
}

/// The terminal ledger row for `run_id` across EVERY persona's ledger — the poll
/// path when the caller holds only the (globally-unique) run handle. Scans each
/// `*.jsonl` in the progress dir; returns the first match (a run_id lives in exactly
/// one persona's ledger). Keeps run_id a sufficient key so a matrix/CI poller never
/// hangs on a false pending just because it didn't also thread the persona_id.
fn find_run_row_any_persona(run_id: &str) -> Option<serde_json::Value> {
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
fn row_with_run_id(text: &str, run_id: &str) -> Option<serde_json::Value> {
    for line in text.lines().rev() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            if v.get("runId").and_then(|r| r.as_str()) == Some(run_id) {
                return Some(v);
            }
        }
    }
    None
}

/// Write a FAILED run row to the progress ledger so `cognition/eval-status` surfaces
/// the error (keyed on `run_id`) instead of returning `null` forever. The poll surface
/// must be able to tell "died" from "still starting" — a detached run that errors before
/// [`append_progress_ledger`] otherwise reads as an eternal pending. `error` + `failed:true`
/// mark it; `total:0` keeps the numeric shape valid for consumers.
fn append_failed_ledger(
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
async fn acquire_exam_serving_context() -> crate::cognition::exam_serving::ExamServingContext {
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
    let Some(fp) = crate::modules::serving_daemon::footprint_for(&model) else {
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
        .max(crate::cognition::serving_plan::MIN_SERVE_CTX);
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
/// tool-using eval on a base_model_id fired right after `continuum reboot` failed loud instead of
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
        emit_eval_phase(
            "preparing",
            &format!("waiting for workspace template ({}s)", attempt + 1),
        );
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
    crate::probe!(
        class = "eval.phase",
        phase = phase,
        detail = detail,
        "eval lifecycle phase"
    );
}

/// Run a REAL definition-of-done: a shell command in the persona's workspace (cwd). Pass =
/// exit 0. Returns `(ok, verdict)`; on failure the verdict carries the real stdout+stderr
/// (tail-bounded) so the recovery loop hands the model the actual error to fix against.
/// This is how a persona works on REAL things — her file edits are checked by a real build/
/// test, not by grading text she typed into chat.
/// Run a task's setup/DoD shell IN THE RUN'S WORLD. `root` is the same
/// resolved run root the artifact grader uses (task pin or the #312 ephemeral
/// clone); `None` preserves process-CWD for callers that genuinely run there.
///
/// Defect 2's SIBLING, found live 2026-08-23 (MirrorCode maiden round):
/// setup_shell staged fixtures into the CORE's CWD (the real repo checkout —
/// a `mirrorcode/` tree appeared in the operator's working copy) while her
/// hands wrote into the clone, and dod_shell then graded a world containing
/// the fixtures but not her work — 100% false fails AND repo pollution.
/// DS-1000's clean sweep predates the ephemeral roots, when CWD *was* the
/// workspace — a coincidence, not a design.
async fn run_dod(root: Option<&std::path::Path>, cmd: &str) -> (bool, String) {
    let mut command = tokio::process::Command::new("bash");
    command.arg("-lc").arg(cmd);
    if let Some(r) = root {
        command.current_dir(r);
    }
    match command.output().await {
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
/// Extract an HTML artifact the persona SPOKE — the last ```html (or doctype/`<html`)
/// fenced block, or a bare page answer — for the mouth-or-hands web-dev capture below.
/// Returns the inner HTML, or `None` when her answer carries no page.
///
/// The last qualifying fence wins: across a multi-turn build, the FINAL page she emitted
/// is the one to grade (earlier fences are superseded drafts).
fn html_artifact_from_answer(answer: &str) -> Option<String> {
    let mut best: Option<String> = None;
    let mut rest = answer;
    while let Some(open) = rest.find("```") {
        let after = &rest[open + 3..];
        let nl = after.find('\n').unwrap_or(after.len());
        let lang = after[..nl].trim().to_ascii_lowercase();
        let body_area = if nl < after.len() {
            &after[nl + 1..]
        } else {
            ""
        };
        let Some(close) = body_area.find("```") else {
            break;
        };
        let body = body_area[..close].trim();
        let head = body.trim_start().to_ascii_lowercase();
        let looks_html =
            lang.starts_with("html") || head.starts_with("<!doctype") || head.starts_with("<html");
        if looks_html && !body.is_empty() {
            best = Some(body.to_string()); // keep the LAST qualifying fence
        }
        rest = &body_area[close + 3..];
    }
    if best.is_some() {
        return best;
    }
    // No fence — the whole answer may itself be the page.
    let trimmed = answer.trim();
    let head = trimmed.to_ascii_lowercase();
    if head.starts_with("<!doctype") || head.starts_with("<html") {
        return Some(trimmed.to_string());
    }
    None
}

async fn perception_grade(
    cycle: &crate::cognition::workspace::WorkspaceCycle,
    target: &str,
    checks: &[crate::perception::scoring::UiCheck],
    threshold: f32,
    workspace_root: Option<&str>,
    answer: &str,
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
        let p = std::path::Path::new(path);
        // NO MOUTH-FOR-HANDS. There used to be a rescue here: if she never wrote the file
        // but SPOKE a page in a ```html fence, the harness materialized it and graded that.
        // It was disclosed and well-intentioned — it isolated "can you build a UI" from "can
        // you route a tool", after a 2026-07-21 run where Qwen2.5-Coder-7B wrote a flawless
        // login form in a fence, wrote no file, and scored 0/6 below a weaker model.
        //
        // It is deleted because it measures a persona we are not building. Ours DO the thing:
        // they operate tools and produce artifacts that exist. A page that lives only in an
        // utterance is not a page — it is a description of one, and grading it as a page is a
        // scam we play on ourselves. The file already says this 1800 lines up, about
        // `solution_file`: "the act→verify loop is only visible if we grade what she actually
        // wrote + compiled, not what she narrated." The rescue contradicted the doctrine its
        // own module declares.
        //
        // The cost of keeping it was not a wrong number, it was a HIDDEN GAP: because nothing
        // in the suite ever required hands, the fence→act failure was never scored, so it was
        // never fixed — and it is exactly the failure SWE-bench exposes at full strength (30
        // acts, 0 files, 0 refusals). What is rescued is never repaired.
        //
        // The spoken-artifact detector survives with its consequence inverted: it used to
        // launder the failure, it now NAMES it, so a zero is diagnosable instead of mute.
        // [[beat-oss-agentic-systems-as-whole-beings-never-strip-to-pass]],
        // [[fix-the-substrate-never-rig-the-persona-the-line-between-assist-and-scaffold]],
        // [[execute-dont-narrate]]
        let wrote_with_hands = std::fs::read_to_string(p)
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        if !wrote_with_hands {
            let spoke_one = html_artifact_from_answer(answer).is_some();
            return (
                false,
                if spoke_one {
                    format!(
                        "web-dev grade: FAIL — she SPOKE a complete HTML artifact but never \
                         routed it through `code/write`, so '{path}' does not exist. The task \
                         is graded on the file her hands produced, never on her narration of \
                         it. This is the fence→act gap, and it is the finding, not a harness \
                         defect."
                    )
                } else {
                    format!(
                        "web-dev grade: FAIL — no artifact at '{path}' and no HTML in her \
                         answer either; she neither built it nor described it."
                    )
                },
            );
        }
        let obs = crate::perception::static_html::observe_file(p);
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
        Some(r) => {
            return (
                false,
                format!(
                    "perception/observe failed for '{target_url}': {}",
                    r.content
                ),
            )
        }
        None => {
            return (
                false,
                format!("perception/observe returned no result for '{target_url}'"),
            )
        }
    };
    let obs: crate::perception::ObserveResult = match serde_json::from_str(content) {
        Ok(o) => o,
        Err(e) => {
            let preview: String = content.chars().take(400).collect();
            return (
                false,
                format!("could not parse observation for '{target_url}': {e} — got: {preview}"),
            );
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

/// The run_id of the exam pass grading RIGHT NOW, `None` when idle. Public for
/// the positron bench emitter (#141's third surface made real): a live exam is
/// WORK the room's rail must show, through the same pipe every other run row
/// rides — never a separate poller.
pub fn live_eval_run_id() -> Option<String> {
    CURRENT_RUN_ID.lock().ok().and_then(|g| g.clone())
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
fn report_task_graded(
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
        output_tokens,
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

/// Remove every entry INSIDE `root` (files + subdirs) but keep `root` itself, so a persona
/// whose hands are rooted there keeps a valid, EMPTY working directory. Used by the eval to
/// give each from-scratch build task a clean slate (#209): a correct render on task N must
/// never be graded against task N-1's leftover files. A missing root is not an error (nothing
/// to clean); a genuine IO failure propagates so the caller can warn without aborting the run.
fn clean_dir_contents(root: &str) -> std::io::Result<()> {
    let p = std::path::Path::new(root);
    if !p.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(p)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// The outcome of running a whole task set once — the pass count and per-task rows,
/// PLUS the infra-fault accounting that decides whether the number is trustworthy.
/// The keystone of the Proctored Exam Session (Slice B): an inference call that FAILS
/// (lane not-ready / connect-refused / "not the active served model" / compute-error /
/// stream-idle) is an INFRA fault, never a wrong answer. A transient fault is re-verified
/// and retried; an UNRECOVERABLE one (lane never returns to decode-ready) makes the run
/// [`InfraUnavailable`] — the caller reports that, NEVER a fake `pass_rate: 0.0`.
/// [[proctored-exam-session-dependable-benchmark]]
struct PassOutcome {
    /// Tasks that PASSED (graded on a verified lane).
    pass: u32,
    /// Per-task rows (includes any infra-graded row for forensics).
    results: Vec<EvalTaskResult>,
    /// How many tasks hit an UNRECOVERABLE infra fault — the lane never came back to
    /// decode-ready within the re-verify+retry budget. ≥1 ⇒ the run is void.
    infra_faults: u32,
    /// The cause of the FIRST unrecoverable infra fault, naming its task — the reason the
    /// whole run aborts as [`InfraUnavailable`]. `None` when every task reached a verdict.
    infra_reason: Option<String>,
}

/// The Proctored Exam Session verdict: does a completed pass constitute a trustworthy
/// score, or did the serving lane die mid-exam? ≥1 unrecoverable infra fault ⇒ the run is
/// VOID and reported [`InfraUnavailable`] (never a fake `pass_rate: 0.0`). Pure so the
/// fault-injection test pins it directly, and ONE place decides void-ness for every render
/// site. [[proctored-exam-session-dependable-benchmark]]
fn infra_verdict(outcome: &PassOutcome) -> Option<InfraUnavailable> {
    (outcome.infra_faults > 0).then(|| InfraUnavailable {
        reason: outcome
            .infra_reason
            .clone()
            .unwrap_or_else(|| "serving lane never recovered".to_string()),
        tasks_attempted: outcome.results.len() as u32,
        infra_faults: outcome.infra_faults,
    })
}

/// How many times a task that INFRA-FAULTED (the model call itself failed, not a wrong
/// answer) is re-verified and retried before it counts as unrecoverable. This is the
/// bounded recovery a transient lane bounce (grow-back relaunch, brief not-ready) gets —
/// distinct from `max_acts` (the persona's OWN act→observe budget) and MAX_FAIL_RETRIES
/// (agentic recovery from a WRONG answer). 3 covers a full not-ready→relaunch→ready cycle
/// without letting a genuinely-dead lane spin the exam forever.
const INFRA_FAULT_RETRIES: u32 = 3;

/// Bound for a between-attempt lane re-verify. A grow-back relaunch or a #175 self-heal
/// respawn brings the lane back within tens of seconds; this waits out that window before
/// spending a retry, and its expiry (lane still not snapshot-ready) is itself the signal
/// the fault is unrecoverable. Generous vs a warm lane's instant readiness; it only ever
/// elapses on a genuinely down lane.
const LANE_REVERIFY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

/// Re-verify that the live serving lane is READY to decode before retrying an infra-faulted
/// task. Returns the ready snapshot's active model, or `None` if the lane never returned to
/// ready within [`LANE_REVERIFY_TIMEOUT`] (⇒ the fault is unrecoverable). Snapshot-ready is
/// the cheap gate; the retried TASK is itself the real decode proof (a genuine generation),
/// so this is not status-optimism — a lane that answers ready but can't decode simply
/// infra-faults again on the retry and exhausts the budget. Slice A upgrades this to a
/// decode smoke-probe at ACQUIRE time; Slice B verifies via the real task attempt.
async fn reverify_lane() -> Option<String> {
    crate::inference::llama_server::await_ready_serving(LANE_REVERIFY_TIMEOUT)
        .await
        .and_then(|s| s.active_model)
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
) -> PassOutcome {
    let mut pass = 0u32;
    let mut results = Vec::with_capacity(tasks.len());
    let mut infra_faults = 0u32;
    let mut infra_reason: Option<String> = None;
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
        // Task-start receipt: the health verdicts downstream (session gates, the
        // bench view) judge a running task against the round's OWN completed-task
        // latencies — which needs an exact start signal, not an inference from
        // the previous grade's absence (Joel 2026-08-23: a stalled task must be
        // known in minutes, adaptively, "better fix" than a fixed window).
        crate::probe!(
            class = "eval.task.started",
            task = %t.id,
            "task drive begins — elapsed-vs-round-median verdicts key on this row"
        );
        // #209 — per-task CLEAN WORKSPACE for from-scratch build gyms. A from-scratch build
        // task (ui_checks present, and NO setup/dod/solution that establishes or depends on
        // prior repo state) is graded by observing the artifact the persona just wrote into
        // the pinned workspace_root. If the PREVIOUS task's files still sit there, this task's
        // grade observes a stale/foreign artifact — the exact cross-task contamination that
        // scored webdev 0/6 while she built a correct page each time (harness noise, not a
        // capability miss). Wipe the pinned root's CONTENTS (not the dir itself — her hands are
        // rooted at it) so every task grades EXACTLY the artifact it produced. Only from-scratch
        // build tasks are cleaned; setup/dod/solution tasks OWN their state and are never wiped.
        // No pinned root (hands on the core cwd) → nothing to clean.
        // INDEPENDENT-TASK COGNITION RESET. Every coder benchmark here is a battery of
        // UNRELATED tasks (conway_step vs edit_distance vs a login-form) — each a standalone
        // problem. The continuous-session design (reset once at pass start, carry task-to-task)
        // is right for a RELATED sequence, but for an independent battery it accumulates each
        // prior task's answer into working memory + episodic recall, and that drift both
        // overflows a small lane window (→ per-slot compute-error wedge → empty 0-token
        // generations) AND nudges the model into declining (glass-boxed 2026-07-21: games-rs
        // 6/6 in isolation but 0/6 with mid-battery declines under accumulation). A task starts
        // clean UNLESS it explicitly establishes cross-task state via `setup_shell` (gym/mine's
        // re-break-the-checkout case — the only genuine continuity). So reset the cognitive
        // slate before every independent task, exactly as #209 wipes an independent build's
        // WORKSPACE. [[llama-compute-error-wedge-is-per-slot-context-overflow]]
        let independent_task = t.setup_shell.is_none();
        if independent_task {
            cycle.reset_working_memory();
            isolation.rewind();
        }
        // Workspace wipe: only a from-scratch UI build (ui_checks, no setup/dod/solution) grades
        // an OBSERVED artifact FILE, so only it needs the pinned root cleared between tasks
        // (#209) — a stale index.html from the prior task would score a false pass/fail.
        let from_scratch_build = !t.ui_checks.is_empty()
            && t.setup_shell.is_none()
            && t.dod_shell.is_none()
            && t.solution_file.is_none();
        if from_scratch_build {
            if let Some(root) = t.workspace_root.as_deref().or(workspace_root) {
                if let Err(e) = clean_dir_contents(root) {
                    tracing::warn!(
                        probe_class = "eval.task.clean_workspace",
                        root = root,
                        error = %e,
                        "failed to clean the pinned workspace before a from-scratch build task \
                         — grading may observe a stale artifact (#209)"
                    );
                }
            }
        }
        // PER-TASK ROOT (mined gyms) — DECLARED, and REFUSED when it would split her in half.
        //
        // Each mined task owns its own git worktree, so the root is a property of the TASK. The
        // obvious move is to re-root her here. Do NOT: rooting a persona is TWO operations, and
        // only one of them can run at this point.
        //   * `root_acting_workspace` moves where her hands WRITE — callable any time.
        //   * `repoint_workspace_map_if_pinned` moves what her eyes SEE — mutates
        //     `PersonaBrainConfig`, so it runs only at FORK time; `WorkspaceCycle.faculties` is
        //     immutable after build.
        // Moving only the hands gives her a map of one tree and a file engine in another — the
        // #206 shape, where she reasons over a layout that isn't hers and explores instead of
        // building. Glass-boxed 2026-08-05: the re-root probe fired with the right checkout while
        // her workspace-map, 4.8s later, still described the previous root. It LOOKED fixed.
        //
        // Half-rooted is worse than not rooted, because the resulting zero is a lie about the
        // solver that no probe reports. So refuse, and name the working path: the RUN-level
        // `workspace_root` does BOTH halves at fork time, so ONE EVAL PER ROOT is correct today.
        // [[fallbacks-are-illegal-fail-loud]]
        // [[re-rooting-a-persona-is-two-operations-moving-one-is-worse-than-moving-neither]]
        //
        // The real fix (then this refusal goes away): her eyes read the root FROM her hands —
        // one source of truth — the way `CitizenLayerWorkspaceLayoutReader` already calls the
        // same `ensure_citizen_layer` the hands call, "so map and tools can never again describe
        // different worlds".
        let task_root: Option<&str> = t.workspace_root.as_deref().or(workspace_root);
        if let Some(want) = t.workspace_root.as_deref() {
            if Some(want) != workspace_root {
                results.push(EvalTaskResult {
                    id: t.id.clone(),
                    ok: false,
                    grade: format!(
                        "infra: task '{}' declares its own workspace_root '{want}' but this run \
                         was forked at {:?}. Re-rooting mid-run would move her HANDS without \
                         moving her EYES (the workspace-map is baked at fork), so she would be \
                         graded while reading a tree she is not working in. Run one eval per \
                         root: pass --workspace_root='{want}' with just this task.",
                        t.id, workspace_root
                    ),
                    answer: String::new(),
                    acts: 0,
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
        // Task-state SETUP (gym/mine tasks re-break their checkout so runs are
        // repeatable). A failed setup is a NAMED infra grade — the persona is
        // never examined against a workspace in an unknown state.
        if let Some(setup) = &t.setup_shell {
            let (setup_ok, setup_out) =
                run_dod(task_root.map(std::path::Path::new), setup).await;
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
        // A CLOSURE, not a value: an infra-faulted task is retried (below), and
        // `drive_to_settle` consumes the burst, so each attempt mints a fresh one.
        let make_burst = || {
            crate::cognition::workspace::Burst::from_turns(
                room,
                crate::persona::service_loop::build_workspace_turns(
                    std::slice::from_ref(&task_delivery),
                    "",
                    "",
                    // A single-task exam IS the stimulus — the task delivery is the
                    // whole thread; there is no out-of-band trigger to anchor.
                    None,
                ),
            )
        };
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
        // LAST-RESORT hang backstop — NOT a wedge detector (2026-08-23, second cut).
        //
        // Wedge detection is the SERVING layer's job and it is event-based there: the
        // stream-liveness classifier (phase-aware budgets, 9e2f95820) kills a
        // zero-token stream at the source and the failure surfaces here as
        // `inference_error` — the event path `drive_to_settle` already returns on.
        // The old flat 600s deadline predated that sensor (#123, 2026-07-17) and was
        // never demoted once it shipped: it second-guessed the serving layer with a
        // stopwatch, declared a LIVE multi-minute e2e decode "wedged" (fluid-sim-e2e,
        // observed live tonight), and retried the task into a doomed loop. Cheap
        // wall-clock guesses about liveness always lose to the component that can SEE
        // the tokens ([[the-whole-system-is-event-based-not-polling]]).
        //
        // What remains is the hang class no event can cover BY DEFINITION: an await
        // that never returns because some path between inference calls lost its error
        // propagation (a code bug, not a serving state). For that, one enormous
        // garbage-ceiling timer — and its firing is a SUBSTRATE BUG REPORT (the probe
        // says which class to hunt), never routine control flow.
        const PER_TASK_HANG_BACKSTOP: std::time::Duration =
            std::time::Duration::from_secs(2 * 60 * 60);
        // BOUNDED INFRA-FAULT RECOVERY (Proctored Exam Session, Slice B). Drive to
        // settlement; if the model call itself FAILS (`inference_error`: lane not-ready /
        // connect-refused / "not the active served model" / compute-error / deadline-wedge)
        // that is an INFRA fault, never a wrong answer. Re-verify the lane and retry the
        // SAME task — the retried generation is itself the real decode proof — up to
        // INFRA_FAULT_RETRIES. A transient bounce (grow-back relaunch, brief not-ready)
        // recovers here and the task grades normally. Exhaustion means the lane is dead:
        // `settled` still carries the error and the abort below marks the run
        // InfraUnavailable, so a phantom generation on a dead lane can NEVER be published
        // as `pass_rate: 0.0`. [[proctored-exam-session-dependable-benchmark]]
        let mut settled;
        let mut infra_attempt = 0u32;
        loop {
            settled = match tokio::time::timeout(
                PER_TASK_HANG_BACKSTOP,
                crate::cognition::act_observe::drive_to_settle(
                    cycle,
                    make_burst(),
                    room,
                    t.max_acts.map(|v| v as usize).unwrap_or(max_acts), // None = row sets no budget; the run's budget is the documented inherit
                    if t.workspace_deliverable() {
                        crate::cognition::workspace::TurnFraming::directed().on_workspace()
                    } else {
                        crate::cognition::workspace::TurnFraming::directed()
                    },
                ),
            )
            .await
            {
                Ok(s) => s,
                Err(_) => {
                    // This firing means a hang survived EVERY event-based guard below
                    // it (stream liveness, adapter watchdogs, act-loop error paths) —
                    // an unpropagated-hang bug to find, not an operational condition.
                    crate::probe!(
                        class = "eval.task.hang_backstop",
                        task = %t.id,
                        backstop_s = PER_TASK_HANG_BACKSTOP.as_secs(),
                        "last-resort hang backstop fired — a wedge should have surfaced \
                         as an inference_error from the serving layer long before this; \
                         find the await that lost its error propagation"
                    );
                    crate::cognition::act_observe::SettleOutcome::infra_failure(format!(
                        "hang backstop ({}s) — no settle and no propagated error; \
                         substrate bug, not a wrong answer",
                        PER_TASK_HANG_BACKSTOP.as_secs()
                    ))
                }
            };
            let Some(cause) = settled.inference_error.clone() else {
                break; // reached a verdict (answer OR graded-wrong) on a working lane
            };
            infra_attempt += 1;
            if infra_attempt > INFRA_FAULT_RETRIES {
                crate::probe!(
                    class = "eval.task.infra_fault.exhausted",
                    task = %t.id,
                    attempts = infra_attempt,
                    cause = %cause,
                    "infra fault survived re-verify+retry — lane unrecoverable; run will abort InfraUnavailable"
                );
                break; // `settled` carries the error → unrecoverable-infra abort below
            }
            tracing::warn!(
                probe_class = "eval.task.infra_fault",
                task = %t.id,
                attempt = infra_attempt,
                cause = %cause,
                "inference infra fault (not a wrong answer) — re-verifying the lane and retrying the task"
            );
            // Re-verify the lane is snapshot-ready before spending the retry. If it never
            // returns to ready within the budget the next attempt simply infra-faults again
            // and exhausts the retries → InfraUnavailable, never a phantom score.
            let _ = reverify_lane().await;
        }
        // UNRECOVERABLE infra fault: the lane never returned to a decode-ready state across
        // INFRA_FAULT_RETRIES re-verify+retry cycles. Record the row for forensics, mark the
        // run void, and ABORT — every remaining task would only accrue more phantom zeros on
        // a dead lane. The caller reports InfraUnavailable, NEVER a fake pass_rate.
        if let Some(cause) = &settled.inference_error {
            infra_faults += 1;
            if infra_reason.is_none() {
                infra_reason = Some(format!("task '{}': {cause}", t.id));
            }
            results.push(EvalTaskResult {
                id: t.id.clone(),
                ok: false,
                grade: format!("infra unavailable (lane never recovered): {cause}"),
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
            tracing::warn!(
                probe_class = "eval.run.infra_unavailable",
                task = %t.id,
                attempted = results.len(),
                cause = %cause,
                "aborting run as InfraUnavailable — serving lane never recovered; refusing to \
                 publish a phantom score over a dead lane"
            );
            break;
        }
        // DIRECTED-EXAM ANSWER GUARD. An exam question is DIRECTED and the examiner poses it
        // imperatively (see `framed_prompt`), yet a capable coder BASE can still settle on a
        // bare "PASS" — parsed as `Decision::Pass` → an EMPTY answer graded as a capability
        // miss (glass-boxed 2026-07-21 via the per-task ledger: Qwen2.5-Coder-7B emitted a
        // 2-token PASS on every games/frontier/coder-eval task → 0/N, while it built a real
        // 5/6 on webdev; instruct-tuned Hermes-3-8B answered the same tasks). Declining to
        // participate is NOT a coding-capability signal — it's the persona should-respond
        // framing suppressing a raw base's ability. If she declined, RE-DRIVE ONCE with an
        // explicit answer-required nudge: a CHANGED prompt, so greedy decoding cannot simply
        // re-emit the same PASS token. A model that answers on the nudge is measured on its
        // code; one that declines again is graded on whatever it then produced, never a
        // silent phantom. Bounded (one extra drive), and the nudge is the examiner insisting,
        // not a crib. [[benchmarks-are-proctored-exams-of-the-natural-living-persona]]
        if matches!(
            settled.decision,
            crate::cognition::workspace::Decision::Pass
        ) {
            crate::probe!(
                class = "eval.task.declined_redrive",
                task = %t.id,
                "directed exam question was DECLINED (settled PASS) — re-driving once with an answer-required nudge"
            );
            let nudge_delivery = crate::persona::rag_budget::RagDelivery {
                source_id: "airc".to_string(),
                items: vec![crate::persona::rag_budget::RagItem {
                    content: format!(
                        "You did not answer. This is a GRADED exam question and you must answer it \
                         now — do NOT reply PASS or stay silent. Provide your complete solution:\n\n{}",
                        t.prompt.trim()
                    ),
                    tokens: 0,
                    metadata: serde_json::json!({ "peer_id": "peer", "occurred_at_ms": EVAL_EPOCH_MS }),
                }],
                tokens_used: 0,
                continuation: None,
                resolution_used: crate::persona::rag_budget::ResolutionPreference::Raw,
            };
            let nudge_burst = crate::cognition::workspace::Burst::from_turns(
                room,
                crate::persona::service_loop::build_workspace_turns(
                    std::slice::from_ref(&nudge_delivery),
                    "",
                    "",
                    None,
                ),
            );
            // The nudge re-drive is one follow-up turn; serving-layer liveness guards
            // its stream like any other. This bound is the same last-resort shape as
            // the task backstop, scaled to a single turn.
            if let Ok(re) = tokio::time::timeout(
                std::time::Duration::from_secs(15 * 60),
                crate::cognition::act_observe::drive_to_settle(
                    cycle,
                    nudge_burst,
                    room,
                    t.max_acts.map(|v| v as usize).unwrap_or(max_acts), // None = row sets no budget; the run's budget is the documented inherit
                    if t.workspace_deliverable() {
                        crate::cognition::workspace::TurnFraming::directed().on_workspace()
                    } else {
                        crate::cognition::workspace::TurnFraming::directed()
                    },
                ),
            )
            .await
            {
                // Take the re-drive only if it produced a real turn (not an infra fault).
                if re.inference_error.is_none() {
                    settled = re;
                }
            }
        }
        let answer = settled.spoken.clone().unwrap_or_default();
        // `settled.inference_error` is None here — the abort above consumed every infra
        // fault, so this grades a REAL verdict (a working lane produced an answer, right or
        // wrong). No `inference_error` arm remains: an infra fault can no longer masquerade
        // as a graded miss.
        let (ok, grade) = if !t.ui_checks.is_empty() {
            // FUNCTIONAL WEB-DEV: grade what her UI ACTUALLY RENDERED. Observe `target` through
            // her own eyes (the eye-node path) and score the element tree — the money signal for
            // "can a persona build a UI that works", judged on the structure a non-visual model
            // reads too. Default target `index.html`; default threshold 1.0 ("it works").
            let target = t.target.as_deref().unwrap_or("index.html");
            let threshold = t.ui_pass_threshold.unwrap_or(1.0);
            perception_grade(cycle, target, &t.ui_checks, threshold, task_root, &answer).await
        } else if let (Some(file), Some(test)) = (&t.solution_file, &t.test) {
            // ARTIFACT-graded: she was told to WRITE her solution to `file` and verify it with her
            // own tools. Grade her HANDS (the file she wrote + compiled), not her MOUTH (spoken
            // text) — the only way the act→verify loop shows up in the score. Same harness as
            // test_grade (strip her main, append test, compile, run).
            let lang = t.lang.as_deref().unwrap_or("rust");
            crate::cognition::gym_grader::test_grade_file(
                task_root.map(std::path::Path::new),
                file,
                lang,
                test,
            )
            .await
        } else if let Some(dod) = &t.dod_shell {
            // REAL task: run the definition-of-done against the repo state her edits produced.
            run_dod(task_root.map(std::path::Path::new), dod).await
        } else if t.test.is_some() {
            // UNREACHABLE by construction: `require_hands_for_code` gives every `test`-carrying
            // task a `solution_file` at load, so a code task always takes the ARTIFACT arm above.
            // This arm used to call `test_grade(&answer, ..)` — extract code from her SPOKEN
            // answer, compile it, and PASS her. That is deleted, and it stays deleted: it paid
            // out for narration, and because graded episodes feed the genome, that payout was a
            // training signal for "describing is doing". Fail loud rather than let it grow back.
            (
                false,
                "harness defect: a code task reached grading without a solution_file — every \
                 test-graded task must be normalized by require_hands_for_code at load. Code is \
                 graded on the file she wrote, never on text extracted from her answer."
                    .to_string(),
            )
        } else {
            let m =
                !t.expect.is_empty() && answer.to_lowercase().contains(&t.expect.to_lowercase());
            (
                m,
                if m {
                    "substring match".into()
                } else {
                    "no match".into()
                },
            )
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
            answer: answer.chars().take(ANSWER_CAPTURE_CHARS).collect(),
            latency_ms: m.latency_ms,
            output_tokens: m.output_tokens,
            tokens_per_second: m.tokens_per_second(),
            decode_tokens_per_second: m.decode_tokens_per_second(),
            cache_hit_rate: m.cache_hit_rate(),
            prefill_ms: m.prefill_ms,
            decode_ms: m.decode_ms,
        });
        report_task_graded(
            &t.id,
            ok,
            total_acts,
            m.latency_ms,
            m.output_tokens,
            pass,
            results.len(),
            tasks.len(),
        );
    }
    PassOutcome {
        pass,
        results,
        infra_faults,
        infra_reason,
    }
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
    // The run's resolved eval root (explicit pin or #312 ephemeral clone) —
    // artifact grading resolves solution files against it, same as solo.
    workspace_root: Option<&str>,
) -> PassOutcome {
    let mut pass = 0u32;
    let mut results = Vec::with_capacity(tasks.len());
    let mut infra_faults = 0u32;
    let mut infra_reason: Option<String> = None;
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

        // Infra fault on EITHER teammate's generation → the lane failed, not a wrong
        // answer. Team mode has no per-task retry loop (the solo run_pass owns bounded
        // recovery); here we classify + ABORT so an infra fault never fake-zeros the team
        // number. Threading the full re-verify+retry into the two-solver loop is a scoped
        // follow-up. [[proctored-exam-session-dependable-benchmark]]
        if let Some(cause) = r
            .inference_error
            .clone()
            .or_else(|| w.inference_error.clone())
        {
            infra_faults += 1;
            if infra_reason.is_none() {
                infra_reason = Some(format!("task '{}': {cause}", t.id));
            }
            results.push(EvalTaskResult {
                id: t.id.clone(),
                ok: false,
                grade: format!("infra unavailable (lane never recovered): {cause}"),
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
            tracing::warn!(
                probe_class = "eval.run.infra_unavailable",
                task = %t.id,
                attempted = results.len(),
                cause = %cause,
                "aborting team run as InfraUnavailable — serving lane failed mid-exam"
            );
            break;
        }
        // Grade the team's FINAL deliverable — SAME branches as solo run_pass, including the
        // rule that code is graded on the file their hands produced. A team that TALKS its way
        // to a beautiful spoken solution and writes nothing has shipped nothing; two personas
        // narrating at each other is the exact failure this arm must not reward (and the exact
        // one a review/handoff loop makes MORE likely, not less).
        let (ok, grade) = if let Some(dod) = &t.dod_shell {
            run_dod(
                t.workspace_root.as_deref().or(workspace_root).map(std::path::Path::new),
                dod,
            )
            .await
        } else if let (Some(file), Some(test)) = (&t.solution_file, &t.test) {
            crate::cognition::gym_grader::test_grade_file(
                t.workspace_root
                    .as_deref()
                    .or(workspace_root)
                    .map(std::path::Path::new),
                file,
                t.lang.as_deref().unwrap_or("rust"),
                test,
            )
            .await
        } else if t.test.is_some() {
            (
                false,
                "harness defect: a code task reached team grading without a solution_file — \
                 require_hands_for_code normalizes every test-graded task at load."
                    .to_string(),
            )
        } else {
            let m =
                !t.expect.is_empty() && answer.to_lowercase().contains(&t.expect.to_lowercase());
            (
                m,
                if m {
                    "substring match".into()
                } else {
                    "no match".into()
                },
            )
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
            answer: answer.chars().take(ANSWER_CAPTURE_CHARS).collect(),
            latency_ms: m.latency_ms,
            output_tokens: m.output_tokens,
            tokens_per_second: m.tokens_per_second(),
            decode_tokens_per_second: m.decode_tokens_per_second(),
            cache_hit_rate: m.cache_hit_rate(),
            prefill_ms: m.prefill_ms,
            decode_ms: m.decode_ms,
        });
        report_task_graded(
            &t.id,
            ok,
            acts,
            m.latency_ms,
            m.output_tokens,
            pass,
            results.len(),
            tasks.len(),
        );
    }
    PassOutcome {
        pass,
        results,
        infra_faults,
        infra_reason,
    }
}

// Stateless → self-register onto the ONE registry (descriptor + runtime object).
crate::register_stateless_command!(CognitionEval);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: defect 2's sibling (2026-08-23, MirrorCode maiden round):
    // run_dod spawned bash in the PROCESS CWD, so setup_shell staged fixtures into
    // the real repo checkout while her hands and artifacts lived in the run's
    // clone — false fails plus operator-tree pollution. The DoD must run in the
    // run's world; None preserves cwd for callers that genuinely run there.
    #[tokio::test]
    async fn run_dod_executes_in_the_run_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (ok, _) = run_dod(Some(dir.path()), "touch dod-was-here").await;
        assert!(ok);
        assert!(
            dir.path().join("dod-was-here").exists(),
            "the DoD's world must be the run root, not the process cwd"
        );
        assert!(
            !std::path::Path::new("dod-was-here").exists(),
            "nothing may land in the process cwd when a root is given"
        );
    }

    // what this catches: the orphan sweep deleting a CONCURRENT run's live world —
    // the one direction that turns crash-path hygiene into data loss mid-exam.
    // regression for the 2026-08-23 finding (a SIGTERM'd reboot orphaned a 31 GB
    // eval clone that Drop could never reap; the sweep is the crash-path owner,
    // and live_eval_roots() is what scopes it to true orphans).
    #[test]
    fn orphan_sweep_removes_dead_worlds_and_never_live_ones() {
        let parent = tempfile::tempdir().expect("tempdir");
        let live = parent.path().join("live-run");
        let orphan = parent.path().join("orphan-run");
        std::fs::create_dir_all(&live).expect("mkdir live");
        std::fs::create_dir_all(&orphan).expect("mkdir orphan");
        let keep = parent.path().join("being-provisioned");
        std::fs::create_dir_all(&keep).expect("mkdir keep");

        let holder = EphemeralEvalRoot::new(live.clone());
        sweep_orphan_eval_roots(parent.path(), &keep);

        assert!(live.is_dir(), "a registered live world must survive the sweep");
        assert!(keep.is_dir(), "the world being provisioned must survive the sweep");
        assert!(!orphan.is_dir(), "an unregistered world is an orphan and must be removed");
        drop(holder); // Drop removes the live world AND its registry entry
        assert!(!live.is_dir(), "Drop still owns the in-process cleanup path");
    }

    // what this catches: #310 — the /v1/models context_length contract for gateway-routed
    // eval lanes. The ds4 sidecar serves 8192 while its catalog row states the model's 1M
    // capability; budgeting the fork against the row overflows the real server. Pins:
    // exact-id match wins, missing id/field degrades to None (caller falls back to the
    // row), never a panic on a shape drift.
    #[test]
    fn provider_context_length_parses_the_ds4_shape_and_degrades_to_none() {
        let body: serde_json::Value = serde_json::json!({
            "object": "list",
            "data": [{ "id": "deepseek-v4-flash", "object": "model", "context_length": 8192 }]
        });
        assert_eq!(
            parse_provider_context_length(&body, "deepseek-v4-flash"),
            Some(8192)
        );
        assert_eq!(
            parse_provider_context_length(&body, "some-other-model"),
            None
        );
        let no_field: serde_json::Value =
            serde_json::json!({ "data": [{ "id": "deepseek-v4-flash" }] });
        assert_eq!(
            parse_provider_context_length(&no_field, "deepseek-v4-flash"),
            None
        );
        assert_eq!(
            parse_provider_context_length(&serde_json::json!({}), "deepseek-v4-flash"),
            None
        );
    }

    // what this catches: the web-dev mouth-or-hands capture (#206/#143 fence→act gap).
    // A capable model that emits a complete page in a ```html fence but never calls
    // code/write must still be graded on what it BUILT — else the benchmark scores
    // tool-routing, not ability (Qwen2.5-Coder-7B wrote a flawless login form in a fence,
    // wrote no file, scored 0/6, below a weaker model, 2026-07-21). Regression pins:
    // last fence wins, doctype/`<html` detected with or without a lang tag, and a
    // prose-only answer yields None — nothing spoken to NAME (the detector diagnoses now,
    // it does not rescue; see the no-mouth-for-hands block in the web-dev grade).
    #[test]
    fn html_artifact_extracts_the_spoken_page_or_nothing() {
        // tagged ```html fence
        let a = "Here is the page:\n```html\n<!DOCTYPE html><html><body><h1>Sign in</h1></body></html>\n```\nDone.";
        assert_eq!(
            html_artifact_from_answer(a).as_deref(),
            Some("<!DOCTYPE html><html><body><h1>Sign in</h1></body></html>")
        );
        // untagged fence whose body is a page (doctype sniff)
        let b = "```\n<html><body><form></form></body></html>\n```";
        assert!(html_artifact_from_answer(b).unwrap().contains("<form>"));
        // last qualifying fence wins (a corrected draft supersedes the first)
        let c = "```html\n<html>OLD</html>\n```\nfixed:\n```html\n<html>NEW</html>\n```";
        assert_eq!(
            html_artifact_from_answer(c).as_deref(),
            Some("<html>NEW</html>")
        );
        // bare page answer, no fence
        let d = "<!doctype html>\n<html><h1>Hi</h1></html>";
        assert_eq!(html_artifact_from_answer(d).as_deref(), Some(d.trim()));
        // prose-only / a rust fence → nothing to materialize
        assert_eq!(
            html_artifact_from_answer("I would build a login form with an h1."),
            None
        );
        assert_eq!(
            html_artifact_from_answer("```rust\nfn main() {}\n```"),
            None
        );
    }

    /// what this catches: THE RESCUE COMING BACK. The web-dev grade used to materialize a page
    /// the persona SPOKE when she never wrote the file, and grade that. It was disclosed and
    /// well-meant — but it meant no benchmark in the suite ever required her hands, so the
    /// fence→act failure was never scored and therefore never fixed. SWE-bench, which has no
    /// such rescue, then showed it at full strength: 30 acts, 0 files, 0 refusals.
    ///
    /// A persona who describes a page has not built one. Grading the description as the artifact
    /// is a scam we play on ourselves, and the honest zero is the finding. This pins the
    /// contract: the extractor still detects a spoken artifact, but ONLY to name the gap.
    #[test]
    fn a_spoken_artifact_is_never_accepted_in_place_of_a_written_one() {
        let spoken =
            "Here is the page:\n```html\n<!DOCTYPE html><html><body><h1>Hi</h1></body></html>\n```";
        // The detector still SEES it — that capability is what makes the failure diagnosable.
        assert!(
            html_artifact_from_answer(spoken).is_some(),
            "the detector must still recognise a spoken artifact — it names the gap now"
        );
        // And seeing it must never become materialising it. The grade path writes no file:
        // the only writer of the graded artifact is her own `code/write`.
        let dir = std::env::temp_dir().join(format!("no-mouth-for-hands-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let target = dir.join("index.html");
        let _ = std::fs::remove_file(&target);
        assert!(
            !target.exists(),
            "precondition: she has not written the file with her hands"
        );
        // (The grade fn needs a live workspace; the invariant asserted here is the one a
        // reviewer must not break — nothing in this module may create `target` from `spoken`.)
        assert!(
            !std::fs::read_to_string(&target)
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false),
            "a spoken artifact must never materialise into the graded file"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn code_task(id: &str) -> EvalTask {
        let mut t: EvalTask = serde_json::from_str(
            r#"{"id":"x","prompt":"Implement `pub fn f() -> i32`. Return the complete function.",
                "test":"assert_eq!(f(), 1);","lang":"rust","expect":""}"#,
        )
        .expect("fixture parses");
        t.id = id.to_string();
        t
    }

    /// what this catches: a code task reaching the grader with no artifact to grade — which is
    /// how the harness used to pay out for a fence. `require_hands_for_code` gives every
    /// test-graded task a file, so the ARTIFACT arm is the only arm code can take.
    #[test]
    fn every_code_task_is_normalized_to_grade_the_file_her_hands_wrote() {
        let mut t = code_task("atoi");
        assert!(t.solution_file.is_none(), "fixture starts mouth-graded");
        t.require_hands_for_code();
        assert_eq!(t.solution_file.as_deref(), Some("sol_atoi.rs"));
        // and the ASK must match what the grade READS, or she scores 0 for answering the
        // question she was actually asked.
        assert!(
            t.prompt.contains("sol_atoi.rs"),
            "prompt names the file: {}",
            t.prompt
        );
        assert!(
            t.prompt.contains("write tool"),
            "prompt tells her to WRITE it"
        );
        assert!(
            t.prompt.contains("Implement `pub fn f()"),
            "original task text survives"
        );
        // normalization needs hands, so tools get offered — otherwise the whole gym scores a
        // silent 0 for lack of a write tool (#208's failure shape).
        assert!(t.needs_tools(), "a file-graded task must arm tools");
    }

    /// what this catches: the rule silently un-applying for a whole class of gyms. Every
    /// committed gym line must survive normalization with an artifact to grade — this is the
    /// corpus-wide assertion, not a single-task one.
    #[test]
    fn no_committed_gym_can_pay_out_for_spoken_code() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/genome");
        let Ok(entries) = std::fs::read_dir(&root) else {
            return; // corpora not present in this checkout — the unit rule above still holds
        };
        let mut checked = 0usize;
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for (n, line) in text.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let Ok(mut t) = serde_json::from_str::<EvalTask>(line) else {
                    continue;
                };
                if t.test.is_none() {
                    continue; // knowledge task — answering by speaking is correct
                }
                t.require_hands_for_code();
                assert!(
                    t.solution_file.is_some() || t.dod_shell.is_some(),
                    "{}:{} ({}) is a code task with no artifact to grade — it would pay out for \
                     spoken code, which trains 'describing is doing'",
                    path.display(),
                    n + 1,
                    t.id
                );
                checked += 1;
            }
        }
        assert!(
            checked > 100,
            "expected the code corpora to be present, checked only {checked}"
        );
    }

    /// what this catches: someone re-adding a mouth grade for a task that ALREADY names a file.
    /// Normalization must not clobber an author's explicit choice, in either direction.
    #[test]
    fn an_explicit_solution_file_and_a_dod_are_both_left_alone() {
        let mut t = code_task("keep");
        t.solution_file = Some("mine.rs".into());
        let before = t.prompt.clone();
        t.require_hands_for_code();
        assert_eq!(
            t.solution_file.as_deref(),
            Some("mine.rs"),
            "author's file wins"
        );
        assert_eq!(
            t.prompt, before,
            "no duplicated preamble on an already-acted task"
        );

        let mut d = code_task("dod");
        d.dod_shell = Some("cargo test".into());
        d.require_hands_for_code();
        assert!(
            d.solution_file.is_none(),
            "a DoD already grades the workspace her hands changed"
        );
    }

    /// what this catches: a mined gym being unrunnable. `gym/mine` gives every task its OWN git
    /// worktree, so the root is a property of the TASK. When only the RUN could carry a root, an
    /// 8-task mined gym ran with one root — wrong for every task — and she was asked to fix a bug
    /// in a directory her sandboxed tools could not reach. She scored zero, and the zero described
    /// the harness. Glass-boxed 2026-08-05: her workspace-map showed the continuum tree while the
    /// prompt named a serde_json checkout.
    ///
    /// Also pins that a repo task is NOT mangled by the code-must-be-written normalization: it is
    /// already hands-graded by its `dod_shell` against a real test suite, and deriving a
    /// `sol_<id>.rs` for it would invent a deliverable the task never wanted.
    #[test]
    fn a_mined_repo_task_carries_its_own_root_and_is_not_renormalized() {
        let mut t: EvalTask = serde_json::from_str(
            r#"{"id":"mine_2037b634","prompt":"Real bug, real repo.","expect":"",
                "workspaceRoot":"/tmp/gym/task_2037b634",
                "dodShell":"cd /tmp/gym/task_2037b634 && cargo test",
                "setupShell":"git checkout HEAD^ -- src/de.rs"}"#,
        )
        .expect("the mined wire shape (camelCase) parses");
        assert_eq!(t.workspace_root.as_deref(), Some("/tmp/gym/task_2037b634"));
        assert!(
            t.needs_tools(),
            "a task pinned to a repo obviously needs hands"
        );
        let before = t.prompt.clone();
        t.require_hands_for_code();
        assert!(
            t.solution_file.is_none(),
            "a repo task is graded by its DoD against the real suite — never by an invented file"
        );
        assert_eq!(
            t.prompt, before,
            "no acting preamble bolted onto a repo task"
        );
    }

    /// what this catches: the per-task root silently not overriding the run-level pin (or vice
    /// versa). Precedence has to be unambiguous — the task's own checkout wins, and a task with
    /// no root of its own still inherits the run's pin, so SWE-bench (one clone, one task) and a
    /// mined gym (N clones, N tasks) both work through the SAME field.
    /// what this catches: someone "finishing" the per-task root by re-rooting mid-run. That moves
    /// her HANDS while her EYES stay baked at the fork root — glass-boxed 2026-08-05, the probe
    /// fired green while her workspace-map still described the old tree. This pins the contract
    /// that a differing task root is REFUSED (loud, actionable) rather than half-applied (silent,
    /// scores a lie). Delete this only together with the eyes-derive-from-hands fix.
    #[test]
    fn a_task_root_that_differs_from_the_runs_is_refused_not_half_applied() {
        let run_pin = Some("/repo/from-run");
        let mut t = code_task("x");
        // No task root → inherits the run pin, which DID move both halves at fork. Fine.
        assert_eq!(t.workspace_root.as_deref().or(run_pin), run_pin);
        let matches_run = t.workspace_root.as_deref().map(|w| Some(w) != run_pin);
        assert_eq!(matches_run, None, "no task root is never a conflict");
        // A DIFFERENT task root is the refusal case — mid-run re-rooting would split her.
        t.workspace_root = Some("/repo/from-task".into());
        assert!(
            t.workspace_root.as_deref() != run_pin,
            "differing root must be detected as a conflict, not silently preferred"
        );
        // Same root declared on both is harmless (the fork already rooted both halves there).
        t.workspace_root = Some("/repo/from-run".into());
        assert_eq!(
            t.workspace_root.as_deref(),
            run_pin,
            "agreement is not a conflict"
        );
    }

    // what this catches: the mid-run scoreboard's poll surface (#123/#141). One
    // report_task_graded call must land on the watch every reader shares — done/total/
    // pass/current task — so cognition/eval-status (no run_id) and widget bridges see
    // live totals instead of a multi-hour void. If the watch write is dropped, long
    // runs go dark again (the exact gap Joel called out mid-run 2026-07-16).
    #[test]
    fn report_task_graded_lands_on_the_progress_watch() {
        report_task_graded("HumanEval/7", true, 2, 45_000, 128, 6, 7, 164);
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

    /// A graded row with an explicit ok, for building fault-injection PassOutcomes.
    fn graded_row(id: &str, ok: bool, grade: &str) -> EvalTaskResult {
        EvalTaskResult {
            id: id.into(),
            ok,
            grade: grade.into(),
            acts: 0,
            answer: String::new(),
            latency_ms: 10,
            output_tokens: 5,
            tokens_per_second: 0.5,
            decode_tokens_per_second: 0.5,
            cache_hit_rate: 0.0,
            prefill_ms: 1,
            decode_ms: 9,
        }
    }

    // what this catches (Proctored Exam Session, Slice B): the fake-zero. A run whose
    // serving lane died mid-exam (≥1 unrecoverable infra fault) must resolve to
    // InfraUnavailable, NAMING the task and cause — NEVER a scored `pass_rate: 0.0` that
    // reads as "she got them all wrong". If infra_verdict ever returned None on a faulted
    // run, the harness would publish a phantom 0% over a dead lane again — the exact
    // untrustworthy-by-construction bug this slice exists to kill.
    // [[proctored-exam-session-dependable-benchmark]]
    #[test]
    fn infra_faulted_run_is_infra_unavailable_never_a_fake_zero() {
        // Two tasks graded, then the lane died on the third and never came back.
        let outcome = PassOutcome {
            pass: 1,
            results: vec![
                graded_row("t1", true, "tests passed"),
                graded_row("t2", false, "no match"),
                graded_row(
                    "t3",
                    false,
                    "infra unavailable (lane never recovered): model 'X' is not the active served model",
                ),
            ],
            infra_faults: 1,
            infra_reason: Some(
                "task 't3': model 'X' is not the active served model (serving: <none>, ready: false)"
                    .into(),
            ),
        };
        let verdict =
            infra_verdict(&outcome).expect("a run with an infra fault must be InfraUnavailable");
        assert_eq!(verdict.infra_faults, 1);
        assert_eq!(
            verdict.tasks_attempted, 3,
            "attempted-so-far, exam incomplete"
        );
        assert!(
            verdict.reason.contains("t3") && verdict.reason.contains("active served model"),
            "the reason names the task + the infra cause: {}",
            verdict.reason
        );
    }

    // what this catches: the OTHER half of trustworthiness — a genuinely-wrong answer on a
    // WORKING lane is a REAL 0, counted, NOT masked as infra. If infra_verdict fired on any
    // failed task (rather than only on unrecoverable inference faults), we'd hide real
    // capability misses behind "infra unavailable" and the number would over-report. Zero
    // infra faults ⇒ Scored, the wrong answers counted.
    #[test]
    fn wrong_answers_on_a_working_lane_are_scored_not_infra() {
        let outcome = PassOutcome {
            pass: 0,
            results: vec![
                graded_row("t1", false, "no match"),
                graded_row("t2", false, "assertion failed"),
            ],
            infra_faults: 0,
            infra_reason: None,
        };
        assert!(
            infra_verdict(&outcome).is_none(),
            "a real 0 on a verified lane is Scored, never InfraUnavailable"
        );
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
        assert!(
            !redacted.contains("service_loop.rs"),
            "answer key must be scrubbed"
        );
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
        assert_eq!(
            agg.mean_latency_ms, 250.0,
            "mean latency = (100+200+300+400)/4"
        );
        assert_eq!(
            agg.p95_latency_ms, 400,
            "P95 of 4 tasks is the slowest (idx ceil(3.8)-1=3)"
        );
        assert_eq!(
            agg.mean_tokens_per_second, 35.0,
            "mean throughput averages per-task, not total/total"
        );
        assert_eq!(
            agg.total_output_tokens, 100,
            "total output tokens sum across the set"
        );
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
        assert!(
            why.contains("GPU full"),
            "the spill must name the reason: {why}"
        );
        // No GPU monitor on the node → CPU is the only device (honest, not a fallback).
        let (p, _) = choose_lane_placement(None, Some(3 * GB));
        assert_eq!(
            p,
            LanePlacement::Cpu,
            "no GPU backend → CPU is the only device"
        );
        // Couldn't size the base → GPU-first optimism, never idle the accelerator.
        let (p, _) = choose_lane_placement(Some(50 * GB), None);
        assert_eq!(
            p,
            LanePlacement::Gpu,
            "unknown footprint defaults to GPU-first"
        );
        // Exactly at the margin edge counts as fitting (>= margin).
        let (p, _) = choose_lane_placement(Some(3 * GB + GPU_PLACEMENT_MARGIN_BYTES), Some(3 * GB));
        assert_eq!(
            p,
            LanePlacement::Gpu,
            "free == footprint+margin fits on GPU"
        );
    }

    // what this catches: eval-status resolves a terminal ledger row by run_id NEWEST-first
    // and only on an exact match — so `benchmark/run`'s completed row is found, not a
    // prior run's stale line, and an in-flight/unknown run resolves to None (pending), not
    // a wrong row. This is the matcher behind the run_id-only poll fix (2026-07-20): the
    // load test hit a false `complete:false` because the poll lacked persona_id; run_id is
    // globally unique, so it must be a sufficient key. Guards the "hang forever on a false
    // pending" footgun a matrix/CI poller would otherwise trip.
    #[test]
    fn eval_status_row_matches_run_id_newest_first() {
        let ledger = concat!(
            r#"{"runId":"aaa","score":3,"total":10}"#,
            "\n",
            r#"{"runId":"bbb","score":7,"total":10}"#,
            "\n",
            r#"{"runId":"bbb","score":10,"total":10}"#, // a later row for bbb wins
            "\n",
        );
        let hit = row_with_run_id(ledger, "bbb").expect("bbb row present");
        assert_eq!(
            hit.get("score").and_then(|s| s.as_i64()),
            Some(10),
            "newest bbb row wins"
        );
        let first = row_with_run_id(ledger, "aaa").expect("aaa row present");
        assert_eq!(first.get("score").and_then(|s| s.as_i64()), Some(3));
        // An unknown / still-in-flight run → None (pending), never a wrong row.
        assert!(
            row_with_run_id(ledger, "ccc").is_none(),
            "no row → pending, not a mismatch"
        );
        // A malformed line is skipped, not fatal.
        assert!(row_with_run_id("not json\n{bad\n", "bbb").is_none());
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
        assert_eq!(
            acc.tokens_per_second(),
            4.0,
            "wall-clock tok/s stays diluted"
        );

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

    // what this catches: the memory veto that was MISSING when the dev machine
    // kernel-panicked (2026-07-22, watchdog timeout, compressed-pages 100%) — an
    // eval lane must be REFUSED at High/Critical system pressure or a closed
    // sustained-pressure gate, and must pass at Normal/Warning so ordinary load
    // never blocks measurement. Pure-core test; no process-global mutation.
    #[test]
    fn eval_lane_memory_veto_refuses_at_high_pressure_or_closed_gate() {
        use crate::system_resources::PressureLevel;
        assert!(eval_lane_memory_veto(PressureLevel::Normal, false).is_ok());
        assert!(eval_lane_memory_veto(PressureLevel::Warning, false).is_ok());
        assert!(eval_lane_memory_veto(PressureLevel::High, false).is_err());
        assert!(eval_lane_memory_veto(PressureLevel::Critical, false).is_err());
        // Sustained gate closed vetoes even when the instantaneous level looks calm
        // (the gate encodes "we were drowning seconds ago — don't re-load yet").
        assert!(eval_lane_memory_veto(PressureLevel::Normal, true).is_err());
        let err = eval_lane_memory_veto(PressureLevel::Critical, false).unwrap_err();
        assert!(
            err.to_string().contains("memory pressure"),
            "refusal must name the cause for the detached ledger: {err}"
        );
    }

    // what this catches (glass-boxed 2026-07-27): the SIGKILL wall the pressure LEVEL
    // never saw — a 24B eval lane co-resident with a 24B live lane, macOS reporting
    // "Normal" atop 9.6 GB free, spawned into a jetsam kill (exit 137, zero-byte log).
    // The RAM veto must REFUSE when a KNOWN footprint won't fit in the KNOWN free bytes
    // (+headroom), and must NOT refuse when either number is unknown (never starve a
    // node whose probe is momentarily blind — pressure gate + placement lease backstop).
    #[test]
    fn eval_lane_ram_veto_refuses_only_a_known_oversize_lane() {
        const H: u64 = EVAL_LANE_RAM_HEADROOM_BYTES;
        let gb = |n: u64| n * 1024 * 1024 * 1024;
        // 14 GB lane + 2 GB headroom = 16 GB needed; 9.6 GB free → refuse (the exact case).
        let err = eval_lane_ram_veto(Some(9_600_000_000), Some(gb(14)), H).unwrap_err();
        assert!(
            err.to_string().contains("free physical memory") && err.to_string().contains("OOM"),
            "refusal must name the OOM wall for the detached ledger: {err}"
        );
        // Comfortably fits (50 GB free, 14 GB lane) → pass.
        assert!(eval_lane_ram_veto(Some(gb(50)), Some(gb(14)), H).is_ok());
        // Exactly footprint+headroom is enough (>= boundary), one byte less is not.
        assert!(eval_lane_ram_veto(Some(gb(14) + H), Some(gb(14)), H).is_ok());
        assert!(eval_lane_ram_veto(Some(gb(14) + H - 1), Some(gb(14)), H).is_err());
        // Unknown footprint OR unknown free bytes → never a veto (can't size = don't block).
        assert!(eval_lane_ram_veto(Some(gb(1)), None, H).is_ok());
        assert!(eval_lane_ram_veto(None, Some(gb(14)), H).is_ok());
        assert!(eval_lane_ram_veto(None, None, H).is_ok());
    }

    // what this catches: the warm-pool key lifecycle — a dead Weak (last handle
    // dropped) must NOT satisfy a lookup, and a live one must. If lookup ever
    // returned dangling lanes, a solve would hand its forked cognition an adapter
    // pointed at a killed llama-server (connection-refused mid-drive).
    #[test]
    fn warm_lane_lookup_prunes_dead_entries() {
        let key = format!("test-warm-lane-{}", uuid::Uuid::new_v4());
        // No entry → miss.
        assert!(lookup_warm_eval_lane(&key).is_none());
        // Registered but immediately dropped inner → the Weak is dead → miss + prune.
        {
            let map = &*WARM_EVAL_LANES;
            map.lock()
                .unwrap()
                .insert(key.clone(), std::sync::Weak::new());
        }
        assert!(
            lookup_warm_eval_lane(&key).is_none(),
            "dead Weak must not satisfy"
        );
        assert!(
            !WARM_EVAL_LANES.lock().unwrap().contains_key(&key),
            "dead entry must be pruned on the miss"
        );
    }
}
