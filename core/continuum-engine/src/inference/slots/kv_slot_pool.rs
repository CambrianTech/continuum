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
            // cost_of_loss ∝ the activity's tail tokens (what a cold re-entry
            // re-prefills), discounted by staleness — halved per hour idle — so
            // an abandoned giant eventually yields to live small activities.
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
        let keys = if load { ledger.load() } else { std::collections::HashSet::new() };
        let loaded = keys.len();
        if self.ledger.set(ledger).is_err() {
            return 0;
        }
        let snapshot = {
            let mut saved = self.saved.lock();
            saved.extend(keys);
            self.ledger_snapshot(&saved)
        };
        self.persist_ledger(snapshot);
        loaded
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
        let snapshot = {
            let saved = self.saved.lock();
            self.ledger_snapshot(&saved)
        };
        self.persist_ledger(snapshot);
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
    /// holding a resident's live KV: no scratch slot, and some slot attributed to an activity.
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

    /// Record the activity's current prompt size — the cost basis the priced
    /// eviction reads. Called at pin time with the request's token estimate.
    pub fn note_tail(&self, key: &ActivityKey, tokens: u64) {
        if let Some(lease) = self.pool.get(key) {
            lease.tail_tokens.store(tokens.max(1), Ordering::Relaxed);
        }
    }

    /// Lease the slot for `key`: warm reuse if the activity is resident, a free
    /// index otherwise, evicting the engine-chosen (LRU, later priced) resident
    /// when full. `None` only if the pool cannot free an index (all pinned).
    pub async fn lease(&self, key: ActivityKey) -> Option<u32> {
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
                        persona = %key.persona,
                        room = %key.room,
                        "all slots are pinned by in-flight turns: nothing to evict"
                    );
                    return None;
                }
                continuum_base::probe!(
                    class = "inference.slot_affinity.evicted",
                    evicted_count = evicted,
                    "all slots held — engine evicted the least-valuable activity; its \
                     warm prefix is forfeit and its next turn re-prefills (or restores \
                     its page at this node's measured switch cost — inference.kv_page.action ms)",
                );
            }
            let free = Arc::clone(&self.free);
            let res = self
                .pool
                .load_or_share(key, move |_| async move {
                    let slot = free
                        .lock()
                        .pop()
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
                        persona = %key.persona,
                        room = %key.room,
                        slot = lease.slot as u64,
                        "activity pinned to a llama-server slot — its prefix warms HERE",
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
    /// overwrites the slot, and whether to RESTORE this activity's own page.
    ///
    /// The rules, in order:
    /// - slot still holds THIS activity (warm re-lease) → no save, no restore:
    ///   the in-slot KV is fresher than any page on disk.
    /// - slot last warmed a DIFFERENT activity → save THEIRS first (their KV is
    ///   physically still in the server slot), then restore OURS if a page
    ///   exists.
    /// - slot holder unknown (fresh pool over a pre-existing server) → restore
    ///   ours if a page exists; nothing known to save.
    pub async fn lease_paged(&self, key: ActivityKey) -> Option<SlotPaging> {
        let slot = self.lease(key).await?;
        Some(self.plan_paging(slot, key))
    }

    /// Resolve the physical index from the pinned assignment, not a prior lease
    /// that another executor thread could have evicted before the pin.
    pub(crate) fn pin_slot(&self, key: &ActivityKey) -> Option<(u32, SlotPin)> {
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

    /// Caller holds the physical-slot permit and activity pin before changing attribution.
    pub(crate) fn plan_paging(&self, slot: u32, key: ActivityKey) -> SlotPaging {
        let prev = self.holders.lock().insert(slot, key);
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

    /// Pin this activity's slot for the duration of a turn. While the returned
    /// [`SlotPin`] is held, priced eviction skips this slot, so a concurrent
    /// returner can never lease (and restore into) a slot that is still decoding
    /// this turn's KV — the decoupling that forced the old restore timeout. RAII:
    /// drop the handle to release. `None` if the activity is not resident (nothing
    /// to protect — a caller that just leased it will always get `Some`).
    pub fn pin(&self, key: &ActivityKey) -> Option<SlotPin> {
        self.pool.pin(key)
    }

    /// Detach the resident identity before transient traffic overwrites a slot.
    /// The caller holds the decode permit and keeps the returned pin through the
    /// request. A later activity must restore its saved page, never mistake the
    /// transient KV for its own warm state or save it under its own filename.
    pub(crate) fn take_resident(&self, slot: u32) -> (Option<ActivityKey>, Option<SlotPin>) {
        let previous = self.holders.lock().remove(&slot);
        let pin = previous.and_then(|key| self.pin(&key));
        (previous, pin)
    }

    /// Failed/cancelled generation never proves whose KV now occupies the slot.
    /// Saved pages remain valid; only the provisional physical attribution clears.
    pub(crate) fn forget_resident(&self, slot: u32, key: ActivityKey) {
        let mut holders = self.holders.lock();
        if holders.get(&slot) == Some(&key) {
            holders.remove(&slot);
        }
    }

    /// Record that `key`'s page was successfully written to disk — it becomes
    /// restorable. Called by the adapter AFTER the save HTTP call succeeds.
    pub fn note_saved(&self, key: ActivityKey) {
        let snapshot = {
            let mut saved = self.saved.lock();
            saved.insert(key);
            self.ledger_snapshot(&saved)
        };
        self.persist_ledger(snapshot);
    }

    /// The page turned out unusable (restore failed: file gone, geometry
    /// mismatch) — stop offering it so the turn falls back to a plain prefill
    /// instead of retrying a dead restore every pin. Also called BEFORE a save
    /// overwrites the page, so an interrupted save is never restorable.
    pub fn note_page_lost(&self, key: &ActivityKey) {
        let snapshot = {
            let mut saved = self.saved.lock();
            if !saved.remove(key) {
                return;
            }
            self.ledger_snapshot(&saved)
        };
        self.persist_ledger(snapshot);
    }

    /// The set as it stands, numbered under the lock that changed it, so writes taken
    /// outside that lock can still be applied in the set's order ([`SavedLedger::write`]
    /// drops a snapshot older than the one already on disk). `None` without a ledger.
    pub(super) fn ledger_snapshot(&self, saved: &std::collections::HashSet<ActivityKey>) -> Option<(u64, Vec<(Uuid, Uuid)>)> {
        self.ledger.get()?;
        let generation = self.ledger_generation.fetch_add(1, Ordering::Relaxed) + 1;
        let mut keys: Vec<(Uuid, Uuid)> = saved.iter().map(|k| (k.persona, k.room)).collect();
        keys.sort_unstable();
        Some((generation, keys))
    }

    /// Write a snapshot beside the pages, OUTSIDE the set's lock: the write may retry a
    /// Windows sharing violation for ~1.6 s (`replace_file`), and a turn's paging must
    /// never wait on that. Nothing to write without a ledger (no page dir, or a contract
    /// that cannot be fingerprinted).
    pub(super) fn persist_ledger(&self, snapshot: Option<(u64, Vec<(Uuid, Uuid)>)>) {
        let (Some(ledger), Some((generation, keys))) = (self.ledger.get(), snapshot) else {
            return;
        };
        if let Err(error) = ledger.write(generation, keys) {
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

/// The confirmed-save set of one page dir, on disk, so it outlives the process
/// (a deploy). Before this, eligibility lived only in [`KvSlotPool::saved`], which
/// starts empty in every core: on the M5 2026-10-10 the first turns after the 18:49Z
/// deploy reused nothing (turn_cached 0 of 31,811 and 27,250) while their pages sat on
/// disk. The file holds exactly the in-memory set: a key enters only after the engine
/// confirms its save and leaves before any save overwrites it, so a timed-out or
/// interrupted save is never in it. It is read back only under the SAME contract
/// fingerprint it was written with (the exact-contract rule of #4367, unchanged).
pub(crate) struct SavedLedger {
    pub(super) path: std::path::PathBuf,
    pub(super) fingerprint: String,
    /// The generation of the snapshot on disk; a write serializes on this and skips a
    /// snapshot older than it.
    pub(super) written: Mutex<u64>,
    /// The engine process these pages belong to, stamped by the spawn once the engine is
    /// ready ([`stamp_engine_incarnation`]) or carried over by an adoption.
    pub(super) incarnation: std::sync::OnceLock<(u32, u64)>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct SavedLedgerFile {
    pub(super) fingerprint: String,
    pub(super) keys: Vec<(Uuid, Uuid)>,
    /// The engine process that wrote these pages: (pid, OS start time). A core that
    /// ADOPTS a running engine has no launch contract of its own; this is how it proves
    /// the ledger is that very process's, and so under its contract. Absent on a ledger
    /// written before the spawn stamped it: such a ledger is never adopted.
    #[serde(default)]
    pub(super) incarnation: Option<(u32, u64)>,
}

/// The ledger's file name inside a page dir. Not an `a-*.bin` page, so the page
/// store's trim never counts or removes it; the generation sweep removes it with its dir.
pub(super) const SAVED_LEDGER_FILE: &str = "saved-ledger.json";

impl SavedLedger {
    pub(super) fn in_dir(page_dir: &std::path::Path, fingerprint: String) -> Self {
        Self {
            path: page_dir.join(SAVED_LEDGER_FILE),
            fingerprint,
            written: Mutex::new(0),
            incarnation: std::sync::OnceLock::new(),
        }
    }

    /// The ledger a running engine left in `page_dir`, if that engine is the process
    /// `incarnation` names: the fingerprint it was written under becomes this ledger's
    /// own, because it is the same process under the same launch contract. Any other
    /// ledger (another process, an unstamped one, none) is not this engine's.
    pub(super) fn of_running_engine(page_dir: &std::path::Path, incarnation: (u32, u64)) -> Result<Self, AdoptRefusal> {
        let bytes = match std::fs::read(page_dir.join(SAVED_LEDGER_FILE)) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(AdoptRefusal::NoLedger),
            Err(_) => return Err(AdoptRefusal::Unreadable),
        };
        let file = serde_json::from_slice::<SavedLedgerFile>(&bytes).map_err(|_| AdoptRefusal::Unreadable)?;
        match file.incarnation {
            None => return Err(AdoptRefusal::Unstamped),
            Some(written_by) if written_by != incarnation => return Err(AdoptRefusal::AnotherProcess),
            Some(_) => {}
        }
        let ledger = Self::in_dir(page_dir, file.fingerprint);
        let _ = ledger.incarnation.set(incarnation);
        Ok(ledger)
    }

    /// Atomic replace (write a sibling, then `replace_file`) into a dir that must already
    /// exist: the lane creates its page dir; the ledger never creates one. Writes are
    /// serialized and a snapshot older than the one on disk is dropped, so the file is
    /// always the newest set even when writers race.
    pub(super) fn write(&self, generation: u64, keys: Vec<(Uuid, Uuid)>) -> std::io::Result<()> {
        let mut written = self.written.lock();
        if generation <= *written {
            return Ok(());
        }
        let file = SavedLedgerFile { fingerprint: self.fingerprint.clone(), keys, incarnation: self.incarnation.get().copied() };
        let bytes = serde_json::to_vec(&file)?; // file on disk: the page dir's confirmed-save ledger, read back by `load`
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, bytes)?;
        continuum_base::fs_portable::replace::replace_file(&tmp, &self.path)?;
        *written = generation;
        Ok(())
    }

    /// The keys this ledger may restore: written under the same fingerprint, with
    /// their page file still present. Anything else is no eligibility, never a guess:
    /// a missing file, another contract's ledger, a malformed one.
    pub(super) fn load(&self) -> std::collections::HashSet<ActivityKey> {
        let Some(dir) = self.path.parent() else {
            return std::collections::HashSet::new();
        };
        let unread = |why: &str| {
            continuum_base::probe!(
                class = "inference.kv_page.ledger_unread",
                path = %self.path.display(),
                why = why,
                "no confirmed-save ledger restored: the pages in this dir re-prefill once"
            );
            std::collections::HashSet::new()
        };
        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return std::collections::HashSet::new(),
            Err(_) => return unread("unreadable"),
        };
        let Ok(file) = serde_json::from_slice::<SavedLedgerFile>(&bytes) else {
            return unread("malformed");
        };
        if file.fingerprint != self.fingerprint {
            return unread("written under another contract");
        }
        let listed = file.keys.len();
        let keys: Vec<ActivityKey> = file.keys.into_iter().filter_map(|(persona, room)| ActivityKey::new(persona, room)).collect();
        if keys.len() < listed {
            // A key with a nil persona or room names no activity: its page re-prefills once.
            continuum_base::probe!(
                class = "inference.kv_page.ledger_keys_dropped",
                path = %self.path.display(),
                dropped = (listed - keys.len()) as u64,
                "confirmed-save ledger entries without an activity were dropped: their pages re-prefill once"
            );
        }
        keys.into_iter().filter(|k| dir.join(page_filename(k)).is_file()).collect()
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
    pub engine: String,
    /// Cold-path revision evidence for the resolved model, adapters and engine.
    /// Unknown revisions refuse saved-page carry-over, never guess compatibility.
    pub revisions: Option<Vec<(std::path::PathBuf, u64, std::time::SystemTime)>>,
}

impl KvPageContract {
    /// A stable digest of the whole contract, for the on-disk confirmed-save ledger:
    /// two contracts with the same fingerprint are equal field for field. `None` under
    /// the same conditions that refuse in-process carry-over (no page dir, unknown cache
    /// type, unknown revisions): what cannot be compared exactly is never persisted.
    pub(crate) fn fingerprint(&self) -> Option<String> {
        use sha2::Digest;
        let page_dir = self.page_dir.as_ref()?;
        let cache_type = self.cache_type.as_ref()?;
        let revisions = self.revisions.as_ref()?;
        let mut h = sha2::Sha256::new();
        let mut field = |label: &str, value: &[u8]| {
            h.update(label.as_bytes());
            h.update((value.len() as u64).to_le_bytes());
            h.update(value);
        };
        field("model_id", self.model_id.as_bytes());
        field("model", self.model.as_os_str().as_encoded_bytes());
        for a in &self.adapters {
            field("adapter", a.as_bytes());
        }
        field("page_dir", page_dir.as_os_str().as_encoded_bytes());
        field("context", &self.context.to_le_bytes());
        field("slots", &self.slots.to_le_bytes());
        field("cache_type", cache_type.as_bytes());
        field("engine", self.engine.as_bytes());
        for (path, size, modified) in revisions {
            field("revision", path.as_os_str().as_encoded_bytes());
            field("size", &size.to_le_bytes());
            let nanos = modified.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).ok()?;
            field("modified", &nanos.to_le_bytes());
        }
        Some(format!("{:x}", h.finalize()))
    }
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
    /// The read side of `EndpointState::ready`, for turns that hold through a transition.
    pub(super) ready_rx: tokio::sync::watch::Receiver<bool>,
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
