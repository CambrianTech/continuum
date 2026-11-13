//! Which liveness budget a generation stream is owed, given what the lane is
//! actually doing right now.
//!
//! # The defect this exists for, measured live 2026-08-21
//!
//! Four citizens resident on an M5, lane healthy, `persona.turn.*` = **0 across the
//! ledger's 868 minutes**. The chain, every link measured:
//!
//! ```text
//! demand           94,370 tok   against a 58,112-token served window (over_window 1.62)
//! prefill rate        382 tok/s  (measured: 603 tok → 17.7s TTFB; 5,728 tok → 31.1s)
//! ⇒ first byte       ~170 s      for a window-sized prompt
//! watchdog             90 s      ← fires first, every time
//! ```
//!
//! So the lane was killed mid-prefill and the turn retried — Atlas re-sent the
//! byte-identical 94,370-token demand 21 minutes later and died the same way. 44
//! `persona.settle.deliberation_retry` rows carry the verdict the watchdog reached:
//! *"the slot HAD started our work and then stopped mid-stream, so the backend is
//! stuck or dead."* The backend was neither.
//!
//! The downstream damage is what made it a round-killer rather than a slow turn: the
//! ambient permit is held across that wait (`persona/service_loop.rs`), so three
//! permits were occupied ~90s each by turns that could never finish. Measured
//! **+4 yields/min, exactly, for 25 consecutive minutes, with zero successes** — the
//! perfect periodicity is the tell, because real contention is bursty.
//!
//! # The actual mistake, which was one line and not a missing feature
//!
//! Both budgets already existed and both were correctly sized *for their own job*:
//!
//! - [`STREAM_IDLE_TIMEOUT_SECS`] (90s) — its doc calls it a per-token DECODE
//!   watchdog, for "a slow-but-producing decode… a token every few hundred ms".
//! - [`PRE_STREAM_HEADER_TIMEOUT_SECS`] (300s) — its doc says it is "sized for a
//!   worst-case full-window prefill queued behind co-tenants (minutes)".
//!
//! The adapter switched from the second to the first the instant the FIRST
//! `prompt_progress` frame arrived — which llama.cpp emits at 0% ingestion, before
//! any real work. So the decode watchdog was policing the prefill, and the budget
//! written for exactly this situation was skipped past in the first few seconds.
//!
//! **Prefill is not decode.** Ingesting 58k tokens behind three co-tenants is the
//! same regime as waiting in the queue: bulk work, contended, minutes-scale. Decode
//! is the steady drip the 90s budget was written for. This module names that
//! distinction once so no caller has to re-derive it.
//!
//! # Why the wedge detector keeps its teeth
//!
//! This does NOT weaken #385. Liveness is keyed on the GAP BETWEEN PROGRESS EVENTS,
//! never on elapsed total — the caller stamps `last_progress` only when `processed`
//! strictly advances, so a slot replaying a frozen counter (the wedge signature,
//! `n_decoded` stuck at 1 for hours on 2026-08-09) still fails, just at the prefill
//! budget instead of the decode one. A healthy prefill emits a frame per batch
//! iteration — `send_partial_response(slot, {}, true)` fires for every slot in
//! `SLOT_STATE_PROCESSING_PROMPT` on every server loop pass — so a real ingest never
//! approaches even the 90s gap. Only a slot that stopped being SCHEDULED goes quiet,
//! and that is precisely the case worth waiting longer on.
//!
//! # Lanes that never report progress
//!
//! A provider that emits no `prompt_progress` (every cloud adapter) transitions
//! straight `Queued → Decoding` on its first content delta, so its budgets are
//! bit-for-bit what they were before this module existed. The change is a strict
//! addition for lanes that DO report, never a relaxation for lanes that don't.

use std::time::Duration;

/// What the lane is doing for us right now.
///
/// Deliberately transport-agnostic: it is derived from parsed stream events, not
/// from any particular provider's JSON, so the same phase machine serves llama.cpp
/// (which reports prefill) and cloud providers (which do not).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamPhase {
    /// POSTed; nothing has come back. The slot may not have been assigned yet.
    Queued,
    /// Prefill is advancing — bulk ingest, contended with co-tenants.
    Prefilling { processed: u64, total: u64 },
    /// Prefill is done (or the provider never reported it and tokens are flowing).
    /// Output should now arrive as a steady drip.
    Decoding,
}

impl StreamPhase {
    /// Has the lane begun our work? Drives the diagnosis in the timeout message —
    /// "never started" (dead backend vs oversubscribed queue) reads very differently
    /// from "started then stopped", and #446 showed that mislabelling the first as
    /// the second relaunches healthy busy lanes every two minutes.
    pub fn has_started(&self) -> bool {
        !matches!(self, StreamPhase::Queued)
    }

    /// Fold a prefill-progress report into the phase.
    ///
    /// A frame whose `processed` has reached `total` means ingest is done and decode
    /// is next, so we advance to [`StreamPhase::Decoding`] rather than waiting for
    /// the first token — otherwise a model that thinks for a while before emitting
    /// anything visible would sit on the prefill budget with no prefill left to do.
    ///
    /// [`StreamPhase::Decoding`] is TERMINAL: a late or stray progress frame arriving
    /// mid-decode must not re-open the generous budget, or the #385 wedge detector
    /// loses its teeth for the rest of the stream.
    pub fn on_prefill(self, processed: u64, total: u64) -> Self {
        match self {
            StreamPhase::Decoding => self,
            _ if total > 0 && processed >= total => StreamPhase::Decoding,
            _ => StreamPhase::Prefilling { processed, total },
        }
    }

    /// Fold "real output arrived" (content / reasoning / tool delta / finish) into
    /// the phase. Once anything is generated, prefill is definitionally over.
    pub fn on_output(self) -> Self {
        StreamPhase::Decoding
    }
}

/// How long the stream may stay silent, given its phase.
///
/// Both budgets are passed in rather than read from constants here, so the policy is
/// testable without a clock and the two timeout values keep exactly one definition
/// each (they live beside the HTTP code that also uses them for the header wait).
///
/// The rule, in one line: **bulk work gets the bulk budget; the drip gets the drip
/// budget.**
pub fn idle_budget(phase: StreamPhase, bulk: Duration, drip: Duration) -> Duration {
    match phase {
        // Unassigned or ingesting: both are "waiting on minutes-scale work behind
        // co-tenants", which is the case PRE_STREAM_HEADER_TIMEOUT_SECS was written for.
        StreamPhase::Queued | StreamPhase::Prefilling { .. } => bulk,
        // Generating: a token every few hundred ms is the contract, so silence is
        // genuinely suspicious at the decode budget.
        StreamPhase::Decoding => drip,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BULK: Duration = Duration::from_secs(300);
    const DRIP: Duration = Duration::from_secs(90);

    /// The live 2026-08-21 numbers: Atlas's prompt against the served window.
    const LIVE_TOTAL: u64 = 58_112;
    /// ~382 tok/s measured ⇒ a window-sized prefill needs ~170s of headroom.
    const LIVE_PREFILL_SECS: u64 = 170;

    // what this catches: THE ROUND-KILLER. A prompt mid-prefill must be judged
    // against the bulk budget. If this ever returns the drip budget, a window-sized
    // prompt is again structurally incapable of surviving its own liveness check —
    // measured live as 44 retries, 0 completed turns in 868 minutes, and the
    // starvation cascade behind it.
    #[test]
    fn a_prompt_still_prefilling_gets_the_bulk_budget() {
        let phase = StreamPhase::Queued.on_prefill(12_288, LIVE_TOTAL);
        let budget = idle_budget(phase, BULK, DRIP);
        assert_eq!(budget, BULK, "prefill must not be policed by the decode watchdog");
        assert!(
            budget.as_secs() > LIVE_PREFILL_SECS,
            "the budget must exceed the ~{LIVE_PREFILL_SECS}s a 58k-token prefill \
             actually takes at the measured 382 tok/s, or the fix is cosmetic"
        );
    }

    // what this catches: the exact off-by-one-phase that caused the outage. llama.cpp
    // emits a 0% progress frame the moment the slot is assigned ("this is to signal
    // the client that the request has started processing"). Treating that as "started
    // → decode budget" is what dropped 300s to 90s in the first few seconds.
    #[test]
    fn the_zero_percent_frame_does_not_start_the_decode_clock() {
        let phase = StreamPhase::Queued.on_prefill(0, LIVE_TOTAL);
        assert_eq!(phase, StreamPhase::Prefilling { processed: 0, total: LIVE_TOTAL });
        assert_eq!(idle_budget(phase, BULK, DRIP), BULK);
        assert!(phase.has_started(), "the slot IS assigned — the diagnosis text depends on this");
    }

    // what this catches: leaving a finished prefill on the generous budget forever,
    // which would blunt #385. Once ingest completes, the drip contract applies.
    #[test]
    fn a_completed_prefill_switches_to_the_decode_budget() {
        let phase = StreamPhase::Queued.on_prefill(LIVE_TOTAL, LIVE_TOTAL);
        assert_eq!(phase, StreamPhase::Decoding);
        assert_eq!(idle_budget(phase, BULK, DRIP), DRIP);
    }

    // what this catches: a thinking model that finishes prefill and reasons privately
    // for a while. Prefill is over, so it is decode's contract that applies — we must
    // not park on the bulk budget just because no VISIBLE token has appeared.
    #[test]
    fn prefill_completion_not_first_token_ends_the_bulk_phase() {
        let phase = StreamPhase::Prefilling { processed: 58_000, total: LIVE_TOTAL }
            .on_prefill(LIVE_TOTAL, LIVE_TOTAL);
        assert_eq!(idle_budget(phase, BULK, DRIP), DRIP);
    }

    // what this catches: a regression for every provider that reports no prefill at
    // all (all cloud adapters). Their budgets must be bit-for-bit what they were
    // before this module existed: bulk until first output, drip after.
    #[test]
    fn a_lane_that_never_reports_prefill_behaves_exactly_as_before() {
        let queued = StreamPhase::Queued;
        assert_eq!(idle_budget(queued, BULK, DRIP), BULK);
        assert!(!queued.has_started());

        let decoding = queued.on_output();
        assert_eq!(idle_budget(decoding, BULK, DRIP), DRIP);
        assert!(decoding.has_started());
    }

    // what this catches: a total of 0 (llama.cpp sends this on the initial signalling
    // frame before token counts are known) being read as "0 >= 0 ⇒ complete", which
    // would jump a just-assigned slot straight onto the decode budget — the outage
    // again, by a different route.
    #[test]
    fn an_unknown_total_is_never_mistaken_for_a_finished_prefill() {
        let phase = StreamPhase::Queued.on_prefill(0, 0);
        assert_eq!(phase, StreamPhase::Prefilling { processed: 0, total: 0 });
        assert_eq!(idle_budget(phase, BULK, DRIP), BULK);
    }

    // what this catches: output arriving after prefill can never demote the phase
    // back to a laxer budget — the wedge detector's teeth depend on decode staying
    // on the drip budget for the rest of the stream.
    #[test]
    fn decoding_is_terminal() {
        let phase = StreamPhase::Decoding.on_prefill(1, LIVE_TOTAL);
        assert_eq!(
            idle_budget(phase, BULK, DRIP),
            DRIP,
            "a late/stray progress frame must not re-open the bulk budget mid-decode"
        );
    }
}
