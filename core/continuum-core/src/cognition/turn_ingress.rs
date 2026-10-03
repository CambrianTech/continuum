//! THE DOOR A STOPPING NODE CLOSES.
//!
//! One process-wide flag: may a persona's service loop begin a NEW turn?
//!
//! # Why this is not `quiesced`
//!
//! `PersonaAircRuntimeRegistry`'s per-persona `quiesced` flag suspends the autonomic
//! SELF-TICK and deliberately leaves her reachable — its contract is "she stops wandering
//! and still picks up the phone", because a measurement lease must not make a citizen
//! unreachable. That is the right behaviour for a benchmark and the wrong one for a stop:
//! a quiesced citizen still starts turns when addressed, so a `save_state` taken after a
//! quiesce can still land underneath a turn halfway through writing.
//!
//! So this is a different question with a different answer, and folding it into `quiesced`
//! would break the lease's promise. Closing this door stops EVERY new turn — directed,
//! self-directed, forked — and lets the ones already running finish.
//!
//! # One-way, on purpose
//!
//! There is no `open()`. The only caller is the shutdown drain, and the process exits
//! immediately afterwards; a reopen verb would exist solely to be called by mistake. A
//! node that wants turns again starts a core.

use crate::runtime::AdmissionGate;

/// The process's turn gate. One [`AdmissionGate`]: open/closed and the in-flight count in
/// one word, so an admission cannot slip between a close and the drain's read.
///
/// The gate logic lives in `runtime::admission_gate` rather than here because the log
/// queue needs exactly the same invariant, and when it was written twice the second copy
/// reintroduced the race the first had just removed.
static GATE: AdmissionGate = AdmissionGate::new();

/// SERVICE-LOOP turns running right now, across every persona in this process.
///
/// # What this number does NOT include, stated because a drain keys on it
///
/// Only work that passed through [`admit`] is counted, and today that is the persona
/// service loop. A `cognition/eval` fork, or any caller that reaches the cognition
/// faculties directly rather than through a citizen's loop, runs UNCOUNTED — so a drain
/// can report the node quiet while such a call is mid-flight.
///
/// That is a narrowed contract, not an oversight, and it is narrowed rather than widened
/// because the alternative is worse: an eval fork holding a permit would keep the whole
/// node from draining for the length of a benchmark, and a permit taken somewhere that
/// does not release it on every path leaks a phantom turn that no drain can ever clear.
/// The service loop is the one caller whose entry and exit are a single bounded scope.
///
/// The consequence a reader must not be surprised by: a stop taken DURING an eval saves a
/// consistent citizen but may cut the eval. Widening this is a real piece of work —
/// permits at the faculty boundary, with the same RAII discipline — and it belongs to
/// whoever gives `cognition/eval` a lifecycle, not to a shutdown rail. Raised in review
/// by Astra.
pub fn in_flight() -> u64 {
    GATE.in_flight()
}

/// ADMIT a turn, or refuse because the node is stopping.
///
/// # Why not read `activity_gate`'s engaged flag
///
/// `persona_engaged` is stamped where a serving LANE is acquired, deliberately — a room
/// wake alone is not wakefulness, or a busy room would cancel every dream. So a turn that
/// has been admitted and is still composing context, or is queued for a lane, is not yet
/// `engaged`. Draining on that flag would walk past exactly the turns that have taken
/// input and not yet written anything, which are the ones whose loss is invisible.
#[must_use = "the permit IS the turn's admission; dropping it immediately ends the turn"]
pub fn admit() -> Option<crate::runtime::Permit<'static>> {
    GATE.admit()
}

/// Shut the door. Returns whether THIS call closed it, so a second drain — a signal
/// racing the `system/shutdown` verb, since both reach the same broadcast — can tell it is
/// re-entering rather than report a fresh close.
pub fn close() -> bool {
    GATE.close()
}

/// Is the node stopping? Read by a turn already admitted, at each act boundary, so a
/// multi-act tool loop ends after the act in hand instead of starting its next generation
/// (Fable on #4684: a 12-act solve otherwise rides the whole settle cap). A bare read is
/// safe HERE because the gate only ever closes: a stale `false` lets one more act run, and
/// nothing can make a `true` wrong.
pub fn is_closing() -> bool {
    !GATE.is_open()
}

/// The longest a stop waits for admitted turns to finish before it saves (card 32fa22ba).
///
/// The module drain's own budget is 1.8 s, sized to the runtime's 2 s phase — and a turn
/// is not 2 s on any tier this grid runs. The IntelMac's CPU 1.5B takes minutes, the M5's
/// 27B decodes about 3 tok/s per stream: 40 IntelMac deploys and 187 M5 deploys logged
/// `cognition (drain Incomplete { in_flight: N })`, up to 15 citizens cut at once. The
/// wait is the turns' MEASURED remaining time ([`settle_bound_ms`]); this is only its
/// ceiling, so a wedged turn cannot hold a deploy forever. One constant, read by the core
/// (how long to wait) and the CLI (how long to wait for the core), so the two cannot drift.
pub const TURN_SETTLE_CAP: std::time::Duration = std::time::Duration::from_secs(300);

/// How long the turns in flight are expected to still need, ms. Pure.
///
/// `turns` is each measured turn in flight as (elapsed ms, her typical turn ms — `None`
/// when she has not finished one this boot). `uncounted` = more turns hold a permit than
/// have a measured start (a turn admitted and still composing), so at least one is
/// unknown. An unknown turn waits the cap: an absence is not a number. A known one waits
/// half again her typical turn past what it has already run, and never less than a
/// quarter of a turn, so one running long is given time rather than cut at once.
pub(crate) fn settle_bound_ms(turns: &[(u64, Option<u64>)], uncounted: bool, cap_ms: u64) -> u64 {
    let unknown = if uncounted { cap_ms } else { 0 };
    turns
        .iter()
        .map(|&(elapsed, typical)| match typical {
            Some(t) => (t + t / 2).saturating_sub(elapsed).max(t / 4),
            None => cap_ms,
        })
        .fold(unknown, u64::max)
        .min(cap_ms)
}

/// What the settle before a stop did — the probe's numbers, so a deploy that still cuts
/// turns says how many, and against which bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TurnSettle {
    pub in_flight_at_close: u64,
    pub cut: u64,
    pub bound_ms: u64,
    /// Where the bound came from: `nothing_in_flight`, `measured` (the turns' own
    /// remaining time), or `cap` (an unknown turn, or measured past the cap).
    pub bound_source: &'static str,
    pub waited_ms: u64,
}

/// CLOSE THE DOOR, THEN LET THE CITIZENS FINISH: the step before a stop saves.
///
/// Called only from the runtime's shutdown task, which no connection owns, so the door
/// this closes is always followed by the shutdown it exists for (the one-way rule above).
/// The module drain that follows closes the same door again (a no-op) and waits its own
/// 1.8 s for anything still left — the backstop, unchanged.
pub(crate) async fn settle(cap: std::time::Duration) -> TurnSettle {
    const POLL: std::time::Duration = std::time::Duration::from_millis(200);
    close();
    let started = std::time::Instant::now();
    let in_flight_at_close = in_flight();
    let cap_ms = cap.as_millis().min(u128::from(u64::MAX)) as u64;
    let turns = crate::cognition::resource_admission::in_flight_turns(crate::persona::trace::now_ms());
    let bound_ms = settle_bound_ms(&turns, in_flight_at_close > turns.len() as u64, cap_ms);
    let bound_source = if in_flight_at_close == 0 {
        "nothing_in_flight"
    } else if bound_ms >= cap_ms {
        "cap"
    } else {
        "measured"
    };
    let deadline = started + std::time::Duration::from_millis(bound_ms);
    let mut poll = tokio::time::interval(POLL);
    while in_flight() > 0 && std::time::Instant::now() < deadline {
        poll.tick().await;
    }
    TurnSettle {
        in_flight_at_close,
        cut: in_flight(),
        bound_ms,
        bound_source,
        waited_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the process gate being wired to something other than an open
    // AdmissionGate — a gate that defaulted closed would make every citizen refuse to take
    // a turn, a total silent outage no shutdown test would exercise.
    //
    // The gate's BEHAVIOUR — the admit/close race, the CAS retry, the shared word — is
    // tested against the real type in `runtime::admission_gate`, on instances a test can
    // close without poisoning the process. This module's job is only to hold one and hand
    // it to the service loop, so that is all this asserts.
    #[test]
    fn the_process_turn_gate_starts_open_and_empty() {
        // Openness is asserted by ADMITTING, not by a separate reader: a bare `is_open`
        // on this module had no caller at all — the service loop takes a permit — so it
        // was dead weight and is gone.
        //
        // It was NOT the reachability-ratchet failure, though I first claimed it was. The
        // scanner greps `pub struct ` / `pub enum ` (production_reachability.rs:125) and
        // does not count functions at all, so removing one could never have moved the
        // number. The counted item was `pub struct ShutdownOperation`, now private.
        // Corrected by Astra, who READ the scanner rather than inferring what it measures
        // — which is what I should have done before claiming a fix for it.
        let permit = admit().expect("an open gate admits");
        assert_eq!(
            in_flight(),
            1,
            "an admitted turn must be visible to a drain"
        );
        drop(permit);
        assert_eq!(in_flight(), 0, "and invisible once it ends");
    }

    // regression for card 32fa22ba (IntelMac: 40 deploys, M5: 187, each cutting up to 15
    // turns on a fixed 1.8 s drain). what this catches: the settle's wait is the turns'
    // measured REMAINING time — not zero for a turn with minutes left, not the cap for
    // one about to finish — an unknown turn (unmeasured, or admitted without a measured
    // start) waits the cap, an overrunning turn still gets a quarter turn, and nothing
    // ever exceeds the cap.
    #[test]
    fn the_settle_waits_the_measured_remaining_turn_and_never_past_the_cap() {
        const CAP: u64 = 300_000;
        assert_eq!(settle_bound_ms(&[], false, CAP), 0, "nothing in flight: no wait");
        // A 4-minute CPU turn one minute in: half again past the typical, minus elapsed.
        assert_eq!(settle_bound_ms(&[(60_000, Some(240_000))], false, CAP), 300_000);
        assert_eq!(settle_bound_ms(&[(200_000, Some(240_000))], false, CAP), 160_000);
        // Running long: a quarter turn, not an instant cut.
        assert_eq!(settle_bound_ms(&[(500_000, Some(240_000))], false, CAP), 60_000);
        // The longest of several decides; the cap bounds it.
        assert_eq!(settle_bound_ms(&[(1_000, Some(20_000)), (0, Some(900_000))], false, CAP), CAP);
        // Unknown turns wait the cap.
        assert_eq!(settle_bound_ms(&[(1_000, None)], false, CAP), CAP);
        assert_eq!(settle_bound_ms(&[(1_000, Some(20_000))], true, CAP), CAP);
        // A zero cap is no settle at all.
        assert_eq!(settle_bound_ms(&[(0, Some(240_000))], true, 0), 0);
    }
}
