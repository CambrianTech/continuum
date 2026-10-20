use super::*;

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
    pub(super) slot: u32,
    pub(super) free: Arc<Mutex<Vec<u32>>>,
    /// The activity's last-known prompt size in tokens — the COST basis for the
    /// priced eviction policy (cost_of_loss ∝ tail re-prefill). Updated at each
    /// pin from the request's own token estimate; an estimate is fine because
    /// eviction only needs slots COMPARABLE, not absolutely priced.
    pub(super) tail_tokens: AtomicU64,
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
pub(super) const RECENCY_TIEBREAK_MS: u64 = 600_000;

/// One token-cost step in the eviction score — must exceed the largest possible
/// recency bonus so a single token of tail always outweighs any freshness.
pub(super) const COST_STEP: i64 = 1 << 20;

pub struct KvSlotPool {
    pub(super) pool: PagedResourcePool<ActivityKey, Arc<KvSlotLease>>,
    pub(super) free: Arc<Mutex<Vec<u32>>>,
    pub(super) n_slots: u32,
    /// The reserved non-citizen slot (highest index) when the server has ≥3
    /// slots — where ALL non-Turn traffic lands, so it structurally cannot
    /// evict a citizen's warm tail. `None` on small servers (≤2 slots): there,
    /// non-Turn traffic uses `cache_prompt: false` and BORROWS a citizen slot
    /// through the pool ([`Self::transient_slot`]) with its resident saved first.
    /// Two slots used to keep an "unpinned fallback" instead — measured 2026-09-26
    /// on BigMama (card 4deaed09): the server placed that traffic by similarity
    /// onto a resident's slot and overwrote her page; Sahar reused 0 tokens on
    /// every turn while Kimi, on the other slot, reused 44,950.
    pub(super) scratch: Option<u32>,
    /// slot index → the activity whose KV last WARMED that slot. This is the
    /// paging ledger's "who is resident" half: when a lease hands a slot to a
    /// DIFFERENT activity, the previous holder's KV is still physically in the
    /// server slot and must be SAVED to its disk page before the new turn
    /// overwrites it. Tiny map (n_slots entries), cold path (one pin per turn).
    pub(super) holders: Mutex<std::collections::HashMap<u32, ActivityKey>>,
    /// Activities with a valid KV page on disk (written via
    /// `/slots/{id}?action=save` under the lane's `--slot-save-path`). The
    /// restore half of the ledger: a re-entering activity whose slot was
    /// recycled restores its page (at this node's measured switch cost) instead of
    /// re-prefilling (~35s at 22k — the 330× cliff the restore economy names).
    pub(super) saved: Mutex<std::collections::HashSet<ActivityKey>>,
    /// Physical-slot admission also excludes concurrent requests for the same key.
    pub(super) operations: Vec<Arc<tokio::sync::Semaphore>>,
    /// Signalled whenever a slot's operation permit is released: what a turn that found every
    /// slot pinned waits on before it leases again (card ff9ecfd8), so it never runs unpinned
    /// over a resident's KV. A wake signal, not a queue: the order stays the server's own.
    pub(super) released: Arc<tokio::sync::Notify>,
    /// Where [`Self::saved`] persists, set once when the engine is ready under a contract
    /// that has a page dir and a fingerprint ([`SavedLedger`]). Unset = in-memory only.
    pub(super) ledger: std::sync::OnceLock<SavedLedger>,
    /// Numbers each change of [`Self::saved`] for the ledger, so a write taken outside the
    /// lock never replaces a newer one ([`Self::ledger_snapshot`]).
    pub(super) ledger_generation: AtomicU64,
}

/// A slot's operation permit. Dropping it wakes turns waiting for a slot
/// ([`KvSlotPool::released`]); the holder's pin was already dropped (field order in
/// `TurnAdmission`), so a woken turn finds that lease evictable.
pub struct SlotPermit {
    pub(super) _permit: tokio::sync::OwnedSemaphorePermit,
    pub(super) released: Arc<tokio::sync::Notify>,
}

impl Drop for SlotPermit {
    fn drop(&mut self) {
        self.released.notify_waiters();
    }
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

pub(super) static PAGE_DIR: parking_lot::Mutex<Option<std::path::PathBuf>> = parking_lot::Mutex::new(None);

/// The spawn — the one place that knows the live geometry dir — registers it so
/// a save can trim the store it just wrote into.
pub fn note_page_dir(dir: &std::path::Path) {
    *PAGE_DIR.lock() = Some(dir.to_path_buf());
}

/// The one disk read in flight, and the dir it reads. ONE: every page dir is under one store on one
/// volume, so a second concurrent read only adds a second stuck thread when that volume stops
/// answering, and a plan whose geometry churns could otherwise start one per window (root on
/// #5060). The read is its own task: it lands its samples and clears this slot whether or not
/// anyone is still waiting on it.
pub(super) static SEEDING: parking_lot::Mutex<Option<(std::path::PathBuf, futures::future::Shared<futures::future::BoxFuture<'static, ()>>)>> =
    parking_lot::Mutex::new(None);

#[cfg(test)]
pub(super) static SEED_READS: parking_lot::Mutex<Vec<std::path::PathBuf>> = parking_lot::Mutex::new(Vec::new());

/// A restarted core starts with no state samples while the pages it would measure sit on disk
/// (IntelMac, 2026-10-11 02:46Z: derived 1,792 MiB beside states up to 865 MB). Seed the EXACT page
/// dir a sizing is about to read from its newest pages, before it reads: they are exactly the saves
/// a running core would have measured. Seeding at the spawn sized every fresh core's first lane from
/// the KV estimate, because the sizing for that spawn runs first (Kimi's node, 2026-10-15: core
/// started 12:15:15Z, launch derived 3,232 MiB 20 s later, and the re-home relaunched it at
/// 9,477 MiB; card 37eb1925).
///
/// The sample cache is the readiness authority: a dir it knows (measured, or read and empty) is not
/// read again, and a dir it evicted is unknown again. One read is in flight at a time ([`SEEDING`]);
/// while another dir's read is still running, this dir stays unknown and its sizing uses the
/// estimate, and the next ask reads it. The read runs on the blocking pool, never on a worker (root
/// on #5060), and each asker waits at most the disk reporters'
/// [`REPORTER_TIMEOUT`](crate::system_resources::disk_pressure::REPORTER_TIMEOUT): an unavailable
/// disk leaves one read in flight and the sizing on its estimate, and never holds serving. A read
/// that fails leaves the dir unknown (probed), so the next ask reads again.
pub async fn seed_state_samples(dir: &std::path::Path) {
    seed_state_samples_with(dir, |dir| newest_page_states(dir, STATE_SAMPLES)).await;
}

pub(super) async fn seed_state_samples_with(
    dir: &std::path::Path,
    read: impl FnOnce(&std::path::Path) -> Result<Vec<u64>, String> + Send + 'static,
) {
    use crate::runtime::daemon::{guarded, Guarded};
    let reading = {
        let mut seeding = SEEDING.lock();
        if state_dir_known_in(&RECENT_STATE_BYTES.lock(), dir) {
            return;
        }
        match &*seeding {
            Some((reading, read)) if reading == dir => read.clone(),
            Some((reading, _)) => {
                crate::probe!(
                    class = "inference.kv_page.seed_busy",
                    dir = %dir.display(),
                    reading = %reading.display(),
                    "another page dir is still being read; this sizing uses the estimate, the next ask reads this one",
                );
                return;
            }
            None => {
                let landed = spawn_state_read(dir.to_path_buf(), read);
                *seeding = Some((dir.to_path_buf(), landed.clone()));
                landed
            }
        }
    };
    if let Guarded::TimedOut = guarded(crate::system_resources::disk_pressure::REPORTER_TIMEOUT, reading).await {
        crate::probe!(
            class = "inference.kv_page.seed_pending",
            dir = %dir.display(),
            "the saved pages are still being read; this sizing uses the estimate, the next one the pages",
        );
    }
}

/// Start the read of `dir` as its own task and return its completion for askers to wait on. The
/// task lands the samples (a live sample already there outranks them), THEN clears [`SEEDING`], so
/// no asker between the two starts a second read; it never holds both locks.
pub(super) fn spawn_state_read(
    dir: std::path::PathBuf,
    read: impl FnOnce(&std::path::Path) -> Result<Vec<u64>, String> + Send + 'static,
) -> futures::future::Shared<futures::future::BoxFuture<'static, ()>> {
    use futures::FutureExt;
    #[cfg(test)]
    SEED_READS.lock().push(dir.clone());
    let released = dir.clone();
    let task = tokio::spawn(async move {
        let path = dir.clone();
        match tokio::task::spawn_blocking(move || read(&path))
            .await
            .map_err(|error| error.to_string())
            .and_then(|states| states)
        {
            Ok(states) => seed_dir_in(&mut RECENT_STATE_BYTES.lock(), &dir, states),
            Err(error) => crate::probe!(
                class = "inference.kv_page.seed_failed",
                dir = %dir.display(),
                error = %error,
                "reading the saved pages on disk failed; left unmeasured, the next ask reads again",
            ),
        }
        let mut seeding = SEEDING.lock();
        if seeding.as_ref().is_some_and(|(reading, _)| reading == &dir) {
            *seeding = None;
        }
    });
    async move {
        // The read's own failures (the reader, the blocking pool) release the slot inside the
        // task. Only a panic of the task itself skips that, and then the slot is released here,
        // when an asker next waits on it: waiter-dependent, and with no reachable panic in the
        // task body, not relied on (root on #5060).
        if task.await.is_err() {
            let mut seeding = SEEDING.lock();
            if seeding.as_ref().is_some_and(|(reading, _)| reading == &released) {
                *seeding = None;
            }
        }
    }
    .boxed()
    .shared()
}

/// The engine's sidecars beside each page, saved and restored with it: the context checkpoints
/// (`.ckpt`) and the adapter vector the state was computed under (`.lora`, card e5eaf9f4). ONE
/// list, so every scan sizes a page whole and every trim removes it whole.
pub(super) const PAGE_SIDECARS: [&str; 2] = [".ckpt", ".lora"];

/// The bytes of a page's sidecars (`.ckpt` + `.lora`) in the live page dir, or `None` when no dir
/// is registered or none exists. A save or restore reads and writes them with the page, and the
/// engine's own `n_read`/`n_written` count only the `.bin`, so a probe that times the whole switch
/// names them separately (BigMama on #4957: ~1.2-1.6 GB of checkpoints on Kimi's page).
pub fn page_sidecar_bytes(page: &str) -> Option<u64> {
    let dir = PAGE_DIR.lock().clone()?;
    let sizes: Vec<u64> = PAGE_SIDECARS
        .iter()
        .filter_map(|s| std::fs::metadata(dir.join(format!("{page}{s}"))).ok().map(|m| m.len()))
        .collect();
    (!sizes.is_empty()).then(|| sizes.iter().sum())
}

/// Every page in `dir` (`a-*.bin`), as (page name -> (the page file's mtime, bytes of the page
/// and its sidecars)). Reads the dir once. A dir that does not exist holds no pages, and an entry
/// that vanishes between the listing and its stat (a trim racing this read) is skipped; any other
/// failure is an error, so no reader mistakes an unreadable store for an empty one (root on #5060).
pub(super) fn pages_in(dir: &std::path::Path) -> Result<std::collections::HashMap<String, (std::time::SystemTime, u64)>, String> {
    let mut pages = std::collections::HashMap::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(pages),
        Err(error) => return Err(format!("{}: {error}", dir.display())),
    };
    for e in entries {
        let e = e.map_err(|error| format!("{}: {error}", dir.display()))?;
        let name = e.file_name().to_string_lossy().into_owned();
        let md = match e.metadata() {
            Ok(md) => md,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("{}: {error}", e.path().display())),
        };
        let sidecar = PAGE_SIDECARS.iter().find_map(|s| name.strip_suffix(s));
        let page = sidecar.unwrap_or(&name).to_string(); // unwrap_or: a name with no sidecar suffix IS the page name
        if !md.is_file() || !(page.starts_with("a-") && page.ends_with(".bin")) {
            continue;
        }
        let entry = pages.entry(page).or_insert((std::time::SystemTime::UNIX_EPOCH, 0));
        entry.1 += md.len();
        if sidecar.is_none() {
            entry.0 = md.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH); // unwrap_or: an unreadable mtime sorts oldest, the safe side for an eviction candidate and the least likely sample
        }
    }
    Ok(pages)
}

/// The sizes of the newest `n` saved states in `dir` (each page with its sidecars), oldest
/// first, or why the dir could not be read ([`pages_in`]).
pub(super) fn newest_page_states(dir: &std::path::Path, n: usize) -> Result<Vec<u64>, String> {
    let mut by_age: Vec<(std::time::SystemTime, u64)> = pages_in(dir)?.into_values().filter(|(_, b)| *b > 0).collect();
    by_age.sort_by_key(|(t, _)| *t);
    let skip = by_age.len().saturating_sub(n);
    Ok(by_age.into_iter().skip(skip).map(|(_, b)| b).collect())
}

/// After a successful save of `just_saved` (a page filename), keep the live
/// geometry dir under [`KV_PAGE_STORE_MAX_BYTES`]. Never touches the page just
/// written. Cheap: one `read_dir` per save, a few files.
pub fn trim_page_store_after_save(just_saved: &str) {
    let Some(dir) = PAGE_DIR.lock().clone() else {
        return;
    };
    // The page just written IS one mind's whole saved state (tokens, KV, recurrent state, and
    // the context checkpoints in its sidecar): a measurement of what the host prompt cache
    // must hold per mind, which the KV estimate alone misses on a hybrid model.
    let page_bytes = |name: &str| std::fs::metadata(dir.join(name)).map(|m| m.len()).unwrap_or(0); // unwrap_or: an absent sidecar adds nothing
    let state = PAGE_SIDECARS.iter().fold(page_bytes(just_saved), |sum, s| sum.saturating_add(page_bytes(&format!("{just_saved}{s}"))));
    note_state_bytes(&dir, state);
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

/// How many recent saved-state sizes [`measured_state_bytes`] remembers per page dir, and how
/// many page dirs it remembers (the least recently written is dropped).
pub(super) const STATE_SAMPLES: usize = 16;

pub(super) const STATE_DIRS: usize = 8;

/// Recent saved-state sizes, keyed by the PAGE DIR they were written to: a page dir is one
/// model at one geometry ([`crate::inference::llama_server::kv_page_dir`]), so a second model's
/// lane on the node is never sized by another model's states (Fable on #4930).
pub(super) type StateSamples = std::collections::VecDeque<(std::path::PathBuf, std::collections::VecDeque<u64>)>;

pub(super) static RECENT_STATE_BYTES: parking_lot::Mutex<StateSamples> = parking_lot::Mutex::new(std::collections::VecDeque::new());

pub(crate) fn note_state_bytes(dir: &std::path::Path, bytes: u64) {
    if bytes == 0 {
        return;
    }
    note_state_bytes_in(&mut RECENT_STATE_BYTES.lock(), dir, bytes);
}

/// PURE: record `bytes` under `dir`, bounded by [`STATE_SAMPLES`] per dir and [`STATE_DIRS`] dirs.
pub(super) fn note_state_bytes_in(samples: &mut StateSamples, dir: &std::path::Path, bytes: u64) {
    let mut recent = match samples.iter().position(|(d, _)| d == dir) {
        Some(i) => samples.remove(i).map(|(_, r)| r).unwrap_or_default(), // unwrap_or_default: position just found it, so this is always Some
        None => std::collections::VecDeque::new(),
    };
    recent.push_back(bytes);
    while recent.len() > STATE_SAMPLES {
        recent.pop_front();
    }
    samples.push_back((dir.to_path_buf(), recent));
    while samples.len() > STATE_DIRS {
        samples.pop_front();
    }
}

/// PURE: whether `dir` has an entry yet, measured or seeded empty.
pub(super) fn state_dir_known_in(samples: &StateSamples, dir: &std::path::Path) -> bool {
    samples.iter().any(|(d, _)| d == dir)
}

/// PURE: give an unknown `dir` its disk samples (possibly none), within [`STATE_DIRS`]. A dir
/// already known keeps what it has.
pub(super) fn seed_dir_in(samples: &mut StateSamples, dir: &std::path::Path, seeded: Vec<u64>) {
    if state_dir_known_in(samples, dir) {
        return;
    }
    let skip = seeded.len().saturating_sub(STATE_SAMPLES);
    samples.push_back((dir.to_path_buf(), seeded.into_iter().skip(skip).collect()));
    while samples.len() > STATE_DIRS {
        samples.pop_front();
    }
}

/// PURE: the largest recent state recorded under `dir`.
pub(super) fn measured_state_bytes_in(samples: &StateSamples, dir: &std::path::Path) -> Option<u64> {
    samples.iter().find(|(d, _)| d == dir).and_then(|(_, r)| r.iter().copied().max())
}

/// The largest of the last [`STATE_SAMPLES`] saved states written to `page_dir` (page +
/// checkpoint sidecar), or `None` before the first save there. What one mind's state really costs the host
/// prompt cache: on the 5090, 2026-10-11, the engine reported states of 4.3-5.8 GB for the
/// hybrid 27B at a 68k window while the cache was sized from the KV estimate at 4,288 MiB,
/// so 104 states were skipped and 220 evicted, and two minds on one slot found nothing.
/// The largest, not a median: the engine refuses any state over the limit outright.
pub fn measured_state_bytes(page_dir: &std::path::Path) -> Option<u64> {
    measured_state_bytes_in(&RECENT_STATE_BYTES.lock(), page_dir)
}

/// Pure trim: pages (`a-*.bin`) with their sidecars ([`PAGE_SIDECARS`]), oldest mtime first,
/// until the dir is within `max_bytes`. Returns (pages removed, bytes freed,
/// bytes kept). The page named `keep` is never removed; neither is any page
/// younger than `min_age_ms` (an active resident's working set).
pub fn trim_page_store(
    dir: &std::path::Path,
    keep: &str,
    max_bytes: u64,
    min_age_ms: u64,
) -> (usize, u64, u64) {
    // page name → (mtime, bytes incl. sidecars). An unreadable store trims nothing: deleting on a
    // partial listing could remove what the read did not see.
    let pages = match pages_in(dir) {
        Ok(pages) => pages,
        Err(error) => {
            crate::probe!(
                class = "inference.kv_page.store_unreadable",
                dir = %dir.display(),
                error = %error,
                "the KV page store could not be read; nothing trimmed",
            );
            return (0, 0, 0);
        }
    };
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
        for sidecar in PAGE_SIDECARS {
            let _ = std::fs::remove_file(dir.join(format!("{name}{sidecar}")));
        }
        removed += 1;
        freed += bytes;
        total = total.saturating_sub(bytes);
    }
    (removed, freed, total)
}
