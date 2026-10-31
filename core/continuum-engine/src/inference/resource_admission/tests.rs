//! Tests for resource admission.
#![cfg(test)]

#[path = "aging_tests.rs"]
mod aging;


// what this catches (card c84d885a, S1): a seat's queue measured where it forms, and
// "unmeasured" never reading as "fast". The beacon carries (p50, samples); a spiller
// that saw p50 = 0 with 0 samples and treated it as a zero-wait seat would pile onto
// an unmeasured one — the same absence-read-as-a-value error that cost a day.
#[test]
fn a_seats_measured_wait_is_a_median_with_a_count_and_unmeasured_is_not_fast() {
    assert_eq!(p50_of(std::iter::empty()), (0, 0), "no samples: unmeasured, not 0 ms");
    assert_eq!(p50_of([5_000u64].into_iter()), (5_000, 1));
    // the median, not the mean: one 7,058 s outlier does not become the seat's wait
    assert_eq!(p50_of([100u64, 200, 300, 7_058_000].into_iter()).0, 300);
    assert_eq!(p50_of([216_000u64, 216_000, 1_387_000].into_iter()), (216_000, 3));
}


// what this catches (BigMama's correction on #4197): leased-in LANES with a floor from
// local residents alone — the seat grows demand it then refuses to fit. The prompt
// sizes of the generates a seat serves for others must be remembered (bounded, newest
// kept, zero never a sample) so the plan's median floor sees the grid's prompts.
#[test]
fn leased_in_prompt_sizes_are_remembered_bounded_and_zero_is_not_a_sample() {
    let mut samples = PromptSamples::new();
    samples.record(PromptSample::Requested(0));
    assert!(samples.snapshot().is_empty(), "an empty completion is not a prompt size");
    // what this catches (the 5090, 2026-09-20): a refusal at the served window is a
    // sample of the demand the seat could not hold — it must reach the pool, or the
    // pool only ever sees what fit and the window measures itself.
    samples.record(PromptSample::Requested(56_057));
    for _ in 0..=LEASED_IN_SENT_SAMPLES {
        samples.record(PromptSample::Served { endpoint: "seat".into(), tokens: 828, geometry: ServedGeometry { model_id: "m".into(), generation: uuid::Uuid::new_v4(), per_slot_window: 62_720, lanes: 1 } });
    }
    assert_eq!(samples.snapshot().iter().filter_map(PromptSample::requested).collect::<Vec<_>>(), vec![56_057], "a burst of local successes cannot evict unmet demand");
    assert_eq!(samples.snapshot().iter().find_map(|s| s.served("seat", Some("m"))), Some(828));
    for i in 1..=(LEASED_IN_SENT_SAMPLES as u32 + 8) {
        samples.record(PromptSample::Requested(30_000 + i));
    }
    let s: Vec<_> = samples.snapshot().iter().filter_map(PromptSample::requested).collect();
    assert_eq!(s.len(), LEASED_IN_SENT_SAMPLES, "bounded");
    assert_eq!(s.last().copied(), Some(30_000 + LEASED_IN_SENT_SAMPLES as u32 + 8), "newest requested sample kept");
    assert!(*s.first().expect("nonempty") > 30_000 + 8, "oldest aged out by demand, not service");
}


// what this catches (card c84d885a): a seat that serves other nodes' minds and never
// counts them. The planner must see "how many did I serve AT ONCE this interval", so
// the gauge keeps a peak; and a call still in flight when the planner reads must not
// vanish from the next interval (the re-arm-at-current rule), or a long remote turn
// spanning two ticks would read as zero demand on the second.
#[test]
fn a_leased_in_gauge_remembers_its_peak_per_interval_and_never_forgets_a_running_call() {
    let g = PeakGauge::new();
    assert_eq!(g.take_peak(), 0, "nothing served yet");
    let a = g.enter();
    let b = g.enter();
    let c = g.enter();
    drop(b);
    assert_eq!(g.inflight(), 2);
    // the interval's peak was 3, not the 2 in flight at read time
    assert_eq!(g.take_peak(), 3);
    // re-armed at the CURRENT concurrency: the two still running are the floor of the
    // next interval, not forgotten
    assert_eq!(g.take_peak(), 2);
    drop(a);
    drop(c);
    assert_eq!(g.inflight(), 0);
    assert_eq!(g.take_peak(), 2, "the last interval still saw two running");
    assert_eq!(g.take_peak(), 0, "and now nothing");
}

use super::*;
use crate::inference::served_window_store::ServedGeometry;
use std::sync::atomic::Ordering;

// No serialization lock lives here any more, and that is the point. It used to exist
// because the ambient-permit and serving-lane tests mutated PROCESS-GLOBAL admission
// statics — and its own comment admitted the lock was not enough: "TEST_SERIAL only
// serializes THIS module, but a cross-module guard could bump it mid-loop" (#1960/#191).
// The in-flight Gauge test was rescued first by driving a LOCAL instance; these two now
// do the same with `LaneAdmission::new()`. The race is gone at the SOURCE, not merely
// serialized, and the tests run in parallel.

// what this catches (#139 idle admission): the in-flight gauge counts each
// model-call entry, releases on drop, and reads SATURATED exactly at the serving
// concurrency (serving_plan::MAX_LANES) — the signal a self-tick yields on so an
// idle deliberation never deepens the queue live conversation waits behind.
//
// Drives a LOCAL Gauge instance — the SAME type production's process-global
// INFLIGHT_MODEL_CALLS is, but one nothing else in the process can reach — so the
// absolute-count assertions are deterministic under cargo's parallel execution. The
// prior version read the process-global counter and a sibling test's InflightModelCall
// guard could bump it mid-loop (the #1960 flaky class, canary red 2026-07-25); scoping
// the gauge kills that race at the root, so no TEST_SERIAL / drain-wait is needed here.
#[test]
fn inflight_gauge_counts_releases_and_saturates_at_serving_concurrency() {
    let max = crate::inference::planning::serving_plan::MAX_LANES as usize;
    let gauge = Gauge::new();

    assert_eq!(gauge.inflight(), 0, "a fresh gauge starts empty");
    assert!(!gauge.saturated(max), "an empty gauge is not saturated");

    let mut guards = Vec::new();
    for expected in 1..=max {
        guards.push(gauge.enter());
        assert_eq!(gauge.inflight(), expected);
    }
    // Every decode slot busy → one more call would queue behind the fleet.
    assert!(
        gauge.saturated(max),
        "MAX_LANES outstanding must read saturated"
    );

    guards.pop(); // free a slot
    assert_eq!(gauge.inflight(), max - 1);
    assert!(
        !gauge.saturated(max),
        "freeing a slot clears saturation — an idle self-tick may run again"
    );

    drop(guards);
    assert_eq!(gauge.inflight(), 0, "all guards released → baseline");
}

// what this catches (#139 lane starvation): a directed (addressed) turn must never
// queue behind non-directed model calls. Non-directed callers are capped at
// (MAX_LANES-1) lanes, so a directed caller always finds a reserved lane — this is
// the fix for the glass-boxed 8-minute directed-turn wait behind an idle self-tick
// + one long ambient turn on the two decode lanes. This is the only test that
// touches the process-global serving-lane semaphores, so it starts all-free.
#[tokio::test]
async fn directed_turn_always_finds_a_reserved_lane() {
    let gate: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
    use std::time::Duration;
    let budget = gate.nondirected_budget();

    // Fill the ENTIRE non-directed budget (all lanes idle/ambient work may hold).
    let mut nondirected = Vec::new();
    for _ in 0..budget {
        nondirected.push(gate.acquire_serving_lane(None, LanePriority::Ambient, None).await);
    }

    // On a machine with a lane to reserve (MAX_LANES >= 2), a directed call still
    // acquires immediately — it is not blocked by the saturated non-directed budget.
    if gate.lane_count() > 1 {
        let directed = tokio::time::timeout(
            Duration::from_millis(250),
            gate.acquire_serving_lane(None, LanePriority::Directed, None),
        )
        .await;
        assert!(
            directed.is_ok(),
            "a directed turn must get a reserved lane, never queue behind non-directed work"
        );

        // And a FURTHER non-directed call must now WAIT (its budget is full) — it
        // times out rather than stealing the lane the directed turn is using.
        let extra_nondirected = tokio::time::timeout(
            Duration::from_millis(150),
            gate.acquire_serving_lane(None, LanePriority::Ambient, None),
        )
        .await;
        assert!(
            extra_nondirected.is_err(),
            "non-directed work over its (MAX_LANES-1) budget must wait, not preempt"
        );
        drop(directed);
    }

    drop(nondirected); // release (nothing else can observe this gate anyway)
}

// what this catches: boot-created pools must shrink when the first real plan
// serves one lane, including the non-directed reservation.
#[test]
fn boot_ceiling_pools_shrink_to_the_first_served_count() {
    let gate = LaneAdmission::new();
    let serving = gate.serving_lanes();
    let nondirected = gate.nondirected_lanes();
    gate.set_served_lane_count(1);
    assert_eq!(serving.available_permits(), 1);
    assert_eq!(nondirected.available_permits(), 1);
}

// what this catches: shrink never revokes held permits; the unchanged-plan
// publication on the existing serving tick drains debt, and a newer target
// replaces the old debt without double-adding or losing capacity.
#[test]
fn held_shrink_debt_reconciles_to_the_latest_plan() {
    let gate = LaneAdmission::new();
    gate.set_served_lane_count(4);
    let serving = gate.serving_lanes();
    let nondirected = gate.nondirected_lanes();
    let held = serving.try_acquire_many(4).expect("four serving lanes");
    let nonheld = nondirected
        .try_acquire_many(3)
        .expect("three non-directed lanes");
    gate.set_served_lane_count(1);
    assert_eq!(gate.serving_installed.load(Ordering::Acquire), 4);
    assert_eq!(serving.available_permits(), 0);
    drop(held);
    drop(nonheld);
    // This call is unconditional on every existing serving-daemon tick,
    // outside the plan-fingerprint/probe-change guard.
    gate.set_served_lane_count(1);
    assert_eq!(serving.available_permits(), 1);
    assert_eq!(nondirected.available_permits(), 1);
    gate.set_served_lane_count(4);
    let held = serving.try_acquire_many(4).expect("regrown lanes");
    gate.set_served_lane_count(1);
    gate.set_served_lane_count(3); // supersedes the older shrink while held
    drop(held);
    gate.set_served_lane_count(3);
    gate.set_served_lane_count(3); // repeated stable plan is idempotent
    assert_eq!(serving.available_permits(), 3);
    assert_eq!(gate.serving_installed.load(Ordering::Acquire), 3);
    assert_eq!(nondirected.available_permits(), 2);
}

// what this catches: queued semaphore waiters receive released permits
// directly, so periodic forget_permits alone can never see idle capacity.
// Drive the production acquisition seam without sleeps or another tick.
#[tokio::test]
async fn queued_acquirers_retire_shrink_debt_before_granting() {
    for pool in 0..2 {
        let gate = LaneAdmission::new();
        gate.set_served_lane_count(4);
        let (sem, installed, budget) = match pool {
            0 => (
                gate.serving_lanes().clone(),
                &gate.serving_installed,
                LaneBudget::Physical,
            ),
            _ => (
                gate.nondirected_lanes().clone(),
                &gate.nondirected_installed,
                LaneBudget::NonDirected,
            ),
        };
        let count = sem.available_permits();
        let mut holders: Vec<_> = (0..count)
            .map(|_| sem.clone().try_acquire_owned().unwrap())
            .collect();
        let mut waiters: Vec<_> = (0..count)
            .map(|_| Box::pin(gate.acquire_resized(&sem, installed, budget)))
            .collect();
        for waiter in &mut waiters {
            assert!(futures::poll!(waiter.as_mut()).is_pending());
        }
        gate.set_served_lane_count(1);
        for _ in 1..count {
            drop(holders.pop());
            for waiter in &mut waiters {
                assert!(
                    futures::poll!(waiter.as_mut()).is_pending(),
                    "surplus must retire, not admit"
                );
            }
        }
        assert_eq!(installed.load(Ordering::Acquire), 1);
        drop(holders.pop());
        let mut granted = Vec::new();
        for waiter in &mut waiters {
            if let std::task::Poll::Ready(permit) = futures::poll!(waiter.as_mut()) {
                granted.push(permit);
            }
        }
        assert_eq!(granted.len(), 1, "only the final target capacity may run");
        drop(waiters); // cancellation must release queued reservations
        drop(granted);
        assert_eq!(sem.available_permits(), 1);
    }
}

// what this catches: canceling a work waiter while shrink retires permits
// must not leave a phantom work count that permanently excludes ambient work.
#[tokio::test]
async fn cancelled_work_waiter_clears_its_priority_reservation() {
    let gate: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
    gate.set_served_lane_count(2);
    let held = gate
        .nondirected_lanes()
        .clone()
        .try_acquire_owned()
        .unwrap();
    let mut waiting = Box::pin(gate.acquire_serving_lane(None, LanePriority::Work, None));
    assert!(futures::poll!(waiting.as_mut()).is_pending());
    assert_eq!(gate.work_waiting(), 1);
    gate.set_served_lane_count(1);
    drop(waiting);
    assert_eq!(gate.work_waiting(), 0);
    drop(held);
    let mut ambient = Box::pin(gate.acquire_nondirected_as_ambient());
    assert!(futures::poll!(ambient.as_mut()).is_ready());
}
// what this catches: serving_lane_count reads the LIVE published count when serving is
// up, and only falls back to the MAX_LANES ceiling when nothing has published yet —
// the exact fix so the admission reservation sizes by what's SERVED (e.g. 4), not the
// ceiling (6).
//
// Drives its OWN gate. The prior version swapped the process-global atomic and restored
// it at the end — brittle in two ways at once: a sibling test reading the count in the
// window between swap and restore sees a value this test invented, and a panic anywhere
// in between leaves the global corrupted for every test that follows. Owning the state
// removes both, with neither a lock nor a restore.
#[test]
fn serving_lane_count_prefers_the_live_published_count_over_the_ceiling() {
    let gate = LaneAdmission::new();
    assert_eq!(
        gate.lane_count(),
        (crate::inference::planning::serving_plan::MAX_LANES as usize).max(1),
        "unset → MAX_LANES ceiling fallback"
    );
    gate.set_served_lane_count(4);
    assert_eq!(
        gate.lane_count(),
        4,
        "published live count wins over the ceiling"
    );
}

// what this catches (2026-09-15 20:35Z, the IDLE hour): a parked WORK wait that is
// CANCELLED (the turn's future dropped — a yield to a directed line, the settle
// deadline, a despawn) must leave `work_waiting` at zero, or every ambient caller
// parks forever on a pool with free lanes. Sixteen minds sat idle for fifty
// minutes with all seven lanes free on exactly this.
#[tokio::test]
async fn a_cancelled_work_wait_is_uncounted_and_ambient_turns_flow_again() {
    use std::time::Duration;
    let gate: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
    gate.set_served_lane_count(2); // non-directed budget = 1
    let held = gate.acquire_serving_lane(None, LanePriority::Ambient, None).await;
    let work = tokio::spawn(async move { gate.acquire_serving_lane(None, LanePriority::Work, None).await });
    // Deterministic (Cormac's note on #4083): wait until the task is COUNTED, under a
    // bound, instead of a fixed sleep a loaded runner can outlast.
    let counted = tokio::time::timeout(Duration::from_secs(5), async {
        while gate.work_waiting() != 1 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    assert!(counted.is_ok(), "the work caller is counted while it waits");
    work.abort(); // the cancellation every real exit takes
    let _ = work.await;
    assert_eq!(
        gate.work_waiting(),
        0,
        "a cancelled wait is uncounted by its guard"
    );
    drop(held);
    let ambient = tokio::time::timeout(
        Duration::from_millis(1500),
        gate.acquire_serving_lane(None, LanePriority::Ambient, None),
    )
    .await;
    assert!(
        ambient.is_ok(),
        "with no work waiting, the ambient turn takes the free lane"
    );
}

// what this catches (2026-09-15, the READING hour): a mind holding a card mid-edit
// queued for the non-directed budget behind a mind musing with no card. With one
// non-directed lane taken, a WORK caller and an AMBIENT caller both arrive; when the
// lane frees, the work caller gets it and the ambient caller is still parked. And
// once no work waits, the ambient caller takes the next free lane — parked, not
// starved.
#[tokio::test]
async fn a_held_card_takes_the_lane_before_a_musing_turn() {
    use std::time::Duration;
    let gate: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
    gate.set_served_lane_count(2); // non-directed budget = 1
    let held = gate.acquire_serving_lane(None, LanePriority::Ambient, None).await;

    let work = tokio::spawn(async move { gate.acquire_serving_lane(None, LanePriority::Work, None).await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        gate.work_waiting(),
        1,
        "the work caller is counted while it waits"
    );
    let ambient =
        tokio::spawn(async move { gate.acquire_serving_lane(None, LanePriority::Ambient, None).await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!ambient.is_finished(), "ambient parks while work waits");

    drop(held);
    let work_permit = tokio::time::timeout(Duration::from_millis(500), work)
        .await
        .expect("work gets the freed lane")
        .expect("join");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !ambient.is_finished(),
        "ambient is still parked: the work caller holds the only non-directed lane"
    );
    assert_eq!(gate.work_waiting(), 0);

    drop(work_permit);
    let ambient_permit = tokio::time::timeout(Duration::from_millis(1500), ambient)
        .await
        .expect("ambient takes the lane once no work waits")
        .expect("join");
    drop(ambient_permit);
}

// what this catches (card c84d885a, S1b): the node's OWN lane wait is one more ring of the
// same shape as the seat's — a median with its count, (0, 0) when nothing was measured —
// so a chooser never reads an unmeasured home as "no queue" or a measured home as "0".
#[test]
fn a_nodes_own_lane_wait_is_a_median_with_a_count_of_the_same_shape_as_the_seats() {
    let ring = WaitRing::new();
    assert_eq!(ring.p50(), (0, 0), "unmeasured is (0, 0), never a fast lane");
    for ms in [40_000u64, 5, 210_000, 7, 9] {
        ring.note(ms);
    }
    assert_eq!(ring.p50(), (9, 5), "the median, not the mean — one outlier is not the queue");
    for _ in 0..(WAIT_RING_SAMPLES * 2) {
        ring.note(1);
    }
    assert_eq!(ring.p50(), (1, WAIT_RING_SAMPLES as u32), "bounded: the window forgets");
}
// what this catches (card cff534ba, S3): the lane wait being bounded by a CONSTANT
// instead of by the queue it is a wait on. M5, 2026-09-20 ~21:55Z (4 residents on 2
// lanes at 67,072): a `persona.act.pace` row read `residue_ms = 1,500,001` with
// `model_ms = 0` — 1500 s to the millisecond is the 25-minute per-act TICK_DEADLINE
// expiring at the serving gate, not work. A bound sized for an ACT is not a bound
// for a QUEUE. This one is `ceil((queue_ahead + 1) / lanes)` measured lane turns,
// times the slack, clamped — and UNMEASURED lanes get `None` so the caller uses the
// named ceiling rather than a short guess (busy is not dead).
#[test]
fn the_lane_wait_bound_is_derived_from_the_measured_queue_not_a_constant() {
    use std::time::Duration;
    // Never measured a lane: no number to derive from, so no derived bound.
    assert_eq!(lane_wait_bound(3, 2, 0, 0), None, "unmeasured is not a fast queue");
    assert_eq!(lane_wait_bound(3, 2, 180_000, 0), None, "zero samples is unmeasured");
    // The M5 shape: 3 minds ahead of her on 2 lanes, lanes turning at a 120 s median.
    // Her place in line is 4th → ceil(4/2) = 2 turns ahead → 2 × 120 s × slack.
    assert_eq!(
        lane_wait_bound(3, 2, 120_000, 12),
        Some(Duration::from_secs(2 * 120 * LANE_WAIT_SLACK as u64)),
        "two rounds of measured lane turnover, with slack"
    );
    // Double the queue and she waits proportionally longer — the bound MOVES with
    // the measurement, which is the whole point of deriving it.
    assert_eq!(
        lane_wait_bound(7, 2, 60_000, 12),
        Some(Duration::from_secs(4 * 60 * LANE_WAIT_SLACK as u64)),
        "four rounds ahead of her"
    );
    // An empty queue on a fast node still waits the floor — a median is not a promise.
    assert_eq!(lane_wait_bound(0, 4, 1_000, 30), Some(LANE_WAIT_FLOOR));
    // A long queue on slow lanes is clamped: past this it is a starve, not a queue,
    // and she defers with her act budget intact instead of burning the act deadline.
    assert_eq!(lane_wait_bound(40, 1, 600_000, 30), Some(LANE_WAIT_CEILING));
    assert!(
        LANE_WAIT_CEILING < Duration::from_secs(25 * 60),
        "the wait's own bound must trip BEFORE the per-act TICK_DEADLINE, or the act \
         deadline is still what ends the wait (residue_ms = 1,500,001)"
    );
    // Zero lanes cannot divide by zero: the floor of one lane is the honest read.
    assert!(lane_wait_bound(1, 0, 120_000, 5).is_some());
}

// what this catches (card ebce2ba0 — the measured families, one to one): the queue's
// estimate is a FLOOR under the mind's own measured turn, never a ceiling over it.
//
// The M5 2026-09-20 logged `delib.gate.lane_wait bound_secs=165 lane_hold_p50_ms=82918
// queue_ahead=0 lanes_serving=2` SIX times, and six generations died with the capture
// reading "request future dropped before a terminal response" at 165,843 / 165,844 /
// 165,845 / 165,856 / 165,917 / 165,982 ms — each within milliseconds of that bound,
// each first in line, none having reached the model. The same derivation at a later
// p50 gave the ~412 s (four) and ~485 s (two) families, and 60,009 ms is the floor
// itself. A median over TWO recorded lane holds decided how long a mind waits while
// the lanes were actually holding four to seven minutes.
#[test]
fn the_queues_estimate_is_a_floor_under_her_own_measured_turn_never_a_ceiling() {
    use crate::inference::turn_bound::{effective_bound, effective_bound_with_source, from_expectation, BoundSource};
    use std::time::Duration;
    // The exact row: p50 82,918 ms over 2 samples, nobody ahead, 2 lanes serving.
    let queue = lane_wait_bound(0, 2, 82_918, 2).expect("measured");
    assert_eq!(queue, Duration::from_millis(165_836), "the 165 s the six minds waited");
    // A mind whose own turns measure ~7 minutes on this box may not be told a lane is
    // hopeless in 165 s. Her bound is her expectation with headroom, and it wins.
    let hers = from_expectation(Some(Duration::from_secs(424))).expect("a measured turn");
    let (bound, source) = effective_bound_with_source(queue, Some(hers));
    assert_eq!(source, BoundSource::TurnBound);
    assert_eq!(bound, hers);
    assert!(bound > queue, "the measured turn RAISES the wait; 165 s killed healthy work");
    // AND `LANE_WAIT_CEILING` has stopped being a ceiling on the WAIT. It still
    // clamps the QUEUE's own estimate (a queue estimate past ten minutes is a starve,
    // not a queue) — `lane_wait_bound(7, 1, 300_000, 30)` computes 4,800 s and returns
    // the 600 s ceiling — but her measured turn rides ABOVE that clamp, which is the
    // whole point: a constant may raise a wait and may never shorten one.
    let clamped = lane_wait_bound(7, 1, 300_000, 30).expect("measured");
    assert_eq!(clamped, LANE_WAIT_CEILING, "the queue's estimate is still clamped");
    assert_eq!(
        effective_bound(clamped, Some(hers)),
        hers,
        "her 848 s turn is NOT capped by the 600 s ceiling — the constant is a floor"
    );
    // …and the queue keeps its own number whenever IT is the larger of the two: a
    // mind with a one-minute turn on a queue two rounds deep waits the queue's 480 s.
    let deep = lane_wait_bound(3, 2, 120_000, 12).expect("measured");
    let short = from_expectation(Some(Duration::from_secs(60))).expect("a measured turn");
    assert_eq!(deep, Duration::from_secs(480));
    assert_eq!(effective_bound(deep, Some(short)), deep, "the larger of the two, always");
    // No measured turn (her first, or an unmeasured box) → the queue governs alone,
    // exactly as before this change.
    assert_eq!(effective_bound(queue, None), queue);
    assert_eq!(effective_bound(queue, from_expectation(None)), queue);
    // THE COMPOSITION INVARIANT the flat ceiling used to carry: the gate must still
    // trip before the ACT does, now that both are derived. The act covers two turn
    // bounds, the gate one — so the gate trips first at every measured rate.
    let (act, _) = crate::inference::turn_bound::act_bound_with_source(
        crate::inference::turn_bound::TICK_DEADLINE,
        Some(Duration::from_secs(424)),
    );
    assert!(act > bound, "the act must outlast the lane wait it contains");
}

// what this catches (the M5, 2026-10-28): a reserved lane idling for a mention while work
// queues. Lent whenever nothing directed is pending, whatever the call's length (a 71 s
// median act never fit the old budget, so the reserve was never lent); a pending directed
// line re-arms it. (The loan itself is a try-acquire on a FREE physical lane, never a park;
// that lives in `acquire_serving_lane`.)
#[test]
fn the_reserved_lane_is_lent_to_work_whenever_nothing_directed_is_pending() {
    assert!(reserve_lendable(false), "no slot idles for a mention that is not pending");
    assert!(!reserve_lendable(true), "a pending directed line re-arms the reserve");
}

// what this catches: the health line's directed-wait numbers are nearest-rank
// percentiles over the ring — one sample is its own p90, ten samples put p90 at the
// ninth, and an empty ring is (0, 0, 0), never a fabricated zero wait.
#[test]
fn directed_wait_percentiles_are_nearest_rank() {
    assert_eq!(percentiles_of(std::iter::empty()), (0, 0, 0));
    assert_eq!(percentiles_of([7].into_iter()), (7, 7, 1));
    assert_eq!(percentiles_of((1..=10).map(|x| x * 100)), (600, 900, 10));
    assert_eq!(percentiles_of([5, 1, 9, 3].into_iter()), (5, 9, 4));
}


// A busy directed reserve is not evidence that a stuck work budget advances.
#[tokio::test]
async fn queue_progress_names_only_blocking_budget_holders() {
    let gate: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
    gate.set_served_lane_count(2);
    let worker = uuid::Uuid::new_v4();
    let directed = uuid::Uuid::new_v4();
    let work = gate.acquire_serving_lane(Some(worker), LanePriority::Work, None).await;
    let reserve = gate.acquire_serving_lane(Some(directed), LanePriority::Directed, None).await;
    let (wait, acquire) = gate.serving_lane_request(None, LanePriority::Work, None);
    let before = wait.progress();
    assert_eq!(before.holders.len(), 1);
    assert_eq!(before.holders[0].1, worker);
    drop(reserve);
    let reserve = gate.acquire_serving_lane(Some(directed), LanePriority::Directed, None).await;
    assert_eq!(wait.progress().grants, before.grants, "reserved-lane churn cannot hide leaked work permit");
    drop(acquire);
    drop(work);
    drop(reserve);
    assert!(wait.progress().holders.is_empty());
}

// The dependency crosses budgets when a nondirected holder is still waiting
// for physical capacity: the physical holders are then relevant to its follower.
#[tokio::test]
async fn queue_progress_follows_budget_holders_waiting_for_physical_slots() {
    let gate: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
    gate.set_served_lane_count(2);
    let one = uuid::Uuid::new_v4();
    let two = uuid::Uuid::new_v4();
    let first = gate.acquire_serving_lane(Some(one), LanePriority::Directed, None).await;
    let second = gate.acquire_serving_lane(Some(two), LanePriority::Directed, None).await;
    let (_, acquire) = gate.serving_lane_request(None, LanePriority::Work, None);
    tokio::pin!(acquire);
    assert!(std::future::poll_fn(|cx| std::task::Poll::Ready(std::future::Future::poll(acquire.as_mut(), cx).is_pending())).await);
    let (follower, queued) = gate.serving_lane_request(None, LanePriority::Work, None);
    let holders = follower.progress().holders;
    assert_eq!(holders.len(), 2);
    assert!(holders.iter().any(|(_, persona)| *persona == one));
    assert!(holders.iter().any(|(_, persona)| *persona == two));
    drop(queued);
    drop(first);
    drop(second);
}
