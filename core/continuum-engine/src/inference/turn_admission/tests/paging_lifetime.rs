#![cfg(test)]

use super::*;
use std::time::Duration;

// Sahar's measured generation estimate was 2s while healthy restores took >3s.
// The production admission policy must let the owned restore finish and admit
// subsequent useful generation instead of abandoning it on that prediction.
#[tokio::test]
async fn healthy_restore_outlives_generation_prediction() -> Result<(), Box<dyn std::error::Error>> {
    use axum::{extract::Path, routing::post, Json, Router};
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let app = Router::new().route("/slots/{slot}", post({
        let entered = entered.clone();
        let release = release.clone();
        move |Path(slot): Path<u32>, Json(body): Json<serde_json::Value>| {
            let entered = entered.clone();
            let release = release.clone();
            async move {
                entered.notify_one();
                release.notified().await;
                Json(json!({"id_slot": slot, "filename": body["filename"], "n_restored": 42}))
            }
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let root = format!("http://{}", listener.local_addr()?);
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let pool = crate::inference::slots::directory().ensure_pool(&root, 3);
    let key = PageKey::mind(Uuid::new_v4()).ok_or("activity key")?;
    pool.note_saved(key);
    let sem = Arc::new(Semaphore::new(1));
    let client = reqwest::Client::new();
    let mut admitting = Box::pin(admit_turn(&sem, Some(key), Some(pool.clone()),
        &client, &root, 100, AdmissionPatience::for_turn()));
    tokio::select! {
        result = &mut admitting => return Err(format!("admitted before restore: {:?}", result.err()).into()),
        _ = entered.notified() => {}
    }
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(3)).await;
    assert!(futures::poll!(&mut admitting).is_pending());
    assert_eq!(sem.available_permits(), 0);
    tokio::time::resume();
    release.notify_one();
    let admitted = admitting.as_mut().await?;
    assert!(admitted.page_confirmed);
    assert!(admitted.slot().is_some());
    drop(admitted);
    drop(admitting);
    let subsequent = admit_turn(&sem, Some(key), Some(pool), &client, &root,
        100, AdmissionPatience::for_turn()).await?;
    assert!(subsequent.page_confirmed);
    drop(subsequent);
    assert_eq!(sem.available_permits(), 1);
    server.abort();
    let _ = server.await;
    Ok(())
}

// What this catches: abandonment during evictee SAVE must retain its durable
// ledger receipt, but must not launch the next RESTORE for a requester who left.
#[tokio::test]
async fn abandoned_context_switch_finishes_save_without_starting_restore() {
    use axum::{
        extract::{Path, Query},
        routing::post,
        Json, Router,
    };
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let calls = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let app = Router::new().route(
        "/slots/{slot}",
        post({
            let entered = entered.clone();
            let release = release.clone();
            let calls = calls.clone();
            move |Path(slot): Path<u32>,
                  Query(query): Query<std::collections::HashMap<String, String>>,
                  Json(body): Json<serde_json::Value>| {
                let entered = entered.clone();
                let release = release.clone();
                let calls = calls.clone();
                async move {
                    let action = query.get("action").expect("test: action").clone();
                    calls.lock().expect("test: calls").push(action.clone());
                    entered.notify_one();
                    release.notified().await;
                    let count = if action == "save" {
                        "n_saved"
                    } else {
                        "n_restored"
                    };
                    Json(json!({"id_slot": slot, "filename": body["filename"], (count): 42}))
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test: bind");
    let root = format!("http://{}", listener.local_addr().expect("test: address"));
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let directory = crate::inference::slots::directory();
    let pool = directory.ensure_pool(&root, 1);
    assert!(
        directory
            .negotiate_page_admission(&root, &pool, None, None)
            .await
    );
    let old = PageKey::mind(Uuid::new_v4()).expect("test: old activity");
    let next = PageKey::mind(Uuid::new_v4()).expect("test: next activity");
    let sem = Arc::new(Semaphore::new(1));
    let client = reqwest::Client::new();
    let mut resident = admit_turn(
        &sem,
        Some(old),
        Some(pool.clone()),
        &client,
        &root,
        100,
        Duration::ZERO,
    )
    .await
    .expect("test: cold resident");
    resident.generation_completed();
    drop(resident);
    pool.note_saved(next);
    let mut admitting = Box::pin(admit_turn(
        &sem,
        Some(next),
        Some(pool.clone()),
        &client,
        &root,
        100,
        Duration::ZERO,
    ));
    tokio::time::timeout(Duration::from_secs(5), async {
        tokio::select! {
            _ = &mut admitting => panic!("test: save completed before receipt"),
            _ = entered.notified() => {}
        }
    })
    .await
    .expect("test: save reached engine");
    drop(admitting);
    release.notify_one();
    let permit = tokio::time::timeout(Duration::from_secs(5), pool.acquire_slot(0))
        .await
        .expect("test: abandoned switch released after save")
        .expect("test: physical slot");
    assert_eq!(calls.lock().expect("test: calls").as_slice(), &["save"]);
    assert!(
        pool.plan_paging(0, old).restore,
        "completed save remains eligible for its owner"
    );
    assert!(!directory.endpoint(&root).paging_recovery_required());
    drop(permit);
    server.abort();
    let _ = server.await;
}

// Regression for 424e0961: a caller's deadline/drop released a live restore's
// generation reader and physical slot, quarantining a healthy engine repeatedly.
// Exercise actual admit_turn and HTTP; only the requester clock is advanced.
#[tokio::test]
async fn abandoned_restore_retains_slot_lane_and_generation_until_terminal_receipt() {
    use axum::{extract::Path, routing::post, Json, Router};

    for deadline in [false, true] {
        for verified in [false, true] {
            let entered = Arc::new(tokio::sync::Notify::new());
            let release = Arc::new(tokio::sync::Notify::new());
            let app = Router::new().route("/slots/{slot}", post({
                let entered = entered.clone();
                let release = release.clone();
                move |Path(slot): Path<u32>, Json(body): Json<serde_json::Value>| {
                    let entered = entered.clone();
                    let release = release.clone();
                    async move {
                        entered.notify_one();
                        release.notified().await;
                        Json(if verified {
                            json!({"id_slot": slot, "filename": body["filename"], "n_restored": 42})
                        } else {
                            json!({"headers_are_not_completion": true})
                        })
                    }
                }
            }));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("test: bind");
            let root = format!(
                "http://{}",
                listener.local_addr().expect("test: bound address")
            );
            let server = tokio::spawn(async move { axum::serve(listener, app).await });
            let endpoint = crate::inference::slots::directory().endpoint(&root);
            let pool = crate::inference::slots::directory().ensure_pool(&root, 3);
            let key = PageKey::mind(Uuid::new_v4()).expect("test: activity");
            pool.note_saved(key);
            let sem = Arc::new(Semaphore::new(1));
            let client = reqwest::Client::new();
            let mut admitting = Box::pin(admit_turn(
                &sem,
                Some(key),
                Some(pool.clone()),
                &client,
                &root,
                100,
                AdmissionPatience::until(tokio::time::Instant::now() + Duration::from_secs(5)),
            ));
            tokio::time::timeout(Duration::from_secs(5), async {
                tokio::select! {
                    _ = &mut admitting => panic!("test: admission completed before paging receipt"),
                    _ = entered.notified() => {}
                }
            })
            .await
            .expect("test: restore dispatched");
            let slot = pool.lease(key).await.expect("test: pinned activity");
            if deadline {
                tokio::time::pause();
                tokio::time::advance(Duration::from_secs(6)).await;
                let result = admitting.as_mut().await;
                assert!(
                    matches!(result, Err(ref error) if error == "turn deadline elapsed during admission")
                );
                tokio::time::resume();
            }
            drop(admitting);
            assert!(MindTurn::try_begin(key).is_none(), "paging retains the mind gate");
            assert_eq!(
                sem.available_permits(),
                0,
                "abandonment cannot release the paging lane"
            );
            assert!(
                !endpoint.paging_recovery_required(),
                "abandonment is not an uncertain engine receipt"
            );
            let mut physical = Box::pin(pool.acquire_slot(slot));
            assert!(
                futures::poll!(&mut physical).is_pending(),
                "no same-slot reuse during IO"
            );
            let mut transition = Box::pin(endpoint.transition());
            assert!(
                futures::poll!(&mut transition).is_pending(),
                "replacement must drain the exact reader"
            );
            release.notify_one();
            let permit = tokio::time::timeout(Duration::from_secs(5), physical)
                .await
                .expect("test: terminal receipt releases slot")
                .expect("test: slot permit");
            let drained = tokio::time::timeout(Duration::from_secs(5), transition)
                .await
                .expect("test: terminal receipt releases generation reader");
            assert_eq!(sem.available_permits(), 1);
            assert_eq!(
                endpoint.paging_recovery_required(),
                !verified,
                "unknown receipts still quarantine, even after the caller has left"
            );
            assert_eq!(
                pool.plan_paging(slot, key).already_resident,
                verified,
                "only a verified restore survives abandoned result delivery"
            );
            drop(permit);
            drop(drained);
            server.abort();
            let _ = server.await;
        }
    }
}

// What this catches: retaining only the header future leaves an endpoint reader
// forever if a server sends headers and then never finishes its receipt body.
#[tokio::test]
async fn paging_progress_watchdog_covers_the_complete_response_body() {
    use axum::{
        body::{Body, Bytes},
        routing::post,
        Router,
    };
    use futures::StreamExt;
    let headers = Arc::new(tokio::sync::Notify::new());
    let app = Router::new().route(
        "/slots/0",
        post({
            let headers = headers.clone();
            move || {
                let headers = headers.clone();
                async move {
                    Body::from_stream(
                        futures::stream::once(async move {
                            headers.notify_one();
                            Ok::<_, std::convert::Infallible>(Bytes::from_static(b"{\"id_slot\":"))
                        })
                        .chain(futures::stream::pending()),
                    )
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test: bind");
    let root = format!("http://{}", listener.local_addr().expect("test: address"));
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let client = reqwest::Client::new();
    let mut probes = 0;
    let receipt = bounded_page_receipt(
        client.post(format!("{root}/slots/0")),
        Duration::from_millis(20),
        || {
            probes += 1;
            let initial = probes == 1;
            let headers = headers.clone();
            async move {
                if initial {
                    // Start the watchdog interval only after the real HTTP body
                    // begins. No paused clock or shared p90/probe state is involved.
                    headers.notified().await;
                    EngineProbe::Busy
                } else {
                    EngineProbe::OperationEnded
                }
            }
        },
        |_| {},
    );
    let outcome = tokio::time::timeout(Duration::from_secs(5), receipt)
        .await
        .expect("test: watchdog ends incomplete body receipt");
    assert!(
        outcome.is_none(),
        "headers must not escape as a completed receipt"
    );
    assert_eq!(
        probes, 2,
        "initial observation then terminal watchdog checkpoint"
    );
    server.abort();
    let _ = server.await;
}

// A task panic must close the waiter, release the exact lifecycle reader, and
// retain fail-closed uncertainty. A retained task is not a detached success path.
#[tokio::test]
async fn paging_owner_panic_releases_guards_and_quarantines_uncertain_io() {
    let root = format!("http://paging-panic-{}", Uuid::new_v4());
    let directory = crate::inference::slots::directory();
    let endpoint = directory.endpoint(&root);
    let pool = directory.ensure_pool(&root, 1);
    assert!(
        directory
            .negotiate_page_admission(&root, &pool, None, None)
            .await
    );
    let key = PageKey::mind(Uuid::new_v4()).expect("test: activity");
    let sem = Arc::new(Semaphore::new(1));
    let client = reqwest::Client::new();
    let admission = admit_turn(
        &sem,
        Some(key),
        Some(pool.clone()),
        &client,
        &root,
        100,
        Duration::ZERO,
    )
    .await
    .expect("test: cold admission");
    let slot = admission.slot().expect("test: physical slot");
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let task_entered = entered.clone();
    let task_release = release.clone();
    let mut waiter = Box::pin(
        admission.retain_paging(&root, move |mut admission, _| async move {
            admission.paging_in_flight = true;
            task_entered.notify_one();
            task_release.notified().await;
            // Keep the actual production admission in the unwinding task.
            assert!(admission.slot().is_some());
            assert!(admission.paging_in_flight);
            panic!("test: paging owner fault after dispatch");
        }),
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        tokio::select! {
            _ = &mut waiter => panic!("test: owner ended before injected fault"),
            _ = entered.notified() => {}
        }
    })
    .await
    .expect("test: owner entered");
    let mut transition = Box::pin(endpoint.transition());
    assert!(futures::poll!(&mut transition).is_pending());
    assert_eq!(sem.available_permits(), 0);
    release.notify_one();
    let result = tokio::time::timeout(Duration::from_secs(5), waiter)
        .await
        .expect("test: panic closes waiter");
    assert!(
        matches!(result, Err(ref error) if error == "endpoint paging owner ended without a result")
    );
    let drained = tokio::time::timeout(Duration::from_secs(5), transition)
        .await
        .expect("test: panic drains reader rather than retaining an ownership cycle");
    assert!(endpoint.paging_recovery_required());
    assert_eq!(sem.available_permits(), 1);
    let permit = tokio::time::timeout(Duration::from_secs(5), pool.acquire_slot(slot))
        .await
        .expect("test: physical guard released")
        .expect("test: physical permit");
    assert!(!pool.plan_paging(slot, key).already_resident);
    drop(permit);
    drop(drained);
}
