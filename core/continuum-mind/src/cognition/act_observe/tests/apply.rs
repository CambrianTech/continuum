#![cfg(test)]

use super::*;
use crate::cognition::workspace::Situation;
use crate::cognition::workspace::TurnFraming;
use crate::cognition::workspace::WorkspaceCycle;
use crate::cognition::workspace::ActingBody;
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
use crate::cognition::working_memory::WorkingMemory;
use crate::cognition::working_memory::WorkingMemoryFaculty;

// what this catches: an act is scoped to the room it is FOR (one mind is in
// many rooms — the body is room-agnostic, `room_id` flows per-call), the
// observation correlates each call to its result in first person, and the
// outcome becomes a recallable engram. Regresses the multi-room steer + the
// result-as-memory choice (ACTING-ORGANISM §3.3).
// Regression17d3e6ff: real card lookup must resolve admitted hands before
// board grounding, ledger rendering and execution, without moving the wake.
#[tokio::test]
async fn bound_card_activity_is_shared_by_perception_and_execution() {
    crate::persona::cognition_extensions::install(); // the card's board room is persona's to read
    use crate::cognition::{
        llm_deliberation_faculty::LlmDeliberationFaculty,
        rag_source_faculty::{RagSourceFaculty, SaliencePolicy},
    };
    use crate::persona::{
        airc_runtime::PersonaAircRuntime, airc_runtime_registry::PersonaAircRuntimeRegistry,
        room_board_source::RoomBoardSource,
    };
    let home = tempfile::tempdir().unwrap();
    let _home = crate::paths::test_home::HomeGuard::set(home.path()).await;
    let airc = Arc::new(
        airc_lib::Airc::open_with_wire_root_for_test(home.path(), home.path())
            .await
            .unwrap(),
    );
    let task = airc.join("bound-activity-task").await.unwrap();
    let card = airc
        .create_work_card(airc_lib::CreateWorkCard::new(
            airc_lib::RepoId::new("fixture/activity").unwrap(),
            "implement subject change",
            airc_lib::Priority::P2,
        ))
        .await
        .unwrap();
    let claim = airc
        .claim_work_card(airc_lib::ClaimWorkCard {
            card_id: card,
            ttl_ms: 600_000,
        })
        .await
        .unwrap();
    let wake = airc.join("bound-activity-wake").await.unwrap();
    let persona = airc.peer_id().as_uuid();
    PersonaAircRuntimeRegistry::set_global(PersonaAircRuntimeRegistry::new());
    let registry = PersonaAircRuntimeRegistry::try_global().unwrap();
    let runtime = registry.register(PersonaAircRuntime::from_attached(
        persona,
        "Ivar",
        home.path().to_path_buf(),
        airc.clone(),
        wake.channel,
        crate::persona::identity_provider::PersonaIdentitySource::FreshlyMinted,
    ));
    let hands =
        crate::identity::hands_context::HandsContext::for_claim(persona, card.as_uuid(), claim.as_uuid());
    let checkout = airc_lib::work_worktree::worktree_path_for(card).unwrap();
    assert!(checkout.starts_with(home.path()));
    std::fs::create_dir_all(checkout.join(".git")).unwrap();
    let exec = Arc::new(RecordingExecutor {
        seen_context: Arc::new(Mutex::new(None)),
        result_content: "read subject file".into(),
    });
    let wm = Arc::new(WorkingMemory::new(8));
    let (board_source, _dirty) = crate::cognition::cached_source::CachedRagSource::new(Arc::new(
        RoomBoardSource::new(persona, runtime).for_room(wake.channel.as_uuid()),
    ));
    let board = RagSourceFaculty::new(persona, board_source, SaliencePolicy::StandingFraming);
    let cycle = WorkspaceCycle::new(
        vec![
            Arc::new(board),
            Arc::new(WorkingMemoryFaculty::new(wm.clone())),
            Arc::new(ActThenSpeak::new()),
        ],
        Arc::new(SalienceArbiter),
        8,
    )
    .with_acting(Arc::new(ActingBody {
        persona_id: persona,
        persona_name: "Ivar".into(),
        executor: exec.clone(),
        admission: admission(),
        working_memory: wm.clone(),
    }));
    let board_state = airc.work_board_in(&task).await.unwrap();
    let focus = board_state.card(card).unwrap();
    let binding = crate::persona::card_hands::bind_card(&cycle, persona, focus).await;
    assert_eq!(binding.hands.as_ref().map(|h| h.context), Some(hands));
    let burst = crate::cognition::workspace::Burst::raw_in(
        crate::identity::ActivityRoom::from_uuid(wake.channel.as_uuid()).unwrap(),
        "work on my held card".into(),
    )
    .in_hands(Some(hands));
    let _ = settle_step(
        &cycle,
        burst.clone(),
        true,
        TurnFraming::default(),
        Situation::default(),
        &ActChain::new(),
    )
    .await;
    assert_eq!(
        *exec.seen_context.lock().unwrap(),
        Some(task.channel.as_uuid()),
        "tools execute in the card activity"
    );
    let ws = cycle.run(burst).await;
    assert_eq!(ws.room_id, wake.channel.as_uuid());
    assert_eq!(ws.activity_room_id, task.channel.as_uuid());
    assert!(
        ws.broadcast
            .iter()
            .any(|c| c.content.contains("implement subject change")),
        "the task board reaches perception"
    );
    let prompt = LlmDeliberationFaculty::new(
        persona,
        "Ivar",
        "You are Ivar.",
        Arc::new(crate::ai::heuristic_adapter::HeuristicInferenceAdapter::new()),
    )
    .with_working_memory(wm)
    .prompt_view(&ws);
    // Her acts reach her through the action stream (the steps ledger it replaced is gone).
    let text: String = prompt.messages.iter().map(|m| m.content_text()).collect::<Vec<_>>().join("\n");
    let actions = text.find("[actions]").map(|at| &text[at..]).expect("test: the action stream is in her view");
    assert!(actions.contains("code/run"), "{actions}");
    assert!(!actions.contains("nothing has executed yet"), "{actions}");
    assert_eq!(airc.current_room().await.unwrap().channel, wake.channel);
    let private = crate::persona::mind_room::mind_room_id(persona);
    let private_ws = cycle
        .run(
            crate::cognition::workspace::Burst::raw_in(
                crate::identity::ActivityRoom::from_uuid(private).unwrap(),
                "private thought".into(),
            )
            .in_hands(Some(hands)),
        )
        .await;
    assert_eq!(
        private_ws.activity_room_id, private,
        "private thoughts never adopt public task scope"
    );
    // An eval's fresh hands and a roomless fixture never borrow the focused claim.
    let fork = crate::identity::hands_context::HandsContext::fresh();
    assert_eq!(
        crate::cognition::persona_workspace::activity_room_for(persona, Some(fork), Uuid::nil())
            .await,
        Uuid::nil(),
    );
    let unbound_ws = cycle
        .run(crate::cognition::workspace::Burst::from("synthetic exam").in_hands(Some(fork)))
        .await;
    assert_eq!(unbound_ws.activity_room_id, unbound_ws.room_id);
    assert!(!unbound_ws
        .broadcast
        .iter()
        .any(|c| c.content.contains("implement subject change")));
    assert_eq!(
        crate::cognition::persona_workspace::activity_room_for(
            persona,
            None,
            wake.channel.as_uuid()
        )
        .await,
        wake.channel.as_uuid(),
        "default hands do not inherit the focused claim's card",
    );
    registry.remove(persona);
}

#[tokio::test]
async fn apply_act_scopes_to_the_room_and_observes_the_result() {
    let exec = Arc::new(RecordingExecutor {
        seen_context: Arc::new(Mutex::new(None)),
        result_content: "4\n".into(),
    });
    let adm = admission();
    let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter), 8)
        .with_acting(body(exec.clone(), adm.clone()));

    let room = Uuid::new_v4();
    let acts = match apply_act(
        &cycle,
        &[tool_call()],
        "check the math",
        room,
        &ActChain::new(),
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
        _max_result_chars: usize,
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
    let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter), 8)
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
    let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter), 8);
    assert!(
        matches!(
            apply_act(
                &cycle,
                &[tool_call()],
                "try",
                Uuid::new_v4(),
                &ActChain::new(),
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
    let cycle = WorkspaceCycle::new(Vec::new(), Arc::new(SalienceArbiter), 8)
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
    )
            .await,
            ActOutcome::ExecutorError { .. }
        ),
        "a batch-level failure surfaces as ExecutorError, distinct from NoHands"
    );
    assert_eq!(adm.engram_count(), 0, "a failed act admits no memory");
}
