use super::*;

/// The committed, discriminating coder set used when the caller passes neither
/// inline `tasks` nor an explicit `eval_set` path. Resolved (not read raw)
/// through [`super::gym::resolve_gym`], so the default — like every committed
/// gym — comes from the embedded registry, CWD- and deployment-independent. A
/// *custom* set still uses `--eval_set <path>` (an existing on-disk file wins);
/// only changing a *committed* gym needs a rebuild — the right trade, since the
/// committed gyms must be reliable. The whole class of "core launched from a
/// different cwd → file-not-found → silent degrade" is killed by going through
/// the resolver instead of `std::fs::read_to_string`.
pub(super) const DEFAULT_EVAL_SET: &str = "docs/genome/coder-eval.jsonl";

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
pub(super) const DEFAULT_MAX_ACTS: u32 = 32;

/// Fixed "exam epoch" stamped on every task's burst (`[t=<ms>] peer: …`). The
/// live burst carries each message's real `occurred_at_ms`; the eval pins it to
/// a constant so the perceived prompt — and therefore the greedy reward metric —
/// is byte-reproducible across runs. Value is an arbitrary plausible ms epoch;
/// only its constancy matters.
pub(super) const EVAL_EPOCH_MS: u64 = 1_700_000_000_000;

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
/// always floored at [`MIN_SERVE_CTX`](crate::inference::planning::MIN_SERVE_CTX)
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
pub(super) fn plan_eval_lane_ctx(base: &crate::model_registry::Model) -> u32 {
    use crate::inference::planning::MIN_SERVE_CTX;
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
pub(super) fn exam_fit_window(base: &crate::model_registry::Model) -> Option<u32> {
    let fp = crate::cognition::serving_plan::footprint::footprint_for(base)?;
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
pub(super) fn stable_window_rung(window: u32) -> u32 {
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
/// down when the LAST handle drops; accounting waits for actual exit — one authority, nothing
/// fights ([[resource-authority-is-a-system-concern]], #56).
///
/// Named fields, NOT a positional tuple, so new lane state threads as ONE field
/// ([[structs-by-reference-not-massive-param-lists]]).
pub(crate) struct EvalLaneInner {
    /// The throwaway server requests retirement on drop (#59); its process owner
    /// retains the governed reservation until actual child exit. `None` for a
    /// gateway-routed lane on an EXTERNAL provider (#310): the model is served by someone
    /// else's process (ds4 sidecar, cloud row) — there is nothing to spawn, hold, or kill.
    pub(super) lane: Option<crate::inference::llama_server::EphemeralServingLane>,
    /// Adapter pinned to THIS lane (never the global serving root). `pub(crate)` so the
    /// teacher path (`genome/teach`) can generate against a DEDICATED bare-base lane —
    /// the same isolation that makes the eval score trustworthy — instead of the live
    /// multi-LoRA serving lane that OOMs the Metal backend on a real generation (#175).
    pub(crate) adapter: std::sync::Arc<dyn crate::ai::adapter::AIProviderAdapter>,
    /// The lane's REAL served `/props` window — what the fork's cognition budgets against.
    pub(crate) served_ctx: u32,
    /// Where + why the lane landed (GPU/CPU), surfaced on the eval result.
    pub(super) placement: PlacementEvidence,
    /// Manifest identity captured when this lane loaded its gene; never re-resolved after evaluation.
    pub(super) measured_gene: Option<crate::forge::adapter_manifest::TrainedAdapter>,
}

/// A cheap, cloneable handle to a (possibly shared) [`EvalLaneInner`]. Field reads
/// (`lane.adapter`, `lane.served_ctx`, `lane.placement`) work transparently through
/// `Deref`. Dropping a handle only tears the lane down if it was the last one.
#[derive(Clone)]
pub(crate) struct EvalLane {
    pub(super) inner: std::sync::Arc<EvalLaneInner>,
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
pub(super) static WARM_EVAL_LANES: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, std::sync::Weak<EvalLaneInner>>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// Guards the WARM-LANE map across a cold spawn: a racer for the same key must find
/// the winner's lane instead of building a second one, which needs the lookup, the
/// build and the insert to be one critical section. A `tokio` mutex because it is held
/// across the spawn `.await` — never a `std` lock across await
/// (docs/architecture/CONCURRENCY-STYLE-GUIDE.md).
///
/// It ALSO used to be the only thing serializing cold-spawns against the GPU, and that
/// half has moved down to `EphemeralServingLane::spawn`'s `EPHEMERAL_SPAWN_GATE` where
/// every ephemeral lane inherits it — the vision sidecar spawns through the same
/// primitive and inherited nothing from this one, which is how three sidecars ended up
/// co-resident. What remains here is the same-key racer concern, which is eval's alone.
///
/// LOCK ORDER: this gate, then the primitive's. That is the only nesting in the tree
/// (the sidecar takes the primitive's alone), so the order cannot invert.
pub(super) static EVAL_LANE_SPAWN_GATE: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

/// Return a handle to a live warm lane for `key`, pruning a dangling entry on a miss.
pub(super) fn lookup_warm_eval_lane(key: &str) -> Option<EvalLane> {
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
pub(super) async fn warm_or_spawn_eval_lane<F, Fut>(key: &str, build: F) -> Result<EvalLane, CommandError>
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
    continuum_base::probe!(class = "eval.lane.acquire", step = "gate_wait", key = %key, "cold-spawn gate: waiting");
    let _gate = EVAL_LANE_SPAWN_GATE.lock().await;
    continuum_base::probe!(class = "eval.lane.acquire", step = "gate_held", key = %key, "cold-spawn gate: held");
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
pub(super) const EVAL_LANE_BASE_PORT: u16 = 58_200;

/// How much of a graded answer to persist in the result. A coder answer (code + reasoning)
/// fits far under this; the generous bound exists ONLY to cap a pathological loop-to-length
/// generation (which is captured up to here, still a diagnosable signal). The old 200-char
/// cap stored only the preamble and blinded every failure diagnosis + correction-corpus mine.
// context-budget-exempt: observability: how much of an answer is CAPTURED into the run ledger for humans, never text sent to a model
pub(super) const ANSWER_CAPTURE_CHARS: usize = 24_000;

/// Per-task detail retained in the DURABLE run ledger row (`append_progress_ledger`).
/// The ledger stored only the summary (score/total), so a detached run's per-task
/// verdicts were unrecoverable — a webdev pass/fail could not be glass-boxed after the
/// fact (glass-boxed 2026-07-21: could not tell whether an `acts=0` webdev miss was the
/// mouth-or-hands capture not firing or the spoken page failing a strict check). We now
/// persist a compact per-task array; the diagnostic `grade` verdict + answer head are kept
/// ONLY for FAILED tasks, so a 164-task humaneval row stays lean while a failing 6-task
/// webdev row is fully inspectable. [[self-test-via-command-feedback-surface-never-blind]]
// context-budget-exempt: observability: ledger rendering width for a failed grade, human-facing only
pub(super) const LEDGER_FAIL_GRADE_CHARS: usize = 800;

// context-budget-exempt: observability: ledger rendering width for a failed answer, human-facing only
pub(super) const LEDGER_FAIL_ANSWER_CHARS: usize = 1_200;

/// Small headroom (bytes) kept free on the GPU so a lane placed right at the edge
/// can't trip Metal's decode-time command-buffer OOM. Deliberately SMALL: the
/// policy is GPU-FIRST — fill the accelerator, aim for ~100% GPU utilization
/// ([[optimization-is-always-first]]; Joel: "near 100% GPU utilization if you're
/// doing it right. Fill in GPU lanes first"). This keeps us off the exact cliff;
/// it is NOT a conservative reserve that idles the GPU.
pub(super) const GPU_PLACEMENT_MARGIN_BYTES: u64 = 256 * 1024 * 1024;

/// Wall-clock TTL for the eval lane's governed VRAM lease (#56/G1). Expiry
/// makes a lease eligible for the governor's reclaim protocol; it is not proof
/// of child exit. The process owner retains the RAII guard until actual exit.
pub(super) const EVAL_LANE_LEASE_TTL_MS: u64 = 30 * 60 * 1000;

/// Where a coexisting eval lane runs + WHY. Rides out on the eval result so a CPU
/// placement is VISIBLE in the harness (a CPU-pinned lane is ~10× slower; surfacing
/// the device + the headroom numbers means a slow run is explained by data, never a
/// silent degrade). The benchmark self-reports which device it measured on.
#[derive(Debug, Clone)]
pub(super) struct PlacementEvidence {
    pub(super) placement: LanePlacement,
    pub(super) device: String,
    pub(super) reason: String,
    pub(super) free_vram_bytes: Option<u64>,
    pub(super) footprint_bytes: Option<u64>,
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
pub(super) fn choose_lane_placement(
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
pub(super) fn decide_eval_lane_placement(
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
        captured_at_ms: EpochMs::now().millis(),
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
pub(super) fn refuse_eval_lane_under_memory_pressure(footprint: Option<u64>) -> Result<(), CommandError> {
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
pub(super) fn eval_lane_footprint(base: &crate::model_registry::Model) -> Option<u64> {
    crate::model_registry::artifacts::resolve_gguf_for_model(base)
        .and_then(|p| std::fs::metadata(&p).ok())
        .map(|m| (m.len() as f64 * 1.25) as u64)
}

/// Free physical memory kept in reserve beyond the lane's own footprint — the OS, the
/// live persona lane's in-flight buffers, and page-cache churn all need slack, and a
/// lane placed at the exact edge is the one jetsam picks. If a bigger cushion is ever
/// wanted it belongs here, one named constant, not scattered magic.
pub(super) const EVAL_LANE_RAM_HEADROOM_BYTES: u64 = 2 * 1024 * 1024 * 1024; // 2 GiB

/// Whether a lane of `footprint` bytes fits in `available` free physical memory with
/// `headroom` to spare. Pure so the load-bearing "would this OOM the machine" decision
/// is unit-tested without process-global memory state. Refuses ONLY when BOTH numbers
/// are known and it genuinely won't fit — an unknown footprint or an unread free-bytes
/// reading is NOT a veto (the pressure-level gate and the placement lease remain the
/// backstops; we never refuse a lane we couldn't size, to avoid starving a node whose
/// probe is momentarily blind).
pub(super) fn eval_lane_ram_veto(
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
pub(super) const EVAL_LANE_PRESSURE_WAIT: std::time::Duration = std::time::Duration::from_secs(180);

/// The deferring form of the memory veto: poll until pressure clears or the
/// bounded deadline lapses (then fail loud with the veto's own error). The
/// veto's error message says "retry when pressure clears" — for a DETACHED
/// solve/eval there is no operator watching to retry, so the system honors its
/// own advice for one bounded window. Never a silent wait: each deferred tick
/// probes, so a stall is visible in the trace, not a mystery hang.
pub(super) async fn await_eval_lane_memory_headroom(footprint: Option<u64>) -> Result<(), CommandError> {
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
                continuum_base::probe!(
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
pub(super) fn eval_lane_memory_veto(
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

pub(super) fn eval_lane_request(bytes: u64) -> crate::resources::LeaseRequest {
    crate::resources::LeaseRequest {
        consumer_id: "eval-lane".to_string(),
        kind: crate::resources::ResourceKind::Vram,
        bytes,
        ttl_ms: EVAL_LANE_LEASE_TTL_MS,
        // The actual process owner retains this grant through observed child exit.
        reclaim_policy: crate::resources::ReclaimPolicy::Pinned,
    }
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
pub(super) fn acquire_eval_lane_slot(
    footprint: Option<u64>,
) -> (
    LanePlacement,
    String,
    Option<crate::resources::LeaseGuard>,
    Option<u64>,
) {
    use crate::resources::{LeaseError, ResourceDaemon};
    match (ResourceDaemon::global(), footprint) {
        (Some(daemon), Some(fp)) => {
            let req = eval_lane_request(fp);
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
pub(super) fn governed_vram_available() -> Option<u64> {
    crate::resources::ResourceDaemon::global().and_then(|d| {
        d.board()
            .kinds
            .iter()
            .find(|k| k.kind == crate::resources::ResourceKind::Vram)
            .map(|k| k.available_bytes)
    })
}
