//! HeuristicInferenceAdapter — production-runnable canned/heuristic
//! inference, registered as a peer adapter alongside Anthropic / OpenAI
//! / local Candle.
//!
//! Joel (2026-05-31): "Even if you were afraid of local LLM you could
//! run proxy models, like a fake or canned response like heuristic LLM
//! stand in... I would also make sure the inference command is used.
//! Always should be. Could have this fake model. As an adapter."
//!
//! ### Why it exists
//!
//! Per [[inference-is-an-adapter-always-in-the-loop]], the fake /
//! heuristic adapter is a first-class peer impl, not test scaffolding.
//! It unlocks: (1) headless CI without GGUFs or cloud keys; (2)
//! deterministic replay (same prompt → same response, byte-for-byte);
//! (3) sandbox + demo runs on machines that can't host any LLM; (4)
//! low-end-hardware behavior parity ([[optimizing-for-low-end-
//! compounds-on-high-end]]) when even a small CPU LLM is too heavy.
//!
//! ### Determinism contract
//!
//! Same `(model, messages, system_prompt, temperature, max_tokens)`
//! tuple → same response text, byte-for-byte. Replay relies on this.
//! Implementation: SHA-256 of the canonical prompt → stable response.
//! Adapter does NOT consult clocks, RNGs, or environment.
//!
//! ### What the response looks like
//!
//! `[heuristic:<8-char-hash>] ack: "<last-200-chars-of-last-user-message>"`
//!
//! Enough to prove (a) the inference command surface is wired,
//! (b) the prompt actually reached the adapter, (c) the response is
//! distinct per prompt. NOT enough to be confused with real model
//! output — the `[heuristic:...]` prefix and quoted echo make it
//! unmistakable in logs and traces.
//!
//! ### Doctrine alignment
//!
//! - [[inference-is-an-adapter-always-in-the-loop]] — peer adapter
//!   registered via the canonical AdapterRegistry, callable through
//!   inference/llm/request like any other adapter
//! - [[observability-is-half-the-architecture]] — flows through the
//!   same telemetry as every other adapter; mechanic-grade response
//!   shape (hash + echo) makes "did the prompt reach me?" trivially
//!   answerable
//! - [[substrate-is-a-good-citizen-on-the-host]] — zero hardware
//!   footprint; appropriate for any machine, any environment
//! - [[rust-is-the-core-node-is-the-shell]] — pure-Rust, no Node /
//!   TS / cloud / GPU dependency

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use crate::ai::adapter::{
    AIProviderAdapter, AdapterCapabilities, ApiStyle, InferenceDevice,
};
use crate::ai::types::{
    ChatMessage, ContentPart, CostPer1kTokens, FinishReason, HealthState, HealthStatus,
    MessageContent, ModelInfo, TextGenerationRequest, TextGenerationResponse, UsageMetrics,
};
use crate::model_registry::Capability;

/// Provider ID used to register + select this adapter from the global
/// AdapterRegistry. `Commands.execute('inference/llm/request', {
/// provider: HEURISTIC_PROVIDER_ID, ... })` always routes here.
pub const HEURISTIC_PROVIDER_ID: &str = "heuristic";

/// Default model name. Adapters don't need real model metadata, but
/// the response carries the model field so callers can verify which
/// adapter handled the request.
pub const HEURISTIC_DEFAULT_MODEL: &str = "heuristic-echo-v1";

/// Echo length cap — last N chars of the most recent user message
/// surfaces in the response.
const ECHO_CHARS: usize = 200;

/// Char-to-token ratio (same rough heuristic the rest of the L1 RAG
/// pipeline uses for cost estimation).
const CHARS_PER_TOKEN: usize = 4;

/// The adapter struct itself. No mutable state, no clock access, no
/// external resources — instances are cheap and interchangeable.
///
/// Configuration knobs are all opt-in via builder methods; production
/// callers use `HeuristicInferenceAdapter::new()` and pay zero cost
/// for the unused knobs. Tests, replay rigs, simulated-slow-network
/// scenarios, and warmup-failure substrate diagnostics set them via
/// `.with_*` methods.
///
/// Per [[test-fixtures-are-system-primitives]]: validation behaviors
/// (delay injection, warmup failure, etc.) belong on the production
/// primitive, not as bespoke `#[cfg(test)]` clones. The same struct
/// powers the CI heuristic path, latency-floor regression tests,
/// supervisor warmup-error tests, and any future component that
/// needs a deterministic adapter with controllable timing.
#[derive(Debug, Default)]
pub struct HeuristicInferenceAdapter {
    /// Sleep injected before every `generate_text` returns. 0 (default)
    /// is the production-cheap shape. Setting this is useful for
    /// latency-floor regression tests + simulating slow-network
    /// adapters (e.g., a future cross-grid inference adapter that
    /// pays a real round-trip).
    inject_delay_ms: u64,
    /// If Some, `warmup()` returns Err with this reason. Production
    /// uses None (warmup succeeds with no-op). Tests + diagnostic
    /// substrate paths use this to exercise the
    /// `SupervisorError::AdapterWarmup` typed-failure path.
    warmup_failure: Option<String>,
    /// Optional counter incremented on every `warmup()` call. Shared
    /// `Arc<AtomicUsize>` so a test can register the same counter
    /// across multiple adapters built by a factory and assert "warmup
    /// was called N times across the substrate." Per
    /// [[test-fixtures-are-system-primitives]] this is the observer
    /// hook that lets the supervisor tests verify the
    /// init-once-handle-then-lease contract without resorting to
    /// bespoke FakeAdapter types.
    warmup_observer: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
    /// Same shape for `generate_text` — counts substrate-side hot-path
    /// inference calls so tests can assert per-turn counts.
    generate_observer: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
}

impl HeuristicInferenceAdapter {
    /// Zero-config constructor — what production code uses.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inject a real `tokio::time::sleep` before every `generate_text`
    /// returns. Used by per-turn latency tests to verify the metric
    /// reflects actual wall-clock, and by simulated-network scenarios
    /// to model adapters that pay real round-trip cost.
    pub fn with_delay_ms(mut self, ms: u64) -> Self {
        self.inject_delay_ms = ms;
        self
    }

    /// Make `warmup()` return Err with this reason. Used by supervisor
    /// + service-loop tests to exercise the typed `AdapterWarmup`
    /// failure path per [[no-fallbacks-ever]].
    pub fn with_warmup_failure(mut self, reason: impl Into<String>) -> Self {
        self.warmup_failure = Some(reason.into());
        self
    }

    /// Register a shared counter that increments on every `warmup()`
    /// call. The same `Arc<AtomicUsize>` can be passed to multiple
    /// adapters so tests can assert substrate-wide warmup invocation
    /// counts without bespoke factory state.
    pub fn with_warmup_observer(
        mut self,
        counter: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) -> Self {
        self.warmup_observer = Some(counter);
        self
    }

    /// Register a shared counter that increments on every
    /// `generate_text()` call.
    pub fn with_generate_observer(
        mut self,
        counter: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) -> Self {
        self.generate_observer = Some(counter);
        self
    }

    /// Pull the last user message's text (or "" if absent). Walks
    /// `messages` from the back; first user-role message with text
    /// wins. System prompts and assistant turns are skipped — the
    /// echo is grounded in what the model would actually be asked.
    fn last_user_text(messages: &[ChatMessage]) -> String {
        for msg in messages.iter().rev() {
            if msg.role != "user" {
                continue;
            }
            match &msg.content {
                MessageContent::Text(s) => return s.clone(),
                MessageContent::Parts(parts) => {
                    // Concat text parts in order; ignore non-text
                    // (images, tool results — those need their own
                    // peer adapters per [[ai-namespace-multimodal-
                    // crutches]]).
                    let mut buf = String::new();
                    for part in parts {
                        if let ContentPart::Text { text } = part {
                            if !buf.is_empty() {
                                buf.push(' ');
                            }
                            buf.push_str(text);
                        }
                    }
                    if !buf.is_empty() {
                        return buf;
                    }
                }
            }
        }
        String::new()
    }

    /// Compute a deterministic 8-char hex prefix tying the response
    /// to its inputs. Same canonical inputs → same hash → same
    /// response text. This is the replay contract.
    fn determinism_prefix(req: &TextGenerationRequest) -> String {
        let mut hasher = Sha256::new();
        if let Some(model) = &req.model {
            hasher.update(b"model=");
            hasher.update(model.as_bytes());
            hasher.update(b"\n");
        }
        if let Some(sys) = &req.system_prompt {
            hasher.update(b"system=");
            hasher.update(sys.as_bytes());
            hasher.update(b"\n");
        }
        if let Some(t) = req.temperature {
            hasher.update(format!("temperature={t}\n").as_bytes());
        }
        if let Some(m) = req.max_tokens {
            hasher.update(format!("max_tokens={m}\n").as_bytes());
        }
        for (i, msg) in req.messages.iter().enumerate() {
            hasher.update(format!("msg[{i}].role={}\n", msg.role).as_bytes());
            match &msg.content {
                MessageContent::Text(s) => {
                    hasher.update(b"msg.text=");
                    hasher.update(s.as_bytes());
                    hasher.update(b"\n");
                }
                MessageContent::Parts(parts) => {
                    for (j, p) in parts.iter().enumerate() {
                        if let ContentPart::Text { text } = p {
                            hasher.update(format!("msg[{i}].part[{j}].text=").as_bytes());
                            hasher.update(text.as_bytes());
                            hasher.update(b"\n");
                        }
                    }
                }
            }
        }
        let digest = hasher.finalize();
        let hex: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
        hex
    }

    fn estimate_tokens(text: &str) -> u32 {
        ((text.chars().count() / CHARS_PER_TOKEN) as u32).saturating_add(1)
    }

    /// Build the response text from the request. Pure function —
    /// no I/O, no clock, no RNG. Replay-safe.
    ///
    /// When the request asks for JSON-shaped output
    /// (`response_format = JsonObject`), the heuristic wraps its
    /// echo in the substrate's persona-cognition contract:
    /// `{"will_respond": true, "response": "<echo>"}`. This lets the
    /// test path through `rag_inspect::run_inference_probe` succeed
    /// against a heuristic adapter — substrate plumbing still
    /// validates end-to-end without a real LLM, per the
    /// system-test-primitives doctrine. The real cognition
    /// (will_respond chosen by the LLM) requires a real model
    /// per Joel: "use real LLMs. We can't know if we use fake
    /// algorithms."
    pub fn build_response_text(req: &TextGenerationRequest) -> String {
        let prefix = Self::determinism_prefix(req);
        let last = Self::last_user_text(&req.messages);
        let echoed: String = last.chars().rev().take(ECHO_CHARS).collect::<String>()
            .chars().rev().collect();
        let plain = if echoed.is_empty() {
            format!("[heuristic:{prefix}] ack: (no user text in prompt)")
        } else {
            format!("[heuristic:{prefix}] ack: \"{echoed}\"")
        };
        if matches!(
            req.response_format,
            Some(crate::ai::types::ResponseFormat::JsonObject)
        ) {
            // Emit the substrate's decide-and-respond JSON shape so
            // the rag_inspect inference probe's JSON parser is
            // exercised end-to-end. `will_respond: true` keeps the
            // happy path going.
            let inner =
                serde_json::to_string(&plain).expect("plain string serializes");
            return format!(
                "{{\"will_respond\":true,\"response\":{inner}}}"
            );
        }
        plain
    }
}

#[async_trait]
impl AIProviderAdapter for HeuristicInferenceAdapter {
    fn provider_id(&self) -> &str {
        HEURISTIC_PROVIDER_ID
    }

    fn name(&self) -> &str {
        "Heuristic (deterministic stand-in)"
    }

    /// **NOT** production-capable. Heuristic outputs are deterministic
    /// canned responses — not real cognition. Per [[no-fallbacks-ever]]
    /// and [[no-if-statements-use-llms-for-cognition]], heuristic is
    /// also gated behind `cfg(any(test, feature = "test-fixtures"))`
    /// at the module level so production binaries cannot link it at
    /// all; this trait flag is belt-and-suspenders for test-context
    /// selectors that want to distinguish real-cognition adapters from
    /// fixtures.
    fn is_production_capable(&self) -> bool {
        false
    }


    fn capabilities(&self) -> AdapterCapabilities {
        // Heuristic adapter intentionally advertises only text I/O — tool
        // use, vision, embeddings, etc. are peer-adapter territory (per
        // [[ai-namespace-multimodal-crutches]]). A future
        // HeuristicVisionAdapter / HeuristicEmbeddingAdapter would each add
        // its capability to this set.
        AdapterCapabilities {
            // Local in the "no network, no GPU" sense.
            is_local: true,
            // Effectively unlimited — we never reject by length.
            max_context_window: Some(u32::MAX),
            max_output_tokens: Some(4096),
            // Deterministic text-only adapter — no protocols beyond text I/O.
            tool_call_protocol: crate::model_registry::ToolProtocol::None,
            structured_output_protocol: crate::ai::adapter::StructuredOutputProtocol::None,
            ..AdapterCapabilities::text_only()
        }
    }

    fn api_style(&self) -> ApiStyle {
        ApiStyle::Local
    }

    fn default_model(&self) -> &str {
        HEURISTIC_DEFAULT_MODEL
    }

    async fn initialize(&mut self) -> Result<(), String> {
        Ok(())
    }

    async fn warmup(&self) -> Result<(), String> {
        // Observer fires before the failure check so tests can assert
        // "warmup was attempted" independent of "warmup succeeded."
        if let Some(c) = &self.warmup_observer {
            c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        // Production: succeeds with no-op (default `warmup_failure: None`).
        // Test / diagnostic: caller used `.with_warmup_failure(reason)`
        // — return Err with that reason so the supervisor surfaces
        // `SupervisorError::AdapterWarmup` per [[no-fallbacks-ever]].
        if let Some(reason) = &self.warmup_failure {
            return Err(reason.clone());
        }
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), String> {
        Ok(())
    }

    async fn generate_text(
        &self,
        request: TextGenerationRequest,
    ) -> Result<TextGenerationResponse, String> {
        // Observer fires for substrate-side hot-path inference call
        // counts.
        if let Some(c) = &self.generate_observer {
            c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        // Inject real wall-clock if the caller configured a delay. Used
        // by latency-floor regression tests to verify the substrate's
        // turn_latency metric reflects actual elapsed time, and by
        // future simulated-network adapters. Production callers use
        // `new()` with delay=0 and pay zero overhead.
        if self.inject_delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.inject_delay_ms))
                .await;
        }
        let model = request
            .model
            .clone()
            .unwrap_or_else(|| HEURISTIC_DEFAULT_MODEL.to_string());
        let text = Self::build_response_text(&request);

        // Token accounting: input = system + all message text;
        // output = response text. Same chars/4 heuristic the rest
        // of the L1 RAG pipeline uses.
        let mut input_chars: usize = 0;
        if let Some(sys) = &request.system_prompt {
            input_chars += sys.chars().count();
        }
        for msg in &request.messages {
            match &msg.content {
                MessageContent::Text(s) => input_chars += s.chars().count(),
                MessageContent::Parts(parts) => {
                    for p in parts {
                        if let ContentPart::Text { text } = p {
                            input_chars += text.chars().count();
                        }
                    }
                }
            }
        }
        let input_tokens = ((input_chars / CHARS_PER_TOKEN) as u32).saturating_add(1);
        let output_tokens = Self::estimate_tokens(&text);

        let request_id = request
            .request_id
            .clone()
            .unwrap_or_else(|| format!("heuristic-{}", Self::determinism_prefix(&request)));

        Ok(TextGenerationResponse {
            text,
            finish_reason: FinishReason::Stop,
            model,
            provider: HEURISTIC_PROVIDER_ID.to_string(),
            usage: UsageMetrics {
                input_tokens,
                output_tokens,
                total_tokens: input_tokens.saturating_add(output_tokens),
                estimated_cost: Some(0.0),
            },
            // response_time_ms is non-zero on real adapters; we
            // report 0 (the response is computed synchronously
            // from a hash — there's no meaningful latency).
            response_time_ms: 0,
            request_id,
            content: None,
            tool_calls: None,
            reasoning: None,
            routing: None,
            error: None,
            timing: None,
        })
    }

    async fn health_check(&self) -> HealthStatus {
        HealthStatus {
            status: HealthState::Healthy,
            api_available: true,
            response_time_ms: 0,
            error_rate: 0.0,
            last_checked: 0,
            message: Some(
                "heuristic adapter — always available, deterministic, zero cost".to_string(),
            ),
        }
    }

    async fn get_available_models(&self) -> Vec<ModelInfo> {
        // One canonical model. Listed so registry consumers can see
        // it; the adapter accepts any model name in practice.
        vec![ModelInfo {
            id: HEURISTIC_DEFAULT_MODEL.to_string(),
            name: "Heuristic Echo v1".to_string(),
            provider: HEURISTIC_PROVIDER_ID.to_string(),
            capabilities: vec![Capability::TextGeneration, Capability::Chat],
            context_window: u32::MAX,
            max_output_tokens: 4_096,
            cost_per_1k_tokens: CostPer1kTokens {
                input: 0.0,
                output: 0.0,
            },
            tokens_per_second: 1_000_000.0,
        }]
    }

    fn device_type(&self) -> InferenceDevice {
        InferenceDevice::Cpu
    }

    /// Declared model prefix: ONLY model names starting with
    /// `"heuristic"` resolve here. The substrate uses real model names
    /// like `qwen2.5-7b`, `claude-sonnet`, `deepseek-coder-1.3b`, etc.
    /// — none of which match. Combined with `is_production_capable() =
    /// false` and the cfg-gated module, this is a third structural
    /// barrier against auto-discovery: even at test time, a caller
    /// that asks for a real model by name never lands here.
    ///
    /// Joel (2026-06-01): "The fake shit is a CHOSEN model adapter no
    /// other form. Declaration." This IS the declaration.
    fn supported_model_prefixes(&self) -> Vec<&'static str> {
        vec!["heuristic"]
    }

    /// Strict opt-in only. The previous implementation returned `true`
    /// for any model name — which was THE leak path: a caller passing
    /// `model = Some("qwen2.5-7b")` would route to heuristic if no real
    /// adapter was registered first. Now: heuristic responds only to
    /// model names that explicitly start with `"heuristic"`. Production
    /// model names never match. Per Joel (2026-06-01): "The fake shit
    /// is a CHOSEN model adapter no other form."
    fn supports_model(&self, model_name: &str) -> bool {
        model_name.to_lowercase().starts_with("heuristic")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::adapter::AdapterRegistry;
    use crate::ai::types::ChatMessage;

    fn msg(role: &str, text: &str) -> ChatMessage {
        ChatMessage {
            role: role.to_string(),
            content: MessageContent::Text(text.to_string()),
            name: None,
        }
    }
    fn user_msg(text: &str) -> ChatMessage {
        msg("user", text)
    }
    fn system_msg(text: &str) -> ChatMessage {
        msg("system", text)
    }
    fn assistant_msg(text: &str) -> ChatMessage {
        msg("assistant", text)
    }

    fn req_with(messages: Vec<ChatMessage>) -> TextGenerationRequest {
        TextGenerationRequest {
            messages,
            system_prompt: None,
            model: Some(HEURISTIC_DEFAULT_MODEL.to_string()),
            provider: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            top_k: None,
            repeat_penalty: None,
            frequency_penalty: None,
            repeat_last_n: None,
            stop_sequences: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            active_adapters: None,
            request_id: None,
            user_id: None,
            room_id: None,
            purpose: None,
            persona_id: None,
        }
    }

    #[tokio::test]
    async fn same_prompt_yields_byte_identical_response_text() {
        let adapter = HeuristicInferenceAdapter::new();
        let req_a = req_with(vec![user_msg("hello world")]);
        let req_b = req_with(vec![user_msg("hello world")]);
        let resp_a = adapter.generate_text(req_a).await.unwrap();
        let resp_b = adapter.generate_text(req_b).await.unwrap();
        assert_eq!(
            resp_a.text, resp_b.text,
            "determinism contract: same prompt → identical text"
        );
    }

    #[tokio::test]
    async fn different_prompts_yield_different_response_text() {
        let adapter = HeuristicInferenceAdapter::new();
        let resp_a = adapter
            .generate_text(req_with(vec![user_msg("alpha")]))
            .await
            .unwrap();
        let resp_b = adapter
            .generate_text(req_with(vec![user_msg("beta")]))
            .await
            .unwrap();
        assert_ne!(resp_a.text, resp_b.text);
    }

    #[tokio::test]
    async fn response_echoes_last_user_message_with_heuristic_prefix() {
        let adapter = HeuristicInferenceAdapter::new();
        let resp = adapter
            .generate_text(req_with(vec![
                system_msg("you are nice"),
                user_msg("first question"),
                assistant_msg("first answer"),
                user_msg("second question — please answer this"),
            ]))
            .await
            .unwrap();
        assert!(
            resp.text.contains("second question — please answer this"),
            "must echo the LATEST user message, got: {}",
            resp.text
        );
        assert!(
            resp.text.starts_with("[heuristic:"),
            "must carry the heuristic prefix, got: {}",
            resp.text
        );
    }

    #[tokio::test]
    async fn no_user_message_still_produces_marker_response() {
        let adapter = HeuristicInferenceAdapter::new();
        let resp = adapter
            .generate_text(req_with(vec![system_msg("system only, no user")]))
            .await
            .unwrap();
        assert!(resp.text.contains("(no user text in prompt)"));
        assert!(resp.text.starts_with("[heuristic:"));
    }

    #[tokio::test]
    async fn finish_reason_is_stop_for_every_request() {
        let adapter = HeuristicInferenceAdapter::new();
        let resp = adapter
            .generate_text(req_with(vec![user_msg("anything")]))
            .await
            .unwrap();
        assert_eq!(resp.finish_reason, FinishReason::Stop);
    }

    #[tokio::test]
    async fn usage_metrics_are_populated_and_nonzero_for_nonempty_prompt() {
        let adapter = HeuristicInferenceAdapter::new();
        let resp = adapter
            .generate_text(req_with(vec![user_msg("a long-ish prompt here for token estimation")]))
            .await
            .unwrap();
        assert!(resp.usage.input_tokens > 0);
        assert!(resp.usage.output_tokens > 0);
        assert_eq!(
            resp.usage.total_tokens,
            resp.usage.input_tokens + resp.usage.output_tokens
        );
    }

    #[tokio::test]
    async fn provider_field_in_response_matches_provider_id_constant() {
        let adapter = HeuristicInferenceAdapter::new();
        let resp = adapter
            .generate_text(req_with(vec![user_msg("hi")]))
            .await
            .unwrap();
        assert_eq!(resp.provider, HEURISTIC_PROVIDER_ID);
    }

    #[tokio::test]
    async fn registers_and_round_trips_through_AdapterRegistry() {
        let mut registry = AdapterRegistry::new();
        registry.register(std::sync::Arc::new(HeuristicInferenceAdapter::new()), 99);
        assert!(registry.is_registered(HEURISTIC_PROVIDER_ID));
        let available = registry.available();
        assert!(available.contains(&HEURISTIC_PROVIDER_ID));
    }

    #[tokio::test]
    async fn health_check_reports_healthy() {
        let adapter = HeuristicInferenceAdapter::new();
        let h = adapter.health_check().await;
        assert!(matches!(h.status, HealthState::Healthy));
        assert!(h.api_available);
    }

    #[tokio::test]
    async fn capabilities_admit_text_chat_but_not_modality_specific() {
        let adapter = HeuristicInferenceAdapter::new();
        let caps = adapter.capabilities();
        assert!(caps.has(Capability::TextGeneration));
        assert!(caps.has(Capability::Chat));
        assert!(!caps.has(Capability::ToolUse));
        assert!(!caps.has(Capability::Vision));
        assert!(!caps.has(Capability::Embedding));
        assert!(caps.is_local);
    }

    /// Strict model match — heuristic ONLY responds to model names that
    /// explicitly start with `"heuristic"`. The previous test asserted
    /// the OPPOSITE (heuristic accepted any model name including real
    /// production IDs like "anthropic/claude-opus-4-7"), and that was
    /// the silent-substitution path Joel called out (2026-06-01: "You
    /// mix this fake shit in and it's going live ALL THE TIME"). Per
    /// [[no-fallbacks-ever]] + [[no-if-statements-use-llms-for-cognition]],
    /// heuristic is a CHOSEN adapter — callers must pass an explicit
    /// `heuristic-*` model name or `provider = "heuristic"`.
    #[tokio::test]
    async fn supports_only_heuristic_model_names_never_substitutes_for_real_models() {
        let adapter = HeuristicInferenceAdapter::new();
        // Explicit heuristic model names: yes.
        assert!(adapter.supports_model("heuristic"));
        assert!(adapter.supports_model("heuristic-echo-v1"));
        assert!(adapter.supports_model("Heuristic-Test"));
        // Real production model names: NEVER.
        assert!(!adapter.supports_model("anthropic/claude-opus-4-7"));
        assert!(!adapter.supports_model("gpt-4"));
        assert!(!adapter.supports_model("qwen3.5-4b-code-forged-Q4_K_M"));
        assert!(!adapter.supports_model("some-future-model"));
    }

    /// The slice-completing test: drive the heuristic adapter
    /// through the REAL `inference/llm/request` ServiceModule path,
    /// proving the canonical command surface routes to it. This is
    /// what makes "every persona/sentinel/test/CI/replay path goes
    /// through the inference command" actually true per
    /// [[inference-is-an-adapter-always-in-the-loop]].
    #[tokio::test]
    async fn routes_through_inference_llm_request_command_surface() {
        use crate::genome::working_set::ArtifactId;
        use crate::identity::PeerId;
        use crate::inference::llm_module::{
            CompositionPlan, GenerationBudget, InferenceRequest, InferenceRequestId,
            SamplingParams,
        };
        use crate::inference::llm_module_service::{InferenceLlmModule, COMMAND_REQUEST};
        use crate::runtime::service_module::{CommandResult, ServiceModule};
        use std::sync::Arc;
        use uuid::Uuid;

        let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
        let module = InferenceLlmModule::with_adapter(adapter);

        let request = InferenceRequest {
            request_id: InferenceRequestId::new(Uuid::from_u128(7)),
            persona: PeerId::from_uuid(Uuid::from_u128(8)),
            composition: CompositionPlan(ArtifactId::new(Uuid::from_u128(9))),
            prompt_tokens: vec![],
            prompt_text: Some("integration prompt for heuristic adapter".to_string()),
            budget: GenerationBudget {
                max_tokens: 100,
                max_duration_ms: 5_000,
            },
            sampling: SamplingParams::default(),
            stop_sequences: vec![],
        };
        let params = serde_json::to_value(&request).unwrap();
        let result = module
            .handle_command(COMMAND_REQUEST, params)
            .await
            .expect("inference/llm/request must route to heuristic adapter");

        match result {
            CommandResult::Json(v) => {
                let response = v.as_object().expect("InferenceResponse is an object");
                let complete = response
                    .get("complete")
                    .expect("response.complete present")
                    .as_object()
                    .unwrap();
                let completion_text = complete
                    .get("completionText")
                    .and_then(|v| v.as_str())
                    .expect("heuristic adapter populates completionText");
                assert!(
                    completion_text.starts_with("[heuristic:"),
                    "must be the heuristic adapter's output, got: {completion_text}"
                );
                assert!(
                    completion_text.contains("integration prompt for heuristic adapter"),
                    "must echo the prompt, got: {completion_text}"
                );
            }
            other => panic!("expected CommandResult::Json, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn temperature_and_max_tokens_change_response_deterministic_prefix() {
        let adapter = HeuristicInferenceAdapter::new();
        let mut req_a = req_with(vec![user_msg("same prompt text")]);
        let mut req_b = req_with(vec![user_msg("same prompt text")]);
        req_a.temperature = Some(0.0);
        req_b.temperature = Some(0.9);
        let resp_a = adapter.generate_text(req_a).await.unwrap();
        let resp_b = adapter.generate_text(req_b).await.unwrap();
        assert_ne!(
            resp_a.text, resp_b.text,
            "different sampling params should change the determinism prefix"
        );
    }
}
