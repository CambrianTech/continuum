//! One embedding request, tokenized before anything is claimed: its sequences, and the context that
//! embeds them whole. Its memory is known before the context exists, so the lease asks for exactly
//! what this request allocates, never a bound for the longest text.
use llama::Model;

/// What one context token of the embedding model allocates, measured on the M5's engine (engine-c,
/// Qwen3-Embedding-0.6B Q8_0, flash attention on, 2026-11-10; n_ctx = n_batch = n_ubatch at 256,
/// 512, 1,024 and 2,048, linear within 0.1%): Metal KV 112 KiB, Metal compute 601 KiB at its
/// largest per-token reading (256), CPU compute 12 KiB at its largest (2,048). The fork reports no
/// context's memory before it exists (card 9bddda93: a context-measure API in the fork owns this
/// number). It is the rate of a context on the Metal device; a context in host memory
/// (`LlamaCppBackend::context_pool`) or on another backend has its own, measured there.
const CONTEXT_BYTES_PER_TOKEN: u64 = 114_688 + 615_424 + 12_314;

/// The context's fixed allocation beside its tokens: the CPU output buffer (0.58 MiB, measured with
/// the per-token costs above), rounded up to a MiB.
const CONTEXT_FIXED_BYTES: u64 = 1024 * 1024;

/// One request's sequences and the window that embeds the longest of them in one batch.
pub(crate) struct EmbeddingBatch {
    sequences: Vec<Vec<i32>>,
    n_ctx: u32,
}

impl EmbeddingBatch {
    /// Tokenize `texts`. Non-causal embedding needs the entire sequence in one batch, so the window
    /// is the longest sequence: never a private token ceiling or head clip, refused only above what
    /// the model was trained on.
    pub(crate) fn tokenize(model: &Model, texts: &[String]) -> Result<Self, String> {
        let sequences = texts
            .iter()
            .map(|text| model.tokenize(text, true, false))
            .collect::<Result<Vec<_>, _>>()?;
        if sequences.iter().any(Vec::is_empty) {
            return Err("embedding tokenization produced an empty sequence".into());
        }
        let longest = sequences.iter().map(Vec::len).max().ok_or("empty embedding request")?;
        let n_ctx = u32::try_from(longest).map_err(|_| "embedding sequence length exceeds native representation")?;
        let model_limit = model.n_ctx_train();
        if model_limit == 0 || n_ctx > model_limit {
            return Err(format!("embedding sequence length {n_ctx} exceeds model context {model_limit}"));
        }
        Ok(Self { sequences, n_ctx })
    }

    /// What this batch's context allocates: the per-token cost over llama's padded window, plus its
    /// fixed buffers. What the embed lane leases before it creates the context.
    pub(crate) fn context_bytes(&self) -> u64 {
        context_bytes(self.n_ctx)
    }

    /// Embed every sequence in one context sized to the longest; KV is cleared between them.
    pub(crate) fn embed(self, model: &Model) -> Result<Vec<Vec<f32>>, String> {
        let mut ctx = model.new_context(llama::ContextParams {
            embeddings: true,
            pooling_type: llama::PoolingType::Last,
            n_ctx: self.n_ctx,
            n_batch: self.n_ctx,
            n_ubatch: self.n_ctx,
            ..Default::default()
        })?;
        let mut out = Vec::with_capacity(self.sequences.len());
        for tokens in self.sequences {
            ctx.memory_clear(true);
            out.push(ctx.embed(&tokens)?);
        }
        Ok(out)
    }
}

/// What a context of `n_ctx` tokens allocates (see [`CONTEXT_BYTES_PER_TOKEN`]).
pub(crate) fn context_bytes(n_ctx: u32) -> u64 {
    u64::from(llama::ContextParams::allocated_n_ctx(n_ctx)) * CONTEXT_BYTES_PER_TOKEN + CONTEXT_FIXED_BYTES
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
}
