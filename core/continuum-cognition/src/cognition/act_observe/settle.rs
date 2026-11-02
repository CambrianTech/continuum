//! Settlement stepping: `settle_step` — the ONE place a `Decision` becomes
//! speech-or-action — and `drive_to_settle`, which loops steps under an external
//! budget until the mind settles. Extracted verbatim from `act_observe` (pure
//! code-motion, #386 decomposition).
//!
//! `settle_step` is shared by the live heartbeat (called ONCE per metronome tick)
//! and the eval driver (`drive_to_settle`, which loops because the grader replaces
//! the metronome). Live and eval make a turn the IDENTICAL way; only pacing differs.

use continuum_base::time::EpochMs;
use std::sync::Arc;
use uuid::Uuid;

use crate::cognition::workspace::{Burst, Decision, Situation, TurnFraming, TurnMetrics, WorkspaceCycle};

use super::apply::apply_act;
use super::perception::{any_real_receipt, batch_evidence, claimed_file_without_act, collect_touched_paths, mutated_workspace, ConcernPhase, wrote_without_observation};
use super::types::{SettleOutcome, SettleStep, SettlementFailure};
use crate::inference::turn_bound::TICK_DEADLINE;

mod tick_deadline;
mod turn_credit;
pub use turn_credit::TurnCredit;
pub use tick_deadline::*;
mod settle_to_outcome;
use settle_to_outcome::*;
mod settle_step;
pub use settle_step::*;

#[cfg(test)]
mod tests;
