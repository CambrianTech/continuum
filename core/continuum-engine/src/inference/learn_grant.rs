//! THE LEARNING GRANT: how much slower her decoding may run while learning runs on her lane.
//!
//! Learning takes the decode headroom serving is not using, never what serving needs. Two laws
//! bound it, both already this crate's:
//! - the decode floor ([`DECODE_FLOOR_TPS`]): below it a stream is slower than a person reads,
//!   so learning may slow her only down to the floor, from her MEASURED rate on this box;
//! - the latency law (Joel, 2026-09-16: "a 1.5x tax is fine, 5-10x is disqualifying"): a turn
//!   decoding beside a window takes at most [`LATENCY_TAX`] times as long, however fast the box.
//!
//! No rate measured, or a rate already at the floor, is no headroom: the bound is
//! [`SlowdownPpm::IDLE_ONLY`], and the engine trains in serving's idle gaps. The engine paces
//! only while a slot decodes, so the grant never stops learning on an idle lane.
//!
//! It replaces one fixed 10% for every box and every load. The grant is read when a job starts;
//! a live re-grant waits on
//! the engine taking a bound mid-run (MIND-DATAFLOW-OVERHAUL row 6, "a duty budget on the wire").

use super::decode_knee::{rate_for, MeasuredRate, DECODE_FLOOR_TPS};
use continuum_engine_protocol::SlowdownPpm;

/// The latency law's tax: a turn beside a window takes at most this many times as long.
pub const LATENCY_TAX: f64 = 1.5;

/// The grant, with the measured rate it came from (for the job's probe).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LearnGrant {
    pub max_slowdown: SlowdownPpm,
    pub rate: MeasuredRate,
}

/// PURE: the slowdown a measured rate can give learning. Her rate may fall to the larger of the
/// floor and `rate / LATENCY_TAX`; the fraction lost is the grant.
pub fn grant_from(rate: MeasuredRate) -> LearnGrant {
    let lowest = |tps: f64| DECODE_FLOOR_TPS.max(tps / LATENCY_TAX);
    let headroom = rate
        .tps
        .filter(|&tps| tps > DECODE_FLOOR_TPS)
        .and_then(|tps| SlowdownPpm::new(((1.0 - lowest(tps) / tps) * 1e6).floor() as u32));
    // no headroom (unmeasured, at the floor, or under one ppm) is the idle-gaps bound
    let max_slowdown = match headroom {
        Some(s) => s,
        None => SlowdownPpm::IDLE_ONLY,
    };
    LearnGrant { max_slowdown, rate }
}

/// The grant for a job on `model`'s lane, from this box's decode curve now.
pub fn grant_for(model: &str) -> LearnGrant {
    grant_from(rate_for(model))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::decode_knee::RateSource;

    fn at(tps: f64) -> LearnGrant {
        grant_from(MeasuredRate { tps: Some(tps), source: RateSource::Fresh })
    }

    /// what this catches: an unmeasured box guessing a share instead of waiting for idle gaps
    #[test]
    fn no_rate_is_idle_gaps_only() {
        assert_eq!(grant_from(MeasuredRate::UNKNOWN).max_slowdown, SlowdownPpm::IDLE_ONLY);
    }

    /// what this catches: learning slowing a stream that is already at or under the floor
    #[test]
    fn at_or_under_the_floor_is_idle_gaps_only() {
        assert_eq!(at(DECODE_FLOOR_TPS).max_slowdown, SlowdownPpm::IDLE_ONLY);
        assert_eq!(at(6.0).max_slowdown, SlowdownPpm::IDLE_ONLY);
    }

    /// what this catches: the grant taking her below the floor (the M5 with three decoding,
    /// 15 t/s on 9810171b0: down to 10, a third) or past the latency law on a fast box (the
    /// single-stream 25 t/s: a third, not the 60% the floor alone would give)
    #[test]
    fn the_floor_binds_slow_boxes_and_the_tax_binds_fast_ones() {
        assert_eq!(at(12.0).max_slowdown.ppm(), 166_666);
        assert_eq!(at(15.0).max_slowdown.ppm(), 333_333);
        assert_eq!(at(25.0).max_slowdown.ppm(), 333_333);
        assert_eq!(at(68.0).max_slowdown.ppm(), 333_333);
    }
}
