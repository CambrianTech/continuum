//! AIProviderModule — Adapter-based AI provider system
//!
//! Uses the adapter pattern (like ORM) for pluggable AI providers.
//! Single entry point for all text generation with tool calling support.
//!
//! Supported providers (via adapters):
//! - DeepSeek (deepseek-chat, deepseek-reasoner)
//! - Anthropic (claude-sonnet-4-5, claude-opus-4, claude-3-5-haiku)
//! - OpenAI (gpt-4, gpt-4o)
//! - Together AI (llama-3.1-70b)
//! - Groq (llama-3.1-8b-instant)
//! - Fireworks (deepseek-v3)
//! - XAI (grok-3)
//! - Google (gemini-2.0-flash)
//!
//! Commands:
//! - ai/generate: Generate text with optional tool calling
//! - ai/providers/list: List available providers
//! - ai/providers/health: Check provider health

use crate::ai::{
    adapter::{AIProviderAdapter, InferenceDevice},
    AdapterRegistry, AnthropicAdapter, ChatMessage, MessageContent, OpenAICompatibleAdapter,
    RoutingInfo, TextGenerationRequest, TextGenerationResponse,
};
use crate::logging::TimingGuard;
use crate::runtime::{
    CommandResult, ModuleConfig, ModuleContext, ModuleLogger, ModulePriority, ServiceModule,
};
use crate::secrets::get_secret;
use crate::utils::params::Params;
use async_trait::async_trait;
use once_cell::sync::Lazy;
use serde_json::{json, Value};
use std::any::Any;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{OnceCell, RwLock};

/// Provider ID for the Docker Model Runner adapter — single source of truth
/// shared between init-time registration and the watchdog tick.
const DMR_PROVIDER_ID: &str = "docker-model-runner";

/// How often the watchdog probes DMR. Five seconds is the same cadence
/// as the PressureBroker tick — fast enough to recover within ~one
/// chat turn after Docker Desktop restarts; slow enough that the probe
/// (a one-second TCP connect) is essentially free relative to the tick.
const DMR_TICK_INTERVAL: Duration = Duration::from_secs(5);

/// Consecutive failed-probe ticks before the watchdog escalates from
/// "transient blip" to "this is broken — tell the user." At 5s ticks,
/// 6 = 30 seconds, which is the threshold the resource architecture
/// uses for "loud failure, never silent."
const DMR_DOWN_WARN_THRESHOLD_TICKS: u64 = 6;

/// One DMR endpoint discovered by `probe_dmr`. The base_url is None for
/// localhost — the adapter's default constructor already points at
/// `localhost:12434`. A `Some(url)` means the in-container variant
/// where we resolved `model-runner.docker.internal`.
#[derive(Debug, Clone)]
struct DmrEndpoint {
    base_url: Option<String>,
}

/// Global singleton registry - survives module recreation on server restart
static GLOBAL_REGISTRY: Lazy<Arc<RwLock<AdapterRegistry>>> =
    Lazy::new(|| Arc::new(RwLock::new(AdapterRegistry::new())));

/// Track if we've done first-time initialization
static INITIALIZED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Public accessor for the global adapter registry.
/// Used by the HTTP inference endpoint to share adapters with AIProviderModule.
pub fn global_registry() -> Arc<RwLock<AdapterRegistry>> {
    GLOBAL_REGISTRY.clone()
}

/// AIProviderModule - ServiceModule implementation for AI inference
pub struct AIProviderModule {
    registry: Arc<RwLock<AdapterRegistry>>,
    log: OnceCell<Arc<ModuleLogger>>,
    /// GPU memory manager — passed to CandleAdapter for VRAM allocation tracking.
    gpu_manager: Option<Arc<crate::gpu::memory_manager::GpuMemoryManager>>,
    /// DMR watchdog state — counts consecutive down-probe ticks so we can
    /// escalate from quiet recovery to loud user-visible failure at the
    /// 30-second threshold. Atomic so the tick (`&self`) updates it
    /// without taking a write lock on the module.
    dmr_consecutive_down_ticks: Arc<AtomicU64>,
}

impl AIProviderModule {
    pub fn new() -> Self {
        Self {
            registry: GLOBAL_REGISTRY.clone(),
            log: OnceCell::new(),
            gpu_manager: None,
            dmr_consecutive_down_ticks: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Create with GPU memory manager for VRAM-aware local inference.
    pub fn with_gpu_manager(
        gpu_manager: Arc<crate::gpu::memory_manager::GpuMemoryManager>,
    ) -> Self {
        Self {
            registry: GLOBAL_REGISTRY.clone(),
            log: OnceCell::new(),
            gpu_manager: Some(gpu_manager),
            dmr_consecutive_down_ticks: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Probe DMR (Docker Model Runner) reachability and return its endpoint
    /// if reachable. Single source of truth for "is DMR up?" — used by both
    /// init-time registration and the watchdog tick, so the two never drift
    /// on what counts as "available."
    ///
    /// Returns `Some(DmrEndpoint)` when reachable, `None` otherwise. Tries
    /// localhost (host-native Docker Desktop) first, falls back to the
    /// container-internal DNS name if `/.dockerenv` exists. Uses short
    /// connect timeouts so a slow DNS or firewall block can't stall the
    /// tick.
    fn probe_dmr() -> Option<DmrEndpoint> {
        let localhost_ok = std::net::TcpStream::connect_timeout(
            &"127.0.0.1:12434".parse().unwrap(),
            Duration::from_secs(1),
        )
        .is_ok();
        if localhost_ok {
            return Some(DmrEndpoint { base_url: None });
        }

        // Not on localhost — check if we're inside a Docker container.
        // model-runner.docker.internal resolves from inside Docker
        // Desktop containers on Mac, Linux, and Windows (WSL2).
        if !std::path::Path::new("/.dockerenv").exists() {
            return None;
        }
        use std::net::ToSocketAddrs;
        let internal_ok = "model-runner.docker.internal:80"
            .to_socket_addrs()
            .ok()
            .and_then(|mut addrs| addrs.next())
            .map(|addr| std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(2)).is_ok())
            .unwrap_or(false);
        if internal_ok {
            Some(DmrEndpoint {
                base_url: Some("http://model-runner.docker.internal/engines/llama.cpp".to_string()),
            })
        } else {
            None
        }
    }

    /// Build a DMR adapter for the given endpoint. Same construction path
    /// used by both init-time registration and watchdog re-registration —
    /// the two never produce different-shaped adapters.
    fn build_dmr_adapter(endpoint: &DmrEndpoint) -> Box<dyn AIProviderAdapter> {
        let adapter = if let Some(url) = &endpoint.base_url {
            OpenAICompatibleAdapter::from_registry("docker-model-runner")
                .with_runtime_base_url(url.clone())
        } else {
            OpenAICompatibleAdapter::from_registry("docker-model-runner")
        };
        Box::new(adapter)
    }
}

/// Build the user-visible error message when `select()` returns None.
/// Distinguishes:
///   - "no providers at all" (config issue — surfaces config.env hint)
///   - "asked for local but DMR is down" (Docker Desktop needs to be running)
///   - "asked for a specific provider/model that isn't here" (existing message)
///
/// Hoisted out of both `ai/generate` and the convenience `generate_text` so
/// the two paths report the same diagnosis.
fn select_failure_message(
    registry: &AdapterRegistry,
    requested_provider: Option<&str>,
    requested_model: Option<&str>,
) -> String {
    let available = registry.available();
    if available.is_empty() {
        return "No AI providers configured. Add API keys to ~/.continuum/config.env, \
                or start Docker Desktop for local AI."
            .to_string();
    }
    // The "local" sentinel means "give me whatever the best local adapter is."
    // If the user asked for that and DMR isn't in the registry, the watchdog
    // either (a) hasn't seen DMR come up yet or (b) saw it crash and dropped
    // it. Either way, the actionable message is "start Docker Desktop."
    let asked_local = requested_provider == Some("local");
    let dmr_registered = registry.is_registered(DMR_PROVIDER_ID);
    if asked_local && !dmr_registered {
        return format!(
            "Local AI is unavailable — Docker Desktop is not running or Docker Model \
             Runner isn't enabled. To enable: docker desktop enable model-runner --tcp=12434. \
             Other available providers: {:?}",
            available
        );
    }
    format!(
        "Requested provider/model not available (provider={:?}, model={:?}). Available: {:?}",
        requested_provider, requested_model, available
    )
}

// Re-open the AIProviderModule impl block so the rest of the methods
// (parse_request, response_to_json, etc.) stay where they were.
impl AIProviderModule {
    /// Get logger (panics if called before initialize)
    fn log(&self) -> &ModuleLogger {
        self.log
            .get()
            .expect("AIProviderModule not initialized")
            .as_ref()
    }

    /// Register all available adapters
    async fn register_adapters(&self) -> Result<(), String> {
        // Check global flag to prevent re-initialization (survives module recreation)
        if INITIALIZED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            self.log()
                .info("Adapters already initialized (global), skipping re-registration");
            return Ok(());
        }

        let mut registry = self.registry.write().await;

        // Priority order (lower = higher priority):
        // 0: DeepSeek (best price/performance)
        // 1: Anthropic (best reasoning)
        // 2: OpenAI
        // 3: Groq (fast)
        // 4: Together
        // 5: Fireworks
        // 6: XAI
        // 7: Google
        //
        // HeuristicInferenceAdapter is NOT auto-registered here.
        //
        // Per [[no-fallbacks-ever]] and [[no-if-statements-use-llms-for-
        // cognition]] (Joel, 2026-06-01): "You mix this fake shit in and
        // it's going live ALL THE TIME. The fake shit is a CHOSEN model
        // adapter no other form. Declaration." Previously this module
        // unconditionally registered the heuristic adapter at priority 99
        // with the comment "never auto-selects over real adapters" — that
        // assumption was wrong. Any production code path that called
        // `select()` without specifying a model could end up at the
        // heuristic. The structural fix: heuristic adapter is gated
        // behind `cfg(any(test, feature = "test-fixtures"))` so production
        // binaries cannot link it; tests that legitimately want it
        // register it explicitly in their setup code (no global default
        // registration, no silent availability).

        // Only register adapters that have API keys configured
        if get_secret("DEEPSEEK_API_KEY").is_some() {
            self.log().info("Registering DeepSeek adapter");
            registry.register(
                Box::new(OpenAICompatibleAdapter::from_registry("deepseek")),
                0,
            );
        }

        if get_secret("ANTHROPIC_API_KEY").is_some() {
            self.log().info("Registering Anthropic adapter");
            registry.register(Box::new(AnthropicAdapter::new()), 1);
        }

        if get_secret("OPENAI_API_KEY").is_some() {
            self.log().info("Registering OpenAI adapter");
            registry.register(
                Box::new(OpenAICompatibleAdapter::from_registry("openai")),
                2,
            );
        }

        if get_secret("GROQ_API_KEY").is_some() {
            self.log().info("Registering Groq adapter");
            registry.register(Box::new(OpenAICompatibleAdapter::from_registry("groq")), 3);
        }

        if get_secret("TOGETHER_API_KEY").is_some() {
            self.log().info("Registering Together adapter");
            registry.register(
                Box::new(OpenAICompatibleAdapter::from_registry("together")),
                4,
            );
        }

        if get_secret("FIREWORKS_API_KEY").is_some() {
            self.log().info("Registering Fireworks adapter");
            registry.register(
                Box::new(OpenAICompatibleAdapter::from_registry("fireworks")),
                5,
            );
        }

        if get_secret("XAI_API_KEY").is_some() {
            self.log().info("Registering XAI adapter");
            registry.register(Box::new(OpenAICompatibleAdapter::from_registry("xai")), 6);
        }

        if get_secret("GOOGLE_API_KEY").is_some() {
            self.log().info("Registering Google adapter");
            registry.register(
                Box::new(OpenAICompatibleAdapter::from_registry("google")),
                7,
            );
        }

        // In-process llama.cpp adapter — bypasses DMR's container Metal toolchain,
        // which on M5 Pro fails to compile the tensor-API source (`has tensor=false`)
        // and falls back to a degraded path running at 22 tok/s. Our host-built
        // vendored llama.cpp compiles Metal correctly and measures 33 tok/s on the
        // same hardware (50% improvement, smoke test:
        // tests/llamacpp_metal_throughput.rs). Priority 0 — wins over DMR for
        // model IDs we own (continuum-ai/qwen3.5-*). DMR remains the runtime for
        // anything else.
        //
        // Registered eagerly when the GGUF file exists on disk. We intentionally
        // do NOT register a stub adapter that would silently fail later — per the
        // no-fallback rule, callers asking for our forge model should get either
        // a working in-process backend or a hard error at select() time naming
        // exactly which file is missing.
        // Register one in-process adapter PER llamacpp-local model row
        // whose GGUF (and, for multimodal, mmproj) is on disk. Each
        // adapter binds to a single GGUF — that's the backend's design
        // (one model per backend) — so multiple llamacpp-local rows
        // (text + vision + audio + future variants) need one adapter
        // each. Routing in AdapterRegistry::select picks by model id,
        // so they don't collide.
        //
        // Earlier shape called `LlamaCppAdapter::new()` for "the default"
        // and then iterated for the rest, but `new()` picks via HashMap
        // iteration order which is non-deterministic — caused a bug
        // where qwen3.5 got registered twice and qwen2-vl was skipped.
        // Now we iterate ALL rows uniformly.
        if let Some(reg_arc) = crate::model_registry::try_global() {
            for model_meta in reg_arc.models_for_provider(crate::inference::LLAMACPP_PROVIDER_ID) {
                let Some(gguf_path) = model_meta.gguf_local_path.clone() else {
                    self.log().info(&format!(
                        "Skipping in-process adapter for `{}` — artifact resolver found no local GGUF. \
                         Pull the model identified by gguf_hint or run the model download flow.",
                        model_meta.id
                    ));
                    continue;
                };
                if !gguf_path.exists() {
                    self.log().info(&format!(
                        "Skipping in-process adapter for `{}` — GGUF missing at {}. \
                         Install must pull this artifact for first-launch parity.",
                        model_meta.id,
                        gguf_path.display()
                    ));
                    continue;
                }
                // For vision/audio rows the mmproj is also required.
                // backend.generate_with_image / generate_with_audio
                // returns a clean error when mmproj is absent — we log
                // the gap upfront so install scripts catch it before
                // a real user hits "model declares Vision but mmproj
                // missing" at request time.
                let needs_mmproj = model_meta.has(crate::model_registry::types::Capability::Vision)
                    || model_meta.has(crate::model_registry::types::Capability::AudioInput);
                if needs_mmproj {
                    match &model_meta.mmproj_local_path {
                        None => self.log().info(&format!(
                            "Adapter `{}` declares Vision/AudioInput but TOML has no \
                             mmproj_local_path — multimodal calls will hard-error. \
                             Add `mmproj_local_path = \"...\"` to the row.",
                            model_meta.id
                        )),
                        Some(p) if !p.exists() => self.log().info(&format!(
                            "Adapter `{}` declares Vision/AudioInput but mmproj file \
                             missing at {} — multimodal calls will hard-error. \
                             Install must pull this artifact alongside the GGUF.",
                            model_meta.id,
                            p.display()
                        )),
                        Some(_) => {} // present + on disk, good
                    }
                }
                self.log().info(&format!(
                    "Registering in-process llama.cpp adapter for model `{}`",
                    model_meta.id
                ));
                // Clamp to 32768 tokens. Models like qwen3.5-4b advertise
                // n_ctx_train=262144, which would allocate a multi-GB F16
                // KV cache per seq on load and reliably fail first-decode
                // with `llama_decode returned -3` on any Mac that can't
                // fit ~50GB of scratch. 32768 matches DMR's default and
                // comfortably exceeds every persona RAG we currently
                // build. Raise after footprint_registry reports real KV
                // bytes and we have telemetry proving headroom.
                let adapter_base = crate::inference::LlamaCppAdapter::with_model_id(
                    gguf_path.clone(),
                    model_meta.id.clone(),
                )
                .with_context_length(32768);

                // Probe the GGUF architecture at registration time and
                // enable multi-seq continuous batching when safe (per
                // task #110 / batching_probe.rs). Coordinator-managed
                // lane multiplexing (per task #109) requires
                // n_seq_max>1 in the in-backend scheduler. Standard
                // transformers (Llama / Qwen-2.5 / Gemma-2 / Mistral /
                // ...) classify as SafeForMultiSeq; qwen3 / mamba /
                // rwkv / jamba / etc. classify as SingleSeqOnly and
                // we keep them at 1. Default n_seq_max for safe
                // architectures is 4 — matches the realistic-floor
                // coordinator config (4 concurrent lanes). The probe
                // is cheap (GGUF header only, no weights), runs once
                // per adapter registration.
                const N_SEQ_MAX_FOR_SAFE_MULTISEQ: u32 = 4;
                let adapter = match crate::inference::batching_probe::probe_gguf_batching_safety(
                    &gguf_path,
                ) {
                    Ok(verdict) if verdict.safe_for_multi_seq() => {
                        self.log().info(&format!(
                            "Architecture `{}` is safe for multi-seq batching; enabling n_seq_max={} \
                             for coordinator-managed lane multiplexing",
                            verdict.arch(),
                            N_SEQ_MAX_FOR_SAFE_MULTISEQ
                        ));
                        adapter_base.with_n_seq_max(N_SEQ_MAX_FOR_SAFE_MULTISEQ)
                    }
                    Ok(verdict) => {
                        self.log().info(&format!(
                            "Architecture `{}` not safe for multi-seq batching ({}); \
                             keeping n_seq_max=1",
                            verdict.arch(),
                            match &verdict {
                                crate::inference::batching_probe::BatchingSafety::SingleSeqOnly { reason, .. } => reason.as_str(),
                                _ => "architecture not in curated safe list",
                            }
                        ));
                        adapter_base
                    }
                    Err(err) => {
                        self.log().warn(&format!(
                            "Batching probe failed for `{}`: {err} — keeping n_seq_max=1 \
                             (conservative default)",
                            model_meta.id
                        ));
                        adapter_base
                    }
                };
                // Priority 0 — wins over DMR for the model ids it claims.
                registry.register(Box::new(adapter), 0);
            }
        } else {
            self.log().info(
                "In-process llama.cpp adapter NOT registered — model_registry not initialized. \
                 Local chat will route to DMR or cloud only.",
            );
        }

        // Docker Model Runner — preferred local provider when reachable. Routes
        // to llama.cpp-metal/cuda or vllm-metal depending on platform, all running
        // host-native via Docker Desktop. ~50 tok/s on M5 (Qwen2.5-7B Q4_K_M),
        // beats Candle's ~10 tok/s by 5x because Candle's Metal path goes through
        // ggml-via-candle while Model Runner is direct llama.cpp-metal.
        //
        // Initial probe + register; ongoing health is the watchdog `tick()`'s
        // job (DMR_TICK_INTERVAL = 5s). If Docker Desktop crashes mid-session,
        // the watchdog deregisters the DMR adapter so `select()` immediately
        // surfaces the right hard error to the user instead of failing in
        // generate_text against a now-unreachable endpoint.
        match Self::probe_dmr() {
            Some(endpoint) => {
                let desc = endpoint
                    .base_url
                    .as_deref()
                    .unwrap_or("localhost:12434 (host-native)");
                self.log().info(&format!(
                    "Registering Docker Model Runner adapter ({})",
                    desc
                ));
                registry.register(
                    Self::build_dmr_adapter(&endpoint),
                    // Priority 1 — sits BELOW the in-process llama.cpp adapter
                    // (priority 0) so DMR only wins for models LlamaCppAdapter
                    // doesn't claim. Critical on Mac M5 where DMR's container
                    // Metal toolchain is degraded vs the host-built bundled
                    // llama.cpp (verified 2026-04-19: 33 tok/s container vs
                    // 47 tok/s in-process for the same forge model).
                    1,
                );
            }
            None => {
                self.log().info(
                    "Docker Model Runner not reachable on localhost:12434 \
                     (nor model-runner.docker.internal inside container). \
                     Watchdog will keep probing; will register automatically \
                     once Docker Desktop comes up. To enable: \
                     docker desktop enable model-runner --tcp=12434",
                );
            }
        }

        // Candle is NOT registered in the AI provider's inference registry.
        // Candle is a TRAINING framework (LoRA fine-tuning, autodiff, safetensors).
        // It does not belong in the same registry as inference providers.
        // Training callers access Candle through the training/plasticity module
        // directly — NOT through the AI provider's adapter selection.
        //
        // Previously registered here "at lowest priority" with the excuse that
        // it would "never be picked for chat." That's wrong — it showed up
        // in the available providers list, confused error messages, and violated
        // separation of concerns. Training and inference are different activities
        // with different registries.

        // Initialize all registered adapters
        registry.initialize_all().await?;

        let available = registry.available();
        self.log().info(&format!(
            "AIProviderModule initialized with {} providers: {:?}",
            available.len(),
            available
        ));

        if available.is_empty() {
            self.log()
                .warn("No providers available! Add API keys to ~/.continuum/config.env");
        }

        Ok(())
    }

    /// Parse TextGenerationRequest from JSON params
    fn parse_request(&self, params: &Value) -> Result<TextGenerationRequest, String> {
        let p = Params::new(params);

        // Parse messages (array) or simple prompt (string)
        let messages: Vec<ChatMessage> = if let Some(msgs) = p.value("messages") {
            serde_json::from_value(msgs.clone())
                .map_err(|e| format!("Failed to parse messages: {}", e))?
        } else if let Some(prompt) = p.str_opt("prompt") {
            vec![ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text(prompt.to_string()),
                name: None,
            }]
        } else {
            return Err("Missing messages or prompt".to_string());
        };

        if messages.is_empty() {
            return Err("Messages cannot be empty".to_string());
        }

        Ok(TextGenerationRequest {
            messages,
            system_prompt: p.string_opt_alias("system_prompt", "systemPrompt"),
            model: p.str_opt("model").map(String::from),
            provider: p.str_opt("provider").map(String::from),
            temperature: p.f32_opt("temperature"),
            max_tokens: p.u64_opt_alias("max_tokens", "maxTokens").map(|t| t as u32),
            top_p: p.f64_opt_alias("top_p", "topP").map(|t| t as f32),
            top_k: p.u64_opt_alias("top_k", "topK").map(|t| t as u32),
            repeat_penalty: p
                .f32_opt("repeat_penalty")
                .or_else(|| p.f32_opt("repeatPenalty")),
            stop_sequences: p
                .json_opt("stop_sequences")
                .or_else(|| p.json_opt("stopSequences")),
            tools: p.json_opt("tools"),
            tool_choice: p.json_opt("tool_choice"),
            response_format: None,
            active_adapters: p.json_opt("activeAdapters"),
            request_id: p.string_opt_alias("request_id", "requestId"),
            user_id: p.string_opt_alias("user_id", "userId"),
            room_id: p.string_opt_alias("room_id", "roomId"),
            purpose: p.str_opt("purpose").map(String::from),
            // Caller-provided persona attribution. TS sends `personaId`
            // (camelCase) per Continuum convention; snake_case alias
            // accepted for symmetry with the sibling fields.
            persona_id: p.string_opt_alias("persona_id", "personaId"),
        })
    }

    /// Convert response to JSON Value
    fn response_to_json(&self, response: &TextGenerationResponse) -> Value {
        let mut result = json!({
            "success": true,
            "text": response.text,
            "finishReason": format!("{}", response.finish_reason),
            "model": response.model,
            "provider": response.provider,
            "usage": {
                "inputTokens": response.usage.input_tokens,
                "outputTokens": response.usage.output_tokens,
                "totalTokens": response.usage.total_tokens,
                "estimatedCost": response.usage.estimated_cost
            },
            "responseTimeMs": response.response_time_ms,
            "requestId": response.request_id
        });

        // Add content blocks if present
        if let Some(content) = &response.content {
            result["content"] = serde_json::to_value(content).unwrap_or(json!([]));
        }

        // Add tool calls if present
        if let Some(tool_calls) = &response.tool_calls {
            result["toolCalls"] = serde_json::to_value(tool_calls).unwrap_or(json!([]));
        }

        // Add routing info if present
        if let Some(routing) = &response.routing {
            result["routing"] = serde_json::to_value(routing).unwrap_or(json!({}));
        }

        result
    }
}

impl Default for AIProviderModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for AIProviderModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "ai_provider",
            priority: ModulePriority::Normal,
            command_prefixes: &["ai/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            // Local inference adapters fan out into GPU/ORT/llama threadpools.
            // Letting every persona call ai/generate concurrently saturates the
            // machine and lowers throughput. Queue at the runtime boundary; the
            // backend scheduler can batch/serialize work deliberately.
            max_concurrency: 1,
            // DMR watchdog cadence — see DMR_TICK_INTERVAL. The runtime's
            // `start_tick_loops` spawns one tokio task that calls `tick()`
            // on this interval; on every fire we probe DMR and reconcile
            // the registry.
            tick_interval: Some(DMR_TICK_INTERVAL),
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // Store logger for this module
        let _ = self.log.set(ctx.logger("ai_provider"));
        self.register_adapters().await
    }

    /// Watchdog tick — reconcile the registered state of DMR with what's
    /// actually reachable on the wire.
    ///
    /// State machine (each tick is one transition):
    ///
    ///   currently registered   probe up   action
    ///   ───────────────────   ────────   ────────────────────────────────
    ///   true                   true       no-op (steady-state happy path)
    ///   true                   false      DEREGISTER + log warn (Docker
    ///                                     just crashed; subsequent
    ///                                     `select()` will surface the
    ///                                     correct hard error)
    ///   false                  true       REGISTER + log info (Docker
    ///                                     Desktop just came back; reset
    ///                                     the consecutive-down counter)
    ///   false                  false      increment consecutive_down;
    ///                                     log a loud warn at the
    ///                                     30-second threshold so the
    ///                                     situation is diagnosable
    ///
    /// All adapter mutations go through the existing `registry.register`
    /// + new `registry.deregister`. No special-case state on the module
    /// beyond the consecutive-down tick counter.
    async fn tick(&self) -> Result<(), String> {
        let probe = Self::probe_dmr();
        // Reading is_registered first under a read lock keeps the common
        // steady-state path lock-free against the inference path.
        let currently_registered = self.registry.read().await.is_registered(DMR_PROVIDER_ID);

        match (currently_registered, probe) {
            (true, Some(_)) => {
                // Steady-state happy path: DMR is up and registered. Reset
                // the down-counter in case we were transiently flapping
                // (probe failed mid-tick last time but recovered now).
                self.dmr_consecutive_down_ticks.store(0, Ordering::Release);
            }
            (true, None) => {
                // DMR was registered but is no longer reachable. Deregister
                // immediately so the very next inference request fails loud
                // at `select()` instead of at `generate_text` with an
                // arbitrary connection error.
                let mut registry = self.registry.write().await;
                if registry.deregister(DMR_PROVIDER_ID) {
                    self.log().warn(
                        "Docker Model Runner became unreachable — \
                         deregistered. Local AI is unavailable until \
                         Docker Desktop comes back. Watchdog will \
                         re-register automatically.",
                    );
                }
                self.dmr_consecutive_down_ticks
                    .fetch_add(1, Ordering::AcqRel);
            }
            (false, Some(endpoint)) => {
                // Recovery path: Docker Desktop just came back. Build the
                // adapter, INITIALIZE IT (fetch /v1/models to populate the
                // live runtime catalog so supports_model can answer
                // honestly — without this, the freshly-registered adapter
                // returns false for every supports_model query and select()
                // hard-errors even though DMR is back), THEN register.
                //
                // If init fails (DMR is up but the model-list fetch errors
                // — common transient state in the first second after Docker
                // restarts), skip THIS tick and let the next one retry.
                // The adapter stays unregistered until init succeeds, which
                // is the safer state than registering a half-initialized
                // adapter that will silently reject every request.
                let mut adapter = Self::build_dmr_adapter(&endpoint);
                let desc = endpoint
                    .base_url
                    .as_deref()
                    .unwrap_or("localhost:12434 (host-native)");
                if let Err(e) = adapter.initialize().await {
                    self.log().warn(&format!(
                        "DMR is reachable ({desc}) but adapter.initialize() \
                         failed — will retry on next tick. Cause: {e}"
                    ));
                    // Don't increment down-counter: TCP probe succeeded; this
                    // is an init transient. Next tick will see "still false,
                    // probe still up" and re-attempt.
                    return Ok(());
                }
                let mut registry = self.registry.write().await;
                // Priority 1 here mirrors the init-time registration —
                // DMR sits below the in-process llama.cpp adapter so it
                // only wins for models LlamaCppAdapter doesn't claim.
                registry.register(adapter, 1);
                self.log().info(&format!(
                    "Docker Model Runner reachable again — re-registered ({}). \
                     Local AI is available.",
                    desc
                ));
                self.dmr_consecutive_down_ticks.store(0, Ordering::Release);
            }
            (false, None) => {
                // Still down. Escalate to a loud user-visible warning at
                // the 30-second threshold so a stalled Docker Desktop is
                // diagnosable rather than silently degrading every chat
                // turn. After warning, suppress repeats — same threshold
                // re-checked when the counter wraps past 6 multiples.
                let prev = self
                    .dmr_consecutive_down_ticks
                    .fetch_add(1, Ordering::AcqRel);
                let now = prev + 1;
                if now == DMR_DOWN_WARN_THRESHOLD_TICKS {
                    self.log().warn(
                        "Docker Model Runner has been unreachable for ≥30s. \
                         Docker Desktop needs to be running for local AI. \
                         Will keep probing every 5s.",
                    );
                }
            }
        }
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        match command {
            "ai/generate" => {
                let _timer = TimingGuard::new("module", "ai_generate");

                // Parse request
                let request = self.parse_request(&params)?;

                // Get registry
                let registry = self.registry.read().await;

                // Select adapter
                let (provider_id, adapter) = registry
                    .select(
                        request.provider.as_deref(),
                        request.model.as_deref(),
                        InferenceDevice::default(),
                    )
                    .ok_or_else(|| {
                        select_failure_message(
                            &registry,
                            request.provider.as_deref(),
                            request.model.as_deref(),
                        )
                    })?;

                self.log().info(&format!(
                    "Using {} adapter for model {:?}",
                    provider_id, request.model
                ));

                // Generate text
                let mut response = adapter.generate_text(request).await?;

                // Add routing info (preserve adapters_applied from adapter response)
                let prior_routing = response.routing.take();
                response.routing = Some(RoutingInfo {
                    provider: provider_id.to_string(),
                    is_local: adapter.capabilities().is_local,
                    routing_reason: prior_routing
                        .as_ref()
                        .map(|r| r.routing_reason.clone())
                        .unwrap_or_else(|| "adapter_selected".to_string()),
                    adapters_applied: prior_routing
                        .as_ref()
                        .map(|r| r.adapters_applied.clone())
                        .unwrap_or_default(),
                    model_mapped: None,
                    model_requested: prior_routing.and_then(|r| r.model_requested),
                });

                Ok(CommandResult::Json(self.response_to_json(&response)))
            }

            "ai/providers/list" => {
                let registry = self.registry.read().await;
                let available = registry.available();

                // Get all provider info
                let mut providers_info = Vec::new();
                for id in &available {
                    if let Some(adapter) = registry.get(id) {
                        let caps = adapter.capabilities();
                        providers_info.push(json!({
                            "id": id,
                            "name": adapter.name(),
                            "defaultModel": adapter.default_model(),
                            "capabilities": {
                                "textGeneration": caps.supports_text_generation,
                                "chat": caps.supports_chat,
                                "toolUse": caps.supports_tool_use,
                                "vision": caps.supports_vision,
                                "streaming": caps.supports_streaming,
                                "embeddings": caps.supports_embeddings,
                                "isLocal": caps.is_local,
                                "maxContextWindow": caps.max_context_window
                            }
                        }));
                    }
                }

                Ok(CommandResult::Json(json!({
                    "success": true,
                    "available": available,
                    "providers": providers_info,
                    "count": available.len()
                })))
            }

            // Return ModelInfo for a specific provider+model.
            // Called once at persona boot — PRG caches and passes the struct.
            // Eliminates ALL lookup functions (getContextWindow, isSlowLocalModel, etc).
            "ai/model-info" => {
                let p = Params::new(&params);
                let provider = p.str_opt("provider");
                let model = p.str_opt("model");

                let registry = self.registry.read().await;
                let (provider_id, adapter) = registry
                    .select(provider, model, InferenceDevice::default())
                    .ok_or("No adapter available for requested provider/model")?;

                let models = adapter.get_available_models().await;
                let model_name = model.unwrap_or(adapter.default_model());

                // Find exact model or return default
                let info = models
                    .iter()
                    .find(|m| {
                        m.id.to_lowercase().contains(&model_name.to_lowercase())
                            || model_name.to_lowercase().contains(&m.id.to_lowercase())
                    })
                    .or_else(|| models.first());

                match info {
                    Some(model_info) => Ok(CommandResult::Json(json!({
                        "success": true,
                        "provider": provider_id,
                        "modelInfo": serde_json::to_value(model_info).unwrap_or(Value::Null)
                    }))),
                    None => Ok(CommandResult::Json(json!({
                        "success": false,
                        "error": format!("No model info available for {}/{}", provider_id, model_name)
                    }))),
                }
            }

            "ai/providers/health" => {
                let registry = self.registry.read().await;
                let available = registry.available();

                let mut health_results = Vec::new();
                for id in &available {
                    if let Some(adapter) = registry.get(id) {
                        let health = adapter.health_check().await;
                        health_results.push(json!({
                            "provider": id,
                            "name": adapter.name(),
                            "status": format!("{:?}", health.status).to_lowercase(),
                            "apiAvailable": health.api_available,
                            "responseTimeMs": health.response_time_ms,
                            "message": health.message
                        }));
                    }
                }

                Ok(CommandResult::Json(json!({
                    "success": true,
                    "providers": health_results
                })))
            }

            "ai/models/list" => {
                let registry = self.registry.read().await;
                let available = registry.available();

                let mut all_models = Vec::new();
                for id in &available {
                    if let Some(adapter) = registry.get(id) {
                        let models = adapter.get_available_models().await;
                        for model in models {
                            all_models.push(serde_json::to_value(&model).unwrap_or(json!({})));
                        }
                    }
                }

                Ok(CommandResult::Json(json!({
                    "success": true,
                    "models": all_models,
                    "count": all_models.len()
                })))
            }

            "ai/lora/list" => {
                let registry = self.registry.read().await;
                let available = registry.available();

                let mut all_adapters = Vec::new();
                for id in &available {
                    if let Some(adapter) = registry.get(id) {
                        let lora_adapters = adapter.list_lora_adapters();
                        for lora in lora_adapters {
                            all_adapters.push(json!({
                                "provider": id,
                                "adapterId": lora.adapter_id,
                                "path": lora.path,
                                "scale": lora.scale,
                                "loaded": lora.loaded,
                                "active": lora.active
                            }));
                        }
                    }
                }

                Ok(CommandResult::Json(json!({
                    "success": true,
                    "adapters": all_adapters,
                    "count": all_adapters.len()
                })))
            }

            "ai/lora/capabilities" => {
                let registry = self.registry.read().await;
                let available = registry.available();

                let mut capabilities = Vec::new();
                for id in &available {
                    if let Some(adapter) = registry.get(id) {
                        let caps = adapter.lora_capabilities();
                        capabilities.push(json!({
                            "provider": id,
                            "capabilities": format!("{:?}", caps)
                        }));
                    }
                }

                Ok(CommandResult::Json(json!({
                    "success": true,
                    "providers": capabilities
                })))
            }

            _ => {
                // Forward unknown ai/* commands directly to TypeScript via Unix socket.
                // MUST use execute_ts (not execute) to bypass Rust registry — otherwise
                // the registry matches "ai/" prefix back to this module → infinite recursion.
                use crate::runtime::command_executor;
                let log = crate::runtime::logger("ai_provider");
                log.info(&format!(
                    "Forwarding '{}' to TypeScript via Unix socket (bypassing registry)",
                    command
                ));
                command_executor::execute_ts(command, params).await
            }
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ============================================================================
// STANDALONE GENERATE FUNCTION (for internal use by other modules)
// ============================================================================

/// Generate text using the best available provider
/// This is a convenience function for internal use (e.g., AgentModule)
pub async fn generate_text(
    registry: &AdapterRegistry,
    request: TextGenerationRequest,
) -> Result<TextGenerationResponse, String> {
    let (provider_id, adapter) = registry
        .select(
            request.provider.as_deref(),
            request.model.as_deref(),
            InferenceDevice::default(),
        )
        .ok_or_else(|| {
            select_failure_message(
                registry,
                request.provider.as_deref(),
                request.model.as_deref(),
            )
        })?;

    let mut response = adapter.generate_text(request).await?;

    // Add routing info
    response.routing = Some(RoutingInfo {
        provider: provider_id.to_string(),
        is_local: adapter.capabilities().is_local,
        routing_reason: "generate_text_call".to_string(),
        adapters_applied: vec![],
        model_mapped: None,
        model_requested: response
            .routing
            .as_ref()
            .and_then(|r| r.model_requested.clone()),
    });

    Ok(response)
}
