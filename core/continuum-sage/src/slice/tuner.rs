//! The slice's control loop: it climbs measured throughput, constantly, against the wait bound.
//!
//! Joel, 2026-11-22: "It's a control system optimizing for throughput!" The slice is not computed
//! from a formula; it is the loop's state. Each window (one rotation of the ready minds) the loop
//! measures what the engine did, steps per second of engine time (steps and switches both count),
//! and moves the slice in whichever direction last raised it: extremum seeking on the measured
//! curve. Longer slices mean fewer switches; whatever else bends the curve (warm caches, batching,
//! the switch itself) the loop finds the peak by measurement and follows it as load, minds and
//! models change.
//!
//! The wait bound is the wall it climbs against, never a target: the slice can never exceed what
//! keeps every ready mind's next step inside the bound, and a window in which any mind waited past
//! the bound turns the loop back at once.

/// The loop's own gains: how far one window moves the slice, as a fraction of it. A move that kept
/// raising throughput grows the next; a move that lowered it reverses and shrinks it, so the loop
/// settles on the peak yet never stops probing. Its bounds are measured, not set: a move is never
/// less than one step (the engine switches only at a step, so a smaller move changes nothing) and
/// never more than half the slice (a multiplicative move past one would drive it through zero).
const GAIN_START: f64 = 0.25;
const GAIN_GROW: f64 = 1.25;
const GAIN_SHRINK: f64 = 0.5;

use super::control::Control;

/// How far one window may move the slice, as fractions of it, from what this window measured.
#[derive(Debug, Clone, Copy)]
struct Moves {
    /// One step: the engine switches only at a step, so a smaller move changes nothing measurable.
    least: f64,
    /// Half the slice: a move is multiplicative (a move down leaves `slice * (1 - gain)`), so a gain
    /// at or past one drives the slice through zero, and a half is bisection: a move down at most
    /// halves it. Structural, not tuned.
    most: f64,
}

impl Moves {
    fn of(slice: f64, floor: f64) -> Option<Self> {
        // Before a slice or a step is measured there is nothing to take a fraction of.
        if slice <= 0.0 || floor <= 0.0 {
            return None;
        }
        let least = floor / slice;
        Some(Self { least, most: 0.5_f64.max(least) })
    }
}

trait Hold {
    /// A gain held to what this window allows; unchanged when nothing is measured yet.
    fn hold(&self, gain: f64) -> f64;
}

impl Hold for Option<Moves> {
    fn hold(&self, gain: f64) -> f64 {
        match self {
            Some(moves) => gain.clamp(moves.least, moves.most),
            None => gain,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct Window {
    steps: u64,
    /// Engine seconds this window: stepping and switching.
    engine: f64,
}

#[derive(Debug, Clone)]
pub struct Tuner {
    /// The slice now, seconds.
    slice: f64,
    gain: f64,
    /// +1 lengthens, -1 shortens.
    direction: f64,
    /// The last closed window's throughput, steps per engine second.
    last: Option<f64>,
    window: Window,
}

impl Default for Tuner {
    fn default() -> Self {
        Self { slice: 0.0, gain: GAIN_START, direction: 1.0, last: None, window: Window::default() }
    }
}

impl Control for Tuner {
    fn name(&self) -> &'static str {
        "throughput-climb"
    }

    /// The slice now, seconds, already inside the bounds the last window was closed with.
    fn slice(&self) -> f64 {
        self.slice
    }

    fn stepped(&mut self, seconds: f64) {
        self.window.steps += 1;
        self.window.engine += seconds;
    }

    fn switched(&mut self, seconds: f64) {
        self.window.engine += seconds;
    }

    /// Measure the window, move the slice, and hold it in `[floor, wall]`.
    fn close(&mut self, over_bound: bool, floor: f64, wall: f64) {
        let window = std::mem::take(&mut self.window);
        let wall = wall.max(floor);
        let moves = Moves::of(self.slice, floor);
        if over_bound {
            // The wall was crossed: back off now and measure afresh from there.
            self.direction = -1.0;
            self.slice *= 1.0 - self.gain;
            self.last = None;
        } else if window.engine > 0.0 {
            let throughput = window.steps as f64 / window.engine;
            match self.last {
                Some(last) if throughput < last => {
                    self.direction = -self.direction;
                    self.gain = moves.hold(self.gain * GAIN_SHRINK);
                }
                Some(_) => self.gain = moves.hold(self.gain * GAIN_GROW),
                None => {}
            }
            self.last = Some(throughput);
            self.slice *= 1.0 + self.direction * self.gain;
        }
        let held = self.slice.clamp(floor, wall);
        if held != self.slice && self.last.is_some() {
            // Pinned at a bound: probe back inward next window instead of pushing on the bound.
            self.direction = if held >= wall { -1.0 } else { 1.0 };
        }
        self.slice = held;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How long a run is and the floor and wall it holds the slice between.
    struct Run {
        windows: usize,
        floor: f64,
        wall: f64,
    }

    /// Runs the loop on a throughput curve: each window, the slice it chose earns `rate(slice)`.
    fn settle(tuner: &mut Tuner, run: Run, rate: impl Fn(f64) -> f64) -> Vec<f64> {
        let Run { windows, floor, wall } = run;
        let mut slices = Vec::new();
        for _ in 0..windows {
            let slice = tuner.slice().max(floor);
            let steps = 100.0;
            tuner.window = Window { steps: steps as u64, engine: steps / rate(slice) };
            tuner.close(false, floor, wall);
            slices.push(tuner.slice());
        }
        slices
    }

    // what this catches: a slice set by formula instead of found by measurement. On a curve with an
    // interior peak (switching overhead hurts short slices, a cost growing with length hurts long
    // ones) the loop climbs to the peak from either side and stays near it, still probing.
    #[test]
    fn the_loop_finds_the_throughput_peak_and_keeps_probing() {
        let switch = 0.026;
        let rate = |slice: f64| (slice / (slice + switch)) / (1.0 + slice / 2.0) * 50.0;
        // The peak of that curve, by search: where the loop must end up.
        let peak = (1..10_000).map(|i| i as f64 * 0.001).max_by(|a, b| rate(*a).total_cmp(&rate(*b))).unwrap_or(0.0);
        for start in [0.02, 5.0] {
            let mut tuner = Tuner { slice: start, ..Tuner::default() };
            let slices = settle(&mut tuner, Run { windows: 200, floor: 0.02, wall: 10.0 }, rate);
            let tail = &slices[150..];
            let mean = tail.iter().sum::<f64>() / tail.len() as f64;
            assert!((mean / peak - 1.0).abs() < 0.15, "from {start}: settled at {mean:.3}, peak {peak:.3}");
            assert!(tail.windows(2).any(|w| w[0] != w[1]), "it keeps probing");
            assert!(rate(mean) > 0.97 * rate(peak), "within 3% of the best throughput");
        }
    }

    // what this catches: the wait bound treated as a target or ignored. On a curve that only rises
    // (switching is the only cost) the loop climbs to the wall and never past it; a window over the
    // bound backs it off at once.
    #[test]
    fn the_wall_holds_and_a_crossing_backs_off() {
        let rate = |slice: f64| slice / (slice + 0.026) * 50.0;
        let mut tuner = Tuner::default();
        let slices = settle(&mut tuner, Run { windows: 100, floor: 0.02, wall: 0.4 }, rate);
        assert!(slices.iter().all(|s| *s <= 0.4), "never past the wall");
        assert!(slices[99] > 0.3, "it climbed toward the wall: {:.3}", slices[99]);
        let before = tuner.slice();
        tuner.close(true, 0.02, 0.4);
        assert!(tuner.slice() < before, "a crossing turns it back");
    }
}
