//! Worker Pool for Concurrent Inference
//!
//! Manages multiple model instances for parallel inference.
//! Each worker has its own model + KV cache, enabling true concurrency.
//!
//! Architecture:
//! - N workers, each with own QuantizedModelState
//! - Request channel distributes work to available workers
//! - Response channels return results to callers
//! - Semaphore tracks available workers

use log::info;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, oneshot, Semaphore};

use crate::quantized_model::{generate_text_quantized, load_default_quantized};

/// Request sent to worker pool
pub struct InferenceRequest {
    pub prompt: String,
    pub max_tokens: usize,
    pub temperature: f64,
    pub response_tx: oneshot::Sender<InferenceResponse>,
}

/// Response from worker
pub struct InferenceResponse {
    pub text: String,
    pub tokens: usize,
    pub duration_ms: u64,
    pub worker_id: usize,
    pub error: Option<String>,
}

/// Statistics for the worker pool
pub struct PoolStats {
    pub requests_completed: AtomicU64,
    pub requests_pending: AtomicU64,
    pub total_tokens_generated: AtomicU64,
    pub total_inference_ms: AtomicU64,
}

impl PoolStats {
    pub fn new() -> Self {
        Self {
            requests_completed: AtomicU64::new(0),
            requests_pending: AtomicU64::new(0),
            total_tokens_generated: AtomicU64::new(0),
            total_inference_ms: AtomicU64::new(0),
        }
    }
}

/// Worker pool managing multiple model instances
pub struct WorkerPool {
    request_tx: mpsc::Sender<InferenceRequest>,
    stats: Arc<PoolStats>, // Used internally by workers, not exposed
    pub num_workers: usize,
    pub available: Arc<Semaphore>,
}

impl WorkerPool {
    /// Create a new worker pool with N model instances
    ///
    /// Memory usage: ~2GB per worker for quantized model
    /// Recommended: 3-4 workers for 8GB limit
    pub async fn new(num_workers: usize) -> Result<Self, String> {
        info!("🏭 Creating worker pool with {num_workers} workers...");
        let start = Instant::now();

        // Channel for distributing requests (bounded to prevent memory explosion)
        let (request_tx, request_rx) = mpsc::channel::<InferenceRequest>(num_workers * 2);
        let request_rx = Arc::new(tokio::sync::Mutex::new(request_rx));

        let stats = Arc::new(PoolStats::new());
        let available = Arc::new(Semaphore::new(num_workers));

        // Spawn worker tasks
        for worker_id in 0..num_workers {
            let rx = request_rx.clone();
            let stats = stats.clone();
            let available = available.clone();

            tokio::spawn(async move {
                // Each worker loads its own model instance
                info!("  Worker {worker_id}: Loading model...");
                let load_start = Instant::now();

                let mut model_state = match load_default_quantized() {
                    Ok(state) => {
                        info!(
                            "  Worker {}: Model ready in {:.1}s",
                            worker_id,
                            load_start.elapsed().as_secs_f32()
                        );
                        state
                    }
                    Err(e) => {
                        info!("  Worker {worker_id}: Failed to load model: {e}");
                        return;
                    }
                };

                // Worker loop - process requests forever
                loop {
                    // Wait for a request (blocks until one arrives)
                    let request = {
                        let mut rx_guard = rx.lock().await;
                        match rx_guard.recv().await {
                            Some(req) => req,
                            None => {
                                info!("  Worker {worker_id}: Channel closed, shutting down");
                                return;
                            }
                        }
                    };

                    stats.requests_pending.fetch_add(1, Ordering::SeqCst);
                    let gen_start = Instant::now();

                    // Generate response
                    let response = match generate_text_quantized(
                        &mut model_state,
                        &request.prompt,
                        request.max_tokens,
                        request.temperature,
                    ) {
                        Ok((text, tokens)) => {
                            let duration_ms = gen_start.elapsed().as_millis() as u64;
                            stats.total_tokens_generated.fetch_add(tokens as u64, Ordering::SeqCst);
                            stats.total_inference_ms.fetch_add(duration_ms, Ordering::SeqCst);

                            InferenceResponse {
                                text,
                                tokens,
                                duration_ms,
                                worker_id,
                                error: None,
                            }
                        }
                        Err(e) => InferenceResponse {
                            text: String::new(),
                            tokens: 0,
                            duration_ms: gen_start.elapsed().as_millis() as u64,
                            worker_id,
                            error: Some(e),
                        },
                    };

                    stats.requests_pending.fetch_sub(1, Ordering::SeqCst);
                    stats.requests_completed.fetch_add(1, Ordering::SeqCst);

                    // Send response back (ignore error if receiver dropped)
                    let _ = request.response_tx.send(response);

                    // Release semaphore permit
                    available.add_permits(1);
                }
            });
        }

        info!(
            "🏭 Worker pool ready: {} workers in {:.1}s",
            num_workers,
            start.elapsed().as_secs_f32()
        );

        Ok(Self {
            request_tx,
            stats,
            num_workers,
            available,
        })
    }

    /// Submit a request to the pool
    ///
    /// Returns immediately with a response channel.
    /// Caller can await the response or timeout.
    pub async fn submit(
        &self,
        prompt: String,
        max_tokens: usize,
        temperature: f64,
    ) -> Result<oneshot::Receiver<InferenceResponse>, String> {
        // Acquire semaphore permit (blocks if all workers busy)
        // This provides backpressure to prevent queue explosion
        let _permit = self
            .available
            .acquire()
            .await
            .map_err(|e| format!("Semaphore error: {e}"))?;

        // Create response channel
        let (response_tx, response_rx) = oneshot::channel();

        let request = InferenceRequest {
            prompt,
            max_tokens,
            temperature,
            response_tx,
        };

        // Send to worker pool
        self.request_tx
            .send(request)
            .await
            .map_err(|e| format!("Failed to send request: {e}"))?;

        // Note: permit is NOT dropped here - worker will release it after processing
        std::mem::forget(_permit);

        Ok(response_rx)
    }

    /// Get number of available workers
    pub fn available_workers(&self) -> usize {
        self.available.available_permits()
    }

    /// Get pool statistics
    ///
    /// Returns snapshot of current stats:
    /// - requests_completed: Total requests processed
    /// - requests_pending: Currently in-flight requests
    /// - total_tokens_generated: Sum of all generated tokens
    /// - total_inference_ms: Sum of all inference time
    pub fn stats(&self) -> (u64, u64, u64, u64) {
        (
            self.stats.requests_completed.load(Ordering::SeqCst),
            self.stats.requests_pending.load(Ordering::SeqCst),
            self.stats.total_tokens_generated.load(Ordering::SeqCst),
            self.stats.total_inference_ms.load(Ordering::SeqCst),
        )
    }
}
