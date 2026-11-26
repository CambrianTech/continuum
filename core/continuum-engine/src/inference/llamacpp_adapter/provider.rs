//! `LlamaCppAdapter`'s provider surface: the [`AIProviderAdapter`] implementation (lifecycle,
//! generation, embeddings, health, model metadata). Moved out of `llamacpp_adapter.rs` unchanged,
//! the way `openai_adapter/provider.rs` holds that adapter's, so the generation path can grow
//! without crossing the file limit.

use super::*;
use crate::ai::inference_error::InferenceError;

#[async_trait]
impl AIProviderAdapter for LlamaCppAdapter {
    fn provider_id(&self) -> &str {
        LLAMACPP_PROVIDER_ID
    }

    fn name(&self) -> &str {
        "Llama.cpp (in-process Metal/CUDA)"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        // max_context_window: if the backend has been loaded, use the
        // model's actual training ceiling; otherwise leave 0 to signal
        // "ask the model" via model_metadata. Never invent a number.
        let max_ctx = self
            .backend
            .read()
            .as_ref()
            .map(|b| b.n_ctx_train())
            .unwrap_or(0); // unwrap_or: no backend loaded yet; 0 is the capabilities contract's "ask the model" (model_metadata)
        let capabilities = match self.generation {
            GenerationSupport::EmbeddingOnly => vec![Capability::Embedding],
            GenerationSupport::Enabled { .. } => vec![
                Capability::TextGeneration, Capability::Chat, Capability::ToolUse,
                Capability::Streaming, Capability::Embedding,
            ],
        };
        let output_window = match self.generation {
            GenerationSupport::EmbeddingOnly => 0,
            GenerationSupport::Enabled { context } => context,
        };
        AdapterCapabilities::builder()
            .capabilities(capabilities)
            .local()
            .context_window(max_ctx)
            .max_output_tokens(output_window)
            .protocols(crate::ai::adapter::NativeProtocols::ModelTemplate)
            .build()
    }

    fn api_style(&self) -> ApiStyle {
        ApiStyle::Local
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    async fn initialize(&mut self) -> Result<(), String> {
        // Eagerly load the model at initialize time. The previous lazy-load
        // scheme meant `model_metadata()` returned None until the first
        // `generate_text` call, which in turn made TS-side callers of
        // `ai/model-info` get back nothing → they fell through to a
        // hardcoded 8192 context-window fallback, ignoring the model's
        // actual 262144. Eager-load pays the 5-10s cost once at boot and
        // guarantees every downstream consumer sees the model's real
        // capabilities from the first query on.
        //
        // If the GGUF isn't on disk we return Ok without loading —
        // `register_adapters` has already gated registration on
        // `health_check().api_available`, so we only get called when the
        // file exists. If something changed between those two checks
        // (e.g. the file was deleted), the first `generate_text` still
        // falls back to the ensure_loaded path and surfaces a clean
        // model-not-found error then.
        if self.model_path.exists() {
            let _ = self.ensure_loaded()?;
        }
        Ok(())
    }

    async fn warmup(&self) -> Result<(), String> {
        // Tiny throwaway decode against a minimal prompt. The model
        // file is already loaded by `initialize`; this call exercises
        // the KV-cache allocation path, the attention kernels, and
        // the sampler state — so when the first real `generate_text`
        // lands, it pays only the marginal per-token cost, not the
        // cold-cache JIT bill.
        //
        // Per [[init-once-handle-then-lease-zero-copy-refs]]: the
        // adapter's hot path is leased per turn; the substrate pays
        // the init cost ONCE here, at boot, never on a user's first
        // message. On Intel Mac with Qwen 0.5B this saves ~200-500ms
        // off the first turn; on M5 Metal with a larger model the
        // save is multiples of that.
        let warmup_request = TextGenerationRequest {
            messages: vec![crate::ai::types::ChatMessage::text("user", "Hi")],
            system_prompt: None,
            model: None,
            provider: None,
            temperature: Some(0.0),
            max_tokens: Some(1),
            top_p: None,
            top_k: None,
            repeat_penalty: None,
            frequency_penalty: None,
            repeat_last_n: None,
            stop_sequences: None,
            tools: None,
            tail_tools: None,
            view_messages: None,
            tool_choice: None,
            response_format: None,
            active_adapters: None,
            request_id: Some("warmup".to_string()),
            user_id: None,
            room_id: None,
            purpose: Some("warmup".to_string()),
            persona_id: None,
            turn_bound: None,
        };
        // A warm-up decode states no think term: the model's own default, as it always ran.
        let terms = crate::ai::types::DispatchTerms::thinking(crate::ai::types::Think::ModelDefault);
        match crate::ai::stream_sinks::drain_generation(|sink| crate::ai::adapter::DispatchTermsPolicy::generate_admitted(self, warmup_request, terms, sink)).await {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("LlamaCppAdapter warmup decode failed: {e}")),
        }
    }

    async fn shutdown(&mut self) -> Result<(), String> {
        // Drop the backend — releases GPU memory.
        *self.backend.write() = None;
        Ok(())
    }

    /// Convenience drain over [`generate_stream`], as every streaming adapter has it.
    async fn generate_text(
        &self,
        request: TextGenerationRequest,
        dispatched: crate::ai::adapter::Dispatched,
    ) -> Result<TextGenerationResponse, String> {
        crate::ai::stream_sinks::drain_generation(|sink| self.generate_stream(request, sink, dispatched)).await
    }

    /// The text form of [`generate_stream_checked`], for callers outside the typed seam.
    async fn generate_stream(
        &self,
        request: TextGenerationRequest,
        sink: tokio::sync::mpsc::UnboundedSender<crate::ai::adapter::GenerationChunk>,
        dispatched: crate::ai::adapter::Dispatched,
    ) -> Result<TextGenerationResponse, String> {
        self.generate_stream_checked(request, sink, dispatched).await.map_err(|error| error.to_string())
    }

    /// One turn on the in-process engine, its failure typed at this seam: a turn the Core cannot
    /// fit arrives as `ContextExceeded` with the Core's own numbers, so her caller refits once
    /// instead of resending it as a transient fault (the M5, 2026-11-21: seven turns failed at the
    /// stream's end; as text each was retried unchanged). Every other failure is `Transient`, as
    /// it was. The engine collects the whole turn, so the answer reaches `sink` whole.
    async fn generate_stream_checked(
        &self,
        request: TextGenerationRequest,
        sink: tokio::sync::mpsc::UnboundedSender<crate::ai::adapter::GenerationChunk>,
        dispatched: crate::ai::adapter::Dispatched,
    ) -> Result<TextGenerationResponse, InferenceError> {
        // The monotonic origin of every later transition's `since_enter` (`inference.generate.enter`).
        let entered = Instant::now();
        let allocation = self.allocation()?;
        // Her think term: a stated zero budget renders no thinking; any other budget is bounded by
        // the template's own thinking tags below; none stated is the model's default.
        let think_budget = dispatched.terms().think.budget();
        let thinking = !matches!(think_budget, Some(budget) if budget.0 == 0);
        let backend = self.ensure_loaded()?;

        // Use the model's OWN chat template (from GGUF metadata) via
        // llama.cpp's template engine. The previous hand-rolled
        // `<|im_start|>role\n ...<|im_end|>\n` prefix was wrong for
        // qwen3.5 — it caused `<|im_end<|>` special-token leakage in
        // Teacher AI output (2026-04-20). Different models use different
        // boundary tokens; the model is the source of truth.
        // Use the model's own template if embedded in GGUF metadata;
        // Resolution order, no fallback:
        //   1. GGUF metadata `tokenizer.chat_template` (forge bake should
        //      put it here).
        //   2. the Rust catalog (catalog.rs) `chat_template` field (memento's registry —
        //      authoritative when GGUF is silent).
        // No in-code constant. Adding a new model = catalog row, never an
        // adapter edit. If both sources are absent, render_chat passes
        // None to llama.cpp which is its own loud failure (chatml default
        // doesn't match qwen3.5's special tokens — output corruption).
        // Looked up only where it is used: the media path's plain template, and the chat render's
        // one-time init when the GGUF has neither a default nor a named tool_use template (Codex on
        // #6145, common/chat.cpp:761-776). A text turn copies no template string.
        let registry_template = || {
            crate::model_registry::try_global()
                .and_then(|reg| reg.model(backend.model_id()))
                .and_then(|m| m.chat_template.clone())
        };
        // Walk the request to find any image / audio content. If present,
        // the model MUST natively accept that modality (else the bridge
        // is wrong upstream — sensory-bridge converts to text BEFORE
        // reaching here for non-multimodal models). For vision-capable /
        // audio-capable local models with a loaded mmproj, media items
        // splice in as `<__media__>` markers inside the rendered text
        // and the call routes to `backend.generate_with_image()` /
        // `generate_with_audio()` instead of the scheduler.
        //
        // Single-media-per-call scope for v1: libmtmd's C API supports
        // multiple bitmaps per tokenize call (one marker each, in
        // order), but our backend signatures take one bytes blob. The
        // collected_media vector preserves order; if there's >1 item
        // OR a mix of image+audio, we hard-error rather than silently
        // dropping the rest. Multi-media is a follow-up once a real
        // caller needs it (mtmd_tokenize already does the work).
        // Diagnostic: prove what the adapter receives from the caller —
        // counts user message shapes (Text vs Parts) and ContentPart
        // variants. When vision routing breaks, this tells us whether
        // the image got dropped upstream (count=0, request had no
        // ContentPart::Image) vs in our walk (count>0 but
        // generate_with_image still doesn't fire). 2026-04-21: Vision AI
        // was producing wrong answers; this is the probe to localize.
        {
            let mut text_msgs = 0;
            let mut parts_msgs = 0;
            let mut parts_text = 0;
            let mut parts_image = 0;
            let mut parts_audio = 0;
            let mut parts_other = 0;
            for msg in &request.messages {
                match &msg.content {
                    MessageContent::Text(_) => text_msgs += 1,
                    MessageContent::Parts(parts) => {
                        parts_msgs += 1;
                        for p in parts {
                            match p {
                                crate::ai::types::ContentPart::Text { .. } => parts_text += 1,
                                crate::ai::types::ContentPart::Image { .. } => parts_image += 1,
                                crate::ai::types::ContentPart::Audio { .. } => parts_audio += 1,
                                _ => parts_other += 1,
                            }
                        }
                    }
                }
            }
            let log = runtime::logger("llamacpp");
            log.info(&format!(
                "generate_text request: model={} messages={} (text={} parts={}; parts contain text={} image={} audio={} other={})",
                request.model.as_deref().unwrap_or("?"), // unwrap_or: a log line names an unnamed model "?"
                request.messages.len(),
                text_msgs,
                parts_msgs,
                parts_text,
                parts_image,
                parts_audio,
                parts_other,
            ));

            // RTOS probe: inference entry seam. The persona service loop
            // talks through `adapter.generate_text`; this is where the
            // wall-clock cost actually accrues. Class taxonomy lives in
            // docs/architecture/RTOS-DEBUGGER-PROBES.md. Per
            // [[jtag-probes-are-rtos-debugger]]: name the surrounding
            // vars so the operator filtering on
            // `class=="inference.generate.enter"` has the request
            // fingerprint without grepping logs.
            continuum_base::probe!(
                class = "inference.generate.enter",
                model = request.model.as_deref().unwrap_or("?"), // unwrap_or: the probe names an unnamed model "?"
                persona_id = request.persona_id.as_deref().unwrap_or(""), // unwrap_or: a request for no persona probes an empty id
                request = ?request.request_id.as_deref(),
                msg_count = request.messages.len(),
                max_tokens = ?request.max_tokens,
                has_system_prompt = request.system_prompt.is_some(),
                parts_image = parts_image,
                parts_audio = parts_audio,
                "generate_text entry"
            );
        }

        // A text turn renders through the model's own chat handling with her tools (S2b): the same
        // wire messages and tools serving sends, and one render kept to parse what she writes. Only a
        // media turn builds the plain template's messages and media bytes (mtmd has no chat render).
        let render_body = |body: &serde_json::Value| -> Result<llama::ChatRender, super::chat_turn::ChatTurnError> {
            Ok(backend.render_chat_turn(&body.to_string(), &registry_template, llama::ReasoningFormat::Auto)?)
        };
        let render_started = Instant::now();
        let (prompt, collected_media, chat_render) = if !super::chat_turn::carries_media(&request) {
            let rendered = continuum_base::time_sync!("inference.render_chat", {
                super::chat_turn::thinking_request(&request, thinking).and_then(|body| render_body(&body))
            });
            let turn = rendered.and_then(|render| {
                let described = super::chat_turn::described(&render)?;
                let prompt = super::chat_turn::prompt_of(&described)?;
                Ok((prompt, render, described))
            });
            let (prompt, render, described) = turn.map_err(|refused| refused.to_string())?; // a render refusal is Transient, as it was
            (prompt, Vec::new(), Some((render, described)))
        } else {
            let mut collected_media: Vec<(llama::MediaKind, Vec<u8>)> = Vec::new();
            let mut messages: Vec<llama::ChatMsg> = Vec::new();
            if let Some(sys) = request.system_prompt.as_ref() {
                if !sys.is_empty() {
                    messages.push(llama::ChatMsg {
                        role: "system".to_string(),
                        content: sys.clone(),
                    });
                }
            }
            for msg in &request.messages {
                let content = match &msg.content {
                    MessageContent::Text(t) => t.clone(),
                    MessageContent::Parts(parts) => {
                        let mut out = String::new();
                        for p in parts {
                            match p {
                                crate::ai::types::ContentPart::Text { text } => {
                                    out.push_str(text);
                                }
                                crate::ai::types::ContentPart::Image { image } => {
                                    // Splice the marker at this exact spot —
                                    // mtmd_tokenize replaces it with the
                                    // image-token chunk. Position matters
                                    // (text-before-image vs after changes
                                    // what the model sees).
                                    out.push_str(llama::MtmdContext::default_marker());
                                    let bytes = decode_image_bytes(image)?;
                                    collected_media.push((llama::MediaKind::Image, bytes));
                                }
                                crate::ai::types::ContentPart::Audio { audio } => {
                                    // Same shape as image — splice marker,
                                    // collect bytes. mtmd's bitmap helper
                                    // auto-detects audio from magic bytes;
                                    // the modality tag here drives backend
                                    // capability checks (supports_audio
                                    // instead of supports_vision) and
                                    // routing to generate_with_audio.
                                    out.push_str(llama::MtmdContext::default_marker());
                                    let bytes = decode_audio_bytes(audio)?;
                                    collected_media.push((llama::MediaKind::Audio, bytes));
                                }
                                crate::ai::types::ContentPart::Video { .. } => {
                                    return Err("llamacpp_adapter: encoded video input is unsupported; decode it into timestamped frames and audio before model projection. No video was consumed.".into());
                                }
                                crate::ai::types::ContentPart::ToolUse { .. }
                                | crate::ai::types::ContentPart::ToolResult { .. } => {} // handled by tool path
                            }
                        }
                        out
                    }
                };
                messages.push(llama::ChatMsg {
                    role: msg.role.clone(),
                    content,
                });
            }
            // RTOS probe: chat-template rendering is synchronous + small,
            // but cumulative across thousands of turns it can shadow real
            // bottlenecks. Bracketing it lets the operator subtract it
            // from `inference.forward.*` cleanly.
            let prompt = continuum_base::time_sync!("inference.render_chat", {
                let template = backend.model_chat_template().or_else(&registry_template);
                llama::render_chat(template.as_deref(), &messages, true)
            })?;
            (prompt, collected_media, None)
        };

        // No hardcoded cap. If the caller didn't specify, the model can
        // decode up to its trained context. Capping silently at 2048 was
        // the source of clipped JSON/XML output — the model would stop
        // mid-structure and downstream JSON.parse / XML parsers blew up.
        let max_tokens = request
            .max_tokens
            .map(|n| n as usize)
            .unwrap_or_else(|| backend.n_ctx_train() as usize); // unwrap_or_else: a caller with no cap decodes to the model's trained context, never a guessed one
        let sampling = sampling_config_from_request(&request);
        // Where her next act stops matching this prompt (her view, CONTINUUM-ENGINE 4.15): the engine
        // checkpoints her there and her next act prefills only what follows.
        let rendered_at = Instant::now();
        let stable_prefix = match &chat_render {
            Some((_, described)) => super::chat_turn::view_text(&request, described, thinking, &render_body).map_err(|refused| refused.to_string())?,
            None => None,
        };
        // Where an act's submit time goes before her turn reaches the Core (the M5, 2026-11-26: submit to
        // queue 0.6 to 9.5 s at 70k to 226k tokens): her prompt's render, and her view's second render.
        continuum_base::probe!(class = "inference.submit.render", request = ?request.request_id, prompt_chars = prompt.len() as u64,
            view_chars = ?stable_prefix.as_ref().map(super::chat_turn::View::chars), render_ms = render_started.elapsed().saturating_sub(rendered_at.elapsed()).as_millis() as u64,
            view_render_ms = rendered_at.elapsed().as_millis() as u64, "an act's prompt rendered, and her view rendered again");
        // Stop sequences = caller-supplied + model's registry-declared
        // text-form stops. Some GGUFs (the forged qwen3.5 included) carry
        // the wrong tokenizer.ggml.eos_token_id, so is_eog_token never
        // fires for the chat-template terminator and the model loops the
        // same answer until max_tokens. The registry's stop_sequences
        // field carries the correct strings (e.g. `<|im_end|>`) that the
        // scheduler matches against streamed output.
        let mut stop_owned: Vec<String> = request.stop_sequences.iter().flatten().cloned().collect();
        if let Some(model_meta) =
            crate::model_registry::try_global().and_then(|reg| reg.model(backend.model_id()))
        {
            for s in &model_meta.stop_sequences {
                if !stop_owned.contains(s) {
                    stop_owned.push(s.clone());
                }
            }
        }
        // The template's own extra stops (a tool-call format that ends a turn on a marker).
        let template_stops: Vec<String> = chat_render.iter().flat_map(|(_, described)| described.additional_stops.iter()).filter(|stop| !stop_owned.contains(stop)).cloned().collect();
        stop_owned.extend(template_stops);

        let gen_start = Instant::now();
        let backend_for_blocking = backend.clone();
        let prompt_for_blocking = prompt.clone();
        let stable_for_blocking = stable_prefix;
        let stop_for_closure = stop_owned.clone();
        let request_for_blocking = request.request_id.clone();
        // Her need rides this call's scope; read here, before the blocking hop leaves it.
        let urgency = crate::inference::need::current();
        // Parse the wire-format persona_id (Option<String> on the public
        // request type) to Option<Uuid> for the typed scheduler API. A
        // malformed UUID drops to None rather than failing the request —
        // the request itself is still valid, we just can't attribute its
        // KV bytes per-persona. The registry's drift-detection sanity
        // check will surface this if it becomes systemic.
        let persona_id: Option<uuid::Uuid> = request
            .persona_id
            .as_deref()
            .and_then(|s| uuid::Uuid::parse_str(s).ok());

        // Genome paging: the adapter contract carries the genes to apply as
        // `(name, path, scale)`. We page each in (idempotent) and hand the
        // resolved `(id, scale)` list to the scheduler, which applies it
        // context-level before decode. Empty in the common base case.
        let requested_genes: Vec<crate::inference::backends::persona_scheduler::Gene> = request
            .active_adapters
            .iter()
            .flatten()
            .map(|a| crate::inference::backends::persona_scheduler::Gene { id: a.name.clone(), path: std::path::PathBuf::from(&a.path), scale: a.scale as f32 })
            .collect();

        // Her sampler, from the render that produced her prompt when she has one: its tool grammar
        // (a call she starts can name only the parameters her tools declare), generation prompt and
        // thinking tags, with her think budget. A media turn's is plain. Built here, on her act's
        // thread; the engine only samples.
        // A JSON schema is enforced through her render's grammar; a turn with no render (a media turn)
        // has none to enforce it with, so it is refused, never answered unconstrained.
        if chat_render.is_none() {
            crate::ai::adapter::refuse_schema(&request)?;
        }
        let sampler = backend.turn_sampler(chat_render.as_ref().map(|(render, _)| render), &sampling, think_budget.map(|budget| budget.0))?;
        let result: Result<crate::inference::backends::llamacpp::Answer, InferenceError> = if collected_media.is_empty() {
            // Pure-text path: scheduler-managed continuous batching.
            // RTOS timing probe: this is the actual LLM forward pass —
            // by far the dominant cost (95%+ on LCD tier per
            // 2026-06-06 baseline). `time_probe!` wraps the JoinHandle
            // future cleanly across the `.await`.
            let genes_for_closure = requested_genes;
            continuum_base::time_probe!(
                "inference.forward.text",
                async move {
                    let answer = tokio::task::spawn_blocking(move || {
                    // Brackets for the wait between entry and the engine's admission: a full
                    // blocking pool delays this line, a gene load delays the next.
                    continuum_base::probe!(
                        class = "inference.generate.worker_started",
                        request = ?request_for_blocking,
                        since_enter = ?entered.elapsed(),
                        "the blocking worker began this generation"
                    );
                    let stop_refs: Vec<&str> =
                        stop_for_closure.iter().map(|s| s.as_str()).collect();
                    // Page in each requested gene (idempotent) before generation.
                    // A missing/unreadable adapter file is a hard error — never
                    // silently run the base model in its place (Rule 2).
                    for gene in &genes_for_closure {
                        backend_for_blocking.ensure_adapter(&gene.id, &gene.path)?;
                    }
                    continuum_base::probe!(
                        class = "inference.generate.genes_ready",
                        request = ?request_for_blocking,
                        genes = genes_for_closure.len(),
                        since_enter = ?entered.elapsed(),
                        "her genes are loaded for this generation"
                    );
                    backend_for_blocking.submit_for_persona(
                        allocation, persona_id,
                        crate::inference::backends::llamacpp::TurnPrompt { text: &prompt_for_blocking, stable_prefix: stable_for_blocking.as_ref().and_then(|view| view.of(&prompt_for_blocking)), urgency, request: request_for_blocking.as_deref() },
                        max_tokens,
                        sampler,
                        &stop_refs,
                        &genes_for_closure,
                    )
                    }).await.map_err(|e| format!("generate task panicked: {e}"))??;
                    answer.collect().await
                }
            )
        } else {
            // Multimodal path: bypass the scheduler — media tokens have
            // a fixed positional layout the scheduler can't interleave
            // with concurrent text seqs. Single-media-per-call scope for
            // v1; mtmd's C API supports multiple media in one prompt
            // (one marker each in order) but our backend signatures take
            // one bytes blob. Hard-error rather than silently dropping
            // extras — clearer signal upstream.
            // Genes on the multimodal bypass path are not yet supported (the
            // mtmd single-flight path doesn't route through the scheduler that
            // applies set_loras). Fail loud rather than silently dropping the
            // requested adapter (Rule 2).
            if !requested_genes.is_empty() {
                let names: Vec<&str> = requested_genes
                    .iter()
                    .map(|gene| gene.id.as_str())
                    .collect();
                return Err(format!(
                    "llamacpp_adapter: LoRA genes {names:?} requested with media — not supported \
                     on the multimodal bypass path (v1). Genes apply only on the text scheduler \
                     path; send the gene-bearing request without media."
                ).into());
            }
            if collected_media.len() > 1 {
                let kinds: Vec<String> = collected_media
                    .iter()
                    .map(|(k, _)| format!("{:?}", k))
                    .collect();
                return Err(format!(
                    "llamacpp_adapter: multi-media not yet supported in this adapter \
                     ({} items: {}); send one media item per request until backend.\
                     generate_with_media accepts &[(MediaKind, Vec<u8>)]",
                    collected_media.len(),
                    kinds.join(", ")
                ).into());
            }
            let Some((kind, media_bytes)) = collected_media.into_iter().next() else {
                return Err("llamacpp_adapter: the media path was taken with no media item".into());
            };
            // RTOS timing probe: mtmd path runs single-flight (no
            // scheduler batching for media) so the timing here is
            // direct end-to-end. Separate seam from the text path so
            // operators can `jq` text-only vs mtmd cost distinctly.
            continuum_base::time_probe!(
                "inference.forward.multimodal",
                tokio::task::spawn_blocking(move || {
                    let stop_refs: Vec<&str> =
                        stop_for_closure.iter().map(|s| s.as_str()).collect();
                    match kind {
                        llama::MediaKind::Image => backend_for_blocking.generate_with_image(
                        allocation,                            &prompt_for_blocking,
                            &media_bytes,
                            max_tokens,
                            sampler,
                            &stop_refs,
                        ),
                        llama::MediaKind::Audio => backend_for_blocking.generate_with_audio(
                        allocation,                            &prompt_for_blocking,
                            &media_bytes,
                            max_tokens,
                            sampler,
                            &stop_refs,
                        ),
                    }
                })
            )
            .map_err(|e| format!("generate_with_media task panicked: {e}"))?
        };
        let crate::inference::backends::llamacpp::Answer { text: written, facts } = result?;
        let tokens = facts.generated;
        // What she wrote, read by the render that produced her prompt: content, reasoning and her
        // native calls (assembled by the one owner the streamed path uses). A media turn's output
        // is its text.
        let end = facts.end.generation_end();
        let (text, reasoning, tool_calls) = match &chat_render {
            Some((render, _)) => match super::chat_turn::parse_turn(render, &written, end) {
                Ok(turn) => {
                    let output = super::chat_turn::GeneratedTurn { written: &written, tokens, end, stops: &stop_owned };
                    turn.probe_if_empty(output, &request);
                    (turn.text, turn.reasoning, turn.tool_calls)
                }
                Err(refused) => return Err(refused.to_string().into()),
            },
            None => (written, None, None),
        };
        let content = crate::inference::native_tool_calls::content_blocks(&text, tool_calls.as_deref());
        let finish_reason = super::chat_turn::finish_reason(end, tool_calls.is_some());

        let elapsed = gen_start.elapsed();
        let tok_per_sec = if elapsed.as_secs_f64() > 0.0 {
            tokens as f64 / elapsed.as_secs_f64()
        } else {
            0.0
        };
        *self.last_throughput_tok_s.write() = tok_per_sec;

        // RTOS probe: inference exit seam. Pair with
        // `inference.generate.enter` via the same span ancestry. The
        // critical campaign metric is `tok_per_sec` — every probe
        // here carries it so a `jq` over `inference.generate.exit`
        // events is the latency dashboard. `text_len` lets the
        // operator catch the silent-truncation class of bug where
        // the model stops short of EOS.
        // Real tokens on the served lane = proof of life for the health heartbeat, which
        // otherwise probes for a slot it cannot get while this very work holds them all.
        if tokens > 0 {
            crate::inference::llama_server::note_real_decode();
        }
        continuum_base::probe!(
            class = "inference.generate.exit",
            model = backend.model_id(),
            tokens_out = tokens,
            text_len = text.len(),
            duration_ms = elapsed.as_millis() as u64,
            tok_per_sec = tok_per_sec,
            "generate_text exit"
        );

        // No tail-strip. Previously this hand-rolled `text.rfind(stop)` and
        // truncated — only existed to clean up the special tokens that
        // leaked from the OLD hand-rolled chat-template prefixes. Now that
        // we use the model's real chat template via `render_chat`, the
        // model's actual EOS tokens stop generation (handled inside the
        // scheduler via `is_eog_token`) and don't leak as text.

        let response = TextGenerationResponse {
            text,
            finish_reason,
            model: backend.model_id().to_string(),
            provider: LLAMACPP_PROVIDER_ID.to_string(),
            // What the Core measured of her turn, never an estimate: her prompt counted exactly, what
            // her held state covered, and the time each phase took.
            usage: facts.usage(),
            response_time_ms: elapsed.as_millis() as u64,
            request_id: format!("llamacpp-{}", continuum_base::time::EpochMs::now().millis()),
            content,
            tool_calls,
            // Split by the model's own reasoning format (the render's parser), never a tag regex.
            reasoning,
            routing: None,
            error: None,
            timing: Some(facts.timing()),
        };
        crate::ai::stream_sinks::deliver_whole(&sink, &response).await;
        Ok(response)
    }

    /// Embeddings via the backend's dedicated embedding-mode context. The loaded
    /// model determines the vector space — for grid-comparable vectors this
    /// adapter must have loaded the canonical Qwen3-Embedding-0.6B (the
    /// `NeuralEmbeddingProvider` is responsible for loading it). `backend.embed`
    /// is a blocking forward pass, so it runs on `spawn_blocking`, off the async
    /// executor (the same bridge as `generate_text`'s forward).
    async fn create_embedding(
        &self,
        request: EmbeddingRequest,
    ) -> Result<EmbeddingResponse, String> {
        let backend = self.ensure_loaded()?;
        let model_id = backend.model_id().to_string();
        let texts: Vec<String> = match request.input {
            EmbeddingInput::Single(s) => vec![s],
            EmbeddingInput::Multiple(v) => v,
        };
        let zero_usage = UsageMetrics {
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            estimated_cost: None,
        };
        if texts.is_empty() {
            return Ok(EmbeddingResponse {
                embeddings: Vec::new(),
                model: model_id,
                provider: LLAMACPP_PROVIDER_ID.to_string(),
                usage: zero_usage,
                response_time_ms: 0,
            });
        }
        let start = Instant::now();

        // ── Governed GPU admission (mirrors inference::single_purpose_lane::acquire_lane_slot) ──
        // Wait without a memory claim. The admitted worker owns both through context teardown,
        // even when the request future is cancelled after spawn_blocking starts.
        let embedding = backend.admit_embedding().await?;
        // Tokenized first, so the context's memory is known before it exists: the lease is what THIS
        // request allocates, in the pool the context resides in, within the lane's need (its weights
        // and its window's context, `embed_lane_memory`). Ungoverned, a context grabbed memory behind
        // the governor's back and, under pressure, decoded to an all-zero vector that silently broke
        // recall; a fixed 1,792 MiB bound refused 91% of short texts on the M5 (26960c09d, 758 of 834
        // in 3 h).
        let residency = backend.embed_residency()?;
        let batch = embedding.tokenize(&texts)?;
        let lease = residency
            .lease(backend.pooled_bytes(batch.width())?, texts.len())
            .await
            .map_err(|e| e.to_string())?;
        let embeddings = embedding.run(batch, lease).await?;
        Ok(EmbeddingResponse {
            embeddings,
            model: model_id,
            provider: LLAMACPP_PROVIDER_ID.to_string(),
            usage: zero_usage,
            response_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn health_check(&self) -> HealthStatus {
        let (healthy, message) = if self.backend.read().is_some() {
            (true, "in-process llama.cpp backend loaded".to_string())
        } else {
            match std::fs::metadata(&self.model_path) {
                Ok(metadata) if metadata.is_file() =>
                    (true, "model file available; backend not loaded".to_string()),
                Ok(_) => (false, "model path is not a regular file".to_string()),
                Err(error) => (false, format!("model file metadata: {error}")),
            }
        };
        HealthStatus {
            status: if healthy {
                HealthState::Healthy
            } else {
                HealthState::Unhealthy
            },
            api_available: healthy,
            response_time_ms: 0,
            error_rate: 0.0,
            last_checked: continuum_base::time::EpochMs::now().millis(),
            message: Some(message),
        }
    }

    async fn get_available_models(&self) -> Vec<ModelInfo> {
        // Identity + capabilities come from the registry (the Rust catalog (catalog.rs)).
        // Runtime overlay (context_window from GGUF metadata, tokens/sec
        // from last measurement) only applies if the backend is loaded;
        // otherwise we return the catalog-declared view and let the first
        // generate_text call refresh the numbers.
        let base = models_for_provider_via_registry(LLAMACPP_PROVIDER_ID);
        let backend_guard = self.backend.read();
        let last_tok_s = *self.last_throughput_tok_s.read();
        base.into_iter()
            .map(|info| match backend_guard.as_ref() {
                Some(b) if info.id == self.default_model => {
                    model_info_with_runtime(info, b, last_tok_s)
                }
                _ => info,
            })
            .collect()
    }

    fn model_metadata(&self, model_id: &str) -> Option<ModelInfo> {
        // Match against the registry (provider's declared models), then
        // overlay runtime fields if the backend happens to be loaded.
        // Matching is case-insensitive on the declared id; no substring
        // special-casing — the id is the contract.
        let want = model_id.to_lowercase();
        let info = models_for_provider_via_registry(LLAMACPP_PROVIDER_ID)
            .into_iter()
            .find(|m| m.id.to_lowercase() == want)?;
        let backend_guard = self.backend.read();
        match backend_guard.as_ref() {
            Some(b) if info.id == self.default_model => Some(model_info_with_runtime(
                info,
                b,
                *self.last_throughput_tok_s.read(),
            )),
            _ => Some(info),
        }
    }

    fn device_type(&self) -> InferenceDevice {
        // Bundled llama.cpp is built with Metal (Mac) / CUDA (Linux) per
        // continuum's build flags. Either way: GPU-class device.
        InferenceDevice::Gpu
    }

    fn supported_model_prefixes(&self) -> Vec<&'static str> {
        // Intentionally empty — this adapter lists its models explicitly
        // in the registry, and `supports_model` below matches against the
        // declared ids directly. The old hardcoded prefixes (qwen3.5-…)
        // would silently match a Qwen3.5 row under a *different* provider
        // (DMR) and mis-route it here. Exact-id match is the contract.
        Vec::new()
    }

    fn supports_model(&self, model_name: &str) -> bool {
        self.default_model.eq_ignore_ascii_case(model_name)
    }
}
