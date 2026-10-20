    use super::*;

    fn make_adapter(
        name: &str,
        domain: &str,
        size_mb: f32,
        priority: f32,
        loaded: bool,
        last_used_ms: u64,
    ) -> GenomeAdapterInfo {
        GenomeAdapterInfo {
            name: name.to_string(),
            domain: domain.to_string(),
            size_mb,
            priority,
            is_loaded: loaded,
            last_used_ms,
            trained_model_name: Some(format!("{}:7b", name)),
            compaction: None,
        }
    }

    // ── Eviction Scoring ──────────────────────────────────────────────

    #[test]
    fn test_critical_adapter_never_evicted() {
        let adapter = make_adapter("critical", "code", 50.0, 0.95, true, 0);
        let score = calculate_eviction_score_at(&adapter, 100_000);
        assert!(
            score.is_infinite(),
            "Critical adapter should return INFINITY"
        );
    }

    #[test]
    fn test_eviction_score_formula() {
        // age = 60s, priority = 0.5 → score = 60 / (0.5 * 10) = 12.0
        let adapter = make_adapter("test", "code", 50.0, 0.5, true, 0);
        let score = calculate_eviction_score_at(&adapter, 60_000);
        assert!(
            (score - 12.0).abs() < 0.001,
            "Score should be 12.0, got {score}"
        );
    }

    #[test]
    fn test_eviction_score_higher_priority_harder_to_evict() {
        let low = make_adapter("low", "code", 50.0, 0.3, true, 0);
        let high = make_adapter("high", "code", 50.0, 0.8, true, 0);
        let now = 60_000;

        let low_score = calculate_eviction_score_at(&low, now);
        let high_score = calculate_eviction_score_at(&high, now);

        assert!(
            low_score > high_score,
            "Low priority ({low_score}) should have higher eviction score than high priority ({high_score})"
        );
    }

    #[test]
    fn test_eviction_score_older_more_evictable() {
        let old = make_adapter("old", "code", 50.0, 0.5, true, 0);
        let recent = make_adapter("recent", "code", 50.0, 0.5, true, 50_000);
        let now = 60_000;

        let old_score = calculate_eviction_score_at(&old, now);
        let recent_score = calculate_eviction_score_at(&recent, now);

        assert!(
            old_score > recent_score,
            "Older adapter ({old_score}) should be more evictable than recent ({recent_score})"
        );
    }

    #[test]
    fn test_eviction_score_just_used_is_zero() {
        let just_used = make_adapter("fresh", "code", 50.0, 0.5, true, 60_000);
        let score = calculate_eviction_score_at(&just_used, 60_000);
        assert!(
            (score - 0.0).abs() < 0.001,
            "Just-used adapter should have score ≈ 0, got {score}"
        );
    }

    // ── Engine: Cache Hit ─────────────────────────────────────────────

    #[test]
    fn test_activate_skill_cache_hit() {
        let mut engine = GenomePagingEngine::new(200.0);
        engine.active.insert(
            "ts-expert".into(),
            make_adapter("ts-expert", "code", 50.0, 0.5, true, 1000),
        );

        let result = engine.activate_skill("ts-expert", 2000);

        assert!(result.activated);
        assert_eq!(result.adapter_name, "ts-expert");
        assert!(result.evicted.is_empty());
        assert!(
            result.to_load.is_none(),
            "Cache hit should not need to load"
        );
        // Verify last_used was updated
        assert_eq!(engine.active.get("ts-expert").unwrap().last_used_ms, 2000);
    }

    // ── Engine: Unknown Skill ─────────────────────────────────────────

    #[test]
    fn test_activate_unknown_skill() {
        let mut engine = GenomePagingEngine::new(200.0);
        let result = engine.activate_skill("nonexistent", 1000);

        assert!(!result.activated);
        assert!(result.to_load.is_none());
    }

    // ── Engine: Simple Load ───────────────────────────────────────────

    #[test]
    fn test_activate_skill_loads_from_available() {
        let mut engine = GenomePagingEngine::new(200.0);
        engine.available.insert(
            "ts-expert".into(),
            make_adapter("ts-expert", "code", 50.0, 0.5, false, 0),
        );

        let result = engine.activate_skill("ts-expert", 5000);

        assert!(result.activated);
        assert_eq!(result.to_load, Some("ts-expert".to_string()));
        assert!(result.evicted.is_empty());
        // Verify moved to active
        assert!(engine.active.contains_key("ts-expert"));
        assert!(!engine.available.contains_key("ts-expert"));
        assert!((engine.memory_used_mb - 50.0).abs() < 0.001);
    }

    // ── Engine: Eviction Required ─────────────────────────────────────

    #[test]
    fn test_activate_skill_evicts_lru_when_full() {
        let mut engine = GenomePagingEngine::new(100.0);
        // Load two 50MB adapters (fills 100MB budget)
        engine.active.insert(
            "old-adapter".into(),
            make_adapter("old-adapter", "chat", 50.0, 0.5, true, 1000),
        );
        engine.active.insert(
            "newer-adapter".into(),
            make_adapter("newer-adapter", "code", 50.0, 0.5, true, 5000),
        );
        engine.memory_used_mb = 100.0;
        // Want to load a third
        engine.available.insert(
            "incoming".into(),
            make_adapter("incoming", "creative", 50.0, 0.5, false, 0),
        );

        let result = engine.activate_skill("incoming", 10_000);

        assert!(result.activated);
        assert_eq!(result.to_load, Some("incoming".to_string()));
        assert_eq!(result.evicted.len(), 1);
        assert_eq!(
            result.evicted[0], "old-adapter",
            "Should evict oldest (last_used=1000)"
        );
        // old-adapter moved to available
        assert!(engine.available.contains_key("old-adapter"));
        assert!(!engine.active.contains_key("old-adapter"));
        // incoming now active
        assert!(engine.active.contains_key("incoming"));
        assert!(
            (engine.memory_used_mb - 100.0).abs() < 0.001,
            "Should still be at 100MB"
        );
    }

    #[test]
    fn test_activate_skill_evicts_multiple_if_needed() {
        let mut engine = GenomePagingEngine::new(200.0);
        // 4 × 50MB = 200MB (full)
        engine.active.insert(
            "a1".into(),
            make_adapter("a1", "code", 50.0, 0.3, true, 1000),
        );
        engine.active.insert(
            "a2".into(),
            make_adapter("a2", "chat", 50.0, 0.4, true, 2000),
        );
        engine.active.insert(
            "a3".into(),
            make_adapter("a3", "creative", 50.0, 0.5, true, 3000),
        );
        engine.active.insert(
            "a4".into(),
            make_adapter("a4", "social", 50.0, 0.6, true, 4000),
        );
        engine.memory_used_mb = 200.0;
        // Big adapter needs 120MB → need used + 120 <= 200 → need to free 120MB → 3 × 50MB
        engine.available.insert(
            "big".into(),
            make_adapter("big", "analysis", 120.0, 0.5, false, 0),
        );

        let result = engine.activate_skill("big", 10_000);

        assert!(result.activated);
        assert_eq!(
            result.evicted.len(),
            3,
            "Should evict 3 adapters to free 150MB for 120MB incoming"
        );
        // a1 (priority=0.3, oldest) should be first evicted
        assert!(
            result.evicted.contains(&"a1".to_string()),
            "a1 (priority=0.3, oldest) should be evicted"
        );
    }

    #[test]
    fn test_critical_adapters_survive_eviction() {
        let mut engine = GenomePagingEngine::new(100.0);
        // Critical adapter + normal adapter fill budget
        engine.active.insert(
            "critical".into(),
            make_adapter("critical", "code", 50.0, 0.95, true, 1000),
        );
        engine.active.insert(
            "normal".into(),
            make_adapter("normal", "chat", 50.0, 0.5, true, 2000),
        );
        engine.memory_used_mb = 100.0;
        engine.available.insert(
            "incoming".into(),
            make_adapter("incoming", "creative", 50.0, 0.5, false, 0),
        );

        let result = engine.activate_skill("incoming", 10_000);

        assert!(result.activated);
        assert_eq!(result.evicted, vec!["normal".to_string()]);
        assert!(
            engine.active.contains_key("critical"),
            "Critical adapter should survive"
        );
    }

    // ── Engine: Pressure-Driven Eviction (broker lever) ──────────────

    #[test]
    fn test_evict_under_pressure_no_op_when_below_target() {
        let mut engine = GenomePagingEngine::new(100.0);
        engine
            .active
            .insert("a".into(), make_adapter("a", "code", 30.0, 0.5, true, 1000));
        engine.memory_used_mb = 30.0; // pressure = 0.30
        let bytes_freed = engine.evict_under_pressure(0.75);
        assert_eq!(bytes_freed, 0, "below-target should not evict");
        assert!(engine.active.contains_key("a"));
    }

    #[test]
    fn test_evict_under_pressure_drops_lru_until_target() {
        let mut engine = GenomePagingEngine::new(100.0);
        // Three normal adapters at 30 MB each = 90% pressure.
        engine.active.insert(
            "oldest".into(),
            make_adapter("oldest", "code", 30.0, 0.5, true, 1000),
        );
        engine.active.insert(
            "middle".into(),
            make_adapter("middle", "chat", 30.0, 0.5, true, 5000),
        );
        engine.active.insert(
            "newest".into(),
            make_adapter("newest", "creative", 30.0, 0.5, true, 9000),
        );
        engine.memory_used_mb = 90.0; // pressure = 0.90
                                      // Drop to ≤ 0.50. Need to free until used ≤ 50 MB → drop two.
        let bytes_freed = engine.evict_under_pressure(0.50);
        assert!(bytes_freed > 0);
        assert!(engine.memory_used_mb <= 50.0);
        // Oldest goes first (highest score), then middle.
        assert!(!engine.active.contains_key("oldest"));
        assert!(!engine.active.contains_key("middle"));
        assert!(engine.active.contains_key("newest"));
    }

    #[test]
    fn test_evict_under_pressure_never_drops_critical_adapters() {
        let mut engine = GenomePagingEngine::new(100.0);
        engine.active.insert(
            "critical_a".into(),
            make_adapter("critical_a", "code", 40.0, 0.95, true, 1000),
        );
        engine.active.insert(
            "critical_b".into(),
            make_adapter("critical_b", "chat", 40.0, 0.95, true, 5000),
        );
        engine.memory_used_mb = 80.0; // pressure = 0.80
                                      // Asks for 0.30 — but every remaining is critical, so loop terminates
                                      // honestly with what was achievable (zero bytes).
        let bytes_freed = engine.evict_under_pressure(0.30);
        assert_eq!(bytes_freed, 0, "all-critical pool yields nothing");
        assert!(engine.active.contains_key("critical_a"));
        assert!(engine.active.contains_key("critical_b"));
    }

    #[test]
    fn test_evict_under_pressure_releases_gpu_guard_for_each_victim() {
        let mut engine = GenomePagingEngine::new(100.0);
        // Two normals; one ancient (forced first victim), one recent.
        engine.active.insert(
            "ancient".into(),
            make_adapter("ancient", "code", 60.0, 0.5, true, 1000),
        );
        engine.active.insert(
            "recent".into(),
            make_adapter("recent", "chat", 30.0, 0.5, true, 9000),
        );
        engine.memory_used_mb = 90.0;
        // Engine has no gpu_manager set → allocation_guards stays empty,
        // but the unregister/remove path should still execute cleanly.
        let bytes_freed = engine.evict_under_pressure(0.50);
        assert!(
            bytes_freed >= 60 * 1024 * 1024,
            "freed at least the ancient adapter"
        );
        assert!(!engine.active.contains_key("ancient"));
        assert!(
            engine.available.contains_key("ancient"),
            "evicted moves to available"
        );
    }

    // ── Engine: Sync State ────────────────────────────────────────────

    #[test]
    fn test_sync_state_replaces_all() {
        let mut engine = GenomePagingEngine::new(200.0);
        engine.active.insert(
            "old".into(),
            make_adapter("old", "code", 50.0, 0.5, true, 1000),
        );
        engine.memory_used_mb = 50.0;

        engine.sync_state(vec![
            make_adapter("new-active", "code", 60.0, 0.5, true, 5000),
            make_adapter("new-available", "chat", 40.0, 0.5, false, 0),
        ]);

        assert!(!engine.active.contains_key("old"));
        assert!(engine.active.contains_key("new-active"));
        assert!(engine.available.contains_key("new-available"));
        assert!((engine.memory_used_mb - 60.0).abs() < 0.001);
    }

    // ── Engine: Memory Pressure ───────────────────────────────────────

    #[test]
    fn test_memory_pressure_calculation() {
        let mut engine = GenomePagingEngine::new(200.0);
        assert!((engine.memory_pressure() - 0.0).abs() < 0.001);

        engine.memory_used_mb = 100.0;
        assert!((engine.memory_pressure() - 0.5).abs() < 0.001);

        engine.memory_used_mb = 200.0;
        assert!((engine.memory_pressure() - 1.0).abs() < 0.001);

        // Over budget capped at 1.0
        engine.memory_used_mb = 250.0;
        assert!((engine.memory_pressure() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_memory_pressure_zero_budget() {
        let engine = GenomePagingEngine::new(0.0);
        assert!((engine.memory_pressure() - 0.0).abs() < 0.001);
    }

    // ── Engine: State Snapshot ─────────────────────────────────────────

    #[test]
    fn test_state_snapshot() {
        let mut engine = GenomePagingEngine::new(200.0);
        engine.active.insert(
            "loaded".into(),
            make_adapter("loaded", "code", 50.0, 0.5, true, 1000),
        );
        engine.available.insert(
            "disk".into(),
            make_adapter("disk", "chat", 40.0, 0.5, false, 0),
        );
        engine.memory_used_mb = 50.0;

        let state = engine.state();

        assert!((state.memory_budget_mb - 200.0).abs() < 0.001);
        assert!((state.memory_used_mb - 50.0).abs() < 0.001);
        assert!((state.memory_pressure - 0.25).abs() < 0.001);
        assert_eq!(state.active_adapters.len(), 1);
        assert_eq!(state.available_adapters.len(), 1);
    }

    // ── Engine: Decision Time ─────────────────────────────────────────

    #[test]
    fn test_decision_time_is_fast() {
        let mut engine = GenomePagingEngine::new(200.0);
        engine.available.insert(
            "test".into(),
            make_adapter("test", "code", 50.0, 0.5, false, 0),
        );

        let result = engine.activate_skill("test", 1000);

        assert!(
            result.decision_time_us < 100,
            "Decision should be <100μs, was {}μs",
            result.decision_time_us
        );
    }

    // ── Domain Activity Tracking ─────────────────────────────────────

    #[test]
    fn test_record_activity_creates_entry() {
        let mut engine = GenomePagingEngine::new(200.0);
        engine.record_activity("code", true);
        engine.record_activity("code", true);
        engine.record_activity("code", false);

        let report = engine.coverage_report();
        assert_eq!(report.total_interactions, 3);
        assert_eq!(report.gaps.len(), 1, "Code domain with no adapter = gap");
        assert_eq!(report.gaps[0].domain, "code");
        assert_eq!(report.gaps[0].interaction_count, 3);
        assert_eq!(report.gaps[0].success_count, 2);
        assert_eq!(report.gaps[0].failure_count, 1);
    }

    #[test]
    fn test_coverage_report_with_adapter() {
        let mut engine = GenomePagingEngine::new(200.0);
        engine.available.insert(
            "ts-expert".into(),
            make_adapter("ts-expert", "code", 50.0, 0.5, false, 0),
        );

        engine.record_activity("code", true);
        engine.record_activity("code", true);
        engine.record_activity("chat", true);

        let report = engine.coverage_report();
        assert_eq!(report.covered.len(), 1, "Code domain has adapter → covered");
        assert_eq!(report.gaps.len(), 1, "Chat domain has no adapter → gap");
        assert_eq!(report.total_interactions, 3);
        assert!((report.coverage_ratio - 2.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn test_coverage_report_empty() {
        let engine = GenomePagingEngine::new(200.0);
        let report = engine.coverage_report();
        assert!(report.covered.is_empty());
        assert!(report.gaps.is_empty());
        assert_eq!(report.total_interactions, 0);
        assert!(
            (report.coverage_ratio - 1.0).abs() < 0.01,
            "No activity = fully covered"
        );
    }

    #[test]
    fn test_gaps_sorted_by_interaction_count() {
        let mut engine = GenomePagingEngine::new(200.0);
        for _ in 0..5 {
            engine.record_activity("chat", true);
        }
        for _ in 0..15 {
            engine.record_activity("creative", true);
        }
        for _ in 0..2 {
            engine.record_activity("analysis", true);
        }

        let report = engine.coverage_report();
        assert_eq!(report.gaps.len(), 3);
        assert_eq!(report.gaps[0].domain, "creative", "Most active gap first");
        assert_eq!(report.gaps[1].domain, "chat");
        assert_eq!(report.gaps[2].domain, "analysis");
    }

    // ── Compacted Adapters ─────────────────────────────────────────────

    #[test]
    fn test_compacted_adapter_smaller_size() {
        let mut engine = GenomePagingEngine::new(200.0);

        // Compacted adapter is smaller — more fit in budget
        let mut compacted = make_adapter("compacted", "code", 35.0, 0.5, false, 0);
        compacted.compaction = Some(CompactionMetadata {
            parameter_reduction: 0.30,
            topology_path: "head_topology.json".to_string(),
            has_compacted_weights: true,
            original_size_mb: 50.0,
            compacted_size_mb: 35.0,
        });
        engine.available.insert("compacted".into(), compacted);

        let normal = make_adapter("normal", "chat", 50.0, 0.5, false, 0);
        engine.available.insert("normal".into(), normal);

        // Load both — compacted takes 35MB, normal 50MB = 85MB (fits in 200MB)
        engine.activate_skill("compacted", 1000);
        engine.activate_skill("normal", 2000);

        assert!((engine.memory_used_mb - 85.0).abs() < 0.001);
        assert!(
            engine.memory_pressure() < 0.5,
            "Should be well under budget"
        );
    }

    #[test]
    fn test_compacted_adapter_metadata_preserved() {
        let mut engine = GenomePagingEngine::new(200.0);
        let mut adapter = make_adapter("compacted", "code", 35.0, 0.5, false, 0);
        adapter.compaction = Some(CompactionMetadata {
            parameter_reduction: 0.25,
            topology_path: "head_topology.json".to_string(),
            has_compacted_weights: true,
            original_size_mb: 50.0,
            compacted_size_mb: 35.0,
        });
        engine.available.insert("compacted".into(), adapter);

        engine.activate_skill("compacted", 1000);

        let active = engine.active.get("compacted").unwrap();
        assert!(active.compaction.is_some());
        let meta = active.compaction.as_ref().unwrap();
        assert!((meta.parameter_reduction - 0.25).abs() < 0.001);
        assert!(meta.has_compacted_weights);
    }

    // ── ts-rs binding tests ───────────────────────────────────────────

    #[test]
    fn export_bindings_compactionmetadata() {
        let cfg = ts_rs::Config::default();
        CompactionMetadata::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_genomeadapterinfo() {
        let cfg = ts_rs::Config::default();
        GenomeAdapterInfo::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_genomepagingstate() {
        let cfg = ts_rs::Config::default();
        GenomePagingState::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_activateskillresult() {
        let cfg = ts_rs::Config::default();
        ActivateSkillResult::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_domainactivity() {
        let cfg = ts_rs::Config::default();
        DomainActivity::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_coveragereport() {
        let cfg = ts_rs::Config::default();
        CoverageReport::export_all(&cfg).unwrap();
    }
