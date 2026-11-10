//! LoRA adapters: projection names and merging an adapter into the quantized weights.

use std::collections::HashMap;

use candle_core::{Result, Tensor};

use super::*;
use super::weights::MlpOrMoe;

/// Projection types for LoRA weight mapping.
#[derive(Debug, Clone, Copy)]
enum Projection {
    Q,
    K,
    V,
    O,
    Gate,
    Up,
    Down,
}

/// Parse a LoRA weight name to extract layer index and projection type.
///
/// Handles PEFT naming conventions:
///   "base_model.model.model.layers.{N}.self_attn.{q|k|v|o}_proj"
///   "base_model.model.model.layers.{N}.mlp.{gate|up|down}_proj"
///   "model.layers.{N}.self_attn.{q|k|v|o}_proj"
fn parse_lora_layer_name(name: &str) -> Option<(usize, Projection)> {
    // Strip common prefixes
    let name = name.strip_prefix("base_model.model.").unwrap_or(name);
    let name = name.strip_prefix("model.").unwrap_or(name);

    // Now expect: "layers.{N}.self_attn.{X}_proj" or "layers.{N}.mlp.{X}_proj"
    let parts: Vec<&str> = name.split('.').collect();

    // Minimum: ["layers", N, "self_attn"|"mlp", "X_proj"]
    if parts.len() < 4 || parts[0] != "layers" {
        return None;
    }

    let layer_idx: usize = parts[1].parse().ok()?;
    let module = parts[2];
    let proj_name = parts[3];

    let projection = match (module, proj_name) {
        ("self_attn", "q_proj") => Projection::Q,
        ("self_attn", "k_proj") => Projection::K,
        ("self_attn", "v_proj") => Projection::V,
        ("self_attn", "o_proj") => Projection::O,
        ("mlp", "gate_proj") => Projection::Gate,
        ("mlp", "up_proj") => Projection::Up,
        ("mlp", "down_proj") => Projection::Down,
        _ => return None,
    };

    Some((layer_idx, projection))
}

impl ModelWeights {
    /// Apply LoRA adapters to quantized weights via mixed-precision merge.
    ///
    /// For each LoRA weight pair (A, B), finds the matching GGUF layer weight,
    /// dequantizes it to FP32, applies W' = W + scale * (B @ A), and stores
    /// the merged weight as a non-quantized QMatMul::Tensor.
    ///
    /// This is the GGUF equivalent of `rebuild_with_stacked_lora` for safetensors.
    /// The key difference: only layers with LoRA adapters are dequantized.
    /// Layers without adapters stay quantized, preserving memory efficiency.
    ///
    /// Returns (merged_count, failed_count).
    pub fn apply_lora_adapters(
        &mut self,
        lora_weights: &HashMap<String, (Tensor, Tensor, f64)>, // layer_name → (lora_a, lora_b, scale)
    ) -> Result<(usize, usize)> {
        let mut merged = 0usize;
        let mut failed = 0usize;

        // Extract device upfront to avoid borrow conflict with self.layers
        let device = self.layers[0].cos.device().clone();

        for (lora_name, (lora_a, lora_b, scale)) in lora_weights {
            // Parse layer index and projection from LoRA name
            // LoRA names: "base_model.model.model.layers.{N}.self_attn.{q|k|v|o}_proj"
            //          or "base_model.model.model.layers.{N}.mlp.{gate|up|down}_proj"
            if let Some((layer_idx, proj)) = parse_lora_layer_name(lora_name) {
                if layer_idx >= self.layers.len() {
                    failed += 1;
                    continue;
                }
                let layer = &mut self.layers[layer_idx];
                let qmatmul = match proj {
                    Projection::Q => &mut layer.attention_wq,
                    Projection::K => &mut layer.attention_wk,
                    Projection::V => &mut layer.attention_wv,
                    Projection::O => &mut layer.attention_wo,
                    Projection::Gate => match &mut layer.mlp_or_moe {
                        MlpOrMoe::Mlp(mlp) => &mut mlp.feed_forward_w1,
                        MlpOrMoe::MoE { .. } => {
                            failed += 1;
                            continue;
                        }
                    },
                    Projection::Up => match &mut layer.mlp_or_moe {
                        MlpOrMoe::Mlp(mlp) => &mut mlp.feed_forward_w3,
                        MlpOrMoe::MoE { .. } => {
                            failed += 1;
                            continue;
                        }
                    },
                    Projection::Down => match &mut layer.mlp_or_moe {
                        MlpOrMoe::Mlp(mlp) => &mut mlp.feed_forward_w2,
                        MlpOrMoe::MoE { .. } => {
                            failed += 1;
                            continue;
                        }
                    },
                };

                match qmatmul.merge_lora(lora_a, lora_b, *scale, &device) {
                    Ok(()) => merged += 1,
                    Err(_e) => failed += 1,
                }
            } else {
                failed += 1;
            }
        }

        Ok((merged, failed))
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_lora_layer_name_peft_format() {
        // Full PEFT format: base_model.model.model.layers.N.module.proj
        let (idx, proj) =
            parse_lora_layer_name("base_model.model.model.layers.5.self_attn.q_proj").unwrap();
        assert_eq!(idx, 5);
        assert!(matches!(proj, Projection::Q));

        let (idx, proj) =
            parse_lora_layer_name("base_model.model.model.layers.0.self_attn.v_proj").unwrap();
        assert_eq!(idx, 0);
        assert!(matches!(proj, Projection::V));

        let (idx, proj) =
            parse_lora_layer_name("base_model.model.model.layers.27.mlp.gate_proj").unwrap();
        assert_eq!(idx, 27);
        assert!(matches!(proj, Projection::Gate));
    }

    #[test]
    fn test_parse_lora_layer_name_short_format() {
        // Short format: model.layers.N.module.proj
        let (idx, proj) = parse_lora_layer_name("model.layers.3.self_attn.o_proj").unwrap();
        assert_eq!(idx, 3);
        assert!(matches!(proj, Projection::O));

        let (idx, proj) = parse_lora_layer_name("model.layers.10.mlp.up_proj").unwrap();
        assert_eq!(idx, 10);
        assert!(matches!(proj, Projection::Up));

        let (idx, proj) = parse_lora_layer_name("model.layers.0.mlp.down_proj").unwrap();
        assert_eq!(idx, 0);
        assert!(matches!(proj, Projection::Down));
    }

    #[test]
    fn test_parse_lora_layer_name_bare_format() {
        // Bare format: layers.N.module.proj (no model. prefix)
        let (idx, proj) = parse_lora_layer_name("layers.7.self_attn.k_proj").unwrap();
        assert_eq!(idx, 7);
        assert!(matches!(proj, Projection::K));
    }

    #[test]
    fn test_parse_lora_layer_name_invalid() {
        assert!(parse_lora_layer_name("some.random.name").is_none());
        assert!(parse_lora_layer_name("").is_none());
        assert!(parse_lora_layer_name("layers.not_a_number.self_attn.q_proj").is_none());
        assert!(parse_lora_layer_name("layers.0.self_attn.unknown_proj").is_none());
    }

    #[test]
    fn test_parse_all_seven_projections() {
        let projections = [
            ("self_attn.q_proj", "Q"),
            ("self_attn.k_proj", "K"),
            ("self_attn.v_proj", "V"),
            ("self_attn.o_proj", "O"),
            ("mlp.gate_proj", "Gate"),
            ("mlp.up_proj", "Up"),
            ("mlp.down_proj", "Down"),
        ];

        for (proj_str, expected) in projections {
            let name = format!("layers.0.{}", proj_str);
            let result = parse_lora_layer_name(&name);
            assert!(result.is_some(), "Failed to parse: {}", name);
            let (idx, proj) = result.unwrap();
            assert_eq!(idx, 0);
            let proj_name = format!("{:?}", proj);
            assert_eq!(proj_name, expected, "Wrong projection for {}", name);
        }
    }
}
