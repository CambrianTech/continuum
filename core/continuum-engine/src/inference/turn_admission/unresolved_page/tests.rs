#![cfg(test)]

use super::*;
use super::super::page_liveness::Condemnation;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

struct Custody(Arc<AtomicBool>);
impl Drop for Custody {
    fn drop(&mut self) { self.0.store(true, Ordering::SeqCst); }
}

fn pending() -> (UnresolvedPage, tokio::sync::oneshot::Sender<PageReply>) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    (UnresolvedPage {
        root: format!("http://late-receipt-{}:8085", uuid::Uuid::new_v4()),
        action: "erase".into(), slot: 0, filename: "page.bin".into(),
        cpu_spent: Duration::from_millis(1), checkpoint: Duration::from_secs(60),
        pending: Box::pin(async move { rx.await.expect("fixture reply sender") }),
        liveness: PageLiveness::new(Box::new(|| None), Condemnation::Never),
    }, tx)
}

#[tokio::test]
async fn a_late_verified_receipt_returns_custody_to_its_continuation() {
    let (operation, reply) = pending();
    let dropped = Arc::new(AtomicBool::new(false));
    let continuation = operation.resume(Custody(dropped.clone()));
    tokio::task::yield_now().await;
    assert!(!dropped.load(Ordering::SeqCst));
    reply.send(Ok((200, Some(serde_json::json!({"id_slot":0,"n_erased":60}))))).expect("reply");
    let PageResolution::Completed(custody) = continuation.await.expect("continuation") else { panic!("verified erase must resume"); };
    assert!(!dropped.load(Ordering::SeqCst), "continuation still owns the slot");
    drop(custody);
    assert!(dropped.load(Ordering::SeqCst));
}

#[tokio::test]
async fn an_invalid_late_receipt_fences_instead_of_resuming_io() {
    let (operation, reply) = pending();
    let endpoint = crate::inference::slots::directory().endpoint(&operation.root);
    let dropped = Arc::new(AtomicBool::new(false));
    let continuation = operation.resume(Custody(dropped.clone()));
    reply.send(Ok((200, Some(serde_json::json!({"id_slot":99,"n_erased":60}))))).expect("reply");
    assert!(matches!(continuation.await.expect("continuation"), PageResolution::Fenced));
    assert!(dropped.load(Ordering::SeqCst));
    assert!(endpoint.admit().await.is_err(), "unverified receipt quarantines before custody release");
}

#[tokio::test]
async fn dropping_the_continuation_keeps_custody_until_the_receipt() {
    let (operation, reply) = pending();
    let dropped = Arc::new(AtomicBool::new(false));
    let continuation = operation.resume(Custody(dropped.clone()));
    drop(continuation);
    tokio::task::yield_now().await;
    assert!(!dropped.load(Ordering::SeqCst));
    reply.send(Ok((200, Some(serde_json::json!({"id_slot":0,"n_erased":60}))))).expect("reply");
    tokio::time::timeout(Duration::from_secs(1), async {
        while !dropped.load(Ordering::SeqCst) { tokio::task::yield_now().await; }
    }).await.expect("receipt releases abandoned continuation");
}

// Uses the production reservation owner without the process-global grants watch.
#[tokio::test]
async fn preparation_custody_requires_a_terminal_page_receipt() {
    use crate::capacity::window_grant::{dyno_seam::OwnerUnderTest, GrantLease, GrantOwner, GrantRevision};
    use crate::inference::engine_residency::EngineIncarnation;
    for outcome in ["completed", "rejected", "malformed", "condemned"] {
        let (operation, reply) = pending();
        let owner = OwnerUnderTest::new();
        let mind = uuid::Uuid::new_v4();
        let engine = Some(EngineIncarnation { pid: 42, started_s: 100, port: 8085 });
        let lease = GrantLease { tokens: 60, engine, revision: GrantRevision::test(1), owner: GrantOwner::this_process() };
        let sem = Arc::new(tokio::sync::Semaphore::new(1));
        let mut admission = super::super::admit_turn(
            &sem, crate::inference::slots::SlotClass::Background, None, None,
            &reqwest::Client::new(), &operation.root, 0, Duration::ZERO,
        ).await.expect("test: admission"); // JUSTIFIED: fixture
        owner.reserve(&mut admission.preparation, mind, lease, 0, Some(lease)).expect("test: claim"); // JUSTIFIED: fixture
        admission.preparation.as_mut().expect("test: claim").mark_dispatched(); // JUSTIFIED: fixture
        if outcome == "condemned" {
            finish_turn(Released::Condemned, admission);
        } else {
            operation.hold_turn(admission);
            let response = match outcome {
                "completed" => (200, Some(serde_json::json!({"id_slot":0,"n_erased":60}))),
                "rejected" => (400, Some(serde_json::json!({"error":{"code":400,"type":"invalid_request_error","message":"rejected"}}))),
                _ => (200, Some(serde_json::json!({"id_slot":99,"n_erased":60}))),
            };
            reply.send(Ok(response)).expect("test: reply"); // JUSTIFIED: fixture
            tokio::time::timeout(Duration::from_secs(2), async {
                while sem.available_permits() == 0 { tokio::task::yield_now().await; }
            }).await.expect("test: receipt processed"); // JUSTIFIED: bounded fixture
        }
        let observed = owner.observe(engine);
        if matches!(outcome, "completed" | "rejected") {
            assert!(observed.charged.is_empty(), "{outcome}: abandoned preparation ends without generation");
        } else {
            assert_eq!(observed.charged.get(&mind), Some(&60), "{outcome}: endpoint closure is not physical completion");
            assert_eq!(observed.unresolved.get(&mind), Some(&1));
        }
    }
}