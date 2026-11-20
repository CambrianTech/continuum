//! A turn's sampler chain from its [`SamplingConfig`] (moved unchanged from the old scheduler).

use llama::{Model, Sampler};

use super::super::SamplingConfig;

/// Greedy at temperature 0 without a grammar; otherwise grammar, top_k, top_p, penalties, temperature,
/// then the distribution. Grammar first, so the structural constraint applies before probabilistic
/// sampling could pick a token it rejects.
pub(super) fn sampler(model: &Model, sampling: &SamplingConfig) -> Sampler {
    if sampling.temperature <= 0.0 && sampling.grammar.is_none() {
        return Sampler::greedy();
    }
    let mut chain = Sampler::chain();
    if let Some(grammar) = sampling.grammar.as_ref() {
        chain = chain.grammar(model, grammar, "root");
    }
    if sampling.top_k > 0 {
        chain = chain.top_k(sampling.top_k as i32);
    }
    if sampling.top_p > 0.0 && sampling.top_p < 1.0 {
        chain = chain.top_p(sampling.top_p as f32, 1);
    }
    // 64 = llama.cpp's default last-n window for the penalty calculation.
    chain = chain.penalties(model.n_vocab(), 64, sampling.repeat_penalty, 0.0, 0.0);
    let temp = if sampling.temperature > 0.0 { sampling.temperature as f32 } else { 0.01 };
    chain.temp(temp).dist(42).build()
}
