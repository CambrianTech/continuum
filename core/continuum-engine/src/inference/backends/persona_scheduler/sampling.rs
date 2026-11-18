//! A turn's sampler chain from its [`SamplingConfig`].

use llama::{Model, Sampler};

use super::super::SamplingConfig;

/// Greedy at temperature 0 with neither a grammar nor a think bound; otherwise the think bound,
/// grammar, top_k, top_p, penalties, temperature, then the distribution. The think bound and the
/// grammar come first, so their constraints apply before probabilistic sampling could pick a token
/// they reject. A think bound llama.cpp refuses fails the turn by name, never a turn left to think
/// unbounded.
pub(super) fn sampler(model: &Model, sampling: &SamplingConfig) -> Result<Sampler, String> {
    if sampling.temperature <= 0.0 && sampling.grammar.is_none() && sampling.think.is_none() {
        return Ok(Sampler::greedy());
    }
    let mut chain = Sampler::chain();
    if let Some(bound) = sampling.think.as_ref() {
        chain = chain.reasoning_budget(model, bound).map_err(|refusal| refusal.to_string())?;
    }
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
    Ok(chain.temp(temp).dist(42).build())
}
