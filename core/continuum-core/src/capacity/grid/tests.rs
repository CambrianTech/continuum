    use super::*;

    // what this catches: ALL THREE grid invariants on one timeline, deterministically, zero
    // hardware. (1) partition → local keeps serving, never blocks (t=60min: exactly the local
    // fit, 2 lanes, experience > 0); (2) nodes joining mid-session GROW the placement (t=80min:
    // a never-before-seen peer is adopted and the grid is back to full demand); (3) a peer dying
    // mid-lease is reclaimed with ZERO stranded lanes (t=20min: its 3 lanes re-derive away, the
    // grid serves what honestly remains). Plus the misfit-parts sum at t=0: 6 lanes only exist
    // as 2+3+1 across three differently-sized nodes, each capped by its OWN fit — zero per-node
    // OOM proves the aggregate never got a vote.
    #[test]
    fn live_placement_survives_the_grid_week_partition_loss_join_and_all() {
        let result = GridSimulator::run(
            &grid_week(),
            &LocalFirstFitPolicy {
                safety_margin_bytes: GB,
            },
        );

        assert_eq!(
            result.score.oom_count, 0,
            "per-node fits must never overflow any node"
        );
        assert_eq!(
            result.score.stranded_lanes, 0,
            "live re-derivation never strands a lane"
        );

        let totals: Vec<u32> = result.placements.iter().map(|p| p.total()).collect();
        assert_eq!(
            totals[0], 6,
            "misfit sum: 2 local + 3 big + 1 small covers the demand"
        );
        assert_eq!(
            totals[1], 3,
            "big peer died mid-lease → its lanes reclaimed, grid serves the rest"
        );
        assert_eq!(totals[2], 6, "peer returned → REGROWN to full demand");
        assert_eq!(
            totals[3], 2,
            "total partition → local keeps serving its honest fit, never blocks"
        );
        assert_eq!(
            totals[4], 6,
            "a brand-new node joining 80 min in is adopted to full demand"
        );

        // The misfit-parts detail at t=0: the small peer contributes exactly its own fit (1),
        // not a share of some aggregate.
        let small_share = result.placements[0].remote.iter().map(|(_, n)| *n).min();
        assert_eq!(
            small_share,
            Some(1),
            "the small node serves exactly what IT fits — 1 lane"
        );
    }

    // what this catches: THE INIT-TIME TRAP, executable. A placement computed once at boot and
    // held (the classify_hardware→frozen-policy-file shape) keeps claiming 3 lanes on the big
    // peer after it dies mid-lease — stranded lanes, personas silently dropped — and can never
    // use the newcomer that joins later. The live policy beats it on stranded count AND on the
    // perception reward. If the sticky control ever stops stranding, the scenario drifted and
    // the gate proves nothing.
    #[test]
    fn init_time_placement_strands_lanes_and_loses_on_experience() {
        let scenario = grid_week();
        let live = GridSimulator::run(
            &scenario,
            &LocalFirstFitPolicy {
                safety_margin_bytes: GB,
            },
        );
        let sticky = GridSimulator::run(
            &scenario,
            &StickyPlacementPolicy::new(LocalFirstFitPolicy {
                safety_margin_bytes: GB,
            }),
        );

        assert!(
            sticky.score.stranded_lanes > 0,
            "holding a boot-time placement across a peer death MUST strand lanes — this is the \
             init-time trap the whole fabric exists to kill, got {:?}",
            sticky.score
        );
        assert_eq!(
            live.score.stranded_lanes, 0,
            "the live policy never strands"
        );
        assert!(
            live.score.mean_experience > sticky.score.mean_experience,
            "re-deriving from every snapshot must WIN the perception reward over init-time \
             placement (live={}, sticky={})",
            live.score.mean_experience,
            sticky.score.mean_experience
        );
    }

    // what this catches: the aggregate lie. Three 4GB nodes hold 12GB "grid free" — an
    // aggregate pool would happily grant all 6 demanded 2GB lanes and OOM every node. The
    // per-node fit rule (the SAME lanes_that_fit FitPolicy runs on one device) serves exactly
    // 1+1+1=3 and overflows nothing. If someone "optimizes" placement into aggregate math,
    // this goes red before a real fleet of misfit machines does.
    #[test]
    fn per_node_fit_refuses_what_the_aggregate_would_promise() {
        let dev = |free_gb: u64| DeviceCapacity {
            gpu_total_bytes: 8 * GB,
            gpu_free_bytes_live: free_gb * GB,
            system_ram_free_bytes: 8 * GB,
        };
        let (a, b) = (PeerId::from_u128(10), PeerId::from_u128(11));
        let grid = GridSnapshot {
            local: dev(4),
            peers: vec![
                PeerCapacity {
                    peer: a,
                    capacity: dev(4),
                    reachable: true,
                },
                PeerCapacity {
                    peer: b,
                    capacity: dev(4),
                    reachable: true,
                },
            ],
        };
        let demand = LeaseRequest {
            consumer: "serving".into(),
            want_concurrency: 6,
            spike_bytes: 2 * GB,
        };

        let placement = LocalFirstFitPolicy {
            safety_margin_bytes: GB,
        }
        .place(&grid, &demand);
        assert_eq!(
            placement.total(),
            3,
            "12GB aggregate would promise 6 lanes; three misfit 4GB nodes honestly fit 1 each"
        );
        assert_eq!(
            placement_oom_count(&grid, &demand, &placement),
            0,
            "and honesty means zero per-node overflow"
        );
    }

    // what this catches (Slice A2): AFFINITY beats byte-fill — the strategic-across-nodes win,
    // and the concrete answer to "video chat AND a benchmark at once". A warm peer (already
    // serving the demand's base) with LESS free RAM must win over an emptier COLD node: routing
    // there avoids a cold weight-load + cross-node transfer AND frees the local GPU for the live
    // video huddle. Model-blind LocalFirstFit floors local and fills by free bytes — it pays the
    // cold load and eats the local accelerator. AffinityFit routes the exam to the warm peer and
    // leaves local at ZERO. If affinity ever collapsed to byte-fill, the grid would fight itself
    // instead of placing each workload where it already fits. [[misfit-grid-is-a-distributed-moe]]
    #[test]
    fn affinity_routes_to_the_warm_peer_over_an_emptier_cold_node() {
        let dev = |free_gb: u64| DeviceCapacity {
            gpu_total_bytes: 32 * GB,
            gpu_free_bytes_live: free_gb * GB,
            system_ram_free_bytes: 16 * GB,
        };
        let warm_peer = PeerId::from_u128(20);
        // Local has the MOST free but is COLD (doesn't hold coder-32b). The peer has less free
        // but already serves coder-32b warm.
        let grid = GridSnapshot {
            local: dev(20),
            peers: vec![PeerCapacity {
                peer: warm_peer,
                capacity: dev(6),
                reachable: true,
            }],
        };
        let demand = LeaseRequest {
            consumer: "eval".into(),
            want_concurrency: 1,
            spike_bytes: 2 * GB,
        };

        // Byte-fill: local wins (emptier + local floor) — the benchmark cold-loads on the live GPU.
        let byte_fill = LocalFirstFitPolicy {
            safety_margin_bytes: GB,
        }
        .place(&grid, &demand);
        assert_eq!(
            byte_fill.local_lanes, 1,
            "byte-fill floors local and ignores the warm peer"
        );
        assert!(
            byte_fill.remote.is_empty(),
            "byte-fill never reaches for the warm peer"
        );

        // Affinity: the lane goes to the WARM peer, leaving the local GPU free for live work.
        let mut warm = HashMap::new();
        warm.insert(warm_peer, vec!["coder-32b".to_string()]);
        let affinity = AffinityFitPolicy {
            safety_margin_bytes: GB,
            demand_base: "coder-32b".to_string(),
            warm,
            local_warm: false,
        }
        .place(&grid, &demand);
        assert_eq!(
            affinity.local_lanes, 0,
            "affinity leaves the local GPU free for the live call"
        );
        assert_eq!(
            affinity.remote,
            vec![(warm_peer, 1)],
            "the exam runs on the node that already holds its weights warm"
        );
        // And it never strands or over-commits (the resilience invariants still hold).
        assert_eq!(stranded_lanes(&grid, &affinity), 0);
        assert_eq!(placement_oom_count(&grid, &demand, &affinity), 0);
    }

    // what this catches: with NO affinity signal (empty demand_base), AffinityFit degrades to the
    // model-blind ordering — locality first, then free — so it never behaves worse than the
    // byte-fill baseline on a cold grid. The affinity policy is a strict SUPERSET: it adds warm
    // routing without losing the misfit-parts guarantees.
    #[test]
    fn affinity_with_no_warm_signal_falls_back_to_locality_then_free() {
        let dev = |free_gb: u64| DeviceCapacity {
            gpu_total_bytes: 32 * GB,
            gpu_free_bytes_live: free_gb * GB,
            system_ram_free_bytes: 16 * GB,
        };
        let peer = PeerId::from_u128(21);
        let grid = GridSnapshot {
            local: dev(6),
            peers: vec![PeerCapacity {
                peer,
                capacity: dev(6),
                reachable: true,
            }],
        };
        let demand = LeaseRequest {
            consumer: "eval".into(),
            want_concurrency: 3,
            spike_bytes: 2 * GB,
        };
        let p = AffinityFitPolicy {
            safety_margin_bytes: GB,
            demand_base: String::new(),
            warm: HashMap::new(),
            local_warm: false,
        }
        .place(&grid, &demand);
        // Local fills first (2 lanes fit in 6GB−1GB margin @2GB spike), then the peer takes 1.
        assert_eq!(p.local_lanes, 2, "no affinity ⇒ local first");
        assert_eq!(
            p.remote,
            vec![(peer, 1)],
            "then spill the remainder to the peer"
        );
        assert_eq!(placement_oom_count(&grid, &demand, &p), 0);
    }

    // what this catches: BITTORRENT SCALE. 40 peers flapping at ~70% uptime for 200 ticks, every
    // reachable peer's free GPU wobbling under its OWNER's workloads — 200 placements over a
    // world where no individual node is ever stable. The live policy must ride the churn: zero
    // per-node OOMs, zero stranded lanes, demand nearly always met (the swarm is collectively
    // reliable even though every endpoint is individually flaky — the BitTorrent property), and
    // the placement visibly ADAPTS (it changes with the churn rather than freezing). The sticky
    // init-time control drowns: its boot placement strands lanes across the run. If the live
    // policy ever strands or OOMs here, the fabric does not actually handle P2P churn.
    #[test]
    fn live_placement_rides_bittorrent_churn_at_swarm_scale() {
        let scenario = bittorrent_swarm(40, 200);
        let live = GridSimulator::run(
            &scenario,
            &LocalFirstFitPolicy {
                safety_margin_bytes: GB,
            },
        );
        let sticky = GridSimulator::run(
            &scenario,
            &StickyPlacementPolicy::new(LocalFirstFitPolicy {
                safety_margin_bytes: GB,
            }),
        );

        assert_eq!(
            live.score.oom_count, 0,
            "no node ever overflows, at any tick"
        );
        assert_eq!(
            live.score.stranded_lanes, 0,
            "no lane is ever left on a vanished peer"
        );
        assert!(
            live.score.mean_experience > 0.95,
            "a 40-peer swarm at 70% uptime collectively covers 8 lanes essentially always \
             (the BitTorrent property), got {}",
            live.score.mean_experience
        );
        assert!(
            live.score.grant_changes > 100,
            "the placement must FOLLOW the churn (peers flap every tick), got {} changes",
            live.score.grant_changes
        );
        assert!(
            sticky.score.stranded_lanes > 50,
            "the init-time placement strands lanes all run long as its boot-time peers flap, \
             got {}",
            sticky.score.stranded_lanes
        );
        assert!(
            live.score.mean_experience > sticky.score.mean_experience,
            "riding the churn beats freezing at boot on the perception reward \
             (live={}, sticky={})",
            live.score.mean_experience,
            sticky.score.mean_experience
        );
    }

    // what this catches: the GLASS BOX. Running traced must yield one decision per tick whose
    // verdicts explain, from facts, what the policy did to every node — including naming the
    // dead peer at t=20min as unreachable/skipped and (for the sticky control) calling out
    // STRANDED lanes explicitly. If the trace goes silent or stops naming trouble, the allocator
    // is a black box again and nobody can see why a room degraded.
    #[test]
    fn glass_box_trace_explains_every_decision_including_the_trouble() {
        let scenario = grid_week();

        // Live policy: the t=20min decision must show the dead big peer skipped, not used.
        let mut live_box = RecordingGridCapture::default();
        GridSimulator::run_traced(
            &scenario,
            &LocalFirstFitPolicy {
                safety_margin_bytes: GB,
            },
            &mut live_box,
        );
        assert_eq!(
            live_box.decisions.len(),
            scenario.timeline.len(),
            "one decision per tick"
        );
        let t20 = &live_box.decisions[1];
        assert!(
            t20.render().contains("unreachable, skipped"),
            "the trace must NAME the dead peer being skipped: {}",
            t20.render()
        );
        let dead_verdict = t20
            .verdicts
            .iter()
            .find(|v| !v.reachable && matches!(v.node, NodePick::Peer(_)))
            .expect("the dead peer appears in the verdicts");
        assert_eq!(dead_verdict.assigned_lanes, 0, "and it got nothing");

        // Sticky control: the same tick's trace must SHOUT the stranded lanes.
        let mut sticky_box = RecordingGridCapture::default();
        GridSimulator::run_traced(
            &scenario,
            &StickyPlacementPolicy::new(LocalFirstFitPolicy {
                safety_margin_bytes: GB,
            }),
            &mut sticky_box,
        );
        assert!(
            sticky_box.decisions[1].render().contains("STRANDED"),
            "stranded lanes must be called out, loudly: {}",
            sticky_box.decisions[1].render()
        );

        // Human view (visible under `cargo test ... -- --nocapture`): the week, as a story.
        println!("--- grid-week, live policy ---\n{}", live_box.render());
        println!("--- grid-week, sticky control ---\n{}", sticky_box.render());
    }
