//! Turn admission — everything a turn must ACQUIRE before it can generate, as ONE
//! RAII guard. Extracted from `openai_adapter::generate_stream`, where it lived
//! inline and tangled with body-building across two scopes (the `if
//! llamacpp_sampling_extensions` block vs. the function body). That tangle is what
//! made the permit/pin lifetime hard to get right by hand — the fix is to make the
//! lifetime a TYPE: the caller binds one [`TurnAdmission`] at function scope and it
//! holds the permit + slot pin for the whole generation, releasing both on drop.
//!
//! ## Event-driven, no timeout (the restore-into-busy-slot fix)
//!
//! The order is load-bearing:
//! 1. **Permit first.** Take the concurrency permit (`Semaphore(lanes)`) BEFORE
//!    leasing a slot. Holding a permit means a lane is genuinely free — an
//!    event-driven wait that wakes when another turn releases, never a timer.
//! 2. **Pin the leased slot** (synchronously, before any save/restore await) so
//!    priced eviction skips it. A concurrent returner can then never lease — and
//!    restore INTO — a slot that is still decoding this turn's KV.
//! 3. **Save/restore** now targets a guaranteed-free, non-decoding slot, so the
//!    restore lands at this node's measured switch cost instead of deferring behind a live decode.
//!
//! Before this, the slot was leased before the permit and never pinned, so a
//! returner grabbed a still-decoding slot and its restore timed out — measured
//! 27/27 `status=0`, worked around with a 90s wait (55/67). Permit-first + pin
//! removes the failure at the source; the 90s wait is gone (see [`kv_page_action`]).

use std::sync::Arc;

use serde_json::json;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::inference::llama_server::EngineProbe;
use crate::inference::slots::{KvSlotPool, MindTurn, PageKey, SlotPermit, SlotPin};

mod paging_lifetime;

/// How this turn's KV reached its slot: the evidence the restore economy is judged by
/// (`inference.restored_turn`). A restore the engine refused is a fast-path MISS, recorded
/// as one, never folded into "nothing to restore" (card f3ba3b56).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageIn {
    /// No page was due: the page prefills by design (its first turn, or no pool).
    None,
    /// Its KV was already resident in the leased slot.
    Resident,
    /// Restored from its saved page.
    Restored,
    /// A restore was due and the engine refused it, so the turn re-prefills in full.
    RestoreRejected,
}

/// What a turn holds for the whole generation. Dropping it releases the concurrency
/// permit and the slot pin (RAII). An incomplete call also invalidates local KV
/// attribution; dropping this guard is not an acknowledgement of backend stop.
pub struct TurnAdmission {
    /// The slot to pin the request to (`id_slot`), or `None` to stay unpinned
    /// (traffic using separate scratch, or a Turn whose page could not be leased).
    slot: Option<u32>,
    /// The slot pin — held so eviction cannot reassign this turn's slot mid-decode.
    _pin: Option<SlotPin>,
    /// Serializes same-slot operations even when distinct adapters admit the key.
    _slot_permit: Option<SlotPermit>,
    /// The pool's slot-release signal, held once this admission pins a slot: its drop raises
    /// it after the pin is gone (see `Drop`), so a turn waiting for a slot wakes.
    released: Option<Arc<tokio::sync::Notify>>,
    page_confirmed: bool,
    page_in: PageIn,
    saved_evictee: bool,
    paging_in_flight: bool,
    /// Released after the pin so the next permit holder can lease the free slot.
    /// `None` only for [`admit_transient`], which holds its slot's operation permit
    /// instead of a lane (it serves no page, so it takes no lane from one).
    _permit: Option<OwnedSemaphorePermit>,
    /// A lease is provisional until the adapter observes successful generation.
    /// Cancellation during paging or generation must not save foreign KV later.
    uncommitted: Option<(Arc<KvSlotPool>, PageKey, u32)>,
    _endpoint: crate::inference::slots::EndpointAdmission,
    scratch: Option<u32>,
    /// The mind's act gate ([`crate::inference::slots::MindTurn`]), held for the whole
    /// generation. Declared LAST so it drops after the pin and permits: the mind's next
    /// act, woken by it, finds her slot already released.
    _mind: Option<MindTurn>,
}

impl TurnAdmission {
    /// The leased slot this turn should pin `id_slot` to, if any.
    pub fn slot(&self) -> Option<u32> {
        self.slot
    }

    pub(crate) fn scratch_slot(&self) -> Option<u32> {
        self.scratch
    }

    /// How this turn's KV reached its slot ([`PageIn`]).
    pub(crate) fn page_in(&self) -> PageIn {
        self.page_in
    }

    async fn page_action(
        &mut self,
        client: &reqwest::Client,
        root: &str,
        slot: u32,
        key: &PageKey,
        action: &str,
    ) -> Result<bool, String> {
        self.paging_in_flight = true;
        match kv_page_action(client, root, slot, key, action).await {
            PageOutcome::Completed => {
                self.paging_in_flight = false;
                Ok(true)
            }
            PageOutcome::Rejected => {
                self.paging_in_flight = false;
                Ok(false)
            }
            PageOutcome::Uncertain => Err("KV paging completion unverified; endpoint quarantined until verified engine replacement".into()),
        }
    }

    /// The adapter received a complete successful generation in this slot.
    pub(crate) fn generation_completed(&mut self) {
        self.uncommitted = None;
    }

    /// Attribute complete provider usage before releasing this engine generation.
    pub(crate) fn record_served_prompt(
        &self,
        endpoint: &str,
        response_model: Option<&str>,
        tokens: u32,
    ) {
        if let Some(geometry) = response_model.and_then(|model| self._endpoint.served_geometry(model)) {
            crate::inference::resource_admission::note_served_prompt(endpoint, geometry, tokens);
        }
    }
}

impl Drop for TurnAdmission {
    fn drop(&mut self) {
        if self.paging_in_flight {
            self._endpoint.quarantine_paging();
        }
        if let Some((pool, key, slot)) = self.uncommitted.take() {
            pool.forget_resident(slot, key);
        }
        // Unpin BEFORE any release is announced, so a woken turn finds this lease evictable.
        // A pin dropped with no slot permit (cancelled while awaiting it, or acquire_slot
        // failed) has no SlotPermit drop to announce it, so it announces here (Fable on #4515).
        let pinned_without_permit = self._pin.is_some() && self._slot_permit.is_none();
        drop(self._pin.take());
        if pinned_without_permit {
            if let Some(released) = &self.released {
                released.notify_waiters();
            }
        }
    }
}

/// Independent admission budgets: readiness waits consume `readiness`; every wait
/// consumes the request's absolute deadline when it has one.
pub struct AdmissionPatience {
    readiness: std::time::Duration,
    deadline: Option<tokio::time::Instant>,
}

impl AdmissionPatience {
    /// A generation occupancy estimate is not an admission expiry. Healthy queue
    /// and paging work stays owned until admitted or the requester cancels.
    pub fn for_turn() -> Self {
        Self {
            readiness: crate::inference::slots::EndpointSlots::TURN_READINESS_PATIENCE,
            deadline: None,
        }
    }

    /// An explicit caller expiry, already expressed on the monotonic clock.
    /// Do not derive this from predicted generation occupancy.
    pub fn until(deadline: tokio::time::Instant) -> Self {
        Self { deadline: Some(deadline), ..Self::for_turn() }
    }
}

impl From<std::time::Duration> for AdmissionPatience {
    fn from(readiness: std::time::Duration) -> Self {
        Self { readiness, deadline: None }
    }
}

/// Admit a turn: wait for the mind's running act (a keyed turn only; see
/// `slots::mind_gate`), acquire the concurrency permit (always), then — for a turn with a
/// [`PageKey`] and a live slot pool — lease + pin its slot and page its KV (save the
/// evictee, restore this mind's page). Returns the guard the
/// caller must hold across the generation.
///
/// `key`/`pool` are `None` for non-Turn traffic (or a cloud provider with no slots):
/// the caller still gets the permit and places the request on scratch (or unpinned)
/// itself. On a single-slot server the pool saves and detaches the resident before
/// transient traffic borrows that slot; `slot()` then returns its index.
///
/// `patience` is how long the turn holds through a CLOSED endpoint (an engine still
/// loading, a transition in flight) before it is refused — the caller's bound, since
/// only the caller knows what its request can afford: the persona adapter passes
/// `AdmissionPatience::for_turn()`. Readiness time is cumulative across retries;
/// healthy slot and adapter waits end only on cancellation or an explicit caller
/// expiry supplied with `AdmissionPatience::until`.
/// A test that asserts immediate readiness refusal passes zero. See `EndpointSlots::admit_when_ready`
/// for the boot window that cost Kimi 16 turns.
pub async fn admit_turn(
    concurrency: &Arc<Semaphore>,
    key: Option<PageKey>,
    pool: Option<Arc<KvSlotPool>>,
    client: &reqwest::Client,
    root: &str,
    approx_tokens: u64,
    patience: impl Into<AdmissionPatience>,
) -> Result<TurnAdmission, String> {
    let patience = patience.into();
    let admission = admit(concurrency, key, pool, client, root, approx_tokens, false, patience.readiness);
    match patience.deadline {
        Some(deadline) => {
            if deadline <= tokio::time::Instant::now() {
                return Err("turn deadline elapsed before admission".into());
            }
            tokio::time::timeout_at(deadline, admission).await
                .map_err(|_| "turn deadline elapsed during admission".to_owned())?
        }
        None => admission.await,
    }
}

/// Best-effort warm-ahead uses the same paging and provisional-attribution owner
/// as a turn. No generation follows, so only proven physical warmth is committed.
pub(crate) async fn warm_ahead(
    concurrency: &Arc<Semaphore>,
    key: PageKey,
    pool: Arc<KvSlotPool>,
    client: &reqwest::Client,
    root: &str,
) -> Result<(), String> {
    let started = std::time::Instant::now();
    // Best-effort: a mind mid-act already has her KV in a slot, and her act queued behind
    // it must not wait on this; held until the warm-ahead's paging is done.
    let Some(_mind) = MindTurn::try_begin(key) else {
        continuum_base::probe!(
            class = "inference.kv_warm_ahead.skipped",
            page = %key,
            reason = "mind_mid_act",
            "warm-ahead skipped: this mind is mid-act, so her KV is already in a slot"
        );
        return Ok(());
    };
    // Best-effort: a closed endpoint is an instant no, never a held permit.
    let mut admission = admit(concurrency, Some(key), Some(pool), client, root, 0, true, std::time::Duration::ZERO).await?;
    if admission.page_confirmed {
        admission.uncommitted = None;
    }
    if let Some(slot) = admission.slot {
        continuum_base::probe!(
            class = "inference.kv_warm_ahead",
            page = %key,
            slot = slot as u64,
            saved_evictee = admission.saved_evictee,
            restored = admission.page_in == PageIn::Restored,
            confirmed = admission.page_confirmed,
            ms = started.elapsed().as_millis() as u64,
            "warm-ahead completed through turn paging admission",
        );
    }
    Ok(())
}

async fn admit(
    concurrency: &Arc<Semaphore>,
    key: Option<PageKey>,
    pool: Option<Arc<KvSlotPool>>,
    client: &reqwest::Client,
    root: &str,
    approx_tokens: u64,
    warm_only: bool,
    patience: std::time::Duration,
) -> Result<TurnAdmission, String> {
    let started = tokio::time::Instant::now();
    let mut readiness_left = patience;
    let mut waited = false;
    // 0. ONE ACT AT A TIME PER MIND — queued here, before the lane, holding nothing
    //    (`slots::mind_gate`). Warm-ahead took its gate itself, without waiting.
    let mut mind = match key {
        Some(k) if !warm_only => Some(MindTurn::begin(k).await?),
        _ => None,
    };
    'admit: loop {
        // 1. PERMIT FIRST — event-driven wait for a free lane. Cannot fail: the
        //    semaphore is never closed over the adapter's lifetime.
        let _permit = concurrency
            .clone()
            .acquire_owned()
            .await
            .expect("adapter semaphore never closed");  // expect: the semaphore lives as long as the adapter, never closed

        // 2. THE ENDPOINT — a turn holds through a closed one (an engine still loading,
        //    a transition in flight) for the caller's `patience` before it is refused;
        //    zero is the instant answer. See `EndpointSlots::admit_when_ready` for the
        //    boot window that cost Kimi 16 turns.
        let readiness_started = tokio::time::Instant::now();
        let endpoint = crate::inference::slots::directory()
            .endpoint(root)
            .admit_when_ready(readiness_left)
            .await?;
        readiness_left = readiness_left.saturating_sub(readiness_started.elapsed());
        // Discovery may have finished before an engine transition. Use the pool
        // owned by the admitted generation, never that stale discovery result.
        let pool = endpoint.pool.clone().or_else(|| pool.clone());
        let mut admission = TurnAdmission {
            slot: None,
            _pin: None,
            _slot_permit: None,
            released: None,
            page_confirmed: false,
            page_in: PageIn::None,
            saved_evictee: false,
            paging_in_flight: false,
            _permit: Some(_permit),
            uncommitted: None,
            scratch: pool.as_ref().and_then(|pool| pool.scratch_slot()),
            _endpoint: endpoint,
            _mind: None,
        };

        if warm_only
            && pool
                .as_ref()
                .is_some_and(|pool| pool.scratch_slot().is_none())
        {
            return Ok(admission);
        }
        if let (Some(k), Some(pool)) = (key, pool.as_ref()) {
            // A TURN NEVER RUNS UNPINNED OVER A RESIDENT (card ff9ecfd8). When every slot is pinned
            // by an in-flight turn, `lease` frees nothing, and a pin can also lose a race to an
            // eviction between lease and pin. Both used to return this admission with no slot: the
            // server then placed the turn on whichever slot freed and overwrote a resident's KV with
            // no save (8 of 13 contended leases on the M5, 19 of 36 on the IntelMac, 2026-09-28).
            // The server cannot run the turn before a slot frees anyway, so the turn waits for a
            // slot release and leases again; the evictee is then saved by the paging plan below.
            // Warm-ahead is best-effort and still gives up at once.
            let pinned = loop {
                let released = pool.released();
                let notified = released.notified();
                tokio::pin!(notified);
                // Registered BEFORE the check, so a release between the check and the wait is
                // never missed.
                notified.as_mut().enable();
                if pool.lease(k).await.is_some() {
                    // Pin before waiting for the physical-slot permit: a same-page
                    // returner must await the prior owner without permitting eviction.
                    if let Some(pinned) = pool.pin_slot(&k) {
                        break Some(pinned);
                    }
                    if warm_only {
                        break None;
                    }
                    // Leased, then evicted before the pin: the pool CHANGED rather than filled,
                    // and no release may ever be announced for it (the evicting worker can cancel
                    // before it pins), so retry now instead of waiting (Codex on #4515).
                    tokio::task::yield_now().await;
                    continue;
                }
                if warm_only {
                    break None;
                }
                waited = true;
                // Never hold a lane while waiting for a slot (Fable on #4515): the lane semaphore
                // is sized to every server slot, scratch included, so a held permit would block
                // traffic that could run now, and a slot holder's nested call could need it.
                // Release the endpoint reader too: keeping it across permit reacquisition
                // cycles with a queued lifecycle writer and a later adapter admission.
                // No pin or provisional page exists yet. Keep the registered notification,
                // then rediscover the pool under the next admitted generation.
                drop(admission);
                notified.await;
                continue 'admit;
            };
            if waited {
                continuum_base::probe!(
                    class = "inference.slot_affinity.waited",
                    page = %k,
                    ms = started.elapsed().as_millis() as u64,
                    "every slot was pinned: the turn waited for a release instead of running unpinned over a resident"
                );
            }
            if let Some((slot, pin)) = pinned {
                admission._pin = Some(pin);
                admission.released = Some(pool.released());
                admission._slot_permit = Some(pool.acquire_slot(slot).await?);
                admission._endpoint.check_ready()?;
                // Before the plan moves the slot's attribution: once a stop has closed page IO,
                // nothing pages a resident out (card 47151d36, `slots::page_io`).
                let page_io = crate::inference::slots::admit_page_io(crate::inference::slots::page_io::gate())
                    .map_err(|closed| closed.to_string())?;
                let pg = pool.plan_paging(slot, k);
                admission.page_confirmed = pg.already_resident;
                if pg.already_resident {
                    admission.page_in = PageIn::Resident;
                }
                admission.uncommitted = Some((Arc::clone(pool), k, pg.slot));
                // Taken on every admission, so only the first turn after a relaunch is blamed on it.
                let discarded = admission._endpoint.take_relaunch_discard(&k);
                if !pg.already_resident && !pg.restore {
                    // Every fast-path miss is loud: this turn prefills from its first token.
                    // With no page on disk that is a first visit; a page on disk the ledger does
                    // not confirm (lost before a save finished, or a ledger that did not load)
                    // is the defect class, never a quiet slower path.
                    // A relaunch that discarded this resident unsaved is the cause, whatever
                    // is on disk (card 6b28a445); otherwise the page store answers.
                    let on_disk = pool.page_on_disk(&k);
                    continuum_base::probe!(
                        class = "inference.kv_page.cold_turn",
                        page = %k,
                        slot = pg.slot as u64,
                        warm_only,
                        cause = match (discarded, on_disk) {
                            (Some(_), _) => "discarded_by_relaunch",
                            (None, Some(true)) => "page_on_disk_unconfirmed",
                            (None, Some(false)) => "no_page",
                            (None, None) => "no_page_dir",
                        },
                        discard_reason = discarded.map_or("", crate::inference::slots::DiscardReason::as_str),
                        "this page's slot holds none of its KV and no restorable page exists: it prefills in full"
                    );
                }

                if pg.save_first.is_some() || pg.restore {
                    admission._mind = mind.take();
                    let pool = Arc::clone(pool);
                    let client = client.clone();
                    let paging_root = root.to_owned();
                    admission = admission.retain_paging(root, move |mut admission, interested| async move {
                        // Held until this task's ledger updates: the stop waits for it.
                        let _page_io = page_io;
                        // 3. The context switch onto a now-free slot: page the evictee out,
                        //    page this activity in. Lands immediately (no defer) because the slot
                        //    is not decoding.
                        if let Some(prev) = pg.save_first {
                            // A failed or cancelled save may have replaced an older file.
                            // Only a completed save can make this page restorable again.
                            pool.note_page_lost(&prev);
                            if admission
                                .page_action(&client, &paging_root, pg.slot, &prev, "save")
                                .await?
                            {
                                pool.note_saved(prev);
                                admission.saved_evictee = true;
                            }
                        }
                        if pg.restore && interested.load(std::sync::atomic::Ordering::Acquire) {
                            let restored = admission
                                .page_action(&client, &paging_root, pg.slot, &k, "restore")
                                .await?;
                            admission.page_confirmed = restored;
                            admission.page_in = if restored {
                                PageIn::Restored
                            } else {
                                PageIn::RestoreRejected
                            };
                            if !restored {
                                pool.note_page_lost(&k);
                                // Reported HERE, at the decision, so a refused restore counts even when
                                // the turn is then cancelled, fails, or reports no timing (Astra on #5190).
                                // The same class and outcome the deploy self-check reads; the completion
                                // probe reports only restores that happened, so nothing counts twice.
                                continuum_base::probe!(
                                    class = "inference.restored_turn",
                                    page = %k,
                                    slot = pg.slot as u64,
                                    outcome = "restore_rejected",
                                    "the engine refused this turn's KV restore: it re-prefills in full"
                                );
                            }
                        }
                        Ok(admission)
                    }).await?;
                }
                // Price basis for the eviction policy (B5): this page's current
                // prompt size — comparable across slots, which is all eviction needs.
                if !warm_only {
                    pool.note_tail(&k, approx_tokens);
                }
                admission.slot = Some(pg.slot);
            }
        } else if let Some(pool) = pool.as_ref().filter(|pool| pool.scratch_slot().is_none()) {
            // A server with no scratch slot (≤2 lanes): background/anonymous traffic
            // borrows a citizen slot THROUGH the pool — the cheapest resident is saved
            // and detached first — and carries that slot pinned, never left to the
            // server's own placement. The two-lane case used to fall through here
            // unpinned (card 4deaed09: the server put it on a resident's slot by
            // similarity and overwrote her page every turn). Remove attribution
            // BEFORE the await so cancellation cannot leave the next page treating
            // transient KV as its own warm tail.
            let slot = pool.transient_slot();
            admission = admission
                .borrow_slot_for_transient(pool, slot, client, root, crate::inference::slots::page_io::gate())
                .await?;
        } else if let (Some(pool), Some(scratch)) = (pool.as_ref(), admission.scratch) {
            // Anonymous traffic lands on the SCRATCH slot, and takes its operation permit
            // like every other slot user. Before this, every non-turn call wrote the
            // scratch KV with no owner, so a holder of that slot (the deploy self-check's
            // cache probe, #4392) was overwritten between its own requests and read a warm
            // restore as cold: the M5 self-check reported `no_reuse` on 2026-09-26 03:01Z
            // while the same round trip on an owned slot measured 658/662 reused. The
            // server already runs one request per slot at a time, so this moves the queue
            // into the owner, where it is visible, rather than adding any.
            admission._slot_permit = Some(pool.acquire_slot(scratch).await?);
            admission._endpoint.check_ready()?;
        }

        if admission._mind.is_none() {
            admission._mind = mind.take();
        }
        return Ok(admission);
    }
}

impl TurnAdmission {
    /// Borrow physical `slot` for traffic that serves no page: wait for the slot's
    /// operation permit (event-driven, behind any live decode), detach its resident and
    /// SAVE the resident's page so its warmth survives, and leave the slot unattributed
    /// so no page later mistakes this traffic's KV for its own.
    ///
    /// The `page_io` permit is taken before the resident is detached, as on the keyed
    /// path: once a stop has closed page IO, a borrow neither detaches a resident nor
    /// starts a save, and a save it started holds the stop until its ledger update.
    async fn borrow_slot_for_transient(
        mut self,
        pool: &Arc<KvSlotPool>,
        slot: u32,
        client: &reqwest::Client,
        root: &str,
        page_io: &'static crate::runtime::admission_gate::AdmissionGate,
    ) -> Result<Self, String> {
        self._slot_permit = Some(pool.acquire_slot(slot).await?);
        self._endpoint.check_ready()?;
        let page_io = crate::inference::slots::admit_page_io(page_io).map_err(|closed| closed.to_string())?;
        let (previous, pin) = pool.take_resident(slot);
        self._pin = pin;
        if let Some(previous) = previous {
            let pool = pool.clone();
            let client = client.clone();
            let paging_root = root.to_owned();
            self = self.retain_paging(root, move |mut admission, _interested| async move {
                // Held until this save's ledger update: the stop waits for it.
                let _page_io = page_io;
                pool.note_page_lost(&previous);
                if admission.page_action(&client, &paging_root, slot, &previous, "save").await? {
                    pool.note_saved(previous);
                    admission.saved_evictee = true;
                }
                Ok(admission)
            }).await?;
        }
        self.slot = Some(slot);
        Ok(self)
    }
}

/// Admit operator traffic — `serving/cache-probe` — onto one physical slot through the
/// same owner as every turn, never around it. It holds no lane (it serves no page);
/// it holds the slot's operation permit, so it waits behind a live decode instead of
/// queueing inside the server, and it saves and detaches the resident first.
///
/// Before this the probe posted straight to the slot: on a live node it queued behind a
/// persona's decode past its caller's patience (the M5 deploy self-check, 2026-09-25,
/// no answer in 280 s — read as `unknown` and held the fleet's deploys), overwrote her
/// warm KV without saving it, and restored its own filler under her attribution, so
/// her next save wrote filler under her name. An endpoint with no slot pool (external,
/// cloud) has no resident to protect; `slot()` is then `None`.
///
/// `slot: None` asks for the pool's SCRATCH slot when the server has one: a slot no
/// page holds, so the probe waits behind nobody and evicts nobody (Cormac's review
/// of #4388: behind a 7-minute turn even an admitted probe on slot 0 outwaits a 280 s
/// caller). Without a scratch slot it borrows slot 0 and saves its resident first.
pub async fn admit_transient(
    client: &reqwest::Client,
    root: &str,
    slot: Option<u32>,
) -> Result<TurnAdmission, String> {
    admit_transient_through(client, root, slot, crate::inference::slots::page_io::gate()).await
}

/// [`admit_transient`] through the page-IO gate `page_io`.
async fn admit_transient_through(
    client: &reqwest::Client,
    root: &str,
    slot: Option<u32>,
    page_io: &'static crate::runtime::admission_gate::AdmissionGate,
) -> Result<TurnAdmission, String> {
    let endpoint = crate::inference::slots::directory()
        .endpoint(root)
        .admit()
        .await?;
    let pool = endpoint.pool.clone();
    let mut admission = TurnAdmission {
        slot: None,
        _pin: None,
        _slot_permit: None,
        released: None,
        page_confirmed: false,
        page_in: PageIn::None,
        saved_evictee: false,
        paging_in_flight: false,
        _permit: None,
        uncommitted: None,
        scratch: pool.as_ref().and_then(|pool| pool.scratch_slot()),
        _endpoint: endpoint,
        _mind: None,
    };
    let Some(pool) = pool else {
        return Ok(admission);
    };
    let slot = slot
        .or_else(|| pool.scratch_slot())
        .unwrap_or_else(|| pool.transient_slot()); // JUSTIFIED unwrap_or_else: no named slot and no scratch slot = the cheapest citizen slot, whose resident is saved and detached first
    if slot >= pool.n_slots() {
        return Err(format!(
            "slot {slot} does not exist on this endpoint ({} slots)",
            pool.n_slots()
        ));
    }
    admission = admission.borrow_slot_for_transient(&pool, slot, client, root, page_io).await?;
    Ok(admission)
}

/// The page switches this node has completed (ms), newest kept: what a save/restore
/// COSTS HERE, measured — the wedge bound derives from it, and the allocator reads it
/// as the per-turn price of fewer lanes than residents on a discrete card.
static KV_PAGE_SWITCH_MS: crate::inference::resource_admission::WaitRing =
    crate::inference::resource_admission::WaitRing::new();

/// The page body for `filename` that names which context checkpoints a saved page carries;
/// the quote (`save-size`) and the write send the same one, or the admitted bytes lie. A
/// restore resumes from the newest checkpoint at or before where her next prompt diverges,
/// and that is the end of her view: the next act resends it and rebuilds the tail after it.
/// So the page keeps its head (the shared head of the first user turn), the newest checkpoint
/// at or before the view end (the engine makes one there, from the request's
/// `view_messages`), and those the engine made for her last prompt's end (an act that resends
/// that prompt whole: 54 of 161 page-ins). Positions, not a count: on the M5 (engine log
/// 2026-10-26, 161 page-ins) the newest four sat inside the closing block, past where her
/// next prompt diverged, and 107 page-ins resumed from the ~11k head with 33-89k matched.
///
/// `keep_newest_checkpoints: 0` is for an engine that predates `keep_view_checkpoints`: it
/// saves the head alone, never all of them (27 measured 11.4 GB of sidecar and a 66-100 s save).
pub(crate) fn page_body(filename: &str) -> serde_json::Value {
    json!({ "filename": filename, "keep_view_checkpoints": true, "keep_newest_checkpoints": 0 })
}

/// Below this a switch is never called a wedge, whatever the ring says — the floor the
/// first switch on a box is judged by (the UMA case lands in ~0.1 s; the floor is 100×).
const KV_PAGE_WEDGE_FLOOR: std::time::Duration = std::time::Duration::from_secs(10);

/// The interval at which a slow page switch asks whether the engine is still alive: the
/// floor, raised to three times this node's measured p90. Past it a switch is only a
/// WEDGE if the engine also stops answering `/health`. Pure.
///
/// The bound was a flat 10 s from the day the M5 measured a restore at ~0.1 s — on
/// unified memory the KV already lives in host RAM. On a discrete card a page is
/// VRAM → host → file: the 5090 (2026-09-21, 26,880-token q8 pages) measured 88
/// switches at p50 4.1 s, p90 6.8 s, **max 10,004 ms** — the flat cap was cutting a
/// healthy switch off and calling the server wedged, and a 60k page would trip it every
/// time. A bound sized from the work (Fable, #4277) cannot mistake a big page for a hang.
pub fn kv_page_wedge_bound(measured_p90_ms: u64) -> std::time::Duration {
    KV_PAGE_WEDGE_FLOOR.max(std::time::Duration::from_millis(measured_p90_ms.saturating_mul(3)))
}

/// Only a complete server receipt establishes terminal paging state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageOutcome {
    Completed,
    Rejected,
    Uncertain,
}

/// How many consecutive checkpoints a switch may wait on an engine that is doing nothing
/// else — no counter moving, no slot processing — before it is called stuck. On such an
/// engine the queue head is the switch itself (a save/restore moves no slot counter), so
/// "quiet" is not evidence of a stall; this caps how long quiet may last.
// derived-or-floor: a CEILING on a wait that has no progress signal to read — the slot counters cannot see a save/restore, so the bound (itself derived from this node's measured switch p90) times this count is the patience for the engine's own queue head; it only ever ENDS a wait, never shortens one that shows progress.
const QUIET_ENGINE_CHECKPOINTS: u32 = 8;

/// A page switch's wait: [`crate::inference::llama_server::wait_while_engine_progresses`]
/// with this module's quiet allowance, because a save/restore moves no slot counter and a
/// quiet engine here is working the switch itself.
async fn wait_while_engine_progresses<T, W, P, PF>(
    work: W,
    bounds: impl Into<crate::inference::llama_server::WaitBounds>,
    progress: P,
    on_busy: impl FnMut(u64),
) -> Option<T>
where
    W: std::future::Future<Output = T>,
    P: FnMut() -> PF,
    PF: std::future::Future<Output = EngineProbe>,
{
    crate::inference::llama_server::wait_while_engine_progresses(work, bounds, QUIET_ENGINE_CHECKPOINTS, progress, on_busy).await
}

/// Execute one page action through the shared turn/warm-ahead boundary. The
/// measured wedge bound is a progress checkpoint (see [`wait_while_engine_progresses`]):
/// a switch queued behind a moving engine keeps waiting; a stalled or silent engine
/// makes the outcome uncertain, and
/// uncertainty never acknowledges remote completion — the admission owner must
/// quarantine before releasing its lease.
pub(crate) async fn kv_page_action(
    client: &reqwest::Client,
    root: &str,
    slot: u32,
    key: &PageKey,
    action: &str,
) -> PageOutcome {
    let path = if action == "save" {
        match crate::inference::slots::directory().page_admission_path(root) {
            Ok(path) => path,
            Err(reason) => {
                continuum_base::probe!(class = "inference.kv_page.admission_refused", slot = slot as u64,
                    reason = %reason, "page save refused before any checkpoint file write");
                return PageOutcome::Rejected;
            }
        }
    } else { None };
    kv_page_action_with_capacity(client, root, slot, key, action, path.is_some(), |bytes| {
        let path = path.ok_or_else(|| "page directory is not owned by this node".to_string())?;
        let volume = crate::system_resources::DiskPressureMonitor::volume_for(&path)
            .ok_or_else(|| "page volume has no disk monitor sample".to_string())?;
        crate::paging::pool::ensure_projected_disk_capacity(
            "kv-pages", volume.total_bytes.saturating_sub(volume.available_bytes),
            volume.total_bytes, bytes,
        ).map_err(|error| error.to_string())
    }).await
}

// Keep receipt consumption inside the watchdog's owned future. Tests use this
// same HTTP boundary with local progress evidence, without global serving state
// or the concurrently updated switch-cost distribution.
async fn bounded_page_receipt<P, PF>(
    request: reqwest::RequestBuilder,
    bounds: impl Into<crate::inference::llama_server::WaitBounds>,
    progress: P,
    on_busy: impl FnMut(u64),
) -> Option<Result<(u16, Option<serde_json::Value>), reqwest::Error>>
where
    P: FnMut() -> PF,
    PF: std::future::Future<Output = EngineProbe>,
{
    let receipt = async {
        let response = request.send().await?;
        let status = response.status().as_u16();
        let body = response.json::<serde_json::Value>().await.ok();
        Ok((status, body))
    };
    wait_while_engine_progresses(receipt, bounds, progress, on_busy).await
}

/// Why a page save's size quote gave no size: each a different fault, so the refusal names which
/// (card 8fa1c147; enrico 2026-10-25/26: 11 refusals and 13 cold turns read only "quote
/// unavailable", and nothing said whether the engine refused, answered for another slot, or was
/// still busy with a prefill when the bound passed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum QuoteMiss {
    /// The request did not reach the engine or its reply did not arrive.
    Unsent(String),
    /// The engine answered with a non-success status.
    Status(u16),
    /// The engine answered for another slot or another file.
    Mismatch,
    /// The answer named no positive size.
    NoSize,
    /// The engine stopped making progress (or stopped answering) before it quoted.
    EngineStopped,
}

impl std::fmt::Display for QuoteMiss {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsent(error) => write!(f, "request not delivered: {error}"),
            Self::Status(status) => write!(f, "engine answered HTTP {status}"),
            Self::Mismatch => f.write_str("engine answered for another slot or file"),
            Self::NoSize => f.write_str("engine named no positive size"),
            Self::EngineStopped => f.write_str("engine stopped progressing before it quoted"),
        }
    }
}

/// Ask the engine what a save of `slot` into `filename` would write. The quote names the same
/// checkpoint subset as the write, or the admitted bytes lie. A non-success status returns before
/// its body is read, so an engine that refuses and then stalls its error body is a `Status` miss
/// at once, never a wait that a stall watchdog could report as `EngineStopped` (Codex on #5441).
pub(crate) async fn request_quote(client: &reqwest::Client, root: &str, slot: u32, filename: &str) -> Result<u64, QuoteMiss> {
    let response = client.post(format!("{}/slots/{slot}?action=save-size", root.trim_end_matches('/')))
        .json(&page_body(filename)).send().await
        .map_err(|error| QuoteMiss::Unsent(error.to_string()))?;
    let status = response.status().as_u16();
    if !response.status().is_success() {
        return Err(QuoteMiss::Status(status));
    }
    let body = response.json::<serde_json::Value>().await.ok();
    read_quote(status, body.as_ref(), slot, filename)
}

/// PURE: the size an engine's `save-size` reply quotes for exactly this slot and file.
pub(crate) fn read_quote(status: u16, body: Option<&serde_json::Value>, slot: u32, filename: &str) -> Result<u64, QuoteMiss> {
    if !(200..300).contains(&status) {
        return Err(QuoteMiss::Status(status));
    }
    let body = body.ok_or(QuoteMiss::NoSize)?;
    if body["id_slot"].as_u64() != Some(u64::from(slot)) || body["filename"].as_str() != Some(filename) {
        return Err(QuoteMiss::Mismatch);
    }
    body["projected_bytes"].as_u64().filter(|bytes| *bytes > 0).ok_or(QuoteMiss::NoSize)
}

// The page-action boundary accepts capacity evidence from its existing owner;
// HTTP fixtures can exercise the same protocol without a global monitor override.
async fn kv_page_action_with_capacity(
    client: &reqwest::Client,
    root: &str,
    slot: u32,
    key: &PageKey,
    action: &str,
    governed: bool,
    admit_bytes: impl FnOnce(u64) -> Result<(), String>,
) -> PageOutcome {
    let url = format!("{}/slots/{}?action={}", root.trim_end_matches('/'), slot, action);
    let filename = crate::inference::slots::page_filename(key);
    let operation = (action == "restore").then(uuid::Uuid::new_v4);
    let (p50_ms, p90_ms, measured) = KV_PAGE_SWITCH_MS.p50_p90();
    let bound = kv_page_wedge_bound(p90_ms);
    // Another slot may be mid-batch: its counters move once per batch, so an engine that is
    // processing and unmoved is judged over the measured batch window, never one checkpoint
    // (card 0b52b957: 3 of 3 saves refused 15-17 s into a ~190 s batch on enrico).
    let model = crate::inference::slots::directory().endpoint(root).launched_model();
    let bounds = crate::inference::llama_server::WaitBounds::for_model(bound, model.as_deref());
    let started = std::time::Instant::now();
    // BUSY IS NOT DEAD. The bound is a checkpoint, never a verdict: llama-server runs
    // slot save/restore on its task queue BETWEEN decode batches, so under a full set
    // of lanes a healthy switch waits behind their prefill. Measured 2026-09-25 (M5,
    // 4 lanes prefilling): a restore ran 50.8 s against a 50.8 s bound (3 × p90), was
    // abandoned as uncertain, quarantined the endpoint and replaced the whole engine —
    // 9 minutes with every citizen dark, back at 3 × 18.7k instead of 4 × 26k. So at
    // each bound the engine's `/slots` work fingerprint is read: while it moves, the
    // queue this switch sits in is moving and the switch keeps waiting. A queue that
    // stops moving, or an engine that stops answering, turns the switch uncertain at
    // that checkpoint, and a dead engine ends it sooner when its socket drops.
    let mut payload = if action == "save" { page_body(&filename) } else { json!({ "filename": filename }) };
    if let Some(operation) = operation {
        payload["operation_id"] = json!(operation);
    }
    if action == "save" && governed {
        // A separate action is required: older engines must not mistake a quote for a write.
        let projected = wait_while_engine_progresses(
            request_quote(client, root, slot, &filename), bounds,
            || crate::inference::llama_server::engine_probe(root.trim_end_matches('/'), client),
            |_| {},
        ).await.unwrap_or(Err(QuoteMiss::EngineStopped)); // unwrap_or: the wait gives up only when the engine stopped progressing or answering
        let admission = projected.map_err(|miss| format!("exact page file quote unavailable ({miss}); no write requested"))
            .and_then(|bytes| { admit_bytes(bytes)?; Ok(bytes) });
        match admission {
            Ok(bytes) => payload["admitted_bytes"] = json!(bytes),
            Err(reason) => {
                continuum_base::probe!(class = "inference.kv_page.admission_refused", slot = slot as u64,
                    reason = %reason, unmoved_ms = bounds.unmoved.as_millis() as u64,
                    unmoved_rate_tps = bounds.unmoved_rate.unwrap_or(0.0), // unwrap_or: 0 = no rate measured, named by the source field
                    unmoved_rate_source = bounds.unmoved_rate_source,
                    "page save refused before any checkpoint file write");
                return PageOutcome::Rejected;
            }
        }
    }
    // The same progress bound covers the complete receipt, not just headers.
    let resp = bounded_page_receipt(
        client.post(&url).json(&payload),
        bounds,
        || async {
            match operation {
                Some(operation) => crate::inference::llama_server::restore_probe(root.trim_end_matches('/'), client, operation, slot).await,
                None => crate::inference::llama_server::engine_probe(root.trim_end_matches('/'), client).await,
            }
        },
        |busy_checkpoints| {
            continuum_base::probe!(
                class = "inference.kv_page.busy_not_dead",
                action = %action,
                slot = slot as u64,
                waited_ms = started.elapsed().as_millis() as u64,
                bound_ms = bound.as_millis() as u64,
                unmoved_ms = bounds.unmoved.as_millis() as u64,
                unmoved_rate_source = bounds.unmoved_rate_source,
                busy_checkpoints,
                "a page switch outlived its bound while the engine's queue kept moving \
                 — waiting on it, not replacing the engine",
            );
        },
    )
    .await
    .and_then(Result::ok);
    let (status, body) = match resp {
        Some(receipt) => receipt,
        None => (0, None), // Status zero records transport/watchdog failure, never success.
    };

    // Headers alone are not a terminal receipt. A disconnected waiter can leave
    // a queued restore behind; malformed/truncated/default responses fail closed.
    // The engine's OWN load time and bytes (`timings.{save,restore}_ms`, `n_written`/`n_read`):
    // its clock starts after a busy slot's deferral, so `ms - engine_ms` is the wait and
    // engine_ms the load (card a40e1bbf; the 5090 2026-10-11: Kimi's restores took 6-7.5 min
    // and nothing could say how much was waiting for the slot and how much was reading).
    let engine_ms = body.as_ref().and_then(|b| {
        b["timings"][if action == "save" { "save_ms" } else { "restore_ms" }].as_f64().map(|ms| ms as u64)
    });
    let engine_bytes = body.as_ref().and_then(|b| b[if action == "save" { "n_written" } else { "n_read" }].as_u64());
    let outcome = match body {
        Some(body)
            if (200..300).contains(&status)
                && body["id_slot"].as_u64() == Some(slot as u64)
                && body["filename"].as_str() == Some(filename.as_str())
                && body[if action == "save" { "n_saved" } else { "n_restored" }].as_u64().is_some() =>
        {
            PageOutcome::Completed
        }
        Some(body)
            if status == 400
                && body["error"]["code"].as_u64() == Some(400)
                && body["error"]["type"].as_str() == Some("invalid_request_error")
                && body["error"]["message"].as_str().is_some() =>
        {
            PageOutcome::Rejected
        }
        _ => PageOutcome::Uncertain,
    };
    // An uncertain receipt quarantines this engine: what it was doing is taken now, while it
    // is still the engine that did not answer (card 3913621a).
    if outcome == PageOutcome::Uncertain {
        let evidence = crate::inference::engine_stall::capture(root).await;
        continuum_base::probe!(
            class = "inference.kv_page.stall_evidence",
            action = %action,
            slot = slot as u64,
            status = status as u64,
            silent_ms = %evidence.silent_ms.map_or_else(|| "unknown".to_string(), |ms| ms.to_string()),
            sample = %evidence.sample.as_ref().map_or_else(|why| format!("none: {why}"), |path| path.display().to_string()),
            "the engine did not complete a page switch: its stack and how long its log was silent, taken before it is replaced"
        );
    }
    let ok = outcome == PageOutcome::Completed;
    let ms = started.elapsed().as_millis() as u64;
    if ok {
        KV_PAGE_SWITCH_MS.note(ms);
    }
    continuum_base::probe!(
        class = "inference.kv_page.action",
        action = %action,
        slot = slot as u64,
        page = %key,
        ok,
        terminal = outcome != PageOutcome::Uncertain,
        status = status as u64,
        ms,
        // absent when the engine gave no timing (an older engine, or no terminal body)
        engine_ms = %engine_ms.map_or_else(|| "none".to_string(), |v| v.to_string()),
        wait_ms = %engine_ms.map_or_else(|| "none".to_string(), |v| ms.saturating_sub(v).to_string()),
        // the engine counts only the .bin; the sidecars (checkpoints, adapter vector) are read and
        // written in the same timed switch, so they are named apart (BigMama on #4957)
        slot_bytes = %engine_bytes.map_or_else(|| "none".to_string(), |v| v.to_string()),
        sidecar_bytes = %crate::inference::slots::page_sidecar_bytes(&filename).map_or_else(|| "none".to_string(), |v| v.to_string()),
        bound_ms = bound.as_millis() as u64,
        node_p50_ms = p50_ms,
        node_p90_ms = p90_ms,
        node_measured = measured as u64,
        "KV page context switch — save pages the evictee's state out, restore pages \
         the returner's state in; an uncertain receipt quarantines the endpoint",
    );
    outcome
}

#[cfg(test)]
mod tests;
