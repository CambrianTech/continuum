//! Work flows through queues, never locks (SERVING-ENGINE §1c; Joel, 2026-10-28: "Its got to
//! flow or have MANY queues"; "Semaphore really only exists for queue inserts and signals").
//!
//! - Every unit of work has a quality-of-service class ([`Qos`]): interactive, work, utility,
//!   background. A higher class never waits behind a lower one.
//! - Each owner (a mind) is a serial queue: one unit of its work runs at a time.
//! - One dispatcher per capacity (an engine's slots) owns the ready set and starts work only when
//!   a unit frees. It wakes on events and on the next aging instant, never by polling.
//! - Waiting work ages one class at a time up to `Work`, so a dream still runs, irregularly and
//!   in the gaps, and never passes a directed reply.
//! - When a higher class waits and nothing is free, the lowest running class is asked to yield at
//!   its next safe point (`Grant::yield_requested`); for a sequence, a page-out at a decode-step
//!   boundary.
//! - Queues stay short, owned here and not by callers: an owner keeps one waiting unit (her
//!   newer work supersedes the older and keeps its place); utility and background queues are
//!   bounded and shed their oldest. A long queue gives every request everyone's latency.
//! - Nobody holds a lock while waiting or working: a grant is a capability whose drop is the
//!   release event.

mod flow;
mod qos;
mod scheduler;
mod stage;
pub mod sim;

pub use flow::{Flow, Grant, Refused};
pub use qos::Qos;
pub use scheduler::{Aging, Decision, Dropped, Scheduler, Ticket};
pub use stage::{spawn_stage, Stage, StageInput};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod sim_tests;
