#![cfg(test)]
    use super::*;

    fn make_scores(
        layer_scores: Vec<Vec<f64>>,
        num_heads: usize,
        num_kv_heads: usize,
    ) -> UtilizationData {
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
        assert_eq!(
            HeadPrecision::from_utilization(0.05),
            HeadPrecision::Removed
        );
        assert_eq!(
            HeadPrecision::from_utilization(0.099),
            HeadPrecision::Removed
        );
    }

    #[test]
    fn test_precision_from_utilization_dormant() {
        assert_eq!(HeadPrecision::from_utilization(0.1), HeadPrecision::Ternary);
        assert_eq!(
            HeadPrecision::from_utilization(0.15),
            HeadPrecision::Ternary
        );
        assert_eq!(
            HeadPrecision::from_utilization(0.199),
            HeadPrecision::Ternary
        );
    }

    #[test]
    fn test_precision_from_utilization_low() {
        assert_eq!(HeadPrecision::from_utilization(0.2), HeadPrecision::Q2);
        assert_eq!(HeadPrecision::from_utilization(0.25), HeadPrecision::Q2);
        assert_eq!(HeadPrecision::from_utilization(0.299), HeadPrecision::Q2);
    }

    #[test]
    fn test_precision_from_utilization_medium() {
        assert_eq!(HeadPrecision::from_utilization(0.3), HeadPrecision::Q4);
        assert_eq!(HeadPrecision::from_utilization(0.4), HeadPrecision::Q4);
        assert_eq!(HeadPrecision::from_utilization(0.499), HeadPrecision::Q4);
    }

    #[test]
    fn test_precision_from_utilization_active() {
        assert_eq!(HeadPrecision::from_utilization(0.5), HeadPrecision::Q8);
        assert_eq!(HeadPrecision::from_utilization(0.6), HeadPrecision::Q8);
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
        assert_eq!(HeadPrecision::Ternary.bits(), 2);
        assert_eq!(HeadPrecision::Q2.bits(), 2);
        assert_eq!(HeadPrecision::Q4.bits(), 4);
        assert_eq!(HeadPrecision::Q8.bits(), 8);
        assert_eq!(HeadPrecision::BF16.bits(), 16);
    }

    #[test]
    fn test_precision_bits_effective() {
        assert!((HeadPrecision::Ternary.bits_effective() - 1.585).abs() < 0.001);
        assert!((HeadPrecision::Q2.bits_effective() - 2.0).abs() < 0.001);
        assert!((HeadPrecision::Q4.bits_effective() - 4.0).abs() < 0.001);
    }

    #[test]
    fn test_precision_bytes_per_param() {
        assert!((HeadPrecision::Ternary.bytes_per_param() - 0.2).abs() < 0.001);
        assert!((HeadPrecision::Q2.bytes_per_param() - 0.25).abs() < 0.001);
        assert!((HeadPrecision::Q4.bytes_per_param() - 0.5).abs() < 0.001);
        assert!((HeadPrecision::Q8.bytes_per_param() - 1.0).abs() < 0.001);
        assert!((HeadPrecision::BF16.bytes_per_param() - 2.0).abs() < 0.001);
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
        assert!(plan[0]
            .head_precisions
            .iter()
            .all(|p| *p == HeadPrecision::BF16));
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
        // 8 heads with varying utilization covering all tiers
        let scores = make_scores(
            vec![vec![0.05, 0.15, 0.25, 0.4, 0.55, 0.65, 0.8, 0.95]],
            8,
            8,
        );
        let plan = compute_optimization_plan(&scores, &default_config());

        let layer = &plan[0];
        // Head 0 (0.05) => Removed
        // Head 1 (0.15) => Ternary
        // Head 2 (0.25) => Q2
        // Head 3 (0.4)  => Q4
        // Head 4 (0.55) => Q8
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
        let scores = make_scores(vec![vec![0.01, 0.01, 0.5, 0.01, 0.01, 0.01]], 6, 2);
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
        let scores = make_scores(vec![vec![0.01, 0.01, 0.01, 0.8, 0.8, 0.8]], 6, 2);
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
        assert!(layer
            .head_precisions
            .iter()
            .any(|p| *p == HeadPrecision::BF16));
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
                vec![0.8, 0.8, 0.8, 0.8],    // Layer 0: all high
                vec![0.01, 0.01, 0.01, 0.8], // Layer 1: 3 dead, 1 high
            ],
            4,
            4,
        );
        let plan = compute_optimization_plan(&scores, &default_config());

        assert_eq!(plan[0].num_heads, 4); // All alive
                                          // Layer 1: 1 alive + min_heads floor (4 min) → resurrect 3
        assert_eq!(plan[1].num_heads, 4);
    }

    // --- Precision profile ---

    #[test]
    fn test_precision_profile_computation() {
        let layers = vec![LayerTopology {
            layer_index: 0,
            num_heads: 5,
            num_kv_heads: 5,
            retained_head_indices: vec![1, 2, 3, 4, 5],
            retained_kv_head_indices: vec![1, 2, 3, 4, 5],
            head_precisions: vec![
                HeadPrecision::Ternary,
                HeadPrecision::Q2,
                HeadPrecision::Q4,
                HeadPrecision::Q8,
                HeadPrecision::BF16,
            ],
            head_scores: vec![0.15, 0.25, 0.4, 0.6, 0.8],
        }];
        let profile = compute_precision_profile(&layers, 6, 1);
        assert_eq!(profile.removed, 1);
        assert_eq!(profile.ternary, 1);
        assert_eq!(profile.q2, 1);
        assert_eq!(profile.q4, 1);
        assert_eq!(profile.q8, 1);
        assert_eq!(profile.bf16, 1);
        assert!((profile.average_bits() - 6.317).abs() < 0.1); // (1.585+2+4+8+16)/5 = 31.585/5
    }

    // --- Saturated head detection ---

    #[test]
    fn test_find_saturated_heads() {
        let scores = make_scores(vec![vec![0.5, 0.95, 0.3, 0.92]], 4, 4);
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
        assert!(
            reduction > 0.49 && reduction < 0.51,
            "Expected ~0.5, got {}",
            reduction
        );
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
        let scores = make_scores(vec![vec![0.05, 0.15, 0.25, 0.4, 0.55, 0.8, 0.95]], 7, 7);
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
        assert_eq!(s.heads_ternary, 1); // Head 1 (0.15) ternary
        assert_eq!(s.heads_q2, 1); // Head 2 (0.25) q2
        assert_eq!(s.heads_q4, 1); // Head 3 (0.4) q4
        assert_eq!(s.heads_q8, 1); // Head 4 (0.55) q8
        assert!(s.heads_bf16 >= 1); // Heads 5,6 are BF16
        assert!(s.heads_saturated >= 1); // Head 6 (0.95) saturated
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

        // Case 2: Active utilization — moderately useful → Q8
        let scores = make_scores(vec![vec![0.55]], 1, 1);
        let plan = compute_optimization_plan(&scores, &config);
        assert_ne!(plan[0].head_precisions[0], HeadPrecision::Removed);
        assert_eq!(plan[0].head_precisions[0], HeadPrecision::Q8);

        // Case 3: Dormant but alive — near-zero cost → Ternary
        let scores = make_scores(vec![vec![0.15]], 1, 1);
        let plan = compute_optimization_plan(&scores, &config);
        assert_ne!(plan[0].head_precisions[0], HeadPrecision::Removed);
        assert_eq!(plan[0].head_precisions[0], HeadPrecision::Ternary);
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

        // At threshold: 0.1 → Ternary (alive!)
        let scores = make_scores(vec![vec![0.1]], 1, 1);
        let plan = compute_optimization_plan(&scores, &config);
        assert_eq!(plan[0].num_heads, 1);
        assert_eq!(plan[0].head_precisions[0], HeadPrecision::Ternary);
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
        // Test exact boundary behavior at each tier transition
        let config = CompactionConfig {
            min_heads_per_layer: 0,
            min_kv_heads_per_layer: 0,
            ..default_config()
        };

        // Boundary: dead_threshold (0.1)
        let scores = make_scores(vec![vec![0.099, 0.1]], 2, 2);
        let plan = compute_optimization_plan(&scores, &config);
        assert_eq!(plan[0].num_heads, 1); // 0.099 removed, 0.1 kept as Ternary

        // Boundary: dormant_threshold (0.2)
        let scores = make_scores(vec![vec![0.199, 0.2]], 2, 2);
        let plan = compute_optimization_plan(&scores, &config);
        assert_eq!(plan[0].head_precisions[0], HeadPrecision::Ternary); // 0.199
        assert_eq!(plan[0].head_precisions[1], HeadPrecision::Q2); // 0.2

        // Boundary: low_threshold (0.3)
        let scores = make_scores(vec![vec![0.299, 0.3]], 2, 2);
        let plan = compute_optimization_plan(&scores, &config);
        assert_eq!(plan[0].head_precisions[0], HeadPrecision::Q2); // 0.299
        assert_eq!(plan[0].head_precisions[1], HeadPrecision::Q4); // 0.3

        // Boundary: medium_threshold (0.5)
        let scores = make_scores(vec![vec![0.499, 0.5]], 2, 2);
        let plan = compute_optimization_plan(&scores, &config);
        assert_eq!(plan[0].head_precisions[0], HeadPrecision::Q4); // 0.499
        assert_eq!(plan[0].head_precisions[1], HeadPrecision::Q8); // 0.5

        // Boundary: high_threshold (0.7)
        let scores = make_scores(vec![vec![0.699, 0.7]], 2, 2);
        let plan = compute_optimization_plan(&scores, &config);
        assert_eq!(plan[0].head_precisions[0], HeadPrecision::Q8); // 0.699
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
            TestCase {
                score: 0.02,
                expected_alive: false,
                expected_precision: None,
            }, // Dead: no gradient flow
            TestCase {
                score: 0.08,
                expected_alive: false,
                expected_precision: None,
            }, // Dead: barely active
            TestCase {
                score: 0.12,
                expected_alive: true,
                expected_precision: Some(HeadPrecision::Ternary),
            }, // Dormant: 1.58-bit
            TestCase {
                score: 0.18,
                expected_alive: true,
                expected_precision: Some(HeadPrecision::Ternary),
            }, // Dormant: 1.58-bit
            TestCase {
                score: 0.25,
                expected_alive: true,
                expected_precision: Some(HeadPrecision::Q2),
            }, // Low: 2-bit
            TestCase {
                score: 0.35,
                expected_alive: true,
                expected_precision: Some(HeadPrecision::Q4),
            }, // Medium: 4-bit
            TestCase {
                score: 0.45,
                expected_alive: true,
                expected_precision: Some(HeadPrecision::Q4),
            }, // Medium: 4-bit
            TestCase {
                score: 0.55,
                expected_alive: true,
                expected_precision: Some(HeadPrecision::Q8),
            }, // Active: 8-bit
            TestCase {
                score: 0.65,
                expected_alive: true,
                expected_precision: Some(HeadPrecision::Q8),
            }, // Active: 8-bit
            TestCase {
                score: 0.75,
                expected_alive: true,
                expected_precision: Some(HeadPrecision::BF16),
            }, // Hot: full precision
            TestCase {
                score: 0.88,
                expected_alive: true,
                expected_precision: Some(HeadPrecision::BF16),
            }, // Hot: essential
            TestCase {
                score: 0.95,
                expected_alive: true,
                expected_precision: Some(HeadPrecision::BF16),
            }, // Saturated: overloaded
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
                    "Case {i}: score={} should be alive",
                    case.score
                );
                assert_eq!(
                    plan[0].head_precisions[0],
                    case.expected_precision.unwrap(),
                    "Case {i}: score={} wrong precision",
                    case.score
                );
            } else {
                assert_eq!(
                    plan[0].num_heads, 0,
                    "Case {i}: score={} should be dead",
                    case.score
                );
            }
        }
    }

    #[test]
    fn test_sentinel_mirror_custom_thresholds() {
        // Our system allows custom thresholds via CompactionConfig
        // Aggressive config: wider dead zone, faster path to full precision
        let aggressive = CompactionConfig {
            dead_threshold: 0.2,    // More aggressive pruning
            dormant_threshold: 0.3, // Wider ternary band
            low_threshold: 0.4,
            medium_threshold: 0.5,
            high_threshold: 0.6, // Faster path to full precision
            min_heads_per_layer: 0,
            min_kv_heads_per_layer: 0,
            ..default_config()
        };

        // Score 0.15 → dead with aggressive config (was Ternary with default)
        let scores = make_scores(vec![vec![0.15]], 1, 1);
        let plan = compute_optimization_plan(&scores, &aggressive);
        assert_eq!(plan[0].num_heads, 0); // Pruned with aggressive threshold

        // Score 0.35 → Q2 with aggressive config
        let scores = make_scores(vec![vec![0.35]], 1, 1);
        let plan = compute_optimization_plan(&scores, &aggressive);
        assert_eq!(plan[0].head_precisions[0], HeadPrecision::Q2);

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
                vec![
                    0.05, 0.8, 0.3, 0.6, 0.9, 0.1, 0.4, 0.7, 0.2, 0.95, 0.5, 0.85,
                ],
                vec![
                    0.1, 0.7, 0.4, 0.5, 0.8, 0.2, 0.3, 0.6, 0.15, 0.9, 0.45, 0.75,
                ],
                vec![
                    0.3, 0.6, 0.5, 0.4, 0.7, 0.35, 0.45, 0.55, 0.25, 0.8, 0.4, 0.65,
                ],
                vec![0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5],
                vec![
                    0.01, 0.01, 0.01, 0.01, 0.9, 0.9, 0.9, 0.9, 0.01, 0.01, 0.9, 0.9,
                ],
                vec![
                    0.7, 0.8, 0.9, 0.95, 0.3, 0.2, 0.1, 0.05, 0.5, 0.6, 0.4, 0.35,
                ],
            ],
            12,
            12,
        );

        let plan = compute_optimization_plan(&scores, &default_config());

        // 6 layers computed
        assert_eq!(plan.len(), 6);

        // Layer 0: head 0 (0.05) dead, head 5 (0.1) Q4, others alive
        assert!(!plan[0].retained_head_indices.contains(&0));
        assert!(plan[0].retained_head_indices.contains(&1)); // 0.8 → BF16

        // Layer 3: all at 0.5 → all Q8
        assert_eq!(plan[3].num_heads, 12);
        assert!(plan[3]
            .head_precisions
            .iter()
            .all(|p| *p == HeadPrecision::Q8));

        // Layer 4: 6 dead heads (0.01), 4 alive at 0.9 + 2 alive at 0.9
        // min_heads_per_layer = 4, so at least 4 survive
        assert!(plan[4].num_heads >= 4);
        // The 0.9 heads should all be BF16
        let bf16_count = plan[4]
            .head_precisions
            .iter()
            .filter(|p| **p == HeadPrecision::BF16)
            .count();
        assert!(
            bf16_count >= 4,
            "Should have at least 4 BF16 heads, got {}",
            bf16_count
        );

        // Layer 5: heads 6 (0.1) and 7 (0.05) should be dead/Ternary
        let head_7_in_retained = plan[5].retained_head_indices.contains(&7);
        if head_7_in_retained {
            // If resurrected by floor, should be at Ternary (minimum alive)
            let idx = plan[5]
                .retained_head_indices
                .iter()
                .position(|&x| x == 7)
                .unwrap();
            assert_eq!(plan[5].head_precisions[idx], HeadPrecision::Ternary);
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
        // Q head 0 promoted to Ternary (minimum alive), Q head 1 at BF16
        assert_eq!(plan[0].head_precisions[0], HeadPrecision::Ternary);
        assert_eq!(plan[0].head_precisions[1], HeadPrecision::BF16);
        // GQA ratio maintained
        assert_eq!(plan[0].num_heads % plan[0].num_kv_heads, 0);
    }

    // --- Budget-aware tests ---

    #[test]
    fn test_estimate_non_attention_bytes_qwen32b() {
        // Qwen 2.5 32B: 64 layers, hidden=5120, intermediate=27648, vocab=152064
        let bytes = estimate_non_attention_bytes(64, 5120, 27648, 152064);
        let gb = bytes as f64 / 1_073_741_824.0;
        // MLP dominates: 64 layers × 3 × 5120 × 27648 × 2 ≈ 51.6 GB
        // Embeddings: 152064 × 5120 × 2 ≈ 1.45 GB × 2 (embed + lm_head)
        assert!(
            gb > 50.0,
            "Non-attention should be >50GB for 32B, got {gb:.1}GB"
        );
        assert!(
            gb < 60.0,
            "Non-attention should be <60GB for 32B, got {gb:.1}GB"
        );
        eprintln!("Qwen 32B non-attention: {gb:.2}GB");
    }

    #[test]
    fn test_budget_aware_respects_target() {
        // 4 layers, 8 heads, 4 KV (2:1 GQA), head_dim=64, hidden=512
        let scores = make_scores(vec![vec![0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.2, 0.1]; 4], 8, 4);
        let config = CompactionConfig::default();

        // Non-attention bytes (small model)
        let non_attn = estimate_non_attention_bytes(4, 512, 1024, 1000);

        // Very tight budget: non-attention + minimal attention
        let tight_target_gb = (non_attn as f64 + 100_000.0) / 1_073_741_824.0;
        let plan = compute_budget_aware_plan(&scores, tight_target_gb, non_attn, 64, 512, &config);

        // With a very tight budget, most heads should be removed
        let total_retained: usize = plan.iter().map(|l| l.num_heads).sum();
        let total_possible = 4 * 8;
        assert!(
            total_retained < total_possible,
            "Tight budget should remove some heads: retained {total_retained}/{total_possible}"
        );
        eprintln!("Tight budget: retained {total_retained}/{total_possible} heads");
    }

    #[test]
    fn test_budget_aware_generous_keeps_all() {
        // Same model but with a generous budget
        let scores = make_scores(vec![vec![0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.2, 0.1]; 4], 8, 4);
        let config = CompactionConfig::default();
        let non_attn = estimate_non_attention_bytes(4, 512, 1024, 1000);

        // Very generous budget: 10GB for a tiny model
        let plan = compute_budget_aware_plan(&scores, 10.0, non_attn, 64, 512, &config);

        // Should keep all heads at high precision
        let total_retained: usize = plan.iter().map(|l| l.num_heads).sum();
        assert_eq!(
            total_retained,
            4 * 8,
            "Generous budget should keep all heads"
        );

        // Best heads should get BF16
        let bf16_count: usize = plan
            .iter()
            .flat_map(|l| &l.head_precisions)
            .filter(|p| **p == HeadPrecision::BF16)
            .count();
        assert!(bf16_count > 0, "Generous budget should assign some BF16");
    }

    #[test]
    fn test_budget_aware_prioritizes_high_utilization() {
        // 1 layer, 4 heads with different scores
        let scores = make_scores(vec![vec![0.9, 0.1, 0.5, 0.3]], 4, 4);
        let config = CompactionConfig {
            min_heads_per_layer: 1,
            ..CompactionConfig::default()
        };
        let non_attn = estimate_non_attention_bytes(1, 512, 1024, 1000);

        // Budget for ~2 heads at BF16
        let head_bytes = 64 * 512 * 2 * 2; // Q+O, BF16
        let kv_bytes = 4.0 * 64.0 * 512.0 * 2.0 * 2.0; // 4 KV heads, K+V, BF16
        let budget_gb = (non_attn as f64 + kv_bytes + head_bytes as f64 * 2.5) / 1_073_741_824.0;

        let plan = compute_budget_aware_plan(&scores, budget_gb, non_attn, 64, 512, &config);

        // Head 0 (score 0.9) should survive at highest precision
        assert!(
            plan[0].retained_head_indices.contains(&0),
            "Highest-scored head should be retained"
        );
        let head0_idx = plan[0]
            .retained_head_indices
            .iter()
            .position(|&i| i == 0)
            .unwrap();
        let head0_prec = &plan[0].head_precisions[head0_idx];

        // Head 1 (score 0.1) is least important
        // If it survived, should be at lower precision than head 0
        if plan[0].retained_head_indices.contains(&1) {
            let head1_idx = plan[0]
                .retained_head_indices
                .iter()
                .position(|&i| i == 1)
                .unwrap();
            let head1_prec = &plan[0].head_precisions[head1_idx];
            assert!(
                head0_prec.bits() >= head1_prec.bits(),
                "Higher-utilization head should get >= precision"
            );
        }
    }
