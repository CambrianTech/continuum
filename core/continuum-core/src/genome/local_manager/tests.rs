    //! End-to-end tests for the local impl. Each test wires a couple
    //! of stub tiers, registers a persona, and verifies the page_in /
    //! page_out / audit_access dispatch.
    use super::*;
    use crate::genome::blob::{ArtifactBlob, Provenance};
    use crate::genome::tier::{EvictionRecord, TierCapacity};
    use crate::genome::working_set::{ArtifactId, PageKind, PageOffset};
    use parking_lot::Mutex;
    use std::sync::Arc;
    use uuid::Uuid;

    /// Stub tier store: records every read/write/observe call so
    /// tests assert "the manager called the right tier in the right
    /// order." Holds a static `Option<PageHandle>` per page for
    /// `read` responses.
    struct StubTier {
        role: TierRole,
        /// Pages this tier has — read returns Ok(handle) for matches,
        /// `TierError::PageNotFound` otherwise.
        pages_present: Mutex<Vec<PageRef>>,
        /// Call log so tests can assert order of tier access.
        reads: Mutex<Vec<PageRef>>,
        observes: Mutex<Vec<PageRef>>,
        /// Artificial read latency. Zero for the fast in-memory stubs;
        /// non-zero to model a slow lower tier (Cold SSD / Frozen) so a
        /// test can assert `PageFault.elapsed_us` reflects real
        /// servicing cost rather than a sub-microsecond stub hit.
        read_delay: std::time::Duration,
    }

    impl StubTier {
        fn new(role: TierRole, pages_present: Vec<PageRef>) -> Arc<Self> {
            Self::with_delay(role, pages_present, std::time::Duration::ZERO)
        }

        /// Stub tier that sleeps `read_delay` on every `read` — models a
        /// slow tier so page-in timing is measurable + non-flaky.
        fn with_delay(
            role: TierRole,
            pages_present: Vec<PageRef>,
            read_delay: std::time::Duration,
        ) -> Arc<Self> {
            Arc::new(Self {
                role,
                pages_present: Mutex::new(pages_present),
                reads: Mutex::new(Vec::new()),
                observes: Mutex::new(Vec::new()),
                read_delay,
            })
        }
    }

    #[async_trait]
    impl TierStore for StubTier {
        fn role(&self) -> TierRole {
            self.role
        }

        async fn read(&self, page: PageRef) -> Result<PageHandle, TierError> {
            if !self.read_delay.is_zero() {
                tokio::time::sleep(self.read_delay).await;
            }
            self.reads.lock().push(page);
            if self.pages_present.lock().contains(&page) {
                Ok(PageHandle {
                    page,
                    tier_role: self.role,
                    size_bytes: 1024,
                })
            } else {
                Err(TierError::PageNotFound { page })
            }
        }

        async fn write(
            &self,
            _page: PageRef,
            _blob: ArtifactBlob,
            _provenance: Provenance,
        ) -> Result<(), TierError> {
            Ok(())
        }

        async fn evict(&self, _target_free_bytes: usize) -> Vec<EvictionRecord> {
            Vec::new()
        }

        fn capacity(&self) -> TierCapacity {
            TierCapacity {
                current_used: 0,
                configured_limit: 100_000_000,
            }
        }

        fn observe_access(&self, page: PageRef) {
            self.observes.lock().push(page);
        }
    }

    fn make_page(low_artifact_bits: u128) -> PageRef {
        PageRef {
            kind: PageKind::LoRALayer,
            artifact: ArtifactId::new(Uuid::from_u128(low_artifact_bits)),
            offset: PageOffset::Whole,
        }
    }

    fn make_persona(low_bits: u128) -> PeerId {
        PeerId::from_uuid(Uuid::from_u128(low_bits))
    }

    fn capacity_uma() -> WorkingSetCapacity {
        WorkingSetCapacity {
            fast_bytes: 1_000_000,
            warm_bytes: 0,
            max_pinned_bytes: 500_000,
        }
    }

    /// What this catches: page_in on an already-resident page returns
    /// the cached handle WITHOUT walking the tier chain. Hot-path
    /// correctness; the whole point of a working set is that the
    /// resident-hit path is cheap.
    #[tokio::test]
    async fn page_in_resident_returns_cached_without_tier_walk() {
        let page = make_page(1);
        let fast = StubTier::new(TierRole::Fast, vec![page]);
        let mgr = LocalWorkingSetManager::new(vec![fast.clone()]);
        let persona = make_persona(7);
        mgr.register_persona(persona, capacity_uma());

        // First call: misses working set, promotes via Fast tier.
        let first = mgr.page_in(persona, page).await;
        match first {
            Err(fault) => {
                assert_eq!(fault.from_role, Some(TierRole::Fast));
                assert_eq!(fault.to_role, TierRole::Fast);
                assert_eq!(fault.persona, persona);
            }
            Ok(_) => panic!("first call should report tier promotion"),
        }
        let reads_after_first = fast.reads.lock().len();
        assert_eq!(reads_after_first, 1);

        // Second call: hits working set, returns Ok without re-reading.
        let second = mgr.page_in(persona, page).await;
        match second {
            Ok(handle) => {
                assert_eq!(handle.tier_role, TierRole::Fast);
                assert_eq!(handle.page, page);
            }
            Err(_) => panic!("second call should be a resident hit"),
        }
        // Tier was NOT re-read on the resident-hit path.
        assert_eq!(fast.reads.lock().len(), reads_after_first);
    }

    /// What this catches: page_in walks tier chain top-down (Fast →
    /// Cold), returns the first hit + records the from_role + to_role
    /// correctly. PageFault.from_role is where the page WAS;
    /// PageFault.to_role is the working set's preferred tier (always
    /// the highest configured).
    #[tokio::test]
    async fn page_in_walks_tier_chain_and_records_promotion() {
        let page = make_page(2);
        let fast = StubTier::new(TierRole::Fast, vec![]);
        let bench = StubTier::new(TierRole::Bench, vec![]);
        let cold = StubTier::new(TierRole::Cold, vec![page]);
        let mgr = LocalWorkingSetManager::new(vec![fast.clone(), bench.clone(), cold.clone()]);
        let persona = make_persona(8);
        mgr.register_persona(persona, capacity_uma());

        let result = mgr.page_in(persona, page).await;
        match result {
            Err(fault) => {
                assert_eq!(fault.from_role, Some(TierRole::Cold));
                assert_eq!(fault.to_role, TierRole::Fast);
                assert_eq!(fault.persona, persona);
                // Eviction cost is None — PR-3 doesn't drive
                // eviction. PR-4 wires the callback.
                assert!(fault.eviction_cost.is_none());
            }
            Ok(_) => panic!("expected PageFault for tier promotion"),
        }

        // Tier walk order: Fast first, then Bench, then Cold.
        assert_eq!(fast.reads.lock().len(), 1);
        assert_eq!(bench.reads.lock().len(), 1);
        assert_eq!(cold.reads.lock().len(), 1);
    }

    /// What this catches: page_in on a page that exists in NO tier
    /// returns a PageFault with `from_role: None` — the typed "true
    /// cold miss" signal sentinel needs to distinguish "page never
    /// existed" from "page was on Cold tier."
    #[tokio::test]
    async fn page_in_true_cold_miss_has_none_from_role() {
        let page = make_page(3);
        let fast = StubTier::new(TierRole::Fast, vec![]);
        let cold = StubTier::new(TierRole::Cold, vec![]);
        let mgr = LocalWorkingSetManager::new(vec![fast, cold]);
        let persona = make_persona(9);
        mgr.register_persona(persona, capacity_uma());

        let result = mgr.page_in(persona, page).await;
        match result {
            Err(fault) => {
                assert_eq!(fault.from_role, None);
                assert_eq!(fault.to_role, TierRole::Fast);
                assert_eq!(fault.page, page);
            }
            Ok(_) => panic!("expected PageFault for true cold miss"),
        }
    }

    /// What this catches: audit_access returns AccessDenied with the
    /// typed shape — not a generic error — when a different persona
    /// tries to read a private page. Same contract PR-2's trait test
    /// pins, now exercised through the LocalWorkingSetManager.
    #[tokio::test]
    async fn audit_access_denies_cross_persona_read() {
        let fast = StubTier::new(TierRole::Fast, vec![]);
        let mgr = LocalWorkingSetManager::new(vec![fast]);
        let owner = make_persona(10);
        let intruder = make_persona(11);
        let page = make_page(4);

        mgr.register_persona(owner, capacity_uma());
        mgr.register_persona(intruder, capacity_uma());
        mgr.register_page_owner(page, owner);

        // Owner: OK.
        assert!(mgr.audit_access(owner, page).is_ok());

        // Intruder: AccessDenied with full context.
        let result = mgr.audit_access(intruder, page);
        match result {
            Err(denied) => {
                assert_eq!(denied.actor, intruder);
                assert_eq!(denied.owner, Some(owner));
                assert!(denied.reason.contains("cross-persona"));
            }
            Ok(()) => panic!("expected AccessDenied"),
        }
    }

    /// What this catches: page_out to a configured tier role observes
    /// the page (signals the tier's bookkeeping) and removes from the
    /// working set. page_out to an unconfigured role returns
    /// `TierError::RoleNotConfigured` — the typed refusal for "you
    /// asked for a role this hardware doesn't have."
    #[tokio::test]
    async fn page_out_observes_target_tier_and_handles_unconfigured() {
        let page = make_page(5);
        let fast = StubTier::new(TierRole::Fast, vec![page]);
        let bench = StubTier::new(TierRole::Bench, vec![]);
        let mgr = LocalWorkingSetManager::new(vec![fast, bench.clone()]);
        let persona = make_persona(12);
        mgr.register_persona(persona, capacity_uma());

        // First, page_in to populate the working set.
        let _ = mgr.page_in(persona, page).await;

        // page_out to Bench: tier observes; working set updates.
        let result = mgr.page_out(persona, page, TierRole::Bench).await;
        assert!(result.is_ok());
        assert!(bench.observes.lock().contains(&page));

        // page_out to Warm: NOT configured on this UMA-like setup
        // (no Warm tier in the vec). Returns typed RoleNotConfigured.
        let result = mgr.page_out(persona, page, TierRole::Warm).await;
        match result {
            Err(TierError::RoleNotConfigured { role }) => {
                assert_eq!(role, TierRole::Warm);
            }
            other => panic!("expected RoleNotConfigured, got {other:?}"),
        }
    }

    /// What this catches: pinned pages survive page_out (skipped
    /// silently per the trait docstring). Composition layer holds
    /// the pin; manager respects it.
    #[tokio::test]
    async fn page_out_skips_pinned_pages_silently() {
        let page = make_page(6);
        let fast = StubTier::new(TierRole::Fast, vec![page]);
        let bench = StubTier::new(TierRole::Bench, vec![]);
        let mgr = LocalWorkingSetManager::new(vec![fast, bench]);
        let persona = make_persona(13);
        mgr.register_persona(persona, capacity_uma());

        let _ = mgr.page_in(persona, page).await;

        // Manually pin the page (composition would normally do this).
        {
            let mut working_sets = mgr.working_sets.write();
            if let Some(ws) = working_sets.get_mut(&persona) {
                let key = serde_json::to_string(&page).unwrap();
                if let Some(resident) = ws.pages.get_mut(&key) {
                    resident.pinned = true;
                }
            }
        }

        // page_out is a no-op for pinned page.
        let result = mgr.page_out(persona, page, TierRole::Bench).await;
        assert!(result.is_ok());

        // Page is still in the working set.
        let snapshot = mgr.working_set_snapshot(persona).unwrap();
        let key = serde_json::to_string(&page).unwrap();
        assert!(snapshot.pages.contains_key(&key));
    }

    /// What this catches: working_set_snapshot reflects what page_in
    /// recorded. Diagnostic helper correctness — tests + telemetry
    /// rely on this to verify state without holding the lock.
    #[tokio::test]
    async fn working_set_snapshot_reflects_page_in_state() {
        let page = make_page(7);
        let fast = StubTier::new(TierRole::Fast, vec![page]);
        let mgr = LocalWorkingSetManager::new(vec![fast]);
        let persona = make_persona(14);
        mgr.register_persona(persona, capacity_uma());

        // Pre-page-in: empty.
        let pre = mgr.working_set_snapshot(persona).unwrap();
        assert!(pre.pages.is_empty());

        // After page_in: one resident page.
        let _ = mgr.page_in(persona, page).await;
        let post = mgr.working_set_snapshot(persona).unwrap();
        assert_eq!(post.pages.len(), 1);
        let key = serde_json::to_string(&page).unwrap();
        let resident = post.pages.get(&key).unwrap();
        assert_eq!(resident.role, TierRole::Fast);
        assert_eq!(resident.access_count_window, 1);
        assert!(!resident.pinned);
    }

    /// What this catches: tier_count returns the configured tier
    /// count. Cheap O(1) — used by the governor's policy diagnostics
    /// to verify the manager was wired with the right Vec<TierConfig>
    /// shape (4 on UMA, 5 on discrete-GPU).
    #[tokio::test]
    async fn tier_count_reflects_configured_tiers() {
        let mgr = LocalWorkingSetManager::new(vec![
            StubTier::new(TierRole::Fast, vec![]),
            StubTier::new(TierRole::Bench, vec![]),
            StubTier::new(TierRole::Cold, vec![]),
            StubTier::new(TierRole::Frozen, vec![]),
        ]);
        assert_eq!(mgr.tier_count(), 4);
    }

    // ─── PR-5 bus-publishing tests ──────────────────────────────

    use crate::genome::bus::{all_genome_artifact_selectors, ACCESS_DENIED_KEY, PAGE_FAULT_KEY};
    use crate::runtime::artifact_handle::{ArtifactKey, ArtifactSelector};
    use crate::runtime::runtime::Runtime;
    use crate::runtime::service_module::{CommandResult, ModuleConfig, ModulePriority, ServiceModule};
    use std::any::Any;

    /// Recording subscriber for the PR-5 bus tests. Captures every
    /// (artifact_key, payload) so the test can assert which fired.
    struct RecorderModule {
        captured: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
    }

    impl RecorderModule {
        fn new() -> (Arc<Self>, Arc<Mutex<Vec<(String, serde_json::Value)>>>) {
            let captured = Arc::new(Mutex::new(Vec::new()));
            let module = Arc::new(Self {
                captured: captured.clone(),
            });
            (module, captured)
        }
    }

    #[async_trait]
    impl ServiceModule for RecorderModule {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: "pr5-recorder",
                priority: ModulePriority::Normal,
                command_prefixes: &[],
                event_subscriptions: &[],
                needs_dedicated_thread: false,
                max_concurrency: 0,
                tick_interval: None,
            }
        }
        async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
            Ok(())
        }
        async fn handle_command(
            &self,
            _: &str,
            _: serde_json::Value,
        ) -> Result<CommandResult, String> {
            Err("not handled".to_string())
        }
        fn artifact_subscriptions(&self) -> Vec<ArtifactSelector> {
            all_genome_artifact_selectors()
        }
        async fn on_artifact_available(
            &self,
            key: &ArtifactKey,
            payload: serde_json::Value,
        ) -> Result<(), String> {
            self.captured
                .lock()
                .push((key.as_str().to_string(), payload));
            Ok(())
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    /// Helper: construct a Runtime + LocalWorkingSetManager wired
    /// through it. Returns the manager + the recorder's captured
    /// events. Used by the next several tests.
    async fn wire_manager_to_runtime(
        tiers: Vec<Arc<dyn TierStore>>,
    ) -> (
        LocalWorkingSetManager,
        Arc<Runtime>,
        Arc<Mutex<Vec<(String, serde_json::Value)>>>,
    ) {
        // Build runtime, register recorder.
        let runtime = Arc::new(Runtime::new());
        let (recorder, captured) = RecorderModule::new();
        runtime.register(recorder);

        // Pull bus + registry as Arcs via the helper accessors.
        // Runtime exposes `bus_arc()` and `registry_arc()` for this.
        let bus = runtime.bus_arc();
        let registry = runtime.registry_arc();

        let mgr = LocalWorkingSetManager::with_bus(tiers, bus, registry);
        (mgr, runtime, captured)
    }

    /// What this catches: with the bus hook wired, `page_in` for a
    /// true cold miss (no tier has the page) publishes a PageFault
    /// with `from_role: None`. The whole chain — manager →
    /// publish_page_fault → bus.subscribe_artifact → recorder
    /// on_artifact_available — fires end-to-end.
    #[tokio::test]
    async fn page_in_true_cold_miss_with_bus_publishes_page_fault() {
        let cold = StubTier::new(TierRole::Cold, vec![]);
        let fast = StubTier::new(TierRole::Fast, vec![]);
        let (mgr, _runtime, captured) = wire_manager_to_runtime(vec![fast, cold]).await;

        let persona = make_persona(30);
        mgr.register_persona(persona, capacity_uma());

        let page = make_page(31);
        let result = mgr.page_in(persona, page).await;
        assert!(result.is_err(), "true cold miss returns Err(PageFault)");

        // Yield to let the spawned publish task run.
        for _ in 0..50 {
            tokio::task::yield_now().await;
            if !captured.lock().is_empty() {
                break;
            }
        }

        let events = captured.lock().clone();
        let faults: Vec<_> = events.iter().filter(|(k, _)| k == PAGE_FAULT_KEY).collect();
        assert_eq!(faults.len(), 1, "exactly one PageFault published");
        let fault: PageFault = serde_json::from_value(faults[0].1.clone()).unwrap();
        assert_eq!(fault.from_role, None, "true cold miss has no from_role");
        assert_eq!(fault.persona, persona);
        assert_eq!(fault.page, page);
    }

    /// What this catches: page_in tier-promotion (page exists in Cold,
    /// promoted to Fast) publishes a PageFault with from_role=Some(Cold)
    /// and to_role=Fast. Sentinel uses this to learn the persona's
    /// promotion pattern.
    #[tokio::test]
    async fn page_in_tier_promotion_with_bus_publishes_correct_fields() {
        let page = make_page(40);
        let cold = StubTier::new(TierRole::Cold, vec![page]);
        let fast = StubTier::new(TierRole::Fast, vec![]);
        let (mgr, _runtime, captured) = wire_manager_to_runtime(vec![fast, cold]).await;

        let persona = make_persona(41);
        mgr.register_persona(persona, capacity_uma());

        let _ = mgr.page_in(persona, page).await;

        for _ in 0..50 {
            tokio::task::yield_now().await;
            if !captured.lock().is_empty() {
                break;
            }
        }

        let events = captured.lock().clone();
        let faults: Vec<_> = events.iter().filter(|(k, _)| k == PAGE_FAULT_KEY).collect();
        assert_eq!(faults.len(), 1);
        let fault: PageFault = serde_json::from_value(faults[0].1.clone()).unwrap();
        assert_eq!(fault.from_role, Some(TierRole::Cold));
        assert_eq!(fault.to_role, TierRole::Fast);
    }

    /// What this catches: page_in resident-hit (page already in the
    /// working set) does NOT publish a PageFault. PageFault is only
    /// for misses — pinning the resident-hit path's silence prevents
    /// noisy events for hot pages.
    #[tokio::test]
    async fn page_in_resident_hit_with_bus_does_not_publish() {
        let page = make_page(50);
        let fast = StubTier::new(TierRole::Fast, vec![page]);
        let (mgr, _runtime, captured) = wire_manager_to_runtime(vec![fast]).await;

        let persona = make_persona(51);
        mgr.register_persona(persona, capacity_uma());

        // First call: tier promotion → 1 PageFault published.
        let _ = mgr.page_in(persona, page).await;
        for _ in 0..50 {
            tokio::task::yield_now().await;
            if !captured.lock().is_empty() {
                break;
            }
        }
        assert_eq!(
            captured
                .lock()
                .iter()
                .filter(|(k, _)| k == PAGE_FAULT_KEY)
                .count(),
            1
        );

        // Second call: resident hit → NO additional PageFault.
        let _ = mgr.page_in(persona, page).await;
        // Yield a few times to give any incorrectly-spawned publish a
        // chance to run — we want to assert no additional event.
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            captured
                .lock()
                .iter()
                .filter(|(k, _)| k == PAGE_FAULT_KEY)
                .count(),
            1,
            "resident-hit path must not publish"
        );
    }

    /// What this catches: audit_access denial spawns a publish through
    /// the current tokio runtime. The sync trait method returns
    /// immediately; the publish completes asynchronously. Test polls
    /// briefly because the spawn isn't synchronously joined.
    #[tokio::test]
    async fn audit_access_denial_with_bus_publishes_via_spawn() {
        let fast = StubTier::new(TierRole::Fast, vec![]);
        let (mgr, _runtime, captured) = wire_manager_to_runtime(vec![fast]).await;

        let owner = make_persona(60);
        let intruder = make_persona(61);
        let page = make_page(62);
        mgr.register_persona(owner, capacity_uma());
        mgr.register_persona(intruder, capacity_uma());
        mgr.register_page_owner(page, owner);

        // Cross-persona access — Err returned immediately, publish
        // spawned.
        let result = mgr.audit_access(intruder, page);
        assert!(result.is_err());

        // Yield so the spawned publish task gets a chance to run.
        // tokio::yield_now() inside a loop bounded by attempts is the
        // safe way to wait without a fixed sleep.
        for _ in 0..50 {
            tokio::task::yield_now().await;
            if !captured.lock().is_empty() {
                break;
            }
        }

        let events = captured.lock().clone();
        let denied_events: Vec<_> = events
            .iter()
            .filter(|(k, _)| k == ACCESS_DENIED_KEY)
            .collect();
        assert_eq!(denied_events.len(), 1, "exactly one AccessDenied published");
        let denied: AccessDenied = serde_json::from_value(denied_events[0].1.clone()).unwrap();
        assert_eq!(denied.actor, intruder);
        assert_eq!(denied.owner, Some(owner));
    }

    /// What this catches: audit_access for same-persona access does
    /// NOT publish. Only denials are observable events.
    #[tokio::test]
    async fn audit_access_allowed_with_bus_does_not_publish() {
        let fast = StubTier::new(TierRole::Fast, vec![]);
        let (mgr, _runtime, captured) = wire_manager_to_runtime(vec![fast]).await;

        let owner = make_persona(70);
        let page = make_page(71);
        mgr.register_persona(owner, capacity_uma());
        mgr.register_page_owner(page, owner);

        // Owner accessing own page: Ok.
        let result = mgr.audit_access(owner, page);
        assert!(result.is_ok());

        // Yield in case anything was queued.
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }

        let events = captured.lock().clone();
        let denied_events: Vec<_> = events
            .iter()
            .filter(|(k, _)| k == ACCESS_DENIED_KEY)
            .collect();
        assert!(denied_events.is_empty(), "no denial = no event");
    }

    /// What this catches: bus-less mode (via `new` instead of
    /// `with_bus`) still works — the trait methods behave identically
    /// to PR-3, just without publishing. Backwards-compat for the
    /// standalone use case.
    #[tokio::test]
    async fn bus_less_mode_does_not_publish_but_methods_work() {
        let page = make_page(80);
        let fast = StubTier::new(TierRole::Fast, vec![page]);
        // `new` instead of `with_bus` — no bus hook.
        let mgr = LocalWorkingSetManager::new(vec![fast]);
        let persona = make_persona(81);
        mgr.register_persona(persona, capacity_uma());

        // page_in still returns Err(PageFault) — caller-side
        // observability still works through the Result arm.
        let result = mgr.page_in(persona, page).await;
        assert!(result.is_err());

        // audit_access still returns the typed denial — no spawn,
        // no publish, no observable side effect (the typed Result
        // is THE signal).
        let result = mgr.audit_access(persona, page);
        assert!(result.is_ok());
    }

    // ─── Measured page-in cost (the genome-overlay multiplicity proof) ───
    //
    // These tests pin the measurement that turns the page-fault system
    // from typed-but-blind into measured: `PageFault.elapsed_us` is now
    // the real servicing cost, not the `0` it shipped with. That number
    // is what proves "persona N = a cheap LoRA page-in, not a model
    // load" — see [[persona-is-a-genome-overlay-not-an-instance]].

    /// What this catches: a LoRA-overlay page-in records REAL servicing
    /// time in `PageFault.elapsed_us` — not the hardcoded `0` it shipped
    /// with. A slow lower tier (3ms read) must surface as elapsed_us in
    /// the millisecond range. This is the measurement the genome-overlay
    /// thesis stands on; sentinel/governor compare overlay page-in cost
    /// across personas from this field on the bus.
    /// regression for: elapsed_us:0 stub at the page_in fault arms.
    #[tokio::test]
    async fn page_in_fault_records_real_elapsed_us() {
        let page = make_page(90);
        // Model a slow cold tier so servicing is measurable + non-flaky.
        // tokio::time::sleep is a floor — jitter only lengthens it.
        let cold = StubTier::with_delay(
            TierRole::Cold,
            vec![page],
            std::time::Duration::from_millis(3),
        );
        let fast = StubTier::new(TierRole::Fast, vec![]);
        let mgr = LocalWorkingSetManager::new(vec![fast, cold]);
        let persona = make_persona(91);
        mgr.register_persona(persona, capacity_uma());

        let fault = mgr.page_in(persona, page).await.unwrap_err();
        assert_eq!(fault.from_role, Some(TierRole::Cold));
        assert!(
            fault.elapsed_us >= 1000,
            "page-in servicing time must be measured (>=1ms for a 3ms tier read), got {}us",
            fault.elapsed_us
        );
    }

    /// What this catches: even a true cold miss (page in NO tier) records
    /// the time it spent walking the whole tier chain — the cold-miss arm
    /// isn't still hardcoded to 0. Two 2ms tiers searched → the elapsed
    /// covers both reads.
    #[tokio::test]
    async fn page_in_cold_miss_records_elapsed_for_full_walk() {
        let page = make_page(92);
        let fast =
            StubTier::with_delay(TierRole::Fast, vec![], std::time::Duration::from_millis(2));
        let cold =
            StubTier::with_delay(TierRole::Cold, vec![], std::time::Duration::from_millis(2));
        let mgr = LocalWorkingSetManager::new(vec![fast, cold]);
        let persona = make_persona(93);
        mgr.register_persona(persona, capacity_uma());

        let fault = mgr.page_in(persona, page).await.unwrap_err();
        assert_eq!(fault.from_role, None);
        assert!(
            fault.elapsed_us >= 2000,
            "cold-miss must time the full tier walk (two 2ms tiers), got {}us",
            fault.elapsed_us
        );
    }

    /// What this catches: THE genome-overlay-as-multiplicity thesis. Two
    /// personas share ONE manager (one base model); each pages in her OWN
    /// LoRA overlay from the cold tier. Each gets a measured page-in
    /// fault, and each working set holds ONLY her own overlay — overlays
    /// don't leak across personas. "Multiple personas" = multiple LoRA
    /// overlays multiplexed through one pager, O(page-in) each.
    #[tokio::test]
    async fn two_personas_page_in_own_overlays_through_one_manager() {
        let overlay_a = make_page(100); // Asha's LoRA overlay
        let overlay_b = make_page(101); // second persona's overlay
        let fast = StubTier::new(TierRole::Fast, vec![]);
        let cold = StubTier::with_delay(
            TierRole::Cold,
            vec![overlay_a, overlay_b],
            std::time::Duration::from_millis(2),
        );
        let mgr = LocalWorkingSetManager::new(vec![fast, cold]);

        let asha = make_persona(100);
        let nova = make_persona(101);
        mgr.register_persona(asha, capacity_uma());
        mgr.register_persona(nova, capacity_uma());

        // Each persona pages in her own overlay through the shared base.
        let fault_a = mgr.page_in(asha, overlay_a).await.unwrap_err();
        let fault_b = mgr.page_in(nova, overlay_b).await.unwrap_err();
        assert!(fault_a.elapsed_us >= 1000 && fault_a.persona == asha);
        assert!(fault_b.elapsed_us >= 1000 && fault_b.persona == nova);

        // Isolation: each working set holds ONLY its own overlay.
        let asha_ws = mgr.working_set_snapshot(asha).unwrap();
        let nova_ws = mgr.working_set_snapshot(nova).unwrap();
        let key_a = serde_json::to_string(&overlay_a).unwrap();
        let key_b = serde_json::to_string(&overlay_b).unwrap();
        assert!(
            asha_ws.pages.contains_key(&key_a) && !asha_ws.pages.contains_key(&key_b),
            "Asha's working set holds only her overlay"
        );
        assert!(
            nova_ws.pages.contains_key(&key_b) && !nova_ws.pages.contains_key(&key_a),
            "Nova's working set holds only her overlay"
        );

        // Resident-hit is the hot path: re-paging an already-resident
        // overlay is a cached Ok with NO fault — the page-in cost is
        // paid once, then it's free. This is why overlay multiplexing
        // scales: O(page-in) once, O(1) thereafter.
        let hit = mgr.page_in(asha, overlay_a).await;
        assert!(
            hit.is_ok(),
            "resident overlay re-page is a hot hit, no fault"
        );
    }

    // ─── Benchmark: science the multiplicity proof ──────────────────────
    //
    // Gated behind `stress-tests` per the test doctrine — default
    // `cargo test` skips it; run it on demand to SEE the numbers:
    //
    //   CARGO_TARGET_DIR="$HOME/.continuum/cache/cargo-target" \
    //   cargo test -p continuum-core --features stress-tests \
    //     genome::local_manager::tests::stress -- --nocapture
    //
    // The headline it benchmarks: per-persona overlay page-in cost stays
    // FLAT as persona count grows — "add persona N" is O(page-in), the
    // claim [[persona-is-a-genome-overlay-not-an-instance]] rests on.
    #[cfg(feature = "stress-tests")]
    mod stress {
        use super::*;

        fn percentile(sorted: &[u64], p: f64) -> u64 {
            if sorted.is_empty() {
                return 0;
            }
            let idx = (((sorted.len() - 1) as f64) * p).round() as usize;
            sorted[idx]
        }

        /// What this catches: regression in per-persona page-in scaling.
        /// Sweeps persona counts through ONE manager (one base model);
        /// each persona pages in a DISTINCT LoRA overlay resident on a
        /// zero-delay cold tier (we measure pager overhead, not disk).
        /// Prints total wall, per-persona mean, p95 servicing-us, and the
        /// resident-hit cost. Flat per-persona cost across counts ⇒ the
        /// genome-overlay-as-multiplicity thesis holds by measurement.
        #[tokio::test]
        async fn bench_overlay_page_in_scales_per_persona() {
            eprintln!("\n=== genome overlay multiplicity bench (one base, N overlays) ===");
            eprintln!(
                "{:>8} | {:>10} | {:>9} | {:>9} | {:>12}",
                "personas", "wall_us", "per_us", "p95_us", "resident_ns"
            );
            for &count in &[1usize, 8, 32, 64, 128, 256] {
                let overlays: Vec<_> = (0..count).map(|i| make_page(1_000 + i as u128)).collect();
                let cold = StubTier::new(TierRole::Cold, overlays.clone());
                let fast = StubTier::new(TierRole::Fast, vec![]);
                let mgr = LocalWorkingSetManager::new(vec![fast, cold]);
                let personas: Vec<_> = (0..count)
                    .map(|i| make_persona(1_000 + i as u128))
                    .collect();
                for &p in &personas {
                    mgr.register_persona(p, capacity_uma());
                }

                let mut serviced = Vec::with_capacity(count);
                let wall = std::time::Instant::now();
                for i in 0..count {
                    let fault = mgr.page_in(personas[i], overlays[i]).await.unwrap_err();
                    serviced.push(fault.elapsed_us);
                }
                let wall_us = wall.elapsed().as_micros() as u64;
                serviced.sort_unstable();
                let per_us = wall_us / count as u64;
                let p95 = percentile(&serviced, 0.95);

                // Resident-hit: re-page persona 0's now-hot overlay.
                let hot = std::time::Instant::now();
                let _ = mgr.page_in(personas[0], overlays[0]).await;
                let resident_ns = hot.elapsed().as_nanos() as u64;

                eprintln!(
                    "{:>8} | {:>10} | {:>9} | {:>9} | {:>12}",
                    count, wall_us, per_us, p95, resident_ns
                );
            }
            eprintln!(
                "=== flat per_us across counts ⇒ overlay multiplicity is O(page-in)/persona ===\n"
            );
        }
    }
