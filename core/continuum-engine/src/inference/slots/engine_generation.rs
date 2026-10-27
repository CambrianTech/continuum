use super::*;

impl EngineGeneration {
    pub(crate) fn same_engine(&self, other: &Self) -> bool {
        self.id == other.id
    }

    pub(crate) fn retiring(&self) {
        if let Some(endpoint) = self.endpoint.upgrade() {
            let state = endpoint.state.lock();
            if state.generation.as_ref().is_some_and(|g| g.id == self.id) {
                state.ready.send_replace(false);
            }
        }
    }
    pub(crate) fn has_exited(&self) -> bool {
        self.exited.load(Ordering::Acquire)
    }

    pub fn observed_exit(&self) {
        self.exited.store(true, Ordering::Release);
        if let Some(endpoint) = self.endpoint.upgrade() {
            let state = endpoint.state.lock();
            if state.generation.as_ref().is_some_and(|g| g.id == self.id) {
                state.ready.send_replace(false);
                continuum_base::probe!(
                    class = "inference.kv_engine.exited",
                    generation = %self.id,
                    "owned engine exit observed; endpoint admissions remain closed"
                );
                // Do not mutate a ledger while an old HTTP future can still write
                // it. Replacement readiness drains admissions and replaces it.
            }
            drop(state);
            // A relaunch that could not save waited for this death to record its loss.
            endpoint.relaunch_exit_observed(self.id);
        }
    }
}

impl EndpointSlots {
    pub(super) fn quarantine_paging(&self) {
        let mut state = self.state.lock();
        state.paging_uncertain = true;
        state.ready.send_replace(false);
        continuum_base::probe!(
            class = "inference.kv_page.uncertain",
            "paging completion unverified; endpoint requires verified engine replacement"
        );
    }

    pub fn paging_recovery_required(&self) -> bool {
        self.state.lock().paging_uncertain
    }
    pub fn is_ready(&self) -> bool {
        let ready = *self.state.lock().ready.borrow();
        ready
    }
    /// How long a TURN holds through a closed endpoint: an engine REPLACEMENT, not one
    /// launch. Measured on BigMama 2026-09-26 14:24:55–14:27:10Z: a KV restore ran past
    /// its bound, the endpoint was quarantined, the engine was retired and relaunched
    /// (two exits, one verified generation) — 135 s of refusals, 36 of them Kimi's. One
    /// launch budget ([`crate::inference::llama_server::DEFAULT_SERVING_WAIT`], 120 s)
    /// would have missed that by 15 s and refused the turns that had waited longest. So:
    /// an exit and a launch, each within the spawner's own budget. Still bounded — past
    /// it the launch itself would have been judged failed twice over.
    pub const TURN_READINESS_PATIENCE: std::time::Duration = std::time::Duration::from_secs(
        2 * crate::inference::llama_server::DEFAULT_SERVING_WAIT.as_secs(),
    );

    /// Admit a TURN, holding through a closed endpoint for up to `patience` before refusing.
    ///
    /// Why a turn holds and why the bound is what it is (BigMama, 2026-09-26): after the
    /// 12:20Z boot Kimi's turn met [`Self::admit`] while the engine was still loading and
    /// was refused 8 times in 1.5 s, then 8 more on the cross-grid fallback, which reached
    /// a peer in the same state — 16 of her 26 requests in 90 minutes failed inside
    /// 12:28:16–12:29:41Z, and the engine was ready at 12:30Z. A loading engine is BUSY,
    /// not dead (Joel: busy is not dead). The writer that closed the endpoint reopens it;
    /// a turn should be standing there when it does. `patience` is the caller's — the
    /// persona adapter passes [`Self::TURN_READINESS_PATIENCE`]; a caller that wants the
    /// instant answer (warm-ahead, the operator probe, a test asserting the refusal)
    /// passes zero. Past it the refusal is the same as before, with the wait in it.
    pub(crate) async fn admit_when_ready(
        self: &Arc<Self>,
        patience: std::time::Duration,
    ) -> Result<EndpointAdmission, String> {
        // Zero patience is the instant answer, not a zero-length timer: a
        // `tokio::time::timeout(ZERO, …)` still goes through the time driver, and
        // under a current-thread runtime that turn is observable (the warm-ahead
        // fixture's parked handler never drained behind it) — so the instant path
        // never touches the timer at all.
        if patience.is_zero() {
            return self.admit().await;
        }
        let mut ready = self.ready_rx.clone();
        let held = std::time::Instant::now();
        let deadline = tokio::time::Instant::now() + patience;
        let mut waited = !*ready.borrow_and_update();
        let mut reopened: u64 = 0;
        // ONE deadline for the whole hold, however many times the endpoint closes and
        // reopens inside it (Cormac's review of #4411, the M5 grow chain: transitions
        // back to back, and the first cut refused a held turn with patience left the
        // moment the second one closed the endpoint behind the first one's reopen).
        loop {
            match tokio::time::timeout_at(deadline, ready.wait_for(|open| *open)).await {
                Ok(Ok(_)) => {}
                Ok(Err(_)) => {
                    return Err("serving endpoint closed while a turn waited for readiness".into())
                }
                Err(_) => {
                    continuum_base::probe!(
                        class = "inference.admission.refused_after_hold",
                        held_ms = held.elapsed().as_millis() as u64,
                        reopened,
                        "engine readiness did not arrive within the turn's patience"
                    );
                    return Err(format!(
                        "serving endpoint is suspended pending engine readiness (held {} s)",
                        held.elapsed().as_secs()
                    ));
                }
            }
            match self.admit().await {
                Ok(admission) => {
                    if waited {
                        continuum_base::probe!(
                            class = "inference.admission.held",
                            held_ms = held.elapsed().as_millis() as u64,
                            reopened,
                            "a turn waited through engine readiness instead of being refused"
                        );
                    }
                    return Ok(admission);
                }
                // Closed again between the reopen and the admission: the next writer
                // is already in. The patience is the turn's, not one transition's.
                Err(_) => {
                    waited = true;
                    reopened += 1;
                }
            }
        }
    }

    pub async fn admit(self: &Arc<Self>) -> Result<EndpointAdmission, String> {
        if !*self.state.lock().ready.borrow() {
            return Err("serving endpoint is suspended pending engine readiness".into());
        }
        let guard = self.gate.clone().read_owned().await;
        let state = self.state.lock();
        if !*state.ready.borrow() {
            return Err("serving endpoint is suspended pending engine readiness".into());
        }
        Ok(EndpointAdmission {
            endpoint: self.clone(),
            _guard: guard,
            pool: state.pool.as_ref().and_then(Clone::clone),
        })
    }

    /// Queue a writer without suspending a replacement on behalf of a stale caller.
    /// Tokio's fair writer queue bars later admissions while existing readers drain.
    pub(crate) async fn transition_for_generation(
        self: &Arc<Self>,
        expected: &EngineGeneration,
    ) -> Result<EndpointTransition, ()> {
        self.transition_for_generation_if(expected, &|| true).await
    }

    pub(crate) async fn transition_for_generation_if(
        self: &Arc<Self>,
        expected: &EngineGeneration,
        current: &(dyn Fn() -> bool + Send + Sync),
    ) -> Result<EndpointTransition, ()> {
        let guard = self.gate.clone().write_owned().await;
        let state = self.state.lock();
        if !state
            .generation
            .as_ref()
            .is_some_and(|g| g.same_engine(expected))
        {
            return Err(());
        }
        drop(state);
        if !current() {
            return Err(());
        }
        let found_ready = self.state.lock().ready.send_replace(false);
        Ok(EndpointTransition {
            endpoint: self.clone(),
            _guard: guard,
            found_ready,
        })
    }

    /// Drain first, then admit a conditional lifecycle operation. A superseded
    /// request (or cancellation while queued) must not suspend the current engine.
    pub(crate) async fn transition_if(
        self: &Arc<Self>,
        current: &(dyn Fn() -> bool + Send + Sync),
    ) -> Result<EndpointTransition, ()> {
        let guard = self.gate.clone().write_owned().await;
        if !current() {
            return Err(());
        }
        let found_ready = self.state.lock().ready.send_replace(false);
        Ok(EndpointTransition {
            endpoint: self.clone(),
            _guard: guard,
            found_ready,
        })
    }

    pub async fn transition(self: &Arc<Self>) -> EndpointTransition {
        self.state.lock().ready.send_replace(false);
        let guard = self.gate.clone().write_owned().await;
        // A preceding transition might have reopened while this writer waited.
        self.state.lock().ready.send_replace(false);
        EndpointTransition {
            endpoint: self.clone(),
            _guard: guard,
            // This writer closes readiness BEFORE it holds the guard, so what it saw is not
            // what it took over (a preceding writer may reopen or close it meanwhile; Codex on
            // #5413). It never hands back: only the conditional writers, which read under the
            // guard, do.
            found_ready: false,
        }
    }
}

impl EndpointTransition {
    /// Hand the endpoint back as this writer found it when its operation changed nothing
    /// (card 931cfd22): a replacement refused before its incumbent was signalled. "The writer
    /// that closed the endpoint reopens it", and only to what it was: open only if it was
    /// open when this writer took it, and only while nothing owned took its place (no owned
    /// generation, certain paging), which is an adopted incumbent left exactly as it was.
    pub(crate) fn hand_back_unchanged(&self) -> bool {
        let state = self.endpoint.state.lock();
        if !self.found_ready || state.generation.is_some() || state.paging_uncertain {
            return false;
        }
        state.ready.send_replace(true);
        true
    }

    /// Save an adopted engine without inventing ownership of its process. The
    /// persisted ledger and the caller's lane record must identify the same live
    /// incarnation. Retirement still requires a fresh identity check by its owner.
    pub(crate) async fn checkpoint_adopted_before_replacement(
        &mut self,
        client: &reqwest::Client,
        root: &str,
        incarnation: crate::inference::engine_residency::EngineIncarnation,
        current: &(dyn Fn() -> bool + Send + Sync),
    ) -> Result<(), String> {
        use crate::inference::engine_residency::Liveness;

        if !Arc::ptr_eq(&directory().endpoint(root), &self.endpoint) {
            return Err("adopted checkpoint URL does not identify the held endpoint".into());
        }
        let pool = {
            let state = self.endpoint.state.lock();
            if state.generation.is_some() || state.paging_uncertain {
                return Err("adopted checkpoint requires an unowned, certain pool".into());
            }
            state.pool.as_ref().and_then(Clone::clone)
                .ok_or("adopted checkpoint requires a known pool")?
        };
        let ledger = pool.ledger.get().ok_or("adopted checkpoint requires a verified ledger")?;
        if ledger.incarnation.get().copied() != Some((incarnation.pid, incarnation.started_s)) {
            return Err("adopted checkpoint ledger names a different process".into());
        }
        let verify_engine = || match incarnation.liveness() {
            Liveness::Alive => Ok(()),
            Liveness::Dead => Err("adopted checkpoint engine exited or PID was reused".into()),
            Liveness::Unknown => Err("adopted checkpoint engine identity is unverified".into()),
        };
        verify_engine()?;
        if !current() {
            return Err("adopted checkpoint intent changed".into());
        }
        let result = self.checkpoint_pool(client, root, &pool, &verify_engine).await;
        if result.is_err() && current() && verify_engine().is_ok() {
            // A terminal refusal did not mutate the engine. Only reopen this
            // exact pool; CheckpointSave quarantines cancellation/uncertainty.
            let state = self.endpoint.state.lock();
            if state.generation.is_none() && !state.paging_uncertain
                && state.pool.as_ref().and_then(Option::as_ref)
                    .is_some_and(|installed| Arc::ptr_eq(installed, &pool)) {
                state.ready.send_replace(true);
            }
        }
        result?;
        if !current() {
            return Err("adopted replacement intent changed after checkpoint".into());
        }
        Ok(())
    }

    /// Planned replacement cannot discard a healthy engine's unsaved residents.
    /// A terminal refusal resumes the exact original ledger; an uncertain write
    /// stays quarantined. Dead engines and quarantined paging use recovery instead:
    /// nothing can be saved, so their unsaved residents are recorded as lost when this
    /// generation's exit is observed, at once if it already was (card 6b28a445).
    pub(crate) async fn checkpoint_before_replacement(
        &mut self,
        client: &reqwest::Client,
        root: &str,
        current: &(dyn Fn() -> bool + Send + Sync),
    ) -> Result<(), String> {
        let generation = self.previous_generation().ok_or("replacement requires a known generation")?;
        if generation.has_exited() || self.endpoint.paging_recovery_required() {
            let reason = if generation.has_exited() {
                DiscardReason::EngineExited
            } else {
                DiscardReason::PagingUncertain
            };
            self.endpoint.pending_discard(reason).on_exit_of(&generation);
            return Ok(());
        }
        let original = self.capture_suspension(&generation)?;
        let result = self.checkpoint_residents(client, root).await.map(|_| ());
        if let Err(error) = result {
            return match self.resume_suspension(&original, current) {
                Ok(()) => Err(format!("replacement deferred; original engine resumed: {error}")),
                Err(resume) => Err(format!("replacement deferred; endpoint remains suspended: {error}; {resume}")),
            };
        }
        if !current() {
            return Err("replacement intent changed after checkpoint".into());
        }
        Ok(())
    }

    pub(crate) fn capture_suspension(
        &self,
        expected: &EngineGeneration,
    ) -> Result<EndpointSuspension, String> {
        let state = self.endpoint.state.lock();
        if expected.has_exited()
            || state.paging_uncertain
            || state.pool_generation != Some(expected.id)
            || !state
                .generation
                .as_ref()
                .is_some_and(|g| g.same_engine(expected))
        {
            return Err("cannot suspend an unknown or uncertain live ledger".into());
        }
        Ok(EndpointSuspension {
            endpoint: self.endpoint.clone(),
            generation: expected.clone(),
            pool: state
                .pool
                .as_ref()
                .and_then(Clone::clone)
                .ok_or("missing live ledger")?,
            contract: state.contract.clone().ok_or("missing live page contract")?,
        })
    }

    pub(crate) fn resume_suspension(
        &self,
        original: &EndpointSuspension,
        current: &(dyn Fn() -> bool + Send + Sync),
    ) -> Result<(), String> {
        if !Arc::ptr_eq(&self.endpoint, &original.endpoint) || !current() {
            return Err("suspended endpoint or intent changed".into());
        }
        let state = self.endpoint.state.lock();
        if original.generation.has_exited()
            || state.paging_uncertain
            || state.pool_generation != Some(original.generation.id)
            || state.contract.as_ref() != Some(&original.contract)
            || !state
                .generation
                .as_ref()
                .is_some_and(|g| g.same_engine(&original.generation))
            || !state
                .pool
                .as_ref()
                .and_then(Option::as_ref)
                .is_some_and(|p| Arc::ptr_eq(p, &original.pool))
        {
            return Err("suspended live ledger changed or paging is uncertain".into());
        }
        state.ready.send_replace(true);
        Ok(())
    }

    /// Save only known physical residents after all endpoint readers have drained.
    /// Refusal never reopens the endpoint or retires its owned child.
    pub(crate) async fn checkpoint_residents(
        &mut self,
        client: &reqwest::Client,
        root: &str,
    ) -> Result<ResidentCheckpoint<'_>, String> {
        if !Arc::ptr_eq(&directory().endpoint(root), &self.endpoint) {
            return Err("checkpoint URL does not identify the held endpoint".into());
        }
        let (generation, pool) = {
            let state = self.endpoint.state.lock();
            if state.paging_uncertain {
                return Err("checkpoint refused: paging completion remains uncertain".into());
            }
            let generation = state
                .generation
                .clone()
                .ok_or("checkpoint requires an owned generation")?;
            if generation.has_exited() || state.pool_generation != Some(generation.id) {
                return Err("checkpoint requires this live generation's verified pool".into());
            }
            if state
                .contract
                .as_ref()
                .and_then(|c| c.page_dir.as_ref())
                .is_none()
            {
                return Err("checkpoint requires a verified page directory".into());
            }
            let pool = state
                .pool
                .as_ref()
                .and_then(Clone::clone)
                .ok_or("checkpoint requires a known slot pool")?;
            state.ready.send_replace(false);
            (generation, pool)
        };
        self.checkpoint_pool(client, root, &pool, &|| {
            if generation.has_exited() {
                Err("checkpoint engine exited".into())
            } else {
                Ok(())
            }
        }).await?;
        Ok(ResidentCheckpoint {
            transition: self,
            generation,
            pool,
        })
    }

    /// One save path for a drained pool. The caller supplies its existing engine
    /// authority: an owned generation or a verified adopted process incarnation.
    /// Losing that authority cannot publish a successful save for its successor.
    async fn checkpoint_pool(
        &mut self,
        client: &reqwest::Client,
        root: &str,
        pool: &Arc<KvSlotPool>,
        verify_engine: &(dyn Fn() -> Result<(), String> + Send + Sync),
    ) -> Result<(), String> {
        use super::super::turn_admission::{kv_page_action, PageOutcome};

        verify_engine()?;
        directory().discover_page_admission(client, root).await?;
        let mut residents: Vec<_> = pool
            .holders
            .lock()
            .iter()
            .map(|(&slot, &key)| (slot, key))
            .collect();
        residents.sort_unstable_by_key(|(slot, _)| *slot);
        for (slot, key) in residents {
            verify_engine()?;
            pool.note_page_lost(&key);
            let mut save = CheckpointSave {
                endpoint: self.endpoint.clone(),
                pending: true,
            };
            match kv_page_action(client, root, slot, &key, "save").await {
                PageOutcome::Completed => {
                    verify_engine()?;
                    save.pending = false;
                    pool.note_saved(key);
                }
                PageOutcome::Rejected => {
                    save.pending = false;
                    return Err(format!("checkpoint save rejected for slot {slot}"));
                }
                PageOutcome::Uncertain => {
                    return Err(format!("checkpoint save completion uncertain for slot {slot}; endpoint quarantined"));
                }
            }
        }
        verify_engine()
    }

    pub(crate) fn previous_generation(&self) -> Option<EngineGeneration> {
        self.endpoint.state.lock().generation.clone()
    }

    pub fn start_generation(&self) -> Result<EngineGeneration, String> {
        let mut state = self.endpoint.state.lock();
        if state.generation.as_ref().is_some_and(|g| !g.has_exited()) {
            return Err("predecessor engine exit remains unverified".into());
        }
        let generation = EngineGeneration {
            id: Uuid::new_v4(),
            exited: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            endpoint: Arc::downgrade(&self.endpoint),
        };
        state.generation = Some(generation.clone());
        state.paging_uncertain = false;
        Ok(generation)
    }

    pub fn ready(
        &self,
        root: &str,
        generation: &EngineGeneration,
        contract: KvPageContract,
    ) -> Result<(), String> {
        let mut state = self.endpoint.state.lock();
        if generation.has_exited()
            || !state
                .generation
                .as_ref()
                .is_some_and(|g| g.id == generation.id)
        {
            return Err("engine generation exited or was superseded before readiness".into());
        }
        if state.paging_uncertain {
            return Err(
                "uncertain paging requires verified predecessor exit and a new engine generation"
                    .into(),
            );
        }
        if *state.ready.borrow() {
            return if state.contract.as_ref() == Some(&contract) {
                Ok(())
            } else {
                Err("ready engine cannot change its page contract without a transition".into())
            };
        }
        // A fresh assignment ledger prevents old Arc holders/pins from affecting
        // the new engine. Copy ONLY saved-page eligibility under an exact contract.
        let pool = Arc::new(KvSlotPool::new(root, contract.slots));
        let mut carried = false;
        // Same page format, not the same launch: a relaunch at another window or slot count
        // keeps every page it can restore (the engine refuses, loudly, one that no longer fits).
        let fingerprint = contract.fingerprint();
        let previous = state.contract.as_ref();
        if fingerprint.is_some()
            && previous.and_then(KvPageContract::fingerprint) == fingerprint
            && previous.and_then(|c| c.page_dir.as_ref()) == contract.page_dir.as_ref()
        {
            if let Some(Some(old)) = state.pool.as_ref() {
                *pool.saved.lock() = old.saved.lock().clone();
                carried = true;
            }
        }
        // The same eligibility across a PROCESS restart: the ledger beside the pages,
        // read only under the same fingerprint (the same exact contract). In-process
        // carry-over is the fresher record when there is one, so the disk is read only
        // when nothing was carried.
        if let (Some(dir), Some(fingerprint)) = (contract.page_dir.as_ref(), fingerprint) {
            let ledger = SavedLedger::in_dir(dir, fingerprint).with_identity_floor(contract.identity_floor());
            let loaded = pool.attach_ledger(ledger, !carried);
            continuum_base::probe!(
                class = "inference.kv_page.ledger_attached",
                endpoint = root,
                carried = carried,
                loaded = loaded as u64,
                "confirmed-save ledger attached: pages saved under this exact contract are restorable, across a core restart too"
            );
        }
        state.pool = Some(Some(pool));
        state.page_admission = None;
        state.pool_generation = Some(generation.id);
        state.contract = Some(contract);
        state.ready.send_replace(true);
        continuum_base::probe!(
            class = "inference.kv_engine.ready",
            endpoint = root,
            generation = %generation.id,
            "verified engine generation installed with fresh physical KV attribution"
        );
        Ok(())
    }
}

impl SlotDirectory {
    pub fn endpoint(&self, root: &str) -> Arc<EndpointSlots> {
        self.endpoints
            .entry(root.trim_end_matches('/').to_string())
            .or_insert_with(|| {
                let (ready, ready_rx) = tokio::sync::watch::channel(true);
                Arc::new(EndpointSlots {
                    gate: Arc::new(tokio::sync::RwLock::new(())),
                    paging_tasks: Mutex::new(tokio::task::JoinSet::new()),
                    ready_rx,
                    discarded: Mutex::new(DiscardLog::default()),
                    state: Mutex::new(EndpointState {
                        ready,
                        paging_uncertain: false,
                        generation: None,
                        contract: None,
                        pool: None,
                        pool_generation: None,
                        page_admission: None,
                    }),
                })
            })
            .clone()
    }
    /// `None` entry == latched Unsupported for this server root.
    pub fn latch_unsupported(&self, root: &str) {
        let endpoint = self.endpoint(root);
        let mut state = endpoint.state.lock();
        if state.generation.is_none() {
            state.pool = Some(None);
        }
    }

    /// `Some(Some(pool))` = pool ready; `Some(None)` = latched unsupported;
    /// `None` = never probed (caller should probe /props).
    pub fn get(&self, root: &str) -> Option<Option<Arc<KvSlotPool>>> {
        self.endpoint(root).state.lock().pool.clone()
    }

    pub(crate) fn page_admission_known(&self, root: &str) -> bool {
        // A refused discovery never authorizes a write. It remains retryable:
        // the existing retained path worker may finish after a bounded wait.
        matches!(self.endpoint(root).state.lock().page_admission, Some(Ok(_)))
    }

    pub fn page_discovery_generation(&self, root: &str) -> Option<Uuid> {
        self.endpoint(root).state.lock().generation.as_ref().map(|generation| generation.id)
    }

    pub(crate) fn page_admission_path(&self, root: &str) -> Result<Option<std::path::PathBuf>, String> {
        self.endpoint(root).state.lock().page_admission.clone()
            .unwrap_or_else(|| Err("page-write capability has not been negotiated".into()))
    }

    /// Cold discovery shared by spawned and adopted pools. Only the admission
    /// path is physical: logical checkpoint filenames and ledger handles do not move.
    pub(crate) async fn negotiate_page_admission(
        &self, root: &str, pool: &Arc<KvSlotPool>, generation: Option<Uuid>,
        capability: Option<&serde_json::Value>,
    ) -> bool {
        let admission = match capability {
            None => Ok(None), // older engines retain their existing paging protocol
            Some(value) if value.as_u64() == Some(1) => match self.page_dir(root) {
                None => Ok(None), // remote owner must perform its own volume admission
                Some(path) => crate::system_resources::disk_reporters::resolve_existing_storage_path(&path).await.map(Some)
                    .map_err(|error| format!("cannot resolve owned page volume: {error}")),
            },
            Some(_) => Err("unrecognized exact page-write capability".into()),
        };
        let endpoint = self.endpoint(root);
        let mut state = endpoint.state.lock();
        if state.generation.as_ref().map(|current| current.id) != generation
            || !state.pool.as_ref().and_then(Option::as_ref).is_some_and(|current| Arc::ptr_eq(current, pool)) {
            return false; // a response from an old incarnation cannot authorize its successor
        }
        if !matches!(state.page_admission, Some(Ok(_))) {
            continuum_base::probe!(class = "inference.kv_page.capability",
                governed = matches!(&admission, Ok(Some(_))),
                refused = admission.is_err(),
                "page-write negotiation; legacy and remote ownership remain unactivated");
            state.page_admission = Some(admission);
        }
        true
    }

    /// Lifecycle checkpoints can precede the first adapter request. They use
    /// the same endpoint capability publication, never an unnegotiated fallback.
    pub(crate) async fn discover_page_admission(&self, client: &reqwest::Client, root: &str) -> Result<(), String> {
        if self.page_admission_known(root) { return self.page_admission_path(root).map(|_| ()); }
        let generation = self.page_discovery_generation(root);
        let pool = self.get(root).flatten().ok_or("page-write discovery requires a pool")?;
        let body = super::super::llama_server::engine_props(root, client).await
            .map_err(|error| format!("page-write capability request failed: {error}"))?;
        if body.get("total_slots").and_then(serde_json::Value::as_u64) != Some(pool.n_slots() as u64) {
            return Err("page-write capabilities do not identify this pool geometry".into());
        }
        if !self.negotiate_page_admission(root, &pool, generation, body.get("slot_save_size_version")).await {
            return Err("engine changed during page-write discovery".into());
        }
        self.page_admission_path(root).map(|_| ())
    }

    /// Actual page directory of the admitted engine, never a reconstructed default.
    pub(crate) fn page_dir(&self, root: &str) -> Option<std::path::PathBuf> {
        let endpoint = self.endpoint(root);
        let state = endpoint.state.lock();
        if let Some(path) = state.contract.as_ref().and_then(|contract| contract.page_dir.clone()) {
            return Some(path);
        }
        // Adoption installs a ledger only after checking the live process incarnation.
        state.pool.as_ref()?.as_ref()?.ledger.get()?.path.parent().map(std::path::Path::to_path_buf)
    }

    /// Install (or return the already-installed) pool for a probed server.
    /// First writer wins — the probe race resolves to ONE pool per root.
    pub fn ensure_pool(&self, root: &str, n_slots: u32) -> Arc<KvSlotPool> {
        let endpoint = self.endpoint(root);
        let mut state = endpoint.state.lock();
        let entry = state
            .pool
            .get_or_insert_with(|| Some(Arc::new(KvSlotPool::new(root, n_slots))));
        match entry {
            Some(pool) => Arc::clone(pool),
            None => {
                // A racing latch_unsupported won — honor it by returning a
                // zero-slot pool that never leases (callers treat None lease
                // as unpinned). Practically unreachable: latch and ensure are
                // driven by the same probe outcome.
                Arc::new(KvSlotPool::new(root, 0))
            }
        }
    }
}

/// The spawn's half of adoption: once the engine at `root` is ready, name the process
/// (pid + OS start time) in its confirmed-save ledger.
pub(crate) fn stamp_engine_incarnation(root: &str, pid: u32, started_s: u64) {
    if started_s == 0 {
        return; // an unreadable start time names no process, so it can prove nothing
    }
    if let Some(Some(pool)) = directory().get(root) {
        pool.stamp_incarnation((pid, started_s));
    }
}

/// The adopting core's half: `pool` was installed by props discovery for a running engine
/// this core did not spawn. If that engine is the live lane `(pid, started_s)` and left a
/// ledger in `page_dir` stamped with that same process, its confirmed saves become
/// restorable here (same process, same contract). Returns the keys loaded; `None` when
/// the ledger is not this engine's.
pub(crate) fn adopt_engine_ledger(pool: &KvSlotPool, page_dir: &std::path::Path, pid: u32, started_s: u64) -> Result<usize, AdoptRefusal> {
    if started_s == 0 {
        return Err(AdoptRefusal::NoStartTime);
    }
    match SavedLedger::of_running_engine(page_dir, (pid, started_s)) {
        Ok(ledger) => Ok(pool.attach_ledger(ledger, true)),
        Err(refusal) => {
            // Nothing of this process's to restore, but it IS this process (the caller verified
            // the live lane): start its ledger now, empty, so what it saves from here on is
            // restorable by the next core that adopts it. Without this, a refused adoption left
            // the pool with no ledger and the chain never began (IntelMac 2026-10-10 22:35Z:
            // why=no_ledger on an engine older than the ledger, kept by every deploy since).
            // Its fingerprint is the incarnation: only an adopter of this same process ever
            // loads it, and a spawned engine's contract fingerprint never equals it.
            let ledger = SavedLedger::in_dir(page_dir, format!("incarnation:{pid}:{started_s}"));
            let _ = ledger.incarnation.set((pid, started_s));
            pool.attach_ledger(ledger, false);
            Err(refusal)
        }
    }
}

/// Why an adopted engine's saved pages stay unrestorable: each is a different thing to fix
/// (Fable on #4923), so the probe names it instead of a bare `adopted=false`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdoptRefusal {
    /// The engine never saved a page under a ledger (nothing to restore, not a fault).
    NoLedger,
    /// A ledger is there but could not be read or parsed.
    Unreadable,
    /// Written before the spawn stamped its engine: it cannot prove which process it is.
    Unstamped,
    /// Stamped by another process (another pid, or the same pid started at another time).
    AnotherProcess,
    /// The live engine's start time could not be read, so nothing can be proven about it.
    NoStartTime,
}

impl AdoptRefusal {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::NoLedger => "no_ledger",
            Self::Unreadable => "unreadable",
            Self::Unstamped => "unstamped",
            Self::AnotherProcess => "another_process",
            Self::NoStartTime => "no_start_time",
        }
    }
}

/// The one process-wide directory (same scope as the serving resource itself:
/// every adapter instance talking to one server shares one assignment).
pub fn directory() -> &'static SlotDirectory {
    static DIR: std::sync::OnceLock<SlotDirectory> = std::sync::OnceLock::new();
    DIR.get_or_init(|| SlotDirectory {
        endpoints: dashmap::DashMap::new(),
    })
}
