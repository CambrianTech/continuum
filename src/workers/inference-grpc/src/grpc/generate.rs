//! Generate handler - Text generation endpoint
//!
//! Handles inference requests with support for:
//! - Worker pool (quantized, concurrent)
//! - Single quantized instance (fallback)
//! - BF16 with LoRA adapters

use log::info;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use crate::inference::{generate_response, Complete, GenerateRequest, GenerateResponse};
use crate::model::{generate_text, ModelState};
use crate::priority_queue::Priority;
use crate::quantized_model::{generate_text_quantized, QuantizedModelState};
use crate::worker_pool::WorkerPool;

use super::service::ServerStats;

/// Generate text from a prompt
///
/// Backend selection:
/// 1. Worker pool (concurrent quantized) - best for high throughput
/// 2. Single quantized instance - fallback when pool unavailable
/// 3. BF16 with LoRA - when adapters are loaded
pub async fn handle_generate(
    request: Request<GenerateRequest>,
    worker_pool: &Option<Arc<WorkerPool>>,
    state: &Arc<RwLock<Option<ModelState>>>,
    quantized_state: &Arc<RwLock<Option<QuantizedModelState>>>,
    stats: &Arc<ServerStats>,
    has_adapters: bool,
) -> Result<Response<ReceiverStream<Result<GenerateResponse, Status>>>, Status> {
    let req = request.into_inner();
    let model_id = req.model_id;
    let prompt = req.prompt;
    let max_tokens = req.max_tokens.max(10) as usize;
    let temperature = if req.temperature > 0.0 {
        req.temperature
    } else {
        0.7
    };

    // Per-persona tracking (optional fields)
    let persona_name = if req.persona_name.is_empty() {
        "unknown".to_string()
    } else {
        req.persona_name
    };
    let _persona_id = req.persona_id; // May be empty (for future per-persona logging)

    // Parse priority level (default to Warm for AI personas)
    let priority = Priority::from_str(&req.priority);
    let priority_str = format!("{:?}", priority);

    // Determine which backend to use
    let has_pool = worker_pool.is_some();
    let has_bf16 = state.read().await.is_some();

    let backend = if has_pool && !has_adapters {
        "pool"
    } else if has_bf16 {
        "bf16"
    } else {
        "quantized"
    };

    info!(
        "🔮 Generate [{}]: model={}, prompt={} chars, max_tokens={}, temp={:.2}, backend={}, priority={}",
        persona_name,
        model_id,
        prompt.len(),
        max_tokens,
        temperature,
        backend,
        priority_str
    );

    let (tx, rx) = mpsc::channel(32);
    stats.inc_pending();

    // Use worker pool for concurrent quantized inference
    if let Some(pool) = worker_pool {
        if !has_adapters {
            let pool = pool.clone();
            let stats = stats.clone();
            let available = pool.available_workers();

            info!("🏭 Using worker pool ({available} available workers)");

            tokio::spawn(async move {
                let start = Instant::now();

                // Submit to pool and wait for response
                let result = match pool.submit(prompt.clone(), max_tokens, temperature).await {
                    Ok(rx) => match rx.await {
                        Ok(resp) => {
                            if let Some(err) = resp.error {
                                Err(err)
                            } else {
                                info!(
                                    "✅ Worker {} completed: {} tokens in {}ms",
                                    resp.worker_id, resp.tokens, resp.duration_ms
                                );
                                Ok((resp.text, resp.tokens))
                            }
                        }
                        Err(_) => Err("Worker response channel closed".to_string()),
                    },
                    Err(e) => Err(e),
                };

                let duration = start.elapsed().as_millis() as i32;
                stats.dec_pending();
                stats.inc_completed();

                let response = build_response(result, duration);

                if tx.send(Ok(response)).await.is_err() {
                    info!("⚠️ Failed to send response, client gone");
                }
            });

            return Ok(Response::new(ReceiverStream::new(rx)));
        }
    }

    // Fallback to single-instance mode (quantized or BF16 with LoRA)
    let state_arc = state.clone();
    let quantized_arc = quantized_state.clone();
    let is_quantized = quantized_state.read().await.is_some();
    let stats = stats.clone();

    tokio::spawn(async move {
        let start = Instant::now();

        // Try quantized model first, fall back to full precision
        let result = if is_quantized {
            let mut q_guard = quantized_arc.write().await;
            match q_guard.as_mut() {
                Some(q_state) => generate_text_quantized(q_state, &prompt, max_tokens, temperature),
                None => Err("Quantized model not available".to_string()),
            }
        } else {
            let mut state_guard = state_arc.write().await;
            match state_guard.as_mut() {
                Some(model_state) => generate_text(model_state, &prompt, max_tokens, temperature),
                None => Err("Model not loaded".to_string()),
            }
        };

        let duration = start.elapsed().as_millis() as i32;
        stats.dec_pending();
        stats.inc_completed();

        let response = build_response(result, duration);

        if tx.send(Ok(response)).await.is_err() {
            info!("⚠️ Failed to send response, client gone");
        } else {
            info!("✅ Response sent ({duration}ms)");
        }
    });

    Ok(Response::new(ReceiverStream::new(rx)))
}

/// Build a GenerateResponse from result
fn build_response(result: Result<(String, usize), String>, duration_ms: i32) -> GenerateResponse {
    match result {
        Ok((text, tokens)) => GenerateResponse {
            response: Some(generate_response::Response::Complete(Complete {
                text,
                tokens: tokens as i32,
                duration_ms,
            })),
        },
        Err(e) => GenerateResponse {
            response: Some(generate_response::Response::Complete(Complete {
                text: format!("ERROR: {e}"),
                tokens: 0,
                duration_ms,
            })),
        },
    }
}
