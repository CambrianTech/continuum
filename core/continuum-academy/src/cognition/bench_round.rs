//! `BenchRound` — what is left of the round tracker after the detached runner went
//! (card c4f4b8cd step 2). In memory only, never persisted, and nothing in production
//! opens a round any more: a benchmark is an adapter that posts cards into an activity's
//! room, and the room runs it (docs/architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md).
//!
//! The readers below remain because step 3 of c4f4b8cd still has to move their callers
//! onto the board (the mind sites: `verdict_board`, `training_producer`, `act_question`,
//! `airc_runtime`'s lapsed-hold recovery, the nav and presence emitters, the eval
//! experience sink). Each answers "no round" for every card until then. When step 3
//! lands, this file is deleted.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use serde_json::Value;
use uuid::Uuid;

mod work_driver;
pub use work_driver::*;
mod review_parent;
pub use review_parent::*;
mod round_card_snapshot;
pub use round_card_snapshot::*;
#[cfg(test)]
mod tests;