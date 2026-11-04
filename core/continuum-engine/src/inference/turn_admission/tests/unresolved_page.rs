#![cfg(test)]
//! An unresolved page switch holds its slot's custody until the engine answers
//! (`turn_admission::unresolved_page`), through the real turn admission and a fake engine.

use super::super::page_liveness::watch_for_test;
use super::*;
use crate::system_resources::process_tree::CpuSample;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

/// The engine's process keeps working: its CPU counter advanced at every read.
fn always_busy() -> Option<CpuSample> {
    Some(CpuSample::Alive { spent: Some(Duration::from_secs(1)) })
}

/// A fake engine whose page switches land only when the test releases them, one per release,
/// and whose `/slots` never answers, as a main loop deep in one batch cannot. With
/// `drops_reply` set, a released switch's reply is cut mid-body: the engine closed the
/// connection, which proves nothing about whether the switch ended.
///
/// `entered` and `cut` are the engine's own events, so a test under paused time waits on what
/// the engine did rather than on virtual timeouts: those run ahead of real socket IO, and a
/// virtual 120 s can pass before a request reaches this engine at all (window 49 on the M5).
struct DelayedEngine {
    root: String,
    release: Arc<tokio::sync::Notify>,
    landed: Arc<AtomicUsize>,
    drops_reply: Arc<AtomicBool>,
    /// A page switch reached the engine.
    entered: Arc<tokio::sync::Notify>,
    /// The engine cut a released switch's reply.
    cut: Arc<tokio::sync::Notify>,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
}

async fn delayed_engine() -> DelayedEngine {
    use axum::{
        body::{Body, Bytes},
        extract::{Path, Query},
        response::{IntoResponse, Response},
        routing::{get, post},
        Json, Router,
    };
    let release = Arc::new(tokio::sync::Notify::new());
    let landed = Arc::new(AtomicUsize::new(0));
    let drops_reply = Arc::new(AtomicBool::new(false));
    let entered = Arc::new(tokio::sync::Notify::new());
    let cut = Arc::new(tokio::sync::Notify::new());
    let app = Router::new()
        .route("/slots", get(|| std::future::pending::<Json<serde_json::Value>>()))
        .route("/slots/{slot}", post({
            let (release, landed, drops_reply) = (release.clone(), landed.clone(), drops_reply.clone());
            let (entered, cut) = (entered.clone(), cut.clone());
            move |Path(slot): Path<u32>,
                  Query(query): Query<std::collections::HashMap<String, String>>,
                  Json(body): Json<serde_json::Value>| {
                let (release, landed, drops_reply) = (release.clone(), landed.clone(), drops_reply.clone());
                let (entered, cut) = (entered.clone(), cut.clone());
                async move {
                    entered.notify_one();
                    release.notified().await;
                    landed.fetch_add(1, Ordering::SeqCst);
                    if drops_reply.load(Ordering::SeqCst) {
                        cut.notify_one();
                        let cut = futures::stream::iter([
                            Ok(Bytes::from_static(b"{\"id_slot\":")),
                            Err(std::io::Error::other("test: the engine closed the connection mid-reply")),
                        ]);
                        return Response::new(Body::from_stream(cut));
                    }
                    let count = if query.get("action").is_some_and(|action| action == "save") { "n_saved" } else { "n_restored" };
                    Json(json!({"id_slot": slot, "filename": body["filename"], (count): 42})).into_response()
                }
            }
        }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
    let root = format!("http://{}", listener.local_addr().expect("test: address"));
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    DelayedEngine { root, release, landed, drops_reply, entered, cut, server }
}

/// Wait for one of the fake engine's own events, bounded in REAL time (the socket's clock): a
/// virtual timeout under a paused clock can pass before the request's bytes arrive.
async fn engine_event(event: &tokio::sync::Notify, never: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let happened = event.notified();
    tokio::pin!(happened);
    loop {
        tokio::select! {
            () = &mut happened => return,
            () = tokio::time::sleep(Duration::from_millis(50)) => assert!(std::time::Instant::now() < deadline, "test: {never}"),
        }
    }
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

// what this catches (Codex's follow-up on #5772): the held custody read a late reply that the
// engine cut mid-body (or a socket closed under the request) as a receipt and let the slot go
// unquarantined, though nothing proved the restore ended inside the engine. The late reply is
// judged by the same terminal-receipt check as the bounded wait: an unverified one quarantines
// the endpoint before the slot is let go, so no same-slot turn gets in until a verified
// replacement.
#[tokio::test(start_paused = true)]
async fn a_reply_cut_mid_body_quarantines_before_the_slot_is_let_go() {
    let engine = delayed_engine().await;
    watch_for_test(&engine.root, always_busy);
    let directory = crate::inference::slots::directory();
    let pool = directory.ensure_pool(&engine.root, 1);
    let held = PageKey::mind(Uuid::new_v4()).expect("test: held activity");
    let next = PageKey::mind(Uuid::new_v4()).expect("test: next activity");
    pool.note_saved(held);
    let sem = Arc::new(Semaphore::new(2));
    let client = reqwest::Client::new();

    let turn = admit_turn(&sem, Some(held), Some(pool.clone()), &client, &engine.root, 100, AdmissionPatience::for_turn()).await;
    assert!(turn.is_err(), "the restore admits no generation");
    // The restore is IN the engine, held: wait on the engine's own event, never a virtual
    // timeout, which can pass before the request's bytes arrive.
    engine_event(&engine.entered, "the restore never reached the engine").await;
    assert_eq!(sem.available_permits(), 1, "unresolved, not uncertain: the held custody keeps its lane");
    assert!(!directory.endpoint(&engine.root).paging_recovery_required(), "unresolved alone never quarantines");
    let mut reuse = Box::pin(admit_turn(&sem, Some(next), Some(pool.clone()), &client, &engine.root, 100, AdmissionPatience::for_turn()));
    assert!(tokio::time::timeout(Duration::from_secs(120), &mut reuse).await.is_err(), "no turn takes a held slot");

    engine.drops_reply.store(true, Ordering::SeqCst);
    engine.release.notify_one();
    engine_event(&engine.cut, "the engine never cut the released reply").await;
    assert_eq!(engine.landed.load(Ordering::SeqCst), 1, "the engine was reached and cut its reply");
    // The held custody judges the cut reply when its bytes arrive: bounded in real time, as the
    // socket is.
    let judged_by = std::time::Instant::now() + Duration::from_secs(30);
    while !directory.endpoint(&engine.root).paging_recovery_required() {
        assert!(std::time::Instant::now() < judged_by, "test: the cut reply was never judged");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let reused = tokio::time::timeout(Duration::from_secs(120), reuse).await;
    assert!(!matches!(reused, Ok(Ok(_))), "a cut reply never lets the slot be reused");
    assert!(directory.endpoint(&engine.root).paging_recovery_required(), "an unverified late reply quarantines the endpoint");
    engine.server.abort();
}

// what this catches (Codex's follow-up on #5772): a lifecycle checkpoint whose save went
// unresolved returned its error with the request held by nothing and the endpoint not
// quarantined, though an error alone does not prove the engine was retired. The checkpoint's
// pending save quarantines the endpoint as the writer lets go.
#[tokio::test(start_paused = true)]
async fn a_checkpoint_left_unresolved_quarantines_the_endpoint() {
    let dir = tempfile::tempdir().expect("test: page dir");
    let engine = delayed_engine().await;
    watch_for_test(&engine.root, always_busy);
    let (endpoint, pool) = super::lifecycle::boot(&engine.root, dir.path()).await;
    let client = reqwest::Client::new();
    let sem = Arc::new(Semaphore::new(2));
    let resident = PageKey::mind(Uuid::new_v4()).expect("test: resident activity");
    let mut turn = admit_turn(&sem, Some(resident), Some(pool), &client, &engine.root, 100, Duration::ZERO)
        .await
        .expect("test: a cold resident");
    turn.generation_completed();
    drop(turn);

    let mut writer = endpoint.transition().await;
    assert!(writer.checkpoint_residents(&client, &engine.root).await.is_err(), "the save never answered");
    drop(writer);
    assert!(endpoint.paging_recovery_required(), "an unresolved checkpoint quarantines: its error alone retires nothing");
    assert!(!endpoint.is_ready(), "no turn is admitted until a verified replacement");
    engine.release.notify_one();
    engine.server.abort();
}
