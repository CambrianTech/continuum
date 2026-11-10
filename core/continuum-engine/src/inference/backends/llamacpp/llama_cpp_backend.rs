use super::*;

impl LlamaCppBackend {
    /// Load a GGUF model.
    pub fn load(config: LlamaCppConfig) -> Result<Self, String> {
        let log = runtime::logger("llamacpp");
        if !config.model_path.exists() {
            return Err(format!(
                "Model file not found: {}",
                config.model_path.display()
            ));
        }
        let model_id = config
            .model_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".into());

        let load_start = Instant::now();
        let model = Model::load(
            &config.model_path,
            ModelParams {
                n_gpu_layers: config.n_gpu_layers,
                use_mmap: true,
            },
        )?;
        let caps = ModelCapabilities::from_model(&model);
        log.info(&format!(
            "Loaded {} in {:.2}s (vocab={}, n_ctx_train={}, n_layer={}, \
             n_head={}, n_head_kv={}, kv={}B/tok)",
            model_id,
            load_start.elapsed().as_secs_f64(),
            model.n_vocab(),
            caps.n_ctx_train,
            caps.n_layer,
            caps.n_head,
            caps.n_head_kv,
            caps.kv_bytes_per_token(config.type_k, config.type_v),
        ));

        Ok(Self {
            embedding_lane: super::embedding_lane::EmbeddingLane::new(),
            model: Arc::new(model),
            config,
            model_id,
            caps,
            scheduler: OnceLock::new(),
            mtmd: Mutex::new(None),
            loras: Mutex::new(HashMap::new()),
        })
    }

    /// The model's real capability facts (read once at load).
    /// The pool a context of this backend resides in: device memory when llama offloads to a GPU
    /// and this backend offloads any layer, else host memory, whatever the machine's kind.
    pub(crate) fn context_pool(&self) -> crate::resources::ResourceKind {
        match self.config.n_gpu_layers != 0 && llama::supports_gpu_offload() {
            true => crate::resources::ResourceKind::Vram,
            false => crate::resources::ResourceKind::Ram,
        }
    }

    pub fn capabilities(&self) -> &ModelCapabilities {
        &self.caps
    }

    /// Generation consumes its caller's explicit context allocation. The backend
    /// validates against model capability; it never invents a memory grant.
    pub fn effective_context_length(
        &self,
        allocation: GenerationAllocation,
    ) -> Result<u32, String> {
        let granted = allocation.tokens();
        if granted == 0 || self.caps.n_ctx_train == 0 || granted > self.caps.n_ctx_train {
            return Err(format!(
                "context allocation {granted} outside model capacity {}",
                self.caps.n_ctx_train
            ));
        }
        Ok(granted)
    }

    /// Admit one embedding context on this backend before claiming its memory.
    /// The returned call binds the model to its permit and retains both through
    /// blocking execution, even if its async caller disappears. Generation uses
    /// its separate scheduler context; embeddings clear KV between input texts.
    pub(crate) async fn admit_embedding(
        &self,
    ) -> Result<super::embedding_lane::EmbeddingCall, String> {
        let permit = self.embedding_lane.acquire().await?;
        Ok(super::embedding_lane::EmbeddingCall {
            model: Arc::clone(&self.model),
            permit,
        })
    }

    /// Lazily load the multimodal projector. Returns Err when
    /// `config.mmproj_path` is None (text-only backend) or when the
    /// mmproj file fails to load. Idempotent — caches the loaded
    /// MtmdContext under the mutex.
    pub(super) fn ensure_mtmd(&self) -> Result<Arc<llama::MtmdContext>, String> {
        let mut guard = self
            .mtmd
            .lock()
            .map_err(|e| format!("mtmd lock poisoned: {e}"))?;
        if let Some(existing) = guard.as_ref() {
            return Ok(existing.clone());
        }
        let mmproj = self.config.mmproj_path.as_ref().ok_or_else(|| {
            format!(
                "model {} has no mmproj configured — text-only backend can't process images. \
                 Set `mmproj_local_path` in the Rust catalog (catalog.rs) AND declare Capability::Vision.",
                self.model_id
            )
        })?;
        if !mmproj.exists() {
            return Err(format!(
                "mmproj file declared but missing on disk: {} (model: {})",
                mmproj.display(),
                self.model_id
            ));
        }
        let ctx = llama::MtmdContext::from_file(mmproj, &self.model).map_err(|e| {
            format!(
                "MtmdContext::from_file failed for {}: {e}",
                mmproj.display()
            )
        })?;
        let arc = Arc::new(ctx);
        *guard = Some(arc.clone());
        Ok(arc)
    }

    /// Single-shot multimodal generation: text prompt + one image →
    /// generated text. Bypasses the continuous-batching scheduler
    /// because image encoding produces tokens that aren't trivially
    /// batchable with concurrent text seqs (image tokens have a
    /// fixed positional layout dictated by the projector). Opens a
    /// fresh per-call llama_context, evaluates the image+text via
    /// `MtmdContext::eval_image`, then samples until EOG / max_tokens
    /// / stop sequence. Concurrent multimodal calls each get their
    /// own context — slower than batched but isolated and correct.
    ///
    /// `prompt_with_marker` MUST contain the model's media marker
    /// (see `llama::MtmdContext::default_marker()`, typically
    /// `<__media__>`) — that's where the image tokens splice in. If
    /// the caller's text doesn't include it, `mtmd_tokenize` returns
    /// an error and we surface it.
    pub fn generate_with_image(
        &self,
        allocation: GenerationAllocation,
        prompt_with_marker: &str,
        image_bytes: &[u8],
        max_tokens: usize,
        sampling: SamplingConfig,
        stop_sequences: &[&str],
    ) -> Result<(String, usize), String> {
        self.generate_with_media(
            allocation,
            prompt_with_marker,
            image_bytes,
            max_tokens,
            sampling,
            stop_sequences,
            llama::MediaKind::Image,
        )
    }

    /// Audio analogue of `generate_with_image`. Same single-shot
    /// per-call-context pattern; the mtmd projector path inside auto-
    /// detects audio vs image from the bytes' magic numbers but the
    /// caller's `MediaKind::Audio` selects the capability check
    /// (`supports_audio` instead of `supports_vision`) and shapes error
    /// messages so a mistakenly-routed audio call doesn't surface as a
    /// confusing "vision unsupported" error.
    ///
    /// Supported audio container formats are whatever miniaudio
    /// understands inside the vendored llama.cpp build (wav, mp3, flac
    /// per upstream `tools/mtmd/mtmd-helper.h`). The caller is expected
    /// to deliver one of those — re-encoding from other formats is a
    /// sensory-bridge concern, not the backend's.
    pub fn generate_with_audio(
        &self,
        allocation: GenerationAllocation,
        prompt_with_marker: &str,
        audio_bytes: &[u8],
        max_tokens: usize,
        sampling: SamplingConfig,
        stop_sequences: &[&str],
    ) -> Result<(String, usize), String> {
        self.generate_with_media(
            allocation,
            prompt_with_marker,
            audio_bytes,
            max_tokens,
            sampling,
            stop_sequences,
            llama::MediaKind::Audio,
        )
    }

    /// Internal workhorse for single-shot multimodal generation. Mirrors
    /// the eval+sample loop the public methods need; the only thing that
    /// differs per modality is the capability check (vision vs audio
    /// projector support) and which `MtmdContext::eval_*` method runs.
    /// Centralizing here avoids the 150-LOC duplication that would land
    /// if image and audio paths were copy-pasted.
    pub(super) fn generate_with_media(
        &self,
        allocation: GenerationAllocation,
        prompt_with_marker: &str,
        media_bytes: &[u8],
        max_tokens: usize,
        sampling: SamplingConfig,
        stop_sequences: &[&str],
        kind: llama::MediaKind,
    ) -> Result<(String, usize), String> {
        let log = runtime::logger("llamacpp");
        let start = Instant::now();
        let mtmd = self.ensure_mtmd()?;
        match kind {
            llama::MediaKind::Image => {
                if !mtmd.supports_vision() {
                    return Err(format!(
                        "model {}'s mmproj does not declare vision support — \
                         caller passed an image but the projector is text-only or audio-only",
                        self.model_id
                    ));
                }
            }
            llama::MediaKind::Audio => {
                if !mtmd.supports_audio() {
                    return Err(format!(
                        "model {}'s mmproj does not declare audio support — \
                         caller passed audio but the projector is text-only or vision-only",
                        self.model_id
                    ));
                }
            }
        }

        // Per-call context — see method-level docstring on why we don't
        // share the scheduler's context.
        //
        // Consume the caller's allocation after validating model capability.
        let per_seq = self.effective_context_length(allocation)?;
        let mut ctx = self
            .model
            .new_context(llama::ContextParams {
                n_ctx: per_seq,
                n_batch: self.config.n_batch,
                n_ubatch: self.config.n_ubatch,
                n_seq_max: 1,
                flash_attn: self.config.flash_attn,
                fused_gdn_ar: self.config.fused_gdn_ar,
                fused_gdn_ch: self.config.fused_gdn_ch,
                type_k: self.config.type_k,
                type_v: self.config.type_v,
                embeddings: false,
                pooling_type: llama::PoolingType::None,
                // MoE expert-selection observer — None for now; the K3 serving path
                // sets a LiveExpertObserver here to feed the residency PGO tally.
                expert_observer: None,
            })
            .map_err(|e| format!("new_context failed: {e}"))?;

        // Eval text + media into the context, advancing n_past.
        let eval_params = llama::MtmdEvalParams {
            n_past: 0,
            n_batch: self.config.n_batch as i32,
            seq_id: 0,
            logits_last: true,
        };
        let eval_result = match kind {
            llama::MediaKind::Image => {
                mtmd.eval_image(&mut ctx, prompt_with_marker, media_bytes, &eval_params)
            }
            llama::MediaKind::Audio => {
                mtmd.eval_audio(&mut ctx, prompt_with_marker, media_bytes, &eval_params)
            }
        };
        let n_past = eval_result.map_err(|e| format!("mtmd eval ({:?}) failed: {e}", kind))?;
        log.info(&format!(
            "mtmd eval done ({:?}): prompt+media consumed {} positions in {}ms",
            kind,
            n_past,
            start.elapsed().as_millis()
        ));

        // Sample-until-done loop. Mirrors LlamaCppBackend::generate but
        // single-seq, no scheduler. EOG / max_tokens / stop-sequence are
        // the three exit conditions, same shape.
        let mut sampler = if sampling.temperature <= 0.0 && sampling.grammar.is_none() {
            llama::Sampler::greedy()
        } else {
            let mut chain = llama::Sampler::chain();
            if let Some(g) = sampling.grammar.as_ref() {
                chain = chain.grammar(&self.model, g, "root");
            }
            if sampling.top_k > 0 {
                chain = chain.top_k(sampling.top_k as i32);
            }
            if sampling.top_p > 0.0 && sampling.top_p < 1.0 {
                chain = chain.top_p(sampling.top_p as f32, 1);
            }
            chain = chain.penalties(self.model.n_vocab(), 64, sampling.repeat_penalty, 0.0, 0.0);
            let temp = if sampling.temperature > 0.0 {
                sampling.temperature as f32
            } else {
                0.01
            };
            chain.temp(temp).dist(42).build()
        };

        let mut output = String::new();
        let mut pos = n_past;
        let mut tokens_generated = 0usize;
        // Sample at -1 = "last logits in last batch" — same convention
        // brew's mtmd-cli uses (mtmd-cli.cpp:186 calls
        // common_sampler_sample(smpl, lctx, -1) right after eval). After
        // mtmd_helper_eval_chunks with logits_last=true, the final
        // text-batch's last token has logits set and llama_get_logits_ith
        // honors -1 as that position.
        loop {
            let token = sampler.sample(&ctx, -1);
            if self.model.is_eog_token(token) {
                break;
            }
            let piece = self.model.token_to_piece(token);
            output.push_str(&piece);
            tokens_generated += 1;
            // Stop sequence early-exit — same end-of-output trim shape
            // as the scheduler path.
            if stop_sequences.iter().any(|s| output.ends_with(s)) {
                break;
            }
            if tokens_generated >= max_tokens {
                break;
            }
            // Push the sampled token back so the next decode can advance.
            let mut batch = llama::Batch::allocated(1, 1);
            batch.push(token, pos, &[0], true);
            if let Err(e) = ctx.decode(&batch) {
                log.warn(&format!("decode failed mid-generation: {e}"));
                break;
            }
            pos += 1;
        }

        log.info(&format!(
            "generate_with_image done: {} tokens in {}ms ({:.1} tok/s)",
            tokens_generated,
            start.elapsed().as_millis(),
            tokens_generated as f64 / start.elapsed().as_secs_f64().max(0.001)
        ));
        Ok((output, tokens_generated))
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Model's trained context length, straight from the GGUF metadata.
    /// Single source of truth — never hardcode a context window in
    /// adapters or RAG budgeters; ask this.
    pub fn n_ctx_train(&self) -> u32 {
        self.model.n_ctx_train()
    }

    /// Model's embedded chat template (Jinja-style string). Used by
    /// adapters to render messages through `llama::render_chat`. None
    /// means the model carries no template — caller decides what to do
    /// (error, default, etc.) instead of a silent fallback.
    pub fn model_chat_template(&self) -> Option<String> {
        self.model.chat_template()
    }

    /// Ensure a LoRA adapter is loaded (idempotent). Used by genome paging.
    pub fn ensure_adapter(&self, id: &str, path: &Path) -> Result<(), String> {
        let mut guard = self
            .loras
            .lock()
            .map_err(|e| format!("LoRA lock poisoned: {e}"))?;
        if guard.contains_key(id) {
            return Ok(());
        }
        let adapter = Arc::new(self.model.load_lora(path)?);
        guard.insert(id.to_string(), adapter);
        Ok(())
    }

    /// Lazily spawn (or get) the scheduler. Single instance per backend —
    /// owns the shared Context and the OS-thread driver loop.
    pub(super) fn scheduler(&self, allocation: GenerationAllocation) -> Result<&Scheduler, String> {
        let per_seq = self.effective_context_length(allocation)?;
        if self.config.n_seq_max == 0 {
            return Err("generation requires a nonzero sequence allocation".into());
        }
        let total_n_ctx = per_seq
            .checked_mul(self.config.n_seq_max)
            .ok_or("context allocation exceeds native representation")?;
        let (bound, scheduler) = self.scheduler.get_or_init(|| {
            (
                allocation,
                Scheduler::spawn(
                    self.model.clone(),
                    SchedulerConfig {
                        n_ctx: total_n_ctx,
                        n_batch: self.config.n_batch,
                        n_ubatch: self.config.n_ubatch,
                        n_seq_max: self.config.n_seq_max,
                        flash_attn: self.config.flash_attn,
                        fused_gdn_ar: self.config.fused_gdn_ar,
                        fused_gdn_ch: self.config.fused_gdn_ch,
                        type_k: self.config.type_k,
                        type_v: self.config.type_v,
                    },
                ),
            )
        });
        if *bound != allocation {
            return Err(
                "scheduler allocation changed; release the existing backend before resizing".into(),
            );
        }
        Ok(scheduler)
    }

    /// Generate text. Routes through the continuous-batching scheduler:
    /// the request becomes one sequence in the shared batch and tokens
    /// stream back as they're sampled. Multiple concurrent generate()
    /// calls share the single Context — weights are read once per decode
    /// step and used to advance every active sequence in parallel.
    ///
    /// `active_loras` is `(id, scale)` pairs naming genes previously paged
    /// in via [`ensure_adapter`]. They are resolved to live adapter handles
    /// and applied context-level by the scheduler before decode (genome
    /// paging). A requested id that was never `ensure_adapter`'d is a hard
    /// error — never a silent base-model run (Rule 2: fail loud).
    pub fn generate(
        &self,
        allocation: GenerationAllocation,
        prompt: &str,
        max_tokens: usize,
        sampling: SamplingConfig,
        stop_sequences: &[&str],
        active_loras: &[(String, f32)],
    ) -> Result<(String, usize), String> {
        // Forwards to the persona-aware variant with persona_id=None so
        // test rigs and ad-hoc probes don't need to change. Production
        // adapter calls go through generate_for_persona() so the registry
        // can attribute KV bytes per-persona.
        self.generate_for_persona(
            allocation,
            None,
            prompt,
            max_tokens,
            sampling,
            stop_sequences,
            active_loras,
        )
    }

    /// Same as `generate` but threads a `persona_id` through to the
    /// scheduler so the registry can attribute the seq slot's KV bytes
    /// to the right persona. Pass `None` for test/ad-hoc paths that
    /// shouldn't appear in per-persona accounting.
    ///
    /// `persona_id` is forwarded as-is into `ActiveSeq::persona_id`. The
    /// actual registry reporting (Piece 2 of the substrate work) hooks
    /// into seq alloc / Done events inside the scheduler — this method's
    /// only job here is to deliver the value.
    pub fn generate_for_persona(
        &self,
        allocation: GenerationAllocation,
        persona_id: Option<uuid::Uuid>,
        prompt: &str,
        max_tokens: usize,
        sampling: SamplingConfig,
        stop_sequences: &[&str],
        active_loras: &[(String, f32)],
    ) -> Result<(String, usize), String> {
        let log = runtime::logger("llamacpp");
        let gen_start = Instant::now();
        let prompt_len_chars = prompt.len();

        // Channel for streaming tokens back from the scheduler.
        let (response_tx, mut response_rx) = tokio::sync::mpsc::unbounded_channel::<TokenEvent>();

        // Resolve requested genes (`id`, scale) to live adapter handles from
        // the cache BEFORE crossing to the scheduler thread (which can't lock
        // our cache). A requested id that was never paged in via
        // `ensure_adapter` is a hard error — never a silent base-model run
        // (Rule 2: fail loud, name the cause). Empty in the common no-gene
        // case, so the base path pays nothing.
        let resolved_loras: Vec<(String, Arc<LoraAdapter>, f32)> = if active_loras.is_empty() {
            Vec::new()
        } else {
            let guard = self
                .loras
                .lock()
                .map_err(|e| format!("LoRA cache lock poisoned: {e}"))?;
            active_loras
                .iter()
                .map(|(id, scale)| {
                    guard
                        .get(id)
                        .map(|h| (id.clone(), Arc::clone(h), *scale))
                        .ok_or_else(|| {
                            format!(
                                "LoRA gene '{id}' requested but not loaded — call \
                                 ensure_adapter(id, path) before generate (Rule 2: \
                                 refuse to silently run the base model in its place)"
                            )
                        })
                })
                .collect::<Result<Vec<_>, String>>()?
        };

        // Caller passes the full SamplingConfig (the value-object pattern
        // — adding fields like `grammar` doesn't require changing this
        // signature). Previously this path silently overwrote the caller's
        // top_k/top_p/repeat_penalty fields with no-op defaults.
        let req = GenerationRequest {
            prompt: prompt.to_string(),
            max_tokens,
            sampling,
            stop_sequences: stop_sequences.iter().map(|s| s.to_string()).collect(),
            active_loras: resolved_loras,
            response_tx,
            persona_id,
        };

        self.scheduler(allocation)?.enqueue(req)?;

        // Collect tokens from the channel until Done/Error. We're called
        // synchronously from spawn_blocking by CandleAdapter — block_in_place
        // a tokio runtime handle to await the channel.
        //
        // Stop-sequence trimming happens here (at the boundary): the
        // scheduler emits the stop sequence's tokens before signaling Done,
        // so we strip them from the collected output.
        let mut output = String::new();
        let runtime_handle = tokio::runtime::Handle::try_current().ok();

        loop {
            let event = if let Some(ref h) = runtime_handle {
                h.block_on(response_rx.recv())
            } else {
                // Fallback for non-tokio callers (e.g. tests). Spin briefly.
                let mut tries = 0u32;
                loop {
                    match response_rx.try_recv() {
                        Ok(e) => break Some(e),
                        Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                            break None;
                        }
                        Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                            std::thread::sleep(std::time::Duration::from_millis(2));
                            tries += 1;
                            if tries > 60_000 {
                                // 2 minutes without a token — treat as fatal.
                                break None;
                            }
                        }
                    }
                }
            };
            match event {
                Some(TokenEvent::Token(piece)) => {
                    output.push_str(&piece);
                }
                Some(TokenEvent::Done {
                    tokens_generated,
                    elapsed_ms,
                }) => {
                    // The scheduler's own count is the receipt; a per-piece tally here
                    // was never read (rustc: value assigned is never read).
                    let n_decoded = tokens_generated;
                    let elapsed = gen_start.elapsed();
                    log.info(&format!(
                        "Generated {} tokens in {:.3}s ({:.1} tok/s, scheduler={}ms, prompt={}chars)",
                        n_decoded,
                        elapsed.as_secs_f64(),
                        n_decoded as f64 / elapsed.as_secs_f64().max(0.001),
                        elapsed_ms,
                        prompt_len_chars
                    ));
                    // Trim trailing stop sequence(s) — scheduler emits them
                    // before signaling Done.
                    for s in stop_sequences {
                        if output.ends_with(s) {
                            output.truncate(output.len() - s.len());
                        }
                    }
                    return Ok((output, n_decoded));
                }
                Some(TokenEvent::Error(e)) => {
                    return Err(format!("scheduler error: {e}"));
                }
                None => {
                    return Err("scheduler closed without Done event".to_string());
                }
            }
        }
    }
}
