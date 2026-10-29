#![cfg(test)]
    // Regression for card 7028c695: unserved demand invalidated actual restart
    // geometry while refusal samples also masqueraded as successful service.
    #[test]
    fn requested_growth_and_observed_service_remain_distinct_across_restart() {
        use crate::inference::planning::serving_plan::ServingDemand;
        use crate::inference::resource_admission::PromptSample;

        let local = Uuid::new_v4();
        let registry = WorkingSetRegistry::new();
        let legacy: PersonaDemand = serde_json::from_str(
            r#"{"peak_tokens":2866096,"last_tokens":2866096,"last_seen_ms":1,"turns":1,"sent_peak":2866096}"#,
        ).expect("legacy demand");
        registry.observed.insert(local, legacy);
        let samples = [
            PromptSample::Requested(56_057),
            PromptSample::Served { endpoint: "seat".into(), tokens: 828, geometry: crate::inference::served_window_store::ServedGeometry { model_id: "m".into(), generation: Uuid::new_v4(), per_slot_window: 62_720, lanes: 1 } },
            PromptSample::Served { endpoint: "seat".into(), tokens: 120_000, geometry: crate::inference::served_window_store::ServedGeometry { model_id: "other".into(), generation: Uuid::new_v4(), per_slot_window: 128_000, lanes: 1 } },
        ];
        let requested: Vec<_> = samples.iter().filter_map(PromptSample::requested).collect();
        let growth = registry.sent_median_with(&[local], &requested).expect("requested demand");
        assert!(ServingDemand::new(1, Some(2_866_096)).with_sent_median(Some(growth)).window_target() > 2_048);
        assert_eq!(samples[1].served("seat", Some("m")), Some(828));
        assert_eq!(samples[1].served("other-seat", Some("m")), None);
        assert_eq!(samples[1].served("seat", Some("absent")), None);
        let PromptSample::Served { geometry, .. } = &samples[1] else { unreachable!() };
        assert_eq!(PromptSample::geometry_median(&samples, "seat", geometry), Some(828));
        assert_eq!(PromptSample::geometry_median(&samples, "neighbor-seat", geometry), None);
        let rebound = crate::inference::served_window_store::ServedGeometry {
            generation: Uuid::new_v4(), ..geometry.clone()
        };
        assert_eq!(PromptSample::geometry_median(&samples, "seat", &rebound), None,
            "same model and geometry on a later generation cannot reuse prior usage");
        assert_eq!(WorkingSetRegistry::sent_sample_for_plan(56_057, 828, 2_048), 56_057, "starvation still requests growth");

        assert_eq!(registry.sent_median_of(&[local]), Some(2_866_096), "geometry migration never discards requested demand");
    }


    // what this catches (BigMama's correction on #4197, 2026-09-18): a seat whose floor is
    // its residents' median alone. The 5090 had two residents; the median WAS one SWE
    // prompt (~60k+), so 2 × 49k stayed under the floor and the seat kept one lane while
    // twelve leased-in coders sending ~30k queued on it. Their prompts must pull the median
    // to what the seat actually serves; and a seat with NO local residents but leased-in
    // minds must still have a floor.
    // what this catches (Joel 2026-09-20, "fix it, this is the bug"): the REQUIREMENT is
    // her typical turn, not her peak. Kimi on the 5090: one 176k turn ever, ~30–60k
    // usually — the requirement is the median of recent turns, the peak stays the
    // peak, and a legacy record with no ring falls back to her last turn.
    #[test]
    fn a_minds_requirement_is_her_typical_turn_never_her_peak() {
        let reg = WorkingSetRegistry::new();
        let kimi = Uuid::new_v4();
        for (i, t) in [176_154u32, 29_911, 56_057, 40_448, 45_000, 31_000, 60_000].iter().enumerate() {
            reg.record_in_memory(kimi, *t, i as u64);
        }
        let d = reg.demand_of(kimi).expect("recorded");
        assert_eq!(d.peak_tokens, 176_154);
        assert_eq!(typical_tokens(&d), Some(45_000), "the median of recent turns");
        assert_eq!(requirement_tokens(&d), 45_000);
        let legacy = PersonaDemand { peak_tokens: 176_154, last_tokens: 29_911, last_seen_ms: 0, prev_seen_ms: 0, turns: 5, sent_peak: 0, recent: [0; RECENT_TURNS], recent_next: 0, need_recent: [0; RECENT_TURNS], need_next: 0, growth_recent: [0; RECENT_TURNS], growth_next: 0 };
        assert_eq!(requirement_tokens(&legacy), 29_911, "no ring: the last turn, never the peak");
        let ser: PersonaDemand = serde_json::from_str(r#"{"peak_tokens":1,"last_tokens":2,"last_seen_ms":3,"turns":4}"#).expect("legacy json");
        assert_eq!(typical_tokens(&ser), None);
        // The ring is bounded: after 40 turns only the last 16 define "typical".
        for i in 0..40u32 {
            reg.record_in_memory(kimi, 100_000 + i, 100 + i as u64);
        }
        let d = reg.demand_of(kimi).expect("recorded");
        assert!(typical_tokens(&d).expect("ring") >= 100_024, "the old small turns aged out");
    }

    // what this catches (card 70706a9e): the NEED ring learns only from turns the fit
    // HELD. Kimi's wish (untrimmed assembly) ran 99k over a 75,776 seat her turns fitted
    // into every time; gating on the wish × 1.25 read her as "over the grid" at her own
    // seat. The need is the post-fit size of held turns, no headroom; a starved turn
    // (truthful minimum did not fit) or an emptied one (framing only — the 5090's
    // self-sealed 2048) contributes nothing, so the ring can never echo a window.
    // Legacy JSON without the ring reads as no need known.
    #[test]
    fn the_need_ring_learns_only_from_held_turns_and_never_a_windows_echo() {
        // The sample rule, pinned.
        assert_eq!(need_sample(70_000, 3_000, true), Some(70_000), "held: what was sent is the need");
        assert_eq!(need_sample(99_000, 3_000, false), None, "starved: no sample, never the demand");
        assert_eq!(need_sample(828, 828, true), None, "framing only survived: the window's echo, not a turn");
        assert_eq!(need_sample(0, 0, true), None);

        let reg = WorkingSetRegistry::new();
        let kimi = Uuid::new_v4();
        // Wish 99k on every turn; need ~70k on the held ones.
        for i in 0..8u32 {
            reg.record_in_memory(kimi, 99_000 + i, i as u64);
            reg.record_need_in_memory(kimi, 70_000 + i, i as u64);
        }
        let d = reg.demand_of(kimi).expect("recorded");
        assert!(typical_tokens(&d).expect("wish ring") >= 99_000, "the wish is still the wish");
        let need = need_tokens(&d).expect("need ring");
        assert!((70_000..70_008).contains(&need), "the need is what held: {need}");
        // A legacy record has no need ring: None, and the caller falls back to the wish.
        let ser: PersonaDemand = serde_json::from_str(r#"{"peak_tokens":1,"last_tokens":2,"last_seen_ms":3,"turns":4}"#).expect("legacy json");
        assert_eq!(need_tokens(&ser), None);
        // Round-trips with the ring.
        let back: PersonaDemand = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        assert_eq!(need_tokens(&back), Some(need));
    }

    #[test]
    fn the_typical_prompt_includes_the_prompts_a_seat_serves_for_other_nodes() {
        let reg = WorkingSetRegistry::new();
        let local = Uuid::new_v4();
        reg.record_sent_in_memory(local, 64_000, 1);
        // what this catches (the 5090, 2026-09-20): a sample taken at a window that
        // trimmed the turn is the window echoed back, never demand — the seat's typical
        // prompt read 828 under a 2048 window while every real prompt was 56k, and the
        // window sealed itself. At a starved window the untrimmed demand is the sample;
        // at a window that held the turn, what was sent is.
        assert_eq!(WorkingSetRegistry::sent_sample_for_plan(56_057, 828, 2048), 56_057, "starved: the demand votes");
        assert_eq!(WorkingSetRegistry::sent_sample_for_plan(30_000, 25_000, 64_000), 25_000, "held: what was sent votes");
        assert_eq!(WorkingSetRegistry::sent_sample_for_plan(30_000, 25_000, 0), 25_000, "no window known: the sent figure stands");
        assert_eq!(reg.sent_median_of(&[local]), Some(64_000), "one resident: her prompt is the median");
        // twelve leased-in coders at ~30k pull the typical prompt to theirs
        let leased: Vec<u32> = (0..12).map(|i| 30_000 + i * 100).collect();
        let m = reg.sent_median_with(&[local], &leased).expect("a pool");
        assert!(m < 40_000, "the median follows the grid's prompts, not the one resident's: {m}");
        assert!(m >= 30_000);
        // no local residents at all, only leased-in: still a floor
        assert_eq!(reg.sent_median_with(&[], &[30_000, 32_000, 34_000]), Some(32_000));
        // zeros are not samples on either side
        assert_eq!(reg.sent_median_with(&[], &[0, 0]), None);
        assert_eq!(reg.sent_median_of(&[Uuid::new_v4()]), None);
    }

    use super::*;

    fn p(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    // what this catches: the thermostat-inside-the-thermometer failure. Demand ABOVE
    // the window currently served is the only signal that can ever grow the window, so
    // the registry must accept and keep it rather than clamping to anything it knows
    // about the current serving state. If this ever starts capping, an 8k-served
    // citizen can never report that her turn wanted 47k, and the window is frozen at
    // whatever it was first set to — which is the exact bug this module replaces.
    #[test]
    fn demand_far_above_the_current_window_is_recorded_not_clamped() {
        let reg = WorkingSetRegistry::new();
        reg.record_in_memory(p(1), 47_000, 1_000);
        assert_eq!(reg.ceiling(), Some(47_000));
        assert_eq!(reg.demand_of(p(1)).map(|d| d.peak_tokens), Some(47_000));
    }

    // what this catches: averaging a coding turn with idle chatter into a window that
    // serves neither. The high-water mark is the point — a later small turn must not
    // walk the ceiling back down, or one quiet minute re-strangles the next code turn.
    #[test]
    fn a_later_smaller_turn_never_lowers_the_peak() {
        let reg = WorkingSetRegistry::new();
        reg.record_in_memory(p(1), 40_000, 1_000);
        reg.record_in_memory(p(1), 900, 2_000);
        let d = reg.demand_of(p(1)).expect("observed");
        assert_eq!(d.peak_tokens, 40_000, "peak is the high-water mark");
        assert_eq!(d.last_tokens, 900, "…and the latest is kept alongside it");
        assert_eq!(d.turns, 2);
        assert_eq!(reg.ceiling(), Some(40_000));
    }

    // what this catches: provisioning the host for one citizen and starving the other.
    // Lanes share one served window, so the ceiling must be the MAX across minds — the
    // busiest resident is the one whose demand decides whether the lane is big enough.
    #[test]
    fn the_ceiling_is_the_busiest_minds_demand_not_an_average() {
        let reg = WorkingSetRegistry::new();
        reg.record_in_memory(p(1), 6_000, 1_000);
        reg.record_in_memory(p(2), 31_000, 1_000);
        reg.record_in_memory(p(3), 4_000, 1_000);
        assert_eq!(reg.ceiling(), Some(31_000));
    }

    // what this catches: the growth path of the measured reply reserve. A turn that
    // stopped at its output cap is a CLAMPED observation — recording it verbatim
    // would freeze the reserve at whatever strangled it (the same measure-the-clamp
    // trap the demand side documents). It must record at double so the next turn's
    // reserve is larger than the cap that cut this one.
    #[test]
    fn a_capped_emission_records_double_and_an_uncapped_one_verbatim() {
        let reg = WorkingSetRegistry::new();
        reg.record_emission_in_memory(p(1), 2_500, 0, EmissionStop::Landed, 1_000);
        assert_eq!(reg.emission_of(p(1)).map(|e| e.peak_tokens), Some(2_500));
        reg.record_emission_in_memory(p(1), 3_000, 0, EmissionStop::CutMidAnswer, 2_000);
        let e = reg.emission_of(p(1)).expect("observed");
        assert_eq!(
            e.peak_tokens, 6_000,
            "a Length stop is a floor, not a measurement"
        );
        assert_eq!(e.turns, 2);
        // A later small reply lowers the peak, but only by the decay step — it must not
        // collapse to the size of one ack (that is the tiny-talker trap the floor exists
        // for), and it must not stay pinned either (that is the ratchet that poisoned a
        // citizen's emission.json at 16,384 and left her 297 tokens of context).
        reg.record_emission_in_memory(p(1), 40, 0, EmissionStop::Landed, 3_000);
        let decayed = reg
            .emission_of(p(1))
            .map(|e| e.peak_tokens)
            .expect("observed");
        assert!(decayed < 6_000, "a peak that can only rise is a ratchet");
        assert!(decayed > 40, "one small reply must not erase a real measurement");
        assert_eq!(decayed, 6_000 * 7 / 8);
    }

    // what this catches: the poisoned-record half of the 2026-09-18 identity defect. A
    // capped turn records at DOUBLE, and with an all-time `.max()` that value outlived
    // every restart because emission.json is persisted on purpose ("a restart is a pause").
    // One truncation therefore reserved half of every future window forever. The decay must
    // make such a record heal ON ITS OWN — no migration, no operator step, no command —
    // because the citizens carrying poisoned files are the ones least able to ask for help.
    #[test]
    fn a_peak_that_was_never_real_heals_itself_without_a_migration() {
        let reg = WorkingSetRegistry::new();
        // Exactly the poisoned value read off a real citizen's emission.json.
        reg.record_emission_in_memory(p(7), 8_192, 0, EmissionStop::CutMidAnswer, 1_000);
        assert_eq!(
            reg.emission_of(p(7)).map(|e| e.peak_tokens),
            Some(16_384),
            "fixture must reproduce the poisoning, not assume it"
        );

        // Her actual replies are a few hundred tokens. Within a work session of them the
        // measurement must come back to something a reply-sized reserve can be built on.
        for turn in 2..=40u64 {
            reg.record_emission_in_memory(p(7), 300, 0, EmissionStop::Landed, turn * 1_000);
        }
        let healed = reg
            .emission_of(p(7))
            .map(|e| e.peak_tokens)
            .expect("observed");
        assert!(
            healed <= 2_048,
            "a peak that was never real is still {healed} after 39 honest turns"
        );
        assert!(healed >= 300, "and it must not fall below what she actually writes");
    }

    // what this catches: a restart that demotes her. The registry is in-memory, so
    // before persistence a reboot erased every measurement and the planner fell back
    // to the cold-start constant until enough turns re-measured — observed live
    // 2026-08-06, where a measured 24,126 window fell to 16,384 across one reboot and
    // the citizens were re-strangled until they earned it back. Joel's standard for a
    // restart is a PAUSE, not a death.
    #[test]
    // regression for the Kimi allowance ratchet (BigMama, 2026-09-26): 41% of her
    // requests failed and the failed ones were the LARGE ones — a think-only cut
    // doubled the need, the allowance grew to hold it, and the longer pass died at
    // the turn deadline instead of landing. Revised on Cormac's review of #4406:
    // halving the cut was a ratchet the other way once the loop closed, and a cut
    // mid-answer is the write that needed room.
    // what this catches: with the loop CLOSED (each cap is the previous need), a run
    // of think-only cuts moves neither the need nor the peak; a mid-answer cut grows
    // only its channel and the peak is the sum, never output × 2; classification is
    // by channel.
    fn a_think_only_cut_moves_nothing_and_a_mid_answer_cut_grows_only_its_channel() {
        let reg = WorkingSetRegistry::default();
        // Her landed shape: ~6.4k-token turns, mostly think.
        for t in 0..MIN_NEED_TURNS as u64 {
            reg.record_emission_in_memory(p(9), 6_448, 6_000, EmissionStop::Landed, t);
        }
        let landed = reg.need_of(p(9)).expect("need measured");
        let peak_landed = reg.emission_of(p(9)).expect("recorded").peak_tokens;
        // The closed loop: every turn is a think-only cut at the cap the last need set.
        let mut cap = landed.total();
        for t in 0..(NEED_SAMPLES as u64 + 4) {
            reg.record_emission_in_memory(p(9), cap, cap, EmissionStop::classify(true, 0), 100 + t);
            let need = reg.need_of(p(9)).expect("need measured");
            assert_eq!(need, landed, "a think-only cut is not a sample: the need does not move (turn {t})");
            let e = reg.emission_of(p(9)).expect("recorded");
            assert_eq!(e.last_tokens, cap, "the peak side takes the cut verbatim");
            assert!(e.peak_tokens <= peak_landed.max(cap), "never output × 2: peak {}", e.peak_tokens);
            cap = need.total();
        }
        // A cut inside the ANSWER is the growth path, on that channel only: the peak is
        // the sum of the channels, not the whole output doubled.
        reg.record_emission_in_memory(p(8), 7_003, 7_000, EmissionStop::classify(true, 3), 1);
        assert_eq!(reg.emission_of(p(8)).expect("recorded").peak_tokens, 7_006, "7,006, not 14,006");
        reg.record_emission_in_memory(p(7), 7_000, 1_000, EmissionStop::classify(true, 6_000), 1);
        let write = reg.emission_of(p(7)).expect("recorded");
        assert_eq!((write.peak_tokens, write.answer_samples[0], write.reasoning_samples[0]), (13_000, 12_000, 1_000));
        // Classification is by channel; only a cap hit is a cut.
        assert_eq!(EmissionStop::classify(false, 0), EmissionStop::Landed);
        assert_eq!(EmissionStop::classify(false, 9), EmissionStop::Landed);
        assert_eq!(EmissionStop::classify(true, 0), EmissionStop::CutMidThought);
        assert_eq!(EmissionStop::classify(true, 1), EmissionStop::CutMidAnswer);
    }

    #[test]
    fn a_measured_peak_survives_a_restart_so_a_reboot_is_a_pause_not_a_demotion() {
        let home = tempfile::tempdir().expect("tmp home");
        // HOME is absent at the shared policy seam; native discovery points at
        // this thread's isolated home. Never mutate the parallel suite's env.
        let _native = crate::paths::NativeHomeOverride::install(home.path());

        let persona = p(9);
        let before = WorkingSetRegistry::new();
        before.record(persona, 24_126, 1_000);
        let _ = before.record_emission(persona, 321, 200, EmissionStop::Landed, 1_001);
        assert_eq!(before.ceiling(), Some(24_126));

        // A fresh process: new registry, nothing in memory.
        let after = WorkingSetRegistry::new();
        assert_eq!(
            after.ceiling(),
            None,
            "a new registry starts genuinely empty"
        );
        assert_eq!(after.rehydrate_all(), 1);
        assert_eq!(
            after.ceiling(),
            Some(24_126),
            "her measured window must survive the restart, not be re-earned turn by turn"
        );

        assert_eq!(after.emission_of(persona), before.emission_of(persona));
        assert_eq!(
            after.emission_of(persona).map(|e| (e.need_turns, e.reasoning_samples[0], e.answer_samples[0])),
            Some((1, 200, 121)),
            "the channel split rides the same file: her reasoning need is not re-earned per boot"
        );
        assert!(home
            .path()
            .join(".continuum/personas")
            .join(persona.to_string())
            .join("working-set.json")
            .is_file());
    }

    // what this catches (the M5, 2026-09-20 20:58–21:14Z): an allowance sized in time alone
    // (20 s × 12 tok/s = 240) cannot hold a thinking pass; Qwen3.8 ended two turns inside
    // the reasoning channel (`think_only reasoning_len=2730`, `2112`). The need is the p90
    // of recent turns per channel with 5/4 headroom; it is UNKNOWN (None) before three
    // turns and for a legacy emission file; a Length stop with no answer is not a need
    // sample; the ring keeps the last 16 so one loud turn ages out.
    #[test]
    fn the_reasoning_need_is_a_p90_with_headroom_and_unknown_before_it_is_measured() {
        let reg = WorkingSetRegistry::new();
        assert_eq!(reg.need_of(p(3)), None, "never measured: unknown, not zero");
        reg.record_emission_in_memory(p(3), 1_000, 700, EmissionStop::Landed, 1);
        reg.record_emission_in_memory(p(3), 1_200, 900, EmissionStop::Landed, 2);
        assert_eq!(reg.need_of(p(3)), None, "two turns are not yet a need");
        reg.record_emission_in_memory(p(3), 900, 600, EmissionStop::Landed, 3);
        let need = reg.need_of(p(3)).expect("three turns measure a need");
        assert_eq!((need.reasoning, need.answer, need.turns), (1_125, 375, 3), "p90 of 3 is the max (900, 300) × 5/4");
        assert_eq!(need.total(), 1_500);
        // A Length stop with NO answer cut the think: the peak takes it, the need ring
        // does not (Cormac's review of #4406 — doubling it here was the ratchet).
        reg.record_emission_in_memory(p(3), 768, 768, EmissionStop::CutMidThought, 4);
        let need = reg.need_of(p(3)).expect("measured");
        assert_eq!((need.reasoning, need.turns), (1_125, 3), "a think-only cut is not a need sample");
        // A Length stop WITH an answer cut the answer: that channel records at double.
        reg.record_emission_in_memory(p(3), 1_000, 600, EmissionStop::CutMidAnswer, 5);
        assert_eq!(reg.emission_of(p(3)).map(|e| e.answer_samples[3]), Some(800));
        // The ring: sixteen quiet turns age the loud one out.
        for t in 6..=22u64 {
            reg.record_emission_in_memory(p(3), 400, 300, EmissionStop::Landed, t);
        }
        let need = reg.need_of(p(3)).expect("measured");
        assert_eq!((need.reasoning, need.answer, need.turns), (375, 125, 16));
        // A legacy emission.json (no rings) reads as no need — never a zero need.
        let legacy: PersonaEmission =
            serde_json::from_str(r#"{"peak_tokens":2500,"last_tokens":2500,"last_seen_ms":1,"turns":9}"#).unwrap();
        assert_eq!(legacy.need_turns, 0);
        reg.emitted.insert(p(4), legacy);
        assert_eq!(reg.need_of(p(4)), None);
        assert_eq!(p90_with_headroom(&[]), 0);
        assert_eq!(p90_with_headroom(&[100; 10]), 125);
        assert_eq!(p90_with_headroom(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]), 12, "the 9th of ten × 5/4, rounded up");
    }

    // what this catches: an invented number standing in for missing data. Before any
    // turn is measured there IS no measurement, and this must say so — the cold-start
    // decision belongs to the serving planner, in one place, not smuggled in here as a
    // default that every caller then inherits without noticing.
    #[test]
    fn no_observations_reports_absence_never_a_stand_in_number() {
        let reg = WorkingSetRegistry::new();
        assert_eq!(reg.ceiling(), None);
        assert_eq!(reg.observed_personas(), 0);
        // A zero-token turn is a defect signal elsewhere, not an observation here.
        reg.record(p(1), 0, 1_000);
        assert_eq!(
            reg.ceiling(),
            None,
            "a zero demand must not register as data"
        );
    }

    // what this catches (the M5, 2026-09-21): `placement_switch` read a mind's seat
    // requirement as `sent_median_of(&[her]).or(node_median)` — so a mind who had sent
    // nothing yet was judged against the median of EVERY resident's sent peak, and
    // `seat_starves` feeds `FallHome`, so she was EVICTED on a number that was not hers.
    //
    // The sibling test above pins this median for lane SIZING, which is what it is for.
    // This one pins the fact that made it wrong for ADMISSION: on a node hosting one huge
    // prompt, the population's answer is the outlier, and it can exceed every seat on the
    // grid while the small mind being judged fits easily. Benchy peaks ~110k; Aiko wants
    // 22-24k and Atlas 21-35k; all three were walked off the same 75,776 seat that holds
    // the latter two with room to spare. Size lanes with the population; admit a mind on
    // her own demand, or on the serve floor when she has none.
    #[test]
    fn the_population_median_sizes_lanes_and_must_never_admit_a_mind() {
        let reg = WorkingSetRegistry::new();
        let benchy = Uuid::new_v4();
        let aiko = Uuid::new_v4();
        reg.record_sent_in_memory(benchy, 110_396, 0);
        reg.record_sent_in_memory(aiko, 24_226, 0);

        assert_eq!(reg.sent_median_of(&[aiko]), Some(24_226), "her own demand is hers");
        let population = reg.sent_median_of(&[benchy, aiko]).expect("two residents");
        assert_eq!(population, 110_396, "the population's answer IS the outlier here");
        assert!(
            population > 75_776 && 24_226 < 75_776,
            "and it exceeds the widest seat on the grid while she fits it — the eviction this removes",
        );
    }
    // what this catches (Cormac on #4409): with the loop CLOSED and a thinking model that
    // thinks to whatever budget it is given, a think stopped AT the deliberation budget
    // must not teach the need ring the budget — verbatim, the next allowance would be
    // 5/4 × (3/4 A + answer) and contract geometrically toward max(time floor, 20 × answer);
    // recorded at the allowance it would compound toward the reserve instead. Skipped as a
    // sample, the allowance holds exactly at her landed shape.
    #[test]
    fn a_think_stopped_at_the_budget_never_contracts_the_allowance() {
        let reg = WorkingSetRegistry::default();
        for t in 0..MIN_NEED_TURNS as u64 {
            reg.record_emission_in_memory(p(6), 6_448, 6_000, EmissionStop::Landed, t);
        }
        let landed = reg.need_of(p(6)).expect("need measured");
        let mut allowance = landed.total();
        let first = allowance;
        for t in 0..(NEED_SAMPLES as u64 + 4) {
            let budget = crate::inference::request_body::deliberation_reasoning_budget(u64::from(allowance))
                .expect("an allowance large enough to budget") as u32;
            let answer = 300u32;
            let stop = EmissionStop::classify_with_budget(false, answer, budget, Some(budget), allowance);
            assert_eq!(stop, EmissionStop::ThinkBudgetHit { allowance }, "turn {t}");
            reg.record_emission_in_memory(p(6), budget + answer, budget, stop, 100 + t);
            let next = reg.need_of(p(6)).expect("need measured").total();
            // A budget-hit is not a sample: the need ring keeps her landed shape exactly,
            // so the allowance neither contracts toward the floor nor compounds toward
            // the reserve (recorded at the allowance it went 8k → 184k in 20 turns).
            assert_eq!(next, first, "the allowance moved on a censored sample at turn {t}");
            allowance = next;
        }
        assert_eq!(allowance, first, "over the whole loop: {first} → {allowance}");
        // The control: the same sequence recorded verbatim contracts. The ring's p90
        // lags a whole ring (the second-largest of the last NEED_SAMPLES), so each 15/16
        // step shows only per ring turnover; ten turnovers make the geometric shrink
        // toward 20 × answer unmistakable.
        let reg2 = WorkingSetRegistry::default();
        for t in 0..MIN_NEED_TURNS as u64 {
            reg2.record_emission_in_memory(p(5), 6_448, 6_000, EmissionStop::Landed, t);
        }
        let mut a2 = reg2.need_of(p(5)).expect("need measured").total();
        let start2 = a2;
        for t in 0..(NEED_SAMPLES as u64 * 10) {
            let budget = crate::inference::request_body::deliberation_reasoning_budget(u64::from(a2)).expect("budget") as u32;
            reg2.record_emission_in_memory(p(5), budget + 300, budget, EmissionStop::classify(false, 300), 200 + t);
            a2 = reg2.need_of(p(5)).expect("need measured").total();
        }
        assert!(a2 < start2, "the control: verbatim samples contract ({start2} → {a2})");
    }



    #[test]
    fn persisted_m5_peaks_do_not_pin_the_publication_floor() -> Result<(), Box<dyn std::error::Error>> {
        let reg = WorkingSetRegistry::new();
        let residents = [p(201), p(202), p(203), p(204)];
        for (id, (peak, recent)) in residents.iter().zip([
            (370_493, 24_064), (339_156, 43_129), (294_317, 31_355), (209_252, 35_500),
        ]) {
            let mut legacy = serde_json::json!({
                "peak_tokens": peak, "last_tokens": recent, "last_seen_ms": 1,
                "turns": 100, "sent_peak": peak,
            });
            legacy["recent"] = serde_json::to_value(vec![recent; RECENT_TURNS])?;
            let demand: PersonaDemand = serde_json::from_value(legacy)?;
            let restored = serde_json::from_str(&serde_json::to_string(&demand)?)?;
            reg.observed.insert(*id, restored);
        }
        assert_eq!(reg.sent_median_of(&residents), Some(35_500));
        assert_eq!(reg.sent_ceiling_of(&residents), Some(43_129));
        assert_eq!(reg.ceiling_of(&residents), Some(370_493), "retain historical diagnostics");
        Ok(())
    }

    #[test]
    fn growth_history_ages_out_peaks_but_never_learns_a_starved_windows_echo()
        -> Result<(), Box<dyn std::error::Error>>
    {
        let reg = WorkingSetRegistry::new();
        let mind = p(205);
        reg.record_sent_in_memory(mind, 339_156, 1);
        for turn in 0..RECENT_TURNS {
            let sample = WorkingSetRegistry::sent_sample_for_plan(56_057, 828, 2_048);
            reg.record_sent_in_memory(mind, sample, turn as u64 + 2);
        }
        assert_eq!(reg.sent_median_of(&[mind]), Some(56_057));
        let Some(demand) = reg.demand_of(mind) else { return Err("missing demand".into()); };
        assert_eq!(demand.sent_peak, 339_156);
        let restarted = WorkingSetRegistry::new();
        restarted.observed.insert(mind, serde_json::from_str(&serde_json::to_string(&demand)?)?);
        assert_eq!(restarted.sent_median_of(&[mind]), Some(56_057));
        for turn in 0..RECENT_TURNS {
            let held = WorkingSetRegistry::sent_sample_for_plan(30_000, 25_000, 64_000);
            restarted.record_sent_in_memory(mind, held, turn as u64 + 20);
        }
        assert_eq!(restarted.sent_median_of(&[mind]), Some(25_000));
        assert_eq!(restarted.sent_median_with(&[mind], &[60_000, 62_000]), Some(60_000));
        Ok(())
    }
