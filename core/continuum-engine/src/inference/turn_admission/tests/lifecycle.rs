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

/// Holds the fake engine's saves while `held`: `arrived` fires when a save reaches the
/// engine, and the save answers once `release` is notified. A save in flight, on demand.
#[derive(Default)]
struct SaveHold {
    held: std::sync::atomic::AtomicBool,
    arrived: tokio::sync::Notify,
    release: tokio::sync::Notify,
}

/// A fake engine whose saves and restores move slot contents through `dir`.
async fn fake_engine(dir: std::path::PathBuf) -> (String, Slots, Arc<SaveHold>, tokio::task::JoinHandle<()>) {
    let slots: Slots = Arc::default();
    let hold: Arc<SaveHold> = Arc::default();
    async fn page(
        State((dir, slots, hold)): State<(std::path::PathBuf, Slots, Arc<SaveHold>)>,
        Path(slot): Path<u32>,
        Query(query): Query<HashMap<String, String>>,
        Json(body): Json<serde_json::Value>,
    ) -> Json<serde_json::Value> {
        let file = dir.join(body["filename"].as_str().expect("test: a page names its file"));
        let action = query.get("action").map(String::as_str).unwrap_or("restore"); // unwrap_or: the fixture reads a bare request as a restore
        if action == "save" && hold.held.load(std::sync::atomic::Ordering::Acquire) {
            hold.arrived.notify_one();
            hold.release.notified().await;
        }
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
        .with_state((dir, slots.clone(), hold.clone()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
    let root = format!("http://{}", listener.local_addr().expect("test: addr"));
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (root, slots, hold, server)
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
    let (root_a, slots_a, _, server_a) = fake_engine(dir.path().to_path_buf()).await;
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
    let (root_b, slots_b, _, server_b) = fake_engine(dir.path().to_path_buf()).await;
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

/// Two minds resident on a fresh two-slot engine (no scratch slot), so a transient
/// borrow must detach and save one: mind 1, the cheaper page at 100 tokens.
async fn two_residents(dir: &std::path::Path) -> (String, Slots, Arc<SaveHold>, Arc<KvSlotPool>, Arc<Semaphore>, tokio::task::JoinHandle<()>) {
    let client = reqwest::Client::new();
    let lanes = Arc::new(Semaphore::new(2));
    let (root, slots, hold, server) = fake_engine(dir.to_path_buf()).await;
    let (_endpoint, pool) = boot(&root, dir).await;
    assert!(pool.scratch_slot().is_none(), "test: two slots reserve no scratch slot");
    for (mind, tokens) in [(1, 100), (2, 200)] {
        act(&lanes, &pool, &client, &root, &slots, mind, tokens).await;
    }
    let borrowed = slots.lock().expect("test: fixture mutex").get(&pool.transient_slot()).cloned();
    assert_eq!(borrowed.as_deref(), Some("1:100"), "test: a borrow takes mind 1's slot, the cheaper page");
    (root, slots, hold, pool, lanes, server)
}

// what this catches (review of #5540 at 3114e285b): a transient borrow (anonymous
// traffic on a pool with no scratch slot, the cache probe) that detaches a resident or
// starts a save after the stop closed page IO, so the stop exits with zero counted work
// while an uncounted save runs, or a resident loses her slot to traffic nobody admitted.
#[tokio::test]
async fn a_transient_borrow_after_the_stop_closes_page_io_is_refused_and_leaves_the_resident() {
    let dir = tempfile::tempdir().expect("test: page dir");
    let client = reqwest::Client::new();
    let (root, slots, _hold, pool, lanes, server) = two_residents(dir.path()).await;
    let gate: &'static crate::runtime::admission_gate::AdmissionGate =
        Box::leak(Box::new(crate::runtime::admission_gate::AdmissionGate::new()));
    gate.close();

    let refused = admit_transient_through(&client, &root, None, gate).await.err();
    assert_eq!(
        refused,
        Some(crate::inference::slots::page_io::PageIoClosed.to_string()),
        "a stopping node refuses the borrow by name"
    );
    assert_eq!(gate.in_flight(), 0, "a refused borrow holds no page IO");
    assert_eq!(
        act(&lanes, &pool, &client, &root, &slots, 1, 110).await,
        PageIn::Resident,
        "mind 1 still holds her slot: nothing detached her"
    );
    server.abort();
}

// what this catches (review of #5540 at 3114e285b): a transient save started before the
// stop closed page IO that does not hold the page permit through its ledger update, so
// the stop exits with the save unconfirmed and her good page untrusted on the next boot
// (Benchy's 163,476-token re-prefill, card 47151d36).
#[tokio::test]
async fn a_transient_save_started_before_the_close_holds_the_stop_until_its_ledger_update() {
    let dir = tempfile::tempdir().expect("test: page dir");
    let client = reqwest::Client::new();
    let (root, slots, hold, pool, lanes, server) = two_residents(dir.path()).await;
    let gate: &'static crate::runtime::admission_gate::AdmissionGate =
        Box::leak(Box::new(crate::runtime::admission_gate::AdmissionGate::new()));

    hold.held.store(true, std::sync::atomic::Ordering::Release);
    let borrow = tokio::spawn({
        let (client, root) = (client.clone(), root.clone());
        async move { admit_transient_through(&client, &root, None, gate).await }
    });
    hold.arrived.notified().await; // mind 1's save is at the engine
    hold.held.store(false, std::sync::atomic::Ordering::Release);

    let settling = crate::inference::slots::page_io::settle(gate, std::time::Duration::from_secs(10));
    tokio::pin!(settling);
    assert!(futures::poll!(&mut settling).is_pending(), "the stop waits for the started save");
    assert_eq!(gate.in_flight(), 1, "the save holds its page permit");
    assert!(gate.admit().is_none(), "the stop closed page IO");

    hold.release.notify_one();
    let settled = settling.await;
    assert_eq!((settled.in_flight_at_close, settled.cut), (1, 0), "the stop cut nothing");
    let borrowed = borrow
        .await
        .expect("test: borrow task")
        .expect("a borrow admitted before the close completes");
    assert!(borrowed.saved_evictee, "the evicted resident's save was confirmed");
    drop(borrowed);
    assert_eq!(
        act(&lanes, &pool, &client, &root, &slots, 1, 120).await,
        PageIn::Restored,
        "mind 1's confirmed page restores: the ledger was updated before the stop moved on"
    );
    server.abort();
}
