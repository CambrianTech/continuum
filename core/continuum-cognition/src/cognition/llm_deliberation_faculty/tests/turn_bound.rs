#![cfg(test)]

use super::*;

// what this catches (card ba82d0a0): a turn's bound is its MEASURED occupancy —
// the uncached prompt at the box's prefill rate plus the output at its decode
// rate — with headroom, and on a slow box it outgrows BOTH fixed floors (300 s
// pre-stream, 600 s remote) that used to kill the turn and read it as dead.
// The IntelMac's shape: 30k cold at ~25 tok/s. A warm cache pays only the
// uncached prefix. An unmeasured or NaN rate is an absence, never a number.
#[test]
fn a_turns_bound_is_its_measured_occupancy_with_headroom_and_outgrows_every_floor() {
    let cold = TurnShape { input: 30_000, cached: 0, output: 800 };
    let expected = occupancy_of(cold, 25.0, 10.0).expect("both rates measured");
    assert_eq!(expected.as_secs(), 1_280, "1200 s of prefill + 80 s of decode");
    let bound = crate::inference::turn_bound::from_expectation(Some(expected))
        .expect("an expectation yields a bound");
    assert_eq!(bound.as_secs(), 2_560);
    assert!(bound > std::time::Duration::from_secs(crate::inference::lane_send::PRE_STREAM_HEADER_TIMEOUT_SECS));
    assert!(bound > crate::inference::airc_remote::transport::REMOTE_INFERENCE_DEADLINE);
    let warm = TurnShape { input: 30_000, cached: 29_000, output: 800 };
    assert_eq!(occupancy_of(warm, 25.0, 10.0).map(|d| d.as_secs()), Some(120));
    assert_eq!(occupancy_of(cold, 0.0, 10.0), None, "an unmeasured prefill rate is not a number");
    assert_eq!(occupancy_of(cold, 25.0, f64::NAN), None, "a NaN decode rate is not a number");
}

// what this catches: the faculty's wire bound IS `from_expectation` of her
// expected occupancy — never a constant, never fabricated. A fresh mind has no
// measured turn behind her, so the request carries no bound and the floors
// govern alone; the Some path is the pure test above (her expectation reads
// this box's serving snapshot and rate ledgers, which a test must not seed).
#[test]
fn a_fresh_mind_carries_no_bound_and_the_floors_govern() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter);
    assert_eq!(faculty.expected_occupancy(), None, "no last turn = no expectation");
    assert_eq!(faculty.turn_bound(), None, "no expectation = no bound (an absence is not a number)");
    assert_eq!(
        crate::inference::turn_bound::effective_bound(
            std::time::Duration::from_secs(crate::inference::lane_send::PRE_STREAM_HEADER_TIMEOUT_SECS),
            faculty.turn_bound(),
        ),
        std::time::Duration::from_secs(crate::inference::lane_send::PRE_STREAM_HEADER_TIMEOUT_SECS),
        "…so the floor governs"
    );
}

// what this catches: the mind's and the engine's page disagreeing. A resident faculty's
// turns, in any room, must announce purposes the engine keys onto her ONE mind page
// (`warm_ahead` warms that same page); a fork's (`fork_eval_cycle`) must key its own page
// per room, or an eval exam would overwrite her warm view and wait behind her act.
#[test]
fn a_residents_turns_warm_her_one_page_and_a_forks_never_do() {
    use crate::inference::slots::class_for;
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let her = Uuid::new_v4();
    let (general, academy) = (Uuid::new_v4(), Uuid::new_v4());
    let resident = LlmDeliberationFaculty::new(her, "Ivar", "You are Ivar.", adapter.clone());
    let fork = LlmDeliberationFaculty::new(her, "Ivar", "You are Ivar.", adapter).forked();
    for kind in [TurnKind::Act, TurnKind::Pass] {
        for room in [general, academy] {
            let engine = |f: &LlmDeliberationFaculty| class_for(Some(f.turn_purpose(kind))).page_key(Some(her), Some(room));
            assert_eq!(engine(&resident), resident.page(room), "the engine keys the page the mind warms ahead");
            assert_eq!(engine(&fork), fork.page(room));
            assert_ne!(fork.page(room), resident.page(room), "a fork never warms her page");
        }
    }
    assert_eq!(resident.page(general), resident.page(academy), "one page across her rooms");
    assert_ne!(fork.page(general), fork.page(academy), "a fork's page is per room");
}
