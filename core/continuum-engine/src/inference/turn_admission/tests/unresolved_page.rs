#![cfg(test)]
//! An unresolved page switch holds its slot's custody until the engine answers
//! (`turn_admission::unresolved_page`), through the real turn admission and a fake engine.

use super::super::page_liveness::watch_for_test;
use super::*;
use crate::system_resources::process_tree::CpuSample;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// The engine's process keeps working: its CPU counter advanced at every read.
fn always_busy() -> Option<CpuSample> {
    Some(CpuSample::Alive { spent: Some(Duration::from_secs(1)) })
}

/// A fake engine whose page switches land only when the test releases them, one per release,
/// and which never answers `/slots` (no route), as a main loop deep in one batch cannot.
struct DelayedEngine {
    root: String,
    release: Arc<tokio::sync::Notify>,
    landed: Arc<AtomicUsize>,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
}

async fn delayed_engine() -> DelayedEngine {
    use axum::{extract::Path, routing::post, Json, Router};
    let release = Arc::new(tokio::sync::Notify::new());
    let landed = Arc::new(AtomicUsize::new(0));
    let app = Router::new().route("/slots/{slot}", post({
        let release = release.clone();
        let landed = landed.clone();
        move |Path(slot): Path<u32>, Json(body): Json<serde_json::Value>| {
            let release = release.clone();
            let landed = landed.clone();
            async move {
                release.notified().await;
                landed.fetch_add(1, Ordering::SeqCst);
                Json(json!({"id_slot": slot, "filename": body["filename"], "n_restored": 42}))
            }
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
    let root = format!("http://{}", listener.local_addr().expect("test: address"));
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    DelayedEngine { root, release, landed, server }
}

// what this catches (Codex's review on #5772): an unresolved switch that kept only its HTTP
// future released the slot's permit, pin and endpoint reader when the turn returned, so a later
// turn reused the slot while the old restore was still queued in the engine (a late restore then
// overwrites another activity's KV) and a replacement no longer waited on it. Through the real
// admission on a one-slot engine: a restore outlives its bound on a busy engine and ends
// unresolved; a same-slot turn and then an engine transition are each attempted before the
// receipt; neither gets in until the engine answers, and the receipt releases the slot.
#[tokio::test(start_paused = true)]
async fn an_unresolved_switch_holds_its_slot_until_the_engine_answers() {
    let engine = delayed_engine().await;
    watch_for_test(&engine.root, always_busy);
    let directory = crate::inference::slots::directory();
    let pool = directory.ensure_pool(&engine.root, 1);
    let held = PageKey::mind(Uuid::new_v4()).expect("test: held activity");
    let next = PageKey::mind(Uuid::new_v4()).expect("test: next activity");
    pool.note_saved(held);
    let sem = Arc::new(Semaphore::new(2));
    let client = reqwest::Client::new();
    let patience = AdmissionPatience::for_turn;

    // The restore outlives its measured bound while the engine's process works: unresolved.
    let turn = admit_turn(&sem, Some(held), Some(pool.clone()), &client, &engine.root, 100, patience()).await;
    assert!(turn.is_err(), "an unresolved switch admits no generation");
    assert_eq!(engine.landed.load(Ordering::SeqCst), 0, "the engine has not answered yet");
    assert_eq!(sem.available_permits(), 1, "the held custody keeps its lane");
    assert!(!directory.endpoint(&engine.root).paging_recovery_required(), "unresolved alone never quarantines");

    // Same-slot reuse waits for the receipt, and gets the slot once the engine has answered.
    let mut reuse = Box::pin(admit_turn(&sem, Some(next), Some(pool.clone()), &client, &engine.root, 100, patience()));
    assert!(tokio::time::timeout(Duration::from_secs(120), &mut reuse).await.is_err(), "no turn takes a held slot");
    engine.release.notify_one();
    let mut reused = tokio::time::timeout(Duration::from_secs(120), reuse)
        .await
        .expect("test: the receipt releases the slot")
        .expect("test: the next turn is admitted");
    assert_eq!(engine.landed.load(Ordering::SeqCst), 1, "the late restore landed before the slot was reused");
    assert_eq!(reused.slot(), Some(0));
    reused.generation_completed();
    drop(reused);

    // The held activity again: unresolved again. An engine transition waits on the custody.
    let turn = admit_turn(&sem, Some(held), Some(pool.clone()), &client, &engine.root, 100, patience()).await;
    assert!(turn.is_err(), "unresolved again");
    let endpoint = directory.endpoint(&engine.root);
    let mut replacing = Box::pin(endpoint.transition());
    assert!(tokio::time::timeout(Duration::from_secs(120), &mut replacing).await.is_err(), "a replacement waits on the held custody");
    engine.release.notify_one();
    let transition = tokio::time::timeout(Duration::from_secs(120), replacing)
        .await
        .expect("test: the receipt lets the replacement through");
    assert_eq!(engine.landed.load(Ordering::SeqCst), 2, "the late restore landed before the replacement held the endpoint");
    assert_eq!(sem.available_permits(), 2, "every lane is free once the receipt is in");
    drop(transition);
    engine.server.abort();
}
