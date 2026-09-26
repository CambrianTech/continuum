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

use crate::inference::slots::{ActivityKey, KvSlotPool, SlotPin};

/// What a turn holds for the whole generation. Dropping it releases the concurrency
/// permit and the slot pin (RAII). An incomplete call also invalidates local KV
/// attribution; dropping this guard is not an acknowledgement of backend stop.
pub struct TurnAdmission {
    /// The slot to pin the request to (`id_slot`), or `None` to stay unpinned
    /// (traffic using separate scratch, or a Turn whose activity could not be leased).
    slot: Option<u32>,
    /// The slot pin — held so eviction cannot reassign this turn's slot mid-decode.
    _pin: Option<SlotPin>,
    /// Serializes same-slot operations even when distinct adapters admit the key.
    _slot_permit: Option<OwnedSemaphorePermit>,
    page_confirmed: bool,
    restored: bool,
    saved_evictee: bool,
    paging_in_flight: bool,
    /// Released after the pin so the next permit holder can lease the free slot.
    /// `None` only for [`admit_transient`], which holds its slot's operation permit
    /// instead of a lane (it serves no activity, so it takes no lane from one).
    _permit: Option<OwnedSemaphorePermit>,
    /// A lease is provisional until the adapter observes successful generation.
    /// Cancellation during paging or generation must not save foreign KV later.
    uncommitted: Option<(Arc<KvSlotPool>, ActivityKey, u32)>,
    _endpoint: crate::inference::slots::EndpointAdmission,
    scratch: Option<u32>,
}

impl TurnAdmission {
    /// The leased slot this turn should pin `id_slot` to, if any.
    pub fn slot(&self) -> Option<u32> {
        self.slot
    }

    pub(crate) fn scratch_slot(&self) -> Option<u32> {
        self.scratch
    }

    /// Whether this turn's KV was restored from a page before it generated — the
    /// evidence the restore economy is judged by (`inference.restored_turn`).
    pub(crate) fn restored(&self) -> bool {
        self.restored
    }

    async fn page_action(
        &mut self,
        client: &reqwest::Client,
        root: &str,
        slot: u32,
        key: &ActivityKey,
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
}

impl Drop for TurnAdmission {
    fn drop(&mut self) {
        if self.paging_in_flight {
            self._endpoint.quarantine_paging();
        }
        if let Some((pool, key, slot)) = self.uncommitted.take() {
            pool.forget_resident(slot, key);
        }
    }
}

/// Admit a turn: acquire the concurrency permit (always), then — for a Turn with a
/// resolvable `(persona, room)` key and a live slot pool — lease + pin its slot and
/// page its KV (save the evictee, restore this activity). Returns the guard the
/// caller must hold across the generation.
///
/// `key`/`pool` are `None` for non-Turn traffic (or a cloud provider with no slots):
/// the caller still gets the permit and places the request on scratch (or unpinned)
/// itself. On a single-slot server the pool saves and detaches the resident before
/// transient traffic borrows that slot; `slot()` then returns its index.
pub async fn admit_turn(
    concurrency: &Arc<Semaphore>,
    key: Option<ActivityKey>,
    pool: Option<Arc<KvSlotPool>>,
    client: &reqwest::Client,
    root: &str,
    approx_tokens: u64,
) -> Result<TurnAdmission, String> {
    admit(concurrency, key, pool, client, root, approx_tokens, false).await
}

/// Best-effort warm-ahead uses the same paging and provisional-attribution owner
/// as a turn. No generation follows, so only proven physical warmth is committed.
pub(crate) async fn warm_ahead(
    concurrency: &Arc<Semaphore>,
    key: ActivityKey,
    pool: Arc<KvSlotPool>,
    client: &reqwest::Client,
    root: &str,
) -> Result<(), String> {
    let started = std::time::Instant::now();
    let mut admission = admit(concurrency, Some(key), Some(pool), client, root, 0, true).await?;
    if admission.page_confirmed {
        admission.uncommitted = None;
    }
    if let Some(slot) = admission.slot {
        crate::probe!(
            class = "inference.kv_warm_ahead",
            persona = %key.persona,
            room = %key.room,
            slot = slot as u64,
            saved_evictee = admission.saved_evictee,
            restored = admission.restored,
            confirmed = admission.page_confirmed,
            ms = started.elapsed().as_millis() as u64,
            "warm-ahead completed through turn paging admission",
        );
    }
    Ok(())
}

async fn admit(
    concurrency: &Arc<Semaphore>,
    key: Option<ActivityKey>,
    pool: Option<Arc<KvSlotPool>>,
    client: &reqwest::Client,
    root: &str,
    approx_tokens: u64,
    warm_only: bool,
) -> Result<TurnAdmission, String> {
    // 1. PERMIT FIRST — event-driven wait for a free lane. Cannot fail: the
    //    semaphore is never closed over the adapter's lifetime.
    let _permit = concurrency
        .clone()
        .acquire_owned()
        .await
        .expect("adapter semaphore never closed");  // expect: the semaphore lives as long as the adapter, never closed

    let endpoint = crate::inference::slots::directory()
        .endpoint(root)
        .admit()
        .await?;
    // Discovery may have finished before an engine transition. Use the pool
    // owned by the admitted generation, never that stale discovery result.
    let pool = endpoint.pool.clone().or(pool);
    let mut admission = TurnAdmission {
        slot: None,
        _pin: None,
        _slot_permit: None,
        page_confirmed: false,
        restored: false,
        saved_evictee: false,
        paging_in_flight: false,
        _permit: Some(_permit),
        uncommitted: None,
        scratch: pool.as_ref().and_then(|pool| pool.scratch_slot()),
        _endpoint: endpoint,
    };

    if warm_only
        && pool
            .as_ref()
            .is_some_and(|pool| pool.scratch_slot().is_none())
    {
        return Ok(admission);
    }
    if let (Some(k), Some(pool)) = (key, pool.as_ref()) {
        if pool.lease(k).await.is_some() {
            // Pin before waiting for the physical-slot permit: a same-activity
            // returner must await the prior owner without permitting eviction.
            let Some((slot, pin)) = pool.pin_slot(&k) else {
                return Ok(admission);
            };
            admission._pin = Some(pin);
            admission._slot_permit = Some(pool.acquire_slot(slot).await?);
            admission._endpoint.check_ready()?;
            let pg = pool.plan_paging(slot, k);
            admission.page_confirmed = pg.already_resident;
            admission.uncommitted = Some((Arc::clone(pool), k, pg.slot));

            // 3. The context switch onto a now-free slot: page the evictee out,
            //    page this activity in. Lands immediately (no defer) because the slot
            //    is not decoding.
            if let Some(prev) = pg.save_first {
                // A failed or cancelled save may have replaced an older file.
                // Only a completed save can make this page restorable again.
                pool.note_page_lost(&prev);
                if admission
                    .page_action(client, root, pg.slot, &prev, "save")
                    .await?
                {
                    pool.note_saved(prev);
                    admission.saved_evictee = true;
                }
            }
            if pg.restore {
                admission.restored = admission
                    .page_action(client, root, pg.slot, &k, "restore")
                    .await?;
                admission.page_confirmed = admission.restored;
                if !admission.restored {
                    pool.note_page_lost(&k);
                }
            }
            // Price basis for the eviction policy (B5): this activity's current
            // prompt size — comparable across slots, which is all eviction needs.
            if !warm_only {
                pool.note_tail(&k, approx_tokens);
            }
            admission.slot = Some(pg.slot);
        }
    } else if let Some(pool) = pool.as_ref().filter(|pool| pool.n_slots() == 1) {
        // A single-slot server has no separate scratch slot. Preserve the
        // resident before background/anonymous traffic borrows its physical slot.
        // Remove attribution BEFORE the await so cancellation cannot leave the
        // next activity treating transient KV as its own warm tail.
        admission.borrow_slot_for_transient(pool, 0, client, root).await?;
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

    Ok(admission)
}

impl TurnAdmission {
    /// Borrow physical `slot` for traffic that serves no activity: wait for the slot's
    /// operation permit (event-driven, behind any live decode), detach its resident and
    /// SAVE the resident's page so its warmth survives, and leave the slot unattributed
    /// so no activity later mistakes this traffic's KV for its own.
    async fn borrow_slot_for_transient(
        &mut self,
        pool: &Arc<KvSlotPool>,
        slot: u32,
        client: &reqwest::Client,
        root: &str,
    ) -> Result<(), String> {
        self._slot_permit = Some(pool.acquire_slot(slot).await?);
        self._endpoint.check_ready()?;
        let (previous, pin) = pool.take_resident(slot);
        self._pin = pin;
        if let Some(previous) = previous {
            pool.note_page_lost(&previous);
            if self.page_action(client, root, slot, &previous, "save").await? {
                pool.note_saved(previous);
                self.saved_evictee = true;
            }
        }
        self.slot = Some(slot);
        Ok(())
    }
}

/// Admit operator traffic — `serving/cache-probe` — onto one physical slot through the
/// same owner as every turn, never around it. It holds no lane (it serves no activity);
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
/// activity holds, so the probe waits behind nobody and evicts nobody (Cormac's review
/// of #4388: behind a 7-minute turn even an admitted probe on slot 0 outwaits a 280 s
/// caller). Without a scratch slot it borrows slot 0 and saves its resident first.
pub(crate) async fn admit_transient(
    client: &reqwest::Client,
    root: &str,
    slot: Option<u32>,
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
        page_confirmed: false,
        restored: false,
        saved_evictee: false,
        paging_in_flight: false,
        _permit: None,
        uncommitted: None,
        scratch: pool.as_ref().and_then(|pool| pool.scratch_slot()),
        _endpoint: endpoint,
    };
    let Some(pool) = pool else {
        return Ok(admission);
    };
    let slot = slot.or_else(|| pool.scratch_slot()).unwrap_or(0); // JUSTIFIED unwrap_or: no named slot and no scratch slot = slot 0, whose resident is saved and detached first
    if slot >= pool.n_slots() {
        return Err(format!(
            "slot {slot} does not exist on this endpoint ({} slots)",
            pool.n_slots()
        ));
    }
    admission.borrow_slot_for_transient(&pool, slot, client, root).await?;
    Ok(admission)
}

/// The page switches this node has completed (ms), newest kept: what a save/restore
/// COSTS HERE, measured — the wedge bound derives from it, and the allocator reads it
/// as the per-turn price of fewer lanes than residents on a discrete card.
static KV_PAGE_SWITCH_MS: crate::cognition::resource_admission::WaitRing =
    crate::cognition::resource_admission::WaitRing::new();

/// Below this a switch is never called a wedge, whatever the ring says — the floor the
/// first switch on a box is judged by (the UMA case lands in ~0.1 s; the floor is 100×).
const KV_PAGE_WEDGE_FLOOR: std::time::Duration = std::time::Duration::from_secs(10);

/// (p50 ms, p90 ms, count) of this node's completed page switches. (0, 0, 0) = unmeasured.
pub fn kv_page_switch_ms() -> (u64, u64, u32) {
    KV_PAGE_SWITCH_MS.p50_p90()
}

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

/// Await `work`, treating each `bound` as a PROGRESS checkpoint rather than a verdict.
/// At each checkpoint `progress()` reads the engine's work fingerprint; the wait goes on
/// only while it has moved since the last read, i.e. the engine's queue is advancing and
/// this switch will be reached. An unchanged fingerprint (the queue is stuck, even if the
/// server still answers) or no answer at all ends the wait as `None`. The first read is
/// taken at the start, so a stall is recognised at the first checkpoint.
///
/// Why progress and not liveness (Cormac's review of #4387): `/health` is answered off
/// the slot queue, so it stays healthy through exactly the queue-side stall the bound
/// exists to catch, and the caller would hold its pin with no way out.
async fn wait_while_engine_progresses<T, W, P, PF>(
    work: W,
    bound: std::time::Duration,
    mut progress: P,
    mut on_busy: impl FnMut(u64),
) -> Option<T>
where
    W: std::future::Future<Output = T>,
    P: FnMut() -> PF,
    PF: std::future::Future<Output = Option<u64>>,
{
    tokio::pin!(work);
    let mut last = progress().await;
    let mut busy_checkpoints: u64 = 0;
    loop {
        tokio::select! {
            done = &mut work => return Some(done),
            _ = tokio::time::sleep(bound) => {
                let now = progress().await;
                match (last, now) {
                    (_, None) => return None,
                    (Some(before), Some(after)) if before == after => return None,
                    _ => {}
                }
                last = now;
                busy_checkpoints += 1;
                on_busy(busy_checkpoints);
            }
        }
    }
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
    key: &ActivityKey,
    action: &str,
) -> PageOutcome {
    let url = format!("{}/slots/{}?action={}", root.trim_end_matches('/'), slot, action);
    let filename = crate::inference::slots::page_filename(key);
    let (p50_ms, p90_ms, measured) = KV_PAGE_SWITCH_MS.p50_p90();
    let bound = kv_page_wedge_bound(p90_ms);
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
    let send = client
        .post(&url)
        .json(&json!({ "filename": filename }))
        .send();
    let resp = wait_while_engine_progresses(
        send,
        bound,
        || crate::inference::llama_server::engine_progress(root.trim_end_matches('/'), client),
        |busy_checkpoints| {
            crate::probe!(
                class = "inference.kv_page.busy_not_dead",
                action = %action,
                slot = slot as u64,
                waited_ms = started.elapsed().as_millis() as u64,
                bound_ms = bound.as_millis() as u64,
                busy_checkpoints,
                "a page switch outlived its bound while the engine's queue kept moving \
                 — waiting on it, not replacing the engine",
            );
        },
    )
    .await
    .and_then(Result::ok);
    let status = resp.as_ref().map(|r| r.status().as_u16()).unwrap_or(0); // JUSTIFIED unwrap_or: status 0 records transport failure or a silent engine, never success.

    // Headers alone are not a terminal receipt. A disconnected waiter can leave
    // a queued restore behind; malformed/truncated/default responses fail closed.
    let outcome = match resp {
        Some(response) => match response.json::<serde_json::Value>().await {
            Ok(body)
                if (200..300).contains(&status)
                    && body["id_slot"].as_u64() == Some(slot as u64)
                    && body["filename"].as_str() == Some(filename.as_str())
                    && body[if action == "save" {
                        "n_saved"
                    } else {
                        "n_restored"
                    }]
                    .as_u64()
                    .is_some() =>
            {
                PageOutcome::Completed
            }
            Ok(body)
                if status == 400
                    && body["error"]["code"].as_u64() == Some(400)
                    && body["error"]["type"].as_str() == Some("invalid_request_error")
                    && body["error"]["message"].as_str().is_some() =>
            {
                PageOutcome::Rejected
            }
            _ => PageOutcome::Uncertain,
        },
        None => PageOutcome::Uncertain,
    };
    let ok = outcome == PageOutcome::Completed;
    let ms = started.elapsed().as_millis() as u64;
    if ok {
        KV_PAGE_SWITCH_MS.note(ms);
    }
    crate::probe!(
        class = "inference.kv_page.action",
        action = %action,
        slot = slot as u64,
        persona = %key.persona,
        room = %key.room,
        ok,
        terminal = outcome != PageOutcome::Uncertain,
        status = status as u64,
        ms,
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
mod tests {
    // what this catches (the 5090, 2026-09-21): a page switch that costs seconds on a
    // discrete card is not a wedge — the bound rises with this node's measured p90 and
    // never falls below the floor; an unmeasured node is judged by the floor alone.
    #[test]
    fn the_wedge_bound_is_sized_from_the_nodes_measured_switches_never_a_flat_cap() {
        use super::kv_page_wedge_bound;
        use std::time::Duration;
        assert_eq!(kv_page_wedge_bound(0), Duration::from_secs(10), "unmeasured: the floor");
        assert_eq!(kv_page_wedge_bound(100), Duration::from_secs(10), "the M5's 0.1 s: the floor");
        assert_eq!(kv_page_wedge_bound(6_835), Duration::from_millis(20_505), "the 5090's p90 × 3: a 10 s switch is a switch");
        assert!(kv_page_wedge_bound(6_835) > Duration::from_millis(10_004), "the max this box measured is inside the bound");
    }

    use super::*;
    use uuid::Uuid;

    // what this catches (M5, 2026-09-25): a page switch slower than its bound on an
    // engine whose queue was moving was abandoned as uncertain and cost every citizen a
    // 9-minute engine replacement. Busy must wait. And (Cormac, #4387) a switch stuck on
    // a queue that stopped moving must NOT wait forever just because the server answers:
    // the wait ends at the first checkpoint that sees no progress, or no answer.
    #[tokio::test(start_paused = true)]
    async fn a_slow_page_switch_waits_while_the_engine_progresses_and_ends_when_it_stalls() {
        let bound = std::time::Duration::from_secs(10);
        let slow = async {
            tokio::time::sleep(bound * 3 + std::time::Duration::from_secs(1)).await;
            "receipt"
        };
        let mut tick = 0u64;
        let mut seen = Vec::new();
        let got = wait_while_engine_progresses(
            slow,
            bound,
            || { tick += 1; let t = tick; async move { Some(t) } },
            |n| seen.push(n),
        )
        .await;
        assert_eq!(got, Some("receipt"), "a moving queue keeps the switch");
        assert_eq!(seen, vec![1, 2, 3], "each bound is a reported checkpoint, not a verdict");

        let stalled = wait_while_engine_progresses(
            std::future::pending::<&str>(),
            bound,
            || async { Some(42) },
            |_| {},
        )
        .await;
        assert_eq!(stalled, None, "an answering engine whose queue does not move ends the wait");

        let silent = wait_while_engine_progresses(
            std::future::pending::<&str>(),
            bound,
            || async { None },
            |_| {},
        )
        .await;
        assert_eq!(silent, None, "an engine that stops answering ends the wait");
    }

    // what this catches: the fingerprint moves when any slot prefills, decodes or takes
    // a new task, and a body that is not the /slots array reads as no answer.
    #[test]
    fn the_slots_fingerprint_moves_with_any_slot_work() {
        use crate::inference::llama_server::slots_progress_fingerprint as fp;
        let a = json!([{"id_task": 7, "n_prompt_tokens_processed": 100, "next_token": [{"n_decoded": 5}]},
                       {"id_task": 3, "n_prompt_tokens_processed": 0, "next_token": [{"n_decoded": 0}]}]);
        let decoded = json!([{"id_task": 7, "n_prompt_tokens_processed": 100, "next_token": [{"n_decoded": 6}]},
                             {"id_task": 3, "n_prompt_tokens_processed": 0, "next_token": [{"n_decoded": 0}]}]);
        let new_task = json!([{"id_task": 7, "n_prompt_tokens_processed": 100, "next_token": [{"n_decoded": 5}]},
                              {"id_task": 4, "n_prompt_tokens_processed": 0, "next_token": [{"n_decoded": 0}]}]);
        assert_ne!(fp(&a), fp(&decoded), "a decoded token is progress");
        assert_ne!(fp(&a), fp(&new_task), "a new task is progress");
        assert_eq!(fp(&a), fp(&a.clone()), "no work, no change");
        assert_eq!(fp(&json!({"error": "no slots"})), None, "not the slots array");
    }

    #[tokio::test]
    async fn anonymous_traffic_waits_for_the_scratch_slot_its_holder_owns() {
        // what this catches (M5, 2026-09-26 03:01Z): non-turn traffic wrote the scratch
        // slot without its permit, so the cache probe holding that slot was overwritten
        // between requests and the self-check held the fleet on a false `no_reuse`.
        let pool = Arc::new(KvSlotPool::new("test://scratch-owner", 3)); // index 2 is scratch
        let scratch = pool.scratch_slot().expect("three slots reserve a scratch slot"); // test: pool contract
        let held = pool.acquire_slot(scratch).await.expect("test: the probe holds scratch");
        let sem = Arc::new(Semaphore::new(4));
        let client = reqwest::Client::new();
        let waiting = admit_turn(&sem, None, Some(pool.clone()), &client, "test://scratch-owner", 0);
        tokio::pin!(waiting);
        assert!(
            futures::poll!(&mut waiting).is_pending(),
            "anonymous traffic entered the scratch slot while its holder owned it"
        );
        drop(held);
        let admitted = waiting.await.expect("test: admitted once the holder releases");
        assert_eq!(admitted.scratch_slot(), Some(scratch));
    }

    // what this catches: the restore-into-busy-slot bug at its source — while a turn
    // holds its admission (permit + pin), a second activity contending for the SAME
    // single slot must NOT be able to evict the pinned slot out from under it. Only
    // once the first admission drops does the slot become leasable again. This is the
    // invariant that makes the 90s restore wait unnecessary. No HTTP: a fresh key has
    // no page, so save/restore are skipped.
    #[tokio::test]
    async fn a_pinned_slot_survives_a_contending_lease_until_the_turn_drops() {
        let pool = Arc::new(KvSlotPool::new("test://admit", 1)); // ONE citizen slot
        let sem = Arc::new(Semaphore::new(1));
        let client = reqwest::Client::new();
        let a = ActivityKey::new(Uuid::from_u128(1), Uuid::from_u128(2)).unwrap();  // test: non-nil ids
        let b = ActivityKey::new(Uuid::from_u128(3), Uuid::from_u128(4)).unwrap();  // test: non-nil ids

        // A is admitted: holds the permit and pins the one slot.
        let adm_a = admit_turn(
            &sem,
            Some(a),
            Some(pool.clone()),
            &client,
            "test://admit",
            100,
        )
        .await
        .expect("test: endpoint admitted");
        let slot_a = adm_a.slot().expect("A leased the slot"); // test: a 1-slot pool leases to the first admission

        // B tries to lease while A is pinned — eviction must skip the pinned slot, so
        // B cannot take slot_a (the pool has no other slot to give).
        assert!(
            pool.lease(b).await.is_none(),
            "a pinned slot was handed to a contending activity — the exact restore-into-busy-slot bug"
        );

        // A's turn ends — the pin (and permit) release.
        drop(adm_a);

        // Now B can lease, and it gets the freed slot.
        let after_cancel = pool.lease_paged(b).await.expect("slot released after cancellation");
        assert_eq!(
            after_cancel.slot,
            slot_a,
            "after the turn dropped, the slot must become leasable again"
        );
        assert_eq!(
            after_cancel.save_first, None,
            "an admission dropped without successful generation must not be saved as activity A"
        );

        // The endpoint lease is shared across adapter semaphores. A different
        // adapter cannot enter while retirement drains this admitted turn.
        let adm = admit_turn(&sem, Some(b), Some(pool), &client, "test://admit", 100)
            .await
            .expect("test: second turn");
        let endpoint = crate::inference::slots::directory().endpoint("test://admit");
        let transition = endpoint.transition();
        tokio::pin!(transition);
        assert!(futures::poll!(&mut transition).is_pending());
        let other_adapter = Arc::new(Semaphore::new(1));
        assert!(
            admit_turn(&other_adapter, None, None, &client, "test://admit", 0)
                .await
                .is_err(),
            "another adapter cannot bypass endpoint suspension"
        );
        drop(adm);
        drop(transition.await);
        assert!(
            admit_turn(&other_adapter, None, None, &client, "test://admit", 0)
                .await
                .is_err(),
            "failed transition remains closed"
        );
    }

    // what this catches: warm-ahead at 4fe7e312 marked a lease warm even when
    // no restore occurred, failed, or was cancelled; later turns skipped restore.
    #[tokio::test]
    async fn warm_ahead_confirms_only_resident_or_successfully_restored_pages() {
        use axum::{
            extract::{Path, Query},
            http::StatusCode,
            routing::post,
            Json, Router,
        };
        use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

        let mode = Arc::new(AtomicU8::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let finished = Arc::new(tokio::sync::Notify::new());
        let app = Router::new().route(
            "/slots/{slot}",
            post({
                let mode = mode.clone();
                let calls = calls.clone();
                let entered = entered.clone();
                let release = release.clone();
                let finished = finished.clone();
                move |Path(slot): Path<u32>, Query(query): Query<std::collections::HashMap<String, String>>, Json(body): Json<serde_json::Value>| {
                    let mode = mode.clone();
                    let calls = calls.clone();
                    let entered = entered.clone();
                    let release = release.clone();
                    let finished = finished.clone();
                    async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        if mode.load(Ordering::SeqCst) == 1 || (mode.load(Ordering::SeqCst) == 7 && slot == 1) {
                            return (StatusCode::BAD_REQUEST, Json(json!({"error": {"code": 400, "type": "invalid_request_error", "message": "Unable to restore slot"}})));
                        }
                        if mode.load(Ordering::SeqCst) == 2 || (mode.load(Ordering::SeqCst) == 5 && slot == 1) {
                            entered.notify_one();
                            release.notified().await;
                            finished.notify_one();
                        }
                        if mode.load(Ordering::SeqCst) == 3 || (mode.load(Ordering::SeqCst) == 6 && slot == 1) {
                            return (StatusCode::OK, Json(json!({"unverified": true})));
                        }
                        let receipt_slot = if mode.load(Ordering::SeqCst) == 4 && slot == 1 { slot + 1 } else { slot };
                        let count = if query.get("action").map(String::as_str) == Some("save") { "n_saved" } else { "n_restored" };
                        (StatusCode::OK, Json(json!({"id_slot": receipt_slot, "filename": body["filename"], (count): 10})))
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("isolated paging HTTP fixture");
        let root = format!(
            "http://{}",
            listener.local_addr().expect("bound fixture address")
        );
        let server = tokio::spawn(async move { axum::serve(listener, app).await });
        let endpoint = crate::inference::slots::directory().endpoint(&root);
        let contract = crate::inference::slots::KvPageContract {
            model_id: "fixture".into(),
            model: "fixture.gguf".into(),
            adapters: vec![],
            page_dir: Some("fixture-pages".into()),
            context: 8192,
            slots: 3,
            cache_type: Some("q8_0".into()),
            engine: "fixture-engine".into(),
            revisions: Some(vec![(
                "fixture.gguf".into(),
                73,
                std::time::SystemTime::UNIX_EPOCH,
            )]),
        };
        let initial = endpoint.transition().await;
        let old = initial.start_generation().expect("initial generation");
        initial
            .ready(&root, &old, contract.clone())
            .expect("initial readiness");
        drop(initial);
        let pool = endpoint
            .admit()
            .await
            .expect("ready pool")
            .pool
            .expect("managed pool");
        let sem = Arc::new(Semaphore::new(2));
        let client = reqwest::Client::new();
        let key = ActivityKey::new(Uuid::new_v4(), Uuid::new_v4()).expect("fixture activity");

        // A discovered multi-slot pool is not sufficient: without reserved
        // scratch, anonymous traffic can still select a speculative restore slot.
        let small_root = format!("test://warm-two-slot-{}", Uuid::new_v4());
        let small = crate::inference::slots::directory().ensure_pool(&small_root, 2);
        small.note_saved(key);
        let stale_discovery = Arc::new(KvSlotPool::new(&small_root, 3));
        warm_ahead(&sem, key, stale_discovery, &client, &small_root)
            .await
            .expect("authoritative two-slot skip");
        let cold = small.lease_paged(key).await.expect("small pool lease");
        assert!(!cold.already_resident);
        assert!(
            cold.restore,
            "skipped warm-ahead must leave the page for the actual turn"
        );

        // A cold lease has no physical KV to attribute, and must issue no HTTP.
        warm_ahead(&sem, key, pool.clone(), &client, &root)
            .await
            .expect("cold warm-ahead");
        let slot = pool.lease(key).await.expect("assigned physical slot");
        assert!(pool.take_resident(slot).0.is_none());
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        pool.note_saved(key);
        mode.store(1, Ordering::SeqCst);
        warm_ahead(&sem, key, pool.clone(), &client, &root)
            .await
            .expect("failed restore is best effort");
        assert!(pool.take_resident(slot).0.is_none());
        let after_failure = pool.lease_paged(key).await.expect("cold after failure");
        assert!(!after_failure.restore, "failed page is no longer offered");
        assert!(!after_failure.already_resident);
        pool.forget_resident(slot, key);

        pool.note_saved(key);
        mode.store(2, Ordering::SeqCst);
        let mut queued = {
            let warming = warm_ahead(&sem, key, pool.clone(), &client, &root);
            tokio::pin!(warming);
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                tokio::select! {
                    result = &mut warming => panic!("restore completed before cancellation: {result:?}"),
                    _ = entered.notified() => {}
                }
            }).await.expect("restore request reached fixture");
            let mut queued = Box::pin(admit_turn(
                &sem,
                Some(key),
                Some(pool.clone()),
                &client,
                &root,
                100,
            ));
            assert!(futures::poll!(&mut queued).is_pending());
            queued
        };
        assert!(
            pool.take_resident(slot).0.is_none(),
            "cancelled restore cannot leave warm attribution"
        );
        assert!(
            queued.as_mut().await.is_err(),
            "already-admitted same-slot waiter must refuse after quarantine"
        );
        drop(queued);
        assert_eq!(sem.available_permits(), 2);
        assert!(
            warm_ahead(&sem, key, pool.clone(), &client, &root)
                .await
                .is_err(),
            "retry must refuse WHILE the old backend request is blocked"
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "quarantine sent no replacement request"
        );
        let replacement = endpoint.transition().await;
        assert!(
            replacement.ready(&root, &old, contract.clone()).is_err(),
            "same-generation readiness cannot clear quarantine"
        );
        assert!(
            replacement.start_generation().is_err(),
            "actual predecessor exit still required"
        );
        // This acknowledges only the fixture request; local Drop does not claim
        // that cancelling an HTTP client stops the production backend operation.
        release.notify_one();
        tokio::time::timeout(std::time::Duration::from_secs(5), finished.notified())
            .await
            .expect("fixture request drained");

        old.observed_exit();
        let new = replacement
            .start_generation()
            .expect("verified predecessor exit");
        replacement
            .ready(&root, &new, contract.clone())
            .expect("new verified generation");
        drop(replacement);
        let pool = endpoint
            .admit()
            .await
            .expect("reopened")
            .pool
            .expect("fresh pool");
        mode.store(0, Ordering::SeqCst);
        warm_ahead(&sem, key, pool.clone(), &client, &root)
            .await
            .expect("successful retry");
        let restored_calls = calls.load(Ordering::SeqCst);
        assert_eq!(restored_calls, 3);
        warm_ahead(&sem, key, pool.clone(), &client, &root)
            .await
            .expect("already resident");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            restored_calls,
            "confirmed residency skips redundant restore"
        );
        assert_eq!(pool.take_resident(slot).0, Some(key));

        // Same-key admission uses the same physical-slot permit even across
        // adapters. It cannot see or erase a prior owner's provisional holder.
        let mut turn = admit_turn(&sem, Some(key), Some(pool.clone()), &client, &root, 100)
            .await
            .expect("turn restore");
        let other_adapter = Arc::new(Semaphore::new(2));
        let warming = warm_ahead(&other_adapter, key, pool.clone(), &client, &root);
        tokio::pin!(warming);
        assert!(futures::poll!(&mut warming).is_pending());
        turn.generation_completed();
        drop(turn);
        tokio::time::timeout(std::time::Duration::from_secs(5), &mut warming)
            .await
            .expect("slot released")
            .expect("warm resident");
        assert_eq!(pool.take_resident(slot).0, Some(key));
        mode.store(3, Ordering::SeqCst);
        assert!(
            warm_ahead(&sem, key, pool.clone(), &client, &root)
                .await
                .is_err(),
            "a status-only or malformed acknowledgement is uncertain"
        );
        assert!(!endpoint.is_ready());
        assert!(pool.take_resident(slot).0.is_none());
        // What this catches: a drained checkpoint must confirm every resident,
        // never carry an old saved bit across a failed write or let cancellation
        // reopen a backend request whose completion is unknown.
        let other = ActivityKey::new(Uuid::new_v4(), Uuid::new_v4()).expect("second resident");
        let unknown_root = format!("test://checkpoint-unknown-{}", Uuid::new_v4());
        let unknown = crate::inference::slots::directory().endpoint(&unknown_root);
        let mut unowned = unknown.transition().await;
        assert!(unowned
            .checkpoint_residents(&client, &unknown_root)
            .await
            .is_err());
        drop(unowned);
        let mut prior = new;
        for behavior in [0, 7, 6, 4, 5] {
            let mut checkpointing = endpoint.transition().await;
            prior.observed_exit();
            let generation = checkpointing
                .start_generation()
                .expect("fixture child replacement");
            let before = calls.load(Ordering::SeqCst);
            assert!(
                checkpointing
                    .checkpoint_residents(&client, &root)
                    .await
                    .is_err(),
                "a new generation cannot checkpoint its predecessor's ledger"
            );
            checkpointing
                .ready(&root, &generation, contract.clone())
                .expect("verified replacement");
            let residents = crate::inference::slots::directory()
                .get(&root)
                .flatten()
                .expect("verified pool");
            residents.plan_paging(0, key);
            residents.plan_paging(1, other);
            residents.note_saved(key);
            residents.note_saved(other);
            assert!(checkpointing
                .checkpoint_residents(&client, &unknown_root)
                .await
                .is_err());
            assert_eq!(
                calls.load(Ordering::SeqCst),
                before,
                "identity refusals issue no HTTP"
            );
            mode.store(behavior, Ordering::SeqCst);
            if behavior == 5 {
                {
                    let saving = checkpointing.checkpoint_residents(&client, &root);
                    tokio::pin!(saving);
                    tokio::time::timeout(std::time::Duration::from_secs(5), async {
                        tokio::select! {
                            _ = &mut saving => panic!("checkpoint completed before fixture release"),
                            _ = entered.notified() => {}
                        }
                    }).await.expect("second save reached fixture");
                    assert!(
                        endpoint.admit().await.is_err(),
                        "writer keeps late turns closed"
                    );
                }
                assert!(
                    endpoint.paging_recovery_required(),
                    "cancelled save quarantines"
                );
                assert!(checkpointing
                    .ready(&root, &generation, contract.clone())
                    .is_err());
                let refused_at = calls.load(Ordering::SeqCst);
                assert!(checkpointing
                    .checkpoint_residents(&client, &root)
                    .await
                    .is_err());
                assert_eq!(calls.load(Ordering::SeqCst), refused_at);
                release.notify_one();
                tokio::time::timeout(std::time::Duration::from_secs(5), finished.notified())
                    .await
                    .expect("cancelled fixture drained");
            } else if behavior == 0 {
                let receipt = checkpointing
                    .checkpoint_residents(&client, &root)
                    .await
                    .expect("both residents saved");
                assert!(
                    receipt.transition_for(&prior).is_err(),
                    "receipt refuses predecessor"
                );
                assert!(receipt.transition_for(&generation).is_ok());
                assert!(
                    endpoint.admit().await.is_err(),
                    "receipt retains closed writer"
                );
                generation.observed_exit();
                assert!(
                    receipt.transition_for(&generation).is_err(),
                    "exit invalidates receipt"
                );
                drop(receipt);
            } else {
                assert!(checkpointing
                    .checkpoint_residents(&client, &root)
                    .await
                    .is_err());
                assert_eq!(
                    endpoint.paging_recovery_required(),
                    behavior != 7,
                    "only terminal rejection proves completion"
                );
                assert!(
                    !generation.has_exited(),
                    "save refusal never retires the child"
                );
            }
            assert_eq!(
                calls.load(Ordering::SeqCst),
                before + 2,
                "only attributed slots are saved"
            );
            residents.take_resident(0);
            assert!(
                residents.plan_paging(0, key).restore,
                "confirmed first save survives partial failure"
            );
            residents.take_resident(1);
            assert_eq!(
                residents.plan_paging(1, other).restore,
                behavior == 0,
                "failed or unconfirmed overwrite invalidates previous saved eligibility"
            );
            drop(checkpointing);
            assert!(
                !endpoint.is_ready(),
                "dropping writer never silently reopens"
            );
            prior = generation;
        }
        server.abort();
    }
}
