#![cfg(test)]
    use super::*;

    fn count_sizer<V>() -> Sizer<V>
    where
        V: 'static,
    {
        Arc::new(|_| 1)
    }

    fn bytes_sizer() -> Sizer<Vec<u8>> {
        Arc::new(|v: &Vec<u8>| v.len() as u64)
    }

    #[tokio::test]
    async fn get_returns_none_on_miss_and_value_on_hit() {
        let pool: PagedResourcePool<String, Vec<u8>> = PagedResourcePool::new(PoolConfig {
            name: "test".to_string(),
            max_bytes: 1024,
            sizer: bytes_sizer(),
            eviction_priority: lru_priority(),
        });
        assert!(pool.get(&"missing".to_string()).is_none());
        pool.insert("k1".to_string(), vec![1, 2, 3]);
        assert_eq!(pool.get(&"k1".to_string()), Some(vec![1, 2, 3]));
    }

    #[tokio::test]
    async fn load_or_share_dedups_concurrent_loads() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let pool: PagedResourcePool<String, u32> = PagedResourcePool::new(PoolConfig {
            name: "test".to_string(),
            max_bytes: 1024,
            sizer: count_sizer(),
            eviction_priority: lru_priority(),
        });
        let load_count = Arc::new(AtomicU32::new(0));
        let lc1 = load_count.clone();
        let lc2 = load_count.clone();
        let lc3 = load_count.clone();
        let key = "shared".to_string();
        let f1 = pool.load_or_share(key.clone(), move |_| {
            let lc = lc1.clone();
            async move {
                lc.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                Ok(42_u32)
            }
        });
        let f2 = pool.load_or_share(key.clone(), move |_| {
            let lc = lc2.clone();
            async move {
                lc.fetch_add(1, Ordering::SeqCst);
                Ok(99_u32)
            }
        });
        let f3 = pool.load_or_share(key.clone(), move |_| {
            let lc = lc3.clone();
            async move {
                lc.fetch_add(1, Ordering::SeqCst);
                Ok(7_u32)
            }
        });
        let (r1, r2, r3) = tokio::join!(f1, f2, f3);
        assert_eq!(r1.unwrap(), 42);
        assert_eq!(r2.unwrap(), 42);
        assert_eq!(r3.unwrap(), 42);
        // All three callers shared one load — counter should be 1, not 3.
        assert_eq!(load_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn pin_prevents_eviction_under_pressure() {
        let pool: PagedResourcePool<String, Vec<u8>> = PagedResourcePool::new(PoolConfig {
            name: "test".to_string(),
            max_bytes: 100,
            sizer: bytes_sizer(),
            eviction_priority: lru_priority(),
        });
        pool.insert("pinned".to_string(), vec![0; 50]);
        let _handle = pool.pin(&"pinned".to_string()).expect("pin should succeed");
        // Push way over budget with unpinned entries.
        for i in 0..5 {
            pool.insert(format!("transient_{i}"), vec![0; 80]);
        }
        // Pinned entry must survive.
        assert!(pool.get(&"pinned".to_string()).is_some());
    }

    #[tokio::test]
    async fn maybe_evict_keeps_total_within_max_bytes() {
        let pool: PagedResourcePool<String, Vec<u8>> = PagedResourcePool::new(PoolConfig {
            name: "test".to_string(),
            max_bytes: 100,
            sizer: bytes_sizer(),
            eviction_priority: lru_priority(),
        });
        // Insert beyond budget. Eviction fires when total > max_bytes,
        // drops back to 75% target. Between firings, total can sit
        // anywhere ≤ max_bytes — that's the contract.
        for i in 0..5 {
            pool.insert(format!("k_{i}"), vec![0; 30]);
        }
        let stats = pool.stats().await;
        assert!(
            stats.total_bytes <= 100,
            "expected total_bytes <= max_bytes (100) after eviction firings, got {}",
            stats.total_bytes
        );
        assert!(
            stats.eviction_count > 0,
            "eviction should have fired at least once"
        );
    }

    // what this catches: the LRU sign inversion. evict_at_least sorts priority
    // ascending and evicts from the front, so lru_priority must give the OLDEST
    // entry the LOWEST value. The first cut negated the timestamp — MRU wearing
    // LRU's name — and every pool using the helper evicted its most-recently-
    // touched entry first (caught live 2026-08-26: the KV slot pool evicted the
    // warm slot that had JUST been refreshed). regression for that inversion.
    #[tokio::test]
    async fn lru_priority_evicts_the_oldest_not_the_newest() {
        let pool: PagedResourcePool<String, u32> = PagedResourcePool::new(PoolConfig {
            name: "lru-order".into(),
            max_bytes: 100,
            sizer: Arc::new(|_| 1),
            eviction_priority: lru_priority(),
        });
        pool.load_or_share("old".to_string(), |_| async { Ok(1u32) })
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        pool.load_or_share("new".to_string(), |_| async { Ok(2u32) })
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        // Refresh "new" so its last_access is strictly freshest.
        let _ = pool.get(&"new".to_string());
        assert_eq!(pool.evict_at_least(1), 1);
        assert!(
            pool.get(&"old".to_string()).is_none(),
            "the OLDEST entry is the one evicted"
        );
        assert!(
            pool.get(&"new".to_string()).is_some(),
            "the freshest entry survives"
        );
    }

    #[tokio::test]
    async fn eviction_drops_to_target_when_far_over() {
        let pool: PagedResourcePool<String, Vec<u8>> = PagedResourcePool::new(PoolConfig {
            name: "test".to_string(),
            max_bytes: 100,
            sizer: bytes_sizer(),
            eviction_priority: lru_priority(),
        });
        // Single insert that's WAY over budget triggers eviction down
        // to 75% target in one pass.
        for i in 0..3 {
            pool.insert(format!("warm_{i}"), vec![0; 30]);
            tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
        }
        // Now insert a big one that pushes over.
        pool.insert("big".to_string(), vec![0; 50]);
        let stats = pool.stats().await;
        // After this single eviction pass, total ≤ 75 (target).
        assert!(
            stats.total_bytes <= 75,
            "single big insert should trigger eviction to 75% target; got {}",
            stats.total_bytes
        );
    }

    #[tokio::test]
    async fn dropped_pin_handle_releases_ref_count() {
        let pool: PagedResourcePool<String, Vec<u8>> = PagedResourcePool::new(PoolConfig {
            name: "test".to_string(),
            max_bytes: 100,
            sizer: bytes_sizer(),
            eviction_priority: lru_priority(),
        });
        pool.insert("x".to_string(), vec![0; 50]);
        {
            let _handle = pool.pin(&"x".to_string()).unwrap();
            let stats = pool.stats().await;
            assert_eq!(stats.pinned_count, 1);
        }
        // _handle dropped here — pin count should return to 0.
        let stats = pool.stats().await;
        assert_eq!(stats.pinned_count, 0);
    }

    #[tokio::test]
    async fn failed_load_does_not_poison_cache() {
        let pool: PagedResourcePool<String, u32> = PagedResourcePool::new(PoolConfig {
            name: "test".to_string(),
            max_bytes: 1024,
            sizer: count_sizer(),
            eviction_priority: lru_priority(),
        });
        // First call fails.
        let r1 = pool
            .load_or_share("k".to_string(), |_| async {
                Err::<u32, String>("boom".to_string())
            })
            .await;
        assert!(r1.is_err());
        // Second call should succeed (no poisoned slot from rejection).
        let r2 = pool
            .load_or_share("k".to_string(), |_| async { Ok(123_u32) })
            .await;
        assert_eq!(r2.unwrap(), 123);
    }

    // what this catches: card d2698cdc — stats waiting for the inflight map
    // must release the entries lock while preserving its occupancy snapshot.
    #[tokio::test]
    async fn stats_pressure_tracks_occupancy() {
        let pool: PagedResourcePool<String, Vec<u8>> = PagedResourcePool::new(PoolConfig {
            name: "test".to_string(),
            max_bytes: 100,
            sizer: bytes_sizer(),
            eviction_priority: lru_priority(),
        });
        pool.insert("k".to_string(), vec![0; 25]);
        let inflight_guard = pool.inner.inflight.lock().await;
        let mut pending_stats = std::pin::pin!(pool.stats());
        assert!(futures::poll!(pending_stats.as_mut()).is_pending());
        assert!(
            pool.inner.entries.try_write().is_some(),
            "a stats wait must not retain the synchronous entries guard"
        );
        pool.insert("later".to_string(), vec![0; 10]);
        drop(inflight_guard);
        let stats = pending_stats.await;
        assert_eq!(stats.entry_count, 1, "occupancy is the pre-wait snapshot");
        assert_eq!(stats.total_bytes, 25);
        assert_eq!(stats.inflight_count, 0);
        assert!((stats.pressure - 0.25).abs() < 0.001);
        let current = pool.stats().await;
        assert_eq!(current.entry_count, 2);
        assert_eq!(current.total_bytes, 35);
    }

    #[tokio::test]
    async fn evict_at_least_frees_requested_amount_without_touching_pinned_entries() {
        let pool: PagedResourcePool<String, Vec<u8>> = PagedResourcePool::new(PoolConfig {
            name: "test".to_string(),
            max_bytes: 1_000,
            sizer: bytes_sizer(),
            eviction_priority: lru_priority(),
        });
        pool.insert("pinned".to_string(), vec![0; 100]);
        let _pin = pool.pin(&"pinned".to_string()).unwrap();
        pool.insert("a".to_string(), vec![0; 40]);
        pool.insert("b".to_string(), vec![0; 50]);
        pool.insert("c".to_string(), vec![0; 60]);

        let freed = pool.evict_at_least(75);

        assert!(
            freed >= 75,
            "expected to free at least 75 bytes, got {freed}"
        );
        assert!(pool.get(&"pinned".to_string()).is_some());
        assert_eq!(pool.stats().await.eviction_count, 2);
    }

    #[test]
    fn resource_pool_trait_exposes_uniform_control_surface() {
        let pool: PagedResourcePool<String, Vec<u8>> = PagedResourcePool::new(PoolConfig {
            name: "docker".to_string(),
            max_bytes: 500,
            sizer: bytes_sizer(),
            eviction_priority: lru_priority(),
        });
        pool.insert("image:a".to_string(), vec![0; 25]);

        let resource: &dyn ResourcePool = &pool;

        assert_eq!(resource.tier_name(), "docker");
        assert_eq!(resource.capacity_bytes(), 500);
        assert_eq!(resource.usage_bytes(), 25);
        let snapshot = resource.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].key, "image:a");
        assert_eq!(snapshot[0].size_bytes, 25);
    }

    #[test]
    fn projected_disk_capacity_allows_usage_at_threshold() {
        let result = ensure_projected_disk_capacity("docker", 900, 1_000, 50);
        assert!(
            result.is_ok(),
            "exactly 95% pressure should be allowed; got {result:?}"
        );
    }

    #[test]
    fn projected_disk_capacity_refuses_usage_over_threshold() {
        let result = ensure_projected_disk_capacity("docker", 900, 1_000, 51);
        let Err(ResourceError::DiskCapacity {
            tier,
            used_bytes,
            capacity_bytes,
            projected_bytes,
            max_pressure_basis_points,
        }) = result
        else {
            panic!("expected DiskCapacity refusal, got {result:?}");
        };

        assert_eq!(tier, "docker");
        assert_eq!(used_bytes, 900);
        assert_eq!(capacity_bytes, 1_000);
        assert_eq!(projected_bytes, 51);
        assert_eq!(
            max_pressure_basis_points,
            DISK_CAPACITY_REFUSAL_BASIS_POINTS
        );
    }

    #[test]
    fn projected_disk_capacity_refuses_saturating_overflow() {
        let result = ensure_projected_disk_capacity("docker", u64::MAX - 5, u64::MAX, 10);
        assert!(
            matches!(result, Err(ResourceError::DiskCapacity { .. })),
            "saturating projected usage over threshold must refuse, got {result:?}"
        );
    }

    #[test]
    fn projected_disk_capacity_rejects_unknown_capacity() {
        let result = ensure_projected_disk_capacity("docker", 0, 0, 1);
        let Err(ResourceError::TierUnavailable { tier, reason }) = result else {
            panic!("expected TierUnavailable for unknown capacity, got {result:?}");
        };

        assert_eq!(tier, "docker");
        assert!(
            reason.contains("capacity is unknown"),
            "reason should explain unknown capacity, got: {reason}"
        );
    }

    #[test]
    fn projected_disk_capacity_rejects_invalid_threshold() {
        let result = ensure_projected_disk_capacity_bps("docker", 0, 1_000, 1, 10_001);
        let Err(ResourceError::TierUnavailable { tier, reason }) = result else {
            panic!("expected TierUnavailable for invalid threshold, got {result:?}");
        };

        assert_eq!(tier, "docker");
        assert!(
            reason.contains("invalid disk capacity threshold"),
            "reason should explain invalid threshold, got: {reason}"
        );
    }

    #[test]
    fn resource_error_exports_ts_shape() {
        ResourceError::export_all(&ts_rs::Config::default()).unwrap();
        ResourcePoolEntry::export_all(&ts_rs::Config::default()).unwrap();
    }
