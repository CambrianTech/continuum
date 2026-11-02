//! Shared fixture construction for engine contracts and their core caller.
#![cfg(any(test, feature = "test-fixtures"))]
#[cfg(any(test, windows))]
use super::*;

/// Observe the real generation's exit without exporting engine internals in production.
#[cfg(any(test, feature = "test-fixtures"))]
#[cfg(windows)]
pub fn generation_has_exited(generation: &crate::inference::slots::EngineGeneration) -> bool {
    generation.has_exited()
}

#[cfg(any(test, feature = "test-fixtures"))]
#[cfg(any(test, windows))]
pub(super) fn target(id: &str) -> ServingTarget {
    use crate::model_registry::types::{Arch, MultiPartyChatStrategy};
    ServingTarget {
        host_prompt_cache_mib: crate::inference::lane_args::CACHE_RAM_MIB,
        model: Model {
            weights_bytes: None,
            mmproj_bytes: None,
            id: id.to_string(),
            name: None,
            provider: "llamacpp-local".to_string(),
            arch: Arch::Qwen2,
            context_window: 32768,
            max_output_tokens: 4096,
            tokens_per_second: 0.0,
            capabilities: std::collections::BTreeSet::new(),
            cost_input_per_1k: 0.0,
            cost_output_per_1k: 0.0,
            gguf_hint: None,
            hf_source: None,
            gguf_local_path: None,
            chat_template: None,
            stop_sequences: Vec::new(),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            mmproj_local_path: None,
            parameter_count: 0,
            sampling: crate::model_registry::types::ModelSampling::default(),
            persona_serving_eligible: true,
            serving: Default::default(), // test/fixture literal: substrate defaults (text-only main lane, unverified kv-shift)
        },
        context_window: 32768,
        lanes: 1,
        adapters: Vec::new(),
        placement: LanePlacement::Gpu,
        expert_placement: None,
        resident_override: None,
        vision_sidecar: false,
    }
}

/// The real Windows launch/retirement scenario; the upper layer supplies only its
/// daemon transaction, so the engine never depends on core.
#[cfg(any(test, feature = "test-fixtures"))]
#[cfg(windows)]
pub async fn exercise_prepared_restore<F, Fut>(mut command: tokio::process::Command, exercise: F)
where
    F: FnOnce(
        Arc<dyn LlamaServerControl>,
        crate::inference::slots::EngineGeneration,
        String,
        tokio::task::JoinHandle<()>,
        Arc<std::sync::atomic::AtomicUsize>,
    ) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    use sha2::Digest;
    use std::sync::atomic::{AtomicUsize, Ordering};
    // Drive real preparation and the shared restore->OS-spawn path with
    // a non-model Windows child. Readiness must fail honestly; this does
    // not claim that a fixture command is a ready inference engine.
    let fixture = tempfile::tempdir().unwrap();
    let home = fixture.path().canonicalize().unwrap();
    let _home = crate::paths::test_home::HomeGuard::set(&home).await;
    let engine = home.join("engine");
    std::fs::create_dir(&engine).unwrap();
    let program = engine.join("llama-server.exe");
    let system =
        PathBuf::from(std::env::var_os("SystemRoot").expect("Windows fixture system root"))
            .join("System32");
    std::fs::copy(system.join("where.exe"), &program).unwrap();
    let bytes = std::fs::read(&program).unwrap();
    std::fs::write(
        engine.join("engine-install.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema": 1, "source_revision": "0123456789012345678901234567890123456789",
            "backend": "cpu", "build_contract": "static-local-backends-v1",
            "runtime_origin": "installed-toolkit-bin-snapshot",
            "platform_contract": "windows-system32-nvidia-driver-v1",
            "files": {"llama-server.exe": format!("{:x}", sha2::Sha256::digest(bytes))}
        }))
        .unwrap(),
    )
    .unwrap();
    let weights = engine.join("weights.gguf");
    candle_core::quantized::gguf_file::write(
        &mut std::fs::File::create(&weights).unwrap(),
        &[],
        &[],
    )
    .unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut process = LlamaServerProcess::with_root(format!("http://127.0.0.1:{port}"));
    process.bin_pin = Some(program.to_string_lossy().into_owned());
    let mut requested = target("prepared-launch-fixture");
    requested.model.gguf_local_path = Some(weights);
    requested.placement = LanePlacement::Cpu;
    let prepared = Arc::new(process.prepare_local_launch(&requested).await.unwrap());
    assert!(
        prepared.local_inputs.is_ok(),
        "fixture must obtain actual original input evidence"
    );
    let original_args = prepared
        .command()
        .unwrap()
        .as_std()
        .get_args()
        .map(|v| v.to_os_string())
        .collect::<Vec<_>>();
    std::fs::write(
        engine.join("chat_template.jinja"),
        b"a later template must not be selected",
    )
    .unwrap();
    requested.context_window = 4096;
    let mut later = prepared.as_ref().clone();
    let mut changed_trace = prepared
        .managed_environment
        .iter()
        .find(|(key, _)| key == "LLAMA_TRACE")
        .and_then(|(_, value)| value.clone())
        .unwrap_or_default();
    changed_trace.push("-different-fixture-choice");
    later
        .managed_environment
        .push(("LLAMA_TRACE".into(), Some(changed_trace)));
    let replay_command = prepared.command().unwrap();
    assert_eq!(
        replay_command
            .as_std()
            .get_args()
            .map(|v| v.to_os_string())
            .collect::<Vec<_>>(),
        original_args
    );
    assert!(!prepared
        .invocation
        .args
        .iter()
        .any(|arg| arg == "--chat-template-file"));
    assert_ne!(
        later
            .command()
            .unwrap()
            .as_std()
            .get_envs()
            .find(|(k, _)| *k == "LLAMA_TRACE")
            .map(|(_, v)| v.map(|v| v.to_os_string())),
        replay_command
            .as_std()
            .get_envs()
            .find(|(k, _)| *k == "LLAMA_TRACE")
            .map(|(_, v)| v.map(|v| v.to_os_string()))
    );
    drop(later);
    let endpoint = crate::inference::slots::directory().endpoint(&process.root);
    let transition = endpoint.transition().await;
    let generation = transition.start_generation().unwrap();
    let mut child = command.spawn().unwrap();
    let child_input = child.stdin.take().unwrap();
    process.install_child(
        child,
        generation.clone(),
        prepared.engine_install.clone(),
        prepared.local_inputs.as_ref().ok().cloned(),
    );
    process.record_verified_target(
        &generation,
        &prepared.target,
        &prepared.invocation,
        Some(prepared.clone()),
        process.checked_engine().unwrap().program(),
        11008,
        4,
    );
    drop(transition);
    let captured = process.owned_serving_target().unwrap();
    let admitted = std::sync::atomic::AtomicBool::new(false);
    let live_refusal = process
        .restore_owned_launch(&captured, &|| {
            admitted.store(true, std::sync::atomic::Ordering::SeqCst);
            true
        })
        .await;
    assert!(
        matches!(live_refusal, Err(LlamaServerError::Spawn(ref why)) if why.contains("exit is not confirmed"))
    );
    assert!(!admitted.load(std::sync::atomic::Ordering::SeqCst));
    let retired = process
        .retire_owned_engine(&captured.identity)
        .await
        .unwrap();
    drop(child_input);
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(10));
        while process.observe_engine_retirement(&retired).unwrap() != EngineRetirementStatus::Exited
        {
            tick.tick().await;
        }
    })
    .await
    .unwrap();
    drop(listener);
    let session = process.begin_owned_restore(&captured).await.unwrap();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        process.restore_owned_session(&session, None, &|| {
            admitted.store(true, std::sync::atomic::Ordering::SeqCst);
            true
        }),
    )
    .await
    .unwrap();
    assert!(admitted.load(std::sync::atomic::Ordering::SeqCst));
    assert!(
        matches!(result, Err(LlamaServerError::Spawn(ref why)) if why.contains("exited during bring-up")),
        "actual non-model child must spawn and then fail readiness"
    );
    assert!(
        process.child.lock().unwrap().as_ref().is_some(),
        "shared spawn installed the child in the real owner"
    );
    assert!(
        process.owned_serving_target().is_none(),
        "failed readiness cannot mint a new restoration receipt"
    );
    // A subsequent generation prevents replay of the earlier receipt even
    // with a caller that says intent is current. Its callback is not run.
    admitted.store(false, std::sync::atomic::Ordering::SeqCst);
    let stale = process
        .restore_owned_launch(&captured, &|| {
            admitted.store(true, std::sync::atomic::Ordering::SeqCst);
            true
        })
        .await;
    assert!(matches!(stale, Err(LlamaServerError::Superseded)));
    assert!(!admitted.load(std::sync::atomic::Ordering::SeqCst));
    // The batch's original session retains its own failed-startup successor,
    // unlike a fresh replay of the old capture. Retry requires actual exit.
    let first_attempt = session.attempt_identity();
    let retired = process.retire_owned_engine(&first_attempt).await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        while process.observe_engine_retirement(&retired).unwrap() != EngineRetirementStatus::Exited
        {
            tick.tick().await;
        }
    })
    .await
    .unwrap();
    let retry = tokio::time::timeout(
        Duration::from_secs(10),
        process.restore_owned_session(&session, None, &|| true),
    )
    .await
    .unwrap();
    assert!(
        matches!(retry, Err(LlamaServerError::Spawn(ref why)) if why.contains("exited during bring-up"))
    );
    assert!(
        session.attempt_identity() != first_attempt,
        "shared spawn alone advances retry authority"
    );
    let retired = process
        .retire_owned_engine(&session.attempt_identity())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        while process.observe_engine_retirement(&retired).unwrap() != EngineRetirementStatus::Exited
        {
            tick.tick().await;
        }
    })
    .await
    .unwrap();
    let transition = endpoint.transition().await;
    let foreign_generation = transition.start_generation().unwrap();
    let mut foreign_child = command.spawn().unwrap();
    let foreign_input = foreign_child.stdin.take().unwrap();
    process.install_child(
        foreign_child,
        foreign_generation.clone(),
        prepared.engine_install.clone(),
        prepared.local_inputs.as_ref().ok().cloned(),
    );
    drop(transition);
    admitted.store(false, std::sync::atomic::Ordering::SeqCst);
    let foreign = process
        .restore_owned_session(&session, None, &|| {
            admitted.store(true, std::sync::atomic::Ordering::SeqCst);
            true
        })
        .await;
    assert!(matches!(foreign, Err(LlamaServerError::Superseded)));
    assert!(!admitted.load(std::sync::atomic::Ordering::SeqCst));

    // Drive the connected daemon transaction with this actual child and
    // original local inputs. The helper cancels AFTER checkpoint/exit;
    // where.exe is an actual restore spawn, never claimed model readiness.
    process.record_verified_target(
        &foreign_generation,
        &prepared.target,
        &prepared.invocation,
        Some(prepared.clone()),
        process.checked_engine().unwrap().program(),
        11008,
        4,
    );
    let transition = endpoint.transition().await;
    transition
        .ready(
            &process.root,
            &foreign_generation,
            crate::inference::slots::KvPageContract {
                model_id: prepared.target.model.id.clone(),
                model: prepared.gguf.clone(),
                adapters: vec![],
                page_dir: Some(prepared.slot_save_dir.clone()),
                context: 11008,
                slots: 4,
                cache_type: Some("q8_0".into()),
                flash_attn: Some(true),
                revisions: Some(vec![]),
            },
        )
        .unwrap();
    drop(transition);
    let pool = endpoint.admit().await.unwrap().pool.unwrap();
    let key = crate::inference::slots::PageKey::mind(uuid::Uuid::new_v4())
        .unwrap();
    let sem = Arc::new(tokio::sync::Semaphore::new(4));
    let mut turn = crate::inference::turn_admission::admit_turn(
        &sem,
        Some(key),
        Some(pool),
        &process.client,
        &process.root,
        10,
        std::time::Duration::ZERO,
    )
    .await
    .unwrap();
    turn.generation_completed();
    drop(turn);
    let saves = Arc::new(AtomicUsize::new(0));
    let app = axum::Router::new()
        // Checkpoint admission discovers the same physical pool before saving.
        // This fixture models a legacy engine without the optional size protocol.
        .route("/props", axum::routing::get(|| async {
            axum::Json(serde_json::json!({"total_slots": 4}))
        }))
        .route("/slots/{slot}", axum::routing::post({
        let saves = saves.clone();
        move |axum::extract::Path(slot): axum::extract::Path<u32>,
            axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
            axum::Json(body): axum::Json<serde_json::Value>| {
            let saves = saves.clone();
            async move {
                assert_eq!(query.get("action").map(String::as_str), Some("save"));
                saves.fetch_add(1, Ordering::SeqCst);
                axum::Json(serde_json::json!({"id_slot":slot,"filename":body["filename"],"n_saved":10}))
            }
        }
    }));
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
        .await
        .unwrap();
    let http = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let root = process.root.clone();
    let process = Arc::new(process);
    exercise(process.clone(), foreign_generation, root, http, saves).await;
    drop(foreign_input);
    drop(process);
    collect_retired_engines();
}
