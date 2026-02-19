//! Genome handler - Multi-adapter stacking
//!
//! Applies multiple LoRA adapters with different scales to create
//! a combined "genome" of capabilities.

use log::info;
use std::time::Instant;
use tonic::{Request, Response, Status};

use crate::inference::{ApplyGenomeRequest, ApplyGenomeResponse};
use crate::model::{rebuild_with_stacked_lora, GenomeAdapter};

use super::service::InferenceService;

/// Apply a genome (stack multiple LoRA adapters)
pub async fn handle_apply_genome(
    request: Request<ApplyGenomeRequest>,
    service: &InferenceService,
) -> Result<Response<ApplyGenomeResponse>, Status> {
    let req = request.into_inner();
    let adapter_entries = req.adapters;

    info!("🧬 ApplyGenome: {} adapters", adapter_entries.len());
    let start = Instant::now();

    // Auto-switch to BF16 if needed
    if let Err(e) = service.ensure_bf16_mode().await {
        return Ok(Response::new(ApplyGenomeResponse {
            success: false,
            error: e,
            apply_time_ms: 0,
            adapters_applied: 0,
            layers_merged: 0,
        }));
    }

    if adapter_entries.is_empty() {
        return Ok(Response::new(ApplyGenomeResponse {
            success: false,
            error: "No adapters specified".to_string(),
            apply_time_ms: 0,
            adapters_applied: 0,
            layers_merged: 0,
        }));
    }

    // Get model state info
    let state = service.state.read().await;
    let (weight_paths, device, dtype, config) = match state.as_ref() {
        Some(model_state) => (
            model_state.weight_paths.clone(),
            model_state.device.clone(),
            model_state.dtype,
            model_state.config.clone(),
        ),
        None => {
            return Ok(Response::new(ApplyGenomeResponse {
                success: false,
                error: "No model loaded".to_string(),
                apply_time_ms: 0,
                adapters_applied: 0,
                layers_merged: 0,
            }));
        }
    };
    drop(state);

    // Collect adapters with weights
    let adapters = service.adapters.read().await;
    let mut genome_adapters: Vec<GenomeAdapter> = Vec::new();
    let mut total_layers = 0;

    for entry in &adapter_entries {
        if let Some(loaded) = adapters.iter().find(|a| a.adapter_id == entry.adapter_id) {
            if let Some(weights) = &loaded.weights {
                total_layers += weights.len();
                genome_adapters.push(GenomeAdapter {
                    adapter_id: entry.adapter_id.clone(),
                    weights: weights.clone(),
                    scale: entry.scale,
                });
            } else {
                return Ok(Response::new(ApplyGenomeResponse {
                    success: false,
                    error: format!("Adapter '{}' has no weights loaded", entry.adapter_id),
                    apply_time_ms: 0,
                    adapters_applied: 0,
                    layers_merged: 0,
                }));
            }
        } else {
            return Ok(Response::new(ApplyGenomeResponse {
                success: false,
                error: format!("Adapter '{}' not found. Load it first.", entry.adapter_id),
                apply_time_ms: 0,
                adapters_applied: 0,
                layers_merged: 0,
            }));
        }
    }
    drop(adapters);

    // Rebuild model with stacked adapters
    let rebuild_result = tokio::task::spawn_blocking(move || {
        rebuild_with_stacked_lora(&weight_paths, &device, dtype, &config, &genome_adapters)
    })
    .await;

    match rebuild_result {
        Ok(Ok(new_model)) => {
            let apply_time_ms = start.elapsed().as_millis() as i64;

            let mut state = service.state.write().await;
            if let Some(model_state) = state.as_mut() {
                model_state.model = new_model;
                model_state.clear_cache();
            }

            info!(
                "✅ Genome applied: {} adapters, {} layers in {}ms",
                adapter_entries.len(),
                total_layers,
                apply_time_ms
            );

            Ok(Response::new(ApplyGenomeResponse {
                success: true,
                error: String::new(),
                apply_time_ms,
                adapters_applied: adapter_entries.len() as i32,
                layers_merged: total_layers as i32,
            }))
        }
        Ok(Err(e)) => {
            info!("❌ Failed to apply genome: {e}");
            Ok(Response::new(ApplyGenomeResponse {
                success: false,
                error: e.to_string(),
                apply_time_ms: 0,
                adapters_applied: 0,
                layers_merged: 0,
            }))
        }
        Err(e) => {
            info!("❌ Genome task failed: {e}");
            Ok(Response::new(ApplyGenomeResponse {
                success: false,
                error: format!("Task join error: {e}"),
                apply_time_ms: 0,
                adapters_applied: 0,
                layers_merged: 0,
            }))
        }
    }
}
