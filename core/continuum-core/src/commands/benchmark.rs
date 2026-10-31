//! `benchmark/*` — first-class benchmark collections, managed like the model catalog. A
//! declarative catalog of known benchmarks (add one = one row, same as a `ModelSpec`) plus
//! `benchmark/list` to see them and `benchmark/import` to adapt one INTO the work board as
//! claimable cards (a `benchmark/round` activity runs it). There is no divergent "run a benchmark" verb: every benchmark goes through
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

use crate::recipe::bench_title::dispatch_card_title;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};


// ---------------------------------------------------------------------------------------
// SWE-bench: the repo-test grader on the command surface.
//
// `swe-bench-lite` and `swe-bench-verified` have been catalog rows since this file was
// written, with `Grader::Python` documented as "catalogued; grader lands with the python
// collections". This is that grader — and it lands in Rust, because a benchmark is substrate
// ([[benchmark-infra-is-substrate-commands-handles-events-never-bash]]). The Python scripts
// it replaces produced numbers nobody could trust: eight runs scored against a clone left at
// HEAD, gold mis-scored on three instances by an id-shape assumption, and a poll loop that
// could not tell a dead dispatch from a working one.
//
// The instance's own pytest suite still runs — that is the SUBJECT under test, exactly as
// `rustc` is for `humaneval-rs`. We write no Python.
// ---------------------------------------------------------------------------------------
use crate::cognition::swe_bench::{self, SweVerdict};

use crate::recipe::benchmark_catalog::{known_benchmarks, BenchmarkSpec};

mod grader;
pub use grader::*;
mod benchmark_dispatch_params;
pub(crate) use benchmark_dispatch_params::*;
mod prepare_gym_cards;
pub(crate) use prepare_gym_cards::*;
mod publish_citizen_deck;
pub use publish_citizen_deck::*;
mod resolve_solver_dir;
pub use resolve_solver_dir::*;
mod scan_workspace_artifact_cards;
pub use scan_workspace_artifact_cards::*;
mod benchmark_fetch;

#[cfg(test)]
mod tests;
