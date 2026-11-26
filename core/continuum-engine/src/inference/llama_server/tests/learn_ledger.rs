#![cfg(test)]
//! The learning ledger's wire (card 4114a2c6), against a fake engine answering the routes fork
//! #87's wiring serves (Codex at 7051ffd95, memory receipt at 427415a7c).
use super::*;

fn ledger_json(refusal: &str) -> serde_json::Value {
    serde_json::json!({
        "engine": "9f2c1e0a7b3d4c5e6f708192a3b4c5d6",
        "clock_us": 1_234_567, "max_period_ms": 86_400_000, "max_expiry_ms": 86_400_000,
        "refusal": refusal,
        "grants": [{
            "id": "a previous core's grant", "period_ms": 2000, "share_ppm": 250000, "max_chunk_us": 40000,
            "expires_at_us": 9_000_000, "revoked": false, "outstanding": 1,
            "committed_steps": 3, "used_us": 4200
        }],
        "memory": {
            "optimizer_owned_buffers": [{"lora_id": 1, "device": "CUDA0", "buffer_type": "CUDA0", "kind": "static", "bytes": 6_488_192, "host": false}],
            "shared_compute_buffers": [{"device": "CUDA0", "buffer_type": "CUDA0", "bytes": 27_257_984, "host": false}],
            "optimizer_unallocated_lora_ids": [0]
        },
        "residencies": [{
            "id": "r-1", "engine": "9f2c1e0a7b3d4c5e6f708192a3b4c5d6", "owner": "core-a",
            "subject": {"load": {"name": "shadow.gguf"}},
            "rows": [{"device": "CUDA0", "buffer_type": "CUDA0", "kind": "weights", "bytes": 90112, "host": false}],
            "leases": [{"kind": "vram", "lease_id": "lease-7", "bytes": 90112}],
            "pending": false, "live": true, "revoked": false, "lora_id": 1
        }]
    })
}

async fn engine(app: axum::Router) -> (LearningEngine, tokio::task::JoinHandle<std::io::Result<()>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
    let port = listener.local_addr().expect("test: addr").port();
    (LearningEngine::serving_here(port).expect("test: this process's start time is readable"), tokio::spawn(async move { axum::serve(listener, app).await }))
}

// what this catches: the ledger read back as the engine reported it, every field typed: a grant
// a previous core issued stays a string id, measured optimizer bytes are carried as measured, and
// an adapter with no optimizer is named (the issuer reconciles from this, never from its request).
#[tokio::test]
async fn the_ledger_reads_back_exactly_as_the_engine_reports_it() {
    let app = axum::Router::new().route("/learn/grants", axum::routing::get(|| async { axum::Json(ledger_json("none")) }));
    let (process, server) = engine(app).await;
    let LedgerAnswer::Ledger(ledger) = process.learn_ledger().await.expect("test: the engine answered") else {
        panic!("test: an engine with the routes has a ledger")
    };
    assert_eq!(ledger.clock_us, 1_234_567);
    assert_eq!(ledger.grants[0].id, "a previous core's grant");
    assert_eq!((ledger.grants[0].outstanding, ledger.grants[0].used_us), (1, Some(4200)));
    assert_eq!(ledger.memory.optimizer[0].bytes, 6_488_192);
    assert_eq!(ledger.memory.unallocated_lora_ids, vec![0]);
    server.abort();
}

// what this catches: an engine older than the ledger read as an empty ledger (which would look like
// "no grants live" and invite registering against a route that does not exist), or a malformed
// answer read as one. The first is NoLedger, the second an error.
#[tokio::test]
async fn an_engine_without_the_routes_has_no_ledger_and_a_malformed_answer_is_an_error() {
    let (old, old_server) = engine(axum::Router::new()).await;
    assert_eq!(old.learn_ledger().await.expect("test: the engine answered"), LedgerAnswer::NoLedger);
    old_server.abort();
    let app = axum::Router::new().route("/learn/grants", axum::routing::get(|| async { axum::Json(serde_json::json!({"grants": []})) }));
    let (broken, broken_server) = engine(app).await;
    assert!(broken.learn_ledger().await.is_err(), "a reply without the ledger's identity is no ledger");
    broken_server.abort();
}

// what this catches: a residency registered with numbers the engine never measured. The core
// leases exactly the dry run's rows (named for what they hold), names the adapter the residency
// covers, and reads a refusal by the engine's residency names.
#[tokio::test]
async fn a_residency_mirrors_the_measured_rows_and_the_leases_that_cover_them() {
    let app = axum::Router::new()
        .route(
            "/lora-adapters/load",
            axum::routing::post(|axum::Json(body): axum::Json<serde_json::Value>| async move {
                assert_eq!(body["dry_run"], true, "the plan allocates nothing");
                assert_eq!(body["name"], "her-gene.gguf", "the plan names the adapter it loads");
                axum::Json(serde_json::json!({"engine": "9f2c1e0a7b3d4c5e6f708192a3b4c5d6", "reserved": false,
                    "weights": [{"device": "CUDA0", "buffer_type": "CUDA0", "bytes": 90112, "host": false}]}))
            }),
        )
        .route(
            "/learn/residency",
            axum::routing::post(|axum::Json(body): axum::Json<serde_json::Value>| async move {
                assert_eq!(body["rows"][0]["kind"], "weights");
                assert_eq!(body["leases"][0], serde_json::json!({"kind": "vram", "lease_id": "lease-7", "bytes": 90112}));
                assert_eq!(body["subject"]["load"]["name"], "her-gene.gguf", "the residency names the adapter it covers");
                axum::Json(ledger_json("uncovered"))
            }),
        );
    let (process, server) = engine(app).await;
    let LedgerAnswer::Ledger(plan) = process.plan_load("her-gene.gguf").await.expect("test: the engine answered") else {
        panic!("test: an engine with the routes plans")
    };
    let terms = ResidencyTerms {
        id: ResidencyId::issue(),
        engine: plan.engine.clone(),
        owner: uuid::Uuid::new_v4(),
        subject: ResidencySubject::Load { name: "her-gene.gguf".into() },
        rows: plan.weights.into_iter().map(|row| ResidencyRow::of(row, RowKind::Weights)).collect(),
        leases: vec![CoveringLease { kind: LeaseKind::Vram, lease_id: "lease-7".into(), bytes: 90112 }],
    };
    let LedgerAnswer::Ledger(registered) = process.register_residency(&terms).await.expect("test: the engine answered") else {
        panic!("test: an engine with the routes has a ledger")
    };
    assert_eq!(registered.refused, Some(ResidencyRefusal::Uncovered));
    assert_eq!(registered.ledger.residencies[0].leases[0].lease_id, "lease-7");
    server.abort();
}
