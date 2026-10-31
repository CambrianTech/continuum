#![cfg(test)]

use super::*;

// What this catches: the explicit command's real owner must retain a shared
// generation after caller cancellation, reject overlapping batches, and return
// the existing graded corpus/actual receipt without retiring resident weights.
#[tokio::test]
async fn explicit_teacher_batch_keeps_shared_generation_owned_until_terminal() {
    // The real shared adapter consumes the process-wide serving watch. Run
    // this boot-wiring fixture in a fresh copy of the existing test binary,
    // so its OnceLock cannot capture or contaminate parallel test daemons.
    const CHILD_ENV: &str = "CONTINUUM_SHARED_TEACHER_FIXTURE_CHILD";
    if std::env::var_os(CHILD_ENV).is_none() {
        let mut child = tokio::process::Command::new(std::env::current_exe().unwrap());
        child.args([
                "--exact",
                "modules::serving_daemon::tests::teacher::explicit_teacher_batch_keeps_shared_generation_owned_until_terminal",
                "--nocapture",
            ]).env(CHILD_ENV, "1").kill_on_drop(true);
        #[cfg(windows)]
        child.creation_flags(0x08000000); // CREATE_NO_WINDOW: no interactive fixture console.
        let output = child.output().await.unwrap();
        assert!(
            output.status.success(),
            "isolated shared teacher fixture failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    // The grader writes into its disk class; the fixture registry gives this child the real
    // class layout under its own temporary home.
    continuum_kernel::system_resources::disk_reporters::prepare_writer_fixture().await;
    crate::model_registry::init_global().expect("model registry for real teacher adapter");
    use crate::cognition::eval::EvalTask;
    use crate::cognition::eval::TeacherBatchRequest;
    use axum::{routing::{get, post}, Router};
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Semaphore::new(0));
    let calls = Arc::new(AtomicUsize::new(0));
    let quoted_body = Arc::new(std::sync::Mutex::new(None::<axum::body::Bytes>));
    let quotes = Arc::new(AtomicUsize::new(0));
    // The shared llama adapter requires authoritative slot geometry before it
    // can admit this teacher request. Match the fixture serving snapshot below.
    let app = Router::new().route("/props", get(|| async {
        axum::Json(serde_json::json!({
            "total_slots": 2,
            "default_generation_settings": {"n_ctx": 32768}
        }))
    })).route("/v1/chat/completions/input_tokens", post({
            let quoted_body = quoted_body.clone(); let quotes = quotes.clone();
            move |body: axum::body::Bytes| {
                let quoted_body = quoted_body.clone(); let quotes = quotes.clone();
                async move {
                    *quoted_body.lock().unwrap() = Some(body);
                    quotes.fetch_add(1, Ordering::SeqCst);
                    axum::Json(serde_json::json!({"input_tokens":10}))
                }
            }
        })).route("/v1/chat/completions", post({
            let entered = entered.clone(); let release = release.clone(); let calls = calls.clone();
            let quoted_body = quoted_body.clone();
            move |body: axum::body::Bytes| {
                let entered = entered.clone(); let release = release.clone(); let calls = calls.clone();
                let quoted_body = quoted_body.clone();
                async move {
                    assert_eq!(quoted_body.lock().unwrap().take(), Some(body),
                        "the real managed adapter must quote this finalized body before generation");
                    calls.fetch_add(1, Ordering::SeqCst);
                    entered.notify_one();
                    release.acquire().await.unwrap().forget();
                    let chunk = serde_json::json!({
                        "id":"owned-teacher-fixture", "model":"teacher-fixture",
                        "choices":[{"index":0,"delta":{"content":"```rust\nfn answer() -> i32 { 2 }\n```"},"finish_reason":null}]
                    });
                    let end = serde_json::json!({"id":"owned-teacher-fixture","model":"teacher-fixture",
                        "choices":[{"index":0,"delta":{},"finish_reason":"stop"}],
                        "usage":{"prompt_tokens":10,"completion_tokens":12,"total_tokens":22}});
                    ([("content-type", "text/event-stream")], format!("data: {chunk}\n\ndata: {end}\n\ndata: [DONE]\n\n"))
                }
            }
        }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}", listener.local_addr().unwrap());
    let http = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let serves = Arc::new(AtomicUsize::new(0));
    let mut configured = daemon_with(Arc::new(FakeServer::healthy(serves.clone(), true)));
    configured.set_model_resolver(Arc::new(|id| {
        let mut model = fake_model(id);
        model.provider = crate::inference::llama_server::PROVIDER_ID.into();
        Some(model)
    }));
    let daemon = Arc::new(configured);
    let mut snapshot = ServingSnapshot::empty();
    snapshot.active_model = Some("teacher-fixture".into());
    snapshot.base_url = format!("{root}/v1");
    snapshot.ready = true;
    snapshot.served_context_window = 32768;
    snapshot.lanes = 2;
    daemon.serving_tx.send_replace(snapshot);
    assert!(
        crate::inference::llama_server::install_serving_state(daemon.subscribe_serving()),
        "fresh child must install this fixture daemon's serving authority"
    );
    crate::inference::llama_server::mark_first_reconcile();
    let task: EvalTask = serde_json::from_value(serde_json::json!({
            "id":"owned-task", "prompt":"Write answer returning two", "lang":"rust", "test":"assert_eq!(answer(), 2);"
        })).unwrap();
    let request = |tasks| TeacherBatchRequest {
        tasks,
        teacher_model: "teacher-fixture".into(),
        temperature: 0.0,
        max_fix_iters: Some(0),
    };
    let first_owner = daemon.clone();
    let first_request = request(vec![task.clone()]);
    let mut first = tokio::spawn(async move { first_owner.run_teacher_batch(first_request).await });
    tokio::time::timeout(Duration::from_secs(5), async {
            tokio::select! {
                _ = entered.notified() => {},
                result = &mut first => match result {
                    Ok(Err(error)) => panic!("teacher returned before HTTP generation: {error}"),
                    Ok(Ok(corpus)) => panic!("teacher returned {} examples before HTTP generation", corpus.examples.len()),
                    Err(error) => panic!("teacher task ended before HTTP generation: {error}"),
                },
            }
        }).await.expect("teacher must reach fixture HTTP within its existing budget");
    assert!(daemon.reconciling.load(Ordering::Acquire));
    release.add_permits(1);
    let corpus = tokio::time::timeout(Duration::from_secs(20), first)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(corpus.examples.len(), 1);
    assert!(
        corpus.examples[0]["metadata"]["teacherGenerations"]
            .as_array()
            .unwrap()
            .len()
            == 1
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        while daemon.reconciling.load(Ordering::Acquire) {
            tick.tick().await;
            daemon.poll_teacher_batch();
        }
    })
    .await
    .unwrap();
    let next_owner = daemon.clone();
    let next_request = request(vec![task.clone(), task]);
    let mut next = tokio::spawn(async move { next_owner.run_teacher_batch(next_request).await });
    tokio::time::timeout(Duration::from_secs(5), async {
            tokio::select! {
                _ = entered.notified() => {},
                result = &mut next => match result {
                    Ok(Err(error)) => panic!("teacher returned before HTTP generation: {error}"),
                    Ok(Ok(corpus)) => panic!("teacher returned {} examples before HTTP generation", corpus.examples.len()),
                    Err(error) => panic!("teacher task ended before HTTP generation: {error}"),
                },
            }
        }).await.expect("teacher must reach fixture HTTP within its existing budget");
    next.abort();
    assert!(next.await.err().expect("cancelled command").is_cancelled());
    assert!(
        daemon.reconciling.load(Ordering::Acquire),
        "caller cancellation does not cancel shared decoding"
    );
    assert!(daemon.run_teacher_batch(request(vec![])).await.is_err());
    release.add_permits(1);
    tokio::time::timeout(Duration::from_secs(20), async {
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        while daemon.reconciling.load(Ordering::Acquire) {
            tick.tick().await;
            daemon.poll_teacher_batch();
        }
    })
    .await
    .unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "cancelled batch schedules no second task"
    );
    assert_eq!(
        quotes.load(Ordering::SeqCst),
        2,
        "each generation is quoted once"
    );
    assert_eq!(
        serves.load(Ordering::SeqCst),
        0,
        "same-model sharing does not retire or spawn"
    );
    assert!(daemon.serving_tx.borrow().ready);
    http.abort();
}
