//! Per-slot throughput verdict — the sensor that was missing on 2026-08-21.
//!
//! # The defect this exists for, measured live
//!
//! A citizen (`e5f4141d`) was working `pallets__flask-4045` in-room. Her run reported
//! `state: running`, heartbeated every minute, and sat at `acts: 4` for twelve minutes.
//! The lane answered `/health` `{"status":"ok"}`, `/props` 200, `/v1/models` 200, port
//! bound, process up 3h58m. Every readiness signal in the system said healthy.
//!
//! The lane's own per-slot numbers said otherwise:
//!
//! ```text
//! slot 1 | task 3   | n_decoded = 1092, tg = 2.65 t/s, tg_3s = 0.96 t/s
//! slot 1 | task 3   | n_decoded = 1093, tg = 2.53 t/s, tg_3s = 0.05 t/s
//! slot 0 | task 605 | prompt processing, progress 0.52, 105 t/s
//! ```
//!
//! **Slot 1 was decoding at 0.05 tokens/second** — one token per twenty seconds — while
//! still holding `task 3` after six hundred tasks had gone by on the neighbouring slot.
//! Prefill was healthy (105 t/s, the expected rate). Only DECODE had collapsed, on ONE
//! slot, and nothing in the system could see it.
//!
//! That is the #386 per-slot wedge, and it is the round-killer: a citizen whose
//! generation runs at 1/600th speed burns her whole attempt deadline producing nothing,
//! while the board renders a healthy pulse the entire time.
//!
//! # Why a whole-lane health check cannot catch it
//!
//! `/health` is lane-scoped and the LANE is fine — the other slot decodes normally, so
//! any aggregate reads healthy. #363 already established that readiness must verify
//! GENERATION rather than the socket; this is the next layer down: **generation must be
//! verified PER SLOT, because a lane is healthy exactly as long as its best slot is.**
//!
//! # Why the floor is a PARAMETER
//!
//! `expected_tps` is passed in, never baked here. What counts as collapsed depends on the
//! model and the device — a 27B on Metal and a 4B on a 5090 have different honest floors,
//! and the catalog Model row is where that expectation lives (#441). A constant here
//! would be a hardcoded LCD clamp of exactly the kind the substrate docs forbid, and it
//! would either miss a real wedge on a fast box or cry wolf on a slow one.
//!
//! # Instrument note
//!
//! The numbers above were read out of llama-server's stderr BY HAND, which is the only
//! way they were visible — and that is itself the defect Joel named the same day:
//! *"STDERR/out is never a place for interfacing, just debug."* Reading a log to DIAGNOSE
//! is correct; a control path may never depend on it. This module is deliberately pure
//! and takes [`SlotObservation`] values, so it works the moment those values arrive from
//! a structured channel (`/slots`, which currently returns HTTP 000 on our pinned build
//! and is the follow-up). The verdict logic must not wait on the transport.

/// One slot's throughput, as observed at a point in time.
///
/// Field names mirror what llama-server reports so the eventual `/slots` mapping is
/// obvious, but this type is transport-agnostic ON PURPOSE — see the module note.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlotObservation {
    /// Slot index within the lane.
    pub id: u32,
    /// The task currently occupying the slot.
    pub task_id: u64,
    /// Tokens decoded so far for this task.
    pub n_decoded: u64,
    /// Average decode rate over the task's life, tokens/sec.
    pub decode_tps: f32,
    /// Decode rate over the last ~3 seconds, tokens/sec. The LEADING indicator: a slot
    /// that just fell over still has a healthy lifetime average for a long while.
    pub decode_tps_3s: f32,
}

/// What the slot is actually doing, as opposed to what the lane claims.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SlotVerdict {
    /// Decoding at or near the expected rate.
    Healthy,
    /// Measurably slow but still progressing — worth a probe, not a reap. Contention and
    /// thermal throttling live here, and both recover on their own.
    Degraded { ratio: f32 },
    /// Effectively stopped. Progress is nominal rather than real: the slot will not
    /// finish this task in any useful time, so the run should be told rather than left
    /// to burn its deadline.
    Wedged { ratio: f32 },
}

impl SlotVerdict {
    /// Does this verdict warrant interrupting a run? `Wedged` only — `Degraded` is a
    /// report. Acting on `Degraded` would reap lanes that are merely busy, which is how
    /// a health check becomes the outage (#446: starvation stamped as wedge evidence,
    /// producing a 2-minute relaunch loop).
    pub fn is_actionable(&self) -> bool {
        matches!(self, SlotVerdict::Wedged { .. })
    }
}

/// Fraction of expected throughput below which a slot is DEGRADED rather than healthy.
/// Above this, normal variance: batching, contention, a long prompt landing.
const DEGRADED_BELOW: f32 = 0.5;

/// Fraction below which the slot is WEDGED. 5% of expected is not "slow" — at the
/// measured 0.05 t/s against a ~30 t/s expectation (0.0017) a 500-token answer takes
/// nearly three hours, which is longer than the attempt deadline it is spending.
const WEDGED_BELOW: f32 = 0.05;

/// Minimum decoded tokens before a rate is trusted. A slot that has just started has a
/// meaningless average, and calling it wedged would reap every cold generation — the
/// [[an-absence-is-an-unfinished-measurement]] error in actuator form.
const MIN_DECODED_FOR_VERDICT: u64 = 32;

/// Classify one slot against the throughput its model/device is expected to deliver.
///
/// Uses the 3-SECOND rate, not the lifetime average, because the lifetime average hides
/// a collapse for a long time: the live slot read `tg = 2.53` (already bad) while
/// `tg_3s = 0.05` (catastrophic). Averaged over a task that has been running for hours,
/// a dead slot still looks merely slow.
pub fn classify(obs: &SlotObservation, expected_tps: f32) -> SlotVerdict {
    // No expectation, no verdict. A catalog row without a measured rate must not be
    // silently assigned one — that would make the sensor fabricate its own threshold.
    if expected_tps <= 0.0 || obs.n_decoded < MIN_DECODED_FOR_VERDICT {
        return SlotVerdict::Healthy;
    }
    let ratio = obs.decode_tps_3s / expected_tps;
    if ratio < WEDGED_BELOW {
        SlotVerdict::Wedged { ratio }
    } else if ratio < DEGRADED_BELOW {
        SlotVerdict::Degraded { ratio }
    } else {
        SlotVerdict::Healthy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The live slot 1 reading from 2026-08-21, verbatim.
    fn wedged_slot_from_the_live_incident() -> SlotObservation {
        SlotObservation {
            id: 1,
            task_id: 3,
            n_decoded: 1093,
            decode_tps: 2.53,
            decode_tps_3s: 0.05,
        }
    }

    // what this catches: THE ROUND-KILLER, as a test instead of a twelve-minute mystery.
    // Measured live — citizen e5f4141d on pallets__flask-4045, lane reporting /health ok
    // the whole time, acts frozen at 4. If this ever returns anything but Wedged, a
    // citizen can again burn a 4.3h attempt deadline at one token per twenty seconds
    // with every dashboard green.
    #[test]
    fn the_live_wedge_is_classified_wedged() {
        let v = classify(&wedged_slot_from_the_live_incident(), 30.0);
        assert!(
            matches!(v, SlotVerdict::Wedged { .. }),
            "0.05 t/s against a 30 t/s expectation must read WEDGED, got {v:?}"
        );
        assert!(v.is_actionable(), "a wedge must be actionable — that is the point");
    }

    // what this catches: the lifetime average masking a collapse. The SAME slot's
    // lifetime figure (2.53 t/s) is 50x its instantaneous rate; classifying on the
    // average would have called this merely Degraded and left her running.
    #[test]
    fn the_lifetime_average_would_have_missed_it() {
        let obs = wedged_slot_from_the_live_incident();
        let on_lifetime = classify(
            &SlotObservation { decode_tps_3s: obs.decode_tps, ..obs },
            30.0,
        );
        assert!(
            !matches!(on_lifetime, SlotVerdict::Wedged { .. }),
            "the lifetime average hides the collapse — which is WHY classify() reads \
             the 3s rate. If this ever becomes Wedged the constants moved and the \
             leading-indicator argument needs rechecking, not the assert flipping."
        );
    }

    // what this catches: reaping a healthy-but-busy slot. #446 — starvation stamped as
    // wedge evidence produced a 2-minute relaunch loop that WAS the outage. Slow is a
    // report; stopped is an action.
    #[test]
    fn merely_slow_is_reported_not_actioned() {
        let obs = SlotObservation {
            id: 0,
            task_id: 605,
            n_decoded: 400,
            decode_tps: 12.0,
            decode_tps_3s: 9.0, // 30% of expected — contention, not death
        };
        let v = classify(&obs, 30.0);
        assert!(matches!(v, SlotVerdict::Degraded { .. }), "got {v:?}");
        assert!(!v.is_actionable(), "Degraded must never reap a lane");
    }

    // what this catches: killing cold generations. A slot 8 tokens into a reply has a
    // meaningless rate; a verdict there is the absence-as-fact error wired to an
    // actuator.
    #[test]
    fn a_slot_that_just_started_is_never_wedged() {
        let obs = SlotObservation {
            id: 2,
            task_id: 900,
            n_decoded: 8,
            decode_tps: 0.4,
            decode_tps_3s: 0.4,
        };
        assert_eq!(classify(&obs, 30.0), SlotVerdict::Healthy);
    }

    // what this catches: the sensor inventing its own floor. A catalog row with no
    // measured expectation must yield NO verdict — never a fabricated threshold, and
    // never a hardcoded LCD clamp (the substrate docs' standing prohibition).
    #[test]
    fn no_expectation_means_no_verdict() {
        assert_eq!(
            classify(&wedged_slot_from_the_live_incident(), 0.0),
            SlotVerdict::Healthy,
            "without a per-model expectation the sensor must abstain, not guess"
        );
    }

    // what this catches: a fast box being judged by a slow box's standard, and vice
    // versa. The SAME observation is healthy or wedged depending on what the device was
    // expected to deliver — which is exactly why expected_tps is a parameter.
    #[test]
    fn the_verdict_follows_the_devices_own_expectation() {
        let obs = SlotObservation {
            id: 0,
            task_id: 1,
            n_decoded: 500,
            decode_tps: 2.0,
            decode_tps_3s: 2.0,
        };
        // A 2 t/s slot on a box expected to do 2.5 t/s (big model, modest hardware): fine.
        assert_eq!(classify(&obs, 2.5), SlotVerdict::Healthy);
        // The same 2 t/s on a 5090 expected to do 60: dead.
        assert!(matches!(classify(&obs, 60.0), SlotVerdict::Wedged { .. }));
    }
}
