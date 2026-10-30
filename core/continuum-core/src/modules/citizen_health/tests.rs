#![cfg(test)]
    use super::*;

    // what this catches (card aed15611): the MINDLESS rule and the bounded choice. A
    // mind whose verdicts were ≥80% gate refusals with no write is mindless; a mind
    // that wrote once, or was refused half the time, or spoke fewer than the floor,
    // is not. The choice is worst-first, never below the resident floor, never more
    // than the per-tick cap.
    #[test]
    fn a_mind_of_recitals_is_paged_out_worst_first_and_never_below_the_floor() {
        let mind = |name: &str, verdicts, refused, acts, writes| MindHour {
            agent_name: name.into(), verdicts, gate_refused: refused, acts, writes, lane_grants: acts,
        };
        assert!(is_mindless(&mind("Sigurd", 21, 18, 3, 0)));
        assert!(!is_mindless(&mind("wrote", 21, 18, 3, 1)), "a write is a mind");
        assert!(!is_mindless(&mind("half", 10, 5, 0, 0)), "half refused is a struggling mind, not a mindless one");
        assert!(!is_mindless(&mind("quiet", 5, 5, 0, 0)), "below the verdict floor: not enough evidence");
        let minds: Vec<(uuid::Uuid, MindHour)> = [
            mind("A", 10, 8, 0, 0),
            mind("B", 20, 20, 0, 0),
            mind("C", 10, 9, 0, 0),
            mind("D", 10, 10, 0, 0),
            mind("ok", 10, 1, 4, 2),
        ]
        .into_iter()
        .map(|m| (uuid::Uuid::new_v4(), m))
        .collect();
        let chosen = mindless_seats(&minds, 16);
        let names: Vec<&str> = chosen.iter().map(|(_, m)| m.agent_name.as_str()).collect();
        assert_eq!(names, ["B", "D", "C"], "worst first, capped at {MINDLESS_MAX_PER_TICK}");
        assert_eq!(mindless_seats(&minds, 3).len(), 1, "3 resident − floor 2 = one seat may rest");
        assert!(mindless_seats(&minds, 2).is_empty(), "at the floor nobody is paged out");
    }

    fn h(resident: u64, lanes: u64, acts: u64, writes: u64) -> CitizenHealth {
        CitizenHealth { window_secs: 3600, resident, lanes, lanes_now: lanes, lanes_sustained: lanes, directed_wait_p50_ms: 0, directed_wait_p90_ms: 0, directed_waits: 0, served_window: 66_000, acts, writes, lanes_granted: acts, settles: 0, credits_staged: 0, credits_settled: 0, stops: StopCounts::default(), lane_starved: 0, generations: 0, generations_dropped: 0, knee: None, prompt_cached_tokens: 0, prompt_prefill_tokens: 0, moves_opportunity: 0, moves_failure: 0, oldest_dormant_turn_age_ms: None, held_for: None }
    }

    // what this catches (card 10bba591): the line carries the grid's half — moves by
    // cause, and the oldest dormant turn when anyone is dormant (never a "0 min ago" for
    // nobody) — and the move ledger is a window like the rest.
    #[test]
    fn the_line_carries_the_moves_and_the_oldest_dormant_turn() {
        let x = h(4, 2, 10, 2);
        let l = line(&x, &verdict(&x));
        assert!(l.contains("moves: 0 on opportunity / 0 on failure"), "{l}");
        assert!(!l.contains("oldest dormant"), "nobody dormant: the line does not invent an age: {l}");
        let moved = CitizenHealth { moves_opportunity: 2, moves_failure: 1, oldest_dormant_turn_age_ms: Some(23 * 60_000 + 5_000), ..x };
        let l = line(&moved, &verdict(&moved));
        assert!(l.contains("moves: 2 on opportunity / 1 on failure · oldest dormant turn 23 min ago"), "{l}");
        note_move(true);
        note_move(false);
        note_move(false);
        let (o, f) = snapshot_moves_and_reset();
        assert!(o >= 1 && f >= 2);
        assert_eq!(snapshot_moves_and_reset(), (0, 0), "a window, not a lifetime");
    }

    // what this catches: the four shapes of 2026-09-14 named by the rule — 16 on 3 is
    // STARVED before it is anything else; acts without writes is READING; a roster
    // with no acts is IDLE; and a roster on enough lanes that writes is HEALTHY.
    #[test]
    fn the_verdict_names_the_afternoons_shapes() {
        assert_eq!(verdict(&h(16, 3, 39, 4)), Verdict::Starved { resident: 16, lanes: 3 });
        // 16 on 6 was "enough lanes" at three per lane; at two per lane it is STARVED and
        // 16 on 8 is the edge — the shapes below stand on 8 lanes.
        assert_eq!(verdict(&h(16, 6, 39, 0)), Verdict::Starved { resident: 16, lanes: 6 });
        assert_eq!(verdict(&h(16, 8, 39, 0)), Verdict::Reading { acts: 39 });
        assert_eq!(
            verdict(&h(16, 8, 0, 0)),
            Verdict::Idle { resident: 16 }
        );
        assert_eq!(verdict(&h(16, 8, 39, 4)), Verdict::Healthy);
        // The first receipt the core ever posted (00:01Z 2026-09-15): 16 residents,
        // 6 lanes, 37 acts, 2 writes — it said "healthy". It is SLOW (on 8 lanes; on 6 it
        // is starved first).
        assert_eq!(verdict(&h(16, 8, 37, 2)), Verdict::Slow { writes: 2, resident: 16 });
        assert_eq!(verdict(&h(4, 2, 10, 1)), Verdict::Healthy, "one write per four minds is the floor, inclusive");
        // 2026-09-16 06:5xZ, the first hour under the decode knee: "STARVED: 16 minds on
        // 5 lanes — the planner owes lanes" while the planner was holding lanes AT the
        // knee on purpose. Lanes at the knee are not owed; seats or decode are.
        let at_knee = CitizenHealth { knee: Some(5), ..h(16, 5, 70, 4) };
        assert_eq!(verdict(&at_knee), Verdict::AtKnee { resident: 16, lanes: 5, knee: 5 });
        assert!(line(&at_knee, &verdict(&at_knee)).contains("never more lanes"));
        let below_knee = CitizenHealth { knee: Some(8), ..h(16, 5, 70, 4) };
        assert_eq!(verdict(&below_knee), Verdict::Starved { resident: 16, lanes: 5 }, "below the knee the planner still owes lanes");
        assert_eq!(verdict(&h(0, 0, 0, 0)), Verdict::Empty { lanes: 0, lanes_granted: 0 }, "an empty node is EMPTY, never 'healthy'");
    }

    // what this catches (card ebce2ba0): a dropped generation reaches the hour line as its
    // OWN number and its own verdict. The M5 2026-09-20 threw away 18 of 50 generations —
    // six at ~165.9 s, four at ~412.2 s, two at ~485.2 s (all of those abandoned at the
    // serving gate before dispatch) and five at 1,207–1,492 s with the model still
    // producing — and the hour line said "READING: n acts, no writes", which points a
    // reader at the citizens. A regression that folds `dropped` into any other counter, or
    // lets READING outrank it, puts the blame back on the minds.
    #[test]
    fn a_dropped_generation_is_counted_rendered_and_outranks_the_reading_verdict() {
        let _ = snapshot_generations_and_reset(); // start this assertion from a known floor
        note_generation_outcome(false);
        note_generation_outcome(true);
        note_generation_outcome(true);
        let (generations, dropped) = snapshot_generations_and_reset();
        // `>=`, not `==`: the ledger is a process global and a sibling test may be
        // generating into it. An UNDER-count is the only thing this can miss, and that is
        // the failure worth catching.
        assert!(generations >= 3 && dropped >= 2, "the ledger counts both halves: {generations}/{dropped}");

        // The rate, derived on read and never stored. An hour that ran nothing did not
        // lose 0% of it.
        let clean = CitizenHealth { generations: 50, generations_dropped: 0, ..h(4, 2, 10, 2) };
        assert_eq!(dropped_pct(&clean), Some(0));
        assert_eq!(dropped_pct(&h(4, 2, 10, 2)), None, "an unmeasured hour is not a clean one");
        assert!(line(&clean, &verdict(&clean)).contains("dropped 0 of 50 (0%)"));
        assert!(line(&h(4, 2, 10, 2), &verdict(&h(4, 2, 10, 2))).contains("dropped n/a"));

        // The measured night: 18 of 50 = 36%, well past the threshold.
        let m5 = CitizenHealth { generations: 50, generations_dropped: 18, ..h(4, 2, 13, 0) };
        assert_eq!(dropped_pct(&m5), Some(36));
        assert_eq!(
            verdict(&m5),
            Verdict::Dropping { dropped: 18, generations: 50 },
            "36% thrown away is a PIPE fault; it must not render as READING"
        );
        let l = line(&m5, &verdict(&m5));
        assert!(l.contains("dropped 18 of 50 (36%)"), "{l}");
        assert!(l.contains("DROPPING"), "{l}");
        assert!(!l.contains("READING"), "{l}");
        // Below the threshold the older verdicts still stand — this does not swallow them.
        let occasional = CitizenHealth { generations: 50, generations_dropped: 2, ..h(4, 2, 13, 0) };
        assert_eq!(verdict(&occasional), Verdict::Reading { acts: 13 }, "4% is a bound trimming an outlier");
        let healthy = CitizenHealth { generations: 50, generations_dropped: 2, ..h(4, 2, 13, 4) };
        assert_eq!(verdict(&healthy), Verdict::Healthy);
    }

    // what this catches: the line carries every number and ends with the verdict, so a
    // reader of the org room acts without a probe query.
    #[test]
    fn the_line_carries_the_numbers_and_ends_with_the_verdict() {
        let x = h(16, 3, 39, 4);
        let l = line(&x, &verdict(&x));
        for needle in ["resident 16", "lanes 3", "acts 39", "writes 4", "cut-think 0", "lane-starved 0", "STARVED"] {
            assert!(l.contains(needle), "{needle} missing from {l}");
        }
        assert!(!l.contains("directed wait"), "no directed call waited: the line does not invent a zero wait");
        // The reserve's guarantee in its own unit, when a directed call did wait.
        let waited = CitizenHealth { directed_wait_p50_ms: 4_200, directed_wait_p90_ms: 61_000, directed_waits: 3, ..x.clone() };
        let l = line(&waited, &verdict(&waited));
        assert!(l.contains("directed wait p50 4s / p90 61s (3 calls)"), "{l}");
        // The prompt cache's receipt (card c119ace7): an unmeasured hour says nothing,
        // a measured one says the fraction of every prompt the lanes did not re-read.
        assert!(!l.contains("prefix reuse"), "no lane reported timings: the line does not invent a 0% reuse");
        let warm = CitizenHealth { prompt_cached_tokens: 228_000, prompt_prefill_tokens: 92_000, ..x.clone() };
        assert_eq!(prefix_reuse_pct(&warm), Some(71));
        let l = line(&warm, &verdict(&warm));
        assert!(l.contains("prefix reuse 71% (228k cached / 92k prefilled)"), "{l}");
    }

    // what this catches: the hour line folding waiting into working, and a merge
    // collapsing the ways an hour fails into one number. M5, 2026-09-20 ~21:55Z (build
    // 0047d521b, 4 residents on 2 lanes at 67,072): the line read "8 acts, 0 writes"
    // while two of those turns were `persona.act.pace` rows with `model_ms=0` and
    // `residue_ms` equal to the whole act — 365,000 and 1,500,001 — i.e. a mind who
    // waited 6 minutes and one who waited 25, both at `lanes_available=0`. A turn that
    // never got a lane (lane-starved), the ways a generation that got one stopped
    // (landed, ended in its think, cut in its answer, cut in its think, at its think
    // budget), and the acts are different failures with different owners. Separate terms,
    // in order, never summed; a cut inside thinking is never read as landed (Aris, the M5
    // 2026-10-30: three such cuts read "think-only 0").
    #[test]
    fn the_line_says_acts_lane_starved_and_every_stop_as_separate_terms() {
        let stops = StopCounts { landed: 3, empty: 1, think_ended: 1, cut_answer: 1, cut_think: 3, think_budget: 1 };
        let m5 = CitizenHealth { acts: 6, lane_starved: 2, stops, ..h(4, 2, 6, 0) };
        let l = line(&m5, &verdict(&m5));
        for needle in [
            "acts 6", "lane-starved 2", "landed 3", "empty 1", "think-ended 1", "cut-answer 1", "cut-think 3",
            "think-budget 1", "writes 0",
        ] {
            assert!(l.contains(needle), "{needle} missing from {l}");
        }
        // Read left to right: work, the waits, then how each generation stopped.
        let at = |n: &str| l.find(n).unwrap_or_else(|| panic!("{n} missing from {l}"));
        assert!(
            at("acts 6") < at("lane-starved 2") && at("lane-starved 2") < at("landed 3"),
            "the terms keep their order: {l}"
        );
        // Never one number standing for several.
        assert!(!l.contains("acts 8"), "the waits are not folded into the acts: {l}");
        assert!(!l.contains("landed 7"), "the cuts are not folded into landed: {l}");
    }

    // what this catches: the ledger is a window — a tick reads AND resets, so the next
    // hour starts from zero rather than accumulating the day.
    #[test]
    fn the_tick_reads_a_window_not_a_lifetime() {
        note_act(true);
        note_act(false);
        note_lane_granted();
        note_settle();
        note_credit_staged();
        note_stop(Stop::CutThink);
        let (a, w, l, s, c, _) = snapshot_and_reset();
        assert!(a >= 2 && w >= 1 && l >= 1 && s >= 1 && c >= 1);
        // what this catches (the M5, 2026-10-30): a cut inside thinking is on the hour's
        // ledger as a cut, and the stops reset with the rest of the window.
        assert!(snapshot_stops_and_reset().cut_think >= 1, "a cut think is on the hour's ledger");
        let (a2, _, _, _, _, _) = snapshot_and_reset();
        assert_eq!(a2, 0);
        // The prompt-cache totals are a window too (card c119ace7), and an unmeasured
        // generation (no timings: 0/0) leaves them untouched.
        note_generation(0, 0);
        note_generation(22_800, 9_300);
        // The lane-starved counter is its own separable window (card cff534ba).
        note_lane_starved();
        assert!(snapshot_lane_starved_and_reset() >= 1, "a lane-starved wait is on the hour's ledger");
        assert_eq!(snapshot_lane_starved_and_reset(), 0, "the tick reset its window");
        let (cached, prefilled) = snapshot_prompt_and_reset();
        assert!(cached >= 22_800 && prefilled >= 9_300, "cached {cached} prefilled {prefilled}");
        // A window, not a lifetime: the read reset it. (`<`, not `== 0`: the ledger is a
        // process global and a parallel test may fold a generation between the two reads.)
        let (cached2, prefilled2) = snapshot_prompt_and_reset();
        assert!(
            cached2 + prefilled2 < cached + prefilled,
            "the tick must reset the window: {cached2}/{prefilled2} after {cached}/{prefilled}"
        );
    }

    // what this catches: a dormant mind that never gets a turn while every seat is taken
    // (card ef25bf6c, 533 minutes on the IntelMac), and a rotation that evicts a working
    // mind or one just seated. Only a quiet resident with a full window of tenure yields,
    // the least served first; nobody yields when nobody waits or a seat already freed.
    #[test]
    fn a_quiet_resident_yields_her_seat_only_while_a_dormant_mind_waits() {
        let hour = |name: &str, acts, writes, grants| MindHour { agent_name: name.into(), acts, writes, lane_grants: grants, ..MindHour::default() };
        let (a, b, c, d) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4(), uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        let residents = vec![
            (a, hour("working", 3, 1, 9)),
            (b, hour("quiet-many-grants", 0, 0, 4)),
            (c, hour("quiet-few-grants", 0, 0, 1)),
            (d, hour("just-seated", 0, 0, 0)),
        ];
        let now = 10 * 3_600_000;
        let seated = |name: &str| Some(if name == "just-seated" { now - 60_000 } else { now - 2 * 3_600_000 });
        let chosen = rotation_candidates(true, 0, &residents, seated, now);
        let names: Vec<&str> = chosen.iter().map(|(_, m)| m.agent_name.as_str()).collect();
        assert_eq!(names, vec!["quiet-few-grants", "quiet-many-grants"], "quiet and tenured only, least served first");
        assert!(rotation_candidates(false, 0, &residents, seated, now).is_empty(), "nobody waits: nobody yields");
        assert!(rotation_candidates(true, 1, &residents, seated, now).is_empty(), "a seat already freed this pass is the clip");
        assert!(rotation_candidates(true, 0, &residents, |_| None, now).is_empty(), "unknown tenure never yields");
    }


    // what this catches (card `6d444769`, and Astra's second review of it): the hour is
    // judged by its MINUTES, not by its pull volume. Each two-minute bucket contributes
    // one value — its maximum — so whoever polls most often cannot decide the median.
    #[test]
    fn the_sustained_lane_count_is_the_median_over_time_buckets_not_over_samples() {
        // The window that produced the card: a node holding about three lanes for an
        // hour, whose `lanes_max` read 8 because it touched eight once.
        let mut hour = [0u64; LANE_BUCKETS];
        for (i, b) in hour.iter_mut().enumerate() {
            *b = if i == 0 { 8 } else { 3 };
        }
        assert_eq!(lanes_sustained_of(&hour), Some(3), "one eight-lane bucket is not the hour");

        // ASTRA'S COUNTEREXAMPLE, which defeated the count floor AND the first-to-last
        // span: one healthy sample at t0, then a burst of one-lane samples ten minutes
        // later during a relaunch. Thirty-two samples, ten minutes apart — and only TWO
        // buckets touched, so it is below the bucket floor and buys nothing.
        let mut burst = [0u64; LANE_BUCKETS];
        burst[0] = 8;
        burst[5] = 1; // t=10min, whatever the volume: a bucket's max is one value
        assert_eq!(
            lanes_sustained_of(&burst),
            None,
            "volume buys nothing — only distinct minutes do"
        );

        // A SINGLE RELAUNCH IN A REAL HOUR cannot move the median either (Cormac's
        // condition on #4244, kept): one bucket at 1 among many at 4.
        let mut steady = [0u64; LANE_BUCKETS];
        for b in steady.iter_mut() { *b = 4; }
        steady[7] = 1;
        assert_eq!(lanes_sustained_of(&steady), Some(4), "one bad bucket is not the hour");

        // THE FLOOR IS DISTINCT BUCKETS, not elapsed time and not sample count.
        let mut thin = [0u64; LANE_BUCKETS];
        for i in 0..LANE_MIN_BUCKETS - 1 { thin[i] = 1; }
        assert_eq!(lanes_sustained_of(&thin), None, "nine two-minute periods is not an hour");
        thin[LANE_MIN_BUCKETS - 1] = 1;
        assert_eq!(lanes_sustained_of(&thin), Some(1), "at the floor it has a shape");
    }

    // what this catches (card `6d444769`, the defect itself): a roster that is starved on
    // the lanes the hour HELD is judged on them, not on the hour's peak — and the line
    // carries the gap, because the gap is the diagnosis.
    #[test]
    fn a_peak_lane_count_can_no_longer_hide_a_starved_roster() {
        let measured = CitizenHealth {
            resident: 16,
            lanes: 8,             // the peak the receipt printed
            lanes_sustained: 3,   // the median of the 699 samples above
            lanes_now: 3,
            ..h(16, 8, 15, 0)
        };
        assert_eq!(
            verdict(&measured),
            Verdict::Starved { resident: 16, lanes: 3 },
            "judged on the lanes the hour held, and the verdict SAYS 3 so the line cannot claim otherwise"
        );
        // AND THE GAP IS IN THE LINE, because the gap is the diagnosis.
        assert!(line(&measured, &verdict(&measured)).contains("lanes 3 (peak 8 this hour)"));

        // THE OLD ARITHMETIC, for contrast: divide by the peak and 16 > 8 x 2 is false, so
        // the roster fell through to a verdict ABOUT THE CITIZENS and nothing rested.
        let by_peak = CitizenHealth { lanes_sustained: 8, ..measured.clone() };
        assert_eq!(verdict(&by_peak), Verdict::Reading { acts: 15 });
    }

    // what this catches (card a18452d6): a node whose only engine a mind's training held read
    // EMPTY twice on enrico (2026-10-27) and was chased as dead. A held hour reads HELD and names
    // the work; an hour with acts is judged as usual even while held.
    #[test]
    fn a_held_engine_reads_held_with_its_work_never_empty() {
        use crate::inference::llama_server::serving_hold::{HeldWork, HoldCause, ServingHold};
        let hold = ServingHold {
            since_ms: 1_793_120_435_262,
            by: HoldCause::Resident {
                work: vec![HeldWork {
                    job: uuid::Uuid::from_u128(0xf633bd36),
                    consumer: "genome-train:f633bd36".into(),
                    base_model: "continuum-ai/qwen2.5-coder-1.5b-instruct-GGUF".into(),
                    engine_pid: 33_971,
                    engine_started_s: 1_793_118_524,
                    port: 58_057,
                }],
            },
        };
        let held = CitizenHealth { held_for: Some(hold.clone()), ..h(0, 0, 0, 0) };
        let v = verdict(&held);
        assert_eq!(v, Verdict::Held { resident: 0, hold });
        let shown = line(&held, &v);
        assert!(shown.contains("HELD") && shown.contains("genome-train:f633bd36") && shown.contains("pid 33971"), "{shown}");
        // An unreadable residency record holds serving too, but confirms no work: the line says
        // ownership is unknown and never claims the engine hosts live work.
        let unknown = CitizenHealth {
            held_for: Some(ServingHold { since_ms: 1_793_120_435_262, by: HoldCause::Unknown { reason: "record unreadable".into() } }),
            ..h(0, 0, 0, 0)
        };
        let unknown_line = line(&unknown, &verdict(&unknown));
        assert!(unknown_line.contains("HELD") && unknown_line.contains("ownership") && unknown_line.contains("record unreadable"), "{unknown_line}");
        assert!(!unknown_line.contains("hosts live work"), "{unknown_line}");
        let acting = CitizenHealth { held_for: held.held_for.clone(), ..h(4, 2, 10, 2) };
        assert_ne!(verdict(&acting).as_str(), "held", "minds that acted are judged on their acts");
    }
