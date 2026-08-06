//! Detecting a serving lane that is lying about its own progress.
//!
//! # The signal
//!
//! On 2026-08-05 a llama-server slot printed this line, unchanged, for **four hours**:
//!
//! ```text
//! slot print_timing: id  1 | task 2104 | prompt processing, n_tokens = 2047,
//! progress = 1.10, t = 14831.42 s / 0.14 tokens per second
//! ```
//!
//! `progress` is a fraction of a prompt consumed. **1.10 cannot happen.** It is not a
//! slow lane, a busy lane, or a lane under memory pressure — it is a lane reporting an
//! arithmetically impossible state, which means the loop driving it has lost its bounds.
//! Nothing killed it. It sat there at 0.14 tok/s while its log grew at 1.2 GB/minute and
//! eventually took the machine to zero bytes free.
//!
//! # Why the existing heartbeat did not catch it
//!
//! `serving_daemon`'s liveness probe asks the lane to decode one token. A wedged SLOT does
//! not necessarily fail that: the other slots still decode, so the probe passes and the
//! lane reads healthy while one slot burns forever. The heartbeat measures *the lane*;
//! this measures *a slot's own account of itself*. Two different questions — which is
//! precisely why one of them can pass for four hours while the other is screaming.
//!
//! This is the same defect class as a route reporting `[ok]` while swallowing traffic:
//! a liveness signal standing in for a progress signal.
//!
//! # Why the log sink is the right detector
//!
//! Since the outage the stderr pump reads **every line the engine emits** to keep the file
//! under its cap ([`super::child_log`]). The evidence is already streaming through a task
//! we own, at zero additional cost. Handing it a [`LineWatch`] costs one comparison per
//! line and needs no new probe, no new tick, and no second connection to the engine.
//!
//! # What it does NOT do
//!
//! It does not kill anything. It raises a [`WedgeFlag`], and the serving daemon — the one
//! authority over the lane's lifecycle — decides. A log sink that could reap a serving
//! process would be a second lifecycle owner, which is the thing the RTOS shape exists to
//! prevent.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::child_log::LineWatch;

/// Progress above this is impossible, not merely surprising.
///
/// A completed prefill legitimately prints `1.00`; the engine formats to a couple of
/// decimals, so a genuine 1.0 can round to `1.000001`. The epsilon keeps the signal
/// meaning *"the loop lost its bounds"* rather than *"floating point"*.
const IMPOSSIBLE_PROGRESS: f32 = 1.001;

/// Impossible lines required before declaring the lane wedged.
///
/// One could in principle be a formatting artifact. Four consecutive ones — with no valid
/// progress line between them — is a slot stuck in a loop. A wedge emits them in
/// milliseconds, so this costs nothing in detection latency; the only thing it buys is
/// immunity to a single cosmetic glitch reaping a healthy lane.
const SIGHTINGS_TO_DECLARE: u32 = 4;

/// Raised by whoever proves the live lane is wedged, taken by the serving daemon's tick.
///
/// A newtype rather than a bare `Arc<AtomicBool>` so the memory ordering lives in ONE
/// place: every reporter gets `Release`, the single consumer gets `AcqRel`, and no future
/// caller can quietly pick `Relaxed` and lose the evidence that motivated the flag.
#[derive(Clone, Default)]
pub struct WedgeFlag(Arc<AtomicBool>);

impl WedgeFlag {
    pub fn new() -> Self {
        Self::default()
    }

    /// Report that the live lane is wedged. Idempotent — many reporters, one bit.
    pub fn raise(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Read and clear. Exactly one caller acts on each raise.
    pub fn take(&self) -> bool {
        self.0.swap(false, Ordering::AcqRel)
    }
}

/// The impossible progress value on this line, if it carries one.
///
/// Returns `None` for a line with no progress field AND for a line whose progress is
/// legitimate — the caller distinguishes those two via [`WedgeWatch`], because "no opinion"
/// and "checked, it's fine" reset the counter differently.
fn progress_of(line: &str) -> Option<f32> {
    let rest = line.split_once("progress = ")?.1;
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(rest.len());
    rest[..end].parse::<f32>().ok()
}

/// Watches a llama-server stderr stream for the impossible-progress signature.
pub struct WedgeWatch {
    flag: WedgeFlag,
    /// Consecutive impossible lines. Reset by any VALID progress line — a healthy lane
    /// interleaves them constantly, a wedged slot emits only the impossible one. Without
    /// the reset, four cosmetic glitches spread across four healthy hours would reap a
    /// working lane.
    sightings: u32,
    /// Report once per child. The flag is take-once anyway, but a latch keeps a wedge from
    /// emitting a probe line per log line — which is how the outage got its volume.
    latched: bool,
}

impl WedgeWatch {
    pub fn new(flag: WedgeFlag) -> Self {
        Self {
            flag,
            sightings: 0,
            latched: false,
        }
    }
}

impl LineWatch for WedgeWatch {
    fn observe(&mut self, line: &str) {
        if self.latched {
            return;
        }
        let Some(progress) = progress_of(line) else {
            return;
        };
        if progress <= IMPOSSIBLE_PROGRESS {
            self.sightings = 0;
            return;
        }
        self.sightings += 1;
        if self.sightings < SIGHTINGS_TO_DECLARE {
            return;
        }
        self.latched = true;
        self.flag.raise();
        crate::probe!(
            class = "serving.wedge",
            progress = progress as f64,
            sightings = self.sightings as u64,
            "a slot reported IMPOSSIBLE progress (>1.0) repeatedly — the lane is wedged; \
             flagged for the serving daemon to reap and respawn (2026-08-05 outage)",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The verbatim line from the 2026-08-05 outage.
    const WEDGED: &str = "slot print_timing: id  1 | task 2104 | prompt processing, \
                          n_tokens = 2047, progress = 1.10, t = 14831.42 s / 0.14 tokens per second";
    const HEALTHY: &str = "slot update_slots: id  0 | task 12 | prompt processing, \
                           n_tokens = 512, progress = 0.25";

    // what this catches: THE four-hour wedge. The exact line that spun for 4.1 hours at
    // 0.14 tok/s must raise the flag — nothing in the system noticed it at the time.
    #[test]
    fn the_outages_own_line_declares_a_wedge() {
        let flag = WedgeFlag::new();
        let mut watch = WedgeWatch::new(flag.clone());
        for _ in 0..SIGHTINGS_TO_DECLARE {
            watch.observe(WEDGED);
        }
        assert!(flag.take(), "impossible progress must flag the lane");
        assert!(!flag.take(), "take clears — exactly one consumer acts");
    }

    // what this catches: a healthy lane is never reaped. Ordinary prefill progress lines,
    // a completed 1.00, and non-progress banner noise must all leave the flag down —
    // a false wedge kills in-flight turns on a working lane.
    #[test]
    fn a_healthy_lane_is_never_declared_wedged() {
        let flag = WedgeFlag::new();
        let mut watch = WedgeWatch::new(flag.clone());
        for line in [
            HEALTHY,
            "slot update_slots: id  0 | task 12 | progress = 1.00",
            "llama_model_loader: loaded meta data with 30 key-value pairs",
            "main: server is listening on http://127.0.0.1:58057",
        ] {
            watch.observe(line);
        }
        assert!(!flag.take(), "a working lane must never be flagged");
    }

    // what this catches: the hysteresis is CONSECUTIVE, not cumulative. Four impossible
    // lines scattered across hours of healthy serving are a formatting quirk; four in a
    // row are a stuck loop. Without the reset the two are indistinguishable and a
    // long-lived healthy lane eventually reaps itself.
    #[test]
    fn scattered_glitches_do_not_accumulate_into_a_reap() {
        let flag = WedgeFlag::new();
        let mut watch = WedgeWatch::new(flag.clone());
        for _ in 0..10 {
            watch.observe(WEDGED);
            watch.observe(HEALTHY);
        }
        assert!(
            !flag.take(),
            "an impossible line followed by real progress is not a wedge"
        );
    }

    // what this catches: the parse. `progress = 1.10,` is followed by a comma in the real
    // line — a parser that swallowed it would return None and the detector would be dead
    // code that always passes. Pin the boundary.
    #[test]
    fn progress_parses_out_of_the_real_line_shape() {
        assert_eq!(progress_of(WEDGED), Some(1.10));
        assert_eq!(progress_of(HEALTHY), Some(0.25));
        assert_eq!(progress_of("no progress field here"), None);
    }
}
