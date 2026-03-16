//! Utilization scoring and GQA-aware optimization plan computation.
//!
//! The core formula: `utilization = 0.8 * gate_value + 0.2 * gradient_magnitude`
//!
//! GQA constraint: In grouped query attention (e.g., Llama 24 Q heads / 8 KV heads),
//! a KV head is only prunable when ALL Q heads in its group are dead. This is because
//! KV heads are shared across their Q head group — removing a KV head kills all Q heads
//! that depend on it.

use super::types::*;

/// Compute the full optimization plan from raw utilization data.
///
/// Returns per-layer topology with head retention decisions, precision assignments,
/// and GQA-aware KV head handling.
pub fn compute_optimization_plan(
    scores: &UtilizationData,
    config: &CompactionConfig,
) -> Vec<LayerTopology> {
    let gqa_ratio = if scores.num_kv_heads > 0 {
        scores.num_heads / scores.num_kv_heads
    } else {
        1
    };

    scores
        .layer_scores
        .iter()
        .enumerate()
        .map(|(layer_idx, head_scores)| {
            compute_layer_topology(
                layer_idx,
                head_scores,
                scores.num_heads,
                scores.num_kv_heads,
                gqa_ratio,
                config,
            )
        })
        .collect()
}

/// Compute topology for a single layer.
fn compute_layer_topology(
    layer_index: usize,
    head_scores: &[f64],
    num_heads: usize,
    num_kv_heads: usize,
    gqa_ratio: usize,
    config: &CompactionConfig,
) -> LayerTopology {
    // Step 1: Assign raw precision tier per Q head based on utilization
    let raw_precisions: Vec<HeadPrecision> = head_scores
        .iter()
        .map(|&score| precision_from_config(score, config))
        .collect();

    // Step 2: Apply GQA constraint — a KV head survives if ANY Q head in its group is alive
    let kv_head_alive: Vec<bool> = (0..num_kv_heads)
        .map(|kv_idx| {
            let q_start = kv_idx * gqa_ratio;
            let q_end = ((kv_idx + 1) * gqa_ratio).min(num_heads);
            (q_start..q_end).any(|q_idx| raw_precisions.get(q_idx) != Some(&HeadPrecision::Removed))
        })
        .collect();

    // Step 3: Enforce GQA group integrity
    // - If a KV head must die (all Q heads dead), ensure ALL Q heads in group are removed
    // - If a KV head lives (any Q head alive), ALL Q heads in its group must survive
    //   This is required because repeat_kv expands KV heads by the GQA ratio,
    //   so num_heads / num_kv_heads must always be an integer.
    let mut adjusted_precisions = raw_precisions;
    for kv_idx in 0..num_kv_heads {
        let q_start = kv_idx * gqa_ratio;
        let q_end = ((kv_idx + 1) * gqa_ratio).min(num_heads);

        if kv_head_alive[kv_idx] {
            // KV head lives — promote any Removed Q heads in this group to Q4
            for q_idx in q_start..q_end {
                if let Some(p) = adjusted_precisions.get_mut(q_idx) {
                    if *p == HeadPrecision::Removed {
                        *p = HeadPrecision::Q4;
                    }
                }
            }
        } else {
            // All Q heads in this group are dead — force them all to Removed
            for q_idx in q_start..q_end {
                if let Some(p) = adjusted_precisions.get_mut(q_idx) {
                    *p = HeadPrecision::Removed;
                }
            }
        }
    }

    // Step 4: Enforce minimum heads floor
    let alive_count = adjusted_precisions
        .iter()
        .filter(|p| **p != HeadPrecision::Removed)
        .count();

    if alive_count < config.min_heads_per_layer && num_heads > 0 {
        // Resurrect the highest-scoring dead heads up to the floor
        let mut scored_dead: Vec<(usize, f64)> = adjusted_precisions
            .iter()
            .enumerate()
            .filter(|(_, p)| **p == HeadPrecision::Removed)
            .map(|(idx, _)| (idx, head_scores.get(idx).copied().unwrap_or(0.0)))
            .collect();
        scored_dead.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let to_resurrect = config.min_heads_per_layer - alive_count;
        for (idx, score) in scored_dead.into_iter().take(to_resurrect) {
            // Resurrect at minimum precision (Q4 since they were nearly dead)
            adjusted_precisions[idx] = HeadPrecision::Q4;

            // Also resurrect the KV head for this Q head's group
            let kv_idx = idx / gqa_ratio;
            // No-op if kv_head_alive already true; we just track it
            let _ = (kv_idx, score);
        }
    }

    // Step 5: Enforce minimum KV heads floor
    let alive_kv_count = compute_alive_kv_heads(&adjusted_precisions, num_kv_heads, gqa_ratio);
    if alive_kv_count < config.min_kv_heads_per_layer && num_kv_heads > 0 {
        let mut kv_scores: Vec<(usize, f64)> = (0..num_kv_heads)
            .filter(|&kv_idx| {
                let q_start = kv_idx * gqa_ratio;
                let q_end = ((kv_idx + 1) * gqa_ratio).min(num_heads);
                (q_start..q_end).all(|q| adjusted_precisions.get(q) == Some(&HeadPrecision::Removed))
            })
            .map(|kv_idx| {
                let q_start = kv_idx * gqa_ratio;
                let q_end = ((kv_idx + 1) * gqa_ratio).min(num_heads);
                let max_score = (q_start..q_end)
                    .filter_map(|q| head_scores.get(q))
                    .cloned()
                    .fold(0.0_f64, f64::max);
                (kv_idx, max_score)
            })
            .collect();
        kv_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let to_resurrect = config.min_kv_heads_per_layer - alive_kv_count;
        for (kv_idx, _) in kv_scores.into_iter().take(to_resurrect) {
            // Resurrect the best Q head in this KV group
            let q_start = kv_idx * gqa_ratio;
            let q_end = ((kv_idx + 1) * gqa_ratio).min(num_heads);
            if let Some(best_q) = (q_start..q_end)
                .max_by(|&a, &b| {
                    head_scores
                        .get(a)
                        .unwrap_or(&0.0)
                        .partial_cmp(head_scores.get(b).unwrap_or(&0.0))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            {
                adjusted_precisions[best_q] = HeadPrecision::Q4;
            }
        }
    }

    // Step 6: Build retained indices and output
    let retained_head_indices: Vec<usize> = adjusted_precisions
        .iter()
        .enumerate()
        .filter(|(_, p)| **p != HeadPrecision::Removed)
        .map(|(idx, _)| idx)
        .collect();

    let retained_kv_head_indices: Vec<usize> =
        compute_retained_kv_indices(&adjusted_precisions, num_kv_heads, gqa_ratio);

    let head_precisions: Vec<HeadPrecision> = retained_head_indices
        .iter()
        .map(|&idx| adjusted_precisions[idx])
        .collect();

    let head_scores_retained: Vec<f64> = retained_head_indices
        .iter()
        .map(|&idx| head_scores.get(idx).copied().unwrap_or(0.0))
        .collect();

    LayerTopology {
        layer_index,
        num_heads: retained_head_indices.len(),
        num_kv_heads: retained_kv_head_indices.len(),
        retained_head_indices,
        retained_kv_head_indices,
        head_precisions,
        head_scores: head_scores_retained,
    }
}

/// Determine precision from configurable thresholds.
fn precision_from_config(score: f64, config: &CompactionConfig) -> HeadPrecision {
    if score < config.dead_threshold {
        HeadPrecision::Removed
    } else if score < config.low_threshold {
        HeadPrecision::Q4
    } else if score < config.high_threshold {
        HeadPrecision::Q8
    } else {
        HeadPrecision::BF16
    }
}

/// Count how many KV heads have at least one alive Q head.
fn compute_alive_kv_heads(
    precisions: &[HeadPrecision],
    num_kv_heads: usize,
    gqa_ratio: usize,
) -> usize {
    (0..num_kv_heads)
        .filter(|&kv_idx| {
            let q_start = kv_idx * gqa_ratio;
            let q_end = ((kv_idx + 1) * gqa_ratio).min(precisions.len());
            (q_start..q_end).any(|q| precisions.get(q) != Some(&HeadPrecision::Removed))
        })
        .count()
}

/// Compute which KV head indices are retained based on Q head survival.
fn compute_retained_kv_indices(
    precisions: &[HeadPrecision],
    num_kv_heads: usize,
    gqa_ratio: usize,
) -> Vec<usize> {
    (0..num_kv_heads)
        .filter(|&kv_idx| {
            let q_start = kv_idx * gqa_ratio;
            let q_end = ((kv_idx + 1) * gqa_ratio).min(precisions.len());
            (q_start..q_end).any(|q| precisions.get(q) != Some(&HeadPrecision::Removed))
        })
        .collect()
}

/// Build a PrecisionProfile summary from layer topologies.
pub fn compute_precision_profile(
    layers: &[LayerTopology],
    original_num_heads: usize,
    num_layers: usize,
) -> PrecisionProfile {
    let mut profile = PrecisionProfile::default();
    let mut total_retained = 0;

    for layer in layers {
        for precision in &layer.head_precisions {
            match precision {
                HeadPrecision::Removed => profile.removed += 1,
                HeadPrecision::Q4 => profile.q4 += 1,
                HeadPrecision::Q8 => profile.q8 += 1,
                HeadPrecision::BF16 => profile.bf16 += 1,
            }
        }
        total_retained += layer.head_precisions.len();
    }

    // Removed count is total original heads minus retained (across all layers)
    let total_original = original_num_heads * num_layers;
    profile.removed = total_original - total_retained;

    profile
}

/// Estimate parameter reduction ratio from layer topologies.
///
/// Computes the fraction of attention parameters saved by compaction.
/// Only counts attention weights (Q/K/V/O projections), not MLP or embeddings.
pub fn estimate_parameter_reduction(
    layers: &[LayerTopology],
    original_num_heads: usize,
    original_num_kv_heads: usize,
    head_dim: usize,
    hidden_size: usize,
) -> f64 {
    if layers.is_empty() || original_num_heads == 0 {
        return 0.0;
    }

    let num_layers = layers.len();

    // Original attention params per layer:
    // Q: [num_heads * head_dim, hidden_size]
    // K: [num_kv_heads * head_dim, hidden_size]
    // V: [num_kv_heads * head_dim, hidden_size]
    // O: [hidden_size, num_heads * head_dim]
    let original_attn_params_per_layer = (original_num_heads * head_dim * hidden_size) // Q
        + (original_num_kv_heads * head_dim * hidden_size)  // K
        + (original_num_kv_heads * head_dim * hidden_size)  // V
        + (hidden_size * original_num_heads * head_dim); // O

    let original_total = original_attn_params_per_layer * num_layers;

    let compacted_total: usize = layers
        .iter()
        .map(|layer| {
            let q_params = layer.num_heads * head_dim * hidden_size;
            let k_params = layer.num_kv_heads * head_dim * hidden_size;
            let v_params = layer.num_kv_heads * head_dim * hidden_size;
            let o_params = hidden_size * layer.num_heads * head_dim;
            // Weight by precision tier (Q4=0.25, Q8=0.5, BF16=1.0)
            // For parameter COUNT, we count full params; for SIZE, precision matters
            q_params + k_params + v_params + o_params
        })
        .sum();

    if original_total == 0 {
        return 0.0;
    }

    1.0 - (compacted_total as f64 / original_total as f64)
}

/// Identify saturated heads (candidates for mitosis/splitting).
pub fn find_saturated_heads(
    scores: &UtilizationData,
    config: &CompactionConfig,
) -> Vec<SaturatedHead> {
    let mut saturated = Vec::new();
    for (layer_idx, layer_scores) in scores.layer_scores.iter().enumerate() {
        for (head_idx, &score) in layer_scores.iter().enumerate() {
            if score > config.saturated_threshold {
                saturated.push(SaturatedHead {
                    layer_index: layer_idx,
                    head_index: head_idx,
                    utilization: score,
                });
            }
        }
    }
    saturated
}

/// Compute per-layer analysis summaries.
pub fn compute_layer_summaries(
    scores: &UtilizationData,
    layers: &[LayerTopology],
    config: &CompactionConfig,
) -> Vec<LayerSummary> {
    scores
        .layer_scores
        .iter()
        .enumerate()
        .zip(layers.iter())
        .map(|((layer_idx, head_scores), topology)| {
            let mut q4 = 0usize;
            let mut q8 = 0usize;
            let mut bf16 = 0usize;
            let mut saturated = 0usize;

            for precision in &topology.head_precisions {
                match precision {
                    HeadPrecision::Removed => {}
                    HeadPrecision::Q4 => q4 += 1,
                    HeadPrecision::Q8 => q8 += 1,
                    HeadPrecision::BF16 => bf16 += 1,
                }
            }

            let removed = head_scores.len() - topology.num_heads;

            for &score in head_scores {
                if score > config.saturated_threshold {
                    saturated += 1;
                }
            }

            let (min_s, max_s, sum_s) = head_scores.iter().fold(
                (f64::MAX, f64::MIN, 0.0),
                |(min, max, sum), &s| (min.min(s), max.max(s), sum + s),
            );

            let mean = if head_scores.is_empty() {
                0.0
            } else {
                sum_s / head_scores.len() as f64
            };

            LayerSummary {
                layer_index: layer_idx,
                heads_removed: removed,
                heads_q4: q4,
                heads_q8: q8,
                heads_bf16: bf16,
                heads_saturated: saturated,
                min_score: if head_scores.is_empty() { 0.0 } else { min_s },
                max_score: if head_scores.is_empty() { 0.0 } else { max_s },
                mean_score: mean,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_scores(layer_scores: Vec<Vec<f64>>, num_heads: usize, num_kv_heads: usize) -> UtilizationData {
        UtilizationData {
            layer_scores,
            num_steps: 100,
            model_name: "test-model".to_string(),
            num_heads,
            num_kv_heads,
        }
    }

    fn default_config() -> CompactionConfig {
        CompactionConfig::default()
    }

    // --- HeadPrecision ---

    #[test]
    fn test_precision_from_utilization_dead() {
        assert_eq!(HeadPrecision::from_utilization(0.0), HeadPrecision::Removed);
        assert_eq!(HeadPrecision::from_utilization(0.05), HeadPrecision::Removed);
        assert_eq!(HeadPrecision::from_utilization(0.099), HeadPrecision::Removed);
    }

    #[test]
    fn test_precision_from_utilization_low() {
        assert_eq!(HeadPrecision::from_utilization(0.1), HeadPrecision::Q4);
        assert_eq!(HeadPrecision::from_utilization(0.2), HeadPrecision::Q4);
        assert_eq!(HeadPrecision::from_utilization(0.299), HeadPrecision::Q4);
    }

    #[test]
    fn test_precision_from_utilization_medium() {
        assert_eq!(HeadPrecision::from_utilization(0.3), HeadPrecision::Q8);
        assert_eq!(HeadPrecision::from_utilization(0.5), HeadPrecision::Q8);
        assert_eq!(HeadPrecision::from_utilization(0.699), HeadPrecision::Q8);
    }

    #[test]
    fn test_precision_from_utilization_high() {
        assert_eq!(HeadPrecision::from_utilization(0.7), HeadPrecision::BF16);
        assert_eq!(HeadPrecision::from_utilization(0.85), HeadPrecision::BF16);
        assert_eq!(HeadPrecision::from_utilization(1.0), HeadPrecision::BF16);
    }

    #[test]
    fn test_precision_bits() {
        assert_eq!(HeadPrecision::Removed.bits(), 0);
        assert_eq!(HeadPrecision::Q4.bits(), 4);
        assert_eq!(HeadPrecision::Q8.bits(), 8);
        assert_eq!(HeadPrecision::BF16.bits(), 16);
    }

    // --- Basic scoring ---

    #[test]
    fn test_all_heads_high_utilization() {
        // All heads at 0.8 => all BF16, nothing removed
        let scores = make_scores(vec![vec![0.8; 8]], 8, 8);
        let plan = compute_optimization_plan(&scores, &default_config());

        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].num_heads, 8);
        assert_eq!(plan[0].num_kv_heads, 8);
        assert!(plan[0].head_precisions.iter().all(|p| *p == HeadPrecision::BF16));
    }

    #[test]
    fn test_all_heads_dead() {
        // All heads at 0.01 => all would be removed, but min_heads floor kicks in
        let config = CompactionConfig {
            min_heads_per_layer: 4,
            min_kv_heads_per_layer: 2,
            ..default_config()
        };
        let scores = make_scores(vec![vec![0.01; 8]], 8, 8);
        let plan = compute_optimization_plan(&scores, &config);

        assert_eq!(plan[0].num_heads, 4); // min_heads floor
        assert!(plan[0].num_kv_heads >= 2); // min_kv_heads floor
    }

    #[test]
    fn test_mixed_utilization() {
        // 8 heads with varying utilization
        let scores = make_scores(
            vec![vec![0.05, 0.15, 0.25, 0.4, 0.5, 0.65, 0.8, 0.95]],
            8, 8,
        );
        let plan = compute_optimization_plan(&scores, &default_config());

        let layer = &plan[0];
        // Head 0 (0.05) => Removed
        // Head 1 (0.15) => Q4
        // Head 2 (0.25) => Q4
        // Head 3 (0.4)  => Q8
        // Head 4 (0.5)  => Q8
        // Head 5 (0.65) => Q8
        // Head 6 (0.8)  => BF16
        // Head 7 (0.95) => BF16
        assert_eq!(layer.num_heads, 7); // Head 0 removed
        assert!(!layer.retained_head_indices.contains(&0));
        assert!(layer.retained_head_indices.contains(&1));
        assert!(layer.retained_head_indices.contains(&7));
    }

    // --- GQA constraints ---

    #[test]
    fn test_gqa_kv_head_survives_if_any_q_head_alive() {
        // 6 Q heads, 2 KV heads (3:1 ratio)
        // KV group 0: Q heads [0,1,2] — head 2 is alive (0.5), heads 0,1 dead (0.01)
        // KV group 1: Q heads [3,4,5] — all dead (0.01)
        let config = CompactionConfig {
            min_heads_per_layer: 1,
            min_kv_heads_per_layer: 1,
            ..default_config()
        };
        let scores = make_scores(
            vec![vec![0.01, 0.01, 0.5, 0.01, 0.01, 0.01]],
            6, 2,
        );
        let plan = compute_optimization_plan(&scores, &config);

        let layer = &plan[0];
        // KV head 0 should survive (Q head 2 is alive)
        // KV head 1 should die (all Q heads dead)
        assert!(layer.retained_kv_head_indices.contains(&0));

        // Q head 2 must survive
        assert!(layer.retained_head_indices.contains(&2));
    }

    #[test]
    fn test_gqa_all_q_heads_dead_kills_kv_head() {
        // 6 Q heads, 2 KV heads (3:1 ratio)
        // KV group 0: Q heads [0,1,2] — all dead
        // KV group 1: Q heads [3,4,5] — all alive
        let config = CompactionConfig {
            min_heads_per_layer: 1,
            min_kv_heads_per_layer: 1,
            ..default_config()
        };
        let scores = make_scores(
            vec![vec![0.01, 0.01, 0.01, 0.8, 0.8, 0.8]],
            6, 2,
        );
        let plan = compute_optimization_plan(&scores, &config);

        let layer = &plan[0];
        // KV head 0 dead, KV head 1 alive
        assert!(!layer.retained_kv_head_indices.contains(&0));
        assert!(layer.retained_kv_head_indices.contains(&1));
        // Q heads 3,4,5 alive; 0,1,2 removed
        assert_eq!(layer.num_heads, 3);
    }

    #[test]
    fn test_gqa_llama_realistic() {
        // Llama-3.2-3B: 24 Q heads, 8 KV heads (3:1 ratio)
        // Simulate realistic scores
        let mut head_scores = vec![0.5; 24]; // Default medium
        // Kill KV group 0 (Q heads 0,1,2)
        head_scores[0] = 0.01;
        head_scores[1] = 0.02;
        head_scores[2] = 0.03;
        // Kill KV group 3 (Q heads 9,10,11)
        head_scores[9] = 0.01;
        head_scores[10] = 0.01;
        head_scores[11] = 0.02;
        // Saturate KV group 7 (Q heads 21,22,23)
        head_scores[21] = 0.95;
        head_scores[22] = 0.92;
        head_scores[23] = 0.88;

        let scores = make_scores(vec![head_scores], 24, 8);
        let plan = compute_optimization_plan(&scores, &default_config());

        let layer = &plan[0];
        // 6 Q heads removed (groups 0 and 3), 18 remaining
        assert_eq!(layer.num_heads, 18);
        // 2 KV heads removed (groups 0 and 3), 6 remaining
        assert_eq!(layer.num_kv_heads, 6);
        // High-util heads should be BF16
        assert!(layer.head_precisions.iter().any(|p| *p == HeadPrecision::BF16));
    }

    // --- Min heads floor ---

    #[test]
    fn test_min_heads_floor_resurrects_best_dead() {
        // 4 heads, all below dead threshold, min_heads = 2
        let config = CompactionConfig {
            min_heads_per_layer: 2,
            min_kv_heads_per_layer: 1,
            ..default_config()
        };
        let scores = make_scores(vec![vec![0.01, 0.05, 0.08, 0.03]], 4, 4);
        let plan = compute_optimization_plan(&scores, &config);

        let layer = &plan[0];
        assert_eq!(layer.num_heads, 2);
        // Should resurrect heads 2 (0.08) and 1 (0.05) — highest scores
        assert!(layer.retained_head_indices.contains(&2));
        assert!(layer.retained_head_indices.contains(&1));
    }

    // --- Multi-layer ---

    #[test]
    fn test_multi_layer_independent() {
        // Each layer gets independent decisions
        let scores = make_scores(
            vec![
                vec![0.8, 0.8, 0.8, 0.8],   // Layer 0: all high
                vec![0.01, 0.01, 0.01, 0.8], // Layer 1: 3 dead, 1 high
            ],
            4, 4,
        );
        let plan = compute_optimization_plan(&scores, &default_config());

        assert_eq!(plan[0].num_heads, 4); // All alive
        // Layer 1: 1 alive + min_heads floor (4 min) → resurrect 3
        assert_eq!(plan[1].num_heads, 4);
    }

    // --- Precision profile ---

    #[test]
    fn test_precision_profile_computation() {
        let layers = vec![
            LayerTopology {
                layer_index: 0,
                num_heads: 3,
                num_kv_heads: 3,
                retained_head_indices: vec![1, 2, 3],
                retained_kv_head_indices: vec![1, 2, 3],
                head_precisions: vec![HeadPrecision::Q4, HeadPrecision::Q8, HeadPrecision::BF16],
                head_scores: vec![0.2, 0.5, 0.8],
            },
        ];
        let profile = compute_precision_profile(&layers, 4, 1);
        assert_eq!(profile.removed, 1);
        assert_eq!(profile.q4, 1);
        assert_eq!(profile.q8, 1);
        assert_eq!(profile.bf16, 1);
    }

    // --- Saturated head detection ---

    #[test]
    fn test_find_saturated_heads() {
        let scores = make_scores(
            vec![vec![0.5, 0.95, 0.3, 0.92]],
            4, 4,
        );
        let saturated = find_saturated_heads(&scores, &default_config());
        assert_eq!(saturated.len(), 2);
        assert_eq!(saturated[0].head_index, 1);
        assert_eq!(saturated[1].head_index, 3);
    }

    // --- Parameter reduction ---

    #[test]
    fn test_parameter_reduction_no_pruning() {
        let layers = vec![LayerTopology {
            layer_index: 0,
            num_heads: 8,
            num_kv_heads: 8,
            retained_head_indices: (0..8).collect(),
            retained_kv_head_indices: (0..8).collect(),
            head_precisions: vec![HeadPrecision::BF16; 8],
            head_scores: vec![0.8; 8],
        }];
        let reduction = estimate_parameter_reduction(&layers, 8, 8, 64, 512);
        assert!((reduction - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_parameter_reduction_half_pruned() {
        // Remove half the Q heads and half the KV heads
        let layers = vec![LayerTopology {
            layer_index: 0,
            num_heads: 4,
            num_kv_heads: 4,
            retained_head_indices: vec![0, 1, 2, 3],
            retained_kv_head_indices: vec![0, 1, 2, 3],
            head_precisions: vec![HeadPrecision::BF16; 4],
            head_scores: vec![0.8; 4],
        }];
        let reduction = estimate_parameter_reduction(&layers, 8, 8, 64, 512);
        assert!(reduction > 0.49 && reduction < 0.51, "Expected ~0.5, got {}", reduction);
    }

    // --- Edge cases ---

    #[test]
    fn test_empty_scores() {
        let scores = make_scores(vec![], 0, 0);
        let plan = compute_optimization_plan(&scores, &default_config());
        assert!(plan.is_empty());
    }

    #[test]
    fn test_single_head_not_removed() {
        // 1 head that's dead, but min_heads = 1 saves it
        let config = CompactionConfig {
            min_heads_per_layer: 1,
            min_kv_heads_per_layer: 1,
            ..default_config()
        };
        let scores = make_scores(vec![vec![0.01]], 1, 1);
        let plan = compute_optimization_plan(&scores, &config);
        assert_eq!(plan[0].num_heads, 1);
    }

    // --- Layer summary ---

    #[test]
    fn test_layer_summaries() {
        let scores = make_scores(
            vec![vec![0.05, 0.2, 0.5, 0.8, 0.95]],
            5, 5,
        );
        let config = CompactionConfig {
            min_heads_per_layer: 1,
            min_kv_heads_per_layer: 1,
            ..default_config()
        };
        let plan = compute_optimization_plan(&scores, &config);
        let summaries = compute_layer_summaries(&scores, &plan, &config);

        assert_eq!(summaries.len(), 1);
        let s = &summaries[0];
        assert_eq!(s.heads_removed, 1); // Head 0 (0.05) removed
        assert!(s.heads_bf16 >= 1);      // Heads 3,4 are BF16
        assert!(s.heads_saturated >= 1);  // Head 4 (0.95) saturated
        assert!((s.min_score - 0.05).abs() < 1e-6);
        assert!((s.max_score - 0.95).abs() < 1e-6);
    }

    // =========================================================================
    // Sentinel-AI mirror tests
    //
    // These test the same decision logic as sentinel-ai's
    // test_plasticity_controller.py, ported to the Rust tier system.
    //
    // Sentinel-AI mapping:
    //   KEEP   -> any non-Removed tier (Q4/Q8/BF16)
    //   PRUNE  -> HeadPrecision::Removed
    //   REVIVE -> N/A (handled by min_heads floor in Rust)
    //
    // The Python system uses entropy+gradient thresholds to decide.
    // The Rust system uses a combined utilization score (0.0-1.0) mapped to
    // precision tiers. The philosophy is the same: dead heads get pruned,
    // active heads get full precision.
    // =========================================================================

    #[test]
    fn test_sentinel_mirror_keep_active_heads() {
        // Mirror: test_decide_head_fate_keep
        // Heads with decent utilization should be kept (not Removed)
        let config = CompactionConfig {
            min_heads_per_layer: 1,
            min_kv_heads_per_layer: 1,
            ..default_config()
        };

        // Case 1: High utilization — clearly useful head → BF16
        let scores = make_scores(vec![vec![0.85]], 1, 1);
        let plan = compute_optimization_plan(&scores, &config);
        assert_ne!(plan[0].head_precisions[0], HeadPrecision::Removed);
        assert_eq!(plan[0].head_precisions[0], HeadPrecision::BF16);

        // Case 2: Medium utilization — moderately useful → Q8
        let scores = make_scores(vec![vec![0.5]], 1, 1);
        let plan = compute_optimization_plan(&scores, &config);
        assert_ne!(plan[0].head_precisions[0], HeadPrecision::Removed);
        assert_eq!(plan[0].head_precisions[0], HeadPrecision::Q8);

        // Case 3: Low but alive utilization — marginal → Q4
        let scores = make_scores(vec![vec![0.15]], 1, 1);
        let plan = compute_optimization_plan(&scores, &config);
        assert_ne!(plan[0].head_precisions[0], HeadPrecision::Removed);
        assert_eq!(plan[0].head_precisions[0], HeadPrecision::Q4);
    }

    #[test]
    fn test_sentinel_mirror_prune_dead_heads() {
        // Mirror: test_decide_head_fate_prune
        // Heads with near-zero utilization should be pruned (Removed)
        let config = CompactionConfig {
            min_heads_per_layer: 0, // Allow full pruning for this test
            min_kv_heads_per_layer: 0,
            ..default_config()
        };

        // Dead head: utilization 0.01 → Removed
        let scores = make_scores(vec![vec![0.01]], 1, 1);
        let plan = compute_optimization_plan(&scores, &config);
        assert_eq!(plan[0].num_heads, 0);

        // Just below threshold: 0.09 → Removed
        let scores = make_scores(vec![vec![0.09]], 1, 1);
        let plan = compute_optimization_plan(&scores, &config);
        assert_eq!(plan[0].num_heads, 0);

        // At threshold: 0.1 → Q4 (alive!)
        let scores = make_scores(vec![vec![0.1]], 1, 1);
        let plan = compute_optimization_plan(&scores, &config);
        assert_eq!(plan[0].num_heads, 1);
        assert_eq!(plan[0].head_precisions[0], HeadPrecision::Q4);
    }

    #[test]
    fn test_sentinel_mirror_revival_via_min_floor() {
        // Mirror: test_decide_head_fate_revive
        // In sentinel-ai, zeroed heads can be revived if gradient returns.
        // In Rust, the min_heads_per_layer floor serves the same purpose:
        // if too many heads are dead, the best dead heads are resurrected.
        let config = CompactionConfig {
            min_heads_per_layer: 2,
            min_kv_heads_per_layer: 1,
            ..default_config()
        };

        // 4 heads, all dead — floor resurrects best 2
        let scores = make_scores(vec![vec![0.05, 0.03, 0.08, 0.02]], 4, 4);
        let plan = compute_optimization_plan(&scores, &config);

        assert_eq!(plan[0].num_heads, 2);
        // Head 2 (0.08) and head 0 (0.05) should be resurrected as highest-scoring dead
        assert!(plan[0].retained_head_indices.contains(&2));
        assert!(plan[0].retained_head_indices.contains(&0));
    }

    #[test]
    fn test_sentinel_mirror_threshold_boundary_behavior() {
        // Mirror: test_thresholds_with_realistic_values
        // Test exact boundary behavior at each tier transition
        let config = CompactionConfig {
            min_heads_per_layer: 0,
            min_kv_heads_per_layer: 0,
            ..default_config()
        };

        // Boundary: dead_threshold (0.1)
        let scores = make_scores(vec![vec![0.099, 0.1]], 2, 2);
        let plan = compute_optimization_plan(&scores, &config);
        assert_eq!(plan[0].num_heads, 1); // 0.099 removed, 0.1 kept as Q4

        // Boundary: low_threshold (0.3)
        let scores = make_scores(vec![vec![0.299, 0.3]], 2, 2);
        let plan = compute_optimization_plan(&scores, &config);
        assert_eq!(plan[0].head_precisions[0], HeadPrecision::Q4);  // 0.299
        assert_eq!(plan[0].head_precisions[1], HeadPrecision::Q8);  // 0.3

        // Boundary: high_threshold (0.7)
        let scores = make_scores(vec![vec![0.699, 0.7]], 2, 2);
        let plan = compute_optimization_plan(&scores, &config);
        assert_eq!(plan[0].head_precisions[0], HeadPrecision::Q8);   // 0.699
        assert_eq!(plan[0].head_precisions[1], HeadPrecision::BF16); // 0.7
    }

    #[test]
    fn test_sentinel_mirror_realistic_scenarios() {
        // Mirror: test_thresholds_with_realistic_values
        // Simulate realistic per-head utilization from actual LoRA training
        struct TestCase {
            score: f64,
            expected_alive: bool,
            expected_precision: Option<HeadPrecision>,
        }

        let cases = vec![
            TestCase { score: 0.02, expected_alive: false, expected_precision: None },           // Dead: no gradient flow
            TestCase { score: 0.08, expected_alive: false, expected_precision: None },           // Dead: barely active
            TestCase { score: 0.12, expected_alive: true, expected_precision: Some(HeadPrecision::Q4) },  // Low: marginal
            TestCase { score: 0.25, expected_alive: true, expected_precision: Some(HeadPrecision::Q4) },  // Low: contributing
            TestCase { score: 0.35, expected_alive: true, expected_precision: Some(HeadPrecision::Q8) },  // Medium: useful
            TestCase { score: 0.55, expected_alive: true, expected_precision: Some(HeadPrecision::Q8) },  // Medium: solid
            TestCase { score: 0.75, expected_alive: true, expected_precision: Some(HeadPrecision::BF16) }, // High: critical
            TestCase { score: 0.88, expected_alive: true, expected_precision: Some(HeadPrecision::BF16) }, // High: essential
            TestCase { score: 0.95, expected_alive: true, expected_precision: Some(HeadPrecision::BF16) }, // Saturated: overloaded
        ];

        let config = CompactionConfig {
            min_heads_per_layer: 0,
            min_kv_heads_per_layer: 0,
            ..default_config()
        };

        for (i, case) in cases.iter().enumerate() {
            let scores = make_scores(vec![vec![case.score]], 1, 1);
            let plan = compute_optimization_plan(&scores, &config);

            if case.expected_alive {
                assert_eq!(
                    plan[0].num_heads, 1,
                    "Case {i}: score={} should be alive", case.score
                );
                assert_eq!(
                    plan[0].head_precisions[0],
                    case.expected_precision.unwrap(),
                    "Case {i}: score={} wrong precision", case.score
                );
            } else {
                assert_eq!(
                    plan[0].num_heads, 0,
                    "Case {i}: score={} should be dead", case.score
                );
            }
        }
    }

    #[test]
    fn test_sentinel_mirror_custom_thresholds() {
        // Mirror: testing with modified thresholds (controller.high_entropy_threshold = 0.4)
        // Our system allows custom thresholds via CompactionConfig
        let aggressive = CompactionConfig {
            dead_threshold: 0.2,  // More aggressive pruning
            low_threshold: 0.4,
            high_threshold: 0.6,  // Faster path to full precision
            min_heads_per_layer: 0,
            min_kv_heads_per_layer: 0,
            ..default_config()
        };

        // Score 0.15 → dead with aggressive config (was Q4 with default)
        let scores = make_scores(vec![vec![0.15]], 1, 1);
        let plan = compute_optimization_plan(&scores, &aggressive);
        assert_eq!(plan[0].num_heads, 0); // Pruned with aggressive threshold

        // Score 0.35 → Q4 with aggressive config (was Q8 with default)
        let scores = make_scores(vec![vec![0.35]], 1, 1);
        let plan = compute_optimization_plan(&scores, &aggressive);
        assert_eq!(plan[0].head_precisions[0], HeadPrecision::Q4);

        // Score 0.65 → BF16 with aggressive config (was Q8 with default)
        let scores = make_scores(vec![vec![0.65]], 1, 1);
        let plan = compute_optimization_plan(&scores, &aggressive);
        assert_eq!(plan[0].head_precisions[0], HeadPrecision::BF16);
    }

    #[test]
    fn test_sentinel_mirror_distilgpt2_scale() {
        // Mirror: test_controller_creation (distilgpt2: 6 layers, 12 heads)
        // Simulate distilgpt2-scale model to match sentinel-ai test fixture
        let scores = make_scores(
            vec![
                vec![0.05, 0.8, 0.3, 0.6, 0.9, 0.1, 0.4, 0.7, 0.2, 0.95, 0.5, 0.85],
                vec![0.1, 0.7, 0.4, 0.5, 0.8, 0.2, 0.3, 0.6, 0.15, 0.9, 0.45, 0.75],
                vec![0.3, 0.6, 0.5, 0.4, 0.7, 0.35, 0.45, 0.55, 0.25, 0.8, 0.4, 0.65],
                vec![0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5],
                vec![0.01, 0.01, 0.01, 0.01, 0.9, 0.9, 0.9, 0.9, 0.01, 0.01, 0.9, 0.9],
                vec![0.7, 0.8, 0.9, 0.95, 0.3, 0.2, 0.1, 0.05, 0.5, 0.6, 0.4, 0.35],
            ],
            12, 12,
        );

        let plan = compute_optimization_plan(&scores, &default_config());

        // 6 layers computed
        assert_eq!(plan.len(), 6);

        // Layer 0: head 0 (0.05) dead, head 5 (0.1) Q4, others alive
        assert!(!plan[0].retained_head_indices.contains(&0));
        assert!(plan[0].retained_head_indices.contains(&1)); // 0.8 → BF16

        // Layer 3: all at 0.5 → all Q8
        assert_eq!(plan[3].num_heads, 12);
        assert!(plan[3].head_precisions.iter().all(|p| *p == HeadPrecision::Q8));

        // Layer 4: 6 dead heads (0.01), 4 alive at 0.9 + 2 alive at 0.9
        // min_heads_per_layer = 4, so at least 4 survive
        assert!(plan[4].num_heads >= 4);
        // The 0.9 heads should all be BF16
        let bf16_count = plan[4].head_precisions.iter()
            .filter(|p| **p == HeadPrecision::BF16)
            .count();
        assert!(bf16_count >= 4, "Should have at least 4 BF16 heads, got {}", bf16_count);

        // Layer 5: heads 6 (0.1) and 7 (0.05) should be dead/Q4
        let head_7_in_retained = plan[5].retained_head_indices.contains(&7);
        if head_7_in_retained {
            // If resurrected by floor, should be at Q4
            let idx = plan[5].retained_head_indices.iter().position(|&x| x == 7).unwrap();
            assert_eq!(plan[5].head_precisions[idx], HeadPrecision::Q4);
        }
    }

    #[test]
    fn test_sentinel_mirror_gqa_respects_group_integrity() {
        // Key test: GQA groups must be all-or-nothing.
        // If Q head 0 is dead but Q head 1 (same KV group) is alive,
        // BOTH Q heads must survive (GQA ratio constraint: num_heads % num_kv_heads == 0).
        let config = CompactionConfig {
            min_heads_per_layer: 0,
            min_kv_heads_per_layer: 0,
            ..default_config()
        };

        // 4 Q heads, 2 KV heads (2:1 ratio)
        // Group 0: Q[0]=dead(0.01), Q[1]=alive(0.8) → KV[0] lives → Q[0] promoted to Q4
        // Group 1: Q[2]=dead(0.01), Q[3]=dead(0.01) → KV[1] dies
        let scores = make_scores(vec![vec![0.01, 0.8, 0.01, 0.01]], 4, 2);
        let plan = compute_optimization_plan(&scores, &config);

        // KV head 0 survives (Q head 1 alive)
        assert!(plan[0].retained_kv_head_indices.contains(&0));
        // KV head 1 dies (both Q heads dead)
        assert!(!plan[0].retained_kv_head_indices.contains(&1));
        // BOTH Q heads in group 0 survive (GQA group integrity)
        assert!(plan[0].retained_head_indices.contains(&0));
        assert!(plan[0].retained_head_indices.contains(&1));
        assert_eq!(plan[0].num_heads, 2);
        // Q head 0 promoted to Q4, Q head 1 at BF16
        assert_eq!(plan[0].head_precisions[0], HeadPrecision::Q4);
        assert_eq!(plan[0].head_precisions[1], HeadPrecision::BF16);
        // GQA ratio maintained
        assert_eq!(plan[0].num_heads % plan[0].num_kv_heads, 0);
    }
}
