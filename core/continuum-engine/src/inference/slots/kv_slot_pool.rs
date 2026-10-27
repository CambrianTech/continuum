use super::*;

impl KvSlotPool {
    pub fn new(server_root: &str, n_slots: u32) -> Self {
        // ≥3 slots: the highest index is RESERVED as scratch and never enters
        // the citizen free list. Costs one window; buys structural immunity
        // from every non-Turn clobber class at once.
        let citizen_slots = citizen_slots(n_slots);
        let scratch = (citizen_slots < n_slots).then_some(citizen_slots);
        // Low indices lease first (pop from the back), deterministic occupancy.
        let free: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new((0..citizen_slots).rev().collect()));
        let pool = PagedResourcePool::new(PoolConfig {
            name: format!("kv-slots {server_root}"),
            // Count-based pool (sizer 1). Capacity is set ABOVE the real slot
            // count on purpose: the free list is the true allocator, so the
            // engine's own over-capacity auto-evict (which drains to 75% —
            // right for byte tiers, thrash for a 4-count pool) can never fire;
            // eviction happens only through the explicit make-room path below.
            max_bytes: (citizen_slots as u64) + 1,
            sizer: Arc::new(|_| 1),
            // PRICED eviction (plan B5, KV-CACHE-ECONOMY §3.2/§6 — the classic
            // heuristic floor, dropped into the engine's own priority seam):
            // cost_of_loss ∝ the page's tail tokens (what a cold re-entry
            // re-prefills), discounted by staleness — halved per hour idle — so
            // an abandoned giant eventually yields to live small pages.
            // Ascending sort evicts lowest first, so the CHEAPEST tail goes
            // first and a 7.6k head can never displace a warm 36k tail (§2's
            // arithmetic, pinned by test below). The learned/bandit policy is a
            // later drop-in behind this same closure seam.
            eviction_priority: Arc::new(|view, lease: &Arc<KvSlotLease>| {
                let tokens = lease.tail_tokens.load(Ordering::Relaxed).max(1);
                let now_ms = continuum_base::time::EpochMs::read().map_or(view.last_access_at, continuum_base::time::EpochMs::millis); // engine has no view of a just-inserted key: fall back to its own stamp
                let stale_hours =
                    now_ms.saturating_sub(view.last_access_at) / (3600 * 1000);
                let cost = (tokens >> stale_hours.min(20)) as i64;
                // EQUAL costs tie-break by RECENCY, never by map-iteration order —
                // without this the tie fell to hash ordering, a platform coin flip
                // (Linux CI evicted a just-refreshed slot; macOS happened not to).
                // Priced first, LRU among equals: a fresher tail scores a bonus
                // strictly smaller than one token-cost step, so pricing always
                // dominates and recency only ever splits exact ties.
                let age_ms = now_ms.saturating_sub(view.last_access_at);
                let recency = RECENCY_TIEBREAK_MS.saturating_sub(age_ms) as i64;
                cost.saturating_mul(COST_STEP) + recency
            }),
        });
        Self {
            pool,
            free,
            n_slots,
            scratch,
            holders: Mutex::new(std::collections::HashMap::new()),
            saved: Mutex::new(std::collections::HashSet::new()),
            operations: (0..n_slots)
                .map(|_| Arc::new(tokio::sync::Semaphore::new(1)))
                .collect(),
            released: Arc::new(tokio::sync::Notify::new()),
            ledger: std::sync::OnceLock::new(),
            ledger_generation: AtomicU64::new(0),
        }
    }

    /// Persist this pool's confirmed-save set to `ledger` from now on. With `load`, the
    /// ledger's own keys (same fingerprint, page file present) become restorable first:
    /// a new core restoring the pages the last one saved. Returns the keys loaded.
    pub(crate) fn attach_ledger(&self, ledger: SavedLedger, load: bool) -> usize {
        let loaded = if load { ledger.load() } else { LoadedLedger::default() };
        let count = loaded.keys.len();
        if self.ledger.set(ledger).is_err() {
            return 0;
        }
        let snapshot = {
            let mut saved = self.saved.lock();
            saved.extend(loaded.keys);
            let mut holders = self.holders.lock();
            // Residents come back only from the same engine process (`SavedLedger::load`):
            // its slots still hold their KV, so their next turn reuses it in place.
            for (slot, key) in loaded.residents {
                if slot < citizen_slots(self.n_slots) {
                    holders.insert(slot, key);
                }
            }
            self.ledger_snapshot(&saved, &holders)
        };
        self.persist_ledger(snapshot);
        count
    }

    /// Stamp the engine process into this pool's ledger and rewrite it, so a core that
    /// later adopts this engine can prove the ledger is its own. Once per ledger.
    pub(super) fn stamp_incarnation(&self, incarnation: (u32, u64)) {
        let Some(ledger) = self.ledger.get() else {
            return;
        };
        if ledger.incarnation.set(incarnation).is_err() {
            return;
        }
        self.persist_ledger(self.current_snapshot());
    }

    pub fn n_slots(&self) -> u32 {
        self.n_slots
    }

    /// The reserved scratch slot for non-Turn traffic, when this server can
    /// spare one.
    pub fn scratch_slot(&self) -> Option<u32> {
        self.scratch
    }

    /// Whether a synthetic decode (the serving heartbeat's smoke probe) could only run on a slot
    /// holding a resident's live KV: no scratch slot, and some slot attributed to an page.
    /// The probe must not run then (card 9e3350bd, the 5090 2026-10-11: on a one-slot lane the
    /// 72-token smoke prompt replaced a 53,980-token resident and the next save wrote that as
    /// her page, 0 checkpoints; every restore after it re-prefilled from ~0).
    pub fn smoke_would_overwrite_a_resident(&self) -> bool {
        self.scratch.is_none() && !self.holders.lock().is_empty()
    }

    /// The citizen slot transient traffic borrows on a server with NO scratch slot:
    /// a slot nobody's KV warms if there is one, else the resident whose page is the
    /// cheapest to lose — the smallest `tail_tokens`, the priced eviction's own cost
    /// basis. The single-slot rule ("save the resident, then borrow") generalised to
    /// every scratch-less pool; the caller saves that resident before the borrow.
    pub(crate) fn transient_slot(&self) -> u32 {
        let holders = self.holders.lock();
        let citizens = citizen_slots(self.n_slots);
        if let Some(free) = (0..citizens).find(|slot| !holders.contains_key(slot)) {
            return free;
        }
        (0..citizens)
            .min_by_key(|slot| {
                holders
                    .get(slot)
                    .and_then(|key| self.pool.get(key))
                    .map_or(0, |lease| lease.tail_tokens.load(Ordering::Relaxed))
            })
            .unwrap_or(0) // unwrap_or: a pool has at least one slot; an empty range is impossible
    }

    /// Record the page's current prompt size — the cost basis the priced
    /// eviction reads. Called at pin time with the request's token estimate.
    pub fn note_tail(&self, key: &PageKey, tokens: u64) {
        if let Some(lease) = self.pool.get(key) {
            lease.tail_tokens.store(tokens.max(1), Ordering::Relaxed);
        }
    }

    /// Lease the slot for `key`: warm reuse if the page is resident, a free
    /// index otherwise, evicting the engine-chosen (LRU, later priced) resident
    /// when full. `None` only if the pool cannot free an index (all pinned).
    pub async fn lease(&self, key: PageKey) -> Option<u32> {
        // Warm path — also refreshes recency in the engine.
        if let Some(lease) = self.pool.get(&key) {
            return Some(lease.slot);
        }
        // Allocation races are retryable, not evidence that every lease is pinned.
        // Only a failed eviction below may return None: admission parks on that
        // result and needs a real holder to eventually signal its release (#4515).
        //
        // INVARIANT this unbounded loop rests on (Fable on #4516): an evicted lease hands its
        // index back SYNCHRONOUSLY (KvSlotLease's Drop pushes to `free`), so each extra pass costs
        // exactly one eviction per race actually lost. Never hold a clone of a NON-pinned
        // `Arc<KvSlotLease>` across an await: if one outlived its eviction, `free` would stay
        // empty and every pass would evict ANOTHER resident, a cascade the old two-round bound
        // capped. Pins are never evicted, so a pinned clone is safe.
        loop {
            if self.free.lock().is_empty() {
                let evicted = self.pool.evict_at_least(1);
                if evicted == 0 {
                    // Nothing to evict: every lease is pinned by an in-flight turn. Said as
                    // what it is (it read "engine evicted" with evicted_count 0, Fable on the
                    // M5); the caller waits for a release rather than running unpinned.
                    continuum_base::probe!(
                        class = "inference.slot_affinity.all_pinned",
                        page = %key,
                        "all slots are pinned by in-flight turns: nothing to evict"
                    );
                    return None;
                }
                continuum_base::probe!(
                    class = "inference.slot_affinity.evicted",
                    evicted_count = evicted,
                    "all slots held — engine evicted the least-valuable page; its \
                     warm prefix is forfeit and its next turn re-prefills (or restores \
                     its page at this node's measured switch cost — inference.kv_page.action ms)",
                );
            }
            let free = Arc::clone(&self.free);
            // The slot that physically still holds this page's KV (evicted from the pool
            // but not yet overwritten, or resumed from the same engine's ledger) is taken
            // back when free: its next turn reuses that KV in place instead of restoring.
            let warm = self.holders.lock().iter().find(|(_, k)| **k == key).map(|(slot, _)| *slot);
            let res = self
                .pool
                .load_or_share(key, move |_| async move {
                    let slot = {
                        let mut free = free.lock();
                        match warm.and_then(|w| free.iter().position(|s| *s == w)) {
                            Some(at) => Some(free.remove(at)),
                            None => free.pop(),
                        }
                    }
                    .ok_or_else(|| "no free slot index".to_string())?;
                    Ok(Arc::new(KvSlotLease {
                        slot,
                        free,
                        tail_tokens: AtomicU64::new(0),
                    }))
                })
                .await;
            match res {
                Ok(lease) => {
                    continuum_base::probe!(
                        class = "inference.slot_affinity.pinned",
                        page = %key,
                        slot = lease.slot as u64,
                        "page pinned to a llama-server slot — its prefix warms HERE",
                    );
                    return Some(lease.slot);
                }
                Err(_) => {
                    // Another allocator took the last index. Recheck eviction after
                    // yielding; exhausting an arbitrary retry count could park a
                    // caller even though the competing lease is already evictable.
                    tokio::task::yield_now().await;
                }
            }
        }
    }

    /// [`Self::lease`] plus the paging ledger: who to SAVE before this turn
    /// overwrites the slot, and whether to RESTORE this page's own page.
    ///
    /// The rules, in order:
    /// - slot still holds THIS page (warm re-lease) → no save, no restore:
    ///   the in-slot KV is fresher than any page on disk.
    /// - slot last warmed a DIFFERENT page → save THEIRS first (their KV is
    ///   physically still in the server slot), then restore OURS if a page
    ///   exists.
    /// - slot holder unknown (fresh pool over a pre-existing server) → restore
    ///   ours if a page exists; nothing known to save.
    pub async fn lease_paged(&self, key: PageKey) -> Option<SlotPaging> {
        let slot = self.lease(key).await?;
        Some(self.plan_paging(slot, key))
    }

    /// Resolve the physical index from the pinned assignment, not a prior lease
    /// that another executor thread could have evicted before the pin.
    pub(crate) fn pin_slot(&self, key: &PageKey) -> Option<(u32, SlotPin)> {
        let pin = self.pin(key)?;
        let slot = pin.value()?.slot;
        Some((slot, pin))
    }

    /// Hold through paging and generation; pins alone only exclude eviction.
    pub async fn acquire_slot(&self, slot: u32) -> Result<SlotPermit, String> {
        let operation = self
            .operations
            .get(slot as usize)
            .ok_or("unknown physical KV slot")?;
        let permit = operation
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| "physical KV slot admission closed".to_string())?;
        Ok(SlotPermit { _permit: permit, released: Arc::clone(&self.released) })
    }

    /// The signal a slot release raises; see [`Self::released`] on the struct.
    pub(crate) fn released(&self) -> Arc<tokio::sync::Notify> {
        Arc::clone(&self.released)
    }

    /// Caller holds the physical-slot permit and page pin before changing attribution.
    pub(crate) fn plan_paging(&self, slot: u32, key: PageKey) -> SlotPaging {
        let prev = self.holders.lock().insert(slot, key);
        if prev != Some(key) {
            self.persist_ledger(self.current_snapshot());
        }
        let save_first = prev.filter(|p| *p != key);
        let slot_holds_ours = prev == Some(key);
        let restore = !slot_holds_ours && self.saved.lock().contains(&key);
        SlotPaging {
            slot,
            save_first,
            restore,
            already_resident: slot_holds_ours,
        }
    }

    /// Pin this page's slot for the duration of a turn. While the returned
    /// [`SlotPin`] is held, priced eviction skips this slot, so a concurrent
    /// returner can never lease (and restore into) a slot that is still decoding
    /// this turn's KV — the decoupling that forced the old restore timeout. RAII:
    /// drop the handle to release. `None` if the page is not resident (nothing
    /// to protect — a caller that just leased it will always get `Some`).
    pub fn pin(&self, key: &PageKey) -> Option<SlotPin> {
        self.pool.pin(key)
    }

    /// Detach the resident identity before transient traffic overwrites a slot.
    /// The caller holds the decode permit and keeps the returned pin through the
    /// request. A later page must restore its saved page, never mistake the
    /// transient KV for its own warm state or save it under its own filename.
    pub(crate) fn take_resident(&self, slot: u32) -> (Option<PageKey>, Option<SlotPin>) {
        let previous = self.holders.lock().remove(&slot);
        if previous.is_some() {
            self.persist_ledger(self.current_snapshot());
        }
        let pin = previous.and_then(|key| self.pin(&key));
        (previous, pin)
    }

    /// Failed/cancelled generation never proves whose KV now occupies the slot.
    /// Saved pages remain valid; only the provisional physical attribution clears.
    pub(crate) fn forget_resident(&self, slot: u32, key: PageKey) {
        let removed = {
            let mut holders = self.holders.lock();
            holders.get(&slot) == Some(&key) && holders.remove(&slot).is_some()
        };
        if removed {
            self.persist_ledger(self.current_snapshot());
        }
    }

    /// Whether `key` has a page file beside this pool's ledger; `None` without a ledger (no
    /// page dir known). One stat, on the cold path only.
    pub(crate) fn page_on_disk(&self, key: &PageKey) -> Option<bool> {
        let dir = self.ledger.get()?.path.parent()?.to_path_buf();
        Some(dir.join(page_filename(key)).is_file())
    }

    /// Record that `key`'s page was successfully written to disk — it becomes
    /// restorable. Called by the adapter AFTER the save HTTP call succeeds.
    pub fn note_saved(&self, key: PageKey) {
        let snapshot = {
            let mut saved = self.saved.lock();
            saved.insert(key);
            self.ledger_snapshot(&saved, &self.holders.lock())
        };
        self.persist_ledger(snapshot);
    }

    /// The page turned out unusable (restore failed: file gone, geometry
    /// mismatch) — stop offering it so the turn falls back to a plain prefill
    /// instead of retrying a dead restore every pin. Also called BEFORE a save
    /// overwrites the page, so an interrupted save is never restorable.
    pub fn note_page_lost(&self, key: &PageKey) {
        let snapshot = {
            let mut saved = self.saved.lock();
            if !saved.remove(key) {
                return;
            }
            self.ledger_snapshot(&saved, &self.holders.lock())
        };
        self.persist_ledger(snapshot);
    }

    /// The set as it stands, numbered under the lock that changed it, so writes taken
    /// outside that lock can still be applied in the set's order ([`SavedLedger::write`]
    /// drops a snapshot older than the one already on disk). `None` without a ledger.
    /// Lock order: `saved`, then `holders`.
    pub(super) fn ledger_snapshot(
        &self,
        saved: &std::collections::HashSet<PageKey>,
        holders: &std::collections::HashMap<u32, PageKey>,
    ) -> Option<LedgerSnapshot> {
        self.ledger.get()?;
        let generation = self.ledger_generation.fetch_add(1, Ordering::Relaxed) + 1;
        // Minds only: a fork dies with the core that ran it, so a restart could only pin its
        // pages on disk. In memory a fork's page is restorable like any other.
        let mut minds: Vec<Uuid> = saved.iter().filter_map(|k| k.as_mind()).map(|m| m.persona()).collect();
        minds.sort_unstable();
        let mut residents: Vec<(u32, Uuid)> =
            holders.iter().filter_map(|(slot, k)| k.as_mind().map(|m| (*slot, m.persona()))).collect();
        residents.sort_unstable();
        Some(LedgerSnapshot { generation, minds, residents })
    }

    /// [`Self::ledger_snapshot`] of the state right now, for a change that held neither lock.
    pub(super) fn current_snapshot(&self) -> Option<LedgerSnapshot> {
        let saved = self.saved.lock();
        let holders = self.holders.lock();
        self.ledger_snapshot(&saved, &holders)
    }

    /// Write a snapshot beside the pages, OUTSIDE the set's lock: the write may retry a
    /// Windows sharing violation for ~1.6 s (`replace_file`), and a turn's paging must
    /// never wait on that. Nothing to write without a ledger (no page dir, or a contract
    /// that cannot be fingerprinted).
    pub(super) fn persist_ledger(&self, snapshot: Option<LedgerSnapshot>) {
        let (Some(ledger), Some(snapshot)) = (self.ledger.get(), snapshot) else {
            return;
        };
        if let Err(error) = ledger.write(snapshot) {
            // The in-memory set stays authoritative for this process; only a restart
            // loses the eligibility the file would have carried.
            continuum_base::probe!(
                class = "inference.kv_page.ledger_unwritten",
                path = %ledger.path.display(),
                error = %error,
                "the confirmed-save ledger could not be written: saved pages stay restorable in this process, a restart will re-prefill them"
            );
        }
    }
}

/// Per-server directory: which roots have a pool, and which are latched
/// unsupported (no /props surface). The transport-side probe
/// (the adapter owns the HTTP client) discovers; this directory owns state.
pub struct SlotDirectory {
    pub(super) endpoints: dashmap::DashMap<String, Arc<EndpointSlots>>,
}

/// Exact launch/page compatibility, supplied by the process owner, never inferred
/// from the URL. Unknown or changed contracts cannot inherit saved-page eligibility.
#[derive(Clone, PartialEq, Eq)]
pub struct KvPageContract {
    pub model_id: String,
    pub model: std::path::PathBuf,
    pub adapters: Vec<String>,
    pub page_dir: Option<std::path::PathBuf>,
    pub context: u32,
    pub slots: u32,
    pub cache_type: Option<String>,
    /// Whether the engine runs flash attention: it decides the V layout a page stores.
    pub flash_attn: Option<bool>,
    /// Cold-path revision evidence for the resolved model, adapters and MTP draft.
    /// Unknown revisions refuse saved-page carry-over, never guess compatibility.
    pub revisions: Option<Vec<(std::path::PathBuf, u64, std::time::SystemTime)>>,
}

pub(super) struct EndpointState {
    /// Whether admissions are open. A `watch` rather than a bool so a TURN can wait
    /// for the writer that closed the endpoint to reopen it ([`EndpointSlots::admit_when_ready`])
    /// instead of being refused on the spot; every write goes through `send_replace`,
    /// so the field and the signal cannot disagree.
    pub(super) ready: tokio::sync::watch::Sender<bool>,
    pub(super) paging_uncertain: bool,
    pub(super) generation: Option<EngineGeneration>,
    pub(super) contract: Option<KvPageContract>,
    pub(super) pool: Option<Option<Arc<KvSlotPool>>>,
    // An installed ledger can outlive its child until replacement readiness.
    pub(super) pool_generation: Option<Uuid>,
    /// Negotiated for this pool incarnation. None is not yet negotiated;
    /// Ok(None) is explicitly legacy or remotely owned, not governed coverage.
    pub(super) page_admission: Option<Result<Option<std::path::PathBuf>, String>>,
}

/// The directory's existing endpoint owner also arbitrates engine transitions.
/// Admission holds a read lease through HTTP completion; transitions drain those
/// leases before retiring the engine. Closing survives cancellation of a transition.
pub struct EndpointSlots {
    pub(super) gate: Arc<tokio::sync::RwLock<()>>,
    pub(super) state: Mutex<EndpointState>,
    /// In-flight paging retains its exact admission after a requester leaves.
    /// Physical slot permits bound this set; no queued admissions run in it.
    pub(super) paging_tasks: Mutex<tokio::task::JoinSet<()>>,
    /// The read side of `EndpointState::ready`, for turns that hold through a transition.
    pub(super) ready_rx: tokio::sync::watch::Receiver<bool>,
    /// Residents a relaunch discarded unsaved, until their next turn takes the record
    /// (card 6b28a445). On the endpoint because the pool does not outlive the engine;
    /// retention is bounded by [`DiscardLog`].
    pub(super) discarded: Mutex<DiscardLog>,
}

pub struct EndpointAdmission {
    pub(super) endpoint: Arc<EndpointSlots>,
    pub(super) _guard: tokio::sync::OwnedRwLockReadGuard<()>,
    pub pool: Option<Arc<KvSlotPool>>,
}

impl EndpointAdmission {
    /// The response must name this owned engine's launch alias. The read lease
    /// prevents replacement while this evidence is checked; a discovered or
    /// remote endpoint without a launch contract cannot certify local service.
    pub(crate) fn served_geometry(&self, response_model: &str) -> Option<crate::inference::served_window_store::ServedGeometry> {
        self.endpoint.served_geometry().filter(|g| g.model_id == response_model)
    }

    /// Called while this generation's read lease still prevents replacement.
    pub fn quarantine_paging(&self) {
        self.endpoint.quarantine_paging();
    }

    pub(crate) fn check_ready(&self) -> Result<(), String> {
        if *self.endpoint.state.lock().ready.borrow() {
            Ok(())
        } else {
            Err("serving endpoint suspended; paging or engine completion remains unverified".into())
        }
    }
}

impl EndpointSlots {
    pub(crate) fn retain_paging(&self, operation: impl std::future::Future<Output = ()> + Send + 'static) {
        let mut tasks = self.paging_tasks.lock();
        while tasks.try_join_next().is_some() {}
        // The endpoint owns the task, and the task owns the admission reader:
        // lifecycle transitions drain that reader before replacing the engine.
        tasks.spawn(operation);
    }

    /// The model this endpoint's engine was launched with, ready or not: a page switch in a
    /// relaunch's drain still waits on that engine's batches.
    pub(crate) fn launched_model(&self) -> Option<String> {
        self.state.lock().contract.as_ref().map(|contract| contract.model_id.clone())
    }

    pub fn served_geometry(&self) -> Option<crate::inference::served_window_store::ServedGeometry> {
        let state = self.state.lock();
        let generation = state.generation.as_ref()?;
        let contract = state.contract.as_ref()?;
        if generation.has_exited() || !*state.ready.borrow() || contract.context == 0 || contract.slots == 0 {
            return None;
        }
        Some(crate::inference::served_window_store::ServedGeometry {
            model_id: contract.model_id.clone(), generation: generation.id,
            per_slot_window: contract.context, lanes: contract.slots,
        })
    }
}

pub struct EndpointTransition {
    pub(super) endpoint: Arc<EndpointSlots>,
    pub(super) _guard: tokio::sync::OwnedRwLockWriteGuard<()>,
    /// Whether the endpoint was open when this writer closed it: what
    /// [`EndpointTransition::hand_back_unchanged`] restores when its operation changed nothing.
    pub(super) found_ready: bool,
}

/// Confirmed saves of every attributed resident. Retains the exclusive writer
/// borrow; revalidates generation and installed ledger before exposing that same
/// transition for composition. Lifecycle mutation through the returned transition
/// requires a fresh validation before reuse. This proves neither launch dependency
/// identity nor child exit/resource capacity.
pub(crate) struct ResidentCheckpoint<'a> {
    pub(super) transition: &'a mut EndpointTransition,
    pub(super) generation: EngineGeneration,
    pub(super) pool: Arc<KvSlotPool>,
}

/// Exact live ledger suspended by one owned operation, not a new page contract.
#[derive(Clone)]
pub(crate) struct EndpointSuspension {
    pub(super) endpoint: Arc<EndpointSlots>,
    pub(super) generation: EngineGeneration,
    pub(super) pool: Arc<KvSlotPool>,
    pub(super) contract: KvPageContract,
}

impl ResidentCheckpoint<'_> {
    /// Keep the original drained writer for eventual lifecycle composition.
    pub(crate) fn transition_for(
        &self,
        expected: &EngineGeneration,
    ) -> Result<&EndpointTransition, String> {
        if !self.generation.same_engine(expected) || self.generation.has_exited() {
            return Err("resident checkpoint generation exited or does not match".into());
        }
        let state = self.transition.endpoint.state.lock();
        if state.paging_uncertain
            || !state
                .generation
                .as_ref()
                .is_some_and(|g| g.same_engine(expected))
            || state.pool_generation != Some(expected.id)
            || !state
                .pool
                .as_ref()
                .and_then(Option::as_ref)
                .is_some_and(|p| Arc::ptr_eq(p, &self.pool))
        {
            return Err("resident checkpoint generation is no longer current".into());
        }
        Ok(self.transition)
    }
}

// A backend request can outlive its cancelled Rust future. Quarantine while the
// writer is still held, matching TurnAdmission's existing read-lease rule.
pub(super) struct CheckpointSave {
    pub(super) endpoint: Arc<EndpointSlots>,
    pub(super) pending: bool,
}

impl Drop for CheckpointSave {
    fn drop(&mut self) {
        if self.pending {
            self.endpoint.quarantine_paging();
        }
    }
}

/// Travels with the actual owned Child, including after retirement. The exit bit
/// belongs to this generation even when the endpoint already has a newer child.
#[derive(Clone)]
pub struct EngineGeneration {
    pub(super) id: Uuid,
    pub(super) exited: Arc<std::sync::atomic::AtomicBool>,
    pub(super) endpoint: std::sync::Weak<EndpointSlots>,
}
