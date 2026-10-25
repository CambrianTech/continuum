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
            // KV head lives — promote any Removed Q heads to minimum alive precision (Ternary)
            // This is the cheapest way to keep them: 1.58 bits vs 4 bits for Q4
            for q_idx in q_start..q_end {
                if let Some(p) = adjusted_precisions.get_mut(q_idx) {
                    if *p == HeadPrecision::Removed {
                        *p = HeadPrecision::minimum_alive();
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
            // Resurrect at minimum alive precision (Ternary — cheapest non-removed)
            adjusted_precisions[idx] = HeadPrecision::minimum_alive();

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
                (q_start..q_end)
                    .all(|q| adjusted_precisions.get(q) == Some(&HeadPrecision::Removed))
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
            if let Some(best_q) = (q_start..q_end).max_by(|&a, &b| {
                head_scores
                    .get(a)
                    .unwrap_or(&0.0)
                    .partial_cmp(head_scores.get(b).unwrap_or(&0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) {
                adjusted_precisions[best_q] = HeadPrecision::minimum_alive();
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
///
/// Seven-tier mapping:
/// ```text
/// 0.0 ──dead── 0.1 ──dormant── 0.2 ──low── 0.3 ──medium── 0.5 ──active── 0.7 ── hot/saturated
///   Removed      Ternary         Q2           Q4              Q8            BF16
/// ```
fn precision_from_config(score: f64, config: &CompactionConfig) -> HeadPrecision {
    if score < config.dead_threshold {
        HeadPrecision::Removed
    } else if score < config.dormant_threshold {
        HeadPrecision::Ternary
    } else if score < config.low_threshold {
        HeadPrecision::Q2
    } else if score < config.medium_threshold {
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
                HeadPrecision::Ternary => profile.ternary += 1,
                HeadPrecision::Q2 => profile.q2 += 1,
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
            let mut ternary = 0usize;
            let mut q2 = 0usize;
            let mut q4 = 0usize;
            let mut q8 = 0usize;
            let mut bf16 = 0usize;
            let mut saturated = 0usize;

            for precision in &topology.head_precisions {
                match precision {
                    HeadPrecision::Removed => {}
                    HeadPrecision::Ternary => ternary += 1,
                    HeadPrecision::Q2 => q2 += 1,
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

            let (min_s, max_s, sum_s) = head_scores
                .iter()
                .fold((f64::MAX, f64::MIN, 0.0), |(min, max, sum), &s| {
                    (min.min(s), max.max(s), sum + s)
                });

            let mean = if head_scores.is_empty() {
                0.0
            } else {
                sum_s / head_scores.len() as f64
            };

            LayerSummary {
                layer_index: layer_idx,
                heads_removed: removed,
                heads_ternary: ternary,
                heads_q2: q2,
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

/// Budget-aware optimization: fit a model within a target size in GB.
///
/// Instead of fixed thresholds, this allocates precision based on a memory budget.
/// Algorithm:
/// 1. Collect all (layer, head, utilization_score) tuples
/// 2. Sort by utilization descending (most important heads first)
/// 3. Greedily assign the highest affordable precision to each head
/// 4. If budget allows, all heads get BF16. If tight, low-utilization heads
///    get aggressively quantized or removed.
///
/// The result: the thresholds EMERGE from the budget rather than being hardcoded.
///
/// Parameters:
/// - `scores`: Raw utilization data from training
/// - `target_gb`: Target model size in GB (e.g., 20.0 for a 20GB budget)
/// - `total_non_attention_bytes`: Bytes for non-attention params (MLP, embeddings, etc.)
///   These are copied verbatim — only attention is compactable.
/// - `head_dim`: Dimension per attention head
/// - `hidden_size`: Model hidden dimension
/// - `config`: Base config for min_heads and GQA constraints
pub fn compute_budget_aware_plan(
    scores: &UtilizationData,
    target_gb: f64,
    total_non_attention_bytes: u64,
    head_dim: usize,
    hidden_size: usize,
    config: &CompactionConfig,
) -> Vec<LayerTopology> {
    let target_bytes = (target_gb * 1_073_741_824.0) as u64;
    let attention_budget = target_bytes.saturating_sub(total_non_attention_bytes);

    let num_layers = scores.layer_scores.len();
    let num_heads = scores.num_heads;
    let num_kv_heads = scores.num_kv_heads;
    let gqa_ratio = if num_kv_heads > 0 {
        num_heads / num_kv_heads
    } else {
        1
    };

    // Collect all heads with their scores
    let mut all_heads: Vec<(usize, usize, f64)> = Vec::new(); // (layer, head_idx, score)
    for (layer_idx, layer_scores) in scores.layer_scores.iter().enumerate() {
        for (head_idx, &score) in layer_scores.iter().enumerate() {
            all_heads.push((layer_idx, head_idx, score));
        }
    }

    // Sort by score descending — assign best precision to most important heads first
    all_heads.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    // Per-head attention parameter count:
    // Q head contributes: head_dim * hidden_size (Q proj rows) + hidden_size * head_dim (O proj cols)
    let params_per_q_head = head_dim * hidden_size * 2; // Q + O

    // KV parameters are shared across GQA group — account separately
    // K head: head_dim * hidden_size, V head: head_dim * hidden_size
    let params_per_kv_head = head_dim * hidden_size * 2; // K + V
                                                         // Total KV bytes at BF16 (we keep KV at BF16 for now — quantizing shared KV is risky)
    let kv_bytes_total = (num_kv_heads * num_layers * params_per_kv_head * 2) as u64; // BF16 = 2 bytes
    let q_budget = attention_budget.saturating_sub(kv_bytes_total);

    // Precision tiers in order of preference (most expensive first)
    let tiers: [(HeadPrecision, f64); 6] = [
        (HeadPrecision::BF16, 2.0),
        (HeadPrecision::Q8, 1.0),
        (HeadPrecision::Q4, 0.5),
        (HeadPrecision::Q2, 0.25),
        (HeadPrecision::Ternary, 0.2),
        (HeadPrecision::Removed, 0.0),
    ];

    // Initialize all heads as Removed
    let mut assignments: Vec<Vec<HeadPrecision>> =
        vec![vec![HeadPrecision::Removed; num_heads]; num_layers];
    let mut used_bytes: u64 = 0;

    // Greedily assign precision: iterate heads by importance, give best affordable tier
    for &(layer_idx, head_idx, _score) in &all_heads {
        let head_params = params_per_q_head as u64;

        let mut assigned = false;
        for &(tier, bytes_per_param) in &tiers {
            if tier == HeadPrecision::Removed {
                // Always affordable — leave as Removed
                break;
            }
            let tier_bytes = (head_params as f64 * bytes_per_param) as u64;
            if used_bytes + tier_bytes <= q_budget {
                assignments[layer_idx][head_idx] = tier;
                used_bytes += tier_bytes;
                assigned = true;
                break;
            }
        }
        if !assigned {
            assignments[layer_idx][head_idx] = HeadPrecision::Removed;
        }
    }

    // Apply GQA constraints and minimum head floors (same as threshold-based path)
    let mut result = Vec::with_capacity(num_layers);
    for layer_idx in 0..num_layers {
        let precisions = &mut assignments[layer_idx];

        // GQA: KV head survives if ANY Q head in its group is alive
        let kv_alive: Vec<bool> = (0..num_kv_heads)
            .map(|kv_idx| {
                let q_start = kv_idx * gqa_ratio;
                let q_end = ((kv_idx + 1) * gqa_ratio).min(num_heads);
                (q_start..q_end).any(|q| precisions[q] != HeadPrecision::Removed)
            })
            .collect();

        // Promote dead Q heads in alive KV groups
        for kv_idx in 0..num_kv_heads {
            let q_start = kv_idx * gqa_ratio;
            let q_end = ((kv_idx + 1) * gqa_ratio).min(num_heads);
            if kv_alive[kv_idx] {
                for q in q_start..q_end {
                    if precisions[q] == HeadPrecision::Removed {
                        precisions[q] = HeadPrecision::Ternary;
                    }
                }
            }
        }

        // Enforce minimum heads
        let alive_count = precisions
            .iter()
            .filter(|p| **p != HeadPrecision::Removed)
            .count();
        if alive_count < config.min_heads_per_layer {
            let mut candidates: Vec<(f64, usize)> = scores.layer_scores[layer_idx]
                .iter()
                .enumerate()
                .filter(|(i, _)| precisions[*i] == HeadPrecision::Removed)
                .map(|(i, &s)| (s, i))
                .collect();
            candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            for (_, idx) in candidates
                .iter()
                .take(config.min_heads_per_layer - alive_count)
            {
                precisions[*idx] = HeadPrecision::Ternary;
            }
        }

        // Build retained lists
        let retained_q: Vec<usize> = (0..num_heads)
            .filter(|&i| precisions[i] != HeadPrecision::Removed)
            .collect();
        let retained_kv: Vec<usize> = (0..num_kv_heads)
            .filter(|&kv_idx| {
                let q_start = kv_idx * gqa_ratio;
                let q_end = ((kv_idx + 1) * gqa_ratio).min(num_heads);
                (q_start..q_end).any(|q| precisions[q] != HeadPrecision::Removed)
            })
            .collect();

        let head_precisions: Vec<HeadPrecision> =
            retained_q.iter().map(|&i| precisions[i]).collect();
        let head_scores: Vec<f64> = retained_q
            .iter()
            .map(|&i| scores.layer_scores[layer_idx][i])
            .collect();

        result.push(LayerTopology {
            layer_index: layer_idx,
            num_heads: retained_q.len(),
            num_kv_heads: retained_kv.len(),
            retained_head_indices: retained_q,
            retained_kv_head_indices: retained_kv,
            head_precisions,
            head_scores,
        });
    }

    result
}

/// Estimate total non-attention bytes for a model.
///
/// These are parameters we can't compact (MLP, embeddings, layer norms).
/// For Llama/Qwen architectures:
/// - Embeddings: vocab_size * hidden_size * 2 (BF16)
/// - Per-layer MLP: 3 * hidden_size * intermediate_size * 2 (gate_proj, up_proj, down_proj)
/// - Per-layer norms: 2 * hidden_size * 2 (input_layernorm, post_attention_layernorm)
/// - Final norm: hidden_size * 2
/// - lm_head: vocab_size * hidden_size * 2 (often tied with embeddings)
pub fn estimate_non_attention_bytes(
    num_layers: usize,
    hidden_size: usize,
    intermediate_size: usize,
    vocab_size: usize,
) -> u64 {
    let bf16 = 2u64;
    let embeddings = vocab_size as u64 * hidden_size as u64 * bf16;
    let mlp_per_layer = 3 * hidden_size as u64 * intermediate_size as u64 * bf16;
    let norms_per_layer = 2 * hidden_size as u64 * bf16;
    let final_norm = hidden_size as u64 * bf16;
    let lm_head = vocab_size as u64 * hidden_size as u64 * bf16; // If not tied

    embeddings + (mlp_per_layer + norms_per_layer) * num_layers as u64 + final_norm + lm_head
}

#[cfg(test)]
mod tests;
