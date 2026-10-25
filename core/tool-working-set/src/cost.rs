//! The price of a schema in the head and of a missed verb, both in prompt-token equivalents:
//! engine time, expressed as the tokens the engine prefills in that time.
//!
//! - A schema in the head is prefilled once per epoch: it costs its own tokens.
//! - A miss costs one extra act (she finds the verb, its schema lands in the tail): the act's
//!   decode, converted at the measured prefill/decode rate ratio, plus the act's fixed
//!   per-request time. The missed schema's own tokens are not added: she needs them in her
//!   prompt either way, in the head or in the tail.
//!
//! Every number is measured from her completed calls ([`CostModel::from_samples`]).

use crate::history::EpochMs;

/// One completed inference call's timing, as a prompt capture records it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActSample {
    pub at: EpochMs,
    pub output_tokens: f64,
    pub prefill_tokens_per_s: f64,
    pub decode_tokens_per_s: f64,
    /// The call's time outside prefill and decode (scheduling, checkpoint restore, parsing).
    pub overhead_ms: f64,
}

/// The engine's measured rates, from which both prices follow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CostModel {
    pub prefill_tokens_per_s: f64,
    pub decode_tokens_per_s: f64,
    /// Tokens one act decodes (her reasoning and the call).
    pub act_decode_tokens: f64,
    pub act_overhead_ms: f64,
    /// How many completed calls the medians came from.
    pub samples: usize,
}

impl CostModel {
    /// The medians of `samples` taken within `window_ms` of the newest one: the engine and
    /// build the residents run on now. `None` when no sample carries both rates.
    pub fn from_samples(samples: &[ActSample], window_ms: u64) -> Option<Self> {
        let newest = samples.iter().map(|sample| sample.at).max()?;
        let recent: Vec<&ActSample> = samples
            .iter()
            .filter(|sample| newest.since(sample.at).millis() <= window_ms)
            .filter(|sample| sample.prefill_tokens_per_s > 0.0 && sample.decode_tokens_per_s > 0.0)
            .collect();
        Some(Self {
            prefill_tokens_per_s: median(recent.iter().map(|sample| sample.prefill_tokens_per_s))?,
            decode_tokens_per_s: median(recent.iter().map(|sample| sample.decode_tokens_per_s))?,
            act_decode_tokens: median(recent.iter().map(|sample| sample.output_tokens))?,
            act_overhead_ms: median(recent.iter().map(|sample| sample.overhead_ms.max(0.0)))?,
            samples: recent.len(),
        })
    }

    /// One extra act, in prompt-token equivalents.
    pub fn miss_tokens(&self) -> f64 {
        self.act_decode_tokens * self.prefill_tokens_per_s / self.decode_tokens_per_s
            + self.act_overhead_ms / 1_000.0 * self.prefill_tokens_per_s
    }

    /// Engine seconds for `tokens` prompt-token equivalents.
    pub fn seconds(&self, tokens: f64) -> f64 {
        tokens / self.prefill_tokens_per_s
    }
}

fn median(values: impl Iterator<Item = f64>) -> Option<f64> {
    let mut values: Vec<f64> = values.filter(|value| value.is_finite()).collect();
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    Some(if values.len().is_multiple_of(2) { (values[mid - 1] + values[mid]) / 2.0 } else { values[mid] })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(at: u64, output: f64, prefill: f64, decode: f64, overhead: f64) -> ActSample {
        ActSample { at: EpochMs::from_millis(at), output_tokens: output, prefill_tokens_per_s: prefill, decode_tokens_per_s: decode, overhead_ms: overhead }
    }

    // what this catches: a stale regime (an older build or engine) pricing today's misses, and
    // the miss price forgetting that a decode token costs prefill/decode times a prefill token.
    #[test]
    fn the_price_comes_from_the_recent_medians_and_converts_decode_at_the_rate_ratio() {
        let samples = [
            sample(0, 9_999.0, 1.0, 1.0, 0.0),
            sample(1_000_000, 100.0, 100.0, 10.0, 1_000.0),
            sample(1_000_100, 300.0, 100.0, 10.0, 3_000.0),
            sample(1_000_200, 200.0, 100.0, 10.0, 2_000.0),
        ];
        let cost = CostModel::from_samples(&samples, 10_000).expect("recent samples");
        assert_eq!(cost.samples, 3, "the sample a million ms older is another regime");
        assert_eq!((cost.act_decode_tokens, cost.act_overhead_ms), (200.0, 2_000.0));
        assert_eq!(cost.miss_tokens(), 200.0 * 10.0 + 2.0 * 100.0);
        assert_eq!(cost.seconds(500.0), 5.0);
        assert_eq!(CostModel::from_samples(&[], 10), None);
    }
}
