#![cfg(test)]
    use super::*;
    use crate::governor::policy_file::{
        CadenceMultipliersFile, ConcurrencyCapsFile, ConsolidationFileSection,
        FederationCadenceFile, PolicyFile, RecallScoreWeightsFile, SpeculationFileSection,
        TierSizesFile,
    };
    use crate::governor::types::{
        CadenceMultipliers, ConcurrencyCaps, ConsolidationSchedule, FederationCadence,
        HardwareClass, PowerSource, RecallScoreWeights, SpeculationLevel, TargetSilicon,
        ThermalClass, ThermalSeverity, TierSizes,
    };

    fn hw(
        silicon: TargetSilicon,
        thermal: ThermalClass,
        vram_mb: u64,
        ram_mb: u64,
    ) -> HardwareClass {
        HardwareClass {
            silicon,
            silicon_model: "test".into(),
            vram_mb,
            system_ram_mb: ram_mb,
            power_source: PowerSource::Plugged,
            thermal_class: thermal,
            battery_pct: None,
            thermal_headroom_pct: None,
        }
    }

    fn pol(applies_to: &str, l1_lora_layers: u32) -> PolicyFile {
        PolicyFile {
            policy_version: 1,
            applies_to: applies_to.into(),
            tier_sizes: TierSizesFile {
                l1_lora_layers,
                l1_kv_tokens: 2048,
                l2_lora_layers: 4,
                l3_lora_layers: 12,
                l3_engrams: 1024,
            },
            cadence_multipliers: CadenceMultipliersFile {
                realtime: 1.0,
                delayed: 1.0,
                background: 1.0,
            },
            concurrency_caps: ConcurrencyCapsFile {
                personas_concurrent: 1,
                inference_lanes: 1,
                foundry_lanes: 0,
                sentinel_lanes: 1,
            },
            speculation: SpeculationFileSection {
                level: SpeculationLevel::Conservative,
            },
            consolidation: ConsolidationFileSection {
                schedule: ConsolidationSchedule::Manual,
            },
            federation: FederationCadenceFile {
                pull_cadence_seconds: 600,
            },
            recall_weights: RecallScoreWeightsFile {
                semantic: 0.4,
                outcome_history: 0.3,
                recency: 0.1,
                tier_proximity: 0.1,
                provenance_trust: 0.1,
            },
        }
    }

    fn initial_policy() -> GovernorPolicy {
        GovernorPolicy {
            policy_version: 0,
            hardware_class: hw(TargetSilicon::None, ThermalClass::Workstation, 0, 0),
            tier_sizes: TierSizes {
                l1_lora_layers: 1,
                l1_kv_tokens: 256,
                l2_lora_layers: 1,
                l3_lora_layers: 1,
                l3_engrams: 1,
            },
            cadence_multipliers: CadenceMultipliers {
                realtime: 1.0,
                delayed: 1.0,
                background: 1.0,
            },
            concurrency_caps: ConcurrencyCaps {
                personas_concurrent: 1,
                inference_lanes: 1,
                foundry_lanes: 0,
                sentinel_lanes: 1,
            },
            speculation_aggressiveness: SpeculationLevel::Off,
            consolidation_schedule: ConsolidationSchedule::Manual,
            federation_pull_cadence: FederationCadence {
                pull_cadence_seconds: 0,
            },
            recall_score_weights: RecallScoreWeights {
                semantic: 0.4,
                outcome_history: 0.3,
                recency: 0.1,
                tier_proximity: 0.1,
                provenance_trust: 0.1,
            },
            cascade_step: 0,
            committed_at_ms: 0,
        }
    }

    // ===== construction =====

    /// What this catches: new() with an initial policy lets
    /// current_policy() return that policy immediately. Smoke test —
    /// governor is ready to serve reads from boot.
    #[test]
    fn new_serves_initial_policy_immediately() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        let p = g.current_policy();
        assert_eq!(p.policy_version, 0);
        assert_eq!(p.hardware_class.silicon, TargetSilicon::None);
    }

    /// What this catches: candidate_count starts at 0 + grows when
    /// set_candidates is called. Defensive — file-watcher (PR-3d) needs
    /// this introspection to verify it loaded files.
    #[test]
    fn candidate_count_reflects_set_candidates() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        assert_eq!(g.candidate_count(), 0);
        g.set_candidates(vec![pol("apple-m", 2), pol("nvidia", 4)]);
        assert_eq!(g.candidate_count(), 2);
        g.set_candidates(vec![]);
        assert_eq!(g.candidate_count(), 0);
    }

    // ===== on_hardware_detected =====

    /// What this catches: on_hardware_detected with a matching
    /// candidate publishes a new policy via arc_swap. The new policy
    /// reflects the matched candidate's tier_sizes (l1_lora_layers=2
    /// for M-Air pol).
    #[test]
    fn on_hardware_detected_publishes_matching_policy() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        g.set_candidates(vec![
            pol(
                "apple-m,thinandlight,uma,vram_mb=0..0,ram_mb=14000..18000",
                2,
            ),
            pol("nvidia,workstation,vram_mb=30000..36000", 8),
        ]);
        let m2_air = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        g.on_hardware_detected(m2_air.clone())
            .expect("matching M-Air policy should publish");
        let p = g.current_policy();
        assert_eq!(p.tier_sizes.l1_lora_layers, 2, "matched M-Air l1_lora=2");
        assert_eq!(p.hardware_class.silicon, TargetSilicon::AppleM);
    }

    /// What this catches: try_hardware_detected returns the typed
    /// error when no candidate matches. Caller path that wants the
    /// failure-mode info.
    #[test]
    fn try_hardware_detected_returns_no_matching_policy_err() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        g.set_candidates(vec![pol("nvidia,workstation,vram_mb=30000..36000", 8)]);
        let m2_air = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        let result = g.try_hardware_detected(m2_air);
        assert!(matches!(
            result,
            Err(PolicySelectionError::NoMatchingPolicy { .. })
        ));
    }

    /// What this catches: on_hardware_detected with NO matching
    /// candidate returns a typed error and leaves the previous policy
    /// IN PLACE. Defensive — a misconfigured policy dir shouldn't wipe
    /// out the governor's running state.
    #[test]
    fn on_hardware_detected_no_match_keeps_previous_policy() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        g.set_candidates(vec![pol("nvidia,workstation,vram_mb=30000..36000", 8)]);
        let m2_air = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        let result = g.on_hardware_detected(m2_air);
        assert!(matches!(
            result,
            Err(PolicySelectionError::NoMatchingPolicy { .. })
        ));
        // Policy should still be the initial one (version 0)
        assert_eq!(g.current_policy().policy_version, 0);
    }

    /// What this catches: on_hardware_detected with empty candidates
    /// returns a typed error and leaves the policy intact. First-boot
    /// before file watcher loads anything = explicit failure + governor
    /// still serves the last committed policy.
    #[test]
    fn on_hardware_detected_empty_candidates_returns_error() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        let m2_air = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        let result = g.on_hardware_detected(m2_air);
        assert!(matches!(
            result,
            Err(PolicySelectionError::NoMatchingPolicy { .. })
        ));
        assert_eq!(g.current_policy().policy_version, 0);
    }

    /// What this catches: successive on_hardware_detected calls
    /// successfully republish. Multiple hardware-change events should
    /// each result in a published policy if a match is found.
    #[test]
    fn successive_hardware_detected_publishes_multiple_times() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        g.set_candidates(vec![
            pol(
                "apple-m,thinandlight,uma,vram_mb=0..0,ram_mb=14000..18000",
                2,
            ),
            pol("nvidia,workstation,vram_mb=30000..36000", 8),
        ]);

        let m2_air = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        g.on_hardware_detected(m2_air)
            .expect("M-Air policy should publish");
        assert_eq!(g.current_policy().tier_sizes.l1_lora_layers, 2);

        let blackwell = hw(
            TargetSilicon::NvidiaCuda,
            ThermalClass::Workstation,
            32 * 1024,
            64 * 1024,
        );
        g.on_hardware_detected(blackwell)
            .expect("Blackwell policy should publish");
        assert_eq!(g.current_policy().tier_sizes.l1_lora_layers, 8);
    }

    // ===== on_pressure_signal =====

    /// What this catches: on_pressure_signal records the signal in
    /// snapshot.recent_signals. PR-3b doesn't react to thresholds yet
    /// (PR-3c does), but it must record.
    #[test]
    fn on_pressure_signal_records_signal() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        g.on_pressure_signal(PressureSignal::Thermal {
            severity: ThermalSeverity::Hot,
        });
        let snap = g.snapshot();
        assert_eq!(snap.recent_signals.len(), 1);
        assert!(matches!(
            snap.recent_signals[0],
            PressureSignal::Thermal {
                severity: ThermalSeverity::Hot
            }
        ));
    }

    /// What this catches: recent_signals ring eviction at capacity.
    /// Pushing CAPACITY+1 signals retains the most recent CAPACITY.
    #[test]
    fn recent_signals_capped_at_capacity() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        for i in 0..(RECENT_SIGNALS_CAPACITY + 5) {
            g.on_pressure_signal(PressureSignal::InferenceQueueDepth { depth: i as u32 });
        }
        let snap = g.snapshot();
        assert_eq!(snap.recent_signals.len(), RECENT_SIGNALS_CAPACITY);
        // The OLDEST 5 (depth 0..4) should have been evicted; depth 5..36
        // should remain.
        match snap.recent_signals[0] {
            PressureSignal::InferenceQueueDepth { depth } => {
                assert_eq!(depth, 5, "front should be depth=5 after 5 evictions");
            }
            other => panic!("expected InferenceQueueDepth, got {other:?}"),
        }
    }

    // ===== snapshot =====

    /// What this catches: snapshot returns the current policy + the
    /// transition count + recent_signals. Telemetry consumer reads
    /// this for VDD reports.
    #[test]
    fn snapshot_includes_policy_and_signals() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        g.set_candidates(vec![pol(
            "apple-m,thinandlight,uma,vram_mb=0..0,ram_mb=14000..18000",
            2,
        )]);
        g.on_hardware_detected(hw(
            TargetSilicon::AppleM,
            ThermalClass::ThinAndLight,
            0,
            16384,
        ))
        .expect("M-Air policy should publish");
        g.on_pressure_signal(PressureSignal::Thermal {
            severity: ThermalSeverity::Warm,
        });

        let snap = g.snapshot();
        assert_eq!(snap.current_policy.tier_sizes.l1_lora_layers, 2);
        assert_eq!(
            snap.cascade_transition_count, 1,
            "1 publish from on_hardware_detected"
        );
        assert_eq!(snap.recent_signals.len(), 1);
    }

    /// What this catches: cascade_transition_count starts at 0 +
    /// increments per publish. Verifies the bump in publish().
    #[test]
    fn cascade_transition_count_increments_per_publish() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        g.set_candidates(vec![
            pol(
                "apple-m,thinandlight,uma,vram_mb=0..0,ram_mb=14000..18000",
                2,
            ),
            pol("nvidia,workstation,vram_mb=30000..36000", 8),
        ]);
        assert_eq!(g.snapshot().cascade_transition_count, 0);

        g.on_hardware_detected(hw(
            TargetSilicon::AppleM,
            ThermalClass::ThinAndLight,
            0,
            16384,
        ))
        .expect("M-Air policy should publish");
        assert_eq!(g.snapshot().cascade_transition_count, 1);

        g.on_hardware_detected(hw(
            TargetSilicon::NvidiaCuda,
            ThermalClass::Workstation,
            32 * 1024,
            64 * 1024,
        ))
        .expect("Blackwell policy should publish");
        assert_eq!(g.snapshot().cascade_transition_count, 2);
    }

    /// What this catches: cascade_transition_count does NOT increment
    /// when on_hardware_detected fails to find a match (policy unchanged
    /// = no publish = no transition). Important — operators should see
    /// 0 if their files don't match anything, not a phantom count.
    #[test]
    fn cascade_transition_count_unchanged_on_no_match() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        g.set_candidates(vec![pol("nvidia,workstation,vram_mb=30000..36000", 8)]);
        let result = g.on_hardware_detected(hw(
            TargetSilicon::AppleM,
            ThermalClass::ThinAndLight,
            0,
            16384,
        ));
        assert!(matches!(
            result,
            Err(PolicySelectionError::NoMatchingPolicy { .. })
        ));
        assert_eq!(g.snapshot().cascade_transition_count, 0);
    }

    /// What this catches (UPDATED in PR-3c2): on_pressure_signal NOW
    /// drives transitions via the cascade evaluator. Thermal Critical
    /// is an emergency signal — jumps cascade_step to MAX (5)
    /// regardless of time-in-step. transition_count increments by 1
    /// (one publish from step 0 → step 5).
    #[test]
    fn pressure_signal_thermal_critical_emergency_advances() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        g.on_pressure_signal(PressureSignal::Thermal {
            severity: ThermalSeverity::Critical,
        });
        let snap = g.snapshot();
        assert_eq!(snap.cascade_transition_count, 1);
        assert_eq!(
            snap.current_policy.cascade_step, 5,
            "thermal Critical → EmergencyAdvanceToMax (step 5)"
        );
        assert_eq!(g.current_cascade_step(), 5);
    }

    /// What this catches: from step 0, a single signal exceeding the
    /// step-0 → step-1 threshold advances to step 1 immediately. No
    /// time-in-step gate for step 0 → step 1 (per spec — brief spikes
    /// CAN enter step 1, gate applies to step 1 → 2 and beyond).
    #[test]
    fn pressure_signal_first_advance_no_gate() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        g.on_pressure_signal(PressureSignal::SpeculationMissRate { rate: 0.7 });
        assert_eq!(
            g.current_cascade_step(),
            1,
            "step 0 → 1 advance fires immediately"
        );
    }

    /// What this catches: from step 1, a second-stage-triggering
    /// signal arriving in < MIN_TIME_IN_STEP_MS is HELD (downgraded
    /// from Advance to Hold). Brief spikes don't escalate.
    #[test]
    fn pressure_signal_step_1_to_2_gated_by_time_in_step() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        // Advance to step 1
        g.on_pressure_signal(PressureSignal::SpeculationMissRate { rate: 0.7 });
        assert_eq!(g.current_cascade_step(), 1);
        // Immediately try to advance to step 2 — should be HELD
        g.on_pressure_signal(PressureSignal::SystemMemHigh { used_pct: 95 });
        assert_eq!(
            g.current_cascade_step(),
            1,
            "step 1 → 2 advance within MIN_TIME_IN_STEP_MS should be Held"
        );
    }

    /// What this catches: EmergencyAdvanceToMax bypasses the time-in-step
    /// gate. Even if step 1 was entered 1ms ago, thermal Critical jumps
    /// to step 5 immediately. Protects hardware.
    #[test]
    fn emergency_bypasses_time_in_step_gate() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        g.on_pressure_signal(PressureSignal::SpeculationMissRate { rate: 0.7 });
        assert_eq!(g.current_cascade_step(), 1);
        // Emergency immediately after — should jump to 5 not Hold
        g.on_pressure_signal(PressureSignal::Thermal {
            severity: ThermalSeverity::Critical,
        });
        assert_eq!(
            g.current_cascade_step(),
            5,
            "emergency bypasses time-in-step gate"
        );
    }

    /// What this catches: Retreat is NOT gated by time-in-step. Cascade
    /// can retreat as soon as conditions clear (per spec — the hysteresis
    /// gap IS the anti-oscillation; doubling-up with a time gate would
    /// over-throttle).
    #[test]
    fn retreat_not_gated_by_time_in_step() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        g.on_pressure_signal(PressureSignal::SpeculationMissRate { rate: 0.7 });
        assert_eq!(g.current_cascade_step(), 1);
        // Retreat immediately — should fire even though step 1 was just entered
        g.on_pressure_signal(PressureSignal::SpeculationMissRate { rate: 0.1 });
        assert_eq!(
            g.current_cascade_step(),
            0,
            "retreat fires regardless of time-in-step"
        );
    }

    /// What this catches: cascade_step changes on signal-driven
    /// transitions DO publish a new policy (policy_version bumps,
    /// committed_at_ms updates, cascade_step is the new value).
    #[test]
    fn signal_driven_transition_publishes_new_policy() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        let before = g.current_policy();
        assert_eq!(before.cascade_step, 0);
        let before_version = before.policy_version;

        g.on_pressure_signal(PressureSignal::SpeculationMissRate { rate: 0.7 });

        let after = g.current_policy();
        assert_eq!(after.cascade_step, 1);
        assert!(after.policy_version > before_version);
        assert!(after.committed_at_ms >= before.committed_at_ms);
    }

    /// What this catches: signals that don't trigger transitions
    /// (e.g. UserActive) do NOT publish a new policy. The
    /// recent_signals ring still records, but cascade_transition_count
    /// stays.
    #[test]
    fn non_transitioning_signals_dont_publish() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        let before_transitions = g.snapshot().cascade_transition_count;
        g.on_pressure_signal(PressureSignal::UserActive { foreground: true });
        let after_transitions = g.snapshot().cascade_transition_count;
        assert_eq!(
            after_transitions, before_transitions,
            "UserActive doesn't transition"
        );
        assert_eq!(
            g.snapshot().recent_signals.len(),
            1,
            "but signal IS recorded"
        );
    }

    // ===== PR-3c4: apply_cascade_step_to_policy wiring + base/active split =====

    /// What this catches: cascade Advance derives active policy from
    /// BASE via apply_cascade_step_to_policy. Active policy after step
    /// 1 has speculation_aggressiveness dropped (per PR-3c3 table).
    #[test]
    fn advance_derives_active_from_base_with_step_transformations() {
        let mut base = initial_policy();
        base.speculation_aggressiveness = SpeculationLevel::Aggressive;
        let g = LocalSubstrateGovernor::new(base);

        g.on_pressure_signal(PressureSignal::SpeculationMissRate { rate: 0.7 });

        let active = g.current_policy();
        assert_eq!(active.cascade_step, 1);
        // Step 1 drops speculation: Aggressive → Balanced
        assert_eq!(
            active.speculation_aggressiveness,
            SpeculationLevel::Balanced
        );
    }

    /// What this catches: emergency-advance-to-max derives active
    /// from base at step 5 — all per-step transformations cumulative.
    /// tier_sizes l1 shrunk, federation cadence maxed, consolidation
    /// Manual. The full-throttle state.
    #[test]
    fn emergency_advance_applies_full_throttle_transformations() {
        let mut base = initial_policy();
        base.tier_sizes.l1_lora_layers = 8;
        base.tier_sizes.l1_kv_tokens = 16384;
        base.federation_pull_cadence.pull_cadence_seconds = 60;
        base.consolidation_schedule = ConsolidationSchedule::Idle;
        base.speculation_aggressiveness = SpeculationLevel::Aggressive;
        base.concurrency_caps.personas_concurrent = 8;
        let g = LocalSubstrateGovernor::new(base);

        g.on_pressure_signal(PressureSignal::Thermal {
            severity: ThermalSeverity::Critical,
        });

        let active = g.current_policy();
        assert_eq!(active.cascade_step, 5);
        // All cumulative transformations applied
        assert_eq!(active.tier_sizes.l1_lora_layers, 6); // 8 * 0.75
        assert_eq!(
            active.federation_pull_cadence.pull_cadence_seconds,
            3600 // MAX_FEDERATION_PULL_CADENCE_SECONDS
        );
        assert_eq!(active.consolidation_schedule, ConsolidationSchedule::Manual);
        assert_eq!(
            active.speculation_aggressiveness,
            SpeculationLevel::Balanced
        ); // Aggr→Balanced
        assert_eq!(active.concurrency_caps.personas_concurrent, 7); // 8-1
    }

    /// What this catches: restore-speculation-one-step-later.
    /// Advance → Retreat keeps speculation at PREVIOUS-step value;
    /// next Hold restores it to current-step value. Anti-oscillation
    /// for the most user-visible cascade flapping.
    #[test]
    fn retreat_holds_speculation_for_one_more_cycle() {
        let mut base = initial_policy();
        base.speculation_aggressiveness = SpeculationLevel::Aggressive;
        let g = LocalSubstrateGovernor::new(base);

        // Advance to step 1 — speculation drops to Balanced
        g.on_pressure_signal(PressureSignal::SpeculationMissRate { rate: 0.7 });
        assert_eq!(g.current_cascade_step(), 1);
        assert_eq!(
            g.current_policy().speculation_aggressiveness,
            SpeculationLevel::Balanced
        );

        // Retreat to step 0 — cascade_step = 0 but speculation STAYS at
        // Balanced (one-step-later semantics)
        g.on_pressure_signal(PressureSignal::SpeculationMissRate { rate: 0.1 });
        assert_eq!(g.current_cascade_step(), 0);
        assert_eq!(
            g.current_policy().speculation_aggressiveness,
            SpeculationLevel::Balanced,
            "speculation should stay at step-1 (Balanced) for one cycle after retreat"
        );

        // Next Hold delivers the speculation restoration — back to Aggressive
        g.on_pressure_signal(PressureSignal::UserActive { foreground: true });
        assert_eq!(
            g.current_policy().speculation_aggressiveness,
            SpeculationLevel::Aggressive,
            "speculation restored to step-0 (Aggressive) on next Hold"
        );
    }

    /// What this catches: re-advancing during pending-retreat clears
    /// the marker (speculation re-throttles immediately to the new
    /// step's value). The asymmetric restore-one-later only applies
    /// to RETREAT, not advance.
    #[test]
    fn advance_during_pending_retreat_clears_marker() {
        let mut base = initial_policy();
        base.speculation_aggressiveness = SpeculationLevel::Aggressive;
        let g = LocalSubstrateGovernor::new(base);

        // Advance to step 1
        g.on_pressure_signal(PressureSignal::SpeculationMissRate { rate: 0.7 });
        // Retreat to step 0 (speculation still Balanced — pending marker set)
        g.on_pressure_signal(PressureSignal::SpeculationMissRate { rate: 0.1 });
        assert_eq!(
            g.current_policy().speculation_aggressiveness,
            SpeculationLevel::Balanced
        );

        // Sleep simulated by manually adjusting last_step_change_ms
        // to bypass the time-in-step gate would be needed here, but
        // since prev_step=0 the gate doesn't apply (step 0 → 1 is
        // immediate). Advance again — speculation jumps back to
        // Balanced (step 1's value), pending marker cleared.
        g.on_pressure_signal(PressureSignal::SpeculationMissRate { rate: 0.7 });
        assert_eq!(g.current_cascade_step(), 1);
        // Step 1's speculation is Balanced (Aggressive → Balanced)
        assert_eq!(
            g.current_policy().speculation_aggressiveness,
            SpeculationLevel::Balanced
        );

        // Now Hold — should NOT restore speculation since marker was
        // cleared by the second advance
        g.on_pressure_signal(PressureSignal::UserActive { foreground: true });
        assert_eq!(
            g.current_policy().speculation_aggressiveness,
            SpeculationLevel::Balanced,
            "after marker cleared, Hold doesn't restore"
        );
    }

    /// What this catches: hardware_detected refreshes the BASE
    /// policy AND resets cascade to step 0. New hardware = fresh start;
    /// existing cascade pressure state is discarded.
    #[test]
    fn hardware_detected_refreshes_base_and_resets_cascade() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        g.set_candidates(vec![policy_with_l1(2), policy_with_l1_nvidia(8)]);

        // Push cascade to step 3
        g.on_pressure_signal(PressureSignal::SpeculationMissRate { rate: 0.7 });
        // Force time to advance past gate (in real run; here we just
        // accept that step 1 is reached, which is enough to prove
        // the reset clears it)
        assert!(g.current_cascade_step() >= 1);

        // Hardware change resets cascade
        let blackwell = hw(
            TargetSilicon::NvidiaCuda,
            ThermalClass::Workstation,
            32 * 1024,
            64 * 1024,
        );
        g.on_hardware_detected(blackwell).unwrap();

        assert_eq!(
            g.current_cascade_step(),
            0,
            "hardware change resets cascade to 0"
        );
        // Active policy is from the new candidate (l1_lora_layers=8 from blackwell)
        assert_eq!(g.current_policy().tier_sizes.l1_lora_layers, 8);
    }

    /// What this catches: derive-from-base means consecutive
    /// transitions don't compound transformations. Advance 0→1→0
    /// returns to the BASE policy values, not to a doubly-transformed
    /// state. This was the not-reversible warning from PR-3c3.
    #[test]
    fn advance_then_retreat_returns_to_base_values_modulo_speculation_dampening() {
        let mut base = initial_policy();
        base.tier_sizes.l1_lora_layers = 8;
        base.tier_sizes.l1_kv_tokens = 16384;
        let g = LocalSubstrateGovernor::new(base);

        // Step 0 → step 1 (only speculation changes; tier_sizes
        // unaffected since step 3 is where l1 shrinks)
        g.on_pressure_signal(PressureSignal::SpeculationMissRate { rate: 0.7 });
        // Retreat to step 0 — tier_sizes back to base
        g.on_pressure_signal(PressureSignal::SpeculationMissRate { rate: 0.1 });

        let active = g.current_policy();
        assert_eq!(active.cascade_step, 0);
        // tier_sizes back to base (step 0 transformation, derived from base)
        assert_eq!(active.tier_sizes.l1_lora_layers, 8);
        assert_eq!(active.tier_sizes.l1_kv_tokens, 16384);
    }

    // Helpers for tests above

    fn policy_with_l1(l1: u32) -> PolicyFile {
        use crate::governor::policy_file::*;
        PolicyFile {
            policy_version: 1,
            applies_to: "apple-m,thinandlight,uma,vram_mb=0..0,ram_mb=14000..18000".into(),
            tier_sizes: TierSizesFile {
                l1_lora_layers: l1,
                l1_kv_tokens: 2048,
                l2_lora_layers: 4,
                l3_lora_layers: 12,
                l3_engrams: 1024,
            },
            cadence_multipliers: CadenceMultipliersFile {
                realtime: 1.0,
                delayed: 1.0,
                background: 1.0,
            },
            concurrency_caps: ConcurrencyCapsFile {
                personas_concurrent: 2,
                inference_lanes: 1,
                foundry_lanes: 0,
                sentinel_lanes: 1,
            },
            speculation: SpeculationFileSection {
                level: SpeculationLevel::Conservative,
            },
            consolidation: ConsolidationFileSection {
                schedule: ConsolidationSchedule::Manual,
            },
            federation: FederationCadenceFile {
                pull_cadence_seconds: 600,
            },
            recall_weights: RecallScoreWeightsFile {
                semantic: 0.4,
                outcome_history: 0.3,
                recency: 0.1,
                tier_proximity: 0.1,
                provenance_trust: 0.1,
            },
        }
    }

    fn policy_with_l1_nvidia(l1: u32) -> PolicyFile {
        let mut p = policy_with_l1(l1);
        p.applies_to = "nvidia,workstation,vram_mb=30000..36000".into();
        p
    }

    // ===== concurrency =====

    /// What this catches: many concurrent reads return the current
    /// policy without blocking. Sanity check on the arc_swap wait-free
    /// claim — if this hangs or deadlocks, the design is wrong.
    #[test]
    fn many_concurrent_reads_dont_block() {
        let g = Arc::new(LocalSubstrateGovernor::new(initial_policy()));
        let mut handles = Vec::new();
        for _ in 0..16 {
            let g_clone = Arc::clone(&g);
            handles.push(std::thread::spawn(move || {
                for _ in 0..1000 {
                    let _ = g_clone.current_policy();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
    }

    /// What this catches: a concurrent reader observes a CONSISTENT
    /// policy snapshot even while a writer is rewriting. arc_swap's
    /// load_full() returns an Arc — the reader holds a stable snapshot
    /// even if a new policy lands a nanosecond later. Test pins this
    /// guarantee.
    #[test]
    fn concurrent_read_during_write_sees_consistent_snapshot() {
        let g = Arc::new(LocalSubstrateGovernor::new(initial_policy()));
        g.set_candidates(vec![
            pol(
                "apple-m,thinandlight,uma,vram_mb=0..0,ram_mb=14000..18000",
                2,
            ),
            pol("nvidia,workstation,vram_mb=30000..36000", 8),
        ]);

        let g_writer = Arc::clone(&g);
        let writer = std::thread::spawn(move || {
            for i in 0..100 {
                let h = if i % 2 == 0 {
                    hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384)
                } else {
                    hw(
                        TargetSilicon::NvidiaCuda,
                        ThermalClass::Workstation,
                        32 * 1024,
                        64 * 1024,
                    )
                };
                g_writer
                    .on_hardware_detected(h)
                    .expect("test candidates should match alternating hardware");
            }
        });

        let g_reader = Arc::clone(&g);
        let reader = std::thread::spawn(move || {
            for _ in 0..500 {
                let p = g_reader.current_policy();
                // Either the initial policy OR an air policy OR a blackwell
                // policy; never garbage. The Arc holds a complete snapshot.
                let l1 = p.tier_sizes.l1_lora_layers;
                assert!(
                    l1 == 1 || l1 == 2 || l1 == 8,
                    "unexpected l1_lora_layers={l1} — torn read of policy?"
                );
            }
        });

        writer.join().unwrap();
        reader.join().unwrap();
    }

    /// What this catches: current_policy() returns the SAME Arc on
    /// back-to-back calls when no write happened. arc_swap.load_full
    /// returns a clone of the same Arc, so two reads share the same
    /// allocation pointer.
    #[test]
    fn current_policy_returns_same_arc_when_no_writes() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        let a = g.current_policy();
        let b = g.current_policy();
        assert!(
            Arc::ptr_eq(&a, &b),
            "expected same Arc pointer on back-to-back reads"
        );
    }
