//! Which of this node's lanes host a live in-engine training run.
//!
//! An in-engine run (`genome::fine_tuning::engine_lora_adapter`) trains INSIDE the serving
//! lane's `llama-server`. Serving must know that, in two places, or it destroys the run it
//! hosts (Kimi's attempt 2 on the 5090, 2026-09-28 12:29-12:30Z, job 4169e970):
//!
//! 1. The lane's footprint sample (`serving_daemon::sample_lane_footprint`) reads everything
//!    the lane holds beyond its weights as serving cost. The training graph, optimizer and
//!    adapter (~7.4 GB there) went into the model's record as fixed per-lane residency, the 27B
//!    stopped fitting, and `usable_gb` read 0.
//! 2. The reconcile then acted on that plan: it downshifted the 27B to a 1.5B, and the swap
//!    killed the lane, and the run with it.
//!
//! So the run registers here for its whole life (an RAII [`TrainingOnLane`] held by the run),
//! and serving asks [`training_resident_on`] before it measures or relaunches that lane.
//! This registry is node-local memory and dies with the core: the run lives in the core's
//! task, so a relaunched core has no run to protect until re-attach exists (card 7bb4e5a2).
//!
//! Keyed by the lane's root url (`LaneRecord::root_url`), trimmed the way a hold scope is:
//! a trailing slash must never make a registration that protects nothing (the #4485 lesson).

use std::collections::BTreeMap;
use std::sync::LazyLock;

/// How long after a run ends its lane still counts as training-resident. The engine frees
/// the training context when the run ends, but a footprint sample taken in the same breath
/// can still see it; one sample interval later it cannot.
// derived-or-floor: derived — one footprint sample interval.
pub const TRAINING_SETTLE_MS: u64 = super::lane_footprint::SAMPLE_EVERY_MS;

#[derive(Debug, Default, Clone, Copy)]
struct LaneTraining {
    /// live runs on the lane (the engine runs one at a time; a count keeps a racing end and
    /// start honest)
    live: u32,
    /// when the last run on the lane ended (unix ms), for [`TRAINING_SETTLE_MS`]
    ended_ms: u64,
    /// why serving replaced the lane under a live run, if it had to (a real emergency), so
    /// the run's failure names it instead of "the lane is gone"
    lost: Option<&'static str>,
}

impl LaneTraining {
    fn resident_at(&self, now_ms: u64) -> bool {
        self.live > 0
            || (self.ended_ms > 0 && now_ms.saturating_sub(self.ended_ms) < TRAINING_SETTLE_MS)
    }
}

static LANES: LazyLock<parking_lot::Mutex<BTreeMap<String, LaneTraining>>> =
    LazyLock::new(|| parking_lot::Mutex::new(BTreeMap::new()));

fn key(lane_url: &str) -> String {
    lane_url.trim_end_matches('/').to_string()
}

fn now_ms() -> u64 {
    crate::persona::trace::now_ms()
}

/// A live run's registration on its lane; dropping it (the run ended, however) releases it.
#[must_use = "the lane counts as training only while this is held"]
pub struct TrainingOnLane {
    key: String,
}

impl Drop for TrainingOnLane {
    fn drop(&mut self) {
        let mut lanes = LANES.lock();
        if let Some(t) = lanes.get_mut(&self.key) {
            t.live = t.live.saturating_sub(1);
            t.ended_ms = now_ms();
            if t.live == 0 {
                t.lost = None;
            }
        }
    }
}

/// Register a live run on `lane_url` for as long as the returned guard is held.
pub fn training_starts_on(lane_url: &str) -> TrainingOnLane {
    let key = key(lane_url);
    LANES.lock().entry(key.clone()).or_default().live += 1;
    TrainingOnLane { key }
}

/// Does `lane_url` host a live run, or one that ended within [`TRAINING_SETTLE_MS`] of `now_ms`?
pub fn training_resident_on(lane_url: &str, now_ms: u64) -> bool {
    LANES
        .lock()
        .get(&key(lane_url))
        .is_some_and(|t| t.resident_at(now_ms))
}

/// Does ANY lane host a live or settling run? The cheap first question for a caller that
/// must otherwise resolve its lane's url (a file read) on every tick.
pub fn any_resident(now_ms: u64) -> bool {
    LANES.lock().values().any(|t| t.resident_at(now_ms))
}

/// Serving is replacing `lane_url` under a live run anyway (an emergency it may not wait
/// out). Recorded so the run's failure says why; returns whether a run was live there.
pub fn lane_lost_under_training(lane_url: &str, why: &'static str) -> bool {
    let mut lanes = LANES.lock();
    match lanes.get_mut(&key(lane_url)) {
        Some(t) if t.live > 0 => {
            t.lost = Some(why);
            true
        }
        _ => false,
    }
}

/// Why serving replaced this run's lane under it, if it did.
pub fn lost_reason(lane_url: &str) -> Option<&'static str> {
    LANES.lock().get(&key(lane_url)).and_then(|t| t.lost)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: Kimi's attempt 2 (5090, 2026-09-28): serving measured and replaced a
    // lane that hosted a live run, because nothing told it one was there. A registration
    // protects its lane however the url is spelled, lasts exactly as long as the run plus
    // the settle interval, protects no other lane, and an emergency replacement is named.
    #[test]
    fn a_live_run_marks_its_lane_until_it_ends_and_settles() {
        let lane = "http://127.0.0.1:61001";
        let guard = training_starts_on(lane);
        let now = now_ms();
        assert!(
            training_resident_on(lane, now) && training_resident_on("http://127.0.0.1:61001/", now),
            "either spelling"
        );
        assert!(
            !training_resident_on("http://127.0.0.1:61002", now),
            "another lane is not training"
        );
        assert!(any_resident(now));
        assert!(lane_lost_under_training(
            lane,
            "emergency: engine quarantined"
        ));
        assert_eq!(lost_reason(lane), Some("emergency: engine quarantined"));
        drop(guard);
        let ended = now_ms();
        assert!(
            training_resident_on(lane, ended),
            "an ended run's lane settles for one sample interval"
        );
        assert!(
            !training_resident_on(lane, ended + TRAINING_SETTLE_MS),
            "then it is serving's again"
        );
        assert!(
            !lane_lost_under_training(lane, "x"),
            "no live run: nothing to lose"
        );
        assert_eq!(lost_reason(lane), None, "the reason ends with the run");
    }
}
