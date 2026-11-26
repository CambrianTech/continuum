//! One pass of her dream: recover accepted reviews, drain her decayed memory, then consolidate her
//! fresh experience, or what she was taught, or review her old beliefs. Awaited by her dream part,
//! so one pass of hers runs at a time and none outlives the part.

use super::*;

/// What a pass did, for her need after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Passed {
    /// Nothing waits: she rests until her next input.
    Rested,
    /// The Core is not ready or a measured round holds it: the pass waits for her next input.
    Deferred,
    /// Work ran; `more` when work still waits (a cluster left or beliefs to review).
    Ran { more: bool },
}

impl DreamPart {
    pub(super) async fn pass(&mut self, trigger: DreamTrigger) -> Passed {
        let persona = self.persona;
        let (store, storage_available) = match self.source.review_boundary_store(persona) {
            Ok(store) => (store, true),
            Err(error) => {
                continuum_base::probe!(class = "dream.review_recovery_deferred", persona = %persona, error = %error,
                    "resident credit storage unavailable; retained boundaries will retry");
                (None, false)
            }
        };
        if let Some(store) = &store {
            self.recover_boundaries(store).await;
        }
        let boundary_ids: Vec<Uuid> = if storage_available { self.her.boundaries.pending.iter().copied().collect() } else { Vec::new() };
        let trigger = if boundary_ids.is_empty() { trigger } else { DreamTrigger::CardBoundary };
        let Some(reflector) = self.source.reflector_for(persona) else {
            return Passed::Rested; // no live reflective surface for her here
        };

        // DRAIN, the other half of the source/drain pair ([[source-drain-is-the-universal-pattern]]):
        // admission is the source, this decay sweep the drain. Before it existed engram salience never
        // decayed in a running persona and personas got "set in their ways" (glass-boxed 2026-07-22:
        // Atlas held 4,336 engrams incl. stale consolidations). Every pass runs it, before anything is
        // consolidated, so stale memory fades even with nothing new to dream about. Idempotent and
        // cheap (`last_decayed_ms` guards double-decay); rehearsed knowledge stays strong.
        let decay = crate::cognition::decay_tick::apply_decay_sweep(reflector.admission.recall_metadata(), continuum_base::time::EpochMs::now().millis());
        if decay.engrams_decayed > 0 {
            continuum_base::probe!(class = "hippocampus.decay", persona = %persona, scanned = decay.engrams_scanned,
                decayed = decay.engrams_decayed, protected = decay.engrams_protected,
                "dream drain: decayed the hippocampus (the source/drain pair is now complete)");
        }

        // LANE COURTESY: never fire dream inference into a serving lane that is down or
        // mid-resuscitation (glass-boxed 2026-07-23: a forced-dream storm hammered a wedged lane
        // through its kill+respawn, 814 failed distillations). No lane at all (adapter-only contexts,
        // unit tests) falls through: distill fails fast there and the queue survives.
        let live = crate::inference::llama_server::current_serving();
        if !live.ready && live.active_model.is_some() {
            return Passed::Deferred;
        }
        // EXAM COURTESY (2026-08-24, round 4ae8e14c): a dream landing between two 35k exam
        // deliberations wipes the slot's cached prefix and the exam re-prefills ~90 s. While any
        // measured round is in flight her dream waits.
        if crate::cognition::eval::live_eval_run_id().is_some() || !crate::cognition::eval::detached_evals_in_flight().is_empty() {
            continuum_base::probe!(class = "dream.deferred_for_exam", persona = %persona, "measured round in flight — dream deferred (slot KV courtesy)");
            return Passed::Deferred;
        }

        // Her lived experience (only EPISODIC engrams; Semantic facts are already distilled) not yet
        // folded into a fact, grouped by a shared recall key: mechanical grouping, the learned step is
        // the distillation.
        self.her.begin(&reflector.admission);
        let fresh = self.her.fresh(&reflector.admission);
        let clusters = cluster_by_recall_key(&fresh);
        if clusters.is_empty() {
            if !boundary_ids.is_empty() {
                finish_review_boundaries(&mut self.her, persona, &boundary_ids, 0, store.as_ref()).await;
                return Passed::Rested; // no eligible cluster; never force a singleton or a training bucket
            }
            // A quiet stretch: consolidate what other agents taught her, once per rest, else review
            // her old beliefs.
            if std::mem::replace(&mut self.taught, Taught::Taken) == Taught::Owed && self.consolidate_received().await {
                return Passed::Ran { more: self.review_waits(&reflector) };
            }
            return self.review_only(&reflector).await;
        }

        if trigger == DreamTrigger::CardBoundary {
            continuum_base::probe!(class = "dream.card_boundary", persona = %persona,
                "a settled card is a consolidation boundary: one pass, its generations at her need on the lane");
            let cluster_count = clusters.len();
            return match dream_pass(&reflector, persona, &clusters, &fresh, &mut self.her).await {
                true => {
                    finish_review_boundaries(&mut self.her, persona, &boundary_ids, cluster_count, store.as_ref()).await;
                    Passed::Ran { more: self.work_waits(&reflector) }
                }
                false => {
                    continuum_base::probe!(class = "dream.review_boundary_retry", persona = %persona,
                        acceptances = boundary_ids.len(),
                        "unfinished episode clusters retain their boundary for her next pass");
                    Passed::Ran { more: false } // a failed pass waits for her next input, never a hot retry
                }
            };
        }
        let complete = dream_pass(&reflector, persona, &clusters, &fresh, &mut self.her).await;
        // A cluster that failed to distill waits for her next input: an erroring lane fails fast, and
        // draining it again at once would spin against it.
        Passed::Ran { more: complete && self.work_waits(&reflector) }
    }

    /// Page her accepted reviews in from durable credit, a page per pass.
    async fn recover_boundaries(&mut self, store: &Arc<dyn ReviewBoundaryLedger>) {
        let state = &self.her.boundaries;
        let (after, initialized) = (state.recovery_after.clone(), state.recovery_initialized);
        let recovery = async {
            if !initialized {
                store.prepare().await?;
            }
            store.page(after, REVIEW_RECOVERY_PAGE).await
        };
        match tokio::time::timeout(REVIEW_RECOVERY_IO_BUDGET, recovery).await {
            Ok(Ok(page)) => {
                let state = &mut self.her.boundaries;
                state.recovery_initialized = true;
                state.pending.extend(page.pending);
                state.recovery_after = page.next;
            }
            error => continuum_base::probe!(class = "dream.review_recovery_deferred", persona = %self.persona, error = ?error,
                "accepted credit remains durable; her next pass retries recovery"),
        }
    }

    /// A REVIEW-ONLY pass (#221 slice 2c): a batch of her oldest unreviewed beliefs, if any are old
    /// enough. Belief hygiene asks at her need (Content outside a rest), so it steps only in the
    /// Core's slack and never storms her lane (the 2026-07-26 storm that the cooldown this replaces
    /// guarded against ran before the lane shared the Core by need).
    async fn review_only(&mut self, reflector: &PersonaReflector) -> Passed {
        let beliefs = self.unreviewed(reflector);
        if beliefs.is_empty() {
            return Passed::Rested;
        }
        let judged = review_batch(reflector, self.persona, beliefs, &mut self.her.reviewed).await;
        Passed::Ran { more: judged && self.review_waits(reflector) }
    }

    /// Her oldest beliefs not yet reviewed and old enough to judge.
    fn unreviewed(&self, reflector: &PersonaReflector) -> Vec<Engram> {
        let cutoff = continuum_base::time::EpochMs::now().millis().saturating_sub(REVIEW_MIN_AGE_MS);
        reflector.admission.semantic_beliefs_oldest_excluding(&self.her.reviewed, cutoff, REVIEW_ONLY_BATCH)
    }

    fn review_waits(&self, reflector: &PersonaReflector) -> bool {
        !self.unreviewed(reflector).is_empty()
    }

    /// Whether any of her work still waits after a pass: a cluster still fresh (one that failed) or a
    /// belief to review.
    fn work_waits(&self, reflector: &PersonaReflector) -> bool {
        self.fresh_clusters(reflector) || self.review_waits(reflector)
    }

    /// Whether a fresh cluster of hers waits to be consolidated.
    pub(super) fn fresh_clusters(&self, reflector: &PersonaReflector) -> bool {
        !cluster_by_recall_key(&self.her.fresh(&reflector.admission)).is_empty()
    }

    /// An autonomic RECEIVED-LESSON consolidation: what other agents taught her (`memory/share`
    /// #2025) into her genome, once per rest, benchmark-gated by the L3 sentinel. The watermark
    /// keeps it idempotent: only lessons newer than the last consolidated one. Returns whether it
    /// was dispatched (no executor or no ready lane dispatches nothing).
    async fn consolidate_received(&mut self) -> bool {
        let Some(executor) = CONSOLIDATE_EXECUTOR.cloned() else { return false };
        let live = crate::inference::llama_server::current_serving();
        if !live.ready {
            return false;
        }
        let Some(base_model) = live.active_model.clone() else { return false };
        let persona = self.persona;
        // Dispatched AS her (her LocalPersona identity; the command re-gates the Privileged
        // training submit internally).
        let conn = continuum_client::Connection::new(crate::runtime::InProcessTransport::new(
            executor,
            Some(crate::routing::CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(persona))),
        ));
        // snake_case: memory/consolidate's params are persona_id / base_model / since_timestamp
        // (camelCase silently failed every autonomic consolidation, measured 2026-08-25).
        let mut params = serde_json::json!({ "persona_id": persona.to_string(), "base_model": base_model });
        if let Some(ts) = &self.her.watermark {
            params["since_timestamp"] = serde_json::Value::String(ts.clone());
        }
        match conn.commands().execute_value("memory/consolidate", params).await {
            Ok(v) => {
                if let Some(ts) = v.get("latest_consolidated_ts").and_then(|t| t.as_str()) {
                    self.her.watermark = Some(ts.to_string());
                }
                continuum_base::probe!(class = "dream.consolidate_received", persona = %persona,
                    consolidated = ?v.get("consolidated").and_then(|c| c.as_u64()),
                    "autonomic received-lesson consolidation dispatched to the training flywheel");
            }
            Err(e) => tracing::warn!(persona = %persona, error = %e, "dream: autonomic received-lesson consolidation failed (deferrable)"),
        }
        true
    }
}
