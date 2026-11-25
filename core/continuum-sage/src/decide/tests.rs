use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use super::cases::lane::{self, Lane, Mind, Phase};
use super::cases::{placement, volume};
use super::*;
use crate::component::Urgency;
use crate::dataflow::{InputRevision, TopicName};

/// A use case written only in this test: nothing in `decide` knows it.
#[derive(Debug, Clone, Copy)]
struct Thermostat;

impl UseCase for Thermostat {
    type Situation = f64;
    type Action = i8;
    type Outcome = f64;
    const NAME: &'static str = "thermostat";

    fn envelope(degrees: &f64, admissible: &mut Vec<i8>) -> Gate<i8> {
        if *degrees > 90.0 {
            return Gate::Forced(-1);
        }
        admissible.extend([0, 1, -1]);
        Gate::Choose
    }

    fn constraint(degrees: &f64) -> Option<Violation> {
        (*degrees > 100.0).then_some(Violation { constraint: "boiling", measured: *degrees, limit: 100.0 })
    }

    fn retain(slot: &mut f64, from: &f64) {
        *slot = *from;
    }

    fn empty() -> f64 {
        0.0
    }
}

/// A second policy for it, also written only here: it warms, and remembers every outcome it learned,
/// in the order it learned them, so replay can be checked against what it saw live.
#[derive(Default)]
struct Warmer {
    learned: Vec<u64>,
}

impl Policy<Thermostat> for Warmer {
    fn id(&self) -> PolicyId {
        PolicyId { name: "warmer", parameters: self.learned.len() as u64 }
    }
    fn choose(&mut self, _: &f64, admissible: &[i8]) -> Choice {
        Choice { index: admissible.iter().position(|&a| a == 1).unwrap_or(0), expected: Some(1.0) } // unwrap_or: the test's own policy takes the first when warming is not offered
    }
    fn learn(&mut self, record: &Record<Thermostat>) {
        self.learned.push(record.id.0);
    }
}

fn read(through: u64) -> Vec<InputRevision> {
    vec![InputRevision { topic: TopicName("test/degrees"), through }]
}

fn capacity(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).expect("test: nonzero") // expect: the test's own precondition
}

// what this catches: the design test (THE-LANE-ON-SAGE.md). A use case added without editing any
// policy runs under the generic Baseline; its envelope forces; its records name what they read and
// close with their outcome; a broken constraint is named; a decision closes once.
#[test]
fn a_new_use_case_runs_unedited_under_the_baseline() {
    let mut decider: Decider<Thermostat> = Decider::new(Box::new(Baseline), capacity(8));
    let Decided::Act(calm, action) = decider.decide(&70.0, &read(1)) else { panic!("test: calm acts") };
    assert_eq!(action, 0, "the baseline takes the first admissible action");
    let Decided::Act(hot, forced) = decider.decide(&95.0, &read(2)) else { panic!("test: hot acts") };
    assert_eq!(forced, -1, "the envelope forces, whatever the policy");
    let closed = decider.outcome(hot, 101.0).expect("test: an open decision closes"); // expect: the test's own precondition
    assert_eq!(closed.violation.map(|v| v.constraint), Some("boiling"));
    assert!(matches!(closed.verdict, Verdict::Forced(-1)) && closed.read == read(2));
    assert!(decider.outcome(calm, 70.5).is_some());
    assert!(decider.outcome(calm, 70.5).is_none(), "a decision closes once");
    assert_eq!(decider.records().map(|r| r.id).collect::<Vec<_>>(), vec![calm, hot], "records in the order decided");
}

// what this catches (Codex on #6278): the record ring overwriting an open decision on wrap, so its late
// outcome vanishes before its constraint is checked and the policy learns. With one slot: A is open, B
// is refused as Full until A's outcome arrives, A is graded and learned once, then B decides.
#[test]
fn an_open_decision_is_never_overwritten_and_its_late_outcome_is_still_graded() {
    let mut decider: Decider<Thermostat> = Decider::new(Box::new(Warmer::default()), capacity(1));
    let Decided::Act(a, _) = decider.decide(&70.0, &read(1)) else { panic!("test: a acts") };
    assert_eq!(decider.decide(&71.0, &read(2)), Decided::Full { open: a }, "the only slot holds an open decision");
    let graded = decider.outcome(a, 101.0).expect("test: a's late outcome closes it"); // expect: the test's own precondition
    assert_eq!(graded.violation.map(|v| v.constraint), Some("boiling"), "its constraint is still checked");
    assert!(matches!(decider.decide(&71.0, &read(2)), Decided::Act(..)), "the slot is free once a closed");
    assert!(decider.outcome(a, 70.0).is_none(), "a is graded once");
}

// what this catches (Codex on #6278): replay that hands a learning policy outcomes in completion order
// as if they were issue order, or never calls learn, so a stateful policy differs from live even on the
// same actions. A and B overlap: A is decided, then B; B's outcome arrives before A's. Live, the policy
// learned B then A, and chose B before learning anything; replay must reproduce exactly that, and an
// action it did not take gets no outcome.
#[test]
fn replay_hands_a_learning_policy_the_live_order_of_choices_and_outcomes() {
    let mut live: Decider<Thermostat> = Decider::new(Box::new(Warmer::default()), capacity(8));
    let Decided::Act(a, _) = live.decide(&70.0, &read(1)) else { panic!("test: a acts") };
    let Decided::Act(b, _) = live.decide(&71.0, &read(2)) else { panic!("test: b acts") };
    assert!(live.outcome(b, 71.5).is_some());
    assert!(live.outcome(a, 70.5).is_some());
    let Decided::Act(c, _) = live.decide(&95.0, &read(3)) else { panic!("test: c acts") };

    let mut again = Warmer::default();
    let replayed = replay(live.records(), &mut again);
    assert_eq!(again.learned, vec![b.0, a.0], "learned in the order the outcomes arrived");
    let by_id = |id| replayed.iter().find(|r| r.id == id).expect("test: replayed"); // expect: the test's own precondition
    assert_eq!((by_id(a).chosen, by_id(a).outcome), (Some(1), Some(70.5)));
    assert_eq!((by_id(c).chosen, by_id(c).outcome), (Some(-1), None), "c is still open: no outcome");

    let mut baseline = Baseline;
    let other = replay(live.records(), &mut baseline);
    let a_other = other.iter().find(|r| r.id == a).expect("test: replayed"); // expect: the test's own precondition
    assert_eq!((a_other.recorded, a_other.chosen, a_other.outcome), (Some(1), Some(0), None), "a different action has no observed outcome");
}

/// A decoding mind that has waited `waited_ms`, 60 MB deep and warm.
fn mind(id: u8, waited_ms: u64) -> Mind<u8> {
    Mind {
        mind: id,
        urgency: Urgency::Want,
        waited: Duration::from_millis(waited_ms),
        deadline: Duration::from_secs(1),
        phase: Phase::Decode,
        kv_bytes: Measured::Value(60 << 20),
        cold_bytes: Measured::Value(0),
        remaining: Measured::Unmeasured,
        step: Measured::Value(Duration::from_millis(26)),
        switch_in: Measured::Value(Duration::from_micros(10)),
        prefix_hit: Measured::Unmeasured,
    }
}

fn envelope<U: UseCase>(situation: &U::Situation) -> (Gate<U::Action>, Vec<U::Action>) {
    let mut admissible = Vec::new();
    let gate = U::envelope(situation, &mut admissible);
    (gate, admissible)
}

// what this catches: the lane's envelope letting a policy starve a mind past her deadline, offering a
// switch to nobody, or a batch wider than the Core can step; and a decision that allocates per step
// (actions are slots and masks, so the record ring and the buffer are reused).
#[test]
fn the_lane_forces_an_overdue_mind_and_offers_batches_only_as_wide_as_the_core() {
    let overdue = lane::Situation { running: Some(mind(1, 0)), ready: vec![mind(2, 400), mind(3, 1_200)], batch_width: 1 };
    assert_eq!(envelope::<Lane<u8>>(&overdue).0, Gate::Forced(lane::Action::Switch(1)));

    let open = lane::Situation { running: Some(mind(1, 0)), ready: vec![mind(2, 10), mind(3, 20)], batch_width: 2 };
    let (gate, offered) = envelope::<Lane<u8>>(&open);
    assert_eq!(gate, Gate::Choose);
    assert_eq!(offered, vec![lane::Action::Continue, lane::Action::Switch(0), lane::Action::Switch(1), lane::Action::Batch(0b1)]);

    let alone = lane::Situation::<u8> { running: None, ready: vec![], batch_width: 1 };
    assert_eq!(envelope::<Lane<u8>>(&alone).0, Gate::Forced(lane::Action::Idle));

    let mut decider: Decider<Lane<u8>> = Decider::new(Box::new(Baseline), capacity(4));
    let step = lane::Outcome { tokens: Measured::Value(1), engine: Duration::from_millis(26), worst_wait: Duration::from_millis(20), worst_deadline: Duration::from_secs(1) };
    for _ in 0..16 {
        let Decided::Act(id, lane::Action::Continue) = decider.decide(&open, &read(1)) else { panic!("test: the baseline continues") };
        assert!(decider.outcome(id, step).is_some(), "each step's outcome closes its record");
    }
    assert_eq!(decider.records().count(), 4, "the ring keeps what it declared");
}

// what this catches (Codex on #6278): observed capacity treated as a plan; a lease that ends before the
// work does, whose price passes the budget, or whose guaranteed rate cannot finish the work in its term
// (a zero rate; 1 unit/s for 100 units), admitting the work; unmeasured work admitted to a peer.
#[test]
fn placement_admits_a_peer_only_under_a_lease_that_carries_the_work_within_budget() {
    let now = Instant::now();
    let measured = |seconds| Measured::Value(Duration::from_secs(seconds));
    let lease = |rate, until_s, per_unit| Capacity::Committed(Lease { rate, until: now + Duration::from_secs(until_s), price: Price { per_unit } });
    let peer = |id, capacity| placement::Holder { id, capacity, transfer: measured(2), queue: measured(1) };
    let holders = vec![
        peer(1, Capacity::Observed { rate: Measured::Value(40.0), as_of: now }),
        peer(2, lease(30.0, 60, 0.05)),
        peer(3, lease(30.0, 7, 0.05)),
        peer(4, lease(30.0, 60, 5.0)),
        peer(5, lease(0.0, 60, 0.05)),
        peer(6, lease(1.0, 60, 0.05)),
    ];
    let work = placement::Work { units: 100.0, predicted: measured(5), budget: 10.0 };
    let situation = placement::Situation { now, work, moves: 1 << 20, warm_here: false, here: lease(30.0, 600, 0.0), holders: holders.clone() };
    let (_, offered) = envelope::<placement::Placement>(&situation);
    assert_eq!(offered, vec![placement::Action::Here, placement::Action::On(1)], "only the lease whose rate finishes the work in its term within budget");
    let unknown = placement::Situation { work: placement::Work { predicted: Measured::Unmeasured, ..work }, holders, ..situation };
    assert_eq!(envelope::<placement::Placement>(&unknown).1, vec![placement::Action::Here], "an unmeasured run cannot be shown to fit");
}

// what this catches: hot work placed on a volume measured slower than one with room (the 5090's SATA
// disk holding a worktree, 2026-11-25), an unmeasured volume refused outright, and (Codex on #6278) a
// full volume chosen when none has room instead of deferring.
#[test]
fn a_hot_working_set_never_lands_on_a_slower_volume_and_no_room_defers() {
    let disk = |id, free_gb: u64, rate| volume::Volume { id, free_bytes: free_gb << 30, rate };
    let volumes = vec![disk(0, 347, Measured::Value(3.0e9)), disk(1, 9_000, Measured::Value(2.0e8)), disk(2, 50, Measured::Unmeasured)];
    let hot = volume::Situation { bytes: 30 << 30, hot: true, volumes: volumes.clone() };
    assert_eq!(envelope::<volume::Placement>(&hot).1, vec![volume::Action::On(0), volume::Action::On(2)]);
    let cold = volume::Situation { hot: false, ..hot.clone() };
    assert!(envelope::<volume::Placement>(&cold).1.contains(&volume::Action::On(1)), "an archive may go to the big disk");
    let huge = volume::Situation { bytes: 20_000 << 30, ..hot };
    assert!(matches!(envelope::<volume::Placement>(&huge).0, Gate::Defer(Deferred { condition: "free bytes on the roomiest volume", .. })));
}

// what this catches: an unmeasured value standing in as a number, a fraction clamped into range, and a
// window that keeps more than it declares or sorts on every read.
#[test]
fn unmeasured_stays_explicit_and_views_keep_what_they_declare() {
    let mut mean = Ewma::new(Fraction::positive(0.5).expect("test: a weight in (0, 1]")); // expect: the test's own precondition
    assert_eq!(mean.mean(), Measured::Unmeasured);
    mean.observe(10.0);
    mean.observe(20.0);
    assert_eq!(mean.mean(), Measured::Value(15.0));
    assert_eq!((Fraction::positive(0.0), Fraction::of(1.5), Fraction::of(f64::NAN)), (None, None, None), "refused, never clamped");
    let [low, half, high] = [0.0, 0.5, 1.0].map(|q| Fraction::of(q).expect("test: a quantile in [0, 1]")); // expect: the test's own precondition
    let mut window = Window::new(capacity(3));
    assert_eq!(window.quantile(half), Measured::Unmeasured);
    for sample in [5.0, 1.0, 9.0, 7.0] {
        window.observe(sample);
    }
    assert_eq!((window.retains(), window.quantile(low), window.quantile(half), window.quantile(high)), (3, Measured::Value(1.0), Measured::Value(7.0), Measured::Value(9.0)));
}

// what this catches: an addressed mind left behind a running one that only wants to continue, and a
// needless switch at equal need (the switch is cheap, but a policy that thrashes is not need-first).
#[test]
fn need_first_switches_to_a_more_urgent_mind_and_otherwise_continues() {
    let urgent = |id, waited| lane::Mind { urgency: Urgency::Urgent, ..mind(id, waited) };
    let mut policy = lane::NeedFirst;
    let mut decide = |situation: &lane::Situation<u8>| {
        let (gate, admissible) = envelope::<Lane<u8>>(situation);
        assert_eq!(gate, Gate::Choose, "nobody is overdue");
        admissible[Policy::<Lane<u8>>::choose(&mut policy, situation, &admissible).index]
    };
    let addressed = lane::Situation { running: Some(mind(1, 0)), ready: vec![mind(2, 300), urgent(3, 10)], batch_width: 1 };
    assert_eq!(decide(&addressed), lane::Action::Switch(1), "the addressed mind takes the next step");
    let equal = lane::Situation { running: Some(mind(1, 0)), ready: vec![mind(2, 300)], batch_width: 1 };
    assert_eq!(decide(&equal), lane::Action::Continue, "at equal need the running mind continues until a bound forces otherwise");
    let idle = lane::Situation { running: None, ready: vec![mind(2, 100), urgent(3, 10), mind(4, 900)], batch_width: 1 };
    assert_eq!(decide(&idle), lane::Action::Switch(1), "with nobody running, the most urgent starts");
}

// what this catches: an algorithm wired in as a type instead of created by name (OpenCV's
// Algorithm::create), a name meaning two algorithms, an unknown name built anyway, a parameter set
// outside its range or under a wrong name, and a saved state that does not restore what was learned.
#[test]
fn lane_algorithms_are_created_by_name_with_typed_params_and_saved_state() {
    let registry = lane::algorithms::<u8>().expect("test: distinct names register"); // expect: the test's own precondition
    assert_eq!(registry.names().collect::<Vec<_>>(), vec!["baseline", "need-first", "cost-model"]);
    assert!(matches!(registry.create("no-such-algorithm"), Err(Unknown { use_case: "lane", name: "no-such-algorithm" })));
    assert!(lane::algorithms::<u8>().and_then(|r| r.register("cost-model", || Box::new(Baseline))).is_err(), "a name means one algorithm");

    let mut cost = registry.create("cost-model").expect("test: registered"); // expect: the test's own precondition
    assert_eq!(cost.params(), &["margin"]);
    assert_eq!(cost.param("margin"), None, "nothing set, saved or measured: no margin yet, never a constant");
    assert!(matches!(cost.set_param("margin", Param::Real(1.5)), Err(Refused::Value { .. })), "refused out of range, never clamped");
    assert!(matches!(cost.set_param("depth", Param::Real(0.1)), Err(Refused::Unknown("depth"))));
    cost.set_param("margin", Param::Real(0.25)).expect("test: in range"); // expect: the test's own precondition
    let mut saved = Vec::new();
    cost.save(&mut saved);
    let mut restored = registry.create("cost-model").expect("test: registered"); // expect: the test's own precondition
    restored.load(&saved).expect("test: its own state loads"); // expect: the test's own precondition
    assert_eq!(restored.param("margin"), Some(Param::Real(0.25)), "a restart resumes what it was set to");
    assert!(saved.split_last().is_some_and(|(_, truncated)| restored.load(truncated).is_err()), "a truncated state is refused");
}

// what this catches: features left unmeasured when the executor reported them, a track kept after its
// mind left, a step time not per
// token, a remainder that ignores what she already decoded, and an unseen mind given invented numbers.
#[test]
fn the_lane_history_fills_what_the_executor_observed_and_nothing_else() {
    use super::cases::lane_history::{LaneHistory, Observation, Views};
    let views = Views { smoothing: Fraction::positive(1.0).expect("test: a weight"), turns: capacity(4) }; // expect: the test's own precondition
    let mut history = LaneHistory::new(views);
    for decoded in [100, 300, 200] {
        history.observe(Observation::TurnEnded { mind: 1u8, decoded });
    }
    history.observe(Observation::Stepped { mind: 1, took: Duration::from_millis(260), tokens: 10, prefill: false });
    history.observe(Observation::SwitchedIn { mind: 1, took: Duration::from_micros(10) });
    history.observe(Observation::Reused { mind: 1, reused: 900, prompt: 1_000 });
    history.observe(Observation::Resident { mind: 1, kv_bytes: 64 << 20, cold_bytes: Measured::Value(0) });
    let median = Fraction::of(0.5).expect("test: a quantile"); // expect: the test's own precondition
    let mut her = mind(1, 0);
    her.step = Measured::Unmeasured;
    history.fill(&mut her, median);
    assert_eq!(her.step, Measured::Value(Duration::from_millis(26)), "per token");
    assert_eq!(her.switch_in, Measured::Value(Duration::from_micros(10)));
    assert_eq!((her.prefix_hit, her.kv_bytes), (Measured::Value(0.9), Measured::Value(64 << 20)));
    assert_eq!(her.remaining, Measured::Value(190), "the median turn (200) less the 10 she decoded");
    let mut stranger = lane::Mind { step: Measured::Unmeasured, ..mind(7, 0) };
    history.fill(&mut stranger, median);
    assert_eq!((stranger.step, stranger.remaining), (Measured::Unmeasured, Measured::Unmeasured), "unseen stays unmeasured");
    history.observe(Observation::Left { mind: 1 });
    let mut gone = lane::Mind { step: Measured::Unmeasured, ..mind(1, 0) };
    history.fill(&mut gone, median);
    assert_eq!(gone.step, Measured::Unmeasured, "a mind that left keeps no track");
}

// what this catches: a cost model that switches on a fixed count instead of costs: a running mind whose
// next step fits every waiter's slack continues; a waiter whose slack is nearly spent is brought in; and
// a costly switch (a deep cold mind) leaves less slack, so she is brought in sooner, before her bound.
#[test]
fn the_cost_model_continues_while_steps_fit_and_brings_in_the_tightest_waiter() {
    let registry = lane::algorithms::<u8>().expect("test: distinct names register"); // expect: the test's own precondition
    let mut cost = registry.create("cost-model").expect("test: registered"); // expect: the test's own precondition
    cost.set_param("margin", Param::Real(0.5)).expect("test: the margin this scenario is written at"); // expect: the test's own precondition
    let mut decide = |situation: &lane::Situation<u8>| {
        let (gate, admissible) = envelope::<Lane<u8>>(situation);
        assert_eq!(gate, Gate::Choose);
        admissible[cost.choose(situation, &admissible).index]
    };
    let running = mind(1, 0); // 26 ms a step
    let fresh = lane::Situation { running: Some(running), ready: vec![mind(2, 100)], batch_width: 1 };
    assert_eq!(decide(&fresh), lane::Action::Continue, "900 ms of slack: her next 26 ms step fits");
    let near = lane::Situation { running: Some(running), ready: vec![mind(2, 100), mind(3, 960)], batch_width: 1 };
    assert_eq!(decide(&near), lane::Action::Switch(1), "40 ms of slack, half kept: the tightest is brought in");
    let cold = lane::Mind { switch_in: Measured::Value(Duration::from_millis(880)), ..mind(4, 100) };
    let costly = lane::Situation { running: Some(running), ready: vec![mind(2, 100), cold], batch_width: 1 };
    assert_eq!(decide(&costly), lane::Action::Switch(1), "her 880 ms switch leaves 20 ms of slack: she is brought in now");
}

// what this catches: a margin fixed at its starting value (a constant dressed as an algorithm), a loop
// that only ever moves one way, a step that never adapts, a reading taken when no one waited, and a
// learned state lost across a restart.
#[test]
fn the_cost_models_margin_is_a_control_loop_both_ways_and_survives_a_restart() {
    let registry = lane::algorithms::<u8>().expect("test: distinct names register"); // expect: the test's own precondition
    let mut policy = registry.create("cost-model").expect("test: registered"); // expect: the test's own precondition
    policy.set_param("margin", Param::Real(0.9)).expect("test: a seeded margin"); // expect: the test's own precondition
    let mut decider = Decider::new(policy, capacity(16));
    let margin = |decider: &Decider<Lane<u8>>| match decider.param("margin") {
        Some(Param::Real(margin)) => margin,
        other => panic!("test: a seeded margin, found {other:?}"),
    };
    let contended = lane::Situation { running: Some(mind(1, 0)), ready: vec![mind(2, 100)], batch_width: 1 };
    let read = |decider: &mut Decider<Lane<u8>>, situation: &lane::Situation<u8>, waited_ms: u64| {
        let Decided::Act(id, _) = decider.decide(situation, &[]) else { panic!("test: the lane acts") };
        let outcome = lane::Outcome { tokens: Measured::Unmeasured, engine: Duration::from_millis(26), worst_wait: Duration::from_millis(waited_ms), worst_deadline: Duration::from_secs(1) };
        decider.outcome(id, outcome);
    };
    let start = margin(&decider);
    read(&mut decider, &contended, 1_200);
    let once = margin(&decider);
    assert!(once < start, "a wait past her bound: the margin falls ({start} -> {once})");
    read(&mut decider, &contended, 1_200);
    let twice = margin(&decider);
    assert!(twice / once < once / start, "agreeing readings grow the step: it falls faster");
    read(&mut decider, &contended, 200);
    let calm = margin(&decider);
    assert!(calm > twice, "every waiter inside her bound: the margin rises ({twice} -> {calm})");
    let breached = calm;
    let alone = lane::Situation { running: Some(mind(1, 0)), ready: vec![], batch_width: 1 };
    read(&mut decider, &alone, 0);
    assert_eq!(margin(&decider), breached, "no one waiting is no reading");

    let mut saved = Vec::new();
    decider.save(&mut saved);
    let mut restarted = Decider::new(registry.create("cost-model").expect("test: registered"), capacity(16)); // expect: the test's own precondition
    restarted.load(&saved).expect("test: its own state loads"); // expect: the test's own precondition
    assert_eq!(margin(&restarted), breached, "a restart resumes the learned margin");
    let mut fresh = Decider::new(registry.create("cost-model").expect("test: registered"), capacity(16)); // expect: the test's own precondition
    fresh.load(&[]).expect("test: nothing saved loads as the start"); // expect: the test's own precondition
    assert_eq!(fresh.param("margin"), None, "nothing saved: unseeded, never a constant");
}

// what this catches: a margin that starts from a constant instead of the node's own measurements: the
// first contended decision seeds it from the waiter's slack and the running mind's measured step
// (one step of headroom), and a running mind with no measured step cannot seed, so the waiter is
// brought in.
#[test]
fn the_cost_models_margin_is_seeded_from_the_first_measurements_it_needs() {
    let registry = lane::algorithms::<u8>().expect("test: distinct names register"); // expect: the test's own precondition
    let mut cost = registry.create("cost-model").expect("test: registered"); // expect: the test's own precondition
    let unmeasured = lane::Situation { running: Some(lane::Mind { step: Measured::Unmeasured, ..mind(1, 0) }), ready: vec![mind(2, 100)], batch_width: 1 };
    let (_, admissible) = envelope::<Lane<u8>>(&unmeasured);
    assert_eq!(admissible[cost.choose(&unmeasured, &admissible).index], lane::Action::Switch(0), "no measured step: nothing to seed from, the waiter comes in");
    assert_eq!(cost.param("margin"), None);
    let measured = lane::Situation { running: Some(mind(1, 0)), ready: vec![mind(2, 100)], batch_width: 1 };
    let (_, admissible) = envelope::<Lane<u8>>(&measured);
    assert_eq!(admissible[cost.choose(&measured, &admissible).index], lane::Action::Continue, "seeded: her 26 ms step fits");
    let waiter = mind(2, 100);
    let slack = waiter.deadline - waiter.waited - waiter.switch_in.value().unwrap_or_default(); // unwrap_or_default: the helper measures it
    let expected = (slack - Duration::from_millis(26)).as_secs_f64() / slack.as_secs_f64();
    assert_eq!(cost.param("margin"), Some(Param::Real(expected)), "one measured step of headroom in the waiter's slack");
}
