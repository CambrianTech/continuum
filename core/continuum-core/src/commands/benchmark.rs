//! `benchmark/*` — first-class benchmark collections, managed like the model catalog. A
//! declarative catalog of known benchmarks (add one = one row, same as a `ModelSpec`) plus
//! `benchmark/list` to see them and `benchmark/dispatch` to adapt one INTO the work board as
//! claimable cards. There is no divergent "run a benchmark" verb: every benchmark goes through
//! the natural kanban loop — citizens claim a card and solve it with their own hands, and the
//! result is graded from the artifacts they produce. This lives in Rust, on the DynCommand
//! registry, so it is
//! ON-GRID: discoverable, persona-callable, and manageable by the daemons — unlike the
//! toolchain-free `benchmarks/coder/oneshot_opponent.py` script, whose ONLY job is letting an
//! OUTSIDER replicate our numbers against their own `/v1` without our stack. Operational
//! benchmarking is Rust; the replication convenience is the lone edge script.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::cognition::bench_title::{bench_card_key, dispatch_card_title};
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

pub(crate) mod credit_recovery;

use crate::cognition::swe_bench::{self, SweVerdict};

mod grader;
pub use grader::*;
mod benchmark_dispatch_params;
pub use benchmark_dispatch_params::*;
mod prepare_gym_cards;
pub(crate) use prepare_gym_cards::*;
mod benchmark_dispatch;
mod benchmark_dispatch_recipe;
mod publish_citizen_deck;
pub use publish_citizen_deck::*;
mod resolve_solver_dir;
pub use resolve_solver_dir::*;
mod fold_run_card;
pub use fold_run_card::*;
mod scan_workspace_artifact_cards;
pub use scan_workspace_artifact_cards::*;
mod benchmark_fetch;

#[cfg(test)]
mod tests;
