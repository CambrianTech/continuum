//! WIRING: the serving daemon's registrations with the memory authority and its injected sources.
//! Split out of serving_daemon.rs (card dbc9711c, BigMama's step 6).

use super::*;

impl ServingDaemonModule {
    /// The current lane demand (≥ 1).
    /// Register serving's autonomic PLANNER to run on the memory authority's tick
    /// (MEMORY-AUTHORITY-DAEMON slice 1b). The lane plan — which model, how many lanes,
    /// what per-slot window — is a MEMORY decision, so it must be computed in the ONE
    /// place that owns memory (the `ResourceDaemon` tick), NOT on serving's own tick
    /// sampling `host_budget()`. `recompute()` (compute the plan from the live host +
    /// publish it to `plan_tx`) now runs as an `on_tick` observer; serving's own tick
    /// keeps only `reconcile_to_plan()` (bring llama-server in line with the published
    /// plan — reacting to the authority's decision, not deciding). Weak handle so the
    /// observer never keeps the module alive past shutdown. Called once at wiring time,
    /// after the module is Arc-wrapped.
    /// Tell the memory authority who we are and how big the device is — **before** the planner
    /// is allowed to run on its tick.
    ///
    /// # Why this is wiring-time and not `initialize` (#438, measured 2026-08-19)
    ///
    /// `governed_vram_ceiling` budgets serving through
    /// [`budget_for_replacing`](crate::resources::ResourceDaemon::budget_for_replacing) —
    /// available PLUS serving's own resident bytes, because a consumer choosing its successor
    /// releases what it holds. That add-back is what makes a past form of ourself count as OURS
    /// rather than as an external squeeze. It is keyed on serving being a *registered consumer*:
    /// with no registration there is no footprint to add back, and the credit silently becomes 0.
    ///
    /// These two calls used to live in `initialize`, which the runtime invokes AFTER the module
    /// is registered — while [`Self::register_planner_on_authority_tick`] attaches the planner at
    /// wiring time. The authority could therefore tick a plan in the gap, and did: a successor
    /// core booting on top of its predecessor's resident 27B read `usable_gb = 0` on a 53 GiB
    /// board for ~0.6 s, because its own 44 GB were filed under nobody. The downshift debounce
    /// held the plan and nothing was actuated — but a guard covering for a budget that is
    /// momentarily lying is not the same as the budget being honest
    /// ([[hand-rolled-ops-are-waste-that-masks-the-real-defect]]).
    ///
    /// Ordering, not duplication: `seed_device_vram_prior` is a set-once prior and
    /// `add_consumer` clears any stale quarantine for the id, so calling this early is safe.
    /// The device's total VRAM is a static hardware fact available immediately — there is no
    /// reason to learn it later than the first tick that plans against it.
    pub fn declare_to_memory_authority(&self) {
        seed_device_vram_prior(self.gpu.total_vram_bytes());
        self.register_as_consumer();
        // Same ordering law as the two lines above, for the OTHER half of the plan's inputs.
        // Those two make our own footprint visible to the authority before a plan may tick;
        // this makes the host's earned DEMAND visible in the same window. Without it the
        // first plans run with `ceiling() == None` and serve BOOTSTRAP_WORKING_SET — measured
        // 2026-08-20 as a 27B held at 16,384 while a 31,834-token peak sat on disk.
        self.working_set.rehydrate_all();
    }

    pub fn register_planner_on_authority_tick(self: &Arc<Self>) {
        let weak = Arc::downgrade(self);
        self.resource_daemon
            .on_tick(Arc::new(move |_board: &LeaseBoard| {
                if let Some(module) = weak.upgrade() {
                    module.recompute();
                }
            }));
    }

    /// Test seam: override how planned model ids resolve to [`Model`] structs,
    /// so the reconcile wiring runs without a populated global registry.
    #[cfg(test)]
    pub(super) fn set_model_resolver(&mut self, resolver: ModelResolver) {
        self.model_resolver = resolver;
    }

    /// Test seam: own the inherited-lane evidence instead of reading the operator's real
    /// pidfile + registry. See [`InheritedLaneSource`] for why this MUST be injected — the
    /// production default reads a filesystem singleton, and the identical leak was already
    /// caught by measurement in `serving_consumer`.
    #[cfg(test)]
    pub(super) fn set_inherited_lane(&mut self, source: InheritedLaneSource) {
        self.inherited_lane = source;
    }

    /// Test seam: own the heartbeat's real-decode evidence instead of inheriting whatever
    /// the process-global happens to hold. See [`DecodeAgeSource`] for why this exists.
    #[cfg(test)]
    /// Tests: the leased-in prompt samples the demand pool folds in, in place of the
    /// process-global ring.
    #[cfg(test)]
    pub(super) fn set_leased_in_sent_source(&mut self, source: LeasedInSentSource) {
        self.leased_in_sent = source;
    }

    pub(super) fn set_decode_age_source(&mut self, source: DecodeAgeSource) {
        self.decode_age = source;
    }

    /// Test seam: own the heartbeat's real-turn-failure evidence (#363) instead of
    /// inheriting the process-global stamped by other tests' adapters. Same rationale
    /// as [`Self::set_decode_age_source`].
    #[cfg(test)]
    pub(super) fn set_real_fails_source(&mut self, source: RealFailsSource) {
        self.real_fails = source;
    }

    /// Subscribe to the published serving plan. Consumers (scheduler, spawner)
    /// hold the receiver and react to plan changes — the ebb/flow seam.
    pub fn subscribe(&self) -> watch::Receiver<Option<ServingPlan>> {
        self.plan_view_tx.subscribe()
    }

    /// Register serving as a MEASURED [`ResourceConsumer`] with the one
    /// per-machine authority (#79) — monitor-not-reserve. Serving does NOT
    /// acquire a lease here; it keeps loading through its own plan/reconcile
    /// loop. It hands the governor exactly the two handles the authority needs:
    ///
    /// - the serving snapshot + a footprint resolver, so the governor's reconcile
    ///   tick background-polls serving's resident VRAM and *attributes* it on the
    ///   board (the fix for the `granted:0 while the GPU is full` blindness), and
    /// - the suppress-set writer, so when a peer needs the bytes the authority can
    ///   ASK serving to free the active model (whole-lease-granular unload), and
    ///   serving answers honestly across the async unload.
    ///
    /// `available = capacity − granted` is untouched — this only surfaces the
    /// measured axis. The footprint resolver is the SAME catalog + footprint
    /// estimator that feeds the serving plan, so there is ONE footprint authority
    /// on the box, not two.
    pub fn register_as_consumer(&self) {
        // The live servable-model candidates, sized to whatever shape serving is running,
        // so the tier-down ranker re-homes only to a model the autonomic plan itself could
        // serve (same suppress/pin/eligibility filter → one catalog, never two). Reads the
        // catalog snapshot + suppress/pin watches at reclaim time; no lock held across the
        // async handshake (it returns an owned Vec).
        let catalog = self.catalog.clone();
        let intent_rx = self.intent.subscribe();
        let candidates: crate::modules::serving_tier_down::TierCandidatesFn =
            Arc::new(move |window: u32, lanes: u32| {
                let snap = catalog.snapshot();
                let intent = intent_rx.borrow();
                servable_candidates(&snap, &intent.suppressed, &intent.pinned)
                    .into_iter()
                    .map(|f| {
                        // Peak (weights + KV + prefill compute reserve), the SAME number
                        // the board attributes to serving — so a shrink target's freed
                        // bytes are measured against serving's true footprint, not a
                        // resident figure that omits the compute buffer (#56/G5).
                        let resident_bytes = f.peak_resident_bytes(window, lanes);
                        crate::modules::serving_tier_down::TierCandidate {
                            model_id: f.model_id,
                            capability_rank: f.capability_rank,
                            resident_bytes,
                        }
                    })
                    .collect()
            });
        let consumer = ServingConsumer::new(
            self.subscribe_serving(),
            self.intent.clone(),
            serving_footprint_fn(self.catalog.clone()),
            serving_pool_kind(),
            // #56: under a VRAM reclaim (a game grabbed the GPU, a peer needs the bytes),
            // shrink to the most-capable smaller model that frees enough — "take our own
            // capacity down to yield, keep answering" — instead of the whole-lease dark.
            // The autonomic plan grows back up when pressure clears. Falls through to a
            // full unload only when no smaller model frees enough.
            Arc::new(crate::modules::serving_tier_down::CatalogTierDownPolicy::new(candidates)),
        )
        // On a discrete GPU the board credits serving with what its ENGINE holds on the
        // device, recorded by this daemon's footprint tick (card 741f5eec).
        .with_engine_device_credit(self.engine_device_credit.clone());
        self.resource_daemon.add_consumer(Arc::new(consumer));

        // THE VISION HOLDER declares itself (#106/#395/#56). Measured on the live board
        // before this: vram physUsed 50.95G / attributed 6.81G / UNOWNED 44.14G — the
        // governor could see the bytes were gone and not who had them, and unowned reads
        // as immovable, so a model that physically fits was refused. The vision provider
        // was one of the largest unowned blocks (~9.4G for the VL-7B sidecar).
        //
        // Monitor-only, and deliberately so: it declares the bytes but refuses reclaim
        // out loud, because the release path is the serving reconcile
        // (`if main_sees { sidecar = None }`), not an inbound handler. Declaring a
        // reclaim it cannot perform would have the authority plan against a release that
        // silently never happens.
        let vision_source = Arc::new(crate::modules::serving_footprints::CatalogFootprintSource::vision(
            self.catalog.clone(),
            self.subscribe_serving(),
        ));
        crate::probe!(
            class = "resources.footprint.wired",
            holder = "vision",
            source = "CatalogFootprintSource::vision",
            "vision footprint source registered with the authority",
        );
        self.resource_daemon
            .add_consumer(Arc::new(crate::modules::serving_footprints::MonitoredHolder::new(
                vision_source,
                "vision provider (sidecar or main lane)",
                "no on-demand release: vision residency is dropped by the serving \
                 reconcile when the main lane can see (#106/#395)",
            )));
    }
}
