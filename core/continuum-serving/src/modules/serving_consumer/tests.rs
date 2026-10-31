#![cfg(test)]
    /// what this catches (card 741f5eec, the 5090 on 2026-10-07): serving's own engine filed
    /// as EXTERNAL on a discrete GPU, and a reading credited to the wrong engine. On a
    /// discrete card the credit is what the engine reported, only for the engine it was read
    /// from (a pre-relaunch reading is never the successor's), nothing when unread, and
    /// never the host-process read in its place. Off a discrete card the host read applies.
    #[test]
    fn a_discrete_engine_is_credited_its_own_device_holding_and_nothing_else() {
        use super::{device_answer, DeviceAnswer, DeviceCreditState};
        let read = |pid, bytes| DeviceCreditState::Discrete { pid: Some(pid), bytes };
        assert_eq!(device_answer(read(7, Some(31_000_000_000)), Some(7)), DeviceAnswer::Discrete(Some(31_000_000_000)));
        assert_eq!(device_answer(read(7, Some(31_000_000_000)), Some(8)), DeviceAnswer::Discrete(None), "a previous engine's reading");
        assert_eq!(device_answer(read(7, Some(31_000_000_000)), None), DeviceAnswer::Discrete(None), "the gap between engines");
        assert_eq!(device_answer(read(7, None), Some(7)), DeviceAnswer::Discrete(None), "unread /props is no credit, not the host read");
        assert_eq!(device_answer(DeviceCreditState::NotDiscrete, Some(7)), DeviceAnswer::NotDiscrete);
    }

    use super::*;
    use crate::modules::serving_tier_down::{DeclineTierDown, TierDown};

    // A hand-driven serving snapshot + suppress pair, standing in for the daemon.
    // The test plays the daemon's role: when it sees a model suppressed, it
    // "unloads" by clearing the snapshot — exactly what the real reconcile does,
    // minus the multi-second llama-server kill. Wired with `DeclineTierDown`, so
    // the only lever is a full unload — the tier-down path has its own rig below.
    fn rig(active: &str, bytes: u64) -> (ServingConsumer, watch::Sender<ServingSnapshot>) {
        let (serving_tx, serving_rx) = watch::channel(ServingSnapshot {
            loading_model: None,
            held_for: None,
            // test fixture: no live readiness was ever CONFIRMED here.
            ready_verified_at_ms: None,
            active_model: Some(active.to_string()),
            ready: true,
            base_url: "http://localhost:0/v1".into(),
            adapters: Vec::new(),
            served_context_window: 11008,
            lanes: 4,
            host_prompt_cache_mib: 0,
            degraded_reason: None,
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
        });
        let intent = ServingIntent::new(None);
        // Flat resident estimate — the shape (window, lanes) is ignored here so the
        // reclaim handshake tests assert against a stable footprint. The test that
        // the window/lanes actually REACH this fn lives separately below.
        let footprint_of: FootprintFn = Arc::new(move |_id: &str, _window: u32, _lanes: u32, _grant: u32| bytes);
        let consumer = ServingConsumer::new(
            serving_rx,
            intent,
            footprint_of,
            ResourceKind::Vram,
            Arc::new(DeclineTierDown),
        )
            .with_inherited_lane(Arc::new(|| None))
            .without_measurement();
        (consumer, serving_tx)
    }

    fn bytes_of(c: &ServingConsumer) -> u64 {
        c.footprint().iter().map(|f| f.bytes).sum()
    }

    // what this catches (card 628dc958): with a process to measure, the credit the
    // replace-myself budget adds back is the LIVE process's physical residency, never the
    // catalog's peak estimate — the M5 charged 47.7 GB for a process holding ~26 GB and
    // planned into swap. Without a process (a backend with nothing to measure) the
    // estimate still stands in.
    #[test]
    fn the_credit_is_the_live_process_residency_not_the_catalog_estimate() {
        let estimate = 47_700_000_000u64;
        let measured = 26_000_000_000u64;
        let (consumer, _tx) = rig("qwen3.8-27b", estimate);
        assert_eq!(bytes_of(&consumer), estimate, "nothing to measure: the estimate stands in");
        let consumer = consumer.with_measured(Arc::new(move |_m: &str| Some(measured)));
        assert_eq!(bytes_of(&consumer), measured, "the process, not the prediction");
        assert!(consumer.footprint()[0].detail.contains("measured resident"));
    }

    // what this catches (card 628dc958, Cormac): across a relaunch the credit is exactly
    // what is resident at each moment — the old engine while it is the live process, 0 in
    // the gap between engines, the successor from its first byte — so the board's physical
    // `available` plus this credit is the same number before, during and after. A prior
    // held while `available` has already risen by the same bytes was A + 2F; an estimate
    // of the loading engine credited bytes not yet resident. Neither survives here.
    #[test]
    fn the_credit_follows_the_bytes_through_a_relaunch_and_is_zero_in_the_gap() {
        let (consumer, tx) = rig("qwen3.8-27b", 20_000_000_000);
        let live = Arc::new(std::sync::Mutex::new(Some(26_000_000_000u64)));
        let probe = live.clone();
        let consumer = consumer.with_measured(Arc::new(move |_m: &str| *probe.lock().unwrap())); // JUSTIFIED unwrap: a test mutex
        tx.send_modify(|s| s.ready_verified_at_ms = Some(1_000));
        assert_eq!(bytes_of(&consumer), 26_000_000_000, "ready: the live process");
        // Torn down for a relaunch, the old process still alive for a moment.
        tx.send_modify(|s| {
            s.ready = false;
            s.active_model = None;
            s.loading_model = Some("qwen3.8-27b".into());
        });
        assert_eq!(bytes_of(&consumer), 26_000_000_000, "loading, old process alive: its bytes");
        // The gap: no live process — nothing resident, nothing credited.
        *live.lock().unwrap() = None; // JUSTIFIED unwrap: a test mutex
        assert_eq!(bytes_of(&consumer), 0, "the gap credits nothing: available already rose by those bytes");
        // The successor from its first resident byte, growing as it loads.
        *live.lock().unwrap() = Some(9_000_000_000); // JUSTIFIED unwrap: a test mutex
        assert_eq!(bytes_of(&consumer), 9_000_000_000, "the successor counts from its first byte");
        tx.send_modify(|s| {
            s.ready = true;
            s.loading_model = None;
            s.active_model = Some("qwen3.8-27b".into());
            s.ready_verified_at_ms = Some(2_000);
        });
        *live.lock().unwrap() = Some(30_000_000_000); // JUSTIFIED unwrap: a test mutex
        assert_eq!(bytes_of(&consumer), 30_000_000_000, "ready: the successor's own residency");
    }

    /// A tier-down policy that always proposes re-homing to a fixed smaller model
    /// at a fixed resident size — the outlier that exercises the Partial handshake.
    struct AlwaysTierDown {
        target: String,
        resident_after: u64,
    }
    impl TierDownPolicy for AlwaysTierDown {
        fn choose(&self, _ctx: &TierDownContext) -> Option<TierDown> {
            Some(TierDown {
                target_model: self.target.clone(),
                resident_after: self.resident_after,
            })
        }
    }

    fn ask() -> ReclaimRequest {
        ReclaimRequest {
            kind: ResourceKind::Vram,
            target_bytes: 8_000,
            deadline_ms: 1_000,
            reason: ReclaimReason::Pressure,
        }
    }

    // what this catches: footprint reports the active model's resident VRAM only
    // while it is live+ready. If serving reported a footprint for a model that
    // is not actually resident, the daemon's scan-vs-footprint reconcile would
    // see phantom bytes; if it reported nothing while a model is live, the
    // authority would think serving is reclaimable-free and over-grant into it.
    /// A rig whose footprint actually SCALES with the lane count — the shared `rig`
    /// deliberately returns a flat number ("the shape is ignored here"), which is right
    /// for the reclaim-handshake tests and useless for these: with a flat resolver a
    /// 4-lane and a 1-lane serve are the same size, so a shrink is unobservable. I wrote
    /// three tests against the flat rig before reading it.
    fn shape_rig(bytes_per_lane: u64) -> (ServingConsumer, watch::Sender<ServingSnapshot>) {
        let (serving_tx, serving_rx) = watch::channel(ServingSnapshot {
            loading_model: None,
            held_for: None,
            ready_verified_at_ms: Some(1_000),
            active_model: Some("qwen3.8-27b".into()),
            ready: true,
            base_url: "http://localhost:0/v1".into(),
            adapters: Vec::new(),
            served_context_window: 22_528,
            lanes: 4,
            host_prompt_cache_mib: 0,
            degraded_reason: None,
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
        });
        let intent = ServingIntent::new(None);
        let footprint_of: FootprintFn =
            Arc::new(move |_id: &str, _w: u32, lanes: u32, _grant: u32| bytes_per_lane * lanes as u64);
        let consumer = ServingConsumer::new(
            serving_rx,
            intent,
            footprint_of,
            ResourceKind::Vram,
            Arc::new(DeclineTierDown),
        )
            .with_inherited_lane(Arc::new(|| None))
            .without_measurement();
        (consumer, serving_tx)
    }


    // what this catches: a high-water that can never be released. If the snapshot carries
    // no `ready_verified_at_ms`, nothing can ever prove the old process died — holding a
    // claim on those bytes would strand them for the life of the process. No proof
    // channel, no claim.
    #[tokio::test]
    async fn without_a_verification_stamp_no_high_water_is_claimed() {
        let (consumer, serving_tx) = rig("qwen3-coder-30b", 18_000); // stamp is None
        assert_eq!(consumer.footprint()[0].bytes, 18_000);
        serving_tx.send_modify(|s| s.ready = false);
        assert!(
            consumer.footprint().is_empty(),
            "with no way to ever prove release, we must not claim bytes we cannot give back"
        );
    }

    // what this catches: THE COLD-LOAD HOLE, measured live 2026-08-19 AFTER the
    // high-water fix and NOT covered by it. On a first boot the consumer has no prior
    // residency and `active_ready()` is None the whole way up, so the board read
    // `serving 0.00 GB` while physical climbed 29.90 → 36.88 GB. The cause was upstream:
    // the snapshot was binary, so throughout the load window it FORGOT what it was
    // loading and the consumer had nothing to name. Now it does.
    #[tokio::test]
    async fn a_cold_load_charges_the_model_being_brought_up() {
        let (consumer, serving_tx) = shape_rig(6_000_000_000);
        serving_tx.send_modify(|s| {
            // Cold: never ready, no prior residency, no verification stamp — exactly the
            // first boot of a fresh install.
            s.ready = false;
            s.active_model = None;
            s.ready_verified_at_ms = None;
            s.loading_model = Some("qwen3.8-27b".into());
        });
        let fp = consumer.footprint();
        assert_eq!(fp.len(), 1, "a loading lane must be attributed, not silent");
        assert!(fp[0].bytes > 0, "the weights are resident from spawn");
        assert!(fp[0].detail.contains("loading"));
    }

    // what this catches: THE MEASURED #438 COLLAPSE. Mid-reshape the board read
    // `serving 25.93 GB (1 lane)` against `phys 47.25 GB` — the old 4-lane process was
    // still resident while the snapshot had already flipped to the new shape. The 21.3 GB
    // gap read as unowned, unowned reads as immovable, `available` fell to 8.42 GB, the
    // 27B stopped fitting, and the planner took a 0.5B. Shrinking the SHAPE must not
    // shrink what we claim to HOLD until something proves the bytes came back.
    #[tokio::test]
    async fn a_shrinking_shape_does_not_shrink_reported_residency_without_proof() {
        let (consumer, serving_tx) = shape_rig(6_000_000_000);
        let wide = consumer.footprint()[0].bytes;
        assert!(wide > 0, "a ready 4-lane serve must attribute something");

        // The plan reshapes to 1 lane. No NEW readiness verification yet — the old
        // process may still be resident, and nothing has shown us otherwise.
        serving_tx.send_modify(|s| s.lanes = 1);
        let during = consumer.footprint();
        assert_eq!(
            during[0].bytes, wide,
            "must still claim the wider residency until a verification proves release"
        );
        assert!(during[0].detail.contains("not yet confirmed released"));
    }

    // what this catches: the OTHER half — `active_ready()` returns None while a lane
    // loads or relaunches, and the old code returned an EMPTY vec. Zero attributed while
    // gigabytes are demonstrably resident is the same defect wearing a different face.
    #[tokio::test]
    async fn a_loading_lane_still_reports_the_bytes_it_has_not_released() {
        let (consumer, serving_tx) = shape_rig(6_000_000_000);
        let held = consumer.footprint()[0].bytes;

        serving_tx.send_modify(|s| s.ready = false); // relaunching
        let during = consumer.footprint();
        assert_eq!(during.len(), 1, "must NOT go silent while bytes are resident");
        assert_eq!(during[0].bytes, held);
        assert!(during[0].detail.contains("loading or relaunching"));
    }

    // what this catches: the opposite failure — never decaying, which strands capacity
    // forever. A FRESH readiness verification is proof the previous process is gone, so
    // the claim must collapse to the live plan. Evidence, not a timer.
    #[tokio::test]
    async fn a_new_readiness_verification_releases_the_high_water() {
        let (consumer, serving_tx) = shape_rig(6_000_000_000);
        let wide = consumer.footprint()[0].bytes;

        serving_tx.send_modify(|s| s.lanes = 1);
        assert_eq!(consumer.footprint()[0].bytes, wide, "no proof yet");

        // The new 1-lane process verifies ready → the old one is gone.
        serving_tx.send_modify(|s| s.ready_verified_at_ms = Some(2_000));
        let after = consumer.footprint()[0].bytes;
        assert!(
            after < wide,
            "a confirmed relaunch must release the claim, or capacity is stranded forever"
        );
    }

    #[tokio::test]
    async fn footprint_is_the_active_ready_model_only() {
        let (consumer, serving_tx) = rig("qwen3-coder-30b", 18_000);
        let fp = consumer.footprint();
        assert_eq!(fp.len(), 1);
        assert_eq!(fp[0].kind, ResourceKind::Vram);
        assert_eq!(fp[0].bytes, 18_000);

        // not-ready (a load still coming up) → holds nothing reclaimable yet
        serving_tx.send_modify(|s| s.ready = false);
        assert!(consumer.footprint().is_empty());

        // nothing loaded → empty
        serving_tx.send_modify(|s| {
            s.ready = true;
            s.active_model = None;
        });
        assert!(consumer.footprint().is_empty());
    }

    // what this catches: the served per-slot window AND lane count from the live
    // snapshot reach the FootprintFn, so serving's residency is charged as
    // weights + lanes × KV(window) — not weights alone (the pre-#79 under-report
    // that let serving's own KV masquerade as external contention on the board).
    // If active_ready() dropped either the window or the lanes, the resident
    // estimate would collapse back to weights-only and this fails.
    #[tokio::test]
    async fn footprint_passes_served_window_and_lanes_to_resolver() {
        let seen: Arc<Mutex<Option<(String, u32, u32)>>> = Arc::new(Mutex::new(None));
        let seen_w = seen.clone();
        let (serving_tx, serving_rx) = watch::channel(ServingSnapshot {
            loading_model: None,
            held_for: None,
            // test fixture: no live readiness was ever CONFIRMED here.
            ready_verified_at_ms: None,
            active_model: Some("coder-14b".into()),
            ready: true,
            base_url: "http://localhost:0/v1".into(),
            adapters: Vec::new(),
            served_context_window: 11008,
            lanes: 3,
            host_prompt_cache_mib: 0,
            degraded_reason: None,
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
        });
        let footprint_of: FootprintFn = Arc::new(move |id: &str, window: u32, lanes: u32, _grant: u32| {
            *seen_w.lock() = Some((id.to_string(), window, lanes));
            // weights(1000) + lanes × kv_per_token(10) × window
            1000 + lanes as u64 * 10 * window as u64
        });
        let intent = ServingIntent::new(None);
        let consumer = ServingConsumer::new(
            serving_rx,
            intent,
            footprint_of,
            ResourceKind::Vram,
            Arc::new(DeclineTierDown),
        )
            .with_inherited_lane(Arc::new(|| None))
            .without_measurement();

        let fp = consumer.footprint();
        assert_eq!(fp.len(), 1);
        assert_eq!(
            *seen.lock(),
            Some(("coder-14b".into(), 11008, 3)),
            "the snapshot's window + lane count must reach the resolver"
        );
        assert_eq!(
            fp[0].bytes,
            1000 + 3 * 10 * 11008,
            "resident folds per-lane KV"
        );
        assert!(fp[0].detail.contains("3 lane(s) × 11008 ctx"));

        // A re-home to a single lane at a smaller window shrinks the charged KV.
        serving_tx.send_modify(|s| {
            s.lanes = 1;
            s.served_context_window = 4096;
        });
        let fp = consumer.footprint();
        assert_eq!(*seen.lock(), Some(("coder-14b".into(), 4096, 1)));
        assert_eq!(fp[0].bytes, 1000 + 1 * 10 * 4096);
    }

    // what this catches: the honest Deferred→Released handshake across an async
    // unload. Serving must NOT claim bytes freed on the first ask (the VRAM is
    // still resident until the reconcile lands) — claiming early is exactly the
    // over-grant OOM this whole task exists to prevent. It suppresses, defers,
    // and only reports Released — with the real footprint — once the snapshot
    // confirms the model is gone.
    #[tokio::test]
    async fn suppress_then_defer_then_release_when_unload_lands() {
        let (consumer, serving_tx) = rig("qwen3-coder-30b", 18_000);

        // First ask: suppresses + defers, frees nothing yet.
        let first = consumer.reclaim(ask()).await;
        assert_eq!(first.status, ReclaimStatus::Deferred);
        assert_eq!(first.freed_bytes, 0);
        assert!(consumer
            .intent
            .snapshot()
            .suppressed
            .contains("qwen3-coder-30b"));
        let revision = consumer.intent.snapshot().revision;
        consumer.suppress_model("qwen3-coder-30b");
        assert_eq!(
            consumer.intent.snapshot().revision,
            revision,
            "pressure suppression retry is a no-op"
        );

        // Re-ask while still resident (reconcile not done): still deferred.
        let second = consumer.reclaim(ask()).await;
        assert_eq!(second.status, ReclaimStatus::Deferred);
        assert_eq!(second.freed_bytes, 0);

        // The daemon's reconcile unloads it → snapshot clears.
        serving_tx.send_modify(|s| s.active_model = None);

        // Now the ask resolves Released with the exact footprint.
        let third = consumer.reclaim(ask()).await;
        assert_eq!(third.status, ReclaimStatus::Released);
        assert_eq!(third.freed_bytes, 18_000);

        // And it's no longer pending — a further ask reports nothing left.
        let fourth = consumer.reclaim(ask()).await;
        assert_eq!(fourth.status, ReclaimStatus::Released);
        assert_eq!(fourth.freed_bytes, 0);
    }

    // Build a consumer with an injected tier-down policy, exposing the pin
    // receiver so the test can watch the re-home target the consumer sets. Plays
    // the daemon's role by hand — no llama-server, no catalog.
    fn tier_down_rig(
        active: &str,
        current: u64,
        policy: Arc<dyn TierDownPolicy>,
    ) -> (
        ServingConsumer,
        watch::Sender<ServingSnapshot>,
        watch::Receiver<ServingIntentSnapshot>,
    ) {
        let (serving_tx, serving_rx) = watch::channel(ServingSnapshot {
            loading_model: None,
            held_for: None,
            // test fixture: no live readiness was ever CONFIRMED here.
            ready_verified_at_ms: None,
            active_model: Some(active.to_string()),
            ready: true,
            base_url: "http://localhost:0/v1".into(),
            adapters: Vec::new(),
            served_context_window: 11008,
            lanes: 4,
            host_prompt_cache_mib: 0,
            degraded_reason: None,
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
        });
        let intent = ServingIntent::new(None);
        let pin_rx = intent.subscribe();
        let footprint_of: FootprintFn = Arc::new(move |_id: &str, _w: u32, _l: u32, _grant: u32| current);
        let consumer =
            ServingConsumer::new(serving_rx, intent, footprint_of, ResourceKind::Vram, policy)
                .with_inherited_lane(Arc::new(|| None))
            .without_measurement();
        (consumer, serving_tx, pin_rx)
    }

    // what this catches: THE tier-down lever (#79) — under Pressure, when a policy
    // chooses to re-home to a smaller base, serving PINS that model (does not go
    // dark via suppress), defers across the async swap, and reports Partial with
    // the freed DELTA (old − new) once the snapshot shows the smaller model
    // active. Reporting Released, or the whole footprint, or suppressing instead
    // of pinning would each be a lie: serving still holds the smaller model.
    #[tokio::test]
    async fn pressure_tier_down_pins_smaller_then_reports_partial() {
        // active holds 18_000; policy re-homes to "coder-7b" resident at 6_000 →
        // freed delta = 12_000, and serving keeps serving the 6_000 model.
        let policy = Arc::new(AlwaysTierDown {
            target: "coder-7b".into(),
            resident_after: 6_000,
        });
        let (consumer, serving_tx, pin_rx) = tier_down_rig("coder-30b", 18_000, policy);

        // First ask under Pressure: pins the smaller model, defers, frees nothing
        // yet (the swap is async), and does NOT suppress (never goes dark).
        let first = consumer.reclaim(ask()).await;
        assert_eq!(first.status, ReclaimStatus::Deferred);
        assert_eq!(first.freed_bytes, 0);
        assert_eq!(
            pin_rx.borrow().pinned.as_deref(),
            Some("coder-7b"),
            "re-home pinned"
        );
        let revision = pin_rx.borrow().revision;
        consumer.pin_model("coder-7b");
        assert_eq!(
            pin_rx.borrow().revision,
            revision,
            "pressure pin retry is a no-op"
        );
        assert!(
            !consumer.intent.snapshot().suppressed.contains("coder-30b"),
            "tier-down pins, never suppresses — serving must not go dark"
        );

        // Re-ask while the old model is still active (reconcile not done): deferred.
        let second = consumer.reclaim(ask()).await;
        assert_eq!(second.status, ReclaimStatus::Deferred);

        // The daemon's reconcile swaps to the smaller model → snapshot shows it.
        serving_tx.send_modify(|s| s.active_model = Some("coder-7b".into()));

        // Now the ask resolves Partial with the freed DELTA (18_000 − 6_000).
        let third = consumer.reclaim(ask()).await;
        assert_eq!(third.status, ReclaimStatus::Partial);
        assert_eq!(third.freed_bytes, 12_000);

        // Cleared — a further ask starts fresh (would tier down the 7b next time).
        let fourth = consumer.reclaim(ask()).await;
        assert_eq!(
            fourth.status,
            ReclaimStatus::Deferred,
            "new cycle, not stuck"
        );
    }

    // what this catches: reason gating. Shutdown wants EVERYTHING gone and
    // Rebalance is moving the lease OFF this box — a smaller model here serves
    // neither, so the tier-down policy is not even consulted; both go straight to
    // the full-unload (suppress) path. If tier-down fired on Shutdown, serving
    // would keep holding a smaller model when the authority needs it fully gone.
    #[tokio::test]
    async fn shutdown_and_rebalance_skip_tier_down_and_fully_unload() {
        for reason in [ReclaimReason::Shutdown, ReclaimReason::Rebalance] {
            let policy = Arc::new(AlwaysTierDown {
                target: "coder-7b".into(),
                resident_after: 6_000,
            });
            let (consumer, _tx, pin_rx) = tier_down_rig("coder-30b", 18_000, policy);
            let out = consumer
                .reclaim(ReclaimRequest {
                    kind: ResourceKind::Vram,
                    target_bytes: 8_000,
                    deadline_ms: 1_000,
                    reason,
                })
                .await;
            assert_eq!(out.status, ReclaimStatus::Deferred);
            assert!(
                pin_rx.borrow().pinned.is_none(),
                "{reason:?} must not pin/tier-down"
            );
            assert!(
                consumer.intent.snapshot().suppressed.contains("coder-30b"),
                "{reason:?} suppresses for a full unload"
            );
        }
    }

    // what this catches: the phantom-guard. A policy that proposes a NON-shrinking
    // target (resident_after >= current, or an empty id) is ignored and the
    // consumer falls through to the honest full unload — it never carries out a
    // lateral/up "tier-down" that would free nothing while claiming it did.
    #[tokio::test]
    async fn non_shrinking_tier_down_is_ignored_and_falls_through_to_unload() {
        // resident_after (20_000) > current (18_000): not a shrink.
        let policy = Arc::new(AlwaysTierDown {
            target: "bigger-not-smaller".into(),
            resident_after: 20_000,
        });
        let (consumer, _tx, pin_rx) = tier_down_rig("coder-30b", 18_000, policy);
        let out = consumer.reclaim(ask()).await;
        assert_eq!(out.status, ReclaimStatus::Deferred);
        assert!(
            pin_rx.borrow().pinned.is_none(),
            "non-shrink proposal must not pin"
        );
        assert!(
            consumer.intent.snapshot().suppressed.contains("coder-30b"),
            "falls through to full unload"
        );
    }

    // what this catches: an ask for a kind serving doesn't hold is refused with a
    // named reason, never a silent freed=0 the authority could misread as "freed
    // nothing, all fine" and stop asking the consumer that actually holds it.
    #[tokio::test]
    async fn non_vram_ask_is_refused_and_named() {
        let (consumer, _tx) = rig("m", 1_000);
        let out = consumer
            .reclaim(ReclaimRequest {
                kind: ResourceKind::Ram,
                target_bytes: 1_000,
                deadline_ms: 100,
                reason: ReclaimReason::Pressure,
            })
            .await;
        assert_eq!(out.status, ReclaimStatus::Refused);
        assert_eq!(out.freed_bytes, 0);
        assert!(out.detail.unwrap().contains("Ram"));
    }

    fn inherited_rec(window: u32, lanes: u32) -> LaneRecord {
        LaneRecord {
            pid: 64372,
            port: 58057,
            role: crate::inference::lane_registry::LaneRole::Live,
            model: "ggml-org/Qwen3.8-27B-GGUF".into(),
            context_window: window,
            lanes,
            page_dir: None,
            engine_bin: None,
            weights: None,
            started_s: 0,
            placement: None,
        }
    }

    // what this catches: THE 2026-08-19 GOVERNOR DEFECT. A lane inherited from a crashed
    // predecessor holds real bytes, and if the consumer reports nothing for it the
    // governor files those bytes under `external` — someone else's. Measured live: that
    // read as usable_gb=2 on a 64 GB box, so the planner chose a 3B and reaped a healthy
    // 27B to make room for it, TWICE in one boot. A past form of ourself must be counted
    // as OURS, sized from its own custody record, with a reason that says where it came
    // from.
    #[test]
    fn a_lane_inherited_from_a_previous_generation_is_counted_as_ours() {
        let rec = inherited_rec(22_784, 4);
        let (bytes, why) = inherited_footprint(&rec, |_m, w, l| u64::from(w) * u64::from(l))
            .expect("a fully-described inherited lane must be sizable");
        assert_eq!(bytes, 22_784 * 4, "sized from the record's OWN shape");
        assert!(
            why.contains("inherited") && why.contains("64372"),
            "the reason must name the past form it came from, got {why:?}"
        );
    }

    // what this catches: the refusal half, which is the one that keeps this honest. A
    // record written before the shape fields existed carries 0 — that is UNKNOWN, not a
    // zero-byte lane. Regression here (returning Some(0)) hands the governor a QUANTITY
    // where it should get an absence, which is the same fabricated-zero class as #438's
    // usable_gb=0 and the sidecar's two-zeros bug. Refuse instead
    // ([[an-absence-is-an-unfinished-measurement]]).
    #[test]
    fn an_old_record_without_a_shape_is_unknown_and_never_a_zero_byte_lane() {
        for rec in [inherited_rec(0, 4), inherited_rec(22_784, 0), inherited_rec(0, 0)] {
            assert!(
                inherited_footprint(&rec, |_m, _w, _l| 999).is_none(),
                "a record missing its shape must refuse to be sized, never report 0"
            );
        }
    }
