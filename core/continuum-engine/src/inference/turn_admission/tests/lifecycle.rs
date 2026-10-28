//! A mind's KV across a stop and a boot: she resumes where she left off.
//!
//! The validator for the lifecycle the M5 broke four ways on 2026-10-27 (the tool head at
//! launch, a save cut by the exit, a resident left unsaved in her slot, a boot plan made
//! before the ledger was counted), each found by a live restart costing minutes of
//! residents' time. Here a fake engine's slots hold "mind:tokens"; a save writes that to
//! the page file and a restore reads it back, so the test can say exactly where each mind
//! resumes. Seconds, no residents.
#![cfg(test)]
use super::*;
use axum::{
    extract::{Path, Query, State},
    routing::post,
    Json, Router,
};
use std::collections::HashMap;

/// What each slot of the fake engine holds: `Some("mind:tokens")` or empty.
type Slots = Arc<std::sync::Mutex<HashMap<u32, String>>>;

/// A fake engine whose saves and restores move slot contents through `dir`.
async fn fake_engine(dir: std::path::PathBuf) -> (String, Slots, tokio::task::JoinHandle<()>) {
    let slots: Slots = Arc::default();
    async fn page(
        State((dir, slots)): State<(std::path::PathBuf, Slots)>,
        Path(slot): Path<u32>,
        Query(query): Query<HashMap<String, String>>,
        Json(body): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        let file = dir.join(body["filename"].as_str().expect("test: a page names its file"));
        let action = query.get("action").map(String::as_str).unwrap_or("restore"); // unwrap_or: the fixture reads a bare request as a restore
        let mut slots = slots.lock().expect("test: fixture mutex");
        match action {
            "save-size" => Json(json!({"id_slot": slot, "filename": body["filename"], "projected_bytes": 100})),
            "save" => {
                let held = slots.get(&slot).cloned().unwrap_or_default(); // unwrap_or_default: an empty slot saves an empty page
                std::fs::write(&file, held).expect("test: page write");
                Json(json!({"id_slot": slot, "filename": body["filename"], "n_saved": 10}))
            }
            _ => {
                let page = std::fs::read_to_string(&file).expect("test: a restore reads a saved page");
                slots.insert(slot, page);
                Json(json!({"id_slot": slot, "filename": body["filename"], "n_restored": 10}))
            }
        }
    }
    let app = Router::new()
        .route("/props", axum::routing::get(|| async { Json(json!({"total_slots": 2})) }))
        .route("/slots/{slot}", post(page))
        .with_state((dir, slots.clone()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
    let root = format!("http://{}", listener.local_addr().expect("test: addr"));
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (root, slots, server)
}

/// One core: a ready generation on `root` whose page contract names `dir`.
async fn boot(root: &str, dir: &std::path::Path) -> (Arc<crate::inference::slots::EndpointSlots>, Arc<KvSlotPool>) {
    let endpoint = crate::inference::slots::directory().endpoint(root);
    let contract = crate::inference::slots::KvPageContract {
        model_id: "fixture".into(),
        model: "fixture.gguf".into(),
        adapters: vec![],
        page_dir: Some(dir.to_path_buf()),
        context: 8192,
        slots: 2,
        cache_type: Some("q8_0".into()),
        flash_attn: Some(true),
        revisions: Some(vec![("fixture.gguf".into(), 73, std::time::SystemTime::UNIX_EPOCH)]),
    };
    let transition = endpoint.transition().await;
    let generation = transition.start_generation().expect("test: generation");
    transition.ready(root, &generation, contract).expect("test: ready");
    drop(transition);
    let pool = endpoint.admit().await.expect("test: ready pool").pool.expect("test: managed pool");
    // The adapter's probe negotiates page writes before any turn; this fixture speaks the
    // legacy paging protocol.
    assert!(
        crate::inference::slots::directory()
            .negotiate_page_admission(root, &pool, crate::inference::slots::directory().page_discovery_generation(root), None)
            .await,
        "test: page admission negotiated"
    );
    (endpoint, pool)
}

/// One act of `mind`: admitted (paging as the pool decides), then her slot holds `tokens`.
async fn act(lanes: &Arc<Semaphore>, pool: &Arc<KvSlotPool>, client: &reqwest::Client, root: &str, slots: &Slots, mind: u128, tokens: u64) -> PageIn {
    let key = PageKey::mind(Uuid::from_u128(mind)).expect("test: non-nil id");
    let mut turn = admit_turn(lanes, Some(key), Some(pool.clone()), client, root, tokens, std::time::Duration::ZERO)
        .await
        .expect("test: admitted");
    let slot = turn.slot().expect("test: a mind's act holds a slot");
    let resumed = turn.page_in();
    slots.lock().expect("test: fixture mutex").insert(slot, format!("{mind}:{tokens}"));
    turn.generation_completed();
    resumed
}

// what this catches (the M5, 2026-10-27 19:48Z and 20:46Z): a mind returning from a stop
// anywhere but where she left off: cold because her last save was never confirmed, or
// from an older page because she was still resident in a slot when the core stopped.
#[tokio::test]
async fn every_mind_resumes_where_she_left_off_across_a_stop_and_a_boot() {
    let dir = tempfile::tempdir().expect("test: page dir");
    let client = reqwest::Client::new();
    let lanes = Arc::new(Semaphore::new(2));

    // Core A: three minds on two slots, so one is paged out and two stay resident.
    let (root_a, slots_a, server_a) = fake_engine(dir.path().to_path_buf()).await;
    let (endpoint_a, pool_a) = boot(&root_a, dir.path()).await;
    for (mind, tokens) in [(1, 100), (2, 200), (3, 300), (1, 150), (2, 250)] {
        act(&lanes, &pool_a, &client, &root_a, &slots_a, mind, tokens).await;
    }
    // Mind 3 was paged out at 300; minds 1 and 2 are resident at 150 and 250.
    let stopped = crate::inference::slots::page_io::checkpoint_endpoint(&client, &root_a, &endpoint_a, std::time::Duration::from_secs(10))
        .await
        .expect("test: an engine was launched here");
    stopped.expect("test: the stop saves every resident");
    server_a.abort();

    // Core B: a new engine, an empty slot set, the same page directory.
    let (root_b, slots_b, server_b) = fake_engine(dir.path().to_path_buf()).await;
    let (_endpoint_b, pool_b) = boot(&root_b, dir.path()).await;
    for (mind, last) in [(1u128, 150u64), (2, 250), (3, 300)] {
        let key = PageKey::mind(Uuid::from_u128(mind)).expect("test: non-nil id");
        let mut turn = admit_turn(&lanes, Some(key), Some(pool_b.clone()), &client, &root_b, last, std::time::Duration::ZERO)
            .await
            .expect("test: admitted after the boot");
        assert_eq!(turn.page_in(), PageIn::Restored, "mind {mind} restores from her page, never cold");
        let slot = turn.slot().expect("test: a slot");
        let held = slots_b.lock().expect("test: fixture mutex").get(&slot).cloned();
        assert_eq!(held, Some(format!("{mind}:{last}")), "mind {mind} resumes where she left off");
        turn.generation_completed();
    }
    server_b.abort();
}
