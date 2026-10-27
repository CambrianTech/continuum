//! Construction, builder overrides and lazy backend load for [`LlamaCppAdapter`].

use super::*;

/// What a local llama.cpp adapter launches with: the model, its GGUF on disk, and the knobs
/// its caller already resolved. An engine value, so the adapter never names the persona layer
/// whose profile produces it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlamaCppLaunch {
    pub model_id: String,
    pub gguf_path: PathBuf,
    /// Per-sequence context the KV cache is bounded to.
    pub context_length: u32,
    pub n_seq_max: u32,
    pub n_ubatch: u32,
    /// -1 = every layer on the GPU, 0 = CPU only, N = the bottom N layers.
    pub n_gpu_layers: i32,
}

impl LlamaCppAdapter {
    /// Construct from the model_registry. Looks up the first model under
    /// provider `llamacpp-local` whose GGUF artifact resolved locally
    /// and uses its id + path. If the registry has no such row, panics
    /// — that's a config bug, not a runtime failure mode (per the
    /// no-fallback rule).
    ///
    /// Prefer [`Self::try_new`] when calling from a path that should
    /// surface the missing-Qwen state as observable runtime health
    /// rather than crashing the process. Boot-time health checks
    /// (continuum status, ai/status, install-time validators) MUST use
    /// `try_new` so an install with no Qwen seeded reports
    /// `NoLocalModelLoadable` cleanly instead of crash-looping.
    pub fn new() -> Self {
        Self::try_new().unwrap_or_else(|err| panic!("{err}"))
    }

    /// Result-returning variant of [`Self::new`]. Returns
    /// [`NoLocalModelLoadable`] when the registry has no `llamacpp-local`
    /// row with a resolved `gguf_local_path` — the typed failure mode
    /// for "install seeded no local Qwen GGUF" which surfaces at
    /// install-time on hosts where the model-init container did not
    /// download a chat-capable model (RTX 5090 finding, 2026-05-11). The
    /// caller decides whether to crash (legacy `new()` behavior),
    /// degrade, or report the error to operators.
    pub fn try_new() -> Result<Self, NoLocalModelLoadable> {
        let reg = crate::model_registry::global();
        Self::try_new_from(reg.models_for_provider(LLAMACPP_PROVIDER_ID))
    }

    /// Pure variant of [`Self::try_new`] taking a model iterator
    /// directly — lets tests assemble synthetic registries without going
    /// through the global singleton. Production code uses
    /// [`Self::try_new`] which calls this with `global().models_for_provider(...)`.
    pub fn try_new_from<'a, I>(models: I) -> Result<Self, NoLocalModelLoadable>
    where
        I: IntoIterator<Item = &'a crate::model_registry::Model>,
    {
        let candidates: Vec<&crate::model_registry::Model> = models.into_iter().collect();
        let with_path: Vec<&crate::model_registry::Model> = candidates
            .iter()
            .copied()
            .filter(|m| m.gguf_local_path.is_some())
            .collect();
        let model = with_path.first().ok_or_else(|| NoLocalModelLoadable {
            provider_id: LLAMACPP_PROVIDER_ID.to_string(),
            rows_in_registry: candidates.len(),
            rows_with_gguf_local_path: 0,
        })?;
        let model_path = model
            .gguf_local_path
            .clone()
            .expect("gguf_local_path present — filtered above");
        Ok(Self {
            backend: Arc::new(RwLock::new(None)),
            model_path,
            last_throughput_tok_s: Arc::new(RwLock::new(0.0)),
            default_model: model.id.clone(),
            context_length_override: None,
            kv_quant_policy: crate::inference::kv_quant::KvQuantPolicy::default(),
            n_seq_max_override: None,
            n_ubatch_override: None,
            n_gpu_layers_override: None,
        })
    }

    /// Override the model path. Useful for tests + when the model isn't
    /// at the registry's declared location.
    pub fn with_model_path(mut self, path: PathBuf) -> Self {
        self.model_path = path;
        self
    }

    /// Construct an adapter bound to a SPECIFIC `(model_path, model_id)`
    /// pair. `new()` picks "first llamacpp-local with a gguf path" which
    /// is fine for the default text model but a registry that holds
    /// multiple llamacpp-local entries (text + vision) needs a way to
    /// say which one this adapter instance serves.
    ///
    /// The `model_id` MUST match a row in the Rust catalog (catalog.rs) so the
    /// adapter can look up that model's chat_template, mmproj_path,
    /// stop_sequences, and capabilities. A mismatch produces silently
    /// wrong output (wrong chat template → garbled response).
    pub fn with_model_id(model_path: PathBuf, model_id: String) -> Self {
        Self {
            backend: Arc::new(RwLock::new(None)),
            model_path,
            last_throughput_tok_s: Arc::new(RwLock::new(0.0)),
            default_model: model_id,
            context_length_override: None,
            kv_quant_policy: crate::inference::kv_quant::KvQuantPolicy::default(),
            n_seq_max_override: None,
            n_ubatch_override: None,
            n_gpu_layers_override: None,
        }
    }

    /// **The intent-driven constructor** per
    /// [[intent-driven-api-not-hot-patches]] (Joel, 2026-06-01).
    /// Replaces the chain of `with_model_id().with_context_length()
    /// .with_n_seq_max()...` with one call that takes the launch its caller
    /// already resolved and derives every knob from it.
    ///
    /// A persona's launch comes from her inference profile
    /// (`PersonaInferenceProfile::llamacpp_launch`, #121), which refuses a
    /// profile with no local GGUF per [[no-fallbacks-ever]] before it reaches here.
    /// Callers never touch n_ubatch, n_seq_max, n_gpu_layers, etc. directly.
    pub fn for_launch(launch: &LlamaCppLaunch) -> Self {
        Self {
            backend: Arc::new(RwLock::new(None)),
            model_path: launch.gguf_path.clone(),
            last_throughput_tok_s: Arc::new(RwLock::new(0.0)),
            default_model: launch.model_id.clone(),
            context_length_override: Some(launch.context_length),
            kv_quant_policy: crate::inference::kv_quant::KvQuantPolicy::default(),
            n_seq_max_override: Some(launch.n_seq_max),
            n_ubatch_override: Some(launch.n_ubatch),
            n_gpu_layers_override: Some(launch.n_gpu_layers),
        }
    }

    /// Override the per-sequence context budget. Pass smaller-than-trained
    /// to bound the KV cache allocation (qwen3.5-4b @ 262K = 24GB; @ 16K
    /// = 500MB). Tests should always set this to keep the suite cheap and
    /// avoid leaving 24GB processes lingering when llama.cpp's Metal
    /// cleanup SIGABRTs prevent clean exit (see PR #17869).
    pub fn with_context_length(mut self, n: u32) -> Self {
        self.context_length_override = Some(n);
        self
    }

    /// Override the per-residency KV quant policy. Default is
    /// `KvQuantPolicy::default()` — F16/F16 active for max decode speed,
    /// Q8_0/F16 cpu-resident for compression with quality, Q8_0/Q8_0
    /// spilled for minimum file size. Override per recipe / hardware
    /// tier. See docs/architecture/PERSONA-CONTEXT-PAGING.md §16.
    pub fn with_kv_quant_policy(
        mut self,
        policy: crate::inference::kv_quant::KvQuantPolicy,
    ) -> Self {
        self.kv_quant_policy = policy;
        self
    }

    /// Enable multi-seq continuous batching at the in-backend
    /// scheduler. Sets `LlamaCppConfig::n_seq_max = n`, which sizes
    /// the shared `Context`'s seq pool. Coordinator wiring at
    /// `InferenceCoordinator` time SHOULD set this to match (or
    /// modestly exceed) the lane budget's `max_concurrency` so the
    /// scheduler can actually serve every admitted lane in parallel.
    ///
    /// **WARNING (model-specific):** qwen3.5 (and any model with a
    /// Gated-Delta-Net or recurrent KV layer that llama.cpp's Metal
    /// graph can't multiplex) MUST keep n_seq_max=1. Standard Llama,
    /// Qwen-2.5, Gemma-2, and similar transformer architectures
    /// multiplex cleanly. Caller verifies model compatibility — the
    /// adapter doesn't auto-detect today. (Q21 follow-up: probe the
    /// model architecture at load time and refuse n_seq_max>1 for
    /// known-incompatible families.)
    pub fn with_n_seq_max(mut self, n: u32) -> Self {
        self.n_seq_max_override = Some(n.max(1));
        self
    }

    /// Current n_seq_max setting (None = single-seq default).
    /// Coordinators use this to size their admission budgets — if
    /// the adapter reports None, max_concurrency is effectively 1
    /// regardless of what the lane budget says.
    pub fn n_seq_max(&self) -> Option<u32> {
        self.n_seq_max_override
    }

    /// Size the backend's KV by a recipe's persona budgets. The adapter
    /// computes `sum(persona seeds)` bounded by the model's
    /// `n_ctx_train` ceiling, then sets `context_length` accordingly.
    /// Replaces the bandaid `with_context_length(magic_number)` calls
    /// in test rigs and recipe loaders — declare WHO is in the recipe
    /// and what they're DOING, the adapter computes the budget.
    ///
    /// See docs/architecture/PERSONA-CONTEXT-PAGING.md §14 for the
    /// task-default seed table this consumes.
    pub fn with_recipe_budget(
        mut self,
        budget: &crate::inference::recipe_budget::RecipeBudget,
    ) -> Self {
        let seed_sum = budget.sum_of_seed_tokens();
        // Floor of 1024 — even an empty recipe needs SOME context for
        // ad-hoc inference. The budget is a sizing hint; the policy
        // grows it later from observed demand. Above the floor,
        // honor the recipe sum.
        let computed = seed_sum.max(1024);
        self.context_length_override = Some(computed);
        self
    }

    /// Lazy-load the backend on first use. Cheap if already loaded.
    pub(super) fn ensure_loaded(&self) -> Result<Arc<LlamaCppBackend>, String> {
        // Fast path — already loaded.
        if let Some(b) = self.backend.read().as_ref() {
            return Ok(b.clone());
        }

        // Slow path — load. Take write lock; another thread may have raced
        // here, so check again before constructing.
        let mut guard = self.backend.write();
        if let Some(b) = guard.as_ref() {
            return Ok(b.clone());
        }

        if !self.model_path.exists() {
            return Err(format!(
                "model GGUF not found at {:?} for model `{}` — \
                 either pull the artifact identified by the registry \
                 `gguf_hint` or \
                 override via with_model_path()",
                self.model_path, self.default_model,
            ));
        }

        enforce_residency(&self.model_path).map_err(|block| {
            format!(
                "refusing to load local llama.cpp model `{}` because residency gate failed: {block}",
                self.default_model
            )
        })?;

        // KV quant for the Active tier (the tier the backend is loaded
        // into). CpuResident and Idle quants apply later when the paging
        // substrate transitions sequences out of Active. Single source of
        // truth: the policy on this adapter, declared by the caller.
        let active_kv = self
            .kv_quant_policy
            .for_residency(crate::inference::kv_quant::Residency::Active);
        // Resolve the multimodal projector via the ONE registry resolver — the
        // single source of truth for "where is this model's mmproj" (declared
        // local path first, else the projector sitting beside the GGUF in the
        // HF cache snapshot, existence-checked). Reading the raw
        // `mmproj_local_path` field here would miss the self-provisioned sibling
        // and skip the existence check; `resolve_mmproj_for_model` is what
        // `llama-server` uses too, so both serving paths agree. When set, the
        // backend's generate_with_image route lazily loads the MtmdContext from
        // it; when None, generate_with_image returns a clear error rather than
        // silently bridging to text — a config issue, not a degraded experience.
        let mmproj_path = crate::model_registry::try_global().and_then(|reg| {
            reg.model(&self.default_model)
                .and_then(crate::model_registry::artifacts::resolve_mmproj_for_model)
        });
        // CONTINUUM_TIER is set by install.sh's hardware probe (commit
        // 7b3b8e086) — when the install detects a Mac Intel + discrete
        // AMD or integrated Intel UHD host, it exports
        // CONTINUUM_TIER=mac_intel_discrete because llama.cpp's
        // Metal-AMD shaders produce garbled tokens at 0.8 tok/s with
        // hundreds of nil tensor buffer errors (continuum 2026-05-30
        // evidence on MacBookPro15,1 / Radeon Pro 560X). CPU-only at
        // 1.1 tok/s + coherent output beats broken Metal every time
        // — n_gpu_layers=0 forces the CPU path. Follow-up: native
        // Rust probe at adapter construction so this doesn't depend
        // on the install-time env-var trust chain (see task tracker).
        // Profile-driven override wins per [[intent-driven-api-not-hot-
        // patches]] — the substrate already resolved the right value from
        // the persona's tier_descriptor. Env var stays as the legacy
        // operator escape hatch for ad-hoc ad-hoc construction (tests,
        // smoke binaries that don't carry a profile yet).
        let n_gpu_layers: i32 = self
            .n_gpu_layers_override
            .unwrap_or_else(|| match std::env::var("CONTINUUM_TIER").as_deref() {
                Ok("mac_intel_discrete") => 0,
                _ => -1,
            });
        // Defense-in-depth (task #110): the realistic-lane work lifted
        // n_seq_max to a caller-controlled knob, but the substrate
        // MUST NOT enable multi-seq batching on architectures that
        // llama.cpp's batched decode aborts on (qwen3 Gated-Delta-Net,
        // mamba / rwkv / jamba / griffin / recurrentgemma /
        // falcon_mamba). The probe reads the GGUF's general.architecture
        // and classifies. Unsafe architectures clamp n_seq_max → 1
        // regardless of what the caller configured. A `tracing::warn!`
        // surfaces the clamp so operators see the safety net firing
        // instead of silent quality loss.
        let requested_n_seq_max = self.n_seq_max_override.unwrap_or(1);
        let effective_n_seq_max = if requested_n_seq_max > 1 {
            match crate::inference::batching_probe::probe_gguf_batching_safety(&self.model_path) {
                Ok(verdict) => {
                    let clamped = verdict.clamp_n_seq_max(requested_n_seq_max);
                    if clamped < requested_n_seq_max {
                        tracing::warn!(
                            target: "continuum_core::inference::llamacpp_adapter",
                            arch = %verdict.arch(),
                            requested = requested_n_seq_max,
                            effective = clamped,
                            "batching_probe: clamped n_seq_max — architecture is not safe for multi-seq batching; \
                             continuous batching disabled for this adapter. Coordinator lanes \
                             will queue at the in-backend scheduler instead of running in parallel."
                        );
                    }
                    clamped
                }
                Err(err) => {
                    // Probe failure shouldn't block adapter load — but
                    // we conservatively clamp to 1 since we can't
                    // verify safety. Logged so operators chase the
                    // root cause (malformed GGUF metadata).
                    tracing::warn!(
                        target: "continuum_core::inference::llamacpp_adapter",
                        error = %err,
                        requested = requested_n_seq_max,
                        "batching_probe failed — conservatively clamping n_seq_max to 1"
                    );
                    1
                }
            }
        } else {
            requested_n_seq_max
        };

        let config = LlamaCppConfig {
            model_path: self.model_path.clone(),
            mmproj_path,
            n_gpu_layers,
            // None = honor model's n_ctx_train. Adapter caller can shrink
            // this via with_context_length() to bound the KV cache (24GB
            // at 262K → 500MB at 16K).
            context_length: self.context_length_override,
            // n_seq_max comes from the adapter's override clamped by
            // the model-arch probe above. Standard transformers
            // (Llama, Qwen-2.5, Gemma-2, ...) pass through; recurrent
            // / state-space / hybrid families (qwen3, mamba, rwkv,
            // jamba, ...) clamp to 1. See task #110.
            n_seq_max: effective_n_seq_max,
            // Profile-driven override wins per [[intent-driven-api-not-
            // hot-patches]]. Fallback to 512 (not the old 128 default)
            // because compute-graph reservation matches n_ubatch and a
            // RAG-built persona prompt arrives at 200-500 tokens — at
            // n_ubatch=128 the scheduler panicked with "decode: failed
            // to find a memory slot for batch of size 337" during #130
            // 2026-06-01 multi-persona LCD chat. 512 covers realistic
            // persona prompts without ballooning memory (graph nodes
            // scale with n_ubatch but at ~4 KiB per node × 942 nodes ×
            // 4 multiplier we're talking ~15 MiB per scheduler — trivial).
            // Future: derive from the Rust catalog (catalog.rs) row per [[orm-everything-
            // not-hand-edited-files]] so each model declares its own
            // realistic batch ceiling.
            n_ubatch: self.n_ubatch_override.unwrap_or(512),
            flash_attn: FlashAttn::Disabled,
            fused_gdn_ar: false,
            fused_gdn_ch: false,
            type_k: active_kv.k,
            type_v: active_kv.v,
            ..Default::default()
        };
        let backend = LlamaCppBackend::load(config)
            .map_err(|e| format!("LlamaCppBackend::load failed: {e}"))?;

        // Report model_weights bytes to the global FootprintRegistry so
        // the policy can see the on-disk size charged against this process
        // (mmap'd, so file size ≈ resident bytes for the model itself).
        // Backend-scoped key: two adapters loading two different GGUFs
        // produce two distinct entries instead of overwriting each other.
        // The size source is fs::metadata, not a backend method, because
        // llama.cpp doesn't expose a "bytes loaded" counter and the file
        // size is the most honest first-cut number.
        if let Ok(meta) = std::fs::metadata(&self.model_path) {
            use crate::inference::footprint_registry::{global, FootprintKey, ResourceType};
            use crate::inference::kv_quant::Residency;
            global().report_authoritative(
                FootprintKey::for_backend(
                    backend.model_id(),
                    ResourceType::ModelWeights,
                    Residency::Active,
                ),
                meta.len(),
            );
        }

        let arc = Arc::new(backend);
        *guard = Some(arc.clone());
        Ok(arc)
    }

}

impl Default for LlamaCppAdapter {
    fn default() -> Self {
        Self::new()
    }
}
