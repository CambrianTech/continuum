//! Citizen health is a SUBSTRATE RECEIPT, not a hand read.
//!
//! Joel, 2026-09-14: "this is up to you their health." That afternoon the health of
//! sixteen minds (3 lanes at 120k, 4 writes an hour, a fifth of every receipt "let me
//! take stock") was found by an operator querying probes by hand; every number was
//! already in the process and nobody owned the drift. This module owns it: an
//! in-process ledger fed at the three seams that already probe (an act observed, a
//! lane granted, a card settled), snapshotted once an hour beside residency and the
//! served lanes, judged by one pure rule, probed, and said in the org room in one line.
//!
//! The rule names the shape, not a score: STARVED (more minds than lanes can turn),
//! READING (acts without writes), IDLE (residents, no acts), or HEALTHY. The line is
//! what a human or a peer acts on; the probe is what a test or a dashboard reads.
//!
//! THE RECEIPT HAS AN ACTOR (card c84d885a, S3 — Joel 2026-09-18: "govern for
//! reliability"): the tick pays what the line used to only say. Until 2026-10-04 a second
//! rule here rested seats when the hour's CARD PULLS were lane-deferred; the pull was
//! deleted that day (HER-LOOP-IS-HER-OWN.md row A: nothing hands a citizen a card), and a
//! rest whose reason can no longer occur went with it. A starved or at-the-knee roster
//! is still named on the line; the mindless rule and the slow clip are the seat remedies.
//! Cadence follows the RTOS shape: the module's own `tick_interval`, no task of its own
//! ([[docs/architecture/CONCURRENCY-STYLE-GUIDE.md]]).

use continuum_base::time::EpochMs;
use crate::runtime::{CommandResult, CommandSchema, ModuleConfig, ModulePriority, ServiceModule};
use std::any::Any;
use std::sync::atomic::Ordering;

use crate::cognition::health_ledger::*;
mod rest_seats;
pub use rest_seats::*;
mod resident_service;
pub use resident_service::current_resident_alarms;

#[cfg(test)]
mod tests;

/// What the grid allocator last published about THIS hour's placement: the oldest
/// dormant mind's turn age. `None` = nothing published yet or nobody dormant.
pub(super) fn oldest_dormant_turn_age_ms() -> Option<u64> {
    crate::modules::grid_allocator::current().and_then(|p| p.oldest_dormant_turn_age_ms())
}

/// Why a seat rests — each rule that pages out, with its own receipt.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RestCause {
    Mindless,
    /// The slow clip: a dormant mind is waiting and this seat's hour held nothing.
    Rotation,
}
