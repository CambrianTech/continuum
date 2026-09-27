//! THE PREFILL KNEE, measured and observe-only (card e370a673, slice 1).
//!
//! The IntelMac, 2026-09-27, one hour with no build running: 6 slots x 32k on a 6-core CPU,
//! the whole server prefilling one 2,048-token ubatch per ~45 s (~45 tok/s), 42 of 45
//! generations failed. The plan sized that lane by memory; on a CPU lane the limit is the
//! prefill rate, shared by every slot. The decode side already has its knee
//! (`inference::decode_knee`); this is the prefill side's measurement.
//!
//! Two inputs, both chosen so the bound cannot feed on itself (Cormac's review of the
//! design):
//! - the SERVER-TOTAL prefill rate, read from `/slots` (never the per-request rate, which
//!   reads about total/N with N slots prefilling, so bounding N by it would loop). Counted
//!   only across intervals when some slot still has prompt left to prefill, and published
//!   only once those busy intervals span [`WINDOW_BUSY_MS`]: counters move in whole
//!   ubatches, so a short window reads 0 or a burst, and an idle server is not a
//!   0 tok/s server.
//! - the per-turn PREFILLED tokens (what the lane had to read, cache hits excluded), from
//!   the one generation seam, with how many turns were cold, so a lane cut that turns
//!   reuse into re-prefill shows up instead of hiding in a median.
//!
//! Slice 1 CLAMPS NOTHING. It publishes `serving.prefill_knee.would_clamp`: the bound these
//! inputs would set beside the lanes actually served. The clamp is slice 2, after a real
//! hour shows the bound is stable and the prefilled median does not climb with the lanes.

use std::sync::LazyLock;

/// Busy time a rate must span before it is published: several ubatches on the slowest
/// lane measured (one 2,048-token ubatch per ~45 s on the IntelMac).
// derived-or-floor: a floor — five minutes spans ~7 ubatches at the slowest measured rate, so the per-ubatch step averages out.
pub const WINDOW_BUSY_MS: u64 = 5 * 60 * 1000;

/// The longest gap between two reads that still counts as one interval. A longer gap means
/// reads failed in between (the engine was down or relaunching), and the downtime is not
/// prefill time: counting it would publish a falsely slow rate right after a relaunch, one
/// this knee's own clamp would cause (Kimi on #4454).
// derived-or-floor: a ceiling — several health ticks; only ever DISCARDS an interval, never shortens one.
pub const MAX_READ_GAP_MS: u64 = 2 * 60 * 1000;

/// Turns kept for the prefilled median: the recent regime, not the day.
// derived-or-floor: a floor — enough turns that one cold outlier moves the median by nothing.
pub const TURNS_KEPT: usize = 64;

/// One slot as `/slots` reports it: the task it serves, how many prompt tokens that task
/// has, and how many it has prefilled so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotCount {
    pub id: u64,
    pub task: i64,
    pub prompt: u64,
    pub processed: u64,
    pub processing: bool,
}

impl SlotCount {
    /// This slot is serving a request that still has prompt left to read. `is_processing`
    /// is required: a slot cancelled mid-prefill keeps its stale counters while idle, and
    /// counting it made every later interval busy with no tokens, so the rate drifted to 0,
    /// toward fewer lanes (Cormac on #4454; the IntelMac's slot 3 read 2048 / 0 at 07:08Z).
    fn prefilling(&self) -> bool {
        self.processing && self.processed < self.prompt
    }
}

/// Read the slot counters from a `/slots` body. `None` when it is not the slots array.
pub fn slot_counts_of(slots: &serde_json::Value) -> Option<Vec<SlotCount>> {
    let n = |v: &serde_json::Value| v.as_u64().unwrap_or(0); // JUSTIFIED unwrap_or: an absent counter is a slot with nothing prefilled
    Some(
        slots
            .as_array()?
            .iter()
            .map(|s| SlotCount {
                id: n(&s["id"]),
                task: s["id_task"].as_i64().unwrap_or(-1), // JUSTIFIED unwrap_or: no task id is its own task (-1), never equal to a real one
                prompt: n(&s["n_prompt_tokens"]),
                processed: n(&s["n_prompt_tokens_processed"]),
                processing: s["is_processing"].as_bool() == Some(true),
            })
            .collect(),
    )
}

/// Tokens prefilled between two reads, summed over slots. A slot on the same task counts
/// its advance; a slot on a new task counts that task's progress so far (the tail of the
/// old task between reads is missed, which only ever under-counts).
fn prefilled_between(before: &[SlotCount], now: &[SlotCount]) -> u64 {
    now.iter()
        .map(|slot| match before.iter().find(|b| b.id == slot.id) {
            Some(b) if b.task == slot.task => slot.processed.saturating_sub(b.processed),
            _ => slot.processed,
        })
        .sum()
}

/// The server-total prefill rate over busy intervals only.
#[derive(Debug, Default)]
pub struct PrefillWindow {
    last: Option<(Vec<SlotCount>, u64)>,
    tokens: u64,
    busy_ms: u64,
}

impl PrefillWindow {
    /// Feed one `/slots` read. Returns the server-total prefill rate (tok/s) once the busy
    /// intervals seen since the last publication span [`WINDOW_BUSY_MS`], then starts a new
    /// window. An interval counts as busy when some slot had prompt left to read at its
    /// start; idle intervals add neither time nor tokens.
    pub fn observe(&mut self, now: Vec<SlotCount>, now_ms: u64) -> Option<f64> {
        if let Some((before, at)) = self.last.take() {
            // A relaunch between the reads: the gap held no reads, or a slot's task counter
            // went backwards (a fresh engine numbers its tasks from zero). Neither interval
            // is prefill time; the window resumes from this read.
            let restarted = now.iter().any(|s| before.iter().any(|b| b.id == s.id && s.task < b.task));
            if now_ms.saturating_sub(at) <= MAX_READ_GAP_MS && !restarted && before.iter().any(SlotCount::prefilling) {
                self.tokens += prefilled_between(&before, &now);
                self.busy_ms += now_ms.saturating_sub(at);
            }
        }
        self.last = Some((now, now_ms));
        if self.busy_ms < WINDOW_BUSY_MS || self.tokens == 0 {
            return None;
        }
        let rate = self.tokens as f64 * 1000.0 / self.busy_ms as f64;
        self.tokens = 0;
        self.busy_ms = 0;
        Some(rate)
    }
}

/// The last [`TURNS_KEPT`] turns' prefilled tokens, and whether each was cold (no cache hit).
#[derive(Debug, Default)]
pub struct TurnPrefill {
    turns: std::collections::VecDeque<(u32, bool)>,
}

impl TurnPrefill {
    pub fn note(&mut self, cached: u32, prefilled: u32) {
        if self.turns.len() == TURNS_KEPT {
            self.turns.pop_front();
        }
        self.turns.push_back((prefilled, cached == 0));
    }
    /// Median prefilled tokens per turn; `None` with no turns.
    pub fn median(&self) -> Option<u32> {
        let mut v: Vec<u32> = self.turns.iter().map(|(p, _)| *p).collect();
        if v.is_empty() {
            return None;
        }
        v.sort_unstable();
        Some(v[v.len() / 2])
    }
    pub fn turns(&self) -> usize {
        self.turns.len()
    }
    pub fn cold(&self) -> usize {
        self.turns.iter().filter(|(_, cold)| *cold).count()
    }
}

/// The lanes a server-total prefill rate can serve inside `ttft` at `per_turn` prefilled
/// tokens a turn: at least one. `None` when a turn costs no prefill (all cache): prefill is
/// then not the limit.
pub fn prefill_lanes(rate_tps: f64, per_turn: u32, ttft: std::time::Duration) -> Option<u32> {
    if per_turn == 0 || !rate_tps.is_finite() || rate_tps <= 0.0 {
        return None;
    }
    let lanes = (rate_tps * ttft.as_secs_f64() / f64::from(per_turn)).floor();
    Some((lanes as u32).max(1))
}

static WINDOW: LazyLock<parking_lot::Mutex<PrefillWindow>> = LazyLock::new(Default::default);
static TURNS: LazyLock<parking_lot::Mutex<TurnPrefill>> = LazyLock::new(Default::default);

/// The generation seam: one turn's cache split (fed beside `citizen_health::note_generation`).
/// Both 0 = the lane reported no timings: an absence, never a datum.
pub fn note_turn(cached: u32, prefilled: u32) {
    if cached == 0 && prefilled == 0 {
        return;
    }
    TURNS.lock().note(cached, prefilled);
}

/// Feed one `/slots` read; when a window completes, publish the bound these inputs would
/// set beside the lanes served. Observe-only: nothing reads this to size a lane.
pub fn observe_slots(slots: &serde_json::Value, now_ms: u64, served_lanes: usize) {
    let Some(counts) = slot_counts_of(slots) else { return };
    let Some(rate) = WINDOW.lock().observe(counts, now_ms) else { return };
    let turns = TURNS.lock();
    let median = turns.median();
    let ttft = crate::inference::prefill_rate::UNATTENDED_TTFT;
    let bound = median.and_then(|m| prefill_lanes(rate, m, ttft));
    crate::probe!(
        class = "serving.prefill_knee.would_clamp",
        server_prefill_tps = rate,
        prefilled_median = median.unwrap_or(0), // probe field: 0 = no turn measured yet
        turns = turns.turns() as u64,
        cold_turns = turns.cold() as u64,
        ttft_secs = ttft.as_secs(),
        served_lanes = served_lanes as u64,
        would_bound = bound.unwrap_or(0), // probe field: 0 = no bound (no turns, or all cache)
        would_clamp = bound.is_some_and(|b| (b as usize) < served_lanes),
        "prefill knee, observe-only: the lanes this server's measured prefill could serve inside the turn budget, beside the lanes it serves"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(id: u64, task: i64, prompt: u64, processed: u64) -> SlotCount {
        SlotCount { id, task, prompt, processed, processing: processed < prompt }
    }

    // what this catches: a rate that swings with the ubatch step, or reads an idle server
    // as a 0 tok/s one (Cormac on the e370a673 design). The IntelMac shape: one 2,048-token
    // ubatch per ~45 s for the whole server, read every 15 s; the rate over five busy
    // minutes is ~45 tok/s, idle stretches add nothing, and a new task on a slot counts its
    // own progress.
    #[test]
    fn the_server_prefill_rate_spans_ubatches_and_ignores_idle_time() {
        let mut w = PrefillWindow::default();
        let mut t = 0u64;
        let mut processed = 0u64;
        let mut out = None;
        // an idle hour first: no slot has prompt left, nothing is counted
        for _ in 0..240 {
            assert_eq!(w.observe(vec![slot(0, 1, 100, 100)], t), None);
            t += 15_000;
        }
        // then a 20k prompt prefilled at one ubatch per 45 s, read every 15 s
        for step in 0..40u64 {
            if step % 3 == 2 {
                processed = (processed + 2048).min(20_000);
            }
            if let Some(r) = w.observe(vec![slot(0, 2, 20_000, processed), slot(1, 9, 0, 0)], t) {
                out = Some(r);
                break;
            }
            t += 15_000;
        }
        let rate = out.expect("five busy minutes publish a rate");
        assert!((40.0..=50.0).contains(&rate), "~45 tok/s, not a per-read swing: {rate}");
        assert_eq!(prefilled_between(&[slot(0, 1, 50, 50)], &[slot(0, 2, 900, 300)]), 300, "a new task counts its own progress");
    }

    // what this catches: a relaunch published as a slow rate (Kimi on #4454). The knee's
    // own clamp relaunches the lane; the gap with no reads, and a fresh engine whose task
    // counter starts over, must add neither time nor tokens, or the first window after the
    // clamp reads the downtime as slow prefill and pushes the lanes down again.
    #[test]
    fn a_relaunch_between_reads_is_not_prefill_time() {
        let mut w = PrefillWindow::default();
        assert_eq!(w.observe(vec![slot(0, 500, 20_000, 4_096)], 0), None);
        // ten minutes with no reads (the engine relaunching), then a fresh engine at task 3
        assert_eq!(w.observe(vec![slot(0, 3, 20_000, 2_048)], 600_000), None);
        assert_eq!(w.busy_ms, 0, "the gap is not busy time");
        // a read one tick later on the same task counts normally again
        let _ = w.observe(vec![slot(0, 3, 20_000, 4_096)], 615_000);
        assert_eq!((w.tokens, w.busy_ms), (2_048, 15_000));
        // a task counter that went backwards inside one tick is a relaunch too
        let _ = w.observe(vec![slot(0, 1, 20_000, 1_024)], 630_000);
        assert_eq!((w.tokens, w.busy_ms), (2_048, 15_000), "a restarted engine adds nothing");
    }

    // what this catches: an abandoned slot read as busy forever (Cormac on #4454). A request
    // cut off mid-prefill leaves its slot idle with stale partial counters (the IntelMac's
    // slot 3, 2048 / 0, at 07:08Z); it must add no busy time, or the rate drifts to zero.
    #[test]
    fn an_idle_slot_with_stale_partial_counters_is_not_prefilling() {
        let stale = SlotCount { id: 3, task: 40, prompt: 2_048, processed: 0, processing: false };
        let mut w = PrefillWindow::default();
        for i in 0..40u64 {
            assert_eq!(w.observe(vec![stale], i * 15_000), None);
        }
        assert_eq!(w.busy_ms, 0, "an idle slot is idle whatever its counters say");
        let parsed = slot_counts_of(&serde_json::json!([
            {"id": 3, "id_task": 40, "n_prompt_tokens": 2048, "n_prompt_tokens_processed": 0, "is_processing": false}
        ]))
        .expect("test: slots");
        assert_eq!(parsed, vec![stale]);
    }

    // what this catches: the bound's arithmetic and its edges. 45 tok/s over a 180 s budget
    // at 8k prefilled a turn serves one lane (the IntelMac serves six); an all-cache regime
    // has no prefill bound; the median resists one cold outlier and counts the cold turns.
    #[test]
    fn the_prefill_bound_is_rate_times_budget_over_the_prefilled_median() {
        let ttft = std::time::Duration::from_secs(180);
        assert_eq!(prefill_lanes(45.0, 8_000, ttft), Some(1));
        assert_eq!(prefill_lanes(450.0, 20_000, ttft), Some(4));
        assert_eq!(prefill_lanes(45.0, 0, ttft), None, "all cache: prefill is not the limit");
        let mut turns = TurnPrefill::default();
        for _ in 0..5 {
            turns.note(20_000, 1_000);
        }
        turns.note(0, 30_000);
        assert_eq!(turns.median(), Some(1_000));
        assert_eq!((turns.turns(), turns.cold()), (6, 1));
    }
}
