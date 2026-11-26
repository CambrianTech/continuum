#![cfg(test)]
use super::*;
use crate::inference::learning::{BufferType, Optimizer, PersonaId, ShadowSource, Slot};
use crate::inference::learning::FreshShadow;
use std::num::{NonZeroU32, NonZeroU64};
use std::sync::atomic::{AtomicBool, Ordering};

/// What the fake engine was told, in order, by kind and seat.
#[derive(Debug, Clone, PartialEq)]
enum Told {
    Measure(SeatId),
    Load(SeatId),
    Seat(SeatId),
    Learn(SeatId, StepId),
    Checkpoint(SeatId, PathBuf),
    Release(SeatId),
}

/// An engine that queues what it is told, or (closed) hands every command back.
#[derive(Default)]
struct Port {
    told: parking_lot::Mutex<Vec<Told>>,
    closed: AtomicBool,
}

impl LearnPort for Port {
    fn tell(&self, learning: Learning) -> Result<(), Unavailable> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(Unavailable { returned: learning });
        }
        let told = match &learning {
            Learning::Measure(m) => Told::Measure(m.seat),
            Learning::Load { seat, .. } => Told::Load(*seat),
            Learning::Seat(s) => Told::Seat(s.seat),
            Learning::Learn(l) => Told::Learn(l.seat, l.step),
            Learning::Checkpoint { seat, into, .. } => Told::Checkpoint(*seat, into.clone()),
            Learning::Release { seat } => Told::Release(*seat),
        };
        self.told.lock().push(told);
        Ok(())
    }
}

/// A guard that says when it is dropped: the leases a returned command carried must drop with it.
struct Guard(Arc<AtomicBool>);

impl Drop for Guard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

impl OptimizerLease for Guard {
    fn admit(&self, _slot: Slot, _bytes: u64) -> bool {
        true
    }
    fn release(&self, _slot: Slot, _bytes: u64) {}
}

impl ReplyAdmission for Guard {
    fn workspace(&self, _buffer: BufferType, _required: u64, _held: u64) -> bool {
        true
    }
}

/// The guard type the tests hold: any lease, type-erased.
type G = Box<dyn Send>;

fn g(guard: Box<Guard>) -> G {
    guard
}

fn guard() -> (Box<Guard>, Arc<AtomicBool>) {
    let dropped = Arc::new(AtomicBool::new(false));
    (Box::new(Guard(Arc::clone(&dropped))), dropped)
}

fn measured(port: &Port) -> SeatRun<G> {
    let shadow = ShadowSource::Fresh(FreshShadow { name: "s.gguf".into(), rank: 8, alpha: 16.0, targets: vec!["attn_q".into()], top_layers: 4, seed: 7 });
    let optimizer = Optimizer::AdamW { learning_rate: 1e-4, beta1: 0.9, beta2: 0.999, eps: 1e-8, weight_decay: 0.0 };
    let measure = Measure { seat: SeatId(Uuid::new_v4()), mind: PersonaId(Uuid::new_v4()), shadow, optimizer, sized_for: lesson() };
    SeatRun::measure(port, measure).expect("test: the port is open")
}

/// Room for any seat: a fit the engine never sizes a lesson down for.
const ROOM: Fit = Fit { vram: u64::MAX, ram: u64::MAX };

fn loaded() -> SeatState {
    SeatState::Loaded { optimizer: Vec::new(), workspace: Vec::new(), reply_rows: NonZeroU32::MIN, lesson_rows: NonZeroU32::MIN }
}

fn lesson() -> Lesson {
    Lesson { messages: vec![serde_json::json!({"role": "assistant", "content": "Topeka."})], tools: None }
}

/// A seat measured, loaded and seated, holding its three guards.
fn seated(port: &Port) -> SeatRun<G> {
    let mut run = measured(port);
    let id = run.seat;
    assert_eq!(run.hear(port, &Said::Seat { seat: id, state: SeatState::Measured { weights: Vec::new() } }), Some(Heard::LeaseWeights(Vec::new())));
    run.load(port, g(guard().0), ROOM).expect("test: measured, so it loads");
    assert!(matches!(run.hear(port, &Said::Seat { seat: id, state: loaded() }), Some(Heard::LeaseSeat { .. })));
    run.seat(port, guard().0, g(guard().0), g(guard().0)).expect("test: loaded, so it seats");
    assert_eq!(
        run.hear(port, &Said::Seat { seat: id, state: SeatState::Seated { optimizer: OptimizerStart::Cold } }),
        Some(Heard::Seated { optimizer: OptimizerStart::Cold })
    );
    assert_eq!(run.held(), 3, "the weights, optimizer and workspace leases");
    run
}

fn learned(steps: u64, loss: f64) -> StepOutcome {
    StepOutcome::Learned { steps: NonZeroU64::new(steps).expect("test: at least one step"), loss: Some(loss) }
}

// what this catches: a trainer that moves the journal on a step's answer instead of a saved
// generation, or loses what it learned after the checkpoint it is answering. The seat's life is
// told in the engine's order, learned progress stays volatile until the Saved that holds it, and a
// lesson learned after that checkpoint stays volatile past it.
#[test]
fn progress_moves_the_journal_only_with_the_generation_that_holds_it() {
    let port = Port::default();
    let mut run = seated(&port);
    let id = run.seat;
    let submission = Uuid::new_v4();

    let step = run.learn(&port, Position { submission, learned: 0 }, lesson(), guard().0).expect("test: seated, none in flight");
    assert!(!run.ready(), "one lesson in flight at a time");
    assert_eq!(
        run.hear(&port, &Said::Step { seat: id, step, outcome: learned(3, 0.5) }),
        Some(Heard::Learned { position: Position { submission, learned: 0 }, steps: 3, loss: Some(0.5) })
    );
    assert_eq!(run.progress().get(&submission), Some(&1), "learned, volatile");

    let into = PathBuf::from("/gen/1");
    assert!(run.checkpoint(&port, PathBuf::from("/gen/1"), Arc::from("binding")).expect("test: seated"));
    let step = run.learn(&port, Position { submission, learned: 1 }, lesson(), guard().0).expect("test: seated");
    run.hear(&port, &Said::Step { seat: id, step, outcome: learned(1, 0.4) });

    assert_eq!(
        run.hear(&port, &Said::Generation { seat: id, outcome: GenerationOutcome::Saved { into: into.clone(), steps: 3 } }),
        Some(Heard::Saved { into, steps: 3, positions: vec![Position { submission, learned: 1 }] }),
        "the generation holds what was learned when it was told"
    );
    assert_eq!(run.progress().get(&submission), Some(&2), "the lesson learned after the checkpoint stays volatile");

    run.release(&port).expect("test: open");
    assert_eq!(run.hear(&port, &Said::Seat { seat: id, state: SeatState::Released }), Some(Heard::Released));
    let told: Vec<Told> = port.told.lock().clone();
    assert!(matches!(told.as_slice(), [Told::Measure(_), Told::Load(_), Told::Seat(_), Told::Learn(..), Told::Checkpoint(..), Told::Learn(..), Told::Release(_)]), "{told:?}");
}

// what this catches: a step answer for a lesson this seat did not tell (another seat's, or a stale
// step) counted as learned, or a second lesson told while one is in flight.
#[test]
fn only_the_lesson_in_flight_is_answered() {
    let port = Port::default();
    let mut run = seated(&port);
    let id = run.seat;
    let submission = Uuid::new_v4();
    let step = run.learn(&port, Position { submission, learned: 0 }, lesson(), guard().0).expect("test: seated");
    assert!(matches!(run.learn(&port, Position { submission, learned: 1 }, lesson(), guard().0), Err(NotTold::OutOfOrder(_))));

    let other = SeatId(Uuid::new_v4());
    assert_eq!(run.hear(&port, &Said::Step { seat: other, step, outcome: learned(1, 0.1) }), None, "another seat's answer");
    assert_eq!(run.hear(&port, &Said::Step { seat: id, step: StepId(Uuid::new_v4()), outcome: learned(1, 0.1) }), None, "a step it never told");
    assert!(run.progress().is_empty());
    assert_eq!(run.hear(&port, &Said::Step { seat: id, step, outcome: StepOutcome::Unchanged }), Some(Heard::Again { position: Position { submission, learned: 0 } }));
    assert!(run.ready(), "answered: the next lesson may be told");
}

// what this catches: an uncertain step, or a non-finite loss (a poisoned update), kept as progress
// or saved. Either drops everything volatile, cancels the pending checkpoints and tells Release.
#[test]
fn an_uncertain_step_drops_its_progress_and_lets_the_seat_go() {
    for outcome in [StepOutcome::Uncertain { steps: 1 }, learned(1, f64::NAN)] {
        let port = Port::default();
        let mut run = seated(&port);
        let id = run.seat;
        let submission = Uuid::new_v4();
        let step = run.learn(&port, Position { submission, learned: 0 }, lesson(), guard().0).expect("test: seated");
        run.hear(&port, &Said::Step { seat: id, step, outcome: learned(1, 0.5) });
        run.checkpoint(&port, PathBuf::from("/gen/1"), Arc::from("b")).expect("test: seated");
        let step = run.learn(&port, Position { submission, learned: 1 }, lesson(), guard().0).expect("test: seated");

        assert_eq!(run.hear(&port, &Said::Step { seat: id, step, outcome: outcome.clone() }), Some(Heard::Uncertain), "{outcome:?}");
        assert!(run.progress().is_empty(), "nothing it learned is kept");
        assert_eq!(run.hear(&port, &Said::Generation { seat: id, outcome: GenerationOutcome::Saved { into: "/gen/1".into(), steps: 1 } }), None, "its checkpoint is not committed");
        assert!(matches!(port.told.lock().last(), Some(Told::Release(seat)) if *seat == id), "Release was told");
    }
}

// what this catches: a refused checkpoint taken as saved, or its positions lost. They come back
// as Unsaved and stay volatile, so a later checkpoint publishes them.
#[test]
fn a_refused_checkpoint_keeps_its_progress_volatile() {
    let port = Port::default();
    let mut run = seated(&port);
    let id = run.seat;
    let submission = Uuid::new_v4();
    let step = run.learn(&port, Position { submission, learned: 4 }, lesson(), guard().0).expect("test: seated");
    run.hear(&port, &Said::Step { seat: id, step, outcome: learned(1, 0.5) });
    run.checkpoint(&port, PathBuf::from("/gen/1"), Arc::from("b")).expect("test: seated");
    assert_eq!(
        run.hear(&port, &Said::Generation { seat: id, outcome: GenerationOutcome::Refused { why: Arc::from("disk") } }),
        Some(Heard::Unsaved { why: Arc::from("disk"), positions: vec![Position { submission, learned: 5 }] })
    );
    assert_eq!(run.progress().get(&submission), Some(&5), "still volatile, published by a later checkpoint");
}


// what this catches (Codex on #6254): a quarantined seat's leases dropped while the engine has not
// proved its memory released, so the authority lends bytes native code may still hold. Quarantined
// keeps every guard; only the Released that follows drops them.
#[test]
fn a_quarantined_seat_keeps_its_leases_until_it_is_released() {
    let port = Port::default();
    let mut run = seated(&port);
    let id = run.seat;
    let (weights, weights_dropped) = guard();
    run.guards.push(g(weights));
    assert_eq!(
        run.hear(&port, &Said::Seat { seat: id, state: SeatState::Quarantined { why: Arc::from("restore unconfirmed") } }),
        Some(Heard::Quarantined { why: Arc::from("restore unconfirmed") })
    );
    assert_eq!(run.held(), 4, "quarantined: every lease is still held");
    assert!(!weights_dropped.load(Ordering::Relaxed));
    assert!(!run.ready(), "nothing more is learned into it");
    assert_eq!(run.hear(&port, &Said::Seat { seat: id, state: SeatState::Released }), Some(Heard::Released));
    assert_eq!(run.held(), 0, "released: the engine proved its memory free");
    assert!(weights_dropped.load(Ordering::Relaxed));
}

// what this catches: a seat whose guards outlive a refusal that holds nothing, a load or seat told
// out of order, or a command no engine took that keeps the lease taken for it. A refusal drops
// every guard; out of order is refused by name; no engine drops the guard handed to it.
#[test]
fn guards_go_only_when_nothing_of_the_seat_remains() {
    let port = Port::default();
    let mut run = measured(&port);
    let (early, early_dropped) = guard();
    assert!(matches!(run.load(&port, g(early), ROOM), Err(NotTold::OutOfOrder(_))), "loaded before its weights were measured");
    assert!(early_dropped.load(Ordering::Relaxed), "the refused lease is dropped");

    let id = run.seat;
    run.hear(&port, &Said::Seat { seat: id, state: SeatState::Measured { weights: Vec::new() } });
    port.closed.store(true, Ordering::Relaxed);
    let (weights, weights_dropped) = guard();
    assert!(matches!(run.load(&port, g(weights), ROOM), Err(NotTold::NoEngine)));
    assert!(weights_dropped.load(Ordering::Relaxed), "no engine: the weights lease is dropped, nothing was allocated");

    port.closed.store(false, Ordering::Relaxed);
    let (weights, weights_dropped) = guard();
    run.load(&port, g(weights), ROOM).expect("test: measured, open");
    assert_eq!(run.held(), 1);
    assert_eq!(
        run.hear(&port, &Said::Seat { seat: id, state: SeatState::Refused { why: Arc::from("geometry") } }),
        Some(Heard::NotSeated { why: Arc::from("geometry") })
    );
    assert_eq!(run.held(), 0, "refused: the engine holds nothing, so neither does the trainer");
    assert!(weights_dropped.load(Ordering::Relaxed));
}

// what this catches (Codex on c22b747e5): an uncertain step whose Release reached no engine treated
// as a dropped seat, its leases freed while the engine that ran the step may still hold the shadow.
// The guards stay in custody, quarantined, and the release told again later drops them only on its
// Released.
#[test]
fn an_undelivered_release_keeps_the_seats_guards() {
    let port = Port::default();
    let mut run = seated(&port);
    let id = run.seat;
    let step = run.learn(&port, Position { submission: Uuid::new_v4(), learned: 0 }, lesson(), guard().0).expect("test: seated");
    port.closed.store(true, Ordering::Relaxed);
    assert_eq!(run.hear(&port, &Said::Step { seat: id, step, outcome: StepOutcome::Uncertain { steps: 1 } }), Some(Heard::Uncertain));
    assert_eq!(run.held(), 3, "no engine took the Release: nothing proves the shadow gone");
    assert!(!run.ready());

    port.closed.store(false, Ordering::Relaxed);
    run.release(&port).expect("test: the engine is back");
    assert!(matches!(port.told.lock().last(), Some(Told::Release(seat)) if *seat == id));
    assert_eq!(run.held(), 3, "told, not yet answered");
    assert_eq!(run.hear(&port, &Said::Seat { seat: id, state: SeatState::Released }), Some(Heard::Released));
    assert_eq!(run.held(), 0);
}

// what this catches (Codex on c22b747e5): a checkpoint told while a lesson is in flight. The
// engine's queue saves weights holding that lesson while the positions snapshot misses it, so a
// recovery learns it twice. The checkpoint waits for the step's answer.
#[test]
fn a_checkpoint_waits_for_the_lesson_in_flight() {
    let port = Port::default();
    let mut run = seated(&port);
    let id = run.seat;
    let submission = Uuid::new_v4();
    let step = run.learn(&port, Position { submission, learned: 0 }, lesson(), guard().0).expect("test: seated");
    run.hear(&port, &Said::Step { seat: id, step, outcome: learned(1, 0.5) });
    let step = run.learn(&port, Position { submission, learned: 1 }, lesson(), guard().0).expect("test: seated");
    assert!(matches!(run.checkpoint(&port, PathBuf::from("/gen/1"), Arc::from("b")), Err(NotTold::OutOfOrder(_))), "a lesson is in flight");
    run.hear(&port, &Said::Step { seat: id, step, outcome: learned(1, 0.4) });
    assert!(run.checkpoint(&port, PathBuf::from("/gen/1"), Arc::from("b")).expect("test: answered"));
    assert_eq!(
        run.hear(&port, &Said::Generation { seat: id, outcome: GenerationOutcome::Saved { into: PathBuf::from("/gen/1"), steps: 2 } }),
        Some(Heard::Saved { into: PathBuf::from("/gen/1"), steps: 2, positions: vec![Position { submission, learned: 2 }] }),
        "the generation's positions are the frontier of the weights it saved"
    );
}

// what this catches (Codex on #6254 L2): a rejected seat command read as an empty refusal, its
// guards dropped while the seat still holds what it held, or the seat left in the phase it was
// told into. Kept returns the seat to where it was, every guard held, and the command can be told
// again.
#[test]
fn a_kept_seat_command_leaves_the_seat_where_it_was() {
    let port = Port::default();
    let mut run = measured(&port);
    let id = run.seat;
    run.hear(&port, &Said::Seat { seat: id, state: SeatState::Measured { weights: Vec::new() } });
    run.load(&port, g(guard().0), ROOM).expect("test: measured");
    run.hear(&port, &Said::Seat { seat: id, state: loaded() });
    run.seat(&port, guard().0, g(guard().0), g(guard().0)).expect("test: loaded");

    assert_eq!(
        run.hear(&port, &Said::Seat { seat: id, state: SeatState::Kept { why: Arc::from("retain refused") } }),
        Some(Heard::Kept { why: Arc::from("retain refused") })
    );
    assert_eq!(run.held(), 3, "kept: every guard still held");
    assert!(!run.ready(), "not seated");
    run.seat(&port, guard().0, g(guard().0), g(guard().0)).expect("test: back to loaded, so it seats again");
    assert_eq!(run.hear(&port, &Said::Seat { seat: id, state: SeatState::Seated { optimizer: OptimizerStart::Cold } }), Some(Heard::Seated { optimizer: OptimizerStart::Cold }));
    assert!(run.ready());
}

// what this catches (Codex on #6255): a lesson too long for its seat that loses or replays what the
// seat learned before it. Short A learns, oversize B answers TooLong; the trainer checkpoints, the
// Saved commits A exactly once, the seat releases, and a new seat sized for B starts with B still
// pending and nothing of A left volatile to learn twice.
#[test]
fn a_lesson_too_long_commits_what_was_learned_before_the_seat_is_measured_again() {
    let port = Port::default();
    let mut run = seated(&port);
    let id = run.seat;
    let submission = Uuid::new_v4();
    let a = run.learn(&port, Position { submission, learned: 0 }, lesson(), guard().0).expect("test: seated");
    run.hear(&port, &Said::Step { seat: id, step: a, outcome: learned(1, 0.5) });
    let b = run.learn(&port, Position { submission, learned: 1 }, lesson(), guard().0).expect("test: seated");
    let need = NonZeroU32::new(4_096).expect("test: nonzero");
    assert_eq!(
        run.hear(&port, &Said::Step { seat: id, step: b, outcome: StepOutcome::TooLong { need } }),
        Some(Heard::TooLong { position: Position { submission, learned: 1 }, need })
    );
    assert_eq!(run.progress().get(&submission), Some(&1), "A is learned and still volatile; B is not");

    assert!(run.checkpoint(&port, PathBuf::from("/gen/1"), Arc::from("b")).expect("test: B is answered, the seat is ready"));
    assert_eq!(
        run.hear(&port, &Said::Generation { seat: id, outcome: GenerationOutcome::Saved { into: PathBuf::from("/gen/1"), steps: 1 } }),
        Some(Heard::Saved { into: PathBuf::from("/gen/1"), steps: 1, positions: vec![Position { submission, learned: 1 }] }),
        "A is committed exactly once"
    );
    assert!(run.progress().is_empty(), "nothing volatile is left to replay");
    run.release(&port).expect("test: open");
    assert_eq!(run.hear(&port, &Said::Seat { seat: id, state: SeatState::Released }), Some(Heard::Released));

    let resized = seated(&port);
    assert!(resized.progress().is_empty(), "the new seat starts cold, with B (position 1) still pending");
    assert_ne!(resized.seat, id);
}
