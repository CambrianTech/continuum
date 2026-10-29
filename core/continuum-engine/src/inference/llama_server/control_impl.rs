//! Control impl responsibility of the llama-server process owner.
use super::*;
#[async_trait]
impl LlamaServerControl for LlamaServerProcess {
    fn paging_recovery_required(&self) -> bool {
        crate::inference::slots::directory()
            .endpoint(&self.root)
            .paging_recovery_required()
    }

    fn mmproj_on_lane(&self) -> Option<bool> {
        LlamaServerProcess::mmproj_on_lane(self)
    }

    fn launched_prompt_cache_mib(&self) -> Option<u32> {
        LlamaServerProcess::launched_prompt_cache_mib(self)
    }

    async fn active_model(&self) -> Result<Option<String>, LlamaServerError> {
        if !crate::inference::slots::directory()
            .endpoint(&self.root)
            .is_ready()
        {
            return Err(LlamaServerError::Unreachable(
                "owned engine transition has not verified readiness".into(),
            ));
        }
        // `/v1/models` reports the id we launched with via `--alias`, so the
        // comparison in `ensure_model_serving` is exact. A connection error
        // means nothing is up (the normal pre-spawn state) → Unreachable.
        let url = format!("{}/models", self.v1_url);
        let resp = self
            .client
            .get(&url)
            .timeout(PROBE_TIMEOUT)
            .send()
            .await
            .map_err(|e| LlamaServerError::Unreachable(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(LlamaServerError::Unreachable(format!(
                "status {}",
                resp.status()
            )));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| LlamaServerError::Unreachable(e.to_string()))?;
        let id = body
            .get("data")
            .and_then(|d| d.as_array())
            .and_then(|arr| arr.first())
            .and_then(|m| m.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        Ok(id)
    }

    async fn active_adapters(&self) -> Result<Vec<String>, LlamaServerError> {
        // The set we launched the current child with. llama.cpp exposes no query
        // for `/lora-adapters` contents, so the truthful answer is what WE loaded.
        Ok(self.served_adapters.lock().unwrap().clone())
    }

    async fn adopt_genome_in_place(
        &self,
        target: &ServingTarget,
    ) -> Result<GenomeInPlace, LlamaServerError> {
        let Some(dir) = engine_train_dir().filter(|_| engine_accepts_train_dir()) else {
            return Ok(GenomeInPlace::Unsupported);
        };
        self.converge_genome(&dir, target).await
    }

    async fn served_context_window(&self) -> Result<u32, LlamaServerError> {
        // The per-slot window the server actually serves is
        // `default_generation_settings.n_ctx` — the launch `-c / --parallel`
        // per-slot value AFTER llama.cpp's internal 256-multiple padding.
        let body = self.props_json().await?;
        // Fail loud if the shape is missing the field — never guess a window. The
        // daemon turns this into "publish the gap" (no ready snapshot), so a
        // malformed /props degrades to not-ready and self-heals next tick rather
        // than poisoning every persona's prompt budget.
        body.get("default_generation_settings")
            .and_then(|s| s.get("n_ctx"))
            .and_then(|v| v.as_u64())
            .map(|n| n as u32)
            .ok_or_else(|| {
                LlamaServerError::Unreachable(
                    "/props missing default_generation_settings.n_ctx (server up but shape \
                     unexpected — refusing to guess the served window)"
                        .to_string(),
                )
            })
    }

    async fn served_engine_build(&self) -> Result<Option<String>, LlamaServerError> {
        let body = self.props_json().await?;
        Ok(body
            .get("build_info")
            .and_then(|v| v.as_str())
            .map(str::to_string))
    }

    async fn served_lanes(&self) -> Result<u32, LlamaServerError> {
        // Same root-level `/props` as the served window. llama.cpp publishes the
        // slot count it was launched with (`--parallel` / `n_seq_max`) as the
        // top-level `total_slots`. A connection error means nothing is up (the
        // normal pre-spawn state) → Unreachable, which the caller reads as
        // "lanes OK" so a probe hiccup never relaunches a healthy lane.
        let url = format!("{}/props", self.root);
        let resp = self
            .client
            .get(&url)
            .timeout(PROBE_TIMEOUT)
            .send()
            .await
            .map_err(|e| LlamaServerError::Unreachable(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(LlamaServerError::Unreachable(format!(
                "status {}",
                resp.status()
            )));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| LlamaServerError::Unreachable(e.to_string()))?;
        // An older build that publishes no `total_slots` is UNKNOWN, not zero —
        // and unknown must not be dressed up as a number the grow-gate compares
        // against. Returning `Err` routes it to the caller's no-spurious-relaunch
        // arm, the same place a probe failure lands.
        body.get("total_slots")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32)
            .ok_or_else(|| {
                LlamaServerError::Unreachable(
                    "/props carries no total_slots (server up but this build does not report \
                     its slot count — refusing to guess the served lane count)"
                        .to_string(),
                )
            })
    }

    async fn multimodal_support(&self) -> Result<Option<MultimodalSupport>, LlamaServerError> {
        // Same root-level `/props` as the served window. llama.cpp publishes a
        // `modalities: { vision: bool, audio: bool }` block once an mtmd
        // projector loads (and `vision:false` when launched text-only). An
        // ABSENT block is `Ok(None)` — "this build can't say", which the pure
        // verdict (`vision_lane_ready`) refuses to treat as sight.
        let url = format!("{}/props", self.root);
        let resp = self
            .client
            .get(&url)
            .timeout(PROBE_TIMEOUT)
            .send()
            .await
            .map_err(|e| LlamaServerError::Unreachable(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(LlamaServerError::Unreachable(format!(
                "status {}",
                resp.status()
            )));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| LlamaServerError::Unreachable(e.to_string()))?;
        Ok(body.get("modalities").map(|m| MultimodalSupport {
            vision: m.get("vision").and_then(|v| v.as_bool()).unwrap_or(false),
            audio: m.get("audio").and_then(|v| v.as_bool()).unwrap_or(false),
        }))
    }

    async fn decode_smoke_ok(&self) -> bool {
        // A REAL multi-token generation, not just HTTP 200. A wedged lane (the
        // "~2 tokens then stop for EVERY request" failure mode, observed intermittently
        // on fresh ephemeral eval lanes) still answers a `max_tokens: 1` request with
        // 200 — so a status-only probe passes it, and every downstream task then decodes
        // ~2 tokens and silently scores 0. The probe forces a prompt a healthy model
        // MUST answer with many tokens and verifies BOTH volume (token floor) and
        // CONTENT (the digits actually count upward — the embedding-model-on-the-port
        // incident sailed past a count-only gate with "uiuiui…" garbage).
        //
        // Judged by [`smoke_verdict`] over BOTH response channels, because a THINKING
        // model can spend the whole quick budget inside its reasoning channel with a
        // perfectly healthy decode path (2026-08-15: this exact shape marked serving
        // degraded forever and killed a full bench round while the server generated
        // at 30 t/s). That outcome is ThinkStarved, not Dead — retry once with a
        // budget that lets the model finish thinking and ANSWER. A wedged lane
        // (~2 tokens) and the garbage-content lane both still read Dead on EITHER
        // budget. A 500 "Compute error" (the wedged-orphan signature) still fails
        // fast on status inside `smoke_attempt`.
        // A lane that has PROVEN its model thinks starts at the thinking budget —
        // the quick attempt is structurally incapable of succeeding there, and paying
        // it anyway cost two generations per readiness check, ~30 identical
        // `think_retry` rows/hour on a lane citizens were starving for. First check
        // per lane still takes the cheap path; the latch below learns from it.
        //
        // NEVER OVER A MIND'S KV. The seat is decided here, once, for every caller (the
        // heartbeat, the reconciler's adoption probe, readiness): the guard used to live in
        // the heartbeat only, and the reconciler re-probed an adopted engine every ensure
        // tick, wiping a resident's slot each time (M5, 2026-10-25). With no free seat the
        // lane's own `/slots` answering is the proof, and the residents' deliveries carry it.
        let slots_url = format!("{}/slots", self.root);
        let slots = match self.client.get(&slots_url).timeout(PROBE_TIMEOUT).send().await {
            Ok(resp) if resp.status().is_success() => resp.json::<serde_json::Value>().await.ok(),
            _ => None,
        };
        let Some(slots) = slots else {
            return false;
        };
        if smoke_seat(scratch_slot_for(&self.v1_url), &slots) == SmokeSeat::Occupied {
            continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.smoke.deferred_to_delivery",
                root = %self.root,
                "every slot holds a mind's tokens and there is no scratch slot: no synthetic \
                 decode (it would replace her KV); the lane answered /slots, and her own \
                 deliveries prove decode"
            );
            return true;
        }
        if self
            .smoke_proven_thinking
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return match smoke_attempt(&self.client, &self.v1_url, THINKING_SMOKE_BUDGET).await {
                Some((tok, visible, rlen)) => {
                    matches!(smoke_verdict(tok, &visible, rlen), SmokeVerdict::Alive)
                }
                None => false,
            };
        }
        let Some((tok, visible, rlen)) =
            smoke_attempt(&self.client, &self.v1_url, QUICK_SMOKE_BUDGET).await
        else {
            return false;
        };
        match smoke_verdict(tok, &visible, rlen) {
            SmokeVerdict::Alive => true,
            SmokeVerdict::Dead => false,
            SmokeVerdict::ThinkStarved => {
                self.smoke_proven_thinking
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.smoke.think_retry",
                    quick_tokens = tok,
                    reasoning_len = rlen as u64,
                    retry_budget = THINKING_SMOKE_BUDGET,
                    "smoke decode proven but the whole quick budget went to private \
                     reasoning — thinking model, not a wedge; retrying with room to \
                     answer, and LATCHING: this lane's checks start at the thinking \
                     budget from now on (one discovery per lane, not one per tick)"
                );
                match smoke_attempt(&self.client, &self.v1_url, THINKING_SMOKE_BUDGET).await {
                    Some((tok, visible, rlen)) => {
                        matches!(smoke_verdict(tok, &visible, rlen), SmokeVerdict::Alive)
                    }
                    None => false,
                }
            }
        }
    }

    async fn slots_activity_fingerprint(&self) -> Option<u64> {
        // Control-plane read (`/slots` is on the HTTP thread, answers while compute
        // is saturated). Any failure → None: the caller must not read a fetch error
        // as either progress or freeze.
        let url = format!("{}/slots", self.root);
        let body: serde_json::Value = self
            .client
            .get(&url)
            .timeout(PROBE_TIMEOUT)
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()?;
        slots_activity_fingerprint_of(&body)
    }

    async fn slots_body(&self) -> Option<serde_json::Value> {
        // Same bounded control-plane read as the fingerprint; any failure → None.
        let url = format!("{}/slots", self.root);
        self.client
            .get(&url)
            .timeout(PROBE_TIMEOUT)
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()
    }

    async fn slots_max_inflight_prompt_tokens(&self) -> Option<u64> {
        // Same control-plane read as the fingerprint; any failure → None
        // (nothing to protect, base patience applies).
        let url = format!("{}/slots", self.root);
        let body: serde_json::Value = self
            .client
            .get(&url)
            .timeout(PROBE_TIMEOUT)
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()?;
        slots_max_inflight_prompt_tokens_of(&body)
    }

    fn owns_child(&self) -> bool {
        self.child.lock().unwrap().is_some()
    }

    async fn record_observed_placement(&self) {
        self.stamp_observed_placement().await;
    }

    fn follow_engine_log(&self) {
        self.follow_served_engine();
    }

    fn served_incarnation(&self) -> Option<crate::inference::engine_residency::EngineIncarnation> {
        let (_host, port) = split_host_port(&self.root);
        crate::inference::lane_registry::live_incarnation().filter(|lane| lane.port == port)
    }

    fn launch_kv(&self) -> Option<crate::inference::lane_process::LaunchKv> {
        let (_host, port) = split_host_port(&self.root);
        let record = crate::inference::lane_registry::live_lane().filter(|r| r.port == port)?;
        let argv = crate::inference::lane_process::command_args(record.pid)?;
        Some(crate::inference::lane_process::launch_kv_in(&argv))
    }

    fn wedge_flag(&self) -> Option<crate::inference::wedge::WedgeFlag> {
        self.wedge.clone()
    }

    fn owned_serving_target(&self) -> Option<OwnedServingTarget> {
        self.child
            .lock()
            .unwrap() // JUSTIFIED: poison means prior panic mutating the owned child.
            .as_ref()
            .filter(|owned| !owned.generation.has_exited())
            .and_then(|owned| owned.verified_target.clone())
    }

    fn owned_engine(&self) -> Option<OwnedEngineIdentity> {
        self.child
            .lock()
            .unwrap() // JUSTIFIED: poison means a prior panic while mutating the owned child.
            .as_ref()
            .map(|owned| OwnedEngineIdentity {
                owner: Arc::downgrade(&self.child),
                generation: owned.generation.clone(),
            })
    }

    async fn retire_owned_engine(
        &self,
        expected: &OwnedEngineIdentity,
    ) -> Result<EngineRetirementReceipt, EngineRetirementError> {
        self.retire_owned_engine_with(expected, tokio::process::Child::start_kill)
            .await
    }

    fn observe_engine_retirement(
        &self,
        receipt: &EngineRetirementReceipt,
    ) -> Result<EngineRetirementStatus, EngineRetirementError> {
        if !receipt.identity.owner.ptr_eq(&Arc::downgrade(&self.child)) {
            return Err(EngineRetirementError::NotOwned);
        }
        PAGE_DIR_GUARD.lock().collect_exited();
        Ok(if receipt.identity.generation.has_exited() {
            EngineRetirementStatus::Exited
        } else {
            EngineRetirementStatus::Pending
        })
    }

    async fn idle(&self) -> Result<(), LlamaServerError> {
        self.idle_if_current(&|| true).await
    }

    async fn idle_if_current(
        &self,
        current: &ServingLifecycleCheck<'_>,
    ) -> Result<(), LlamaServerError> {
        let endpoint = crate::inference::slots::directory().endpoint(&self.root);
        let _transition = endpoint
            .transition_if(current)
            .await
            .map_err(|()| LlamaServerError::Superseded)?;
        let had_own_child = self.child.lock().unwrap().is_some(); // unwrap: poisoned = a prior panic mid-kill; same policy as kill_child's lock
        self.kill_child();
        let (_host, port) = split_host_port(&self.root);
        // An owned child is already retiring under its actual process handle.
        // Canonical reclaim is for an adopted/orphan lane, not a second signal
        // followed by pidfile deletion before our owned child has reported exit.
        let reclaimed = if self.is_live_lane && !had_own_child {
            Some(format!("{:?}", crate::inference::lane_pidfile::reclaim(port).await))
        } else {
            None
        };
        continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.lane.idled",
            port,
            killed_own_child = had_own_child, // Legacy field: signal requested, not proof of exit.
            reclaim = reclaimed.as_deref().unwrap_or("owned child or non-live lane"), // unwrap_or: None labels the deliberate absence of canonical reclaim, not observed process exit
            "lane retirement requested on an empty plan; owned-child exit remains independently observed"
        );
        Ok(())
    }

    async fn serve(&self, target: &ServingTarget) -> Result<(), LlamaServerError> {
        self.serve_if_current(target, &|| true).await
    }

    fn last_launch_generation(&self) -> Option<crate::inference::slots::EngineGeneration> {
        self.last_started.lock().clone()
    }

    async fn serve_if_current(
        &self,
        target: &ServingTarget,
        current: &ServingLifecycleCheck<'_>,
    ) -> Result<(), LlamaServerError> {
        let prepared = self.prepare_local_launch(target).await?;
        self.launch_prepared(Arc::new(prepared), current, None, None)
            .await
    }

    async fn validate_owned_launch(
        &self,
        original: &OwnedServingTarget,
    ) -> Result<(), LlamaServerError> {
        if !original.identity.owner.ptr_eq(&Arc::downgrade(&self.child)) {
            return Err(LlamaServerError::Spawn(
                "original launch belongs to another controller".into(),
            ));
        }
        let prepared = original
            .prepared
            .as_ref()
            .ok_or_else(|| LlamaServerError::Spawn("original launch was not captured".into()))?;
        prepared.validate_restoration(&self.root, &prepared.engine_program).await
    }

    async fn restore_owned_launch(
        &self,
        original: &OwnedServingTarget,
        current: &ServingLifecycleCheck<'_>,
    ) -> Result<(), LlamaServerError> {
        let session = self.begin_owned_restore(original).await?;
        self.restore_owned_session(&session, None, current).await
    }

    async fn begin_owned_restore(
        &self,
        original: &OwnedServingTarget,
    ) -> Result<OwnedRestoreSession, LlamaServerError> {
        self.validate_owned_launch(original).await?;
        let prepared = original
            .prepared
            .as_ref()
            .ok_or_else(|| LlamaServerError::Spawn("original launch was not captured".into()))?;
        Ok(OwnedRestoreSession {
            original: original.clone(),
            expected: parking_lot::Mutex::new(original.identity.generation.clone()),
            _pages: PageDirReservation::take(&prepared.slot_save_dir),
            suspension: parking_lot::Mutex::new(None),
        })
    }

    async fn checkpoint_owned_restore(
        &self,
        session: &OwnedRestoreSession,
        current: &ServingLifecycleCheck<'_>,
    ) -> Result<EngineRetirementReceipt, LlamaServerError> {
        self.validate_owned_launch(&session.original).await?;
        let expected = &session.original.identity;
        self.check_owned_engine(expected)
            .map_err(|e| LlamaServerError::Spawn(e.to_string()))?;
        let endpoint = crate::inference::slots::directory().endpoint(&self.root);
        let mut transition = endpoint
            .transition_for_generation_if(&expected.generation, current)
            .await
            .map_err(|()| LlamaServerError::Superseded)?;
        *session.suspension.lock() = Some(
            transition
                .capture_suspension(&expected.generation)
                .map_err(LlamaServerError::Spawn)?,
        );
        let checkpoint = transition
            .checkpoint_residents(&self.client, &self.root)
            .await
            .map_err(LlamaServerError::Spawn)?;
        self.validate_owned_launch(&session.original).await?;
        if !current() {
            return Err(LlamaServerError::Superseded);
        }
        let drained = checkpoint
            .transition_for(&expected.generation)
            .map_err(LlamaServerError::Spawn)?;
        self.retire_owned_engine_drained(expected, drained, tokio::process::Child::start_kill)
            .map_err(|e| LlamaServerError::Spawn(e.to_string()))
    }

    async fn resume_owned_checkpoint(
        &self,
        session: &OwnedRestoreSession,
        current: &ServingLifecycleCheck<'_>,
    ) -> Result<(), LlamaServerError> {
        self.check_owned_engine(&session.original.identity)
            .map_err(|e| LlamaServerError::Spawn(e.to_string()))?;
        let suspension =
            session.suspension.lock().clone().ok_or_else(|| {
                LlamaServerError::Spawn("original endpoint was not suspended".into())
            })?;
        let endpoint = crate::inference::slots::directory().endpoint(&self.root);
        let transition = endpoint
            .transition_for_generation_if(&session.original.identity.generation, current)
            .await
            .map_err(|()| LlamaServerError::Superseded)?;
        self.check_owned_engine(&session.original.identity)
            .map_err(|e| LlamaServerError::Spawn(e.to_string()))?;
        transition
            .resume_suspension(&suspension, current)
            .map_err(LlamaServerError::Spawn)
    }

    async fn restore_owned_session(
        &self,
        session: &OwnedRestoreSession,
        reservation: Option<crate::resources::LeaseGuard>,
        current: &ServingLifecycleCheck<'_>,
    ) -> Result<(), LlamaServerError> {
        self.validate_owned_launch(&session.original).await?;
        let expected = session.expected.lock().clone();
        if !expected.has_exited() {
            return Err(LlamaServerError::Spawn(
                "restore attempt child exit is not confirmed".into(),
            ));
        }
        let prepared =
            session.original.prepared.as_ref().ok_or_else(|| {
                LlamaServerError::Spawn("original launch was not captured".into())
            })?;
        self.launch_prepared(prepared.clone(), current, Some(session), reservation)
            .await
    }
}
