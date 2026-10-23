//! A remote request's deadline from the serving seat's own measured service time
//! (card b3e5b582). The fixed floor is sized for a fast seat: on 2026-10-23 IntelMac's
//! Iris asked the M5's 27B for a 2,520-token reply, gave up at the 600 s floor, and the
//! seat finished at 655 s, generating for no one. The seat beacons how long it takes;
//! the requester reads that instead of guessing.

use std::time::Duration;

use uuid::Uuid;

use crate::capacity::gossip::{global_ledger, CapacityOffer, FRESHNESS_WINDOW_MS};
use crate::inference::turn_bound::{BoundSource, ACT_CEILING};

/// PURE: how long `offer`'s seat is measured to take for a request that may decode up
/// to `max_tokens`: its median lane wait plus that decode at its measured rate. `None`
/// when the beacon measures neither (zero samples or a zero rate is UNMEASURED, never
/// fast). Prefill is not beaconed, so this is a lower bound on service time; the
/// transport's floor still covers prefill.
pub(super) fn seat_service_estimate(
    offer: &CapacityOffer,
    max_tokens: Option<u32>,
) -> Option<Duration> {
    let wait = (offer.lane_wait_samples > 0).then(|| Duration::from_millis(offer.lane_wait_p50_ms));
    let decode = match max_tokens {
        Some(tokens) if tokens > 0 && offer.decode_tps_milli > 0 => Some(Duration::from_millis(
            u64::from(tokens) * 1_000_000 / u64::from(offer.decode_tps_milli),
        )),
        _ => None,
    };
    // Each measured part counts; an unmeasured part adds nothing rather than a guess.
    match (wait, decode) {
        (None, None) => None,
        (Some(part), None) | (None, Some(part)) => Some(part),
        (Some(wait), Some(decode)) => Some(wait + decode),
    }
}

/// PURE: the deadline after the seat's estimate is considered. The estimate only RAISES
/// the bound already in force (floor or turn bound), and never past [`ACT_CEILING`]: a
/// wait the owning act would reap anyway buys nothing.
pub(super) fn raise_to_seat(
    current: (Duration, BoundSource),
    estimate: Option<Duration>,
) -> (Duration, BoundSource) {
    match estimate.map(|e| e.min(ACT_CEILING)) {
        Some(seat) if seat > current.0 => (seat, BoundSource::SeatMeasured),
        _ => current,
    }
}

/// The seat's beacon as last heard, if it is still fresh ([`FRESHNESS_WINDOW_MS`]). A
/// stale or missing beacon contributes nothing: the deadline falls back to today's rule.
pub(super) fn fresh_seat_offer(seat: Uuid, now_ms: u64) -> Option<CapacityOffer> {
    global_ledger()
        .foreign_offers_with_age()
        .into_iter()
        .find(|(peer, _, heard_at_ms)| {
            *peer == seat && now_ms.saturating_sub(*heard_at_ms) <= FRESHNESS_WINDOW_MS
        })
        .map(|(_, offer, _)| offer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn beacon(
        lane_wait_p50_ms: u64,
        lane_wait_samples: u32,
        decode_tps_milli: u32,
    ) -> CapacityOffer {
        serde_json::from_value(serde_json::json!({
            "gpuTotalBytes": 1u64, "gpuFreeBytesLive": 0u64, "systemRamFreeBytes": 0u64, "atMs": 0u64,
            "laneWaitP50Ms": lane_wait_p50_ms, "laneWaitSamples": lane_wait_samples,
            "decodeTpsMilli": decode_tps_milli,
        }))
        .expect("the wire shape of a seat's beacon")
    }

    const FLOOR: (Duration, BoundSource) = (Duration::from_secs(600), BoundSource::Floor);

    // what this catches (card b3e5b582, IntelMac Iris 2026-10-23): a slow seat's measured
    // decode raises the deadline past the 600 s floor, so the requester stays for the
    // answer the seat is producing instead of abandoning it at 600 s.
    #[test]
    fn a_slow_seat_raises_the_deadline_by_its_own_measurement() {
        // 2,520 tokens at 3.85 tok/s ~= 654.5 s of decode, plus a 60 s median queue.
        let estimate = seat_service_estimate(&beacon(60_000, 12, 3_850), Some(2_520));
        assert_eq!(estimate, Some(Duration::from_millis(60_000 + 654_545)));
        assert_eq!(
            raise_to_seat(FLOOR, estimate),
            (Duration::from_millis(714_545), BoundSource::SeatMeasured)
        );
    }

    // what this catches: absence is unmeasured, never fast and never a guess. A beacon
    // with no samples and no rate leaves today's deadline exactly as it was, and a fast
    // seat never LOWERS a floor or turn bound.
    #[test]
    fn an_unmeasured_or_fast_seat_leaves_the_deadline_alone() {
        assert_eq!(
            seat_service_estimate(&beacon(60_000, 0, 0), Some(2_520)),
            None
        );
        assert_eq!(seat_service_estimate(&beacon(0, 0, 3_850), None), None);
        assert_eq!(raise_to_seat(FLOOR, None), FLOOR);
        let fast = seat_service_estimate(&beacon(1_000, 5, 40_000), Some(256));
        assert_eq!(raise_to_seat(FLOOR, fast), FLOOR);
        let turn = (Duration::from_secs(900), BoundSource::TurnBound);
        assert_eq!(raise_to_seat(turn, Some(Duration::from_secs(700))), turn);
    }

    // what this catches: a seat measured as glacially slow cannot hold a request past the
    // owning act's ceiling; the act would reap it anyway.
    #[test]
    fn the_seat_estimate_is_capped_at_the_act_ceiling() {
        let glacial = seat_service_estimate(&beacon(0, 0, 1), Some(32_768));
        assert_eq!(
            raise_to_seat(FLOOR, glacial),
            (ACT_CEILING, BoundSource::SeatMeasured)
        );
    }
}
