#![cfg(test)]

use super::*;
use crate::cognition::workspace::WorkspaceCycle;
use crate::cognition::workspace::SalienceArbiter;
use crate::cognition::tool_executor::CallVerdict;
use crate::cognition::tool_executor::NativeBatchOutcome;
use crate::cognition::tool_executor::ParsedToolBatch;
use crate::cognition::tool_executor::ToolError;
use crate::cognition::tool_executor::ToolExecutionContext;
use crate::cognition::tool_executor::ToolExecutor;
use crate::cognition::tool_executor::ToolOutcome;
use crate::ai::types::ToolCall;
use uuid::Uuid;
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::Mutex;

// what this catches (stop-drain S2, Codex): a tool batch running unrecorded. When the turn's
// pre-effect mark cannot be confirmed, nothing is dispatched (the executor is never called),
// the act is unfulfilled, and she is told why in working memory instead of losing it silently.
#[tokio::test]
async fn a_batch_whose_effect_mark_is_refused_never_reaches_the_executor() {
    struct Refused;
    #[async_trait]
    impl crate::cognition::act_observe::EffectMark for Refused {
        fn turn(&self) -> Uuid {
            Uuid::nil()
        }
        async fn before(&self, _effect: &crate::cognition::act_observe::Effect) -> Result<(), String> {
            Err("obligation store unreachable".into())
        }
    }
    let exec = Arc::new(RecordingExecutor {
        seen_context: Arc::new(Mutex::new(None)),
        result_content: "4\n".into(),
    });
    let adm = admission();
    let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter))
        .with_acting(body(exec.clone(), adm.clone()));
    let chain = ActChain::new().with_effects(Some(crate::cognition::act_observe::EffectGate(Arc::new(Refused))));
    let outcome = apply_act(&cycle, &[tool_call()], "check the math", Uuid::new_v4(), &chain, None, None).await;
    assert!(!outcome.produced_an_act(), "a refused mark is an unfulfilled act: {outcome:?}");
    assert!(exec.seen_context.lock().unwrap().is_none(), "the executor was never called"); // unwrap: test lock
    let told = cycle.acting().expect("the test cycle has hands").working_memory.recent_entries(); // expect: built with_acting above
    assert!(
        told.iter().any(|entry| entry.text.contains("[not run]") && entry.text.contains("obligation store unreachable")),
        "she perceives why her batch did not run"
    );
}

// what this catches (card 286af878; enrico 2026-10-27 02:54Z): a reported-only sentinel call
// recorded as an effect. Eitan's cut turn carried only `tools/<cut at the output limit>`, which never
// runs, yet S2 marked it, so the restart read the turn as uncertain and never handed it back. A
// sentinel-only batch takes no mark: even a gate that refuses every mark does not stop it.
#[tokio::test]
async fn a_sentinel_only_batch_is_no_effect_and_takes_no_mark() {
    struct Refused;
    #[async_trait]
    impl crate::cognition::act_observe::EffectMark for Refused {
        fn turn(&self) -> Uuid {
            Uuid::nil()
        }
        async fn before(&self, _effect: &crate::cognition::act_observe::Effect) -> Result<(), String> {
            Err("a sentinel must never be marked".into())
        }
    }
    let exec = Arc::new(RecordingExecutor {
        seen_context: Arc::new(Mutex::new(None)),
        result_content: "reported".into(),
    });
    let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter))
        .with_acting(body(exec.clone(), admission()));
    let chain = ActChain::new().with_effects(Some(crate::cognition::act_observe::EffectGate(Arc::new(Refused))));
    let cut = ToolCall {
        id: "tool-attempt-cut-at-limit".into(),
        name: crate::cognition::tool_executor::command_executor::CUT_AT_LIMIT_SENTINEL.into(),
        input: serde_json::json!({}),
    };
    apply_act(&cycle, &[cut], "composing a long reply", Uuid::new_v4(), &chain, None, None).await;
    assert!(exec.seen_context.lock().unwrap().is_some(), "the sentinel batch was not refused by the mark"); // unwrap: test lock
    let told = cycle.acting().expect("the test cycle has hands").working_memory.recent_entries(); // expect: built with_acting above
    assert!(!told.iter().any(|entry| entry.text.contains("[not run]")), "no 'not run' notice for a sentinel");
}

// what this catches: an act is scoped to the room it is FOR (one mind is in
// many rooms — the body is room-agnostic, `room_id` flows per-call), the
// observation correlates each call to its result in first person, and the
// outcome becomes a recallable engram. Regresses the multi-room steer + the
// result-as-memory choice (ACTING-ORGANISM §3.3).
#[tokio::test]
async fn apply_act_scopes_to_the_room_and_observes_the_result() {
    let exec = Arc::new(RecordingExecutor {
        seen_context: Arc::new(Mutex::new(None)),
        result_content: "4\n".into(),
    });
    let adm = admission();
    let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter))
        .with_acting(body(exec.clone(), adm.clone()));

    let room = Uuid::new_v4();
    let acts = match apply_act(
        &cycle,
        &[tool_call()],
        "check the math",
        room,
        &ActChain::new(),
        None,
        None,
    )
    .await
    {
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
    assert_eq!(
        obs.call.name, "code/run",
        "the typed act names the tool it ran"
    );
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
    assert!(obs
        .render_recall("check the math")
        .contains("check the math"));
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

/// A hand whose result came back RUNNING, through an ALIAS, with a body that has
/// already been FOLDED to an unparseable preview — the three conditions the old
/// act seam failed on simultaneously.
struct FoldedRunningExecutor {
    handle: Uuid,
}

#[async_trait]
impl ToolExecutor for FoldedRunningExecutor {
    async fn execute_native_batch(
        &self,
        calls: &[ToolCall],
        _context: &ToolExecutionContext,
    ) -> Result<NativeBatchOutcome, ToolError> {
        let results = calls
            .iter()
            .map(|c| crate::ai::types::ToolResult {
                tool_use_id: c.id.clone(),
                // What a FOLD actually leaves behind: a head-truncated preview of
                // the JSON. `serde_json::from_str::<ShellExecuteResponse>` on this
                // returns Err — which is precisely why the old re-parse could not
                // recover the handle from a flood-sized result.
                content: format!("{{\"execution_id\":\"{}\",\"status\":\"runni", self.handle),
                is_error: None,
                custody: crate::ai::types::ResultCustody::Pointer { handle: "spill-1".into() },
            })
            .collect();
        Ok(NativeBatchOutcome {
            results,
            media: Vec::new(),
            stored_ids: Vec::new(),
            // The executor decoded the FULL value before folding and carries both
            // facts forward. This is the handoff under test.
            verdicts: calls
                .iter()
                .map(|c| CallVerdict {
                    image: None,
                    additional_images: None,
                    tool_use_id: c.id.clone(),
                    verdict: crate::sdk_codegen::ActVerdict::Declared(
                        crate::sdk_codegen::ToolVerdict::Running,
                    ),
                    dispatch_handle: Some(self.handle),
                })
                .collect(),
        })
    }

    async fn parse_response(
        &self,
        _response_text: &str,
        _model_family: Option<&str>,
    ) -> Result<ParsedToolBatch, ToolError> {
        unreachable!("native only")
    }

    async fn store_outcome(
        &self,
        _outcome: &ToolOutcome,
        _context: &ToolExecutionContext,
    ) -> Result<Uuid, ToolError> {
        Ok(Uuid::nil())
    }
}

// what this catches (Astra on #4352): the CHANGED HANDOFF, under the three
// conditions that broke the old one AT ONCE — the call arrives through the
// declared alias `bash`, the result is still RUNNING, and its body has already
// been folded to an unparseable preview.
//
// The old seam recovered the handle by `serde_json::from_str::<ShellExecuteResponse>`
// on `typed_result.content` behind `call.name.replace('_',"/") == "code/shell"`.
// Each half fails here independently: `bash` never matched the name test, and the
// folded preview does not parse. A batch-only test with a small body under an 8000
// char budget cannot reach either — the body must actually be folded and the name
// must actually be an alias, which is what this fixture forces.
#[tokio::test]
async fn a_folded_running_alias_still_registers_its_dispatch_handle() {
    let handle = Uuid::new_v4();
    let exec = Arc::new(FoldedRunningExecutor { handle });
    let the_body = body(exec.clone(), admission());
    let wm = the_body.working_memory.clone();
    let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter))
        .with_acting(the_body);

    let call = ToolCall {
        id: "fold-1".into(),
        // THE ALIAS, not the canonical name.
        name: "bash".into(),
        input: serde_json::json!({ "command": "cargo test --workspace" }),
    };
    let acts = match apply_act(
        &cycle,
        std::slice::from_ref(&call),
        "building",
        Uuid::new_v4(),
        &ActChain::new(),
        None,
        None,
    )
    .await
    {
        ActOutcome::Acted { acts } => acts,
        other => panic!("expected Acted, got {other:?}"),
    };

    // The verdict reached the act seam and survived as the command's own word.
    assert_eq!(
        acts[0].output.verdict,
        crate::sdk_codegen::ActVerdict::Declared(crate::sdk_codegen::ToolVerdict::Running),
        "a running call must arrive at the seam as running, through an alias"
    );

    // AND THE HANDLE REGISTERED — the half that used to need a successful re-parse.
    // Without it the completion event has no label to fold against and the finished
    // build never lands in her working memory.
    let label = wm.dispatched_label(handle).expect(
        "the command-declared handle must register the dispatch even though the body was folded and the name was an alias", // JUSTIFIED: the assertion IS that this is Some
    );
    assert!(
        label.contains("cargo test"),
        "the dispatch is labelled from the call's own command: {label}"
    );

    // And the folded body proves the old path could NOT have done this.
    assert!(
        serde_json::from_str::<crate::code::shell_types::ShellExecuteResponse>(
            &acts[0].output.result.content
        )
        .is_err(),
        "the fixture's body must genuinely not parse, or this test proves nothing"
    );
}

// what this catches: with no hands (no ActingBody on the cycle), the driver
// ABSTAINS rather than fabricating a result — the no-hands path that used to
// live in the faculty now lives here (the faculty only emits the Act verdict).
#[tokio::test]
async fn apply_act_without_hands_abstains() {
    let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter));
    assert!(
        matches!(
            apply_act(
                &cycle,
                &[tool_call()],
                "try",
                Uuid::new_v4(),
                &ActChain::new(),
        None,
        None,
    )
            .await,
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
    let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter))
        .with_acting(body(Arc::new(FailingExecutor), adm.clone()));
    assert!(
        matches!(
            apply_act(
                &cycle,
                &[tool_call()],
                "run",
                Uuid::new_v4(),
                &ActChain::new(),
        None,
        None,
    )
            .await,
            ActOutcome::ExecutorError { .. }
        ),
        "a batch-level failure surfaces as ExecutorError, distinct from NoHands"
    );
    assert_eq!(adm.engram_count(), 0, "a failed act admits no memory");
}

// what this catches (card fe8554d2): her calls carry the generation that issued them, bound by
// the act seam, so work/submit can tie a submission to the turn that made it. Every call of one
// act (two submits in one batch) carries the same generation; an act with no generation is not
// stamped; hands that cannot stamp still act, unstamped, as before.
#[tokio::test]
async fn an_acts_calls_carry_the_generation_that_issued_them() {
    #[derive(Clone)]
    struct StampingExecutor {
        issued: Option<String>,
        seen: Arc<Mutex<Vec<Option<String>>>>,
    }
    #[async_trait]
    impl ToolExecutor for StampingExecutor {
        fn issued_by(&self, generation: String) -> Option<Arc<dyn ToolExecutor>> {
            Some(Arc::new(Self { issued: Some(generation), seen: self.seen.clone() }))
        }
        async fn execute_native_batch(
            &self,
            calls: &[ToolCall],
            _context: &ToolExecutionContext,
        ) -> Result<NativeBatchOutcome, ToolError> {
            if let Ok(mut seen) = self.seen.lock() {
                seen.extend(calls.iter().map(|_| self.issued.clone()));
            }
            Ok(NativeBatchOutcome {
                results: calls
                    .iter()
                    .map(|c| crate::ai::types::ToolResult {
                        tool_use_id: c.id.clone(),
                        content: "ok".into(),
                        is_error: None,
                        custody: crate::ai::types::ResultCustody::Unsaved,
                    })
                    .collect(),
                media: Vec::new(),
                stored_ids: Vec::new(),
                verdicts: Vec::new(),
            })
        }
        async fn parse_response(&self, _response_text: &str, _model_family: Option<&str>) -> Result<ParsedToolBatch, ToolError> {
            Ok(ParsedToolBatch { tool_calls: Vec::new(), cleaned_text: String::new(), parse_time_us: 0 })
        }
        async fn store_outcome(&self, _outcome: &ToolOutcome, _context: &ToolExecutionContext) -> Result<Uuid, ToolError> {
            Ok(Uuid::nil())
        }
    }
    let seen_of = |exec: &StampingExecutor| match exec.seen.lock() {
        Ok(seen) => seen.clone(),
        Err(poisoned) => panic!("stamp record poisoned: {poisoned}"),
    };
    let two_submits = [
        ToolCall { id: "submit-1".into(), name: "work/submit".into(), input: serde_json::json!({"card_id": "a"}) },
        ToolCall { id: "submit-2".into(), name: "work/submit".into(), input: serde_json::json!({"card_id": "b"}) },
    ];

    let exec = StampingExecutor { issued: None, seen: Arc::new(Mutex::new(Vec::new())) };
    let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter))
        .with_acting(body(Arc::new(exec.clone()), admission()));
    let stamped = apply_act(&cycle, &two_submits, "submit both", Uuid::new_v4(), &ActChain::new(), None, Some("gen-1".into())).await;
    assert!(matches!(stamped, ActOutcome::Acted { .. }), "{stamped:?}");
    assert_eq!(seen_of(&exec), vec![Some("gen-1".to_string()); 2], "one act, one issuing generation, every call");

    let exec = StampingExecutor { issued: None, seen: Arc::new(Mutex::new(Vec::new())) };
    let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter))
        .with_acting(body(Arc::new(exec.clone()), admission()));
    let unstamped = apply_act(&cycle, &[tool_call()], "no generation", Uuid::new_v4(), &ActChain::new(), None, None).await;
    assert!(matches!(unstamped, ActOutcome::Acted { .. }), "{unstamped:?}");
    assert_eq!(seen_of(&exec), vec![None], "no generation, no stamp");

    let plain = Arc::new(RecordingExecutor { seen_context: Arc::new(Mutex::new(None)), result_content: "ok".into() });
    let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter)).with_acting(body(plain, admission()));
    let acted = apply_act(&cycle, &[tool_call()], "cannot stamp", Uuid::new_v4(), &ActChain::new(), None, Some("gen-2".into())).await;
    assert!(matches!(acted, ActOutcome::Acted { .. }), "hands that cannot stamp still act: {acted:?}");
}
