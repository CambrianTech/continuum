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
use crate::persona::admission_state::AdmissionState;
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::Mutex;
use crate::cognition::working_memory::WorkingMemory;

/// A `ToolExecutor` that records the `context_id` it was handed and returns a
/// canned per-call result — so a test can assert BOTH that the act was scoped
/// to the right room and that the observation correlates call→result.
#[derive(Clone)]
struct RecordingExecutor {
    seen_context: Arc<Mutex<Option<Uuid>>>,
    result_content: String,
}

#[async_trait]
impl ToolExecutor for RecordingExecutor {
    fn in_hands(&self, _persona: Uuid, _context: crate::identity::hands_context::HandsContext) -> Option<Arc<dyn ToolExecutor>> {
        Some(Arc::new(self.clone()))
    }
    async fn execute_native_batch(
        &self,
        calls: &[ToolCall],
        context: &ToolExecutionContext,
        _max_result_chars: usize,
    ) -> Result<NativeBatchOutcome, ToolError> {
        *self.seen_context.lock().unwrap() = Some(context.context_id);
        let results = calls
            .iter()
            .map(|c| crate::ai::types::ToolResult {
                tool_use_id: c.id.clone(),
                content: self.result_content.clone(),
                is_error: None,
                custody: crate::ai::types::ResultCustody::Unsaved,
            })
            .collect();
        Ok(NativeBatchOutcome {
            results,
            media: Vec::new(),
            stored_ids: Vec::new(),
            // Test doubles project nothing — the act seam must behave exactly
            // as it does for a command that never opted in.
            verdicts: Vec::new(),
        })
    }

    async fn parse_response(
        &self,
        _response_text: &str,
        _model_family: Option<&str>,
    ) -> Result<ParsedToolBatch, ToolError> {
        Ok(ParsedToolBatch {
            tool_calls: Vec::new(),
            cleaned_text: String::new(),
            parse_time_us: 0,
        })
    }

    async fn store_outcome(
        &self,
        _outcome: &ToolOutcome,
        _context: &ToolExecutionContext,
    ) -> Result<Uuid, ToolError> {
        Ok(Uuid::nil())
    }
}

/// An executor whose batch always fails at the batch level (channel down).
struct FailingExecutor;
#[async_trait]
impl ToolExecutor for FailingExecutor {
    async fn execute_native_batch(
        &self,
        _calls: &[ToolCall],
        _context: &ToolExecutionContext,
        _max_result_chars: usize,
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

/// Deliberation faculty: reaches for its hands once, then SETTLES into a Speak
/// the moment it perceives a NEW act-observation it has not yet answered — the
/// canonical act→observe arc the driver exists to run.
///
/// It perceives its own hands through the working-memory proprioception channel
/// (the `WorkingMemoryFaculty` stamps each act `[action #n]`), NOT the deleted
/// world-state fold. It remembers the highest action stamp it has already spoken
/// about (`responded_through`) so that across SEPARATE concerns — where the
/// volatile buffer still carries the prior concern's action — it re-awakens and
/// acts again instead of mistaking old proprioception for "already answered."
/// That is the faculty remembering its own last conclusion (legitimate, content-
/// driven), not an iteration counter in the agentic sense.
struct ActThenSpeak {
    responded_through: std::sync::atomic::AtomicU64,
    admission_open: Option<Arc<std::sync::atomic::AtomicBool>>,
}
impl ActThenSpeak {
    fn new() -> Self {
        Self {
            responded_through: std::sync::atomic::AtomicU64::new(0),
            admission_open: None,
        }
    }
}
/// Highest `[action #N]` stamp present in the assembled perception, 0 if none.
fn latest_action_seq(perceived: &str) -> u64 {
    perceived
        .split("[action #")
        .skip(1)
        .filter_map(|s| {
            let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse::<u64>().ok()
        })
        .max()
        .unwrap_or(0)
}
#[async_trait]
impl Faculty for ActThenSpeak {
    fn id(&self) -> FacultyId {
        FacultyId::Deliberation
    }
    fn reacts_to_broadcast(&self) -> bool {
        true
    }
    async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
        if self.admission_open.as_ref().is_some_and(|gate| !gate.load(std::sync::atomic::Ordering::SeqCst)) {
            return Some(Contribution::deliberation_fault("lane starved before dispatch"));
        }

        use std::sync::atomic::Ordering;
        let latest = latest_action_seq(&ws.perceived());
        if latest > self.responded_through.load(Ordering::Relaxed) {
            self.responded_through.store(latest, Ordering::Relaxed);
            Some(Contribution::verdict(
                Decision::Speak {
                    text: "the answer is 4".into(),
                },
                0.9,
                "settled after observing a fresh result",
            ))
        } else {
            Some(Contribution::verdict(
                Decision::Act {
                    calls: vec![tool_call()],
                    intent: "run the code".into(),
                },
                0.9,
                "reaching for hands",
            ))
        }
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

fn tool_call() -> ToolCall {
    ToolCall {
        id: "call-1".into(),
        name: "code/run".into(),
        input: serde_json::json!({ "lang": "rust", "code": "fn main() { println!(\"{}\", 2 + 2); }" }),
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

