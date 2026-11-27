#![cfg(test)]
    use super::*;
    use crate::resources::capacity::MockCapacitySource;
    use crate::resources::Ask;
    use crate::resources::consumer::{ConsumerFootprint, ReclaimRequest, ReclaimStatus};
    use std::sync::atomic::{AtomicU64, Ordering};

    // what this catches: the drift probe must fire on a MATERIAL CHANGE, not every
    // tick — a stable untracked residency logged 1/sec is the flood this fixes (live
    // 2026-07-14: 8976 identical lines). First drift fires; an unchanged/tiny-jitter
    // drift stays silent; a material move re-fires; a resolved drift resets so the
    // next residency fires again.
    #[test]
    fn drift_probe_reports_on_material_change_not_every_tick() {
        let gb: u64 = 1024 * 1024 * 1024;
        let mut last = 0u64;
        // First untracked residency → fires.
        assert!(drift_should_report(2 * gb, &mut last), "first drift fires");
        // Same drift next tick → silent (this is the 1/sec flood we're killing).
        assert!(
            !drift_should_report(2 * gb, &mut last),
            "stable drift stays silent"
        );
        // Sub-threshold jitter → still silent.
        assert!(
            !drift_should_report(2 * gb + 1024 * 1024, &mut last),
            "1MiB jitter is not material"
        );
        // A material move (a lane spin-up) → re-fires.
        assert!(
            drift_should_report(2 * gb + DRIFT_REPORT_DELTA_BYTES, &mut last),
            "material move re-fires"
        );
        // Drift resolves → silent, but state resets…
        assert!(!drift_should_report(0, &mut last), "resolution is silent");
        assert_eq!(last, 0, "resolved drift resets the baseline");
        // …so a returning residency fires again.
        assert!(
            drift_should_report(2 * gb, &mut last),
            "a returning residency re-fires"
        );
    }

    /// A scriptable consumer: holds bytes, frees per a configurable response so a
    /// scenario can make it Release / tier-down-Partial / Defer / panic on cue.
    /// This is the rung-2 fake-consumer half of the test ladder (the
    /// `CapacitySource` mock is the capacity half).
    ///
    /// The ledger reclaims **whole leases** — `request.target_bytes` is the whole
    /// victim lease, not a fraction. A consumer that wants to keep some bytes
    /// answers the whole-lease ask with a *Partial* tier-down: it frees a chunk
    /// of its own choosing and reports exactly that. The modes model the four
    /// honest responses to that ask:
    /// - `"release"` — free everything asked (full eviction).
    /// - `"partial"` — tier-down: free a fixed `partial_chunk`, keep the rest.
    /// - `"defer"`   — free nothing now (draining); patient backpressure.
    /// - `"panic"`   — a broken consumer; must be isolated + quarantined.
    struct ScriptedConsumer {
        id: String,
        held: AtomicU64,
        /// Bytes a `"partial"` reclaim frees per ask — the tier-down step size.
        partial_chunk: AtomicU64,
        mode: Mutex<&'static str>,
    }

    impl ScriptedConsumer {
        fn new(id: &str, held: u64, mode: &'static str) -> Arc<Self> {
            Arc::new(Self {
                id: id.into(),
                held: AtomicU64::new(held),
                partial_chunk: AtomicU64::new(0),
                mode: Mutex::new(mode),
            })
        }
        fn with_partial_chunk(self: Arc<Self>, chunk: u64) -> Arc<Self> {
            self.partial_chunk.store(chunk, Ordering::SeqCst);
            self
        }
        fn set_mode(&self, m: &'static str) {
            *self.mode.lock() = m;
        }
        fn held(&self) -> u64 {
            self.held.load(Ordering::SeqCst)
        }
        fn free_bytes(&self, want: u64) -> u64 {
            let before = self.held();
            let freed = before.min(want);
            self.held.store(before - freed, Ordering::SeqCst);
            freed
        }
    }

    #[async_trait::async_trait]
    impl ResourceConsumer for ScriptedConsumer {
        fn consumer_id(&self) -> &str {
            &self.id
        }
        fn footprint(&self) -> Vec<ConsumerFootprint> {
            vec![ConsumerFootprint {
                kind: ResourceKind::Vram,
                bytes: self.held(),
                detail: "scripted".into(),
            }]
        }
        async fn reclaim(&self, request: ReclaimRequest) -> ReclaimOutcome {
            let mode = *self.mode.lock();
            match mode {
                "panic" => panic!("scripted panic"),
                "defer" => ReclaimOutcome {
                    freed_bytes: 0,
                    status: ReclaimStatus::Deferred,
                    detail: Some("draining".into()),
                },
                "partial" => {
                    // Tier-down: free a fixed chunk of the whole-lease ask, keep
                    // the rest alive at lower fidelity.
                    let freed = self.free_bytes(self.partial_chunk.load(Ordering::SeqCst));
                    ReclaimOutcome {
                        freed_bytes: freed,
                        status: ReclaimStatus::Partial,
                        detail: Some("tier-down".into()),
                    }
                }
                _ => {
                    // Full release: free everything asked (the whole lease).
                    let freed = self.free_bytes(request.target_bytes);
                    ReclaimOutcome::released(freed)
                }
            }
        }
    }

    fn req(consumer: &str, bytes: u64, ttl_ms: u64, policy: ReclaimPolicy) -> LeaseRequest {
        LeaseRequest {
            consumer_id: consumer.into(),
            kind: ResourceKind::Vram,
            bytes,
            ttl_ms,
            reclaim_policy: policy,
        }
    }

    // what this catches: the RAII lease guard (#56/G1) — `acquire_guarded` reserves
    // bytes that show on the board, and dropping the guard returns them WITHOUT a
    // manual `release`. This is the primitive the ephemeral eval lane holds for its
    // lifetime; if Drop didn't release, every eval would permanently leak its VRAM
    // reservation off the board and the box would starve after a few benchmarks.
    // Also pins the honest refusal: a request past the ceiling is InsufficientCapacity,
    // not an over-grant (the "pressure, not OOM" answer the caller spills to CPU on).
    #[tokio::test]
    async fn lease_guard_reserves_then_releases_on_drop() {
        let src = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 10_000));
        let daemon = ResourceDaemon::start(
            vec![src],
            vec![],
            DaemonConfig {
                tick_interval: Duration::from_millis(20),
                min_reclaim_budget: Duration::from_millis(100),
                governor: GovernorConfig {
                    min_dwell_ms: 0,
                    graceful_grace_ms: 50,
                },
            },
        );
        let vram_available = |d: &ResourceDaemon| {
            d.board()
                .kinds
                .iter()
                .find(|k| k.kind == ResourceKind::Vram)
                .map(|k| k.available_bytes)
                .unwrap_or(0)
        };
        assert_eq!(
            vram_available(&daemon),
            10_000,
            "starts with the full ceiling free"
        );

        {
            let guard = daemon
                .acquire_guarded(&req("eval-lane", 6_000, 60_000, ReclaimPolicy::Pinned))
                .expect("6GB fits under the 10GB ceiling");
            assert_eq!(guard.bytes(), 6_000, "guard reports the held bytes");
            let view = guard.view().expect("live lease");
            let busy = daemon.governor.lock();
            assert_eq!(view.bytes_at(0), 6_000, "native callback reads without locking the governor");
            drop(busy);
            // What this catches: a planner must not pair a post-release budget
            // with a pre-release lease and omit the same reservation twice.
            let read = daemon.replacement_budget_snapshot("serving", ResourceKind::Vram);
            assert_eq!(read.bytes, 4_000);
            assert_eq!(read.capacity_bytes, 10_000, "the pool's capacity, read with the budget");
            assert_eq!(read.own_bytes, 0, "serving reported no residency: nothing of its own was added back");
            assert_eq!(read.charged.len(), 1);
            assert_eq!(read.charged[0].consumer_id, "eval-lane");
            assert_eq!(read.charged[0].bytes, 6_000);
            let own = daemon.replacement_budget_snapshot("eval-lane", ResourceKind::Vram);
            assert!(own.charged.is_empty(), "replacement's own lease is not another consumer");
            let wrong_pool = daemon.replacement_budget_snapshot("serving", ResourceKind::Ram);
            assert!(wrong_pool.charged.is_empty(), "only this memory pool's leases are charged");
            assert_eq!(
                daemon.board().leases.len(),
                1,
                "the reservation is on the board"
            );
            assert_eq!(
                vram_available(&daemon),
                4_000,
                "available drops by the held bytes"
            );

            // A second ask that exceeds the remaining 4GB is refused HONESTLY (the
            // caller spills to CPU on exactly this) — never an over-grant.
            match daemon.acquire_guarded(&req("eval-lane-2", 5_000, 60_000, ReclaimPolicy::Pinned))
            {
                Err(LeaseError::InsufficientCapacity {
                    available,
                    requested,
                    ..
                }) => {
                    assert_eq!(available, 4_000);
                    assert_eq!(requested, 5_000);
                }
                Err(e) => panic!("expected InsufficientCapacity, got a different error: {e:?}"),
                Ok(_) => panic!(
                    "expected InsufficientCapacity — the board over-granted past its ceiling"
                ),
            }
        } // guard drops here → release

        assert_eq!(
            daemon.board().leases.len(),
            0,
            "drop released the reservation — no leak"
        );
        assert_eq!(
            vram_available(&daemon),
            10_000,
            "the full ceiling is free again after drop"
        );
    }

    // what this catches: card 26df. A board built under the lock and published after
    // it is released can be overwritten by an older one: the tick captured its board,
    // ran its observers (which acquire), and published the captured copy LAST, wiping
    // the newer lease off the board. A guard that read the board then saw its own grant
    // as 0 (an admitted 6.1 GB train run did). An observer here acquires on every tick,
    // so after any tick the board must name every lease acquired, whatever the ticks'
    // timing; the old order drops the newest one every time.
    #[tokio::test]
    async fn a_tick_never_publishes_a_board_older_than_a_lease_its_observers_took() {
        let src = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 10_000));
        let daemon = ResourceDaemon::start(
            vec![src],
            vec![],
            DaemonConfig {
                tick_interval: Duration::from_secs(3600),
                min_reclaim_budget: Duration::from_millis(100),
                governor: GovernorConfig { min_dwell_ms: 0, graceful_grace_ms: 50 },
            },
        );
        let acquired = Arc::new(AtomicU64::new(0));
        let (weak, count) = (Arc::downgrade(&daemon), acquired.clone());
        daemon.on_tick(Arc::new(move |_board: &LeaseBoard| {
            if let Some(d) = weak.upgrade() {
                d.acquire(&req("observer", 1, 60_000, ReclaimPolicy::Graceful))
                    .expect("1 byte of 10,000 is always free");
                count.fetch_add(1, Ordering::SeqCst);
            }
        }));
        daemon.tick().await;
        let held = acquired.load(Ordering::SeqCst);
        assert!(held >= 1, "the manual tick ran the observer");
        assert_eq!(
            daemon.board().leases.len() as u64,
            held,
            "the published board names every lease the observers took, including the one taken during the last tick"
        );
    }

    // what this catches: the base "decide under the authority's tick" pattern
    // (MEMORY-AUTHORITY-DAEMON). A registered `on_tick` observer runs EVERY reconcile
    // tick with the authoritative fresh board and can read the live VRAM available — the
    // seam that lets serving/eval compute their memory decision in the ONE authority
    // instead of sampling memory on their own hot path. Regresses dropping the fan-out.
    #[tokio::test]
    async fn on_tick_runs_observers_with_the_live_board() {
        let src = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 10_000));
        let daemon = ResourceDaemon::start(
            vec![src],
            vec![],
            DaemonConfig {
                tick_interval: Duration::from_millis(20),
                min_reclaim_budget: Duration::from_millis(100),
                governor: GovernorConfig {
                    min_dwell_ms: 0,
                    graceful_grace_ms: 50,
                },
            },
        );
        let seen_vram = Arc::new(AtomicU64::new(0));
        let sink = seen_vram.clone();
        daemon.on_tick(Arc::new(move |board: &LeaseBoard| {
            // Runs UNDER the authority's tick with the live board — reads the memory
            // truth, never samples memory itself.
            let vram = board
                .kinds
                .iter()
                .find(|k| k.kind == ResourceKind::Vram)
                .map(|k| k.available_bytes)
                .unwrap_or(0);
            sink.store(vram, Ordering::SeqCst);
        }));
        // The daemon's own tick fans the board out to the observer within a few ticks.
        let fired = wait_until(&daemon, |_| seen_vram.load(Ordering::SeqCst) > 0).await;
        assert!(
            fired,
            "an on_tick observer must run under the authority's reconcile tick"
        );
        assert_eq!(
            seen_vram.load(Ordering::SeqCst),
            10_000,
            "the observer saw the live board's VRAM available — the authority's memory truth"
        );
    }

    // Poll the board until a predicate holds or we exhaust attempts (the daemon
    // ticks asynchronously). Keeps tests deterministic without sleeping a fixed
    // wall-clock budget that would flake on slow CI.
    async fn wait_until(
        daemon: &ResourceDaemon,
        mut pred: impl FnMut(&LeaseBoard) -> bool,
    ) -> bool {
        for _ in 0..200 {
            if pred(&daemon.board()) {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        false
    }

    // what this catches: rung-2 end-to-end — the async daemon reads a capacity
    // source, detects an over-budget condition when the scan shrinks under live
    // grants, drives the consumer's async reclaim CONCURRENTLY off its tick, and
    // folds the freed bytes back so the board settles within the ceiling. The
    // ledger reclaims the whole lease; the consumer here fully releases it. This
    // is the whole shell over the (separately unit-tested) deterministic
    // governor: if the scan→reconcile→reclaim→apply wiring is wrong, it is wrong
    // here.
    #[tokio::test]
    async fn daemon_full_release_claws_back_to_ceiling() {
        let src = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 10_000));
        let serving = ScriptedConsumer::new("serving", 8_000, "release");
        let daemon = ResourceDaemon::start(
            vec![src.clone()],
            vec![serving.clone()],
            DaemonConfig {
                tick_interval: Duration::from_millis(20),
                min_reclaim_budget: Duration::from_millis(100),
                governor: GovernorConfig {
                    min_dwell_ms: 0,
                    graceful_grace_ms: 50,
                },
            },
        );

        // serving leases 8GB (graceful) — within the 10GB ceiling.
        daemon
            .acquire(&req("serving", 8_000, 60_000, ReclaimPolicy::Graceful))
            .unwrap();
        assert_eq!(daemon.board().leases.len(), 1);

        // A scan shrinks VRAM to 5GB (a game grabbed 5GB) → over budget. The
        // ledger's only victim is the whole 8GB lease; serving fully releases it.
        src.set_ceiling(5_000);

        let settled = wait_until(&daemon, |b| {
            b.leases.iter().map(|l| l.bytes).sum::<u64>() == 0
        })
        .await;
        assert!(
            settled,
            "daemon should drive the reclaim and free the lease"
        );
        assert_eq!(serving.held(), 0, "consumer actually freed its bytes");
        assert!(!daemon.is_over_budget(), "back within budget after reclaim");
    }

    // what this catches: the tier-down path — a consumer answers the whole-lease
    // ask by freeing only a chunk (Partial) and living on smaller. The daemon
    // must fold the honest partial delta (lease shrinks by exactly what was
    // freed, stays alive) and then STOP asking once granted is back within the
    // ceiling — no over-reclaim, no thrash. This is adaptive quality scaling
    // (qwen-30B → qwen-7B) reduced to its byte handshake.
    #[tokio::test]
    async fn daemon_partial_tierdown_shrinks_lease_and_settles() {
        let src = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 10_000));
        // Frees a fixed 3GB chunk per ask — exactly the overage when squeezed to 5GB.
        let serving = ScriptedConsumer::new("serving", 8_000, "partial").with_partial_chunk(3_000);
        let daemon = ResourceDaemon::start(
            vec![src.clone()],
            vec![serving.clone()],
            DaemonConfig {
                tick_interval: Duration::from_millis(20),
                min_reclaim_budget: Duration::from_millis(100),
                governor: GovernorConfig {
                    min_dwell_ms: 0,
                    graceful_grace_ms: 50,
                },
            },
        );

        let lease = daemon
            .acquire(&req("serving", 8_000, 60_000, ReclaimPolicy::Graceful))
            .unwrap();
        src.set_ceiling(5_000); // 3GB over budget

        // Wait on the actual reclaim effect — the lease shrunk to its freed size —
        // not on the over-budget flag, which reads false at t=0 before the daemon
        // has noticed the squeeze.
        let settled = wait_until(&daemon, |b| {
            b.leases
                .iter()
                .any(|l| l.lease_id == lease.lease_id && l.bytes == 5_000)
        })
        .await;
        assert!(
            settled,
            "tier-down should shrink the lease to its freed size"
        );
        assert_eq!(
            serving.held(),
            5_000,
            "consumer tier-down freed exactly the overage"
        );
        assert!(
            !daemon.is_over_budget(),
            "granted back within the ceiling — settled, no thrash"
        );
    }

    // what this catches: a Deferred reclaim is patient backpressure across the
    // async boundary — the daemon must NOT yank or drop the bytes, the lease
    // stays fully held, and a later mode-flip to release lets the next tick
    // actually reclaim. Proves the two-phase handshake survives the real timer.
    #[tokio::test]
    async fn deferred_reclaim_holds_then_releases_when_consumer_is_ready() {
        let src = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 4_000));
        let serving = ScriptedConsumer::new("serving", 0, "defer");
        let daemon = ResourceDaemon::start(
            vec![src.clone()],
            vec![serving.clone()],
            DaemonConfig {
                tick_interval: Duration::from_millis(20),
                min_reclaim_budget: Duration::from_millis(100),
                governor: GovernorConfig {
                    min_dwell_ms: 0,
                    graceful_grace_ms: 0,
                },
            },
        );

        let lease = daemon
            .acquire(&req("serving", 4_000, 60_000, ReclaimPolicy::Graceful))
            .unwrap();
        serving.held.store(4_000, Ordering::SeqCst);

        // Squeeze to 1GB → 3GB over budget, but the consumer defers.
        src.set_ceiling(1_000);
        // First wait for the daemon to NOTICE the squeeze (over_budget latches),
        // then prove across several further ticks that the deferred lease is never
        // yanked — a bounded negative assertion, not a fixed-time busy-wait.
        assert!(
            wait_until(&daemon, |_| daemon.is_over_budget()).await,
            "daemon should detect the over-budget squeeze"
        );
        for _ in 0..5 {
            tokio::time::sleep(daemon.config.tick_interval).await;
            assert_eq!(
                daemon.board().leases.iter().map(|l| l.bytes).sum::<u64>(),
                4_000,
                "deferred → bytes never yanked"
            );
        }
        assert!(daemon.is_over_budget(), "still over budget while deferring");

        // Consumer becomes ready → next tick reclaims the overage.
        serving.set_mode("release");
        let settled = wait_until(&daemon, |b| {
            b.leases.iter().map(|l| l.bytes).sum::<u64>() <= 1_000
        })
        .await;
        assert!(settled, "once ready, the daemon reclaims to the ceiling");
        let _ = lease;
    }

    // what this catches: the MEASUREMENT axis wired end-to-end through the daemon
    // tick — a consumer holding VRAM with NO lease (serving's real posture) must
    // surface on the published board as measured_bytes + an attribution, polled
    // from footprint() every tick, WITHOUT any lease and WITHOUT disturbing
    // `available`. This is the "0 tracked while the GPU is full" fix: if the
    // footprint poll or set_measured wiring is wrong, the board stays blind here.
    #[tokio::test]
    async fn footprint_poll_surfaces_measured_residency_without_a_lease() {
        let src = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 24_000));
        // Holds 18GB resident (a loaded model) but leases nothing.
        let serving = ScriptedConsumer::new("serving", 18_000, "release");
        let daemon = ResourceDaemon::start(
            vec![src.clone()],
            vec![serving.clone()],
            DaemonConfig {
                tick_interval: Duration::from_millis(20),
                min_reclaim_budget: Duration::from_millis(100),
                governor: GovernorConfig {
                    min_dwell_ms: 0,
                    graceful_grace_ms: 50,
                },
            },
        );

        // No acquire — serving holds bytes but never leased. The tick's footprint
        // poll must still attribute the residency on the board.
        let surfaced = wait_until(&daemon, |b| !b.attributions.is_empty()).await;
        assert!(
            surfaced,
            "footprint poll should attribute measured residency each tick"
        );

        let board = daemon.board();
        assert_eq!(board.attributions.len(), 1);
        assert_eq!(board.attributions[0].consumer_id, "serving");
        assert_eq!(board.attributions[0].bytes, 18_000);
        assert_eq!(board.attributions[0].kind, ResourceKind::Vram);

        let vram = board
            .kinds
            .iter()
            .find(|k| k.kind == ResourceKind::Vram)
            .unwrap();
        assert_eq!(vram.granted_bytes, 0, "nothing leased");
        assert_eq!(vram.measured_bytes, 18_000, "but 18GB measured-resident");
        // available is the honest free-based remainder — capacity − granted, NOT
        // reduced by the measured residency. Measurement reports; it never reserves.
        assert_eq!(
            vram.available_bytes, 24_000,
            "available untouched by measurement"
        );
        assert!(
            !daemon.is_over_budget(),
            "measured residency is not an over-budget condition"
        );
    }

    // what this catches: the un-inversion wired end-to-end through the daemon tick —
    // EXTERNAL physical pressure (a game grabbing VRAM) contracts `available` even
    // with zero leases, because the tick feeds `used_bytes()` into `physical_used`
    // and `available = capacity − max(granted, physical_used)`. Before the
    // un-inversion the board would read `available == capacity` here (blind to the
    // grab) and the daemon would happily commit into memory the game already took —
    // the OOM the whole task fixes. When the grab exceeds the FIXED ceiling the
    // daemon goes over-budget off physical usage alone (no lease shrank), and since
    // none of the overage is a lease we hold there is nothing safe to reclaim — the
    // daemon stays alive and simply refuses new demand (we can't evict a game).
    #[tokio::test]
    async fn external_physical_pressure_contracts_available_without_a_lease() {
        let src = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 24_000));
        let daemon = ResourceDaemon::start(
            vec![src.clone()],
            Vec::new(),
            DaemonConfig {
                tick_interval: Duration::from_millis(20),
                min_reclaim_budget: Duration::from_millis(100),
                governor: GovernorConfig {
                    min_dwell_ms: 0,
                    graceful_grace_ms: 50,
                },
            },
        );

        // A game grabs 21GB of VRAM: physical_used = 21_000, ceiling fixed at 24_000.
        src.set_used(21_000);
        let contracted = wait_until(&daemon, |b| {
            b.kinds.iter().any(|k| k.physical_used_bytes == 21_000)
        })
        .await;
        assert!(
            contracted,
            "the tick must feed external physical usage onto the board"
        );

        let board = daemon.board();
        let vram = board
            .kinds
            .iter()
            .find(|k| k.kind == ResourceKind::Vram)
            .unwrap();
        assert_eq!(
            vram.capacity_bytes, 24_000,
            "ceiling is fixed — the grab did not move it"
        );
        assert_eq!(vram.granted_bytes, 0, "we hold no lease");
        assert_eq!(
            vram.physical_used_bytes, 21_000,
            "but 21GB is physically resident"
        );
        assert_eq!(
            vram.external_bytes, 21_000,
            "all of it external — no consumer of ours claims it"
        );
        assert_eq!(
            vram.available_bytes, 3_000,
            "available = 24k − max(0, 21k) = 3k, NOT the blind 24k"
        );
        assert!(
            !daemon.is_over_budget(),
            "21k < 24k ceiling — tight, not over"
        );

        // The game grabs more, past the ceiling: physical_used = 26_000 > 24_000.
        src.set_used(26_000);
        let over = wait_until(&daemon, |_| daemon.is_over_budget()).await;
        assert!(
            over,
            "physical usage over the fixed ceiling is an over-budget condition on its own"
        );
        let board = daemon.board();
        let vram = board
            .kinds
            .iter()
            .find(|k| k.kind == ResourceKind::Vram)
            .unwrap();
        assert_eq!(
            vram.available_bytes, 0,
            "committed exceeds capacity → zero to commit"
        );
        // Nothing safe to reclaim (no lease of ours) — the daemon stays alive and
        // keeps publishing, it does not thrash trying to evict a game's memory.
        assert!(
            daemon.board().leases.is_empty(),
            "no lease existed to be reclaimed"
        );
    }

    // what this catches: a panicking consumer is isolated (catch_unwind) and
    // quarantined after the threshold — one bad leaseholder can never crash the
    // daemon task or stall reclaims for the others. The daemon stays alive and
    // keeps publishing the board.
    #[tokio::test]
    async fn panicking_consumer_is_isolated_and_quarantined() {
        let src = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 4_000));
        let bad = ScriptedConsumer::new("bad", 4_000, "panic");
        let daemon = ResourceDaemon::start(
            vec![src.clone()],
            vec![bad.clone()],
            DaemonConfig {
                tick_interval: Duration::from_millis(20),
                min_reclaim_budget: Duration::from_millis(50),
                governor: GovernorConfig {
                    min_dwell_ms: 0,
                    graceful_grace_ms: 0,
                },
            },
        );
        daemon
            .acquire(&req("bad", 4_000, 60_000, ReclaimPolicy::Graceful))
            .unwrap();
        src.set_ceiling(1_000); // over budget → daemon keeps asking, consumer keeps panicking

        // Wait for the daemon to NOTICE the squeeze (a live tick processed the new
        // ceiling) — not just for the acquire-published board to show the lease.
        assert!(
            wait_until(&daemon, |_| daemon.is_over_budget()).await,
            "daemon should detect the over-budget squeeze"
        );
        // Across several panic-reclaim ticks the daemon stays alive and keeps
        // publishing a coherent board: the bad lease is still accounted (fail-loud,
        // not silently lost) and never yanked despite the consumer panicking.
        for _ in 0..5 {
            tokio::time::sleep(daemon.config.tick_interval).await;
        }
        assert!(
            daemon.board().leases.iter().any(|l| l.consumer_id == "bad"),
            "panicking consumer's bytes stay accounted, never yanked"
        );
        assert_eq!(bad.held(), 4_000, "panicking consumer freed nothing");
        assert!(
            daemon.is_over_budget(),
            "still over budget — nothing was freed"
        );
    }

    // what this catches (card 9d856843): a component waiting for memory that cannot hear a release
    // (nothing said on the bus), finds the topic empty when it attaches late, or wakes on boards
    // that changed nothing it watches. The authority says its kinds at once, says each lease and
    // release in order, and says nothing for an unchanged board; a second writer is refused.
    #[tokio::test]
    async fn what_is_free_is_said_at_attach_and_on_each_change_only() {
        let src = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 10_000));
        let daemon = ResourceDaemon::start(vec![src], vec![], DaemonConfig::default());
        let bus = continuum_sage::component::Bus::new();
        daemon.say_on(&bus).expect("test: the authority is the first writer");
        assert!(daemon.say_on(&bus).is_err(), "a second writer is refused");
        let free = |seq: u64| {
            let said = bus.reader::<Free>(FREE, continuum_sage::component::Key::ONE).latest().expect("test: a value is said");
            assert_eq!(said.provenance.by.seq, seq, "said {seq} times");
            said.value.kind(ResourceKind::Vram).expect("test: VRAM is governed").available
        };
        assert_eq!(free(1), 10_000, "said at once: a late reader reads a value");

        let guard = daemon.acquire_guarded(&req("held", 4_000, u64::MAX, ReclaimPolicy::Pinned)).expect("test: granted");
        assert_eq!(free(2), 6_000, "a lease is said");
        daemon.publish_locked(&daemon.governor.lock());
        assert_eq!(free(2), 6_000, "an unchanged board says nothing");
        drop(guard);
        assert_eq!(free(3), 10_000, "a release is said, with nothing else changing");
    }

    // what this catches (Codex's contract for the trainer, 2026-11-26): kernel leases take everything, a
    // seat's ask waits, a kernel release grants it as a lease its owner takes without asking again, and a
    // ticket dropped while waiting leaves no wait behind.
    #[tokio::test]
    async fn an_ask_waits_on_the_authority_and_is_granted_on_release() {
        let src = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 10_000));
        let config = DaemonConfig { tick_interval: Duration::from_secs(3600), min_reclaim_budget: Duration::from_millis(100), governor: GovernorConfig { min_dwell_ms: 0, graceful_grace_ms: 50 } };
        let daemon = ResourceDaemon::start(vec![src], vec![], config);
        let serving = daemon.acquire_guarded(&req("serving", 10_000, u64::MAX, ReclaimPolicy::Pinned)).expect("test: serving takes everything");
        let Ok(Ask::Waiting { ticket: mut seat, short }) = daemon.ask(&req("learning:her", 5_000, u64::MAX, ReclaimPolicy::Graceful)) else {
            panic!("test: the seat waits")
        };
        assert_eq!(short, 5_000);
        assert!(seat.granted().is_none(), "nothing released yet");
        let Ok(Ask::Waiting { ticket: gone, .. }) = daemon.ask(&req("learning:him", 1_000, u64::MAX, ReclaimPolicy::Graceful)) else {
            panic!("test: a second seat waits")
        };
        drop(gone); // its owner stopped wanting it
        drop(serving); // a kernel release
        let lease = seat.granted().expect("test: granted on the release, not asked again");
        assert_eq!(lease.bytes(), 5_000);
        assert_eq!(daemon.board().kinds.iter().find(|k| k.kind == ResourceKind::Vram).map(|k| k.available_bytes), Some(5_000), "the withdrawn seat holds nothing");
        drop(lease);
        assert_eq!(daemon.board().kinds.iter().find(|k| k.kind == ResourceKind::Vram).map(|k| k.available_bytes), Some(10_000));
    }

    // what this catches (Cormac and Codex, the 5090, 2026-11-26): a lesson sized within the whole card
    // asked for 29.5 GB while the core held 30.7 GB of it, and waited forever. The ceiling is capacity
    // less what nothing can demote (residents' weights) and what others hold; a residency let go moves its
    // bytes to external until the physical reading drops, so it never opens room that is not there. The
    // byte counts are the 5090's board at 15:07Z; the core's weights split is a fixture, not a reading.
    #[tokio::test(start_paused = true)]
    async fn the_ceiling_is_capacity_less_fixed_weights_and_what_others_hold() {
        const CAPACITY: u64 = 34_190_917_632;
        const USED: u64 = 32_436_649_984;
        const CORE: u64 = 30_721_118_208;
        const CORE_WEIGHTS: u64 = 17_000_000_000;
        const EMBED: u64 = 633_205_120;
        let source = Arc::new(MockCapacitySource::new(ResourceKind::Vram, CAPACITY));
        source.set_used(USED);
        let daemon = ResourceDaemon::start(vec![source as Arc<dyn CapacitySource>], vec![], DaemonConfig::default());
        let ceiling = |daemon: &ResourceDaemon| Free::of(&daemon.board()).kind(ResourceKind::Vram).map(|kind| kind.ceiling);
        let _embed = daemon.hold_residency("embed", [ResidentFloor { kind: ResourceKind::Vram, resident: EMBED, headroom: 0, fixed: EMBED }]);
        let core = daemon.hold_residency("core", [ResidentFloor { kind: ResourceKind::Vram, resident: CORE, headroom: 0, fixed: CORE_WEIGHTS }]);
        let external = USED - CORE - EMBED;
        assert_eq!(ceiling(&daemon), Some(CAPACITY - CORE_WEIGHTS - EMBED - external), "the core's KV and workspace count toward the ceiling, its weights never");
        drop(core);
        assert_eq!(ceiling(&daemon), Some(CAPACITY - EMBED - (USED - EMBED)), "let go but still resident: external, not room");
    }

    /// A holder that answers the law as scripted and counts what it was asked.
    struct Answering {
        asked: std::sync::Mutex<Vec<crate::resources::ShrinkDue>>,
        answer: ReclaimOutcome,
    }

    impl crate::resources::ShrinkHolder for Answering {
        fn shrink(&self, due: &crate::resources::ShrinkDue) -> ReclaimOutcome {
            let due = due.clone();
            self.asked.lock().expect("test: the holder's log").push(due); // expect: the test's own lock
            self.answer.clone()
        }
    }

    fn vram_free(daemon: &ResourceDaemon) -> Option<u64> {
        daemon.board().kinds.iter().find(|k| k.kind == ResourceKind::Vram).map(|k| k.available_bytes)
    }

    // what this catches (step 3 part 6): a learning seat's elastic lease never asked to give back while a
    // higher wait starves, asked again on every tick, or its deferred answer lost: the law asks once, the
    // seat finishes, and releasing its lease grants the higher wait.
    #[tokio::test]
    async fn the_law_asks_an_elastic_lease_once_and_its_release_grants_the_higher_wait() {
        let src = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 1_000));
        let config = DaemonConfig { tick_interval: Duration::from_secs(3600), min_reclaim_budget: Duration::from_millis(100), governor: GovernorConfig { min_dwell_ms: 0, graceful_grace_ms: 50 } };
        let daemon = ResourceDaemon::start(vec![src], vec![], config);
        let seat = daemon.acquire_guarded(&req("learning:her", 600, u64::MAX, ReclaimPolicy::Graceful)).expect("test: the seat fits");
        let holder = Arc::new(Answering { asked: std::sync::Mutex::new(Vec::new()), answer: ReclaimOutcome { freed_bytes: 0, status: ReclaimStatus::Deferred, detail: None } });
        let registered: Arc<dyn crate::resources::ShrinkHolder> = holder.clone();
        daemon.hold_elastic(&seat, crate::resources::Elastic { floor: 0, priority: 10 }, registered).expect("test: the seat is elastic");
        let Ok(Ask::Waiting { ticket: mut turn, .. }) = daemon.ask(&req("serving", 700, u64::MAX, ReclaimPolicy::Pinned)) else { panic!("test: the turn waits") };
        turn.prioritize(200);
        daemon.deliver_shrinks();
        daemon.deliver_shrinks();
        let asked = holder.asked.lock().expect("test: the holder's log").clone(); // expect: the test's own lock
        assert_eq!(asked.len(), 1, "asked once, never again for the same target");
        assert_eq!((asked[0].held, asked[0].to), (600, 300), "asked to give back exactly what the turn lacks");
        assert!(turn.granted().is_none(), "deferred: nothing is given back yet");
        drop(seat); // the seat finished: its lease is released, its own fence
        let lease = turn.granted().expect("test: the release grants the turn");
        assert_eq!(lease.bytes(), 700);
        assert_eq!(vram_free(&daemon), Some(300));
    }

    // what this catches (Fable and Codex on #6287 at 6083030588): a wait that ALREADY outranks a lease when it
    // turns elastic. Its shrink is said once, so a holder registered after the governor section released could
    // miss it, and the wait would never get room. The holder now registers in the same section, before the
    // lease turns elastic, so hold_elastic itself delivers that shrink exactly once. (The gap a daemon tick
    // could fall into is gone structurally; no test can force a tick into it without a hook in production.)
    #[tokio::test]
    async fn a_wait_that_already_outranks_is_asked_of_the_new_holder_once() {
        let src = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 1_000));
        let config = DaemonConfig { tick_interval: Duration::from_secs(3600), min_reclaim_budget: Duration::from_millis(100), governor: GovernorConfig { min_dwell_ms: 0, graceful_grace_ms: 50 } };
        let daemon = ResourceDaemon::start(vec![src], vec![], config);
        let seat = daemon.acquire_guarded(&req("learning:her", 600, u64::MAX, ReclaimPolicy::Graceful)).expect("test: the seat fits");
        let Ok(Ask::Waiting { ticket: turn, .. }) = daemon.ask(&req("serving", 700, u64::MAX, ReclaimPolicy::Pinned)) else { panic!("test: the turn waits") };
        turn.prioritize(200); // the turn outranks before the seat is elastic: nothing to ask yet
        let holder = Arc::new(Answering { asked: std::sync::Mutex::new(Vec::new()), answer: ReclaimOutcome { freed_bytes: 0, status: ReclaimStatus::Deferred, detail: None } });
        let registered: Arc<dyn crate::resources::ShrinkHolder> = holder.clone();
        daemon.hold_elastic(&seat, crate::resources::Elastic { floor: 0, priority: 10 }, registered).expect("test: the seat is elastic");
        daemon.deliver_shrinks();
        let asked = holder.asked.lock().expect("test: the holder's log").clone(); // expect: the test's own lock
        assert_eq!(asked.len(), 1, "the waiting turn's shrink reached the new holder, once");
        assert_eq!((asked[0].held, asked[0].to), (600, 300));
    }

    // what this catches: bytes a holder gives back at once not credited until some later release.
    #[tokio::test]
    async fn bytes_given_back_at_once_shrink_the_lease_and_grant_the_wait() {
        let src = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 1_000));
        let config = DaemonConfig { tick_interval: Duration::from_secs(3600), min_reclaim_budget: Duration::from_millis(100), governor: GovernorConfig { min_dwell_ms: 0, graceful_grace_ms: 50 } };
        let daemon = ResourceDaemon::start(vec![src], vec![], config);
        let window = daemon.acquire_guarded(&req("embed", 600, u64::MAX, ReclaimPolicy::Graceful)).expect("test: the window fits");
        let holder = Arc::new(Answering { asked: std::sync::Mutex::new(Vec::new()), answer: ReclaimOutcome { freed_bytes: 300, status: ReclaimStatus::Partial, detail: None } });
        daemon.hold_elastic(&window, crate::resources::Elastic { floor: 100, priority: 10 }, holder).expect("test: elastic");
        let Ok(Ask::Waiting { ticket: mut turn, .. }) = daemon.ask(&req("serving", 700, u64::MAX, ReclaimPolicy::Pinned)) else { panic!("test: the turn waits") };
        turn.prioritize(200);
        daemon.deliver_shrinks();
        assert_eq!(window.bytes(), 300, "given back at once: the lease shrank");
        assert!(turn.granted().is_some(), "and the turn is granted from it");
    }
