//! LoRA Adapter Loading and Weight Merging
//!
//! Handles loading LoRA adapters from safetensor files and merging
//! their weights with base model weights.
//!
//! LoRA formula: W' = W + scale * (B @ A)
//! where A is [rank, in_features] and B is [out_features, rank]

use candle_core::{DType, Device, Tensor};
use tracing::{debug, info};
use std::collections::HashMap;

/// LoRA weight pair (A and B matrices)
#[derive(Clone)]
pub struct LoRAWeights {
    pub lora_a: Tensor, // [rank, in_features]
    pub lora_b: Tensor, // [out_features, rank]
    pub scale: f64,
}

/// Loaded LoRA adapter metadata
#[derive(Clone)]
pub struct LoadedAdapter {
    pub adapter_id: String,
    pub path: String,
    pub scale: f64,
    pub weights: Option<HashMap<String, LoRAWeights>>,
    pub active: bool,
}

impl LoadedAdapter {
    pub fn new(adapter_id: String, path: String, scale: f64) -> Self {
        Self {
            adapter_id,
            path,
            scale,
            weights: None,
            active: false,
        }
    }
}

/// Load LoRA adapter from safetensor file
pub fn load_lora_adapter(
    adapter_path: &str,
    device: &Device,
    dtype: DType,
    scale: f64,
) -> Result<HashMap<String, LoRAWeights>, Box<dyn std::error::Error + Send + Sync>> {
    use safetensors::SafeTensors;

    info!("Loading LoRA adapter from: {adapter_path}");

    // Resolve path: if directory, find adapter_model.safetensors inside
    let resolved_path = if std::path::Path::new(adapter_path).is_dir() {
        let safetensors = std::path::Path::new(adapter_path).join("adapter_model.safetensors");
        if safetensors.exists() {
            info!("Resolved directory to: {}", safetensors.display());
            safetensors.to_string_lossy().to_string()
        } else {
            return Err(format!("No adapter_model.safetensors found in directory: {adapter_path}").into());
        }
    } else {
        adapter_path.to_string()
    };

    // Read the safetensor file
    let data = std::fs::read(&resolved_path)?;
    let tensors = SafeTensors::deserialize(&data)?;

    let mut lora_pairs: HashMap<String, LoRAWeights> = HashMap::new();
    let mut pending_a: HashMap<String, Tensor> = HashMap::new();
    let mut pending_b: HashMap<String, Tensor> = HashMap::new();

    // Parse all tensors and pair up lora_A and lora_B
    for (name, tensor_view) in tensors.tensors() {
        // Extract base layer name (remove .lora_A.weight or .lora_B.weight suffix)
        let (base_name, is_a) = if name.ends_with(".lora_A.weight") {
            (name.trim_end_matches(".lora_A.weight").to_string(), true)
        } else if name.ends_with(".lora_B.weight") {
            (name.trim_end_matches(".lora_B.weight").to_string(), false)
        } else {
            // Not a LoRA weight, skip
            debug!("  Skipping non-LoRA tensor: {name}");
            continue;
        };

        // Convert safetensors view to Candle tensor
        let shape: Vec<usize> = tensor_view.shape().to_vec();
        let st_dtype = tensor_view.dtype();

        // Get raw data and convert to target dtype
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
            _ => {
                return Err(format!("Unsupported LoRA tensor dtype: {st_dtype:?}").into());
            }
        };

        // Convert to target dtype if needed
        let tensor = if tensor.dtype() != dtype {
            tensor.to_dtype(dtype)?
        } else {
            tensor
        };

        debug!("  LoRA tensor: {name} shape={shape:?}");

        if is_a {
            pending_a.insert(base_name, tensor);
        } else {
            pending_b.insert(base_name, tensor);
        }
    }

    // Pair up A and B matrices
    for (base_name, lora_a) in pending_a {
        if let Some(lora_b) = pending_b.remove(&base_name) {
            info!(
                "  Paired: {} (A: {:?}, B: {:?})",
                base_name,
                lora_a.dims(),
                lora_b.dims()
            );
            lora_pairs.insert(
                base_name,
                LoRAWeights {
                    lora_a,
                    lora_b,
                    scale,
                },
            );
        } else {
            info!("  No B matrix for {base_name}");
        }
    }

    // Check for orphaned B matrices
    for base_name in pending_b.keys() {
        info!("  No A matrix for {base_name}");
    }

    info!("Loaded {} LoRA weight pairs", lora_pairs.len());
    Ok(lora_pairs)
}

/// Compute merged weight: W' = W + scale * (B @ A)
pub fn merge_lora_weight(
    base_weight: &Tensor,
    lora: &LoRAWeights,
) -> Result<Tensor, candle_core::Error> {
    // LoRA formula: W' = W + scale * (B @ A)
    // A is [rank, in_features], B is [out_features, rank]
    // B @ A gives [out_features, in_features]
    let delta = lora.lora_b.matmul(&lora.lora_a)?;
    let scaled_delta = (delta * lora.scale)?;
    base_weight.add(&scaled_delta)
}

/// Map LoRA layer names to Llama model weight names
///
/// LoRA adapters typically use names like:
///   base_model.model.model.layers.0.self_attn.q_proj
///
/// Candle Llama uses:
///   model.layers.0.self_attn.q_proj.weight
pub fn map_lora_name_to_model_name(lora_name: &str) -> String {
    // Strip "base_model." prefix (only once, not repeatedly)
    let cleaned = lora_name.strip_prefix("base_model.").unwrap_or(lora_name);

    // Strip ONE extra "model." if there are two (e.g., "model.model.layers" -> "model.layers")
    let cleaned = cleaned
        .strip_prefix("model.model.")
        .map(|s| format!("model.{s}"))
        .unwrap_or_else(|| cleaned.to_string());

    // Add .weight suffix if not present
    if cleaned.ends_with(".weight") {
        cleaned.to_string()
    } else {
        format!("{cleaned}.weight")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lora_name_mapping() {
        // PEFT/HuggingFace format: base_model.model.model.layers.X.Y
        assert_eq!(
            map_lora_name_to_model_name("base_model.model.model.layers.0.self_attn.q_proj"),
            "model.layers.0.self_attn.q_proj.weight"
        );
        // Single model prefix
        assert_eq!(
            map_lora_name_to_model_name("model.layers.5.mlp.gate_proj"),
            "model.layers.5.mlp.gate_proj.weight"
        );
        // Already correct format
        assert_eq!(
            map_lora_name_to_model_name("model.layers.5.mlp.gate_proj.weight"),
            "model.layers.5.mlp.gate_proj.weight"
        );
    }
}
