//! A sampler pipeline: the chain the engine samples each token through (moved from expert_eval_cb.rs, unchanged).

use super::*;

/// A GBNF constraint for the chain: the grammar, its start symbol, and (for a lazy grammar) what
/// wakes it.
#[derive(Debug, Clone, Copy)]
pub struct Grammar<'a> {
    pub gbnf: &'a str,
    pub root: &'a str,
    pub lazy: Option<LazyTriggers<'a>>,
}

/// What wakes a lazy grammar: regex patterns matched against the output from its start (the
/// grammar is fed from the first match group), and tokens that wake it as they are sampled.
#[derive(Debug, Clone, Copy)]
pub struct LazyTriggers<'a> {
    pub patterns: &'a [String],
    pub tokens: &'a [i32],
}

/// Why a grammar could not join the chain.
#[derive(Debug, thiserror::Error)]
pub enum GrammarRefusal {
    #[error("the grammar, its root or a trigger pattern holds a NUL byte")]
    Nul(#[from] std::ffi::NulError),
    #[error("llama.cpp could not parse the grammar")]
    Unparsed,
}

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

    /// Add a GBNF grammar constraint: tokens the grammar rejects get probability zero. Needs the
    /// model's vocab, and belongs early in the chain (before temp / dist) so the constraint applies
    /// before probabilistic sampling. A lazy grammar (`grammar.lazy`) stays inert until one of its
    /// triggers appears, as the model's chat handling asks for tool calls: free text, then a call
    /// held to its schema.
    ///
    /// A grammar llama.cpp cannot parse is refused, never skipped: a turn that asked for a
    /// constraint and silently sampled without one emits what the caller said it must not.
    pub fn grammar(self, model: &Model, grammar: &Grammar<'_>) -> Result<Self, GrammarRefusal> {
        let gbnf = std::ffi::CString::new(grammar.gbnf)?;
        let root = std::ffi::CString::new(grammar.root)?;
        let vocab = unsafe { sys::llama_model_get_vocab(model.ptr.as_ptr()) };
        let s = match grammar.lazy {
            None => unsafe { sys::llama_sampler_init_grammar(vocab, gbnf.as_ptr(), root.as_ptr()) },
            Some(lazy) => {
                let patterns = lazy.patterns.iter().map(|p| std::ffi::CString::new(p.as_str())).collect::<Result<Vec<_>, _>>()?;
                let pattern_ptrs: Vec<*const std::ffi::c_char> = patterns.iter().map(|p| p.as_ptr()).collect();
                unsafe {
                    sys::llama_sampler_init_grammar_lazy_patterns(
                        vocab,
                        gbnf.as_ptr(),
                        root.as_ptr(),
                        pattern_ptrs.as_ptr().cast_mut(),
                        pattern_ptrs.len(),
                        lazy.tokens.as_ptr(),
                        lazy.tokens.len(),
                    )
                }
            }
        };
        if s.is_null() {
            return Err(GrammarRefusal::Unparsed);
        }
        Ok(self.add(s))
    }

    pub fn build(self) -> Sampler {
        Sampler { ptr: self.chain }
    }
}
