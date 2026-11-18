//! A sampler pipeline: the chain the engine samples each token through (moved from expert_eval_cb.rs, unchanged).

use super::*;

/// A sampler pipeline. Build with `Sampler::chain()` and chain methods, or
/// use `Sampler::greedy()` for deterministic argmax.
pub struct Sampler {
    pub(super) ptr: NonNull<sys::llama_sampler>,
}

unsafe impl Send for Sampler {}

impl Sampler {
    /// Deterministic argmax sampler.
    pub fn greedy() -> Self {
        let raw = unsafe { sys::llama_sampler_init_greedy() };
        // SAFETY: init_greedy is infallible in upstream llama.cpp.
        Self {
            ptr: NonNull::new(raw).expect("llama_sampler_init_greedy returned null"),
        }
    }

    /// Start building a sampler chain. Samplers apply in insertion order;
    /// terminate the chain with `dist(seed)` for probabilistic sampling or
    /// leave unterminated for argmax-after-filters.
    pub fn chain() -> SamplerChainBuilder {
        let params = unsafe { sys::llama_sampler_chain_default_params() };
        let raw = unsafe { sys::llama_sampler_chain_init(params) };
        SamplerChainBuilder {
            chain: NonNull::new(raw).expect("llama_sampler_chain_init returned null"),
        }
    }

    /// Sample and accept the next token from logits at `idx` in the context.
    /// llama.cpp's `llama_sampler_sample` applies the sampler chain and then
    /// calls `llama_sampler_accept` before returning; callers must not accept
    /// the returned token again.
    pub fn sample(&mut self, ctx: &Context<'_>, idx: i32) -> i32 {
        unsafe { sys::llama_sampler_sample(self.ptr.as_ptr(), ctx.ptr.as_ptr(), idx) }
    }

    /// Notify the sampler that an externally-selected token was accepted.
    /// Do not call this after `sample()`; `sample()` already accepts.
    pub fn accept(&mut self, token: i32) {
        unsafe {
            sys::llama_sampler_accept(self.ptr.as_ptr(), token);
        }
    }

    /// Reset sampler state (e.g., clear penalty history).
    pub fn reset(&mut self) {
        unsafe {
            sys::llama_sampler_reset(self.ptr.as_ptr());
        }
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        unsafe {
            sys::llama_sampler_free(self.ptr.as_ptr());
        }
    }
}

/// Builder for a sampler chain.
pub struct SamplerChainBuilder {
    pub(super) chain: NonNull<sys::llama_sampler>,
}

impl SamplerChainBuilder {
    pub(super) fn add(self, smpl: *mut sys::llama_sampler) -> Self {
        // SAFETY: chain takes ownership of smpl per llama.h docs.
        unsafe {
            sys::llama_sampler_chain_add(self.chain.as_ptr(), smpl);
        }
        self
    }

    pub fn top_k(self, k: i32) -> Self {
        let s = unsafe { sys::llama_sampler_init_top_k(k) };
        self.add(s)
    }

    pub fn top_p(self, p: f32, min_keep: usize) -> Self {
        let s = unsafe { sys::llama_sampler_init_top_p(p, min_keep) };
        self.add(s)
    }

    pub fn min_p(self, p: f32, min_keep: usize) -> Self {
        let s = unsafe { sys::llama_sampler_init_min_p(p, min_keep) };
        self.add(s)
    }

    pub fn temp(self, t: f32) -> Self {
        let s = unsafe { sys::llama_sampler_init_temp(t) };
        self.add(s)
    }

    /// Repetition/frequency/presence penalties, llama.cpp style.
    /// `n_vocab` = the model's vocab size (upstream b10636 added it back to the
    /// signature); `last_n` = number of recent tokens to consider (0 disables,
    /// -1 = n_ctx).
    pub fn penalties(self, n_vocab: i32, last_n: i32, repeat: f32, freq: f32, presence: f32) -> Self {
        let s = unsafe { sys::llama_sampler_init_penalties(n_vocab, last_n, repeat, freq, presence) };
        self.add(s)
    }

    /// Probabilistic final step. Usually the last thing in a chain.
    pub fn dist(self, seed: u32) -> Self {
        let s = unsafe { sys::llama_sampler_init_dist(seed) };
        self.add(s)
    }

    /// Add a GBNF grammar constraint. Forces output to match the grammar
    /// — invalid tokens get probability zero. `grammar_root` is the
    /// start-symbol name in the grammar (typically "root"). Use this to
    /// enforce JSON output or any other structured format.
    ///
    /// Needs the model's vocab — pass the loaded `Model` so the chain
    /// can wire the grammar against the right token table. Belongs early
    /// in the chain (before temp / dist), so the constraint applies
    /// before probabilistic sampling.
    pub fn grammar(self, model: &Model, grammar_str: &str, grammar_root: &str) -> Self {
        let g = std::ffi::CString::new(grammar_str).expect("grammar contains nul");
        let r = std::ffi::CString::new(grammar_root).expect("grammar_root contains nul");
        let s = unsafe {
            let vocab = sys::llama_model_get_vocab(model.ptr.as_ptr());
            sys::llama_sampler_init_grammar(vocab, g.as_ptr(), r.as_ptr())
        };
        // llama.cpp returns NULL on grammar parse failure. Adding a null
        // sampler to the chain crashes inside llama_sampler_sample on
        // first use (verified 2026-04-20: 'scheduler closed without Done
        // event' for all personas when JSON grammar didn't parse). Skip
        // the null pointer rather than ship a corrupted chain — caller
        // gets unconstrained sampling instead of a crash.
        if s.is_null() {
            eprintln!("[safe.rs] grammar parse failed for root='{grammar_root}' — skipping (chain unconstrained)");
            return self;
        }
        self.add(s)
    }

    pub fn build(self) -> Sampler {
        Sampler { ptr: self.chain }
    }
}
