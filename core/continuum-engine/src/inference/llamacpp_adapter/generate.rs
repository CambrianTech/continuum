//! Text generation for the in-process llama.cpp adapter: render the request through the model's chat
//! template, page in its genes, and decode through the persona scheduler (or the single-flight media
//! path). Moved out of `llamacpp_adapter.rs` unchanged, so the file holding the trait impl stays
//! small and this turn has one owner.

use super::*;

impl LlamaCppAdapter {
    /// One text generation: the body of [`AIProviderAdapter::generate_text`].
    pub(super) async fn generate(&self, request: TextGenerationRequest) -> Result<TextGenerationResponse, String> {
        let allocation = match self.generation {
            GenerationSupport::EmbeddingOnly => return Err("embedding-only adapter does not support text generation".into()),
            GenerationSupport::Enabled { context } => GenerationAllocation::new(
                std::num::NonZeroU32::new(context).ok_or("invalid zero generation allocation")?
            ),
        };
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
        let registry_template: Option<String> = crate::model_registry::try_global()
            .and_then(|reg| reg.model(backend.model_id()))
            .and_then(|m| m.chat_template.clone());
        let template_string = backend.model_chat_template().or(registry_template);
        let template = template_string.as_deref();
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
                msg_count = request.messages.len(),
                max_tokens = ?request.max_tokens,
                has_system_prompt = request.system_prompt.is_some(),
                parts_image = parts_image,
                parts_audio = parts_audio,
                "generate_text entry"
            );
        }

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
        // A text turn renders through the model's own chat handling with her tools (S2b): the same
        // wire messages and tools serving sends, and one render kept to parse what she writes. The
        // media path keeps the plain template until mtmd takes a chat render.
        let chat_render = if collected_media.is_empty() {
            let rendered = continuum_base::time_sync!("inference.render_chat", {
                super::chat_turn::chat_request(&request).and_then(|body| {
                    Ok(backend.render_chat_turn(&body.to_string(), template, llama::ReasoningFormat::Auto)?)
                })
            });
            match rendered {
                Ok(render) => Some(render),
                Err(refused) => return Err(refused.to_string()),
            }
        } else {
            None
        };
        let prompt = match &chat_render {
            Some(render) => match super::chat_turn::prompt_of(render) {
                Ok(prompt) => prompt,
                Err(refused) => return Err(refused.to_string()),
            },
            None => continuum_base::time_sync!("inference.render_chat", {
                llama::render_chat(template.as_deref(), &messages, true)
            })?,
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

        let gen_start = Instant::now();
        let backend_for_blocking = backend.clone();
        let prompt_for_blocking = prompt.clone();
        let stop_for_closure = stop_owned.clone();
        let sampling_for_closure = sampling.clone();
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
        let requested_genes: Vec<(String, std::path::PathBuf, f32)> = request
            .active_adapters
            .iter()
            .flatten()
            .map(|a| (a.name.clone(), std::path::PathBuf::from(&a.path), a.scale as f32))
            .collect();

        let result: Result<(String, usize), String> = if collected_media.is_empty() {
            // Pure-text path: scheduler-managed continuous batching.
            // RTOS timing probe: this is the actual LLM forward pass —
            // by far the dominant cost (95%+ on LCD tier per
            // 2026-06-06 baseline). `time_probe!` wraps the JoinHandle
            // future cleanly across the `.await`.
            let genes_for_closure = requested_genes;
            continuum_base::time_probe!(
                "inference.forward.text",
                tokio::task::spawn_blocking(move || {
                    let stop_refs: Vec<&str> =
                        stop_for_closure.iter().map(|s| s.as_str()).collect();
                    // Page in each requested gene (idempotent) before generation.
                    // A missing/unreadable adapter file is a hard error — never
                    // silently run the base model in its place (Rule 2).
                    for (id, path, _) in &genes_for_closure {
                        backend_for_blocking.ensure_adapter(id, path)?;
                    }
                    let active_loras: Vec<(String, f32)> = genes_for_closure
                        .iter()
                        .map(|(id, _, scale)| (id.clone(), *scale))
                        .collect();
                    backend_for_blocking.generate_for_persona(
                        allocation,                        persona_id,
                        &prompt_for_blocking,
                        max_tokens,
                        sampling_for_closure,
                        &stop_refs,
                        &active_loras,
                    )
                })
            )
            .map_err(|e| format!("generate task panicked: {e}"))?
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
                    .map(|(id, _, _)| id.as_str())
                    .collect();
                return Err(format!(
                    "llamacpp_adapter: LoRA genes {names:?} requested with media — not supported \
                     on the multimodal bypass path (v1). Genes apply only on the text scheduler \
                     path; send the gene-bearing request without media."
                ));
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
                ));
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
                            sampling_for_closure,
                            &stop_refs,
                        ),
                        llama::MediaKind::Audio => backend_for_blocking.generate_with_audio(
                        allocation,                            &prompt_for_blocking,
                            &media_bytes,
                            max_tokens,
                            sampling_for_closure,
                            &stop_refs,
                        ),
                    }
                })
            )
            .map_err(|e| format!("generate_with_media task panicked: {e}"))?
        };
        let (written, tokens) = result?;
        // What she wrote, read by the render that produced her prompt: content, reasoning and her
        // native calls (assembled by the one owner the streamed path uses). A media turn's output
        // is its text.
        let (text, reasoning, tool_calls) = match &chat_render {
            Some(render) => match super::chat_turn::parse_turn(render, &written, super::chat_turn::generation_end(tokens, max_tokens)) {
                Ok(turn) => (turn.text, turn.reasoning, turn.tool_calls),
                Err(refused) => return Err(refused.to_string()),
            },
            None => (written, None, None),
        };
        let content = crate::inference::native_tool_calls::content_blocks(&text, tool_calls.as_deref());
        let finish_reason = if tool_calls.is_some() { FinishReason::ToolUse } else { FinishReason::Stop };

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

        Ok(TextGenerationResponse {
            text,
            finish_reason,
            model: backend.model_id().to_string(),
            provider: LLAMACPP_PROVIDER_ID.to_string(),
            usage: UsageMetrics {
                input_tokens: 0, // backend doesn't return this currently; future enhancement
                output_tokens: tokens as u32,
                total_tokens: tokens as u32,
                estimated_cost: None,
            },
            response_time_ms: elapsed.as_millis() as u64,
            request_id: format!("llamacpp-{}", continuum_base::time::EpochMs::now().millis()),
            content,
            tool_calls,
            // Split by the model's own reasoning format (the render's parser), never a tag regex.
            reasoning,
            routing: None,
            error: None,
            timing: None,
        })
    }
}
