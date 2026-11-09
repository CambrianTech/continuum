//! Seating against a fake engine and the real resource authority (a mock capacity source).
use super::*;
use std::sync::{Arc, Mutex};

const ENGINE: &str = "9f2c1e0a7b3d4c5e6f708192a3b4c5d6";
const WEIGHTS: u64 = 90_112;

fn ledger(refusal: &str) -> serde_json::Value {
    serde_json::json!({
        "engine": ENGINE, "clock_us": 1, "refusal": refusal, "grants": [], "residencies": [],
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
    assert_eq!((seat.lora_id, leased(&authority)), (3, WEIGHTS), "the shadow's measured bytes are leased, no more");
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
