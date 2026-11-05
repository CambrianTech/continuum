use super::*;
use axum::{extract::{Path, Query, State}, routing::post, Json, Router};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
struct Engine {
    actions: Mutex<Vec<String>>,
    saved: Mutex<Option<String>>,
    resident: Mutex<Option<String>>,
    reject_save: bool,
}
async fn action(State(engine): State<Arc<Engine>>, Path(slot): Path<u32>, Query(q): Query<HashMap<String, String>>, Json(body): Json<Value>) -> (axum::http::StatusCode, Json<Value>) {
    let action = q.get("action").expect("action");
    engine.actions.lock().expect("actions").push(action.clone());
    if action == "save" && engine.reject_save {
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

async fn fixture(reject_save: bool) -> (String, Arc<Engine>, Arc<KvSlotPool>, PageKey, Custody, tokio::task::JoinHandle<()>) {
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
        _mind: super::super::MindTurn::reclaim(persona).await,
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
