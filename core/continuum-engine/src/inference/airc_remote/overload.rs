//! A seat that refused a mind as overloaded is not asked again until it reopens
//! (card 03660a77).
//!
//! IntelMac, 2026-10-23: the M5 refused Paige at once ("2 generations outstanding for 2
//! served lanes") and she asked it 13 more times. Her placement switch moves her home
//! before her next act when it can; when this node has no home lane for her, the lane
//! itself must not ask the full seat again until the seat says it has room.

use uuid::Uuid;

use crate::capacity::gossip::{GridCapacityLedger, FRESHNESS_WINDOW_MS};

/// PURE over the ledger: has `seat` reopened since it refused this mind at `refused_at_ms`?
/// Only a beacon HEARD after the refusal, still fresh at `now_ms` ([`FRESHNESS_WINDOW_MS`],
/// the ledger's one freshness rule), that offers a free slot. All three times are this
/// node's clock (the ledger stamps when it heard a beacon), so no cross-node skew enters.
pub(super) fn seat_reopened(
    ledger: &GridCapacityLedger,
    seat: Uuid,
    refused_at_ms: u64,
    now_ms: u64,
) -> bool {
    ledger
        .foreign_offers_with_age()
        .into_iter()
        .any(|(peer, offer, heard_at_ms)| {
            peer == seat
                && heard_at_ms > refused_at_ms
                && now_ms.saturating_sub(heard_at_ms) <= FRESHNESS_WINDOW_MS
                && offer.free_slots_live > 0
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capacity::gossip::CapacityOffer;

    fn beacon(free_slots_live: u32, at_ms: u64) -> CapacityOffer {
        serde_json::from_value(serde_json::json!({
            "gpuTotalBytes": 1, "gpuFreeBytesLive": 0, "systemRamFreeBytes": 0,
            "atMs": at_ms, "freeSlotsLive": free_slots_live,
        }))
        .expect("decodes") // boundary: the wire shape of a seat's beacon
    }

    // what this catches (Codex review of d6c9592ad): the ledger keeps stale rows on
    // purpose, so a free-slot beacon heard after the refusal can be hours old by her next
    // act. It reopens the seat only while fresh; a beacon from before the refusal, or one
    // offering no slot, never does.
    #[test]
    fn only_a_fresh_post_refusal_beacon_with_a_free_slot_reopens_the_seat() {
        let seat = Uuid::new_v4();
        let refused = 1_000_000;
        let ledger = GridCapacityLedger::default();
        ledger.hear(seat, beacon(1, refused - 1), refused - 1);
        assert!(!seat_reopened(&ledger, seat, refused, refused + 1), "heard before the refusal");
        ledger.hear(seat, beacon(0, refused + 1), refused + 1);
        assert!(!seat_reopened(&ledger, seat, refused, refused + 2), "no free slot");
        ledger.hear(seat, beacon(1, refused + 2), refused + 2);
        assert!(seat_reopened(&ledger, seat, refused, refused + 3));
        assert!(
            !seat_reopened(&ledger, seat, refused, refused + 2 + FRESHNESS_WINDOW_MS + 1),
            "the same beacon, gone stale by her next act"
        );
        assert!(!seat_reopened(&ledger, Uuid::new_v4(), refused, refused + 3), "another seat");
    }
}
