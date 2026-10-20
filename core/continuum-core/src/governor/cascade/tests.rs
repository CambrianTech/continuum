    use super::*;

    fn thresh() -> CascadeThresholds {
        CascadeThresholds::default()
    }

    // ===== Emergency: thermal Critical + battery <emergency =====

    /// What this catches: thermal Critical immediately jumps to MAX
    /// regardless of current step. Protects hardware from sustained
    /// thermal damage.
    #[test]
    fn thermal_critical_triggers_emergency_max() {
        for step in 0..=CASCADE_STEP_MAX {
            let action = evaluate_next_step(
                step,
                &PressureSignal::Thermal {
                    severity: ThermalSeverity::Critical,
                },
                &thresh(),
            );
            assert_eq!(
                action,
                CascadeAction::EmergencyAdvanceToMax,
                "step={step} should emergency-max on thermal Critical"
            );
        }
    }

    /// What this catches: battery below emergency_pct (default 10%)
    /// triggers EmergencyAdvanceToMax. Protects user from system
    /// shutdown mid-task.
    #[test]
    fn battery_below_emergency_triggers_emergency_max() {
        let action = evaluate_next_step(
            0,
            &PressureSignal::BatteryLow { remaining_pct: 9 },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::EmergencyAdvanceToMax);
    }

    /// What this catches: battery exactly at emergency_pct (10%) does
    /// NOT trigger emergency (boundary — < emergency, not <=).
    #[test]
    fn battery_at_emergency_pct_boundary_does_not_emergency() {
        let action = evaluate_next_step(
            0,
            &PressureSignal::BatteryLow { remaining_pct: 10 },
            &thresh(),
        );
        assert_ne!(action, CascadeAction::EmergencyAdvanceToMax);
    }

    // ===== Step 0 → Step 1 (speculation + queue + VRAM) =====

    /// What this catches: speculation miss rate > 0.5 at step 0
    /// triggers Advance. Spec table row 1.
    #[test]
    fn spec_miss_high_at_step_0_advances() {
        let action = evaluate_next_step(
            0,
            &PressureSignal::SpeculationMissRate { rate: 0.6 },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::Advance);
    }

    /// What this catches: speculation miss = 0.5 exactly doesn't advance
    /// (strict > threshold). Boundary test.
    #[test]
    fn spec_miss_at_threshold_doesnt_advance() {
        let action = evaluate_next_step(
            0,
            &PressureSignal::SpeculationMissRate { rate: 0.5 },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::Hold);
    }

    /// What this catches: inference queue depth > 16 triggers Advance.
    #[test]
    fn inference_queue_high_at_step_0_advances() {
        let action = evaluate_next_step(
            0,
            &PressureSignal::InferenceQueueDepth { depth: 17 },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::Advance);
    }

    /// What this catches: VRAM > 85% triggers Advance.
    #[test]
    fn vram_high_at_step_0_advances() {
        let action = evaluate_next_step(0, &PressureSignal::VRAMHigh { used_pct: 90 }, &thresh());
        assert_eq!(action, CascadeAction::Advance);
    }

    /// What this catches: VRAM at 85% (exactly threshold) does NOT
    /// advance. Boundary.
    #[test]
    fn vram_at_threshold_doesnt_advance() {
        let action = evaluate_next_step(0, &PressureSignal::VRAMHigh { used_pct: 85 }, &thresh());
        assert_eq!(action, CascadeAction::Hold);
    }

    // ===== Step 1 → Step 0 (retreat) =====

    /// What this catches: speculation miss < 0.3 at step 1 triggers
    /// Retreat. Hysteresis: advance was at 0.5, retreat at 0.3 — gap
    /// prevents oscillation around a single threshold.
    #[test]
    fn spec_miss_low_at_step_1_retreats() {
        let action = evaluate_next_step(
            1,
            &PressureSignal::SpeculationMissRate { rate: 0.2 },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::Retreat);
    }

    /// What this catches: speculation miss between retreat (0.3) and
    /// advance (0.5) thresholds → Hold. The hysteresis gap.
    #[test]
    fn spec_miss_in_hysteresis_gap_holds() {
        for rate in &[0.31, 0.40, 0.49] {
            let action = evaluate_next_step(
                1,
                &PressureSignal::SpeculationMissRate { rate: *rate },
                &thresh(),
            );
            assert_eq!(
                action,
                CascadeAction::Hold,
                "rate {rate} should Hold in gap"
            );
        }
    }

    /// What this catches: inference queue < 8 at step 1 retreats.
    #[test]
    fn inference_queue_low_at_step_1_retreats() {
        let action = evaluate_next_step(
            1,
            &PressureSignal::InferenceQueueDepth { depth: 5 },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::Retreat);
    }

    /// What this catches: VRAM < 70 at step 1 retreats.
    #[test]
    fn vram_low_at_step_1_retreats() {
        let action = evaluate_next_step(1, &PressureSignal::VRAMHigh { used_pct: 60 }, &thresh());
        assert_eq!(action, CascadeAction::Retreat);
    }

    // ===== Step 1 → Step 2 (advance on mem + thermal) =====

    /// What this catches: system mem > 85 at step 1 advances to step 2.
    /// Spec table row 2.
    #[test]
    fn system_mem_high_at_step_1_advances() {
        let action = evaluate_next_step(
            1,
            &PressureSignal::SystemMemHigh { used_pct: 90 },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::Advance);
    }

    /// What this catches: thermal Hot at step 1 advances to step 2.
    #[test]
    fn thermal_hot_at_step_1_advances() {
        let action = evaluate_next_step(
            1,
            &PressureSignal::Thermal {
                severity: ThermalSeverity::Hot,
            },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::Advance);
    }

    /// What this catches: thermal Warm or Cool at step 1 → Retreat
    /// (cascade can step down when thermal clears).
    #[test]
    fn thermal_warm_at_step_1_retreats() {
        for severity in &[ThermalSeverity::Warm, ThermalSeverity::Cool] {
            let action = evaluate_next_step(
                1,
                &PressureSignal::Thermal {
                    severity: *severity,
                },
                &thresh(),
            );
            assert_eq!(
                action,
                CascadeAction::Retreat,
                "severity={severity:?} should retreat"
            );
        }
    }

    // ===== Step 2 → Step 3 (advance on battery low) =====

    /// What this catches: battery < 15% at step 2 advances to step 3
    /// (NOT emergency — emergency is < 10%).
    #[test]
    fn battery_low_at_step_2_advances_not_emergency() {
        let action = evaluate_next_step(
            2,
            &PressureSignal::BatteryLow { remaining_pct: 12 },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::Advance);
    }

    /// What this catches: step 2 retreats on mem-clear.
    #[test]
    fn step_2_retreats_on_mem_clear() {
        let action = evaluate_next_step(
            2,
            &PressureSignal::SystemMemHigh { used_pct: 60 },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::Retreat);
    }

    // ===== Step 3, 4, 5 — battery + thermal retreat paths =====

    /// What this catches: battery > 25% at steps 3/4 retreats.
    #[test]
    fn battery_recovered_at_steps_3_and_4_retreats() {
        for step in &[3, 4] {
            let action = evaluate_next_step(
                *step,
                &PressureSignal::BatteryLow { remaining_pct: 30 },
                &thresh(),
            );
            assert_eq!(action, CascadeAction::Retreat, "step={step} should retreat");
        }
    }

    /// What this catches: at step 5 (max throttle), only Cool thermal
    /// retreats; Warm or Hot Holds. Strictest retreat condition.
    #[test]
    fn step_5_only_cool_thermal_retreats() {
        let cool = evaluate_next_step(
            5,
            &PressureSignal::Thermal {
                severity: ThermalSeverity::Cool,
            },
            &thresh(),
        );
        assert_eq!(cool, CascadeAction::Retreat);

        for non_cool in &[ThermalSeverity::Warm, ThermalSeverity::Hot] {
            let action = evaluate_next_step(
                5,
                &PressureSignal::Thermal {
                    severity: *non_cool,
                },
                &thresh(),
            );
            assert_eq!(
                action,
                CascadeAction::Hold,
                "severity={non_cool:?} at max step holds"
            );
        }
    }

    // ===== UserActive informational only =====

    /// What this catches: UserActive doesn't drive cascade transitions
    /// in PR-3c1 (signal exists for PR-3c2's user-foreground weighting
    /// but doesn't fire enter/exit).
    #[test]
    fn user_active_holds_at_every_step() {
        for step in 0..=CASCADE_STEP_MAX {
            for foreground in [true, false] {
                let action =
                    evaluate_next_step(step, &PressureSignal::UserActive { foreground }, &thresh());
                assert_eq!(
                    action,
                    CascadeAction::Hold,
                    "step={step} foreground={foreground} should Hold"
                );
            }
        }
    }

    // ===== apply_action =====

    /// What this catches: Hold doesn't move the step.
    #[test]
    fn apply_hold_keeps_step() {
        for step in 0..=CASCADE_STEP_MAX {
            assert_eq!(apply_action(step, CascadeAction::Hold), step);
        }
    }

    /// What this catches: Advance bumps by 1, capped at MAX.
    #[test]
    fn apply_advance_bumps_one_capped_at_max() {
        assert_eq!(apply_action(0, CascadeAction::Advance), 1);
        assert_eq!(apply_action(3, CascadeAction::Advance), 4);
        assert_eq!(
            apply_action(CASCADE_STEP_MAX, CascadeAction::Advance),
            CASCADE_STEP_MAX
        );
    }

    /// What this catches: Retreat drops by 1, saturated at MIN.
    #[test]
    fn apply_retreat_drops_one_saturated_at_min() {
        assert_eq!(apply_action(5, CascadeAction::Retreat), 4);
        assert_eq!(apply_action(1, CascadeAction::Retreat), 0);
        assert_eq!(apply_action(0, CascadeAction::Retreat), 0);
    }

    /// What this catches: EmergencyAdvanceToMax jumps from any step
    /// to MAX in one operation.
    #[test]
    fn apply_emergency_advances_to_max_from_any_step() {
        for step in 0..=CASCADE_STEP_MAX {
            assert_eq!(
                apply_action(step, CascadeAction::EmergencyAdvanceToMax),
                CASCADE_STEP_MAX,
                "step={step} should jump to MAX"
            );
        }
    }

    // ===== Determinism + serde =====

    /// What this catches: pure-function determinism. Same inputs →
    /// same output. PR-3c2 can rely on this for the wire-replay path.
    #[test]
    fn evaluate_is_deterministic() {
        let signal = PressureSignal::SpeculationMissRate { rate: 0.7 };
        let a = evaluate_next_step(0, &signal, &thresh());
        let b = evaluate_next_step(0, &signal, &thresh());
        assert_eq!(a, b);
    }

    /// What this catches: CascadeAction tagged-union round-trips with
    /// `kind` discriminator. PR-3c2 emits these via the trace bus +
    /// the wire shape must round-trip cleanly for replay/inspection.
    #[test]
    fn cascade_action_tagged_union_round_trips() {
        let actions = vec![
            CascadeAction::Hold,
            CascadeAction::Advance,
            CascadeAction::Retreat,
            CascadeAction::EmergencyAdvanceToMax,
        ];
        for a in &actions {
            let j = serde_json::to_string(a).unwrap();
            let back: CascadeAction = serde_json::from_str(&j).unwrap();
            assert_eq!(*a, back);
            assert!(j.contains("\"kind\":\""), "tag missing: {j}");
        }
    }

    /// What this catches: CascadeThresholds default values match the
    /// spec's §"Adjustment Cascade" table. If anyone tunes defaults
    /// without updating the spec, this test catches the drift.
    #[test]
    fn cascade_thresholds_defaults_match_spec_table() {
        let t = CascadeThresholds::default();
        // Spec row 1
        assert_eq!(t.spec_miss_rate_advance, 0.5);
        assert_eq!(t.spec_miss_rate_retreat, 0.3);
        assert_eq!(t.vram_used_pct_advance, 85);
        assert_eq!(t.vram_used_pct_retreat, 70);
        // Spec row 2
        assert_eq!(t.system_mem_used_pct_advance, 85);
        assert_eq!(t.system_mem_used_pct_retreat, 70);
        assert_eq!(t.thermal_advance, ThermalSeverity::Hot);
        // Spec row 3
        assert_eq!(t.battery_pct_advance, 15);
        assert_eq!(t.battery_pct_retreat, 25);
        assert_eq!(t.battery_pct_emergency, 10);
    }

    /// What this catches: emergency signals beat all other path
    /// evaluations. Even at step 0, thermal Critical jumps to MAX —
    /// no "first match wins" with a quieter step-0 path.
    #[test]
    fn emergency_signals_priority_over_step_evaluation() {
        let action = evaluate_next_step(
            0,
            &PressureSignal::Thermal {
                severity: ThermalSeverity::Critical,
            },
            &thresh(),
        );
        assert_eq!(action, CascadeAction::EmergencyAdvanceToMax);
    }

    // ===== apply_cascade_step_to_policy (PR-3c3) =====

    use crate::governor::types::{
        CadenceMultipliers, ConcurrencyCaps, ConsolidationSchedule, FederationCadence,
        GovernorPolicy, HardwareClass, PowerSource, RecallScoreWeights, SpeculationLevel,
        TargetSilicon, ThermalClass, TierSizes,
    };

    fn base_policy_5090() -> GovernorPolicy {
        // Approximation of the spec's 5090 anchor policy. Used as the
        // canonical base for cascade-step tests.
        GovernorPolicy {
            policy_version: 1,
            hardware_class: HardwareClass {
                silicon: TargetSilicon::NvidiaCuda,
                silicon_model: "RTX 5090".into(),
                vram_mb: 32 * 1024,
                system_ram_mb: 64 * 1024,
                power_source: PowerSource::Plugged,
                thermal_class: ThermalClass::Workstation,
                battery_pct: None,
                thermal_headroom_pct: None,
            },
            tier_sizes: TierSizes {
                l1_lora_layers: 8,
                l1_kv_tokens: 16384,
                l2_lora_layers: 16,
                l3_lora_layers: 40,
                l3_engrams: 10240,
            },
            cadence_multipliers: CadenceMultipliers {
                realtime: 1.0,
                delayed: 1.0,
                background: 1.5,
            },
            concurrency_caps: ConcurrencyCaps {
                personas_concurrent: 8,
                inference_lanes: 4,
                foundry_lanes: 1,
                sentinel_lanes: 2,
            },
            speculation_aggressiveness: SpeculationLevel::Aggressive,
            consolidation_schedule: ConsolidationSchedule::Idle,
            federation_pull_cadence: FederationCadence {
                pull_cadence_seconds: 60,
            },
            recall_score_weights: RecallScoreWeights {
                semantic: 0.4,
                outcome_history: 0.3,
                recency: 0.1,
                tier_proximity: 0.1,
                provenance_trust: 0.1,
            },
            cascade_step: 0,
            committed_at_ms: 1000,
        }
    }

    /// What this catches: step 0 == base (cascade unchanged, no
    /// throttling applied). Identity case — pinning that the function
    /// doesn't accidentally modify the base policy when step=0.
    #[test]
    fn apply_step_0_equals_base_except_cascade_step() {
        let base = base_policy_5090();
        let after = apply_cascade_step_to_policy(&base, 0);
        assert_eq!(after.cascade_step, 0);
        assert_eq!(
            after.speculation_aggressiveness,
            base.speculation_aggressiveness
        );
        assert_eq!(
            after.concurrency_caps.personas_concurrent,
            base.concurrency_caps.personas_concurrent
        );
        assert_eq!(
            after.tier_sizes.l1_lora_layers,
            base.tier_sizes.l1_lora_layers
        );
        assert_eq!(after.consolidation_schedule, base.consolidation_schedule);
    }

    /// What this catches: step 1 drops speculation by one notch.
    /// Aggressive → Balanced (then Balanced → Conservative, Conservative
    /// → Off via separate base policies in the next test).
    #[test]
    fn apply_step_1_drops_speculation_aggressive_to_balanced() {
        let base = base_policy_5090();
        assert_eq!(
            base.speculation_aggressiveness,
            SpeculationLevel::Aggressive
        );
        let after = apply_cascade_step_to_policy(&base, 1);
        assert_eq!(after.cascade_step, 1);
        assert_eq!(after.speculation_aggressiveness, SpeculationLevel::Balanced);
    }

    /// What this catches: speculation drop ladder covers every variant.
    /// Aggressive→Balanced, Balanced→Conservative, Conservative→Off,
    /// Off→Off (already minimum).
    #[test]
    fn apply_step_1_speculation_drops_one_notch_per_variant() {
        for (input, expected) in &[
            (SpeculationLevel::Aggressive, SpeculationLevel::Balanced),
            (SpeculationLevel::Balanced, SpeculationLevel::Conservative),
            (SpeculationLevel::Conservative, SpeculationLevel::Off),
            (SpeculationLevel::Off, SpeculationLevel::Off),
        ] {
            let mut base = base_policy_5090();
            base.speculation_aggressiveness = *input;
            let after = apply_cascade_step_to_policy(&base, 1);
            assert_eq!(
                after.speculation_aggressiveness, *expected,
                "from {input:?} should drop to {expected:?}"
            );
        }
    }

    /// What this catches: step 2 personas_concurrent decreases by 1
    /// (5090 has 8 → step 2 = 7). Cumulative with step 1's speculation
    /// drop.
    #[test]
    fn apply_step_2_drops_personas_concurrent_and_keeps_speculation_drop() {
        let base = base_policy_5090();
        let after = apply_cascade_step_to_policy(&base, 2);
        assert_eq!(after.cascade_step, 2);
        assert_eq!(after.concurrency_caps.personas_concurrent, 7); // 8 - 1
                                                                   // Cumulative: step 1's speculation drop still applies
        assert_eq!(after.speculation_aggressiveness, SpeculationLevel::Balanced);
    }

    /// What this catches: step 2 personas_concurrent floor at 1.
    /// Defensive — a base with 1 persona shouldn't go to 0 (kills the
    /// inference pool entirely).
    #[test]
    fn apply_step_2_personas_concurrent_floor_at_one() {
        let mut base = base_policy_5090();
        base.concurrency_caps.personas_concurrent = 1;
        let after = apply_cascade_step_to_policy(&base, 2);
        assert_eq!(after.concurrency_caps.personas_concurrent, 1);
    }

    /// What this catches: step 2 stretches non-realtime cadence
    /// multipliers to at least 2.0. Realtime stays unchanged.
    #[test]
    fn apply_step_2_stretches_non_realtime_cadence() {
        let base = base_policy_5090();
        let after = apply_cascade_step_to_policy(&base, 2);
        assert_eq!(
            after.cadence_multipliers.realtime,
            base.cadence_multipliers.realtime
        );
        assert!(after.cadence_multipliers.delayed >= 2.0);
        assert!(after.cadence_multipliers.background >= 2.0);
    }

    /// What this catches: step 2 doesn't SHRINK already-stretched
    /// cadence multipliers. If base already has background = 3.0, step
    /// 2 keeps 3.0 (uses max).
    #[test]
    fn apply_step_2_doesnt_shrink_already_stretched_cadence() {
        let mut base = base_policy_5090();
        base.cadence_multipliers.background = 3.0;
        let after = apply_cascade_step_to_policy(&base, 2);
        assert_eq!(after.cadence_multipliers.background, 3.0);
    }

    /// What this catches: step 3 shrinks l1_lora_layers + l1_kv_tokens
    /// by ~25%. 8 * 0.75 = 6. 16384 * 0.75 = 12288.
    #[test]
    fn apply_step_3_shrinks_l1_25_percent() {
        let base = base_policy_5090();
        let after = apply_cascade_step_to_policy(&base, 3);
        assert_eq!(after.cascade_step, 3);
        assert_eq!(after.tier_sizes.l1_lora_layers, 6); // 8 * 0.75
        assert_eq!(after.tier_sizes.l1_kv_tokens, 12288); // 16384 * 0.75
                                                          // L2/L3 untouched at step 3
        assert_eq!(
            after.tier_sizes.l2_lora_layers,
            base.tier_sizes.l2_lora_layers
        );
    }

    /// What this catches: l1 floor at 1 when base is already small.
    /// 1 * 0.75 = 0.75 → floor 0 → max(0, 1) = 1.
    #[test]
    fn apply_step_3_l1_floors_at_one() {
        let mut base = base_policy_5090();
        base.tier_sizes.l1_lora_layers = 1;
        base.tier_sizes.l1_kv_tokens = 1;
        let after = apply_cascade_step_to_policy(&base, 3);
        assert_eq!(after.tier_sizes.l1_lora_layers, 1);
        assert_eq!(after.tier_sizes.l1_kv_tokens, 1);
    }

    /// What this catches: step 4 federation cadence = max
    /// (MAX_FEDERATION_PULL_CADENCE_SECONDS). Slows pulls to once-
    /// per-hour under sustained pressure.
    #[test]
    fn apply_step_4_maxes_federation_cadence() {
        let base = base_policy_5090();
        assert_eq!(base.federation_pull_cadence.pull_cadence_seconds, 60);
        let after = apply_cascade_step_to_policy(&base, 4);
        assert_eq!(after.cascade_step, 4);
        assert_eq!(
            after.federation_pull_cadence.pull_cadence_seconds,
            MAX_FEDERATION_PULL_CADENCE_SECONDS
        );
    }

    /// What this catches: step 5 consolidation = Manual. Suspends
    /// automatic consolidation under maximum pressure (operator must
    /// explicitly trigger; substrate stops doing it on its own).
    #[test]
    fn apply_step_5_consolidation_manual() {
        let base = base_policy_5090();
        assert_eq!(base.consolidation_schedule, ConsolidationSchedule::Idle);
        let after = apply_cascade_step_to_policy(&base, 5);
        assert_eq!(after.cascade_step, 5);
        assert_eq!(after.consolidation_schedule, ConsolidationSchedule::Manual);
    }

    /// What this catches: step 5 is CUMULATIVE — all prior step
    /// transformations also applied. Speculation dropped + personas
    /// reduced + tier_sizes shrunk + federation maxed + consolidation
    /// Manual. The full-throttle state.
    #[test]
    fn apply_step_5_cumulative_all_transformations() {
        let base = base_policy_5090();
        let after = apply_cascade_step_to_policy(&base, 5);
        // Step 1
        assert_eq!(after.speculation_aggressiveness, SpeculationLevel::Balanced);
        // Step 2
        assert_eq!(after.concurrency_caps.personas_concurrent, 7);
        assert!(after.cadence_multipliers.delayed >= 2.0);
        // Step 3
        assert_eq!(after.tier_sizes.l1_lora_layers, 6);
        // Step 4
        assert_eq!(
            after.federation_pull_cadence.pull_cadence_seconds,
            MAX_FEDERATION_PULL_CADENCE_SECONDS
        );
        // Step 5
        assert_eq!(after.consolidation_schedule, ConsolidationSchedule::Manual);
    }

    /// What this catches: step value > MAX is clamped to MAX. Defensive
    /// against caller bugs (passes 7 instead of 5).
    #[test]
    fn apply_step_above_max_clamps_to_max() {
        let base = base_policy_5090();
        let after = apply_cascade_step_to_policy(&base, 99);
        assert_eq!(after.cascade_step, CASCADE_STEP_MAX);
        // Should have all step-5 transformations
        assert_eq!(after.consolidation_schedule, ConsolidationSchedule::Manual);
    }

    /// What this catches: pure-function determinism. Same inputs →
    /// same output. Tests pin this so the caller can cache the
    /// transformation result if the (base_policy, step) tuple is stable.
    #[test]
    fn apply_cascade_step_is_deterministic() {
        let base = base_policy_5090();
        let a = apply_cascade_step_to_policy(&base, 3);
        let b = apply_cascade_step_to_policy(&base, 3);
        assert_eq!(a, b);
    }

    /// What this catches: applying step N then step 0 to the result
    /// does NOT restore base — the step transformations are NOT
    /// reversible from a transformed policy. Caller MUST keep the
    /// original base + re-apply step 0 from it (which is what the
    /// LocalSubstrateGovernor does — stores base separately from
    /// active).
    #[test]
    fn apply_cascade_step_not_reversible_via_step_0_on_transformed() {
        let base = base_policy_5090();
        let throttled = apply_cascade_step_to_policy(&base, 3);
        let reset_attempt = apply_cascade_step_to_policy(&throttled, 0);
        // step is 0 again
        assert_eq!(reset_attempt.cascade_step, 0);
        // But tier_sizes is STILL shrunk (step 0 doesn't undo step 3's
        // shrink — it just doesn't re-apply it from a now-shrunk base).
        assert_eq!(
            reset_attempt.tier_sizes.l1_lora_layers, throttled.tier_sizes.l1_lora_layers,
            "step 0 from transformed policy ≠ base; caller MUST hold base separately"
        );
    }

    /// What this catches: MAX_FEDERATION_PULL_CADENCE_SECONDS const
    /// is the spec's max-cadence value. Drift catcher — if someone
    /// tunes this without updating the spec, test fails.
    #[test]
    fn max_federation_cadence_const_pinned() {
        assert_eq!(MAX_FEDERATION_PULL_CADENCE_SECONDS, 3600);
    }
