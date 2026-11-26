#![cfg(test)]
use super::*;
use crate::commands::training_trigger::test_support::ex;

fn row(id: Uuid, learned: u32, examples: usize) -> Arrival {
    Arrival {
        id,
        sequence: 1,
        key: BucketKey { persona_id: Uuid::nil(), trait_kind: "geography".into(), base_model: "qwen".into() },
        persona_name: "fixture".into(),
        examples: (0..examples).map(|i| ex(&format!("q{i}"), &format!("a{i}"))).collect(),
        learned,
        lora: None,
        schedule: None,
    }
}

// what this catches: a seat told a lesson its bucket already holds (in the journal, in the newest
// committed generation, or learned this seat and still volatile), so a lesson is learned twice, or
// a later submission's lesson told while an earlier one still waits.
#[test]
fn the_next_lesson_is_past_the_journal_the_generation_and_the_seat() {
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let rows = vec![row(a, 1, 3), row(b, 0, 2)];
    let none = HashMap::new();
    let (position, _) = next_lesson(&rows, &none, &none).expect("test: a waits");
    assert_eq!(position, Position { submission: a, learned: 1 }, "past the journal's cursor");

    let checkpointed = HashMap::from([(a, 2)]);
    assert_eq!(next_lesson(&rows, &checkpointed, &none).map(|(p, _)| p), Some(Position { submission: a, learned: 2 }), "past the generation");

    let progress = HashMap::from([(a, 3)]);
    assert_eq!(next_lesson(&rows, &checkpointed, &progress).map(|(p, _)| p), Some(Position { submission: b, learned: 0 }), "a is done this seat: b is next");

    let progress = HashMap::from([(a, 3), (b, 2)]);
    assert!(next_lesson(&rows, &checkpointed, &progress).is_none(), "nothing left");
}

mod whole_loop {
    use super::*;
    use crate::inference::learning::{GenerationOutcome, Learning, MemoryRole, MemoryRow, OptimizerStart, SeatState, StepOutcome, Unavailable};
    use std::num::{NonZeroU32, NonZeroU64};

    /// An engine that keeps every command it is told, in order, and takes it.
    #[derive(Default)]
    struct Kept(parking_lot::Mutex<Vec<Learning>>);

    impl LearnPort for Kept {
        fn tell(&self, learning: Learning) -> Result<(), Unavailable> {
            self.0.lock().push(learning);
            Ok(())
        }
    }

    impl Kept {
        /// The kinds told so far, by name.
        fn kinds(&self) -> Vec<&'static str> {
            self.0.lock().iter().map(|told| match told {
                Learning::Measure(_) => "measure",
                Learning::Load { .. } => "load",
                Learning::Seat(_) => "seat",
                Learning::Learn(_) => "learn",
                Learning::Checkpoint { .. } => "checkpoint",
                Learning::Release { .. } => "release",
            }).collect()
        }

        fn last_step(&self) -> crate::inference::learning::StepId {
            match self.0.lock().last() {
                Some(Learning::Learn(learn)) => learn.step,
                other => panic!("test: the last command is a lesson, not {:?}", other.map(|_| "another")),
            }
        }

        fn last_into(&self) -> PathBuf {
            match self.0.lock().last() {
                Some(Learning::Checkpoint { into, .. }) => into.clone(),
                _ => panic!("test: the last command is a checkpoint"),
            }
        }
    }

    fn rows(role: MemoryRole) -> Vec<MemoryRow> {
        vec![MemoryRow { device: "CUDA0".into(), buffer_type: "CUDA0".into(), role, bytes: 1 << 20, host: false }]
    }

    // what this catches: the trainer's loop on the engine broken anywhere between a pass and the
    // journal: a seat not measured for her lesson, a lease step skipped, a lesson told twice or past
    // the last, no checkpoint once nothing waits, a Saved that writes no manifest or does not move the
    // journal's cursor, or a seat never released (its leases held forever). Two pending examples go
    // through Measure, Load, Seat, two lessons, a checkpoint and a release, with the engine's answers
    // played by the test, and the journal then holds both learned.
    #[tokio::test]
    async fn two_lessons_are_learned_saved_committed_and_the_seat_let_go() {
        let home = tempfile::tempdir().expect("test: home"); // test fixture
        let _home = crate::paths::test_home::HomeGuard::set(home.path()).await;
        continuum_kernel::system_resources::disk_reporters::prepare_writer_fixture().await; // the engine-train class, as boot registers it
        let (trigger, executor, _dir) = crate::commands::training_trigger::test_support::build_runtime_trigger_only().await;
        let state = trigger.state.clone();
        let persona = Uuid::new_v4();
        let params = serde_json::json!({
            "personaId": persona, "personaName": "loop-fixture", "baseModel": "qwen", "traitKind": "geography",
            "examples": [ex("What is the capital of Kansas?", "Topeka."), ex("What is the capital of Ohio?", "Columbus.")], "source": "operator_curated",
        });
        let answer = executor.execute_json("genome/training-trigger/submit", params).await.expect("test: submit routes"); // test fixture
        assert_eq!(answer["success"], true, "test: accepted: {answer}");
        let key = BucketKey { persona_id: persona, trait_kind: "geography".into(), base_model: "qwen".into() };

        let kept = Arc::new(Kept::default());
        let port: Arc<dyn LearnPort> = kept.clone();
        state.seat_bucket(&port, &key).await.expect("test: the bucket seats");
        assert_eq!(kept.kinds(), ["measure"], "a pass measures one seat for her bucket");
        let seat = match kept.0.lock().first() {
            Some(Learning::Measure(measure)) => {
                assert_eq!(measure.mind, PersonaId(persona));
                assert!(matches!(measure.shadow, ShadowSource::Fresh(_)), "no generation yet: a fresh shadow");
                measure.seat
            }
            _ => panic!("test: measured"),
        };
        assert!(state.seated_on_engine(&key));

        let say = |state_: SeatState| Said::Seat { seat, state: state_ };
        state.on_said(&port, say(SeatState::Measured { weights: rows(MemoryRole::Weights) })).await;
        assert_eq!(kept.kinds().last(), Some(&"load"), "its weights leased, it loads");
        state.on_said(&port, say(SeatState::Loaded { optimizer: rows(MemoryRole::OptimizerStatic), workspace: rows(MemoryRole::SharedComputeRequired), reply_rows: NonZeroU32::MIN.saturating_add(511), lesson_rows: NonZeroU32::MIN.saturating_add(511) })).await;
        assert_eq!(kept.kinds().last(), Some(&"seat"), "its optimizer and workspace leased, it seats");
        state.on_said(&port, say(SeatState::Seated { optimizer: OptimizerStart::Cold })).await;
        for lesson in 0..2 {
            assert_eq!(kept.kinds().last(), Some(&"learn"), "lesson {lesson} is told");
            let step = kept.last_step();
            state.on_said(&port, Said::Step { seat, step, outcome: StepOutcome::Learned { steps: NonZeroU64::MIN, loss: Some(1.0) } }).await;
        }
        assert_eq!(kept.kinds().last(), Some(&"checkpoint"), "nothing waits: what she learned is saved");
        let into = kept.last_into();
        state.on_said(&port, Said::Generation { seat, outcome: GenerationOutcome::Saved { into: into.clone(), steps: 2 } }).await;
        assert!(into.join("manifest.json").is_file(), "the generation's manifest is written beside it");
        assert_eq!(state.bucket_example_count(persona, "geography", "qwen").await, None, "the journal holds both learned");
        assert_eq!(kept.kinds().last(), Some(&"release"), "committed and nothing waits: the seat is let go");
        assert!(state.seated_on_engine(&key), "its leases stay held until the engine says Released");
        state.on_said(&port, say(SeatState::Released)).await;
        assert!(!state.seated_on_engine(&key), "released: nothing of the seat remains");
        assert_eq!(kept.kinds(), ["measure", "load", "seat", "learn", "learn", "checkpoint", "release"]);
    }

    // what this catches (step 3 part 6): a learning seat that holds its memory while a higher wait is short
    // of it, or that gives it back by aborting her lesson. Asked by the law, the seat lets the lesson in
    // flight land, saves and commits what she learned, and is released; the wait is granted on that
    // release, and the lesson she did not reach stays pending for her next seat.
    #[tokio::test]
    async fn a_seat_asked_to_give_back_saves_what_she_learned_and_its_release_grants_the_wait() {
        let home = tempfile::tempdir().expect("test: home"); // test fixture
        let _home = crate::paths::test_home::HomeGuard::set(home.path()).await;
        continuum_kernel::system_resources::disk_reporters::prepare_writer_fixture().await;
        let (trigger, executor, _dir) = crate::commands::training_trigger::test_support::build_runtime_trigger_only().await;
        let state = trigger.state.clone();
        let persona = Uuid::new_v4();
        let params = serde_json::json!({
            "personaId": persona, "personaName": "give-back-fixture", "baseModel": "qwen", "traitKind": "geography",
            "examples": [ex("What is the capital of Kansas?", "Topeka."), ex("What is the capital of Ohio?", "Columbus.")], "source": "operator_curated",
        });
        let answer = executor.execute_json("genome/training-trigger/submit", params).await.expect("test: submit routes"); // test fixture
        assert_eq!(answer["success"], true, "test: accepted: {answer}");
        let key = BucketKey { persona_id: persona, trait_kind: "geography".into(), base_model: "qwen".into() };
        let kept = Arc::new(Kept::default());
        let port: Arc<dyn LearnPort> = kept.clone();
        state.seat_bucket(&port, &key).await.expect("test: the bucket seats");
        let seat = match kept.0.lock().first() {
            Some(Learning::Measure(measure)) => measure.seat,
            _ => panic!("test: measured"),
        };
        let say = |state_: SeatState| Said::Seat { seat, state: state_ };
        state.on_said(&port, say(SeatState::Measured { weights: rows(MemoryRole::Weights) })).await;
        state.on_said(&port, say(SeatState::Loaded { optimizer: rows(MemoryRole::OptimizerStatic), workspace: rows(MemoryRole::SharedComputeRequired), reply_rows: NonZeroU32::MIN, lesson_rows: NonZeroU32::MIN })).await;
        state.on_said(&port, say(SeatState::Seated { optimizer: OptimizerStart::Cold })).await;
        assert_eq!(kept.kinds().last(), Some(&"learn"), "her first lesson is in flight");

        // a turn short of what the seat holds, at a class above learning
        let wanted = (1u64 << 30) - (1 << 20);
        let ask = crate::inference::training_admission::ask_training_memory(&state.authority.resources, "turn", crate::resources::ResourceKind::Vram, wanted);
        let Ok(crate::resources::Ask::Waiting { ticket: mut turn, .. }) = ask else { panic!("test: the turn waits") };
        turn.prioritize(continuum_sage::Qos::Interactive as u8);
        assert!(turn.granted().is_none(), "nothing is taken from her lesson in flight");

        let step = kept.last_step();
        state.on_said(&port, Said::Step { seat, step, outcome: StepOutcome::Learned { steps: NonZeroU64::MIN, loss: Some(1.0) } }).await;
        assert_eq!(kept.kinds().last(), Some(&"checkpoint"), "asked to give back: what she learned is saved, her next lesson waits");
        let into = kept.last_into();
        state.on_said(&port, Said::Generation { seat, outcome: GenerationOutcome::Saved { into, steps: 1 } }).await;
        assert_eq!(kept.kinds().last(), Some(&"release"), "committed: the seat is let go");
        assert_eq!(state.bucket_example_count(persona, "geography", "qwen").await, Some(1), "the lesson she did not reach stays pending");
        state.on_said(&port, say(SeatState::Released)).await;
        assert_eq!(turn.granted().map(|lease| lease.bytes()), Some(wanted), "her release grants the turn");
    }
}

mod held_off {
    use super::*;
    use crate::inference::learning::{Learning, MemoryRole, MemoryRow, SeatState, Unavailable};
    use crate::inference::serving_lifecycle::Admission;

    #[derive(Default)]
    struct Kept(parking_lot::Mutex<Vec<Learning>>);

    impl LearnPort for Kept {
        fn tell(&self, learning: Learning) -> Result<(), Unavailable> {
            self.0.lock().push(learning);
            Ok(())
        }
    }

    impl Kept {
        fn told(&self) -> usize {
            self.0.lock().len()
        }

        fn measured(&self) -> SeatId {
            match self.0.lock().last() {
                Some(Learning::Measure(measure)) => measure.seat,
                _ => panic!("test: the last command is a Measure"),
            }
        }
    }

    /// Her bucket with one example waiting, on a trainer whose authority governs 1 GiB of VRAM.
    async fn waiting() -> (Arc<TrainingTriggerState>, BucketKey, tempfile::TempDir, tempfile::TempDir, crate::paths::test_home::HomeGuard) {
        let home = tempfile::tempdir().expect("test: home"); // test fixture
        let guard = crate::paths::test_home::HomeGuard::set(home.path()).await;
        let (trigger, executor, dir) = crate::commands::training_trigger::test_support::build_runtime_trigger_only().await;
        let persona = Uuid::new_v4();
        let params = serde_json::json!({
            "personaId": persona, "personaName": "held-off-fixture", "baseModel": "qwen", "traitKind": "geography",
            "examples": [ex("What is the capital of Kansas?", "Topeka.")], "source": "operator_curated",
        });
        let answer = executor.execute_json("genome/training-trigger/submit", params).await.expect("test: submit routes"); // test fixture
        assert_eq!(answer["success"], true, "test: accepted: {answer}");
        (trigger.state.clone(), BucketKey { persona_id: persona, trait_kind: "geography".into(), base_model: "qwen".into() }, home, dir, guard)
    }

    /// Seat `key` on `kept` and have the engine measure its weights at `bytes`: the authority will
    /// not lease them, so the seat is told to go. Its Released has not arrived yet.
    async fn refusing(state: &Arc<TrainingTriggerState>, key: &BucketKey, kept: &Arc<Kept>, bytes: u64) -> SeatId {
        let port: Arc<dyn LearnPort> = kept.clone();
        state.seat_bucket(&port, key).await.expect("test: the bucket seats");
        let seat = kept.measured();
        let weights = vec![MemoryRow { device: "CUDA0".into(), buffer_type: "CUDA0".into(), role: MemoryRole::Weights, bytes, host: false }];
        state.on_said(&port, Said::Seat { seat, state: SeatState::Measured { weights } }).await;
        assert!(matches!(kept.0.lock().last(), Some(Learning::Release { .. })), "refused, the seat is let go");
        seat
    }

    /// [`refusing`], then the engine says Released: what a pass does when the authority will not
    /// lease what was measured.
    async fn refused(state: &Arc<TrainingTriggerState>, key: &BucketKey, kept: &Arc<Kept>, bytes: u64) {
        let port: Arc<dyn LearnPort> = kept.clone();
        let seat = refusing(state, key, kept, bytes).await;
        state.on_said(&port, Said::Seat { seat, state: SeatState::Released }).await;
        assert!(!state.seated_on_engine(key));
    }

    // what this catches (the 5090, 2026-11-24): a fresh shadow measured at 428,581,700,665,600 VRAM
    // bytes, more than the whole device, read as "waiting for memory", so every wake measured it
    // again through the engine's whole pre-lease path. A need past the device holds the bucket off
    // on that engine; another engine measures it again.
    #[tokio::test]
    async fn a_need_past_the_whole_device_is_not_measured_again_on_that_engine() {
        let (state, key, _home, _dir, _guard) = waiting().await;
        let kept = Arc::new(Kept::default());
        refused(&state, &key, &kept, 428_581_700_665_600).await;
        let told = kept.told();
        let port: Arc<dyn LearnPort> = kept.clone();
        state.seat_bucket(&port, &key).await.expect("test: the pass ends cleanly");
        assert_eq!(kept.told(), told, "the same engine is not asked to measure it again");

        let another = Arc::new(Kept::default());
        let replaced: Arc<dyn LearnPort> = another.clone();
        state.seat_bucket(&replaced, &key).await.expect("test: the bucket seats");
        assert_eq!(another.told(), 1, "another engine measures it again");
        another.measured();
    }

    // what this catches: a bucket the authority refused for memory measured again on every wake
    // while nothing was freed. It waits until the board shows what it needs free, then is measured.
    #[tokio::test]
    async fn a_bucket_short_of_memory_is_measured_again_once_that_much_is_free() {
        let (state, key, _home, _dir, _guard) = waiting().await;
        let occupied = occupy(&state);
        let kept = Arc::new(Kept::default());
        refused(&state, &key, &kept, 512 << 20).await;
        let told = kept.told();
        let port: Arc<dyn LearnPort> = kept.clone();
        state.seat_bucket(&port, &key).await.expect("test: the pass ends cleanly");
        assert_eq!(kept.told(), told, "nothing was freed: not measured again");

        drop(occupied);
        state.seat_bucket(&port, &key).await.expect("test: the bucket seats");
        assert_eq!(kept.told(), told + 1, "that much is free: measured again");
        kept.measured();
    }

    /// The occupier that keeps 900 MiB of the trainer's 1 GiB VRAM: a 512 MiB need is refused.
    fn occupy(state: &Arc<TrainingTriggerState>) -> crate::resources::LeaseGuard {
        crate::inference::training_admission::try_training_memory(&state.authority.resources, Admission::Free(&state.authority.serving), "occupier", crate::resources::ResourceKind::Vram, 900 << 20)
            .expect("test: the authority answers")
            .expect("test: 900 MiB of the 1 GiB is granted")
    }

    // what this catches (Codex on 9d856843): a bucket held for memory that only an arrival or a
    // serving change would ever seat again. A release alone, said by the authority, seats it; no
    // pass runs.
    #[tokio::test]
    async fn a_release_alone_seats_a_bucket_held_for_memory() {
        let (state, key, _home, _dir, _guard) = waiting().await;
        let occupied = occupy(&state);
        let kept = Arc::new(Kept::default());
        refused(&state, &key, &kept, 512 << 20).await;
        let told = kept.told();
        let port: Arc<dyn LearnPort> = kept.clone();
        state.on_free(&port, "qwen").await;
        assert_eq!(kept.told(), told, "nothing freed yet: the wake measures nothing");

        drop(occupied);
        state.on_free(&port, "qwen").await;
        assert_eq!(kept.told(), told + 1, "the release alone seats it again");
        kept.measured();
    }

    // what this catches (the 5090, 2026-11-26: 97 Loaded, 0 Seated in four minutes): a seat whose
    // optimizer was refused was held for that lease alone, so its own weights, freed by its release,
    // covered the hold, and it was measured, loaded and refused again in a loop. It waits until the
    // whole seat (weights, optimizer and workspace) fits.
    #[tokio::test]
    async fn a_seat_refused_its_optimizer_waits_for_the_whole_seat_not_its_own_freed_weights() {
        use std::num::NonZeroU32;
        let (state, key, _home, _dir, _guard) = waiting().await;
        let occupied = crate::inference::training_admission::try_training_memory(&state.authority.resources, Admission::Free(&state.authority.serving), "occupier", crate::resources::ResourceKind::Vram, 600 << 20)
            .expect("test: the authority answers")
            .expect("test: 600 MiB of the 1 GiB is granted");
        let kept = Arc::new(Kept::default());
        let port: Arc<dyn LearnPort> = kept.clone();
        state.seat_bucket(&port, &key).await.expect("test: the bucket seats");
        let seat = kept.measured();
        let vram = |role, bytes| vec![MemoryRow { device: "CUDA0".into(), buffer_type: "CUDA0".into(), role, bytes, host: false }];
        state.on_said(&port, Said::Seat { seat, state: SeatState::Measured { weights: vram(MemoryRole::Weights, 300 << 20) } }).await;
        assert!(matches!(kept.0.lock().last(), Some(Learning::Load { .. })), "its weights fit: it loads");
        // what this catches: a lesson sized past the governed device (the 5090's 1.9 TB lesson, held
        // as a defect forever). The engine is told the room beside her weights: 1 GiB less 300 MiB.
        assert!(
            matches!(kept.0.lock().last(), Some(Learning::Load { fit, .. }) if fit.vram == (1 << 30) - (300 << 20)),
            "the load carries what the device governs beside the seat's weights"
        );
        let loaded = SeatState::Loaded { optimizer: vram(MemoryRole::OptimizerStatic, 200 << 20), workspace: vram(MemoryRole::SharedComputeRequired, 1 << 20), reply_rows: NonZeroU32::MIN, lesson_rows: NonZeroU32::MIN };
        state.on_said(&port, Said::Seat { seat, state: loaded }).await;
        assert!(matches!(kept.0.lock().last(), Some(Learning::Release { .. })), "its optimizer refused: the seat is let go");
        let told = kept.told();
        state.on_said(&port, Said::Seat { seat, state: SeatState::Released }).await;
        assert_eq!(kept.told(), told, "its own freed weights do not seat it again: the whole seat does not fit");

        drop(occupied);
        state.on_free(&port, "qwen").await;
        assert_eq!(kept.told(), told + 1, "the whole seat fits: measured again");
        kept.measured();
    }

    // what this catches (Codex on 9d856843): a release said while the refused seat was still being
    // let go. Its wake finds the bucket still seated and seats nothing, and no later change ever
    // comes, so the bucket would wait forever. The seat's own Released sees the memory free and
    // seats it again.
    #[tokio::test]
    async fn a_release_before_the_refused_seat_is_let_go_seats_it_on_its_released() {
        let (state, key, _home, _dir, _guard) = waiting().await;
        let occupied = occupy(&state);
        let kept = Arc::new(Kept::default());
        let port: Arc<dyn LearnPort> = kept.clone();
        let seat = refusing(&state, &key, &kept, 512 << 20).await;
        drop(occupied);
        let told = kept.told();
        state.on_free(&port, "qwen").await;
        assert_eq!(kept.told(), told, "still being let go: the wake seats nothing");

        state.on_said(&port, Said::Seat { seat, state: SeatState::Released }).await;
        assert_eq!(kept.told(), told + 1, "its Released finds the memory free and seats it again");
        kept.measured();
    }

    // what this catches (Codex's contract for the trainer, 2026-11-26): serving holds the device, a seat
    // refused its optimizer asks for its whole seat and waits; the release grants that ask at once (a
    // competing taker finds nothing), and the seat is measured, loaded and seated on stages carved from the
    // grant, with no second ask and nothing of the reserve left over.
    #[tokio::test]
    async fn a_seat_waits_on_its_ask_and_seats_on_the_carved_grant_without_asking_again() {
        use std::num::NonZeroU32;
        let (state, key, _home, _dir, _guard) = waiting().await;
        let free_vram = |state: &Arc<TrainingTriggerState>| state.authority.resources.board().kinds.iter().find(|k| k.kind == crate::resources::ResourceKind::Vram).map(|k| k.available_bytes);
        let occupied = crate::inference::training_admission::try_training_memory(&state.authority.resources, Admission::Free(&state.authority.serving), "occupier", crate::resources::ResourceKind::Vram, 600 << 20)
            .expect("test: the authority answers")
            .expect("test: 600 MiB of the 1 GiB is granted");
        let kept = Arc::new(Kept::default());
        let port: Arc<dyn LearnPort> = kept.clone();
        state.seat_bucket(&port, &key).await.expect("test: the bucket seats");
        let seat = kept.measured();
        let vram = |role, bytes| vec![MemoryRow { device: "CUDA0".into(), buffer_type: "CUDA0".into(), role, bytes, host: false }];
        let measured = SeatState::Measured { weights: vram(MemoryRole::Weights, 300 << 20) };
        let loaded = || SeatState::Loaded { optimizer: vram(MemoryRole::OptimizerStatic, 200 << 20), workspace: vram(MemoryRole::SharedComputeRequired, 1 << 20), reply_rows: NonZeroU32::MIN, lesson_rows: NonZeroU32::MIN };
        state.on_said(&port, Said::Seat { seat, state: measured.clone() }).await;
        state.on_said(&port, Said::Seat { seat, state: loaded() }).await;
        assert!(matches!(kept.0.lock().last(), Some(Learning::Release { .. })), "its optimizer refused: let go, its whole seat asked for");
        state.on_said(&port, Said::Seat { seat, state: SeatState::Released }).await;

        drop(occupied);
        let competing = crate::inference::training_admission::try_training_memory(&state.authority.resources, Admission::Free(&state.authority.serving), "competitor", crate::resources::ResourceKind::Vram, 600 << 20)
            .expect("test: the authority answers");
        assert!(competing.is_err(), "the release granted the seat's ask at once: a competing taker finds nothing");
        assert_eq!(free_vram(&state), Some((1 << 30) - (501 << 20)), "the whole seat (300 + 200 + 1 MiB) is the seat's reserve");
        // another taker holds everything free beside the reserve: only the reserve can seat her now
        let rest = crate::inference::training_admission::try_training_memory(&state.authority.resources, Admission::Free(&state.authority.serving), "competitor", crate::resources::ResourceKind::Vram, (1 << 30) - (501 << 20))
            .expect("test: the authority answers")
            .expect("test: what is free beside the reserve is granted");

        let told = kept.told();
        state.on_free(&port, "qwen").await;
        assert_eq!(kept.told(), told + 1, "its grant seats it again");
        let seat = kept.measured();
        state.on_said(&port, Said::Seat { seat, state: measured }).await;
        assert!(matches!(kept.0.lock().last(), Some(Learning::Load { .. })), "its weights are carved from the reserve: it loads");
        state.on_said(&port, Said::Seat { seat, state: loaded() }).await;
        assert!(matches!(kept.0.lock().last(), Some(Learning::Seat(_))), "its optimizer and workspace are carved too: it seats");
        assert_eq!(free_vram(&state), Some(0), "every stage spent the reserve, nothing more was asked and nothing is left over");
        drop(rest);
    }

    // what this catches: a granted reserve kept for a bucket whose pass seats nothing (nothing pending, not
    // now, a refusal): held from every other consumer with nothing to spend it on, for as long as the
    // trainer runs.
    #[tokio::test]
    async fn a_pass_that_seats_nothing_gives_its_reserve_back() {
        let (state, _key, _home, _dir, _guard) = waiting().await;
        let free_vram = |state: &Arc<TrainingTriggerState>| state.authority.resources.board().kinds.iter().find(|k| k.kind == crate::resources::ResourceKind::Vram).map(|k| k.available_bytes);
        let before = free_vram(&state);
        let empty = BucketKey { persona_id: Uuid::new_v4(), trait_kind: "nothing-pending".into(), base_model: "qwen".into() };
        let reserve = crate::inference::training_admission::try_training_memory(&state.authority.resources, Admission::Free(&state.authority.serving), "learning:reserve", crate::resources::ResourceKind::Vram, 100 << 20)
            .expect("test: the authority answers")
            .expect("test: 100 MiB is granted");
        let held_for = empty.clone();
        state.on_engine.reserves.lock().insert(held_for, vec![reserve]);
        assert_ne!(free_vram(&state), before, "the reserve is held");
        let kept = Arc::new(Kept::default());
        let port: Arc<dyn LearnPort> = kept.clone();
        state.seat_bucket(&port, &empty).await.expect("test: the pass ends cleanly");
        assert_eq!(free_vram(&state), before, "nothing pending: the reserve is given back");
    }
}
