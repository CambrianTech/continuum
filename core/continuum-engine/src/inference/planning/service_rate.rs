//! WHAT A NODE CAN SERVE, measured (card df58b8b8).
//!
//! Joel, 2026-09-27: "Don't dumb our systems and coders down if the system could handle one
//! of these at a decent enough speed." The M5 that day served Qwen3.8-27B to 12 residents on
//! 6 lanes because it was the most capable model that FIT: an act took ~1000 s, ~850 s of it
//! in the model, and the hour wrote nothing. Ornith-1.5-35B-A3B sat on the same disk. Fit is
//! memory; serving is rate. This module is the rate side, pure.
//!
//! "Decent" is ABSOLUTE, and it is the two lines already in tree, not a new one:
//! - decode: each stream at or above [`DECODE_FLOOR_TPS`], the rate below which a stream is
//!   slower than a person reads (the decode knee's floor);
//! - prefill: the `n` in flight share the server's measured prefill rate, and each turn's
//!   prefill must land inside the audience's time-to-first-token budget
//!   ([`crate::inference::prefill_rate::Audience::ttft`], the prefill knee's bound).
//!
//! A relative bar (a multiple of the fastest candidate) was the first design and is wrong: an
//! A3B MoE decodes 3-5x faster than a dense 27B on ANY box, so it would drop the 27B on the
//! 5090 too, which is exactly the dumbing down the rule forbids.
//!
//! Measured or unknown, never guessed: a model without a prefill rate or any decode point has
//! no verdict ([`Decent::Unmeasured`]). It is never chosen blind, and never judged on an
//! absence.

use crate::inference::decode_knee::{DECODE_FLOOR_TPS, MIN_KNEE_LANES};
use std::collections::BTreeMap;
use std::time::Duration;

/// The work one generation costs on this node, measured and model-independent: the prompt
/// tokens the lane had to prefill (cache hits excluded) and the tokens it decoded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Workload {
    pub prefill_tokens: f64,
    pub decode_tokens: f64,
}

/// One model's measured rates on this node.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelRates {
    /// The SERVER-TOTAL prefill rate (tok/s), shared by every slot. `None` = unmeasured.
    pub prefill_tps: Option<f64>,
    /// Per-stream decode tok/s by in-flight count.
    pub decode_per_stream: BTreeMap<u32, f64>,
}

impl ModelRates {
    /// Per-stream decode at `n` in flight: the measured point, else from the nearest
    /// measured point below by the memory-bound physics (the aggregate holds roughly
    /// constant, so per-stream scales by k/n), else the nearest above (fewer streams decode
    /// no slower). `None` = no decode point at all.
    pub fn decode_at(&self, n: u32) -> Option<f64> {
        let n = n.max(1);
        if let Some(&tps) = self.decode_per_stream.get(&n) {
            return Some(tps);
        }
        if let Some((&k, &tps)) = self.decode_per_stream.range(..n).next_back() {
            return Some(tps * f64::from(k) / f64::from(n));
        }
        self.decode_per_stream.range(n..).next().map(|(_, &tps)| tps)
    }

    fn prefill(&self) -> Option<f64> {
        self.prefill_tps.filter(|r| r.is_finite() && *r > 0.0)
    }
}

/// Whether a model serves `n` minds decently on this node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Decent {
    Yes,
    /// Each stream would decode below the floor.
    DecodeTooSlow { per_stream_tps: f64 },
    /// The turn's share of prefill would overrun the budget.
    PrefillTooSlow { secs: f64 },
    /// A rate is missing.
    Unmeasured,
}

/// PURE: is `rates` decent for `n` in flight on `work`, inside `ttft`?
pub fn decent(rates: &ModelRates, work: Workload, n: u32, ttft: Duration) -> Decent {
    let n = n.max(1);
    let (Some(prefill), Some(decode)) = (rates.prefill(), rates.decode_at(n).filter(|r| r.is_finite() && *r > 0.0)) else {
        return Decent::Unmeasured;
    };
    if decode < DECODE_FLOOR_TPS {
        return Decent::DecodeTooSlow { per_stream_tps: decode };
    }
    let secs = f64::from(n) * work.prefill_tokens / prefill;
    if secs > ttft.as_secs_f64() {
        return Decent::PrefillTooSlow { secs };
    }
    Decent::Yes
}

/// PURE: the most minds `rates` serves decently (at most `cap`), or `None` when it is not
/// decent even for one, or unmeasured. This is the node's seat count for that model: the
/// measured replacement for a constant number of minds per lane.
pub fn seats(rates: &ModelRates, work: Workload, cap: u32, ttft: Duration) -> Option<u32> {
    (1..=cap.max(1)).rev().find(|&n| decent(rates, work, n, ttft) == Decent::Yes)
}

/// PURE: whether the serving model must give way to `challenger`. Only when the incumbent is
/// not decent even at [`MIN_KNEE_LANES`] (measured, not missing) while the challenger is.
/// Anywhere between, the incumbent holds, so a roster that rests and wakes cannot flap the
/// base (a switch relaunches the lane and every slot re-prefills cold).
pub fn incumbent_displaced(incumbent: &ModelRates, challenger: &ModelRates, work: Workload, ttft: Duration) -> bool {
    let at = MIN_KNEE_LANES;
    !matches!(decent(incumbent, work, at, ttft), Decent::Yes | Decent::Unmeasured)
        && decent(challenger, work, at, ttft) == Decent::Yes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rates(prefill: f64, decode: &[(u32, f64)]) -> ModelRates {
        ModelRates { prefill_tps: Some(prefill), decode_per_stream: decode.iter().copied().collect() }
    }

    const TTFT: Duration = Duration::from_secs(180);
    /// The M5, 2026-09-27: ~5,600 prefilled and ~1,000 decoded tokens a generation.
    const M5: Workload = Workload { prefill_tokens: 5_600.0, decode_tokens: 1_000.0 };

    // what this catches (card df58b8b8): the planner crowning the model that FITS over the one
    // that SERVES, and the opposite failure, dumbing a box down to the fastest model when the
    // capable one is decent. On the M5's own curves the 27B decodes 6.7 t/s even alone: not
    // decent for anyone, so it gives way to Ornith, which seats five. On a box where the 27B
    // decodes 40 t/s it holds, even though Ornith would be faster still.
    #[test]
    fn the_capable_model_holds_wherever_it_is_decent_and_gives_way_only_where_it_is_not() {
        let qwen_m5 = rates(108.0, &[(1, 6.7), (2, 5.2), (3, 3.5), (4, 2.4), (5, 2.2), (6, 1.7)]);
        let ornith_m5 = rates(576.0, &[(1, 32.2), (2, 28.1)]);
        assert_eq!(seats(&qwen_m5, M5, 8, TTFT), None, "not decent even for one mind");
        assert!(incumbent_displaced(&qwen_m5, &ornith_m5, M5, TTFT));
        // 28.1 x 2 / n >= 10 holds to n = 5; prefill 5 x 5600 / 576 = 49 s is inside the budget
        assert_eq!(seats(&ornith_m5, M5, 8, TTFT), Some(5));

        let qwen_5090 = rates(2_500.0, &[(1, 55.0), (2, 45.0), (4, 30.0)]);
        let ornith_5090 = rates(6_000.0, &[(1, 160.0), (2, 150.0)]);
        assert!(!incumbent_displaced(&qwen_5090, &ornith_5090, M5, TTFT), "a decent 27B is never swapped for a faster model");
        assert_eq!(seats(&qwen_5090, M5, 16, TTFT), Some(12), "30 x 4 / 12 = 10 t/s is the last decent seat");
    }

    // what this catches: an absence read as a rate. A model with no prefill or no decode point
    // has no verdict, no seats, and never displaces or is displaced.
    #[test]
    fn an_unmeasured_model_is_neither_seated_nor_used_to_displace() {
        let blind = ModelRates { prefill_tps: None, decode_per_stream: [(1, 90.0)].into_iter().collect() };
        let slow = rates(100.0, &[(1, 5.0)]);
        assert_eq!(decent(&blind, M5, 1, TTFT), Decent::Unmeasured);
        assert_eq!(seats(&blind, M5, 8, TTFT), None);
        assert!(!incumbent_displaced(&slow, &blind, M5, TTFT), "an unmeasured challenger displaces nobody");
        assert!(!incumbent_displaced(&blind, &rates(500.0, &[(1, 50.0)]), M5, TTFT), "an unmeasured incumbent holds");
    }

    // what this catches: prefill priced per stream instead of shared. Eight minds each
    // prefilling 5,600 tokens on a 108 t/s server wait 415 s for their share, past the budget,
    // however fast each would decode.
    #[test]
    fn prefill_is_shared_by_every_mind_in_flight() {
        let fast_decode_slow_prefill = rates(108.0, &[(1, 80.0)]);
        assert_eq!(decent(&fast_decode_slow_prefill, M5, 3, TTFT), Decent::Yes);
        assert!(matches!(decent(&fast_decode_slow_prefill, M5, 8, TTFT), Decent::PrefillTooSlow { secs } if secs > 400.0));
    }

    // what this catches: a decode rate invented for an unmeasured concurrency. Above the
    // highest point the memory-bound physics holds the aggregate (per-stream scales k/n);
    // below the lowest, fewer streams decode no slower than the lowest measured.
    #[test]
    fn decode_between_points_follows_the_constant_aggregate() {
        let r = rates(100.0, &[(2, 20.0), (4, 9.0)]);
        assert_eq!(r.decode_at(4), Some(9.0));
        assert!((r.decode_at(8).unwrap() - 4.5).abs() < 1e-9);
        assert!((r.decode_at(3).unwrap() - 20.0 * 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(r.decode_at(1), Some(20.0));
        assert_eq!(ModelRates::default().decode_at(3), None);
    }
}
