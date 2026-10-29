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
        let restore_generation = restore_session.map(|session| session.expected.lock().clone());
        let restoring = restore_generation.is_some();
        if restoring {
            prepared.validate_restoration(&self.root, &prepared.engine_program).await?;
        }
        let target = &prepared.target;
        let gguf = prepared.gguf.clone();
        let slot_save_dir = prepared.slot_save_dir.clone();
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
        // A launch that stopped waiting while its engine still loaded is finished as the SAME
        // engine, not replaced (card ea6c9aff): replacing it would ask the replacement checkpoint
        // for a ledger that engine never had. Intent is this transition's own admission.
        if !restoring {
            if let Some(tail) = self.recoverable_loading(&prepared) {
                return self.wait_and_publish(&transition, tail).await;
            }
        }

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
                Some(pid) => match continuum_process::command_name(pid) {
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
        // The transition drained the incumbent's readers and the candidate spawned: only now is
        // its `tail_tools` evidence published, for this incarnation. The launch tail carries it,
        // so a launch that fails, is cancelled or is discarded from here on withdraws it.
        let evidence = self.publish_launched_evidence(&prepared.engine_program, child_pid);
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

        let tail = super::ready_tail::ReadyTail {
            prepared: prepared.clone(),
            generation: generation.clone(),
            child_pid,
            page_dir_ready,
            debug_flag,
            restoring,
            evidence,
        };
        self.wait_and_publish(&transition, tail).await
    }

    /// Publish the spawned `pid`'s `tail_tools` evidence from `program`'s slot stamp, on a lane
    /// that has an evidence cell ([`LaunchedEvidence`]).
    pub(super) fn publish_launched_evidence(&self, program: &str, pid: Option<u32>) -> Option<LaunchedEvidence> {
        let (_, port) = split_host_port(&self.root);
        let incarnation = pid.and_then(|pid| crate::inference::engine_residency::EngineIncarnation::of(pid, port));
        self.served_evidence.map(|cell| LaunchedEvidence::publish(cell, program, incarnation))
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
