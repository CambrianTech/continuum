use super::*;

/// The ggml eval callback. Called twice per graph node: `ask=true` BEFORE compute (return
/// true to be re-called after), `ask=false` AFTER compute (data resident). We read only on
/// `ask=false`. Always returns `true` to keep the graph running — observation never
/// perturbs inference (`prediction is a HINT, never a gate`).
pub(super) unsafe extern "C" fn expert_eval_cb(
    t: *mut sys::ggml_tensor,
    ask: bool,
    user_data: *mut std::os::raw::c_void,
) -> bool {
    if ask || t.is_null() || user_data.is_null() {
        return true;
    }
    // SAFETY: user_data is the &ExpertObserverBridge set in new_context; it outlives every
    // callback (freed in Context::drop, after llama_free).
    let bridge = &*(user_data as *const ExpertObserverBridge);

    // Cheap name filter — only ffn_moe_topk-<il> nodes carry expert selections.
    let name_ptr = sys::ggml_get_name(t);
    if name_ptr.is_null() {
        return true;
    }
    let Some(layer) = std::ffi::CStr::from_ptr(name_ptr)
        .to_str()
        .ok()
        .and_then(parse_moe_topk_layer)
    else {
        return true;
    };

    // selected_experts is I32 `[n_expert_used, n_tokens]`. Guard the type defensively —
    // never read a non-I32 tensor as i32.
    let ty = (*t).type_;
    let (ne0, ne1) = ((*t).ne[0], (*t).ne[1]);
    if ty != sys::ggml_type_GGML_TYPE_I32 || ne0 <= 0 || ne1 <= 0 {
        return true;
    }
    let count = (ne0 as usize).saturating_mul(ne1 as usize);
    let mut buf = vec![0i32; count];
    // Copy from the backend buffer (GPU or CPU) into host memory.
    sys::ggml_backend_tensor_get(
        t,
        buf.as_mut_ptr() as *mut std::os::raw::c_void,
        0,
        count * std::mem::size_of::<i32>(),
    );
    // ne0 = n_expert_used (the row width); pass it so the observer can unpack per-token
    // rows for cross-layer trajectory capture. ne0 > 0 guaranteed by the guard above.
    bridge.observer.observe(layer, &buf, ne0 as usize);
    true
}

/// Context parameters.
#[derive(Debug, Clone)]
pub struct ContextParams {
    pub n_ctx: u32,
    pub n_batch: u32,
    /// Physical Metal/CUDA graph size for prompt processing. Keep separate
    /// from n_batch so the scheduler can accept larger logical prompt chunks
    /// while reserving smaller backend graphs on model families with fragile
    /// fused kernels.
    pub n_ubatch: u32,
    /// Maximum parallel sequences. Default llama.cpp sets this > 1 which
    /// DIVIDES n_ctx among sequences — a 4096 n_ctx with default n_seq_max
    /// yields only ~512-1024 usable positions per sequence, making RAG
    /// prompts >1k tokens fail `llama_decode` with rc=1 ("no KV slot").
    /// Single-persona chat only uses sequence 0, so default to 1.
    pub n_seq_max: u32,
    /// Flash attention setting. Default `Auto` — runtime picks per-backend.
    pub flash_attn: FlashAttn,
    /// Fused Gated Delta Net autoregressive graph. Some new Metal stacks can
    /// compile the kernels but throw foreign exceptions during graph setup;
    /// callers can disable the fused graph while keeping the model on GPU.
    pub fused_gdn_ar: bool,
    /// Fused Gated Delta Net chunked graph. Same contract as fused_gdn_ar.
    pub fused_gdn_ch: bool,
    /// KV cache element type for K. Default `F16` (lossless).
    pub type_k: KvCacheType,
    /// KV cache element type for V. Default `F16` (lossless).
    pub type_v: KvCacheType,
    /// Put the context into EMBEDDING mode (`llama_context_params.embeddings`).
    /// Default false (generation). Required for [`Context::embed`]; a context
    /// built for generation cannot embed and vice-versa.
    pub embeddings: bool,
    /// How per-token embeddings are pooled into one sequence vector
    /// (`llama_pooling_type`). Only meaningful when `embeddings == true`. Default
    /// `Mean`. Retrieval embedders are trained for a specific pooling — set it to
    /// match the model (Qwen3-Embedding-0.6B uses last-token).
    pub pooling_type: PoolingType,
    /// Optional live MoE expert-selection observer (the OBSERVE seam of the
    /// expert-affinity predictor). When set, a ggml eval-callback taps the
    /// `ffn_moe_topk` nodes and feeds every `(layer, selected experts)` to it. `None`
    /// (default) registers NO callback — a non-MoE context pays exactly zero overhead.
    pub expert_observer: Option<std::sync::Arc<dyn ExpertObserver>>,
}

/// Sequence-embedding pooling strategy — maps to `llama_pooling_type`. Set it to
/// match the embedding model's training; wrong pooling = degraded retrieval (a
/// silent-quality bug, not a crash).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolingType {
    /// No pooling — per-token embeddings only.
    None,
    /// Mean of token embeddings (common default).
    Mean,
    /// CLS / first-token embedding.
    Cls,
    /// Last-token embedding (Qwen3-Embedding family).
    Last,
}

impl Default for ContextParams {
    fn default() -> Self {
        Self {
            n_ctx: 4096,
            n_batch: 512,
            n_ubatch: 512,
            n_seq_max: 1,
            flash_attn: FlashAttn::Auto,
            fused_gdn_ar: true,
            fused_gdn_ch: true,
            type_k: KvCacheType::F16,
            type_v: KvCacheType::F16,
            embeddings: false,
            pooling_type: PoolingType::Mean,
            expert_observer: None,
        }
    }
}

/// An inference context for a model. Single-threaded.
pub struct Context<'m> {
    pub(super) ptr: NonNull<sys::llama_context>,
    pub(super) _model: PhantomData<&'m Model>,
    /// Keeps the expert-observer bridge alive for the callback's `user_data` pointer.
    /// Declared AFTER `ptr` so it drops after `Drop::drop`'s `llama_free` runs — no
    /// callback can fire against a freed bridge. `None` when no observer was set.
    pub(super) _observer: Option<Box<ExpertObserverBridge>>,
}

impl<'m> Context<'m> {
    /// Raw pointer to the underlying llama_context. Required by sibling
    /// crates that bind to FFI APIs taking `llama_context*` (e.g., the
    /// multimodal projector via `mtmd_helper_eval_chunks`). Pointer is
    /// valid for the Context's lifetime; callers MUST NOT free it.
    pub fn as_ptr(&mut self) -> *mut sys::llama_context {
        self.ptr.as_ptr()
    }

    /// Context window size.
    pub fn n_ctx(&self) -> u32 {
        unsafe { sys::llama_n_ctx(self.ptr.as_ptr()) }
    }

    /// Bytes llama.cpp has actually committed to the KV cache for the given
    /// sequence id. The honest source of truth for per-seq KV size — works
    /// across any model architecture (uniform attention, hybrid attention+SSM
    /// like qwen3.5 where only some layers carry KV, MoE) because llama.cpp
    /// computes it from the actual cache layout it built, not from a Rust-side
    /// "just multiply n_layer × n_head_kv × head_dim" estimate that drifts on
    /// hybrid arches.
    ///
    /// Returns 0 if the seq doesn't exist or has no committed KV (e.g.,
    /// before its first decode). Used by the FootprintRegistry to attribute
    /// per-persona KV bytes — see `inference::footprint_registry`.
    pub fn seq_state_bytes(&self, seq_id: i32) -> u64 {
        unsafe { sys::llama_state_seq_get_size(self.ptr.as_ptr(), seq_id) as u64 }
    }

    /// Process a batch through the model (updates KV cache, produces logits
    /// for tokens where `batch.push(..., want_logits=true)` was called).
    ///
    /// Return codes from llama.cpp:
    ///   0 — success
    ///   1 — no KV slot (reduce batch or raise n_ctx)
    ///   2 — aborted (partial state retained)
    ///  -1 — invalid batch
    /// <-1 — fatal
    pub fn decode(&mut self, batch: &Batch) -> Result<(), String> {
        let rc = unsafe { sys::llama_decode(self.ptr.as_ptr(), batch.inner) };
        if rc == 0 {
            Ok(())
        } else {
            Err(format!("llama_decode returned {rc}"))
        }
    }

    /// Compute a pooled, L2-normalized embedding for `tokens`. Requires the
    /// context to have been built with `ContextParams { embeddings: true,
    /// pooling_type: ... }`. Decodes the tokens as sequence 0, then reads the
    /// pooled sequence embedding via `llama_get_embeddings_seq`. The vector is
    /// L2-normalized so callers use a plain dot product for cosine.
    ///
    /// Fails LOUD on a null or degenerate (zero / non-finite) embedding — a null
    /// pointer means the context was NOT built in embedding mode (or nothing
    /// decoded). Per the no-silent-degrade rule we never feed an empty/short
    /// vector into recall.
    pub fn embed(&mut self, tokens: &[i32]) -> Result<Vec<f32>, String> {
        if tokens.is_empty() {
            return Err("embed: empty token slice".to_string());
        }
        let n_embd = unsafe { sys::llama_model_n_embd(self.model_ptr()) };
        if n_embd <= 0 {
            return Err(format!("embed: invalid n_embd {n_embd}"));
        }
        let batch = Batch::for_tokens(tokens.to_vec());
        self.decode(&batch)?;
        let ptr = unsafe { sys::llama_get_embeddings_seq(self.ptr.as_ptr(), 0) };
        if ptr.is_null() {
            return Err("embed: llama_get_embeddings_seq returned null — context not in \
                        embedding mode (ContextParams.embeddings=true + a pooling_type), \
                        or nothing was decoded"
                .to_string());
        }
        let raw = unsafe { std::slice::from_raw_parts(ptr, n_embd as usize) };
        // L2-normalize so downstream cosine is a dot product; reject degenerate.
        let norm: f32 = raw.iter().map(|x| x * x).sum::<f32>().sqrt();
        if !norm.is_finite() || norm == 0.0 {
            return Err("embed: degenerate (zero / non-finite) embedding".to_string());
        }
        Ok(raw.iter().map(|x| x / norm).collect())
    }

    /// Logits for the i-th token position in the last batch (tokens with
    /// `want_logits=true`). Length = n_vocab.
    ///
    /// Use `-1` for the last token that had logits requested.
    pub fn logits_ith(&self, i: i32) -> &[f32] {
        let n_vocab =
            unsafe { sys::llama_vocab_n_tokens(sys::llama_model_get_vocab(self.model_ptr())) }
                as usize;
        unsafe {
            let ptr = sys::llama_get_logits_ith(self.ptr.as_ptr(), i);
            if ptr.is_null() {
                &[]
            } else {
                std::slice::from_raw_parts(ptr, n_vocab)
            }
        }
    }

    /// Mutable logits for the i-th position — for repetition penalty / logit bias
    /// applied before sampling without routing through a sampler.
    pub fn logits_ith_mut(&mut self, i: i32) -> &mut [f32] {
        let n_vocab =
            unsafe { sys::llama_vocab_n_tokens(sys::llama_model_get_vocab(self.model_ptr())) }
                as usize;
        unsafe {
            let ptr = sys::llama_get_logits_ith(self.ptr.as_ptr(), i);
            if ptr.is_null() {
                &mut []
            } else {
                std::slice::from_raw_parts_mut(ptr, n_vocab)
            }
        }
    }

    /// Set the active LoRA adapter set on this context.
    ///
    /// This is the hot-swap primitive — cheap enough to call between tokens
    /// for genome paging. Passing an empty slice clears all adapters.
    pub fn set_loras(&mut self, adapters: &[(&LoraAdapter, f32)]) -> Result<(), String> {
        let mut ptrs: Vec<*mut sys::llama_adapter_lora> =
            adapters.iter().map(|(a, _)| a.ptr.as_ptr()).collect();
        let mut scales: Vec<f32> = adapters.iter().map(|(_, s)| *s).collect();
        let rc = unsafe {
            sys::llama_set_adapters_lora(
                self.ptr.as_ptr(),
                if ptrs.is_empty() {
                    std::ptr::null_mut()
                } else {
                    ptrs.as_mut_ptr()
                },
                ptrs.len(),
                if scales.is_empty() {
                    std::ptr::null_mut()
                } else {
                    scales.as_mut_ptr()
                },
            )
        };
        if rc == 0 {
            Ok(())
        } else {
            Err(format!("llama_set_adapters_lora returned {rc}"))
        }
    }

    /// Clear all LoRA adapters.
    pub fn clear_loras(&mut self) -> Result<(), String> {
        self.set_loras(&[])
    }

    /// Number of threads used for single-token generation.
    pub fn set_n_threads(&mut self, n_threads: i32, n_threads_batch: i32) {
        unsafe {
            sys::llama_set_n_threads(self.ptr.as_ptr(), n_threads, n_threads_batch);
        }
    }

    pub(super) fn model_ptr(&self) -> *const sys::llama_model {
        unsafe { sys::llama_get_model(self.ptr.as_ptr()) }
    }

    /// Free the KV cache for a given sequence id (positions [p0, p1)).
    /// Use `p0=-1, p1=-1` to remove the whole sequence. Required when
    /// reusing a seq_id slot in a shared multi-sequence context.
    pub fn memory_seq_rm(&mut self, seq_id: i32, p0: i32, p1: i32) -> bool {
        unsafe {
            let mem = sys::llama_get_memory(self.ptr.as_ptr());
            sys::llama_memory_seq_rm(mem, seq_id, p0, p1)
        }
    }

    /// Wipe the entire KV cache for this context.
    /// `data=true` also clears the underlying data buffers; `false` only
    /// clears metadata (faster, sufficient for reuse-without-leak).
    pub fn memory_clear(&mut self, data: bool) {
        unsafe {
            let mem = sys::llama_get_memory(self.ptr.as_ptr());
            sys::llama_memory_clear(mem, data);
        }
    }

    /// Highest absolute KV position currently held for `seq_id`, or -1 if
    /// the sequence is empty. Used by the batch scheduler to know what
    /// `pos` to assign the next token in a continuous-batching loop.
    pub fn memory_seq_pos_max(&self, seq_id: i32) -> i32 {
        unsafe {
            let mem = sys::llama_get_memory(self.ptr.as_ptr());
            sys::llama_memory_seq_pos_max(mem, seq_id)
        }
    }
}

impl<'m> Drop for Context<'m> {
    fn drop(&mut self) {
        unsafe {
            sys::llama_free(self.ptr.as_ptr());
        }
    }
}

/// A batch of tokens to feed into `Context::decode`.
///
/// Two construction modes:
///   * `for_tokens(&[...])` — single sequence, positions auto-assigned.
///     Cheapest path for generation. Uses `llama_batch_get_one`.
///   * `allocated(n_tokens, n_seq_max)` — preallocated for manual push,
///     supports multi-sequence. Uses `llama_batch_init` / `_free`.
pub struct Batch {
    pub(super) inner: sys::llama_batch,
    pub(super) storage: BatchStorage,
}

pub(super) enum BatchStorage {
    /// Owns the token Vec that `inner.token` points into. The Vec is never
    /// read directly from Rust — llama.cpp dereferences the raw pointer
    /// during decode — but it MUST outlive the batch, so we pin it here.
    OneSequence(#[allow(dead_code)] Vec<i32>),
    /// C-allocated via `llama_batch_init`; must `llama_batch_free` on drop.
    /// The seq_id[i] slots are llama-owned (pre-allocated with n_seq_max
    /// slots each); we write INTO them on push, never replace the pointers.
    /// `capacity` is the llama-allocated array length — push must fail
    /// (not corrupt memory) if callers try to exceed it.
    Allocated { n_seq_max: i32, capacity: i32 },
}

unsafe impl Send for Batch {}

impl Batch {
    /// Single-sequence batch of the given tokens. Positions auto-assigned by
    /// `llama_decode` based on KV state. Only the last token gets logits.
    pub fn for_tokens(mut tokens: Vec<i32>) -> Self {
        backend_init();
        // SAFETY: tokens' backing storage is kept alive via storage field;
        // llama_batch_get_one points at the slice, does not take ownership.
        let inner = unsafe { sys::llama_batch_get_one(tokens.as_mut_ptr(), tokens.len() as i32) };
        Self {
            inner,
            storage: BatchStorage::OneSequence(tokens),
        }
    }

    /// Preallocated batch capable of holding up to `n_tokens` with up to
    /// `n_seq_max` sequences per token. Populate with `push`.
    pub fn allocated(n_tokens: i32, n_seq_max: i32) -> Self {
        backend_init();
        let inner = unsafe { sys::llama_batch_init(n_tokens, 0, n_seq_max) };
        let mut b = Self {
            inner,
            storage: BatchStorage::Allocated {
                n_seq_max,
                capacity: n_tokens,
            },
        };
        // init leaves n_tokens uninitialized; clear forces it to 0.
        b.clear();
        b
    }

    /// Append a token to an `allocated` batch. Panics if called on a
    /// `for_tokens` batch, if the batch is already at capacity, or if
    /// `seq_ids.len() > n_seq_max`.
    pub fn push(&mut self, token: i32, pos: i32, seq_ids: &[i32], want_logits: bool) {
        let (n_seq_max, capacity) = match self.storage {
            BatchStorage::Allocated {
                n_seq_max,
                capacity,
            } => (n_seq_max, capacity),
            BatchStorage::OneSequence(_) => panic!("push() on single-sequence batch"),
        };
        assert!(
            self.inner.n_tokens < capacity,
            "Batch::push overflow: n_tokens={} already at capacity={}. \
             Chunk your prefill into capacity-sized decode calls \
             (prompts longer than the batch size must be decoded in pieces).",
            self.inner.n_tokens,
            capacity
        );
        assert!(
            seq_ids.len() as i32 <= n_seq_max,
            "seq_ids.len()={} exceeds n_seq_max={}",
            seq_ids.len(),
            n_seq_max
        );
        let idx = self.inner.n_tokens as usize;
        // SAFETY: we write INTO llama-allocated arrays (token/pos/n_seq_id/
        // logits each sized n_tokens; seq_id[idx] sized n_seq_max). We do not
        // replace any pointer — `llama_batch_free` walks seq_id[] and frees
        // each slot, so keeping llama's allocation intact is required.
        unsafe {
            *self.inner.token.add(idx) = token;
            *self.inner.pos.add(idx) = pos;
            *self.inner.n_seq_id.add(idx) = seq_ids.len() as i32;
            let seq_slot = *self.inner.seq_id.add(idx);
            for (i, &sid) in seq_ids.iter().enumerate() {
                *seq_slot.add(i) = sid;
            }
            *self.inner.logits.add(idx) = i8::from(want_logits);
        }
        self.inner.n_tokens += 1;
    }

    /// Reset an `allocated` batch to empty.
    pub fn clear(&mut self) {
        self.inner.n_tokens = 0;
    }

    pub fn n_tokens(&self) -> i32 {
        self.inner.n_tokens
    }
}

impl Drop for Batch {
    fn drop(&mut self) {
        if matches!(self.storage, BatchStorage::Allocated { .. }) {
            unsafe {
                sys::llama_batch_free(self.inner);
            }
        }
        // OneSequence: Vec drop handles token memory; batch struct itself is
        // stack-allocated, no free needed.
    }
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
