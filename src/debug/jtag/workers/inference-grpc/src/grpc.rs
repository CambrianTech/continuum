/**
 * gRPC Service Implementation
 *
 * Implements the Inference trait for the gRPC server.
 */

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use log::info;

use crate::inference::inference_server::Inference;
use crate::inference::{
    generate_response, Complete, GenerateRequest, GenerateResponse, PingRequest, PingResponse,
    LoadModelRequest, LoadModelResponse, UnloadModelRequest, UnloadModelResponse,
    ListModelsRequest, ListModelsResponse, ModelInfo,
    LoadAdapterRequest, LoadAdapterResponse, UnloadAdapterRequest, UnloadAdapterResponse,
    ListAdaptersRequest, ListAdaptersResponse, AdapterInfo,
    StatusRequest, StatusResponse,
};
use crate::model::{ModelState, load_model_by_id, generate_text, rebuild_with_lora_from_paths};
use crate::lora::{self, LoadedAdapter};

/// Server statistics tracking
pub struct ServerStats {
    pub requests_completed: AtomicU64,
    pub requests_pending: AtomicU64,
}

impl ServerStats {
    pub fn new() -> Self {
        Self {
            requests_completed: AtomicU64::new(0),
            requests_pending: AtomicU64::new(0),
        }
    }
}

/// Main gRPC service struct
pub struct InferenceService {
    pub state: Arc<RwLock<Option<ModelState>>>,
    pub stats: Arc<ServerStats>,
    pub adapters: Arc<RwLock<Vec<LoadedAdapter>>>,
}

impl InferenceService {
    pub fn new(state: Option<ModelState>) -> Self {
        Self {
            state: Arc::new(RwLock::new(state)),
            stats: Arc::new(ServerStats::new()),
            adapters: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

#[tonic::async_trait]
impl Inference for InferenceService {
    async fn ping(&self, _request: Request<PingRequest>) -> Result<Response<PingResponse>, Status> {
        let state = self.state.read().await;
        let model_loaded = state.is_some();

        Ok(Response::new(PingResponse {
            message: if model_loaded { "pong (model loaded)".to_string() } else { "pong (no model)".to_string() },
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64,
        }))
    }

    type GenerateStream = ReceiverStream<Result<GenerateResponse, Status>>;

    async fn generate(
        &self,
        request: Request<GenerateRequest>,
    ) -> Result<Response<Self::GenerateStream>, Status> {
        let req = request.into_inner();
        let model_id = req.model_id;
        let prompt = req.prompt;
        let max_tokens = req.max_tokens.max(10) as usize;
        let temperature = if req.temperature > 0.0 { req.temperature } else { 0.7 };

        info!("🔮 Generate: model={}, prompt={} chars, max_tokens={}, temp={:.2}",
            model_id, prompt.len(), max_tokens, temperature);

        let (tx, rx) = mpsc::channel(32);
        let state_arc = self.state.clone();
        let stats = self.stats.clone();

        stats.requests_pending.fetch_add(1, Ordering::SeqCst);

        tokio::spawn(async move {
            let start = Instant::now();

            // Get exclusive access to model
            let mut state_guard = state_arc.write().await;

            let result = match state_guard.as_mut() {
                Some(model_state) => {
                    generate_text(model_state, &prompt, max_tokens, temperature)
                }
                None => {
                    Err("Model not loaded".to_string())
                }
            };

            drop(state_guard);

            let duration = start.elapsed().as_millis() as i32;

            let response = match result {
                Ok((text, tokens)) => {
                    GenerateResponse {
                        response: Some(generate_response::Response::Complete(Complete {
                            text,
                            tokens: tokens as i32,
                            duration_ms: duration,
                        })),
                    }
                }
                Err(e) => {
                    GenerateResponse {
                        response: Some(generate_response::Response::Complete(Complete {
                            text: format!("ERROR: {}", e),
                            tokens: 0,
                            duration_ms: duration,
                        })),
                    }
                }
            };

            stats.requests_pending.fetch_sub(1, Ordering::SeqCst);
            stats.requests_completed.fetch_add(1, Ordering::SeqCst);

            if tx.send(Ok(response)).await.is_err() {
                info!("⚠️ Failed to send response, client gone");
            } else {
                info!("✅ Response sent ({}ms)", duration);
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    // ========================================================================
    // Model Management
    // ========================================================================

    async fn load_model(
        &self,
        request: Request<LoadModelRequest>,
    ) -> Result<Response<LoadModelResponse>, Status> {
        let req = request.into_inner();
        let model_id = req.model_id;

        info!("📥 LoadModel: {}", model_id);
        let start = Instant::now();

        let result = tokio::task::spawn_blocking(move || {
            load_model_by_id(&model_id)
        }).await;

        match result {
            Ok(Ok(new_state)) => {
                let load_time_ms = start.elapsed().as_millis() as i64;

                let mut state = self.state.write().await;
                *state = Some(new_state);

                info!("✅ Model loaded in {}ms", load_time_ms);
                Ok(Response::new(LoadModelResponse {
                    success: true,
                    error: String::new(),
                    load_time_ms,
                    memory_bytes: 0,
                }))
            }
            Ok(Err(e)) => {
                info!("❌ Failed to load model: {}", e);
                Ok(Response::new(LoadModelResponse {
                    success: false,
                    error: e.to_string(),
                    load_time_ms: 0,
                    memory_bytes: 0,
                }))
            }
            Err(e) => {
                info!("❌ Load task failed: {}", e);
                Ok(Response::new(LoadModelResponse {
                    success: false,
                    error: format!("Task join error: {}", e),
                    load_time_ms: 0,
                    memory_bytes: 0,
                }))
            }
        }
    }

    async fn unload_model(
        &self,
        _request: Request<UnloadModelRequest>,
    ) -> Result<Response<UnloadModelResponse>, Status> {
        info!("📤 UnloadModel");

        let mut state = self.state.write().await;
        if state.is_some() {
            *state = None;
            info!("✅ Model unloaded");
            Ok(Response::new(UnloadModelResponse {
                success: true,
                error: String::new(),
            }))
        } else {
            Ok(Response::new(UnloadModelResponse {
                success: false,
                error: "No model loaded".to_string(),
            }))
        }
    }

    async fn list_models(
        &self,
        _request: Request<ListModelsRequest>,
    ) -> Result<Response<ListModelsResponse>, Status> {
        let state = self.state.read().await;

        let models = if let Some(ref model_state) = *state {
            vec![ModelInfo {
                model_id: model_state.model_id.clone(),
                loaded: true,
                memory_bytes: 0,
                dtype: format!("{:?}", model_state.dtype),
            }]
        } else {
            vec![]
        };

        Ok(Response::new(ListModelsResponse { models }))
    }

    // ========================================================================
    // LoRA Adapter Management
    // ========================================================================

    async fn load_adapter(
        &self,
        request: Request<LoadAdapterRequest>,
    ) -> Result<Response<LoadAdapterResponse>, Status> {
        let req = request.into_inner();
        let adapter_path = req.adapter_path.clone();
        let adapter_id = req.adapter_id.clone();
        let scale = if req.scale > 0.0 { req.scale } else { 1.0 };
        let merge = req.merge; // If true, merge weights into model

        info!("📦 LoadAdapter: {} from {} (scale={}, merge={})", adapter_id, adapter_path, scale, merge);
        let start = Instant::now();

        // Get device and dtype from current model
        let state = self.state.read().await;
        let (device, dtype) = match state.as_ref() {
            Some(model_state) => (model_state.device.clone(), model_state.dtype),
            None => {
                return Ok(Response::new(LoadAdapterResponse {
                    success: false,
                    error: "No model loaded - load a model first".to_string(),
                    load_time_ms: 0,
                }));
            }
        };
        drop(state);

        // Load LoRA weights in blocking task
        let adapter_path_clone = adapter_path.clone();
        let result = tokio::task::spawn_blocking(move || {
            lora::load_lora_adapter(&adapter_path_clone, &device, dtype, scale)
        }).await;

        match result {
            Ok(Ok(weights)) => {
                let num_layers = weights.len();

                // Store adapter metadata
                let mut adapter = LoadedAdapter::new(adapter_id.clone(), adapter_path.clone(), scale);
                adapter.weights = Some(weights.clone());
                adapter.active = true;

                let mut adapters = self.adapters.write().await;
                adapters.push(adapter);
                drop(adapters);

                // If merge requested, rebuild model with LoRA weights
                if merge {
                    info!("  Merging LoRA weights into model...");
                    let mut state = self.state.write().await;
                    if let Some(model_state) = state.as_mut() {
                        // Clone data needed for blocking task
                        let weight_paths = model_state.weight_paths.clone();
                        let device = model_state.device.clone();
                        let dtype = model_state.dtype;
                        let config = model_state.config.clone();
                        let weights_for_merge = weights.clone();

                        let rebuild_result = tokio::task::spawn_blocking(move || {
                            rebuild_with_lora_from_paths(&weight_paths, &device, dtype, &config, &weights_for_merge)
                        }).await;

                        match rebuild_result {
                            Ok(Ok(new_model)) => {
                                model_state.model = new_model;
                                model_state.clear_cache();
                                info!("  ✓ Model rebuilt with LoRA weights");
                            }
                            Ok(Err(e)) => {
                                info!("  ⚠ Failed to rebuild model: {}", e);
                            }
                            Err(e) => {
                                info!("  ⚠ Rebuild task failed: {}", e);
                            }
                        }
                    }
                }

                let load_time_ms = start.elapsed().as_millis() as i64;
                info!("✅ Adapter loaded: {} ({} layer pairs, {}ms)",
                    adapter_id, num_layers, load_time_ms);

                Ok(Response::new(LoadAdapterResponse {
                    success: true,
                    error: format!("Loaded {} LoRA layer pairs{}", num_layers, if merge { " (merged)" } else { "" }),
                    load_time_ms,
                }))
            }
            Ok(Err(e)) => {
                info!("❌ Failed to load adapter: {}", e);
                Ok(Response::new(LoadAdapterResponse {
                    success: false,
                    error: e.to_string(),
                    load_time_ms: 0,
                }))
            }
            Err(e) => {
                info!("❌ Load task failed: {}", e);
                Ok(Response::new(LoadAdapterResponse {
                    success: false,
                    error: format!("Task join error: {}", e),
                    load_time_ms: 0,
                }))
            }
        }
    }

    async fn unload_adapter(
        &self,
        request: Request<UnloadAdapterRequest>,
    ) -> Result<Response<UnloadAdapterResponse>, Status> {
        let adapter_id = request.into_inner().adapter_id;
        info!("📦 UnloadAdapter: {}", adapter_id);

        let mut adapters = self.adapters.write().await;
        let initial_len = adapters.len();
        adapters.retain(|a| a.adapter_id != adapter_id);

        if adapters.len() < initial_len {
            info!("✅ Adapter unloaded");
            Ok(Response::new(UnloadAdapterResponse {
                success: true,
                error: String::new(),
            }))
        } else {
            Ok(Response::new(UnloadAdapterResponse {
                success: false,
                error: format!("Adapter '{}' not found", adapter_id),
            }))
        }
    }

    async fn list_adapters(
        &self,
        _request: Request<ListAdaptersRequest>,
    ) -> Result<Response<ListAdaptersResponse>, Status> {
        let adapters = self.adapters.read().await;

        let adapter_list: Vec<AdapterInfo> = adapters.iter().map(|a| {
            AdapterInfo {
                adapter_id: a.adapter_id.clone(),
                path: a.path.clone(),
                scale: a.scale,
                active: a.active,
            }
        }).collect();

        Ok(Response::new(ListAdaptersResponse { adapters: adapter_list }))
    }

    // ========================================================================
    // Server Status
    // ========================================================================

    async fn status(
        &self,
        _request: Request<StatusRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let state = self.state.read().await;
        let adapters = self.adapters.read().await;

        let current_model = state.as_ref()
            .map(|s| s.model_id.clone())
            .unwrap_or_default();

        let active_adapters: Vec<String> = adapters.iter()
            .filter(|a| a.active)
            .map(|a| a.adapter_id.clone())
            .collect();

        Ok(Response::new(StatusResponse {
            healthy: state.is_some(),
            current_model,
            memory_used_bytes: 0,
            memory_total_bytes: 0,
            requests_pending: self.stats.requests_pending.load(Ordering::SeqCst) as i32,
            requests_completed: self.stats.requests_completed.load(Ordering::SeqCst) as i32,
            active_adapters,
        }))
    }
}
