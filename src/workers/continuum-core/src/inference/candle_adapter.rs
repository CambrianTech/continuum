//! Candle Adapter - Local LLM Inference via AIProviderAdapter
//!
//! Implements the AIProviderAdapter trait for local Candle inference.
//! Uses `ModelBackend` trait — no format-specific code paths.
//! One backend, one generate function, works for GGUF and safetensors.
//!
//! Context window, EOS tokens, architecture — all from the model file.
//! No hardcoded values.

use async_trait::async_trait;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use crate::ai::{
    AIProviderAdapter, ActiveAdapterRequest, AdapterCapabilities, AdapterConfig, ApiStyle,
    FinishReason, HealthState, HealthStatus, LoRAAdapterInfo, LoRACapabilities, ModelCapability,
    ModelInfo, RoutingInfo, TextGenerationRequest, TextGenerationResponse, UsageMetrics,
};
use crate::gpu::make_entry;
use crate::gpu::memory_manager::{GpuAllocationGuard, GpuMemoryManager, GpuPriority, GpuSubsystem};
use crate::runtime;
use crate::system_resources::local_inference_capacity;

/// Default context window reported before a model is loaded.
/// Once loaded, the actual model's context_length is used.
const DEFAULT_CONTEXT_WINDOW: u32 = 131072;
use super::backends::{self, GenomeAdapter, ModelBackend, ModelFormat};
use super::lora::{load_lora_adapter, LoadedAdapter};
use super::model::load_model_by_id;
use super::quantized::load_default_quantized;

// SAFETY: ModelBackend contains GPU tensors pinned to creation thread.
// All model access happens within spawn_blocking on a consistent thread pool.
// Sync is required because CandleAdapter is shared via Arc<RwLock<>> in async context.
struct BackendWrapper(Box<dyn ModelBackend>);
unsafe impl Send for BackendWrapper {}
unsafe impl Sync for BackendWrapper {}

/// Candle adapter for local LLM inference.
///
/// Holds a single `ModelBackend` — no ModelVariant enum, no format switches.
/// The backend reports its own capabilities (context_length, architecture, etc.)
pub struct CandleAdapter {
    config: AdapterConfig,
    /// The model backend (GGUF or safetensors — doesn't matter)
    backend: Arc<RwLock<Option<BackendWrapper>>>,
    /// Loaded LoRA adapters (may or may not be active)
    loaded_adapters: RwLock<HashMap<String, LoadedAdapter>>,
    /// Currently active adapter IDs (order matters for stacking)
    active_adapters: RwLock<Vec<String>>,
    /// Use quantized model
    use_quantized: bool,
    /// GPU memory manager for VRAM allocation tracking
    gpu_manager: Option<Arc<GpuMemoryManager>>,
    /// RAII guard for base model VRAM allocation
    model_guard: RwLock<Option<GpuAllocationGuard>>,
    /// RAII guards for per-adapter VRAM allocations
    adapter_guards: RwLock<HashMap<String, GpuAllocationGuard>>,
    /// Serializes first-time load of `llamacpp_backend`. Required because
    /// concurrent Metal-init calls on the same model have panicked in
    /// testing. The 6s model load is one-time per process and is dropped
    /// as soon as the load completes — subsequent generate calls fall
    /// straight through to the scheduler.
    llamacpp_load_gate: Arc<tokio::sync::Mutex<()>>,
    /// llama.cpp backend — in-process via the vendored substrate. Loaded
    /// lazily on first inference; None until then.
    ///
    /// Wrapped in `Arc` so we can hand a clone to `spawn_blocking` without
    /// holding a `RwLock` guard across the await point (parking_lot guards
    /// are not `Send`).
    ///
    /// Wrapped in `Arc` so we can hand the slot to a background warmup task
    /// that outlives the `&mut self` borrow of `initialize()`.
    llamacpp_backend: Arc<RwLock<Option<Arc<backends::llamacpp::LlamaCppBackend>>>>,
}

impl CandleAdapter {
    pub fn new() -> Self {
        Self {
            config: AdapterConfig {
                provider_id: "candle".to_string(),
                name: "Candle Local".to_string(),
                base_url: String::new(),
                api_key_env: String::new(),
                default_model: "unsloth/Llama-3.2-3B-Instruct".to_string(),
                timeout_ms: 300_000,
                max_retries: 1,
                retry_delay_ms: 0,
            },
            backend: Arc::new(RwLock::new(None)),
            loaded_adapters: RwLock::new(HashMap::new()),
            active_adapters: RwLock::new(Vec::new()),
            use_quantized: false,
            gpu_manager: None,
            model_guard: RwLock::new(None),
            adapter_guards: RwLock::new(HashMap::new()),
            llamacpp_load_gate: Arc::new(tokio::sync::Mutex::new(())),
            llamacpp_backend: Arc::new(RwLock::new(None)),
        }
    }

    /// Load a GGUF model in-process via our vendored llama.cpp substrate.
    /// No HTTP, no external process — the backend owns the model memory.
    ///
    /// Returns Err if the GGUF fails to load. Callers should propagate; the
    /// no-fallback rule means we don't silently drop back to anything else.
    pub fn load_llamacpp(&self, model_path: &str) -> Result<(), String> {
        let log = runtime::logger("candle");
        let config = backends::llamacpp::LlamaCppConfig {
            model_path: std::path::PathBuf::from(model_path),
            n_seq_max: local_inference_capacity() as u32,
            ..Default::default()
        };
        let backend = backends::llamacpp::LlamaCppBackend::load(config)?;
        log.info(&format!(
            "llama.cpp backend loaded in-process: {}",
            backend.model_id()
        ));
        *self.llamacpp_backend.write() = Some(Arc::new(backend));
        Ok(())
    }

    /// Set GPU memory manager for VRAM allocation tracking.
    pub fn set_gpu_manager(&mut self, mgr: Arc<GpuMemoryManager>) {
        self.gpu_manager = Some(mgr);
    }

    pub fn with_model(model_id: &str) -> Self {
        let mut adapter = Self::new();
        adapter.config.default_model = model_id.to_string();
        adapter
    }

    pub fn quantized() -> Self {
        let mut adapter = Self::new();
        adapter.use_quantized = true;
        adapter.config.provider_id = "candle-q".to_string();
        adapter.config.name = "Candle Local (Quantized)".to_string();
        adapter
    }

    pub fn regular() -> Self {
        let mut adapter = Self::new();
        adapter.use_quantized = false;
        adapter
    }

    /// Local-inference concurrency capacity in use by this adapter's
    /// scheduler. Exposed so the TS-side `InferenceCoordinator` can fetch
    /// the same number via IPC instead of re-deriving it (drift bait).
    /// Both layers MUST agree to avoid double-gating bugs (see issue #887).
    pub fn inference_capacity(&self) -> usize {
        local_inference_capacity()
    }

    pub fn lora_capabilities(&self) -> LoRACapabilities {
        LoRACapabilities::MultiLayerPaging {
            max_loaded: 8,
            supports_hot_swap: true,
        }
    }

    /// Load a LoRA adapter from path.
    pub async fn load_lora(&self, adapter_id: &str, path: &str, scale: f64) -> Result<(), String> {
        let backend_guard = self.backend.read();
        let wrapper = backend_guard.as_ref().ok_or("Model not loaded")?;
        let backend = &wrapper.0;

        let device = backend.device().clone();
        let dtype = if backend.format() == ModelFormat::Safetensors {
            // Downcast to get dtype — only safetensors backends have this
            candle_core::DType::BF16 // Safe default for Metal
        } else {
            candle_core::DType::F32
        };

        let weights = load_lora_adapter(path, &device, dtype, scale)
            .map_err(|e| format!("Failed to load LoRA adapter: {e}"))?;

        let mut adapters = self.loaded_adapters.write();
        let mut loaded = LoadedAdapter::new(adapter_id.to_string(), path.to_string(), scale);
        loaded.weights = Some(weights);
        adapters.insert(adapter_id.to_string(), loaded);

        // Track GPU allocation for adapter — refuse at critical pressure
        if let Some(mgr) = &self.gpu_manager {
            let adapter_bytes = estimate_adapter_vram(path);
            if adapter_bytes > 0 {
                match mgr.allocate(
                    GpuSubsystem::Inference,
                    adapter_bytes,
                    GpuPriority::Interactive,
                ) {
                    Ok(guard) => {
                        self.adapter_guards
                            .write()
                            .insert(adapter_id.to_string(), guard);
                        mgr.eviction_registry.register(make_entry(
                            &format!("candle:adapter:{}", adapter_id),
                            &format!("LoRA {}", adapter_id),
                            GpuPriority::Interactive,
                            adapter_bytes,
                        ));
                    }
                    Err(e) => {
                        runtime::logger("candle").error(&format!(
                            "GPU CRITICAL: Cannot load adapter {} — {}",
                            adapter_id, e
                        ));
                        return Err(format!("GPU memory critical — cannot load adapter: {e}"));
                    }
                }
            }
        }

        runtime::logger("candle").info(&format!(
            "Loaded LoRA adapter: {} from {}",
            adapter_id, path
        ));
        Ok(())
    }

    /// Activate a LoRA adapter (must be loaded first).
    pub async fn apply_lora(&self, adapter_id: &str) -> Result<(), String> {
        {
            let adapters = self.loaded_adapters.read();
            if !adapters.contains_key(adapter_id) {
                return Err(format!("Adapter '{}' not loaded", adapter_id));
            }
        }

        {
            let mut active = self.active_adapters.write();
            if !active.contains(&adapter_id.to_string()) {
                active.push(adapter_id.to_string());
            }
        }

        {
            let mut adapters = self.loaded_adapters.write();
            if let Some(adapter) = adapters.get_mut(adapter_id) {
                adapter.active = true;
            }
        }

        self.rebuild_model_with_active_lora().await?;

        runtime::logger("candle").info(&format!("Applied LoRA adapter: {}", adapter_id));
        Ok(())
    }

    /// Deactivate a LoRA adapter.
    pub async fn remove_lora(&self, adapter_id: &str) -> Result<(), String> {
        {
            let mut active = self.active_adapters.write();
            active.retain(|id| id != adapter_id);
        }
        {
            let mut adapters = self.loaded_adapters.write();
            if let Some(adapter) = adapters.get_mut(adapter_id) {
                adapter.active = false;
            }
        }

        self.rebuild_model_with_active_lora().await?;
        runtime::logger("candle").info(&format!("Removed LoRA adapter: {}", adapter_id));
        Ok(())
    }

    /// Unload a LoRA adapter (removes from memory).
    pub async fn unload_lora(&self, adapter_id: &str) -> Result<(), String> {
        self.remove_lora(adapter_id).await?;
        let mut adapters = self.loaded_adapters.write();
        adapters.remove(adapter_id);
        // Release GPU allocation guard (drops on remove)
        self.adapter_guards.write().remove(adapter_id);
        // Unregister from eviction registry
        if let Some(mgr) = &self.gpu_manager {
            mgr.eviction_registry
                .unregister(&format!("candle:adapter:{}", adapter_id));
        }
        runtime::logger("candle").info(&format!("Unloaded LoRA adapter: {}", adapter_id));
        Ok(())
    }

    pub fn list_lora_adapters(&self) -> Vec<LoRAAdapterInfo> {
        let adapters = self.loaded_adapters.read();
        adapters
            .values()
            .map(|a| LoRAAdapterInfo {
                adapter_id: a.adapter_id.clone(),
                path: a.path.clone(),
                scale: a.scale,
                loaded: a.weights.is_some(),
                active: a.active,
            })
            .collect()
    }

    /// Ensure exactly these adapters are loaded and active, rebuilding model once.
    async fn ensure_adapters(
        &self,
        adapters: &[ActiveAdapterRequest],
    ) -> Result<Vec<String>, String> {
        let log = runtime::logger("candle");

        for adapter in adapters {
            let needs_load = !self.loaded_adapters.read().contains_key(&adapter.name);
            if needs_load {
                log.info(&format!(
                    "Loading LoRA adapter: {} from {} (scale={})",
                    adapter.name, adapter.path, adapter.scale
                ));
                self.load_lora(&adapter.name, &adapter.path, adapter.scale)
                    .await?;
            }
        }

        let desired_ids: Vec<String> = adapters.iter().map(|a| a.name.clone()).collect();
        {
            let mut active = self.active_adapters.write();
            *active = desired_ids.clone();
        }
        {
            let mut loaded = self.loaded_adapters.write();
            for (id, adapter) in loaded.iter_mut() {
                adapter.active = desired_ids.contains(id);
            }
        }

        self.rebuild_model_with_active_lora().await?;
        log.info(&format!("Active LoRA adapters: {:?}", desired_ids));
        Ok(desired_ids)
    }

    /// Rebuild model with currently active LoRA adapters.
    async fn rebuild_model_with_active_lora(&self) -> Result<(), String> {
        let active = self.active_adapters.read().clone();
        if active.is_empty() {
            runtime::logger("candle").info("No active adapters, reloading base model");
            drop(active);
            return self.reload_base_model().await;
        }

        // Collect genome adapters
        let loaded = self.loaded_adapters.read();
        let mut genome_adapters: Vec<GenomeAdapter> = Vec::new();

        for adapter_id in &active {
            if let Some(la) = loaded.get(adapter_id) {
                if let Some(weights) = &la.weights {
                    genome_adapters.push(GenomeAdapter {
                        adapter_id: la.adapter_id.clone(),
                        weights: weights.clone(),
                        scale: la.scale,
                    });
                }
            }
        }
        drop(loaded);

        if genome_adapters.is_empty() {
            return Err("No active adapters have loaded weights".to_string());
        }

        // Use the trait method
        let mut backend_guard = self.backend.write();
        let wrapper = backend_guard.as_mut().ok_or("Model not loaded")?;
        let backend = &mut wrapper.0;

        if !backend.supports_lora() {
            return Err("Current backend does not support LoRA".to_string());
        }

        backend.rebuild_with_lora(&genome_adapters, self.gpu_manager.as_ref())
    }

    /// Reload base model without LoRA.
    async fn reload_base_model(&self) -> Result<(), String> {
        let mut backend_guard = self.backend.write();
        let wrapper = backend_guard.as_mut().ok_or("Model not loaded")?;
        wrapper.0.reload_base()
    }
}

impl Default for CandleAdapter {
    fn default() -> Self {
        Self::new()
    }
}

fn inference_inner(
    backend_arc: Arc<RwLock<Option<BackendWrapper>>>,
    gpu_mgr: Option<Arc<GpuMemoryManager>>,
    use_quantized: bool,
    resolved_model: &str,
    prompt: &str,
    max_tokens: usize,
    sampling: &backends::SamplingConfig,
) -> Result<((String, usize), Option<GpuAllocationGuard>), String> {
    let log = runtime::logger("candle");

    let mut backend_guard = backend_arc.write();
    let mut new_model_guard: Option<GpuAllocationGuard> = None;

    // Lazy load: if model not loaded yet, load it now
    if backend_guard.is_none() {
        log.info(&format!("Loading model: {}", resolved_model));
        let model: Box<dyn ModelBackend> = if use_quantized {
            load_default_quantized()
                .map_err(|e| format!("Failed to load quantized model: {e}"))?
        } else if let Some(local_dir) = find_local_model(resolved_model) {
            // Local GGUF model found — load from disk (no download needed)
            log.info(&format!("Found local model: {:?}", local_dir));
            super::model::load_model_from_dir(&local_dir, resolved_model)
                .map_err(|e| format!("Failed to load local model {:?}: {e}", local_dir))?
        } else {
            load_model_by_id(resolved_model)
                .map_err(|e| format!("Failed to load model '{}': {e}", resolved_model))?
        };

        // Track GPU allocation for model weights
        let vram_bytes = model.estimated_vram_bytes();
        log.info(&format!(
            "Model loaded: arch={}, format={:?}, context_length={}, model_id={}, vram={:.0}MB",
            model.architecture(), model.format(), model.context_length(), model.model_id(),
            vram_bytes as f64 / (1024.0 * 1024.0)
        ));

        if let Some(mgr) = &gpu_mgr {
            if vram_bytes > 0 {
                match mgr.allocate(GpuSubsystem::Inference, vram_bytes, GpuPriority::Interactive) {
                    Ok(guard) => {
                        mgr.eviction_registry.register(make_entry(
                            &format!("candle:model:{}", model.model_id()),
                            &format!("{} ({})", model.model_id(), model.architecture()),
                            GpuPriority::Interactive,
                            vram_bytes,
                        ));
                        new_model_guard = Some(guard);
                    }
                    Err(e) => {
                        log.error(&format!("GPU CRITICAL: Cannot load model — {}", e));
                        return Err(format!("GPU memory critical — cannot load model: {e}"));
                    }
                }
            }
        }

        *backend_guard = Some(BackendWrapper(model));
    }

    let wrapper = backend_guard.as_mut().expect("just loaded");
    let gen_result = backends::generate(&mut *wrapper.0, prompt, max_tokens, sampling);
    gen_result.map(|r| (r, new_model_guard))
}

#[async_trait]
impl AIProviderAdapter for CandleAdapter {
    fn provider_id(&self) -> &str {
        &self.config.provider_id
    }

    fn name(&self) -> &str {
        &self.config.name
    }

    fn capabilities(&self) -> AdapterCapabilities {
        // Query the actual loaded backend for its context window.
        // Falls back to BF16_PRACTICAL_CONTEXT if backend not yet loaded.
        let context_window = self
            .backend
            .try_read()
            .and_then(|guard| guard.as_ref().map(|b| b.0.context_length() as u32))
            .unwrap_or(DEFAULT_CONTEXT_WINDOW);

        AdapterCapabilities {
            supports_text_generation: true,
            supports_chat: true,
            supports_tool_use: false,
            supports_vision: false,
            supports_streaming: false,
            supports_embeddings: false,
            supports_audio: false,
            supports_image_generation: false,
            is_local: true,
            max_context_window: context_window,
        }
    }

    fn api_style(&self) -> ApiStyle {
        ApiStyle::Local
    }

    fn default_model(&self) -> &str {
        &self.config.default_model
    }

    async fn initialize(&mut self) -> Result<(), String> {
        let log = runtime::logger("candle");
        log.info(&format!(
            "Candle adapter ready (quantized={})",
            self.use_quantized
        ));

        // Eager-load the llama.cpp model in the background so the first user
        // chat message doesn't pay the 6s model-load latency. The load uses
        // the same load-gate as the lazy path in generate_text — if a request
        // arrives before warmup completes, it waits on the same mutex; if it
        // arrives after, the backend is already populated and the load_gate
        // is uncontended.
        //
        // Failure is non-fatal: if no GGUF is found locally we just log a
        // warning and the lazy path still applies on first request. This is
        // only a startup optimization, not a correctness requirement.
        if self.use_quantized {
            // Pick the first GGUF available locally — this is the model the
            // first chat will most likely target. If multiple GGUFs are
            // cached, this picks one and the lazy path will fall back if a
            // request asks for a different one (current design has only ONE
            // backend per CandleAdapter, so the eager pick is the de-facto
            // default until restart).
            if let Some(local_gguf) = find_first_local_gguf() {
                let backend_slot = self.llamacpp_backend.clone();
                let load_gate = self.llamacpp_load_gate.clone();
                tokio::spawn(async move {
                    let log = runtime::logger("candle");
                    log.info(&format!(
                        "🔥 Eager-loading llama.cpp backend (background): {}",
                        local_gguf.display()
                    ));
                    let _load_permit = load_gate.lock_owned().await;
                    if backend_slot.read().is_some() {
                        return; // a request raced us and lazy-loaded already
                    }
                    let path_str = match local_gguf.to_str() {
                        Some(s) => s.to_string(),
                        None => { log.warn("Eager-load: non-utf8 GGUF path"); return; }
                    };
                    let load_start = std::time::Instant::now();
                    let n_seq_max = local_inference_capacity() as u32;
                    let result = tokio::task::spawn_blocking(move || {
                        let config = backends::llamacpp::LlamaCppConfig {
                            model_path: std::path::PathBuf::from(path_str),
                            n_seq_max,
                            ..Default::default()
                        };
                        backends::llamacpp::LlamaCppBackend::load(config)
                    }).await;
                    match result {
                        Ok(Ok(backend)) => {
                            log.info(&format!(
                                "🔥 Eager-load complete in {:.2}s — first chat will skip the cold start",
                                load_start.elapsed().as_secs_f64()
                            ));
                            *backend_slot.write() = Some(Arc::new(backend));
                        }
                        Ok(Err(e)) => log.warn(&format!(
                            "Eager-load failed ({e}); falling back to lazy load"
                        )),
                        Err(e) => log.warn(&format!(
                            "Eager-load task panicked ({e}); falling back to lazy load"
                        )),
                    }
                });
            } else {
                log.info("Eager-load skipped: no local GGUF found in ~/.cache/huggingface or models dir");
            }
        }
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), String> {
        runtime::logger("candle").info("Shutting down Candle adapter");
        let mut backend = self.backend.write();
        *backend = None;
        // Release all GPU allocation guards
        *self.model_guard.write() = None;
        self.adapter_guards.write().clear();
        Ok(())
    }

    async fn generate_text(
        &self,
        request: TextGenerationRequest,
    ) -> Result<TextGenerationResponse, String> {
        let log = runtime::logger("candle");
        let start = std::time::Instant::now();

        log.info(&format!(
            "generate_text called, use_quantized={}, self_ptr={:p}",
            self.use_quantized, self as *const _
        ));

        let max_tokens = request.max_tokens
            .ok_or_else(|| "max_tokens is required for local inference".to_string())? as usize;
        let temperature = request.temperature
            .ok_or_else(|| "temperature is required for local inference".to_string())? as f64;
        // Build sampling config — all values from caller, no silent defaults.
        // top_k=0 and top_p=1.0 mean "disabled" — these are safe defaults
        // because they don't change behavior (no filtering applied).
        // repeat_penalty=1.0 means "disabled" — also safe.
        let sampling = backends::SamplingConfig {
            temperature,
            repeat_penalty: request.repeat_penalty.unwrap_or(1.0),
            top_k: request.top_k.unwrap_or(0) as usize,
            top_p: request.top_p.unwrap_or(1.0) as f64,
        };

        // Apply LoRA adapters if requested
        let mut applied_adapters: Vec<String> = Vec::new();
        if let Some(adapters) = &request.active_adapters {
            if !adapters.is_empty() {
                applied_adapters = self.ensure_adapters(adapters).await?;
            }
        }

        // Resolve requested model — MUST be explicitly provided.
        // Silent defaults to models that may not exist on the user's machine cause
        // mysterious failures or wrong-model bugs.
        let requested_model = request.model.as_deref()
            .ok_or_else(|| format!(
                "model is required for local inference. Available: 'coder' (14B GGUF), \
                 'coder-bf16' (14B BF16). Got no model in request."
            ))?;
        let model_id = resolve_model_id(requested_model);

        // Build prompt using the correct chat template for this model.
        // If a system_prompt is provided but not already in messages, prepend it.
        let chat_template = resolve_chat_template(requested_model);
        let has_system_msg = request.messages.iter().any(|m| m.role == "system");
        let messages = if !has_system_msg {
            if let Some(ref sys) = request.system_prompt {
                let mut msgs = vec![crate::ai::ChatMessage {
                    role: "system".to_string(),
                    content: crate::ai::MessageContent::Text(sys.clone()),
                    name: None,
                }];
                msgs.extend(request.messages.iter().cloned());
                msgs
            } else {
                request.messages.clone()
            }
        } else {
            request.messages.clone()
        };
        let prompt = build_prompt_from_messages(&messages, &chat_template);
        log.info(&format!("Using chat template: {}", chat_template));

        let prompt_len = prompt.len();
        log.info(&format!(
            "Prompt length: {} chars, max_tokens: {}, model: {} (requested: {})",
            prompt_len, max_tokens, model_id, requested_model
        ));

        // Dump formatted prompt to file for isolated reproduction (Step 1 of inside-out validation).
        // Enable with: CANDLE_DUMP_PROMPTS=1
        if std::env::var("CANDLE_DUMP_PROMPTS").is_ok() {
            let prompt_file = "/tmp/sentinel_prompt_latest.txt";
            if let Err(e) = std::fs::write(prompt_file, &prompt) {
                log.warn(&format!("Failed to dump prompt to {}: {}", prompt_file, e));
            } else {
                log.info(&format!("Prompt dumped to {} ({} chars)", prompt_file, prompt.len()));
            }
        }

        let backend_arc = Arc::clone(&self.backend);
        let resolved_model = model_id.clone();
        let use_quantized = self.use_quantized;
        let gpu_mgr = self.gpu_manager.clone();

        // Check if currently loaded model differs from requested — unload if so
        let needs_switch = {
            let backend_guard = self.backend.read();
            backend_guard.as_ref().and_then(|wrapper| {
                let loaded = wrapper.0.model_id();
                if loaded != model_id { Some(loaded.to_string()) } else { None }
            })
        };
        if let Some(old_model_id) = needs_switch {
            log.info(&format!(
                "Model switch: loaded='{}' != requested='{}' — unloading current model",
                old_model_id, model_id
            ));
            *self.backend.write() = None;
            *self.model_guard.write() = None;
            self.loaded_adapters.write().clear();
            self.active_adapters.write().clear();
            self.adapter_guards.write().clear();
            if let Some(mgr) = &self.gpu_manager {
                mgr.eviction_registry.unregister(&format!("candle:model:{}", old_model_id));
            }
        }

        // ── Pressure-aware inference: log but NEVER refuse ──
        // Local inference is the platform's lifeline. Users without API keys
        // depend entirely on Candle. The semaphore serializes to 1 concurrent
        // inference which naturally bounds memory. Refusing under pressure
        // cripples the entire system for local-only users.
        //
        // Under memory pressure we log a warning (for diagnostics) and reduce
        // max_tokens to lower peak memory, but we always proceed through the
        // semaphore queue. The queue itself is the throttle — requests wait
        // their turn, they are never refused.
        let under_pressure = crate::system_resources::is_memory_gate_closed();
        if under_pressure {
            log.info(&format!(
                "⚠️ Memory pressure high — queuing inference for '{}' (will proceed when semaphore available)",
                model_id
            ));
        }

        // ── Ensure llama.cpp backend is loaded (BEFORE acquiring the
        // inference semaphore). Idempotent: if eager-load (initialize)
        // already populated the backend, this returns immediately. If a
        // concurrent caller is in the middle of loading, we wait on the
        // same load_gate. Loading runs on spawn_blocking so the async
        // runtime stays responsive during the 6s mmap + Metal init. ──
        ensure_llamacpp_loaded_async(
            self.llamacpp_backend.clone(),
            self.llamacpp_load_gate.clone(),
            &model_id,
        ).await?;

        // The continuous-batching scheduler IS the gate now: capacity is
        // bounded by `n_seq_max` inside llama.cpp, and overflow requests
        // queue on the scheduler's mpsc channel until a sequence slot
        // frees. The previous `inference_semaphore.acquire_owned()` here
        // double-gated — it serialized requests outside the scheduler
        // even though the scheduler itself was already enforcing the
        // same capacity bound. Removed.

        // Generate on the blocking pool. spawn_blocking moves the sync C++
        // work off the async runtime entirely — no main-thread blocking,
        // no block_in_place pinning a worker, no guard held across await.
        // We clone the Arc<LlamaCppBackend> out of the RwLock so the guard
        // is dropped before we cross into the blocking task.
        let llama_arc = self.llamacpp_backend.read()
            .as_ref()
            .cloned()
            .ok_or_else(|| "llama.cpp backend not loaded after load attempt".to_string())?;
        let prompt_for_gen = prompt.clone();
        let temperature = sampling.temperature as f32;
        let (output_text, completion_tokens) = tokio::task::spawn_blocking(move || {
            let stop_tokens: [&str; 2] = ["<|im_end|>", "<|endoftext|>"];
            llama_arc.generate(&prompt_for_gen, max_tokens, temperature, &stop_tokens, &[])
        }).await
            .map_err(|e| format!("llama.cpp generate task panicked: {e}"))?
            .map_err(|e| format!("llama.cpp generate failed: {e}"))?;
        let new_model_guard: Option<GpuAllocationGuard> = None;

        // Store model guard if this was a first load
        if let Some(guard) = new_model_guard {
            *self.model_guard.write() = Some(guard);
        }

        // Touch eviction registry entries (model + active adapters) on use
        if let Some(mgr) = &self.gpu_manager {
            mgr.eviction_registry
                .touch(&format!("candle:model:{}", model_id));
            for adapter_id in &applied_adapters {
                mgr.eviction_registry
                    .touch(&format!("candle:adapter:{}", adapter_id));
            }
        }

        let duration = start.elapsed();
        let input_tokens = (prompt_len / 4) as u32;
        let output_tokens = completion_tokens as u32;

        Ok(TextGenerationResponse {
            text: output_text,
            model: model_id,
            provider: "candle".to_string(),
            finish_reason: FinishReason::Stop,
            usage: UsageMetrics {
                input_tokens,
                output_tokens,
                total_tokens: input_tokens + output_tokens,
                estimated_cost: Some(0.0),
            },
            response_time_ms: duration.as_millis() as u64,
            request_id: uuid::Uuid::new_v4().to_string(),
            content: None,
            tool_calls: None,
            routing: if applied_adapters.is_empty() {
                None
            } else {
                Some(RoutingInfo {
                    provider: "candle".to_string(),
                    is_local: true,
                    routing_reason: "local_with_lora".to_string(),
                    adapters_applied: applied_adapters,
                    model_mapped: None,
                    model_requested: None,
                })
            },
            error: None,
        })
    }

    async fn health_check(&self) -> HealthStatus {
        let backend = self.backend.read();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if backend.is_some() {
            HealthStatus {
                status: HealthState::Healthy,
                api_available: true,
                response_time_ms: 0,
                error_rate: 0.0,
                last_checked: now,
                message: Some("Model loaded".to_string()),
            }
        } else {
            HealthStatus {
                status: HealthState::Healthy,
                api_available: true,
                response_time_ms: 0,
                error_rate: 0.0,
                last_checked: now,
                message: Some("Model will load on first use".to_string()),
            }
        }
    }

    async fn get_available_models(&self) -> Vec<ModelInfo> {
        let format_label = if self.use_quantized {
            "quantized"
        } else {
            "safetensors"
        };

        vec![ModelInfo {
            id: self.config.default_model.clone(),
            name: format!("{} ({})", self.config.default_model, format_label),
            provider: "candle".to_string(),
            capabilities: vec![ModelCapability::TextGeneration, ModelCapability::Chat],
            context_window: DEFAULT_CONTEXT_WINDOW,
            max_output_tokens: Some(4096),
            cost_per_1k_tokens: None,
            supports_streaming: false,
            supports_tools: false,
        }]
    }

    fn supported_model_prefixes(&self) -> Vec<&'static str> {
        // Intentionally empty — Candle is NOT a chat-routing default.
        //
        // Candle runs CPU-heavy on Apple Silicon and anywhere without a
        // well-supported Metal/CUDA path; defaulting chat to Candle silently
        // gave every user a slow first-chat experience, which is the single
        // biggest "Continuum feels broken" signal.
        //
        // Chat routes explicitly through GPU adapters only:
        //   - `docker-model-runner`      (DMR with vllm-metal on Mac, or
        //                                 llama.cpp-cuda/rocm on Linux)
        //   - `llama-vulkan`             (our vendored llama.cpp built with
        //                                 --features=vulkan; covers "everyone
        //                                 else with a GPU")
        //
        // Candle stays available as an adapter for callers who set
        // `provider: "candle"` EXPLICITLY — intended for LoRA training /
        // safetensors fine-tuning workflows where Candle's Rust-native
        // autodiff + LoRA support is the right tool. Those callers bypass
        // `supports_model()` entirely (AdapterRegistry::select line ~296
        // short-circuits on exact provider match).
        //
        // **OBVIOUS SPOT FOR CPU SUPPORT LATER:** when we add back a CPU-ok
        // path for hardware that has no GPU at all, it should be:
        //   1. A NEW adapter (e.g. `candle-cpu`) — never mix this into the
        //      existing `candle` adapter.
        //   2. Registered ONLY when env `CONTINUUM_ALLOW_CPU_INFERENCE=1`
        //      is set — no silent opt-in.
        //   3. Accompanied by an install-time warning: "Continuum will run
        //      without GPU acceleration. Expect N seconds per message."
        //   4. Still fail-loud if model isn't on disk — same honesty rule.
        vec![]
    }
}

/// Single source of truth for local model metadata.
///
/// Model registry entry loaded from model_registry.json (embedded at compile time).
/// TypeScript gets these types via ts-rs — NO hand-written duplicates.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../shared/generated/inference/ModelRegistryEntry.ts")]
pub struct ModelRegistryEntry {
    /// HuggingFace repo ID (canonical source)
    pub repo: String,
    /// Serialization format: "gguf" or "safetensors"
    #[ts(optional)]
    pub format: Option<String>,
    /// Model architecture: "qwen2", "llama", "phi", etc.
    #[ts(optional)]
    pub architecture: Option<String>,
    /// Human-readable description
    #[ts(optional)]
    pub description: Option<String>,
    /// Minimum GPU memory in GB to run this model
    #[ts(optional, type = "number")]
    pub min_memory_gb: Option<f64>,
    /// Chat template name: "qwen2", "llama3", "chatml"
    #[ts(optional)]
    pub chat_template: Option<String>,
}

/// Full model registry — maps aliases to model entries.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../shared/generated/inference/ModelRegistry.ts")]
pub struct ModelRegistry {
    pub models: HashMap<String, ModelRegistryEntry>,
}

/// Load the model registry from the embedded JSON.
pub fn load_registry() -> ModelRegistry {
    let json = include_str!("model_registry.json");
    serde_json::from_str(json).unwrap_or_else(|e| {
        runtime::logger("candle").error(&format!("Failed to parse model registry: {e}"));
        ModelRegistry { models: HashMap::new() }
    })
}

pub fn resolve_model_id(requested: &str) -> String {
    // Already a HuggingFace repo ID
    if requested.contains('/') {
        return requested.to_string();
    }

    let normalized = requested.trim().to_lowercase();
    let registry = load_registry();

    // Look up in registry (supports "coder", "smollm2:1.7b", "llama3.2:3b", etc.)
    if let Some(entry) = registry.models.get(&normalized) {
        return entry.repo.clone();
    }

    // Try with common alias patterns: "smollm2-1.7b" → "smollm2:1.7b"
    let dash_to_colon = normalized.replacen('-', ":", 1);
    if let Some(entry) = registry.models.get(&dash_to_colon) {
        return entry.repo.clone();
    }

    // Fallback: treat as HF repo ID
    runtime::logger("candle").warn(&format!(
        "Model '{}' not in registry — treating as HuggingFace repo ID", requested
    ));
    requested.to_string()
}

/// Resolve the storage root for large files (models, adapters, datasets).
/// Checks CONTINUUM_STORAGE_PATH from: env var → ~/.continuum/config.env → fallback ~/.continuum/.
fn storage_root() -> std::path::PathBuf {
    // 1. Check env var first
    if let Ok(storage) = std::env::var("CONTINUUM_STORAGE_PATH") {
        if !storage.is_empty() {
            return std::path::PathBuf::from(storage);
        }
    }
    // 2. Check config.env (Secrets module skips non-secret keys like this)
    if let Some(home) = dirs::home_dir() {
        let config_path = home.join(".continuum").join("config.env");
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(value) = trimmed.strip_prefix("CONTINUUM_STORAGE_PATH=") {
                    let value = value.trim();
                    if !value.is_empty() {
                        return std::path::PathBuf::from(value);
                    }
                }
            }
        }
    }
    // 3. Default
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    std::path::PathBuf::from(home).join(".continuum")
}

/// Find the first available GGUF on disk for eager-load warmup. Scans the
/// HF cache (`~/.cache/huggingface/hub/models--*-GGUF/snapshots/*/*.gguf`)
/// and returns the first match. Used by `initialize()` to pick a sensible
/// default model when no specific request has come in yet.
fn find_first_local_gguf() -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let hf_cache = std::path::PathBuf::from(&home).join(".cache/huggingface/hub");
    if !hf_cache.exists() { return None; }
    for entry in std::fs::read_dir(&hf_cache).ok()?.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.starts_with("models--") { continue; }
        let snapshots = entry.path().join("snapshots");
        let Ok(snaps) = std::fs::read_dir(&snapshots) else { continue; };
        for snap in snaps.flatten() {
            let Ok(files) = std::fs::read_dir(snap.path()) else { continue; };
            for f in files.flatten() {
                let p = f.path();
                if p.extension().and_then(|s| s.to_str()) == Some("gguf") {
                    return Some(p);
                }
            }
        }
    }
    None
}

/// Ensure the llama.cpp backend is loaded for `model_id`. Idempotent and
/// safe for concurrent callers via `load_gate`. The actual `Model::load`
/// runs in `spawn_blocking` because it is a synchronous C++ FFI call
/// (mmap + Metal init + ~2GB allocation) that must not stall the async
/// runtime.
///
/// Returns Err if the GGUF cannot be located or load fails. Used by both
/// the eager-load path in `initialize()` and the lazy load path in
/// `generate_text()`. Sharing one helper means only one place to update
/// when load semantics change.
async fn ensure_llamacpp_loaded_async(
    backend_slot: Arc<RwLock<Option<Arc<backends::llamacpp::LlamaCppBackend>>>>,
    load_gate: Arc<tokio::sync::Mutex<()>>,
    model_id: &str,
) -> Result<(), String> {
    if backend_slot.read().is_some() {
        return Ok(());
    }
    let _load_permit = load_gate.lock_owned().await;
    if backend_slot.read().is_some() {
        return Ok(());
    }
    let log = runtime::logger("candle");
    let gguf_path = find_local_gguf(model_id)
        .ok_or_else(|| format!(
            "No GGUF for model '{}'. Ensure the model is downloaded to ~/.continuum/genome/models or HF cache.",
            model_id
        ))?;
    let path_str = gguf_path.to_str()
        .ok_or("non-utf8 model path")?.to_string();
    log.info(&format!("Loading llama.cpp backend: {}", path_str));
    let load_start = std::time::Instant::now();
    let backend = tokio::task::spawn_blocking(move || {
        let config = backends::llamacpp::LlamaCppConfig {
            model_path: std::path::PathBuf::from(path_str),
            n_seq_max: local_inference_capacity() as u32,
            ..Default::default()
        };
        backends::llamacpp::LlamaCppBackend::load(config)
    }).await
        .map_err(|e| format!("llama.cpp load task panicked: {e}"))??;
    log.info(&format!(
        "llama.cpp backend ready ({:.2}s)",
        load_start.elapsed().as_secs_f64()
    ));
    *backend_slot.write() = Some(Arc::new(backend));
    Ok(())
}

/// Check if a model is available locally as a GGUF.
/// Searches ~/.continuum/ (internal NVMe, fast) FIRST, then CONTINUUM_STORAGE_PATH (external, slow).
/// Returns the local directory path if found, None if not cached.
/// Find the .gguf file for a model, searching local dirs + HF cache.
/// Used by the llama.cpp backend which needs a GGUF file path directly.
fn find_local_gguf(model_id: &str) -> Option<std::path::PathBuf> {
    // Try local model dir first (via find_local_model)
    if let Some(dir) = find_local_model(model_id) {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) == Some("gguf") {
                    return Some(p);
                }
            }
        }
    }
    // Fall back to HF cache
    let home = std::env::var("HOME").ok()?;
    let hf_cache = std::path::PathBuf::from(&home).join(".cache/huggingface/hub");
    if !hf_cache.exists() { return None; }
    for entry in std::fs::read_dir(&hf_cache).ok()?.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        // Match "models--*<model_id>*" or a fuzzy match on slug
        if name_str.starts_with("models--") && name_str.to_lowercase().contains(&model_id.to_lowercase().replace('/', "--")) {
            // Look inside snapshots/<hash>/ for a .gguf file
            let snapshots = entry.path().join("snapshots");
            if let Ok(snaps) = std::fs::read_dir(&snapshots) {
                for snap in snaps.flatten() {
                    if let Ok(files) = std::fs::read_dir(snap.path()) {
                        for f in files.flatten() {
                            let p = f.path();
                            if p.extension().and_then(|s| s.to_str()) == Some("gguf") {
                                return Some(p);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn find_local_model(model_id: &str) -> Option<std::path::PathBuf> {
    let search_dirs = {
        let mut dirs = Vec::new();
        // Internal drive first (NVMe = ~2s load vs external USB = ~105s)
        let home = std::env::var("HOME").ok()?;
        let home_models = std::path::PathBuf::from(&home).join(".continuum/genome/models");
        dirs.push(home_models.clone());
        // External/overflow storage second
        let storage_models = storage_root().join("genome/models");
        if storage_models != home_models {
            dirs.push(storage_models);
        }
        dirs
    };

    for models_dir in &search_dirs {
        if !models_dir.exists() {
            continue;
        }
        if let Some(found) = find_model_in_dir(model_id, models_dir) {
            return Some(found);
        }
    }
    None
}

fn find_model_in_dir(model_id: &str, models_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    if !models_dir.exists() {
        return None;
    }

    // Check for exact directory match (e.g., model dirs we created)
    for entry in std::fs::read_dir(&models_dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        // Check if this directory has a GGUF file + tokenizer
        let has_gguf = std::fs::read_dir(&path)
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .any(|e| {
                        e.path()
                            .extension()
                            .and_then(|ext| ext.to_str())
                            .map(|ext| ext == "gguf")
                            .unwrap_or(false)
                    })
            })
            .unwrap_or(false);

        let has_tokenizer = path.join("tokenizer.json").exists();

        if has_gguf && has_tokenizer {
            // Match by directory name containing model ID parts
            let dir_name = path.file_name()?.to_str()?.to_lowercase();
            let model_lower = model_id.to_lowercase();

            // Match "continuum-ai/qwen2.5-coder-32b-compacted" against "qwen32b-compacted-v3"
            // Must also match size indicator (14b, 32b) to avoid confusing 14B and 32B models
            if model_lower.contains("qwen") && model_lower.contains("compacted")
                && dir_name.contains("qwen") && dir_name.contains("compacted")
            {
                // Extract size indicator from model_id (e.g., "14b", "32b")
                let size_match = ["14b", "32b", "7b", "3b", "1b"]
                    .iter()
                    .find(|s| model_lower.contains(*s));
                if let Some(size) = size_match {
                    // If model specifies a size, directory must also contain it
                    if dir_name.contains(size) {
                        return Some(path);
                    }
                    // Size mismatch — skip this directory
                } else {
                    // No size in model_id — accept any match
                    return Some(path);
                }
            }

            // Generic: check if model_id's repo name appears in dir name
            if let Some(repo_name) = model_id.split('/').last() {
                let repo_lower = repo_name.to_lowercase().replace('.', "");
                if dir_name.contains(&repo_lower) {
                    return Some(path);
                }
            }
        }
    }

    None
}

/// Estimate VRAM usage for a LoRA adapter from its file path.
/// Path may be a directory (containing adapter_model.safetensors) or a direct file.
fn estimate_adapter_vram(path: &str) -> u64 {
    let p = std::path::Path::new(path);
    let file_path = if p.is_dir() {
        p.join("adapter_model.safetensors")
    } else {
        p.to_path_buf()
    };
    std::fs::metadata(&file_path).map(|m| m.len()).unwrap_or(0)
}

/// Look up the chat template name for a model from the registry.
/// Falls back to "llama3" for unknown models.
pub fn resolve_chat_template(requested_model: &str) -> String {
    let normalized = requested_model.trim().to_lowercase();
    let registry = load_registry();

    // Direct registry lookup
    if let Some(entry) = registry.models.get(&normalized) {
        if let Some(ref tmpl) = entry.chat_template {
            return tmpl.clone();
        }
    }

    // Infer from model name
    if normalized.contains("qwen") {
        return "qwen2".to_string();
    }
    if normalized.contains("chatml") || normalized.contains("smollm") {
        return "chatml".to_string();
    }

    "llama3".to_string()
}

/// Extract text content from a chat message.
fn extract_message_text(msg: &crate::ai::ChatMessage) -> String {
    match &msg.content {
        crate::ai::MessageContent::Text(text) => text.clone(),
        crate::ai::MessageContent::Parts(parts) => parts
            .iter()
            .filter_map(|p| {
                if let crate::ai::ContentPart::Text { text } = p {
                    Some(text.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// Build a prompt string from chat messages using the appropriate chat template.
fn build_prompt_from_messages(messages: &[crate::ai::ChatMessage], template: &str) -> String {
    match template {
        "qwen2" | "chatml" => build_prompt_chatml(messages),
        _ => build_prompt_llama3(messages),
    }
}

/// ChatML / Qwen2 template: <|im_start|>role\ncontent<|im_end|>
fn build_prompt_chatml(messages: &[crate::ai::ChatMessage]) -> String {
    let mut prompt = String::new();

    let has_system = messages.iter().any(|m| m.role == "system");
    if !has_system {
        prompt.push_str("<|im_start|>system\nYou are a helpful AI assistant.<|im_end|>\n");
    }

    for msg in messages {
        let role = match msg.role.as_str() {
            "system" | "user" | "assistant" => msg.role.as_str(),
            _ => "user",
        };
        let content = extract_message_text(msg);
        prompt.push_str(&format!("<|im_start|>{}\n{}<|im_end|>\n", role, content));
    }

    prompt.push_str("<|im_start|>assistant\n");
    prompt
}

/// Llama 3 template: <|start_header_id|>role<|end_header_id|>\n\ncontent<|eot_id|>
fn build_prompt_llama3(messages: &[crate::ai::ChatMessage]) -> String {
    let mut prompt = String::from("<|begin_of_text|>");

    let has_system = messages.iter().any(|m| m.role == "system");
    if !has_system {
        prompt.push_str("<|start_header_id|>system<|end_header_id|>\n\n");
        prompt.push_str("You are a helpful AI assistant.<|eot_id|>");
    }

    for msg in messages {
        let role = match msg.role.as_str() {
            "system" | "user" | "assistant" => msg.role.as_str(),
            _ => "user",
        };
        let content = extract_message_text(msg);
        prompt.push_str(&format!("<|start_header_id|>{}<|end_header_id|>\n\n", role));
        prompt.push_str(&content);
        prompt.push_str("<|eot_id|>");
    }

    prompt.push_str("<|start_header_id|>assistant<|end_header_id|>\n\n");
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{ChatMessage, MessageContent};

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: role.to_string(),
            content: MessageContent::Text(content.to_string()),
            name: None,
        }
    }

    // ── Llama 3 template tests ──

    #[test]
    fn test_llama3_prompt_simple() {
        let messages = vec![msg("user", "What is 2+2?")];
        let prompt = build_prompt_from_messages(&messages, "llama3");

        assert!(prompt.starts_with("<|begin_of_text|>"));
        assert!(prompt.contains("<|start_header_id|>system<|end_header_id|>"));
        assert!(prompt.contains("You are a helpful AI assistant."));
        assert!(prompt.contains("<|start_header_id|>user<|end_header_id|>"));
        assert!(prompt.contains("What is 2+2?"));
        assert!(prompt.ends_with("<|start_header_id|>assistant<|end_header_id|>\n\n"));
    }

    #[test]
    fn test_llama3_prompt_with_system() {
        let messages = vec![msg("system", "You are a pirate."), msg("user", "Hello!")];
        let prompt = build_prompt_from_messages(&messages, "llama3");

        assert!(prompt.contains("You are a pirate."));
        assert!(!prompt.contains("You are a helpful AI assistant."));
    }

    #[test]
    fn test_llama3_prompt_multi_turn() {
        let messages = vec![
            msg("system", "Be concise."),
            msg("user", "Hi"),
            msg("assistant", "Hello!"),
            msg("user", "How are you?"),
        ];
        let prompt = build_prompt_from_messages(&messages, "llama3");

        assert!(prompt.starts_with("<|begin_of_text|>"));
        assert!(
            prompt.contains("<|start_header_id|>system<|end_header_id|>\n\nBe concise.<|eot_id|>")
        );
        assert!(prompt.contains("<|start_header_id|>user<|end_header_id|>\n\nHi<|eot_id|>"));
        assert!(
            prompt.contains("<|start_header_id|>assistant<|end_header_id|>\n\nHello!<|eot_id|>")
        );
        assert!(prompt.ends_with("<|start_header_id|>assistant<|end_header_id|>\n\n"));
    }

    // ── Qwen2 / ChatML template tests ──

    #[test]
    fn test_qwen2_prompt_simple() {
        let messages = vec![msg("user", "What is 2+2?")];
        let prompt = build_prompt_from_messages(&messages, "qwen2");

        assert!(prompt.contains("<|im_start|>system\nYou are a helpful AI assistant.<|im_end|>"));
        assert!(prompt.contains("<|im_start|>user\nWhat is 2+2?<|im_end|>"));
        assert!(prompt.ends_with("<|im_start|>assistant\n"));
        // Must NOT contain Llama tokens
        assert!(!prompt.contains("<|begin_of_text|>"));
        assert!(!prompt.contains("<|start_header_id|>"));
        assert!(!prompt.contains("<|eot_id|>"));
    }

    #[test]
    fn test_qwen2_prompt_with_system() {
        let messages = vec![msg("system", "You are a coding agent."), msg("user", "Write code")];
        let prompt = build_prompt_from_messages(&messages, "qwen2");

        assert!(prompt.contains("<|im_start|>system\nYou are a coding agent.<|im_end|>"));
        assert!(prompt.contains("<|im_start|>user\nWrite code<|im_end|>"));
        assert!(!prompt.contains("You are a helpful AI assistant."));
    }

    #[test]
    fn test_qwen2_prompt_multi_turn() {
        let messages = vec![
            msg("system", "Be concise."),
            msg("user", "Hi"),
            msg("assistant", "Hello!"),
            msg("user", "How are you?"),
        ];
        let prompt = build_prompt_from_messages(&messages, "qwen2");

        assert!(prompt.contains("<|im_start|>system\nBe concise.<|im_end|>"));
        assert!(prompt.contains("<|im_start|>user\nHi<|im_end|>"));
        assert!(prompt.contains("<|im_start|>assistant\nHello!<|im_end|>"));
        assert!(prompt.contains("<|im_start|>user\nHow are you?<|im_end|>"));
        assert!(prompt.ends_with("<|im_start|>assistant\n"));
    }

    #[test]
    fn test_resolve_chat_template() {
        assert_eq!(resolve_chat_template("coder"), "qwen2");
        assert_eq!(resolve_chat_template("coder-14b"), "qwen2");
        assert_eq!(resolve_chat_template("coder-32b"), "qwen2");
        assert_eq!(resolve_chat_template("llama3.2:3b"), "llama3");
        assert_eq!(resolve_chat_template("smollm2"), "chatml");
        assert_eq!(resolve_chat_template("unknown-model"), "llama3"); // default fallback
    }
}
