#![cfg(test)]

use super::*;
use crate::ai::adapter::DispatchTermsPolicy;
use crate::inference::{
    resource_admission::{with_serving_lane, LaneAdmission, LanePriority},
    slots::{directory, PageKey, SlotClass},
    turn_admission::admit_turn,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

// Exercise the real adapter dispatch future, so a wrapper or spawned task that
// loses caller custody cannot pass merely because direct admit_turn tests pass.
#[tokio::test]
async fn adapter_parks_without_caller_lane_and_reacquires_before_http(
) -> Result<(), Box<dyn std::error::Error>> {
    use axum::{
        http::StatusCode,
        routing::{get, post},
        Router,
    };
    let generations = Arc::new(AtomicUsize::new(0));
    let counted = generations.clone();
    let app = Router::new()
    .route("/props", get(|| async {
        axum::Json(serde_json::json!({"total_slots": 1, "default_generation_settings": {"n_ctx": 4096}}))
    }))
    .route("/v1/chat/completions", post(move || {
        let counted = counted.clone();
        async move {
            counted.fetch_add(1, Ordering::SeqCst);
            (StatusCode::BAD_REQUEST, "fixture terminal refusal")
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let mut adapter = test_adapter();
    adapter.config.base_url = format!("http://{address}");
    adapter.config.llamacpp_sampling_extensions = true;
    let endpoints = adapter.endpoints();
    let root = endpoints.root();
    let pool = directory().ensure_pool(root, 1);
    let sem = Arc::new(tokio::sync::Semaphore::new(1));
    let held = admit_turn(
        &sem,
        SlotClass::Turn,
        PageKey::mind(uuid::Uuid::new_v4()),
        Some(pool),
        &adapter.client,
        root,
        100,
        Duration::ZERO,
    )
    .await?;
    let lanes: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
    lanes.set_served_lane_count(1);
    let caller = lanes
        .acquire_serving_lane(None, LanePriority::Directed, None)
        .await;
    let request = TextGenerationRequest {
        max_tokens: Some(32),
        messages: vec![ChatMessage::text("user", "act")],
        purpose: Some("turn".into()),
        persona_id: Some(uuid::Uuid::new_v4().to_string()),
        ..Default::default()
    };
    let terms = crate::ai::types::DispatchTerms {
        output: crate::ai::reply_room::OutputIntent::Fixed(std::num::NonZeroU32::new(32).expect("fixture limit")),
        admission: None, think: None,
    };
    let (sink, _receiver) = tokio::sync::mpsc::unbounded_channel();
    let mut call = Box::pin(with_serving_lane(
        caller,
        adapter.generate_admitted(request, terms, sink),
    ));
    assert!(futures::poll!(&mut call).is_pending());
    let borrower = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::select! {
            result = &mut call => Err(format!("dispatch ended before physical release: {result:?}")),
            lane = lanes.acquire_serving_lane(None, LanePriority::Directed, None) => Ok(lane),
        }
    }).await??;
    assert_eq!(generations.load(Ordering::SeqCst), 0);
    drop(held);
    assert!(futures::poll!(&mut call).is_pending());
    assert_eq!(
        generations.load(Ordering::SeqCst),
        0,
        "free KV is not lane authority"
    );
    drop(borrower);
    let result = tokio::time::timeout(Duration::from_secs(5), call).await?;
    assert!(result.is_err(), "fixture rejects the actual HTTP dispatch");
    assert_eq!(generations.load(Ordering::SeqCst), 1);
    drop(
        tokio::time::timeout(
            Duration::from_secs(2),
            lanes.acquire_serving_lane(None, LanePriority::Directed, None),
        )
        .await?,
    );
    server.abort();
    Ok(())
}
