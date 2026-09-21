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
    /// Released after the pin so the next permit holder can lease the free slot.
    _permit: OwnedSemaphorePermit,
    /// A lease is provisional until the adapter observes successful generation.
    /// Cancellation during paging or generation must not save foreign KV later.
    uncommitted: Option<(Arc<KvSlotPool>, ActivityKey, u32)>,
}

impl TurnAdmission {
    /// The leased slot this turn should pin `id_slot` to, if any.
    pub fn slot(&self) -> Option<u32> {
        self.slot
    }

    /// The adapter received a complete successful generation in this slot.
    pub(crate) fn generation_completed(&mut self) {
        self.uncommitted = None;
    }
}

impl Drop for TurnAdmission {
    fn drop(&mut self) {
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
) -> TurnAdmission {
    // 1. PERMIT FIRST — event-driven wait for a free lane. Cannot fail: the
    //    semaphore is never closed over the adapter's lifetime.
    let _permit = concurrency
        .clone()
        .acquire_owned()
        .await
        .expect("adapter semaphore never closed");  // expect: the semaphore lives as long as the adapter, never closed

    let mut admission = TurnAdmission {
        slot: None,
        _pin: None,
        _permit,
        uncommitted: None,
    };

    if let (Some(k), Some(pool)) = (key, pool.as_ref()) {
        if let Some(pg) = pool.lease_paged(k).await {
            // 2. PIN synchronously, before any await below — a permit-holding
            //    returner leases only an UNPINNED slot, so pinning here (there are
            //    at most lanes-1 other pinned slots while we hold a permit) makes the
            //    slot we just leased un-evictable for the turn. No await between the
            //    lease returning and this pin, so no other task can slip in.
            admission._pin = pool.pin(&k);
            admission.uncommitted = Some((Arc::clone(pool), k, pg.slot));

            // 3. The context switch onto a now-free slot: page the evictee out,
            //    page this activity in. Lands immediately (no defer) because the slot
            //    is not decoding.
            if let Some(prev) = pg.save_first {
                // A failed or cancelled save may have replaced an older file.
                // Only a completed save can make this page restorable again.
                pool.note_page_lost(&prev);
                if kv_page_action(client, root, pg.slot, &prev, "save").await {
                    pool.note_saved(prev);
                }
            }
            if pg.restore && !kv_page_action(client, root, pg.slot, &k, "restore").await {
                // Dead page (geometry swept / file missing): stop offering it; this
                // turn re-prefills plainly.
                pool.note_page_lost(&k);
            }
            // Price basis for the eviction policy (B5): this activity's current
            // prompt size — comparable across slots, which is all eviction needs.
            pool.note_tail(&k, approx_tokens);
            admission.slot = Some(pg.slot);
        }
    } else if let Some(pool) = pool.as_ref().filter(|pool| pool.n_slots() == 1) {
        // A single-slot server has no separate scratch slot. Preserve the
        // resident before background/anonymous traffic borrows its physical slot.
        // Remove attribution BEFORE the await so cancellation cannot leave the
        // next activity treating transient KV as its own warm tail.
        let (previous, pin) = pool.take_resident(0);
        admission._pin = pin;
        if let Some(previous) = previous {
            pool.note_page_lost(&previous);
            if kv_page_action(client, root, 0, &previous, "save").await {
                pool.note_saved(previous);
            }
        }
        admission.slot = Some(0);
    }

    admission
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

/// The bound past which a page switch is a WEDGE, not a slow switch: the floor, raised
/// to three times this node's measured p90. Pure.
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

/// Execute one KV page action against the server (`/slots/{id}?action=save|restore`).
///
/// The bound is a WEDGE DETECTOR, not a wait-for-the-slot: with permit-first + pin
/// admission ([`admit_turn`]) a restore is only ever issued into an already-free
/// slot and never defers behind a live decode; past the bound the server is wedged.
/// (History: pre-fix this path saw 27/27 restores fail `status=0`, then 55/67 with a
/// 90s wait; the pin removes the failure mode instead of waiting it out.)
///
/// `pub(crate)` so the spawned warm-ahead task drives the SAME seam as admission.
pub(crate) async fn kv_page_action(
    client: &reqwest::Client,
    root: &str,
    slot: u32,
    key: &ActivityKey,
    action: &str,
) -> bool {
    let url = format!("{}/slots/{}?action={}", root.trim_end_matches('/'), slot, action);
    let filename = crate::inference::slots::page_filename(key);
    let (p50_ms, p90_ms, measured) = KV_PAGE_SWITCH_MS.p50_p90();
    let bound = kv_page_wedge_bound(p90_ms);
    let started = std::time::Instant::now();
    let resp = client
        .post(&url)
        .timeout(bound)
        .json(&json!({ "filename": filename }))
        .send()
        .await;
    let ok = matches!(&resp, Ok(r) if r.status().is_success());
    let status = resp.as_ref().map(|r| r.status().as_u16()).unwrap_or(0); // 0 = transport error
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
        status = status as u64,
        ms,
        bound_ms = bound.as_millis() as u64,
        node_p50_ms = p50_ms,
        node_p90_ms = p90_ms,
        node_measured = measured as u64,
        "KV page context switch — save pages the evictee's state out, restore pages \
         the returner's state in; `ms` is what it cost HERE (a miss means this turn re-prefills)",
    );
    ok
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
        let adm_a = admit_turn(&sem, Some(a), Some(pool.clone()), &client, "test://admit", 100).await;
        let slot_a = adm_a.slot().expect("A leased the slot");  // test: a 1-slot pool leases to the first admission

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
        assert_eq!(after_cancel.save_first, None,
            "an admission dropped without successful generation must not be saved as activity A");
    }
}
