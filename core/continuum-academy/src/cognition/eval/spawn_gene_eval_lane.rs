use super::*;

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
    let manifest = crate::genome::adapter_manifest::load()
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
    crate::inference::single_purpose_lane::await_lane_memory_headroom(crate::inference::single_purpose_lane::lane_footprint(&base)).await.map_err(CommandError::Internal)?;

    // 3. Host-fit PHYSICAL window (`-c`) for the throwaway lane, derived from the
    //    live serving budget (see `crate::inference::single_purpose_lane::plan_lane_ctx`). One lane: a single
    //    measurement stream, no batching. This sizes the lane's launch `-c` + its
    //    placement; the fork's COGNITION window is read back from the lane's real
    //    `/props` after spawn (below), the same served-truth pin the supervisor
    //    applies to the living persona — so a measurement copy budgets against
    //    exactly what its lane serves, never this planned value
    //    ([[dreaming-mind-eval-must-match-live-cognition]], task #50).
    let lane_ctx = crate::inference::single_purpose_lane::plan_lane_ctx(&base);

    // 3b. GPU-FIRST placement (Joel: fill GPU lanes first, ~100% utilization; CPU is
    //     spillover of last resort, never the default for a coexisting lane). Probe
    //     live free VRAM (net of the living persona's resident lane) + size this base,
    //     then pack the GPU unless it genuinely can't hold this lane. EVIDENCED — the
    //     chosen device + headroom ride out on the result so a CPU spill is VISIBLE in
    //     the harness, not a silent slow path.
    let (placement_evidence, vram_lease) =
        crate::inference::single_purpose_lane::decide_lane_placement(&base, lane_ctx, &format!("eval-lane gene:{}", gene.name), EVAL_LANE_TERMS);

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
            _lane: Some(lane),
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
    manifest: &'a [crate::genome::adapter_manifest::TrainedAdapter],
    alias: &str,
    path: &str,
) -> Result<&'a crate::genome::adapter_manifest::TrainedAdapter, CommandError> {
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
pub(super) async fn build_base_eval_lane_inner(base_id: &str) -> Result<EvalLaneInner, CommandError> {
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
    crate::inference::single_purpose_lane::await_lane_memory_headroom(crate::inference::single_purpose_lane::lane_footprint(&base)).await.map_err(CommandError::Internal)?;
    let lane_ctx = crate::inference::single_purpose_lane::plan_lane_ctx(&base);
    let (placement_evidence, vram_lease) =
        crate::inference::single_purpose_lane::decide_lane_placement(&base, lane_ctx, &format!("eval-lane base:{base_id}"), EVAL_LANE_TERMS);
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
        _lane: Some(lane),
        adapter: std::sync::Arc::new(adapter),
        served_ctx,
        placement: placement_evidence,
    })
}

/// Share only an already served local base or an external provider. This does not
/// consult or populate the cloneable warm pool used by ordinary eval callers.
pub async fn share_teacher_lane(
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
pub struct PrivateTeacherLane {
    pub(super) lane: crate::inference::llama_server::EphemeralServingLane,
    pub(super) target: crate::inference::llama_server::ServingTarget,
    pub(super) adapter: Option<std::sync::Arc<dyn crate::ai::adapter::AIProviderAdapter>>,
}

impl PrivateTeacherLane {
    pub async fn prepare(
        base: &crate::model_registry::Model,
        daemon: &std::sync::Arc<crate::resources::ResourceDaemon>,
    ) -> Result<Option<Self>, CommandError> {
        use crate::inference::llama_server::{EphemeralServingLane, ServingTarget, PROVIDER_ID};
        if base.provider != PROVIDER_ID {
            return Err(CommandError::Invalid(
                "private local borrower requires a local model".into(),
            ));
        }
        let footprint = crate::inference::single_purpose_lane::lane_footprint(base).ok_or_else(|| {
            CommandError::Internal("private teacher requires a known model footprint".into())
        })?;
        // Same pressure/RAM veto as ordinary eval, but return the capacity refusal
        // to the owner so it can negotiate an explicit window rather than waiting
        // for incumbent memory that cannot free until that negotiation occurs.
        if crate::inference::single_purpose_lane::refuse_lane_under_memory_pressure(Some(footprint)).is_err() {
            return Ok(None);
        }
        let lease = match daemon.acquire_guarded(&crate::inference::single_purpose_lane::lane_request(EVAL_LANE_TERMS, footprint)) {
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
            context_window: crate::inference::single_purpose_lane::plan_lane_ctx(base),
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

    pub async fn start(&mut self) -> Result<(), CommandError> {
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

    pub async fn synthesize(
        &self,
        tasks: &[EvalTask],
        temperature: f32,
        max_fix_iters: Option<u32>,
    ) -> Result<crate::genome::teach::RemediationCorpus, CommandError> {
        let adapter = self
            .adapter
            .as_ref()
            .ok_or_else(|| CommandError::Internal("private teacher is not ready".into()))?;
        crate::genome::teach::synthesize_remediation_with_adapter(
            &teach_tasks(tasks),
            &self.target.model.id,
            temperature,
            max_fix_iters,
            adapter,
            None,
        )
        .await
    }

    pub async fn finish(
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
/// box. It can never fit, so `crate::inference::single_purpose_lane::await_lane_memory_headroom` deferred every 5 s for
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
pub(super) async fn share_live_serving_lane(base: &crate::model_registry::Model) -> Option<EvalLaneInner> {
    share_live_serving_lane_from_snapshot(base, &crate::inference::llama_server::current_serving())
        .await
}

pub(super) async fn share_live_serving_lane_from_snapshot(
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
            continuum_base::probe!(
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
    continuum_base::probe!(
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
        _lane: None,
        adapter: std::sync::Arc::new(adapter),
        // The live lane's own `/props` truth, the same authority the living persona
        // budgets against (task #50) — never a recomputed plan value.
        served_ctx: snap.served_context_window,
        placement: crate::inference::single_purpose_lane::PlacementEvidence {
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
pub(super) async fn build_external_eval_lane_inner(
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
        measured_gene: None,
        _lane: None,
        adapter: std::sync::Arc::new(adapter),
        served_ctx,
        // `LanePlacement` drives llama-server spawn flags, which never apply here;
        // `Cpu` carries the honest zero-VRAM-contention semantics, and the device
        // string names the real host.
        placement: crate::inference::single_purpose_lane::PlacementEvidence {
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

/// #312 — the exam's disposable world: a CoW clone of the shared checkout under the
/// system temp dir, one per run. clonefile on APFS / reflink on btrfs-XFS (instant,
/// block-sharing), plain recursive copy where the FS lacks CoW — the same platform
/// split as citizen-layer provisioning (`ensure_citizen_layer_from_base`). `cfg!`
/// (not `#[cfg]`) so both arms type-check on every platform.
pub(super) fn provision_ephemeral_eval_root(tag: &str) -> Result<std::path::PathBuf, String> {
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
    continuum_base::probe!(
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

pub(super) fn write_eval_run_marker(root: &std::path::Path, run_id: &str) {
    let marker = serde_json::json!({
        "runId": run_id,
        "corePid": std::process::id(),
        "startedAtMs": EpochMs::now().millis(),
    });
    // Marker write is best-effort: the run must not die because its guard
    // breadcrumb could not be written; the reboot guard just loses this run.
    // Written through the eval-roots class the run's root lives in (a disk marker read by
    // the CLI process — a true process boundary).
    let written = (|| -> std::io::Result<()> {
        use continuum_kernel::system_resources::disk_reporters::tracked_dir;
        let class = tracked_dir("eval-roots").ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "the eval-roots disk class is not registered")
        })?;
        let within = root.strip_prefix(class.path()).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("{} is not under the eval-roots class {}", root.display(), class.path().display()),
            )
        })?;
        class.writer().replace(&within.join(EVAL_RUN_MARKER), marker.to_string().as_bytes())
    })();
    if let Err(e) = written {
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
pub(super) static LIVE_EVAL_ROOTS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashSet<std::path::PathBuf>>,
> = std::sync::OnceLock::new();

pub(super) fn live_eval_roots() -> &'static std::sync::Mutex<std::collections::HashSet<std::path::PathBuf>> {
    LIVE_EVAL_ROOTS.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
}
