//! Batched Inference for Concurrent Throughput
//!
//! Collects multiple inference requests and processes them in a single
//! batched forward pass through the GPU. This gives near-linear speedup
//! since GPU processes batches in parallel.
//!
//! Architecture:
//! - Requests accumulate for BATCH_WINDOW_MS or until BATCH_SIZE_MAX
//! - Prompts are tokenized and padded to same length
//! - Single batched forward pass processes all prompts
//! - Results are distributed back to original callers

use candle_core::{Device, Tensor};
use candle_transformers::models::quantized_llama::ModelWeights;
use log::{debug, info};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokenizers::Tokenizer;
use tokio::sync::{mpsc, oneshot, Mutex};

/// Maximum batch size (limited by GPU memory)
const BATCH_SIZE_MAX: usize = 4;

/// Maximum time to wait for batch to fill (ms)
const BATCH_WINDOW_MS: u64 = 50;

/// Request in the batch queue
pub struct BatchRequest {
    pub prompt: String,
    pub max_tokens: usize,
    pub temperature: f64,
    pub response_tx: oneshot::Sender<BatchResponse>,
}

/// Response from batched inference
pub struct BatchResponse {
    pub text: String,
    pub tokens: usize,
    pub duration_ms: u64,
    pub error: Option<String>,
}

/// Batched inference state
pub struct BatchInference {
    tokenizer: Arc<Tokenizer>,
    device: Device,
    eos_token_ids: Vec<u32>,
}

impl BatchInference {
    pub fn new(tokenizer: Arc<Tokenizer>, device: Device, eos_token_ids: Vec<u32>) -> Self {
        Self {
            tokenizer,
            device,
            eos_token_ids,
        }
    }

    /// Process a batch of requests in a single forward pass
    pub fn generate_batch(
        &self,
        model: &mut ModelWeights,
        requests: Vec<(String, usize, f64)>, // (prompt, max_tokens, temperature)
    ) -> Vec<Result<(String, usize), String>> {
        let batch_size = requests.len();
        if batch_size == 0 {
            return vec![];
        }

        info!("🔄 Batched inference: {} requests", batch_size);
        let start = Instant::now();

        // Step 1: Tokenize all prompts
        let mut all_tokens: Vec<Vec<u32>> = Vec::with_capacity(batch_size);
        let mut max_len = 0;

        for (prompt, _, _) in &requests {
            match self.tokenizer.encode(prompt.as_str(), true) {
                Ok(encoding) => {
                    let tokens: Vec<u32> = encoding.get_ids().to_vec();
                    max_len = max_len.max(tokens.len());
                    all_tokens.push(tokens);
                }
                Err(e) => {
                    // Handle tokenization error - fill with error result
                    all_tokens.push(vec![]);
                    debug!("Tokenization failed: {}", e);
                }
            }
        }

        // Step 2: Pad all sequences to max_len (left padding with pad token)
        let pad_token = 0u32; // Most models use 0 as pad
        let mut padded_tokens: Vec<Vec<u32>> = Vec::with_capacity(batch_size);
        let mut prompt_lens: Vec<usize> = Vec::with_capacity(batch_size);

        for tokens in &all_tokens {
            let len = tokens.len();
            prompt_lens.push(len);
            if len < max_len {
                let padding = vec![pad_token; max_len - len];
                let mut padded = padding;
                padded.extend(tokens);
                padded_tokens.push(padded);
            } else {
                padded_tokens.push(tokens.clone());
            }
        }

        // Step 3: Create batched input tensor [batch_size, seq_len]
        let flat_tokens: Vec<u32> = padded_tokens.iter().flatten().copied().collect();
        let input = match Tensor::new(&flat_tokens[..], &self.device) {
            Ok(t) => match t.reshape((batch_size, max_len)) {
                Ok(reshaped) => reshaped,
                Err(e) => {
                    return requests
                        .iter()
                        .map(|_| Err(format!("Reshape failed: {}", e)))
                        .collect();
                }
            },
            Err(e) => {
                return requests
                    .iter()
                    .map(|_| Err(format!("Tensor creation failed: {}", e)))
                    .collect();
            }
        };

        // Step 4: Forward pass (batched!)
        let logits = match model.forward(&input, 0) {
            Ok(l) => l,
            Err(e) => {
                return requests
                    .iter()
                    .map(|_| Err(format!("Forward pass failed: {}", e)))
                    .collect();
            }
        };

        // Step 5: Sample next token for each sequence in batch
        // For now, just do greedy sampling from last position
        let results: Vec<Result<(String, usize), String>> = (0..batch_size)
            .map(|i| {
                // Get logits for this batch item, last position
                let seq_logits = match logits.get(i) {
                    Ok(l) => l,
                    Err(e) => return Err(format!("Get batch item failed: {}", e)),
                };

                // Get last token logits
                let last_logits = match seq_logits.get(max_len - 1) {
                    Ok(l) => l,
                    Err(e) => return Err(format!("Get last position failed: {}", e)),
                };

                // Simple greedy decode for now (argmax)
                let token_id = match last_logits.argmax(0) {
                    Ok(t) => match t.to_scalar::<u32>() {
                        Ok(id) => id,
                        Err(e) => return Err(format!("Token to scalar failed: {}", e)),
                    },
                    Err(e) => return Err(format!("Argmax failed: {}", e)),
                };

                // Decode single token
                let text = match self.tokenizer.decode(&[token_id], true) {
                    Ok(s) => s,
                    Err(e) => return Err(format!("Decode failed: {}", e)),
                };

                Ok((text, 1)) // For now, just 1 token
            })
            .collect();

        let duration = start.elapsed();
        info!(
            "✅ Batched inference: {} requests in {:?}",
            batch_size, duration
        );

        results
    }
}

/// Batch collector that accumulates requests and triggers batch processing
pub struct BatchCollector {
    request_tx: mpsc::Sender<BatchRequest>,
}

impl BatchCollector {
    /// Create a new batch collector with a processing loop
    pub fn new(
        tokenizer: Arc<Tokenizer>,
        model: Arc<Mutex<ModelWeights>>,
        device: Device,
        eos_token_ids: Vec<u32>,
    ) -> Self {
        let (request_tx, mut request_rx) = mpsc::channel::<BatchRequest>(32);

        // Spawn batch processing loop
        tokio::spawn(async move {
            let batch_inference = BatchInference::new(tokenizer.clone(), device, eos_token_ids);
            let mut pending: Vec<BatchRequest> = Vec::with_capacity(BATCH_SIZE_MAX);
            let mut batch_start: Option<Instant> = None;

            loop {
                // Wait for request or timeout
                let timeout = if batch_start.is_some() {
                    Duration::from_millis(BATCH_WINDOW_MS)
                } else {
                    Duration::from_secs(3600) // Long timeout when no pending
                };

                match tokio::time::timeout(timeout, request_rx.recv()).await {
                    Ok(Some(request)) => {
                        if batch_start.is_none() {
                            batch_start = Some(Instant::now());
                        }
                        pending.push(request);

                        // Process if batch is full
                        if pending.len() >= BATCH_SIZE_MAX {
                            Self::process_batch(&batch_inference, &model, &mut pending).await;
                            batch_start = None;
                        }
                    }
                    Ok(None) => {
                        // Channel closed
                        break;
                    }
                    Err(_) => {
                        // Timeout - process whatever we have
                        if !pending.is_empty() {
                            Self::process_batch(&batch_inference, &model, &mut pending).await;
                            batch_start = None;
                        }
                    }
                }
            }
        });

        Self { request_tx }
    }

    async fn process_batch(
        batch_inference: &BatchInference,
        model: &Arc<Mutex<ModelWeights>>,
        pending: &mut Vec<BatchRequest>,
    ) {
        let requests: Vec<(String, usize, f64)> = pending
            .iter()
            .map(|r| (r.prompt.clone(), r.max_tokens, r.temperature))
            .collect();

        let mut model_guard = model.lock().await;
        let results = batch_inference.generate_batch(&mut model_guard, requests);
        drop(model_guard);

        // Send results back
        for (request, result) in pending.drain(..).zip(results) {
            let response = match result {
                Ok((text, tokens)) => BatchResponse {
                    text,
                    tokens,
                    duration_ms: 0, // TODO: track per-request
                    error: None,
                },
                Err(e) => BatchResponse {
                    text: String::new(),
                    tokens: 0,
                    duration_ms: 0,
                    error: Some(e),
                },
            };
            let _ = request.response_tx.send(response);
        }
    }

    /// Submit a request for batched processing
    pub async fn submit(
        &self,
        prompt: String,
        max_tokens: usize,
        temperature: f64,
    ) -> Result<oneshot::Receiver<BatchResponse>, String> {
        let (response_tx, response_rx) = oneshot::channel();
        let request = BatchRequest {
            prompt,
            max_tokens,
            temperature,
            response_tx,
        };

        self.request_tx
            .send(request)
            .await
            .map_err(|e| format!("Failed to submit: {}", e))?;

        Ok(response_rx)
    }
}
