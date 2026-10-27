//! A node's service, measured by its minds (card 1c157668; Joel, 2026-10-26: "system should
//! self recognize slowdown and figure it out").
//!
//! The M5 ran an engine that came back degraded from a Compute-error burst for three hours
//! (54 generations/h, median 174 s) on the same plan that served 118/h at 88 s after a clean
//! restart, and nothing noticed. This crate is the noticing: a [`sample::ServiceSample`] per
//! window, a baseline per [`baseline::BaselineKey`] (same node, model, builds, configuration
//! and workload stratum, across engine restarts), and [`detect::observe`], which answers
//! healthy, regressed (with the evidence and the candidate causes) or not enough evidence.
//!
//! [`resident`] is the same noticing per mind: each resident's settled turns become a window,
//! judged on her own stream, and a stall (work asked, nothing completed: Kimi, 2026-10-26,
//! 17 hours of capacity refusals nobody saw) becomes a typed [`resident::ResidentAlarm`] the
//! node publishes on its beacon.
//!
//! It decides nothing about the engine: the planner's reaction consumes a
//! [`detect::Regression`] and proposes. Every bound is the caller's [`detect::ServicePolicy`],
//! never a constant here.

pub mod baseline;
pub mod detect;
pub mod resident;
pub mod sample;
