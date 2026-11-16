//! The gene catalog's owner against a fake engine and the real resource authority (a mock capacity
//! source), replaying an adopted engine's gene change.
use super::*;
use crate::inference::llama_server::genome_catalog::{Add, Retire};
use crate::inference::llama_server::{LearningEngine, LedgerEngine};
use crate::inference::serving_lifecycle::ServingOperation;
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
    LedgerEngine::fixture(engine)
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
                    let received = body.clone();
                    seen.lock().unwrap_or_else(|p| p.into_inner()).push(("load", received)); // test-owned lock
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
                    let status = axum::http::StatusCode::from_u16(unload_status).expect("test: a status"); // test: a fixture step that cannot fail here
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
        )
        .route("/learn/residency/release", axum::routing::post(move || async move { axum::Json(ledger_of(serving)) }))
        // a release is confirmed only by the admitted incarnation's own ledger
        .route("/learn/grants", axum::routing::get(move || async move { axum::Json(ledger_of(serving)) }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind"); // test: a fixture step that cannot fail here
    let port = listener.local_addr().expect("test: addr").port(); // test: a fixture step that cannot fail here
    (LearningEngine::serving_here(port).expect("test: this process's start time is readable"), seen, tokio::spawn(async move { axum::serve(listener, app).await })) // test: a fixture step that cannot fail here
}

fn change<'a>(operation: &'a ServingOperation, engine: &LearningEngine, bound: &str) -> CatalogChange<'a> {
    let engine = engine.to_owned();
    CatalogChange {
        operation,
        engine,
        ledger: identity(bound),
        retire: vec![Retire { gene: "/genes/a.gguf".into(), id: 0 }],
        add: vec![Add { gene: "/genes/b.gguf".into(), name: "gene-b.gguf".into(), staged: "/train/gene-b.gguf".into() }],
    }
}

/// An authority whose serving gate the test holds, as the serving daemon holds it through a
/// reconcile; and its resource daemon, to read what stays leased.
fn held_authority() -> (Authority, ServingOperation, Arc<crate::resources::ResourceDaemon>) {
    let authority = Authority::isolated(10_000_000);
    let operation = authority.serving.try_operation().expect("test: a fresh gate is free"); // test: a fixture step that cannot fail here
    let resources = Arc::clone(&authority.resources);
    (authority, operation, resources)
}

fn leased(resources: &crate::resources::ResourceDaemon) -> u64 {
    10_000_000 - resources.available_for("an observer", crate::resources::ResourceKind::Vram)
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
    // the reconcile holds the real lifecycle gate the whole time (Codex on #6053: a load that took
    // the gate again was refused as Operation during every actual reconcile)
    let (authority, operation, resources) = held_authority();
    let catalog = GeneCatalog::new(authority);
    let (engine, seen, server) = engine(ENGINE, 200).await;
    let outcome = catalog.converge(change(&operation, &engine, ENGINE)).await;
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
    assert_eq!(leased(&resources), WEIGHTS, "leased under the held gate, nothing bypassed");
    server.abort();
}

// what this catches (Codex): an engine replaced between the catalog read and a mutation. The load
// is refused before any memory is leased, and nothing is loaded into the replacement.
#[tokio::test]
async fn a_replaced_engine_refuses_the_load_before_anything_is_leased() {
    let (authority, operation, resources) = held_authority();
    let catalog = GeneCatalog::new(authority);
    let (engine, seen, server) = engine(REPLACEMENT, 200).await;
    let mut asked = change(&operation, &engine, ENGINE);
    asked.retire.clear();
    let outcome = catalog.converge(asked).await;
    assert_eq!(outcome, CatalogOutcome { applied: vec![], end: CatalogEnd::Refused(CatalogRefusal::EngineReplaced) });
    assert!(calls(&seen).iter().all(|(route, body)| *route == "load" && body["dry_run"] == true), "only measured: no residency, no load");
    assert_eq!(leased(&resources), 0);
    server.abort();
}

// what this catches (Codex on #4467): a retire refused because a turn holds the gene must stop the
// change there, as a wait the engine records, with no gene loaded ahead of it.
#[tokio::test]
async fn a_busy_retire_stops_the_change_before_any_gene_joins() {
    let (authority, operation, _resources) = held_authority();
    let catalog = GeneCatalog::new(authority);
    let (engine, seen, server) = engine(ENGINE, 503).await;
    let outcome = catalog.converge(change(&operation, &engine, ENGINE)).await;
    assert_eq!(outcome, CatalogOutcome { applied: vec![], end: CatalogEnd::Busy });
    assert!(calls(&seen).iter().all(|(route, _)| *route == "unload"), "nothing loaded ahead of the busy retire");
    server.abort();
}

// what this catches (Codex on #6053): seats keyed by gene path alone. Loading the same gene on a
// second engine replaced the first engine's seat, orphaning a claim whose settlement could unload a
// still-serving adapter. Each engine's copy is its own seat and lease; retiring it from one leaves
// the other held.
#[tokio::test]
async fn the_same_gene_on_two_engines_is_two_seats_and_retiring_one_keeps_the_other() {
    const SECOND: &str = "11111111111111111111111111111111";
    let (authority, operation, resources) = held_authority();
    let catalog = GeneCatalog::new(authority);
    let (first, _seen_first, first_server) = engine(ENGINE, 200).await;
    let (second, _seen_second, second_server) = engine(SECOND, 200).await;
    let (first_process, second_process) = (first.process(), second.process());
    let load = |engine: &LearningEngine, bound: &str| {
        let mut asked = change(&operation, engine, bound);
        asked.retire.clear();
        asked
    };
    assert_eq!(catalog.converge(load(&first, ENGINE)).await.end, CatalogEnd::Done);
    assert_eq!(catalog.converge(load(&second, SECOND)).await.end, CatalogEnd::Done);
    assert_eq!(leased(&resources), 2 * WEIGHTS, "each engine's copy is leased");
    let retire = CatalogChange {
        operation: &operation,
        engine: first,
        ledger: identity(ENGINE),
        retire: vec![Retire { gene: "/genes/b.gguf".into(), id: 5 }],
        add: vec![],
    };
    assert_eq!(catalog.converge(retire).await.end, CatalogEnd::Done);
    assert!(!catalog.holds(first_process, "/genes/b.gguf").await);
    assert!(catalog.holds(second_process, "/genes/b.gguf").await, "the other engine's seat stays");
    assert_eq!(leased(&resources), WEIGHTS, "and its lease stays held");
    first_server.abort();
    second_server.abort();
}
