//! `agent/solve` — the headless-agent keystone. Drop a persona's WHOLE self into a working
//! directory, hand her one task, let her work it with her hands, and return the patch.
//!
//! This is the primitive every external benchmark harness composes on
//! ([[terminal-bench-is-the-meta-harness-build-one-agent-adapter]],
//! `docs/architecture/BENCHMARK-HARNESS-INTEGRATION.md`): Terminal-Bench, SWE-bench, Aider
//! Polyglot all reduce to "put an agent in a sandbox cwd with a task, collect the diff, grade
//! with our own tests." A benchmark being "LLM-shaped" is an ergonomics problem solved by the
//! RAG (the task is layered into her situation) — NEVER by stripping her
//! ([[benchmark-must-never-score-persona-against-a-soul-stripped-copy]]).
//!
//! She competes WHOLE: `with_tools: true` (hands on), `suppress_recall: false` (memory/RAG on),
//! her genome paged in on the measurement lane. Same drive machinery as `cognition/eval`
//! (`fork_eval_cycle_with_adapter` → `drive_to_settle`), minus the grader — the external harness
//! grades. One drive, two consumers (eval scores; agent/solve emits a patch).

use crate::time::EpochMs;
use crate::cognition::learning_policy::LearningPolicy;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use ts_rs::TS;
use uuid::Uuid;

use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

/// What is NOT part of a solution lives in ONE place, beside the other reading of her work:
/// [`crate::commands::benchmark::SOLUTION_PATH_EXCLUDES`]. This file used to carry its own
/// near-copy that omitted `.airc`, which is precisely the drift `workspace_candidate_diff`'s
/// doc warned about — see the shared constant for the two incidents.
use crate::commands::benchmark::SOLUTION_PATH_EXCLUDES as PATCH_EXCLUDES;

mod default_max_acts;
pub use default_max_acts::*;
mod agent_solve;
pub use agent_solve::*;
mod with;
pub use with::*;
mod agent_solve_2;
pub use agent_solve_2::*;
mod lesson_task_excerpt_chars;
pub use lesson_task_excerpt_chars::*;

#[cfg(test)]
mod tests;
