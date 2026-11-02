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
pub struct EvalLaneInner {
    /// The throwaway server requests retirement on drop (#59); its process owner
    /// retains the governed reservation until actual child exit. `None` for a
    /// gateway-routed lane on an EXTERNAL provider (#310): the model is served by someone
    /// else's process (ds4 sidecar, cloud row) — there is nothing to spawn, hold, or kill.
    pub(super) _lane: Option<crate::inference::llama_server::EphemeralServingLane>,
    /// Adapter pinned to THIS lane (never the global serving root). `pub(crate)` so the
    /// teacher path (`genome/teach`) can generate against a DEDICATED bare-base lane —
    /// the same isolation that makes the eval score trustworthy — instead of the live
    /// multi-LoRA serving lane that OOMs the Metal backend on a real generation (#175).
    pub adapter: std::sync::Arc<dyn crate::ai::adapter::AIProviderAdapter>,
    /// The lane's REAL served `/props` window — what the fork's cognition budgets against.
    pub(crate) served_ctx: u32,
    /// Where + why the lane landed (GPU/CPU), surfaced on the eval result.
    pub(super) placement: crate::inference::single_purpose_lane::PlacementEvidence,
    /// Manifest identity captured when this lane loaded its gene; never re-resolved after evaluation.
    pub(super) measured_gene: Option<crate::genome::adapter_manifest::TrainedAdapter>,
}

/// A cheap, cloneable handle to a (possibly shared) [`EvalLaneInner`]. Field reads
/// (`lane.adapter`, `lane.served_ctx`, `lane.placement`) work transparently through
/// `Deref`. Dropping a handle only tears the lane down if it was the last one.
#[derive(Clone)]
pub struct EvalLane {
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

/// Wall-clock TTL for the eval lane's governed VRAM lease (#56/G1). Expiry
/// makes a lease eligible for the governor's reclaim protocol; it is not proof
/// of child exit. The process owner retains the RAII guard until actual exit.
pub(super) const EVAL_LANE_LEASE_TTL_MS: u64 = 30 * 60 * 1000;

/// The lease terms an eval lane holds its governed slot under.
pub(super) const EVAL_LANE_TERMS: crate::inference::single_purpose_lane::LaneLeaseTerms =
    crate::inference::single_purpose_lane::LaneLeaseTerms { consumer: "eval-lane", ttl_ms: EVAL_LANE_LEASE_TTL_MS };
