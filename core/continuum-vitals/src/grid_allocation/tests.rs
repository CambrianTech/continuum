#![cfg(test)]
    use super::*;

    // what this catches (Astra, Windows 2026-09-22): this node's row in the allocator
    // coming from its PLAN while every peer's comes from their SERVED beacon. A box
    // planning a 2 × 5,533 shrink while its lane actually served 1 × 124,160 read as
    // BelowEveryRequirement and offered zero seats, with live capacity standing there.
    // The local row must carry the same three numbers `grid_capacity` beacons — and must
    // be an ABSENCE, never a substituted intent, when nothing is being served.
    #[test]
    fn the_local_row_is_the_served_geometry_the_beacon_publishes_or_nothing() {
        use continuum_engine::inference::llama_server::ServingSnapshot;
        let served = ServingSnapshot {
            active_model: Some("qwen3.8-27b".into()),
            ready: true,
            lanes: 1,
            served_context_window: 124_160,
            ..ServingSnapshot::empty()
        };
        let row = LanePlan::serving(&served, 9, Some(20.0)).expect("a served lane is a row");
        assert_eq!((row.lanes, row.window), (1, 124_160), "the beacon's numbers, not the plan's");
        assert!(row.holds(&coder().requirement), "a 124k lane seats the 64k role");

        // The intent that caused the misjudgement: the same box planning a shrink. Its
        // row must NOT be built from these numbers.
        let planned_shrink = ServingSnapshot {
            active_model: Some("qwen3.8-27b".into()),
            ready: true,
            lanes: 2,
            served_context_window: 5_533,
            ..ServingSnapshot::empty()
        };
        let shrunk = LanePlan::serving(&planned_shrink, 9, Some(20.0)).expect("still a row");
        assert!(!shrunk.holds(&coder().requirement), "5,533 below 65,536 is the reading that starved the node");

        // Absence, not fabrication: nothing served ⇒ no row, on each of the three ways
        // "nothing" arrives.
        assert!(LanePlan::serving(&ServingSnapshot::empty(), 9, None).is_none(), "no model = no row");
        // MID-RELAUNCH: geometry retained, `ready` false — `serving_consumer` flips the
        // flag and clears nothing. A lane that cannot serve is no seat (Astra's review).
        assert!(
            LanePlan::serving(
                &ServingSnapshot { active_model: Some("m".into()), ready: false, lanes: 1, served_context_window: 32_768, ..ServingSnapshot::empty() },
                9,
                None
            )
            .is_none(),
            "a relaunching lane keeps its numbers and must NOT be announced as capacity"
        );
        assert!(
            LanePlan::serving(&ServingSnapshot { active_model: Some("m".into()), ready: true, lanes: 0, served_context_window: 65_536, ..ServingSnapshot::empty() }, 9, None).is_none(),
            "zero lanes = no row"
        );
        assert!(
            LanePlan::serving(&ServingSnapshot { active_model: Some("m".into()), ready: true, lanes: 2, served_context_window: 0, ..ServingSnapshot::empty() }, 9, None).is_none(),
            "unknown window = no row, never a guess"
        );
    }

    // ---- fixtures: shapes, not Joel's grid. Numbers are illustrative machine classes. ----
    fn coder() -> Role {
        Role {
            name: "coder".into(),
            requirement: Requirement { window: 65_536, target_window: None, min_capability: 7, decode_floor_tps: Some(10.0) },
        }
    }
    fn orchestrator() -> Role {
        Role {
            name: "orchestrator".into(),
            requirement: Requirement { window: 32_768, target_window: None, min_capability: 3, decode_floor_tps: None },
        }
    }
    fn plan(model: &str, cap: u8, window: u32, lanes: u32, tps: Option<f32>) -> LanePlan {
        LanePlan { model_id: model.into(), capability_rank: cap, window, lanes, decode_tps_per_lane: tps }
    }
    const US: Uuid = Uuid::from_u128(0xA11);
    fn minds(role: usize, home: Option<Uuid>, n: usize) -> Vec<Mind> {
        (0..n).map(|_| Mind { id: Uuid::new_v4(), role, home, owner: US }).collect()
    }
    fn offer(node: Uuid, plans: Vec<LanePlan>) -> NodeOffer {
        NodeOffer { node, owner: US, terms: OfferTerms::open(), plans }
    }
    /// A 64 GB unified-memory box: the 27B at 2 lanes or 1 wide lane; a 7B at 4 lanes.
    fn big_box(node: Uuid) -> NodeOffer {
        offer(
            node,
            vec![
                plan("27b", 9, 67_072, 2, Some(14.0)),
                plan("27b", 9, 131_072, 1, Some(14.3)),
                plan("7b", 5, 131_072, 4, Some(40.0)),
            ],
        )
    }
    /// A 32 GB discrete-GPU box: the 27B at 3 lanes × 128k.
    fn gpu_box(node: Uuid) -> NodeOffer {
        offer(node, vec![plan("27b", 9, 131_072, 3, Some(30.0))])
    }
    /// A 16 GB laptop: a 7B or a 1.5B, never the 27B.
    fn small_box(node: Uuid) -> NodeOffer {
        offer(node, vec![plan("7b", 5, 32_768, 1, Some(12.0)), plan("1.5b", 2, 32_768, 3, Some(60.0))])
    }
    fn inputs(nodes: Vec<NodeOffer>, minds: Vec<Mind>) -> GridInputs {
        GridInputs { roles: vec![coder(), orchestrator()], minds, nodes, minds_per_lane: 2, holds: vec![], floors: vec![] }
    }
    fn on(a: &GridAllocation, node: Uuid) -> Vec<Uuid> {
        a.seated.iter().filter(|s| s.node == node).map(|s| s.mind).collect()
    }

    // what this catches (card b8503234, measured 2026-09-21): "no seat" had ONE name for
    // two opposite facts. Benchy's SMALLEST recent prompt (76,952) exceeded the widest
    // lane on the entire grid (75,776), so every node correctly refused her and she
    // landed in `dormant` beside minds who were merely waiting for room. Nothing said
    // the one sentence a human or a peer needed — she read as a citizen who produces
    // nothing rather than a citizen with nowhere to sit. The two want opposite
    // responses: a wider lane / less context / an accepted slow clip, versus patience.
    #[test]
    fn a_mind_with_nowhere_to_sit_is_distinguished_from_one_waiting_for_room() {
        let a_box = Uuid::new_v4();
        // THE SAME GRID BOTH TIMES — only the requirement changes, so the contrast is
        // the fact under test and not the fixture. `big_box` genuinely holds the coder
        // role (capability 9 ≥ 7, a 67k and a 131k plan), which is what makes the
        // waiting case a real wait.
        let full = allocate(&inputs(vec![big_box(a_box)], minds(0, None, 20)));
        assert!(!full.seated.is_empty(), "this role IS servable here");
        assert!(!full.dormant.is_empty(), "twenty minds, far fewer seats");
        assert!(
            full.role_unservable.is_empty(),
            "waiting for room is not unservable: {:?}",
            full.role_unservable,
        );

        // Now a window no offered plan can serve: no node HOLDS the role, `hosts` is
        // empty, and there was never a seat to wait for.
        let mut i = inputs(vec![big_box(a_box)], minds(0, None, 1));
        i.roles[0].requirement.window = 1_000_000;
        let starved = allocate(&i);
        assert!(starved.seated.is_empty(), "no plan on this grid meets her requirement");
        assert_eq!(starved.dormant.len(), 1);
        assert_eq!(
            starved.role_unservable, starved.dormant,
            "no node holds her role: unservable, never merely waiting",
        );

        // And a grid with room for everyone reports neither.
        let roomy = allocate(&inputs(vec![big_box(a_box)], minds(0, None, 1)));
        assert!(roomy.dormant.is_empty() && roomy.role_unservable.is_empty());
    }

    // what this catches (card 426a26fb): the operator's EXCLUSIVE hold as an input — on the
    // held box only its names sit and the box has only that many seats; the rest of the
    // roster goes where the grid has room; a name with no mind behind it is NOT a seat to
    // mint (the spawner used to draw the hold's count and ask the provider every second).
    #[test]
    fn an_exclusive_hold_defines_a_boxs_roster_and_nothing_is_minted_for_it() {
        let a_box = Uuid::new_v4();
        let b_box = Uuid::new_v4();
        let m = minds(0, Some(a_box), 6); // all six call the first box home
        let free = allocate(&inputs(vec![big_box(a_box), big_box(b_box)], m.clone()));
        assert_eq!(on(&free, a_box).len(), 4, "without a hold: four at home, two spill");
        assert_eq!(on(&free, b_box).len(), 2);
        let mut i = inputs(vec![big_box(a_box), big_box(b_box)], m.clone());
        i.holds.push(Hold { node: a_box, only: vec![m[4].id, m[5].id], unresolved: 0, exclusive: true });
        let held = allocate(&i);
        assert_eq!(held.node(a_box).unwrap().seats, 2, "the hold defines the roster: two seats");
        assert_eq!(on(&held, a_box), vec![m[4].id, m[5].id], "only the named sit there — and they sit FIRST, though last in order");
        assert_eq!(on(&held, b_box).len(), 4, "the other four go where the grid has room");
        assert!(held.dormant.is_empty());
        assert!(held.open.is_empty());
        // A name with no mind behind it: one seat stays empty and is NOT open to mint.
        i.holds[0].only = vec![m[4].id, Uuid::new_v4()];
        let ghost = allocate(&i);
        assert_eq!(on(&ghost, a_box), vec![m[4].id]);
        assert!(ghost.open.iter().all(|o| o.node != a_box), "a held box is never minted for");
        assert_eq!(ghost.dormant.len(), 1, "six minds, five seats (one held empty)");
        // An exclusive hold naming nobody restricts nothing (the file's own semantics).
        i.holds[0].only.clear();
        assert_eq!(on(&allocate(&i), a_box).len(), 4);
        // A hold or a floor is an input: the key moves with it (the daemon recomputes).
        let k = inputs_key(&i);
        i.holds[0].only.push(m[0].id);
        assert_ne!(inputs_key(&i), k);
        i.floors.push(RoleFloor { role: 1, min_seats: 1 });
        assert_ne!(inputs_key(&i), k);
    }

    // what this catches: a DERIVED hold (a working round's team) orders, never restricts —
    // its names seat first on its node, everyone else after, never fewer minds.
    #[test]
    fn a_derived_hold_seats_its_team_first_and_never_fewer() {
        let n = Uuid::new_v4();
        let m = minds(0, Some(n), 6);
        let mut i = inputs(vec![big_box(n)], m.clone());
        i.holds.push(Hold { node: n, only: vec![m[5].id, m[4].id], unresolved: 0, exclusive: false });
        let a = allocate(&i);
        assert_eq!(a.node(n).unwrap().seats, 4, "a derived hold does not cap the seats");
        let seated = on(&a, n);
        assert_eq!(&seated[..2], &[m[4].id, m[5].id], "the team first, stable in their own order");
        assert_eq!(&seated[2..], &[m[0].id, m[1].id], "then the rest in their order");
        assert_eq!(a.dormant, vec![m[2].id, m[3].id]);
        // A named mind whose home is elsewhere sits on the hold's node first.
        let mut away = minds(0, Some(Uuid::new_v4()), 1);
        away.extend(minds(0, Some(n), 4));
        let mut i = inputs(vec![big_box(n)], away.clone());
        i.holds.push(Hold { node: n, only: vec![away[0].id], unresolved: 0, exclusive: false });
        assert_eq!(on(&allocate(&i), n)[0], away[0].id, "the hold's node is her home first");
    }

    // what this catches: a recipe's declared team as an input — a 4-seat box, 5 coders and
    // one orchestrator: with no floor the coders take every seat (priority); with a floor of
    // one orchestrator it is 3 + 1. A floor the grid cannot fill for want of MINDS is the
    // spawn signal's first claim: open for the orchestrator before the box's best role.
    #[test]
    fn a_declared_team_is_seated_before_a_higher_role_takes_the_rest_and_claims_the_spawn_signal() {
        let n = Uuid::new_v4();
        let mut m = minds(0, Some(n), 5);
        m.extend(minds(1, Some(n), 1));
        let plain = allocate(&inputs(vec![big_box(n)], m.clone()));
        assert_eq!(plain.seated.iter().filter(|s| s.role == 1).count(), 0, "no floor: priority seats the coders");
        let mut i = inputs(vec![big_box(n)], m.clone());
        i.floors.push(RoleFloor { role: 1, min_seats: 1 });
        let a = allocate(&i);
        assert_eq!(a.seated.iter().filter(|s| s.role == 0).count(), 3);
        assert_eq!(a.seated.iter().filter(|s| s.role == 1).count(), 1);
        assert_eq!(a.dormant.len(), 2, "two coders wait");
        assert!(a.open.is_empty());
        // A floor above the minds that exist reserves nothing for nobody: 3 + 1 still.
        i.floors[0].min_seats = 3;
        let a = allocate(&i);
        assert_eq!(a.seated.iter().filter(|s| s.role == 0).count(), 3);
        assert_eq!(a.seated.iter().filter(|s| s.role == 1).count(), 1);
        assert!(a.open.is_empty(), "no free seat: the coders that exist take them");
        // Two coders, no orchestrator, floor one: the open seats name the orchestrator FIRST.
        let mut i = inputs(vec![big_box(n)], minds(0, Some(n), 2));
        i.floors.push(RoleFloor { role: 1, min_seats: 1 });
        let a = allocate(&i);
        assert_eq!(a.open, vec![OpenSeats { node: n, owner: US, role: 1, count: 1 }, OpenSeats { node: n, owner: US, role: 0, count: 1 }]);
        // A floor for a role no node holds opens nothing (the small box cannot seat a coder).
        let s = Uuid::new_v4();
        let mut i = inputs(vec![small_box(s)], vec![]);
        i.floors.push(RoleFloor { role: 0, min_seats: 2 });
        let a = allocate(&i);
        assert_eq!(a.open, vec![OpenSeats { node: s, owner: US, role: 1, count: 2 }], "the 7B holds orchestration only");
    }

    // what this catches: the 2026-09-20 07:52Z shape — a box whose only runnable plans are
    // 2,048-token lanes. Before this, sixteen minds drew onto it and every turn refused.
    #[test]
    fn a_box_that_can_only_serve_two_k_seats_nobody_and_says_how_far_off_it_is() {
        let n = Uuid::new_v4();
        let two_k = offer(n, vec![plan("27b", 9, 2_048, 1, None), plan("27b", 9, 2_048, 2, None)]);
        let m = minds(0, Some(n), 6);
        let a = allocate(&inputs(vec![two_k], m.clone()));
        assert_eq!(a.node(n).unwrap().verdict, NodeVerdict::BelowEveryRequirement { best_window: 2_048, best_capability: 9 });
        assert!(a.seated.is_empty());
        assert_eq!(a.dormant.len(), 6, "every mind dormant, identity kept — none seated on a useless lane");
        assert_eq!(a.open_total(), 0, "and nothing tells the spawner to mint more");
    }

    // what this catches: a lone big box seats coders on the 27B at requirement — capability
    // before lane count (the 7B at 4 lanes has more seats and is not chosen), lanes before
    // window (2 × 67k beats 1 × 131k), minds beyond the seats dormant.
    #[test]
    fn a_lone_big_box_seats_its_coders_on_the_most_capable_model_then_the_most_lanes() {
        let n = Uuid::new_v4();
        let m = minds(0, Some(n), 6);
        let a = allocate(&inputs(vec![big_box(n)], m.clone()));
        let node = a.node(n).unwrap();
        assert_eq!(node.verdict, NodeVerdict::Hosts { role: 0 });
        assert_eq!(node.plan.as_ref().unwrap(), &plan("27b", 9, 67_072, 2, Some(14.0)));
        assert_eq!(node.seats, 4);
        assert_eq!(a.seated.len(), 4);
        assert_eq!(a.dormant.len(), 2);
        assert!(a.seated.iter().all(|s| s.node == n && s.role == 0));
    }

    // what this catches: the grid growing — a GPU box joins, the seats grow from 4 to 10,
    // the two dormant minds wake ON THE NEW BOX (residents exist between the grid), the
    // four seated stay home, and the four seats nobody fills are the spawn signal.
    #[test]
    fn a_gpu_box_joining_wakes_the_dormant_keeps_the_seated_home_and_opens_seats_to_mint() {
        let big = Uuid::new_v4();
        let gpu = Uuid::new_v4();
        let m = minds(0, Some(big), 6);
        let alone = allocate(&inputs(vec![big_box(big)], m.clone()));
        let joined = allocate(&inputs(vec![big_box(big), gpu_box(gpu)], m.clone()));
        assert_eq!(joined.seats_for(0), 10);
        assert!(joined.dormant.is_empty(), "every existing mind has a seat now");
        for s in &alone.seated {
            assert_eq!(joined.seat_of(s.mind), Some(big), "a mind seated at home stays home");
        }
        for d in &alone.dormant {
            assert_eq!(joined.seat_of(*d), Some(gpu), "a dormant mind wakes where the seat opened");
        }
        assert_eq!(joined.open, vec![OpenSeats { node: gpu, owner: US, role: 0, count: 4 }]);
    }

    // what this catches: the grid shrinking — the GPU box drops; its minds reseat on the
    // big box's free seats, the rest go dormant, nothing is minted.
    #[test]
    fn a_node_dropping_reseats_what_the_grid_can_hold_and_the_rest_go_dormant() {
        let big = Uuid::new_v4();
        let gpu = Uuid::new_v4();
        let mut m = minds(0, Some(big), 2);
        m.extend(minds(0, Some(gpu), 6));
        let both = allocate(&inputs(vec![big_box(big), gpu_box(gpu)], m.clone()));
        assert_eq!(both.seated.len(), 8);
        assert_eq!(both.open_total(), 2);
        let dropped = allocate(&inputs(vec![big_box(big)], m.clone()));
        assert_eq!(dropped.seated.len(), 4, "four seats on the box that remains");
        assert_eq!(dropped.dormant.len(), 4);
        assert_eq!(dropped.open_total(), 0);
        assert!(dropped.seated.iter().all(|s| s.node == big));
        // The two homed on the big box keep their seats; two of the GPU box's minds move.
        for mind in m.iter().filter(|x| x.home == Some(big)) {
            assert_eq!(dropped.seat_of(mind.id), Some(big));
        }
    }

    // what this catches: a weak node is not a bad coder seat — it hosts the role it CAN hold
    // (orchestration), and coders go where the capability is when a big box joins.
    #[test]
    fn a_small_box_hosts_orchestration_and_coders_go_where_the_capability_is() {
        let small = Uuid::new_v4();
        let big = Uuid::new_v4();
        let mut m = minds(0, Some(small), 2);
        m.extend(minds(1, Some(small), 1));
        let alone = allocate(&inputs(vec![small_box(small)], m.clone()));
        let node = alone.node(small).unwrap();
        assert_eq!(node.verdict, NodeVerdict::Hosts { role: 1 }, "the 7B holds orchestration, never the coder role");
        assert_eq!(node.plan.as_ref().unwrap().model_id, "7b", "the most capable holding model, not the 1.5B with more lanes");
        assert_eq!(alone.dormant.len(), 2, "the coders wait for a box that can hold them");
        assert_eq!(alone.seated.len(), 1);
        let joined = allocate(&inputs(vec![small_box(small), big_box(big)], m.clone()));
        assert!(joined.dormant.is_empty());
        assert!(joined.seated.iter().filter(|s| s.role == 0).all(|s| s.node == big), "coders seat on the big box");
        assert_eq!(joined.node(small).unwrap().verdict, NodeVerdict::Hosts { role: 1 }, "the small box still hosts orchestration");
    }

    // what this catches: the decode knee is a requirement — 2 lanes at 6.3 t/s do not seat a
    // coder whose floor is 10; an UNMEASURED decode is not a refusal (absence ≠ number).
    #[test]
    fn a_starved_decode_does_not_hold_and_an_unmeasured_one_is_not_refused() {
        let req = coder().requirement;
        assert!(!plan("27b", 9, 67_072, 2, Some(6.3)).holds(&req));
        assert!(plan("27b", 9, 67_072, 1, Some(14.3)).holds(&req));
        assert!(plan("27b", 9, 67_072, 2, None).holds(&req));
        assert!(!plan("27b", 9, 65_535, 2, None).holds(&req), "one token under the window is under");
        assert!(!plan("7b", 6, 131_072, 4, Some(40.0)).holds(&req), "capability is a requirement, not a preference");
        assert!(!plan("27b", 9, 131_072, 0, None).holds(&req), "zero lanes seat nobody");
        let n = Uuid::new_v4();
        let knee = offer(n, vec![plan("27b", 9, 67_072, 2, Some(6.3)), plan("27b", 9, 131_072, 1, Some(14.3))]);
        let a = allocate(&inputs(vec![knee], minds(0, Some(n), 3)));
        assert_eq!(a.node(n).unwrap().plan.as_ref().unwrap().lanes, 1, "the knee sheds the second lane; the one lane that decodes is the seat");
        assert_eq!(a.seated.len(), 2);
        assert_eq!(a.dormant.len(), 1);
    }

    // what this catches: the population is the SEATS, never the minds on disk — too few
    // minds opens seats to mint; too many leaves the extra dormant; both are the same law.
    #[test]
    fn open_seats_are_the_spawn_signal_and_extra_minds_are_dormant() {
        let n = Uuid::new_v4();
        let few = allocate(&inputs(vec![big_box(n)], minds(0, None, 1)));
        assert_eq!(few.seated.len(), 1);
        assert_eq!(few.open, vec![OpenSeats { node: n, owner: US, role: 0, count: 3 }]);
        let many = allocate(&inputs(vec![big_box(n)], minds(0, None, 25)));
        assert_eq!(many.seated.len(), 4);
        assert_eq!(many.dormant.len(), 21);
        assert_eq!(many.open_total(), 0);
        let none = allocate(&inputs(vec![], minds(0, None, 3)));
        assert_eq!(none.dormant.len(), 3, "no grid, no seats, every mind dormant");
        let empty = allocate(&inputs(vec![offer(n, vec![])], vec![]));
        assert_eq!(empty.node(n).unwrap().verdict, NodeVerdict::NothingRunnable);
    }

    // what this catches (Cormac's condition on #4259): one engine at one window holds every
    // role at or below it — a 4-seat coder box with 2 coders and 3 orchestrators seats
    // 2 + 2, leaves 1 orchestrator dormant and NO seat open. Before, it seated the coders,
    // held 2 seats open for coders that did not exist, and sent 3 orchestrators dormant.
    #[test]
    fn a_node_seats_every_role_its_plan_holds_in_priority_order() {
        let n = Uuid::new_v4();
        let mut m = minds(0, Some(n), 2);
        m.extend(minds(1, Some(n), 3));
        let a = allocate(&inputs(vec![big_box(n)], m));
        let node = a.node(n).unwrap();
        assert_eq!(node.verdict, NodeVerdict::Hosts { role: 0 }, "chosen for the coders — capability first");
        assert_eq!(node.holds, vec![0, 1], "…and it holds orchestrators too");
        assert_eq!(a.seated.iter().filter(|s| s.role == 0).count(), 2);
        assert_eq!(a.seated.iter().filter(|s| s.role == 1).count(), 2);
        assert_eq!(a.dormant.len(), 1);
        assert_eq!(a.open_total(), 0, "no seat is open while a mind of a held role waits");
        // Priority holds under pressure: 5 coders and 3 orchestrators on the same 4 seats
        // seat 4 coders; every orchestrator waits.
        let mut m = minds(0, Some(n), 5);
        m.extend(minds(1, Some(n), 3));
        let a = allocate(&inputs(vec![big_box(n)], m));
        assert_eq!(a.seated.iter().filter(|s| s.role == 0).count(), 4);
        assert_eq!(a.seated.iter().filter(|s| s.role == 1).count(), 0);
        assert_eq!(a.dormant.len(), 4);
    }

    // what this catches: a homeless mind (fresh identity, home unknown) seats wherever the
    // grid has a free seat, spread to the node with the most room.
    #[test]
    fn a_mind_without_a_home_seats_where_the_grid_has_the_most_room() {
        let big = Uuid::new_v4();
        let gpu = Uuid::new_v4();
        let a = allocate(&inputs(vec![big_box(big), gpu_box(gpu)], minds(0, None, 1)));
        assert_eq!(a.seat_of(a.seated[0].mind), Some(gpu), "six free seats there, four here");
    }

    // what this catches (Joel: "prove that last case too"): a node ANOTHER HUMAN brings
    // is one more offer, with terms. Their coder box lends 2 seats for coders only. Our
    // dormant coders take exactly those two — never a third, never an orchestrator —
    // their own minds seat on it freely, and when they withdraw the lend, ours come home
    // or go dormant. The negotiation's outcome is the terms; the machinery is unchanged.
    #[test]
    fn a_peer_owned_node_lends_seats_within_its_terms_and_never_beyond() {
        let them = Uuid::from_u128(0xB0B);
        let ours = Uuid::new_v4();
        let theirs = Uuid::new_v4();
        let mut m = minds(0, Some(ours), 7); // 7 coders, our box seats 4
        m.push(Mind { id: Uuid::new_v4(), role: 1, home: Some(ours), owner: US }); // an orchestrator of ours
        let their_minds: Vec<Mind> = (0..2).map(|_| Mind { id: Uuid::new_v4(), role: 0, home: Some(theirs), owner: them }).collect();
        m.extend(their_minds.iter().cloned());
        let lending = NodeOffer {
            node: theirs,
            owner: them,
            terms: OfferTerms { seats_lent: Some(2), roles: Some(vec![0]) },
            plans: vec![plan("27b", 9, 131_072, 3, Some(30.0))], // 6 seats
        };
        let a = allocate(&inputs(vec![big_box(ours), lending.clone()], m.clone()));
        let on_theirs = |a: &GridAllocation, owner: Uuid| a.seated.iter().filter(|s| s.node == theirs && m.iter().any(|x| x.id == s.mind && x.owner == owner)).count();
        assert_eq!(on_theirs(&a, them), 2, "their own minds seat on their box, no terms spent");
        assert_eq!(on_theirs(&a, US), 2, "we take exactly the two seats lent");
        assert_eq!(a.seated.iter().filter(|s| s.node == ours).count(), 4, "our box full: 4 coders (the orchestrator waits — coders first)");
        assert_eq!(a.dormant.len(), 2, "one coder and the orchestrator: two lent seats, coders only, and our box is full");
        assert_eq!(a.open, vec![OpenSeats { node: theirs, owner: them, role: 0, count: 2 }], "their two spare seats are THEIRS to fill — our spawner mints nothing for a loan already spent");
        assert!(a.open.iter().all(|o| o.owner != US), "no open seat of ours anywhere");
        // A foreign node that declared no terms lends nothing.
        let silent_terms = NodeOffer { terms: OfferTerms::default(), ..lending.clone() };
        let a = allocate(&inputs(vec![big_box(ours), silent_terms], m.clone()));
        assert_eq!(on_theirs(&a, US), 0, "no declared terms = no loan");
        assert_eq!(on_theirs(&a, them), 2);
        // They withdraw the lend: ours come off their box.
        let withdrawn = NodeOffer { terms: OfferTerms { seats_lent: Some(0), roles: None }, ..lending.clone() };
        let a = allocate(&inputs(vec![big_box(ours), withdrawn], m.clone()));
        assert_eq!(on_theirs(&a, US), 0);
        assert_eq!(on_theirs(&a, them), 2);
        assert_eq!(a.dormant.len(), 4);
        // Open terms between our OWN nodes are never consulted: the earlier scenarios.
        assert!(OfferTerms::open().admits_role(7));
    }

    // what this catches: the offer book folds beacons into the allocator's inputs — a node
    // JOINS on its first beacon, stays while fresh, LEAVES when silent, RETURNS on the next
    // beacon — and the inputs key changes exactly when the allocation could.
    #[test]
    fn the_offer_book_names_join_silence_and_return_and_the_key_moves_only_with_them() {
        let silent = 6 * 60 * 60 * 1000;
        let big = Uuid::new_v4();
        let gpu = Uuid::new_v4();
        let m = minds(0, Some(big), 6);
        let mut book = OfferBook::default();
        book.hear(big_box(big), 1_000);
        let k1 = inputs_key(&inputs(book.live(1_000, silent), m.clone()));
        assert_eq!(inputs_key(&inputs(book.live(2_000, silent), m.clone())), k1, "a repeated beacon is not a change");
        book.hear(gpu_box(gpu), 3_000);
        let joined = book.live(3_000, silent);
        assert_eq!(joined.len(), 2, "join");
        assert_eq!(joined[0].node.min(joined[1].node), joined[0].node, "ordered by node id");
        let k2 = inputs_key(&inputs(joined.clone(), m.clone()));
        assert_ne!(k2, k1);
        assert_eq!(allocate(&inputs(joined, m.clone())).dormant.len(), 0, "six seated across both");
        let later = 3_000 + silent + 1;
        let alone = book.live(later, silent);
        assert_eq!(alone.len(), 0, "both silent: nobody live");
        book.hear(big_box(big), later);
        assert_eq!(book.live(later, silent).len(), 1, "the big box returned; the GPU box is still silent");
        assert_eq!(inputs_key(&inputs(book.live(later, silent), m.clone())), k1, "the same grid as at the start keys the same");
        assert_eq!(book.forget_silent(later, silent), vec![gpu], "the silent one is forgotten, named");
        assert_eq!(allocate(&inputs(book.live(later, silent), m.clone())).dormant.len(), 2, "…and the allocation is the lone-box one again");
    }

    // what this catches (card 10bba591): a recipe's citizens become the allocator's roles
    // and floors from DATA — declared requirements taken as written, undeclared ones at
    // the measured typical prompt (never a constant), authoring order as priority, a
    // repeated role adding a seat to its floor with the strictest requirement kept.
    #[test]
    fn a_recipes_citizens_are_the_allocators_roles_and_floors() {
        use continuum_experience::experience::recipe::{CitizenRecipe, CitizenRequirement};
        use continuum_experience::experience::role::RoleId;
        let coder = |window: u32| CitizenRecipe {
            role: RoleId::Coder,
            requirement: Some(CitizenRequirement { window_tokens: window, min_capability: 7, decode_floor_tps: Some(10.0) }),
        };
        let helper = CitizenRecipe { role: RoleId::Helper, requirement: None };
        let lax = CitizenRecipe {
            role: RoleId::Coder,
            requirement: Some(CitizenRequirement { window_tokens: 8_192, min_capability: 3, decode_floor_tps: Some(5.0) }),
        };
        let (roles, floors) = roles_from(&[coder(65_536), helper.clone(), coder(131_072), lax], Some(70_071));
        assert_eq!(roles.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), vec!["coder", "helper"], "authoring order is priority");
        assert_eq!(roles[0].requirement, Requirement { window: 131_072, target_window: None, min_capability: 7, decode_floor_tps: Some(10.0) }, "the strictest declared requirement wins on every axis — a later laxer citizen loosens nothing");
        assert_eq!(roles[1].requirement, Requirement { window: continuum_engine::inference::planning::serving_plan::BOOTSTRAP_WORKING_SET, target_window: Some(70_071), min_capability: 0, decode_floor_tps: None }, "undeclared demand is a target, never a gate");
        assert_eq!(floors, vec![RoleFloor { role: 0, min_seats: 3 }, RoleFloor { role: 1, min_seats: 1 }]);
        let (roles, _) = roles_from(&[helper], None);
        assert_eq!(roles[0].requirement.window, continuum_engine::inference::planning::serving_plan::BOOTSTRAP_WORKING_SET, "nothing measured yet = the bootstrap working-set floor");
        assert!(roles_from(&[], Some(1)).0.is_empty());
    }

    // Regression: a measured turn larger than every host window made all three
    // healthy nodes BelowEveryRequirement, leaving Kimi and Sahar dormant.
    #[test]
    fn measured_window_above_the_fleet_does_not_remove_its_seats() {
        use continuum_experience::experience::recipe::CitizenRecipe;
        use continuum_experience::experience::role::RoleId;
        let node = Uuid::new_v4();
        let (roles, floors) = roles_from(
            &[CitizenRecipe { role: RoleId::Helper, requirement: None }],
            Some(150_000),
        );
        let mut i = inputs(vec![
            offer(node, vec![plan("27b", 255, 65_280, 1, Some(50.0))]),
            offer(Uuid::new_v4(), vec![plan("peer", 42, 78_592, 3, None)]),
            offer(Uuid::new_v4(), vec![plan("thin", 40, 32_768, 1, None)]),
        ], minds(0, Some(node), 2));
        i.roles = roles;
        i.floors = floors;
        let a = allocate(&i);
        assert_eq!(a.seated.len(), 2);
        assert!(a.dormant.is_empty());
        assert!(a.nodes.iter().all(|n| n.seats > 0));
        let key = inputs_key(&i);
        i.roles[0].requirement.target_window = Some(160_000);
        assert_ne!(key, inputs_key(&i), "new measurements must recompute selection");
        i.nodes[0].plans[0].window = continuum_engine::inference::planning::serving_plan::BOOTSTRAP_WORKING_SET - 1;
        assert_eq!(allocate(&i).node(node).unwrap().seats, 0, "the bootstrap working-set floor remains hard");
    }

    // Regression: measured targets must neither become declared gates nor loosen
    // declared requirements when citizens of the same role are combined.
    #[test]
    fn measured_targets_preserve_declared_gates_and_capability_order() {
        use continuum_experience::experience::recipe::{CitizenRecipe, CitizenRequirement};
        use continuum_experience::experience::role::RoleId;
        let declared = CitizenRecipe {
            role: RoleId::Coder,
            requirement: Some(CitizenRequirement { window_tokens: 32_768, min_capability: 7, decode_floor_tps: Some(10.0) }),
        };
        let measured = CitizenRecipe { role: RoleId::Coder, requirement: None };
        for citizens in [[declared.clone(), measured.clone()], [measured.clone(), declared.clone()]] {
            let (roles, _) = roles_from(&citizens, Some(100_000));
            let req = &roles[0].requirement;
            assert_eq!(req.window, 32_768);
            assert_eq!(req.target_window, Some(100_000));
            assert!(!plan("short", 9, 32_767, 1, Some(20.0)).holds(req));
            assert!(!plan("weak", 6, 131_072, 1, Some(20.0)).holds(req));
            assert!(!plan("slow", 9, 131_072, 1, Some(5.0)).holds(req));
            let candidates = vec![
                plan("strong-wide", 9, 65_536, 1, Some(20.0)),
                plan("strong-many", 9, 32_768, 3, Some(20.0)),
                plan("weaker-fitting", 8, 131_072, 4, Some(20.0)),
            ];
            assert_eq!(best_plan_for(&candidates, req).unwrap().model_id, "strong-wide");
            assert_eq!(candidates[0].better_than(&candidates[1], req), Some(BetterBy::Window));
            assert_eq!(candidates[1].better_than(&candidates[0], req), None, "migration cannot undo target-fit selection");
        }
    }

    // what this catches (card 10bba591): the "better seat" order IS the allocator's —
    // capability first, then lanes, then window — strict on every axis, so equal plans
    // are never a reason to move and a wider window never outranks a stronger model.
    #[test]
    fn a_better_seat_is_judged_in_the_allocators_own_order_and_never_on_equality() {
        let base = plan("27b", 9, 67_072, 2, None);
        let req = Requirement { window: 0, target_window: None, min_capability: 0, decode_floor_tps: None };
        assert_eq!(base.better_than(&base, &req), None, "equal plans: no move");
        assert_eq!(plan("27b", 9, 67_072, 2, Some(14.0)).better_than(&base, &req), None, "decode is not an axis of the order");
        assert_eq!(plan("30b", 10, 2_048, 1, None).better_than(&base, &req), Some(BetterBy::Capability), "capability beats everything below it");
        assert_eq!(base.better_than(&plan("30b", 10, 2_048, 1, None), &req), None);
        assert_eq!(plan("27b", 9, 32_768, 3, None).better_than(&base, &req), Some(BetterBy::Lanes), "same rank: lanes before window");
        assert_eq!(plan("27b", 9, 131_072, 1, None).better_than(&base, &req), None, "fewer lanes: no move, however wide");
        assert_eq!(plan("27b", 9, 131_072, 2, None).better_than(&base, &req), Some(BetterBy::Window), "same rank and lanes: the wider window");
        assert_eq!(BetterBy::Window.as_str(), "window");
    }

    // what this catches (card 10bba591): the node roster the spawner draws from is what
    // the allocation seated there PLUS what stands open for that owner — a lender's open
    // seats are not ours, and a node that made no offer is an absence (the spawner then
    // keeps its own prior), never a zero.
    #[test]
    fn a_nodes_roster_is_what_is_seated_there_plus_what_is_open_for_that_owner() {
        let n = Uuid::new_v4();
        let a = allocate(&inputs(vec![big_box(n)], minds(0, Some(n), 1)));
        assert_eq!(a.roster_on(n, US), Some(GridRoster { seated: 1, open: 3, resuming: 0 }));
        assert_eq!(a.roster_on(n, US).map(GridRoster::seats), Some(4), "one seated, three to mint: four seats");
        assert_eq!(a.roster_on(n, Uuid::from_u128(0xB0B)), Some(GridRoster { seated: 1, open: 0, resuming: 0 }), "the open seats are the owner's, not a stranger's");
        assert_eq!(a.roster_on(Uuid::new_v4(), US), None, "a node that made no offer is an absence");
        assert_eq!(a.plan_of_seat(a.seated[0].mind).map(|p| p.lanes), Some(2));
    }

    // what this catches: the outlier validation — the REAL per-node planner's plan reads
    // as an offer with no translation layer, so the daemon feeds `allocate` what it
    // already publishes. A cold 64 GB-class budget planning a 27B-class footprint.
    #[test]
    fn a_real_serving_plan_reads_as_a_lane_plan() {
        use continuum_engine::inference::planning::serving_plan::{plan_serving, HostBudget, ModelFootprint, ServingDemand};
        let model = ModelFootprint {
            model_id: "coder-27b".into(),
            weights_bytes: 19_000_000_000,
            kv_per_token: 32_768,
            context_window: 262_144,
            capability_rank: 9,
            fixed_per_lane_bytes: 0,
        };
        let host = HostBudget { usable_bytes: 25_000_000_000, perf_cores: 12 };
        let plan = plan_serving(&host, &[model], &ServingDemand::new(2, Some(70_000))).expect("a 27B fits a 25 GB budget");
        let lane = LanePlan::of(&plan, None);
        assert_eq!(lane.model_id, "coder-27b");
        assert_eq!(lane.capability_rank, 9);
        assert_eq!(lane.window, plan.served_context_window);
        assert_eq!(lane.lanes, plan.lanes);
        assert!(lane.lanes >= 1);
        let n = Uuid::new_v4();
        let i = inputs(vec![offer(n, vec![lane.clone()])], minds(0, Some(n), 2));
        let a = allocate(&i);
        // The node hosts the first role its real plan holds — a 25 GB budget planning a
        // 27B for two lanes lands under the coder window here (2 × ~44k) and hosts
        // orchestration; the verdict names the plan's real width either way.
        let expected = match i.roles.iter().position(|r| lane.holds(&r.requirement)) {
            Some(role) => NodeVerdict::Hosts { role },
            None => NodeVerdict::BelowEveryRequirement { best_window: lane.window, best_capability: 9 },
        };
        assert_eq!(a.node(n).unwrap().verdict, expected);
        assert!(lane.window < coder().requirement.window, "the shape this fixture pins: two lanes on 25 GB do not reach the coder window");
    }

    // M5 2026-10-27: two live members of a four-name hold published only two
    // roster seats; the other two could never register. Reserve their resume
    // capacity without advertising it as room to mint unrelated identities.
    #[test]
    fn held_members_can_register_before_the_roster_is_full() {
        let n = Uuid::new_v4();
        let members = minds(0, Some(n), 4);
        let mut i = inputs(vec![big_box(n)], members[..2].to_vec());
        i.holds.push(Hold { node: n, only: members[..2].iter().map(|m| m.id).collect(), unresolved: 2, exclusive: true });
        let a = allocate(&i);
        assert_eq!(a.roster_on(n, US), Some(GridRoster { seated: 2, open: 0, resuming: 2 }));
        assert_eq!(a.roster_on(n, US).unwrap().seats(), 4);
        assert_eq!(a.open_total(), 0, "reserved names cannot mint unrelated citizens");
        assert_eq!(a.roster_on(n, Uuid::new_v4()).unwrap().resuming, 0);
        let k = inputs_key(&i);
        i.holds[0].unresolved = 1;
        assert_ne!(inputs_key(&i), k, "pending membership is a real input");
        i.holds[0].unresolved = 0;
        i.holds[0].only = members.iter().map(|m| m.id).collect();
        i.minds = members;
        let registered = allocate(&i);
        assert_eq!(registered.roster_on(n, US), Some(GridRoster { seated: 4, open: 0, resuming: 0 }));
    }

    #[test]
    fn an_unresolved_hold_stays_exclusive_and_grows_with_lane_capacity() {
        let n = Uuid::new_v4();
        let mut i = inputs(vec![offer(n, vec![plan("27b", 9, 131_072, 1, Some(14.0))])], minds(0, Some(n), 1));
        i.holds.push(Hold { node: n, only: vec![], unresolved: 4, exclusive: true });
        let a = allocate(&i);
        assert!(on(&a, n).is_empty(), "unresolved membership is not an empty unrestricted hold");
        assert_eq!(a.roster_on(n, US), Some(GridRoster { seated: 0, open: 0, resuming: 2 }));
        let k = inputs_key(&i);
        i.nodes = vec![big_box(n)];
        assert_ne!(inputs_key(&i), k);
        let a = allocate(&i);
        assert_eq!(a.roster_on(n, US), Some(GridRoster { seated: 0, open: 0, resuming: 4 }));
        assert_eq!(a.open_total(), 0);
    }
