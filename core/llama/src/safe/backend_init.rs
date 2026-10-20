use super::*;

pub(super) static BACKEND_INIT: Once = Once::new();

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
pub(super) unsafe fn ensure_backend_registered(
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
pub(super) unsafe fn assert_gpu_backend_registered_when_expected() {
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
    pub(super) ptr: NonNull<sys::llama_model>,
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
        // Upstream 2026-07 replaced `use_mmap: bool` with the load_mode enum
        // (NONE/MMAP/MLOCK/DIRECT_IO — DIRECT_IO is the future NVMe cold-expert read path).
        ffi_params.load_mode = if params.use_mmap {
            sys::llama_load_mode_LLAMA_LOAD_MODE_MMAP
        } else {
            sys::llama_load_mode_LLAMA_LOAD_MODE_NONE
        };

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

    /// Per-layer KV head count. `0` marks a recurrent (SSM/KDA) layer that
    /// holds NO per-token KV cache. Hybrid models (kimi-linear, kimi-k3,
    /// jamba) vary this per layer — sizing KV from the scalar [`n_head_kv`]
    /// times [`n_layer`] overestimates them by orders of magnitude (kimi-k3:
    /// 69 of 93 layers carry zero KV; the 24 MLA layers cache ONE compressed
    /// head). Fork export `llama_model_n_head_kv_il` (continuum #238).
    pub fn n_head_kv_il(&self, il: u32) -> u32 {
        let n = unsafe { sys::llama_model_n_head_kv_il(self.ptr.as_ptr(), il as i32) };
        n.max(0) as u32
    }

    /// Per-layer K-head width in elements — for MLA models this is the
    /// COMPRESSED cache width (kimi-k3: 576 = kv_lora 512 + rope 64), which is
    /// why deriving it as `n_embd / n_head` misprices them.
    pub fn n_embd_head_k_il(&self, il: u32) -> u32 {
        let n = unsafe { sys::llama_model_n_embd_head_k_il(self.ptr.as_ptr(), il as i32) };
        n.max(0) as u32
    }

    /// Per-layer V-head width in elements (see [`n_embd_head_k_il`]).
    pub fn n_embd_head_v_il(&self, il: u32) -> u32 {
        let n = unsafe { sys::llama_model_n_embd_head_v_il(self.ptr.as_ptr(), il as i32) };
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

    /// K3 slice-2 (Model A — weight WRITE): replace expert `expert_index`'s weights in the
    /// three resident stacked MoE projection tensors (`blk.{layer}.ffn_{gate,up,down}_exps`)
    /// with `gate`/`up`/`down` bytes. This is weight-REPLACEMENT into a loaded model — the
    /// per-expert genome path (specialize / compact an expert in place) — NOT paging: the
    /// full tensor must be resident (all `n_expert` slots). True per-expert paging (a K-slot
    /// tensor + a router logical→physical remap) is the separate Model-B fork.
    ///
    /// The byte offset for each projection is computed from the LIVE tensor's own `nb[2]`
    /// (ggml's quant-aware stride of the expert dim — the outermost of `{n_embd, n_ff,
    /// n_expert}`), so it is correct for any quantization with zero parsed-stride guessing.
    /// Each slice must be exactly one expert's stride (`nb[2]`).
    ///
    /// Fail-loud (never a silent partial write): a projection tensor is absent (not an MoE
    /// layer / wrong index), `expert_index` is out of range, or a byte length ≠ the live
    /// per-expert stride. Behavioral validation is Gate B (needs a loaded MoE) — this is the
    /// compile-validated mechanism; nothing calls it until the genome path (#226/#228) does.
    pub fn upload_expert(
        &self,
        layer: u32,
        expert_index: u32,
        gate: &[u8],
        up: &[u8],
        down: &[u8],
    ) -> Result<(), String> {
        for (proj, bytes) in [("ffn_gate_exps", gate), ("ffn_up_exps", up), ("ffn_down_exps", down)] {
            let name = std::ffi::CString::new(format!("blk.{layer}.{proj}"))
                .map_err(|e| format!("upload_expert: bad tensor name: {e}"))?;
            // SAFETY: `as_ptr` is a live model for its lifetime; the fork's accessor returns a
            // tensor from `tensors_by_name` or null (never a dangling pointer).
            let tensor = unsafe { sys::llama_model_get_tensor(self.as_ptr(), name.as_ptr()) };
            if tensor.is_null() {
                return Err(format!(
                    "upload_expert: no tensor blk.{layer}.{proj} (not an MoE layer, or layer out of range)"
                ));
            }
            // SAFETY: `tensor` is non-null and valid for the model's lifetime; reading its
            // dims/strides is a plain field read.
            let (n_expert, stride) = unsafe { ((*tensor).ne[2], (*tensor).nb[2]) };
            if (expert_index as i64) >= n_expert {
                return Err(format!(
                    "upload_expert: expert {expert_index} out of range (blk.{layer}.{proj} has {n_expert})"
                ));
            }
            if bytes.len() != stride {
                return Err(format!(
                    "upload_expert: {} bytes != live per-expert stride {stride} for blk.{layer}.{proj}",
                    bytes.len()
                ));
            }
            let offset = expert_index as usize * stride;
            // SAFETY: offset + len == (expert_index+1)*stride ≤ n_expert*stride (the tensor's
            // expert-dim extent, checked above), so the write stays in-bounds. The backend
            // copies host→device (GPU) or host→host (CPU) transparently.
            unsafe {
                sys::ggml_backend_tensor_set(
                    tensor,
                    bytes.as_ptr() as *const std::os::raw::c_void,
                    offset,
                    bytes.len(),
                );
            }
        }
        Ok(())
    }
}

impl Drop for Model {
    fn drop(&mut self) {
        unsafe {
            sys::llama_model_free(self.ptr.as_ptr());
        }
    }
}

/// A LoRA adapter loaded against a model. Can be hot-swapped on a context
/// without rebuilding the context — this is the primitive genome paging uses.
///
/// No borrow on `Model` (see `Model::load_lora` docs for the invariant).
pub struct LoraAdapter {
    pub(super) ptr: NonNull<sys::llama_adapter_lora>,
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

/// Sink for live MoE expert selections. `observe` is called from INSIDE the compute
/// callback for every `ffn_moe_topk` node, so it MUST be cheap + non-blocking (tally into
/// an atomic map — never lock, never do I/O). `Send + Sync` because the callback can run
/// on a backend thread; `Debug` so `ContextParams` keeps its derive.
pub trait ExpertObserver: Send + Sync + std::fmt::Debug {
    /// `layer` = transformer block index; `experts` = the selected expert indices for the
    /// tokens in this decode batch (flattened row-major `[n_expert_used, n_tokens]`);
    /// `n_expert_used` = the row width, so a consumer can unpack `experts` into per-token
    /// rows (`experts[t*n_expert_used .. (t+1)*n_expert_used]`) for cross-layer trajectory
    /// capture. `hits`-only consumers ignore it and tally the flat slice.
    fn observe(&self, layer: u32, experts: &[i32], n_expert_used: usize);
}

/// Parse the block index from a `ffn_moe_topk` node name. llama.cpp's graph callback
/// formats per-layer tensors as `"<name>-<il>"` (`ggml_format_name`), so the eval callback
/// sees `"ffn_moe_topk-<il>"`. `Some(layer)` for that shape; `None` for any other node
/// (the callback fires for EVERY node — this is the cheap filter). Pure + unit-tested; the
/// FFI callback is a thin wrapper over it.
pub(super) fn parse_moe_topk_layer(name: &str) -> Option<u32> {
    let rest = name.strip_prefix("ffn_moe_topk")?;
    // Bare "ffn_moe_topk" (no per-layer suffix) can't be attributed to a layer → skip.
    rest.strip_prefix('-')?.parse::<u32>().ok()
}

/// Owns the caller's observer for the context's whole life; its raw pointer is the ggml
/// callback's `user_data`. Freed only AFTER `llama_free` (Context::drop's body runs first,
/// so no callback can fire after the context is gone) — see `Drop for Context`.
#[derive(Debug)]
pub(super) struct ExpertObserverBridge {
    pub(super) observer: std::sync::Arc<dyn ExpertObserver>,
}
