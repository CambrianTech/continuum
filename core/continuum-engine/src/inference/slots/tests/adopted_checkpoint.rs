use super::*;
use axum::{extract::Path, http::StatusCode, routing::post, Json, Router};
use std::sync::atomic::{AtomicUsize, Ordering};

// Regression: core adoption has no Child/generation, but its physical residents
// must still be saved. A failed save must not authorize retirement or fake warmth.
#[tokio::test]
async fn adopted_checkpoint_uses_its_ledger_and_classifies_save_completion() -> Result<(), Box<dyn std::error::Error>> {
    for behavior in 0..4 {
        let calls = Arc::new(AtomicUsize::new(0));
        let save_started = Arc::new(tokio::sync::Notify::new());
        let app = Router::new().route("/props", axum::routing::get(|| async {
            Json(serde_json::json!({"total_slots": 3}))
        })).route("/slots/{slot}", post({
            let save_started = save_started.clone();
            let calls = calls.clone();
            move |Path(slot): Path<u32>, Json(body): Json<serde_json::Value>| {
                let save_started = save_started.clone();
                let calls = calls.clone();
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    if behavior == 3 {
                        save_started.notify_one();
                        std::future::pending::<()>().await;
                    }
                    match behavior {
                        1 => (StatusCode::BAD_REQUEST, Json(serde_json::json!({
                            "error": {"code": 400, "type": "invalid_request_error", "message": "save refused"}
                        }))),
                        2 => (StatusCode::OK, Json(serde_json::json!({"unverified": true}))),
                        _ => (StatusCode::OK, Json(serde_json::json!({
                            "id_slot": slot, "filename": body["filename"], "n_saved": 10
                        }))),
                    }
                }
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let root = format!("http://{address}");
        let server = tokio::spawn(async move { axum::serve(listener, app).await });
        let pool = directory().ensure_pool(&root, 3);
        let endpoint = directory().endpoint(&root);
        let process = crate::inference::engine_residency::EngineIncarnation::of(
            std::process::id(), address.port(),
        ).ok_or("fixture process identity unavailable")?;
        let pages = tempfile::tempdir()?;
        assert_eq!(
            adopt_engine_ledger(&pool, pages.path(), process.pid, process.started_s),
            Err(AdoptRefusal::NoLedger),
            "fresh adoption installs an empty stamped ledger, with no prior pages",
        );
        let key = PageKey::mind(Uuid::new_v4()).ok_or("fixture mind key unavailable")?;
        pool.plan_paging(0, key);
        let mut transition = endpoint.transition().await;
        let client = reqwest::Client::new();
        let mut wrong_process = process;
        wrong_process.started_s += 1;
        assert!(transition.checkpoint_adopted_before_replacement(
            &client, &root, wrong_process, &|| true,
        ).await.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0, "wrong ledger identity sends no save");
        let result = if behavior == 3 {
            {
                let save = transition.checkpoint_adopted_before_replacement(
                    &client, &root, process, &|| true,
                );
                tokio::pin!(save);
                tokio::select! {
                    result = &mut save => return Err(format!("pending save unexpectedly completed: {result:?}").into()),
                    () = save_started.notified() => {},
                    () = tokio::time::sleep(std::time::Duration::from_secs(5)) => return Err("save request never reached fixture".into()),
                }
                // Drop the pending HTTP save while its physical completion is unknown.
            }
            Err(String::from("cancelled by fixture"))
        } else { transition.checkpoint_adopted_before_replacement(
            &client, &root, process, &|| true,
        ).await };
        assert_eq!(result.is_ok(), behavior == 0);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(endpoint.is_ready(), behavior == 1, "only terminal refusal resumes");
        assert_eq!(endpoint.paging_recovery_required(), behavior >= 2);
        assert!(transition.previous_generation().is_none(), "adoption invents no Child ownership");
        pool.take_resident(0);
        assert_eq!(pool.plan_paging(0, key).restore, behavior == 0);
        drop(transition);
        server.abort();
    }
    Ok(())
}
