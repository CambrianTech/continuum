//! The replay: every act of a sequence, in order, through `page_in` then `act`, each
//! measured. A failed act is recorded and the replay goes on, so an engine that dies
//! shows up in the report as failed acts and relaunches instead of ending the run.

use std::collections::HashMap;
use std::time::Instant;

use crate::acts::{available_reuse, ActSequence};
use crate::report::ActRecord;
use crate::target::{ActRequest, EngineStats, MindId, ServingTarget, TargetError};

pub struct Replayed {
    pub acts: Vec<ActRecord>,
    pub engine: EngineStats,
    pub wall_ms: f64,
}

pub fn run(target: &mut dyn ServingTarget, sequence: &ActSequence, progress: bool) -> Result<Replayed, TargetError> {
    let started = Instant::now();
    let mut last: HashMap<&MindId, &ActRequest> = HashMap::new();
    let mut records = Vec::with_capacity(sequence.acts.len());
    for (index, act) in sequence.acts.iter().enumerate() {
        let available = available_reuse(last.get(&act.mind).copied(), &act.request);
        let first_act = !last.contains_key(&act.mind);
        last.insert(&act.mind, &act.request);
        let t0 = Instant::now();
        let page_in = target.page_in(&act.mind);
        let outcome = match &page_in {
            Ok(_) => Some(target.act(&act.mind, &act.request)),
            Err(_) => None,
        };
        let wall_ms = t0.elapsed().as_secs_f64() * 1e3;
        let error = match (&page_in, &outcome) {
            (Err(e), _) | (_, Some(Err(e))) => Some(e.to_string()),
            _ => None,
        };
        let record = ActRecord {
            index,
            mind: act.mind.clone(),
            source: act.source.clone(),
            first_act,
            available_reuse: available,
            page_in: page_in.ok(),
            outcome: outcome.and_then(Result::ok),
            wall_ms,
            error,
        };
        if progress {
            eprintln!("{}", record.line());
        }
        records.push(record);
    }
    let wall_ms = started.elapsed().as_secs_f64() * 1e3;
    Ok(Replayed { acts: records, engine: target.stats()?, wall_ms })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeEngine;
    use crate::report::Summary;
    use crate::seated::{page_name, SeatedTarget};
    use crate::synthetic::{build, SyntheticSpec};
    use crate::target::PageInKind;

    fn four_minds() -> ActSequence {
        build(&SyntheticSpec { minds: 4, acts: 32, shared_head_chars: 400, base_chars: 3000, tail_chars: 300, reply_chars: 80, switch: 0.6, seed: 11, n_predict: 2 })
    }

    // what this catches: the replay misreporting the paging economy. Four minds on two
    // seats must show one cold act per mind and no more, displacements that save a page,
    // restores that bring a mind back with her prefix (warm reuse at the ceiling, since
    // the fake restores exactly), every act accounted for, and zero relaunches.
    #[test]
    fn four_minds_on_two_seats_page_and_reuse() {
        let seq = four_minds();
        let mut target = SeatedTarget::new(FakeEngine::new(2));
        let replayed = run(&mut target, &seq, false).expect("replay"); // expect: the fake never fails stats
        let s = Summary::of(&replayed.acts, &replayed.engine);
        assert_eq!((s.acts, s.failed, s.relaunches), (32, 0, Some(0)));
        assert_eq!(s.page_ins.get(&PageInKind::Cold), Some(&4));
        assert!(s.page_ins.get(&PageInKind::Restored).is_some_and(|&n| n > 0));
        assert!(s.evictions > 0);
        assert_eq!(s.page_ins.values().sum::<usize>(), 32);
        assert!(s.warm_reuse > 0.85 && s.warm_reuse <= 1.0, "warm reuse {}", s.warm_reuse);
        assert!(s.warm_reuse >= s.warm_available_reuse - 0.02, "{} vs ceiling {}", s.warm_reuse, s.warm_available_reuse);
    }

    // what this catches: an engine death hidden by the replay. The failed act must be
    // recorded, the run must continue, the relaunch must be counted, and the next act
    // must not claim its KV was resident (the relaunch dropped it). A refused restore is
    // its own kind, never folded into a cold start.
    #[test]
    fn a_dead_engine_is_a_failed_act_and_a_counted_relaunch() {
        let seq = four_minds();
        let mut engine = FakeEngine::new(2);
        engine.die_before_act = Some(5);
        engine.refuse_restore = vec![page_name(&MindId("mind-0".into()))];
        let mut target = SeatedTarget::new(engine);
        let replayed = run(&mut target, &seq, false).expect("replay"); // expect: the fake never fails stats
        let s = Summary::of(&replayed.acts, &replayed.engine);
        assert_eq!((s.failed, s.relaunches), (1, Some(1)));
        assert!(s.page_ins.get(&PageInKind::RestoreRejected).is_some_and(|&n| n > 0));
        let after = &replayed.acts[6];
        assert_ne!(after.page_in.as_ref().map(|p| p.kind), Some(PageInKind::Resident));
        assert_eq!(replayed.acts.len(), 32);
    }
}
