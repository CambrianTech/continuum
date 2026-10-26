//! A mind's REPLY RESERVE, from the tail of what she has measurably written.
//!
//! The reserve is the room her view leaves for the reply. It was a constant, the act's
//! runaway bound (12,288 tokens), on a window of 58,880 whose replies measured p99 1,202
//! and max 1,526 (the M5, 24 h to 2026-10-26, 1,063 completions, reasoning included): a
//! fifth of the window held for a reply eight times longer than any she wrote, so her
//! view turned over every 2-4 acts.
//!
//! The reserve is now the max of her last [`REPLY_TAIL_SAMPLES`] measured outputs × 5/4.
//! Its error bound is stated and checked: replayed over those captures (each reserve read
//! only from the turns before it), 2.1% of replies exceed it (`tests` below, against the
//! committed fixture). A reply longer than the reserve is never clipped by it: the request
//! carries the window's physical room after the prompt, not the reserve
//! (`DeliberationPromptView::reply_allowance`), and the longer reply joins the ring, so
//! the next reserve holds it.

use serde::{Deserialize, Serialize};

/// Turns the tail is read from. With the max of N exchangeable samples, the next one
/// exceeds it with probability 1/(N+1); 64 keeps that near 1.5% before headroom.
// derived-or-floor: a SAMPLE COUNT, the statistic's window, not a bound: the reserve is the measured max over it, and its exceed rate is checked against the M5 captures below
pub const REPLY_TAIL_SAMPLES: usize = 64;
/// Turns observed before the tail sizes a reserve: the same bar the need asks.
// derived-or-floor: a TRUST bar on the sample count; below it the cold prior stands, so it never clips a measured value
const MIN_TAIL_TURNS: u32 = 3;
/// Headroom over the max (5/4), an integer ratio like the need's.
// derived-or-floor: a ratio on a measured max that only ever raises the reserve; its exceed rate (2.1%) is the validator below
const TAIL_HEADROOM_NUM: u32 = 5;
const TAIL_HEADROOM_DEN: u32 = 4;

/// The last [`REPLY_TAIL_SAMPLES`] whole outputs (think + answer) of one mind, a ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "TailWire", into = "TailWire")]
pub struct ReplyTail {
    samples: [u32; REPLY_TAIL_SAMPLES],
    /// Samples pushed (saturating); the cursor is `turns % REPLY_TAIL_SAMPLES`.
    turns: u32,
}

impl Default for ReplyTail {
    fn default() -> Self {
        Self { samples: [0; REPLY_TAIL_SAMPLES], turns: 0 }
    }
}

impl ReplyTail {
    /// One measured output, in the server's tokens.
    pub fn push(&mut self, output_tokens: u32) {
        self.samples[self.turns as usize % REPLY_TAIL_SAMPLES] = output_tokens;
        self.turns = self.turns.saturating_add(1);
    }

    /// The reserve her view leaves for the reply: the ring's max × 5/4. `None` below
    /// [`MIN_TAIL_TURNS`]: unknown is not zero, the caller keeps its cold prior.
    pub fn reserve(&self) -> Option<u32> {
        if self.turns < MIN_TAIL_TURNS {
            return None;
        }
        let filled = (self.turns as usize).min(REPLY_TAIL_SAMPLES);
        let max = self.samples[..filled].iter().copied().max()?;
        Some(max.saturating_mul(TAIL_HEADROOM_NUM).div_ceil(TAIL_HEADROOM_DEN))
    }
}

/// The durable form: serde derives arrays only up to 32, so the ring is a list on disk.
/// A list of another length (a file from a different ring size) keeps its newest
/// samples in order; `turns` restarts at what it kept, so the cursor stays honest.
#[derive(Serialize, Deserialize)]
struct TailWire {
    samples: Vec<u32>,
    turns: u32,
}

impl From<ReplyTail> for TailWire {
    fn from(tail: ReplyTail) -> Self {
        let filled = (tail.turns as usize).min(REPLY_TAIL_SAMPLES);
        let start = if (tail.turns as usize) < REPLY_TAIL_SAMPLES { 0 } else { tail.turns as usize % REPLY_TAIL_SAMPLES };
        let samples = (0..filled).map(|i| tail.samples[(start + i) % REPLY_TAIL_SAMPLES]).collect();
        Self { samples, turns: tail.turns }
    }
}

impl From<TailWire> for ReplyTail {
    fn from(wire: TailWire) -> Self {
        let mut tail = Self::default();
        let keep = wire.samples.len().min(REPLY_TAIL_SAMPLES);
        for &sample in &wire.samples[wire.samples.len() - keep..] {
            tail.push(sample);
        }
        tail
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the reserve's stated error bound drifting. Replays the M5's
    // landed outputs (24 h to 2026-10-26, reasoning tokens included) per mind, in order,
    // each reserve read only from that mind's earlier turns, and asserts the measured
    // rates: at most 2.1% of replies exceed the reserve, and the median reserve is a
    // reply's size, far under the 12,288 runaway bound it replaces.
    #[test]
    fn the_tail_reserve_holds_the_measured_exceed_rate_on_the_m5_captures() {
        #[derive(Deserialize)]
        struct Fixture {
            minds: std::collections::BTreeMap<String, Vec<u32>>,
        }
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../../fixtures/reply-output-tokens-m5-2026-10-26.json"
        ))
        .expect("the committed fixture parses"); // JUSTIFIED: a committed test fixture
        let (mut sized, mut exceeded) = (0usize, 0usize);
        let mut reserves = Vec::new();
        for outputs in fixture.minds.values() {
            let mut tail = ReplyTail::default();
            for &output in outputs {
                if let Some(reserve) = tail.reserve() {
                    sized += 1;
                    exceeded += usize::from(output > reserve);
                    reserves.push(reserve);
                }
                tail.push(output);
            }
        }
        assert!(sized > 1_000, "the replay covers the fixture: {sized}");
        assert!(exceeded * 1_000 <= sized * 21, "{exceeded} of {sized} exceeded the reserve");
        reserves.sort_unstable();
        let median = reserves[reserves.len() / 2];
        assert!(median * 4 < 12_288, "median reserve {median}");
    }

    // what this catches: a cold ring sizing a reserve from one tiny ack, and the durable
    // list losing order or the cursor across a save (a restart must not re-earn the tail).
    #[test]
    fn the_tail_waits_for_three_turns_and_survives_a_save_in_order() {
        let mut tail = ReplyTail::default();
        tail.push(40);
        tail.push(1_000);
        assert_eq!(tail.reserve(), None);
        tail.push(200);
        assert_eq!(tail.reserve(), Some(1_250));
        for n in 0..(REPLY_TAIL_SAMPLES as u32 + 5) {
            tail.push(100 + n);
        }
        let reloaded: ReplyTail =
            serde_json::from_str(&serde_json::to_string(&tail).expect("serializes")).expect("parses"); // JUSTIFIED: test
        assert_eq!(reloaded.reserve(), tail.reserve(), "the 1,000 aged out, the newest 64 kept");
        assert_eq!(reloaded.reserve(), Some((100 + REPLY_TAIL_SAMPLES as u32 + 4) * 5 / 4));
        let legacy: ReplyTail = serde_json::from_str(r#"{"samples":[9,8],"turns":2}"#).expect("parses"); // JUSTIFIED: test
        assert_eq!(legacy.reserve(), None);
    }
}
