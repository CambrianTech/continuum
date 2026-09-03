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
//!    restore lands (~0.1s) instead of deferring behind a live decode.
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
/// permit and the slot pin (RAII) — never before the generation completes, because
/// the caller binds it at function scope alongside the streamed response.
pub struct TurnAdmission {
    /// The slot to pin the request to (`id_slot`), or `None` to stay unpinned
    /// (non-Turn traffic, or a Turn whose activity could not be leased).
    slot: Option<u32>,
    /// The concurrency permit — held so no more than `lanes` turns decode at once.
    _permit: OwnedSemaphorePermit,
    /// The slot pin — held so eviction cannot reassign this turn's slot mid-decode.
    _pin: Option<SlotPin>,
}

impl TurnAdmission {
    /// The leased slot this turn should pin `id_slot` to, if any.
    pub fn slot(&self) -> Option<u32> {
        self.slot
    }
}

/// Admit a turn: acquire the concurrency permit (always), then — for a Turn with a
/// resolvable `(persona, room)` key and a live slot pool — lease + pin its slot and
/// page its KV (save the evictee, restore this activity). Returns the guard the
/// caller must hold across the generation.
///
/// `key`/`pool` are `None` for non-Turn traffic (or a cloud provider with no slots):
/// the caller still gets the permit, `slot()` is `None`, and it places the request
/// on scratch (or unpinned) itself.
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

    let mut slot = None;
    let mut _pin = None;

    if let (Some(k), Some(pool)) = (key, pool) {
        if let Some(pg) = pool.lease_paged(k).await {
            // 2. PIN synchronously, before any await below — a permit-holding
            //    returner leases only an UNPINNED slot, so pinning here (there are
            //    at most lanes-1 other pinned slots while we hold a permit) makes the
            //    slot we just leased un-evictable for the turn. No await between the
            //    lease returning and this pin, so no other task can slip in.
            _pin = pool.pin(&k);

            // 3. The context switch onto a now-free slot: page the evictee out,
            //    page this activity in. Lands immediately (no defer) because the slot
            //    is not decoding.
            if let Some(prev) = pg.save_first {
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
            slot = Some(pg.slot);
        }
    }

    TurnAdmission { slot, _permit, _pin }
}

/// Execute one KV page action against the server (`/slots/{id}?action=save|restore`).
///
/// The 10s cap is a WEDGE DETECTOR, not a wait-for-the-slot: with permit-first + pin
/// admission ([`admit_turn`]) a restore is only ever issued into an already-free
/// slot, so it lands in ~0.1s and never defers behind a live decode. A restore
/// taking >10s now means a wedged server. (History: pre-fix this path saw 27/27
/// restores fail `status=0`, then 55/67 with a 90s wait; the pin removes the failure
/// mode instead of waiting it out, so the wait is gone.)
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
    let started = std::time::Instant::now();
    let resp = client
        .post(&url)
        .timeout(std::time::Duration::from_secs(10))
        .json(&json!({ "filename": filename }))
        .send()
        .await;
    let ok = matches!(&resp, Ok(r) if r.status().is_success());
    let status = resp.as_ref().map(|r| r.status().as_u16()).unwrap_or(0); // 0 = transport error
    crate::probe!(
        class = "inference.kv_page.action",
        action = %action,
        slot = slot as u64,
        persona = %key.persona,
        room = %key.room,
        ok,
        status = status as u64,
        ms = started.elapsed().as_millis() as u64,
        "KV page context switch — save pages the evictee's state out, restore pages \
         the returner's state in (~0.1s measured; a miss means this turn re-prefills)",
    );
    ok
}

#[cfg(test)]
mod tests {
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
        assert_eq!(
            pool.lease(b).await,
            Some(slot_a),
            "after the turn dropped, the slot must become leasable again"
        );
    }
}
