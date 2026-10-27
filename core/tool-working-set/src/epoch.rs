//! A mind's history cut into epochs: the spans one head serves.
//!
//! The live core turns her view over when it no longer fits (`delib.view.turnover`), and a
//! relaunch drops the KV regardless. Neither is recorded across her whole history, so the
//! replay models an epoch as a measured wall-clock span: it starts at an act and holds every
//! act within `span_ms` of that start, and the next epoch starts at the next act after it.
//! Idle time therefore creates no empty epochs.

use std::ops::Range;

use crate::history::{EpochMs, Call};

/// One epoch: when it turned over and the calls it held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Epoch {
    pub start: EpochMs,
    /// Indexes into the mind's calls, oldest first.
    pub calls: Range<usize>,
}

/// Cut `calls` (oldest first) into epochs of at most `span_ms` each.
pub fn segment(calls: &[Call], span_ms: u64) -> Vec<Epoch> {
    let mut epochs: Vec<Epoch> = Vec::new();
    for (index, call) in calls.iter().enumerate() {
        match epochs.last_mut() {
            Some(open) if call.at.since(open.start).millis() < span_ms => open.calls.end = index + 1,
            _ => epochs.push(Epoch { start: call.at, calls: index..index + 1 }),
        }
    }
    epochs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::{ToolSchema, Vocabulary};

    // what this catches: an epoch that starts on a clock grid instead of at her act (idle
    // hours would become empty epochs and dilute every per-epoch number), or a call landing
    // in the wrong epoch at the span boundary.
    #[test]
    fn epochs_start_at_an_act_and_hold_what_falls_inside_the_span() {
        let vocab = Vocabulary::new(vec![ToolSchema { name: "code/read".into(), description: String::new(), tokens: 1 }]);
        let tool = vocab.expect("one verb").id("code/read").expect("listed");
        let call = |ms: u64| Call { at: EpochMs::from_millis(ms), tool, activity: None, text: String::new() };
        let calls: Vec<Call> = [0, 10, 99, 100, 5_000, 5_050].into_iter().map(call).collect();
        let epochs = segment(&calls, 100);
        assert_eq!(
            epochs,
            vec![
                Epoch { start: EpochMs::from_millis(0), calls: 0..3 },
                Epoch { start: EpochMs::from_millis(100), calls: 3..4 },
                Epoch { start: EpochMs::from_millis(5_000), calls: 4..6 },
            ]
        );
    }
}
