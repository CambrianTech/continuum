use std::time::{Duration, Instant};

use super::cases::lane::{self, Lane, Mind, Phase};
use super::cases::{placement, volume};
use super::*;
use crate::component::Urgency;
use crate::dataflow::{InputRevision, TopicName};

/// A use case written only in this test: nothing in `decide` knows it.
struct Thermostat;

impl UseCase for Thermostat {
    type Situation = f64;
    type Action = i8;
    type Outcome = f64;
    const NAME: &'static str = "thermostat";

    fn envelope(degrees: &f64) -> Envelope<i8> {
        if *degrees > 90.0 {
            Envelope::Forced(-1)
        } else {
            Envelope::Admissible(NonEmpty::new(0, [1, -1]))
        }
    }

    fn constraint(degrees: &f64) -> Option<Violation> {
        (*degrees > 100.0).then_some(Violation { constraint: "boiling", measured: *degrees, limit: 100.0 })
    }
}

/// A second policy for it, also written only here.
struct Warmer;

impl Policy<Thermostat> for Warmer {
    fn id(&self) -> PolicyId {
        PolicyId { name: "warmer", parameters: 1 }
    }
    fn choose(&mut self, _: &f64, admissible: &NonEmpty<i8>) -> Choice<i8> {
        Choice { action: if admissible.contains(&1) { 1 } else { *admissible.first() }, expected: Some(1.0) }
    }
}

fn read(through: u64) -> Vec<InputRevision> {
    vec![InputRevision { topic: TopicName("test/degrees"), through }]
}

// what this catches: the design test (THE-LANE-ON-SAGE.md). A use case added without editing any
// policy runs under the generic Baseline; its envelope forces; its records name what they read and
// close with their outcome; a broken constraint is named. Then a different policy is tried on the
// recorded history with no producer or executor: it sees each situation as it was, and an action
// that was not taken gets no outcome.
#[test]
fn a_new_use_case_runs_unedited_and_a_new_policy_replays_its_history() {
    let mut decider: Decider<Thermostat> = Decider::new(Box::new(Baseline), 8);
    let (calm, action) = decider.decide(70.0, &read(1));
    assert_eq!(action, 0, "the baseline takes the first admissible action");
    let (hot, forced) = decider.decide(95.0, &read(2));
    assert_eq!(forced, -1, "the envelope forces, whatever the policy");
    let closed = decider.outcome(hot, 101.0).expect("test: an open decision closes"); // expect: the test's own precondition
    assert_eq!(closed.violation.map(|v| v.constraint), Some("boiling"));
    assert!(closed.forced && closed.read == read(2));
    assert!(decider.outcome(calm, 70.5).is_some());
    assert!(decider.outcome(calm, 70.5).is_none(), "a decision closes once");

    let replayed = replay(decider.closed(), &mut Warmer);
    let by_id = |id| replayed.iter().find(|r| r.id == id).expect("test: replayed"); // expect: the test's own precondition
    assert_eq!((by_id(calm).recorded, by_id(calm).chosen, by_id(calm).outcome), (0, 1, None), "a different action has no observed outcome");
    assert_eq!((by_id(hot).chosen, by_id(hot).outcome), (-1, Some(101.0)), "the same forced action keeps its recorded outcome");
}

/// A decoding mind that has waited `waited_ms`, 60 MB deep and warm.
fn mind(id: u8, waited_ms: u64) -> Mind<u8> {
    Mind {
        mind: id,
        urgency: Urgency::Want,
        waited: Duration::from_millis(waited_ms),
        deadline: Duration::from_secs(1),
        phase: Phase::Decode,
        kv_bytes: 60 << 20,
        cold_bytes: 0,
        remaining: Measured::Unmeasured,
        step: Measured::Value(Duration::from_millis(26)),
        switch_in: Measured::Value(Duration::from_micros(10)),
        prefix_hit: Measured::Unmeasured,
    }
}

// what this catches: the lane's envelope letting a policy starve a mind past her deadline, offering
// a switch to nobody, or offering a batch wider than the Core can step.
#[test]
fn the_lane_forces_an_overdue_mind_and_offers_batches_only_as_wide_as_the_core() {
    let overdue = lane::Situation { running: Some(mind(1, 0)), ready: vec![mind(2, 400), mind(3, 1_200)], batch_width: 1 };
    assert_eq!(Lane::<u8>::envelope(&overdue), Envelope::Forced(lane::Action::Switch(3)));

    let open = lane::Situation { running: Some(mind(1, 0)), ready: vec![mind(2, 10), mind(3, 20)], batch_width: 2 };
    let Envelope::Admissible(actions) = Lane::<u8>::envelope(&open) else { panic!("test: nobody is overdue") };
    let offered: Vec<_> = actions.iter().cloned().collect();
    assert_eq!(offered, vec![lane::Action::Continue, lane::Action::Switch(2), lane::Action::Switch(3), lane::Action::Batch(vec![1, 2])]);

    let alone = lane::Situation { running: None, ready: vec![], batch_width: 1 };
    assert_eq!(Lane::<u8>::envelope(&alone), Envelope::Forced(lane::Action::Idle));
}

// what this catches: observed capacity treated as a plan (a peer that looks idle receiving work with no
// lease), and a lease counted after it ended.
#[test]
fn placement_admits_a_peer_only_while_its_lease_is_in_force() {
    let now = Instant::now();
    let peer = |id, capacity| placement::Holder { id: placement::HolderId(id), capacity, transfer: Measured::Unmeasured, queue: Measured::Unmeasured };
    let idle_looking = Capacity::Observed { rate: Measured::Value(40.0), as_of: now };
    let leased = Capacity::Committed(Lease { rate: 30.0, until: now + Duration::from_secs(60), price: Price { per_unit: 0.2 } });
    let expired = Capacity::Committed(Lease { rate: 30.0, until: now, price: Price { per_unit: 0.2 } });
    let situation = placement::Situation { now, moves: 1 << 20, warm_here: false, here: idle_looking, holders: vec![peer(1, idle_looking), peer(2, leased), peer(3, expired)] };
    let Envelope::Admissible(actions) = placement::Placement::envelope(&situation) else { panic!("test: here is always admissible") };
    assert_eq!(actions.iter().cloned().collect::<Vec<_>>(), vec![placement::Action::Here, placement::Action::On(placement::HolderId(2))]);
}

// what this catches: hot work placed on a volume measured slower than one with room (the 5090's SATA
// disk holding a worktree, 2026-11-25), and an unmeasured volume refused outright.
#[test]
fn a_hot_working_set_never_lands_on_a_slower_volume_with_a_faster_one_free() {
    let disk = |id, free_gb: u64, rate| volume::Volume { id: volume::VolumeId(id), free_bytes: free_gb << 30, rate };
    let volumes = NonEmpty::new(disk(0, 347, Measured::Value(3.0e9)), [disk(1, 9_000, Measured::Value(2.0e8)), disk(2, 50, Measured::Unmeasured)]);
    let hot = volume::Situation { bytes: 30 << 30, hot: true, volumes: volumes.clone() };
    let Envelope::Admissible(actions) = volume::Placement::envelope(&hot) else { panic!("test: a volume has room") };
    assert_eq!(actions.iter().cloned().collect::<Vec<_>>(), vec![volume::Action::On(volume::VolumeId(0)), volume::Action::On(volume::VolumeId(2))]);
    let cold = volume::Situation { bytes: 30 << 30, hot: false, volumes };
    let Envelope::Admissible(actions) = volume::Placement::envelope(&cold) else { panic!("test: a volume has room") };
    assert!(actions.contains(&volume::Action::On(volume::VolumeId(1))), "an archive may go to the big disk");
}

// what this catches: an unmeasured value standing in as a number, and a window that keeps more than
// it declares.
#[test]
fn unmeasured_stays_explicit_and_views_keep_what_they_declare() {
    let mut mean = Ewma::new(Fraction::positive(0.5).expect("test: a weight in (0, 1]")); // expect: the test's own precondition
    assert_eq!(mean.mean(), Measured::Unmeasured);
    mean.observe(10.0);
    mean.observe(20.0);
    assert_eq!(mean.mean(), Measured::Value(15.0));
    assert_eq!((Fraction::positive(0.0), Fraction::of(1.5), Fraction::of(f64::NAN)), (None, None, None), "refused, never clamped");
    let [low, half, high] = [0.0, 0.5, 1.0].map(|q| Fraction::of(q).expect("test: a quantile in [0, 1]")); // expect: the test's own precondition
    let mut window = Window::new(std::num::NonZeroUsize::new(3).expect("test: nonzero")); // expect: the test's own precondition
    assert_eq!(window.quantile(half), Measured::Unmeasured);
    for sample in [5.0, 1.0, 9.0, 7.0] {
        window.observe(sample);
    }
    assert_eq!((window.retains(), window.quantile(low), window.quantile(high)), (3, Measured::Value(1.0), Measured::Value(9.0)));
}
