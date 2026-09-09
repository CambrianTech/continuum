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
        assert_eq!(in_flight(), 1, "an admitted turn must be visible to a drain");
        drop(permit);
        assert_eq!(in_flight(), 0, "and invisible once it ends");
    }
}
