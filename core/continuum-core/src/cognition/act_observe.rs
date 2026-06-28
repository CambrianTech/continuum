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

use uuid::Uuid;

use super::workspace::{Decision, TurnMetrics, WorkspaceCycle};
use crate::ai::types::ToolCall;
use crate::persona::types::{InboxMessage, SenderType};

/// Max chars of a single tool result folded into the next perception / engram.
/// A bound on what we re-inject, NOT a clamp on the model's own generation: the
/// model owns its output length (a hung child is bounded by `code/run`'s wall
/// clock instead). Generous — a traceback she needs to read to self-correct must
/// survive intact.
const RESULT_FOLD_MAX_CHARS: usize = 16_000;

/// Max chars of an act-observation recorded into the VOLATILE working-memory
/// scratchpad (proprioception). Much smaller than `RESULT_FOLD_MAX_CHARS` — working
/// memory is a rolling "what my hands just did" pointer (the full result lives in
/// the engram for relevance recall), and the answer to a lookup is almost always in
/// the head of the result. Keeps the perception bid compact while still carrying
/// enough for the mind to answer from its own recent action.
const WM_ACTION_HEAD_CHARS: usize = 800;

/// The result of driving a mind to settlement.
pub struct SettleOutcome {
    /// The verdict the mind settled on: `Speak`/`RaiseUnprompted`/`Pass` when it
    /// settled, or the final un-driven `Act` if the external budget ran out
    /// mid-action (the grader grades that as "did not finish" — honest, never a
    /// fabricated answer).
    pub decision: Decision,
    /// The spoken text, present only when the settled decision is `Speak` /
    /// `RaiseUnprompted`. This is what an external observer (the grader, or a peer
    /// in the room) reads.
    pub spoken: Option<String>,
    /// How many actions were executed before settling.
    pub acts: usize,
    /// The final world-state, with each action's observation folded in — what the
    /// last tick perceived. Captured for replay/forensics.
    pub world_state: String,
    /// The accumulated cost of settling this task: every act→observe deliberation
    /// generation's latency + tokens, summed. `tokens_per_second()` re-derives
    /// throughput from the totals. This is the speed/latency the eval reports next
    /// to the accuracy grade — the same number a live turn could surface for the
    /// serving governor.
    pub metrics: TurnMetrics,
}

/// Execute ONE `Act` verdict: run its calls through the persona's hands, admit
/// the outcome as an Episodic engram (the result becomes memory), and return the
/// observation text so the caller can fold it into the next perception.
///
/// `room_id` is the room THIS act is about — passed per-call because one mind is
/// in many rooms at once (a persona, like a Claude tab, is in multiple rooms
/// simultaneously); the [`ActingBody`](super::workspace::ActingBody) itself is
/// room-agnostic.
///
/// Returns `None` (abstain — never a fabricated success) when the mind has no
/// hands or the executor errors. The admission is best-effort: an un-admitted
/// observation still flows back via the returned text, so re-perception works
/// regardless; admission is what makes it durable long-term memory.
pub async fn apply_act(
    cycle: &WorkspaceCycle,
    calls: &[ToolCall],
    intent: &str,
    room_id: Uuid,
) -> Option<String> {
    let body = cycle.acting()?; // no hands → cannot act (and tools were never offered)

    let ctx = crate::cognition::tool_executor::ToolExecutionContext {
        persona_id: body.persona_id,
        persona_name: body.persona_name.clone(),
        // Session is the EPHEMERAL connection instance and is NEVER load-bearing
        // for where an action lands (per IDENTITY-SCOPE-PEER-LIVENESS-MODEL §A.5).
        // The room is the context the action scopes to.
        session_id: Uuid::nil(),
        context_id: room_id,
        caller_context: serde_json::Value::Null,
        persona_config: crate::cognition::tool_executor::PersonaMediaConfigLite {
            auto_load_media: false,
            supported_media_types: Vec::new(),
        },
    };

    let outcome = match body
        .executor
        .execute_native_batch(calls, &ctx, RESULT_FOLD_MAX_CHARS)
        .await
    {
        Ok(o) => o,
        Err(e) => {
            // Fail loud-ish: the hand could not run. Abstain — do NOT synthesize a
            // result the mind would then "remember" as fact ([[fallbacks-are-illegal-fail-loud]]).
            tracing::warn!(
                persona = %body.persona_name,
                error = %e,
                "act→observe: tool batch failed; abstaining (no fabricated outcome)"
            );
            return None;
        }
    };

    // Form the observation: what she did and what came back. First person, because
    // this is the persona observing her OWN hands — the engram reads like a memory
    // of acting, not a log line.
    let mut observation = String::new();
    for (i, call) in calls.iter().enumerate() {
        let result = outcome.results.get(i);
        let body_text = match result {
            Some(r) => r.content.as_str(),
            None => "(no result returned)",
        };
        let args = serde_json::to_string(&call.input).unwrap_or_else(|_| "{}".to_string());
        observation.push_str(&format!(
            "I ran {}({}) because {}.\nResult:\n{}\n\n",
            call.name,
            args,
            intent.trim(),
            body_text.trim(),
        ));
    }
    let observation = observation.trim().to_string();

    // Admit the outcome as an Episodic engram through the ONE production admit
    // path (a self-observation message from the persona to itself). This is the
    // result-as-memory choice: next tick, recall can surface "I ran X → got Y" the
    // same way it surfaces anything else the persona knows. Best-effort — an
    // admission hiccup must never wedge the act→observe loop.
    let now_ms = now_ms();
    let self_observation = InboxMessage {
        id: Uuid::new_v4(),
        room_id,
        sender_id: body.persona_id,
        sender_name: body.persona_name.clone(),
        sender_type: SenderType::Persona,
        content: observation.clone(),
        timestamp: now_ms,
        priority: 1.0,
        source_modality: None,
        voice_session_id: None,
    };
    if let Err(e) = body.admission.admit(&self_observation, None) {
        tracing::debug!(
            persona = %body.persona_name,
            error = %e,
            "act→observe: self-observation not admitted (folds into perception anyway)"
        );
    }

    // Proprioception: record the act + result head into VOLATILE working memory too.
    // The engram admit above is content-deduped (correct for long-term memory: don't
    // store the same fact twice), which means a REPEATED identical act is a no-op
    // there — and with thinking suppressed (gateway default) the reasoning channel is
    // dark too. Both channels that carry "what just happened" go silent on a repeat,
    // so perception is byte-identical and greedy decode re-emits the identical Act
    // forever. Working memory is the recency/proprioception channel that fixes it: it
    // is append-only and `#n`-stamped, so a repeat still SHIFTS the perception window
    // next tick and the mind can see its own hands (and that it's repeating itself).
    // This is the shared live↔eval channel, not the eval-only `[you just acted]` fold.
    // See [[act-results-need-a-recency-channel-not-semantic-recall]].
    let head: String = observation.chars().take(WM_ACTION_HEAD_CHARS).collect();
    body.working_memory.record_action(&head);

    crate::probe!(
        class = "persona.act.observed",
        persona = %body.persona_name,
        room_id = %room_id,
        tools = calls.len(),
        chars = observation.len(),
        "acted and observed the result"
    );

    Some(observation)
}

/// Drive the mind to SETTLEMENT: tick → if `Act`, run it + fold the observation
/// into the next perception → re-tick → until it `Speak`s/`Pass`es or the
/// external `max_acts` budget is spent.
///
/// `max_acts` is the EXTERNAL observer's stopwatch — the grader holds it because
/// the eval room has no heartbeat to pace re-perception. It is NOT a cap that
/// lives in the persona's head (the live path has no such bound; an "acts
/// forever" persona is a fitness gap to train away, never a substrate ceiling —
/// §4). When the budget runs out mid-action, the final un-driven `Act` is
/// returned and the grader scores it as unfinished — never a fabricated answer.
pub async fn drive_to_settle(
    cycle: &WorkspaceCycle,
    world_state: impl Into<String>,
    room_id: Uuid,
    max_acts: usize,
) -> SettleOutcome {
    let world = world_state.into();
    let mut acts = 0usize;
    // Fold each tick's deliberation cost in, so the settled outcome reports the
    // task's TOTAL speed/latency (a multi-act task pays for every generation).
    let mut metrics = TurnMetrics::default();

    loop {
        // ONE settlement step through the SHARED primitive the live heartbeat uses
        // (`settle_step`). The only thing this driver adds is the LOOP — because the
        // eval room has no metronome, the grader re-perceives by calling step again.
        // `may_act = acts < max_acts` gates ACTING (not speaking): past the budget
        // she may still settle into a Speak, but a fresh Act is returned un-driven.
        let (step, step_metrics) = settle_step(cycle, world.clone(), room_id, acts < max_acts).await;
        if let Some(m) = step_metrics {
            metrics.accumulate(m);
        }
        match step {
            SettleStep::Spoke(text) => {
                return SettleOutcome {
                    spoken: Some(text.clone()),
                    decision: Decision::Speak { text },
                    acts,
                    world_state: world,
                    metrics,
                };
            }
            SettleStep::Acted { .. } => {
                acts += 1;
                // The observation re-enters perception through MEMORY + the volatile
                // working-memory recency channel — `apply_act` admitted it and
                // recorded a stamped proprioception trace, and the next `settle_step`
                // re-perceives. This is BYTE-FOR-BYTE the live heartbeat motion
                // (service_loop apply via the SAME `settle_step`, then the metronome
                // re-perceives next tick). `world` is held CONSTANT across iterations:
                // memory is the only thing that changes, exactly as in life — no
                // eval-only `[you just acted]` fold. See
                // [[act-results-need-a-recency-channel-not-semantic-recall]].
            }
            // Budget spent on a fresh Act, OR the act could not be carried out (no
            // hands / exec error). Either way she did not settle in the observer's
            // window — return the un-driven Act so the grader scores it as unfinished,
            // never a fabricated answer.
            SettleStep::WouldAct { calls, intent } | SettleStep::ActUnfulfilled { calls, intent } => {
                return SettleOutcome {
                    decision: Decision::Act { calls, intent },
                    spoken: None,
                    acts,
                    world_state: world,
                    metrics,
                };
            }
            SettleStep::Passed => {
                return SettleOutcome {
                    decision: Decision::Pass,
                    spoken: None,
                    acts,
                    world_state: world,
                    metrics,
                };
            }
        }
    }
}

/// The outcome of ONE settlement [`settle_step`].
#[derive(Debug)]
pub enum SettleStep {
    /// She settled on speech (`Speak`/`RaiseUnprompted`) — the prose turn an
    /// observer (a peer, or the grader) reads.
    Spoke(String),
    /// She reached for her hands AND the act was carried out; the result is admitted
    /// as memory + a stamped proprioception trace. The caller re-perceives next
    /// (live: next metronome tick; eval: next loop step). The calls+intent ride
    /// along so a caller that paces acting (the eval budget) can report the final
    /// Act if its budget runs out on the following step.
    Acted { calls: Vec<ToolCall>, intent: String },
    /// She decided to act but the caller's budget said no this step (`may_act =
    /// false`) — the act was NOT executed. Only the eval driver passes `may_act =
    /// false`; the live heartbeat always permits its one act, so it never sees this.
    WouldAct { calls: Vec<ToolCall>, intent: String },
    /// She chose silence (`Pass`) — honored as a turn that produces no utterance.
    Passed,
    /// She reached for an act that could NOT be carried out (no hands / executor
    /// error). No utterance; the intent rides along for honest logging/grading.
    ActUnfulfilled { calls: Vec<ToolCall>, intent: String },
}

/// ONE step of settlement — the single place a `Decision` becomes speech-or-action,
/// shared by the live heartbeat (`persona::service_loop`, called ONCE per metronome
/// tick) and the eval driver ([`drive_to_settle`], which loops steps because the
/// grader replaces the metronome). Live and eval therefore make a turn the
/// IDENTICAL way — run the cycle over `world`, read the `Decision`, and on `Act`
/// run it once + admit the result as memory. The ONLY difference between the two is
/// pacing (metronome vs synchronous loop), never the per-step motion.
///
/// `may_act` lets the caller pace ACTING without changing the motion: `true` (live,
/// always) runs the act; `false` (eval, past its act budget) returns [`SettleStep::
/// WouldAct`] without executing, so the budget gates acting while still letting a
/// later step settle into a Speak.
pub async fn settle_step(
    cycle: &WorkspaceCycle,
    world: String,
    room_id: Uuid,
    may_act: bool,
) -> (SettleStep, Option<TurnMetrics>) {
    let ws = cycle.run_in_room(world, room_id).await;
    // The cost of THIS tick's deliberation generation — latency + tokens of the
    // model call behind the verdict. Carried out alongside the step so the caller
    // (the eval driver, or the live heartbeat) can accumulate per-turn speed and
    // latency without re-timing the brain. `None` when no verdict carried metrics.
    let metrics = ws.metrics();
    let step = match ws.decision().cloned() {
        Some(Decision::Act { calls, intent }) => {
            if !may_act {
                SettleStep::WouldAct { calls, intent }
            } else {
                match apply_act(cycle, &calls, &intent, room_id).await {
                    Some(_observation) => SettleStep::Acted { calls, intent },
                    None => SettleStep::ActUnfulfilled { calls, intent },
                }
            }
        }
        Some(Decision::Speak { text }) | Some(Decision::RaiseUnprompted { text }) => {
            SettleStep::Spoke(text)
        }
        Some(Decision::Pass) | None => SettleStep::Passed,
    };
    (step, metrics)
}

/// Epoch-ms wall clock for stamping a self-observation. A real timestamp (not a
/// monotonic tick) so the engram orders correctly against chat messages in recall.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn tool_call() -> ToolCall {
        ToolCall {
            id: "call-1".into(),
            name: "code/run".into(),
            input: serde_json::json!({ "lang": "rust", "code": "fn main() { println!(\"{}\", 2 + 2); }" }),
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
        let observation = apply_act(&cycle, &[tool_call()], "check the math", room)
            .await
            .expect("acted");

        assert_eq!(
            *exec.seen_context.lock().unwrap(),
            Some(room),
            "the act must be scoped to the room it is for, not a phantom nil room"
        );
        assert!(observation.contains("code/run"), "names the tool it ran");
        assert!(observation.contains("check the math"), "carries the intent");
        assert!(
            observation.contains('4'),
            "folds in the result the hand returned"
        );
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
            apply_act(&cycle, &[tool_call()], "try", Uuid::new_v4())
                .await
                .is_none(),
            "no hands → None, never a fabricated success"
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
        assert!(apply_act(&cycle, &[tool_call()], "run", Uuid::new_v4())
            .await
            .is_none());
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
            drive_to_settle(&cycle, "[eval]\npeer: what is 2+2?", Uuid::new_v4(), 8).await;

        assert_eq!(outcome.acts, 1, "acted exactly once before settling");
        assert_eq!(outcome.spoken.as_deref(), Some("the answer is 4"));
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

        let outcome = drive_to_settle(&cycle, "go", Uuid::new_v4(), 2).await;

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

        let (deferred, _) = settle_step(&cycle, "go".into(), Uuid::new_v4(), false).await;
        assert!(
            matches!(deferred, SettleStep::WouldAct { .. }),
            "may_act=false defers the act"
        );
        assert!(
            exec.seen_context.lock().unwrap().is_none(),
            "a deferred act NEVER touches the executor"
        );

        let (ran, _) = settle_step(&cycle, "go".into(), Uuid::new_v4(), true).await;
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
        let a = drive_to_settle(&cycle, "[eval]\npeer: concern A?", room, 8).await;
        assert_eq!(a.acts, 1, "settled concern A after one act→observe");
        assert!(a.spoken.is_some(), "concern A got a spoken answer");
        assert_eq!(adm.engram_count(), 1, "concern A left exactly one memory");

        // Concern B on the SAME living mind — it must wake again, not stay halted.
        let b = drive_to_settle(&cycle, "[eval]\npeer: a totally different concern B?", room, 8).await;
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
}
