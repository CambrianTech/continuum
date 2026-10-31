#![cfg(test)]
    use super::*;
    use crate::cognition::llm_deliberation_faculty::admission_clock;
    use crate::inference::resource_admission::{LaneAdmission, LanePriority};

    // Inject only the engine observation; acquisition and watchdog remain real.
    async fn healthy_wait<T>(acquire: impl std::future::Future<Output=T>) -> T {
        let mut tokens = 0;
        admission_clock::waiting_with_probe(acquire, move || {
            tokens += 1;
            std::future::ready(admission_clock::QueueProgress {
                grants: (0, 0),
                holders: vec![(1, crate::inference::llama_server::InFlightSlot { root: "fixture".into(), slot: 0 }, Some(crate::inference::llama_server::SlotWork { task: 1, processing: true, work: tokens }))],
            })
        }).await
    }

    #[tokio::test(start_paused = true)]
    async fn never_granted_queue_without_progress_ends_and_releases_entry() -> Result<(), Box<dyn std::error::Error>> {
        let gate: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
        gate.set_served_lane_count(2);
        let held = gate.acquire_serving_lane(None, LanePriority::Work, None).await;
        let (wait, acquire) = gate.serving_lane_request(None, LanePriority::Work, None);
        let bound = std::time::Duration::from_secs(10);
        let start = tokio::time::Instant::now();
        let turn = tokio::spawn(super::until_act_stalls(start + bound, bound, None, admission_clock::waiting(wait, acquire)));
        queued(gate, 1).await;
        assert!(turn.await?.is_err(), "a held permit without progress cannot park forever");
        assert_eq!(start.elapsed(), bound);
        assert_eq!(gate.work_waiting(), 0);
        drop(held);
        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn queue_that_stops_advancing_ends_at_the_next_checkpoint() {
        let bound = std::time::Duration::from_secs(10);
        let start = tokio::time::Instant::now();
        let mut samples = 0;
        let waiting = admission_clock::waiting_with_probe(std::future::pending::<()>(), move || {
            samples += 1;
            std::future::ready(admission_clock::QueueProgress { grants: (0, samples.min(2)), holders: vec![] })
        });
        assert!(super::until_act_stalls(start + bound, bound, None, waiting).await.is_err());
        assert_eq!(start.elapsed(), bound * 2, "one grant earns one checkpoint, not a permanent exemption");
    }

    #[tokio::test(start_paused = true)]
    async fn processing_without_more_tokens_is_not_queue_progress() {
        let bound = std::time::Duration::from_secs(10);
        let start = tokio::time::Instant::now();
        let waiting = admission_clock::waiting_with_probe(std::future::pending::<()>(), || std::future::ready(admission_clock::QueueProgress {
            grants: (0, 0),
            holders: vec![(1, crate::inference::llama_server::InFlightSlot { root: "fixture".into(), slot: 0 }, Some(crate::inference::llama_server::SlotWork { task: 1, processing: true, work: 42 }))],
        }));
        assert!(super::until_act_stalls(start + bound, bound, None, waiting).await.is_err());
        assert_eq!(start.elapsed(), bound, "a processing flag alone cannot renew the wait");
    }

    #[tokio::test(start_paused = true)]
    async fn late_queue_registration_and_completion_cannot_revive_expired_budget() {
        let bound = std::time::Duration::from_secs(10);
        let expired = tokio::time::Instant::now() - bound;
        let step = async {
            healthy_wait(std::future::ready(())).await;
            std::future::pending::<()>().await;
        };
        let start = tokio::time::Instant::now();
        assert!(super::until_act_stalls(expired, bound, None, step).await.is_err());
        assert_eq!(start.elapsed(), std::time::Duration::ZERO);
    }

    async fn queued(gate: &LaneAdmission, count: usize) {
        for _ in 0..100 {
            if gate.work_waiting() == count { return; }
            tokio::task::yield_now().await;
        }
        assert_eq!(gate.work_waiting(), count, "scheduler registered the expected waiters");
    }

    // The real scheduler's FIFO entry survives the OUTER act deadline, not just
    // the inner wait estimate. A later arrival remains behind the original mind.
    #[tokio::test(start_paused = true)]
    async fn queued_turn_survives_outer_deadline_without_losing_fifo() -> Result<(), Box<dyn std::error::Error>> {
        let gate: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
        gate.set_served_lane_count(2);
        let held = gate.acquire_serving_lane(None, LanePriority::Work, None).await;
        let bound = std::time::Duration::from_secs(10);
        let first = tokio::spawn(super::until_act_stalls(
            tokio::time::Instant::now() + bound, bound, None,
            healthy_wait(gate.acquire_serving_lane(None, LanePriority::Work, None)),
        ));
        queued(gate, 1).await;
        tokio::time::advance(bound * 3).await;
        tokio::task::yield_now().await;
        assert!(!first.is_finished(), "waiting is not a stalled generation");
        let second = tokio::spawn(gate.acquire_serving_lane(None, LanePriority::Work, None));
        queued(gate, 2).await;
        drop(held);
        let result = first.await?;
        assert!(result.is_ok());
        assert!(!second.is_finished(), "later arrival cannot overtake");
        drop(result);
        drop(second.await?);
        assert_eq!(gate.work_waiting(), 0);
        Ok(())
    }

    // Queue time pauses the remaining budget; it does not reset it on grant.
    #[tokio::test(start_paused = true)]
    async fn grant_resumes_only_the_watchdogs_remaining_budget() -> Result<(), Box<dyn std::error::Error>> {
        let gate: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
        gate.set_served_lane_count(2);
        let held = gate.acquire_serving_lane(None, LanePriority::Work, None).await;
        let bound = std::time::Duration::from_secs(10);
        let start = tokio::time::Instant::now();
        let (ready, grant) = tokio::sync::oneshot::channel();
        let turn = tokio::spawn(super::until_act_stalls(start + bound, bound, None, async move {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let _permit = healthy_wait(gate.acquire_serving_lane(None, LanePriority::Work, None)).await;
            let _ = ready.send(());
            std::future::pending::<()>().await;
        }));
        tokio::task::yield_now().await;
        tokio::time::advance(std::time::Duration::from_secs(3)).await;
        queued(gate, 1).await;
        tokio::time::advance(std::time::Duration::from_secs(30)).await;
        drop(held);
        grant.await?;
        assert!(turn.await?.is_err(), "post-grant stalled generation still ends");
        assert_eq!(start.elapsed(), std::time::Duration::from_secs(40));
        Ok(())
    }

    // A genuine cancellation still drops the same scheduler entry, even after
    // the old outer deadline. The wait receipt cannot keep a detached turn alive.
    #[tokio::test(start_paused = true)]
    async fn cancelled_queued_turn_releases_its_original_entry() -> Result<(), Box<dyn std::error::Error>> {
        let gate: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
        gate.set_served_lane_count(2);
        let held = gate.acquire_serving_lane(None, LanePriority::Work, None).await;
        let bound = std::time::Duration::from_secs(10);
        let turn = tokio::spawn(super::until_act_stalls(
            tokio::time::Instant::now() + bound, bound, None,
            healthy_wait(gate.acquire_serving_lane(None, LanePriority::Work, None)),
        ));
        queued(gate, 1).await;
        tokio::time::advance(bound * 3).await;
        turn.abort();
        assert!(matches!(turn.await, Err(error) if error.is_cancelled()));
        assert_eq!(gate.work_waiting(), 0);
        drop(held);
        drop(gate.acquire_serving_lane(None, LanePriority::Work, None).await);
        Ok(())
    }

    // Directed input cancels the original entry; another step of the same persona
    // cannot borrow this step's pause (no persona-wide flag or permanent exemption).
    #[tokio::test(start_paused = true)]
    async fn directed_yield_cleans_wait_and_other_step_keeps_its_deadline() -> Result<(), Box<dyn std::error::Error>> {
        let gate: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
        gate.set_served_lane_count(2);
        let held = gate.acquire_serving_lane(None, LanePriority::Work, None).await;
        let persona = uuid::Uuid::new_v4();
        let bound = std::time::Duration::from_secs(10);
        let deadline = tokio::time::Instant::now() + bound;
        let turn = tokio::spawn(super::until_act_stalls(deadline, bound, Some(persona), async move {
            tokio::select! {
                _ = healthy_wait(gate.acquire_serving_lane(Some(persona), LanePriority::Work, None)) => false,
                _ = crate::cognition::directed_pending::wait(persona) => true,
            }
        }));
        queued(gate, 1).await;
        let other = tokio::spawn(super::until_act_stalls(deadline, bound, Some(persona), std::future::pending::<()>()));
        tokio::time::advance(bound * 3).await;
        assert!(other.await?.is_err());
        assert!(!turn.is_finished());
        crate::cognition::directed_pending::signal(persona);
        assert_eq!(turn.await?, Ok(true));
        crate::cognition::directed_pending::clear(persona);
        assert_eq!(gate.work_waiting(), 0);
        drop(held);
        Ok(())
    }

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
