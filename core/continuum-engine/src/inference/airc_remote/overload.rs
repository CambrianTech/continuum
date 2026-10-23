//! A seat that refused a mind as overloaded is not asked again until it reopens
//! (card 03660a77).
//!
//! IntelMac, 2026-10-23: the M5 refused Paige at once ("2 generations outstanding for 2
//! served lanes") and she asked it 13 more times. Her placement switch moves her home
//! before her next act when it can; when this node has no home lane for her, the lane
//! itself must not ask the full seat again until the seat says it has room.

use uuid::Uuid;

use crate::capacity::gossip::GridCapacityLedger;

/// PURE over the ledger: has `seat` reopened since it refused this mind at `refused_at_ms`?
/// Only a beacon HEARD after the refusal that offers a free slot. Both times are this
/// node's clock (the ledger stamps when it heard a beacon), so no cross-node skew enters.
pub(super) fn seat_reopened(ledger: &GridCapacityLedger, seat: Uuid, refused_at_ms: u64) -> bool {
    ledger
        .foreign_offers_with_age()
        .into_iter()
        .any(|(peer, offer, heard_at_ms)| {
            peer == seat && heard_at_ms > refused_at_ms && offer.free_slots_live > 0
        })
}
