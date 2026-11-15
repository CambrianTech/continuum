#![cfg(test)]

use super::*;
use axum::{extract::{Path, Query, State}, routing::post, Json, Router};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;

fn lease_at(root: &str, key: PageKey) -> GrantLease {
    let mut lease = GrantHandoff::for_test(key.persona()).lease();
    lease.engine = Some(crate::inference::engine_residency::EngineIncarnation {
        pid: 7, started_s: 123,
        port: reqwest::Url::parse(root).expect("fixture URL").port().expect("fixture port"),
    });
    lease
}

#[tokio::test]
async fn dispatch_skips_an_active_holder_without_waiting_or_submitting_io() {
    let (root, engine, pool, key, custody, server) = fixture(false).await;
    drop(custody);
    pool.mark_incarnation((7, 123));
    let act = super::super::MindTurn::begin(key).await.expect("active resident");
    let client = reqwest::Client::new();
    // Poll once, without a timer: the busy answer must not depend on the act ending.
    use futures::FutureExt;
    let result = prepare(&client, &root, key.persona(), lease_at(&root, key))
        .now_or_never().expect("active holder never queues reclamation");
    assert!(matches!(result, Err(ReclaimError::Busy(ReclaimBusy::Mind))));
    assert!(engine.actions.lock().expect("actions").is_empty());
    assert_eq!(pool.holders.lock().get(&0), Some(&key));

    // The next eligible holder can be reclaimed while the first remains active.
    let (other_root, other_engine, other_pool, other_key, other_custody, other_server) = fixture(false).await;
    drop(other_custody);
    other_pool.mark_incarnation((7, 123));
    let (mut custody, pool, slot, key) = prepare(&client, &other_root, other_key.persona(), lease_at(&other_root, other_key))
        .await.expect("idle holder admitted");
    custody.ticket = Some(GrantHandoff::for_test(other_key.persona()));
    let proof = reclaim(&client, &other_root, pool, slot, key, custody).await.expect("idle holder reclaimed");
    assert_eq!(proof.into_ticket().persona(), other_key.persona());
    assert_eq!(*other_engine.actions.lock().expect("actions"), ["save", "erase"]);
    assert!(engine.actions.lock().expect("actions").is_empty());
    drop(act);
    server.abort();
    other_server.abort();
}

#[tokio::test]
async fn dispatch_skips_a_busy_slot_and_preserves_the_engine_fence() {
    let (root, engine, pool, key, custody, server) = fixture(false).await;
    drop(custody);
    pool.mark_incarnation((7, 123));
    let occupied = pool.acquire_slot(0).await.expect("slot occupied");
    let client = reqwest::Client::new();
    let result = prepare(&client, &root, key.persona(), lease_at(&root, key)).await;
    assert!(matches!(result, Err(ReclaimError::Busy(ReclaimBusy::Slot))));
    assert!(super::super::MindTurn::try_begin(key).is_some(), "failed preparation released mind custody");
    assert!(engine.actions.lock().expect("actions").is_empty());
    drop(occupied);
    pool.mark_incarnation((8, 124));
    let result = prepare(&client, &root, key.persona(), lease_at(&root, key)).await;
    assert!(matches!(result, Err(ReclaimError::NotReclaimable(NotReclaimable::ChangedEngine))));
    assert!(engine.actions.lock().expect("actions").is_empty(), "replacement engine receives no page IO");
    server.abort();
}

#[derive(Default)]
pub(super) struct Engine {
    actions: Mutex<Vec<String>>,
    saved: Mutex<Option<String>>,
    resident: Mutex<Option<String>>,
    reject_save: bool,
    reject_erase: std::sync::atomic::AtomicBool,
}
async fn action(State(engine): State<Arc<Engine>>, Path(slot): Path<u32>, Query(q): Query<HashMap<String, String>>, Json(body): Json<Value>) -> (axum::http::StatusCode, Json<Value>) {
    let action = q.get("action").expect("action");
    engine.actions.lock().expect("actions").push(action.clone());
    if (action == "save" && engine.reject_save) || (action == "erase" && engine.reject_erase.load(std::sync::atomic::Ordering::SeqCst)) {
        return (axum::http::StatusCode::BAD_REQUEST, Json(json!({"error":{"code":400,"type":"invalid_request_error","message":"save refused"}})));
    }
    match action.as_str() {
        "save" => {
            *engine.saved.lock().expect("saved") = engine.resident.lock().expect("resident").clone();
            (axum::http::StatusCode::OK, Json(json!({"id_slot":slot,"filename":body["filename"],"n_saved":60})))
        }
        "erase" => {
            assert!(engine.saved.lock().expect("saved").is_some(), "never erase before save");
            *engine.resident.lock().expect("resident") = None;
            (axum::http::StatusCode::OK, Json(json!({"id_slot":slot,"n_erased":60})))
        }
        _ => panic!("unexpected page action"),
    }
}

pub(super) async fn fixture(reject_save: bool) -> (String, Arc<Engine>, Arc<KvSlotPool>, PageKey, Custody, tokio::task::JoinHandle<()>) {
    let engine = Arc::new(Engine { reject_save, resident: Mutex::new(Some("resident-context".into())), ..Default::default() });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let root = format!("http://{}", listener.local_addr().expect("addr"));
    let app = Router::new().route("/slots/{slot}", post(action)).with_state(engine.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.expect("serve"); });
    let pool = directory().ensure_pool(&root, 2);
    assert!(directory().negotiate_page_admission(&root, &pool, directory().page_discovery_generation(&root), None).await,
        "fixture explicitly implements the legacy save protocol");
    let persona = uuid::Uuid::new_v4();
    let key = PageKey::mind(persona).expect("mind");
    assert_eq!(pool.lease(key).await, Some(0));
    pool.holders.lock().insert(0, key);
    let gate = Box::leak(Box::new(crate::runtime::admission_gate::AdmissionGate::new()));
    let custody = Custody {
        endpoint: directory().endpoint(&root).admit().await.expect("admit"),
        _mind: super::super::MindTurn::try_reclaim(persona).expect("idle fixture"),
        _permit: pool.acquire_slot(0).await.expect("permit"), _pin: pool.pin(&key),
        _page_io: super::super::admit_page_io(gate).expect("page IO"),
        ticket: Some(GrantHandoff::for_test(persona)), pending: false,
    };
    (root, engine, pool, key, custody, server)
}

#[tokio::test]
async fn verified_save_and_erase_preserve_the_page_and_produce_reclamation_proof() {
    let (root, engine, pool, key, custody, server) = fixture(false).await;
    let proof = reclaim(&reqwest::Client::new(), &root, pool.clone(), 0, key, custody).await.expect("reclaimed");
    assert_eq!(proof.into_ticket().persona(), key.persona());
    assert_eq!(*engine.actions.lock().expect("actions"), ["save", "erase"]);
    assert_eq!(engine.saved.lock().expect("saved").as_deref(), Some("resident-context"));
    assert!(engine.resident.lock().expect("resident").is_none());
    assert!(pool.confirms(&key), "saved page remains restorable");
    assert!(!pool.holders.lock().contains_key(&0), "erased slot is not marked warm");
    assert!(pool.try_acquire_slot(0).is_some(), "custody released after both receipts");
    server.abort();
}

#[tokio::test]
async fn a_refused_save_never_erases_the_resident() {
    let (root, engine, pool, key, custody, server) = fixture(true).await;
    assert!(reclaim(&reqwest::Client::new(), &root, pool.clone(), 0, key, custody).await.is_err());
    assert_eq!(*engine.actions.lock().expect("actions"), ["save"]);
    assert_eq!(engine.resident.lock().expect("resident").as_deref(), Some("resident-context"));
    assert_eq!(pool.holders.lock().get(&0), Some(&key));
    server.abort();
}

#[tokio::test]
async fn a_refused_erase_preserves_the_saved_page_without_reclamation_proof() {
    let (root, engine, pool, key, custody, server) = fixture(false).await;
    engine.reject_erase.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(reclaim(&reqwest::Client::new(), &root, pool.clone(), 0, key, custody).await.is_err());
    assert_eq!(*engine.actions.lock().expect("actions"), ["save", "erase"]);
    assert_eq!(engine.saved.lock().expect("saved").as_deref(), Some("resident-context"));
    assert_eq!(engine.resident.lock().expect("resident").as_deref(), Some("resident-context"));
    assert!(pool.confirms(&key));
    assert!(!pool.holders.lock().contains_key(&0), "erase refusal can mean invalid slot: never invent warm attribution from a rejection");
    server.abort();
}
