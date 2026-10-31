#![cfg(test)]
//! From `cognition::persona_workspace` (its gated-grounding tests): her hands under the REAL
//! `CodeModule` and the production `GridTrustAuthPolicy`, both core surface modules the mind
//! crate cannot name. Each asserts exactly what it asserted before the move.

use std::sync::Arc;

use continuum_base::time::EpochMs;
use uuid::Uuid;

use crate::cognition::persona_workspace::*;
use crate::persona::card_hands::bind_card;
use crate::cognition::workspace::{ActingBody, WorkspaceCycle};
use crate::cognition::admission_state::AdmissionState;
use crate::modules::card_staging;
use crate::modules::code::{CodeModule, CodeState};
use crate::modules::grid::GridTrustAuthPolicy;
use crate::cognition::recall_metadata::RecallMetadataRegistry;
use crate::routing::CallerIdentity;
use crate::cognition::tool_executor::CommandToolExecutor;
use crate::runtime::{CommandExecutor, ModuleRegistry};
use dashmap::DashMap;

/// One core with the REAL `CodeModule` — the single process-global engine map
/// every caller's workspace lives in, which is exactly why the leak is possible.
fn hands_for(persona: Uuid) -> ActingHands {
    let (executor, _) = core_with_code();
    hands_over(Arc::new(CommandToolExecutor::for_persona(executor, persona)), persona)
}

/// A core with the real `CodeModule`, and its state, so a test can see whose hands
/// the engine map holds.
fn core_with_code() -> (Arc<CommandExecutor>, Arc<CodeState>) {
    // Hands admission rides every dispatch as a hook boot installs; so does this core.
    crate::ipc::install_kernel_extensions();
    let registry = Arc::new(ModuleRegistry::new());
    let state = Arc::new(CodeState::new(
        Arc::new(DashMap::new()),
        Arc::new(DashMap::new()),
        tokio::runtime::Handle::current(),
        Arc::new(crate::modules::acting_provider::CoreActing),
    ));
    registry.register(Arc::new(CodeModule::new(state.clone())));
    // The PRODUCTION authorization policy, not an AllowAll fixture: the fixture hid
    // code/release-hands being refused for a local persona (Astra's review of #5005).
    let executor = Arc::new(
        CommandExecutor::new(registry.clone())
            .with_policy(Arc::new(GridTrustAuthPolicy::new()))
            .with_message_bus(Arc::new(crate::runtime::MessageBus::new())),
    );
    // Bound as boot binds it: the code domain reads its callers' trust through this
    // executor's policy, and unbound it would sandbox every resident (Cormac on #5455).
    registry.install_executor_on_all(executor.clone());
    (executor, state)
}

fn hands_over(executor: Arc<dyn crate::cognition::tool_executor::ToolExecutor>, persona: Uuid) -> ActingHands {
    ActingHands {
        persona_id: persona,
        persona_name: "Anwen".to_string(),
        context: executor.hands_context(),
        executor,
        working_memory: Arc::new(crate::cognition::working_memory::WorkingMemory::new(8)),
    }
}

/// What her hands can SEE right now — the honest probe, because "where is the
/// engine rooted" is only interesting insofar as it changes what she can read.
async fn listing(hands: &ActingHands) -> String {
    let ctx = crate::cognition::tool_executor::ToolExecutionContext {
        persona_id: hands.persona_id,
        persona_name: hands.persona_name.clone(),
        session_id: Uuid::new_v4(),
        context_id: Uuid::new_v4(),
        caller_context: serde_json::Value::Null,
        persona_config: crate::cognition::tool_executor::PersonaMediaConfigLite {
            auto_load_media: false,
            supported_media_types: vec![],
        },
    };
    let call = crate::ai::types::ToolCall {
        id: "ls".to_string(),
        name: "code/list".to_string(),
        input: serde_json::json!({ "path": "." }),
    };
    let out = hands
        .executor
        .execute_native_batch(std::slice::from_ref(&call), &ctx)
        .await
        .expect("code/list runs");
    out.results[0].content.clone()
}

// Regression: foreground and background work both record against the card the
// CALL's own hands are bound to (Astra on #5026), never her current focus: a turn
// moving her focus to another claim before the task's first poll cannot move it.
#[tokio::test]
async fn command_work_stays_with_its_card_across_background_dispatch() -> Result<(), String> {
    use crate::persona::cognition_pulse::work_idle_ms;
    let home = tempfile::TempDir::new().unwrap();
    std::fs::write(home.path().join("work.txt"), "work").unwrap();
    let persona = Uuid::new_v4();
    let live = hands_for(persona);
    let (card_a, card_b) = (Uuid::new_v4(), Uuid::new_v4());
    let claim_a = crate::identity::hands_context::HandsContext::for_claim(persona, card_a, Uuid::new_v4());
    let hands = live.in_context(claim_a).expect("claim A's hands");
    drive_create_workspace(&hands, &home.path().to_string_lossy(), &[], "root", false)
        .await
        .unwrap();
    note_acting_root_in(persona, Some(claim_a), Some(home.path().to_path_buf()));
    note_acting_card_in(persona, Some(claim_a), card_a);
    assert!(listing(&hands).await.contains("work.txt"));
    assert!(work_idle_ms(persona, Some(card_a), EpochMs::now().millis()).is_some());

    let exec = hands.executor.command_executor().unwrap();
    let (adapter, _task_directory) = crate::orm::store::fresh_adapter().await;
    exec.set_task_ledger(Arc::new(continuum_kernel::runtime::tasks::TaskLedger::new(adapter).await?))?;
    let mut rx = exec.message_bus().unwrap().receiver();
    let handle = exec.dispatch_background(
        "code/list",
        serde_json::json!({"path": "."}),
        Some(CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(persona)).in_hands(claim_a)),
    ).await.map_err(|e| e.message)?;
    // Acceptance finished before spawning: her focus moves before the task's first poll.
    let claim_b = crate::identity::hands_context::HandsContext::for_claim(persona, card_b, Uuid::new_v4());
    note_acting_root_in(persona, Some(claim_b), Some(home.path().to_path_buf()));
    note_acting_card_in(persona, Some(claim_b), card_b);
    set_focus(persona, Some(claim_b));
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let event = rx.recv().await.unwrap();
            if event.name == "command:completed"
                && event.payload["handle"] == handle.to_string()
            {
                assert_eq!(event.payload["success"], true);
                break;
            }
        }
    })
    .await
    .expect("background completion");
    let now = EpochMs::now().millis();
    assert!(work_idle_ms(persona, Some(card_a), now).is_some(), "the work is claim A's");
    assert_eq!(work_idle_ms(persona, Some(card_b), now), None, "never the focus it moved to");
    set_focus(persona, None);
    note_acting_root_in(persona, Some(claim_a), None);
    note_acting_root_in(persona, Some(claim_b), None);
    Ok(())
}

// what this catches (Astra on #5026): admission living in helpers, so a command that
// started after its hands were released (a background command first polled after the
// release) still ran in them; and a release that ran as ordinary admitted work, waiting
// on itself. The DISPATCHER admits every command bound to a context; a release is the
// kernel's, marked as one, and closes the hands to everything after it.
#[tokio::test]
async fn a_command_after_its_hands_were_released_is_refused_at_dispatch() {
    let dir = tempfile::TempDir::new().expect("a workspace");
    let persona = Uuid::new_v4();
    let (executor, _state) = core_with_code();
    let bound = CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(persona))
        .in_hands(crate::identity::hands_context::HandsContext::fresh());
    executor
        .execute_with_caller("code/create-workspace", serde_json::json!({"workspace_root": dir.path().to_string_lossy()}), Some(bound.clone()))
        .await
        .expect("the hands are bound");
    executor
        .execute_with_caller("code/list", serde_json::json!({"path": "."}), Some(bound.clone()))
        .await
        .expect("open hands admit a command");

    let unmarked = CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(persona))
        .in_hands(crate::identity::hands_context::HandsContext::fresh());
    let refused = executor.execute_with_caller("code/release-hands", serde_json::json!({}), Some(unmarked)).await;
    assert!(refused.is_err(), "a release run as ordinary work is refused: {refused:?}");

    executor
        .execute_with_caller("code/release-hands", serde_json::json!({}), Some(bound.clone().releasing_hands()))
        .await
        .expect("the kernel's release ends the hands");
    let late = executor.execute_with_caller("code/list", serde_json::json!({"path": "."}), Some(bound)).await;
    assert!(late.as_ref().is_err_and(|e| e.contains("released")), "a command after the release is refused at dispatch: {late:?}");
}

// what this catches (Cormac on #5455): the code domain reading trust through an
// executor never bound to it. Unbound, every resident is the sandbox and every test
// stays green; bound as boot binds it, under the production policy, a local persona's
// file verbs reach the host (Joel: "don't restrict them").
#[tokio::test]
async fn a_resident_under_the_production_policy_reads_the_host_not_the_sandbox() {
    let workspace = tempfile::TempDir::new().expect("her workspace");
    let elsewhere = tempfile::TempDir::new().expect("a directory outside it");
    let outside = elsewhere.path().join("OUTSIDE_her_workspace.md");
    std::fs::write(&outside, "read from the host\n").unwrap();
    let persona = Uuid::new_v4();
    let (executor, _state) = core_with_code();
    let resident = CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(persona))
        .in_hands(crate::identity::hands_context::HandsContext::fresh());
    executor
        .execute_with_caller("code/create-workspace", serde_json::json!({"workspace_root": workspace.path().to_string_lossy()}), Some(resident.clone()))
        .await
        .expect("her hands are bound");
    let read = executor
        .execute_with_caller("code/read", serde_json::json!({"file_path": outside.to_string_lossy()}), Some(resident))
        .await;
    assert!(
        read.as_ref().is_ok_and(|r| format!("{r:?}").contains("read from the host")),
        "a resident reads outside her workspace, never the sandbox: {read:?}"
    );
}

// what this catches (card fc9e9fd7; Kimi #11697, #312): one set of hands moved by
// every turn and measurement that needed another root. A card claim's hands are their
// own: rooted at the card, they never move her default hands, every turn of the claim
// finds them where they were rooted, and a re-acquired claim is NEW hands that don't
// inherit the old one's place. Her default hands can't be rooted or ended at all.
#[tokio::test]
async fn a_claims_hands_are_its_own_and_her_default_hands_never_move() {
    let card_dir = tempfile::TempDir::new().expect("card checkout");
    let home = tempfile::TempDir::new().expect("home dir");
    std::fs::write(card_dir.path().join("CARD_ONLY.rs"), "// card\n").unwrap();
    std::fs::write(home.path().join("HER_OWN_notes.md"), "# mine\n").unwrap();
    let persona = Uuid::new_v4();
    let live = hands_for(persona);
    drive_create_workspace(&live, &home.path().to_string_lossy(), &[], "home", false).await.expect("her home");

    let (card, claim) = (Uuid::new_v4(), Uuid::new_v4());
    let context = crate::identity::hands_context::HandsContext::for_claim(persona, card, claim);
    let claim_hands = live.in_context(context).expect("her executor acts in a claim's context");
    drive_create_workspace(&claim_hands, &card_dir.path().to_string_lossy(), &[], "bind", false).await.expect("rooted once");
    assert!(listing(&claim_hands).await.contains("CARD_ONLY.rs"), "the claim's hands stand in the card");
    let at_home = listing(&live).await;
    assert!(at_home.contains("HER_OWN_notes.md") && !at_home.contains("CARD_ONLY.rs"), "her default hands never moved: {at_home}");

    let next_turn = live.in_context(crate::identity::hands_context::HandsContext::for_claim(persona, card, claim)).expect("same claim");
    assert!(listing(&next_turn).await.contains("CARD_ONLY.rs"), "the next turn of the claim finds its hands where they were rooted");
    let reacquired = crate::identity::hands_context::HandsContext::for_claim(persona, card, Uuid::new_v4());
    assert_ne!(reacquired, context, "a re-acquired claim is new hands, never the old one's");

    // A no-setup gym still needs bound hands. Previously its claim staged
    // Ordinary and the resolver searched only SWE checkouts, so valid
    // code/write and code/shell calls were refused as unrooted.
    let _home = crate::paths::test_home::HomeGuard::set(home.path()).await;
    std::env::set_var("CONTINUUM_HOME", home.path());
    let gym_card = airc_lib::WorkCard {
        card_id: airc_work::WorkCardId::new(),
        repo: airc_work::RepoId::new("CambrianTech/continuum").unwrap(),
        title: "[bench coder-write-eval] sum_evens: write and verify".into(),
        body: None, priority: airc_work::Priority::P2, lane_id: None,
        state: airc_work::CardState::Claimed,
        owner: Some(airc_core::PeerId::from_uuid(persona)),
        claim_provenance: None,
        claim_id: Some(airc_work::ClaimId::from_uuid(Uuid::new_v4())),
        claim_expires_at_ms: None, last_heartbeat_at_ms: None,
        pull_request: None, created_by: airc_core::PeerId::new(),
        created_at_ms: 0, updated_at_ms: 0, reviews: None,
        submissions: Vec::new(), last_submission_rejection: None,
    };
    let workspace = crate::persona::card_checkout::gym_workspace(home.path(), persona);
    assert!(crate::persona::card_checkout::checkout_path_for(&persona, &gym_card).is_none());
    assert_eq!(
        card_staging::stage_for_card(home.path(), persona, &gym_card).await,
        card_staging::Staging::Ready { path: workspace.clone() },
    );
    let cycle = WorkspaceCycle::new(vec![], Arc::new(crate::cognition::workspace::SalienceArbiter))
        .with_acting(Arc::new(ActingBody {
            persona_id: persona, persona_name: live.persona_name.clone(),
            executor: live.executor.clone(), working_memory: live.working_memory.clone(),
            admission: Arc::new(AdmissionState::new(Arc::new(RecallMetadataRegistry::new()))),
        }));
    let bound = bind_card(&cycle, persona, &gym_card).await;
    let gym = &bound.hands.as_ref().expect("a claimed no-setup gym has real hands").hands;
    let (written, failed) = call(gym, "code/write", serde_json::json!({"file_path":"binding-proof.txt", "content":"gym-owned\n"})).await;
    assert!(!failed, "actual write is admitted: {written}");
    assert_eq!(std::fs::read_to_string(workspace.join("binding-proof.txt")).unwrap(), "gym-owned\n");
    let (ran, failed) = call(gym, "code/shell", serde_json::json!({"cmd":"cat binding-proof.txt", "wait_ms":20000})).await;
    assert!(!failed && ran.contains("gym-owned"), "actual shell sees the same artifact: {ran}");
    assert!(!home.path().join("binding-proof.txt").exists());
    assert!(!card_dir.path().join("binding-proof.txt").exists());
    assert!(listing(&claim_hands).await.contains("CARD_ONLY.rs"), "the prior claim's hands never moved");
    assert!(listing(&live).await.contains("HER_OWN_notes.md"), "resident hands never moved");
}


/// One tool call through these hands: its content, and whether it was an error.
async fn call(hands: &ActingHands, name: &str, input: serde_json::Value) -> (String, bool) {
    // A shell admits its cargo root through the disk ownership registry; install the
    // test one here, never rely on an earlier test in the binary having done it.
    crate::system_resources::disk_reporters::prepare_writer_fixture().await;
    let ctx = crate::cognition::tool_executor::ToolExecutionContext {
        persona_id: hands.persona_id,
        persona_name: hands.persona_name.clone(),
        session_id: Uuid::new_v4(),
        context_id: Uuid::new_v4(),
        caller_context: serde_json::Value::Null,
        persona_config: crate::cognition::tool_executor::PersonaMediaConfigLite {
            auto_load_media: false,
            supported_media_types: vec![],
        },
    };
    let call = crate::ai::types::ToolCall { id: name.to_string(), name: name.to_string(), input };
    let out = hands.executor.execute_native_batch(std::slice::from_ref(&call), &ctx).await.expect("the call runs");
    (out.results[0].content.clone(), out.results[0].is_error.is_some())
}

/// Root a set of hands the way `root_acting_workspace` does: the workspace, then its
/// place in the acting-root record.
async fn root(hands: &ActingHands, dir: &std::path::Path) {
    drive_create_workspace(hands, &dir.to_string_lossy(), &[], "root", false).await.expect("roots");
    note_acting_root_in(hands.persona_id, hands.context, Some(dir.to_path_buf()));
}

// what this catches: an eval, a solve or a replay moving or borrowing the LIVING
// persona's hands (#312, and Astra's review of #5005). A fork used to clone her live
// executor, so its rooting moved her live tools, its restore moved them home, and a
// default eval left her live hands at a deleted clone. Under the PRODUCTION policy, a
// fork's own hands stand in an exam while hers stand at home, at the same time:
// listing, Python execution and her live acting place all stay hers; a context never
// rooted gets no workspace (never her desk); a running operation is killed and
// confirmed by the awaited close; and an unclosed fork is still released on drop.
#[tokio::test]
async fn a_forks_own_hands_never_move_her_live_hands_and_end_with_the_fork() {
    let exam = tempfile::TempDir::new().expect("exam dir");
    let home = tempfile::TempDir::new().expect("home dir");
    std::fs::write(exam.path().join("EXAM_ONLY_blueprints.py"), "# flask\n").unwrap();
    std::fs::write(home.path().join("HER_OWN_notes.md"), "# mine\n").unwrap();
    let persona = Uuid::new_v4();
    let (executor, state) = core_with_code();
    let live_exec = CommandToolExecutor::for_persona(executor, persona);
    let live = hands_over(Arc::new(live_exec.clone()), persona);
    root(&live, home.path()).await;
    let live_card = Uuid::new_v4();
    note_acting_card_in(persona, None, live_card);

    let fork_exec = crate::cognition::tool_executor::ToolExecutor::in_fresh_hands(&live_exec, persona)
        .expect("a live persona's hands can open a fork's own");
    let fork = hands_over(fork_exec, persona);
    assert!(fork.context.is_some() && live.context.is_none(), "the fork's hands are a context of their own");

    // Fail closed: an acting context that was never rooted has no workspace, never hers.
    let (unrooted, refused) = call(&fork, "code/list", serde_json::json!({"path": "."})).await;
    assert!(refused, "an unrooted context must be refused, not handed her desk: {unrooted}");

    root(&fork, exam.path()).await;
    let (seen_live, _) = call(&live, "code/list", serde_json::json!({"path": "."})).await;
    let (seen_fork, _) = call(&fork, "code/list", serde_json::json!({"path": "."})).await;
    assert!(
        seen_live.contains("HER_OWN_notes.md") && !seen_live.contains("EXAM_ONLY_blueprints.py"),
        "rooting the fork moved HER live hands into the exam (#312): {seen_live}"
    );
    assert!(seen_fork.contains("EXAM_ONLY_blueprints.py"), "the fork's hands must stand in the exam: {seen_fork}");

    // Python runs where THESE hands stand: the fork's in the exam, hers at home.
    let listdir = serde_json::json!({"lang": "python", "code": "import os; print(sorted(os.listdir('.')))"});
    let (py_fork, _) = call(&fork, "code/run", listdir.clone()).await;
    let (py_live, _) = call(&live, "code/run", listdir).await;
    assert!(py_fork.contains("EXAM_ONLY_blueprints.py") && !py_fork.contains("HER_OWN_notes.md"), "the fork's Python must run in the exam: {py_fork}");
    assert!(py_live.contains("HER_OWN_notes.md") && !py_live.contains("EXAM_ONLY_blueprints.py"), "her Python must run at home: {py_live}");

    // Provenance: the fork's rooting is its own; her live place and card are unchanged.
    assert_eq!(acting_root_of(persona), Some(home.path().to_path_buf()), "her live place is still home");
    assert_eq!(acting_card_of(persona), Some(live_card), "her receipts still belong to her live card");
    assert_eq!(acting_card_in(persona, fork.context), None, "the fork never inherits her card");

    // A running operation in the fork is ended by the awaited close, under the
    // production policy, and confirmed.
    let (started, _) = call(&fork, "code/shell", serde_json::json!({"cmd": "sleep 600", "wait_ms": 100})).await;
    assert!(started.contains("running") || started.contains("Running"), "the fork has a running operation: {started}");
    close_acting_context(&fork).await.expect("the fork's context closes, with its kill confirmed");
    assert_eq!(state.file_engines.len(), 1, "only her live engine remains");
    assert_eq!(acting_root_in(persona, fork.context), None, "the fork's place is forgotten");
    let (after, _) = call(&live, "code/list", serde_json::json!({"path": "."})).await;
    assert!(after.contains("HER_OWN_notes.md"), "ending the fork leaves her hands as they were: {after}");

    // An unclosed fork (an abnormal exit) is still released when its hands drop.
    let dropped = hands_over(
        crate::cognition::tool_executor::ToolExecutor::in_fresh_hands(&live_exec, persona).expect("another fork"),
        persona,
    );
    root(&dropped, exam.path()).await;
    assert_eq!(state.file_engines.len(), 2);
    let dropped_context = dropped.context;
    drop(dropped);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while state.file_engines.len() != 1 || acting_root_in(persona, dropped_context).is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("an unclosed fork is released, and its place forgotten, when its hands drop");
    note_acting_root_in(persona, None, None);
}

// what this catches (Astra's re-review of #5005): an eval cancelled while closing its
// forks' hands deleting the world they still run in, while their release was merely
// scheduled. After the cancel the world, the fork's engine and its place must all
// remain; once the owned cleanup confirms the kill, all three go.
#[tokio::test]
async fn a_cancelled_eval_keeps_its_world_until_its_forks_are_confirmed_ended() {
    use futures::FutureExt;
    let parent = tempfile::TempDir::new().expect("parent dir");
    let world_dir = parent.path().join("run");
    std::fs::create_dir_all(&world_dir).unwrap();
    std::fs::write(world_dir.join("EXAM_ONLY_blueprints.py"), "# flask\n").unwrap();
    let persona = Uuid::new_v4();
    let (executor, state) = core_with_code();
    let live_exec = CommandToolExecutor::for_persona(executor, persona);
    let fork = hands_over(
        crate::cognition::tool_executor::ToolExecutor::in_fresh_hands(&live_exec, persona).expect("a fork's own hands"),
        persona,
    );
    let context = fork.context;
    let mut world = crate::cognition::eval::EvalWorld::new(Some(crate::cognition::eval::EphemeralEvalRoot::new(world_dir.clone())));
    world.hold(Some(fork.clone()));
    root(&fork, &world_dir).await;
    let (started, _) = call(&fork, "code/shell", serde_json::json!({"cmd": "sleep 600", "wait_ms": 100})).await;
    assert!(started.contains("running") || started.contains("Running"), "the fork has a running operation: {started}");
    drop(fork);

    // Poll the close once (it starts the kill, then waits on the reap this
    // single-threaded runtime can't have run yet), and cancel it there.
    assert!(world.finish().now_or_never().is_none(), "the close can't confirm before the reap runs");
    assert!(world_dir.is_dir(), "a cancelled close must not delete the world its fork still runs in");
    assert_eq!(state.file_engines.len(), 1, "the fork's engine is kept until its kill is confirmed");
    assert!(acting_root_in(persona, context).is_some(), "the fork's place is kept until its kill is confirmed");

    // The owned cleanup outlives the cancel: it confirms the kill, then cleans up.
    // 10 s is this test's bound on a SIGKILLed `sleep` being reaped, not a product timeout.
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while world_dir.exists() || !state.file_engines.is_empty() || acting_root_in(persona, context).is_some() {
            tokio::time::sleep(continuum_process::KILL_POLL).await;
        }
    })
    .await
    .expect("once the kill is confirmed, the world, the engine and the place all go");
}

// what this catches: a pure-cognition persona has no hands, so there is nothing to
// root and nothing to restore. `of` must say so rather than have the caller invent
// a handle — the fail-loud in `root_acting_workspace` depends on this being None.
#[test]
fn a_handless_cycle_yields_no_hands() {
    let cycle = crate::cognition::workspace::WorkspaceCycle::new(
        vec![],
        Arc::new(crate::cognition::workspace::SalienceArbiter),
    );
    assert!(ActingHands::of(&cycle).is_none());
}
