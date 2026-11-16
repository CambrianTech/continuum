//! The gene catalog's owner against a fake engine and the real resource authority (a mock capacity
//! source), replaying an adopted engine's gene change.
use super::*;
use crate::inference::llama_server::genome_catalog::{Add, Retire};
use crate::inference::llama_server::{LearningEngine, LedgerEngine};
use std::sync::Mutex;

const ENGINE: &str = "9f2c1e0a7b3d4c5e6f708192a3b4c5d6";
const REPLACEMENT: &str = "0000000000000000000000000000beef";
const WEIGHTS: u64 = 90_112;

fn ledger_of(engine: &str) -> serde_json::Value {
    serde_json::json!({
        "engine": engine, "clock_us": 1, "max_period_ms": 86_400_000, "max_expiry_ms": 86_400_000, "refusal": "none", "grants": [], "residencies": [],
        "memory": {"optimizer_owned_buffers": [], "shared_compute_buffers": [], "optimizer_unallocated_lora_ids": []}
    })
}

fn identity(engine: &str) -> LedgerEngine {
    serde_json::from_value(serde_json::json!(engine)).expect("test: an engine identity")
}

/// Every call the fake engine received, by route.
type Seen = Arc<Mutex<Vec<(&'static str, serde_json::Value)>>>;

/// A fake engine whose incarnation is `serving`; it answers an unload with `unload_status`.
async fn engine(serving: &'static str, unload_status: u16) -> (LearningEngine, Seen, tokio::task::JoinHandle<std::io::Result<()>>) {
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let (load_seen, residency_seen, unload_seen) = (Arc::clone(&seen), Arc::clone(&seen), Arc::clone(&seen));
    let app = axum::Router::new()
        .route(
            "/lora-adapters/load",
            axum::routing::post(move |axum::Json(body): axum::Json<serde_json::Value>| {
                let seen = Arc::clone(&load_seen);
                async move {
                    seen.lock().unwrap_or_else(|p| p.into_inner()).push(("load", body.clone())); // test-owned lock
                    if body["dry_run"] == true {
                        return axum::Json(serde_json::json!({"engine": serving, "reserved": false,
                            "weights": [{"device": "CUDA0", "buffer_type": "CUDA0", "bytes": WEIGHTS, "host": false}]}));
                    }
                    axum::Json(serde_json::json!({"success": true, "id": 5, "path": "/train/gene-b.gguf"}))
                }
            }),
        )
        .route(
            "/lora-adapters/unload",
            axum::routing::post(move |axum::Json(body): axum::Json<serde_json::Value>| {
                let seen = Arc::clone(&unload_seen);
                async move {
                    seen.lock().unwrap_or_else(|p| p.into_inner()).push(("unload", body)); // test-owned lock
                    let status = axum::http::StatusCode::from_u16(unload_status).expect("test: a status");
                    (status, axum::Json(serde_json::json!({"success": true, "id": 0, "path": "/genes/a.gguf"})))
                }
            }),
        )
        .route(
            "/learn/residency",
            axum::routing::post(move |axum::Json(body): axum::Json<serde_json::Value>| {
                let seen = Arc::clone(&residency_seen);
                async move {
                    seen.lock().unwrap_or_else(|p| p.into_inner()).push(("residency", body)); // test-owned lock
                    axum::Json(ledger_of(serving))
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
    let port = listener.local_addr().expect("test: addr").port();
    (LearningEngine::serving_here(port).expect("test: this process's start time is readable"), seen, tokio::spawn(async move { axum::serve(listener, app).await }))
}

fn change(engine: LearningEngine, bound: &str) -> CatalogChange {
    CatalogChange {
        engine,
        ledger: identity(bound),
        retire: vec![Retire { gene: "/genes/a.gguf".into(), id: 0 }],
        add: vec![Add { gene: "/genes/b.gguf".into(), name: "gene-b.gguf".into(), staged: "/train/gene-b.gguf".into() }],
    }
}

fn calls(seen: &Seen) -> Vec<(&'static str, serde_json::Value)> {
    seen.lock().unwrap_or_else(|p| p.into_inner()).clone() // test-owned lock
}

// what this catches (the 03:27 adoption failure, Codex's cutover blocker): an adopted engine's gene
// change unloading by bare id (the fork refuses "engine incarnation required") and loading with no
// residency (the fork requires one). The unload carries the engine identity read before the change;
// the load is measured, leased at exactly its bytes, registered, and made inside that residency.
#[tokio::test]
async fn an_adopted_engines_gene_change_unloads_bound_and_loads_inside_a_leased_residency() {
    let catalog = GeneCatalog::new(Authority::isolated(10_000_000));
    let (engine, seen, server) = engine(ENGINE, 200).await;
    let outcome = catalog.converge(change(engine, ENGINE)).await;
    assert_eq!(outcome, CatalogOutcome {
        applied: vec![CatalogStep::Retired("/genes/a.gguf".into()), CatalogStep::Added("/genes/b.gguf".into())],
        end: CatalogEnd::Done,
    });
    let calls = calls(&seen);
    assert_eq!(calls[0].0, "unload");
    assert_eq!((calls[0].1["engine"].as_str(), calls[0].1["id"].as_i64()), (Some(ENGINE), Some(0)), "bound to the engine read first");
    assert_eq!((calls[1].0, calls[1].1["dry_run"].as_bool()), ("load", Some(true)), "measured before anything is leased");
    assert_eq!(calls[2].0, "residency");
    assert_eq!(calls[2].1["leases"][0]["bytes"], WEIGHTS, "leased at exactly the measured bytes");
    assert_eq!(calls[2].1["subject"]["load"]["name"], "gene-b.gguf");
    assert_eq!((calls[3].0, &calls[3].1["residency"]), ("load", &calls[2].1["id"]), "loaded inside that residency");
    server.abort();
}

// what this catches (Codex): an engine replaced between the catalog read and a mutation. The load
// is refused before any memory is leased, and nothing is loaded into the replacement.
#[tokio::test]
async fn a_replaced_engine_refuses_the_load_before_anything_is_leased() {
    let authority = Authority::isolated(10_000_000);
    let catalog = GeneCatalog::new(authority);
    let (engine, seen, server) = engine(REPLACEMENT, 200).await;
    let mut asked = change(engine, ENGINE);
    asked.retire.clear();
    let outcome = catalog.converge(asked).await;
    assert_eq!(outcome, CatalogOutcome { applied: vec![], end: CatalogEnd::Refused(CatalogRefusal::EngineReplaced) });
    assert!(calls(&seen).iter().all(|(route, body)| *route == "load" && body["dry_run"] == true), "only measured: no residency, no load");
    server.abort();
}

// what this catches (Codex on #4467): a retire refused because a turn holds the gene must stop the
// change there, as a wait the engine records, with no gene loaded ahead of it.
#[tokio::test]
async fn a_busy_retire_stops_the_change_before_any_gene_joins() {
    let catalog = GeneCatalog::new(Authority::isolated(10_000_000));
    let (engine, seen, server) = engine(ENGINE, 503).await;
    let outcome = catalog.converge(change(engine, ENGINE)).await;
    assert_eq!(outcome, CatalogOutcome { applied: vec![], end: CatalogEnd::Busy });
    assert!(calls(&seen).iter().all(|(route, _)| *route == "unload"), "nothing loaded ahead of the busy retire");
    server.abort();
}
