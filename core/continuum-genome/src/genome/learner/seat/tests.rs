#![cfg(test)]
//! Seating against a fake engine and the real resource authority (a mock capacity source).
use super::*;
use std::sync::{Arc, Mutex};

const ENGINE: &str = "9f2c1e0a7b3d4c5e6f708192a3b4c5d6";
const WEIGHTS: u64 = 90_112;

fn ledger(refusal: &str) -> serde_json::Value {
    ledger_of(ENGINE, refusal)
}

/// The ledger as engine incarnation `engine` reports it.
fn ledger_of(engine: &str, refusal: &str) -> serde_json::Value {
    serde_json::json!({
        "engine": engine, "clock_us": 1, "max_period_ms": 86_400_000, "max_expiry_ms": 86_400_000, "refusal": refusal, "grants": [], "residencies": [],
        "memory": {"optimizer_owned_buffers": [], "shared_compute_buffers": [], "optimizer_unallocated_lora_ids": []}
    })
}

fn authority() -> Authority {
    let resources = crate::resources::ResourceDaemon::start(
        vec![Arc::new(crate::resources::MockCapacitySource::new(crate::resources::ResourceKind::Vram, 10_000_000))],
        vec![],
        crate::resources::DaemonConfig::default(),
    );
    Authority { resources, serving: crate::inference::serving_lifecycle::LifecycleGate::unowned(true), custody: crate::genome::learner::custody::Custody::isolated() }
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
    let port = listener.local_addr().expect("test: addr").port();
    (LearningEngine::serving_here(port).expect("test: this process's start time is readable"), seen, tokio::spawn(async move { axum::serve(listener, app).await }))
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

// what this catches (Codex on #5905): a seat short of memory waiting inside the owner, so a learner
// holding her serving resources (her act gate, a lane, her slot) parked her act behind a capacity
// wait. Short, the seat answers at once holding nothing; after a real capacity change on the same
// authority the same seat is granted. Nothing bypasses the authority.
#[tokio::test]
async fn a_seat_short_of_memory_answers_at_once_holding_nothing_and_seats_once_capacity_returns() {
    use crate::inference::training_admission::TrainingWait;
    let source = Arc::new(crate::resources::MockCapacitySource::new(crate::resources::ResourceKind::Vram, WEIGHTS - 1));
    let resources = crate::resources::ResourceDaemon::start(vec![Arc::clone(&source) as Arc<dyn crate::resources::CapacitySource>], vec![], crate::resources::DaemonConfig::default());
    let authority = Authority { resources, serving: crate::inference::serving_lifecycle::LifecycleGate::unowned(true), custody: crate::genome::learner::custody::Custody::isolated() };
    let (engine, _seen, server) = engine("none").await;
    let mut changes = authority.resources.subscribe();
    changes.borrow_and_update();
    let short = tokio::time::timeout(std::time::Duration::from_secs(5), seat(&authority, engine.clone(), "learning:her", shadow()))
        .await
        .expect("test: a short seat answers, it never waits")
        .err()
        .expect("test: short of memory");
    let SeatRefusal::AwaitingMemory(wait) = short else { panic!("test: awaiting memory, got {short}") };
    assert!(matches!(wait, TrainingWait::Capacity { needed: WEIGHTS, .. }), "{wait}");
    let free = || authority.resources.available_for("an observer", crate::resources::ResourceKind::Vram);
    assert_eq!(free(), WEIGHTS - 1, "nothing is held while it waits");
    source.set_ceiling(10_000_000);
    // the learner's next pass, once the authority has published the capacity it needs (#5925)
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while free() < WEIGHTS {
            changes.changed().await.expect("test: the authority is open");
        }
    })
    .await
    .expect("test: the authority publishes the returned capacity");
    let seated = match seat(&authority, engine.clone(), "learning:her", shadow()).await {
        Ok(seated) => seated,
        Err(other) => panic!("test: seated once capacity returned, got {other}"),
    };
    assert_eq!((seated.shadow.id, 10_000_000 - free()), (3, WEIGHTS), "exactly the measured bytes, once capacity returned");
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
    engine_relaunched_after_the_load(ENGINE, release).await
}

/// The same, except that after the lost load the address answers as incarnation `after` (a
/// relaunch on the same port when `after` is not `ENGINE`).
async fn engine_relaunched_after_the_load(after: &'static str, release: &'static str) -> (LearningEngine, tokio::task::JoinHandle<std::io::Result<()>>) {
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
        .route("/learn/grants", axum::routing::get(move || async move { axum::Json(ledger_of(after, "none")) }))
        .route("/learn/residency/release", axum::routing::post(move || async move { axum::Json(ledger_of(after, release)) }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
    let port = listener.local_addr().expect("test: addr").port();
    (LearningEngine::serving_here(port).expect("test: this process's start time is readable"), tokio::spawn(async move { axum::serve(listener, app).await }))
}

// what this catches: a lost load reply read as proof that nothing loaded (Codex's audit of fed5f3f64).
// Unless the engine confirms the residency's release, its lease stays in custody as an orphan, and a
// settle pass that still cannot confirm keeps it.
#[tokio::test]
async fn a_lost_load_keeps_its_lease_until_the_engine_confirms_the_release() {
    let authority = authority();
    let (engine, server) = engine_losing_the_load("busy").await;
    let refused = seat(&authority, engine, "learning:her", shadow()).await.err().expect("test: refused");
    assert!(matches!(refused, SeatRefusal::Unloaded(_)), "{refused}");
    assert_eq!((leased(&authority), authority.custody.held()), (WEIGHTS, (1, 1)), "held in custody, orphaned");
    assert_eq!(authority.custody.settle_orphans().await, 1, "still unconfirmed, still held");
    assert_eq!(leased(&authority), WEIGHTS);
    server.abort();
}

// what this catches: the other side of the fence. A release the engine confirms proves nothing is
// live in the residency, so the lease leaves custody.
#[tokio::test]
async fn a_lost_load_the_engine_confirms_empty_releases_its_lease() {
    let authority = authority();
    let (engine, server) = engine_losing_the_load("none").await;
    let refused = seat(&authority, engine, "learning:her", shadow()).await.err().expect("test: refused");
    assert!(matches!(refused, SeatRefusal::Unloaded(_)), "{refused}");
    assert_eq!((leased(&authority), authority.custody.held()), (0, (0, 0)), "a confirmed-empty residency holds nothing");
    server.abort();
}

// what this catches: a relaunch at the same address read as the old engine's release (Codex's audit of
// 7912e8f1c, STAGE-CONTRACT 2). The new incarnation does not know the residency, which proves nothing
// about the old process's memory; while that process is not verifiably dead the lease stays.
#[tokio::test]
async fn another_incarnation_at_the_address_proves_nothing_about_the_old_memory() {
    let authority = authority();
    let (engine, server) = engine_relaunched_after_the_load("0b5e55ed0b5e55ed0b5e55ed0b5e55ed", "unknown_residency").await;
    let refused = seat(&authority, engine, "learning:her", shadow()).await.err().expect("test: refused");
    assert!(matches!(refused, SeatRefusal::Unloaded(_)), "{refused}");
    assert_eq!((leased(&authority), authority.custody.held()), (WEIGHTS, (1, 1)), "held: the admitted process is alive");
    assert_eq!(authority.custody.settle_orphans().await, 1, "still unproven");
    server.abort();
}

// what this catches: a release that reached a relaunched engine (it refuses stale_engine and changes
// nothing) read as anything but unproven.
#[tokio::test]
async fn a_release_refused_as_stale_keeps_its_lease() {
    let authority = authority();
    let (engine, server) = engine_losing_the_load("stale_engine").await;
    let refused = seat(&authority, engine, "learning:her", shadow()).await.err().expect("test: refused");
    assert!(matches!(refused, SeatRefusal::Unloaded(_)), "{refused}");
    assert_eq!((leased(&authority), authority.custody.held()), (WEIGHTS, (1, 1)), "held: nothing was released");
    server.abort();
}

// what this catches: a seat future dropped after its residency registered (a cancelled caller, a
// timeout) taking its leases with it while the engine may be loading the shadow (Codex's audit of
// d6242e612). The lease stays in custody until settle unloads the bound shadow and the engine
// confirms the release.
#[tokio::test]
async fn a_cancelled_seat_leaves_its_lease_in_custody_until_settled() {
    let authority = authority();
    let registered = Arc::new(Mutex::new(None::<String>));
    let (on_register, on_ledger) = (Arc::clone(&registered), Arc::clone(&registered));
    let unloaded = Arc::new(Mutex::new(false));
    let on_unload = Arc::clone(&unloaded);
    let app = axum::Router::new()
        .route(
            "/lora-adapters/fresh",
            axum::routing::post(|axum::Json(body): axum::Json<serde_json::Value>| async move {
                if body["dry_run"] == true {
                    return axum::Json(serde_json::json!({"engine": ENGINE, "reserved": false,
                        "weights": [{"device": "CUDA0", "buffer_type": "CUDA0", "bytes": WEIGHTS, "host": false}]}));
                }
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await; // the load the caller stops waiting for
                axum::Json(serde_json::json!({"success": true, "id": 3, "path": "/train/her-shadow.gguf"}))
            }),
        )
        .route(
            "/learn/residency",
            axum::routing::post(move |axum::Json(body): axum::Json<serde_json::Value>| {
                let registered = Arc::clone(&on_register);
                async move {
                    *registered.lock().unwrap_or_else(|p| p.into_inner()) = body["id"].as_str().map(str::to_owned); // test-owned lock
                    axum::Json(ledger("none"))
                }
            }),
        )
        .route(
            "/learn/grants",
            axum::routing::get(move || {
                let registered = Arc::clone(&on_ledger);
                async move {
                    let id = registered.lock().unwrap_or_else(|p| p.into_inner()).clone(); // test-owned lock
                    let mut answer = ledger("none");
                    answer["residencies"] = serde_json::json!([{"id": id, "engine": ENGINE, "owner": "core", "subject": {"fresh": {}},
                        "rows": [], "leases": [], "pending": false, "live": true, "revoked": false, "lora_id": 3}]);
                    axum::Json(answer)
                }
            }),
        )
        .route(
            "/lora-adapters/unload",
            axum::routing::post(move |axum::Json(body): axum::Json<serde_json::Value>| {
                let unloaded = Arc::clone(&on_unload);
                async move {
                    assert_eq!(body["engine"], ENGINE, "the unload names the incarnation that loaded it");
                    *unloaded.lock().unwrap_or_else(|p| p.into_inner()) = true; // test-owned lock
                    axum::Json(serde_json::json!({"success": true, "id": 3, "path": "/train/her-shadow.gguf"}))
                }
            }),
        )
        .route("/learn/residency/release", axum::routing::post(|| async { axum::Json(ledger("none")) }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
    let port = listener.local_addr().expect("test: addr").port();
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let cancelled = tokio::time::timeout(std::time::Duration::from_millis(500), seat(&authority, LearningEngine::serving_here(port).expect("test: this process's start time is readable"), "learning:her", shadow())).await;
    assert!(cancelled.is_err(), "test: the caller stopped waiting mid-load");
    assert_eq!((leased(&authority), authority.custody.held()), (WEIGHTS, (1, 1)), "the lease outlives the dropped future");
    assert_eq!(authority.custody.settle_orphans().await, 0, "settled");
    assert!(*unloaded.lock().unwrap_or_else(|p| p.into_inner()), "the bound shadow was unloaded before the release"); // test-owned lock
    assert_eq!((leased(&authority), authority.custody.held()), (0, (0, 0)));
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
    let port = listener.local_addr().expect("test: addr").port();
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let mind = crate::genome::learner::mind::MindLearner::start(&authority, LearningEngine::serving_here(port).expect("test: this process's start time is readable"), "learning:her", shadow(), std::time::Duration::from_secs(60))
        .await
        .map_err(|why| why.to_string())
        .expect("test: started");
    let (mind, _why) = mind.stop().await.err().expect("test: the release is refused");
    assert_eq!(leased(&authority), WEIGHTS, "her weights stay leased while the engine holds them");
    drop(mind);
    assert_eq!((leased(&authority), authority.custody.held()), (WEIGHTS, (1, 1)), "dropped, she is an orphan in custody, still leased");
    server.abort();
}
