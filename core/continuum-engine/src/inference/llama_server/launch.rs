//! Launch responsibility of the llama-server process owner.
use super::*;
impl LlamaServerProcess {

    pub(super) async fn launch_prepared(
        &self,
        prepared: Arc<PreparedLocalLaunch>,
        current: &ServingLifecycleCheck<'_>,
        restore_session: Option<&OwnedRestoreSession>,
        mut reservation: Option<crate::resources::LeaseGuard>,
    ) -> Result<(), LlamaServerError> {
        if prepared.endpoint != self.root {
            return Err(LlamaServerError::Spawn(
                "prepared launch belongs to another endpoint".into(),
            ));
        }
        if self.is_live_lane {
            // A new engine process is coming: what the last one could take is not evidence
            // for it (`tail_tools`); the reconcile reads the new one's own `/props`.
            forget_served_engine();
        }
        let restore_generation = restore_session.map(|session| session.expected.lock().clone());
        let restoring = restore_generation.is_some();
        if restoring {
            prepared.validate_restoration(&self.root, &prepared.engine_program).await?;
        }
        let target = &prepared.target;
        let gguf = prepared.gguf.clone();
        let slot_save_dir = prepared.slot_save_dir.clone();
        let page_cache_type = prepared.page_cache_type.clone();
        let invocation = &prepared.invocation;
        let engine_install = prepared.engine_install.clone();
        let (_, port) = split_host_port(&self.root);
        let lanes = target.parallel_lanes();
        let total_ctx = target.served_total_ctx();
        if let Some(receipt) = &engine_install {
            receipt.confirm().await.map_err(LlamaServerError::Spawn)?;
        }
        if let Ok(inputs) = &prepared.local_inputs {
            inputs.validate().await.map_err(LlamaServerError::Spawn)?;
        }
        let endpoint = crate::inference::slots::directory().endpoint(&self.root);
        let mut transition = match restore_generation.as_ref() {
            Some(expected) => {
                endpoint
                    .transition_for_generation_if(expected, &|| {
                        // Check every physical precondition before the caller's admission
                        // callback commits intent/artifact state or closes readiness.
                        let empty = self.child.lock().unwrap().is_none(); // JUSTIFIED: poison means a prior panic mutating the owned child.
                        expected.has_exited() && empty && current()
                    })
                    .await
            }
            None => endpoint.transition_if(current).await,
        }
        .map_err(|()| LlamaServerError::Superseded)?;

        // One server at a time: kill the old child before binding the port.
        // Whether we already OWN a child decides if a stale-orphan reap is needed:
        // a relaunch (we own one) frees the port via `kill_child` and must NOT pay
        // the reclaim wait; a fresh claim (we own none) is where a crashed
        // predecessor's orphan may still hold the canonical port.
        let had_own_child = self.child.lock().unwrap().is_some();
        // The engine being replaced holds the residents' freshest KV in its slots, newer than
        // any page on disk. Save each attributed resident before the kill, through the same
        // owner the restore session uses: on the M5 at 2026-10-23 05:09Z a relaunch killed two
        // residents' slots unsaved and their next acts re-prefilled 70k-115k tokens. The
        // transition has drained every reader, so no turn is decoding. A failed save
        // defers replacement rather than destroying the only fresh resident state.
        if had_own_child && !restoring {
            let started = std::time::Instant::now();
            let outcome = transition.checkpoint_before_replacement(&self.client, &self.root, current).await;
            continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "inference.kv_page.relaunch_checkpoint",
                ok = outcome.is_ok(),
                error = %outcome.as_ref().err().map_or("", String::as_str),
                ms = started.elapsed().as_millis() as u64,
                "resident checkpoint decision before engine replacement"
            );
            outcome.map_err(LlamaServerError::Spawn)?;
        }
        if self.is_live_lane && !had_own_child && !restoring {
            if let Err(error) = self.retire_adopted(&mut transition, current).await {
                handle_retirement_failure(&transition, &error);
                return Err(error);
            }
        }
        self.kill_child();

        // A freed listener is not proof that the predecessor stopped decoding.
        // Poll ONLY its retained owned Child; cancelled/failed transitions remain
        // closed, and the next attempt waits on the same generation's receipt.
        if let Some(previous) = transition.previous_generation() {
            let wait = async {
                let mut tick = tokio::time::interval(Duration::from_millis(100));
                loop {
                    tick.tick().await;
                    PAGE_DIR_GUARD.lock().collect_exited();
                    if previous.has_exited() {
                        break;
                    }
                }
            };
            tokio::time::timeout(Duration::from_secs(10), wait)
                .await
                .map_err(|_| {
                    LlamaServerError::Spawn(
                        "predecessor child exit remains unverified; endpoint stays suspended"
                            .into(),
                    )
                })?;
        }

        // An occupied listener without retirement authority is never reaped by
        // command name. Owned and adopted predecessors were handled above.
        if !crate::inference::lane_process::wait_port_free(port, Duration::from_secs(8)).await {
            let occupant = match crate::inference::lane_process::pid_listening_on_port(port) {
                Some(pid) => match crate::inference::lane_process::command_name(pid) {
                    Some(command) => format!("pid {pid}, command {command:?}"),
                    None => format!("pid {pid}, command unavailable"),
                },
                None => "listener identity unavailable".to_owned(),
            };
            return Err(LlamaServerError::Spawn(format!(
                "port {port} remains occupied ({occupant}); no retirement authority for this listener. \
                 Have its machine/deployment owner stop it through its supported lifecycle, \
                 verify the port is free, then retry serving reconciliation"
            )));
        }
        self.mmproj_on_lane.store(
            if prepared.mmproj { 2 } else { 1 },
            std::sync::atomic::Ordering::Relaxed,
        );
        self.launched_prompt_cache_mib.store(
            target.host_prompt_cache_mib,
            std::sync::atomic::Ordering::Relaxed,
        );
        // KV disk-paging dir (restore economy): one per MODEL, so a relaunch at
        // another window keeps every page (the engine refuses, loudly, one that
        // no longer fits its slot; the ledger refuses another model's). Tracked +
        // eviction-decided in system_resources (the no-new-cache-dir-without-
        // eviction law).
        // Remember this geometry ACROSS RUNS: the next boot's plan serves it first, so
        // the pages under this dir are still restorable after a reboot.
        // The geometry is remembered when the launch SETTLES (serving_daemon: cooldown
        // 1 → 0 with the lane serving), never at spawn — a spawn that gets re-homed within
        // its cooldown must not become the next boot's ceiling.
        // The dirs OTHER live lanes page into: protected from the sweep below. The
        // inventory is the durable lane registry (every recorded llama-server still
        // alive), not a process-local "last spawn" — a core restart that ADOPTS a
        // warm server has no spawn memory of it (Astra's review of #4069). Sweeping
        // at spawn (the prior order) deleted the running server's pages on every
        // relaunch attempt — including attempts that never replaced it (2026-09-15:
        // `--c59171` swept at 13:35 while the 08:44 server was still writing into it;
        // 9 restores then failed "failed to open"). A live lane recorded WITHOUT a
        // page dir (pre-field record) makes the inventory incomplete → no sweep.
        // Reserve NOW — before the dir is even created, before the child exists,
        // before the record is written — so a sibling spawn's sweep (live or
        // ephemeral; they share no gate) sees this dir in the inventory from this
        // instant and can never delete it between mkdir and the reservation (Astra's
        // third review of #4069). Released after the record carries it.
        let (mut page_dir_reservation, page_dir_ready) =
            PageDirReservation::take_and_create(&slot_save_dir);
        // Capture the server's stderr to a per-port log file (#175). llama.cpp prints
        // its load banner AND — critically — the underlying ggml/Metal fault behind a
        // `{"code":500,"message":"Compute error"}` HTTP reply to stderr. The prior
        // `Stdio::null()` threw that root cause away, so a compute-error storm was
        // undiagnosable from outside the process (glass-boxed 2026-07-15: every request
        // 500'd in ~7ms and the WHY went to /dev/null). Fail-soft: if the log can't be
        // opened, fall back to null and say so — an unreadable log must never block
        // serving (same posture as the pidfile below).
        // [[never-blind-feedback-driven-iteration]] [[self-test-via-command-feedback-surface-never-blind]]
        // Capture the server's stderr (#175). llama.cpp prints its load banner AND the
        // underlying ggml/Metal fault behind a `{"code":500}` reply here; `Stdio::null()`
        // threw that root cause away.
        //
        // PIPED, not a raw file fd. Handing the child a file means the CHILD owns the write
        // and nothing can bound it: on 2026-08-05 a wedged slot printed `progress = 1.10`
        // at 1.2 GB/min for four hours and the log reached 172 GB, taking the machine to
        // zero bytes free. `code::child_log` owns the file instead and rotates at a few MB.
        // [[never-blind-feedback-driven-iteration]]
        let log_path = crate::inference::child_log::llama_server_log(port);
        if let Some(dir) = log_path.as_ref().and_then(|p| p.parent()) {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut cmd = prepared.command()?;
        // THE LANE IS ITS OWN PROCESS GROUP (card 59052747). A plain child sat in the
        // core's group, so every group-addressed signal aimed at the core — the stop
        // rail's `kill(-core_pid, SIGTERM)`, launchd reaping a job's group on exit —
        // took the warm engine with it, eleven lines before the reboot path promised to
        // leave it for adoption. Its fate is decided by NAME (the lane registry, the
        // ownership sweep, the adopt-or-reap rail), never by which group it happened
        // to be born into. Windows has no process groups here; ownership does the job.
        #[cfg(unix)]
        cmd.process_group(0);
        if let Some(receipt) = &engine_install {
            receipt.confirm().await.map_err(LlamaServerError::Spawn)?;
        }
        if let Ok(inputs) = &prepared.local_inputs {
            inputs.validate().await.map_err(LlamaServerError::Spawn)?;
        }
        // The weights file the engine is about to open, so an adapter trained in this lane names
        // the bytes it read, not whatever the registry resolves later (card 3fd98325). A stat
        // that fails disarms only that provenance: the lane serves either way.
        let weights = match tokio::task::spawn_blocking({
            let gguf = gguf.clone();
            move || crate::inference::weights_stamp::WeightsStamp::of(&gguf)
        })
        .await
        {
            Ok(Ok(stamp)) => Some(stamp),
            Ok(Err(e)) => {
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.weights_unstamped",
                    artifact = %gguf.display(),
                    error = e.to_string().as_str(),
                    "the lane's weights could not be stamped: adapters trained in it cannot be returned across the grid",
                );
                None
            }
            Err(e) => {
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.weights_unstamped",
                    artifact = %gguf.display(),
                    error = e.to_string().as_str(),
                    "the weights stamp task failed: adapters trained in this lane cannot be returned across the grid",
                );
                None
            }
        };
        // Clone before OS spawn, so allocation cannot strand an uninstalled child.
        let child_install = engine_install.clone();
        let child_inputs = prepared.local_inputs.as_ref().ok().cloned();
        // Admit the generation before creating a child: a refused predecessor
        // cannot leave a newly spawned process outside the retained owner.
        let generation = transition
            .start_generation()
            .map_err(LlamaServerError::Spawn)?;
        *self.last_started.lock() = Some(generation.clone());
        // No await separates generation creation, receipt advance and actual child
        // installation. A failed OS spawn marks this exact generation exited.
        if let Some(session) = restore_session {
            *session.expected.lock() = generation.clone();
        }
        let mut child = match cmd.stdout(Stdio::null()).stderr(Stdio::piped()).spawn() {
            Ok(child) => child,
            Err(error) => {
                generation.observed_exit(); // No child was created for this generation.
                return Err(LlamaServerError::Spawn(format!("{}: {error}", prepared.engine_program)));
            }
        };
        let child_pid = child.id();
        let child_stderr = child.stderr.take();
        if let Some(grant) = reservation.take() {
            *self.pending_resource_reservation.lock() = Some(grant);
        }
        self.install_child(child, generation.clone(), child_install, child_inputs);
        // Launch arguments are evidence of what we requested, not a readback of
        // cache capacity or occupancy. Adopted engines do not pass this seam.
        continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.prompt_cache.launch",
            model = target.model.id.as_str(),
            artifact = %gguf.display(),
            engine = %prepared.engine_program,
            pid = ?child_pid,
            port = port,
            requested_mib = target.host_prompt_cache_mib as u64,
            applied_mib = "unobserved",
            total_ctx = total_ctx as u64,
            lanes = lanes as u64,
            requested_cache_type_k = ?invocation.value_of("--cache-type-k"),
            requested_cache_type_v = ?invocation.value_of("--cache-type-v"),
            omitted_cache_type_source = "engine_default_unobserved",
            checkpoints = ?invocation.value_of("--ctx-checkpoints"),
            checkpoint_min_step = ?invocation.value_of("--checkpoint-min-step"),
            draft_model = ?invocation.value_of("--spec-draft-model"),
            serialized_state_bytes = "unobserved",
            rejection_count = "unobserved",
            "spawned engine with requested cache limit; applied limit and serialized-state telemetry are not exposed by current engine API"
        );
        // Hand stderr to its reader, with the engine's watchers on every line
        // ([`super::stderr_sink`]).
        let (watch, debug_flag) = self.stderr_watch();
        let attached = match (child_stderr, child_pid) {
            (Some(stderr), Some(pid)) => super::stderr_sink::attach(stderr, pid, port, watch).await,
            _ => Err("the engine's stderr or pid could not be taken".to_string()),
        };
        if let Err(reason) = attached {
            // An engine nobody reads: its faults would go unseen, and a full pipe would block
            // its writes. It does not serve (card 1d7d6d4c).
            self.kill_child();
            return Err(LlamaServerError::Spawn(format!("{}: {reason}", prepared.engine_program)));
        }

        // Record the child pid so a SIGKILLed core's successor can reclaim THIS
        // port instead of fleeing to a GPU-competing one. Live lane only — the
        // ephemeral lane owns its own scanned port and must not touch the canonical
        // pidfile. A write failure DISARMS future reclaim but must NOT fail the
        // serve: the server is up and serving; the pidfile is a recovery aid, not a
        // serving precondition. Surface it loud (probe) without aborting.
        if self.is_live_lane {
            match child_pid {
                Some(pid) => {
                    if let Err(e) = crate::inference::lane_pidfile::write(pid) {
                        continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.lane_pidfile",
                            port = port,
                            error = e.to_string().as_str(),
                            "lane pidfile write failed — orphan reclaim disarmed for this run",
                        );
                    }
                }
                None => continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.lane_pidfile",
                    port = port,
                    "spawned child has no pid — orphan reclaim disarmed for this run",
                ),
            }
        }

        // Register EVERY lane we spawn (live AND ephemeral) in the lane registry so
        // a crashed core's successor reaps this exact process instead of leaving a
        // ~6 GB orphan. `lane_pidfile` above only covers the live lane's canonical
        // port; the registry is what closes the ephemeral-lane leak. Same as the
        // pidfile, a write failure only DISARMS future reclaim for this lane — it
        // never fails the serve.
        if let Some(pid) = child_pid {
            let role = if self.is_live_lane {
                crate::inference::lane_registry::LaneRole::Live
            } else {
                crate::inference::lane_registry::LaneRole::Ephemeral
            };
            let rec = crate::inference::lane_registry::LaneRecord {
                pid,
                port,
                role,
                model: target.model_id().to_string(),
                // The SHAPE, so a successor core can size this lane as its own
                // without probing it — the difference between counting our
                // predecessor's bytes as ours and counting them as foreign.
                context_window: target.context_window,
                lanes: target.lanes,
                page_dir: Some(slot_save_dir.clone()),
                engine_bin: Some(PathBuf::from(&prepared.engine_program)),
                // the OS start time with the pid names this exact process (residency step 1)
                started_s: crate::inference::engine_residency::process_start_s(pid).unwrap_or(0), // unwrap_or: 0 never matches a live process, so an unreadable start time holds nothing
                placement: Some(target.placement),
                weights,
            };
            // Record + release under the page-dir guard (one critical section); a
            // failed write keeps the reservation for the lane's lifetime.
            if let Err(e) = page_dir_reservation.handoff(&rec) {
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.lane_registry",
                    port = port,
                    error = e.to_string().as_str(),
                    "lane registry write failed — orphan reclaim disarmed for this lane",
                );
            }
        }

        // A reservation the handoff could not release (record write failed) lives
        // as long as the child does — beside it, not in this frame.
        *self.retained_page_dir.lock() = page_dir_reservation
            .is_held()
            .then_some(page_dir_reservation);
        // Remember the genome set this child was launched with — the truthful
        // catalog record for the relaunch decision (llama.cpp can't report it).
        *self.served_adapters.lock().unwrap() = target.adapter_paths();

        self.wait_ready().await?;
        // Second half of the debug-build gate (see debug_build_watch): the server
        // said so on its own stderr while coming up — refuse the lane, loudly.
        if debug_flag.is_set() {
            if let Some(pid) = child_pid {
                crate::inference::lane_process::kill9(pid);
            }
            continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.debug_build_refused",
                bin = %prepared.engine_program,
                "refused to serve from a DEBUG build (startup stderr) — build for speed"
            );
            return Err(LlamaServerError::Spawn(format!(
                "{} announced itself a DEBUG build on startup (asserts enabled; its speed is not valid). \
                 Rebuild llama-server in release from the canary pin and relaunch.",
                prepared.engine_program
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
                    crate::inference::lane_process::kill9(pid);
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
                    prepared.engine_program,
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
            &prepared.engine_program,
            context,
            slots,
        );
        Ok(())
    }
}

/// Only a refusal before signalling may return admission to the incumbent.
pub(super) fn handle_retirement_failure(
    transition: &crate::inference::slots::EndpointTransition,
    error: &LlamaServerError,
) {
    if let LlamaServerError::RetirementRefused(why) = error {
        let reopened = transition.hand_back_unchanged();
        continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.replace.refused_handed_back",
            reopened,
            why = why.as_str(),
            "a replacement was refused before its incumbent was signalled; the endpoint is handed back as the transition found it"
        );
    }
}
