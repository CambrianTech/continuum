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
//! 1. **Permit first.** Take the concurrency permit (the endpoint's `DecodeBound`) BEFORE
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

use crate::inference::resource_admission::{LanePriority, ServingLanePermit};
use crate::inference::slots::{DecodeBound, DecodePermit, KvSlotPool, MindTurn, PageKey, SlotClass, SlotPermit, SlotPin};

mod preparation;
pub(crate) use preparation::{AdmissionFailure, Preparation};
pub(crate) use request::admit_prepared_turn;
mod request;
pub use request::{admit_turn, AdmissionPatience, HER_ACT_FIRST};

mod page_liveness;
mod page_quote;
mod page_receipt;
mod paging_lifetime;
mod unresolved_page;
mod transient;
mod learning;

pub use transient::admit_transient;
#[cfg(test)]
use transient::admit_transient_through;

use page_receipt::{terminal_receipt, Terminal};
use page_liveness::{bounded_page_receipt, wait_while_engine_progresses, PageLiveness, Receipt};
#[cfg(test)]
use page_quote::read_quote;
use page_quote::{request_quote, QuoteMiss};
pub(crate) use unresolved_page::{PageResolution, UnresolvedPage};

/// How this turn's KV reached its slot: the evidence the restore economy is judged by
/// (`inference.restored_turn`). A restore the engine refused is a fast-path MISS, recorded
/// as one, never folded into "nothing to restore" (card f3ba3b56).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageIn {
    /// No page was due: the page prefills by design (its first turn, or no pool).
    None,
    /// Its KV was already resident in the leased slot.
    Resident,
    /// Restored from its saved page, which held her newest known sequence.
    Restored,
    /// Restored from a page OLDER than her newest completed sequence, which no slot held any
    /// more (`SequenceBasis::PageBehindLive`): a regression, whatever the engine then reuses.
    RestoredBehindLive,
    /// A restore was due and the engine refused it, so the turn re-prefills in full.
    RestoreRejected,
}

/// Whether the engine was ever asked to generate on this admission: the one fact that says
/// whether dropping it can have touched her KV (Benchy, M5 ed142f14c: a refit dropped before its
/// POST forgot her residency, and her retry restored an older page over her live sequence).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dispatch {
    /// Nothing was sent: the slot holds what paging left there.
    Unsent,
    /// A generation request was sent to the engine.
    Sent,
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
    _permit: Option<DecodePermit>,
    /// A lease is provisional until the adapter observes successful generation.
    /// Cancellation during paging or generation must not save foreign KV later.
    uncommitted: Option<(Arc<KvSlotPool>, PageKey, u32)>,
    dispatch: Dispatch,
    preparation: Option<crate::capacity::window_grant::GrantReservation>,
    _endpoint: crate::inference::slots::EndpointAdmission,
    scratch: Option<u32>,
    /// A dream's ambient serving lane, taken only where it borrows a citizen slot (no scratch
    /// slot): there it must yield to residents through the lane gate. With a scratch slot its
    /// admission is that slot's permit, and it holds no resident lane (Cormac, card 920c43bc).
    _lane: Option<ServingLanePermit>,
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

    /// The engine incarnation this turn's endpoint admission is bound to (`EndpointAdmission::incarnation`).
    pub(crate) fn engine_incarnation(&self) -> Option<(u32, u64)> {
        self._endpoint.incarnation()
    }

    /// How this turn's KV reached its slot ([`PageIn`]).
    pub(crate) fn page_in(&self) -> PageIn {
        self.page_in
    }

    /// Page `key` through `slot`. The admission is CONSUMED: a terminal receipt hands it back;
    /// an uncertain switch drops it in flight (its drop quarantines); an unresolved switch moves
    /// it, with the request, into the endpoint's custody until the engine answers (Codex on
    /// #5772), so no later turn can take this slot while the engine still works on it.
    async fn page_action(
        mut self,
        client: &reqwest::Client,
        root: &str,
        slot: u32,
        key: &PageKey,
        action: &str,
    ) -> Result<(Self, bool), String> {
        self.paging_in_flight = true;
        if let Some(grant) = &mut self.preparation { grant.mark_dispatched(); }
        match kv_page_action(client, root, slot, key, action).await {
            PageOutcome::Completed => {
                self.preparation_io_completed();
                self.paging_in_flight = false;
                Ok((self, true))
            }
            PageOutcome::Rejected => {
                self.preparation_io_completed();
                self.paging_in_flight = false;
                Ok((self, false))
            }
            PageOutcome::Unresolved(unresolved) => {
                self.paging_in_flight = false;
                // The mind is not the slot: her next act may proceed on another slot or cold.
                drop(self._mind.take());
                unresolved.hold_turn(self);
                Err("KV page switch unresolved: the engine kept working without a receipt; its slot is held \
                    until the engine answers, the page is held not resident, the engine is not replaced on this alone".into())
            }
            PageOutcome::Uncertain => Err("KV paging completion unverified; endpoint quarantined until verified engine replacement".into()),
        }
    }

    /// The adapter is sending this admission's generation request to the engine now. Called
    /// at every attempt's POST, never before: a request refused earlier (a context refit, a
    /// changed grant) leaves the slot as paging left it.
    pub(crate) fn dispatching(&mut self) {
        self.dispatch = Dispatch::Sent;
        if let Some((pool, key, slot)) = &self.uncommitted {
            pool.note_dispatched(*slot, *key);
        }
    }

    /// The adapter received a complete successful generation in this slot.
    pub(crate) fn generation_completed(&mut self) {
        if let Some((pool, key, slot)) = self.uncommitted.take() {
            pool.note_generated(slot, key);
        }
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
            // Nothing sent: the slot holds what paging left, which is hers when paging confirmed
            // it (resident, or a verified restore), so she keeps her residency. A sent request
            // or unconfirmed paging proves nothing about whose KV is there.
            if self.dispatch == Dispatch::Sent || !self.page_confirmed {
                pool.forget_resident(slot, key);
            }
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

/// Best-effort warm-ahead uses the same paging and provisional-attribution owner
/// as a turn. No generation follows, so only proven physical warmth is committed.
pub(crate) async fn warm_ahead(
    concurrency: impl Into<DecodeBound>,
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
    // No generation follows: its drop keeps only the warmth paging confirmed (`Dispatch::Unsent`).
    let concurrency = concurrency.into();
    let admission = admit(&concurrency, SlotClass::Turn, Some(key), Some(pool), client, root, 0, true, std::time::Duration::ZERO, None).await.map_err(AdmissionFailure::into_text)?;
    if let Some(slot) = admission.slot {
        continuum_base::probe!(
            class = "inference.kv_warm_ahead",
            page = %key,
            slot = slot as u64,
            saved_evictee = admission.saved_evictee,
            restored = matches!(admission.page_in, PageIn::Restored | PageIn::RestoredBehindLive),
            confirmed = admission.page_confirmed,
            ms = started.elapsed().as_millis() as u64,
            "warm-ahead completed through turn paging admission",
        );
    }
    Ok(())
}

async fn admit(
    concurrency: &DecodeBound,
    class: SlotClass,
    key: Option<PageKey>,
    pool: Option<Arc<KvSlotPool>>,
    client: &reqwest::Client,
    root: &str,
    approx_tokens: u64,
    warm_only: bool,
    patience: std::time::Duration,
    preparation: Option<Preparation>,
) -> Result<TurnAdmission, AdmissionFailure> {
    let started = tokio::time::Instant::now();
    let mut readiness_left = patience;
    let mut waited = false;
    let mut caller_lane = if warm_only {
        None
    } else {
        crate::inference::resource_admission::take_serving_lane()?
    };
    let reacquire = caller_lane.as_ref().map(ServingLanePermit::reacquisition);
    // A KEYED DREAM (her own background work on her page: a learning step) never waits holding her
    // mind gate: it queues for its ambient lane, the permit and her slot holding nothing, and takes
    // her gate only once her slot is pinned and permitted, without waiting (`HER_ACT_FIRST`).
    // Queued at step 0 like a turn, it held the gate through every wait below and her own act
    // queued behind her learning (Codex on card 23069702).
    let keyed_dream = class == SlotClass::Dream && key.is_some();
    // 0. ONE ACT AT A TIME PER MIND — queued here, before the lane, holding nothing
    //    (`slots::mind_gate`). Warm-ahead took its gate itself, without waiting.
    let mut mind = match key {
        Some(k) if !warm_only && !keyed_dream => {
            let begin = MindTurn::begin(k);
            tokio::pin!(begin);
            Some(match futures::poll!(&mut begin) {
                std::task::Poll::Ready(result) => result?,
                std::task::Poll::Pending => {
                    // Keep this exact queued mind acquisition, but lend its
                    // caller lane while it waits for page custody or a prior act.
                    drop(caller_lane.take());
                    begin.await?
                }
            })
        }
        _ => None,
    };
    // A dream's ambient serving lane, held only while it must borrow a citizen slot.
    let mut lane: Option<ServingLanePermit> = None;
    'admit: loop {
        if caller_lane.is_none() {
            if let Some(authority) = &reacquire {
                caller_lane = Some(authority.acquire().await);
            }
        }
        // 1. PERMIT FIRST — event-driven wait for a free lane. Cannot fail: the
        //    semaphore is never closed over the adapter's lifetime. A dream holding its lane
        //    only TRIES: when the adapter is full it lets the lane go, waits for a permit
        //    holding nothing, and takes both again (Codex on #5786).
        let _permit = match lane.as_ref().or(caller_lane.as_ref()).map(|_| concurrency.try_acquire()) {
            Some(Ok(permit)) => permit,
            Some(Err(_)) => {
                lane = None;
                caller_lane = None;
                drop(concurrency.acquire().await);
                continue 'admit;
            }
            None => concurrency.acquire().await,
        };

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
            dispatch: Dispatch::Unsent,
            preparation: None,
            scratch: pool.as_ref().and_then(|pool| pool.scratch_slot()),
            _endpoint: endpoint,
            _mind: None,
            _lane: caller_lane.take(),
        };

        // A DREAM YIELDS TO RESIDENTS (Fable, M5 0b2854327 01:24-01:50Z: a dream parked on scratch
        // holding a lane while Aiko waited 1,141 s and Benchy 132 s with two slots idle). Decided
        // each pass from the ADMITTED generation's pool (Codex on #5786: a transition can change
        // it). Where it borrows a citizen slot (no scratch slot) it holds an ambient lane, so a
        // waiting turn still wins, and takes it holding nothing; with a scratch slot that slot is
        // its admission and it holds no lane.
        // A keyed dream always borrows a citizen slot (her page), scratch or not, so it always
        // holds the ambient lane: on a node with a scratch slot it would otherwise outrank residents.
        if class == SlotClass::Dream && (keyed_dream || pool.as_ref().is_some_and(|pool| pool.scratch_slot().is_none())) {
            if lane.is_none() {
                drop(admission);
                lane = Some(crate::inference::resource_admission::acquire_serving_lane(None, LanePriority::Ambient, None).await);
                continue 'admit;
            }
        } else {
            lane = None;
        }

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
                lane = None; // a waiting dream holds no lane either (Codex on #5898); only dreams hold one
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
                admission._slot_permit = Some(match pool.try_acquire_slot(slot) {
                    Some(permit) => permit,
                    // A keyed dream waits for her busy slot holding nothing: no pin, no lane, no
                    // gate (Codex on #5898). A turn keeps its pin through the wait (a same-page
                    // returner must await the prior owner without permitting eviction).
                    None if keyed_dream => {
                        drop(admission);
                        lane = None;
                        wait_until_free(pool, slot).await?;
                        continue 'admit;
                    }
                    None => pool.acquire_slot(slot).await?,
                });
                admission._endpoint.check_ready()?;
                if keyed_dream {
                    // Her gate, without waiting: her act running or queued comes first. Returning
                    // drops this admission, releasing the pin, the slot permit and the lane.
                    mind = Some(MindTurn::try_begin(k).ok_or_else(|| HER_ACT_FIRST.to_owned())?);
                }
                // Before the plan moves the slot's attribution: once a stop has closed page IO,
                // nothing pages a resident out (card 47151d36, `slots::page_io`).
                let page_io = crate::inference::slots::admit_page_io(crate::inference::slots::page_io::gate())
                    .map_err(|closed| closed.to_string())?;
                admission.reserve_preparation(preparation)?;
                let pg = pool.plan_paging(slot, k);
                admission.page_confirmed = pg.already_resident;
                if pg.already_resident {
                    admission.page_in = PageIn::Resident;
                }
                // Every decision against her live sequence that is not plain residency is loud.
                if matches!(pg.basis, crate::inference::slots::SequenceBasis::LiveKept | crate::inference::slots::SequenceBasis::PageBehindLive) {
                    continuum_base::probe!(
                        class = "inference.kv_page.live_sequence",
                        page = %k,
                        slot = pg.slot as u64,
                        basis = ?pg.basis,
                        "her attribution was lost: LiveKept resumes the newer sequence in her slot; PageBehindLive restores a page older than her newest sequence (a regression)"
                    );
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
                            let saved;
                            (admission, saved) = admission.page_action(&client, &paging_root, pg.slot, &prev, "save").await?;
                            if saved {
                                pool.note_saved(prev);
                                admission.saved_evictee = true;
                            }
                        }
                        if pg.restore && interested.load(std::sync::atomic::Ordering::Acquire) {
                            let restored;
                            pool.note_overwritten(pg.slot);
                            (admission, restored) = admission.page_action(&client, &paging_root, pg.slot, &k, "restore").await?;
                            admission.page_confirmed = restored;
                            admission.page_in = match (restored, pg.basis) {
                                (true, crate::inference::slots::SequenceBasis::PageBehindLive) => PageIn::RestoredBehindLive,
                                (true, _) => PageIn::Restored,
                                (false, _) => PageIn::RestoreRejected,
                            };
                            if restored {
                                pool.note_restored(pg.slot, k);
                            }
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
            let Some(permit) = pool.try_acquire_slot(slot) else {
                drop(admission);
                lane = None;
                wait_until_free(pool, slot).await?;
                continue 'admit;
            };
            admission = admission
                .borrow_slot_for_transient(pool, slot, permit, client, root, crate::inference::slots::page_io::gate())
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
            // Queued behind the scratch slot's holder, it holds NOTHING: not the adapter
            // permit (sized to every slot, so a held one blocks traffic that could run now)
            // and no serving lane (card 920c43bc).
            let Some(permit) = pool.try_acquire_slot(scratch) else {
                drop(admission);
                wait_until_free(pool, scratch).await?;
                continue 'admit;
            };
            admission._slot_permit = Some(permit);
            admission._endpoint.check_ready()?;
        }

        if admission._mind.is_none() {
            admission._mind = mind.take();
        }
        if admission._lane.is_none() {
            admission._lane = lane.take();
        }
        return Ok(admission);
    }
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
#[derive(Debug)]
pub(crate) enum PageOutcome {
    Completed,
    Rejected,
    /// The engine stopped, vanished or went silent with no receipt: the endpoint's paging is
    /// unverified and the engine must be replaced.
    Uncertain,
    /// No receipt within the switch's bound while the engine's own process kept spending CPU
    /// (`page_liveness`). CPU is evidence against replacing the engine, not proof the switch
    /// progresses, so the engine is NOT replaced on this alone. The request is still in flight,
    /// and the only way to consume it is [`UnresolvedPage::hold`] with the slot's custody, kept
    /// until the engine answers (`unresolved_page`). The page is held as an uncertain switch
    /// leaves it (not resident in the slot; a save's page not restorable).
    Unresolved(UnresolvedPage),
}

/// Execute one page action through the shared turn/warm-ahead boundary. The
/// measured wedge bound is a progress checkpoint (see [`wait_while_engine_progresses`]):
/// a switch queued behind a moving engine keeps waiting; a stopped or vanished engine
/// makes the outcome uncertain, and
/// uncertainty never acknowledges remote completion — the admission owner must
/// quarantine before releasing its lease. A switch whose engine kept spending CPU without
/// a receipt is [`PageOutcome::Unresolved`], never a replacement on that alone.
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
    // `/slots` is itself a task on that queue, so it cannot answer during one long batch:
    // the engine PROCESS's CPU, read beside it, says busy or dead (`page_liveness`).
    let liveness = PageLiveness::for_root(root);
    let mut payload = if action == "save" { page_body(&filename) } else { json!({ "filename": filename }) };
    if let Some(operation) = operation {
        payload["operation_id"] = json!(operation);
    }
    if action == "save" && governed {
        // A separate action is required: older engines must not mistake a quote for a write.
        let projected = wait_while_engine_progresses(
            request_quote(client, root, slot, &filename), bounds,
            || {
                let probe = crate::inference::llama_server::slots_probe(root.trim_end_matches('/'), client);
                let liveness = &liveness;
                liveness.probe(probe)
            },
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
    let receipt = bounded_page_receipt(
        client.post(&url).json(&payload),
        bounds,
        &liveness,
        || async {
            match operation {
                Some(operation) => crate::inference::llama_server::restore_probe(root.trim_end_matches('/'), client, operation, slot).await,
                None => crate::inference::llama_server::slots_probe(root.trim_end_matches('/'), client).await,
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
                // the engine process's CPU over this checkpoint: the busy evidence when `/slots` is silent
                engine_cpu_ms = %liveness.cpu_spent().map_or_else(|| "unread".to_string(), |spent| spent.as_millis().to_string()),
                "a page switch outlived its bound while the engine kept working \
                 — waiting on it, not replacing the engine",
            );
        },
    )
    .await;
    let mut unresolved = None;
    let (status, body) = match receipt {
        Receipt::Answered(Ok(reply)) => reply,
        // Status zero records transport/watchdog failure, never success.
        Receipt::Answered(Err(_)) | Receipt::Stopped => (0, None),
        Receipt::Unresolved { cpu_spent, pending } => {
            unresolved = Some(UnresolvedPage {
                root: root.to_owned(),
                action: action.to_owned(),
                slot,
                filename: filename.clone(),
                cpu_spent,
                checkpoint: bounds.checkpoint,
                pending,
                liveness,
            });
            (0, None)
        }
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
    let outcome = match terminal_receipt(action, slot, &filename, status, body.as_ref()) {
        Some(Terminal::Completed) => PageOutcome::Completed,
        Some(Terminal::Rejected) => PageOutcome::Rejected,
        None => unresolved.map_or(PageOutcome::Uncertain, PageOutcome::Unresolved),
    };
    if let PageOutcome::Unresolved(unresolved) = &outcome {
        let cpu_spent = unresolved.cpu_spent();
        continuum_base::probe!(
            class = "inference.kv_page.unresolved",
            action = %action,
            slot = slot as u64,
            waited_ms = started.elapsed().as_millis() as u64,
            unmoved_ms = bounds.unmoved.as_millis() as u64,
            engine_cpu_ms = cpu_spent.as_millis() as u64,
            "a page switch outlived its measured bound while the engine's process kept spending CPU: \
             held unresolved with its slot's custody until the engine answers; the engine is not replaced on this alone"
        );
    }
    // A switch without a receipt: what the engine was doing is taken now, while it is still the
    // engine that did not answer (card 3913621a), and before a replacement if one follows.
    if matches!(outcome, PageOutcome::Uncertain | PageOutcome::Unresolved(_)) {
        let evidence = crate::inference::engine_stall::capture(root).await;
        continuum_base::probe!(
            class = "inference.kv_page.stall_evidence",
            action = %action,
            slot = slot as u64,
            status = status as u64,
            silent_ms = %evidence.silent_ms.map_or_else(|| "unknown".to_string(), |ms| ms.to_string()),
            sample = %evidence.sample.as_ref().map_or_else(|why| format!("none: {why}"), |path| path.display().to_string()),
            unresolved = matches!(outcome, PageOutcome::Unresolved(_)),
            "the engine did not complete a page switch: its stack and how long its log was silent"
        );
    }
    let ok = matches!(outcome, PageOutcome::Completed);
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
        terminal = matches!(outcome, PageOutcome::Completed | PageOutcome::Rejected),
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

/// Wait until `slot` is free, holding nothing, and let it go: the caller then takes every gate
/// again (its slot without waiting). Waiting on the slot's own semaphore cannot miss the release
/// the way a notification raised before the permit returns could; the next pass may still lose
/// the slot to another caller and wait again, which is how a non-turn call yields.
async fn wait_until_free(pool: &KvSlotPool, slot: u32) -> Result<(), String> {
    drop(pool.acquire_slot(slot).await?);
    Ok(())
}

#[cfg(test)]
mod tests;
