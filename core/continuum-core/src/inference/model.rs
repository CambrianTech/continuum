//! Inference model utilities — minimal post-#1280 surface.
//!
//! Pre-#1280 this file was 857 LOC of `ContinuumModel` + safetensors
//! loaders + tokenizer resolution + `select_best_device` panic-on-no-GPU.
//! All of that was reachable only from `CandleAdapter` (also deleted in
//! #1280) — production routes local inference through `LlamaCppAdapter`,
//! not through the Candle path.
//!
//! What survives: `rebuild_with_stacked_lora`, the in-memory LoRA-merge
//! helper used by `inference/backends/llama_safetensors.rs::CompactLlamaSafetensorsBackend`
//! (itself test-only — exercised by plasticity validation tests). Phase 2
//! of #1280 will delete that backend + this helper together once
//! plasticity's LoRA training infrastructure is migrated or retired.
//!
//! The no-CPU-fallback contract that used to live as a `panic!` inside
//! `select_best_device` is now enforced by the live llama.cpp path:
//! `LlamaCppConfig::default()` sets `n_gpu_layers: -1` (all layers on
//! GPU); llama.cpp itself loud-fails the model load if no GPU device is
//! available. `tests/no_cpu_fallback_contract.rs` was updated atomically
//! to assert against the LlamaCppConfig invariant rather than the
//! deleted panic site.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::llama::Llama;

use crate::runtime;

use super::backends::GenomeAdapter;
use super::lora::{map_lora_name_to_model_name, merge_lora_weight, LoRAWeights};

/// Rebuild a Llama model from base safetensors weights, with all LoRA
/// adapters in `adapters` stacked and merged into the base weights.
///
/// Used by `CompactLlamaSafetensorsBackend` (plasticity test scaffolding)
/// to materialize a model with a specific genome configuration before
/// running a forward pass.
pub fn rebuild_with_stacked_lora(
    weight_paths: &[PathBuf],
    device: &Device,
    dtype: DType,
    config: &candle_transformers::models::llama::Config,
    adapters: &[GenomeAdapter],
) -> Result<Llama, Box<dyn std::error::Error + Send + Sync>> {
    use safetensors::SafeTensors;

    let total_layers: usize = adapters.iter().map(|a| a.weights.len()).sum();
    runtime::logger("candle").info(&format!(
        "Rebuilding model with {} adapters ({} total LoRA layers)",
        adapters.len(),
        total_layers
    ));
    let start = Instant::now();

    let mut all_tensors: HashMap<String, Tensor> = HashMap::new();

    for path in weight_paths {
        let data = std::fs::read(path)?;
        let tensors = SafeTensors::deserialize(&data)?;

        for (name, tensor_view) in tensors.tensors() {
            let shape: Vec<usize> = tensor_view.shape().to_vec();
            let st_dtype = tensor_view.dtype();

            let tensor = match st_dtype {
                safetensors::Dtype::F32 => {
                    let data: Vec<f32> = tensor_view
                        .data()
                        .chunks(4)
                        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                        .collect();
                    Tensor::from_vec(data, shape.as_slice(), device)?
                }
                safetensors::Dtype::F16 => {
                    let data: Vec<half::f16> = tensor_view
                        .data()
                        .chunks(2)
                        .map(|b| half::f16::from_le_bytes([b[0], b[1]]))
                        .collect();
                    let f32_data: Vec<f32> = data.iter().map(|x| x.to_f32()).collect();
                    Tensor::from_vec(f32_data, shape.as_slice(), device)?
                }
                safetensors::Dtype::BF16 => {
                    let data: Vec<half::bf16> = tensor_view
                        .data()
                        .chunks(2)
                        .map(|b| half::bf16::from_le_bytes([b[0], b[1]]))
                        .collect();
                    let f32_data: Vec<f32> = data.iter().map(|x| x.to_f32()).collect();
                    Tensor::from_vec(f32_data, shape.as_slice(), device)?
                }
                _ => continue,
            };

            let tensor = if tensor.dtype() != dtype {
                tensor.to_dtype(dtype)?
            } else {
                tensor
            };

            all_tensors.insert(name.to_string(), tensor);
        }
    }

    runtime::logger("candle").info(&format!("  Loaded {} base tensors", all_tensors.len()));

    // Apply LoRA deltas from ALL adapters: W' = W + sum(scale_i x B_i @ A_i)
    let mut merged_count = 0;
    let mut failed_count = 0;

    for adapter in adapters {
        runtime::logger("candle").info(&format!(
            "  Applying adapter '{}' (scale={}, {} layers)",
            adapter.adapter_id,
            adapter.scale,
            adapter.weights.len()
        ));

        for (lora_name, lora) in &adapter.weights {
            let model_name = map_lora_name_to_model_name(lora_name);

            if let Some(base_weight) = all_tensors.get(&model_name) {
                let effective_scale = lora.scale * adapter.scale;
                let scaled_lora = LoRAWeights {
                    lora_a: lora.lora_a.clone(),
                    lora_b: lora.lora_b.clone(),
                    scale: effective_scale,
                };

                match merge_lora_weight(base_weight, &scaled_lora) {
                    Ok(merged) => {
                        all_tensors.insert(model_name.clone(), merged);
                        merged_count += 1;
                    }
                    Err(e) => {
                        runtime::logger("candle")
                            .debug(&format!("  Failed to merge {}: {}", lora_name, e));
                        failed_count += 1;
                    }
                }
            } else {
                failed_count += 1;
            }
        }
    }

    if failed_count > 0 {
        runtime::logger("candle").info(&format!("  {} LoRA layers failed to merge", failed_count));
    }

    runtime::logger("candle").info(&format!(
        "  Merged {} LoRA layers from {} adapters",
        merged_count,
        adapters.len()
    ));

    let vb = VarBuilder::from_tensors(all_tensors, dtype, device);
    let model = Llama::load(vb, config)?;

    let duration = start.elapsed();
    runtime::logger("candle").info(&format!("Genome applied in {:?}", duration));

    Ok(model)
}
