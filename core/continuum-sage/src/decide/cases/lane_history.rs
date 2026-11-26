//! The lane's history: what the Core measured of each mind, which every lane algorithm reads. The
//! executor (the Core worker) reports what happened, and nothing else writes here; the history fills
//! each mind's features in a situation, and what it has not measured stays `Unmeasured`.
//! hot-path: the Core reports a step and the lane reads features at every step.

use std::num::NonZeroUsize;
use std::time::Duration;

use super::super::measured::{Ewma, Fraction, LineFit, Measured, Window};
use super::lane::{Mind, Phase};

/// What the executor saw happen to one mind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Observation<K> {
    /// One step for her: how long it took on the Core, how many tokens it moved, and whether it was a
    /// prefill chunk (hundreds of tokens per step) or a decode step (one).
    /// `switched`: the step right after she was brought in, which pays what the switch threw away
    /// (the engine's reusable graph, rebuilt for her), beyond the bind `SwitchedIn` measured.
    Stepped { mind: K, took: Duration, tokens: u64, prefill: bool, switched: bool },
    /// She was brought onto the Core: how long the bind took.
    SwitchedIn { mind: K, took: Duration },
    /// Her turn ended after decoding this many tokens.
    TurnEnded { mind: K, decoded: u64 },
    /// Her turn began reusing `reused` of `prompt` tokens she already held.
    Reused { mind: K, reused: u64, prompt: u64 },
    /// Her blocks' bytes, and how many are not resident (unknown until the block layer reports it).
    Resident { mind: K, kv_bytes: u64, cold_bytes: Measured<u64> },
    /// She left this Core (a one-off finished, a mind forgotten): her track goes with her, so the
    /// history holds only the minds the Core still serves.
    Left { mind: K },
}

/// How the history summarizes, set by whoever builds it.
#[derive(Debug, Clone, Copy)]
pub struct Views {
    /// The weight of the newest sample in her mean step times and switch cost.
    pub smoothing: Fraction,
    /// How many of her finished turns her turn-length prediction remembers.
    pub turns: NonZeroUsize,
    /// The weight of the newest decision in the background class's arrival and served rates: a rate,
    /// so over many decisions (the lane's replay window, `lane::DECISIONS`, by default).
    pub rates: Fraction,
}

/// Sixteen finished turns: a working session's worth, so a prediction follows her current activity.
const TURNS: NonZeroUsize = match NonZeroUsize::new(16) {
    Some(turns) => turns,
    None => NonZeroUsize::MIN,
};

impl Default for Views {
    /// The starting views: a quarter weight (about the last four steps) and her last sixteen turns.
    /// The lane's owner may set others; a learned algorithm reads the same history either way.
    fn default() -> Self {
        Self { smoothing: Fraction::QUARTER, turns: TURNS, rates: Fraction::reciprocal(super::lane::DECISIONS) }
    }
}

/// The background class's account.
#[derive(Debug, Clone, Copy)]
struct Background {
    backlog: Duration,
    arrived: Ewma,
    served: Ewma,
}

/// One mind's track.
#[derive(Debug, Clone)]
struct Track<K> {
    mind: K,
    /// Seconds per token, by phase, over every step she took (a switched one too, so a mind that is
    /// always switched in still has a rate, the rebuild in it: a conservative one to size her steps
    /// by): a prefill token costs a small fraction of a decode step.
    prefill: Ewma,
    decode: Ewma,
    /// The same over her steps that followed no switch: what a switched step is compared with.
    plain: [Ewma; 2],
    /// Her prefill steps' seconds against their tokens: a fixed part every chunk pays and a part a
    /// token, so a chunk is sized by what it buys (the M5, 2026-11-26: chunks sized by a per-token rate
    /// shrank under a crowd toward one token, each paying a full step: 3,197 tokens in 57.6 s).
    chunks: LineFit,
    switch_in: Ewma,
    /// What a switched step took beyond her plain steps at the same count of tokens: the rest of a
    /// switch's cost (the M5, 2026-11-25: binds 0.28 ms, the graph rebuild after them 5.6 ms a step).
    rebuild: Ewma,
    prefix_hit: Ewma,
    /// Decoded tokens per finished turn.
    turn_tokens: Window,
    /// Tokens decoded in her current turn so far.
    decoded: u64,
    kv_bytes: Measured<u64>,
    cold_bytes: Measured<u64>,
}

/// Every mind the lane has seen, a linear table: a Core serves a handful of minds, and a scan of a few
/// entries beats a hash at that size.
#[derive(Debug, Clone)]
pub struct LaneHistory<K> {
    views: Views,
    tracks: Vec<Track<K>>,
    /// Seconds per token on this Core, by phase (prefill, decode), over every mind's steps: the served
    /// model's rate, which a mind not yet measured is filled with, so her first step is sized by what
    /// this Core measured, never by a whole batch (the M5, 2026-11-25 20:39 to 20:49Z: minds whose rate
    /// went unmeasured were stepped a whole batch at a time, 7.9 s waits under a 1 s bound).
    core: [Ewma; 2],
    /// The background class: its backlog at the last decision, and its arrival and served rates (Core
    /// seconds a decision), so its guarantee (its queue in check) is a measured comparison.
    background: Background,
    /// Decoded tokens per finished turn on this Core, every mind's: a mind with no finished turn of her
    /// own (a one-off: a dream, a warmup) is predicted from it, so her remainder is counted.
    turns: Window,
    /// Every mind's prefill steps on this Core, the same line: a mind not yet measured is sized by it.
    chunks: LineFit,
}

impl<K: Copy + PartialEq> LaneHistory<K> {
    pub fn new(views: Views) -> Self {
        let rate = || Ewma::new(views.rates);
        Self { views, tracks: Vec::new(), core: [Ewma::new(views.smoothing); 2], background: Background { backlog: Duration::ZERO, arrived: rate(), served: rate() }, turns: Window::new(views.turns), chunks: LineFit::new(views.smoothing) } // cold: built once, with the lane
    }

    fn track(&mut self, mind: K) -> &mut Track<K> {
        let views = self.views;
        let at = match self.tracks.iter().position(|track| track.mind == mind) {
            Some(at) => at,
            None => {
                let ewma = || Ewma::new(views.smoothing);
                self.tracks.push(Track { mind, prefill: ewma(), decode: ewma(), plain: [ewma(), ewma()], chunks: LineFit::new(views.smoothing), switch_in: ewma(), rebuild: ewma(), prefix_hit: ewma(), turn_tokens: Window::new(views.turns), decoded: 0, kv_bytes: Measured::Unmeasured, cold_bytes: Measured::Unmeasured });
                self.tracks.len() - 1
            }
        };
        &mut self.tracks[at]
    }

    /// One decision's account of the background class (minds with no per-item bound): `backlog`, the
    /// Core time its ready minds' remaining work is predicted to take now, and `served`, the Core time
    /// it got since the last decision. What arrived is what the backlog grew by plus what was served.
    /// Owed when the served rate has fallen behind the arrival rate: its queue would grow, so it takes
    /// steps; otherwise it runs only when no act is ready (Joel, 2026-11-25: learning through the day
    /// at the rate that keeps its queue in check, dreaming in the quiet).
    pub fn background(&mut self, backlog: Duration, served: Duration) -> bool {
        let account = &mut self.background;
        let arrived = backlog.as_secs_f64() - account.backlog.as_secs_f64() + served.as_secs_f64();
        account.backlog = backlog;
        account.arrived.observe(arrived);
        account.served.observe(served.as_secs_f64());
        matches!((account.served.mean(), account.arrived.mean()), (Measured::Value(served), Measured::Value(arrived)) if served < arrived)
    }

    pub fn observe(&mut self, observation: Observation<K>) {
        match observation {
            Observation::Stepped { mind, took, tokens, prefill, switched } => {
                let phase = usize::from(!prefill);
                if tokens > 0 {
                    self.core[phase].observe(took.as_secs_f64() / tokens as f64);
                    if prefill {
                        self.chunks.observe(tokens as f64, took.as_secs_f64());
                    }
                }
                let track = self.track(mind);
                if !prefill {
                    track.decoded += tokens;
                }
                if tokens == 0 {
                    return;
                }
                let per_token = took.as_secs_f64() / tokens as f64;
                (if prefill { &mut track.prefill } else { &mut track.decode }).observe(per_token);
                if prefill {
                    track.chunks.observe(tokens as f64, took.as_secs_f64());
                }
                // A switched step is not one of her plain steps: what it took beyond them, at her plain
                // time a token, is the switch's rebuild. Before a plain step is measured there is
                // nothing to compare it with, so it waits for one.
                match (switched, track.plain[phase].mean()) {
                    (false, _) => track.plain[phase].observe(per_token),
                    (true, Measured::Value(plain)) => track.rebuild.observe((took.as_secs_f64() - plain * tokens as f64).max(0.0)),
                    (true, Measured::Unmeasured) => {}
                }
            }
            Observation::SwitchedIn { mind, took } => self.track(mind).switch_in.observe(took.as_secs_f64()),
            Observation::TurnEnded { mind, decoded } => {
                self.turns.observe(decoded as f64);
                let track = self.track(mind);
                track.turn_tokens.observe(decoded as f64);
                track.decoded = 0;
            }
            Observation::Reused { mind, reused, prompt } => {
                if prompt > 0 {
                    self.track(mind).prefix_hit.observe(reused as f64 / prompt as f64);
                }
            }
            Observation::Resident { mind, kv_bytes, cold_bytes } => {
                let track = self.track(mind);
                track.kv_bytes = Measured::Value(kv_bytes);
                track.cold_bytes = cold_bytes;
            }
            Observation::Left { mind } => {
                if let Some(at) = self.tracks.iter().position(|track| track.mind == mind) {
                    self.tracks.swap_remove(at);
                }
            }
        }
    }

    /// Fill `mind`'s measured features from her track: her per-token step time in her current phase,
    /// her switch cost, prefix hits, depth, and her predicted remainder in that phase (a prefilling
    /// mind's prefill; a decoding mind's typical turn, the `typical` quantile of her finished turns,
    /// less what she has decoded in this one). Unseen stays `Unmeasured`.
    pub fn fill(&self, mind: &mut Mind<K>, typical: Fraction) {
        let seconds = |measured: Measured<f64>| measured.value().and_then(|s| Duration::try_from_secs_f64(s).ok()).map_or(Measured::Unmeasured, Measured::Value);
        // her rate in her phase and her typical turn, or this Core's until hers are measured
        let phase = usize::from(matches!(mind.phase, Phase::Decode));
        let core = self.core[phase].mean();
        let track = self.tracks.iter().find(|track| track.mind == mind.mind);
        let own = track.map_or(Measured::Unmeasured, |track| if phase == 0 { track.prefill.mean() } else { track.decode.mean() }); // map_or: no track is nothing measured of her
        let rate = match own {
            Measured::Value(_) => own,
            Measured::Unmeasured => core,
        };
        let turn = match track.map(|track| track.turn_tokens.quantile(typical)) {
            Some(Measured::Value(turn)) => Measured::Value(turn),
            _ => self.turns.quantile(typical),
        };
        let decoded = track.map_or(0, |track| track.decoded); // map_or: no track has decoded nothing
        (mind.step, mind.remaining) = match mind.phase {
            Phase::Prefill { remaining } => (seconds(rate), Measured::Value(remaining)),
            Phase::Decode => (seconds(rate), turn.value().map_or(Measured::Unmeasured, |turn| Measured::Value((turn as u64).saturating_sub(decoded)))),
        };
        // A prefilling mind's chunk: its fixed part and its part a token, from her own chunks once they
        // vary in size, else this Core's; her step is then the part a token, never the two mixed.
        if matches!(mind.phase, Phase::Prefill { .. }) {
            let line = match track.map(|track| track.chunks.fit()) {
                Some(Measured::Value(fit)) => Measured::Value(fit),
                _ => self.chunks.fit(),
            };
            if let Measured::Value((fixed, per_token)) = line {
                (mind.overhead, mind.step) = (seconds(Measured::Value(fixed)), seconds(Measured::Value(per_token)));
            }
        }
        let Some(track) = track else { return };
        // her bind and the rebuild her next step pays after it; a part not yet seen adds nothing yet
        mind.switch_in = match (track.switch_in.mean(), track.rebuild.mean()) {
            (Measured::Value(bind), Measured::Value(rebuild)) => seconds(Measured::Value(bind + rebuild)),
            (bind, Measured::Unmeasured) => seconds(bind),
            (Measured::Unmeasured, rebuild) => seconds(rebuild),
        };
        mind.prefix_hit = track.prefix_hit.mean();
        mind.kv_bytes = track.kv_bytes;
        mind.cold_bytes = track.cold_bytes;
    }
}
