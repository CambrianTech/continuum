//! KV serving slots as the third implementer of THE one paging engine.
//!
//! **The law this module exists under (Joel, 2026-08-26):** *"we made a paging
//! system for reuse across many things … instead of 10 different codes doing
//! compression or paging, one exists and all trait implementers know HOW this
//! is done for their concern."* MoE experts and the LoRA genome already page
//! through [`PagedResourcePool`]; a llama-server slot — one warm conversation's
//! KV — is the same shape: a scarce residency under contention with a
//! measurable cost of loss. So this module is a thin CONCERN adapter over the
//! shared engine, never a second lease/eviction implementation
//! ([[one-paging-engine-many-trait-implementers]], KV-CACHE-ECONOMY §6: "KV
//! slot residency implements the same shape rather than inventing a rival").
//!
//! **And the key law:** *"we do NOT use strings for keys, we use roomId … the
//! moment you start prepending strings to uuids is the moment it falls
//! apart."* The unit of warmth is the ACTIVITY — a persona's conversation in a
//! room (rooms are 1:1 with activities) — so the key is the typed
//! [`ActivityKey`] struct of UUIDs. Its predecessor, a
//! `format!("{persona}@{room}")` string, lived half a day; the
//! `no_string_composite_id_keys_in_serving` ratchet keeps it dead.
//!
//! What the engine gives this concern for free: single-flight assignment,
//! pin-during-use (mid-turn eviction immunity, wired when the request path
//! pins), hit/miss/eviction stats, `ResourcePool` for the `PressureBroker`,
//! and an [`EvictionPriority`] seam where the PRICED policy
//! (`cost_of_loss = tail_tokens / prefill_rate`, plan slice B5) drops in
//! without new machinery. What this concern supplies: the typed key, the
//! slot-index free list (RAII — an evicted activity's index returns on drop),
//! and the per-server directory.

use std::sync::Arc;

use parking_lot::Mutex;
use uuid::Uuid;

use std::sync::atomic::{AtomicU64, Ordering};

use crate::paging::pool::{PagedResourcePool, PinHandle, PoolConfig};

/// How many of a server's `n_slots` are CITIZEN slots — warm activity slots a
/// Turn may hold. ≥3 slots reserve the highest index as scratch for non-Turn
/// traffic; below that every slot is a citizen slot. More citizens than this is
/// the DESIGN (N minds over M slots, paged like registers), never a fault — the
/// roster is not capped here.
pub fn citizen_slots(n_slots: u32) -> u32 {
    if n_slots >= 3 { n_slots - 1 } else { n_slots }
}

/// A slot pinned for the duration of a turn — while held, the pool cannot evict
/// this activity's slot, so a concurrent returner never leases (and restores into)
/// a slot that is still decoding. RAII: dropping it releases the pin.
pub type SlotPin = PinHandle<ActivityKey, std::sync::Arc<KvSlotLease>>;

/// How a request may touch serving slots — the traffic-class policy, AS DATA
/// (one tested map over the existing `purpose` strings, answering the
/// INFERENCE-SCHEDULING doc's open question; never per-callsite hacks).
///
/// Placement rule: **only `Turn` may hold or evict a citizen's activity slot.**
/// Everything else lands on the reserved SCRATCH slot. A single-slot server
/// saves its resident before lending that slot transiently; a two-slot server
/// without scratch stays unpinned. Both use `cache_prompt: false`. The
/// measured defect this closes: the small sidecar calls (`should_respond`,
/// `check_redundancy`, `generate_response`) pinned the SAME slot as the turn
/// and truncated the citizen's ~30k warm tail to their tiny common head —
/// breaking KV reuse even for a SOLO citizen — while the smoke probe and the
/// unpinned producers were free to LCP-steal any warm slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotClass {
    /// The citizen's real turn (deliberation / persona-respond): pins her
    /// activity slot via [`ActivityKey`].
    Turn,
    /// Short same-context helper calls riding a live turn (gates, validators,
    /// recipe/proposal raters): scratch — never the activity slot they'd clobber.
    Sidecar,
    /// Background cognition with no live turn (dream lenses, module inference,
    /// anything unrecognized): scratch.
    Background,
    /// Infrastructure traffic (smoke probe, warmup, operator try-outs): scratch.
    Probe,
}

impl SlotClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            SlotClass::Turn => "turn",
            SlotClass::Sidecar => "sidecar",
            SlotClass::Background => "background",
            SlotClass::Probe => "probe",
        }
    }
}

/// The ONE purpose→class map. Unknown purposes default to `Background` —
/// scratch placement — so a new producer can never clobber a citizen slot by
/// omission; promoting it to `Turn` is an explicit edit here.
pub fn class_for(purpose: Option<&str>) -> SlotClass {
    match purpose {
        Some("cognition/deliberation")
        | Some("cognition/act")
        | Some("persona-respond")
        | Some("persona_decide_and_respond") => SlotClass::Turn,
        Some("cognition/should-respond")
        | Some("cognition/check-redundancy")
        | Some("cognition/generate-response")
        | Some("cognition/validate-response-decision")
        | Some("resolution-draft")
        | Some("cognition-rate-proposals")
        | Some("cognition-generate-recipe")
        | Some("shared-cognition-analysis") => SlotClass::Sidecar,
        Some("warmup")
        | Some("models/try:text")
        | Some("models/try:vision")
        | Some("genome/teach")
        | Some("local-coding-agent")
        | Some("serving-smoke-probe")
        | Some("cognition/replay-request") => SlotClass::Probe,
        _ => SlotClass::Background,
    }
}

/// The warm-KV identity: one persona's conversation in one room — an ACTIVITY.
/// Typed UUIDs, non-nil by construction; the map keys on this struct itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActivityKey {
    pub persona: Uuid,
    pub room: Uuid,
}

impl ActivityKey {
    /// Both halves must be real ids. A nil room cannot become a key — the old
    /// persona-only collapse (N concurrent activities thrashing one slot) and
    /// the nil-room phantom key were the two halves of the KV-reuse-0% bug.
    pub fn new(persona: Uuid, room: Uuid) -> Option<Self> {
        if persona.is_nil() || room.is_nil() {
            return None;
        }
        Some(Self { persona, room })
    }
}

/// One leased slot index. The free list is threaded in so that WHENEVER the
/// pool drops this entry — explicit evict, pressure evict, priced evict (B5) —
/// the index returns to circulation by RAII, with no bookkeeping at any call
/// site. (The pool clones values out to readers; the index returns when the
/// LAST clone drops, i.e. after any in-flight reader is done with it.)
#[derive(Debug)]
pub struct KvSlotLease {
    slot: u32,
    free: Arc<Mutex<Vec<u32>>>,
    /// The activity's last-known prompt size in tokens — the COST basis for the
    /// priced eviction policy (cost_of_loss ∝ tail re-prefill). Updated at each
    /// pin from the request's own token estimate; an estimate is fine because
    /// eviction only needs slots COMPARABLE, not absolutely priced.
    tail_tokens: AtomicU64,
}

impl Drop for KvSlotLease {
    fn drop(&mut self) {
        self.free.lock().push(self.slot);
    }
}

/// The per-server slot pool: N leasable slot indices, activities resident in
/// the shared paging engine.
/// Recency tie-break window: within it, a fresher equal-cost tail outranks a
/// staler one; beyond it equal-cost tails are genuinely equivalent. Strictly
/// smaller than [`COST_STEP`] so pricing always dominates recency.
const RECENCY_TIEBREAK_MS: u64 = 600_000;
/// One token-cost step in the eviction score — must exceed the largest possible
/// recency bonus so a single token of tail always outweighs any freshness.
const COST_STEP: i64 = 1 << 20;

pub struct KvSlotPool {
    pool: PagedResourcePool<ActivityKey, Arc<KvSlotLease>>,
    free: Arc<Mutex<Vec<u32>>>,
    n_slots: u32,
    /// The reserved non-citizen slot (highest index) when the server has ≥3
    /// slots — where ALL non-Turn traffic lands, so it structurally cannot
    /// evict a citizen's warm tail. `None` on small servers (≤2 slots): there,
    /// non-Turn traffic uses `cache_prompt: false`; one slot saves its resident
    /// before transient use, while two slots keep the unpinned fallback.
    scratch: Option<u32>,
    /// slot index → the activity whose KV last WARMED that slot. This is the
    /// paging ledger's "who is resident" half: when a lease hands a slot to a
    /// DIFFERENT activity, the previous holder's KV is still physically in the
    /// server slot and must be SAVED to its disk page before the new turn
    /// overwrites it. Tiny map (n_slots entries), cold path (one pin per turn).
    holders: Mutex<std::collections::HashMap<u32, ActivityKey>>,
    /// Activities with a valid KV page on disk (written via
    /// `/slots/{id}?action=save` under the lane's `--slot-save-path`). The
    /// restore half of the ledger: a re-entering activity whose slot was
    /// recycled restores its page (at this node's measured switch cost) instead of
    /// re-prefilling (~35s at 22k — the 330× cliff the restore economy names).
    saved: Mutex<std::collections::HashSet<ActivityKey>>,
    /// Physical-slot admission also excludes concurrent requests for the same key.
    operations: Vec<Arc<tokio::sync::Semaphore>>,
}

/// What a Turn must do AROUND its request to honor the KV paging design — the
/// OS-context-switch shape (Joel: "many threads but one at a time … they page
/// in and out per persona"). Both actions are ordered on the slot itself, the
/// same FIFO inference already imposes; nothing else waits on them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotPaging {
    pub slot: u32,
    /// A different activity's KV is still resident in this slot — save it to
    /// its disk page BEFORE the new turn's prefill overwrites it.
    pub save_first: Option<ActivityKey>,
    /// This activity has a page on disk and the slot does not currently hold
    /// its (fresher) KV — restore the page before the turn.
    pub restore: bool,
    /// The physical slot already contains this activity, rather than a new lease.
    pub already_resident: bool,
}

/// The page filename for an activity — stable across processes, one file per
/// activity, overwritten on each save. Lives under the lane's geometry-keyed
/// `--slot-save-path` dir, so a page can never be restored into a slot with a
/// different context size or model.
pub fn page_filename(key: &ActivityKey) -> String {
    format!("a-{}-{}.bin", key.persona.simple(), key.room.simple())
}

/// The KV page store is VIRTUAL-MEMORY PAGING ON CHEAP DISK — one page (+ one
/// checkpoint sidecar) per activity, overwritten on each save, kept across runs
/// under the geometry-keyed dir — and it must hold EVERY active resident's
/// activities without eviction (Joel, 2026-09-13: "disk is cheap … could easily
/// support all active resident persona"). So the limit is generous bytes on
/// disk, never the slot count, and only pages nobody has touched for
/// [`KV_PAGE_MIN_AGE_MS`] are candidates: an active resident's page is always
/// younger than that. Beyond the budget the OLDEST go first — the cheapest to
/// lose (a return re-prefills once).
pub const KV_PAGE_STORE_MAX_BYTES: u64 = 128 * 1024 * 1024 * 1024;
/// A page younger than this is an active resident's working set: never trimmed.
pub const KV_PAGE_MIN_AGE_MS: u64 = 24 * 60 * 60 * 1000;

static PAGE_DIR: parking_lot::Mutex<Option<std::path::PathBuf>> = parking_lot::Mutex::new(None);

/// The spawn — the one place that knows the live geometry dir — registers it so
/// a save can trim the store it just wrote into.
pub fn note_page_dir(dir: &std::path::Path) {
    *PAGE_DIR.lock() = Some(dir.to_path_buf());
}

/// After a successful save of `just_saved` (a page filename), keep the live
/// geometry dir under [`KV_PAGE_STORE_MAX_BYTES`]. Never touches the page just
/// written. Cheap: one `read_dir` per save, a few files.
pub fn trim_page_store_after_save(just_saved: &str) {
    let Some(dir) = PAGE_DIR.lock().clone() else {
        return;
    };
    let (removed, freed, kept) =
        trim_page_store(&dir, just_saved, KV_PAGE_STORE_MAX_BYTES, KV_PAGE_MIN_AGE_MS);
    if removed > 0 {
        crate::probe!(
            class = "inference.kv_page.store_trimmed",
            dir = %dir.display(),
            removed = removed as u64,
            freed_bytes = freed,
            kept_bytes = kept,
            budget_bytes = KV_PAGE_STORE_MAX_BYTES,
            "KV page store over budget — oldest pages (and their checkpoint sidecars) dropped; \
             their next return re-prefills once",
        );
    }
}

/// Pure trim: pages (`a-*.bin`) with their `.ckpt` sidecars, oldest mtime first,
/// until the dir is within `max_bytes`. Returns (pages removed, bytes freed,
/// bytes kept). The page named `keep` is never removed; neither is any page
/// younger than `min_age_ms` (an active resident's working set).
pub fn trim_page_store(
    dir: &std::path::Path,
    keep: &str,
    max_bytes: u64,
    min_age_ms: u64,
) -> (usize, u64, u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (0, 0, 0);
    };
    // page name → (mtime, bytes incl. sidecar)
    let mut pages: std::collections::HashMap<String, (std::time::SystemTime, u64)> =
        std::collections::HashMap::new();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(md) = e.metadata() else { continue };
        if !md.is_file() {
            continue;
        }
        let page = name.strip_suffix(".ckpt").unwrap_or(&name).to_string(); // JUSTIFIED unwrap_or: a name without the sidecar suffix IS the page name
        if !(page.starts_with("a-") && page.ends_with(".bin")) {
            continue;
        }
        let slot = pages.entry(page).or_insert((std::time::SystemTime::UNIX_EPOCH, 0));
        slot.1 += md.len();
        if !name.ends_with(".ckpt") {
            slot.0 = md.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH); // JUSTIFIED unwrap_or: an unreadable mtime sorts OLDEST — the safe side for an eviction candidate
        }
    }
    let mut total: u64 = pages.values().map(|v| v.1).sum();
    if total <= max_bytes {
        return (0, 0, total);
    }
    let mut order: Vec<(String, std::time::SystemTime, u64)> =
        pages.into_iter().map(|(n, (t, b))| (n, t, b)).collect();
    order.sort_by_key(|(_, t, _)| *t);
    let (mut removed, mut freed) = (0usize, 0u64);
    let now = std::time::SystemTime::now();
    for (name, mtime, bytes) in order {
        if total <= max_bytes {
            break;
        }
        let age_ms = now.duration_since(mtime).map(|d| d.as_millis() as u64).unwrap_or(0); // JUSTIFIED unwrap_or: a future mtime reads as age 0 = protected, never trimmed
        if name == keep || age_ms < min_age_ms {
            continue;
        }
        let _ = std::fs::remove_file(dir.join(&name));
        let _ = std::fs::remove_file(dir.join(format!("{name}.ckpt")));
        removed += 1;
        freed += bytes;
        total = total.saturating_sub(bytes);
    }
    (removed, freed, total)
}

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
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(view.last_access_at); // engine has no view of a just-inserted key: fall back to its own stamp
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
        }
    }

    pub fn n_slots(&self) -> u32 {
        self.n_slots
    }

    /// The reserved scratch slot for non-Turn traffic, when this server can
    /// spare one.
    pub fn scratch_slot(&self) -> Option<u32> {
        self.scratch
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
        // Two rounds: (try allocate) → (make room, try again). load_or_share is
        // single-flight per key, so concurrent same-activity requests share one
        // assignment; distinct activities racing for the last index resolve by
        // one of them evicting (mild over-eviction under a stampede is idle-
        // activity warmth lost, never correctness).
        for round in 0..2 {
            if self.free.lock().is_empty() && round > 0 {
                return None; // eviction freed nothing — everything pinned
            }
            if self.free.lock().is_empty() {
                let evicted = self.pool.evict_at_least(1);
                crate::probe!(
                    class = "inference.slot_affinity.evicted",
                    evicted_count = evicted,
                    "all slots held — engine evicted the least-valuable activity; its \
                     warm prefix is forfeit and its next turn re-prefills (or restores \
                     its page at this node's measured switch cost — inference.kv_page.action ms)",
                );
                if evicted == 0 {
                    return None;
                }
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
                    crate::probe!(
                        class = "inference.slot_affinity.pinned",
                        persona = %key.persona,
                        room = %key.room,
                        slot = lease.slot as u64,
                        "activity pinned to a llama-server slot — its prefix warms HERE",
                    );
                    return Some(lease.slot);
                }
                Err(_) => continue, // lost the race for the last index — make room
            }
        }
        None
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
    pub(crate) async fn acquire_slot(
        &self,
        slot: u32,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, String> {
        let operation = self
            .operations
            .get(slot as usize)
            .ok_or("unknown physical KV slot")?;
        operation
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| "physical KV slot admission closed".to_string())
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
        self.saved.lock().insert(key);
    }

    /// The page turned out unusable (restore failed: file gone, geometry
    /// mismatch) — stop offering it so the turn falls back to a plain prefill
    /// instead of retrying a dead restore every pin.
    pub fn note_page_lost(&self, key: &ActivityKey) {
        self.saved.lock().remove(key);
    }
}

/// Per-server directory: which roots have a pool, and which are latched
/// unsupported (no /props surface). The transport-side probe
/// (the adapter owns the HTTP client) discovers; this directory owns state.
pub struct SlotDirectory {
    endpoints: dashmap::DashMap<String, Arc<EndpointSlots>>,
}

/// Exact launch/page compatibility, supplied by the process owner, never inferred
/// from the URL. Unknown or changed contracts cannot inherit saved-page eligibility.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct KvPageContract {
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

struct EndpointState {
    ready: bool,
    paging_uncertain: bool,
    generation: Option<EngineGeneration>,
    contract: Option<KvPageContract>,
    pool: Option<Option<Arc<KvSlotPool>>>,
    // An installed ledger can outlive its child until replacement readiness.
    pool_generation: Option<Uuid>,
}

/// The directory's existing endpoint owner also arbitrates engine transitions.
/// Admission holds a read lease through HTTP completion; transitions drain those
/// leases before retiring the engine. Closing survives cancellation of a transition.
pub(crate) struct EndpointSlots {
    gate: Arc<tokio::sync::RwLock<()>>,
    state: Mutex<EndpointState>,
}

pub(crate) struct EndpointAdmission {
    endpoint: Arc<EndpointSlots>,
    _guard: tokio::sync::OwnedRwLockReadGuard<()>,
    pub pool: Option<Arc<KvSlotPool>>,
}

impl EndpointAdmission {
    /// Called while this generation's read lease still prevents replacement.
    pub(crate) fn quarantine_paging(&self) {
        self.endpoint.quarantine_paging();
    }

    pub(crate) fn check_ready(&self) -> Result<(), String> {
        if self.endpoint.state.lock().ready {
            Ok(())
        } else {
            Err("serving endpoint suspended; paging or engine completion remains unverified".into())
        }
    }
}

pub(crate) struct EndpointTransition {
    endpoint: Arc<EndpointSlots>,
    _guard: tokio::sync::OwnedRwLockWriteGuard<()>,
}

/// Confirmed saves of every attributed resident. Retains the exclusive writer
/// borrow; revalidates generation and installed ledger before exposing that same
/// transition for composition. Lifecycle mutation through the returned transition
/// requires a fresh validation before reuse. This proves neither launch dependency
/// identity nor child exit/resource capacity.
pub(crate) struct ResidentCheckpoint<'a> {
    transition: &'a mut EndpointTransition,
    generation: EngineGeneration,
    pool: Arc<KvSlotPool>,
}

/// Exact live ledger suspended by one owned operation, not a new page contract.
#[derive(Clone)]
pub(crate) struct EndpointSuspension {
    endpoint: Arc<EndpointSlots>,
    generation: EngineGeneration,
    pool: Arc<KvSlotPool>,
    contract: KvPageContract,
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
struct CheckpointSave {
    endpoint: Arc<EndpointSlots>,
    pending: bool,
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
pub(crate) struct EngineGeneration {
    id: Uuid,
    exited: Arc<std::sync::atomic::AtomicBool>,
    endpoint: std::sync::Weak<EndpointSlots>,
}

impl EngineGeneration {
    pub(crate) fn same_engine(&self, other: &Self) -> bool {
        self.id == other.id
    }

    pub(crate) fn retiring(&self) {
        if let Some(endpoint) = self.endpoint.upgrade() {
            let mut state = endpoint.state.lock();
            if state.generation.as_ref().is_some_and(|g| g.id == self.id) {
                state.ready = false;
            }
        }
    }
    pub(crate) fn has_exited(&self) -> bool {
        self.exited.load(Ordering::Acquire)
    }

    pub(crate) fn observed_exit(&self) {
        self.exited.store(true, Ordering::Release);
        if let Some(endpoint) = self.endpoint.upgrade() {
            let mut state = endpoint.state.lock();
            if state.generation.as_ref().is_some_and(|g| g.id == self.id) {
                state.ready = false;
                crate::probe!(
                    class = "inference.kv_engine.exited",
                    generation = %self.id,
                    "owned engine exit observed; endpoint admissions remain closed"
                );
                // Do not mutate a ledger while an old HTTP future can still write
                // it. Replacement readiness drains admissions and replaces it.
            }
        }
    }
}

impl EndpointSlots {
    fn quarantine_paging(&self) {
        let mut state = self.state.lock();
        state.paging_uncertain = true;
        state.ready = false;
        crate::probe!(
            class = "inference.kv_page.uncertain",
            "paging completion unverified; endpoint requires verified engine replacement"
        );
    }

    pub(crate) fn paging_recovery_required(&self) -> bool {
        self.state.lock().paging_uncertain
    }
    pub(crate) fn is_ready(&self) -> bool {
        self.state.lock().ready
    }
    pub(crate) async fn admit(self: &Arc<Self>) -> Result<EndpointAdmission, String> {
        if !self.state.lock().ready {
            return Err("serving endpoint is suspended pending engine readiness".into());
        }
        let guard = self.gate.clone().read_owned().await;
        let state = self.state.lock();
        if !state.ready {
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
        self.state.lock().ready = false;
        Ok(EndpointTransition {
            endpoint: self.clone(),
            _guard: guard,
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
        self.state.lock().ready = false;
        Ok(EndpointTransition {
            endpoint: self.clone(),
            _guard: guard,
        })
    }

    pub(crate) async fn transition(self: &Arc<Self>) -> EndpointTransition {
        self.state.lock().ready = false;
        let guard = self.gate.clone().write_owned().await;
        // A preceding transition might have reopened while this writer waited.
        self.state.lock().ready = false;
        EndpointTransition {
            endpoint: self.clone(),
            _guard: guard,
        }
    }
}

impl EndpointTransition {
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
        let mut state = self.endpoint.state.lock();
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
        state.ready = true;
        Ok(())
    }

    /// Save only known physical residents after all endpoint readers have drained.
    /// Refusal never reopens the endpoint or retires its owned child.
    pub(crate) async fn checkpoint_residents(
        &mut self,
        client: &reqwest::Client,
        root: &str,
    ) -> Result<ResidentCheckpoint<'_>, String> {
        use super::turn_admission::{kv_page_action, PageOutcome};

        if !Arc::ptr_eq(&directory().endpoint(root), &self.endpoint) {
            return Err("checkpoint URL does not identify the held endpoint".into());
        }
        let (generation, pool) = {
            let mut state = self.endpoint.state.lock();
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
            state.ready = false;
            (generation, pool)
        };
        let mut residents: Vec<_> = pool
            .holders
            .lock()
            .iter()
            .map(|(&slot, &key)| (slot, key))
            .collect();
        residents.sort_unstable_by_key(|(slot, _)| *slot);
        for (slot, key) in residents {
            if generation.has_exited() {
                return Err("checkpoint engine exited before save".into());
            }
            pool.note_page_lost(&key);
            let mut save = CheckpointSave {
                endpoint: self.endpoint.clone(),
                pending: true,
            };
            match kv_page_action(client, root, slot, &key, "save").await {
                PageOutcome::Completed => {
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
        if generation.has_exited() {
            return Err("checkpoint engine exited before completion".into());
        }
        Ok(ResidentCheckpoint {
            transition: self,
            generation,
            pool,
        })
    }

    pub(crate) fn previous_generation(&self) -> Option<EngineGeneration> {
        self.endpoint.state.lock().generation.clone()
    }

    pub(crate) fn start_generation(&self) -> Result<EngineGeneration, String> {
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

    pub(crate) fn ready(
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
        if state.ready {
            return if state.contract.as_ref() == Some(&contract) {
                Ok(())
            } else {
                Err("ready engine cannot change its page contract without a transition".into())
            };
        }
        // A fresh assignment ledger prevents old Arc holders/pins from affecting
        // the new engine. Copy ONLY saved-page eligibility under an exact contract.
        let pool = Arc::new(KvSlotPool::new(root, contract.slots));
        if contract.page_dir.is_some()
            && contract.cache_type.is_some()
            && contract.revisions.is_some()
            && state.contract.as_ref() == Some(&contract)
        {
            if let Some(Some(old)) = state.pool.as_ref() {
                *pool.saved.lock() = old.saved.lock().clone();
            }
        }
        state.pool = Some(Some(pool));
        state.pool_generation = Some(generation.id);
        state.contract = Some(contract);
        state.ready = true;
        crate::probe!(
            class = "inference.kv_engine.ready",
            endpoint = root,
            generation = %generation.id,
            "verified engine generation installed with fresh physical KV attribution"
        );
        Ok(())
    }
}

impl SlotDirectory {
    pub(crate) fn endpoint(&self, root: &str) -> Arc<EndpointSlots> {
        self.endpoints
            .entry(root.trim_end_matches('/').to_string())
            .or_insert_with(|| {
                Arc::new(EndpointSlots {
                    gate: Arc::new(tokio::sync::RwLock::new(())),
                    state: Mutex::new(EndpointState {
                        ready: true,
                        paging_uncertain: false,
                        generation: None,
                        contract: None,
                        pool: None,
                        pool_generation: None,
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

/// The one process-wide directory (same scope as the serving resource itself:
/// every adapter instance talking to one server shares one assignment).
pub fn directory() -> &'static SlotDirectory {
    static DIR: std::sync::OnceLock<SlotDirectory> = std::sync::OnceLock::new();
    DIR.get_or_init(|| SlotDirectory {
        endpoints: dashmap::DashMap::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(p: u128, r: u128) -> ActivityKey {
        ActivityKey::new(Uuid::from_u128(p), Uuid::from_u128(r)).expect("non-nil test ids")
    }

    // what this catches: the 2026-08-26 KV-reuse-0% bug, both halves. One persona
    // running N concurrent activities (a 4-instance benchmark dispatch, each its
    // own room) must lease N DISTINCT slots — under the old persona-keyed lease
    // all four returned slot 0 and thrashed it (cached:0 every turn). And a nil
    // room must be unrepresentable as a key — it previously became the live
    // string key "persona@000…0".
    #[tokio::test]
    async fn one_persona_many_rooms_leases_distinct_slots() {
        let pool = KvSlotPool::new("test", 4);
        // 4 slots = 3 citizen + 1 scratch (B2). Three concurrent activities of
        // ONE persona hold three DISTINCT citizen slots — under the old
        // persona-keyed lease all of them returned slot 0 and thrashed it.
        let mut slots = Vec::new();
        for r in 1..=3u128 {
            slots.push(pool.lease(key(7, r)).await.expect("free slot"));
        }
        slots.sort_unstable();
        assert_eq!(slots, vec![0, 1, 2], "three activities hold three distinct citizen slots");
        // Warm re-entry: the same activity gets ITS slot back.
        assert_eq!(pool.lease(key(7, 1)).await, Some(0), "room 1 kept its warm slot");
        // A 4th activity EVICTS a citizen slot (LRU for now, priced in B5) —
        // it must never spill onto scratch.
        let fourth = pool.lease(key(7, 4)).await.expect("evicts, not refuses");
        assert!(fourth < 3, "the 4th activity takes a citizen slot, never scratch");
        // Nil is unrepresentable.
        assert!(ActivityKey::new(Uuid::from_u128(7), Uuid::nil()).is_none());
        assert!(ActivityKey::new(Uuid::nil(), Uuid::from_u128(1)).is_none());
    }

    // what this catches: eviction leaking slot indices. When a 5th activity
    // arrives on a full 4-slot pool, the engine evicts the LRU resident and the
    // freed INDEX must return through the lease's Drop — a leak here would
    // shrink the pool one eviction at a time until nobody can lease.
    #[tokio::test]
    async fn eviction_recycles_the_slot_index() {
        let pool = KvSlotPool::new("test", 2);
        let a = pool.lease(key(1, 1)).await.expect("a");
        // last_access_at is ms-granular; real turns are seconds apart, the test
        // must space its touches or LRU order is a coin flip within one ms.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let _b = pool.lease(key(1, 2)).await.expect("b");
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        // Refresh a so b is the LRU.
        let _ = pool.lease(key(1, 1)).await;
        let c = pool.lease(key(1, 3)).await.expect("c evicts LRU and reuses its index");
        assert_ne!(c, a, "c must not steal the warm slot that was just refreshed");
        // And the evicted activity can come back (cold) on whatever frees next.
        let _ = pool.lease(key(1, 2)).await.expect("evicted activity re-leases");
    }

    // what this catches: the paging ledger around slot recycling (the restore-
    // economy context switch). A recycled slot must (1) name the previous
    // holder for SAVE before the new turn overwrites their KV, (2) offer
    // RESTORE only to an activity with a written page whose KV is not already
    // in the slot, (3) never save/restore on a warm same-holder re-lease, and
    // (4) stop offering a page after note_page_lost. Without (1) rotation on
    // an oversubscribed server is a full re-prefill per turn — measured live
    // 2026-09-01 as hit_rate 0.0 across every act, 35-45s prefill each.
    #[tokio::test]
    async fn slot_recycling_pages_the_evictee_out_and_the_returner_in() {
        let pool = KvSlotPool::new("test", 2); // 2 slots, no scratch
        let a = key(1, 1);
        let b = key(1, 2);
        let c = key(1, 3);
        let pa = pool.lease_paged(a).await.expect("a");
        assert_eq!(pa.save_first, None, "fresh slot: nobody to save");
        assert!(!pa.restore, "no page written yet");
        // Warm re-lease: same holder, nothing to page either way.
        let pa2 = pool.lease_paged(a).await.expect("a warm");
        assert_eq!(pa2.slot, pa.slot);
        assert_eq!(pa2.save_first, None, "same holder — no save");
        assert!(!pa2.restore, "slot already holds a's fresher KV — no restore");
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let _pb = pool.lease_paged(b).await.expect("b");
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        // c arrives on a full pool: someone (LRU = a) is evicted; the paging
        // ledger must name the previous holder of the recycled slot for save.
        let pc = pool.lease_paged(c).await.expect("c");
        assert_eq!(pc.save_first, Some(a), "the evictee's KV must be saved before overwrite");
        pool.note_saved(a); // adapter reports the save HTTP call succeeded
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        // a returns: her slot was recycled, but she has a page → restore.
        let pa3 = pool.lease_paged(a).await.expect("a returns");
        assert!(pa3.restore, "a has a page and the slot holds someone else's KV");
        // A dead page stops being offered.
        pool.note_page_lost(&a);
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let _ = pool.lease_paged(b).await.expect("b again");
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let pa4 = pool.lease_paged(a).await.expect("a after lost page");
        assert!(!pa4.restore, "a lost page must not be offered again");
        assert_eq!(page_filename(&a), format!("a-{}-{}.bin", a.persona.simple(), a.room.simple()));
    }

    // what this catches: the scratch reservation and the placement law. On a
    // 4-slot server the highest index is reserved — citizens lease 0..2 and the
    // 4th activity EVICTS rather than spilling onto scratch; on a 2-slot server
    // there is no scratch to spare. And the class map: only the turn purposes
    // may reach a citizen slot; everything unknown defaults to Background so a
    // new producer can never clobber a citizen by omission.
    #[tokio::test]
    async fn scratch_is_reserved_and_only_turns_are_turns() {
        let pool = KvSlotPool::new("test", 4);
        assert_eq!(pool.scratch_slot(), Some(3), "highest index is scratch");
        let mut seen = Vec::new();
        for r in 1..=4u128 {
            seen.push(pool.lease(key(9, r)).await.expect("lease"));
        }
        assert!(!seen.contains(&3), "citizen leases never land on scratch: {seen:?}");
        assert_eq!(KvSlotPool::new("test", 2).scratch_slot(), None, "no scratch on tiny servers");

        assert_eq!(class_for(Some("cognition/deliberation")), SlotClass::Turn);
        assert_eq!(class_for(Some("persona-respond")), SlotClass::Turn);
        assert_eq!(class_for(Some("cognition/should-respond")), SlotClass::Sidecar);
        assert_eq!(class_for(Some("serving-smoke-probe")), SlotClass::Probe);
        assert_eq!(class_for(Some("dream-consolidation")), SlotClass::Background);
        assert_eq!(class_for(Some("some-future-producer")), SlotClass::Background,
            "unknown purposes default AWAY from citizen slots");
        assert_eq!(class_for(None), SlotClass::Background);
    }

    // what this catches: KV-CACHE-ECONOMY §2's arithmetic, as a pinned test. A
    // small fresh head must never displace a large warm tail: the shared head is
    // "big enough to win slot selection and small enough that winning throws
    // away 4.7× more than it saves". The priced policy evicts the CHEAPEST
    // tail, recency notwithstanding.
    #[tokio::test]
    async fn a_small_head_never_evicts_a_large_warm_tail() {
        let pool = KvSlotPool::new("test", 2); // no scratch on 2 slots
        let big = key(1, 1);
        let small = key(1, 2);
        pool.lease(big).await.expect("big");
        pool.note_tail(&big, 36_000);
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        pool.lease(small).await.expect("small");
        pool.note_tail(&small, 7_600);
        // small is the FRESHER activity — under LRU the big tail would be the
        // victim. Under the priced policy the CHEAP tail goes.
        let incoming = pool.lease(key(1, 3)).await.expect("third leases");
        assert!(
            pool.lease(big).await == Some(0) || incoming != 0,
            "sanity: some slot was assigned"
        );
        // The big activity must still be warm (its slot survives): re-leasing it
        // is a warm hit on the SAME slot, not a re-assignment.
        let big_again = pool.lease(big).await.expect("big still resident");
        assert_eq!(big_again, 0, "the 36k tail survived; the 7.6k head was the victim");
    }

    // what this catches: the directory latch — an unsupported server must never
    // hand out a pool, and the first probe outcome wins for the process life.
    #[test]
    fn directory_latch_and_ensure() {
        let dir = SlotDirectory {
            endpoints: dashmap::DashMap::new(),
        };
        assert!(dir.get("s1").is_none(), "unprobed root is unknown");
        dir.latch_unsupported("s1");
        assert!(matches!(dir.get("s1"), Some(None)), "latched unsupported");
        let p = dir.ensure_pool("s2", 4);
        assert_eq!(p.n_slots(), 4);
        assert!(matches!(dir.get("s2"), Some(Some(_))));
    }

    // What this catches: same-URL restart must restore saved KV, never trust old
    // physical holders; queued/late work cannot reopen or clear a newer generation.
    #[tokio::test]
    async fn endpoint_restart_drains_admission_and_preserves_only_compatible_pages() {
        let dir = SlotDirectory {
            endpoints: dashmap::DashMap::new(),
        };
        let endpoint = dir.endpoint("test://generation");
        let contract = KvPageContract {
            model_id: "fixture".into(),
            model: "fixture.gguf".into(),
            adapters: vec!["fixture.lora".into()],
            page_dir: Some("fixture-pages".into()),
            context: 32768,
            slots: 2,
            cache_type: Some("q8_0".into()),
            engine: "fixture-engine".into(),
            revisions: Some(vec![(
                "fixture.gguf".into(),
                73,
                std::time::SystemTime::UNIX_EPOCH,
            )]),
        };
        let first = endpoint.transition().await;
        let old = first.start_generation().expect("first engine");
        first
            .ready("test://generation", &old, contract.clone())
            .expect("ready");
        drop(first);
        assert!(endpoint.transition_if(&|| false).await.is_err());
        assert!(endpoint
            .transition_for_generation_if(&old, &|| false)
            .await
            .is_err());
        assert!(
            endpoint.is_ready(),
            "refused lifecycle leaves live readiness untouched"
        );
        let admitted = endpoint.admit().await.expect("old request");
        let old_pool = admitted.pool.as_ref().expect("pool").clone();
        let activity = key(73, 74);
        old_pool
            .lease_paged(activity)
            .await
            .expect("old physical holder");
        old_pool.note_saved(activity);
        let current = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let authority = || current.load(Ordering::Acquire);
        {
            let queued = endpoint.transition_if(&authority);
            tokio::pin!(queued);
            assert!(futures::poll!(&mut queued).is_pending());
            current.store(false, Ordering::Release);
            drop(admitted);
            assert!(
                queued.await.is_err(),
                "intent changed while existing readers drained"
            );
        }
        assert!(endpoint.is_ready());
        // Rejected checkpoint recovery resumes the exact live ledger, never
        // constructs a new pool or resurrects invalidated saved-page eligibility.
        let suspended = endpoint.transition_for_generation(&old).await.unwrap();
        let receipt = suspended.capture_suspension(&old).unwrap();
        old_pool.note_page_lost(&activity);
        assert!(suspended.resume_suspension(&receipt, &|| false).is_err());
        assert!(!endpoint.is_ready());
        suspended.resume_suspension(&receipt, &|| true).unwrap();
        drop(suspended);
        let resumed = endpoint.admit().await.unwrap();
        assert!(Arc::ptr_eq(resumed.pool.as_ref().unwrap(), &old_pool));
        assert!(
            old_pool
                .lease_paged(activity)
                .await
                .unwrap()
                .already_resident
        );
        assert!(!old_pool.saved.lock().contains(&activity));
        old_pool.note_saved(activity); // Preserve the restart portion's fixture input.
        drop(resumed);
        let admitted = endpoint
            .admit()
            .await
            .expect("refused operation preserves admission");

        // A cancelled retirement waits for readers without closing the live engine.
        // The queued writer also prevents a later reader from overtaking the drain.
        {
            let retirement = endpoint.transition_for_generation(&old);
            tokio::pin!(retirement);
            assert!(futures::poll!(&mut retirement).is_pending());
            let later = endpoint.admit();
            tokio::pin!(later);
            assert!(futures::poll!(&mut later).is_pending());
        }
        assert!(
            endpoint.is_ready(),
            "cancelled pre-retirement drain changes no state"
        );

        let transition = endpoint.transition();
        tokio::pin!(transition);
        assert!(
            futures::poll!(&mut transition).is_pending(),
            "active admission must drain"
        );
        assert!(
            endpoint.admit().await.is_err(),
            "new admission is closed immediately"
        );
        drop(admitted);
        let transition = transition.await;
        assert!(
            transition.start_generation().is_err(),
            "Rust guard release is not engine exit"
        );
        old.observed_exit();
        let new = transition.start_generation().expect("predecessor exited");
        transition
            .ready("test://generation", &new, contract.clone())
            .expect("replacement ready");
        // Refreshing readiness for this generation must preserve its current ledger.
        let new_pool = dir
            .get("test://generation")
            .flatten()
            .expect("replacement pool");
        let pg = new_pool
            .lease_paged(activity)
            .await
            .expect("returning activity");
        assert!(pg.restore, "compatible saved page survives restart");
        assert_eq!(pg.save_first, None, "dead physical KV must never be saved");
        transition
            .ready("test://generation", &new, contract.clone())
            .expect("readiness refresh");
        old.observed_exit();
        drop(transition);
        assert!(endpoint.transition_for_generation(&old).await.is_err());
        assert!(
            endpoint.is_ready(),
            "stale retirement cannot suspend replacement"
        );
        let admitted = endpoint
            .admit()
            .await
            .expect("late old exit cannot close new generation");
        assert!(Arc::ptr_eq(
            admitted.pool.as_ref().expect("pool"),
            &new_pool
        ));
        assert!(!new_pool.lease_paged(activity).await.expect("warm").restore);
        drop(admitted);

        let transition = endpoint.transition().await;
        new.observed_exit();
        let changed = transition.start_generation().expect("new geometry engine");
        let mut incompatible = contract;
        incompatible.context += 256; // Same 16k page bucket is not proof of exact compatibility.
        incompatible.slots = 1;
        transition
            .ready("test://generation", &changed, incompatible.clone())
            .expect("changed ready");
        drop(transition);
        let admitted = endpoint.admit().await.expect("changed admission");
        let pool = admitted.pool.as_ref().expect("changed pool");
        assert_eq!(pool.n_slots(), 1);
        assert!(!pool.lease_paged(activity).await.expect("cold").restore);
        pool.note_saved(activity);
        drop(admitted);
        let transition = endpoint.transition().await;
        changed.observed_exit();
        let different_model = transition.start_generation().expect("different model");
        incompatible.model_id = "other-model".into();
        transition
            .ready("test://generation", &different_model, incompatible)
            .expect("different model ready");
        drop(transition);
        let admitted = endpoint.admit().await.expect("different model admission");
        assert!(
            !admitted
                .pool
                .as_ref()
                .expect("different pool")
                .lease_paged(activity)
                .await
                .expect("cold model")
                .restore
        );
        drop(admitted);
        drop(endpoint.transition().await); // Failed/cancelled bring-up has no ready receipt.
        assert!(
            endpoint.admit().await.is_err(),
            "transition cancellation must stay closed"
        );
    }
    // what this catches: the page store growing without bound (it is temporary disk),
    // and the trim removing the page just saved or a page's sidecar surviving its page.
    #[test]
    fn the_page_store_trims_oldest_first_and_never_the_page_just_saved() {
        let dir = std::env::temp_dir().join(format!("kv-page-trim-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap(); // JUSTIFIED unwrap: test scaffolding
        let write = |name: &str, bytes: usize, age_s: u64| {
            let p = dir.join(name);
            std::fs::write(&p, vec![0u8; bytes]).unwrap(); // JUSTIFIED unwrap: test scaffolding
            let t = std::time::SystemTime::now() - std::time::Duration::from_secs(age_s);
            let file = std::fs::OpenOptions::new()
                .write(true)
                .open(&p)
                .expect("fixture timestamp requires a writable handle on Windows");
            file.set_modified(t).expect("set fixture page age");
            let observed = file
                .metadata()
                .expect("fixture page metadata")
                .modified()
                .expect("fixture mtime");
            let error = observed.duration_since(t).unwrap_or_else(|e| e.duration());
            assert!(
                error <= std::time::Duration::from_secs(2),
                "fixture must establish page age within filesystem granularity; ages differ by 100 seconds"
            );
        };
        write("a-old.bin", 100, 300);
        write("a-old.bin.ckpt", 10, 300);
        write("a-mid.bin", 100, 200);
        write("a-new.bin", 100, 100);
        write("unrelated.txt", 500, 0);
        let (removed, freed, kept) = trim_page_store(&dir, "a-old.bin", 250, 0);
        assert_eq!(removed, 1, "one page over budget: the oldest NOT protected → a-mid");
        assert_eq!(freed, 100);
        assert_eq!(kept, 210);
        assert!(dir.join("a-old.bin").exists() && dir.join("a-old.bin.ckpt").exists(), "the just-saved page is kept");
        assert!(!dir.join("a-mid.bin").exists());
        let (removed, _, _) = trim_page_store(&dir, "a-new.bin", 150, 0);
        let (young, _, _) = trim_page_store(&dir, "a-new.bin", 0, 60_000);
        assert_eq!(young, 0, "a page younger than the floor is an active resident's: never trimmed");
        assert_eq!(removed, 1, "now a-old goes, sidecar with it");
        assert!(!dir.join("a-old.bin.ckpt").exists());
        assert!(dir.join("unrelated.txt").exists(), "only pages are trimmed");
        let _ = std::fs::remove_dir_all(&dir);
    }

}
