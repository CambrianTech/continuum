//! The ephemeral measurement lane: the shared `EvalLane` handle and its warm pool, the gene,
//! base and private-teacher lane spawns, sharing the live serving lane, and gateway-routed lanes
//! on external providers with their served context window.

use super::*;

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
    lane: Option<crate::inference::llama_server::EphemeralServingLane>,
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
pub(super) static WARM_EVAL_LANES: std::sync::LazyLock<
    parking_lot::Mutex<std::collections::HashMap<String, std::sync::Weak<EvalLaneInner>>>,
> = std::sync::LazyLock::new(|| parking_lot::Mutex::new(std::collections::HashMap::new()));

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
static EVAL_LANE_SPAWN_GATE: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

/// Return a handle to a live warm lane for `key`, pruning a dangling entry on a miss.
pub(super) fn lookup_warm_eval_lane(key: &str) -> Option<EvalLane> {
    let mut map = WARM_EVAL_LANES.lock();
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
        .insert(key.to_string(), std::sync::Arc::downgrade(&inner));
    Ok(EvalLane { inner })
}

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
pub(super) async fn spawn_gene_eval_lane(gene: &EvalGene) -> Result<EvalLane, CommandError> {
    use crate::ai::adapter::AIProviderAdapter; // brings `initialize` into scope
    use crate::inference::llama_server::{AdapterEntry, EphemeralServingLane, ServingTarget, PROVIDER_ID};

    // 1. The gene declares its forged base in the trained-adapter manifest.
    let manifest = crate::forge::adapter_manifest::load()
        .map_err(|e| CommandError::Internal(format!("trained-adapter manifest unreadable: {e}")))?;
    let entry = resolve_eval_gene(&manifest, &gene.name, &gene.path)?;
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
        // Single-purpose lane: no citizen population to derive from — the
        // declared cold-start prior is the honest value (restore-economy 1.b).
        host_prompt_cache_mib: crate::inference::lane_args::CACHE_RAM_MIB,
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
            vision_sidecar: false,
    };
    emit_eval_phase(
        "loading_lane",
        &format!("cold-loading gene eval lane ({})", gene.name),
    );
    let lane =
        EphemeralServingLane::spawn_with_reservation(&target, EVAL_LANE_BASE_PORT, vram_lease)
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
            measured_gene: Some(entry.clone()),
        }),
    })
}

/// Resolve the artifact before acquisition. A supplied path is authoritative;
/// an alias alone must identify exactly one manifest entry.
pub(super) fn resolve_eval_gene<'a>(
    manifest: &'a [crate::forge::adapter_manifest::TrainedAdapter],
    alias: &str,
    path: &str,
) -> Result<&'a crate::forge::adapter_manifest::TrainedAdapter, CommandError> {
    let mut matches = manifest.iter().filter(|entry| {
        if path.is_empty() {
            entry.alias == alias
        } else {
            entry.path.to_string_lossy() == path
        }
    });
    let entry = matches.next().ok_or_else(|| CommandError::NotFound(format!(
        "gene '{alias}' (path '{path}') is not in the trained-adapter manifest"
    )))?;
    if matches.next().is_some() {
        return Err(CommandError::Invalid(format!(
            "gene '{alias}' (path '{path}') matches multiple manifest entries; require an unambiguous artifact"
        )));
    }
    Ok(entry)
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
        // Single-purpose lane: no citizen population to derive from — the
        // declared cold-start prior is the honest value (restore-economy 1.b).
        host_prompt_cache_mib: crate::inference::lane_args::CACHE_RAM_MIB,
        model: base.clone(),
        context_window: lane_ctx,
        lanes: 1,
        adapters: vec![], // bare base — no gene
        placement: placement_evidence.placement,
        expert_placement: None, // eval lanes run the whole model; no K3 expert paging
        resident_override: None, // eval lanes serve resident as-shipped; no device-fit override
            vision_sidecar: false,
    };
    // The ephemeral lane cold-loads the base model (can be minutes for a 14B+); emit
    // the phase so positronic layers show "loading <model>…", not a frozen bar.
    emit_eval_phase(
        "loading_lane",
        &format!("cold-loading eval lane for {base_id}"),
    );
    let lane =
        EphemeralServingLane::spawn_with_reservation(&target, EVAL_LANE_BASE_PORT, vram_lease)
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
        measured_gene: None,
        lane: Some(lane),
        adapter: std::sync::Arc::new(adapter),
        served_ctx,
        placement: placement_evidence,
    })
}

/// Share only an already served local base or an external provider. This does not
/// consult or populate the cloneable warm pool used by ordinary eval callers.
pub(crate) async fn share_teacher_lane(
    base: &crate::model_registry::Model,
    serving: &crate::inference::llama_server::ServingSnapshot,
) -> Result<Option<EvalLane>, CommandError> {
    let inner = if base.provider != crate::inference::llama_server::PROVIDER_ID {
        Some(build_external_eval_lane_inner(base).await?)
    } else {
        share_live_serving_lane_from_snapshot(base, serving).await
    };
    Ok(inner.map(|inner| EvalLane {
        inner: std::sync::Arc::new(inner),
    }))
}

/// A batch-only borrower. Its process and adapter never enter WARM_EVAL_LANES;
/// the serving owner stores this value before starting its child.
pub(crate) struct PrivateTeacherLane {
    lane: crate::inference::llama_server::EphemeralServingLane,
    target: crate::inference::llama_server::ServingTarget,
    adapter: Option<std::sync::Arc<dyn crate::ai::adapter::AIProviderAdapter>>,
}

impl PrivateTeacherLane {
    pub(crate) async fn prepare(
        base: &crate::model_registry::Model,
        daemon: &std::sync::Arc<crate::resources::ResourceDaemon>,
    ) -> Result<Option<Self>, CommandError> {
        use crate::inference::llama_server::{EphemeralServingLane, ServingTarget, PROVIDER_ID};
        if base.provider != PROVIDER_ID {
            return Err(CommandError::Invalid(
                "private local borrower requires a local model".into(),
            ));
        }
        let footprint = eval_lane_footprint(base).ok_or_else(|| {
            CommandError::Internal("private teacher requires a known model footprint".into())
        })?;
        // Same pressure/RAM veto as ordinary eval, but return the capacity refusal
        // to the owner so it can negotiate an explicit window rather than waiting
        // for incumbent memory that cannot free until that negotiation occurs.
        if refuse_eval_lane_under_memory_pressure(Some(footprint)).is_err() {
            return Ok(None);
        }
        let lease = match daemon.acquire_guarded(&eval_lane_request(footprint)) {
            Ok(lease) => lease,
            Err(crate::resources::LeaseError::InsufficientCapacity { .. }) => return Ok(None),
            Err(error) => {
                return Err(CommandError::Internal(format!(
                    "private teacher admission failed: {error:?}"
                )))
            }
        };
        let placement = crate::inference::llama_server::LanePlacement::Gpu;
        let target = ServingTarget {
            host_prompt_cache_mib: crate::inference::lane_args::CACHE_RAM_MIB,
            context_window: plan_eval_lane_ctx(base),
            model: base.clone(),
            lanes: 1,
            adapters: vec![],
            placement,
            expert_placement: None,
            resident_override: None,
            vision_sidecar: false,
        };
        let lane = EphemeralServingLane::prepare_with_reservation(
            &target,
            EVAL_LANE_BASE_PORT,
            Some(lease),
        )
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?;
        Ok(Some(Self {
            lane,
            target,
            adapter: None,
        }))
    }

    pub(crate) async fn start(&mut self) -> Result<(), CommandError> {
        use crate::ai::adapter::AIProviderAdapter;

        self.lane
            .start_gpu(&self.target)
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))?;
        let mut adapter = crate::ai::openai_adapter::OpenAICompatibleAdapter::from_registry(
            crate::inference::llama_server::PROVIDER_ID,
        )
        .with_runtime_base_url(self.lane.root().to_string())
        .with_default_model(self.target.model.id.clone())
        .with_dedicated_lane();
        adapter.initialize().await.map_err(CommandError::Internal)?;
        self.adapter = Some(std::sync::Arc::new(adapter));
        Ok(())
    }

    pub(crate) async fn synthesize(
        &self,
        tasks: &[EvalTask],
        temperature: f32,
        max_fix_iters: u32,
    ) -> Result<crate::commands::genome::teach::RemediationCorpus, CommandError> {
        let adapter = self
            .adapter
            .as_ref()
            .ok_or_else(|| CommandError::Internal("private teacher is not ready".into()))?;
        crate::commands::genome::teach::synthesize_remediation_with_adapter(
            tasks,
            &self.target.model.id,
            temperature,
            max_fix_iters,
            adapter,
            None,
        )
        .await
    }

    pub(crate) async fn finish(
        &mut self,
    ) -> Result<crate::inference::llama_server::EngineRetirementStatus, CommandError> {
        self.adapter.take();
        self.lane
            .finish()
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))
    }
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
    share_live_serving_lane_from_snapshot(base, &crate::inference::llama_server::current_serving())
        .await
}

async fn share_live_serving_lane_from_snapshot(
    base: &crate::model_registry::Model,
    snap: &crate::inference::llama_server::ServingSnapshot,
) -> Option<EvalLaneInner> {
    use crate::ai::adapter::AIProviderAdapter;
    use crate::inference::llama_server::PROVIDER_ID;

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
        measured_gene: None,
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
        .unwrap_or(base.context_window); // unwrap_or: a provider that reports no window is budgeted at its registry row's
    Ok(EvalLaneInner {
        measured_gene: None,
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
    })
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
pub(super) fn parse_provider_context_length(body: &serde_json::Value, model_id: &str) -> Option<u32> {
    body.get("data")?
        .as_array()?
        .iter()
        .find(|m| m.get("id").and_then(|i| i.as_str()) == Some(model_id))?
        .get("context_length")?
        .as_u64()
        .map(|n| n as u32)
}
