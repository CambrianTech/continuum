//! Shared fixtures for the act_observe tests: stand-in executors, faculties and
//! bodies. Scenario files sit beside this one and pull these in via `use super::*;`.

#![cfg(test)]

mod apply;
mod chain;
mod credit;
mod drive;
mod investigation;

use super::*;
use super::test_support::*;
use crate::cognition::workspace::Decision;
use crate::cognition::workspace::ActingBody;
use crate::cognition::workspace::Contribution;
use crate::cognition::workspace::Faculty;
use crate::cognition::workspace::FacultyId;
use crate::cognition::workspace::Workspace;
use crate::cognition::tool_executor::NativeBatchOutcome;
use crate::cognition::tool_executor::ParsedToolBatch;
use crate::cognition::tool_executor::ToolError;
use crate::cognition::tool_executor::ToolExecutionContext;
use crate::cognition::tool_executor::ToolExecutor;
use crate::cognition::tool_executor::ToolOutcome;
use crate::ai::types::ToolCall;
use uuid::Uuid;
use crate::cognition::admission_state::AdmissionState;
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::Mutex;
use crate::cognition::working_memory::WorkingMemory;

/// An executor whose batch always fails at the batch level (channel down).
struct FailingExecutor;
#[async_trait]
impl ToolExecutor for FailingExecutor {
    async fn execute_native_batch(
        &self,
        _calls: &[ToolCall],
        _context: &ToolExecutionContext,
    ) -> Result<NativeBatchOutcome, ToolError> {
        Err(ToolError::ExecutionFailed {
            tool: "code/run".into(),
            underlying: "ipc channel down".into(),
        })
    }
    async fn parse_response(
        &self,
        _t: &str,
        _f: Option<&str>,
    ) -> Result<ParsedToolBatch, ToolError> {
        Ok(ParsedToolBatch {
            tool_calls: Vec::new(),
            cleaned_text: String::new(),
            parse_time_us: 0,
        })
    }
    async fn store_outcome(
        &self,
        _o: &ToolOutcome,
        _c: &ToolExecutionContext,
    ) -> Result<Uuid, ToolError> {
        Ok(Uuid::nil())
    }
}

/// Deliberation faculty that NEVER settles — always wants to act again. Models
/// the "acts forever" fitness gap the external grader bounds with `max_acts`.
struct AlwaysAct;
#[async_trait]
impl Faculty for AlwaysAct {
    fn id(&self) -> FacultyId {
        FacultyId::Deliberation
    }
    fn reacts_to_broadcast(&self) -> bool {
        true
    }
    async fn contribute(&self, _ws: &Workspace) -> Option<Contribution> {
        Some(Contribution::verdict(
            Decision::Act {
                calls: vec![tool_call()],
                intent: "act again".into(),
            },
            0.9,
            "never settles",
        ))
    }
}

/// Only ever speaks, and COUNTS how many generations it was asked for — the
/// instrument for "did the drive hand her another tick, or settle on the first
/// Speak?".
struct CountingSpeaker {
    generations: Mutex<usize>,
}
impl CountingSpeaker {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            generations: Mutex::new(0),
        })
    }
    fn generations(&self) -> usize {
        *self.generations.lock().expect("lock")
    }
}
#[async_trait]
impl Faculty for CountingSpeaker {
    fn id(&self) -> FacultyId {
        FacultyId::Deliberation
    }
    fn reacts_to_broadcast(&self) -> bool {
        true
    }
    async fn contribute(&self, _ws: &Workspace) -> Option<Contribution> {
        *self.generations.lock().expect("lock") += 1;
        Some(Contribution::verdict(
            Decision::Speak {
                text: "here is my analysis of the bug: the call to subs() is wrong".into(),
            },
            0.95,
            "explaining rather than editing",
        ))
    }
}


/// Unwrap the typed acts of an `Acted` outcome (panics on NoHands/ExecutorError) —
/// the typed sibling of the old `.expect("acted")` on the `Option<String>`.
fn acts_of(outcome: ActOutcome) -> Vec<Observation> {
    match outcome {
        ActOutcome::Acted { acts } => acts,
        other => panic!("expected Acted, got {other:?}"),
    }
}

fn body(executor: Arc<dyn ToolExecutor>, admission: Arc<AdmissionState>) -> Arc<ActingBody> {
    body_with_wm(executor, admission, Arc::new(WorkingMemory::new(3)))
}

