//! The `AIProviderAdapter` implementation for `OpenAICompatibleAdapter`.

use super::embedding::{build_embedding_body, parse_embedding_response, parse_embedding_usage};
use super::*;

// `apply_no_think_switch` / `close_trailing_assistant` live in `crate::inference::request_body` (S3b decompose).

// `apply_enable_thinking_false` lives in `crate::inference::request_body` (S3b decompose).

#[async_trait]
impl AIProviderAdapter for OpenAICompatibleAdapter {
    fn provider_id(&self) -> &str {
        &self.config.provider_id
    }

    /// The provider declares the family (our fork's gateway); the engine the request for `model`
    /// actually reaches decides, by the build that engine itself reports (`route_now`).
    fn accepts_tail_tools(&self, model: &str) -> bool {
        self.route_now(model, &self.endpoints()).tail_tools
    }

    /// The live set the daemon's reconcile / `/v1/models` refresh verified
    /// (`runtime_models`), sorted for stable output; the catalog default only
    /// when nothing has been verified yet. `config.default_model` is a
    /// derived catalog value and can misname the lane (5090 2026-07-24), so
    /// a refusal must not point callers at it when the truth is known.
    fn served_model_ids(&self) -> Vec<String> {
        let guard = self.runtime_models.read().unwrap();
        match guard.as_ref().filter(|set| !set.is_empty()) {
            Some(set) => {
                let mut ids: Vec<String> = set.iter().cloned().collect();
                ids.sort();
                ids
            }
            None => vec![self.config.default_model.clone()],
        }
    }

    fn generation_admission(&self) -> crate::ai::adapter::GenerationAdmissionOwner {
        self.generation_admission_for(&crate::inference::llama_server::current_serving().base_url)
    }

    fn name(&self) -> &str {
        &self.config.name
    }

    /// Restore the next page during prompt assembly through the same
    /// admission and paging owner as generation. Only a completed restore or
    /// existing physical residency is warm; failure leaves the turn backstop
    /// responsible for restoring or prefilling normally.
    fn warm_ahead(&self, key: crate::inference::slots::PageKey) {
        let root = self.endpoints().root().to_string();
        // Discovery is the pin path's job — warm-ahead only acts on servers
        // whose slot pool is already installed (every server after its first
        // real turn), so it can never race the /props probe or mis-latch it.
        let Some(Some(pool)) = crate::inference::slots::directory().get(&root) else {
            return;
        };
        if pool.scratch_slot().is_none() {
            // Without reserved scratch, already-admitted keyless traffic can
            // select any physical slot and bypass speculative paging admission.
            // Said, not silent: the restore then waits for the turn itself (the M5
            // serves two citizen slots and no scratch, so this is every act there).
            continuum_base::probe!(
                class = "inference.kv_warm_ahead.skipped",
                page = %key,
                reason = "no_scratch_slot",
                "warm-ahead skipped: this server has no scratch slot, so the page restore runs at admission instead of during prompt assembly"
            );
            return;
        }
        let client = self.client.clone();
        let concurrency = self.concurrency.clone();
        tokio::spawn(async move {
            let _ = crate::inference::turn_admission::warm_ahead(
                &concurrency,
                key,
                pool,
                &client,
                &root,
            )
            .await;
        });
    }

    fn capabilities(&self) -> AdapterCapabilities {
        // The capability set is already projected from the registry at
        // construction (#65) — the adapter just hands it through, adding the
        // scalar/protocol axes. Tool-use drives the native function-calling +
        // JSON-Schema protocols; vision/embeddings/image-gen ride in the set
        // itself, and any modality absent from the set is bridged (vision →
        // VisionDescriptionService, audio → STT/TTS) before the request lands.
        let supports_tools = self.config.capabilities.contains(&Capability::ToolUse);
        AdapterCapabilities::builder()
            .capabilities(self.config.capabilities.iter().copied())
            .remote()
            // Sourced from the served model's declared ceiling (#46) — never a
            // hardcoded per-adapter clamp. The fallbacks only apply when no
            // model row is present (a mis-provisioned adapter), which fails
            // loud downstream anyway.
            .context_window(
                self.config
                    .models
                    .first()
                    .map(|m| m.context_window)
                    .unwrap_or(128_000),
            )
            .max_output_tokens(
                self.config
                    .models
                    .first()
                    .map(|m| m.max_output_tokens)
                    .unwrap_or(16_384),
            )
            // Native function-calling + JSON-Schema when the served model does
            // tools; otherwise the model is a competent chat model and cognition
            // emulates tools/schema in-prompt.
            .protocols(if supports_tools {
                crate::ai::adapter::NativeProtocols::FunctionCalling
            } else {
                crate::ai::adapter::NativeProtocols::PromptEmulated
            })
            .build()
    }

    fn api_style(&self) -> ApiStyle {
        ApiStyle::OpenAI
    }

    /// The live served window of the lane THIS adapter is bound to (the one source
    /// cognition sizes its prompt to). A DEDICATED lane (`with_dedicated_lane`, an
    /// eval fork's `EphemeralServingLane`) is its own authority — its window was
    /// pinned from ITS `/props` at spawn and rides on the binding, so report `None`
    /// and let that stand; the GLOBAL gateway snapshot describes a DIFFERENT server.
    /// A shared single-resident gateway reports the gateway's CURRENT served slot
    /// (the live `/props` truth), tracked up AND down, so a relaunch is followed
    /// without a clamp. A not-ready / zero snapshot → `None` (the binding window
    /// stands until the next ready tick). Mirrors the `!self.dedicated_lane`
    /// readiness-guard exemption above — same lane, same authority.
    fn live_served_window(&self) -> Option<u32> {
        if self.dedicated_lane || !self.config.single_resident_model {
            return None;
        }
        let s = crate::inference::llama_server::current_serving();
        (s.ready && s.served_context_window > 0).then_some(s.served_context_window)
    }

    fn capacity_changed(
        &self,
        observed: Option<u32>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        if self.dedicated_lane || !self.config.single_resident_model {
            return Box::pin(std::future::pending());
        }
        let Some(mut changes) = crate::inference::llama_server::serving_state_receiver() else {
            return Box::pin(std::future::pending());
        };
        Box::pin(async move {
            loop {
                // Subscribe first; a publish between this read and changed is retained.
                if self.live_served_window() != observed {
                    return;
                }
                if changes.changed().await.is_err() {
                    return std::future::pending::<()>().await;
                }
            }
        })
    }

    /// Reports the LoRA capability the endpoint DISCOVERED about itself (via the
    /// `GET /lora-adapters` probe), never a declared per-provider table. Reads
    /// the cached probe result: `Supported` only after a 200 catalog response,
    /// so the fabric's capability-aware selection sees the truth the endpoint
    /// told us — not a guess. `Unknown`/`Unsupported` → `None`.
    fn lora_capabilities(&self) -> LoRACapabilities {
        match &*self.lora_support.read().unwrap() {
            LoraSupport::Supported(catalog) => LoRACapabilities::MultiLayerPaging {
                max_loaded: catalog.len(),
                supports_hot_swap: true,
            },
            _ => LoRACapabilities::None,
        }
    }

    fn default_model(&self) -> &str {
        &self.config.default_model
    }

    fn model_metadata(&self, model_id: &str) -> Option<ModelInfo> {
        self.config.models.iter().find(|model| model.id == model_id).cloned()
    }

    async fn initialize(&mut self) -> Result<(), String> {
        // The decode permit tracks the LIVE served slot count (B3): grow adds
        // permits the moment a relaunch serves more slots; shrink retires them
        // as in-flight generations finish. One task per adapter, canonical
        // watch-receiver shape — no polling, no locks across await.
        if self.config.single_resident_model
            && !self
                .permit_reconciler_started
                .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            if let Some(mut rx) = crate::inference::llama_server::serving_state_receiver() {
                let sem = self.concurrency.clone();
                let issued = self.permit_target.clone();
                tokio::spawn(async move {
                    loop {
                        if rx.changed().await.is_err() {
                            return; // serving sender gone — process teardown
                        }
                        let target = { rx.borrow().lanes.max(1) as usize };
                        let cur = issued.load(std::sync::atomic::Ordering::SeqCst);
                        if target > cur {
                            sem.add_permits(target - cur);
                            issued.store(target, std::sync::atomic::Ordering::SeqCst);
                            continuum_base::probe!(
                                class = "inference.permits.grown",
                                from = cur as u64,
                                to = target as u64,
                                "decode permits grew to the served slot count"
                            );
                        } else if target < cur {
                            // Retire the surplus as generations finish — a
                            // smaller relaunch is never over-admitted, and a
                            // long drain re-checks the latest snapshot on the
                            // next loop pass.
                            for _ in 0..(cur - target) {
                                match sem.acquire().await {
                                    Ok(permit) => permit.forget(),
                                    Err(_) => return,
                                }
                            }
                            issued.store(target, std::sync::atomic::Ordering::SeqCst);
                            continuum_base::probe!(
                                class = "inference.permits.shrunk",
                                from = cur as u64,
                                to = target as u64,
                                "decode permits retired down to the served slot count"
                            );
                        }
                    }
                });
            }
        }
        // Only require API key if provider needs auth. Providers without
        // an `api_key_env` in the catalog (localhost DMR, llamacpp-local) skip
        // this entirely — their `requires_auth` is false.
        if self.config.requires_auth {
            let key_env = self.config.api_key_env.as_deref().unwrap_or_else(|| {
                panic!(
                    "provider `{}` requires auth but has no api_key_env in the catalog",
                    self.config.provider_id
                )
            });
            self.api_key = get_secret(key_env).map(|s| s.to_string());
            if self.api_key.is_none() {
                return Err(format!(
                    "{} API key not configured ({})",
                    self.config.name, key_env
                ));
            }
        }

        self.initialized = true;

        // Populate runtime_models for adapters with dynamic catalogs (DMR).
        // Best-effort: if the endpoint isn't reachable right now, init still
        // succeeds — runtime_models stays None → supports_model returns false
        // → registry hard-errors instead of silently routing to this adapter.
        // That's the correct failure mode: don't falsely claim availability.
        // Gated on the TYPED capability (#55), never the provider id.
        if self.config.dynamic_model_catalog {
            if let Err(e) = self.refresh_runtime_models().await {
                clog_warn!(
                    "DMR model catalog fetch failed at init: {}. DMR will report no models available until a successful refresh.",
                    e
                );
            } else {
                let count = self
                    .runtime_models
                    .read()
                    .unwrap()
                    .as_ref()
                    .map(|s| s.len())
                    .unwrap_or(0);
                clog_info!("DMR live model catalog: {} model(s) available", count);
            }
        }

        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), String> {
        self.initialized = false;
        Ok(())
    }

    /// Convenience drain over [`generate_stream`]: when a caller wants the whole
    /// answer (no live tokens), the shared drain consumes chunks as they arrive
    /// and returns the assembled response. Same SSE path and liveness watchdog.
    async fn generate_text(
        &self,
        request: TextGenerationRequest,
    ) -> Result<TextGenerationResponse, String> {
        crate::ai::stream_sinks::drain_generation(|sink| self.generate_stream(request, sink)).await
    }

    /// Streaming-first generation: each token reaches `sink` the INSTANT the
    /// backend decodes it (the low-latency primitive — same shape as audio samples
    /// or video frames), and the fully-assembled [`TextGenerationResponse`] is
    /// returned when the stream completes. Liveness is the per-token idle watchdog
    /// ([`STREAM_IDLE_TIMEOUT_SECS`]), never a wall-clock total.
    async fn generate_stream(
        &self,
        request: TextGenerationRequest,
        sink: tokio::sync::mpsc::UnboundedSender<GenerationChunk>,
    ) -> Result<TextGenerationResponse, String> {
        self.generate_stream_checked(request, sink)
            .await
            .map_err(|error| error.to_string())
    }

    async fn generate_stream_checked(
        &self,
        request: TextGenerationRequest,
        sink: tokio::sync::mpsc::UnboundedSender<GenerationChunk>,
    ) -> Result<TextGenerationResponse, crate::ai::inference_error::InferenceError> {
        self.stream_checked(request, sink).await
    }

    /// Create embeddings over the OpenAI-compatible `/v1/embeddings` endpoint.
    /// This is the path continuum's neural recall (`NeuralEmbeddingProvider`)
    /// takes through the local llama-server /v1 gateway — it replaces the in-process
    /// fastembed/ONNX embedder. Degrades to an `Err` (never panics) when the
    /// endpoint is unreachable or the model isn't an embedding model; the caller
    /// falls back to the lexical embedder.
    async fn create_embedding(
        &self,
        request: EmbeddingRequest,
    ) -> Result<EmbeddingResponse, String> {
        let start = Instant::now();

        // The model is the embedding SPACE identity — no silent default among
        // chat models ([[no-fallbacks-ever]]). NeuralEmbeddingProvider always
        // pins the canonical embedding slug; a None here is a config error.
        let model = request.model.clone().ok_or_else(|| {
            format!(
                "{} embeddings require an explicit model (the embedding-space identity)",
                self.config.name
            )
        })?;

        let body = build_embedding_body(&request.input, &model);

        let url = self.endpoints().embeddings();

        let mut request_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");
        if self.config.requires_auth {
            if let Some(api_key) = &self.api_key {
                request_builder =
                    request_builder.header("Authorization", format!("Bearer {api_key}"));
            }
        }

        let response = request_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("{} embeddings POST failed: {e}", self.config.name))?;

        let status = response.status();
        if !status.is_success() {
            let err_body = response.text().await.unwrap_or_default();
            return Err(format!(
                "{} /v1/embeddings {status}: {err_body}",
                self.config.name
            ));
        }

        let json: Value = response
            .json()
            .await
            .map_err(|e| format!("{} embeddings response parse failed: {e}", self.config.name))?;

        let embeddings = parse_embedding_response(&json)?;
        let usage = parse_embedding_usage(&json);

        Ok(EmbeddingResponse {
            embeddings,
            model,
            provider: self.config.provider_id.clone(),
            usage,
            response_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn health_check(&self) -> HealthStatus {
        // Only require API key if provider needs auth
        if self.config.requires_auth && self.api_key.is_none() {
            return HealthStatus {
                status: HealthState::Unhealthy,
                api_available: false,
                response_time_ms: 0,
                error_rate: 1.0,
                last_checked: continuum_base::time::EpochMs::now().millis(),
                message: Some(format!("{} API key not configured", self.config.name)),
            };
        }

        let start = Instant::now();

        // Try to list models as health check
        let url = self.endpoints().models();

        let mut request_builder = self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs(5));

        // Only add Authorization header if provider requires auth
        if self.config.requires_auth {
            if let Some(api_key) = &self.api_key {
                request_builder =
                    request_builder.header("Authorization", format!("Bearer {}", api_key));
            }
        }

        let result = request_builder.send().await;

        let response_time_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(resp) if resp.status().is_success() => HealthStatus {
                status: HealthState::Healthy,
                api_available: true,
                response_time_ms,
                error_rate: 0.0,
                last_checked: continuum_base::time::EpochMs::now().millis(),
                message: Some(format!("{} API is accessible", self.config.name)),
            },
            Ok(resp) => HealthStatus {
                status: HealthState::Unhealthy,
                api_available: false,
                response_time_ms,
                error_rate: 1.0,
                last_checked: continuum_base::time::EpochMs::now().millis(),
                message: Some(format!("{} returned {}", self.config.name, resp.status())),
            },
            Err(e) => HealthStatus {
                status: HealthState::Unhealthy,
                api_available: false,
                response_time_ms,
                error_rate: 1.0,
                last_checked: continuum_base::time::EpochMs::now().millis(),
                message: Some(format!("{} error: {}", self.config.name, e)),
            },
        }
    }

    async fn get_available_models(&self) -> Vec<ModelInfo> {
        self.config.models.clone()
    }

    fn supported_model_prefixes(&self) -> Vec<&'static str> {
        // Intentionally empty: prefixes live in the registry's
        // `Provider.model_prefixes` and are consulted directly by
        // `supports_model` below. The trait's Vec<&'static str> return
        // can't carry the registry's dynamic Vec<String> without leaking,
        // so we bypass it rather than faking a static slice.
        Vec::new()
    }

    /// Dynamic catalog for DMR, registry-declared prefix match for
    /// everyone else.
    ///
    /// The default trait impl uses `starts_with` against
    /// `supported_model_prefixes`. We override because prefixes now live
    /// in the Rust catalog (catalog.rs) (Provider.model_prefixes), not as
    /// `&'static str` embedded in code. A dynamic-catalog gateway (DMR) is
    /// special-cased because its catalog depends on `docker model pull`
    /// history — so we check the live runtime_models set populated at init.
    ///
    /// Returning false when the live set is empty/missing is the right
    /// behavior: AdapterRegistry::select hard-errors when no adapter
    /// supports a model, which surfaces the real problem ("user never
    /// pulled X") instead of silently routing to some other provider.
    /// Gated on the TYPED `dynamic_model_catalog` capability (#55), not
    /// the provider id.
    fn supports_model(&self, model_name: &str) -> bool {
        if self.config.dynamic_model_catalog {
            return self.runtime_models_contain(model_name);
        }
        let lower = model_name.to_lowercase();
        // Exact id match against the registry's declared models.
        if self
            .config
            .models
            .iter()
            .any(|m| m.id.to_lowercase() == lower)
        {
            return true;
        }
        // Family prefix match for "id we haven't listed yet but this
        // provider clearly owns" (e.g. gpt-5-preview → openai).
        self.config
            .model_prefixes
            .iter()
            .any(|prefix| lower.starts_with(&prefix.to_lowercase()))
    }
}

impl OpenAICompatibleAdapter {
    pub(super) fn generation_admission_for(
        &self,
        serving_base: &str,
    ) -> crate::ai::adapter::GenerationAdmissionOwner {
        use crate::ai::adapter::GenerationAdmissionOwner;
        if !self.dedicated_lane && self.targets_serving_endpoint(serving_base) {
            GenerationAdmissionOwner::SharedLocal
        } else {
            GenerationAdmissionOwner::Adapter
        }
    }
}
