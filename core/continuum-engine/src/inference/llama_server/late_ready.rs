//! An engine whose launch stopped waiting before it answered, finished as the SAME engine
//! (card ea6c9aff).
//!
//! Windows, d85388c37, 2026-10-27: engine 3548 loaded its weights from an HDD and reported load
//! progress 0.0 for minutes, past `wait_ready`'s stall allowance. The launch returned `NotReady`
//! before `publish_ready`, so the generation's pool and ledger were never published. The engine
//! came up healthy at 22:29, but every later reconcile saw the endpoint unready, tried to replace
//! it, and `capture_suspension` refused ("cannot suspend an unknown or uncertain live ledger")
//! because the generation never had a pool. Serving stayed not ready behind a healthy engine.
//!
//! Now a launch that times out while its engine lives keeps what it had decided. The next launch
//! for this endpoint finishes that engine instead of replacing it, but only when the engine is
//! still the same one (the same generation, alive, the same pid) and the request is still the
//! same launch (the same engine program, weights, model, adapters and invocation). Intent is
//! the transition's own admission (`current`). Nothing is reset: the pool and ledger are
//! published once, by the same `transition.ready` a prompt launch uses.

use super::ready_tail::ReadyTail;
use super::*;

/// A launch whose readiness wait ended while its engine was still alive and loading.
pub(super) struct LoadingLaunch {
    tail: ReadyTail,
}

/// What makes two launches the same launch: everything the engine was started with.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LaunchContract {
    engine_program: String,
    gguf: PathBuf,
    model_id: String,
    adapters: Vec<String>,
    invocation: crate::inference::lane_args::LaneInvocation,
}

impl LaunchContract {
    fn of(prepared: &PreparedLocalLaunch) -> Self {
        Self {
            engine_program: prepared.engine_program.clone(),
            gguf: prepared.gguf.clone(),
            model_id: prepared.target.model.id.clone(),
            adapters: prepared.target.adapter_paths(),
            invocation: prepared.invocation.clone(),
        }
    }
}

/// The owned engine as it stands now: its generation, whether that generation has exited, and
/// its pid.
struct OwnedNow {
    generation: crate::inference::slots::EngineGeneration,
    exited: bool,
    pid: Option<u32>,
}

/// PURE: why a kept launch may NOT be finished, or `None` when it may. It may only when the
/// owned engine is still the kept one (same generation, not exited, same pid) and the request
/// is the same launch.
fn recovery_refusal(
    kept: &LaunchContract,
    kept_generation: &crate::inference::slots::EngineGeneration,
    kept_pid: Option<u32>,
    owned: Option<&OwnedNow>,
    requested: &LaunchContract,
) -> Option<&'static str> {
    let Some(owned) = owned else { return Some("no owned engine remains") };
    if !owned.generation.same_engine(kept_generation) {
        return Some("the owned engine is another generation");
    }
    if owned.exited {
        return Some("the kept engine has exited");
    }
    if kept_pid.is_none() || owned.pid != kept_pid {
        return Some("the owned engine's pid is not the kept engine's");
    }
    if requested != kept {
        return Some("the request is a different launch than the one the engine was started for");
    }
    None
}

impl LlamaServerProcess {
    /// After a readiness wait failed: keep the launch when its engine is alive and was only
    /// slow (`NotReady`), so a later launch can finish it. A restore, an exited engine, or any
    /// other failure keeps nothing.
    pub(super) fn keep_loading_if_alive(&self, error: &LlamaServerError, tail: ReadyTail) {
        if tail.restoring || !matches!(error, LlamaServerError::NotReady(..)) || self.child_exit_status().is_some() {
            return;
        }
        continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.late_ready.kept",
            pid = ?tail.child_pid,
            model = tail.prepared.target.model_id(),
            "the readiness wait ended while the engine still loads: kept, so the next launch finishes it instead of replacing it"
        );
        *self.loading.lock() = Some(LoadingLaunch { tail });
    }

    /// The kept launch the request may finish, or `None` (any kept launch that may not is
    /// dropped, said so).
    pub(super) fn recoverable_loading(&self, requested: &PreparedLocalLaunch) -> Option<ReadyTail> {
        let kept = self.loading.lock().take()?;
        let owned = {
            let child = self.child.lock().unwrap(); // JUSTIFIED: poison means a prior panic mutating the owned child.
            child.as_ref().map(|owned| OwnedNow {
                generation: owned.generation.clone(),
                exited: owned.generation.has_exited(),
                pid: owned.child.id(),
            })
        };
        let refusal = recovery_refusal(
            &LaunchContract::of(&kept.tail.prepared),
            &kept.tail.generation,
            kept.tail.child_pid,
            owned.as_ref(),
            &LaunchContract::of(requested),
        );
        match refusal {
            None => {
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.late_ready.resumed",
                    pid = ?kept.tail.child_pid,
                    model = kept.tail.prepared.target.model_id(),
                    "the same engine, the same launch: finishing it instead of replacing it"
                );
                Some(kept.tail)
            }
            Some(why) => {
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.late_ready.refused",
                    pid = ?kept.tail.child_pid,
                    why = why,
                    "a kept launch may not be finished: it is dropped and never published"
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract(model: &str, ctx: &str) -> LaunchContract {
        LaunchContract {
            engine_program: "llama-server".into(),
            gguf: PathBuf::from(format!("/models/{model}.gguf")),
            model_id: model.into(),
            adapters: vec![],
            invocation: crate::inference::lane_args::LaneInvocation { args: vec!["-c".into(), ctx.into()], ..Default::default() },
        }
    }

    // what this catches (card ea6c9aff, Codex's spec): a late engine published when it is not
    // the kept one (another generation, exited, a different pid) or when the request changed
    // (another model, another window): the stale incarnation or intention must refuse. Only the
    // same live engine for the same launch is finished.
    #[test]
    fn only_the_same_live_engine_for_the_same_launch_is_finished() {
        // Two endpoints, one generation each: a transition starts one generation until its
        // predecessor's exit is observed.
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");
        let generation_on = |root: &'static str| {
            rt.block_on(async {
                let endpoint = crate::inference::slots::directory().endpoint(root);
                let transition = endpoint.transition().await;
                transition.start_generation().expect("a generation")
            })
        };
        let kept_generation = generation_on("http://127.0.0.1:1/late-ready-kept");
        let other_generation = generation_on("http://127.0.0.1:1/late-ready-other");
        let kept = contract("qwen", "16384");
        let now = |generation: &crate::inference::slots::EngineGeneration, exited: bool, pid: Option<u32>| OwnedNow {
            generation: generation.clone(),
            exited,
            pid,
        };
        let same = now(&kept_generation, false, Some(3548));
        assert_eq!(recovery_refusal(&kept, &kept_generation, Some(3548), Some(&same), &kept), None, "the same live engine, the same launch");
        assert!(recovery_refusal(&kept, &kept_generation, Some(3548), None, &kept).is_some(), "no engine");
        assert!(recovery_refusal(&kept, &kept_generation, Some(3548), Some(&now(&other_generation, false, Some(3548))), &kept).is_some(), "another generation");
        assert!(recovery_refusal(&kept, &kept_generation, Some(3548), Some(&now(&kept_generation, true, Some(3548))), &kept).is_some(), "exited");
        assert!(recovery_refusal(&kept, &kept_generation, Some(3548), Some(&now(&kept_generation, false, Some(4000))), &kept).is_some(), "another pid");
        assert!(recovery_refusal(&kept, &kept_generation, None, Some(&same), &kept).is_some(), "an unknown kept pid proves nothing");
        assert!(recovery_refusal(&kept, &kept_generation, Some(3548), Some(&same), &contract("qwen", "32768")).is_some(), "another window");
        assert!(recovery_refusal(&kept, &kept_generation, Some(3548), Some(&same), &contract("llama", "16384")).is_some(), "another model");
    }

    /// An engine that answers readiness: `/health` 200, an empty `/slots`, a decode that counts,
    /// and `/props` with its served window and slots.
    async fn ready_engine() -> String {
        let app = axum::Router::new()
            .route("/health", axum::routing::get(|| async { axum::Json(serde_json::json!({ "status": "ok" })) }))
            .route("/slots", axum::routing::get(|| async { axum::Json(serde_json::json!([])) }))
            .route("/props", axum::routing::get(|| async {
                axum::Json(serde_json::json!({ "default_generation_settings": { "n_ctx": 32768 }, "total_slots": 1 }))
            }))
            .route("/v1/models", axum::routing::get(|| async {
                axum::Json(serde_json::json!({ "data": [{ "id": "late-model" }] }))
            }))
            .route("/v1/chat/completions", axum::routing::post(|| async {
                axum::Json(serde_json::json!({
                    "usage": { "completion_tokens": 24 },
                    "choices": [{ "message": { "content": "1, 2, 3, 4, 5, 6, 7, 8, 9, 10" } }]
                }))
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind the fake engine");
        let addr = listener.local_addr().expect("its address");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        format!("http://{addr}")
    }

    fn prepared(root: &str, dir: &std::path::Path, ctx: &str) -> Arc<PreparedLocalLaunch> {
        let mut target = super::super::test_fixtures::target("late-model");
        target.placement = LanePlacement::Cpu;
        Arc::new(PreparedLocalLaunch {
            target,
            engine_program: "llama-server".into(),
            endpoint: root.to_string(),
            invocation: crate::inference::lane_args::LaneInvocation { args: vec!["-c".into(), ctx.into()], ..Default::default() },
            engine_install: None,
            gguf: dir.join("late.gguf"),
            slot_save_dir: dir.join("pages"),
            page_cache_type: None,
            page_flash_attn: None,
            mtp_draft: None,
            mmproj: false,
            local_inputs: Err("fixture: an ordinary launch".into()),
            managed_environment: Vec::new(),
            expected_geometry: None,
        })
    }

    /// The state a readiness timeout leaves: a live engine installed under a started, never
    /// published generation, and its launch kept by the same path a real `NotReady` takes.
    async fn timed_out_launch(root: &str, kept: Arc<PreparedLocalLaunch>) -> (LlamaServerProcess, crate::inference::slots::EngineGeneration, Option<u32>) {
        let process = LlamaServerProcess::with_root(root.to_string());
        let child = tokio::process::Command::new("sleep").arg("120").kill_on_drop(true).spawn().expect("a live engine stand-in");
        let pid = child.id();
        let endpoint = crate::inference::slots::directory().endpoint(root);
        let transition = endpoint.transition().await;
        let generation = transition.start_generation().expect("the engine's generation");
        process.install_child(child, generation.clone(), None, None);
        drop(transition);
        let tail = ReadyTail {
            prepared: kept,
            generation: generation.clone(),
            child_pid: pid,
            page_dir_ready: false,
            debug_flag: Default::default(),
            restoring: false,
        };
        process.keep_loading_if_alive(&LlamaServerError::NotReady(Duration::from_secs(90), "load progress quiet".into()), tail);
        (process, generation, pid)
    }

    /// End a fixture's stand-in engine and await its exit before the process drops. Dropping a
    /// process with a live child retires it through the shared page-dir guard, and until that
    /// exit is collected every sibling test's page-dir census reads incomplete.
    async fn release(process: LlamaServerProcess) {
        let owned = process.child.lock().unwrap().take(); // JUSTIFIED: test fixture; poison is a prior test panic
        if let Some(mut owned) = owned {
            let _ = owned.child.kill().await;
        }
        drop(process);
    }

    fn owned_pid(process: &LlamaServerProcess) -> Option<u32> {
        process.child.lock().unwrap().as_ref().and_then(|owned| owned.child.id()) // JUSTIFIED: test fixture; poison is a prior test panic
    }

    // what this catches (card ea6c9aff; Windows engine 3548, 2026-10-27): a launch that timed out
    // while its engine loaded, whose late-ready engine was never published, so every later launch
    // asked the replacement checkpoint for a ledger that engine never had and serving stayed down.
    // Through the real launch entry: the same request finishes the SAME engine (no relaunch, the
    // ledger published for its generation); a changed request does not publish it.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_timed_out_launch_finishes_its_late_engine_and_a_changed_request_does_not() {
        let dir = tempfile::tempdir().expect("tempdir");

        let root = ready_engine().await;
        let kept = prepared(&root, dir.path(), "16384");
        let (process, generation, pid) = timed_out_launch(&root, Arc::clone(&kept)).await;
        let endpoint = crate::inference::slots::directory().endpoint(&root);
        assert!(endpoint.transition().await.capture_suspension(&generation).is_err(), "before: the generation has no ledger (the Windows refusal)");
        process.launch_prepared(prepared(&root, dir.path(), "16384"), &|| true, None, None).await.expect("the same launch finishes the late engine");
        assert!(endpoint.transition().await.capture_suspension(&generation).is_ok(), "after: the SAME generation's ledger is published");
        assert_eq!(owned_pid(&process), pid, "the engine was finished, not replaced");

        let changed_root = ready_engine().await;
        let (changed_process, changed_generation, changed_pid) =
            timed_out_launch(&changed_root, prepared(&changed_root, dir.path(), "16384")).await;
        let changed_endpoint = crate::inference::slots::directory().endpoint(&changed_root);
        let refused = changed_process.launch_prepared(prepared(&changed_root, dir.path(), "32768"), &|| true, None, None).await;
        assert!(refused.is_err(), "a different launch is not a finish");
        assert!(changed_endpoint.transition().await.capture_suspension(&changed_generation).is_err(), "the late engine is never published for a changed request");
        assert_eq!(owned_pid(&changed_process), changed_pid, "and it is not silently killed either");
        release(process).await;
        release(changed_process).await;
    }

    // what this catches (Codex on card ea6c9aff, the deployment boundary): a recovery that
    // depends on the predecessor core's memory. A core restarted beside a healthy engine it never
    // spawned (a fresh process: no owned child, no kept launch) adopts it through the normal
    // reconcile, serving the target model, without relaunching it.
    #[tokio::test]
    async fn a_restarted_core_adopts_a_healthy_engine_it_never_spawned() {
        let root = ready_engine().await;
        let fresh = LlamaServerProcess::with_root(root.clone());
        let mut target = super::super::test_fixtures::target("late-model");
        target.placement = LanePlacement::Cpu;
        let outcome = ensure_model_serving_if_current(&fresh, &target, false, &|| true).await;
        assert!(matches!(outcome, EnsureOutcome::AlreadyServing), "a healthy engine is adopted, not replaced: {outcome:?}");
        assert!(!fresh.owns_child(), "the restarted core spawned nothing");
    }
}
