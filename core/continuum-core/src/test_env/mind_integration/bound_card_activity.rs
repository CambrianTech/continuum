#![cfg(test)]
//! From `cognition::act_observe` (mind split step 7): the bound card's activity, read through
//! persona's board and bound by persona's card hands, is the one cognition perceives and acts
//! in. It needs both crates, so it lives here. Asserts exactly what it asserted before the move.

use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::cognition::act_observe::test_support::{admission, ActThenSpeak, RecordingExecutor};
use crate::cognition::act_observe::{settle_step, ActChain};
use crate::cognition::working_memory::WorkingMemory;
use crate::cognition::workspace::{
    ActingBody, SalienceArbiter, Situation, TurnFraming, WorkspaceCycle,
};

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
    let hands = crate::identity::hands_context::HandsContext::for_claim(
        persona,
        card.as_uuid(),
        claim.as_uuid(),
    );
    let checkout = airc_lib::work_worktree::worktree_path_for(card).unwrap();
    assert!(checkout.starts_with(home.path()));
    // Hands requires a usable checkout, not an interrupted clone's bare .git directory.
    assert!(std::fs::create_dir_all(&checkout).is_ok());
    assert!(std::fs::write(checkout.join("subject.txt"), "subject\n").is_ok());
    for args in [
        vec!["init", "-b", "fixture"],
        vec!["add", "subject.txt"],
        vec![
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "fixture",
        ],
    ] {
        assert!(
            std::process::Command::new("git")
                .args(&args)
                .current_dir(&checkout)
                .output()
                .is_ok_and(|output| output.status.success()),
            "git {args:?}"
        );
    }
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
            Arc::new(ActThenSpeak::new()),
        ],
        Arc::new(SalienceArbiter),
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
    let text: String = prompt
        .messages
        .iter()
        .map(|m| m.content_text())
        .collect::<Vec<_>>()
        .join("\n");
    let actions = text
        .find("[actions]")
        .map(|at| &text[at..])
        .expect("test: the action stream is in her view");
    assert!(actions.contains("code/run"), "{actions}");
    assert!(!actions.contains("nothing has executed yet"), "{actions}");
    assert_eq!(airc.current_room().await.unwrap().channel, wake.channel);
    let private = crate::cognition::mind_room::mind_room_id(persona);
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
