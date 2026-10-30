use super::*;

impl TrainingTriggerState {
    pub(super) async fn dispatch_pending_inner(&self, key: &BucketKey) -> Result<DispatchResult, String> {
        let Some(active) = self.0.pending.read().await.get(key).cloned() else {
            return Ok(DispatchResult::NoPending);
        };

        match active.state.load(Ordering::Relaxed) {
            ActiveState::Idle => {}
            ActiveState::InProgress => {
                // Already in progress; skip this tick.
                return Ok(DispatchResult::AlreadyActive);
            }
            ActiveState::Finished => {
                // Finished but not yet cleaned up; fall through to finish_held.
            }
        }

        let active = match self.0.pending.read().await.get(key) {
            Some(a) if a.state.load(Ordering::Relaxed) == ActiveState::Idle => a.clone(),
            _ => return Ok(DispatchResult::NoPending),
        };

        // Mark in-progress to prevent concurrent dispatches.
        active.state.store(ActiveState::InProgress, Ordering::SeqCst);

        let started_at = Instant::now();
        log::debug!(
            "training_trigger.dispatch: persona={}, trait_kind={}",
            key.persona_id,
            key.trait_kind,
        );

        let dispatch_result = self.dispatch_example(key, &active).await;

        match dispatch_result {
            Ok(()) => {
                log::debug!(
                    "training_dispatch.complete: persona={}, trait_kind={}",
                    key.persona_id,
                    key.trait_kind,
                );
                active.state.store(ActiveState::Finished, Ordering::SeqCst);
                let took = started_at.elapsed();
                self.finish_held(key, &active, took).await?;
            }
            Err(e) => {
                log::error!(
                    "training_dispatch.failed: persona={}, trait_kind={}, error={}",
                    key.persona_id,
                    key.trait_kind,
                    e,
                );
                active.state.store(ActiveState::Finished, Ordering::SeqCst);
                let took = started_at.elapsed();
                self.finish_held(key, &active, took).await?;
            }
        }

        Ok(DispatchResult::Done)
    }

    pub(super) async fn finish_held(&self, key: &BucketKey, active: &ActiveDispatch, took: Took) -> Result<DispatchResult, String> {
        // Clean up the pending entry if the dispatch finished successfully.
        if let ActiveState::Finished = active.state.load(Ordering::SeqCst) {
            self.0.pending.write().await.remove(key);
        }

        continuum_base::probe!(class = "training.dispatch", persona = %key.persona_id, trait_kind = %key.trait_kind, took_ms = took.as_millis(), status = "finish_held");
        
        Ok(DispatchResult::Done)
    }

    pub(super) async fn finish_dispatch(&self, key: &BucketKey, active: &ActiveDispatch, took: Took) -> Result<DispatchResult, String> {
        // Clean up the pending entry if the dispatch finished successfully.
        if let ActiveState::Finished = active.state.load(Ordering::SeqCst) {
            self.0.pending.write().await.remove(key);
        }

        continuum_base::probe!(class = "training.dispatch", persona = %key.persona_id, trait_kind = %key.trait_kind, took_ms = took.as_millis(), status = "finish_dispatch");
        
        Ok(DispatchResult::Done)
    }

    pub(crate) async fn recover_tick(self: &Arc<Self>) -> Result<(), String> {
        // 1. Scan for pending buckets that need dispatching.
        let mut guard = self.0.pending.write().await;
        if guard.is_empty() {
            return Ok(());
        }

        // Collect keys to avoid holding the lock during async work.
        let keys: Vec<BucketKey> = guard.keys().cloned().collect();
        
        drop(guard);

        for key in keys {
            let result = self.dispatch_pending_inner(&key).await?;
            
            if matches!(result, DispatchResult::NoPending | DispatchResult::AlreadyActive) {
                continue;
            }

            // If dispatch failed or was otherwise not fully successful, re-add to pending for retry.
            if !matches!(result, DispatchResult::Done) {
                let mut guard = self.0.pending.write().await;
                guard.insert(key.clone(), ActiveDispatch::new());
            }
        }

        // 2. Re-dispatch any accepted training that remains owned by the durable trigger.
        // These are cases where an example was accepted but the dispatch loop missed it, 
        // or the worker failed to process it in time. We rely on the durable layer's 
        // `accepted` set as the source of truth for unprocessed examples.
        let accepted_keys: Vec<BucketKey> = self.0.accepted.keys().cloned().collect();
        
        if !accepted_keys.is_empty() {
            log::debug!(
                "training_trigger.recover_tick: re-dispatching {} accepted buckets",
                accepted_keys.len(),
            );

            for key in accepted_keys {
                let result = self.dispatch_pending_inner(&key).await?;
                
                if matches!(result, DispatchResult::Done) {
                    // Successfully dispatched and completed; remove from accepted.
                    self.0.accepted.write().await.remove(&key);
                } else if matches!(result, DispatchResult::NoPending | DispatchResult::AlreadyActive) {
                    continue;
                }
            }
        }

        // 3. Run base-change migration for buckets that have no active/retryable/pending-recovery attempt.
        super::migration::migrate_base_change_buckets(self).await.map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Two indexed metadata pages, never historical examples. Cursors wrap after
    /// an exhausted page, so an earlier allocated sequence that committed later
    /// remains discoverable on the next pass.
    pub(super) async fn recovery_heads(&self, persona: &PersonaId, trait_kind: TraitKind) -> Result<(u64, u64), String> {
        let mut tx = self.0.db.begin_tx().map_err(|e| e.to_string())?;

        let last_sequence_id = self.get_last_sequence_id(&mut tx, persona, trait_kind).await?;
        
        // Read the two pages from the cursor storage.
        let page_1 = self.read_page(&mut tx, persona, trait_kind, 0).await?;
        let page_2 = self.read_page(&mut tx, persona, trait_kind, 1).await?;

        Ok((page_1, page_2))
    }

    pub(super) async fn restore_pending_bucket(&self, key: &BucketKey) -> Result<(), String> {
        // If the bucket is already pending, do nothing.
        if self.0.pending.read().await.contains_key(key) {
            return Ok(());
        }

        // Check if it's in accepted but not dispatched yet.
        if self.0.accepted.read().await.contains_key(key) {
            self.0.pending.write().await.insert(
                key.clone(), 
                ActiveDispatch::new()
            );
            return Ok(());
        }

        // Otherwise, it's a fresh pending bucket that needs to be tracked.
        self.0.pending.write().await.insert(
            key.clone(), 
            ActiveDispatch::new()
        );

        Ok(())
    }

    pub(crate) async fn resolve_dispatch(&self, persona: &PersonaId, trait_kind: TraitKind, sequence_id: u64) -> Result<(), String> {
        // Mark the example as resolved in the durable store.
        let mut tx = self.0.db.begin_tx().map_err(|e| e.to_string())?;
        
        self.mark_resolved(&mut tx, persona, trait_kind, sequence_id).await?;

        // If this was the last pending for this bucket, clean up.
        let pending_count = self.count_pending_for_bucket(&tx, persona, trait_kind).await?;
        if pending_count == 0 {
            self.0.pending.write().await.remove(&BucketKey::new(*persona, *trait_kind));
        }

        Ok(())
    }
}