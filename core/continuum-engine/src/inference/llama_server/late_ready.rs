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
            engine_program: prepared.engine_program().to_string(),
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

/// Why a kept launch may not be finished now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refusal {
    /// The kept engine is not the owned one any more: the kept launch can never be finished.
    EngineGone(&'static str),
    /// The engine is the kept one but this request is another launch: it is not published for
    /// this request, and stays kept for the launch it was started for (Codex on #5545).
    OtherLaunch,
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
) -> Option<Refusal> {
    let Some(owned) = owned else { return Some(Refusal::EngineGone("no owned engine remains")) };
    if !owned.generation.same_engine(kept_generation) {
        return Some(Refusal::EngineGone("the owned engine is another generation"));
    }
    if owned.exited {
        return Some(Refusal::EngineGone("the kept engine has exited"));
    }
    if kept_pid.is_none() || owned.pid != kept_pid {
        return Some(Refusal::EngineGone("the owned engine's pid is not the kept engine's"));
    }
    if requested != kept {
        return Some(Refusal::OtherLaunch);
    }
    None
}

/// A kept launch held across the readiness wait AND publication. Only a committed publication
/// spends it: a wait that fails slow keeps it ([`Kept::settle`]), and a wait or a publication that
/// fails or is cancelled puts it back on drop, so recovery is never lost to a cancelled or failed
/// launch (Codex on #5545).
struct Kept<'a> {
    process: &'a LlamaServerProcess,
    tail: Option<ReadyTail>,
}

impl<'a> Kept<'a> {
    fn new(process: &'a LlamaServerProcess, tail: ReadyTail) -> Self {
        Self { process, tail: Some(tail) }
    }

    /// The launch, borrowed for publishing; the keeper stays armed meanwhile.
    fn launch(&self) -> Option<&ReadyTail> {
        self.tail.as_ref()
    }

    /// Publication committed: the launch is spent and nothing is kept; the candidate's evidence
    /// stands for the engine now serving.
    fn disarm(mut self) {
        if let Some(evidence) = self.tail.take().and_then(|tail| tail.evidence) {
            evidence.keep();
        }
    }

    /// After a failed wait: kept when the engine is alive and was only slow, else discarded.
    fn settle(mut self, error: &LlamaServerError) {
        if let Some(tail) = self.tail.take() {
            if matches!(error, LlamaServerError::NotReady(..)) {
                self.process.keep_loading_if_alive(tail, "the readiness wait ended while the engine still loads");
            }
        }
    }
}

impl Drop for Kept<'_> {
    fn drop(&mut self) {
        if let Some(tail) = self.tail.take() {
            self.process.keep_loading_if_alive(tail, "the launch was cancelled while its engine loads");
        }
    }
}

impl LlamaServerProcess {
    /// Wait for the engine, then publish it, holding the launch so neither a slow wait nor a
    /// cancelled one loses what was decided.
    pub(super) async fn wait_and_publish(
        &self,
        transition: &crate::inference::slots::EndpointTransition,
        tail: ReadyTail,
    ) -> Result<(), LlamaServerError> {
        let kept = Kept::new(self, tail);
        if let Err(error) = self.wait_ready().await {
            kept.settle(&error);
            return Err(error);
        }
        let Some(launch) = kept.launch() else {
            return Err(LlamaServerError::Spawn("the launch to publish was already spent".into()));
        };
        // Armed through publication: a publish that fails, or is cancelled at any await before
        // the ledger commits, puts the launch back while its engine lives.
        let published = self.publish_ready(transition, launch).await;
        if published.is_ok() {
            kept.disarm();
        }
        published
    }

    /// Keep a launch whose engine is alive, so a later launch can finish it. A restore or an
    /// exited engine keeps nothing.
    fn keep_loading_if_alive(&self, tail: ReadyTail, why: &'static str) {
        if tail.restoring || self.child_exit_status().is_some() {
            return;
        }
        continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.late_ready.kept",
            pid = ?tail.child_pid,
            model = tail.prepared.target.model_id(),
            why = why,
            "kept, so the next launch finishes this engine instead of replacing it"
        );
        *self.loading.lock() = Some(LoadingLaunch { tail });
    }

    /// The kept launch the request may finish, or `None`. A kept launch whose engine is gone is
    /// dropped; one kept for a different launch stays kept, so the launch it was started for can
    /// still finish it.
    pub(super) fn recoverable_loading(&self, requested: &PreparedLocalLaunch) -> Option<ReadyTail> {
        let owned = {
            let child = self.child.lock().unwrap(); // JUSTIFIED: poison means a prior panic mutating the owned child.
            child.as_ref().map(|owned| OwnedNow {
                generation: owned.generation.clone(),
                exited: owned.generation.has_exited(),
                pid: owned.child.id(),
            })
        };
        // The owned engine is read first, its lock released, before the kept launch's: the two
        // locks are never held together.
        let mut slot = self.loading.lock();
        let kept = slot.as_ref()?;
        let refusal = recovery_refusal(
            &LaunchContract::of(&kept.tail.prepared),
            &kept.tail.generation,
            kept.tail.child_pid,
            owned.as_ref(),
            &LaunchContract::of(requested),
        );
        let pid = kept.tail.child_pid;
        match refusal {
            None => {
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.late_ready.resumed",
                    pid = ?pid,
                    model = kept.tail.prepared.target.model_id(),
                    "the same engine, the same launch: finishing it instead of replacing it"
                );
                slot.take().map(|kept| kept.tail)
            }
            Some(Refusal::OtherLaunch) => {
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.late_ready.refused",
                    pid = ?pid,
                    why = "the request is a different launch than the one the engine was started for",
                    kept = true,
                    "not published for this request; kept for the launch it was started for"
                );
                None
            }
            Some(Refusal::EngineGone(why)) => {
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.late_ready.refused",
                    pid = ?pid,
                    why = why,
                    kept = false,
                    "the kept engine is gone: the kept launch is dropped and never published"
                );
                slot.take();
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::engine_features::{evidence_takes_tail_tools, pinned_build, ServedEngine};
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
        let gone = |r: Option<Refusal>| matches!(r, Some(Refusal::EngineGone(_)));
        assert_eq!(recovery_refusal(&kept, &kept_generation, Some(3548), Some(&same), &kept), None, "the same live engine, the same launch");
        assert!(gone(recovery_refusal(&kept, &kept_generation, Some(3548), None, &kept)), "no engine");
        assert!(gone(recovery_refusal(&kept, &kept_generation, Some(3548), Some(&now(&other_generation, false, Some(3548))), &kept)), "another generation");
        assert!(gone(recovery_refusal(&kept, &kept_generation, Some(3548), Some(&now(&kept_generation, true, Some(3548))), &kept)), "exited");
        assert!(gone(recovery_refusal(&kept, &kept_generation, Some(3548), Some(&now(&kept_generation, false, Some(4000))), &kept)), "another pid");
        assert!(gone(recovery_refusal(&kept, &kept_generation, None, Some(&same), &kept)), "an unknown kept pid proves nothing");
        // Another launch for the SAME live engine is refused but never destroys the kept launch.
        assert_eq!(recovery_refusal(&kept, &kept_generation, Some(3548), Some(&same), &contract("qwen", "32768")), Some(Refusal::OtherLaunch), "another window");
        assert_eq!(recovery_refusal(&kept, &kept_generation, Some(3548), Some(&same), &contract("llama", "16384")), Some(Refusal::OtherLaunch), "another model");
    }

    /// A fake engine whose `/health` answers 503 "loading" until `ready` is set, then 200; with
    /// an empty `/slots`, a decode that counts, `/props` with its window and slots, and its model.
    async fn fake_engine(ready: Arc<std::sync::atomic::AtomicBool>, props_open: Arc<std::sync::atomic::AtomicBool>) -> String {
        let app = axum::Router::new()
            .route("/health", axum::routing::get(move || {
                let ready = Arc::clone(&ready);
                async move {
                    if ready.load(std::sync::atomic::Ordering::SeqCst) {
                        (axum::http::StatusCode::OK, axum::Json(serde_json::json!({ "status": "ok" })))
                    } else {
                        (axum::http::StatusCode::SERVICE_UNAVAILABLE, axum::Json(serde_json::json!({
                            "load_progress": { "stages": ["text_model"], "current": "text_model", "value": 0.0 }
                        })))
                    }
                }
            }))
            .route("/slots", axum::routing::get(|| async { axum::Json(serde_json::json!([])) }))
            .route("/props", axum::routing::get(move || {
                let props_open = Arc::clone(&props_open);
                async move {
                    // Held until opened: a publication reading it stalls here, cancellable.
                    while !props_open.load(std::sync::atomic::Ordering::SeqCst) {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                    axum::Json(serde_json::json!({ "default_generation_settings": { "n_ctx": 32768 }, "total_slots": 1 }))
                }
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

    fn ready(now: bool) -> Arc<std::sync::atomic::AtomicBool> {
        Arc::new(std::sync::atomic::AtomicBool::new(now))
    }

    fn prepared(root: &str, dir: &std::path::Path, ctx: &str) -> Arc<PreparedLocalLaunch> {
        let mut target = super::super::test_fixtures::target("late-model");
        target.placement = LanePlacement::Cpu;
        Arc::new(PreparedLocalLaunch {
            target,
            engine: super::super::checked_engine::CheckedEngine::Operator { program: "llama-server".into() },
            engine_custody: Default::default(),
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

    /// Set in a stand-in engine: this test binary, re-run for one test, blocks on its stdin
    /// until the fixture lets it go. The same process works on every platform (the retirement
    /// fixture's pattern), so the recovery is exercised where it repairs (Codex on #5545).
    const STAND_IN: &str = "CONTINUUM_LATE_READY_STAND_IN";

    /// When this process IS a stand-in engine: block until released, then report true.
    fn serve_as_stand_in() -> bool {
        if std::env::var_os(STAND_IN).is_none() {
            return false;
        }
        let mut byte = [0];
        let _ = std::io::Read::read(&mut std::io::stdin(), &mut byte);
        true
    }

    /// A live stand-in engine process, kept alive by its open stdin.
    fn stand_in_engine(test: &str) -> (tokio::process::Child, tokio::process::ChildStdin) {
        let mut command = tokio::process::Command::new(std::env::current_exe().expect("the test binary"));
        command
            .args(["--exact", &format!("inference::llama_server::late_ready::tests::{test}")])
            .env(STAND_IN, "1")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        let mut child = command.spawn().expect("a stand-in engine");
        let held = child.stdin.take().expect("its stdin, held open");
        (child, held)
    }

    /// The state a readiness timeout leaves: a live engine installed under a started, never
    /// published generation, and its launch kept by the same path a real `NotReady` takes.
    /// `evidence`, when given, is the lane's evidence cell: the spawned engine's evidence is
    /// published into it at the point a launch publishes it, right after the child is installed.
    async fn timed_out_launch(
        root: &str,
        kept: Arc<PreparedLocalLaunch>,
        test: &str,
        evidence: Option<&'static ServedEvidence>,
    ) -> (LlamaServerProcess, crate::inference::slots::EngineGeneration, Option<u32>, tokio::process::ChildStdin) {
        let mut process = LlamaServerProcess::with_root(root.to_string());
        process.served_evidence = evidence;
        let (child, held) = stand_in_engine(test);
        let pid = child.id();
        let endpoint = crate::inference::slots::directory().endpoint(root);
        let transition = endpoint.transition().await;
        let generation = transition.start_generation().expect("the engine's generation");
        process.install_child(child, generation.clone(), None, None);
        let evidence = process.publish_launched_evidence(kept.engine_program(), pid);
        drop(transition);
        let tail = ReadyTail {
            prepared: kept,
            generation: generation.clone(),
            child_pid: pid,
            page_dir_ready: false,
            debug_flag: Default::default(),
            restoring: false,
            evidence,
        };
        Kept::new(&process, tail).settle(&LlamaServerError::NotReady(Duration::from_secs(90), "load progress quiet".into()));
        (process, generation, pid, held)
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

    async fn ledger_published(root: &str, generation: &crate::inference::slots::EngineGeneration) -> bool {
        crate::inference::slots::directory().endpoint(root).transition().await.capture_suspension(generation).is_ok()
    }

    // what this catches (card ea6c9aff; Windows engine 3548, 2026-10-27; Codex on #5545): a
    // launch that timed out while its engine loaded, whose late-ready engine was never published,
    // so every later launch asked the replacement checkpoint for a ledger that engine never had;
    // and a changed request that destroyed the kept launch, so even the ORIGINAL request could
    // never finish that engine. Through the real launch entry: a changed request is refused and
    // leaves the launch kept; the original request then finishes the SAME engine (same pid, the
    // generation's ledger published).
    #[tokio::test]
    async fn a_changed_request_leaves_the_late_engine_for_the_launch_that_started_it() {
        const TEST: &str = "a_changed_request_leaves_the_late_engine_for_the_launch_that_started_it";
        if serve_as_stand_in() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let root = fake_engine(ready(true), ready(true)).await;
        let (process, generation, pid, _held) = timed_out_launch(&root, prepared(&root, dir.path(), "16384"), TEST, None).await;
        assert!(!ledger_published(&root, &generation).await, "before: the generation has no ledger (the Windows refusal)");

        let changed = process.launch_prepared(prepared(&root, dir.path(), "32768"), &|| true, None, None).await;
        assert!(changed.is_err(), "a different launch is not a finish");
        assert!(!ledger_published(&root, &generation).await, "not published for a changed request");
        assert!(process.loading.lock().is_some(), "and the launch stays kept for the one it was started for");
        assert_eq!(owned_pid(&process), pid, "the engine is not killed either");

        process.launch_prepared(prepared(&root, dir.path(), "16384"), &|| true, None, None).await.expect("the original launch finishes the late engine");
        assert!(ledger_published(&root, &generation).await, "after: the SAME generation's ledger is published");
        assert_eq!(owned_pid(&process), pid, "finished, not replaced");
        release(process).await;
    }

    // what this catches (Codex on #5545): a recovery that loses the kept launch when the launch
    // waiting for the engine is cancelled (a reconcile dropped mid-wait), so the engine can never
    // be finished. The cancelled wait puts the launch back; once the engine is ready the next
    // launch finishes the same engine.
    #[tokio::test]
    async fn a_cancelled_wait_keeps_the_launch_for_the_next_one() {
        const TEST: &str = "a_cancelled_wait_keeps_the_launch_for_the_next_one";
        if serve_as_stand_in() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let engine_ready = ready(false);
        let root = fake_engine(Arc::clone(&engine_ready), ready(true)).await;
        let (process, generation, pid, _held) = timed_out_launch(&root, prepared(&root, dir.path(), "16384"), TEST, None).await;
        let process = Arc::new(process);

        let waiting = tokio::spawn({
            let (process, launch) = (Arc::clone(&process), prepared(&root, dir.path(), "16384"));
            async move { process.launch_prepared(launch, &|| true, None, None).await }
        });
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(process.loading.lock().is_none(), "the recovering launch holds the kept launch while it waits");
        waiting.abort();
        assert!(waiting.await.is_err_and(|e| e.is_cancelled()));
        assert!(process.loading.lock().is_some(), "the cancelled wait put the launch back");

        engine_ready.store(true, std::sync::atomic::Ordering::SeqCst);
        process.launch_prepared(prepared(&root, dir.path(), "16384"), &|| true, None, None).await.expect("the next launch finishes the same engine");
        assert!(ledger_published(&root, &generation).await, "the same generation's ledger is published");
        assert_eq!(owned_pid(&process), pid);
        if let Ok(process) = Arc::try_unwrap(process) {
            release(process).await;
        }
    }

    // what this catches (Codex on card ea6c9aff, the deployment boundary): a recovery that
    // depends on the predecessor core's memory. A core restarted beside a healthy engine it never
    // spawned (a fresh process: no owned child, no kept launch) adopts it through the normal
    // reconcile, serving the target model, without relaunching it.
    #[tokio::test]
    async fn a_restarted_core_adopts_a_healthy_engine_it_never_spawned() {
        let root = fake_engine(ready(true), ready(true)).await;
        let fresh = LlamaServerProcess::with_root(root.clone());
        let mut target = super::super::test_fixtures::target("late-model");
        target.placement = LanePlacement::Cpu;
        let outcome = ensure_model_serving_if_current(&fresh, &target, false, &|| true).await;
        assert!(matches!(outcome, EnsureOutcome::AlreadyServing), "a healthy engine is adopted, not replaced: {outcome:?}");
        assert!(!fresh.owns_child(), "the restarted core spawned nothing");
    }

    // what this catches (Codex on #5545): a publication cancelled after the engine answered
    // ready but before its ledger committed (here at the `/props` geometry read) losing the only
    // kept launch, which recreates the trap. The keeper stays armed until the ledger commits:
    // the abort leaves the launch kept, and the original retry finishes the same engine.
    #[tokio::test]
    async fn a_publication_cancelled_before_its_ledger_keeps_the_launch() {
        const TEST: &str = "a_publication_cancelled_before_its_ledger_keeps_the_launch";
        if serve_as_stand_in() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let props_open = ready(false);
        let root = fake_engine(ready(true), Arc::clone(&props_open)).await;
        let (process, generation, pid, _held) = timed_out_launch(&root, prepared(&root, dir.path(), "16384"), TEST, None).await;
        let process = Arc::new(process);

        let publishing = tokio::spawn({
            let (process, launch) = (Arc::clone(&process), prepared(&root, dir.path(), "16384"));
            async move { process.launch_prepared(launch, &|| true, None, None).await }
        });
        tokio::time::sleep(Duration::from_millis(800)).await;
        assert!(!publishing.is_finished(), "publication is held at the /props read");
        publishing.abort();
        assert!(publishing.await.is_err_and(|e| e.is_cancelled()));
        assert!(process.loading.lock().is_some(), "the cancelled publication put the launch back");
        assert!(!ledger_published(&root, &generation).await, "nothing was committed");

        props_open.store(true, std::sync::atomic::Ordering::SeqCst);
        process.launch_prepared(prepared(&root, dir.path(), "16384"), &|| true, None, None).await.expect("the original retry finishes the same engine");
        assert!(ledger_published(&root, &generation).await, "the same generation's ledger is published");
        assert_eq!(owned_pid(&process), pid);
        if let Ok(process) = Arc::try_unwrap(process) {
            release(process).await;
        }
    }

    /// An evidence cell holding the incumbent's own `/props` reading: the pinned build, which
    /// takes `tail_tools`. Every candidate launched below is unstamped ("llama-server" sits in no
    /// owned slot), so its evidence takes none: the two capabilities differ.
    fn incumbent_evidence() -> &'static ServedEvidence {
        let cell: &'static ServedEvidence = Box::leak(Box::new(ServedEvidence::default()));
        let incumbent = crate::inference::engine_residency::EngineIncarnation { pid: 100, started_s: 1_700_000_000, port: 1 };
        cell.lock().observe(Some(incumbent), Ok(Some(&pinned_build())));
        assert!(evidence_takes_tail_tools(cell), "the incumbent takes tail_tools");
        cell
    }

    // what this catches (Codex on 07644dc3f): a launch refused before its transition (here a
    // superseded admission; a restoration, receipt or inputs refusal returns at the same side of
    // it) that had already published the unlaunched candidate's `tail_tools` evidence, so the
    // incumbent still serving was advertised the candidate's capability.
    #[tokio::test]
    async fn a_launch_refused_before_its_transition_leaves_the_incumbents_evidence() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = "http://127.0.0.1:1/evidence-refused";
        let cell = incumbent_evidence();
        let mut process = LlamaServerProcess::with_root(root.to_string());
        process.served_evidence = Some(cell);

        let refused = process.launch_prepared(prepared(root, dir.path(), "16384"), &|| false, None, None).await;
        assert!(matches!(refused, Err(LlamaServerError::Superseded)), "the admission refuses: {refused:?}");
        assert!(evidence_takes_tail_tools(cell), "the incumbent's evidence stands, not the candidate's");
        assert!(!process.owns_child(), "nothing was spawned");
    }

    // what this catches: the other side of the same move. The spawned candidate's evidence is
    // published while it loads, and a launch that publishes the engine keeps it (not the
    // incumbent's), so her first act after a restart sees what the new engine takes.
    #[tokio::test]
    async fn a_published_launch_keeps_the_candidates_evidence() {
        const TEST: &str = "a_published_launch_keeps_the_candidates_evidence";
        if serve_as_stand_in() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let root = fake_engine(ready(true), ready(true)).await;
        let cell = incumbent_evidence();
        let (process, generation, pid, _held) = timed_out_launch(&root, prepared(&root, dir.path(), "16384"), TEST, Some(cell)).await;
        assert!(!evidence_takes_tail_tools(cell), "the spawned candidate's evidence is published while it loads");

        process.launch_prepared(prepared(&root, dir.path(), "16384"), &|| true, None, None).await.expect("the launch publishes the engine");
        assert!(ledger_published(&root, &generation).await, "published");
        assert_eq!(owned_pid(&process), pid);
        assert!(!evidence_takes_tail_tools(cell), "the published candidate's evidence stands");
        release(process).await;
    }

    // what this catches (Codex on 07644dc3f and 2b37cbfab): a launch that fails after its
    // candidate's evidence was published (a wait that did not end slow, so the launch is
    // discarded) leaving that evidence advertised, or bringing back the evidence of the incumbent
    // its transition already retired. Discarding the launch leaves no evidence at all.
    #[tokio::test]
    async fn a_discarded_launch_leaves_no_evidence_not_the_retired_incumbents() {
        const TEST: &str = "a_discarded_launch_leaves_no_evidence_not_the_retired_incumbents";
        if serve_as_stand_in() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let root = fake_engine(ready(false), ready(true)).await;
        let cell = incumbent_evidence();
        let (process, _generation, _pid, _held) = timed_out_launch(&root, prepared(&root, dir.path(), "16384"), TEST, Some(cell)).await;
        assert!(!evidence_takes_tail_tools(cell), "the spawned candidate's evidence is published");

        let tail = process.loading.lock().take().map(|kept| kept.tail).expect("the kept launch");
        Kept::new(&process, tail).settle(&LlamaServerError::Spawn("the engine exited while loading".into()));
        assert!(process.loading.lock().is_none(), "a launch that did not end slow is discarded");
        // The transition retired the incumbent before publication: a discarded candidate leaves no
        // evidence, never the dead incumbent's capability (Codex on 2b37cbfab).
        assert_eq!(*cell.lock(), ServedEngine::default(), "a discarded candidate leaves no evidence");
        assert!(!evidence_takes_tail_tools(cell), "and no retired engine's capability");
        release(process).await;
    }
}
