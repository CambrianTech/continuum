//! The lane's history: what the Core measured of each mind, which every lane algorithm reads. The
//! executor (the Core worker) reports what happened, and nothing else writes here; the history fills
//! each mind's features in a situation, and what it has not measured stays `Unmeasured`.
//! hot-path: the Core reports a step and the lane reads features at every step.

use std::num::NonZeroUsize;
use std::time::Duration;

use super::super::measured::{Ewma, Fraction, Measured, Window};
use super::lane::{Mind, Phase};

/// What the executor saw happen to one mind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Observation<K> {
    /// One step for her: how long it took on the Core, how many tokens it moved, and whether it was a
    /// prefill chunk (hundreds of tokens per step) or a decode step (one).
    Stepped { mind: K, took: Duration, tokens: u64, prefill: bool },
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
        Self { smoothing: Fraction::QUARTER, turns: TURNS }
    }
}

/// One mind's track.
#[derive(Debug, Clone)]
struct Track<K> {
    mind: K,
    /// Seconds per token, by phase: a prefill token costs a small fraction of a decode step.
    prefill: Ewma,
    decode: Ewma,
    switch_in: Ewma,
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
}

impl<K: Copy + PartialEq> LaneHistory<K> {
    pub fn new(views: Views) -> Self {
        Self { views, tracks: Vec::new() } // cold: built once, with the lane
    }

    fn track(&mut self, mind: K) -> &mut Track<K> {
        let views = self.views;
        let at = match self.tracks.iter().position(|track| track.mind == mind) {
            Some(at) => at,
            None => {
                let ewma = || Ewma::new(views.smoothing);
                self.tracks.push(Track { mind, prefill: ewma(), decode: ewma(), switch_in: ewma(), prefix_hit: ewma(), turn_tokens: Window::new(views.turns), decoded: 0, kv_bytes: Measured::Unmeasured, cold_bytes: Measured::Unmeasured });
                self.tracks.len() - 1
            }
        };
        &mut self.tracks[at]
    }

    pub fn observe(&mut self, observation: Observation<K>) {
        match observation {
            Observation::Stepped { mind, took, tokens, prefill } => {
                let track = self.track(mind);
                if tokens > 0 {
                    let per_token = took.as_secs_f64() / tokens as f64;
                    if prefill {
                        track.prefill.observe(per_token);
                    } else {
                        track.decode.observe(per_token);
                        track.decoded += tokens;
                    }
                }
            }
            Observation::SwitchedIn { mind, took } => self.track(mind).switch_in.observe(took.as_secs_f64()),
            Observation::TurnEnded { mind, decoded } => {
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
        let Some(track) = self.tracks.iter().find(|track| track.mind == mind.mind) else { return };
        let seconds = |measured: Measured<f64>| measured.value().and_then(|s| Duration::try_from_secs_f64(s).ok()).map_or(Measured::Unmeasured, Measured::Value);
        mind.switch_in = seconds(track.switch_in.mean());
        mind.prefix_hit = track.prefix_hit.mean();
        mind.kv_bytes = track.kv_bytes;
        mind.cold_bytes = track.cold_bytes;
        (mind.step, mind.remaining) = match mind.phase {
            Phase::Prefill { remaining } => (seconds(track.prefill.mean()), Measured::Value(remaining)),
            Phase::Decode => (seconds(track.decode.mean()), track.turn_tokens.quantile(typical).value().map_or(Measured::Unmeasured, |turn| Measured::Value((turn as u64).saturating_sub(track.decoded)))),
        };
    }
}
