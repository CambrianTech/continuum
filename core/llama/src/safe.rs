//! Safe Rust wrapper over llama.cpp FFI.

use std::ffi::CString;
use std::marker::PhantomData;
use std::os::raw::c_char;
use std::path::Path;
use std::ptr::NonNull;
use std::sync::Once;

use crate::sys;

static BACKEND_INIT: Once = Once::new();

/// Initialize the llama backend. Idempotent.
///
/// Also force-registers all compiled-in ggml backends (Metal, CUDA, BLAS,
/// CPU). The +whole-archive link modifier should be enough on its own, but
/// in practice rlib metadata or downstream bin link order can lose the
/// static-initializer invocations — so we make a direct call to guarantee
/// backends are populated before the first model load. Without this, the
/// llama_model_load path can segfault in ggml_backend_dev_type() when the
/// backend registry is empty.
///
/// On macOS with the metal feature we ALSO call `ggml_backend_metal_reg()`
/// directly. Verified 2026-04-19: even with `+whole-archive=ggml-metal`,
/// `nm` on the linked binary showed zero `ggml_backend_metal_*` symbols,
/// causing `load_tensors: layer N assigned to device CPU` for ALL 32 layers
/// of qwen3.5-4b — i.e. inference was running 100% on CPU at 33 tok/s. The
/// explicit register call from Rust creates a live reference path the
/// linker can't strip, forcing the Metal backend to load and register
/// before the first model is read. Same defensive pattern for CUDA on
/// Linux + Vulkan on Linux when those features are enabled.
pub fn backend_init() {
    BACKEND_INIT.call_once(|| {
        unsafe {
            sys::llama_backend_init();
            sys::ggml_backend_load_all();

            // Force-register statically linked GPU backends ONLY IF NOT
            // ALREADY PRESENT. Earlier comment claimed
            // `ggml_backend_register` was idempotent — it is NOT. Reading
            // ggml-backend-reg.cpp, register_backend() unconditionally
            // push_backs onto the backends vector, with no identity check.
            // Verified 2026-04-21 against Qwen2-VL-7B: when Metal was
            // double-registered (static-init path ran AND we called the
            // defensive register), the vision encoder's first-token
            // logits diverged dramatically — top token became
            // `<|box_start|>` (bbox detection) instead of `A` (natural
            // language description). Same model files via brew's
            // mtmd-cli → correct output. Same C reproducer linking the
            // SAME vendored .a files → correct output. Only the Rust
            // path with the duplicate register call diverged. Removing
            // the duplicate register restored vision behavior end-to-end.
            //
            // The defensive register from #38 still earns its keep when
            // dead_strip DID drop the static initializer (otherwise we
            // silently run on CPU). Guard it so it only fires in that
            // case: scan the registered backends by name and skip if the
            // expected one is already there.
            #[cfg(all(feature = "metal", target_os = "macos"))]
            ensure_backend_registered("Metal", || sys::ggml_backend_metal_reg());

            #[cfg(all(feature = "cuda", target_os = "linux"))]
            ensure_backend_registered("CUDA", || sys::ggml_backend_cuda_reg());

            #[cfg(all(feature = "vulkan", target_os = "linux"))]
            ensure_backend_registered("Vulkan", || sys::ggml_backend_vk_reg());

            // Fail-hard guard. If we're on a platform that should have a GPU
            // backend but the registry only contains CPU after registration,
            // we're about to silently run inference on CPU at ~5x slower than
            // GPU — exactly the regression we just diagnosed and fixed. Per
            // the no-silent-degrade rule, panic loudly with an actionable
            // message rather than ship CPU performance dressed as Metal.
            //
            // The check counts non-CPU registered devices via the public
            // backend registry API. If it fails, the build has lost the
            // GPU backend somewhere between cmake config, link, and load.
            assert_gpu_backend_registered_when_expected();
        }
    });
}

/// Register `reg_factory()`'s backend iff its exact `ggml_backend_reg_t`
/// pointer is NOT already in the registry. Guards against
/// double-registration — `ggml_backend_register` does NOT dedup (verified
/// 2026-04-21 by reading ggml-backend-reg.cpp::register_backend, which
/// unconditionally push_backs onto the backends vector).
///
/// Pointer identity is the right comparison here: `ggml_backend_metal_reg()`
/// (and its CUDA/Vulkan peers) returns a pointer to a process-wide static
/// registry entry. If the static initializer already registered it, the
/// same pointer is already in the list. Name-matching would also work but
/// drifts with upstream string choices (Metal's name is "MTL" not "Metal").
///
/// Double-registration symptom (2026-04-21): Qwen2-VL-7B vision encoder
/// first-token logits diverged — top token became `<|box_start|>` (bbox
/// detection mode) instead of `A` (natural-language description). The
/// model files + prompt + context params were identical to brew's
/// mtmd-cli and a C reproducer; only the Rust path hit this because only
/// Rust was calling the defensive register after ggml_backend_load_all.
#[allow(dead_code)] // used only under GPU feature gates
unsafe fn ensure_backend_registered(
    _tag: &str,
    reg_factory: impl FnOnce() -> sys::ggml_backend_reg_t,
) {
    let candidate = reg_factory();
    if candidate.is_null() {
        return; // factory returned nothing — nothing to register
    }
    let n = sys::ggml_backend_reg_count();
    for i in 0..n {
        if sys::ggml_backend_reg_get(i) == candidate {
            return; // static init or load_all already added this exact backend
        }
    }
    // Not present — the defensive path from #38: static init got
    // stripped, so register explicitly.
    sys::ggml_backend_register(candidate);
}

/// Walks the registered backend devices and asserts that — if the build
/// expected a GPU backend (Mac+metal, Linux+cuda, Linux+vulkan) — at least
/// one non-CPU device is present. Panics with an actionable message if not.
///
/// The point is to catch the failure mode we discovered 2026-04-19: a build
/// that thinks it has Metal but actually only has CPU because the feature
/// flag wasn't propagated. That used to silently run at ~33 tok/s instead
/// of GPU speed; now it crashes at startup so the cause is unmissable.
unsafe fn assert_gpu_backend_registered_when_expected() {
    let expects_gpu = cfg!(any(
        all(feature = "metal", target_os = "macos"),
        all(feature = "cuda", target_os = "linux"),
        all(feature = "vulkan", target_os = "linux"),
    ));
    if !expects_gpu {
        return;
    }

    let n_devices = sys::ggml_backend_dev_count();
    let mut found_gpu = false;
    let mut device_names: Vec<String> = Vec::new();
    for i in 0..n_devices {
        let dev = sys::ggml_backend_dev_get(i);
        if dev.is_null() {
            continue;
        }
        let dev_type = sys::ggml_backend_dev_type(dev);
        let name_ptr = sys::ggml_backend_dev_name(dev);
        let name = if name_ptr.is_null() {
            "<unnamed>".to_string()
        } else {
            std::ffi::CStr::from_ptr(name_ptr)
                .to_string_lossy()
                .into_owned()
        };
        // Anything that isn't CPU counts as a GPU/accelerator device for
        // this purpose. ggml_backend_dev_type_GGML_BACKEND_DEVICE_TYPE_CPU
        // is the constant we're excluding; everything else (GPU, ACCEL)
        // satisfies the guard.
        if dev_type != sys::ggml_backend_dev_type_GGML_BACKEND_DEVICE_TYPE_CPU {
            found_gpu = true;
        }
        device_names.push(format!("{}({:?})", name, dev_type));
    }

    if !found_gpu {
        panic!(
            "FATAL: build expected a GPU backend (Mac+metal / Linux+cuda / \
             Linux+vulkan) but the ggml backend registry only has CPU \
             devices after init. Refusing to run inference at CPU speeds \
             dressed as GPU. Registered devices: {:?}. Fix: rebuild with \
             the appropriate `--features` flag (`metal`, `cuda`, `vulkan`) \
             OR update llama/build.rs so the static GPU backend archive \
             actually links into the binary.",
            device_names
        );
    }
}

/// A loaded llama model. Thread-safe (contexts are single-threaded but model is shared).
pub struct Model {
    ptr: NonNull<sys::llama_model>,
}

unsafe impl Send for Model {}
unsafe impl Sync for Model {}

/// One message in a chat sequence: role + content. Input to `render_chat`.
#[derive(Debug, Clone)]
pub struct ChatMsg {
    pub role: String,
    pub content: String,
}

/// Render a chat sequence through a Jinja-style template string, using
/// llama.cpp's built-in template engine. Pure function — takes the
/// template directly so it's unit-testable without loading a GGUF.
///
/// `template`: the model's `tokenizer.chat_template` string, typically
/// obtained from `Model::chat_template()`. If you pass a non-existent
/// template string llama.cpp falls back to a chatml default — prefer
/// making the caller decide what to do when the model doesn't carry one.
///
/// `add_assistant`: append the assistant-turn-start tokens, telling the
/// model "now generate a reply." Set true for inference, false for
/// evaluating an existing assistant message.
///
/// Returns the rendered prompt string ready for tokenization. Callers
/// must NEVER hand-roll `<|im_start|>...` prefixes — different models
/// use different boundary tokens, and getting it wrong causes the model
/// to emit the boundary tokens as text (the `<|im_end<|>` leak we saw
/// in Teacher AI output 2026-04-20).
pub fn render_chat(
    template: Option<&str>,
    messages: &[ChatMsg],
    add_assistant: bool,
) -> Result<String, String> {
    if messages.is_empty() {
        return Err("render_chat: messages empty".to_string());
    }
    // None → pass NULL to llama.cpp; it falls back to its built-in chatml
    // default. Useful for GGUFs that don't embed a template in metadata
    // (continuum-ai/qwen3.5-4b-code-forged is one such model — see
    // forge recipe TODO to add tokenizer.chat_template at next bake).
    let tmpl_c = template
        .map(|t| CString::new(t).map_err(|e| format!("template has nul byte: {e}")))
        .transpose()?;
    let owned: Vec<(CString, CString)> = messages
        .iter()
        .map(|m| {
            let r = CString::new(m.role.as_str()).map_err(|e| format!("role {e}"))?;
            let c = CString::new(m.content.as_str()).map_err(|e| format!("content {e}"))?;
            Ok::<(CString, CString), String>((r, c))
        })
        .collect::<Result<_, _>>()?;
    let chat: Vec<sys::llama_chat_message> = owned
        .iter()
        .map(|(r, c)| sys::llama_chat_message {
            role: r.as_ptr(),
            content: c.as_ptr(),
        })
        .collect();

    let tmpl_ptr = tmpl_c
        .as_ref()
        .map(|c| c.as_ptr())
        .unwrap_or(std::ptr::null());
    let render = |buf: &mut Vec<i8>| -> i32 {
        unsafe {
            sys::llama_chat_apply_template(
                tmpl_ptr,
                chat.as_ptr(),
                chat.len(),
                add_assistant,
                // Cast to *mut c_char so the call type-checks on both
                // macOS (c_char = i8) and Linux (c_char = u8). Without
                // this cast the bare *mut i8 from Vec<i8>::as_mut_ptr()
                // mismatches Linux's *mut u8 expectation, breaking the
                // docker Linux build (caught by pre-push docker phase
                // on commit fa4b1034d's push attempt).
                buf.as_mut_ptr() as *mut std::os::raw::c_char,
                buf.len() as i32,
            )
        }
    };

    let initial: usize = messages
        .iter()
        .map(|m| m.role.len() + m.content.len())
        .sum::<usize>()
        * 2
        + 256;
    let mut buf = vec![0i8; initial];
    let mut n = render(&mut buf);
    if n < 0 {
        return Err(format!("llama_chat_apply_template rc={n}"));
    }
    if (n as usize) > buf.len() {
        buf.resize(n as usize, 0);
        n = render(&mut buf);
        if n < 0 || (n as usize) > buf.len() {
            return Err(format!("llama_chat_apply_template retry rc={n}"));
        }
    }
    let bytes: Vec<u8> = buf.into_iter().take(n as usize).map(|b| b as u8).collect();
    String::from_utf8(bytes).map_err(|e| format!("template output not utf-8: {e}"))
}

/// Model load parameters.
#[derive(Debug, Clone)]
pub struct ModelParams {
    /// Number of layers to offload to GPU (-1 = all)
    pub n_gpu_layers: i32,
    /// mmap the file (faster load, usually safe)
    pub use_mmap: bool,
}

impl Default for ModelParams {
    fn default() -> Self {
        Self {
            n_gpu_layers: -1,
            use_mmap: true,
        }
    }
}

impl Model {
    /// Load a GGUF model from disk.
    pub fn load(path: impl AsRef<Path>, params: ModelParams) -> Result<Self, String> {
        backend_init();

        let path = path.as_ref();
        let c_path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|e| format!("invalid path: {e}"))?;

        let mut ffi_params = unsafe { sys::llama_model_default_params() };
        ffi_params.n_gpu_layers = params.n_gpu_layers;
        ffi_params.use_mmap = params.use_mmap;

        let raw = unsafe { sys::llama_model_load_from_file(c_path.as_ptr(), ffi_params) };
        let ptr = NonNull::new(raw)
            .ok_or_else(|| format!("failed to load model from {}", path.display()))?;

        Ok(Self { ptr })
    }

    /// Vocabulary size.
    pub fn n_vocab(&self) -> i32 {
        unsafe { sys::llama_vocab_n_tokens(sys::llama_model_get_vocab(self.ptr.as_ptr())) }
    }

    /// Hidden-state dimension (embedding size).
    pub fn n_embd(&self) -> i32 {
        unsafe { sys::llama_model_n_embd(self.ptr.as_ptr()) }
    }

    /// Trained context length, as recorded in the GGUF metadata
    /// (`<arch>.context_length`). This is the model's OWN ceiling — not
    /// a system default, not a RAG budget guess. Use this everywhere a
    /// "context window" is needed; if a smaller `n_ctx` is intentional
    /// (e.g. memory pressure on a tier with low VRAM), pass it explicitly
    /// rather than redefining the model's natural capability.
    pub fn n_ctx_train(&self) -> u32 {
        let n = unsafe { sys::llama_model_n_ctx_train(self.ptr.as_ptr()) };
        if n > 0 {
            n as u32
        } else {
            0
        }
    }

    /// Number of transformer blocks (layers). KV cache is allocated per
    /// layer, so this is a multiplier in the per-token KV memory cost.
    pub fn n_layer(&self) -> u32 {
        let n = unsafe { sys::llama_model_n_layer(self.ptr.as_ptr()) };
        n.max(0) as u32
    }

    /// Number of attention (query) heads.
    pub fn n_head(&self) -> u32 {
        let n = unsafe { sys::llama_model_n_head(self.ptr.as_ptr()) };
        n.max(0) as u32
    }

    /// Number of key/value heads (GQA: ≤ `n_head`). The KV cache stores one
    /// K and one V vector per KV head per layer per token, so this — not
    /// `n_head` — drives KV memory.
    pub fn n_head_kv(&self) -> u32 {
        let n = unsafe { sys::llama_model_n_head_kv(self.ptr.as_ptr()) };
        n.max(0) as u32
    }

    /// Create an inference context.
    pub fn new_context(&self, params: ContextParams) -> Result<Context<'_>, String> {
        let mut ffi = unsafe { sys::llama_context_default_params() };
        ffi.n_ctx = params.n_ctx;
        ffi.n_batch = params.n_batch;
        ffi.n_ubatch = params.n_ubatch;
        ffi.n_seq_max = params.n_seq_max;
        ffi.flash_attn_type = match params.flash_attn {
            FlashAttn::Auto => sys::llama_flash_attn_type_LLAMA_FLASH_ATTN_TYPE_AUTO,
            FlashAttn::Enabled => sys::llama_flash_attn_type_LLAMA_FLASH_ATTN_TYPE_ENABLED,
            FlashAttn::Disabled => sys::llama_flash_attn_type_LLAMA_FLASH_ATTN_TYPE_DISABLED,
        };
        ffi.fused_gdn_ar = params.fused_gdn_ar;
        ffi.fused_gdn_ch = params.fused_gdn_ch;
        ffi.type_k = match params.type_k {
            KvCacheType::F16 => sys::ggml_type_GGML_TYPE_F16,
            KvCacheType::Q8_0 => sys::ggml_type_GGML_TYPE_Q8_0,
        };
        ffi.type_v = match params.type_v {
            KvCacheType::F16 => sys::ggml_type_GGML_TYPE_F16,
            KvCacheType::Q8_0 => sys::ggml_type_GGML_TYPE_Q8_0,
        };
        ffi.embeddings = params.embeddings;
        ffi.pooling_type = match params.pooling_type {
            PoolingType::None => sys::llama_pooling_type_LLAMA_POOLING_TYPE_NONE,
            PoolingType::Mean => sys::llama_pooling_type_LLAMA_POOLING_TYPE_MEAN,
            PoolingType::Cls => sys::llama_pooling_type_LLAMA_POOLING_TYPE_CLS,
            PoolingType::Last => sys::llama_pooling_type_LLAMA_POOLING_TYPE_LAST,
        };

        // OBSERVE seam: if an expert observer is set, register the ggml eval-callback that
        // taps ffn_moe_topk. The bridge is boxed and owned by the returned Context, so its
        // pointer (the callback's user_data) stays valid for the context's whole life
        // (Box contents are address-stable across the move into Context) and is freed only
        // after llama_free (Context::drop's body runs first).
        let observer_bridge = params
            .expert_observer
            .map(|observer| Box::new(ExpertObserverBridge { observer }));
        if let Some(bridge) = observer_bridge.as_ref() {
            ffi.cb_eval = Some(expert_eval_cb);
            ffi.cb_eval_user_data =
                bridge.as_ref() as *const ExpertObserverBridge as *mut std::os::raw::c_void;
        }

        let raw = unsafe { sys::llama_new_context_with_model(self.ptr.as_ptr(), ffi) };
        let ctx = NonNull::new(raw).ok_or_else(|| "failed to create context".to_string())?;
        Ok(Context {
            ptr: ctx,
            _model: PhantomData,
            _observer: observer_bridge,
        })
    }

    /// Load a LoRA adapter bound to this model. Used for genome paging.
    ///
    /// The returned adapter must not outlive the model it was loaded from
    /// (llama.cpp frees adapter memory when the model is dropped). This is
    /// NOT expressed in the type system because self-referential owners
    /// (e.g. a backend holding `Model` + `HashMap<Id, LoraAdapter>`) can't
    /// be written safely with a borrowed lifetime. Callers must drop
    /// adapters before the model — typically via field ordering.
    pub fn load_lora(&self, path: impl AsRef<Path>) -> Result<LoraAdapter, String> {
        let path = path.as_ref();
        let c_path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|e| format!("invalid path: {e}"))?;
        let raw = unsafe { sys::llama_adapter_lora_init(self.ptr.as_ptr(), c_path.as_ptr()) };
        let ptr = NonNull::new(raw)
            .ok_or_else(|| format!("failed to load LoRA from {}", path.display()))?;
        Ok(LoraAdapter { ptr })
    }

    /// Tokenize a string.
    pub fn tokenize(&self, text: &str, add_bos: bool, special: bool) -> Result<Vec<i32>, String> {
        let vocab = unsafe { sys::llama_model_get_vocab(self.ptr.as_ptr()) };
        let c_text = CString::new(text).map_err(|e| format!("invalid text: {e}"))?;
        let text_len = c_text.as_bytes().len() as i32;

        // First call with size 0 to get required size (negative = required)
        let required = unsafe {
            sys::llama_tokenize(
                vocab,
                c_text.as_ptr(),
                text_len,
                std::ptr::null_mut(),
                0,
                add_bos,
                special,
            )
        };
        let size = required.unsigned_abs() as usize;
        let mut tokens = vec![0i32; size];
        let n = unsafe {
            sys::llama_tokenize(
                vocab,
                c_text.as_ptr(),
                text_len,
                tokens.as_mut_ptr(),
                size as i32,
                add_bos,
                special,
            )
        };
        if n < 0 {
            return Err(format!("tokenize failed: {n}"));
        }
        tokens.truncate(n as usize);
        Ok(tokens)
    }

    /// The model's embedded chat template string (GGUF metadata
    /// `tokenizer.chat_template`). `None` if the model carries no
    /// template — caller can pass a default to `render_chat` or error.
    pub fn chat_template(&self) -> Option<String> {
        let p = unsafe { sys::llama_model_chat_template(self.ptr.as_ptr(), std::ptr::null()) };
        if p.is_null() {
            None
        } else {
            unsafe { std::ffi::CStr::from_ptr(p) }
                .to_str()
                .ok()
                .map(String::from)
        }
    }

    /// Convert a token to its UTF-8 string representation.
    pub fn token_to_piece(&self, token: i32) -> String {
        let vocab = unsafe { sys::llama_model_get_vocab(self.ptr.as_ptr()) };
        let mut buf = vec![0u8; 128];
        let n = unsafe {
            sys::llama_token_to_piece(
                vocab,
                token,
                buf.as_mut_ptr() as *mut c_char,
                buf.len() as i32,
                0,
                false,
            )
        };
        if n < 0 {
            return String::new();
        }
        buf.truncate(n as usize);
        String::from_utf8_lossy(&buf).into_owned()
    }

    /// Check if token is end-of-generation.
    pub fn is_eog_token(&self, token: i32) -> bool {
        let vocab = unsafe { sys::llama_model_get_vocab(self.ptr.as_ptr()) };
        unsafe { sys::llama_vocab_is_eog(vocab, token) }
    }
}

impl Model {
    /// Raw pointer to the underlying llama_model. Required by sibling
    /// crates that bind to FFI APIs taking `const llama_model*` as input
    /// (e.g., the multimodal projector via `mtmd_init_from_file`). The
    /// pointer remains valid for the Model's lifetime; callers MUST NOT
    /// free it.
    pub fn as_ptr(&self) -> *mut sys::llama_model {
        self.ptr.as_ptr()
    }
}

impl Drop for Model {
    fn drop(&mut self) {
        unsafe {
            sys::llama_model_free(self.ptr.as_ptr());
        }
    }
}

// ─── LoRA adapter ────────────────────────────────────────────────────────

/// A LoRA adapter loaded against a model. Can be hot-swapped on a context
/// without rebuilding the context — this is the primitive genome paging uses.
///
/// No borrow on `Model` (see `Model::load_lora` docs for the invariant).
pub struct LoraAdapter {
    ptr: NonNull<sys::llama_adapter_lora>,
}

unsafe impl Send for LoraAdapter {}
unsafe impl Sync for LoraAdapter {}

impl Drop for LoraAdapter {
    fn drop(&mut self) {
        unsafe {
            sys::llama_adapter_lora_free(self.ptr.as_ptr());
        }
    }
}

// ─── Context ─────────────────────────────────────────────────────────────

/// Flash-attention selection for the context.
///
/// `Auto` (the default) lets the runtime decide per-backend — on Metal +
/// supported head dims (qwen3.5-4b's V head_dim=256 qualifies) llama.cpp
/// enables FA automatically. `Enabled` forces it on (will error if the
/// shape isn't supported). `Disabled` reverts to the unfused attention
/// path, which is what the binding's prior behavior was implicitly doing
/// because we never set the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashAttn {
    Auto,
    Enabled,
    Disabled,
}

/// KV cache element type. f16 is the lossless default. q8_0 halves the KV
/// memory footprint with <1% quality loss — enables more parallel sequences
/// and longer contexts at the same VRAM budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KvCacheType {
    F16,
    Q8_0,
}

impl KvCacheType {
    /// Bytes per cached element, scaled ×100 so block-quantized types keep
    /// their fractional cost in integer math. F16 = 2.00 bytes/elem.
    /// Q8_0 packs 32 quants + an f16 scale into a 34-byte block = 1.0625
    /// bytes/elem. Used to size KV memory from the model's real dimensions
    /// rather than guessing a context window — the same "derive from the
    /// real artifact, never hardcode" rule the rest of the loader follows.
    pub fn bytes_per_elem_x100(self) -> u64 {
        match self {
            KvCacheType::F16 => 200,
            KvCacheType::Q8_0 => 106,
        }
    }
}

// ─── MoE expert-selection observability (the OBSERVE half of the expert-affinity predictor) ───
//
// A ggml eval-callback taps the `ffn_moe_topk` node — llama.cpp's per-token, per-layer
// expert selection (argsort-top-k of expert indices, I32 `[n_expert_used, n_tokens]`,
// tagged `cb(selected_experts, "ffn_moe_topk", il)` in llama-graph.cpp). This is the
// "measured beats predicted" PGO signal: which experts ACTUALLY fire, live. core/llama
// stays generic — it emits `(layer, selected experts)` to a caller-provided
// `ExpertObserver`; the residency + prediction brain lives in continuum-core.

/// Sink for live MoE expert selections. `observe` is called from INSIDE the compute
/// callback for every `ffn_moe_topk` node, so it MUST be cheap + non-blocking (tally into
/// an atomic map — never lock, never do I/O). `Send + Sync` because the callback can run
/// on a backend thread; `Debug` so `ContextParams` keeps its derive.
pub trait ExpertObserver: Send + Sync + std::fmt::Debug {
    /// `layer` = transformer block index; `experts` = the selected expert indices for the
    /// tokens in this decode batch (flattened row-major `[n_expert_used, n_tokens]`).
    fn observe(&self, layer: u32, experts: &[i32]);
}

/// Parse the block index from a `ffn_moe_topk` node name. llama.cpp's graph callback
/// formats per-layer tensors as `"<name>-<il>"` (`ggml_format_name`), so the eval callback
/// sees `"ffn_moe_topk-<il>"`. `Some(layer)` for that shape; `None` for any other node
/// (the callback fires for EVERY node — this is the cheap filter). Pure + unit-tested; the
/// FFI callback is a thin wrapper over it.
fn parse_moe_topk_layer(name: &str) -> Option<u32> {
    let rest = name.strip_prefix("ffn_moe_topk")?;
    // Bare "ffn_moe_topk" (no per-layer suffix) can't be attributed to a layer → skip.
    rest.strip_prefix('-')?.parse::<u32>().ok()
}

/// Owns the caller's observer for the context's whole life; its raw pointer is the ggml
/// callback's `user_data`. Freed only AFTER `llama_free` (Context::drop's body runs first,
/// so no callback can fire after the context is gone) — see `Drop for Context`.
#[derive(Debug)]
struct ExpertObserverBridge {
    observer: std::sync::Arc<dyn ExpertObserver>,
}

/// The ggml eval callback. Called twice per graph node: `ask=true` BEFORE compute (return
/// true to be re-called after), `ask=false` AFTER compute (data resident). We read only on
/// `ask=false`. Always returns `true` to keep the graph running — observation never
/// perturbs inference (`prediction is a HINT, never a gate`).
unsafe extern "C" fn expert_eval_cb(
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
    bridge.observer.observe(layer, &buf);
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
    ptr: NonNull<sys::llama_context>,
    _model: PhantomData<&'m Model>,
    /// Keeps the expert-observer bridge alive for the callback's `user_data` pointer.
    /// Declared AFTER `ptr` so it drops after `Drop::drop`'s `llama_free` runs — no
    /// callback can fire against a freed bridge. `None` when no observer was set.
    _observer: Option<Box<ExpertObserverBridge>>,
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

    fn model_ptr(&self) -> *const sys::llama_model {
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

// ─── Batch ───────────────────────────────────────────────────────────────

/// A batch of tokens to feed into `Context::decode`.
///
/// Two construction modes:
///   * `for_tokens(&[...])` — single sequence, positions auto-assigned.
///     Cheapest path for generation. Uses `llama_batch_get_one`.
///   * `allocated(n_tokens, n_seq_max)` — preallocated for manual push,
///     supports multi-sequence. Uses `llama_batch_init` / `_free`.
pub struct Batch {
    inner: sys::llama_batch,
    storage: BatchStorage,
}

enum BatchStorage {
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

// ─── Sampler ─────────────────────────────────────────────────────────────

/// A sampler pipeline. Build with `Sampler::chain()` and chain methods, or
/// use `Sampler::greedy()` for deterministic argmax.
pub struct Sampler {
    ptr: NonNull<sys::llama_sampler>,
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
    chain: NonNull<sys::llama_sampler>,
}

impl SamplerChainBuilder {
    fn add(self, smpl: *mut sys::llama_sampler) -> Self {
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
    /// `last_n` = number of recent tokens to consider (0 disables, -1 = n_ctx).
    pub fn penalties(self, last_n: i32, repeat: f32, freq: f32, presence: f32) -> Self {
        let s = unsafe { sys::llama_sampler_init_penalties(last_n, repeat, freq, presence) };
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

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// what this catches: the `ffn_moe_topk-<il>` name parse — the cheap per-node filter
    /// that turns llama.cpp's graph-callback tensor name into a layer index. A regression
    /// (matching the wrong node, or mis-parsing the suffix) either drops the PGO signal
    /// silently or mis-attributes experts to the wrong layer. The FFI callback is a thin
    /// wrapper over this pure fn.
    #[test]
    fn parse_moe_topk_layer_extracts_block_index() {
        assert_eq!(parse_moe_topk_layer("ffn_moe_topk-0"), Some(0));
        assert_eq!(parse_moe_topk_layer("ffn_moe_topk-31"), Some(31));
        assert_eq!(parse_moe_topk_layer("ffn_moe_topk-895"), Some(895)); // K3 has ~896
        // bare name (no per-layer suffix) → can't attribute a layer → skip
        assert_eq!(parse_moe_topk_layer("ffn_moe_topk"), None);
        // wrong node → None (the callback fires for EVERY node; only topk carries selection)
        assert_eq!(parse_moe_topk_layer("ffn_moe_gate-3"), None);
        assert_eq!(parse_moe_topk_layer("attn_norm-2"), None);
        // malformed suffix → None, never a panic
        assert_eq!(parse_moe_topk_layer("ffn_moe_topk-"), None);
        assert_eq!(parse_moe_topk_layer("ffn_moe_topk-abc"), None);
    }

    /// what this catches: the `ExpertObserver` sink contract — the callback hands
    /// `(layer, selected expert ids)` straight through. A `Send + Sync + Debug` mock
    /// records them; this pins the trait shape the continuum-side residency tally consumes.
    #[test]
    fn expert_observer_receives_layer_and_selected_experts() {
        use std::sync::Mutex;
        #[derive(Debug, Default)]
        struct RecordingObserver {
            seen: Mutex<Vec<(u32, Vec<i32>)>>,
        }
        impl ExpertObserver for RecordingObserver {
            fn observe(&self, layer: u32, experts: &[i32]) {
                self.seen.lock().unwrap().push((layer, experts.to_vec()));
            }
        }
        let obs = RecordingObserver::default();
        // Two tokens, top-2 at layer 5 (experts [3,7] then [3,1], flattened), then layer 6.
        obs.observe(5, &[3, 7, 3, 1]);
        obs.observe(6, &[0, 2]);
        let seen = obs.seen.lock().unwrap();
        assert_eq!(seen.as_slice(), &[(5, vec![3, 7, 3, 1]), (6, vec![0, 2])]);
    }

    #[test]
    fn sampler_greedy_builds_and_drops() {
        let _s = Sampler::greedy();
    }

    #[test]
    fn sampler_chain_builds_and_drops() {
        let _s = Sampler::chain()
            .top_k(40)
            .top_p(0.9, 1)
            .temp(0.8)
            .dist(42)
            .build();
    }

    #[test]
    fn batch_for_tokens_roundtrip() {
        let b = Batch::for_tokens(vec![1, 2, 3, 4]);
        assert_eq!(b.n_tokens(), 4);
    }

    #[test]
    fn batch_allocated_push_clear() {
        let mut b = Batch::allocated(8, 1);
        assert_eq!(b.n_tokens(), 0);
        b.push(42, 0, &[0], true);
        b.push(43, 1, &[0], true);
        assert_eq!(b.n_tokens(), 2);
        b.clear();
        assert_eq!(b.n_tokens(), 0);
    }
}
