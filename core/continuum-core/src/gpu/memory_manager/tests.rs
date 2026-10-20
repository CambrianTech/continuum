    use super::*;

    fn test_manager(total_mb: u64) -> Arc<GpuMemoryManager> {
        let total_bytes = total_mb * 1024 * 1024;
        let reserve_bytes = (total_bytes as f64 * RESERVE_PCT) as u64;
        let usable = total_bytes - reserve_bytes;

        let (pressure_tx, pressure_rx) = watch::channel(0.0f32);

        Arc::new(GpuMemoryManager {
            total_vram_bytes: total_bytes,
            gpu_name: "Test GPU".to_string(),
            subsystems: [
                SubsystemBudget::new(usable),
                SubsystemBudget::new(usable),
                SubsystemBudget::new(usable),
            ],
            reserve_bytes,
            pressure_tx,
            pressure_rx,
            allocation_counts: [
                AtomicU32::new(0),
                AtomicU32::new(0),
                AtomicU32::new(0),
                AtomicU32::new(0),
            ],
            eviction_registry: EvictionRegistry::new(),
        })
    }

    #[test]
    fn test_detect_returns_nonzero() {
        let (total, name) = detect_gpu();
        assert!(total > 0, "Total VRAM should be > 0");
        assert!(!name.is_empty(), "GPU name should not be empty");
    }

    #[test]
    fn test_initial_pressure_zero() {
        let mgr = test_manager(36_864); // 36GB like M3 Max
        assert!((mgr.pressure() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_allocate_and_release() {
        let mgr = test_manager(1024); // 1GB
        let inference_budget = mgr.inference_budget_bytes();

        // Allocate half the inference budget at Interactive priority
        let half = inference_budget / 2;
        let guard = mgr
            .allocate(GpuSubsystem::Inference, half, GpuPriority::Interactive)
            .unwrap();
        assert!(mgr.pressure() > 0.0);

        // Release
        drop(guard);
        assert!((mgr.pressure() - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_raii_guard_releases_on_drop() {
        let mgr = test_manager(1024);
        let used_before = mgr.subsystems[GpuSubsystem::Inference.index()].used();

        {
            let _guard = mgr
                .allocate(
                    GpuSubsystem::Inference,
                    100 * 1024 * 1024,
                    GpuPriority::Interactive,
                )
                .unwrap();
            let used_during = mgr.subsystems[GpuSubsystem::Inference.index()].used();
            assert_eq!(used_during, used_before + 100 * 1024 * 1024);
        }

        let used_after = mgr.subsystems[GpuSubsystem::Inference.index()].used();
        assert_eq!(used_after, used_before);
    }

    #[test]
    fn test_manual_release() {
        let mgr = test_manager(1024);
        let guard = mgr
            .allocate(
                GpuSubsystem::Tts,
                50 * 1024 * 1024,
                GpuPriority::Interactive,
            )
            .unwrap();
        assert!(mgr.subsystems[GpuSubsystem::Tts.index()].used() > 0);

        guard.release();
        assert_eq!(mgr.subsystems[GpuSubsystem::Tts.index()].used(), 0);
    }

    // what this catches: the static 75/10/10 partition is GONE (memory-authority slice 4).
    // Every subsystem's soft budget is the FULL shared usable pool (~95% of total after
    // the driver reserve), not a demand-blind fraction — so inference is never falsely
    // capped at 75% while tts/rendering sit idle. Contention is arbitrated live by the
    // pressure gate, not by a fixed split. Regresses the "nothing static" invariant.
    #[test]
    fn test_no_static_partition_all_subsystems_share_the_usable_pool() {
        let mgr = test_manager(36_864); // 36GB
        let stats = mgr.stats();

        let usable_pct = (1.0 - RESERVE_PCT) as f32; // ~0.95
        for (name, budget_mb) in [
            ("inference", stats.inference.budget_mb),
            ("tts", stats.tts.budget_mb),
            ("rendering", stats.rendering.budget_mb),
        ] {
            let pct = budget_mb / stats.total_vram_mb;
            assert!(
                (pct - usable_pct).abs() < 0.01,
                "{name} should draw the full usable pool (~{:.0}%), got {:.1}%",
                usable_pct * 100.0,
                pct * 100.0
            );
        }

        // And they are equal — one shared pool, no per-subsystem carve-up.
        assert!((stats.inference.budget_mb - stats.tts.budget_mb).abs() < 1.0);
        assert!((stats.inference.budget_mb - stats.rendering.budget_mb).abs() < 1.0);
    }

    #[test]
    fn test_per_persona_budget() {
        let mgr = test_manager(36_864); // 36GB
        let total_inference = mgr.inference_budget_mb();

        let per_1 = mgr.per_persona_inference_budget_mb(1);
        assert!((per_1 - total_inference).abs() < 1.0);

        let per_5 = mgr.per_persona_inference_budget_mb(5);
        assert!((per_5 - total_inference / 5.0).abs() < 1.0);

        // 0 personas treated as 1
        let per_0 = mgr.per_persona_inference_budget_mb(0);
        assert!((per_0 - total_inference).abs() < 1.0);
    }

    #[test]
    fn test_stats_snapshot() {
        let mgr = test_manager(1024);
        let _guard = mgr
            .allocate(
                GpuSubsystem::Inference,
                100 * 1024 * 1024,
                GpuPriority::Interactive,
            )
            .unwrap();

        let stats = mgr.stats();
        assert_eq!(stats.gpu_name, "Test GPU");
        assert!((stats.total_vram_mb - 1024.0).abs() < 1.0);
        assert!(stats.inference.used_mb > 99.0);
        assert!(stats.pressure > 0.0);
    }

    #[test]
    fn test_pressure_watch_channel() {
        let mgr = test_manager(1024);
        let rx = mgr.pressure_rx();

        // Initial pressure is 0
        assert!(*rx.borrow() < 0.01);

        // Allocate should update pressure
        let _guard = mgr
            .allocate(
                GpuSubsystem::Inference,
                500 * 1024 * 1024,
                GpuPriority::Realtime,
            )
            .unwrap();
        assert!(rx.has_changed().unwrap_or(false) || *rx.borrow() > 0.0);
    }

    // what this catches: SubsystemBudget is a SOFT limit — exceeding it does NOT reject
    // the allocation; only the dynamic pressure gate does. With the static partition gone,
    // the soft budget defaults to the full usable pool, so we shrink it explicitly via
    // set_budget to prove the soft-vs-hard distinction still holds: over the soft budget
    // but well under the Realtime pressure gate → still succeeds.
    #[test]
    fn test_over_budget_soft_limit() {
        let mgr = test_manager(1024);
        // Pin a tiny soft budget so the next allocation is over it, yet far under the
        // physical pressure gate (200MB / ~972MB usable ≈ 0.21 ≪ 0.95 Realtime gate).
        mgr.set_budget(GpuSubsystem::Inference, 100 * 1024 * 1024);

        let guard = mgr.allocate(
            GpuSubsystem::Inference,
            200 * 1024 * 1024,
            GpuPriority::Realtime,
        );
        assert!(
            guard.is_ok(),
            "Over-soft-budget allocation should succeed — only the pressure gate rejects"
        );
    }

    #[test]
    fn test_release_saturating() {
        let mgr = test_manager(1024);
        // Release without prior allocation — should not underflow
        mgr.release(GpuSubsystem::Inference, 999 * 1024 * 1024);
        assert_eq!(mgr.subsystems[GpuSubsystem::Inference.index()].used(), 0);
    }

    #[test]
    fn test_multiple_subsystem_pressure() {
        let mgr = test_manager(1024);

        let _g1 = mgr
            .allocate(
                GpuSubsystem::Rendering,
                50 * 1024 * 1024,
                GpuPriority::Realtime,
            )
            .unwrap();
        let _g2 = mgr
            .allocate(
                GpuSubsystem::Inference,
                200 * 1024 * 1024,
                GpuPriority::Interactive,
            )
            .unwrap();
        let _g3 = mgr
            .allocate(
                GpuSubsystem::Tts,
                30 * 1024 * 1024,
                GpuPriority::Interactive,
            )
            .unwrap();

        let stats = mgr.stats();
        assert!(stats.rendering.used_mb > 49.0);
        assert!(stats.inference.used_mb > 199.0);
        assert!(stats.tts.used_mb > 29.0);

        // Total pressure from all subsystems
        let expected_total = 280.0; // 50+200+30
        assert!(stats.total_used_mb > expected_total - 1.0);
    }

    // ── Priority gating tests ────────────────────────────────────────

    #[test]
    fn test_realtime_only_rejected_at_critical() {
        // Realtime priority: gate = 0.95 (PRESSURE_CRITICAL)
        // Fill to 90% — Realtime should still succeed
        let mgr = test_manager(1024);
        let usable = 1024_u64 * 1024 * 1024 - (1024_u64 * 1024 * 1024 * 5 / 100);

        let fill = (usable as f64 * 0.90) as u64;
        let _fill = mgr
            .allocate(GpuSubsystem::Inference, fill, GpuPriority::Realtime)
            .unwrap();
        assert!(mgr.pressure() >= PRESSURE_HIGH);

        // Realtime at 90% pressure — should succeed
        let small = 1024 * 1024; // 1MB
        let result = mgr.allocate(GpuSubsystem::Rendering, small, GpuPriority::Realtime);
        assert!(result.is_ok(), "Realtime should succeed at 90% pressure");
    }

    #[test]
    fn test_interactive_rejected_at_high() {
        // Interactive priority: gate = 0.80 (PRESSURE_HIGH)
        // Fill to 82% — Interactive should be rejected
        let mgr = test_manager(1024);
        let usable = 1024_u64 * 1024 * 1024 - (1024_u64 * 1024 * 1024 * 5 / 100);

        let fill = (usable as f64 * 0.82) as u64;
        let _fill = mgr
            .allocate(GpuSubsystem::Inference, fill, GpuPriority::Realtime)
            .unwrap();
        assert!(mgr.pressure() >= PRESSURE_HIGH);

        // Interactive at 82% — should be rejected (gate = 0.80)
        let small = 1024 * 1024; // 1MB
        let result = mgr.allocate(GpuSubsystem::Tts, small, GpuPriority::Interactive);
        assert!(
            result.is_err(),
            "Interactive should be rejected at 82% pressure"
        );
    }

    #[test]
    fn test_background_rejected_at_warning() {
        // Background priority: gate = 0.60 (PRESSURE_WARNING)
        // Fill to 62% — Background should be rejected
        let mgr = test_manager(1024);
        let usable = 1024_u64 * 1024 * 1024 - (1024_u64 * 1024 * 1024 * 5 / 100);

        let fill = (usable as f64 * 0.62) as u64;
        let _fill = mgr
            .allocate(GpuSubsystem::Inference, fill, GpuPriority::Realtime)
            .unwrap();
        assert!(mgr.pressure() >= PRESSURE_WARNING);

        // Background at 62% — should be rejected (gate = 0.60)
        let small = 1024 * 1024;
        let result = mgr.allocate(GpuSubsystem::Inference, small, GpuPriority::Background);
        assert!(
            result.is_err(),
            "Background should be rejected at 62% pressure"
        );

        // But Interactive should still succeed at 62% (gate = 0.80)
        let result2 = mgr.allocate(GpuSubsystem::Inference, small, GpuPriority::Interactive);
        assert!(
            result2.is_ok(),
            "Interactive should succeed at 62% pressure"
        );
    }

    #[test]
    fn test_batch_rejected_at_50_percent() {
        // Batch priority: gate = 0.50
        // Fill to 52% — Batch should be rejected
        let mgr = test_manager(1024);
        let usable = 1024_u64 * 1024 * 1024 - (1024_u64 * 1024 * 1024 * 5 / 100);

        let fill = (usable as f64 * 0.52) as u64;
        let _fill = mgr
            .allocate(GpuSubsystem::Inference, fill, GpuPriority::Realtime)
            .unwrap();

        let small = 1024 * 1024;
        let result = mgr.allocate(GpuSubsystem::Inference, small, GpuPriority::Batch);
        assert!(result.is_err(), "Batch should be rejected at 52% pressure");

        // Background should still succeed at 52% (gate = 0.60)
        let result2 = mgr.allocate(GpuSubsystem::Inference, small, GpuPriority::Background);
        assert!(result2.is_ok(), "Background should succeed at 52% pressure");
    }

    #[test]
    fn test_pressure_gate_error_contains_priority_info() {
        let mgr = test_manager(1024);
        let usable = 1024_u64 * 1024 * 1024 - (1024_u64 * 1024 * 1024 * 5 / 100);

        // Fill to 65%
        let fill = (usable as f64 * 0.65) as u64;
        let _fill = mgr
            .allocate(GpuSubsystem::Inference, fill, GpuPriority::Realtime)
            .unwrap();

        // Background allocation should fail with PressureGate error
        let result = mgr.allocate(GpuSubsystem::Tts, 1024 * 1024, GpuPriority::Background);
        match result {
            Err(GpuError::PressureGate { priority, gate, .. }) => {
                assert_eq!(priority, GpuPriority::Background);
                assert!((gate - PRESSURE_WARNING).abs() < 0.001);
            }
            _ => panic!("Expected PressureGate error"),
        }
    }

    // ── Allocation counter tests ──────────────────────────────────────

    #[test]
    fn test_allocation_counters_increment_and_decrement() {
        let mgr = test_manager(1024);

        assert_eq!(mgr.allocation_count(GpuPriority::Interactive), 0);
        assert_eq!(mgr.allocation_count(GpuPriority::Realtime), 0);

        let g1 = mgr
            .allocate(
                GpuSubsystem::Inference,
                10 * 1024 * 1024,
                GpuPriority::Interactive,
            )
            .unwrap();
        assert_eq!(mgr.allocation_count(GpuPriority::Interactive), 1);

        let g2 = mgr
            .allocate(GpuSubsystem::Tts, 5 * 1024 * 1024, GpuPriority::Interactive)
            .unwrap();
        assert_eq!(mgr.allocation_count(GpuPriority::Interactive), 2);

        let _g3 = mgr
            .allocate(
                GpuSubsystem::Rendering,
                5 * 1024 * 1024,
                GpuPriority::Realtime,
            )
            .unwrap();
        assert_eq!(mgr.allocation_count(GpuPriority::Realtime), 1);

        // Drop g1 — Interactive should decrement
        drop(g1);
        assert_eq!(mgr.allocation_count(GpuPriority::Interactive), 1);

        // Manual release g2 — Interactive should decrement again
        g2.release();
        assert_eq!(mgr.allocation_count(GpuPriority::Interactive), 0);

        // Realtime unchanged
        assert_eq!(mgr.allocation_count(GpuPriority::Realtime), 1);
    }

    #[test]
    fn test_stats_includes_allocation_counts() {
        let mgr = test_manager(1024);

        let _g1 = mgr
            .allocate(
                GpuSubsystem::Rendering,
                5 * 1024 * 1024,
                GpuPriority::Realtime,
            )
            .unwrap();
        let _g2 = mgr
            .allocate(
                GpuSubsystem::Inference,
                10 * 1024 * 1024,
                GpuPriority::Interactive,
            )
            .unwrap();
        let _g3 = mgr
            .allocate(
                GpuSubsystem::Inference,
                5 * 1024 * 1024,
                GpuPriority::Background,
            )
            .unwrap();

        let stats = mgr.stats();
        assert_eq!(stats.allocations_by_priority.realtime, 1);
        assert_eq!(stats.allocations_by_priority.interactive, 1);
        assert_eq!(stats.allocations_by_priority.background, 1);
        assert_eq!(stats.allocations_by_priority.batch, 0);
    }

    #[test]
    fn test_rejected_allocation_does_not_increment_counter() {
        let mgr = test_manager(1024);
        let usable = 1024_u64 * 1024 * 1024 - (1024_u64 * 1024 * 1024 * 5 / 100);

        // Fill to 65%
        let fill = (usable as f64 * 0.65) as u64;
        let _fill = mgr
            .allocate(GpuSubsystem::Inference, fill, GpuPriority::Realtime)
            .unwrap();

        // Background allocation should fail — counter should NOT increment
        assert_eq!(mgr.allocation_count(GpuPriority::Background), 0);
        let _ = mgr.allocate(GpuSubsystem::Tts, 1024 * 1024, GpuPriority::Background);
        assert_eq!(
            mgr.allocation_count(GpuPriority::Background),
            0,
            "Rejected allocation should not increment counter"
        );
    }

    #[test]
    fn test_guard_stores_priority() {
        let mgr = test_manager(1024);
        let guard = mgr
            .allocate(
                GpuSubsystem::Inference,
                10 * 1024 * 1024,
                GpuPriority::Background,
            )
            .unwrap();
        assert_eq!(guard.priority(), GpuPriority::Background);
    }

    // ── Concurrency tests ─────────────────────────────────────────────

    #[test]
    fn test_critical_pressure_rollback() {
        // Verify optimistic-allocate-then-rollback works:
        // Fill to near-critical, then allocate just enough to push past.
        let mgr = test_manager(1024); // 1GB
        let usable = 1024_u64 * 1024 * 1024 - (1024_u64 * 1024 * 1024 * 5 / 100); // ~972MB usable

        // Fill to 94% of usable across subsystems (Realtime so it doesn't get gated)
        let fill_bytes = (usable as f64 * 0.94) as u64;
        let _fill = mgr
            .allocate(GpuSubsystem::Inference, fill_bytes, GpuPriority::Realtime)
            .unwrap();
        assert!(mgr.pressure() < PRESSURE_CRITICAL);

        // This Realtime allocation should push past 95% — should be rejected and rolled back
        let overfill = (usable as f64 * 0.10) as u64; // 10% more → 104% total
        let result = mgr.allocate(GpuSubsystem::Tts, overfill, GpuPriority::Realtime);
        assert!(
            result.is_err(),
            "Should reject allocation that pushes past critical"
        );

        // Verify the bytes were rolled back (only fill_bytes should remain)
        let tts_used = mgr.subsystems[GpuSubsystem::Tts.index()].used();
        assert_eq!(
            tts_used, 0,
            "TTS used should be 0 after rollback, got {}",
            tts_used
        );
    }

    #[test]
    fn test_concurrent_allocation_safety() {
        // Simulate the TOCTOU scenario: many threads racing to allocate.
        // With optimistic-rollback, at most one should succeed when total
        // would push past the gate.
        let mgr = test_manager(1024); // 1GB
        let usable = 1024_u64 * 1024 * 1024 - (1024_u64 * 1024 * 1024 * 5 / 100);

        // Fill to 90% (with Realtime so it doesn't get gated early)
        let fill = (usable as f64 * 0.90) as u64;
        let _fill_guard = mgr
            .allocate(GpuSubsystem::Inference, fill, GpuPriority::Realtime)
            .unwrap();

        // Now 10 threads try Realtime allocation of 2% each — only some should succeed
        // (total would be 90% + 20% = 110%, well past critical)
        let chunk = (usable as f64 * 0.02) as u64;
        let mgr_ref = &mgr;

        let results: Vec<bool> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..10)
                .map(|_| {
                    s.spawn(move || {
                        mgr_ref
                            .allocate(GpuSubsystem::Tts, chunk, GpuPriority::Realtime)
                            .is_ok()
                    })
                })
                .collect();

            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });

        let successes = results.iter().filter(|&&ok| ok).count();
        let final_pressure = mgr.pressure();

        // Some allocations should succeed, but final pressure must stay below critical.
        // (The exact number depends on scheduling, but pressure must be safe.)
        assert!(
            final_pressure < PRESSURE_CRITICAL,
            "Final pressure {:.1}% should be below critical {:.0}% — {} of 10 allocations succeeded",
            final_pressure * 100.0, PRESSURE_CRITICAL * 100.0, successes
        );
    }

    // ── Priority ordering tests ──────────────────────────────────────

    #[test]
    fn test_priority_ordering() {
        assert!(GpuPriority::Realtime < GpuPriority::Interactive);
        assert!(GpuPriority::Interactive < GpuPriority::Background);
        assert!(GpuPriority::Background < GpuPriority::Batch);
    }

    #[test]
    fn test_pressure_gates_are_monotonically_decreasing() {
        // Higher priority = higher gate = harder to reject
        assert!(GpuPriority::Realtime.pressure_gate() > GpuPriority::Interactive.pressure_gate());
        assert!(GpuPriority::Interactive.pressure_gate() > GpuPriority::Background.pressure_gate());
        assert!(GpuPriority::Background.pressure_gate() > GpuPriority::Batch.pressure_gate());
    }

    #[test]
    fn test_eviction_weights_match_priority() {
        assert!(
            GpuPriority::Realtime.eviction_weight() > GpuPriority::Interactive.eviction_weight()
        );
        assert!(
            GpuPriority::Interactive.eviction_weight() > GpuPriority::Background.eviction_weight()
        );
        assert!(GpuPriority::Background.eviction_weight() > GpuPriority::Batch.eviction_weight());
    }

    // ── ts-rs binding tests ─────────────────────────────────────────────

    #[test]
    fn export_bindings_gpu_stats() {
        let cfg = ts_rs::Config::default();
        GpuStats::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_subsystem_stats() {
        let cfg = ts_rs::Config::default();
        SubsystemStats::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_gpu_priority() {
        let cfg = ts_rs::Config::default();
        GpuPriority::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_allocations_by_priority() {
        let cfg = ts_rs::Config::default();
        AllocationsByPriority::export_all(&cfg).unwrap();
    }
