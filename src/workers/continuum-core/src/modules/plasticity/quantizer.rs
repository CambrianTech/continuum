//! Per-head mixed precision quantization.
//!
//! After compaction removes dead heads, surviving heads get per-head quantization:
//! - Q4 heads: 4-bit quantized (low utilization, heavy compression)
//! - Q8 heads: 8-bit quantized (medium utilization)
//! - BF16 heads: full precision (high utilization)
//!
//! Storage: Option A — separate sub-tensors per precision tier.
//! Each attention projection is split into up to 3 sub-tensors:
//! ```text
//! model.layers.0.self_attn.q_proj.weight.q4    — [n_q4_heads * head_dim, hidden]
//! model.layers.0.self_attn.q_proj.weight.q8    — [n_q8_heads * head_dim, hidden]
//! model.layers.0.self_attn.q_proj.weight.bf16  — [n_bf16_heads * head_dim, hidden]
//! ```
//! The topology maps each head index to its sub-tensor and offset within it.

use super::types::*;

/// Per-head quantization plan for a single layer.
#[derive(Debug, Clone)]
pub struct LayerQuantizationPlan {
    pub layer_index: usize,
    /// Indices into the RETAINED head list (not original indices) grouped by precision
    pub q4_indices: Vec<usize>,
    pub q8_indices: Vec<usize>,
    pub bf16_indices: Vec<usize>,
}

/// Compute quantization plans from a topology.
pub fn compute_quantization_plans(topology: &HeadTopology) -> Vec<LayerQuantizationPlan> {
    topology
        .layers
        .iter()
        .map(|layer| {
            let mut q4_indices = Vec::new();
            let mut q8_indices = Vec::new();
            let mut bf16_indices = Vec::new();

            for (i, precision) in layer.head_precisions.iter().enumerate() {
                match precision {
                    HeadPrecision::Removed => {} // Should not appear in retained list
                    HeadPrecision::Q4 => q4_indices.push(i),
                    HeadPrecision::Q8 => q8_indices.push(i),
                    HeadPrecision::BF16 => bf16_indices.push(i),
                }
            }

            LayerQuantizationPlan {
                layer_index: layer.layer_index,
                q4_indices,
                q8_indices,
                bf16_indices,
            }
        })
        .collect()
}

/// Quantize a BF16 f32 value to 4-bit (absmax block quantization).
///
/// Block quantization: for a block of values, find the absmax scale,
/// then quantize each value to [-7, 7] (signed 4-bit).
pub fn quantize_block_q4(values: &[f32]) -> (Vec<u8>, f32) {
    if values.is_empty() {
        return (vec![], 0.0);
    }

    let absmax = values
        .iter()
        .map(|v| v.abs())
        .fold(0.0_f32, f32::max);

    let scale = if absmax > 0.0 { absmax / 7.0 } else { 1.0 };

    // Pack two 4-bit values per byte
    let mut packed = Vec::with_capacity((values.len() + 1) / 2);
    for chunk in values.chunks(2) {
        let lo = quantize_scalar_q4(chunk[0], scale);
        let hi = if chunk.len() > 1 {
            quantize_scalar_q4(chunk[1], scale)
        } else {
            0
        };
        packed.push((hi << 4) | (lo & 0x0F));
    }

    (packed, scale)
}

/// Quantize a single f32 to signed 4-bit [-7, 7].
fn quantize_scalar_q4(value: f32, scale: f32) -> u8 {
    let quantized = (value / scale).round().clamp(-7.0, 7.0) as i8;
    // Store as unsigned (offset by 8 to fit in 0-15)
    ((quantized + 8) as u8) & 0x0F
}

/// Dequantize a 4-bit packed byte to two f32 values.
pub fn dequantize_block_q4(packed: &[u8], scale: f32, count: usize) -> Vec<f32> {
    let mut result = Vec::with_capacity(count);
    for (i, &byte) in packed.iter().enumerate() {
        let lo = (byte & 0x0F) as i8 - 8;
        result.push(lo as f32 * scale);
        if i * 2 + 1 < count {
            let hi = ((byte >> 4) & 0x0F) as i8 - 8;
            result.push(hi as f32 * scale);
        }
    }
    result.truncate(count);
    result
}

/// Quantize a block of f32 values to 8-bit (absmax block quantization).
pub fn quantize_block_q8(values: &[f32]) -> (Vec<i8>, f32) {
    if values.is_empty() {
        return (vec![], 0.0);
    }

    let absmax = values
        .iter()
        .map(|v| v.abs())
        .fold(0.0_f32, f32::max);

    let scale = if absmax > 0.0 { absmax / 127.0 } else { 1.0 };

    let quantized: Vec<i8> = values
        .iter()
        .map(|&v| (v / scale).round().clamp(-127.0, 127.0) as i8)
        .collect();

    (quantized, scale)
}

/// Dequantize 8-bit values back to f32.
pub fn dequantize_block_q8(quantized: &[i8], scale: f32) -> Vec<f32> {
    quantized.iter().map(|&q| q as f32 * scale).collect()
}

/// Estimate memory savings from mixed-precision quantization.
///
/// Returns (original_bytes, quantized_bytes) for a single layer's attention weights.
/// Both values use BF16 (2 bytes/param) as the baseline for "original".
pub fn estimate_layer_savings(
    layer: &LayerTopology,
    head_dim: usize,
    hidden_size: usize,
) -> (u64, u64) {
    let bf16_bytes_per_param = 2u64;
    let hd = head_dim as u64;
    let hs = hidden_size as u64;

    // Original: all retained heads at BF16 for Q, K, V, O
    // Q: [num_heads * head_dim, hidden] params = num_heads * head_dim * hidden
    // O: [hidden, num_heads * head_dim] params = hidden * num_heads * head_dim
    // K: [num_kv_heads * head_dim, hidden]
    // V: [num_kv_heads * head_dim, hidden]
    let q_params = layer.num_heads as u64 * hd * hs;
    let o_params = hs * layer.num_heads as u64 * hd;
    let k_params = layer.num_kv_heads as u64 * hd * hs;
    let v_params = layer.num_kv_heads as u64 * hd * hs;
    let total_params = q_params + o_params + k_params + v_params;
    let original_bytes = total_params * bf16_bytes_per_param;

    // Quantized: Q and O heads get per-head precision; K and V stay BF16
    let mut quantized_bytes = 0u64;
    for precision in &layer.head_precisions {
        // Each Q head contributes head_dim rows in Q proj + head_dim cols in O proj
        let params_per_head_qo = hd * hs * 2; // Q + O
        let bytes = match precision {
            HeadPrecision::Removed => 0,
            HeadPrecision::Q4 => params_per_head_qo / 2,   // 4 bits = 0.5 bytes
            HeadPrecision::Q8 => params_per_head_qo,         // 8 bits = 1 byte
            HeadPrecision::BF16 => params_per_head_qo * 2,   // 16 bits = 2 bytes
        };
        quantized_bytes += bytes;
    }

    // K and V projections stay at BF16 (KV heads are shared, not per-Q-head quantized)
    quantized_bytes += (k_params + v_params) * bf16_bytes_per_param;

    (original_bytes, quantized_bytes)
}

/// Estimate total memory savings across all layers.
pub fn estimate_total_savings(
    topology: &HeadTopology,
    hidden_size: usize,
) -> (u64, u64) {
    let mut total_original = 0u64;
    let mut total_quantized = 0u64;

    for layer in &topology.layers {
        let (orig, quant) = estimate_layer_savings(layer, topology.head_dim, hidden_size);
        total_original += orig;
        total_quantized += quant;
    }

    (total_original, total_quantized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantize_dequantize_q4_roundtrip() {
        let values = vec![1.0, -0.5, 0.25, -1.0, 0.0, 0.75];
        let (packed, scale) = quantize_block_q4(&values);
        let restored = dequantize_block_q4(&packed, scale, values.len());

        // Q4 has limited precision, so check within tolerance
        for (orig, restored) in values.iter().zip(restored.iter()) {
            let error = (orig - restored).abs();
            assert!(
                error < scale * 1.5,
                "Q4 roundtrip error too large: orig={orig}, restored={restored}, error={error}"
            );
        }
    }

    #[test]
    fn test_quantize_dequantize_q8_roundtrip() {
        let values = vec![1.0, -0.5, 0.25, -1.0, 0.0, 0.75, -0.125, 0.9];
        let (quantized, scale) = quantize_block_q8(&values);
        let restored = dequantize_block_q8(&quantized, scale);

        // Q8 should be much more precise than Q4
        for (orig, restored) in values.iter().zip(restored.iter()) {
            let error = (orig - restored).abs();
            assert!(
                error < scale * 1.5,
                "Q8 roundtrip error too large: orig={orig}, restored={restored}, error={error}"
            );
        }
    }

    #[test]
    fn test_quantize_q4_zeros() {
        let values = vec![0.0, 0.0, 0.0, 0.0];
        let (packed, scale) = quantize_block_q4(&values);
        let restored = dequantize_block_q4(&packed, scale, values.len());
        for v in &restored {
            assert_eq!(*v, 0.0);
        }
    }

    #[test]
    fn test_quantize_q4_empty() {
        let (packed, scale) = quantize_block_q4(&[]);
        assert!(packed.is_empty());
        assert_eq!(scale, 0.0);
    }

    #[test]
    fn test_quantize_q8_single() {
        let values = vec![0.5];
        let (quantized, scale) = quantize_block_q8(&values);
        let restored = dequantize_block_q8(&quantized, scale);
        assert!((values[0] - restored[0]).abs() < 0.01);
    }

    #[test]
    fn test_compute_quantization_plans() {
        let topology = HeadTopology {
            base_model: "test".to_string(),
            layers: vec![LayerTopology {
                layer_index: 0,
                num_heads: 5,
                num_kv_heads: 5,
                retained_head_indices: vec![0, 1, 2, 3, 4],
                retained_kv_head_indices: vec![0, 1, 2, 3, 4],
                head_precisions: vec![
                    HeadPrecision::Q4,
                    HeadPrecision::Q8,
                    HeadPrecision::BF16,
                    HeadPrecision::Q4,
                    HeadPrecision::BF16,
                ],
                head_scores: vec![0.15, 0.5, 0.8, 0.2, 0.85],
            }],
            original_num_heads: 8,
            original_num_kv_heads: 8,
            head_dim: 64,
            parameter_reduction: 0.375,
            precision_profile: PrecisionProfile {
                removed: 3,
                q4: 2,
                q8: 1,
                bf16: 2,
            },
            created_at: "2026-03-16T00:00:00Z".to_string(),
        };

        let plans = compute_quantization_plans(&topology);
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].q4_indices, vec![0, 3]);
        assert_eq!(plans[0].q8_indices, vec![1]);
        assert_eq!(plans[0].bf16_indices, vec![2, 4]);
    }

    #[test]
    fn test_estimate_layer_savings_all_bf16() {
        let layer = LayerTopology {
            layer_index: 0,
            num_heads: 4,
            num_kv_heads: 4,
            retained_head_indices: vec![0, 1, 2, 3],
            retained_kv_head_indices: vec![0, 1, 2, 3],
            head_precisions: vec![HeadPrecision::BF16; 4],
            head_scores: vec![0.8; 4],
        };

        let (orig, quant) = estimate_layer_savings(&layer, 64, 512);
        // All BF16 → no savings from quantization
        assert_eq!(orig, quant);
    }

    #[test]
    fn test_estimate_layer_savings_mixed() {
        let layer = LayerTopology {
            layer_index: 0,
            num_heads: 4,
            num_kv_heads: 4,
            retained_head_indices: vec![0, 1, 2, 3],
            retained_kv_head_indices: vec![0, 1, 2, 3],
            head_precisions: vec![
                HeadPrecision::Q4,
                HeadPrecision::Q8,
                HeadPrecision::Q8,
                HeadPrecision::BF16,
            ],
            head_scores: vec![0.15, 0.4, 0.5, 0.8],
        };

        let (orig, quant) = estimate_layer_savings(&layer, 64, 512);
        assert!(quant < orig, "Mixed precision should save memory: orig={orig}, quant={quant}");
    }
}
