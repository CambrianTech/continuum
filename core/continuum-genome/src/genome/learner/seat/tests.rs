//! Seating against a fake engine and the real resource authority (a mock capacity source).
use super::*;
use std::sync::{Arc, Mutex};

const ENGINE: &str = "9f2c1e0a7b3d4c5e6f708192a3b4c5d6";
const WEIGHTS: u64 = 90_112;

fn ledger(refusal: &str) -> serde_json::Value {
    serde_json::json!({
        "engine": ENGINE, "clock_us": 1, "max_period_ms": 86_400_000, "max_expiry_ms": 86_400_000, "refusal": refusal, "grants": [], "residencies": [],
        "memory": {"optimizer_owned_buffers": [], "shared_compute_buffers": [], "optimizer_unallocated_lora_ids": []}
    })
}

fn authority() -> Authority {
    let resources = crate::resources::ResourceDaemon::start(
        vec![Arc::new(crate::resources::MockCapacitySource::new(crate::resources::ResourceKind::Vram, 10_000_000))],
        vec![],
        crate::resources::DaemonConfig::default(),
    );
    Authority { resources, serving: crate::inference::serving_lifecycle::LifecycleGate::unowned(true) }
}

fn shadow() -> FreshShadow {
    FreshShadow { name: "her-shadow.gguf".into(), rank: 8, alpha: 16.0, targets: vec!["attn_q".into(), "attn_v".into()], top_layers: 1, seed: 7 }
}

/// A fake engine that plans `WEIGHTS` of VRAM, answers residency registration with `residency`,
/// and records what each route received.
async fn engine(residency: &'static str) -> (LearningEngine, Arc<Mutex<Vec<serde_json::Value>>>, tokio::task::JoinHandle<std::io::Result<()>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (fresh_seen, residency_seen) = (Arc::clone(&seen), Arc::clone(&seen));
    let app = axum::Router::new()
        .route(
            "/lora-adapters/fresh",
            axum::routing::post(move |axum::Json(body): axum::Json<serde_json::Value>| {
                let seen = Arc::clone(&fresh_seen);
                async move {
                    seen.lock().unwrap_or_else(|p| p.into_inner()).push(body.clone()); // test-owned lock
                    if body["dry_run"] == true {
                        return axum::Json(serde_json::json!({"engine": ENGINE, "reserved": false,
                            "weights": [{"device": "CUDA0", "buffer_type": "CUDA0", "bytes": WEIGHTS, "host": false}]}));
                    }
                    axum::Json(serde_json::json!({"success": true, "id": 3, "path": "/train/her-shadow.gguf"}))
                }
            }),
        )
        .route(
            "/learn/residency",
            axum::routing::post(move |axum::Json(body): axum::Json<serde_json::Value>| {
                let seen = Arc::clone(&residency_seen);
                async move {
                    seen.lock().unwrap_or_else(|p| p.into_inner()).push(body); // test-owned lock
                    axum::Json(ledger(residency))
                }
            }),
        )
        .route("/learn/residency/release", axum::routing::post(|| async { axum::Json(ledger("none")) }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
    let root = format!("http://{}", listener.local_addr().expect("test: addr"));
    (LearningEngine::at(root), seen, tokio::spawn(async move { axum::serve(listener, app).await }))
}

fn leased(authority: &Authority) -> u64 {
    10_000_000 - authority.resources.available_for("an observer", crate::resources::ResourceKind::Vram)
}

// what this catches: a shadow loaded on memory nobody leased, or leased at a guessed size. The lease
// is exactly the engine's measured bytes, the residency names that lease, and the shadow is written
// inside that residency.
#[tokio::test]
async fn a_shadow_is_seated_on_exactly_the_measured_lease() {
    let authority = authority();
    let (engine, seen, server) = engine("none").await;
    let seat = seat(&authority, engine, "learning:her", shadow()).await.expect("test: seated");
    assert_eq!((seat.shadow.id, leased(&authority)), (3, WEIGHTS), "the shadow's measured bytes are leased, no more");
    let calls = seen.lock().unwrap_or_else(|p| p.into_inner()).clone(); // test-owned lock
    assert_eq!(calls[0]["dry_run"], true, "measured first");
    assert_eq!(calls[1]["leases"][0]["bytes"], WEIGHTS, "the residency names the lease");
    assert_eq!(calls[1]["rows"][0]["kind"], "weights");
    assert_eq!(calls[2]["residency"], calls[1]["id"], "the shadow is written inside that residency");
    server.abort();
}

// what this catches: a refused residency leaving its lease held (memory charged for a shadow that
// never loads). The refusal is typed and the board is back where it was.
#[tokio::test]
async fn a_refused_residency_releases_its_lease() {
    let authority = authority();
    let (engine, seen, server) = engine("stale_engine").await;
    let refused = seat(&authority, engine, "learning:her", shadow()).await.err().expect("test: refused");
    assert!(matches!(refused, SeatRefusal::Unmirrored(crate::inference::llama_server::ResidencyRefusal::StaleEngine)), "{refused}");
    assert_eq!(leased(&authority), 0, "nothing stays leased");
    assert_eq!(seen.lock().unwrap_or_else(|p| p.into_inner()).len(), 2, "nothing was written after the refusal"); // test-owned lock
    server.abort();
}

/// A fake engine that plans `WEIGHTS`, registers residencies, loses the shadow's load reply (a 504
/// after the write may or may not have happened), shows no adapter bound in its ledger, and answers
/// a residency release with `release`.
async fn engine_losing_the_load(release: &'static str) -> (LearningEngine, tokio::task::JoinHandle<std::io::Result<()>>) {
    let app = axum::Router::new()
        .route(
            "/lora-adapters/fresh",
            axum::routing::post(|axum::Json(body): axum::Json<serde_json::Value>| async move {
                if body["dry_run"] == true {
                    return (axum::http::StatusCode::OK, axum::Json(serde_json::json!({"engine": ENGINE, "reserved": false,
                        "weights": [{"device": "CUDA0", "buffer_type": "CUDA0", "bytes": WEIGHTS, "host": false}]})));
                }
                (axum::http::StatusCode::GATEWAY_TIMEOUT, axum::Json(serde_json::json!({"error": "upstream timed out"})))
            }),
        )
        .route("/learn/residency", axum::routing::post(|| async { axum::Json(ledger("none")) }))
        .route("/learn/grants", axum::routing::get(|| async { axum::Json(ledger("none")) }))
        .route("/learn/residency/release", axum::routing::post(move || async move { axum::Json(ledger(release)) }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
    let root = format!("http://{}", listener.local_addr().expect("test: addr"));
    (LearningEngine::at(root), tokio::spawn(async move { axum::serve(listener, app).await }))
}

// what this catches: a lost load reply read as proof that nothing loaded (Codex's audit of fed5f3f64).
// Unless the engine confirms the residency's release, the lease stays held inside the refusal.
#[tokio::test]
async fn a_lost_load_keeps_its_lease_until_the_engine_confirms_the_release() {
    let authority = authority();
    let (engine, server) = engine_losing_the_load("busy").await;
    let refused = seat(&authority, engine, "learning:her", shadow()).await.err().expect("test: refused");
    let SeatRefusal::Unfenced { retained, .. } = refused else { panic!("test: an unconfirmed load is unfenced, got {refused}") };
    assert_eq!(leased(&authority), WEIGHTS, "the lease is held while the engine may hold the shadow");
    drop(retained);
    server.abort();
}

// what this catches: the other side of the fence. A release the engine confirms proves nothing is
// live in the residency, so the lease drops and the refusal is a plain unloaded.
#[tokio::test]
async fn a_lost_load_the_engine_confirms_empty_releases_its_lease() {
    let authority = authority();
    let (engine, server) = engine_losing_the_load("none").await;
    let refused = seat(&authority, engine, "learning:her", shadow()).await.err().expect("test: refused");
    assert!(matches!(refused, SeatRefusal::Unloaded(_)), "{refused}");
    assert_eq!(leased(&authority), 0, "a confirmed-empty residency holds nothing");
    server.abort();
}

// what this catches: a stop that fails partway dropping the value, and with it leases the engine's
// memory still holds (Codex's audit of fed5f3f64). A refused release hands the mind back, lease held.
#[tokio::test]
async fn a_refused_stop_hands_the_mind_back_with_its_lease() {
    let authority = authority();
    let app = axum::Router::new()
        .route(
            "/lora-adapters/fresh",
            axum::routing::post(|axum::Json(body): axum::Json<serde_json::Value>| async move {
                if body["dry_run"] == true {
                    return axum::Json(serde_json::json!({"engine": ENGINE, "reserved": false,
                        "weights": [{"device": "CUDA0", "buffer_type": "CUDA0", "bytes": WEIGHTS, "host": false}]}));
                }
                axum::Json(serde_json::json!({"success": true, "id": 3, "path": "/train/her-shadow.gguf"}))
            }),
        )
        .route("/lora-adapters/unload", axum::routing::post(|| async { axum::Json(serde_json::json!({"success": true, "id": 3, "path": "/train/her-shadow.gguf"})) }))
        .route("/learn/residency", axum::routing::post(|| async { axum::Json(ledger("none")) }))
        .route("/learn/grants", axum::routing::post(|| async { axum::Json(ledger("none")) }))
        .route("/learn/grants/revoke", axum::routing::post(|| async { axum::Json(ledger("none")) }))
        .route("/learn/residency/release", axum::routing::post(|| async { axum::Json(ledger("busy")) }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
    let root = format!("http://{}", listener.local_addr().expect("test: addr"));
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let mind = crate::genome::learner::mind::MindLearner::start(&authority, LearningEngine::at(root), "learning:her", shadow(), std::time::Duration::from_secs(60))
        .await
        .map_err(|why| why.to_string())
        .expect("test: started");
    let (mind, _why) = mind.stop().await.err().expect("test: the release is refused");
    assert_eq!(leased(&authority), WEIGHTS, "her weights stay leased while the engine holds them");
    drop(mind);
    server.abort();
}
