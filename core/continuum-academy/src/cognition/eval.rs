//! `cognition/eval` — the test-graded coder gym as a first-class command.
//!
//! Drives a held-out CODER eval through a persona's LIVE cognition — the SAME
//! spawned [`WorkspaceCycle`](super::persona_workspace), the same model, faculties,
//! tools, and `GridTrustAuthPolicy` gate as a real room turn. There is no alternate
//! "eval mind": she really thinks. The only special power the grader holds is the
//! stopwatch — a synthetic eval room has no heartbeat to pace re-perception, so the
//! OBSERVER bounds how many act→observe cycles a task may take (`max_acts`) before
//! it counts as unfinished ([[live-prompt-comes-from-workspacecycle-not-airc-source]],
//! ACTING-ORGANISM.md §4).
//!
//! This is how we DETECT whether a change (a trained LoRA, a prompt, a better base
//! model) actually made her a better coder — the number, not a vibe (SELF-EVOLVING-
//! GENOME §6 slice 1: until lift is real, every later slice is a hypothesis).
//!
//! Grading is OBJECTIVE when a task carries a `test`: take her code, append the
//! test, RUN it, pass = exit 0 (the P1 keystone of ROADMAP-TO-CODING-ITSELF — "did
//! her change make the tests pass?", not substring-on-prose). Descriptive tasks
//! (no `test`) fall back to a case-insensitive substring match on `expect`.
//!
//! Access: `Privileged` → `Trusted`, same tier as [`cognition/trace`] and
//! [`cognition/prompt`]. Driving another mind through a gym (and executing the code
//! it writes) is for trusted local citizens and the owner, never an arbitrary
//! remote `Provisional` peer.
//!
//! [`cognition/trace`]: super::introspect_commands::CognitionTrace
//! [`cognition/prompt`]: super::introspect_commands::CognitionPrompt

use continuum_base::time::EpochMs;
use crate::cognition::learning_policy::LearningPolicy;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

mod default_eval_set;
pub(crate) use default_eval_set::*;
mod spawn_gene_eval_lane;
pub use spawn_gene_eval_lane::*;
mod sweep_orphan_eval_roots;
pub use sweep_orphan_eval_roots::*;
mod eval_task_result;
pub use eval_task_result::*;
mod cognition_eval;
mod format_exam_lesson;
pub use format_exam_lesson::*;
mod fork_eval_cycle_waiting;
pub use fork_eval_cycle_waiting::*;
mod infra_verdict;
use infra_verdict::*;
mod run_pass;
use run_pass::*;
mod eval_settle;
use eval_settle::*;
mod teacher_synthesis;
mod teacher_serving;
pub use teacher_serving::{TeacherBatchRequest, TeacherServing};
mod warm_gate;
use warm_gate::*;
mod gene_exam;
pub use gene_exam::MindExamHost;
pub use teacher_synthesis::*;

#[cfg(test)]
mod tests;
