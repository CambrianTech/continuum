#![cfg(test)]
    use super::*;

    // what this catches (card 6f3218ed): the act bound's extension leaking to an act with no
    // pinned generation. Without a slot to read, the deadline ends the act at the bound,
    // exactly as the plain timeout did; a step that finishes first returns its output.
    #[tokio::test(start_paused = true)]
    async fn an_act_with_no_pinned_slot_ends_at_its_bound_as_before() {
        let bound = std::time::Duration::from_secs(1500);
        let start = tokio::time::Instant::now();
        let never = super::until_act_stalls(start + bound, bound, None, std::future::pending::<()>()).await;
        assert!(never.is_err());
        assert_eq!(start.elapsed(), bound, "no extension without a pinned slot");
        let unpinned = uuid::Uuid::new_v4();
        let quick = super::until_act_stalls(start + bound * 2, bound, Some(unpinned), async { 42 }).await;
        assert_eq!(quick, Ok(42));
    }

    // what this catches: a turn iteration that never reached the model being counted as
    // an act. M5, 2026-09-20 ~21:55Z, build 0047d521b, 4 residents on 2 lanes at 67,072:
    // two `persona.act.pace` rows read `model_ms=0` with `residue_ms` equal to the WHOLE
    // act — 365,000 and 1,500,001 — while `delib.gate.lane_wait` read
    // `lanes_available=0`. Those were waits. A regression that classified them as acts
    // is what made the hour line say "8 acts, 0 writes".
    #[test]
    fn an_iteration_that_never_reached_the_model_is_a_wait_not_an_act() {
        // The two measured starved rows: no model time, no generation dispatched.
        assert_eq!(classify_act(0, 0), ActClass::LaneStarved, "365,000 ms residue, model_ms=0");
        // A real act: the model answered.
        assert_eq!(classify_act(18_400, 1), ActClass::Act);
        // A FAULTED generation still pushed a receipt — it reached the lane, so it is an
        // act that failed, never a starve. Misreading it as a starve would hide the very
        // inference faults `settle.inference_failed` exists to surface.
        assert_eq!(classify_act(0, 1), ActClass::Act);
        assert!(can_defer_input(0, 0, 0, 0));
        assert!(!can_defer_input(1, 0, 0, 0), "a dispatched generation is not replayable input");
        assert!(!can_defer_input(0, 1, 1, 0), "completed actions survive without replay");
        assert!(!can_defer_input(0, 0, 0, 1), "a prior transient fault must not disappear when the next step starves");
    }

    // what this catches: a lane wait advancing the pace cadence it is then judged
    // against — the measurement that can only rise. The M5 hour's rows were act_secs
    // 164, 249, 365, 866, 920, 1411, 1500 with rolling_mean_secs 693–1018; the 365 and
    // 1500 rows were waits, and folding them in inflated the mean every act after them.
    #[test]
    fn a_starved_turn_leaves_the_pace_cadence_where_it_was() {
        let mut pace = PaceCadence::default();
        assert!(pace.record(164.0, ActClass::Act));
        assert!(pace.record(249.0, ActClass::Act));
        let before = (pace.samples(), pace.mean_secs());
        // The 6-minute wait and the 25-minute one: neither is a sample.
        assert!(!pace.record(365.0, ActClass::LaneStarved));
        assert!(!pace.record(1500.0, ActClass::LaneStarved));
        assert_eq!(
            (pace.samples(), pace.mean_secs()),
            before,
            "a wait advances neither the sample count nor the mean"
        );
        // A real act does advance it.
        assert!(pace.record(866.0, ActClass::Act));
        assert_eq!(pace.samples(), 3);
        assert!(
            (pace.mean_secs() - (164.0 + 249.0 + 866.0) / 3.0).abs() < 1e-9,
            "the mean stands on acts only: {}",
            pace.mean_secs()
        );
    }

    // what this catches: "slow" being called on fewer than four real acts, which is how
    // a turn whose first iterations were lane waits would have been judged slow against
    // a mean built out of waiting. Same rule as the original `acts >= 3`, written in the
    // cadence's own unit.
    #[test]
    fn slow_needs_four_act_samples_never_a_wait() {
        let mut pace = PaceCadence::default();
        for _ in 0..6 {
            pace.record(1500.0, ActClass::LaneStarved);
        }
        assert_eq!(pace.samples(), 0);
        assert!(!pace.is_slow(1500.0), "six waits are not four acts");
        for _ in 0..4 {
            pace.record(10.0, ActClass::Act);
        }
        assert!(pace.is_slow(60.0), "6x the turn's own mean, on four real samples");
        assert!(!pace.is_slow(15.0), "under 2x the mean is not slow");
    }
