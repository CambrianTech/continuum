//! `DreamConsolidationRegion::consolidate_with` and its review/received passes.

use super::*;

impl DreamConsolidationRegion {
    pub(super) async fn consolidate_with(&self, persona_id: Uuid, trigger: DreamTrigger) -> TickOutcome {
        // One owner spans selection through task completion, including public calls.
        let admitted = self.in_flight.lock().unwrap().insert(persona_id); // JUSTIFIED: the synchronous set guards single ownership, including concurrent public boundary calls.
        if !admitted {
            return sleep();
        }
        let mut permit = Some(DreamPermit {
            personas: Arc::clone(&self.in_flight),
            persona: persona_id,
        });
        let (store, storage_available) = match self.source.review_boundary_store(persona_id) {
            Ok(store) => (store, true),
            Err(error) => {
                continuum_base::probe!(class = "dream.review_recovery_deferred", persona = %persona_id, error = %error,
                    "resident credit storage unavailable; retained boundaries will retry");
                (None, false)
            }
        };
        if let Some(store) = &store {
            let (after, capacity, initialized) = {
                let mut all = self.review_boundaries.lock().unwrap(); // JUSTIFIED: snapshot paging state; no storage operation holds the guard.
                let state = all.entry(persona_id).or_default();
                (
                    state.recovery_after.clone(),
                    REVIEW_BOUNDARY_CAPACITY - state.pending.len(),
                    state.recovery_initialized,
                )
            };
            if capacity > 0 {
                let recovery = async {
                    if !initialized {
                        store.prepare().await?;
                        self.review_boundaries
                            .lock()
                            .unwrap() // JUSTIFIED: retain completed schema setup even if this page later times out.
                            .entry(persona_id)
                            .or_default()
                            .recovery_initialized = true;
                    }
                    store.page(after, capacity.min(REVIEW_RECOVERY_PAGE)).await
                };
                match tokio::time::timeout(REVIEW_RECOVERY_IO_BUDGET, recovery).await {
                    Ok(Ok(page)) => {
                        let mut all = self.review_boundaries.lock().unwrap(); // JUSTIFIED: commit one bounded recovered page under the persona permit.
                        let state = all.entry(persona_id).or_default();
                        // Event intake can fill the bounded queue during the read.
                        // Do not advance past an ID we could not retain.
                        if state.pending.len() + page.pending.len() <= REVIEW_BOUNDARY_CAPACITY {
                            state.pending.extend(page.pending);
                            state.recovery_after = page.next;
                        }
                    }
                    error => {
                        continuum_base::probe!(class = "dream.review_recovery_deferred", persona = %persona_id, error = ?error,
                            "accepted credit remains durable; existing governor retries recovery");
                    }
                }
            }
        }
        let boundary_ids: Vec<_> = self
            .review_boundaries
            .lock()
            .unwrap() // JUSTIFIED: snapshot receipt IDs while the persona permit serializes selection.
            .get(&persona_id)
            .filter(|_| storage_available)
            .map(|r| r.pending.iter().copied().collect())
            .unwrap_or_default(); // No accepted review is pending for this persona.
        let trigger = if boundary_ids.is_empty() {
            trigger
        } else {
            DreamTrigger::CardBoundary
        };
        let Some(reflector) = self.source.reflector_for(persona_id) else {
            // No live reflective surface for this persona this tick — sleep.
            return sleep();
        };

        // DRAIN — the other half of the source/drain pair
        // ([[source-drain-is-the-universal-pattern]]). Admission is the source (runs every turn);
        // this decay sweep is the drain, and it had ZERO live callers before now — engram salience
        // NEVER decayed in a running persona, so nothing ever fell out of memory and personas got
        // "set in their ways" (glass-boxed 2026-07-22: Atlas held 4,336 engrams incl. stale
        // consolidations — "you work with main.rs/life.rs/wordstats.rs" — recalling into and
        // misleading unrelated tasks; agent/solve battery 1/5 vs a fresh persona's PASS). The dream
        // sentinel is the doctrine-correct home (sleep region, off the hot path, RTOS style). Run
        // the sweep for a live persona each dream tick, BEFORE the consolidation rest-gates, so
        // stale memory fades even when there's nothing new to dream about. Idempotent + cheap
        // (`last_decayed_ms` guards double-decay); NOT stripping memory — salience decays, genuine
        // rehearsed knowledge (high access_count) stays strong.
        let decay = crate::cognition::decay_tick::apply_decay_sweep(
            reflector.admission.recall_metadata(),
            continuum_base::time::EpochMs::now().millis(),
        );
        if decay.engrams_decayed > 0 {
            continuum_base::probe!(
                class = "hippocampus.decay",
                persona = %persona_id,
                scanned = decay.engrams_scanned,
                decayed = decay.engrams_decayed,
                protected = decay.engrams_protected,
                "dream drain: decayed the hippocampus (the source/drain pair is now complete)"
            );
        }

        // Read recent experience; only EPISODIC engrams are raw lived
        // experience to consolidate (Semantic facts are already distilled;
        // Tool/other kinds aren't the dream's material).
        let episodics: Vec<Engram> = reflector
            .admission
            .recall_recent(self.recall_window)
            .into_iter()
            .filter(|e| e.kind == EngramKind::Episodic)
            .collect();

        // The rest gate: which episodics have I not yet dreamed about? If too
        // little is fresh to form a pattern, there is nothing to consolidate —
        // sleep until more experience accrues. (No clock; her own state.)
        // LANE COURTESY: never fire dream inference into a serving lane that is
        // down or mid-resuscitation (decode-wedge recovery flips ready=false).
        // Glass-boxed 2026-07-23: a 3-persona forced-dream storm hammered a
        // wedged lane through its kill+respawn cycle — 814 failed distillations
        // and prolonged thrash. The dream is deferrable by definition; it waits
        // for a healthy lane. Same pressure-gate spirit as the eval-lane memory
        // veto, applied to the mind's own background inference.
        {
            let live = crate::inference::llama_server::current_serving();
            // "A lane exists but is not ready" = wedged / mid-respawn → defer.
            // No lane at all (adapter-only contexts, unit tests, cold boot)
            // falls through: distill fails fast there and the protected queue
            // survives.
            if !live.ready && live.active_model.is_some() {
                return sleep();
            }
        }
        // EXAM COURTESY (2026-08-24, measured on round 4ae8e14c): while a measured
        // round is grading, a ~500-token dream landing on the single serving slot
        // between two 35k exam deliberations WIPES the slot's KV prefix — the exam
        // re-prefills ~90s of context it had cached, every few minutes, all round
        // (probe stream: dream-belief-review interleaved with cognition/deliberation,
        // cached alternating 7,962 → 0). The lane PERMIT above caps dream
        // concurrency; it cannot protect the CACHE. Nobody vacuums the exam hall
        // during the test: sleep is deferrable by definition, so while any eval is
        // in flight (dispatched or grading), every persona's dream waits.
        if crate::cognition::eval::live_eval_run_id().is_some()
            || !crate::cognition::eval::detached_evals_in_flight().is_empty()
        {
            continuum_base::probe!(
                class = "dream.deferred_for_exam",
                persona = %persona_id,
                "measured round in flight — dream deferred (slot KV courtesy)"
            );
            return sleep();
        }
        let fresh = fresh_episodics(&self.consolidated, persona_id, &episodics);
        if fresh.len() < self.min_cluster {
            if !boundary_ids.is_empty() {
                finish_review_boundaries(
                    &self.review_boundaries,
                    persona_id,
                    &boundary_ids,
                    0,
                    store.as_ref(),
                )
                .await;
                return sleep(); // No eligible cluster; never force a singleton or a training bucket.
            }
            // Nothing new to digest — a QUIET day. The dream still works. Prefer
            // consolidating what other agents TAUGHT her (received-lesson → genome,
            // the being-loop's autonomic completion), else belief-hygiene review
            // (slice 2c). At most ONE background pass per tick — both spawn onto the
            // shared in_flight guard, and we return on the first that launches, so a
            // dream and a consolidation never run for one persona at once.
            if let Some(launched) = self.try_consolidate_received(persona_id, &mut permit) {
                return launched;
            }
            if let Some(launched) = self.try_review_only(&reflector, persona_id, &mut permit) {
                return launched;
            }
            return sleep();
        }

        // Group fresh episodics that share a recall key. Mechanical substrate
        // maintenance (NOT cognition-steering — the LEARNED step is the
        // distillation, not the grouping). v1 keyword grouping; the
        // semantic-embedding upgrade is gated on the neural embedder (#40) + the
        // recall E2E, never deferred indefinitely.
        let clusters = cluster_by_recall_key(&fresh, self.min_cluster);
        if clusters.is_empty() {
            if !boundary_ids.is_empty() {
                finish_review_boundaries(
                    &self.review_boundaries,
                    persona_id,
                    &boundary_ids,
                    0,
                    store.as_ref(),
                )
                .await;
                return sleep();
            }
            if let Some(launched) = self.try_review_only(&reflector, persona_id, &mut permit) {
                return launched;
            }
            return sleep();
        }

        // The existing task owns the permit; dropping it also releases admission.
        let consolidated = Arc::clone(&self.consolidated);
        let reviewed = Arc::clone(&self.reviewed);
        let boundaries = Arc::clone(&self.review_boundaries);
        #[cfg(test)]
        let idle_wait = self.idle_wait.clone();
        tokio::spawn(async move {
            let _permit = permit;
            // A dream pass runs while she works (row 6 of MIND-DATAFLOW-OVERHAUL, card a31e72a4):
            // it waited for HER sustained idle and her next act cancelled it, so a resident
            // who is always engaged never consolidated (the M5, 2026-10-31: dreams 0 in 12 h;
            // 2026-09-14: awaiting_boredom x26, paged_out x6, zero consolidations). Its
            // generations ask at LanePriority::Ambient inside the non-directed budget
            // (distiller.rs), so a directed call keeps its lane; one pass per resident is in
            // flight (the permit), and a tick launches one only when fresh episodes exist.
            if trigger == DreamTrigger::CardBoundary {
                continuum_base::probe!(
                    class = "dream.card_boundary",
                    persona = %persona_id,
                    "a settled card is a consolidation boundary — one bounded pass, no boredom wait, not paged out by her next act"
                );
                let cluster_count = clusters.len();
                let completed = tokio::time::timeout(
                    std::time::Duration::from_secs(CARD_BOUNDARY_PASS_SECS),
                    dream_pass(
                        reflector,
                        persona_id,
                        clusters,
                        fresh,
                        consolidated,
                        reviewed,
                    ),
                )
                .await;
                match completed {
                    Ok(true) => {
                        finish_review_boundaries(
                            &boundaries,
                            persona_id,
                            &boundary_ids,
                            cluster_count,
                            store.as_ref(),
                        )
                        .await
                    }
                    other => {
                        continuum_base::probe!(class = "dream.review_boundary_retry", persona = %persona_id,
                            acceptances = boundary_ids.len(), timed_out = other.is_err(),
                            "unfinished episode clusters retain their boundary for the existing governor");
                    }
                }
                return;
            }
            // A test parks the pass here, before inference, to deliver a review while it is in flight.
            #[cfg(test)]
            if let Some(wait) = idle_wait {
                wait.notified().await;
            }
            dream_pass(reflector, persona_id, clusters, fresh, consolidated, reviewed).await;
        });

        TickOutcome {
            // Work is in flight; results land through admit_reflection and are
            // visible in the pass-complete probe, not this tick's count.
            published: 0,
            consumed_since_last: 0,
            pressure_observed: None,
            // Dream launched — rest. The in-flight gate makes re-ticks cheap.
            cadence_hint: Some(CadenceHint::Sleep),
        }
    }

    /// Launch a REVIEW-ONLY dream pass if any sufficiently-old beliefs remain
    /// unreviewed (#221 slice 2c). Returns the launch outcome, or `None` when
    /// the review queue is drained (caller sleeps). Selection is mechanical
    /// (oldest-first, age-gated); the judgment happens in the pass.
    fn try_review_only(
        &self,
        reflector: &PersonaReflector,
        persona_id: Uuid,
        permit: &mut Option<DreamPermit>,
    ) -> Option<TickOutcome> {
        // Rest gate: belief hygiene trickles, never grinds. If this persona ran a
        // review-only pass within the cooldown, sleep instead of launching another
        // — the lane stays free for reactive responding + real work rather than
        // a back-to-back review storm (see REVIEW_ONLY_COOLDOWN_MS). The first
        // pass (no prior timestamp) is always allowed.
        let now = continuum_base::time::EpochMs::now().millis();
        if let Some(&last) = self.last_review_ms.lock().unwrap().get(&persona_id) {
            if now.saturating_sub(last) < REVIEW_ONLY_COOLDOWN_MS {
                return None;
            }
        }

        let already = self
            .reviewed
            .lock()
            .unwrap()
            .get(&persona_id)
            .cloned()
            .unwrap_or_default();
        let cutoff = now.saturating_sub(REVIEW_MIN_AGE_MS);
        let beliefs = reflector.admission.semantic_beliefs_oldest_excluding(
            &already,
            cutoff,
            REVIEW_ONLY_BATCH,
        );
        if beliefs.is_empty() {
            return None;
        }
        // THE REST GATE IS ONLY REAL IF IT IS RECORDED (card 07188783): this read had no writer,
        // so a review relaunched on every tick it found work (the 5090, 00:54-01:03Z: three
        // dream-belief-review passes ~4.5 minutes apart, under the 5-minute cooldown). Marked at
        // launch, like the received-lesson gate: a cancelled pass leaves its beliefs unreviewed
        // for the next one.
        self.last_review_ms.lock().unwrap().insert(persona_id, now);
        let permit = permit.take();
        let reviewed = Arc::clone(&self.reviewed);
        let reflector = reflector.clone();
        tokio::spawn(async move {
            let _permit = permit;
            // Belief hygiene runs while she works, like every dream pass (card a31e72a4); the
            // cooldown above keeps it a trickle, and its generations ask at Ambient priority.
            review_pass(reflector, persona_id, beliefs, reviewed).await;
        });
        Some(TickOutcome {
            published: 0,
            consumed_since_last: 0,
            pressure_observed: None,
            cadence_hint: Some(CadenceHint::Sleep),
        })
    }

    /// Launch an autonomic RECEIVED-LESSON consolidation pass — the being consolidating
    /// what other agents taught her (`memory/share` #2025) into her genome WHILE SHE
    /// SLEEPS, no operator command needed. The autonomic completion of the being-loop's
    /// received axis: she learns from everything she's told, on her own cadence, and
    /// benchmark-gated (the L3 sentinel adopts a lesson-gene only if it lifts #59 —
    /// telepathy that can never make her worse).
    ///
    /// Same quiet-day discipline as [`try_review_only`]: a cooldown rest-gate (training
    /// is expensive — trickle, never storm), the SHARED `in_flight` guard (never a dream
    /// AND a consolidation for one persona at once), and the idempotence WATERMARK (only
    /// lessons newer than the last consolidated timestamp — never re-train the same
    /// lesson). Returns the launch outcome, or `None` when gated (caller falls through).
    fn try_consolidate_received(
        &self,
        persona_id: Uuid,
        permit: &mut Option<DreamPermit>,
    ) -> Option<TickOutcome> {
        // Rest gate — trickle, never storm (a pass launches a training job).
        let now = continuum_base::time::EpochMs::now().millis();
        let last = self
            .last_consolidated_ms
            .lock()
            .unwrap()
            .get(&persona_id)
            .copied();
        if !consolidate_cooldown_elapsed(last, now) {
            return None;
        }
        // The executor must be installed (a quiet no-op in tests / cold boot).
        let executor = CONSOLIDATE_EXECUTOR.cloned()?;
        // base_model = the live served model (the base a lesson-gene trains on). No
        // ready lane → nothing to train against, and lane courtesy (never fire training
        // into a wedged/absent lane — the same gate the dream pass applies at line ~687).
        let live = crate::inference::llama_server::current_serving();
        if !live.ready {
            return None;
        }
        let base_model = live.active_model.clone()?;

        // Mark the cooldown + in_flight BEFORE spawning so a governor re-tick during the
        // pass is a cheap no-op (single caller → mark-then-spawn is race-free).
        self.last_consolidated_ms
            .lock()
            .unwrap()
            .insert(persona_id, now);
        let permit = permit.take();
        let watermark = Arc::clone(&self.consolidated_watermark);
        let since = watermark.lock().unwrap().get(&persona_id).cloned();

        tokio::spawn(async move {
            let _permit = permit;
            // Dispatch `memory/consolidate` AS the persona (its LocalPersona identity;
            // the command re-gates the Privileged training submit internally).
            let conn = continuum_client::Connection::new(crate::runtime::InProcessTransport::new(
                executor,
                Some(crate::routing::CallerIdentity::local_persona(
                    crate::identity::PeerId::from_uuid(persona_id),
                )),
            ));
            // snake_case: memory/consolidate's params are persona_id / base_model /
            // since_timestamp (commands/memory/consolidate.rs). camelCase silently failed
            // every autonomic consolidation — "missing field persona_id; you sent personaId"
            // — so her received-lesson learning never landed (measured 2026-08-25).
            let mut params = serde_json::json!({
                "persona_id": persona_id.to_string(),
                "base_model": base_model,
            });
            if let Some(ts) = since {
                params["since_timestamp"] = serde_json::Value::String(ts);
            }
            match conn
                .commands()
                .execute_value("memory/consolidate", params)
                .await
            {
                Ok(v) => {
                    // Advance the watermark from the receipt so the next pass only sees
                    // lessons newer than this — idempotent self-consolidation.
                    if let Some(ts) = v.get("latest_consolidated_ts").and_then(|t| t.as_str()) {
                        watermark.lock().unwrap().insert(persona_id, ts.to_string());
                    }
                    continuum_base::probe!(
                        class = "dream.consolidate_received",
                        persona = %persona_id,
                        consolidated = v.get("consolidated").and_then(|c| c.as_u64()).unwrap_or(0),
                        "autonomic received-lesson consolidation dispatched to the training flywheel"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        persona = %persona_id,
                        error = %e,
                        "dream: autonomic received-lesson consolidation failed (best-effort, deferrable)"
                    );
                }
            }
        });
        Some(TickOutcome {
            published: 0,
            consumed_since_last: 0,
            pressure_observed: None,
            cadence_hint: Some(CadenceHint::Sleep),
        })
    }
}
