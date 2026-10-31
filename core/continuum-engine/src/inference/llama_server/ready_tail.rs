//! The publication tail of a launch: everything from a ready engine to its published pool,
//! ledger and verified target (moved from `launch.rs`, card ea6c9aff), so a launch whose
//! readiness wait ended before its engine answered can finish the SAME engine later.
use super::*;

/// What a launch had decided by the time it waited for readiness: what publishing needs.
pub(super) struct ReadyTail {
    pub(super) prepared: Arc<PreparedLocalLaunch>,
    pub(super) generation: crate::inference::slots::EngineGeneration,
    pub(super) child_pid: Option<u32>,
    pub(super) page_dir_ready: bool,
    pub(super) debug_flag: crate::inference::debug_build_watch::DebugBuildFlag,
    pub(super) restoring: bool,
    /// The candidate's published `tail_tools` evidence: kept when the launch publishes, withdrawn
    /// when the tail is dropped unpublished. `None` on a lane with no evidence cell.
    pub(super) evidence: Option<LaunchedEvidence>,
}

impl LlamaServerProcess {
    /// Publish a ready engine: the debug-build and placement gates, the page dir, the live slot
    /// geometry, the pool and ledger (`transition.ready`) and the verified target. The launch is
    /// borrowed, not taken: its keeper stays armed until publication has committed, so a
    /// publication cancelled at any await before the ledger leaves the launch kept (Codex on
    /// #5545).
    pub(super) async fn publish_ready(
        &self,
        transition: &crate::inference::slots::EndpointTransition,
        tail: &ReadyTail,
    ) -> Result<(), LlamaServerError> {
        let ReadyTail { prepared, generation, child_pid, page_dir_ready, debug_flag, restoring, .. } = tail;
        let (child_pid, page_dir_ready, restoring) = (*child_pid, *page_dir_ready, *restoring);
        let target = &prepared.target;
        let gguf = prepared.gguf.clone();
        let slot_save_dir = prepared.slot_save_dir.clone();
        let page_cache_type = prepared.page_cache_type.clone();
        let invocation = &prepared.invocation;
        let engine_install = prepared.engine_install.clone();
        let (_, port) = split_host_port(&self.root);
        // Second half of the debug-build gate (see debug_build_watch): the server
        // said so on its own stderr while coming up — refuse the lane, loudly.
        if debug_flag.is_set() {
            if let Some(pid) = child_pid {
                continuum_process::kill9(pid);
            }
            continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.debug_build_refused",
                bin = %prepared.engine_program(),
                "refused to serve from a DEBUG build (startup stderr) — build for speed"
            );
            return Err(LlamaServerError::Spawn(format!(
                "{} announced itself a DEBUG build on startup (asserts enabled; its speed is not valid). \
                 Rebuild llama-server in release from the canary pin and relaunch.",
                prepared.engine_program()
            )));
        }
        // READY AND ACCEPTED: this lane owns its page dir. Before the ledger attaches
        // (`transition.ready` below), move the newest legacy per-window dir's pages into it,
        // never a dir another live lane was launched with, and only when the inventory is
        // complete (Astra: a live predecessor's unknown dir is never touched). Census + move
        // under ONE guard (Astra's re-review of #4069). Then publish the dir for the
        // save-side trim — after the gates, and only if the dir exists (mkdir failed
        // = the paging tier is amputated for this serve; nothing to trim).
        if page_dir_ready && migrate_legacy_page_dir_guarded(&slot_save_dir).is_none() {
            continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "inference.kv_page.migration_skipped",
                reason = "a live lane's page dir is unknown (recorded before the field existed)",
                "legacy page dir migration skipped — the live-lane inventory is incomplete"
            );
        }
        if page_dir_ready {
            crate::inference::slots::note_page_dir(&slot_save_dir);
        }
        // PLACEMENT CONTRACT READBACK (#441, Joel 2026-08-15: "models that got fucked by
        // serving and are on cpu. You never catch it and we are waiting an eternity").
        // The governor planned a placement; the engine's own load banner says what it
        // ACTUALLY allocated. A GPU-placement lane that offloaded ZERO layers is the
        // whole model on CPU — it answers /health, decodes at a tenth of the planned
        // speed, and nothing else in the system can see the difference. This is a lease
        // violation, not a tuning note: probe LOUD so the daemon/operator respond,
        // never wait for a human to notice the eternity. (Partial offload is legit —
        // MoE cold-expert `-ot` splits offload a subset by design; only 0-of-N on a
        // GPU-intent lane is unambiguous.) The watcher only REPORTS; lifecycle stays
        // with the daemon, same doctrine as the wedge flag.
        if target.placement != LanePlacement::Cpu {
            // A GPU-intent lane that came up on the CPU is REFUSED, not reported.
            // Before 2026-09-05 this arm only probed; BigMama's 5090 then served
            // Devstral from system RAM behind `ready:true lanes:3` (the engine
            // said "no usable GPU found" on its own stderr, the pre-spawn device
            // probe had said CUDA0). "Ready" must be a verified observation — same
            // shape as the debug-build gate above: kill it, name the evidence, and
            // let the daemon's relaunch/degrade path own what happens next.
            // Joaquin's card 0c4317e1 (serve-time pin-match gap), finder credit.
            // THE CHANNEL FIRST (card 44ddb6fc, Joel: "stdout is never a transport"):
            // the engine's own `/props` says where the weights were ALLOCATED. An
            // older engine has no field — channel unavailable, said so once, and
            // the stderr arms below stay as the compensation until every tier
            // serves the fork.
            let residency = match self.weight_residency().await {
                Ok(Some(r)) => Some(r),
                Ok(None) => {
                    tracing::info!(target: "continuum_core::inference::llama_server", probe_class = "serving.placement.channel_unavailable",
                        port = port,
                        "engine reports no model_weight_buffers on /props — placement read \
                         from stderr only (fork 3ca60da3c carries the channel)"
                    );
                    None
                }
                Err(e) => {
                    tracing::info!(target: "continuum_core::inference::llama_server", probe_class = "serving.placement.channel_unreadable",
                        port = port,
                        error = %e,
                        "/props unreadable at readiness — placement read from stderr only"
                    );
                    None
                }
            };
            let channel_says_cpu = residency
                .as_ref()
                .is_some_and(|r| r.total_bytes() > 0 && r.accelerator_bytes() == 0);
            let declined = self.offload.gpu_declined();
            let zero_offload = matches!(self.offload.get(), Some((0, total)) if total > 0);
            if channel_says_cpu || declined || zero_offload {
                if let Some(pid) = child_pid {
                    continuum_process::kill9(pid);
                }
                let residency_line = residency
                    .as_ref()
                    .map(|r| r.summary())
                    .unwrap_or_else(|| "channel unavailable".to_string()); // unwrap_or_else: no field = the channel could not be read; the label says so, never a placement
                let evidence = if channel_says_cpu {
                    "/props model_weight_buffers: 0 accelerator bytes"
                } else if declined {
                    "engine stderr: \"no usable GPU found\""
                } else {
                    "offload banner: 0 layers on the GPU"
                };
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.placement.refused",
                    port = port,
                    model = target.model_id(),
                    evidence = evidence,
                    residency = residency_line.as_str(),
                    total_layers = self.offload.get().map(|(_, t)| t).unwrap_or(0), // unwrap_or: no banner = 0 layers KNOWN; the evidence field carries the declined line
                    "PLACEMENT VIOLATION: a GPU-placement lane came up ON CPU — refused, not served \
                     (ready must be a verified observation, #441 / card 0c4317e1)"
                );
                return Err(LlamaServerError::Spawn(format!(
                    "{} was planned on the GPU and came up on the CPU ({evidence}; residency {residency_line}); refused rather \
                     than serve every citizen from system RAM behind a green /health. Check the \
                     backend build (a DL-backend build can list CUDA0 from a shell and still fail \
                     to load it under the core) — last stderr:\n{}",
                    prepared.engine_program(),
                    self.stderr_log_tail()
                )));
            }
            if let Some(r) = residency.as_ref() {
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.placement.measured",
                    port = port,
                    model = target.model_id(),
                    accelerator_bytes = r.accelerator_bytes(),
                    total_bytes = r.total_bytes(),
                    residency = r.summary().as_str(),
                    "placement read from the engine's own channel — weights allocated as planned"
                );
            }
            match self.offload.get() {
                Some((0, _)) => unreachable!("zero offload on a GPU lane is refused above"),
                Some(_) => {}
                None => {
                    // No banner observed is a different fact from "reported 0" — an
                    // engine build that never prints it would otherwise read as
                    // healthy forever, which is exactly the silent class this exists
                    // to kill. Say so once, quietly.
                    tracing::info!(target: "continuum_core::inference::llama_server", probe_class = "serving.placement.unreported",
                        port = port,
                        "no offload banner observed by readiness — placement readback \
                         unavailable for this lane (#441)"
                    );
                }
            }
        }
        // GENOME DORMANCY (glass-boxed 2026-07-23): llama.cpp loads every
        // `--lora` at scale 1.0 — ALL ACTIVE, superimposed. Four personas'
        // adapters blended at full scale produced the degenerate-decode plague
        // (hangs, digit-splices, zero-walls, prompt-echo) the moment the
        // comma-fix made stacks REALLY load; the month of prior 'stability'
        // was the only-last-adapter bug accidentally serving one costume. The
        // design is paged activation: catalog LOADED, scales DORMANT (0.0),
        // per-request `lora` field activates. Zero them before ready — a lane
        // is not ready while wearing every costume at once.
        if !target.adapters.is_empty() {
            self.zero_adapter_scales().await;
        }
        let context = self.served_context_window().await?;
        let slots = self.served_lanes().await?;
        if context == 0 || slots == 0 || self.child_exit_status().is_some() {
            return Err(LlamaServerError::Spawn(
                "engine readiness lacks live slot geometry".into(),
            ));
        }
        if let Some(receipt) = &engine_install {
            receipt.confirm().await.map_err(LlamaServerError::Spawn)?;
        }
        if let Ok(inputs) = &prepared.local_inputs {
            inputs.validate().await.map_err(LlamaServerError::Spawn)?;
        }
        if restoring {
            prepared.verify_geometry(context, slots)?;
        }
        // Cold lifecycle metadata for paths already resolved by this launcher: the files
        // whose identity a page's KV rests on (model, adapters, MTP draft). If any revision
        // is unreadable, do not carry saved eligibility across engines. Never probe to
        // rediscover it. The engine itself is not one: its pages carry their own state
        // format, which it checks on restore (`KvPageContract::fingerprint`).
        let revisions = match revision_evidence(
            std::iter::once(gguf.clone())
                .chain(target.adapters.iter().map(|adapter| adapter.path.clone()))
                .chain(prepared.mtp_draft.iter().cloned()),
        ) {
            Ok(revisions) => Some(revisions),
            Err((path, error)) => {
                // Every saved page is unrestorable across this launch (card 689084e7): each
                // activity that had one re-prefills cold, so the miss names its file.
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "inference.kv_page.revisions_unreadable",
                    model = target.model_id(),
                    path = %path.display(),
                    error = %error,
                    paging = page_dir_ready,
                    "a launch revision is unreadable: saved KV pages are not restorable across this launch"
                );
                None
            }
        };
        transition
            .ready(
                &self.root,
                &generation,
                crate::inference::slots::KvPageContract {
                    model_id: target.model.id.clone(),
                    model: gguf,
                    adapters: target.adapter_paths(),
                    page_dir: page_dir_ready.then_some(slot_save_dir),
                    context,
                    slots,
                    cache_type: page_cache_type,
                    flash_attn: prepared.page_flash_attn,
                    revisions,
                },
            )
            .map_err(LlamaServerError::Spawn)?;
        // Name this process in its confirmed-save ledger, so a core that adopts this engine
        // after a deploy restores its saved pages instead of re-prefilling every mind.
        if let Some(pid) = child_pid {
            if let Some(started_s) = crate::inference::engine_residency::process_start_s(pid) {
                crate::inference::slots::stamp_engine_incarnation(&self.root, pid, started_s);
            }
        }
        self.record_verified_target(
            &generation,
            target,
            invocation,
            Some(prepared.clone()),
            prepared.engine_program(),
            context,
            slots,
        );
        Ok(())
    }
}
