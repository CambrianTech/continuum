//! Fair service on a virtual clock (ONE-ACT "Coverage audit and deterministic execution"): the
//! production grant owner composed with a scripted engine, driven by recorded demand and by
//! seeded generated schedules, with conservation, legal transitions and ownership asserted after
//! every event, and the feasible-opportunity contract of section 4 asserted at every publication.
//!
//! - [`clock`]: the virtual clock (advance to the next event, never sleep), seeded draws, and the
//!   shrinker that keeps the shortest failing schedule.
//! - [`opportunity`]: section 4's feasible service opportunity as a typed predicate.
//! - [`world`]: the composition and its per-step checks.
//! - [`recorded`]: the 2026-11-04 whole-pool window (dyno 2 and 3) through that composition.
//!
//! Classification (section "Coverage audit"): every scenario here is a PRODUCTION-POLICY REPLAY
//! of the grant owner with scripted actuator receipts. The HTTP paging custodian that constructs
//! a physical receipt is owner integration, covered by `slots::grant_handoff::tests` and
//! `window_grant::lease::fair::tests::http_reclamation_proof_publishes_the_waiter_without_freeing_early`;
//! nothing here is a real-engine or live measurement.
#![cfg(test)]

pub(crate) mod clock;
pub(crate) mod opportunity;
pub(crate) mod recorded;
pub(crate) mod world;

#[cfg(test)]
mod tests;
