//! A remote request's deadline from the serving seat's own measured service time
//! (card b3e5b582). The fixed floor is sized for a fast seat: on 2026-10-23 IntelMac's
//! Iris asked the M5's 27B for a 2,520-token reply, gave up at the 600 s floor, and the
//! seat finished at 655 s, generating for no one. The seat beacons how long it takes;
//! the requester reads that instead of guessing.

use std::time::Duration;

use uuid::Uuid;

use crate::capacity::gossip::{
    global_ledger, CapacityOffer, GridCapacityLedger, FRESHNESS_WINDOW_MS,
};
use crate::inference::turn_bound::{from_expectation, BoundSource, ACT_CEILING};

/// PURE: how long `offer`'s seat is measured to take for a request that may decode up
/// to `max_tokens`: its median lane wait plus that decode at its measured rate. `None`
/// when the beacon measures neither (zero samples or a zero rate is UNMEASURED, never
/// fast). Prefill is not beaconed, so this is a lower bound on service time; once it
/// governs, prefill rides only the variance headroom of [`raise_to_seat`] (unmeasured,
/// no additive guarantee), not the floor.
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

/// PURE: the deadline after the seat's estimate is considered. The estimate is a MEDIAN
/// shape, so it gets the same variance headroom every measured expectation gets
/// ([`from_expectation`], x[`crate::inference::turn_bound::TURN_BOUND_HEADROOM`]): a
/// request slightly slower than the median is healthy, not late. The result only RAISES
/// the bound already in force (floor or turn bound) and is capped at [`ACT_CEILING`].
pub(super) fn raise_to_seat(
    current: (Duration, BoundSource),
    estimate: Option<Duration>,
) -> (Duration, BoundSource) {
    match from_expectation(estimate).map(|bound| bound.min(ACT_CEILING)) {
        Some(seat) if seat > current.0 => (seat, BoundSource::SeatMeasured),
        _ => current,
    }
}

/// The seat's beacon as last heard, if it is still fresh ([`FRESHNESS_WINDOW_MS`]) and
/// measures the model this request runs. A stale or missing beacon, or one serving a
/// different model than the request names, contributes nothing: the deadline falls back
/// to today's rule. (The remote adapter has already rewritten `model` to the lane's
/// served model, `adapter.rs` `remote_lane.model_overridden`, so a match is the norm.)
pub(super) fn fresh_seat_offer(
    ledger: &GridCapacityLedger,
    seat: Uuid,
    model: Option<&str>,
    now_ms: u64,
) -> Option<CapacityOffer> {
    ledger
        .foreign_offers_with_age()
        .into_iter()
        .find(|(peer, _, heard_at_ms)| {
            *peer == seat && now_ms.saturating_sub(*heard_at_ms) <= FRESHNESS_WINDOW_MS
        })
        .map(|(_, offer, _)| offer)
        .filter(|offer| model.is_none() || offer.served_model.as_deref() == model)
}

/// [`fresh_seat_offer`] over this process's ledger.
pub(super) fn heard_seat_offer(
    seat: Uuid,
    model: Option<&str>,
    now_ms: u64,
) -> Option<CapacityOffer> {
    fresh_seat_offer(global_ledger(), seat, model, now_ms)
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
            "decodeTpsMilli": decode_tps_milli, "servedModel": "ggml-org/Qwen3.8-27B-GGUF",
        }))
        .expect("the wire shape of a seat's beacon")
    }

    const FLOOR: (Duration, BoundSource) = (Duration::from_secs(600), BoundSource::Floor);
    const MODEL: Option<&str> = Some("ggml-org/Qwen3.8-27B-GGUF");

    // what this catches (card b3e5b582, IntelMac Iris 2026-10-23): a slow seat's measured
    // service raises the deadline past the 600 s floor WITH variance headroom, so a request
    // slightly slower than the seat's median (a 61 s wait against a 60 s median, the same
    // 654.5 s decode) is still waited for instead of abandoned at the median itself.
    #[test]
    fn a_slow_seat_raises_the_deadline_with_headroom_over_its_median() {
        // 2,520 tokens at 3.85 tok/s ~= 654.5 s of decode, plus a 60 s median queue.
        let estimate = seat_service_estimate(&beacon(60_000, 12, 3_850), Some(2_520));
        assert_eq!(estimate, Some(Duration::from_millis(60_000 + 654_545)));
        let (deadline, source) = raise_to_seat(FLOOR, estimate);
        assert_eq!(
            (deadline, source),
            (
                Duration::from_millis(2 * 714_545),
                BoundSource::SeatMeasured
            )
        );
        let slightly_slow = Duration::from_millis(61_000 + 654_545);
        assert!(
            slightly_slow < deadline,
            "a request just over the median is still covered"
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
        // 400 s x2 = 800 s, below a 900 s turn bound: the turn bound stands.
        let turn = (Duration::from_secs(900), BoundSource::TurnBound);
        assert_eq!(raise_to_seat(turn, Some(Duration::from_secs(400))), turn);
    }

    // what this catches: a seat measured as glacially slow raises the deadline only to the
    // seat-raise cap, never without bound.
    #[test]
    fn the_seat_raise_is_capped_at_the_act_ceiling() {
        let glacial = seat_service_estimate(&beacon(0, 0, 1), Some(32_768));
        assert_eq!(
            raise_to_seat(FLOOR, glacial),
            (ACT_CEILING, BoundSource::SeatMeasured)
        );
    }

    // what this catches: only a FRESH beacon from THIS seat for THIS model counts. A missing
    // peer, a beacon one ms past the freshness window, or one measuring a different model
    // contributes nothing, so the deadline falls back to today's rule instead of reading
    // someone else's (or an old) speed.
    #[test]
    fn only_a_fresh_beacon_from_the_seat_for_the_requested_model_counts() {
        let ledger = GridCapacityLedger::default();
        let seat = Uuid::from_u128(0x5ea7);
        let now = 10_000_000;
        assert!(
            fresh_seat_offer(&ledger, seat, MODEL, now).is_none(),
            "no beacon, no estimate"
        );

        ledger.hear(seat, beacon(60_000, 12, 3_850), now - FRESHNESS_WINDOW_MS);
        assert!(
            fresh_seat_offer(&ledger, seat, MODEL, now).is_some(),
            "at the window edge: fresh"
        );
        assert!(
            fresh_seat_offer(&ledger, seat, MODEL, now + 1).is_none(),
            "one ms past: stale"
        );
        assert!(
            fresh_seat_offer(&ledger, Uuid::from_u128(0xbeef), MODEL, now).is_none(),
            "another seat's beacon is not this seat's"
        );
        assert!(
            fresh_seat_offer(&ledger, seat, Some("continuum-ai/qwen2.5-coder-1.5b"), now).is_none(),
            "a beacon measuring a different model is not this request's speed"
        );
        assert!(
            fresh_seat_offer(&ledger, seat, None, now).is_some(),
            "an unnamed model runs the served one"
        );
    }
}
