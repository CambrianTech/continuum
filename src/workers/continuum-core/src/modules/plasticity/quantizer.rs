//! Per-head mixed precision quantization with sub-4-bit support.
//!
//! After compaction removes dead heads, surviving heads get per-head quantization
//! across six precision tiers:
//! - Ternary heads: 1.58-bit {-1, 0, +1} with scale factor (dormant heads)
//! - Q2 heads: 2-bit signed integer [-1, 0, 1, 2] (low utilization)
//! - Q4 heads: 4-bit quantized (medium-low utilization)
//! - Q8 heads: 8-bit quantized (active utilization)
//! - BF16 heads: full precision (hot utilization)
//!
//! Storage: separate sub-tensors per precision tier per projection per layer.
//! ```text
//! model.layers.0.self_attn.q_proj.weight.ternary — [n_ternary_heads * head_dim, hidden]
//! model.layers.0.self_attn.q_proj.weight.q2      — [n_q2_heads * head_dim, hidden]
//! model.layers.0.self_attn.q_proj.weight.q4      — [n_q4_heads * head_dim, hidden]
//! model.layers.0.self_attn.q_proj.weight.q8      — [n_q8_heads * head_dim, hidden]
//! model.layers.0.self_attn.q_proj.weight.bf16    — [n_bf16_heads * head_dim, hidden]
//! ```
//! The topology maps each head index to its sub-tensor and offset within it.

use super::types::*;

/// Per-head quantization plan for a single layer.
#[derive(Debug, Clone)]
pub struct LayerQuantizationPlan {
    pub layer_index: usize,
    /// Indices into the RETAINED head list (not original indices) grouped by precision
    pub ternary_indices: Vec<usize>,
    pub q2_indices: Vec<usize>,
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
            let mut ternary_indices = Vec::new();
            let mut q2_indices = Vec::new();
            let mut q4_indices = Vec::new();
            let mut q8_indices = Vec::new();
            let mut bf16_indices = Vec::new();

            for (i, precision) in layer.head_precisions.iter().enumerate() {
                match precision {
                    HeadPrecision::Removed => {} // Should not appear in retained list
                    HeadPrecision::Ternary => ternary_indices.push(i),
                    HeadPrecision::Q2 => q2_indices.push(i),
                    HeadPrecision::Q4 => q4_indices.push(i),
                    HeadPrecision::Q8 => q8_indices.push(i),
                    HeadPrecision::BF16 => bf16_indices.push(i),
                }
            }

            LayerQuantizationPlan {
                layer_index: layer.layer_index,
                ternary_indices,
                q2_indices,
                q4_indices,
                q8_indices,
                bf16_indices,
            }
        })
        .collect()
}

// =============================================================================
// Ternary quantization (1.58-bit): {-1, 0, +1} with per-block scale factor
//
// Inspired by BitNet b1.58 (Ma et al., 2024). Each value is mapped to one of
// three states: -1, 0, or +1. A per-block scale factor preserves magnitude.
//
// Packing: 5 ternary values per byte (3^5 = 243 ≤ 255).
// This is more efficient than 2 bits/value (4 per byte) and matches the
// information-theoretic minimum: log2(3) ≈ 1.585 bits.
// =============================================================================

/// Quantize a block of f32 values to ternary {-1, 0, +1} with a scale factor.
///
/// Quantization rule: value / scale → round to nearest of {-1, 0, +1}.
/// The threshold for rounding to 0 vs ±1 is 0.5 * scale (midpoint).
///
/// Returns (packed_bytes, scale) where 5 ternary values are packed per byte.
pub fn quantize_block_ternary(values: &[f32]) -> (Vec<u8>, f32) {
    if values.is_empty() {
        return (vec![], 0.0);
    }

    // Scale = mean absolute value (following BitNet b1.58 convention)
    // This gives better round-trip fidelity than absmax for ternary
    let abs_sum: f32 = values.iter().map(|v| v.abs()).sum();
    let scale = if abs_sum > 0.0 {
        abs_sum / values.len() as f32
    } else {
        1.0
    };

    // Quantize each value to {-1, 0, +1}
    let ternary: Vec<i8> = values
        .iter()
        .map(|&v| {
            let normalized = v / scale;
            if normalized > 0.5 {
                1
            } else if normalized < -0.5 {
                -1
            } else {
                0
            }
        })
        .collect();

    // Pack 5 ternary values per byte using base-3 encoding
    // Each ternary value is mapped: -1→0, 0→1, +1→2
    // Then encoded as: byte = t0 + 3*t1 + 9*t2 + 27*t3 + 81*t4
    let mut packed = Vec::with_capacity((ternary.len() + 4) / 5);
    for chunk in ternary.chunks(5) {
        let mut byte: u8 = 0;
        let mut multiplier: u8 = 1;
        for &t in chunk {
            let mapped = (t + 1) as u8; // -1→0, 0→1, +1→2
            byte += mapped * multiplier;
            multiplier *= 3;
        }
        packed.push(byte);
    }

    (packed, scale)
}

/// Dequantize ternary-packed bytes back to f32 values.
pub fn dequantize_block_ternary(packed: &[u8], scale: f32, count: usize) -> Vec<f32> {
    let mut result = Vec::with_capacity(count);
    for &byte in packed {
        let mut remaining = byte;
        for _ in 0..5 {
            if result.len() >= count {
                break;
            }
            let mapped = (remaining % 3) as i8 - 1; // 0→-1, 1→0, 2→+1
            result.push(mapped as f32 * scale);
            remaining /= 3;
        }
    }
    result.truncate(count);
    result
}

// =============================================================================
// Q2 quantization (2-bit): signed 2-bit integer [-1, 0, 1, 2] with scale
//
// 4 values packed per byte (exact). Slightly higher fidelity than ternary
// with the asymmetric range allowing a small positive bias (common in
// attention weights after training).
// =============================================================================

/// Quantize a block of f32 values to 2-bit signed integer [-1, 0, 1, 2].
///
/// Returns (packed_bytes, scale) where 4 Q2 values are packed per byte.
pub fn quantize_block_q2(values: &[f32]) -> (Vec<u8>, f32) {
    if values.is_empty() {
        return (vec![], 0.0);
    }

    let absmax = values.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);

    // Scale maps the range to [-1, 2], so max positive = 2 * scale
    let scale = if absmax > 0.0 { absmax / 2.0 } else { 1.0 };

    // Pack 4 values per byte (2 bits each)
    let mut packed = Vec::with_capacity((values.len() + 3) / 4);
    for chunk in values.chunks(4) {
        let mut byte: u8 = 0;
        for (j, &v) in chunk.iter().enumerate() {
            let quantized = (v / scale).round().clamp(-1.0, 2.0) as i8;
            // Map [-1, 0, 1, 2] to [0, 1, 2, 3] for unsigned packing
            let mapped = (quantized + 1) as u8;
            byte |= (mapped & 0x03) << (j * 2);
        }
        packed.push(byte);
    }

    (packed, scale)
}

/// Dequantize Q2-packed bytes back to f32 values.
pub fn dequantize_block_q2(packed: &[u8], scale: f32, count: usize) -> Vec<f32> {
    let mut result = Vec::with_capacity(count);
    for &byte in packed {
        for j in 0..4 {
            if result.len() >= count {
                break;
            }
            let mapped = ((byte >> (j * 2)) & 0x03) as i8 - 1; // [0,1,2,3] → [-1,0,1,2]
            result.push(mapped as f32 * scale);
        }
    }
    result.truncate(count);
    result
}

/// Quantize a BF16 f32 value to 4-bit (absmax block quantization).
///
/// Block quantization: for a block of values, find the absmax scale,
/// then quantize each value to [-7, 7] (signed 4-bit).
pub fn quantize_block_q4(values: &[f32]) -> (Vec<u8>, f32) {
    if values.is_empty() {
        return (vec![], 0.0);
    }

    let absmax = values.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);

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

    let absmax = values.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);

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
        let bytes = (params_per_head_qo as f64 * precision.bytes_per_param()) as u64;
        quantized_bytes += bytes;
    }

    // K and V projections stay at BF16 (KV heads are shared, not per-Q-head quantized)
    quantized_bytes += (k_params + v_params) * bf16_bytes_per_param;

    (original_bytes, quantized_bytes)
}

/// Estimate total memory savings across all layers.
pub fn estimate_total_savings(topology: &HeadTopology, hidden_size: usize) -> (u64, u64) {
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

    // --- Ternary (1.58-bit) ---

    #[test]
    fn test_quantize_dequantize_ternary_roundtrip() {
        let values = vec![1.0, -0.5, 0.0, -1.0, 0.3, 0.8, -0.9, 0.0, -0.2, 0.6];
        let (packed, scale) = quantize_block_ternary(&values);
        let restored = dequantize_block_ternary(&packed, scale, values.len());

        assert_eq!(restored.len(), values.len());
        // Ternary values should be exactly {-scale, 0, +scale}
        for v in &restored {
            let normalized = v / scale;
            assert!(
                (normalized - -1.0).abs() < 0.01
                    || (normalized - 0.0).abs() < 0.01
                    || (normalized - 1.0).abs() < 0.01,
                "Ternary value should be {{-1, 0, +1}} * scale, got {v} (normalized={normalized})"
            );
        }
    }

    #[test]
    fn test_ternary_packing_5_per_byte() {
        // 10 values should pack into 2 bytes (5 per byte)
        let values = vec![1.0, -1.0, 0.0, 1.0, -1.0, 0.5, -0.5, 0.0, 0.0, 1.0];
        let (packed, _scale) = quantize_block_ternary(&values);
        assert_eq!(
            packed.len(),
            2,
            "10 ternary values should pack into 2 bytes"
        );
    }

    #[test]
    fn test_ternary_packing_exact_boundary() {
        // 5 values = exactly 1 byte
        let values = vec![1.0, 0.0, -1.0, 0.5, -0.5];
        let (packed, _) = quantize_block_ternary(&values);
        assert_eq!(packed.len(), 1);
    }

    #[test]
    fn test_ternary_zeros() {
        let values = vec![0.0; 10];
        let (packed, _scale) = quantize_block_ternary(&values);
        let restored = dequantize_block_ternary(&packed, 1.0, 10);
        for v in &restored {
            assert_eq!(*v, 0.0);
        }
    }

    #[test]
    fn test_ternary_empty() {
        let (packed, scale) = quantize_block_ternary(&[]);
        assert!(packed.is_empty());
        assert_eq!(scale, 0.0);
    }

    // --- Q2 (2-bit) ---

    #[test]
    fn test_quantize_dequantize_q2_roundtrip() {
        let values = vec![1.0, -0.5, 0.0, -1.0, 0.3, 0.8, -0.7, 0.5];
        let (packed, scale) = quantize_block_q2(&values);
        let restored = dequantize_block_q2(&packed, scale, values.len());

        assert_eq!(restored.len(), values.len());
        // Q2 values should be exactly {-scale, 0, scale, 2*scale}
        for v in &restored {
            let normalized = v / scale;
            assert!(
                (-1.0..=2.0).contains(&normalized.round()),
                "Q2 value should be in [-1, 0, 1, 2] * scale, got {v} (normalized={normalized})"
            );
        }
    }

    #[test]
    fn test_q2_packing_4_per_byte() {
        // 8 values should pack into 2 bytes (4 per byte)
        let values = vec![1.0, -1.0, 0.0, 0.5, -0.5, 1.0, -1.0, 0.0];
        let (packed, _) = quantize_block_q2(&values);
        assert_eq!(packed.len(), 2, "8 Q2 values should pack into 2 bytes");
    }

    #[test]
    fn test_q2_zeros() {
        let values = vec![0.0; 8];
        let (packed, _scale) = quantize_block_q2(&values);
        let restored = dequantize_block_q2(&packed, 1.0, 8);
        for v in &restored {
            assert_eq!(*v, 0.0);
        }
    }

    #[test]
    fn test_q2_empty() {
        let (packed, scale) = quantize_block_q2(&[]);
        assert!(packed.is_empty());
        assert_eq!(scale, 0.0);
    }

    // --- Q4 (4-bit) ---

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
                num_heads: 7,
                num_kv_heads: 7,
                retained_head_indices: vec![0, 1, 2, 3, 4, 5, 6],
                retained_kv_head_indices: vec![0, 1, 2, 3, 4, 5, 6],
                head_precisions: vec![
                    HeadPrecision::Ternary,
                    HeadPrecision::Q2,
                    HeadPrecision::Q4,
                    HeadPrecision::Q8,
                    HeadPrecision::BF16,
                    HeadPrecision::Q4,
                    HeadPrecision::Ternary,
                ],
                head_scores: vec![0.12, 0.22, 0.35, 0.55, 0.8, 0.4, 0.15],
            }],
            original_num_heads: 10,
            original_num_kv_heads: 10,
            head_dim: 64,
            parameter_reduction: 0.3,
            precision_profile: PrecisionProfile {
                removed: 3,
                ternary: 2,
                q2: 1,
                q4: 2,
                q8: 1,
                bf16: 1,
            },
            created_at: "2026-03-16T00:00:00Z".to_string(),
        };

        let plans = compute_quantization_plans(&topology);
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].ternary_indices, vec![0, 6]);
        assert_eq!(plans[0].q2_indices, vec![1]);
        assert_eq!(plans[0].q4_indices, vec![2, 5]);
        assert_eq!(plans[0].q8_indices, vec![3]);
        assert_eq!(plans[0].bf16_indices, vec![4]);
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
            num_heads: 5,
            num_kv_heads: 5,
            retained_head_indices: vec![0, 1, 2, 3, 4],
            retained_kv_head_indices: vec![0, 1, 2, 3, 4],
            head_precisions: vec![
                HeadPrecision::Ternary,
                HeadPrecision::Q2,
                HeadPrecision::Q4,
                HeadPrecision::Q8,
                HeadPrecision::BF16,
            ],
            head_scores: vec![0.12, 0.22, 0.35, 0.55, 0.8],
        };

        let (orig, quant) = estimate_layer_savings(&layer, 64, 512);
        assert!(
            quant < orig,
            "Mixed precision should save memory: orig={orig}, quant={quant}"
        );
        // Ternary at 0.2 bytes/param is 10x smaller than BF16 at 2 bytes/param
        // The savings should be substantial with sub-4-bit tiers
    }
}
