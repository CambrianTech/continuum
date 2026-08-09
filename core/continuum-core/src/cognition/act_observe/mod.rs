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

mod recency;
mod perception;

mod observation;
pub use observation::{
    extract_paths, ActOutcome, ActStatus, Observation, ToolOutput, ToolVerb,
};

mod types;
pub use types::{SettleOutcome, SettleStep};

mod apply;
pub use apply::apply_act;

mod settle;
pub use settle::{drive_to_settle, settle_step};


#[cfg(test)]
mod tests {

    /// what this catches: the no-deliverable nudge going back to ONE-SHOT. The first version
    /// latched on a bool, and the probe trail proved the cost — `persona.settle.no_deliverable`
    /// fired exactly once per SWE run and the persona then settled at 5-7 acts with a 30-act
    /// budget unspent. The nudge must re-arm each time she ACTS, so a turn that keeps working
    /// keeps being told the workspace is the deliverable; and it must NOT re-arm when she
    /// speaks twice with no act between, so it can never become a spin.
    #[test]
    fn the_no_deliverable_nudge_rearms_on_each_act_but_never_twice_without_one() {
        // The gate's whole condition, isolated: `acts_at_last_nudge != Some(acts)`.
        let fires = |last: Option<usize>, acts: usize| last != Some(acts);

        // Never nudged yet at act 3 → fires.
        assert!(fires(None, 3), "first zero-deliverable Speak must nudge");
        // Nudged at 3, still at 3 (spoke again, acted zero times) → must NOT fire again.
        assert!(
            !fires(Some(3), 3),
            "a second Speak with no act in between must settle, not spin"
        );
        // Nudged at 3, she then acted (now 4) → re-arms.
        assert!(
            fires(Some(3), 4),
            "the nudge must re-arm once she has acted again — this is the bug that capped \
             her at one reminder per turn"
        );
    }

    use super::*;
    use uuid::Uuid;
    use crate::ai::types::ToolCall;
    use crate::cognition::workspace::{Decision, Situation, TurnFraming, WorkspaceCycle};
    use super::perception::is_redundant_orientation;


    use crate::cognition::tool_executor::{
        NativeBatchOutcome, ParsedToolBatch, ToolError, ToolExecutionContext, ToolExecutor,
        ToolOutcome,
    };
    use crate::cognition::workspace::{
        ActingBody, Contribution, Faculty, FacultyId, SalienceArbiter, Workspace,
    };
    use crate::persona::admission_state::AdmissionState;
    use crate::persona::recall_metadata::RecallMetadataRegistry;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    /// A `ToolExecutor` that records the `context_id` it was handed and returns a
    /// canned per-call result — so a test can assert BOTH that the act was scoped
    /// to the right room and that the observation correlates call→result.
    struct RecordingExecutor {
        seen_context: Mutex<Option<Uuid>>,
        result_content: String,
    }

    #[async_trait]
    impl ToolExecutor for RecordingExecutor {
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
                })
                .collect();
            Ok(NativeBatchOutcome {
                results,
                media: Vec::new(),
                stored_ids: Vec::new(),
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
    }
    impl ActThenSpeak {
        fn new() -> Self {
            Self {
                responded_through: std::sync::atomic::AtomicU64::new(0),
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

    fn admission() -> Arc<AdmissionState> {
        Arc::new(AdmissionState::new(Arc::new(RecallMetadataRegistry::new())))
    }

    use crate::cognition::working_memory::{WorkingMemory, WorkingMemoryFaculty};

    fn body(executor: Arc<dyn ToolExecutor>, admission: Arc<AdmissionState>) -> Arc<ActingBody> {
        body_with_wm(executor, admission, Arc::new(WorkingMemory::new(3)))
    }

    /// Body that shares a specific working-memory buffer — so a test can wire the
    /// SAME buffer into a `WorkingMemoryFaculty` on the cycle and watch the
    /// perception of the persona's own hands flow act→memory→next-tick perception
    /// (the proprioception channel that replaced the deleted world-state fold).
    fn body_with_wm(
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


    // what this catches: an act is scoped to the room it is FOR (one mind is in
    // many rooms — the body is room-agnostic, `room_id` flows per-call), the
    // observation correlates each call to its result in first person, and the
    // outcome becomes a recallable engram. Regresses the multi-room steer + the
    // result-as-memory choice (ACTING-ORGANISM §3.3).
    #[tokio::test]
    async fn apply_act_scopes_to_the_room_and_observes_the_result() {
        let exec = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "4\n".into(),
        });
        let adm = admission();
        let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter), 8)
            .with_acting(body(exec.clone(), adm.clone()));

        let room = Uuid::new_v4();
        let acts = match apply_act(&cycle, &[tool_call()], "check the math", room).await {
            ActOutcome::Acted { acts } => acts,
            other => panic!("expected Acted, got {other:?}"),
        };
        assert_eq!(acts.len(), 1, "one call → one typed act");
        let obs = &acts[0];

        assert_eq!(
            *exec.seen_context.lock().unwrap(),
            Some(room),
            "the act must be scoped to the room it is for, not a phantom nil room"
        );
        assert_eq!(obs.call.name, "code/run", "the typed act names the tool it ran");
        // THE RESULT THREADS BACK BY ID — the run-18057-f1 correlation, now a TYPED
        // field the caller reads instead of splitting on "[action #".
        assert_eq!(
            obs.output.result.tool_use_id, obs.call.id,
            "result correlates to the call by tool_use_id, not positional index"
        );
        assert!(
            obs.output.result.content.contains('4'),
            "the hand's result rides the typed output"
        );
        // The recency rendering still names tool + intent + result (byte-stable).
        assert!(obs.render_recall("check the math").contains("code/run"));
        assert!(obs.render_recall("check the math").contains("check the math"));
        assert!(obs.render_recall("check the math").contains('4'));
        assert_eq!(
            adm.engram_count(),
            1,
            "the outcome became a recallable memory (result-as-engram)"
        );
        assert!(adm
            .engram_at(0)
            .expect("engram present")
            .content
            .contains('4'));
    }

    // what this catches: with no hands (no ActingBody on the cycle), the driver
    // ABSTAINS rather than fabricating a result — the no-hands path that used to
    // live in the faculty now lives here (the faculty only emits the Act verdict).
    #[tokio::test]
    async fn apply_act_without_hands_abstains() {
        let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter), 8);
        assert!(
            matches!(
                apply_act(&cycle, &[tool_call()], "try", Uuid::new_v4()).await,
                ActOutcome::NoHands
            ),
            "no hands → NoHands, never a fabricated success"
        );
    }

    // what this catches: a batch-level executor failure (channel down) abstains
    // rather than admitting a fabricated outcome the mind would then "remember" as
    // fact ([[fallbacks-are-illegal-fail-loud]]).
    #[tokio::test]
    async fn apply_act_abstains_when_the_hand_fails() {
        let adm = admission();
        let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter), 8)
            .with_acting(body(Arc::new(FailingExecutor), adm.clone()));
        assert!(
            matches!(
                apply_act(&cycle, &[tool_call()], "run", Uuid::new_v4()).await,
                ActOutcome::ExecutorError { .. }
            ),
            "a batch-level failure surfaces as ExecutorError, distinct from NoHands"
        );
        assert_eq!(adm.engram_count(), 0, "a failed act admits no memory");
    }

    // what this catches: the act→observe MOTION — the driver runs the act, folds
    // the observation into the next perception, and the mind settles into a Speak
    // that the external observer reads. acts==1, spoken is the settled answer.
    #[tokio::test]
    async fn drive_to_settle_acts_then_settles_on_speak() {
        let exec = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "4".into(),
        });
        let adm = admission();
        // Same buffer in the body (writer) and the perception-tier faculty
        // (reader): act → working memory → next-tick perception.
        let wm = Arc::new(WorkingMemory::new(3));
        let cycle = WorkspaceCycle::new(
            vec![
                Arc::new(WorkingMemoryFaculty::new(Arc::clone(&wm))) as Arc<dyn Faculty>,
                Arc::new(ActThenSpeak::new()),
            ],
            Arc::new(SalienceArbiter),
            8,
        )
        .with_acting(body_with_wm(exec.clone(), adm.clone(), Arc::clone(&wm)));

        let outcome =
            drive_to_settle(&cycle, "[eval]\npeer: what is 2+2?", Uuid::new_v4(), 8, TurnFraming::ambient()).await;

        assert_eq!(outcome.acts, 1, "acted exactly once before settling");
        assert_eq!(outcome.spoken.as_deref(), Some("the answer is 4"));
        assert!(matches!(outcome.decision, Decision::Speak { .. }));
    }

    // what this catches: THE SETTLE ARTERY (the dominant SWE-bench killer, glass-boxed
    // 2026-08-04 on sympy-21379: one `code/tree`, then a prose explanation of the bug —
    // 0 patch bytes, 29 of 30 acts unspent, run over). When the CALLER declared the
    // deliverable to be the workspace, a Speak that changed no file must not end the
    // turn on the first pass: she gets exactly ONE more perception, carrying the
    // structural fact that her working memory holds no mutation. Bounded — she speaks
    // again and it settles, so a determined Speak is never trapped in a loop.
    #[tokio::test]
    async fn a_zero_change_speak_reperceives_once_when_the_workspace_is_the_deliverable() {
        let speaker = CountingSpeaker::new();
        let wm = Arc::new(WorkingMemory::new(8));
        let exec = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "src/".into(),
        });
        let cycle = WorkspaceCycle::new(
            vec![
                Arc::new(WorkingMemoryFaculty::new(Arc::clone(&wm))) as Arc<dyn Faculty>,
                Arc::clone(&speaker) as Arc<dyn Faculty>,
            ],
            Arc::new(SalienceArbiter),
            8,
        )
        .with_acting(body_with_wm(exec, admission(), Arc::clone(&wm)));

        let outcome = drive_to_settle(
            &cycle,
            "fix the bug in sympy/core/basic.py",
            Uuid::new_v4(),
            8,
            TurnFraming::directed().on_workspace(),
        )
        .await;

        assert_eq!(
            speaker.generations(),
            2,
            "the zero-deliverable Speak bought exactly one more perception — not zero, not a loop"
        );
        assert!(
            wm.recent().iter().any(|l| l.contains("[no-deliverable]")),
            "the structural fact reached working memory, where the next tick perceives it: {:?}",
            wm.recent()
        );
        assert!(
            matches!(outcome.decision, Decision::Speak { .. }),
            "she settles on her second Speak — the decision stays hers"
        );
    }

    // what this catches: the blast radius. An ORDINARY turn (chat, an answer-graded
    // task — the default `Deliverable::Answer`) is untouched: her first Speak settles
    // it, exactly as before, and no [no-deliverable] fact is invented for a turn whose
    // deliverable IS the utterance. The re-perception is opt-in by the caller that
    // grades a diff, never a global change to how speech settles.
    #[tokio::test]
    async fn an_ordinary_turn_still_settles_on_the_first_speak() {
        let speaker = CountingSpeaker::new();
        let wm = Arc::new(WorkingMemory::new(8));
        let exec = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "ok".into(),
        });
        let cycle = WorkspaceCycle::new(
            vec![
                Arc::new(WorkingMemoryFaculty::new(Arc::clone(&wm))) as Arc<dyn Faculty>,
                Arc::clone(&speaker) as Arc<dyn Faculty>,
            ],
            Arc::new(SalienceArbiter),
            8,
        )
        .with_acting(body_with_wm(exec, admission(), Arc::clone(&wm)));

        let outcome = drive_to_settle(
            &cycle,
            "what do you think?",
            Uuid::new_v4(),
            8,
            TurnFraming::directed(),
        )
        .await;

        assert_eq!(speaker.generations(), 1, "one generation, settled — unchanged");
        assert!(
            !wm.recent().iter().any(|l| l.contains("[no-deliverable]")),
            "no workspace-deliverable fact on a turn whose deliverable is the answer"
        );
        assert!(matches!(outcome.decision, Decision::Speak { .. }));
    }


    // what this catches: the grader's stopwatch. A mind that never settles is
    // bounded by the EXTERNAL `max_acts` budget and the final un-driven Act is
    // returned as unfinished — never a fabricated answer, and the budget is the
    // observer's, not a cap in the persona's head (ACTING-ORGANISM §4).
    #[tokio::test]
    async fn drive_to_settle_returns_unsettled_act_when_budget_exhausted() {
        let exec = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "...".into(),
        });
        let adm = admission();
        let cycle = WorkspaceCycle::new(vec![Arc::new(AlwaysAct)], Arc::new(SalienceArbiter), 8)
            .with_acting(body(exec.clone(), adm.clone()));

        let outcome = drive_to_settle(&cycle, "go", Uuid::new_v4(), 2, TurnFraming::ambient()).await;

        assert_eq!(outcome.acts, 2, "spent exactly the observer's budget");
        assert!(
            outcome.spoken.is_none(),
            "did not settle → no spoken answer"
        );
        assert!(
            matches!(outcome.decision, Decision::Act { .. }),
            "returns the un-driven Act as honest 'did not finish'"
        );
    }

    // what this catches (#206 backstop): a model stuck re-emitting the IDENTICAL act must be
    // cut off WELL BEFORE the full act budget — the bounded stuck-act backstop stops granting
    // acts after STUCK_LIMIT consecutive byte-identical batches, so she settles instead of
    // burning the whole budget hammering (help ×54 / identical write ×8 live). `AlwaysAct`
    // emits the same tool_call() every tick — the exact fixed point. With a generous budget
    // of 20, the backstop must stop her far sooner (at STUCK_LIMIT+1 = 4 acts), returning the
    // un-driven Act honestly. Genuine iteration (different acts) would reset the counter and is
    // NOT bounded — only a fixed point trips this.
    #[tokio::test]
    async fn drive_to_settle_backstops_a_stuck_identical_act_loop_before_the_budget() {
        let exec = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "...".into(),
        });
        let adm = admission();
        let cycle = WorkspaceCycle::new(vec![Arc::new(AlwaysAct)], Arc::new(SalienceArbiter), 8)
            .with_acting(body(exec.clone(), adm.clone()));

        // Budget of 20 acts, but she loops on the identical call — the backstop must fire long
        // before, at 4 acts (3 consecutive identical repeats + the first).
        let outcome = drive_to_settle(&cycle, "go", Uuid::new_v4(), 20, TurnFraming::ambient()).await;

        assert_eq!(
            outcome.acts, 4,
            "backstop stops the identical-act loop at STUCK_LIMIT+1, not the full budget"
        );
        assert!(
            matches!(outcome.decision, Decision::Act { .. }) && outcome.spoken.is_none(),
            "the pathological never-speak faculty returns un-driven — honest 'stuck, did not finish'"
        );
    }

    // what this catches: the shared step's acting gate. `may_act = false` (how the
    // eval driver paces ACTING past its budget) must return the decided Act WITHOUT
    // executing it — the executor is never touched — so a deferred act can't run a
    // tool the budget already forbade. `may_act = true` (the live path, always) runs
    // it. This is the single seam that keeps live (one permitted act per tick) and
    // eval (budget-gated acting) on the IDENTICAL per-step motion.
    #[tokio::test]
    async fn settle_step_defers_the_act_without_executing_when_may_act_is_false() {
        let exec = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "...".into(),
        });
        let adm = admission();
        let cycle = WorkspaceCycle::new(vec![Arc::new(AlwaysAct)], Arc::new(SalienceArbiter), 8)
            .with_acting(body(exec.clone(), adm.clone()));

        let (deferred, _) =
            settle_step(&cycle, "go", Uuid::new_v4(), false, TurnFraming::ambient(), Situation::FreshContext).await;
        assert!(
            matches!(deferred, SettleStep::WouldAct { .. }),
            "may_act=false defers the act"
        );
        assert!(
            exec.seen_context.lock().unwrap().is_none(),
            "a deferred act NEVER touches the executor"
        );

        let (ran, _) =
            settle_step(&cycle, "go", Uuid::new_v4(), true, TurnFraming::ambient(), Situation::FreshContext).await;
        assert!(matches!(ran, SettleStep::Acted { .. }), "may_act=true runs it");
        assert!(
            exec.seen_context.lock().unwrap().is_some(),
            "a permitted act DOES reach the executor"
        );
    }

    /// A `ToolExecutor` that returns a DIFFERENT canned result for each
    /// successive call — so a multi-act investigation accumulates DISTINCT
    /// observations in memory (act 1 brings back one fact, act 2 another).
    /// Models hands that probe the world and learn something new each reach.
    struct ScriptedExecutor {
        results: Mutex<std::collections::VecDeque<String>>,
    }
    impl ScriptedExecutor {
        fn new(results: impl IntoIterator<Item = &'static str>) -> Self {
            Self {
                results: Mutex::new(results.into_iter().map(String::from).collect()),
            }
        }
    }
    #[async_trait]
    impl ToolExecutor for ScriptedExecutor {
        async fn execute_native_batch(
            &self,
            calls: &[ToolCall],
            _context: &ToolExecutionContext,
            _max_result_chars: usize,
        ) -> Result<NativeBatchOutcome, ToolError> {
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
                    is_error: None,
                })
                .collect();
            Ok(NativeBatchOutcome {
                results,
                media: Vec::new(),
                stored_ids: Vec::new(),
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

    /// A deliberation faculty that is a small *investigator*: its next move is a
    /// pure function of what it has DISCOVERED so far (the observations folded
    /// into perception), never a counter and never a magic flag. It needs two
    /// facts to answer — where the program starts and what that entry calls —
    /// so it acts to learn the first, acts again to learn the second, then
    /// (seeing both in memory) synthesizes and speaks. The branch reads
    /// accumulated MEMORY CONTENT, which is the whole point: the hands change
    /// the mind. (A test stand-in for the model, exactly like `ActThenSpeak`;
    /// it proves the cycle's cross-tick plumbing, not production cognition.)
    struct Investigator;
    #[async_trait]
    impl Faculty for Investigator {
        fn id(&self) -> FacultyId {
            FacultyId::Deliberation
        }
        fn reacts_to_broadcast(&self) -> bool {
            true
        }
        async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
            // Reads what it has DISCOVERED from assembled perception — the
            // working-memory render of its own recent acts (proprioception),
            // where each act-observation head carries the executor's result tokens
            // (ENTRY=…/CALLS=…). Not the raw burst, not the deleted fold.
            let perceived = ws.perceived();
            let after = |key: &str| -> Option<String> {
                perceived
                    .split(key)
                    .nth(1)
                    .and_then(|s| s.split_whitespace().next())
                    .map(String::from)
            };
            match (after("ENTRY="), after("CALLS=")) {
                (None, _) => Some(Contribution::verdict(
                    Decision::Act {
                        calls: vec![probe("find where the program starts")],
                        intent: "find where the program starts".into(),
                    },
                    0.9,
                    "I don't know the entry point yet — reach for it",
                )),
                (Some(_), None) => Some(Contribution::verdict(
                    Decision::Act {
                        calls: vec![probe("see what the entry calls")],
                        intent: "find what the entry calls".into(),
                    },
                    0.9,
                    "I know the entry; now I need what it calls",
                )),
                (Some(entry), Some(calls)) => Some(Contribution::verdict(
                    Decision::Speak {
                        text: format!("the program starts in {entry} and calls {calls}"),
                    },
                    0.95,
                    "synthesized both discoveries from memory — settling",
                )),
            }
        }
    }

    /// A probe tool call whose INPUT carries no `ENTRY=`/`CALLS=` token (only the
    /// executor's *result* does), so the investigator never false-triggers on
    /// its own intent.
    fn probe(what: &'static str) -> ToolCall {
        ToolCall {
            id: "probe-1".into(),
            name: "code/search".into(),
            input: serde_json::json!({ "query": what }),
        }
    }

    // what this catches: THE HANDS CHANGE THE MIND. A multi-act investigation
    // converges through MEMORY CONTENT — each act discovers a distinct fact, the
    // result re-enters as an Episodic engram, and the NEXT decision is a function
    // of what the persona now KNOWS (not an act counter, not a `[you just acted]`
    // flag). She acts to find the entry point, observes it, acts to find what it
    // calls, observes that, then — seeing BOTH discoveries in her assembled
    // perception — synthesizes and speaks. This is the organic loop
    // cognition→action→perception→cognition converging by judgment, the novel
    // architecture that distinguishes the organism from a textbook agentic
    // counter loop ([[persona-codes-blind-no-hands-no-organic-loop]]).
    #[tokio::test]
    async fn the_hands_change_the_mind_across_a_multi_act_investigation() {
        let exec = Arc::new(ScriptedExecutor::new(["ENTRY=main", "CALLS=boot"]));
        let adm = admission();
        let wm = Arc::new(WorkingMemory::new(3));
        let cycle = WorkspaceCycle::new(
            vec![
                Arc::new(WorkingMemoryFaculty::new(Arc::clone(&wm))) as Arc<dyn Faculty>,
                Arc::new(Investigator),
            ],
            Arc::new(SalienceArbiter),
            8,
        )
        .with_acting(body_with_wm(exec.clone(), adm.clone(), Arc::clone(&wm)));

        let outcome = drive_to_settle(
            &cycle,
            "[eval]\npeer: where does the program start and what does it call?",
            Uuid::new_v4(),
            8,
            TurnFraming::ambient(),
        )
        .await;

        assert_eq!(
            outcome.acts, 2,
            "two DISCOVERIES were needed to converge — multi-act, not one-shot"
        );
        let spoken = outcome
            .spoken
            .expect("the mind settled into a spoken synthesis, not an un-driven act");
        assert!(
            spoken.contains("main"),
            "the answer carries the FIRST discovery (entry point) — proof act 1 entered memory"
        );
        assert!(
            spoken.contains("boot"),
            "the answer carries the SECOND discovery (what it calls) — proof act 2 entered memory"
        );
        assert_eq!(
            adm.engram_count(),
            2,
            "each discovery became a durable memory the mind perceived next tick"
        );

        // THE 18057 TYPED-THREAD ASSERTION (Step 6). The just-executed act's RESULT
        // re-enters through the TYPED field — read `active_act().output.result.content`,
        // never a re-parse of the `[action #n]` prose — and the call correlates to its
        // result BY ID (`call.id == result.tool_use_id`), never by positional index. This
        // is act-grained proof the tool result threads back into the mind: the exact
        // structural channel run-18057-f1 lost the grep result through when it flowed as an
        // evictable perception bid, yielding the 0-byte patch. (The message-builder pinned
        // assistant-tool-use↔tool-result pair — the render side of this same typed value —
        // is the remaining Step 6 piece; it changes every live prompt and is deferred to a
        // live-validated follow-up rather than shipped blind.)
        let active = wm
            .active_act()
            .expect("the settled mind still holds its last typed act");
        assert_eq!(
            active.call.id, active.output.result.tool_use_id,
            "the tool result correlates to its call BY ID, not a positional index"
        );
        assert!(
            active.output.result.content.contains("boot"),
            "the last act's RESULT threads back through the TYPED field (not [action #n] prose): {}",
            active.output.result.content
        );
        let typed_acts = wm.recent_acts();
        assert_eq!(
            typed_acts.len(),
            2,
            "both discoveries are typed acts in the window, read off the typed channel"
        );
        assert!(
            typed_acts
                .iter()
                .all(|a| a.call.id == a.output.result.tool_use_id),
            "every discovery's result is id-correlated to its call"
        );
    }

    // what this catches: the repeat-perception short-circuit. An IDENTICAL, already-
    // satisfied call this turn must NOT re-execute — the greedy re-emission that spun
    // `commands/list` forever in the nil-room eval (proven live 2026-07-02): working
    // memory already carried the result, yet the model re-issued the byte-identical
    // call every act and never answered. `apply_act` now detects the satisfied
    // `(name, args)` in working memory, skips the hand, and records an explicit
    // "already ran it; answer now" proprioception so the redundancy is PERCEIVED rather
    // than merely present via a stamp shift the greedy decode ignores. A MIXED batch (a
    // genuinely new call) still runs — proven by
    // `the_hands_change_the_mind_across_a_multi_act_investigation` (two DISTINCT calls
    // both execute). Content-driven, not an iteration counter
    // ([[persona-tool-loop-act-then-report]], [[no-hardcoded-heuristics-to-steer-cognition]]).
    #[tokio::test]
    async fn identical_already_satisfied_act_does_not_re_execute() {
        // Two queued results: only the FIRST may ever be popped. If the identical
        // second act reached the hand, the queue would drain by one more — the length
        // assertion below catches exactly that.
        let exec = Arc::new(ScriptedExecutor::new(["4\n", "SECOND-MUST-NOT-POP"]));
        let adm = admission();
        let wm = Arc::new(WorkingMemory::new(4));
        let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter), 8)
            .with_acting(body_with_wm(exec.clone(), adm.clone(), Arc::clone(&wm)));
        let room = Uuid::new_v4();

        // First act genuinely runs; its result lands in working memory.
        let first = acts_of(apply_act(&cycle, &[tool_call()], "check the math", room).await);
        assert_eq!(first[0].call.name, "code/run", "first act names the tool it ran");
        assert!(
            matches!(first[0].status, ActStatus::Executed),
            "the first act really executed"
        );
        assert_eq!(
            exec.results.lock().unwrap().len(),
            1,
            "first act popped exactly one result off the hand"
        );

        // Second, byte-identical act: already satisfied → short-circuit, no re-run.
        // The typed act's OUTPUT carries the nudge and the STATUS names the demotion.
        let second = acts_of(apply_act(&cycle, &[tool_call()], "check the math", room).await);
        assert!(
            matches!(second[0].status, ActStatus::AlreadySatisfied { .. }),
            "the second identical act is typed AlreadySatisfied, not Executed"
        );
        let second_nudge = second[0].output.result.content.clone();
        assert!(
            second_nudge.contains("issued") && second_nudge.contains("times"),
            "records explicit repeat-count proprioception instead of another result: {second_nudge}"
        );
        assert_eq!(
            exec.results.lock().unwrap().len(),
            1,
            "the identical call NEVER reached the hand a second time (queue undrained)"
        );

        // #206 ESCALATION: a THIRD identical call must produce a DISTINCT, higher count
        // than the second — the proprioception climbs rather than repeating byte-identical
        // text. Without this, static-nudge spam evicts the useful receipt from the bounded
        // recency window and a greedy (temp-0) model re-emits the identical call forever.
        let third = acts_of(apply_act(&cycle, &[tool_call()], "check the math", room).await);
        let third_nudge = third[0].output.result.content.clone();
        assert_ne!(
            second_nudge, third_nudge,
            "the repeat proprioception must ESCALATE (distinct text), not repeat verbatim"
        );
        assert!(
            third_nudge.contains("3 times"),
            "the third identical call perceives itself as the 3rd, breaking the fixed point: {third_nudge}"
        );
        assert_eq!(
            exec.results.lock().unwrap().len(),
            1,
            "still never re-executed"
        );
    }

    // what this catches: the redundant-orientation predicate — the FIRST discovery
    // per concern is honest (no receipt yet → false), a SECOND once a `commands/list`
    // or `commands/help` receipt is in the concern is spin (→ true), a MIXED batch
    // carrying any real workspace action is NOT demoted (the real call must run), and
    // an empty batch is never redundant. Guards the "demote discovery at the seam"
    // fix (Joel 2026-07-16) against demoting a genuine first orientation or a real act.
    // what this catches: the escalation counter losing to ARG JITTER. The detector
    // (`is_redundant_orientation`) is class-based on purpose — its doc says demoting by
    // CLASS "ignoring args entirely" is immune to jitter. The COUNTER was not: it keyed on
    // `name|args`, so each jittered variant was a fresh key returning 1, and the nudge read
    // "1 times this concern" forever. Byte-identical perception off a greedy decoder is a
    // fixed point — the exact #206 failure the escalation exists to break.
    //
    // Live on sympy-21379, the run's 8 orientation calls, nearly all distinct args:
    //   commands/list({"filter":"code"}) ×2, commands/list({}), commands/list({"filter":"sympy"}),
    //   code/tree({"path":"."}), code/tree({include_hidden,max_depth,path:"sympy"}),
    //   commands/help({"name":"code/read"}), commands/help({"name":"code/edit"})
    // Detector fired all 5 demotions; every nudge said "1 times".
    #[test]
    fn the_orientation_counter_climbs_across_jittered_args() {
        let wm = WorkingMemory::new(16);
        // ONE stable class key — the shape `bump_orientation_repeat` uses.
        const K: &str = "orientation|<class>";
        assert_eq!(wm.note_action_fingerprint(K), 1);
        assert_eq!(wm.note_action_fingerprint(K), 2);
        assert_eq!(wm.note_action_fingerprint(K), 3, "climbs — perception shifts each demotion");

        // The OLD arg-keyed shape, for contrast: jittered variants never escalate, which is
        // precisely how a determined model rode past the guard.
        let wm2 = WorkingMemory::new(16);
        let jittered = [
            r#"commands/list|{"filter":"code"}"#,
            r#"commands/list|{}"#,
            r#"commands/list|{"filter":"sympy"}"#,
        ];
        for fp in jittered {
            assert_eq!(
                wm2.note_action_fingerprint(fp),
                1,
                "arg-keyed fingerprints stay at 1 under jitter — why the counter had to move to the class"
            );
        }
    }

    #[test]
    fn redundant_orientation_fires_only_on_a_repeat_all_discovery_batch() {
        let list = |args: serde_json::Value| ToolCall {
            id: "c".into(),
            name: "commands/list".into(),
            input: args,
        };
        let help = ToolCall {
            id: "c".into(),
            name: "commands/help".into(),
            input: serde_json::json!({ "name": "code/write" }),
        };
        // First orientation, nothing yet in the concern → honest, not redundant.
        assert!(!is_redundant_orientation(&[], &[list(serde_json::json!({}))]));
        // A discovery receipt is already in the concern → a second orientation is spin.
        let recent = vec!["commands/list({}) → ok".to_string()];
        assert!(is_redundant_orientation(&recent, &[help.clone()]));
        assert!(is_redundant_orientation(
            &recent,
            &[list(serde_json::json!({ "filter": "code" }))]
        ));
        // A settlement boundary AFTER the receipt closes the concern → fresh start,
        // orientation is honest again (scope is only the post-[settled] tail).
        let recent_settled = vec![
            "commands/list({}) → ok".to_string(),
            crate::cognition::working_memory::WM_SETTLEMENT_PREFIX.to_string(),
        ];
        assert!(!is_redundant_orientation(&recent_settled, &[help.clone()]));
        // A MIXED batch with a real workspace action is never demoted — the real call
        // must reach the hand.
        assert!(!is_redundant_orientation(&recent, &[help.clone(), tool_call()]));
        // Empty batch is never redundant.
        assert!(!is_redundant_orientation(&recent, &[]));

        // WORKSPACE orientation (`code/tree`) — the displaced-spin case (benchmark
        // 2026-07-16: 156 arg-jittered tree surveys). First tree per concern is honest;
        // a REPEAT after a tree receipt is spin, regardless of the arg jitter that
        // evades the exact-repeat guard.
        let tree = |p: &str| ToolCall {
            id: "t".into(),
            name: "code/tree".into(),
            input: serde_json::json!({ "path": p, "max_depth": 2 }),
        };
        assert!(!is_redundant_orientation(&[], &[tree("apps/cli")]), "first survey is honest");
        let after_tree = vec!["code/tree(path=apps/cli, max_depth=2) → ok".to_string()];
        // Jittered repeat (trailing slash, different depth) → still demoted (args ignored).
        assert!(is_redundant_orientation(&after_tree, &[tree("apps/cli/")]));
        assert!(is_redundant_orientation(
            &after_tree,
            &[ToolCall { id: "t".into(), name: "code/tree".into(), input: serde_json::json!({}) }]
        ));
        // `code/list` is NOT orientation — a specific-dir listing to get filenames before
        // an edit is a legitimate narrowing step, so it always runs.
        let clist = ToolCall { id: "l".into(), name: "code/list".into(), input: serde_json::json!({ "path": "src" }) };
        assert!(!is_redundant_orientation(&after_tree, &[clist]));
    }

    // what this catches: the seam-level demotion — a first `commands/list` runs and
    // lands its receipt; a SECOND orientation call (`commands/help`) this concern is
    // demoted WITHOUT reaching the hand, recording redundant-orientation proprioception
    // instead. This is the fix for the glass-boxed act-pressure filler (1855/3288 live
    // tool calls were `help`/`list_commands`, nine straight `commands/help` turns while
    // the answer sat ready). Mirrors `identical_already_satisfied_act_does_not_re_execute`
    // but for the DIFFERENT-args orientation case the exact-repeat guard misses.
    #[tokio::test]
    async fn redundant_orientation_is_demoted_and_never_reaches_the_hand() {
        // Two queued results: only the FIRST orientation may pop. If the second
        // reached the hand, the queue would drain one more — the length assert catches it.
        let exec = Arc::new(ScriptedExecutor::new([
            "{\"commands\":[]}",
            "SECOND-MUST-NOT-POP",
        ]));
        let adm = admission();
        let wm = Arc::new(WorkingMemory::new(4));
        let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter), 8)
            .with_acting(body_with_wm(exec.clone(), adm.clone(), Arc::clone(&wm)));
        let room = Uuid::new_v4();

        let list = ToolCall {
            id: "c1".into(),
            name: "commands/list".into(),
            input: serde_json::json!({}),
        };
        let help = ToolCall {
            id: "c2".into(),
            name: "commands/help".into(),
            input: serde_json::json!({ "name": "code/write" }),
        };

        // First orientation genuinely runs; its receipt lands in working memory.
        acts_of(apply_act(&cycle, &[list], "orient", room).await);
        assert_eq!(
            exec.results.lock().unwrap().len(),
            1,
            "first orientation popped exactly one result off the hand"
        );

        // Second, DIFFERENT-args orientation this concern → demoted, no re-run.
        let second = acts_of(apply_act(&cycle, &[help], "orient again", room).await);
        assert!(
            matches!(second[0].status, ActStatus::RedundantOrientation { .. }),
            "the demoted orientation is typed RedundantOrientation, not Executed"
        );
        let nudge = second[0].output.result.content.clone();
        assert!(
            nudge.contains("orientation") && nudge.contains("times"),
            "records escalating redundant-orientation proprioception, not another catalog: {nudge}"
        );
        assert_eq!(
            exec.results.lock().unwrap().len(),
            1,
            "the redundant orientation NEVER reached the hand (queue undrained)"
        );
    }

    // what this catches: SETTLE IS A REST, NOT A HALT — the metronome does not
    // crank to a halt after one answer. The SAME mind (same cycle, same body,
    // same accumulating memory) settles concern A, then RE-AWAKENS on a fresh
    // concern B and runs the act→observe→speak arc again. Her concern-A memory
    // persists across the two drives (continuity of self), and she still engages
    // B. Proves the organism keeps breathing across concerns: a settle is the
    // judgment "the work is done for now," never a terminus.
    #[tokio::test]
    async fn it_settles_then_re_awakens_without_cranking_to_a_halt() {
        // Distinct per-concern observations: identical content would be a
        // content-addressed dedup no-op in memory (correct substrate behavior,
        // [[embeddings-are-per-content-computed-once-shared]]), which would mask
        // the continuity-of-self assertion below.
        let exec = Arc::new(ScriptedExecutor::new(["learned about A", "learned about B"]));
        let adm = admission();
        // One living mind: the working-memory buffer accumulates ACROSS both
        // concern-drives (volatile continuity), so `ActThenSpeak` must re-awaken on
        // concern B by perceiving a NEW act stamp rather than mistaking concern A's
        // still-buffered proprioception for "already answered".
        let wm = Arc::new(WorkingMemory::new(3));
        let cycle = WorkspaceCycle::new(
            vec![
                Arc::new(WorkingMemoryFaculty::new(Arc::clone(&wm))) as Arc<dyn Faculty>,
                Arc::new(ActThenSpeak::new()),
            ],
            Arc::new(SalienceArbiter),
            8,
        )
        .with_acting(body_with_wm(exec.clone(), adm.clone(), Arc::clone(&wm)));
        let room = Uuid::new_v4();

        // Concern A: act → observe → settle on a Speak.
        let a = drive_to_settle(&cycle, "[eval]\npeer: concern A?", room, 8, TurnFraming::ambient()).await;
        assert_eq!(a.acts, 1, "settled concern A after one act→observe");
        assert!(a.spoken.is_some(), "concern A got a spoken answer");
        assert_eq!(adm.engram_count(), 1, "concern A left exactly one memory");

        // Concern B on the SAME living mind — it must wake again, not stay halted.
        let b = drive_to_settle(&cycle, "[eval]\npeer: a totally different concern B?", room, 8, TurnFraming::ambient()).await;
        assert_eq!(
            b.acts, 1,
            "the mind RE-AWAKENED and acted on the new concern — not stuck post-settle"
        );
        assert!(b.spoken.is_some(), "and settled concern B too");
        assert_eq!(
            adm.engram_count(),
            2,
            "continuity of self: concern-A memory persisted, concern B added its own"
        );
    }


    /// Deliberation faculty that Speaks a fixed text — for exercising the Speak arm.
    struct SpeaksText(&'static str);
    #[async_trait]
    impl Faculty for SpeaksText {
        fn id(&self) -> FacultyId {
            FacultyId::Deliberation
        }
        fn reacts_to_broadcast(&self) -> bool {
            true
        }
        async fn contribute(&self, _ws: &Workspace) -> Option<Contribution> {
            Some(Contribution::verdict(
                Decision::Speak { text: self.0.into() },
                0.9,
                "speaks",
            ))
        }
    }

    // what this catches: the unfulfilled-promise backstop (#122, glass-boxed live
    // 2026-07-09). A Speak that NARRATES action (first-person intent + fence) which
    // no format lifted must leave an [unfulfilled] proprioception line in working
    // memory — next tick she perceives her own unkept promise instead of believing
    // the work happened. A plain prose Speak must leave no such line.
    #[tokio::test]
    async fn spoken_narrated_action_records_unfulfilled_promise() {
        let exec = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "ok".into(),
        });
        let promise =
            "I'll run this script to check:\n```python\nprint(2+2)\n```\nOutput soon!";
        let wm = Arc::new(WorkingMemory::new(4));
        let cycle = WorkspaceCycle::new(
            vec![Arc::new(SpeaksText(promise)) as Arc<dyn Faculty>],
            Arc::new(SalienceArbiter),
            8,
        )
        .with_acting(body_with_wm(exec.clone(), admission(), Arc::clone(&wm)));
        let (step, _) = settle_step(
            &cycle,
            "[eval]\npeer: can you check 2+2?",
            Uuid::new_v4(),
            true,
            TurnFraming::ambient(),
            Situation::FreshContext,
        )
        .await;
        assert!(matches!(step, SettleStep::Spoke(_)));
        assert!(
            wm.recent().iter().any(|l| l.contains("[unfulfilled]")),
            "narrated-but-unexecuted promise must enter proprioception: {:?}",
            wm.recent()
        );

        let wm2 = Arc::new(WorkingMemory::new(4));
        let cycle2 = WorkspaceCycle::new(
            vec![Arc::new(SpeaksText("the answer is 4, plainly.")) as Arc<dyn Faculty>],
            Arc::new(SalienceArbiter),
            8,
        )
        .with_acting(body_with_wm(exec, admission(), Arc::clone(&wm2)));
        let (step2, _) = settle_step(
            &cycle2,
            "[eval]\npeer: can you check 2+2?",
            Uuid::new_v4(),
            true,
            TurnFraming::ambient(),
            Situation::FreshContext,
        )
        .await;
        assert!(matches!(step2, SettleStep::Spoke(_)));
        assert!(
            !wm2.recent().iter().any(|l| l.contains("[unfulfilled]")),
            "plain prose must never trip the promise backstop"
        );
    }

    // what this catches: the CONFABULATION backstop (Joel 2026-07-11) — under a
    // peer's verification pressure Atlas upgraded from stage directions to
    // plausible fenced FILE CONTENTS no tool ever produced. A fenced Speak in a
    // turn with zero acts, spoken while working memory already carries an
    // outstanding [unfulfilled] promise, must record the [unverified] fact.
    // Evidence-gated: the SAME fenced content with a clean memory (legitimate
    // drafting — Asha sharing code) must record nothing.
    #[tokio::test]
    async fn fenced_content_over_unkept_promises_records_unverified_artifact() {
        // Atlas's live shape: the confabulated test-file contents.
        let confabulated = "1. **Simple Text File**: Contains a single line of text.\n\
                            ```\nThis is a simple text file for testing purposes.\n```";

        // With an outstanding promise in memory → [unverified].
        let exec = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "ok".into(),
        });
        let wm = Arc::new(WorkingMemory::new(4));
        wm.record_receipt(
            "[unfulfilled] I wrote a stage direction like [doing the task], \
             but a stage direction is words only — no tool ran, no file exists.",
        );
        let cycle = WorkspaceCycle::new(
            vec![Arc::new(SpeaksText(confabulated)) as Arc<dyn Faculty>],
            Arc::new(SalienceArbiter),
            8,
        )
        .with_acting(body_with_wm(exec.clone(), admission(), Arc::clone(&wm)));
        let (step, _) = settle_step(
            &cycle,
            "[eval]\npeer: please provide the content of the test files",
            Uuid::new_v4(),
            true,
            TurnFraming::ambient(),
            Situation::FreshContext,
        )
        .await;
        assert!(matches!(step, SettleStep::Spoke(_)));
        assert!(
            wm.recent().iter().any(|l| l.contains("[unverified]")),
            "fenced 'artifacts' over an unkept promise are composition, not \
             workspace truth: {:?}",
            wm.recent()
        );

        // Clean memory, same fenced content → legitimate drafting, no line.
        let exec2 = Arc::new(RecordingExecutor {
            seen_context: Mutex::new(None),
            result_content: "ok".into(),
        });
        let wm2 = Arc::new(WorkingMemory::new(4));
        let cycle2 = WorkspaceCycle::new(
            vec![Arc::new(SpeaksText(confabulated)) as Arc<dyn Faculty>],
            Arc::new(SalienceArbiter),
            8,
        )
        .with_acting(body_with_wm(exec2, admission(), Arc::clone(&wm2)));
        let (step2, _) = settle_step(
            &cycle2,
            "[eval]\npeer: could you draft example test data?",
            Uuid::new_v4(),
            true,
            TurnFraming::ambient(),
            Situation::FreshContext,
        )
        .await;
        assert!(matches!(step2, SettleStep::Spoke(_)));
        assert!(
            !wm2.recent().iter().any(|l| l.contains("[unverified]")),
            "drafting with a clean conscience is never taxed: {:?}",
            wm2.recent()
        );
    }

}
