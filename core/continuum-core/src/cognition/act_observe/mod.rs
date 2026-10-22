//! The act→observe motion — the organism's drive to settle.
//!
//! See [docs/cognition/ACTING-ORGANISM.md]. Action is not a return value the
//! deliberation faculty loops on internally; it is a [`Decision::Act`] verdict
//! the ORGANISM drives. This module holds that drive as a free function over a
//! [`WorkspaceCycle`] — never a method on the cycle, so `run_in_room` stays a
//! pure single tick (§3.3).
//!
//! Two callers share ONE motion (the load-bearing compression):
//! - the live heartbeat (`persona::service_loop`) calls [`apply_act`] ONCE per
//!   tick and lets the metronome bring the next perception — she re-perceives at
//!   heartbeat cadence, never on a synchronous inner counter (§4 Live = no
//!   deadline);
//! - the eval grader (`modules::cognition` `cognition/eval`) calls
//!   [`drive_to_settle`], which loops `apply_act` → re-tick under an EXTERNAL
//!   budget (the grader's stopwatch — the only special power an observer holds),
//!   because the synthetic eval room has no heartbeat servicing it.
//!
//! The load-bearing choice is **result-as-engram**: executing an action admits
//! its outcome as an Episodic engram (the persona observing its own hands), so
//! the result becomes a thing the mind remembers and can be reminded of next
//! tick — unifying it with how she carries every other fact. The disposition to
//! act (build→run→test) is the GENOME's to grow, never a Rust `if`; this module
//! only gives her the hands and the memory of using them.

// This module root is a thin coordinator: the act→observe motion lives in focused
// submodules, surfaced here as the module's public API. Re-injection bounds (tool-result
// fold, echoed args) come from the persona's LIVE served window via `ContextBudget` — never
// a constant; see `cognition/context_budget.rs`.

mod perception;
mod recency;

mod observation;
pub use observation::{extract_paths, ActOutcome, ActStatus, Observation, ToolOutput, ToolVerb};

mod types;
pub use types::{SettleOutcome, SettleStep};

mod apply;
pub(crate) mod disk_change;
pub use apply::{apply_act, ActChain};

mod settle;
pub(crate) use settle::drive_to_settle_with_credit;
pub use settle::{drive_to_settle, drive_to_settle_with_input, settle_step};

#[cfg(test)]
mod tests;
