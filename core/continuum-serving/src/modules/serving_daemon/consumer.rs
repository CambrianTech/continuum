//! Consumer ownership for the serving daemon.

use super::*;

/// Build a footprint for one registry model IFF its GGUF is actually on disk —
/// we can only serve what's present. `None` for cloud models, models with no
/// resolved local file, or anything we can't stat.
///
/// Footprint estimates are honest where we can be (weights = real file size)
/// and COARSE where the registry lacks the data (per-lane KV, capability rank).
/// Refine when the registry carries arch internals (layers × kv_heads ×
/// head_dim) and a real capability score; the classifier consumes whatever
/// precision we give it without changing shape.
/// The [`FootprintFn`] serving hands the [`ResourceGovernor`](crate::resources):
/// resolve an active model id + its live serving shape (served per-slot window,
/// lane count) → total PEAK resident bytes in VRAM, from the SAME live catalog +
/// [`footprint_for`] estimator the serving plan uses (one footprint authority,
/// not two). An id the catalog doesn't know, or a model with no on-disk weights,
/// resolves to `0` — nothing resident to attribute. This reports the FULL PEAK
/// (#56/G5): weights + the KV-cache of every lane at the served window + the
/// concurrent-prefill compute reserve of every lane — i.e. `peak_resident_bytes`,
/// which equals the plan's `chosen_cost` exactly. Folding in the KV term (#79)
/// stopped serving's own KV masquerading as external; folding in the compute
/// reserve (G5) stops the board over-reporting free VRAM by the prefill buffer,
/// which is the bytes a SECOND consumer (the eval lane, a train job) would grab
/// out from under serving's next prefill → the concurrent-OOM.
///
/// …PLUS THE PROMPT CACHE THIS DAEMON GRANTED THE ENGINE (2026-09-20 02:4xZ, the M5).
/// The board attributed 22.8 GB to serving (weights + KV + compute for 1 × 64,512) while
/// the lane's process held 27.5 GB: its anonymous footprint was 8.27 GB against an 8.7 GB
/// `--cache-ram` grant (`serving.footprint.measured host_cache_bytes`). The grant is RAM
/// this daemon handed the engine and it fills over the serve's life, so every byte of it
/// read as EXTERNAL — another holder's — and `budget_for_replacing` (available + our own
/// measured bytes) lost it. Under the 12 GiB Eco reserve that was the second lane: the
/// plan read 18 GB usable and one lane at 2,048 tokens on a box serving two at 64k. The
/// same shape as the KV term (#79) and the compute reserve (G5): a cost we own, filed as
/// someone else's. `attribute_serving` is the pure sum; nothing resident charges nothing.
///
/// The grant is the one on the SNAPSHOT — what the engine was launched with — handed in
/// by the consumer beside the window and lanes it reads from the same snapshot. The first
/// cut read a daemon cell the target derivation rewrote; that cell moved under a running
/// engine (2026-09-20 07:52Z) and the board credited a grant no process held.
pub(super) fn serving_footprint_fn(catalog: Arc<ModelCatalog>) -> FootprintFn {
    Arc::new(
        move |id: &str, served_window: u32, lanes: u32, grant_mib: u32| {
            catalog
                .snapshot()
                .get(id)
                .and_then(|live| footprint_for(&live.model))
                .map(|fp| {
                    attribute_serving(
                        fp.peak_resident_bytes(served_window, lanes),
                        lanes,
                        grant_mib,
                    )
                })
                .unwrap_or(0)
        },
    )
}

/// PURE: what serving holds — the plan's peak resident bytes plus the host prompt cache
/// granted to the running lanes. No lanes (or nothing resident) = no cache to charge.
pub fn attribute_serving(peak_resident_bytes: u64, lanes: u32, granted_cache_mib: u32) -> u64 {
    if peak_resident_bytes == 0 || lanes == 0 {
        return peak_resident_bytes;
    }
    peak_resident_bytes.saturating_add((granted_cache_mib as u64).saturating_mul(1024 * 1024))
}

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

    /// The registry personas report their turn demand into. Cheap clone — handed to
    /// each spawned mind's deliberation faculty so the measurement reaches the planner.
    pub fn working_set(&self) -> crate::cognition::working_set::WorkingSetRegistry {
        self.working_set.clone()
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
        let vision_source = Arc::new(
            crate::inference::serving_footprints::CatalogFootprintSource::vision(
                self.catalog.clone(),
                self.subscribe_serving(),
            ),
        );
        continuum_base::probe!(
            target: "continuum_core::modules::serving_daemon", class = "resources.footprint.wired",
            holder = "vision",
            source = "CatalogFootprintSource::vision",
            "vision footprint source registered with the authority",
        );
        self.resource_daemon.add_consumer(Arc::new(
            crate::inference::serving_footprints::MonitoredHolder::new(
                vision_source,
                "vision provider (sidecar or main lane)",
                "no on-demand release: vision residency is dropped by the serving \
                 reconcile when the main lane can see (#106/#395)",
            ),
        ));
    }
}
