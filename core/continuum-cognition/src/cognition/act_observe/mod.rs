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
pub use types::{SettleOutcome, SettleStep, SettlementFailure};

mod apply;
/// Marks committed before an effect reaches the world (stop-drain S2, card 68811445).
pub mod effect_mark;
pub use effect_mark::{Effect, EffectGate, EffectMark};
pub(crate) mod disk_change;
mod page_budget;
pub use apply::{apply_act, ActChain};

mod settle;
pub use settle::{drive_to_settle_with_credit, TurnCredit};
pub use settle::{drive_to_settle, drive_to_settle_with_input, settle_step};

#[cfg(test)]
mod tests;

/// The acting fixtures this module's tests and core's integration tests share: hands that
/// return a different canned result per call (and can pause, err or feed the conversation
/// mid-act), and the body that holds them.
#[cfg(any(test, feature = "test-fixtures"))]
pub mod test_support {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use uuid::Uuid;

    use crate::ai::types::ToolCall;
    use crate::cognition::tool_executor::{
        NativeBatchOutcome, ParsedToolBatch, ToolError, ToolExecutionContext, ToolExecutor, ToolOutcome,
    };
    use crate::cognition::working_memory::WorkingMemory;
    use crate::cognition::workspace::{ActingBody, Contribution, Decision, Faculty, FacultyId, Workspace};
    use crate::cognition::admission_state::AdmissionState;
    use crate::cognition::recall_metadata::RecallMetadataRegistry;

    pub fn admission() -> Arc<AdmissionState> {
        Arc::new(AdmissionState::new(Arc::new(RecallMetadataRegistry::new())))
    }

    /// Body that shares a specific working-memory buffer — so a test can wire the
    /// SAME buffer into her view's sources and watch the
    /// perception of the persona's own hands flow act→memory→next-tick perception
    /// (the proprioception channel that replaced the deleted world-state fold).
    pub fn body_with_wm(
        executor: Arc<dyn ToolExecutor>,
        admission: Arc<AdmissionState>,
        working_memory: Arc<WorkingMemory>,
    ) -> Arc<ActingBody> {
        Arc::new(ActingBody {
            persona_id: Uuid::new_v4(),
            persona_name: "Asha".into(),
            executor,
            admission,
            working_memory,
        })
    }

    /// A `ToolExecutor` that returns a DIFFERENT canned result for each
    /// successive call — so a multi-act investigation accumulates DISTINCT
    /// observations in memory (act 1 brings back one fact, act 2 another).
    /// Models hands that probe the world and learn something new each reach.
    pub struct ScriptedExecutor {
        pub results: Mutex<std::collections::VecDeque<String>>,
        pub input: Mutex<
            std::collections::VecDeque<(
                crate::cognition::conversation::scripted_conversation::ScriptedConversationFeed,
                Vec<crate::cognition::conversation::IncomingMessage>,
            )>,
        >,
        pub rooms: Mutex<Vec<Uuid>>,
        pub pending_persona: Mutex<Option<Uuid>>,
        pub pause_before: Option<(usize, Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
        pub error_at: Option<usize>,
    }

    impl ScriptedExecutor {
        pub fn new(results: impl IntoIterator<Item = &'static str>) -> Self {
            Self {
                results: Mutex::new(results.into_iter().map(String::from).collect()),
                input: Mutex::new(std::collections::VecDeque::new()),
                rooms: Mutex::new(Vec::new()),
                pending_persona: Mutex::new(None),
                pause_before: None,
                error_at: None,
            }
        }
        pub fn with_error_at(mut self, act: usize) -> Self {
            self.error_at = Some(act);
            self
        }
        pub fn with_incoming(
            self,
            feed: crate::cognition::conversation::scripted_conversation::ScriptedConversationFeed,
            messages: Vec<crate::cognition::conversation::IncomingMessage>,
        ) -> Self {
            self.input.lock().unwrap().push_back((feed, messages));
            self
        }
    }

    #[async_trait]
    impl ToolExecutor for ScriptedExecutor {
        async fn execute_native_batch(
            &self,
            calls: &[ToolCall],
            context: &ToolExecutionContext,
        ) -> Result<NativeBatchOutcome, ToolError> {
            let act = {
                let mut rooms = self.rooms.lock().unwrap();
                rooms.push(context.context_id);
                rooms.len()
            };
            if let Some((before, entered, release)) = &self.pause_before {
                if act == *before {
                    entered.notify_one();
                    release.notified().await;
                }
            }
            if let Some((feed, messages)) = self.input.lock().unwrap().pop_front() {
                for message in messages {
                    feed.push(Ok(Some(message)));
                }
                if let Some(persona) = *self.pending_persona.lock().unwrap() {
                    crate::cognition::directed_pending::signal(persona);
                }
            }
            let content = self
                .results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| "no more results".into());
            let results = calls
                .iter()
                .map(|c| crate::ai::types::ToolResult {
                    tool_use_id: c.id.clone(),
                    content: content.clone(),
                    is_error: (self.error_at == Some(act)).then_some(true),
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

    /// The predicate the moved registry test reads (it classifies verbs off the live registry).
    pub use super::perception::wrote_without_observation;

    // The typed act and working-memory receipts the perception predicates read.
    use crate::ai::types::ToolResult;
    use crate::cognition::act_observe::{extract_paths, ActStatus, Observation, ToolOutput, ToolVerb};
    use crate::cognition::working_memory::{WmEntry, WmKind};

    // Build ONE typed act (call + id-correlated result + precomputed verb/paths)
    // the way the live act seam does — the fixtures the string scans could never
    // reach (the live receipt render is a bare `name(args)`, no `I ran ` opener,
    // so the old `"I ran code/write("` fixtures were the GREEN≠REACHABLE defect).
    pub fn act(name: &str, path: Option<&str>) -> crate::cognition::act_observe::Observation {
        let input = match path {
            Some(p) => serde_json::json!({ "file_path": p }),
            None => serde_json::json!({}),
        };
        Observation {
            call: ToolCall {
                id: "c".into(),
                name: name.into(),
                input: input.clone(),
            },
            output: ToolOutput {
                image: None,
                additional_images: None,
                result: ToolResult {
                    tool_use_id: "c".into(),
                    content: "ok".into(),
                    is_error: None,
                custody: crate::ai::types::ResultCustody::Unsaved,
                },
                verb: ToolVerb::classify(name),
                paths: extract_paths(&input),
                verdict: Default::default(),
            },
            status: ActStatus::Executed,
        }
    }

    // A receipt WmEntry carrying the typed acts (text is irrelevant to the typed
    // predicates — they read `acts`, never re-parse the string).
    pub fn receipt(acts: Vec<Observation>) -> WmEntry {
        WmEntry {
            disk_change: Default::default(),
            native_calls: Vec::new(),
            kind: WmKind::Receipt { n: 1 },
            text: String::new(),
            acts,
            scope: None,
        }
    }

    pub fn settle() -> WmEntry {
        WmEntry {
            disk_change: Default::default(),
            native_calls: Vec::new(),
            kind: WmKind::Settlement,
            text: crate::cognition::working_memory::WM_SETTLEMENT_PREFIX.to_string(),
            acts: Vec::new(),
            scope: None,
        }
    }

    /// A `ToolExecutor` that records the `context_id` it was handed and returns a
    /// canned per-call result — so a test can assert BOTH that the act was scoped
    /// to the right room and that the observation correlates call→result.
    #[derive(Clone)]
    pub struct RecordingExecutor {
        pub seen_context: Arc<Mutex<Option<Uuid>>>,
        pub result_content: String,
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

    /// Deliberation faculty: reaches for its hands once, then SETTLES into a Speak
    /// the moment it perceives a NEW act-observation it has not yet answered — the
    /// canonical act→observe arc the driver exists to run.
    ///
    /// It perceives its own hands through the working-memory proprioception channel
    /// (her trail stamps each act `[action #n]`), NOT the deleted
    /// world-state fold. It remembers the highest action stamp it has already spoken
    /// about (`responded_through`) so that across SEPARATE concerns — where the
    /// volatile buffer still carries the prior concern's action — it re-awakens and
    /// acts again instead of mistaking old proprioception for "already answered."
    /// That is the faculty remembering its own last conclusion (legitimate, content-
    /// driven), not an iteration counter in the agentic sense.
    pub struct ActThenSpeak {
        pub responded_through: std::sync::atomic::AtomicU64,
        pub admission_open: Option<Arc<std::sync::atomic::AtomicBool>>,
        /// The working memory her view reads, when the test wires one (`perceived_with`).
        pub memory: Option<Arc<crate::cognition::working_memory::WorkingMemory>>,
    }
    impl ActThenSpeak {
        pub fn new() -> Self {
            Self {
                responded_through: std::sync::atomic::AtomicU64::new(0),
                admission_open: None,
                memory: None,
            }
        }

        /// Perceive her own acts the way her view gives them, from `memory`.
        pub fn with_memory(mut self, memory: Arc<crate::cognition::working_memory::WorkingMemory>) -> Self {
            self.memory = Some(memory);
            self
        }
    }

    /// What a fixture deliberation perceives, as production's does: the broadcast, plus (when
    /// wired) what her view gives her from working memory, which is her settled acts' results
    /// (the actions source), the newest result riding whole after her view, and her trail.
    pub fn perceived_with(ws: &Workspace, memory: Option<&crate::cognition::working_memory::WorkingMemory>) -> String {
        let mut perceived = ws.perceived();
        if let Some(memory) = memory {
            let (acts, _) = memory.action_renderings();
            for act in acts {
                perceived.push('\n');
                perceived.push_str(&act.full);
            }
            if let Some(newest) = memory.pinned_active_result_block() {
                perceived.push('\n');
                perceived.push_str(&newest);
            }
            for line in memory.recent() {
                perceived.push('\n');
                perceived.push_str(&line);
            }
        }
        perceived
    }
    /// Highest `[action #N]` stamp present in the assembled perception, 0 if none.
    pub fn latest_action_seq(perceived: &str) -> u64 {
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
            let latest = latest_action_seq(&perceived_with(ws, self.memory.as_deref()));
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

    /// The one act every fixture faculty reaches for: run a tiny Rust program.
    pub fn tool_call() -> ToolCall {
        ToolCall {
            id: "call-1".into(),
            name: "code/run".into(),
            input: serde_json::json!({ "lang": "rust", "code": "fn main() { println!(\"{}\", 2 + 2); }" }),
        }
    }
}
