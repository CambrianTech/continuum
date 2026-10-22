#![cfg(test)]
    use super::*;
    use crate::inference::planning::adaptive_throughput::{ResourceClass, TargetSilicon};
    use crate::inference::planning::throughput_lease::{ThroughputLease, ThroughputLeaseRevocationPolicy};
    use crate::gpu::MockMonitor;
    use crate::inference::kv_quant::Residency;
    // PressureTier is already in scope via `use super::*` (module-scope import).

    fn persona_kv_key(persona_id: Uuid) -> FootprintKey {
        FootprintKey::for_persona(persona_id, ResourceType::KvCache, Residency::Active)
    }

    fn lease(
        lease_id: &str,
        target_silicon: TargetSilicon,
        cost_units: u32,
        expires_at_ms: u64,
        revocation_policy: ThroughputLeaseRevocationPolicy,
    ) -> ThroughputLease {
        ThroughputLease {
            lease_id: lease_id.to_string(),
            artifact_key: format!("artifact:{lease_id}"),
            resource_class: ResourceClass::LocalGeneration,
            target_silicon,
            holder_id: "persona:helper".to_string(),
            cost_units,
            acquired_at_ms: 100,
            expires_at_ms,
            revocation_policy,
        }
    }

    /// What this catches: add() not creating new entries OR not
    /// summing into existing ones. Both directions of the basic API.
    ///
    /// Validated 2026-04-21: changed and_modify to overwrite (not add),
    /// test fails because second add doesn't accumulate; reverted.
    #[test]
    fn add_creates_new_entry_and_sums_into_existing() {
        let reg = FootprintRegistry::new();
        let key = persona_kv_key(Uuid::new_v4());
        reg.add(key.clone(), 1000);
        assert_eq!(reg.entry_count(), 1);
        assert_eq!(reg.total_bytes(), 1000);
        reg.add(key.clone(), 500);
        assert_eq!(
            reg.entry_count(),
            1,
            "second add merges into existing entry"
        );
        assert_eq!(reg.total_bytes(), 1500);
    }

    /// What this catches: remove() leaving zero-byte ghost entries that
    /// inflate entry_count() and waste lookup time. When bytes hit 0,
    /// the entry should be removed entirely.
    ///
    /// Validated 2026-04-21: removed the should_delete branch, test
    /// fails because entry_count stays at 1 with 0 bytes; reverted.
    #[test]
    fn remove_deletes_entry_when_bytes_reach_zero() {
        let reg = FootprintRegistry::new();
        let key = persona_kv_key(Uuid::new_v4());
        reg.add(key.clone(), 1000);
        reg.remove(&key, 1000);
        assert_eq!(reg.entry_count(), 0, "zero-byte entry should be removed");
        assert_eq!(reg.total_bytes(), 0);

        reg.add(key.clone(), 1000);
        reg.remove(&key, 300);
        assert_eq!(reg.entry_count(), 1);
        assert_eq!(reg.total_bytes(), 700);
    }

    /// What this catches: persona_total summing across the wrong
    /// dimension (e.g., aggregating by resource type instead of
    /// persona). The policy uses this to answer "how big is X?" —
    /// wrong sum = wrong eviction plan.
    ///
    /// Validated 2026-04-21: changed filter to match recipe_id, test
    /// fails because cross-persona contamination shows up; reverted.
    #[test]
    fn persona_total_aggregates_across_resource_types_for_one_persona() {
        let reg = FootprintRegistry::new();
        let helper = Uuid::new_v4();
        let teacher = Uuid::new_v4();

        reg.add(
            FootprintKey::for_persona(helper, ResourceType::KvCache, Residency::Active),
            1000,
        );
        reg.add(
            FootprintKey::for_persona(helper, ResourceType::LoraAdapter, Residency::Active),
            500,
        );
        reg.add(
            FootprintKey::for_persona(teacher, ResourceType::KvCache, Residency::Active),
            2000,
        );

        assert_eq!(reg.persona_total(helper), 1500);
        assert_eq!(reg.persona_total(teacher), 2000);
        assert_eq!(reg.persona_total(Uuid::new_v4()), 0);
    }

    /// What this catches: by_resource_type aggregation losing entries
    /// (e.g., insert-vs-merge bug). Total of by_resource_type values
    /// must equal total_bytes — if not, some entry got dropped.
    ///
    /// Validated 2026-04-21: changed `+=` to `=`, test fails because
    /// the second persona's KV bytes overwrite the first; reverted.
    #[test]
    fn by_resource_type_sums_match_total_bytes() {
        let reg = FootprintRegistry::new();
        let p1 = Uuid::new_v4();
        let p2 = Uuid::new_v4();
        reg.add(
            FootprintKey::for_persona(p1, ResourceType::KvCache, Residency::Active),
            1000,
        );
        reg.add(
            FootprintKey::for_persona(p2, ResourceType::KvCache, Residency::Active),
            2000,
        );
        reg.add(
            FootprintKey::for_persona(p1, ResourceType::LoraAdapter, Residency::Active),
            500,
        );
        reg.add(
            FootprintKey::shared(ResourceType::ModelWeights, Residency::Active),
            2_500_000_000,
        );

        let by_type = reg.by_resource_type();
        let sum: u64 = by_type.values().sum();
        assert_eq!(sum, reg.total_bytes(), "by_type sum must equal total");
        assert_eq!(by_type.get(&ResourceType::KvCache).copied(), Some(3000));
        assert_eq!(by_type.get(&ResourceType::LoraAdapter).copied(), Some(500));
        assert_eq!(
            by_type.get(&ResourceType::ModelWeights).copied(),
            Some(2_500_000_000)
        );
    }

    /// What this catches: report_authoritative not flipping the
    /// `backend_reported` flag, which would prevent sanity_check from
    /// distinguishing ground-truth entries from accounting drift.
    ///
    /// Validated 2026-04-21: removed the backend_reported = true line,
    /// test fails because the flag stays false; reverted.
    #[test]
    fn report_authoritative_marks_entry_as_backend_reported() {
        let reg = FootprintRegistry::new();
        let key = persona_kv_key(Uuid::new_v4());
        reg.add(key.clone(), 500);
        let initial = reg.entries.get(&key).unwrap().clone();
        assert!(!initial.backend_reported);

        reg.report_authoritative(key.clone(), 1000);
        let after = reg.entries.get(&key).unwrap().clone();
        assert!(
            after.backend_reported,
            "authoritative report should flip the flag"
        );
        assert_eq!(
            after.bytes, 1000,
            "authoritative report overwrites, doesn't add"
        );
    }

    /// What this catches: cheapest_eviction_for picking expensive
    /// entries before cheap ones (sort direction wrong, or cost-per-byte
    /// computation inverted). Greedy ordering MUST be ascending cost.
    ///
    /// Validated 2026-04-21: reversed sort (descending), test fails
    /// because the model_weights entry (high cost) appears in the plan
    /// when KV (low cost) would have sufficed; reverted.
    #[test]
    fn cheapest_eviction_picks_lowest_cost_per_byte_first() {
        let reg = FootprintRegistry::new();
        let p1 = Uuid::new_v4();
        reg.add(
            FootprintKey::for_persona(p1, ResourceType::KvCache, Residency::Active),
            1_000_000,
        );
        reg.add(
            FootprintKey::shared(ResourceType::ModelWeights, Residency::Active),
            2_500_000_000,
        );

        let plan = reg
            .cheapest_eviction_for(500_000, &[])
            .expect("plan should exist");
        assert!(plan.bytes_freed >= 500_000);
        let has_model = plan
            .entries
            .iter()
            .any(|(k, _)| matches!(k.resource_type, ResourceType::ModelWeights));
        assert!(
            !has_model,
            "shouldn't evict model weights when KV alone suffices"
        );
    }

    /// What this catches: cheapest_eviction_for ignoring the
    /// exclude_personas filter and evicting the active speaker. The
    /// policy uses this to protect the currently-speaking persona;
    /// failure here = mid-conversation eviction.
    ///
    /// Validated 2026-04-21: removed the contains() check, test fails
    /// because the active speaker's KV appears in the plan; reverted.
    #[test]
    fn cheapest_eviction_respects_exclude_personas() {
        let reg = FootprintRegistry::new();
        let active = Uuid::new_v4();
        let idle = Uuid::new_v4();
        reg.add(
            FootprintKey::for_persona(active, ResourceType::KvCache, Residency::Active),
            1_000_000,
        );
        reg.add(
            FootprintKey::for_persona(idle, ResourceType::KvCache, Residency::Active),
            1_000_000,
        );

        let plan = reg
            .cheapest_eviction_for(500_000, &[active])
            .expect("plan exists");
        for (key, _) in &plan.entries {
            assert_ne!(
                key.persona_id,
                Some(active),
                "active speaker must not appear in eviction plan"
            );
        }
    }

    /// What this catches: cheapest_eviction_for returning a partial
    /// plan when target is unachievable (silently under-delivers).
    /// The policy needs `None` so it can surface a clear error to
    /// the user instead of evicting half what's needed.
    ///
    /// Validated 2026-04-21: returned Some(partial_plan), test fails
    /// because partial plan is the wrong contract; reverted.
    #[test]
    fn cheapest_eviction_returns_none_when_target_unachievable() {
        let reg = FootprintRegistry::new();
        let p = Uuid::new_v4();
        reg.add(
            FootprintKey::for_persona(p, ResourceType::KvCache, Residency::Active),
            1000,
        );

        let plan = reg.cheapest_eviction_for(1_000_000, &[]);
        assert!(
            plan.is_none(),
            "should return None when target can't be reached"
        );
    }

    /// What this catches: target_bytes=0 panic / inefficient processing.
    /// Edge case: policy queries "free 0 bytes" should return an empty
    /// plan immediately, not iterate the whole registry.
    ///
    /// Validated 2026-04-21: removed the early-return, test still
    /// passes because empty plan is computed correctly; but it iterates
    /// unnecessarily. Kept the early-return for clarity + perf.
    #[test]
    fn cheapest_eviction_zero_target_returns_empty_plan() {
        let reg = FootprintRegistry::new();
        reg.add(persona_kv_key(Uuid::new_v4()), 1000);
        let plan = reg
            .cheapest_eviction_for(0, &[])
            .expect("zero target should yield empty plan");
        assert!(plan.entries.is_empty());
        assert_eq!(plan.bytes_freed, 0);
    }

    /// What this catches: sanity_check incorrectly reporting Healthy
    /// when registry total drifts significantly from monitor's
    /// process_bytes. The policy uses this signal to flag "something
    /// allocates without reporting" bugs.
    ///
    /// Validated 2026-04-21: changed > to <, test fails because
    /// Drifted scenario reports Healthy; reverted.
    #[test]
    fn sanity_check_detects_drift_above_threshold() {
        let reg = FootprintRegistry::new();
        let monitor = MockMonitor::new(8 * 1024 * 1024 * 1024);

        reg.add(persona_kv_key(Uuid::new_v4()), 1_000_000_000);
        monitor.set_process_bytes(1_050_000_000);
        let health = reg.sanity_check(&monitor, 10.0);
        assert!(matches!(health, RegistryHealth::Healthy { .. }));

        monitor.set_process_bytes(2_000_000_000);
        let drifted = reg.sanity_check(&monitor, 10.0);
        match drifted {
            RegistryHealth::Drifted {
                registry_total,
                monitor_process_bytes,
                drift_pct,
            } => {
                assert_eq!(registry_total, 1_000_000_000);
                assert_eq!(monitor_process_bytes, 2_000_000_000);
                assert!(drift_pct > 40.0, "drift should be ~50%, got {drift_pct}");
            }
            _ => panic!("expected Drifted, got {drifted:?}"),
        }
    }

    /// What this catches: `snapshot()` returning numbers that disagree
    /// with the live accessors. Single-pass aggregation MUST match what
    /// `total_bytes()`, `by_resource_type()`, and `persona_total()`
    /// return — otherwise telemetry shows one number while the policy
    /// makes decisions on a different one.
    ///
    /// Validated 2026-04-21: changed by_persona insertion to skip the
    /// persona_id (treating shared keys as person-attributed), test fails
    /// because by_persona contains ghost entries for shared keys; reverted.
    #[test]
    fn snapshot_matches_live_accessors() {
        let reg = FootprintRegistry::new();
        let p1 = Uuid::new_v4();
        let p2 = Uuid::new_v4();
        reg.add(
            FootprintKey::for_persona(p1, ResourceType::KvCache, Residency::Active),
            1000,
        );
        reg.add(
            FootprintKey::for_persona(p1, ResourceType::LoraAdapter, Residency::Active),
            500,
        );
        reg.add(
            FootprintKey::for_persona(p2, ResourceType::KvCache, Residency::Active),
            2000,
        );
        reg.add(
            FootprintKey::shared(ResourceType::ModelWeights, Residency::Active),
            2_500_000_000,
        );

        let snap = reg.snapshot();
        assert_eq!(snap.total_bytes, reg.total_bytes());
        assert_eq!(snap.entry_count, reg.entry_count());
        assert_eq!(snap.by_resource_type, reg.by_resource_type());
        assert_eq!(
            snap.by_persona.get(&p1).copied(),
            Some(reg.persona_total(p1))
        );
        assert_eq!(
            snap.by_persona.get(&p2).copied(),
            Some(reg.persona_total(p2))
        );
        assert_eq!(
            snap.by_persona.values().sum::<u64>(),
            1500 + 2000,
            "by_persona sum excludes the shared model_weights entry"
        );
    }

    /// What this catches: `snapshot()` reading from a stale live view.
    /// Snapshot must reflect ALL writes that completed before snapshot()
    /// returned, even ones interleaved with reads.
    ///
    /// Validated 2026-04-21: implicit — single-pass DashMap iteration is
    /// the only implementation that satisfies this; alternative designs
    /// (cached snapshot updated on write) would race.
    #[test]
    fn snapshot_reflects_writes_completed_before_call() {
        let reg = FootprintRegistry::new();
        let p = Uuid::new_v4();
        let snap_empty = reg.snapshot();
        assert_eq!(snap_empty.total_bytes, 0);
        assert_eq!(snap_empty.entry_count, 0);

        reg.add(
            FootprintKey::for_persona(p, ResourceType::KvCache, Residency::Active),
            4242,
        );
        let snap_after = reg.snapshot();
        assert_eq!(snap_after.total_bytes, 4242);
        assert_eq!(snap_after.entry_count, 1);
        assert_eq!(snap_after.by_persona.get(&p).copied(), Some(4242));
    }

    /// What this catches: `global()` returning fresh registries on each
    /// call (i.e., not actually a singleton). The whole reporting
    /// substrate depends on every caller seeing the same map.
    ///
    /// Validated 2026-04-21: changed get_or_init to FootprintRegistry::new
    /// in a non-singleton helper, test fails because second call's
    /// total_bytes is 0 (didn't see the first add); reverted.
    #[test]
    fn global_is_a_singleton_across_calls() {
        let r1 = global();
        let r2 = global();
        assert!(
            std::ptr::eq(r1, r2),
            "global() must return the same instance on every call"
        );

        let persona = Uuid::new_v4();
        let key = FootprintKey::for_persona(persona, ResourceType::KvCache, Residency::Active);
        let before = r1.persona_total(persona);
        r1.add(key.clone(), 1234);
        let after = r2.persona_total(persona);
        assert_eq!(
            after - before,
            1234,
            "writes through r1 must be visible via r2 (same instance)"
        );
        r2.remove(&key, 1234);
    }

    /// What this catches: `try_global()` lazy-initializing the registry.
    #[test]
    fn try_global_returns_same_instance_as_global_when_initialized() {
        let g = global();
        let tg = try_global().expect("global was just initialized");
        assert!(
            std::ptr::eq(g, tg),
            "try_global must point at the same OnceLock cell"
        );
    }

    /// What this catches: concurrent add/remove from multiple "personas"
    /// causing data races or lost updates. DashMap is sharded internally,
    /// but this test exercises that no top-level state goes through a
    /// mutex our code accidentally added.
    ///
    /// Validated 2026-04-21: implicit — if DashMap weren't lock-free
    /// per-shard, this test would be slow or detect races.
    #[tokio::test(flavor = "multi_thread")]
    async fn concurrent_adds_from_many_personas_do_not_lose_updates() {
        use std::sync::Arc;

        let reg = Arc::new(FootprintRegistry::new());
        let mut handles = Vec::new();
        for _ in 0..100 {
            let reg = Arc::clone(&reg);
            handles.push(tokio::spawn(async move {
                let persona = Uuid::new_v4();
                for _ in 0..10 {
                    reg.add(persona_kv_key(persona), 100);
                }
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(reg.total_bytes(), 100_000);
        assert_eq!(reg.entry_count(), 100);
    }

    #[test]
    fn acquire_and_release_lease_mirrors_footprint_bytes() {
        let reg = FootprintRegistry::new();
        let key = persona_kv_key(Uuid::new_v4());
        reg.acquire_lease(
            lease(
                "turn-1",
                TargetSilicon::Gpu,
                8,
                1_000,
                ThroughputLeaseRevocationPolicy::Graceful,
            ),
            key.clone(),
            4096,
            100,
        )
        .unwrap();

        assert_eq!(reg.total_bytes(), 4096);
        assert_eq!(reg.entry_count(), 1);
        let lease_snapshot = reg.lease_snapshot(200);
        assert_eq!(lease_snapshot.active.len(), 1);
        assert_eq!(
            lease_snapshot
                .cost_by_target_silicon
                .get(&TargetSilicon::Gpu),
            Some(&8)
        );

        let released = reg.release_lease("turn-1").unwrap();
        assert_eq!(released.lease_id, "turn-1");
        assert_eq!(reg.total_bytes(), 0);
        assert_eq!(reg.entry_count(), 0);
    }

    #[test]
    fn duplicate_lease_does_not_double_count_bytes() {
        let reg = FootprintRegistry::new();
        let key = persona_kv_key(Uuid::new_v4());
        let lease = lease(
            "turn-1",
            TargetSilicon::Gpu,
            8,
            1_000,
            ThroughputLeaseRevocationPolicy::Graceful,
        );

        reg.acquire_lease(lease.clone(), key.clone(), 4096, 100)
            .unwrap();
        assert_eq!(
            reg.acquire_lease(lease, key, 4096, 100),
            Err(ThroughputLeaseError::DuplicateLease {
                lease_id: "turn-1".to_string()
            })
        );
        assert_eq!(reg.total_bytes(), 4096);
    }

    #[test]
    fn expiring_leases_removes_their_mirrored_footprints() {
        let reg = FootprintRegistry::new();
        let old_key = persona_kv_key(Uuid::new_v4());
        let fresh_key = persona_kv_key(Uuid::new_v4());
        reg.acquire_lease(
            lease(
                "old",
                TargetSilicon::Gpu,
                4,
                150,
                ThroughputLeaseRevocationPolicy::Hard,
            ),
            old_key,
            1000,
            100,
        )
        .unwrap();
        reg.acquire_lease(
            lease(
                "fresh",
                TargetSilicon::Gpu,
                8,
                1_000,
                ThroughputLeaseRevocationPolicy::Hard,
            ),
            fresh_key,
            2000,
            100,
        )
        .unwrap();

        let snapshot = reg.lease_snapshot(200);
        assert_eq!(snapshot.active.len(), 1);
        assert_eq!(snapshot.expired.len(), 1);
        assert_eq!(reg.total_bytes(), 3000);

        let expired = reg.expire_leases(200);
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].lease_id, "old");
        assert_eq!(reg.total_bytes(), 2000);
        assert_eq!(reg.lease_snapshot(200).expired.len(), 0);
    }

    #[test]
    fn active_pinned_lease_blocks_eviction_candidate() {
        let reg = FootprintRegistry::new();
        let pinned_key = persona_kv_key(Uuid::new_v4());
        let revocable_key = persona_kv_key(Uuid::new_v4());
        reg.acquire_lease(
            lease(
                "pinned",
                TargetSilicon::Gpu,
                8,
                1_000,
                ThroughputLeaseRevocationPolicy::Pinned,
            ),
            pinned_key.clone(),
            1_000_000,
            100,
        )
        .unwrap();
        reg.acquire_lease(
            lease(
                "revocable",
                TargetSilicon::Gpu,
                1,
                1_000,
                ThroughputLeaseRevocationPolicy::Graceful,
            ),
            revocable_key,
            1_000_000,
            100,
        )
        .unwrap();

        let plan = reg
            .cheapest_eviction_for(500_000, &[])
            .expect("revocable lease should be evictable");
        for (key, _) in plan.entries {
            assert_ne!(key, pinned_key, "pinned lease must not be evicted");
        }
    }

    #[test]
    fn active_pinned_lease_can_make_eviction_unachievable() {
        let reg = FootprintRegistry::new();
        let pinned_key = persona_kv_key(Uuid::new_v4());
        reg.acquire_lease(
            lease(
                "pinned",
                TargetSilicon::Gpu,
                8,
                1_000,
                ThroughputLeaseRevocationPolicy::Pinned,
            ),
            pinned_key,
            1_000_000,
            100,
        )
        .unwrap();

        assert!(
            reg.cheapest_eviction_for(500_000, &[]).is_none(),
            "only pinned bytes exist, so eviction should fail loud"
        );
    }

    // ─── revoke_leases_for — pressure-driven lease revocation (slice 2) ──

    /// What this catches: `revoke_leases_for` reclaims footprint that
    /// `cheapest_eviction_for` CANNOT — specifically an *expired* Pinned
    /// lease. Key eviction skips any Pinned-policy key (it doesn't re-check
    /// expiry), so those bytes are stranded; the revocation ladder ranks an
    /// expired lease rank-0 and reclaims it even at Normal tier. This
    /// complementarity is exactly what the two-phase relief depends on.
    #[test]
    fn revoke_reclaims_expired_pinned_that_key_eviction_strands() {
        let reg = FootprintRegistry::new();
        let key = persona_kv_key(Uuid::new_v4());
        reg.acquire_lease(
            lease(
                "expired-pin",
                TargetSilicon::Gpu,
                8,
                150,
                ThroughputLeaseRevocationPolicy::Pinned,
            ),
            key,
            1_000_000,
            100,
        )
        .unwrap();

        // now=200 → lease expired. Key eviction still refuses (Pinned policy).
        assert!(
            reg.cheapest_eviction_for(500_000, &[]).is_none(),
            "cheapest_eviction_for strands the Pinned key even when expired"
        );

        // Revocation ladder reclaims it (expired = rank 0, eligible at Normal).
        let outcome = reg
            .revoke_leases_for(500_000, PressureTier::Normal, 200)
            .expect("expired lease is reclaimable");
        assert_eq!(outcome.bytes_freed, 1_000_000);
        assert_eq!(
            outcome.revoked,
            vec![("expired-pin".to_string(), 1_000_000)]
        );
        assert_eq!(reg.total_bytes(), 0, "footprint actually returned");
    }

    /// What this catches: when policy cannot satisfy the demand (only an
    /// ACTIVE Pinned lease remains), `revoke_leases_for` returns `None` AND
    /// releases nothing — the footprint is untouched. A `None` that still
    /// mutated would be a silent pinned-lease revocation, the exact safety
    /// violation the ladder exists to prevent.
    #[test]
    fn revoke_none_leaves_active_pinned_untouched() {
        let reg = FootprintRegistry::new();
        reg.acquire_lease(
            lease(
                "pinned",
                TargetSilicon::Gpu,
                8,
                9_999,
                ThroughputLeaseRevocationPolicy::Pinned,
            ),
            persona_kv_key(Uuid::new_v4()),
            1_000_000,
            100,
        )
        .unwrap();

        assert!(reg
            .revoke_leases_for(500_000, PressureTier::Critical, 200)
            .is_none());
        assert_eq!(
            reg.total_bytes(),
            1_000_000,
            "pinned footprint must survive a None pass"
        );
        assert_eq!(reg.lease_snapshot(200).active.len(), 1);
    }

    /// What this catches: the REALIZATION (not just the pure selector)
    /// respects tier gating — an active Graceful lease is left intact under
    /// Warning (`None`, nothing released) but revoked under High. Proves the
    /// release path cannot out-run the policy ceiling.
    #[test]
    fn revoke_respects_tier_ceiling_for_active_graceful() {
        let reg = FootprintRegistry::new();
        let key = persona_kv_key(Uuid::new_v4());
        reg.acquire_lease(
            lease(
                "graceful",
                TargetSilicon::Gpu,
                8,
                9_999,
                ThroughputLeaseRevocationPolicy::Graceful,
            ),
            key,
            1_000_000,
            100,
        )
        .unwrap();

        // Warning: rank-2 (active Graceful) is over the ceiling → None, untouched.
        assert!(reg
            .revoke_leases_for(500_000, PressureTier::Warning, 200)
            .is_none());
        assert_eq!(reg.total_bytes(), 1_000_000);

        // High: eligible → released.
        let outcome = reg
            .revoke_leases_for(500_000, PressureTier::High, 200)
            .expect("High may revoke Graceful");
        assert_eq!(outcome.bytes_freed, 1_000_000);
        assert_eq!(reg.total_bytes(), 0);
    }

    /// What this catches: with multiple revocable leases, the realization
    /// releases exactly the least-disruptive SUBSET the selector chose
    /// (Hard before Graceful) and frees their bytes — the other lease is
    /// left holding its footprint. Verifies `revoke_leases_for` releases a
    /// subset, not everything in sight.
    #[test]
    fn revoke_releases_only_the_selected_subset() {
        let reg = FootprintRegistry::new();
        reg.acquire_lease(
            lease(
                "hard",
                TargetSilicon::Gpu,
                8,
                9_999,
                ThroughputLeaseRevocationPolicy::Hard,
            ),
            persona_kv_key(Uuid::new_v4()),
            600_000,
            100,
        )
        .unwrap();
        reg.acquire_lease(
            lease(
                "graceful",
                TargetSilicon::Gpu,
                8,
                9_999,
                ThroughputLeaseRevocationPolicy::Graceful,
            ),
            persona_kv_key(Uuid::new_v4()),
            900_000,
            100,
        )
        .unwrap();

        // target 500k: Hard (600k) alone satisfies, less disruptive than Graceful.
        let outcome = reg
            .revoke_leases_for(500_000, PressureTier::High, 200)
            .expect("hard lease satisfies target");
        assert_eq!(outcome.revoked, vec![("hard".to_string(), 600_000)]);
        assert_eq!(outcome.bytes_freed, 600_000);

        // Graceful lease untouched — still holding its footprint.
        assert_eq!(reg.total_bytes(), 900_000);
        let snap = reg.lease_snapshot(200);
        assert_eq!(snap.active.len(), 1);
        assert_eq!(snap.active[0].lease_id, "graceful");
    }

    /// What this catches: a zero-byte demand is a no-op pass — `Some(empty)`
    /// with nothing released, NOT `None` (which means "can't satisfy"). The
    /// broker reads `None` as escalate; a zero-target `None` would spuriously
    /// escalate on a pass that should do nothing.
    #[test]
    fn revoke_zero_target_is_noop_not_none() {
        let reg = FootprintRegistry::new();
        reg.acquire_lease(
            lease(
                "hard",
                TargetSilicon::Gpu,
                8,
                9_999,
                ThroughputLeaseRevocationPolicy::Hard,
            ),
            persona_kv_key(Uuid::new_v4()),
            600_000,
            100,
        )
        .unwrap();

        let outcome = reg
            .revoke_leases_for(0, PressureTier::Critical, 200)
            .expect("zero target is a no-op, not unachievable");
        assert!(outcome.revoked.is_empty());
        assert_eq!(outcome.bytes_freed, 0);
        assert_eq!(
            reg.total_bytes(),
            600_000,
            "nothing released on a zero-target pass"
        );
    }
