#![cfg(test)]
    use super::*;
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;

    fn persona(id: u128) -> PeerId {
        PeerId::from_uuid(Uuid::from_u128(id))
    }

    /// what this catches: the governor→coordinator silicon translation. A
    /// discrete GPU (NvidiaCuda/AmdRocm/IntelVulkan) MUST map to `Gpu` (not
    /// the UnifiedMemory floor) — that's the GPU-or-bust fix. AppleM →
    /// UnifiedMemory; `None` → `Cpu` (honest, not a silent GPU pretense).
    #[test]
    fn governor_silicon_maps_discrete_gpu_to_gpu_not_floor() {
        assert_eq!(
            coordinator_silicon_for(GovernorSilicon::NvidiaCuda),
            TargetSilicon::Gpu
        );
        assert_eq!(
            coordinator_silicon_for(GovernorSilicon::AmdRocm),
            TargetSilicon::Gpu
        );
        assert_eq!(
            coordinator_silicon_for(GovernorSilicon::IntelVulkan),
            TargetSilicon::Gpu
        );
        assert_eq!(
            coordinator_silicon_for(GovernorSilicon::AppleM),
            TargetSilicon::UnifiedMemory
        );
        assert_eq!(
            coordinator_silicon_for(GovernorSilicon::None),
            TargetSilicon::Cpu
        );
    }

    /// what this catches: `for_silicon` sets BOTH the lane-budget silicon AND
    /// the default to the SAME class — a mismatch would deny admission (the
    /// planner looks up the budget by the lease's target_silicon). Pins that
    /// a Gpu config targets Gpu end-to-end, not a stray UnifiedMemory.
    #[test]
    fn for_silicon_targets_one_silicon_end_to_end() {
        let cfg = CoordinatorConfig::for_silicon(TargetSilicon::Gpu);
        assert_eq!(cfg.default_target_silicon, TargetSilicon::Gpu);
        assert_eq!(cfg.lane_budgets.len(), 1);
        assert_eq!(cfg.lane_budgets[0].target_silicon, TargetSilicon::Gpu);
        // realistic_floor_default is now just for_silicon(UnifiedMemory).
        let floor = CoordinatorConfig::realistic_floor_default();
        assert_eq!(floor.default_target_silicon, TargetSilicon::UnifiedMemory);
        assert_eq!(
            floor.lane_budgets[0].target_silicon,
            TargetSilicon::UnifiedMemory
        );
    }

    /// what this catches: `detected()` runs to completion without panicking
    /// on the test host (the probe→classify→translate chain), and the result
    /// it produces is self-consistent. The end-to-end consistency invariant
    /// itself is owned by `for_silicon_targets_one_silicon_end_to_end`; this
    /// is the smoke test that the real hardware-probe path doesn't blow up.
    #[test]
    fn detected_config_runs_without_panicking() {
        let cfg = CoordinatorConfig::detected();
        assert_eq!(
            cfg.default_target_silicon,
            cfg.lane_budgets[0].target_silicon
        );
    }

    fn small_budget_config() -> CoordinatorConfig {
        // Tight budgets so tests can exercise admission deny without
        // having to set up production-sized numbers.
        CoordinatorConfig {
            lane_budgets: vec![ThroughputLaneBudget {
                resource_class: ResourceClass::LocalGeneration,
                target_silicon: TargetSilicon::Cpu,
                // Cap at 2 concurrent lanes to exercise concurrency
                // backpressure.
                max_concurrency: 2,
                // Cap at 20K cost_units (≈ 2× Chat lanes worth).
                max_cost_units: 20_000,
            }],
            // Tiny per-token bytes so footprint stays trivial in tests.
            bytes_per_token: 1,
            lease_duration_ms: 60_000,
            default_target_silicon: TargetSilicon::Cpu,
        }
    }

    fn build_coordinator() -> InferenceCoordinator {
        let footprint = Arc::new(FootprintRegistry::new());
        let handle_store = Arc::new(InferenceHandleStore::new());
        InferenceCoordinator::new(footprint, handle_store, small_budget_config())
    }

    fn open_chat(
        c: &InferenceCoordinator,
        persona_id: u128,
        now_ms: u64,
    ) -> Result<HandleRef, CoordinatorError> {
        c.open_lane(OpenLaneRequest {
            persona: persona(persona_id),
            task: TaskKind::Chat,
            adapter: Arc::new(HeuristicInferenceAdapter::new()),
            model: None,
            system_prompt: None,
            active_adapters: None,
            class_override: None,
            now_ms,
        })
    }

    // ── basic open + close ──────────────────────────────────────

    #[test]
    fn open_lane_admits_first_persona_returns_handle() {
        let c = build_coordinator();
        let h = open_chat(&c, 1, 1_000_000).unwrap();
        assert_eq!(c.lane_count(), 1);
        assert_eq!(h.owner, crate::inference::handle_store::HANDLE_OWNER);
    }

    #[test]
    fn lane_is_bound_to_handle_and_carries_persona_task_class() {
        let c = build_coordinator();
        let h = open_chat(&c, 1, 1_000_000).unwrap();
        let lane = c.lane_for_handle(&h).unwrap();
        assert_eq!(lane.persona(), persona(1));
        assert_eq!(lane.task(), TaskKind::Chat);
        assert_eq!(lane.class(), LaneClass::Interactive);
        assert_eq!(lane.handle_id(), h.id.as_uuid());
    }

    #[test]
    fn close_lane_releases_and_decrements_count() {
        let c = build_coordinator();
        let h = open_chat(&c, 1, 1_000_000).unwrap();
        assert_eq!(c.lane_count(), 1);
        assert!(c.close_lane(&h).unwrap());
        assert_eq!(c.lane_count(), 0);
        // Double-close is idempotent.
        assert!(!c.close_lane(&h).unwrap());
    }

    // ── admission ───────────────────────────────────────────────

    #[test]
    fn admission_denies_when_concurrency_exceeded() {
        let c = build_coordinator();
        // budget: max_concurrency=2 → first two admit, third denies
        open_chat(&c, 1, 1_000_000).unwrap();
        open_chat(&c, 2, 1_000_000).unwrap();
        let err = open_chat(&c, 3, 1_000_000).unwrap_err();
        match err {
            CoordinatorError::AdmissionDenied { reason, .. } => {
                assert_eq!(reason, AdmissionDenyReason::ResourcePressure);
            }
            other => panic!("expected AdmissionDenied, got {other:?}"),
        }
        assert_eq!(c.lane_count(), 2);
    }

    #[test]
    fn admission_denies_when_cost_units_exceeded() {
        // Two CodingLarge (128K each) blows past 20K max_cost_units.
        let c = build_coordinator();
        let _ = c
            .open_lane(OpenLaneRequest {
                persona: persona(1),
                task: TaskKind::CodingLarge,
                adapter: Arc::new(HeuristicInferenceAdapter::new()),
                model: None,
                system_prompt: None,
                active_adapters: None,
                class_override: None,
                now_ms: 1_000_000,
            })
            .unwrap_err();
        // Even the FIRST CodingLarge fails because its cost_units
        // (128K) exceeds the lane's max_cost_units (20K).
        assert_eq!(c.lane_count(), 0);
    }

    #[test]
    fn admission_denies_when_no_budget_for_silicon() {
        // Config only has Cpu budget; request UnifiedMemory.
        let mut config = small_budget_config();
        config.default_target_silicon = TargetSilicon::UnifiedMemory;
        // Don't add a UnifiedMemory lane budget — admission will say NoBudget.
        let footprint = Arc::new(FootprintRegistry::new());
        let handle_store = Arc::new(InferenceHandleStore::new());
        let c = InferenceCoordinator::new(footprint, handle_store, config);
        let err = open_chat(&c, 1, 1_000_000).unwrap_err();
        match err {
            CoordinatorError::AdmissionDenied { reason, .. } => {
                assert_eq!(reason, AdmissionDenyReason::NoBudget);
            }
            other => panic!("expected NoBudget, got {other:?}"),
        }
    }

    // ── three-persona realistic floor smoke ─────────────────────

    #[test]
    fn three_personas_concurrent_lanes_on_one_adapter_realistic_floor() {
        // The substrate's defining boast at the realistic floor.
        let mut cfg = small_budget_config();
        cfg.lane_budgets[0].max_concurrency = 3;
        cfg.lane_budgets[0].max_cost_units = 30_000;
        let c = InferenceCoordinator::new(
            Arc::new(FootprintRegistry::new()),
            Arc::new(InferenceHandleStore::new()),
            cfg,
        );
        // One shared heuristic adapter.
        let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
        let mk = |id, task| OpenLaneRequest {
            persona: persona(id),
            task,
            adapter: adapter.clone(),
            model: None,
            system_prompt: None,
            active_adapters: None,
            class_override: None,
            now_ms: 1_000_000,
        };
        let h1 = c.open_lane(mk(1, TaskKind::Chat)).unwrap();
        let h2 = c.open_lane(mk(2, TaskKind::VoiceChat)).unwrap();
        let h3 = c.open_lane(mk(3, TaskKind::GameNpcIdle)).unwrap();
        assert_eq!(c.lane_count(), 3);

        // Distinct lanes per persona × task.
        let l1 = c.lane_for_handle(&h1).unwrap();
        let l2 = c.lane_for_handle(&h2).unwrap();
        let l3 = c.lane_for_handle(&h3).unwrap();
        assert_eq!(l1.task(), TaskKind::Chat);
        assert_eq!(l2.task(), TaskKind::VoiceChat);
        assert_eq!(l3.task(), TaskKind::GameNpcIdle);

        // Class derives correctly per task.
        assert_eq!(l1.class(), LaneClass::Interactive);
        assert_eq!(l2.class(), LaneClass::Realtime); // pinned!
        assert_eq!(l3.class(), LaneClass::Background);

        // KV budgets per recipe table.
        assert_eq!(l1.seed_kv_tokens(), 8 * 1024);
        assert_eq!(l2.seed_kv_tokens(), 8 * 1024);
        assert_eq!(l3.seed_kv_tokens(), 4 * 1024);

        // Pinned status follows class.
        assert!(l2.is_pinned());
        assert!(!l1.is_pinned());
        assert!(!l3.is_pinned());
    }

    // ── observability + inspection ──────────────────────────────

    #[test]
    fn inspect_returns_full_snapshot_for_known_handle() {
        let c = build_coordinator();
        let h = open_chat(&c, 7, 1_500_000).unwrap();
        let inspection = c.inspect(&h).unwrap();
        assert_eq!(inspection.persona, persona(7));
        assert_eq!(inspection.task, TaskKind::Chat);
        assert_eq!(inspection.class, LaneClass::Interactive);
        assert_eq!(inspection.handle_id, h.id.as_uuid());
        assert_eq!(inspection.seed_kv_tokens, 8 * 1024);
        assert_eq!(inspection.max_kv_tokens, 16 * 1024);
        assert_eq!(inspection.bytes_accounted, 8 * 1024); // small config bytes_per_token=1
        assert_eq!(inspection.lease_acquired_at_ms, 1_500_000);
        assert_eq!(inspection.lease_expires_at_ms, 1_500_000 + 60_000);
        assert!(!inspection.is_pinned);
    }

    #[test]
    fn inspect_unknown_handle_returns_none() {
        let c = build_coordinator();
        let phantom = HandleRef::mint(
            crate::inference::handle_store::HANDLE_OWNER,
            crate::inference::handle_store::HANDLE_TYPE_TAG,
        );
        assert!(c.inspect(&phantom).is_none());
    }

    // ── class override ──────────────────────────────────────────

    // ── capture sink ────────────────────────────────────────────

    #[test]
    fn capture_sink_records_lane_opened_event_on_successful_open() {
        let sink = Arc::new(InMemoryLaneCaptureSink::new(64));
        let c = InferenceCoordinator::new(
            Arc::new(FootprintRegistry::new()),
            Arc::new(InferenceHandleStore::new()),
            small_budget_config(),
        )
        .with_capture_sink(sink.clone());
        let h = open_chat(&c, 1, 1_000_000).unwrap();
        let events = sink.snapshot();
        assert_eq!(events.len(), 1);
        match &events[0] {
            LaneCaptureEvent::LaneOpened {
                persona: p,
                task,
                class,
                handle_id,
                cost_units,
                target_silicon,
                ..
            } => {
                assert_eq!(*p, persona(1));
                assert_eq!(*task, TaskKind::Chat);
                assert_eq!(*class, LaneClass::Interactive);
                assert_eq!(*handle_id, h.id.as_uuid());
                assert_eq!(*cost_units, 8 * 1024);
                assert_eq!(*target_silicon, TargetSilicon::Cpu);
            }
            other => panic!("expected LaneOpened, got {other:?}"),
        }
    }

    #[test]
    fn capture_sink_records_admission_denied_with_reason() {
        let sink = Arc::new(InMemoryLaneCaptureSink::new(64));
        let c = InferenceCoordinator::new(
            Arc::new(FootprintRegistry::new()),
            Arc::new(InferenceHandleStore::new()),
            small_budget_config(),
        )
        .with_capture_sink(sink.clone());
        open_chat(&c, 1, 1_000_000).unwrap();
        open_chat(&c, 2, 1_000_000).unwrap();
        // Third one denies.
        let _ = open_chat(&c, 3, 1_000_000).unwrap_err();
        let events = sink.snapshot();
        assert_eq!(events.len(), 3); // 2 opened + 1 denied
        match &events[2] {
            LaneCaptureEvent::LaneAdmissionDenied {
                reason,
                persona: p,
                task,
                ..
            } => {
                assert_eq!(*reason, AdmissionDenyReason::ResourcePressure);
                assert_eq!(*p, persona(3));
                assert_eq!(*task, TaskKind::Chat);
            }
            other => panic!("expected LaneAdmissionDenied, got {other:?}"),
        }
    }

    #[test]
    fn capture_sink_records_lane_closed_with_was_present_flag() {
        let sink = Arc::new(InMemoryLaneCaptureSink::new(64));
        let c = InferenceCoordinator::new(
            Arc::new(FootprintRegistry::new()),
            Arc::new(InferenceHandleStore::new()),
            small_budget_config(),
        )
        .with_capture_sink(sink.clone());
        let h = open_chat(&c, 7, 1_000_000).unwrap();
        sink.drain(); // forget the open event
        c.close_lane(&h).unwrap();
        c.close_lane(&h).unwrap(); // double close
        let events = sink.snapshot();
        assert_eq!(events.len(), 2);
        match &events[0] {
            LaneCaptureEvent::LaneClosed { was_present, .. } => assert!(*was_present),
            other => panic!("expected LaneClosed present, got {other:?}"),
        }
        match &events[1] {
            LaneCaptureEvent::LaneClosed { was_present, .. } => assert!(!*was_present),
            other => panic!("expected LaneClosed absent, got {other:?}"),
        }
    }

    #[test]
    fn noop_sink_drops_events_without_panic_or_alloc() {
        // Same workload as the previous test but with the default
        // Noop sink. Just verify it doesn't panic + the coordinator
        // works identically.
        let c = build_coordinator();
        let h = open_chat(&c, 1, 1_000_000).unwrap();
        assert!(c.close_lane(&h).unwrap());
    }

    #[test]
    fn in_memory_sink_capacity_drops_oldest() {
        let sink = InMemoryLaneCaptureSink::new(2);
        sink.record(LaneCaptureEvent::LaneOpened {
            captured_at_ms: 1,
            persona: persona(1),
            task: TaskKind::Chat,
            class: LaneClass::Interactive,
            handle_id: Uuid::nil(),
            lease_id: "a".to_string(),
            cost_units: 1,
            bytes_accounted: 1,
            target_silicon: TargetSilicon::Cpu,
        });
        sink.record(LaneCaptureEvent::LaneOpened {
            captured_at_ms: 2,
            persona: persona(2),
            task: TaskKind::Chat,
            class: LaneClass::Interactive,
            handle_id: Uuid::nil(),
            lease_id: "b".to_string(),
            cost_units: 1,
            bytes_accounted: 1,
            target_silicon: TargetSilicon::Cpu,
        });
        sink.record(LaneCaptureEvent::LaneOpened {
            captured_at_ms: 3,
            persona: persona(3),
            task: TaskKind::Chat,
            class: LaneClass::Interactive,
            handle_id: Uuid::nil(),
            lease_id: "c".to_string(),
            cost_units: 1,
            bytes_accounted: 1,
            target_silicon: TargetSilicon::Cpu,
        });
        // Capacity 2 → first event evicted.
        let events = sink.snapshot();
        assert_eq!(events.len(), 2);
        match &events[0] {
            LaneCaptureEvent::LaneOpened { lease_id, .. } => assert_eq!(lease_id, "b"),
            other => panic!("expected lease 'b', got {other:?}"),
        }
    }

    // ── pressure-driven eviction (step 5) ───────────────────────

    fn open_with_class(
        c: &InferenceCoordinator,
        persona_id: u128,
        task: TaskKind,
        class: LaneClass,
        now_ms: u64,
    ) -> HandleRef {
        c.open_lane(OpenLaneRequest {
            persona: persona(persona_id),
            task,
            adapter: Arc::new(HeuristicInferenceAdapter::new()),
            model: None,
            system_prompt: None,
            active_adapters: None,
            class_override: Some(class),
            now_ms,
        })
        .unwrap()
    }

    fn open_chat_now(c: &InferenceCoordinator, persona_id: u128, now_ms: u64) -> HandleRef {
        c.open_lane(OpenLaneRequest {
            persona: persona(persona_id),
            task: TaskKind::Chat,
            adapter: Arc::new(HeuristicInferenceAdapter::new()),
            model: None,
            system_prompt: None,
            active_adapters: None,
            class_override: None,
            now_ms,
        })
        .unwrap()
    }

    fn eviction_config() -> CoordinatorConfig {
        // Generous concurrency so we can open many lanes before
        // evicting; modest cost_units so multiple lanes fit. Long
        // lease so a typical now=1.5M wall-clock test stays within
        // the lease window (the expired-lease test uses now past
        // acquisition + lease_duration to force expiry).
        CoordinatorConfig {
            lane_budgets: vec![ThroughputLaneBudget {
                resource_class: ResourceClass::LocalGeneration,
                target_silicon: TargetSilicon::Cpu,
                max_concurrency: 16,
                max_cost_units: 200_000,
            }],
            // 1 byte per token so bytes-freed numbers in tests are
            // just seed_kv_tokens (8K Chat → 8192 bytes etc.).
            bytes_per_token: 1,
            lease_duration_ms: 5_000_000,
            default_target_silicon: TargetSilicon::Cpu,
        }
    }

    fn build_eviction_coordinator() -> InferenceCoordinator {
        InferenceCoordinator::new(
            Arc::new(FootprintRegistry::new()),
            Arc::new(InferenceHandleStore::new()),
            eviction_config(),
        )
    }

    #[test]
    fn evict_under_pressure_does_not_touch_pinned_realtime_lane() {
        let c = build_eviction_coordinator();
        // 1 realtime (pinned), 1 background — evict 100MB of pressure.
        let realtime = open_with_class(&c, 1, TaskKind::VoiceChat, LaneClass::Realtime, 1_000_000);
        let _background = open_with_class(
            &c,
            2,
            TaskKind::CodingSmall,
            LaneClass::Background,
            1_000_000,
        );
        let result = c.evict_under_pressure(100_000_000, 1_500_000);
        assert_eq!(result.evicted.len(), 1);
        assert_eq!(result.evicted[0].class, LaneClass::Background);
        // Realtime lane survives.
        assert!(c.lane_for_handle(&realtime).is_some());
        // Only background's bytes were freed (CodingSmall = 32K tokens).
        assert_eq!(result.bytes_freed, 32 * 1024);
        assert!(result.bytes_short > 0); // didn't reach 100MB target
    }

    #[test]
    fn evict_under_pressure_prefers_hard_then_graceful() {
        let c = build_eviction_coordinator();
        // 1 Interactive (Graceful) + 1 Background (Hard) + 1 Sentinel (Hard).
        let _interactive =
            open_with_class(&c, 1, TaskKind::Chat, LaneClass::Interactive, 1_000_000);
        let _background = open_with_class(
            &c,
            2,
            TaskKind::CodingSmall,
            LaneClass::Background,
            1_000_000,
        );
        let _sentinel = open_with_class(
            &c,
            3,
            TaskKind::SentinelEasy,
            LaneClass::Sentinel,
            1_000_000,
        );
        // Evict just one lane's worth (small budget).
        let result = c.evict_under_pressure(1, 1_500_000);
        assert_eq!(result.evicted.len(), 1);
        // First evicted = Hard (Background or Sentinel — older first within tier).
        assert!(matches!(
            result.evicted[0].reason,
            EvictionReason::PressureHard
        ));
        assert!(matches!(
            result.evicted[0].class,
            LaneClass::Background | LaneClass::Sentinel
        ));
    }

    #[test]
    fn evict_under_pressure_picks_oldest_within_same_tier() {
        let c = build_eviction_coordinator();
        // Two Background lanes, different acquired_at_ms.
        let _old = open_with_class(
            &c,
            1,
            TaskKind::CodingSmall,
            LaneClass::Background,
            1_000_000,
        );
        let _new = open_with_class(
            &c,
            2,
            TaskKind::CodingSmall,
            LaneClass::Background,
            2_000_000,
        );
        let result = c.evict_under_pressure(1, 3_000_000);
        assert_eq!(result.evicted.len(), 1);
        // Older lane (persona 1, acquired at 1M) gets evicted first.
        assert_eq!(result.evicted[0].persona, persona(1));
    }

    #[test]
    fn evict_under_pressure_collects_expired_first_even_pinned() {
        // Need ONE lane expired and one active so pressure-priority
        // would normally pick the active background, but expired
        // priority MUST pick the realtime first.
        let c = build_eviction_coordinator();
        // Realtime opens at 1M with 5M lease → expires at 6M.
        let _realtime = open_with_class(&c, 1, TaskKind::VoiceChat, LaneClass::Realtime, 1_000_000);
        // Background opens at 5M with 5M lease → expires at 10M.
        let _background = open_with_class(
            &c,
            2,
            TaskKind::CodingSmall,
            LaneClass::Background,
            5_000_000,
        );
        // Evict at 7M: realtime expired, background still active.
        let result = c.evict_under_pressure(1, 7_000_000);
        assert_eq!(result.evicted.len(), 1);
        assert_eq!(result.evicted[0].class, LaneClass::Realtime);
        assert_eq!(result.evicted[0].reason, EvictionReason::LeaseExpired);
    }

    #[test]
    fn evict_under_pressure_stops_when_target_met() {
        let c = build_eviction_coordinator();
        // 3 Background lanes, each 32K tokens = 32K bytes (with
        // bytes_per_token=1).
        for i in 1..=3 {
            open_with_class(
                &c,
                i,
                TaskKind::CodingSmall,
                LaneClass::Background,
                1_000_000,
            );
        }
        // Target 33K bytes — enough for 2 lanes but not 3.
        let result = c.evict_under_pressure(33_000, 1_500_000);
        assert_eq!(result.evicted.len(), 2);
        assert_eq!(result.bytes_freed, 64 * 1024);
        assert_eq!(result.bytes_short, 0);
        // Third lane still present.
        assert_eq!(c.lane_count(), 1);
    }

    #[test]
    fn evict_under_pressure_reports_bytes_short_when_all_pinned() {
        let c = build_eviction_coordinator();
        // 3 Realtime lanes — pressure can't touch any.
        for i in 1..=3 {
            open_with_class(&c, i, TaskKind::VoiceChat, LaneClass::Realtime, 1_000_000);
        }
        let target = 1_000_000;
        let result = c.evict_under_pressure(target, 1_500_000);
        assert_eq!(result.evicted.len(), 0);
        assert_eq!(result.bytes_freed, 0);
        assert_eq!(result.bytes_short, target);
        assert_eq!(c.lane_count(), 3); // all still present
    }

    #[test]
    fn evict_under_pressure_emits_lane_evicted_capture_with_reason() {
        let sink = Arc::new(InMemoryLaneCaptureSink::new(64));
        let c = InferenceCoordinator::new(
            Arc::new(FootprintRegistry::new()),
            Arc::new(InferenceHandleStore::new()),
            eviction_config(),
        )
        .with_capture_sink(sink.clone());
        let _ = open_chat_now(&c, 1, 1_000_000); // Interactive (Graceful)
        let _ = open_with_class(
            &c,
            2,
            TaskKind::CodingSmall,
            LaneClass::Background,
            1_000_000,
        ); // Hard
        sink.drain(); // forget the LaneOpened events
        let _result = c.evict_under_pressure(1, 1_500_000);
        let events = sink.snapshot();
        assert_eq!(events.len(), 1);
        match &events[0] {
            LaneCaptureEvent::LaneEvicted {
                reason,
                class,
                bytes_freed,
                ..
            } => {
                assert_eq!(*reason, EvictionReason::PressureHard);
                assert_eq!(*class, LaneClass::Background);
                assert_eq!(*bytes_freed, 32 * 1024);
            }
            other => panic!("expected LaneEvicted, got {other:?}"),
        }
    }

    #[test]
    fn evict_under_pressure_with_zero_target_evicts_nothing() {
        let c = build_eviction_coordinator();
        for i in 1..=3 {
            open_chat_now(&c, i, 1_000_000);
        }
        let result = c.evict_under_pressure(0, 1_500_000);
        assert_eq!(result.evicted.len(), 0);
        assert_eq!(result.bytes_freed, 0);
        assert_eq!(result.bytes_short, 0);
        assert_eq!(c.lane_count(), 3);
    }

    #[test]
    fn evict_under_pressure_on_empty_coordinator_is_noop() {
        let c = build_eviction_coordinator();
        let result = c.evict_under_pressure(1_000_000, 1_500_000);
        assert_eq!(result.evicted.len(), 0);
        assert_eq!(result.bytes_freed, 0);
        assert_eq!(result.bytes_short, 1_000_000);
    }

    #[test]
    fn evict_realistic_floor_scenario_three_personas_one_must_yield() {
        // The substrate's defining boast under pressure:
        // 3 lanes (Realtime/Interactive/Background), pressure says
        // free at least 4K bytes. The Background lane yields; the
        // Realtime + Interactive lanes stay warm. This is the
        // multi-persona-on-commodity-hardware story under load.
        let c = build_eviction_coordinator();
        let realtime = open_with_class(&c, 1, TaskKind::VoiceChat, LaneClass::Realtime, 1_000_000);
        let interactive = open_with_class(&c, 2, TaskKind::Chat, LaneClass::Interactive, 1_000_000);
        let _background = open_with_class(
            &c,
            3,
            TaskKind::GameNpcIdle,
            LaneClass::Background,
            1_000_000,
        );
        let result = c.evict_under_pressure(4 * 1024, 1_500_000);
        assert_eq!(result.evicted.len(), 1);
        assert_eq!(result.evicted[0].class, LaneClass::Background);
        assert_eq!(result.evicted[0].reason, EvictionReason::PressureHard);
        // Realtime + Interactive survive.
        assert!(c.lane_for_handle(&realtime).is_some());
        assert!(c.lane_for_handle(&interactive).is_some());
        assert_eq!(c.lane_count(), 2);
    }

    #[test]
    fn class_override_lets_daemon_promote_chat_to_realtime() {
        // A daemon can promote a Chat lane to Realtime when the
        // persona is currently in a voice-engaged state.
        let c = build_coordinator();
        let req = OpenLaneRequest {
            persona: persona(1),
            task: TaskKind::Chat,
            adapter: Arc::new(HeuristicInferenceAdapter::new()),
            model: None,
            system_prompt: None,
            active_adapters: None,
            class_override: Some(LaneClass::Realtime),
            now_ms: 1_000_000,
        };
        let h = c.open_lane(req).unwrap();
        let lane = c.lane_for_handle(&h).unwrap();
        assert_eq!(lane.class(), LaneClass::Realtime);
        assert!(lane.is_pinned());
    }
