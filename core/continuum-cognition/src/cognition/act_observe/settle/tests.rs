#![cfg(test)]
    use super::*;
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

    // what this catches (card 843d5570): a generation the deadline drops in flight read as a
    // lane wait. enrico, 2026-10-26 (core 839f70216): Eitan's generation ran on slot 1, was
    // extended at 20:43:18 and 21:08:18, and ended at the ceiling at 21:33:18; the drop left
    // no receipt, so the act was labelled `lane_starved`. The deadline's answer must say the
    // step had dispatched, and a step that never dispatched must say it had not.
    #[tokio::test(start_paused = true)]
    async fn a_deadline_says_whether_it_dropped_a_dispatched_generation() {
        let bound = std::time::Duration::from_secs(1500);
        let start = tokio::time::Instant::now();
        let dispatched = super::until_act_stalls(start + bound, bound, None, async {
            let _witness = crate::cognition::generation_drop::InFlight::arm("Eitan", None, None);
            std::future::pending::<()>().await
        })
        .await;
        assert_eq!(dispatched, Err(super::Abandoned { dispatched: true }));
        let waited = super::until_act_stalls(start + bound * 2, bound, None, std::future::pending::<()>()).await;
        assert_eq!(waited, Err(super::Abandoned { dispatched: false }), "a step that never armed is a wait");
    }

    // what this catches (Fable on #5498): a generation the act deadline dropped in flight
    // spending the transient-fault retries, each another whole bound on the same engine (one
    // turn held up to four bounds). It surrenders at the first bound as a failed act: an
    // inference error, no completion, nothing deferred.
    #[tokio::test(start_paused = true)]
    async fn a_deadline_dropped_generation_surrenders_the_turn_without_a_retry() {
        use crate::cognition::conversation::scripted_conversation::ScriptedConversation;
        use crate::cognition::conversation::PersonaConversation;
        use crate::cognition::workspace::{
            Burst, Contribution, Faculty, FacultyId, SalienceArbiter, TurnAttention, TurnFraming, Workspace, WorkspaceCycle,
        };
        struct Hangs;
        #[async_trait::async_trait]
        impl Faculty for Hangs {
            fn id(&self) -> FacultyId {
                FacultyId::Deliberation
            }
            fn reacts_to_broadcast(&self) -> bool {
                true
            }
            async fn contribute(&self, _ws: &Workspace) -> Option<Contribution> {
                let _witness = crate::cognition::generation_drop::InFlight::arm("Eitan", None, None);
                std::future::pending().await
            }
        }
        let cycle = WorkspaceCycle::new(vec![Arc::new(Hangs) as Arc<dyn Faculty>], Arc::new(SalienceArbiter));
        let mut conversation = ScriptedConversation::new();
        conversation.prime().await.unwrap(); // unwrap: test fixture
        let room = crate::identity::ActivityRoom::from_uuid(uuid::Uuid::new_v4()).unwrap(); // unwrap: a fresh uuid is a valid room
        let start = tokio::time::Instant::now();
        let settled = crate::cognition::act_observe::drive_to_settle_with_input(
            &cycle,
            Burst::raw_in(room, "a line".into()),
            8,
            TurnFraming::message(TurnAttention::default()),
            &mut conversation,
        )
        .await;
        assert!(settled.inference_error.is_some(), "a failed act");
        assert!(!settled.deferred_before_dispatch, "never a lane wait");
        assert!(settled.generation_receipts.is_empty(), "no completion");
        assert_eq!(start.elapsed(), crate::inference::turn_bound::TICK_DEADLINE, "one bound, no in-place retry");
    }

    // what this catches (card 4d2a64a2): an act on a condemned engine holding the relaunch's
    // drain until its next checkpoint. enrico, 2026-10-26: the quarantine flipped serving
    // not-ready at 21:10Z, and `ensure_serving` waited 23 min for an act whose next
    // checkpoint was 21:33:18. Condemned mid-bound, the act ends at that instant, saying
    // whether its generation had been dispatched.
    #[tokio::test(start_paused = true)]
    async fn a_condemned_engine_ends_the_act_at_once_not_at_its_checkpoint() {
        let bound = std::time::Duration::from_secs(1500);
        let start = tokio::time::Instant::now();
        let condemned = async { tokio::time::sleep(std::time::Duration::from_secs(120)).await };
        let ended = super::until_act_stalls_or(start + bound, bound, None, async {
            let _witness = crate::cognition::generation_drop::InFlight::arm("Eitan", None, None);
            std::future::pending::<()>().await
        }, condemned)
        .await;
        assert_eq!(ended, Err(super::Abandoned { dispatched: true }));
        assert_eq!(start.elapsed(), std::time::Duration::from_secs(120), "at the condemnation, not the 1500 s checkpoint");
        let quick = super::until_act_stalls_or(start + bound * 2, bound, None, async { 7 }, std::future::pending::<()>()).await;
        assert_eq!(quick, Ok(7), "no condemnation: the step's own answer");
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
