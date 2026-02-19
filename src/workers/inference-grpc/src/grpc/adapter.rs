//! LoRA adapter management handlers
//!
//! Handles loading, unloading, listing, and downloading LoRA adapters.

use log::info;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};

use crate::adapter_registry;
use crate::inference::{
    AdapterInfo, AdapterMetadata, DownloadAdapterRequest, DownloadAdapterResponse,
    ListAdaptersRequest, ListAdaptersResponse, LoadAdapterRequest, LoadAdapterResponse,
    UnloadAdapterRequest, UnloadAdapterResponse,
};
use crate::lora::{self, LoadedAdapter};
use crate::model::rebuild_with_lora_from_paths;

use super::service::InferenceService;

/// Load a LoRA adapter from local path
pub async fn handle_load_adapter(
    request: Request<LoadAdapterRequest>,
    service: &InferenceService,
) -> Result<Response<LoadAdapterResponse>, Status> {
    let req = request.into_inner();
    let adapter_path = req.adapter_path.clone();
    let adapter_id = req.adapter_id.clone();
    let scale = if req.scale > 0.0 { req.scale } else { 1.0 };
    let merge = req.merge;

    info!("📦 LoadAdapter: {adapter_id} from {adapter_path} (scale={scale}, merge={merge})");
    let start = Instant::now();

    // Auto-switch to BF16 if needed
    if let Err(e) = service.ensure_bf16_mode().await {
        return Ok(Response::new(LoadAdapterResponse {
            success: false,
            error: e,
            load_time_ms: 0,
        }));
    }

    // Get device and dtype from current model
    let state = service.state.read().await;
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
    })
    .await;

    match result {
        Ok(Ok(weights)) => {
            let num_layers = weights.len();

            // Store adapter metadata
            let mut adapter = LoadedAdapter::new(adapter_id.clone(), adapter_path.clone(), scale);
            adapter.weights = Some(weights.clone());
            adapter.active = true;

            let mut adapters = service.adapters.write().await;
            adapters.push(adapter);
            drop(adapters);

            // If merge requested, rebuild model with LoRA weights
            if merge {
                info!("  Merging LoRA weights into model...");
                let mut state = service.state.write().await;
                if let Some(model_state) = state.as_mut() {
                    let weight_paths = model_state.weight_paths.clone();
                    let device = model_state.device.clone();
                    let dtype = model_state.dtype;
                    let config = model_state.config.clone();
                    let weights_for_merge = weights.clone();

                    let rebuild_result = tokio::task::spawn_blocking(move || {
                        rebuild_with_lora_from_paths(
                            &weight_paths,
                            &device,
                            dtype,
                            &config,
                            &weights_for_merge,
                        )
                    })
                    .await;

                    match rebuild_result {
                        Ok(Ok(new_model)) => {
                            model_state.model = new_model;
                            model_state.clear_cache();
                            info!("  ✓ Model rebuilt with LoRA weights");
                        }
                        Ok(Err(e)) => {
                            info!("  ⚠ Failed to rebuild model: {e}");
                        }
                        Err(e) => {
                            info!("  ⚠ Rebuild task failed: {e}");
                        }
                    }
                }
            }

            let load_time_ms = start.elapsed().as_millis() as i64;
            info!(
                "✅ Adapter loaded: {adapter_id} ({num_layers} layer pairs, {load_time_ms}ms)"
            );

            Ok(Response::new(LoadAdapterResponse {
                success: true,
                error: format!(
                    "Loaded {} LoRA layer pairs{}",
                    num_layers,
                    if merge { " (merged)" } else { "" }
                ),
                load_time_ms,
            }))
        }
        Ok(Err(e)) => {
            info!("❌ Failed to load adapter: {e}");
            Ok(Response::new(LoadAdapterResponse {
                success: false,
                error: e.to_string(),
                load_time_ms: 0,
            }))
        }
        Err(e) => {
            info!("❌ Load task failed: {e}");
            Ok(Response::new(LoadAdapterResponse {
                success: false,
                error: format!("Task join error: {e}"),
                load_time_ms: 0,
            }))
        }
    }
}

/// Unload a LoRA adapter
pub async fn handle_unload_adapter(
    request: Request<UnloadAdapterRequest>,
    adapters: &Arc<RwLock<Vec<LoadedAdapter>>>,
) -> Result<Response<UnloadAdapterResponse>, Status> {
    let adapter_id = request.into_inner().adapter_id;
    info!("📦 UnloadAdapter: {adapter_id}");

    let mut adapters = adapters.write().await;
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
            error: format!("Adapter '{adapter_id}' not found"),
        }))
    }
}

/// List loaded adapters
pub async fn handle_list_adapters(
    _request: Request<ListAdaptersRequest>,
    adapters: &Arc<RwLock<Vec<LoadedAdapter>>>,
) -> Result<Response<ListAdaptersResponse>, Status> {
    let adapters = adapters.read().await;

    let adapter_list: Vec<AdapterInfo> = adapters
        .iter()
        .map(|a| AdapterInfo {
            adapter_id: a.adapter_id.clone(),
            path: a.path.clone(),
            scale: a.scale,
            active: a.active,
        })
        .collect();

    Ok(Response::new(ListAdaptersResponse {
        adapters: adapter_list,
    }))
}

/// Download and load a LoRA adapter from HuggingFace
pub async fn handle_download_adapter(
    request: Request<DownloadAdapterRequest>,
    service: &InferenceService,
) -> Result<Response<DownloadAdapterResponse>, Status> {
    let req = request.into_inner();
    let repo_id = req.repo_id;
    let adapter_id = if req.adapter_id.is_empty() {
        repo_id.clone()
    } else {
        req.adapter_id
    };
    let revision = if req.revision.is_empty() {
        None
    } else {
        Some(req.revision.as_str())
    };
    let scale_override = if req.scale > 0.0 {
        Some(req.scale)
    } else {
        None
    };

    info!("📥 DownloadAdapter from HuggingFace: {repo_id}");
    let start = Instant::now();

    // Auto-switch to BF16 if needed
    if let Err(e) = service.ensure_bf16_mode().await {
        return Ok(Response::new(DownloadAdapterResponse {
            success: false,
            error: e,
            download_time_ms: 0,
            adapter_id: String::new(),
            local_path: String::new(),
            metadata: None,
        }));
    }

    // Check model is loaded
    {
        let state = service.state.read().await;
        if state.is_none() {
            return Ok(Response::new(DownloadAdapterResponse {
                success: false,
                error: "No model loaded - load a model first".to_string(),
                download_time_ms: 0,
                adapter_id: String::new(),
                local_path: String::new(),
                metadata: None,
            }));
        }
    }

    // Download from HuggingFace Hub
    let repo_id_clone = repo_id.clone();
    let revision_owned = revision.map(|s| s.to_string());
    let result = tokio::task::spawn_blocking(move || {
        adapter_registry::download_adapter(&repo_id_clone, revision_owned.as_deref())
    })
    .await;

    match result {
        Ok(Ok(downloaded)) => {
            // Calculate scale: override > config-based > 1.0
            let config_scale =
                downloaded.config.lora_alpha as f64 / downloaded.config.r.max(1) as f64;
            let final_scale = scale_override.unwrap_or(config_scale);
            let weights_path_str = downloaded.weights_path.to_string_lossy().to_string();

            // Now load the weights
            let state = service.state.read().await;
            let (device, dtype) = match state.as_ref() {
                Some(model_state) => (model_state.device.clone(), model_state.dtype),
                None => {
                    return Ok(Response::new(DownloadAdapterResponse {
                        success: false,
                        error: "Downloaded but model unloaded before weight parsing".to_string(),
                        download_time_ms: start.elapsed().as_millis() as i64,
                        adapter_id: String::new(),
                        local_path: weights_path_str,
                        metadata: None,
                    }));
                }
            };
            drop(state);

            // Parse weights in blocking task
            let path_clone = weights_path_str.clone();
            let weights_result = tokio::task::spawn_blocking(move || {
                lora::load_lora_adapter(&path_clone, &device, dtype, final_scale)
            })
            .await;

            match weights_result {
                Ok(Ok(weights)) => {
                    let download_time_ms = start.elapsed().as_millis() as i64;

                    // Create adapter entry with parsed weights
                    let mut adapter = LoadedAdapter::new(
                        adapter_id.clone(),
                        weights_path_str.clone(),
                        final_scale,
                    );
                    adapter.weights = Some(weights);
                    adapter.active = true;

                    let mut adapters = service.adapters.write().await;
                    adapters.push(adapter);

                    let metadata = AdapterMetadata {
                        base_model: downloaded.config.base_model_name_or_path,
                        rank: downloaded.config.r as i32,
                        alpha: downloaded.config.lora_alpha as i32,
                        target_modules: downloaded.config.target_modules,
                        peft_type: downloaded.config.peft_type,
                    };

                    info!(
                        "✅ Downloaded and loaded adapter: {} (r={}, α={}, scale={:.2}) in {}ms",
                        adapter_id, metadata.rank, metadata.alpha, final_scale, download_time_ms
                    );

                    Ok(Response::new(DownloadAdapterResponse {
                        success: true,
                        error: String::new(),
                        download_time_ms,
                        adapter_id,
                        local_path: weights_path_str,
                        metadata: Some(metadata),
                    }))
                }
                Ok(Err(e)) => {
                    info!("❌ Failed to parse adapter weights: {e}");
                    Ok(Response::new(DownloadAdapterResponse {
                        success: false,
                        error: format!("Downloaded but failed to parse weights: {e}"),
                        download_time_ms: start.elapsed().as_millis() as i64,
                        adapter_id: String::new(),
                        local_path: weights_path_str,
                        metadata: None,
                    }))
                }
                Err(e) => {
                    info!("❌ Weight parsing task failed: {e}");
                    Ok(Response::new(DownloadAdapterResponse {
                        success: false,
                        error: format!("Task join error: {e}"),
                        download_time_ms: start.elapsed().as_millis() as i64,
                        adapter_id: String::new(),
                        local_path: weights_path_str,
                        metadata: None,
                    }))
                }
            }
        }
        Ok(Err(e)) => {
            info!("❌ Failed to download adapter: {e}");
            Ok(Response::new(DownloadAdapterResponse {
                success: false,
                error: e.to_string(),
                download_time_ms: 0,
                adapter_id: String::new(),
                local_path: String::new(),
                metadata: None,
            }))
        }
        Err(e) => {
            info!("❌ Download task failed: {e}");
            Ok(Response::new(DownloadAdapterResponse {
                success: false,
                error: format!("Task join error: {e}"),
                download_time_ms: 0,
                adapter_id: String::new(),
                local_path: String::new(),
                metadata: None,
            }))
        }
    }
}
