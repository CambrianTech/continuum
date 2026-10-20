    use super::*;
    use crate::resources::arbiter::TieredArbiter;

    mod self_eviction {
        use super::*;
        use crate::resources::consumer::ConsumerFootprint;

        /// The M5, 2026-08-19, as `resources/board` reported it while the planner was
        /// choosing what to serve. Real numbers — this test IS the incident.
        const CAPACITY: u64 = 55_125_917_696; // 51.3 GiB vram ceiling
        const SERVING_RESIDENT: u64 = 30_253_811_702; // 28.2 GiB of Qwen3.8-27B weights+KV
        const A_27B_NEEDS: u64 = 30_000_000_000; // roughly what it was already holding

        fn ledger_serving_a_27b() -> ResourceLeaseLedger {
            let mut l = ResourceLeaseLedger::new();
            l.set_capacity(ResourceKind::Vram, CAPACITY);
            // The board's physical_used included serving's own weights, as it must —
            // they ARE resident.
            l.set_physical_used(ResourceKind::Vram, 38_171_508_736);
            l.set_measured(
                "serving",
                vec![ConsumerFootprint {
                    kind: ResourceKind::Vram,
                    bytes: SERVING_RESIDENT,
                    detail: "Qwen3.8-27B weights+KV resident".into(),
                }],
            );
            l
        }

        // what this catches: THE RATCHET. A machine already serving model X must always
        // be able to re-plan model X. Before `budget_for_replacing`, serving was handed
        // `available_for` — which subtracts serving's own 28 GB from serving's own budget
        // — so a 27B did not "fit" on a box that was RUNNING IT. The planner stepped down
        // to a 7B, then a 0.5B, and could never climb back, because each smaller incumbent
        // still ate its own successor's headroom. That is #438 and #214, one defect.
        #[test]
        fn a_machine_serving_a_27b_can_still_plan_a_27b() {
            let l = ledger_serving_a_27b();

            let circular = l.available_for("serving", ResourceKind::Vram);
            assert!(
                circular < A_27B_NEEDS,
                "precondition: the OLD number must be too small for the model already \
                 running — that is the bug being pinned ({circular} >= {A_27B_NEEDS})"
            );

            let honest = l.budget_for_replacing("serving", ResourceKind::Vram);
            assert!(
                honest >= A_27B_NEEDS,
                "a machine running a 27B must be able to choose a 27B: budget {honest} < {A_27B_NEEDS}"
            );
            assert_eq!(honest, circular + SERVING_RESIDENT);
        }

        // what this catches (the 5090, 2026-09-17): a declared footprint LARGER than the
        // device. Serving declared 67.5 GB (`peak_resident_bytes` at a 144k window, f16
        // KV) on a 33.6 GB card; available fell to 2.3 GB; the add-back produced a 69.8 GB
        // budget the planner read as `usable_gb=54` and sized a window against. The
        // replace-myself budget is capped at capacity — the device is the ceiling of
        // every number this ledger speaks.
        #[test]
        fn the_replace_myself_budget_never_exceeds_the_device() {
            const CARD: u64 = 33_654_046_720; // the 5090 as the board reports it
            const DECLARED: u64 = 67_555_372_534; // serving's own over-estimate
            let mut l = ResourceLeaseLedger::new();
            l.set_capacity(ResourceKind::Vram, CARD);
            l.set_physical_used(ResourceKind::Vram, 31_372_345_344);
            l.set_measured(
                "serving",
                vec![ConsumerFootprint {
                    kind: ResourceKind::Vram,
                    bytes: DECLARED,
                    detail: "27B at 144640 ctx, f16 KV estimate".into(),
                }],
            );
            let budget = l.budget_for_replacing("serving", ResourceKind::Vram);
            assert_eq!(budget, CARD, "capped at the device, not the declaration");
            assert!(
                l.available_for("serving", ResourceKind::Vram) + DECLARED > CARD,
                "precondition: the uncapped sum really does exceed the card"
            );
        }

        // what this catches: the add-back leaking into OTHER consumers' budgets. Serving
        // releasing its weights is part of serving's swap; it is not headroom for vision or
        // staging, and granting it to them would over-commit the pool.
        #[test]
        fn the_add_back_is_only_ever_the_callers_own_residency() {
            let l = ledger_serving_a_27b();
            assert_eq!(
                l.budget_for_replacing("vision", ResourceKind::Vram),
                l.available_for("vision", ResourceKind::Vram),
                "a peer must see NO add-back from serving's residency"
            );
        }

        // what this catches: someone reaching for the replace-myself budget on the ACQUIRE
        // path. A lease granted against bytes not yet released is a real over-commit — the
        // add-back is only valid for a DECISION about what to become next.
        #[test]
        fn acquire_still_refuses_against_bytes_not_yet_released() {
            let mut l = ledger_serving_a_27b();
            let honest = l.budget_for_replacing("serving", ResourceKind::Vram);
            let err = l.acquire(
                &req("serving", ResourceKind::Vram, honest, ReclaimPolicy::Graceful),
                "lease-1".into(),
                0,
            );
            assert!(
                err.is_err(),
                "acquire must still check available_for — the replace-myself budget is a \
                 planning number, never a grantable one"
            );
        }
    }

    fn req(consumer: &str, kind: ResourceKind, bytes: u64, policy: ReclaimPolicy) -> LeaseRequest {
        LeaseRequest {
            consumer_id: consumer.into(),
            kind,
            bytes,
            ttl_ms: 1_000,
            reclaim_policy: policy,
        }
    }

    // what this catches: THE task-#56 OOM guard. Capacity 10GB, bevy already
    // holds 4GB pinned, serving asks for 8GB → must be refused against the 6GB
    // actually free, NOT granted against the 10GB total. The old serving
    // host_budget() bug over-committed exactly here and OOM'd the box.
    #[test]
    fn acquire_refuses_over_commit_against_what_others_hold() {
        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(ResourceKind::Vram, 10_000);
        ledger
            .acquire(
                &req("bevy", ResourceKind::Vram, 4_000, ReclaimPolicy::Pinned),
                "bevy-1".into(),
                100,
            )
            .expect("bevy fits in 10GB");

        let err = ledger
            .acquire(
                &req(
                    "serving",
                    ResourceKind::Vram,
                    8_000,
                    ReclaimPolicy::Graceful,
                ),
                "serving-1".into(),
                100,
            )
            .expect_err("8GB must not fit alongside bevy's 4GB");
        assert_eq!(
            err,
            LeaseError::InsufficientCapacity {
                kind: ResourceKind::Vram,
                requested: 8_000,
                available: 6_000,
            }
        );
        // A request that DOES fit the 6GB headroom succeeds.
        ledger
            .acquire(
                &req(
                    "serving",
                    ResourceKind::Vram,
                    6_000,
                    ReclaimPolicy::Graceful,
                ),
                "serving-2".into(),
                100,
            )
            .expect("6GB fits the remaining headroom exactly");
        assert_eq!(ledger.available(ResourceKind::Vram), 0);
    }

    // what this catches: expiry must not silently free bytes. An expired lease
    // still counts against granted until release — otherwise the ledger would
    // hand out bytes the holder hasn't actually given back yet (double-grant →
    // OOM the moment the holder is slow to clean up).
    #[test]
    fn expired_lease_still_counts_as_held_until_released() {
        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(ResourceKind::Ram, 1_000);
        ledger
            .acquire(
                &req("livekit", ResourceKind::Ram, 1_000, ReclaimPolicy::Graceful),
                "lk-1".into(),
                0,
            )
            .expect("fits");
        // ttl 1_000 → expired at 2_000
        assert_eq!(ledger.granted(ResourceKind::Ram), 1_000);
        assert_eq!(ledger.expire(2_000).len(), 1);
        assert_eq!(
            ledger.granted(ResourceKind::Ram),
            1_000,
            "still held until released"
        );
        assert_eq!(ledger.available(ResourceKind::Ram), 0);
        ledger.release("lk-1").expect("release frees it");
        assert_eq!(ledger.available(ResourceKind::Ram), 1_000);
    }

    // what this catches: reclaim victim selection — safest-first ordering and
    // the fail-loud refusal. With only a pinned active lease covering the
    // target, select must return None (escalate, don't yank pinned), but an
    // expired lease of any policy is fair game.
    #[test]
    fn select_to_reclaim_orders_safest_first_and_refuses_impossible() {
        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(ResourceKind::Vram, 10_000);
        // pinned, active, 5GB — never reclaimable while live
        ledger
            .acquire(
                &req("bevy", ResourceKind::Vram, 5_000, ReclaimPolicy::Pinned),
                "pin".into(),
                100,
            )
            .unwrap();
        // graceful, active, 2GB, acquired later
        ledger
            .acquire(
                &req(
                    "serving",
                    ResourceKind::Vram,
                    2_000,
                    ReclaimPolicy::Graceful,
                ),
                "grace".into(),
                200,
            )
            .unwrap();
        // hard, active, 2GB
        ledger
            .acquire(
                &req("livekit", ResourceKind::Vram, 2_000, ReclaimPolicy::Hard),
                "hard".into(),
                150,
            )
            .unwrap();

        // Need 3GB: Hard (rank 1) chosen before Graceful (rank 2); 2GB Hard
        // then 2GB Graceful = 4GB ≥ 3GB. dwell 0 → no dwell protection.
        let arbiter = TieredArbiter::default();
        let picks = ledger
            .select_to_reclaim(ResourceKind::Vram, 3_000, 1_000, 0, &arbiter, 0.0)
            .expect("3GB reachable without the pinned lease");
        assert_eq!(
            picks.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
            vec!["hard", "grace"]
        );

        // Need 6GB: only 4GB is non-pinned and active → impossible without
        // touching the pinned lease → None (escalate, don't yank).
        assert!(ledger
            .select_to_reclaim(ResourceKind::Vram, 6_000, 1_000, 0, &arbiter, 0.0)
            .is_none());
    }

    // what this catches: min-dwell hysteresis — the anti-thrash mechanism. A
    // lease granted moments ago must NOT be eligible for reclaim within the
    // dwell window, so the authority can't page it in and rip it back out a
    // tick later. Expired leases must bypass dwell (their bytes are overdue).
    #[test]
    fn min_dwell_protects_fresh_leases_from_reclaim_thrash() {
        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(ResourceKind::Vram, 10_000);
        // granted at t=1000, active (ttl 1000 → expires 2000)
        ledger
            .acquire(
                &req(
                    "serving",
                    ResourceKind::Vram,
                    4_000,
                    ReclaimPolicy::Graceful,
                ),
                "fresh".into(),
                1_000,
            )
            .unwrap();

        let arbiter = TieredArbiter::default();
        // At t=1200 (held 200ms) with a 500ms dwell → protected → can't free it.
        assert!(ledger
            .select_to_reclaim(ResourceKind::Vram, 4_000, 1_200, 500, &arbiter, 0.0)
            .is_none());
        // At t=1600 (held 600ms ≥ 500ms dwell) → now reclaimable.
        let picks = ledger
            .select_to_reclaim(ResourceKind::Vram, 4_000, 1_600, 500, &arbiter, 0.0)
            .expect("past dwell → eligible");
        assert_eq!(
            picks.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
            vec!["fresh"]
        );
        // Once expired (t=2100), dwell no longer shields it even within a window.
        let picks = ledger
            .select_to_reclaim(ResourceKind::Vram, 4_000, 2_100, 10_000, &arbiter, 0.0)
            .expect("expired bypasses dwell");
        assert_eq!(picks.len(), 1);
    }

    // what this catches: the real-time fairness guarantee. A live video call
    // reserves a floor; inference can neither acquire into that floor (even when
    // the bytes are physically idle) nor reclaim the call below it. This is the
    // "always get some time" mechanism — the video call is never starved.
    // what this catches (Codex on the M5 deploy reserve, 2026-09-27): a reserved floor
    // counted on top of its holder's own measured residency. The holder's resident bytes are
    // already in physical_used; the floor withholds only what is neither granted nor held.
    // No grant: the whole floor, then shrinking as it becomes resident. A partial grant: the
    // larger of grant and residency. Residency past the floor: nothing more withheld.
    #[test]
    fn a_reserved_floor_withholds_only_what_its_holder_does_not_already_hold() {
        let gib = 1u64 << 30;
        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(ResourceKind::Ram, 64 * gib);
        ledger.set_physical_used(ResourceKind::Ram, 40 * gib);
        ledger.reserve("deploy-build", ResourceKind::Ram, 10 * gib + gib / 2);
        let foot = |bytes| vec![ConsumerFootprint { kind: ResourceKind::Ram, bytes, detail: "rustc".into() }];
        // idle: nothing resident, the whole floor is withheld from serving
        assert_eq!(ledger.available_for("serving", ResourceKind::Ram), 24 * gib - (10 * gib + gib / 2));
        // building, 6 GiB resident (already inside physical_used): serving loses 10.5 in all, not 16.5
        ledger.set_physical_used(ResourceKind::Ram, 46 * gib);
        ledger.set_measured("deploy-build", foot(6 * gib));
        assert_eq!(ledger.available_for("serving", ResourceKind::Ram), 18 * gib - (4 * gib + gib / 2));
        assert_eq!(64 * gib - 40 * gib - ledger.available_for("serving", ResourceKind::Ram), 10 * gib + gib / 2, "the build costs its floor, once");
        // resident past its floor: nothing more withheld; physical_used carries it
        ledger.set_physical_used(ResourceKind::Ram, 52 * gib);
        ledger.set_measured("deploy-build", foot(12 * gib));
        assert_eq!(ledger.available_for("serving", ResourceKind::Ram), 12 * gib);
        // a partial grant below its residency: the larger of the two meets the floor
        ledger.set_measured("deploy-build", foot(6 * gib));
        ledger.set_physical_used(ResourceKind::Ram, 46 * gib);
        ledger
            .acquire(&req("deploy-build", ResourceKind::Ram, 3 * gib, ReclaimPolicy::Graceful), "b".into(), 100)
            .unwrap();
        assert_eq!(ledger.available_for("serving", ResourceKind::Ram), 18 * gib - (4 * gib + gib / 2));
    }

    #[test]
    fn reservation_floor_blocks_grant_and_reclaim_below_it() {
        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(ResourceKind::Vram, 10_000);
        // The live call reserves 6GB and currently holds 4GB of it.
        ledger.reserve("livekit", ResourceKind::Vram, 6_000);
        ledger
            .acquire(
                &req(
                    "livekit",
                    ResourceKind::Vram,
                    4_000,
                    ReclaimPolicy::Graceful,
                ),
                "call".into(),
                100,
            )
            .unwrap();

        // Physical free = 6GB, but 2GB of it is livekit's unmet reservation →
        // inference may only take 4GB, not 6GB.
        assert_eq!(ledger.available(ResourceKind::Vram), 6_000);
        assert_eq!(ledger.available_for("serving", ResourceKind::Vram), 4_000);
        let err = ledger
            .acquire(
                &req(
                    "serving",
                    ResourceKind::Vram,
                    6_000,
                    ReclaimPolicy::Graceful,
                ),
                "infer".into(),
                100,
            )
            .expect_err("can't eat the call's floor");
        assert_eq!(
            err,
            LeaseError::InsufficientCapacity {
                kind: ResourceKind::Vram,
                requested: 6_000,
                available: 4_000,
            }
        );
        // livekit itself CAN draw its own reservation up to the floor.
        ledger
            .acquire(
                &req(
                    "livekit",
                    ResourceKind::Vram,
                    2_000,
                    ReclaimPolicy::Graceful,
                ),
                "call-2".into(),
                100,
            )
            .expect("reserved consumer reaches its own floor");

        // Now reclaim must never drop livekit below 6GB. It holds exactly 6GB
        // across two leases → none can be taken.
        assert!(ledger
            .select_to_reclaim(
                ResourceKind::Vram,
                2_000,
                1_000,
                0,
                &TieredArbiter::default(),
                0.0
            )
            .is_none());
    }

    // what this catches: renew extends a live lease but rejects an expired one
    // (the holder must re-acquire and re-pass the capacity check). A renew that
    // silently revived an expired lease would skip the over-commit guard.
    #[test]
    fn renew_extends_active_rejects_expired() {
        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(ResourceKind::Disk, 1_000);
        ledger
            .acquire(
                &req("serving", ResourceKind::Disk, 500, ReclaimPolicy::Graceful),
                "d1".into(),
                0,
            )
            .unwrap();
        ledger.renew("d1", 5_000, 500).expect("live lease renews");
        assert_eq!(ledger.lease("d1").unwrap().expires_at_ms, 5_000);
        // now past the new deadline
        let err = ledger
            .renew("d1", 9_000, 6_000)
            .expect_err("expired cannot renew");
        assert_eq!(
            err,
            LeaseError::ExpiredLease {
                lease_id: "d1".into()
            }
        );
        // unknown id is fail-loud, not a no-op
        assert_eq!(
            ledger.renew("ghost", 9_000, 100).expect_err("missing"),
            LeaseError::MissingLease {
                lease_id: "ghost".into()
            }
        );
    }

    // what this catches: the published board reports only live kinds and honest
    // arithmetic (available = capacity − granted). A board that listed empty
    // kinds or mismatched math would mislead the grid/command callers reading it.
    #[test]
    fn board_reports_only_live_kinds_with_honest_math() {
        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(ResourceKind::Vram, 8_000);
        // Ram and Disk untouched and uncapacitied → omitted from the board.
        ledger
            .acquire(
                &req(
                    "serving",
                    ResourceKind::Vram,
                    3_000,
                    ReclaimPolicy::Graceful,
                ),
                "s1".into(),
                0,
            )
            .unwrap();
        let board = ledger.board();
        assert_eq!(board.kinds.len(), 1, "only vram is live");
        let vram = &board.kinds[0];
        assert_eq!(vram.kind, ResourceKind::Vram);
        assert_eq!(vram.capacity_bytes, 8_000);
        assert_eq!(vram.granted_bytes, 3_000);
        assert_eq!(vram.available_bytes, 5_000);
        assert_eq!(vram.lease_count, 1);
        assert_eq!(board.leases.len(), 1);
    }

    // what this catches: the measurement axis is REPORTING-ONLY and never
    // corrupts the available math. A consumer that holds 18GB resident with NO
    // lease (serving's real posture) must surface on the board as measured_bytes
    // + an attribution — fixing the "granted:0 while the GPU is full" blindness —
    // while `available` stays capacity − granted (unchanged by the measurement).
    // If measurement ever leaked into `available`, this test's available_bytes
    // would drop and the honest free-based global remainder would be wrong.
    #[test]
    fn measured_footprint_surfaces_on_board_without_touching_available() {
        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(ResourceKind::Vram, 24_000);
        // Serving holds 18GB resident but leased nothing — the un-inverted case.
        ledger.set_measured(
            "serving",
            vec![ConsumerFootprint {
                kind: ResourceKind::Vram,
                bytes: 18_000,
                detail: "qwen3-coder-30b weights resident".into(),
            }],
        );

        assert_eq!(ledger.measured(ResourceKind::Vram), 18_000);
        assert_eq!(ledger.measured_by("serving", ResourceKind::Vram), 18_000);
        // available is untouched: capacity − granted(0) = full 24GB, NOT
        // 24 − 18. Measurement reports; it does not reserve.
        assert_eq!(ledger.available(ResourceKind::Vram), 24_000);

        let board = ledger.board();
        let vram = board
            .kinds
            .iter()
            .find(|k| k.kind == ResourceKind::Vram)
            .unwrap();
        assert_eq!(vram.granted_bytes, 0, "no lease → nothing granted");
        assert_eq!(vram.measured_bytes, 18_000, "but 18GB measured-resident");
        assert_eq!(
            vram.available_bytes, 24_000,
            "available ignores measurement"
        );
        assert_eq!(board.attributions.len(), 1);
        assert_eq!(board.attributions[0].consumer_id, "serving");
        assert_eq!(board.attributions[0].bytes, 18_000);
        assert_eq!(board.attributions[0].kind, ResourceKind::Vram);

        // A fresh poll restates wholesale: a smaller model now resident.
        ledger.set_measured(
            "serving",
            vec![ConsumerFootprint {
                kind: ResourceKind::Vram,
                bytes: 4_000,
                detail: "qwen2.5-0.5b resident".into(),
            }],
        );
        assert_eq!(
            ledger.measured(ResourceKind::Vram),
            4_000,
            "restated, not summed"
        );

        // An empty poll clears it (serving unloaded everything) → measured axis
        // gone, and with no lease + no capacity-less kind, attribution empties.
        ledger.set_measured("serving", Vec::new());
        assert_eq!(ledger.measured(ResourceKind::Vram), 0);
        assert!(ledger.board().attributions.is_empty());
    }

    // what this catches: THE un-inversion (task #79). Capacity is the FIXED
    // ceiling; the honest commit number is `capacity − max(granted, physical_used)`.
    // Three regimes must hold with ONE formula:
    //   (a) physical_used < granted (a fresh lease not yet resident) → granted wins,
    //       so we don't double-hand-out the bytes we already promised;
    //   (b) physical_used > granted (external grab / unleased residency) → physical
    //       wins, so we refuse to commit bytes that are physically gone;
    //   (c) the external floor = physical_used − measured surfaces on the board as a
    //       grid-contention signal distinct from our own attribution.
    // If `available` ever went back to `capacity − granted`, regime (b) would
    // over-commit into a launching game's VRAM — the exact OOM the un-inversion
    // fixes — and a grid peer could not read this node's honest bearing.
    #[test]
    fn available_is_capacity_minus_max_of_granted_and_physical_used() {
        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(ResourceKind::Vram, 24_000);

        // Regime (a): we granted 10_000 but only 6_000 is resident so far (the
        // lease's allocation is still in flight). committed = max(10k, 6k) = 10k;
        // available = 24k − 10k = 14k — the grant is protected, not double-counted.
        ledger
            .acquire(
                &req(
                    "serving",
                    ResourceKind::Vram,
                    10_000,
                    ReclaimPolicy::Graceful,
                ),
                "lease-a".into(),
                0,
            )
            .unwrap();
        ledger.set_physical_used(ResourceKind::Vram, 6_000);
        assert_eq!(
            ledger.committed(ResourceKind::Vram),
            10_000,
            "granted wins while allocation lags"
        );
        assert_eq!(ledger.available(ResourceKind::Vram), 14_000);

        // Regime (b): a game grabs VRAM — physical_used jumps to 21_000 while our
        // grant is unchanged at 10_000. committed = max(10k, 21k) = 21k; available
        // = 24k − 21k = 3k. We now refuse to hand out the bytes the game took.
        ledger.set_physical_used(ResourceKind::Vram, 21_000);
        assert_eq!(
            ledger.committed(ResourceKind::Vram),
            21_000,
            "physical wins when external pressure exceeds grants"
        );
        assert_eq!(ledger.available(ResourceKind::Vram), 3_000);

        // Regime (c): attribute 8_000 of the residency to serving; the rest of
        // physical_used is external (the game). external = 21k − 8k = 13k.
        ledger.set_measured(
            "serving",
            vec![ConsumerFootprint {
                kind: ResourceKind::Vram,
                bytes: 8_000,
                detail: "weights resident".into(),
            }],
        );
        let board = ledger.board();
        let vram = board
            .kinds
            .iter()
            .find(|k| k.kind == ResourceKind::Vram)
            .unwrap();
        assert_eq!(
            vram.capacity_bytes, 24_000,
            "ceiling is fixed — the grab did not move it"
        );
        assert_eq!(vram.physical_used_bytes, 21_000);
        assert_eq!(vram.measured_bytes, 8_000);
        assert_eq!(
            vram.external_bytes, 13_000,
            "physical − measured = the game's floor"
        );
        assert_eq!(
            vram.available_bytes, 3_000,
            "board available == committed math"
        );
    }

    // what this catches: the mechanism/policy split is real — swapping the
    // arbiter changes WHICH safe victim is taken first, but can never breach a
    // safety bound (pinned-active stays excluded, the reservation floor still
    // holds). Here a deliberately inverted policy that prefers consumer "b"
    // reclaims b's lease before a's, the opposite of the default; if the ledger
    // had baked the ordering in, this swap would have no effect.
    #[test]
    fn arbiter_swap_reorders_victims_without_breaching_safety() {
        struct PrefersB;
        impl LeaseArbiter for PrefersB {
            fn name(&self) -> &str {
                "prefers-b"
            }
            fn reclaim_score(&self, lease: &ResourceLease, _ctx: &ArbiterContext) -> f64 {
                // Higher = reclaimed first. Take consumer "b" before anyone.
                if lease.consumer_id == "b" {
                    100.0
                } else {
                    1.0
                }
            }
            fn demand_urgency(
                &self,
                _req: &LeaseRequest,
                _waited_ms: u64,
                _ctx: &ArbiterContext,
            ) -> f64 {
                0.0
            }
        }

        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(ResourceKind::Vram, 10_000);
        ledger
            .acquire(
                &req("a", ResourceKind::Vram, 2_000, ReclaimPolicy::Graceful),
                "a1".into(),
                100,
            )
            .unwrap();
        ledger
            .acquire(
                &req("b", ResourceKind::Vram, 2_000, ReclaimPolicy::Graceful),
                "b1".into(),
                50,
            )
            .unwrap();
        // pinned-active must stay off-limits no matter what the policy says
        ledger
            .acquire(
                &req("c", ResourceKind::Vram, 5_000, ReclaimPolicy::Pinned),
                "c1".into(),
                10,
            )
            .unwrap();

        // Default (LRU within tier): b1 acquired earlier → taken first for 2GB.
        let default_pick = ledger
            .select_to_reclaim(
                ResourceKind::Vram,
                2_000,
                1_000,
                0,
                &TieredArbiter::default(),
                0.0,
            )
            .unwrap();
        assert_eq!(
            default_pick
                .iter()
                .map(|(id, _)| id.as_str())
                .collect::<Vec<_>>(),
            vec!["b1"]
        );

        // Inverted policy still picks b first here (consumer match), but proves
        // the SCORE drives selection: need 4GB → both graceful taken, never the
        // pinned "c1" (eligibility is the ledger's, unaffected by policy).
        let swapped = ledger
            .select_to_reclaim(ResourceKind::Vram, 4_000, 1_000, 0, &PrefersB, 0.0)
            .unwrap();
        let ids: Vec<&str> = swapped.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["b1", "a1"],
            "b scored higher → reclaimed first; pinned excluded"
        );
        assert!(
            !ids.contains(&"c1"),
            "active pinned never eligible regardless of policy"
        );
    }
