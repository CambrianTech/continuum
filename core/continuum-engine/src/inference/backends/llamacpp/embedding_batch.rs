//! One embedding request, tokenized and planned before anything is claimed: its sequences, how each
//! is embedded (whole, or in windows when longer than the model's context), and the context that
//! embeds the widest piece. Its memory is known before the context exists, so the lease asks for exactly
//! what this request allocates, never a bound for the longest text.
use llama::Model;

/// What one context token of the embedding model allocates, measured on the M5's engine (engine-c,
/// Qwen3-Embedding-0.6B Q8_0, flash attention on, 2026-11-10; n_ctx = n_batch = n_ubatch at 256,
/// 512, 1,024 and 2,048, linear within 0.1%): Metal KV 112 KiB, Metal compute 601 KiB at its
/// largest per-token reading (256), CPU compute 12 KiB at its largest (2,048). The fork reports no
/// context's memory before it exists (card: a context-measure API in the fork owns this number).
const CONTEXT_BYTES_PER_TOKEN: u64 = 114_688 + 615_424 + 12_314;

/// The context's fixed allocation beside its tokens: the CPU output buffer (0.58 MiB, measured with
/// the per-token costs above), rounded up to a MiB.
const CONTEXT_FIXED_BYTES: u64 = 1024 * 1024;

/// One request's sequences, each one's plan, and the window that embeds the widest piece.
pub(crate) struct EmbeddingBatch {
    sequences: Vec<Vec<i32>>,
    prefix: Vec<i32>,
    plans: Vec<Plan>,
    widest: u32,
}

impl EmbeddingBatch {
    /// Tokenize `texts` and plan each. Non-causal embedding needs each piece in one batch, so the
    /// window is the widest piece: a text that fits is one piece, a longer one its windows; never a
    /// private token ceiling or head clip.
    pub(crate) fn tokenize(model: &Model, texts: &[String]) -> Result<Self, String> {
        let sequences = texts
            .iter()
            .map(|text| model.tokenize(text, true, false))
            .collect::<Result<Vec<_>, _>>()?;
        if sequences.iter().any(Vec::is_empty) {
            return Err("embedding tokenization produced an empty sequence".into());
        }
        let model_limit = usize::try_from(model.n_ctx_train())
            .map_err(|_| "embedding model context exceeds native representation")?;
        // The prefix (BOS) is read only when a text needs windows: a batch of texts that fit never
        // depends on how a tokenizer treats an empty string (Codex on #5901).
        let prefix = if sequences.iter().any(|tokens| tokens.len() > model_limit) {
            model.tokenize("", true, false)?
        } else {
            Vec::new()
        };
        let plans = plan(&sequences, &prefix, model_limit)?;
        let widest = plans
            .iter()
            .zip(&sequences)
            .map(|(plan, tokens)| match plan {
                Plan::Direct => tokens.len(),
                Plan::Windows(windows) => windows.iter().map(|w| prefix.len() + w.len()).max().unwrap_or(0), // unwrap_or: plan() never returns an empty window list
            })
            .max()
            .ok_or("empty embedding request")?;
        let widest = u32::try_from(widest).map_err(|_| "embedding sequence length exceeds native representation")?;
        Ok(Self { sequences, prefix, plans, widest })
    }

    /// What this batch's context allocates: the per-token cost over llama's padded window, plus its
    /// fixed buffers. What the embed lane leases before it creates the context.
    pub(crate) fn context_bytes(&self) -> u64 {
        context_bytes(self.widest)
    }

    /// Embed every piece in one context sized to the widest; KV is cleared between pieces.
    pub(crate) fn embed(self, model: &Model) -> Result<Vec<Vec<f32>>, String> {
        let mut ctx = model.new_context(llama::ContextParams {
            embeddings: true,
            pooling_type: llama::PoolingType::Last,
            n_ctx: self.widest,
            n_batch: self.widest,
            n_ubatch: self.widest,
            ..Default::default()
        })?;
        embed_plans(&self.sequences, &self.plans, &self.prefix, |tokens| {
            ctx.memory_clear(true);
            ctx.embed(tokens)
        })
    }
}

/// What a context of `n_ctx` tokens allocates (see [`CONTEXT_BYTES_PER_TOKEN`]).
pub(crate) fn context_bytes(n_ctx: u32) -> u64 {
    u64::from(llama::ContextParams::allocated_n_ctx(n_ctx)) * CONTEXT_BYTES_PER_TOKEN + CONTEXT_FIXED_BYTES
}

/// How one tokenized text is embedded.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Plan {
    /// It fits the context: its own tokens, embedded exactly as they are.
    Direct,
    /// It does not (card 57ef2a06: BigMama's 38,603-token example at a 32,768 context errored the
    /// whole batch): every window of it, each carrying the model's prefix, never its head alone.
    Windows(Vec<std::ops::Range<usize>>),
}

/// Each text's plan. Only a text over `limit` is windowed, and it must begin with `prefix`.
fn plan(sequences: &[Vec<i32>], prefix: &[i32], limit: usize) -> Result<Vec<Plan>, String> {
    sequences
        .iter()
        .map(|tokens| {
            if tokens.len() <= limit {
                return Ok(Plan::Direct);
            }
            if !tokens.starts_with(prefix) {
                return Err(format!(
                    "a {}-token embedding sequence does not begin with the model's {}-token prefix: its windows would be built from the wrong tokens",
                    tokens.len(),
                    prefix.len()
                ));
            }
            windows(tokens.len(), prefix.len(), limit).map(Plan::Windows)
        })
        .collect()
}

/// One vector per text, in order: a direct text is `embed` of its own tokens, untouched; a windowed
/// text is its windows combined by `weighted_mean`.
fn embed_plans(
    sequences: &[Vec<i32>],
    plans: &[Plan],
    prefix: &[i32],
    mut embed: impl FnMut(&[i32]) -> Result<Vec<f32>, String>,
) -> Result<Vec<Vec<f32>>, String> {
    let mut out = Vec::with_capacity(sequences.len());
    for (tokens, plan) in sequences.iter().zip(plans) {
        out.push(match plan {
            Plan::Direct => embed(tokens)?,
            Plan::Windows(windows) => {
                let mut pieces = Vec::with_capacity(windows.len());
                for window in windows {
                    let mut piece = Vec::with_capacity(prefix.len() + window.len());
                    piece.extend_from_slice(prefix);
                    piece.extend_from_slice(&tokens[window.clone()]);
                    pieces.push((embed(&piece)?, window.len()));
                }
                weighted_mean(&pieces)?
            }
        });
    }
    Ok(out)
}

/// The text-token windows of a `len`-token sequence whose first `prefix` tokens are the model's
/// prefix, each fitting `limit` with the prefix carried: one window when it fits, else contiguous
/// windows covering every text token once.
fn windows(len: usize, prefix: usize, limit: usize) -> Result<Vec<std::ops::Range<usize>>, String> {
    let span = limit.checked_sub(prefix).filter(|span| *span > 0).ok_or_else(|| {
        format!("embedding model context {limit} cannot hold its own {prefix}-token prefix and any text")
    })?;
    let text = prefix..len;
    if text.is_empty() {
        return Err("embedding sequence holds no text after its prefix".into());
    }
    Ok(text.clone().step_by(span).map(|start| start..(start + span).min(len)).collect())
}

/// The vector a text's windows stand for together: their mean weighted by the text tokens each
/// holds, L2-normalized like every window (`ctx.embed`), so a one-window text is its window.
fn weighted_mean(pieces: &[(Vec<f32>, usize)]) -> Result<Vec<f32>, String> {
    let (first, _) = pieces.first().ok_or("a text with no embedded window")?;
    let mut sum = vec![0.0f32; first.len()];
    for (vector, weight) in pieces {
        if vector.len() != sum.len() {
            return Err(format!("embedding windows disagree on dimension: {} vs {}", vector.len(), sum.len()));
        }
        for (s, x) in sum.iter_mut().zip(vector) {
            *s += x * *weight as f32;
        }
    }
    let norm = sum.iter().map(|x| x * x).sum::<f32>().sqrt();
    if !norm.is_finite() || norm == 0.0 {
        return Err("embedding windows cancelled to a degenerate vector".into());
    }
    Ok(sum.into_iter().map(|x| x / norm).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;

    // what this catches (the M5 on 26960c09d, 758 failed embeddings in 3 h, 9% success): a lease
    // sized for the longest text refused for every short one. A short text leases its own small
    // context; the measured 2,048-token context (Metal KV 224 + compute 1,201 + CPU 24 + output 0.6
    // MiB = 1,450 MiB) is covered within 1%, never under it.
    #[test]
    fn a_context_leases_what_it_allocates_short_or_long() {
        assert!(context_bytes(40) < 200 * MIB, "a short text: {} MiB", context_bytes(40) / MIB);
        assert_eq!(context_bytes(40), context_bytes(256), "llama pads to 256 tokens");
        let measured_2048 = (224 * MIB) + (1_201 * MIB) + (24 * MIB) + MIB / 2;
        assert!(context_bytes(2_048) >= measured_2048, "{} < {}", context_bytes(2_048), measured_2048);
        assert!(context_bytes(2_048) < measured_2048 + measured_2048 / 100, "within 1%: {}", context_bytes(2_048));
    }

    // what this catches (card 57ef2a06): a text longer than the model's context clipped to its
    // head, or rejected so the whole batch embeds nothing. Every text token lands in exactly one
    // window, each window fits the context with the prefix carried, and a text that fits is one
    // window.
    #[test]
    fn windows_cover_every_text_token_once_within_the_context() {
        assert_eq!(windows(100, 1, 32_768).expect("test: fits"), vec![1..100]);
        let long = windows(38_603, 1, 32_768).expect("test: plans");
        assert_eq!(long, vec![1..32_768, 32_768..38_603], "the BigMama example: two windows, nothing dropped");
        assert!(long.iter().all(|w| 1 + w.len() <= 32_768), "each window fits with its prefix");
        assert_eq!(long.iter().map(|w| w.len()).sum::<usize>(), 38_603 - 1, "every text token once");
        assert!(windows(10, 4, 4).is_err(), "a context that cannot hold its prefix and text refuses");
        assert!(windows(1, 1, 8).is_err(), "a sequence of only its prefix holds no text");
    }

    // what this catches: windows combined unweighted (a short tail outweighs most of the text) or
    // left unnormalized (cosine stops being a dot product); one window must be itself.
    #[test]
    fn windows_combine_by_their_token_share_into_a_unit_vector() {
        let one = weighted_mean(&[(vec![0.6, 0.8], 7)]).expect("test: one window");
        assert!((one[0] - 0.6).abs() < 1e-6 && (one[1] - 0.8).abs() < 1e-6, "{one:?}");
        let two = weighted_mean(&[(vec![1.0, 0.0], 3), (vec![0.0, 1.0], 1)]).expect("test: two windows");
        assert!(two[0] > two[1] * 2.9 && two[0] < two[1] * 3.1, "3:1 by tokens: {two:?}");
        assert!(((two[0] * two[0] + two[1] * two[1]).sqrt() - 1.0).abs() < 1e-6, "unit length");
        assert!(weighted_mean(&[(vec![1.0], 1), (vec![1.0, 0.0], 1)]).is_err(), "dimensions must agree");
        assert!(weighted_mean(&[(vec![1.0, 0.0], 2), (vec![-1.0, 0.0], 2)]).is_err(), "windows that cancel are refused, never normalized to NaN");
        assert!(weighted_mean(&[]).is_err(), "a text with no window has no vector");
    }

    // what this catches (Codex on #5901): a text that fits re-encoded through the window combiner
    // (multiplied by its token count and renormalized, so its f32 vector drifts from the model's),
    // a prefix-only sequence refused, or outputs out of order in a mixed batch. A text that fits is
    // EXACTLY embed(its tokens); a long one is planned into windows; order is kept.
    #[test]
    fn a_text_that_fits_is_embedded_exactly_and_a_mixed_batch_keeps_its_order() {
        // A deterministic stand-in for the model: a non-normalized vector from the tokens, so any
        // re-normalization of a direct text would show as a difference.
        let fake = |tokens: &[i32]| -> Result<Vec<f32>, String> {
            Ok(vec![tokens.len() as f32, tokens.iter().map(|t| *t as f32).sum::<f32>() + 0.1])
        };
        let short = vec![1, 5, 9];
        let prefix_only = vec![1];
        let long: Vec<i32> = std::iter::once(1).chain(2..12).collect(); // 11 tokens, prefix [1]
        let sequences = vec![short.clone(), long.clone(), prefix_only.clone()];
        let plans = plan(&sequences, &[1], 8).expect("test: plans");
        assert_eq!(plans[0], Plan::Direct);
        assert_eq!(plans[1], Plan::Windows(vec![1..8, 8..11]), "only the over-limit text is windowed");
        assert_eq!(plans[2], Plan::Direct, "a prefix-only sequence that fits is embedded as before, never refused");
        let out = embed_plans(&sequences, &plans, &[1], fake).expect("test: embeds");
        assert_eq!(out.len(), 3, "one output per input");
        assert_eq!(out[0], fake(&short).expect("test: fake"), "a fitting text is exactly embed(its tokens)");
        assert_eq!(out[2], fake(&prefix_only).expect("test: fake"));
        assert!(((out[1][0] * out[1][0] + out[1][1] * out[1][1]).sqrt() - 1.0).abs() < 1e-6, "the windowed text is a unit vector");
        assert!(plan(&[vec![7; 20]], &[1], 8).is_err(), "a long text not beginning with the prefix refuses");
    }

    // what this catches (Codex on 57ef2a06): the last, short window of a long text weighted like a
    // full one, so a few hundred tail tokens pull the vector as hard as 32k head tokens.
    #[test]
    fn the_short_final_window_weighs_only_its_own_tokens() {
        let plan = windows(38_603, 1, 32_768).expect("test: plans");
        let weights: Vec<usize> = plan.iter().map(|w| w.len()).collect();
        assert_eq!(weights, vec![32_767, 5_835], "each window weighs the text tokens it holds");
        let mean = weighted_mean(&[(vec![1.0, 0.0], weights[0]), (vec![0.0, 1.0], weights[1])]).expect("test: combine");
        assert!(mean[0] / mean[1] > 5.6 && mean[0] / mean[1] < 5.62, "the head dominates by its token share: {mean:?}");
    }
}
