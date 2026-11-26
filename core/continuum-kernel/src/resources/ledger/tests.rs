#![cfg(test)]
    use super::*;
    use crate::resources::arbiter::TieredArbiter;

    /// An observation instant for a scripted reading, and a reading observed then.
    fn obs(nanos: u64) -> crate::resources::Observed {
        crate::resources::Observed::from_nanos(nanos)
    }
    fn at(used: u64, nanos: u64) -> crate::resources::PhysicalSample {
        crate::resources::PhysicalSample { used, observed: obs(nanos) }
    }

    mod asks;
    mod self_eviction;

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
        // a new grant beside its residency is owed on top of it until measured (Codex on #6124:
        // a lease beside old residency is never hidden inside it), so serving loses its 3 GiB too
        ledger.set_measured("deploy-build", foot(6 * gib));
        ledger.set_physical_used(ResourceKind::Ram, 46 * gib);
        ledger
            .acquire(&req("deploy-build", ResourceKind::Ram, 3 * gib, ReclaimPolicy::Graceful), "b".into(), 100)
            .unwrap();
        assert_eq!(ledger.available_for("serving", ResourceKind::Ram), 18 * gib - (4 * gib + gib / 2) - 3 * gib);
    }

    // what this catches (the M5, 2026-11-14 03:35Z, canary 958ea3056): a boot sizing that takes the
    // embed lane's floor. At a gap the predecessor engine's bytes are still physically used and
    // serving's own are not attributed, so the replace-myself budget reads 0 (a pool sized from it
    // is the 2,048-token dark node); the room beside the others is computed from their claims and
    // never includes the embed floor, whether the embedder holds a lease, only its floor, or more.
    #[test]
    fn the_embed_floor_survives_a_boot_sizing() {
        let capacity = 55_662_788_608; // serving.vram_ceiling_prior.seeded on the M5
        let embed_floor = 186_908_672; // embed.vram.refused requested
        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(ResourceKind::Vram, capacity);
        ledger.set_physical_used(ResourceKind::Vram, capacity);
        ledger.reserve("embed", ResourceKind::Vram, embed_floor);
        assert_eq!(ledger.budget_for_replacing("serving", ResourceKind::Vram), 0, "the gap reading: nothing to size from");
        assert_eq!(ledger.room_beside_others("serving", ResourceKind::Vram), capacity - embed_floor, "computed: the floor stays out");
        ledger.set_measured("embed", vec![ConsumerFootprint { kind: ResourceKind::Vram, bytes: 2 * embed_floor, detail: "context".into() }]);
        assert_eq!(ledger.room_beside_others("serving", ResourceKind::Vram), capacity - 2 * embed_floor, "residency past the floor counts once");
        ledger.set_measured("serving", vec![ConsumerFootprint { kind: ResourceKind::Vram, bytes: capacity, detail: "engine".into() }]);
        assert_eq!(ledger.room_beside_others("serving", ResourceKind::Vram), capacity - 2 * embed_floor, "its own bytes never count against it");
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
        let view = ledger.lease_view("d1").expect("live grant");
        ledger.renew("d1", 5_000, 500).expect("live lease renews");
        assert_eq!(view.bytes_at(4_999), 500);
        assert_eq!(view.bytes_at(5_000), 0);
        ledger.shrink("d1", 100).expect("partial reclaim");
        assert_eq!(view.bytes_at(4_999), 400);
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
        ledger.release("d1").expect("holder released");
        assert_eq!(view.bytes_at(0), 0);
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

        // Regime (a): we granted 10_000 and 6_000 is physically used, whose we cannot tell. Until
        // the holder declares the lease landed and a later sample shows it, the whole lease is
        // owed on top: committed = 6k + 10k = 16k; available = 8k (Codex on #6124: growth beside a
        // lease is never read as the lease landing). Declared and sampled, it is carried by
        // physical use: committed = max(10k, 6k) = 10k; available = 14k, never counted twice.
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
        assert!(ledger.set_physical_sample(ResourceKind::Vram, at(6_000, 1)));
        assert_eq!(ledger.committed(ResourceKind::Vram), 16_000, "an undeclared lease is owed on top");
        ledger.landed("lease-a", obs(2)).unwrap();
        assert!(ledger.set_physical_sample(ResourceKind::Vram, at(6_000, 2)));
        assert_eq!(
            ledger.committed(ResourceKind::Vram),
            10_000,
            "granted wins while allocation lags"
        );
        assert_eq!(ledger.available(ResourceKind::Vram), 14_000);

        // Regime (b): a game grabs VRAM — physical_used jumps to 21_000 while our
        // grant is unchanged at 10_000. committed = max(10k, 21k) = 21k; available
        // = 24k − 21k = 3k. We now refuse to hand out the bytes the game took.
        assert!(ledger.set_physical_sample(ResourceKind::Vram, at(21_000, 3)));
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

    mod unified_backing {
        use super::*;
        use crate::resources::consumer::ConsumerFootprint;

        // The M5, 2026-11-18 01:15Z, as `resources/board` reported it after the deploy of 2606f3b41.
        const VRAM_CEILING: u64 = 55_662_788_608; // min(host - reserve, Metal working-set hint)
        const RAM_CEILING: u64 = 60_129_542_144; // host - reserve
        const HOST_USED: u64 = 59_533_099_008; // every process on the host, ours or not
        const ENGINE_RESIDENT: u64 = 40_814_580_336; // serving's measured weights + KV on the GPU
        const OPTIMIZER: u64 = 4_096_000; // the learning step refused 23 times in 15 minutes

        fn the_m5(backed: bool) -> ResourceLeaseLedger {
            let mut l = ResourceLeaseLedger::new();
            for (kind, ceiling) in [(ResourceKind::Vram, VRAM_CEILING), (ResourceKind::Ram, RAM_CEILING)] {
                l.set_capacity(kind, ceiling);
                l.set_physical_used(kind, HOST_USED);
            }
            l.set_measured(
                "serving",
                vec![ConsumerFootprint { kind: ResourceKind::Vram, bytes: ENGINE_RESIDENT, detail: "Qwen3.6-35B weights + KV".into() }],
            );
            if backed {
                l.set_backing(ResourceKind::Vram, Some(ResourceKind::Ram));
            }
            l
        }

        // what this catches (the M5, 2026-11-18): on unified memory every process on the host was
        // charged against the GPU's working-set ceiling (59.5 GB used against 55.66 GB), so the GPU
        // read 0 free and a 4 MB learning step was refused all night. Backed by the host pool, the
        // GPU is committed by OUR residency and bounded by the host's free bytes: the step fits,
        // and the host's own limit still holds.
        #[test]
        fn a_unified_gpu_is_charged_for_our_residency_and_bounded_by_the_host() {
            assert_eq!(the_m5(false).available(ResourceKind::Vram), 0, "fixture: the night's reading");
            let l = the_m5(true);
            let host_free = RAM_CEILING - HOST_USED;
            assert_eq!(l.available(ResourceKind::Vram), host_free, "bounded by the host pool, which is the tighter limit");
            assert!(l.available_for("learning:her", ResourceKind::Vram) >= OPTIMIZER, "her optimizer fits");
            assert_eq!(l.available(ResourceKind::Ram), host_free, "the host axis is unchanged");
        }

        // what this catches (Cormac on 38736c291): the ceiling charging the host's other processes on the
        // GPU's axis, the 2026-11-18 refusal again: a lesson sized within it would shrink toward the reply
        // alone on the M5. Backed, the GPU is bounded by its own capacity less its fixed bytes and by the
        // host pool's ceiling, which carries the host's other processes once. The weights split is a fixture.
        #[test]
        fn a_unified_gpus_ceiling_is_its_fixed_bytes_and_the_host_pools_room() {
            const WEIGHTS: u64 = 20_000_000_000;
            let mut l = the_m5(true);
            l.fix("serving", ResourceKind::Vram, WEIGHTS);
            let others = HOST_USED - ENGINE_RESIDENT;
            let pool = RAM_CEILING - WEIGHTS - others;
            assert_eq!(l.ceiling(ResourceKind::Ram), pool, "the host pool: its capacity less every fixed byte on it and what others hold");
            assert_eq!(l.ceiling(ResourceKind::Vram), (VRAM_CEILING - WEIGHTS).min(pool), "the GPU: never charged the host's processes on its own axis");
            assert!(l.ceiling(ResourceKind::Vram) > OPTIMIZER);
        }

        // what this catches: the backing letting the GPU spend bytes it does not have. Our own GPU
        // residency still bounds it, and a host grant leaves the GPU less, never the same bytes twice.
        #[test]
        fn the_backing_never_lets_the_shared_bytes_be_spent_twice() {
            let mut l = the_m5(true);
            l.set_physical_used(ResourceKind::Ram, 0);
            l.set_physical_used(ResourceKind::Vram, 0);
            assert_eq!(l.available(ResourceKind::Vram), VRAM_CEILING - ENGINE_RESIDENT, "the GPU's own ceiling binds when the host is free");
            l.set_measured(
                "serving",
                vec![ConsumerFootprint { kind: ResourceKind::Vram, bytes: VRAM_CEILING, detail: "full".into() }],
            );
            assert_eq!(l.available(ResourceKind::Vram), 0, "a GPU full of our residency has nothing to give");
            l.set_backing(ResourceKind::Vram, None);
            l.set_physical_used(ResourceKind::Vram, HOST_USED);
            assert_eq!(l.available(ResourceKind::Vram), 0, "a discrete device keeps capacity minus physical use");
        }

        // what this catches (Codex on #6124): the shared host bytes spent twice between samples. A
        // GPU grant that has not allocated yet was invisible to the host axis until the next scan
        // (and any grant was, while physical use exceeded the grants), so a host grant could take the
        // same bytes; reservations were withheld only on their own axis. Replayed on the 01:15Z board
        // with no sample between grants: every grant, on either axis, spends the shared headroom once;
        // shrink and release return exactly what they free; a floor on either axis binds both; the
        // sample that lands the bytes moves nothing.
        #[test]
        fn mixed_grants_spend_the_shared_headroom_once_before_the_next_sample() {
            const MB: u64 = 1_000_000;
            let mut l = the_m5(true);
            let free = RAM_CEILING - HOST_USED;
            let gpu = l.acquire(&req("learning:her", ResourceKind::Vram, 400 * MB, ReclaimPolicy::Graceful), "gpu".into(), 0).expect("test: fits");
            assert_eq!(l.available(ResourceKind::Ram), free - 400 * MB, "a GPU grant spends the host bytes now");
            assert_eq!(l.available(ResourceKind::Vram), free - 400 * MB);
            let over = free - 400 * MB + 1;
            assert!(l.acquire(&req("embed", ResourceKind::Ram, over, ReclaimPolicy::Graceful), "over".into(), 0).is_err(), "the host cannot hand out the GPU's bytes");
            l.acquire(&req("embed", ResourceKind::Ram, free - 400 * MB, ReclaimPolicy::Graceful), "ram".into(), 0).expect("test: the rest fits");
            assert_eq!(l.available(ResourceKind::Vram), 0, "a host grant spends the GPU's headroom too");
            assert!(l.acquire(&req("other", ResourceKind::Vram, 1, ReclaimPolicy::Graceful), "gpu2".into(), 0).is_err(), "spent once, on both axes");

            assert_eq!(l.shrink(&gpu.lease_id, 100 * MB).expect("test: shrink"), 300 * MB);
            assert_eq!(l.available(ResourceKind::Vram), 100 * MB, "a shrink returns what it frees, no more");
            l.release("ram").expect("test: release");
            assert_eq!(l.available(ResourceKind::Ram), free - 300 * MB);

            l.reserve("call", ResourceKind::Ram, 50 * MB);
            assert_eq!(l.available_for("learning:her", ResourceKind::Vram), free - 350 * MB, "a host floor binds the GPU axis");
            l.reserve("call", ResourceKind::Ram, 0);
            l.reserve("render", ResourceKind::Vram, 50 * MB);
            assert_eq!(l.available_for("embed", ResourceKind::Ram), free - 350 * MB, "a GPU floor binds the host axis");
            l.reserve("render", ResourceKind::Vram, 0);

            // Her 300 MB land: she declares them, and a sample taken after shows them. Physical rises by what
            // stopped being owed; the headroom does not move.
            let before = l.available(ResourceKind::Ram);
            l.landed(&gpu.lease_id, obs(1)).expect("test: her lease is live");
            for kind in [ResourceKind::Vram, ResourceKind::Ram] {
                assert!(l.set_physical_sample(kind, at(HOST_USED + 300 * MB, 1)));
            }
            assert_eq!(l.available(ResourceKind::Ram), before, "landed bytes are counted once, not again");
            assert_eq!(l.available(ResourceKind::Vram), before);
        }

        // The ledger's arithmetic holds at any capacity, so each case runs near the limit of several
        // (none of them is a claim about a machine): what matters is the headroom left beside residency.
        const MB: u64 = 1_000_000;
        const CAPACITIES: [u64; 3] = [8_000 * MB, 48_000 * MB, 512_000 * MB];

        // what this catches (Codex's replay of cb410506c on #6124): a new lease beside its owner's old
        // residency hidden inside it. 20 MB of headroom; an 18 MB stage leaves 2 MB, so a 4.096 MB learning
        // step is refused, on a discrete device and on unified memory, whether serving or a separate
        // consumer holds the stage. Once the stage measures resident, nothing changes.
        #[test]
        fn a_new_lease_beside_old_residency_is_pending_until_measured() {
            for (cap, unified, stager) in CAPACITIES.iter().flat_map(|&cap| [(cap, false, "serving"), (cap, false, "stage"), (cap, true, "serving"), (cap, true, "stage")]) {
                let base = cap - 20 * MB; // resident now: 20 MB of headroom
                let mut l = ResourceLeaseLedger::new();
                for kind in [ResourceKind::Vram, ResourceKind::Ram] {
                    l.set_capacity(kind, cap);
                    l.set_physical_used(kind, base);
                }
                if unified {
                    l.set_backing(ResourceKind::Vram, Some(ResourceKind::Ram));
                }
                let resident = |extra: u64| vec![ConsumerFootprint { kind: ResourceKind::Vram, bytes: base + extra, detail: "weights + KV".into() }];
                l.set_measured("serving", resident(0));
                assert!(l.acquire(&req(stager, ResourceKind::Vram, 18 * MB, ReclaimPolicy::Graceful), "stage".into(), 0).is_ok());
                assert_eq!(l.available(ResourceKind::Vram), 2 * MB, "unified={unified} stager={stager}: the stage is spent now");
                assert!(
                    l.acquire(&req("learning:her", ResourceKind::Vram, 4_096_000, ReclaimPolicy::Graceful), "step".into(), 0).is_err(),
                    "unified={unified} stager={stager}: a 4.096 MB step does not fit 2 MB"
                );
                // the stage lands: its holder declares it, and a sample taken after shows it, counted once
                l.landed("stage", obs(1)).expect("test: the stage is live");
                for kind in [ResourceKind::Vram, ResourceKind::Ram] {
                    assert!(l.set_physical_sample(kind, at(base + 18 * MB, 1)));
                }
                assert_eq!(l.available(ResourceKind::Vram), 2 * MB, "unified={unified} stager={stager}: landed, still counted once");
            }
        }

        // what this catches (Codex's replay of 8a0b59e03 on #6124): growth before a lease credited to it. 50 MB of
        // headroom; serving takes 10 MB, then measures 30 MB more (its 10 landed, 20 of unrelated KV growth),
        // then takes 18 MB more. That 18 is still owed (2 MB left), so a 4.096 MB step is refused, on both
        // topologies; when it lands, it is counted once.
        #[test]
        fn a_later_grant_is_never_credited_with_growth_before_it() {
            for (cap, unified) in CAPACITIES.iter().flat_map(|&cap| [(cap, false), (cap, true)]) {
                let base = cap - 50 * MB;
                let mut l = ResourceLeaseLedger::new();
                let sample = |l: &mut ResourceLeaseLedger, resident: u64, sequence: u64| {
                    for kind in [ResourceKind::Vram, ResourceKind::Ram] {
                        assert!(l.set_physical_sample(kind, at(resident, sequence)));
                    }
                };
                for kind in [ResourceKind::Vram, ResourceKind::Ram] {
                    l.set_capacity(kind, cap);
                }
                if unified {
                    l.set_backing(ResourceKind::Vram, Some(ResourceKind::Ram));
                }
                sample(&mut l, base, 1);
                assert!(l.acquire(&req("serving", ResourceKind::Vram, 10 * MB, ReclaimPolicy::Graceful), "first".into(), 0).is_ok());
                l.landed("first", obs(2)).expect("test: first is live");
                sample(&mut l, base + 30 * MB, 2); // the first 10 landed, 20 of unrelated KV
                assert!(l.acquire(&req("serving", ResourceKind::Vram, 18 * MB, ReclaimPolicy::Graceful), "second".into(), 0).is_ok());
                assert_eq!(l.available(ResourceKind::Vram), 2 * MB, "unified={unified}: the second grant is still owed");
                assert!(
                    l.acquire(&req("learning:her", ResourceKind::Vram, 4_096_000, ReclaimPolicy::Graceful), "step".into(), 0).is_err(),
                    "unified={unified}: a 4.096 MB step does not fit 2 MB"
                );
                l.landed("second", obs(3)).expect("test: second is live");
                assert_eq!(l.available(ResourceKind::Vram), 2 * MB, "unified={unified}: declared, but no sample since shows it");
                sample(&mut l, base + 48 * MB, 3);
                assert_eq!(l.available(ResourceKind::Vram), 2 * MB, "unified={unified}: landed, counted once");
            }
        }

        // what this catches (Codex's replay of c1bb9ab97 on #6124): the same owner's KV growth after a grant read
        // as that grant landing. 70 MB of headroom; serving takes 18 MB for a prefetch, then its KV grows 20 MB
        // before the prefetch lands: 32 MB are left, not 50, and only a declared landing plus a later sample
        // stops the 18 MB being owed.
        // what this catches (Codex's stale-cache replay of c8a484d6f on #6124): a cached reading served again after a
        // landing taken as showing it. 30 MB of headroom; an 18 MB stage is declared landed at t=5, while the monitor
        // still serves the frame its probe began at t=4: the stage stays owed. A frame observed at t=5 shows it, once.
        // On unified memory the backing pool's reading must be that new too.
        #[test]
        fn a_landing_clears_only_on_a_reading_observed_after_it() {
            for (cap, unified) in CAPACITIES.iter().flat_map(|&cap| [(cap, false), (cap, true)]) {
                let base = cap - 30 * MB;
                let mut l = ResourceLeaseLedger::new();
                for kind in [ResourceKind::Vram, ResourceKind::Ram] {
                    l.set_capacity(kind, cap);
                    assert!(l.set_physical_sample(kind, at(base, 1)));
                }
                if unified {
                    l.set_backing(ResourceKind::Vram, Some(ResourceKind::Ram));
                }
                assert!(l.acquire(&req("stage", ResourceKind::Vram, 18 * MB, ReclaimPolicy::Graceful), "stage".into(), 0).is_ok());
                l.landed("stage", obs(5)).expect("test: the stage is live");
                for kind in [ResourceKind::Vram, ResourceKind::Ram] {
                    assert!(l.set_physical_sample(kind, at(base, 4))); // the cached frame, taken before the landing
                }
                assert_eq!(l.available(ResourceKind::Vram), 12 * MB, "unified={unified}: an older frame cannot show the landing");
                assert!(l.set_physical_sample(ResourceKind::Vram, at(base + 18 * MB, 5)));
                if unified {
                    assert_eq!(l.available(ResourceKind::Vram), 12 * MB, "the device shows it, the backing pool has not yet: still owed on top");
                    assert!(l.set_physical_sample(ResourceKind::Ram, at(base + 18 * MB, 5)));
                }
                assert_eq!(l.available(ResourceKind::Vram), 12 * MB, "unified={unified}: observed after the landing, counted once");
            }
        }

        #[test]
        fn growth_beside_an_undeclared_grant_never_lands_it() {
            for (cap, unified) in CAPACITIES.iter().flat_map(|&cap| [(cap, false), (cap, true)]) {
                let base = cap - 70 * MB;
                let mut l = ResourceLeaseLedger::new();
                for kind in [ResourceKind::Vram, ResourceKind::Ram] {
                    l.set_capacity(kind, cap);
                    assert!(l.set_physical_sample(kind, at(base, 1)));
                }
                if unified {
                    l.set_backing(ResourceKind::Vram, Some(ResourceKind::Ram));
                }
                assert!(l.acquire(&req("serving", ResourceKind::Vram, 18 * MB, ReclaimPolicy::Graceful), "prefetch".into(), 0).is_ok());
                for kind in [ResourceKind::Vram, ResourceKind::Ram] {
                    assert!(l.set_physical_sample(kind, at(base + 20 * MB, 2))); // KV, not the prefetch
                }
                assert_eq!(l.available(ResourceKind::Vram), 32 * MB, "unified={unified}: the prefetch is still owed");
                // an older sample arriving late never replaces the newer one
                for kind in [ResourceKind::Vram, ResourceKind::Ram] {
                    assert!(!l.set_physical_sample(kind, at(base, 1)));
                }
                assert_eq!(l.available(ResourceKind::Vram), 32 * MB, "unified={unified}: the late, older sample was refused");
            }
        }
    }

