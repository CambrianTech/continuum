use super::*;
use crate::ai::inference_error::InferenceError;

/// A turn's prompt and the head of it her next act will hold unchanged (her view, without the
/// generation prompt), which the engine checkpoints her at. `None` for a turn with no view.
#[derive(Debug, Clone, Copy)]
pub struct TurnPrompt<'a> {
    pub text: &'a str,
    pub stable_prefix: Option<&'a str>,
    /// How much her mind needs this turn: the Core ranks waiting turns by it.
    pub urgency: continuum_sage::component::Urgency,
    /// The submitted request's id, carried into the engine's transitions.
    pub request: Option<&'a str>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum EmbedPlanError {
    #[error(transparent)]
    Quote(#[from] llama::ContextQuoteError),
    #[error("{0}")]
    NoWindow(&'static str),
}

impl From<EmbedPlanError> for String {
    fn from(error: EmbedPlanError) -> Self { error.to_string() }
}

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
            .unwrap_or_else(|| "unknown".into()); // unwrap_or_else: a model path with no file stem is named "unknown" in logs and receipts

        let load_start = Instant::now();
        let model = Model::load(
            &config.model_path,
            ModelParams {
                n_gpu_layers: config.n_gpu_layers,
                use_mmap: true,
            },
        )?;
        let weights = model.weight_allocation();
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
            held_views: super::super::persona_scheduler::HeldViews::default(),
            chat: OnceLock::new(),
            model: Arc::new(model),
            config,
            model_id,
            weights,
            embed_planner: OnceLock::new(),
            embed_residency: OnceLock::new(),
            caps,
            scheduler: OnceLock::new(),
            mtmd: Mutex::new(None),
            loras: Mutex::new(HashMap::new()),
        })
    }

    pub(crate) fn pooled_bytes(&self, width: u32) -> Result<super::ContextBytes, EmbedPlanError> {
        let planner = match self.embed_planner.get() {
            Some(planner) => planner,
            None => {
                let planner = llama::PooledContextPlanner::open(&self.config.model_path, self.config.n_gpu_layers)
                    .map_err(llama::ContextQuoteError::from)?;
                self.embed_planner.get_or_init(|| planner)
            }
        };
        let bytes = planner.quote(super::embedding_batch::pooled_params(width))?;
        Ok(super::ContextBytes { host: bytes.host, device: bytes.device })
    }

    pub(crate) fn embed_residency(&self) -> Result<&crate::inference::embed_lane_memory::EmbedResidency, EmbedPlanError> {
        if let Some(residency) = self.embed_residency.get() { return Ok(residency); }
        let window = super::EmbedWindow::for_model(self.caps.n_ctx_train).ok_or(EmbedPlanError::NoWindow(super::NO_TRAINED_CONTEXT))?;
        let need = crate::inference::embed_lane_memory::EmbedNeed {
            weights: super::ContextBytes { device: self.weights.device, host: self.weights.host },
            context: self.pooled_bytes(window.tokens())?,
        };
        Ok(self.embed_residency.get_or_init(|| crate::inference::embed_lane_memory::EmbedResidency::claim(need)))
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
        sampler: llama::TurnSampler,
        stop_sequences: &[&str],
    ) -> Result<Answer, InferenceError> {
        self.generate_with_media(
            allocation,
            prompt_with_marker,
            image_bytes,
            max_tokens,
            sampler,
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
        sampler: llama::TurnSampler,
        stop_sequences: &[&str],
    ) -> Result<Answer, InferenceError> {
        self.generate_with_media(
            allocation,
            prompt_with_marker,
            audio_bytes,
            max_tokens,
            sampler,
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
        mut sampler: llama::TurnSampler,
        stop_sequences: &[&str],
        kind: llama::MediaKind,
    ) -> Result<Answer, InferenceError> {
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
                    ).into());
                }
            }
            llama::MediaKind::Audio => {
                if !mtmd.supports_audio() {
                    return Err(format!(
                        "model {}'s mmproj does not declare audio support — \
                         caller passed audio but the projector is text-only or vision-only",
                        self.model_id
                    ).into());
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
            n_past: llama::Pos::ZERO,
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
        let prefill = start.elapsed();
        log.info(&format!(
            "mtmd eval done ({:?}): prompt+media consumed {} positions in {}ms",
            kind,
            n_past,
            start.elapsed().as_millis()
        ));

        // Sample-until-done loop, single-seq, no scheduler, with the sampler the provider built for
        // her turn. EOG / max_tokens / stop-sequence are the three exit conditions.
        let mut output = String::new();
        let mut pos = n_past;
        let mut tokens_generated = 0usize;
        // Sample at -1 = "last logits in last batch" — same convention
        // brew's mtmd-cli uses (mtmd-cli.cpp:186 calls
        // common_sampler_sample(smpl, lctx, -1) right after eval). After
        // mtmd_helper_eval_chunks with logits_last=true, the final
        // text-batch's last token has logits set and llama_get_logits_ith
        // honors -1 as that position.
        let capacity = super::super::persona_scheduler::Capacity::new(ctx.n_ctx() as usize);
        let end = loop {
            let token = sampler.sample(&ctx, -1)?;
            if self.model.is_eog_token(token) {
                break TurnEnd::Finished;
            }
            let piece = self.model.token_to_piece(token);
            output.push_str(&piece);
            tokens_generated += 1;
            // Stop sequence early-exit — same end-of-output trim shape
            // as the scheduler path.
            if stop_sequences.iter().any(|s| output.ends_with(s)) {
                break TurnEnd::Stopped;
            }
            if tokens_generated >= max_tokens {
                break TurnEnd::AtCap;
            }
            if !capacity.holds(pos) {
                break TurnEnd::CapacityReached;
            }
            // Push the sampled token back so the next decode can advance; a refused decode is the
            // turn's failure, never a quietly shorter answer.
            let mut batch = llama::Batch::allocated(1, 1);
            batch.push(token, pos, &[0], true);
            ctx.decode(&batch).map_err(String::from)?;
            let Ok(next) = pos.advanced(1) else { break TurnEnd::CapacityReached };
            pos = next;
        };

        log.info(&format!(
            "generate_with_image done: {} tokens in {}ms ({:.1} tok/s)",
            tokens_generated,
            start.elapsed().as_millis(),
            tokens_generated as f64 / start.elapsed().as_secs_f64().max(0.001)
        ));
        let elapsed = start.elapsed();
        let facts = TurnFacts {
            // the positions the projector consumed
            prompt_tokens: usize::from(n_past),
            reused_tokens: 0,
            generated: tokens_generated,
            prefill,
            decode: elapsed.saturating_sub(prefill),
            elapsed,
            end,
        };
        Ok(Answer { text: output, facts })
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

    /// Render one chat turn through the model's own chat handling (llama.cpp common/chat): the
    /// OpenAI-shaped `request` (messages, tools, tool_choice) with its tools and reasoning. The
    /// templates load once, at the first turn, and decide there: a GGUF carrying its default OR its
    /// named `tool_use` template loads its own (common/chat promotes a lone named one to default,
    /// and any override would mask it); only a GGUF with neither takes `fallback_template`, which is
    /// asked for at that one init and never per turn. The lock is held for the render alone, so it
    /// is released before decode. The returned render keeps its parser for every partial and the
    /// final parse of this request.
    pub fn render_chat_turn(
        &self,
        request: &str,
        fallback_template: impl FnOnce() -> Option<String>,
        reasoning: llama::ReasoningFormat,
    ) -> Result<llama::ChatRender, llama::ChatRefusal> {
        let templates = self
            .chat
            .get_or_init(|| {
                let native = self.model.has_chat_template(None) || self.model.has_chat_template(Some(c"tool_use"));
                let fallback = if native { None } else { fallback_template() };
                llama::ChatTemplates::new(&self.model, fallback.as_deref()).map(Mutex::new)
            })
            .as_ref()
            .map_err(Clone::clone)?;
        // A render reads the templates and never mutates them, so a lock poisoned by a panicking
        // render leaves them intact.
        let templates = templates.lock().unwrap_or_else(std::sync::PoisonError::into_inner); // unwrap_or_else: a poisoned render lock guards immutable templates
        templates.render(request, reasoning)
    }

    /// Her turn's sampler on this model: from `render` when her turn has one (its tool grammar,
    /// generation prompt and thinking tags, with `think_budget`), else from `sampling` alone.
    pub fn turn_sampler(
        &self,
        render: Option<&llama::ChatRender>,
        sampling: &SamplingConfig,
        think_budget: Option<u32>,
    ) -> Result<llama::TurnSampler, llama::ChatRefusal> {
        match render {
            Some(render) => render.sampler(&self.model, sampling.knobs(), think_budget),
            None => llama::TurnSampler::plain(&self.model, sampling.knobs(), sampling.grammar.as_deref()),
        }
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

    /// Lazily spawn (or get) the persona scheduler. Single instance per backend: it owns the context
    /// and the driver thread. The allocation is the Core's whole context, and the mind swapped in has
    /// all of it: one mind decodes at a time, so nothing divides it among minds.
    pub(super) fn scheduler(&self, allocation: GenerationAllocation) -> Result<&Scheduler, String> {
        let total_n_ctx = self.effective_context_length(allocation)?;
        let (bound, scheduler) = self.scheduler.get_or_init(|| {
            (
                allocation,
                Scheduler::spawn(
                    self.model.clone(),
                    SchedulerConfig {
                        model: self.config.model_path.display().to_string(),
                        n_ctx: total_n_ctx,
                        n_batch: self.config.n_batch,
                        n_ubatch: self.config.n_ubatch,
                        type_k: self.config.type_k,
                        type_v: self.config.type_v,
                        lane_algorithm: self.config.lane_algorithm,
                    },
                ),
            )
        });
        if *bound != allocation {
            return Err(
                "scheduler allocation changed; release the existing backend before resizing".into(),
            );
        }
        match scheduler {
            Ok(scheduler) => Ok(scheduler),
            Err(refused) => Err(refused.to_string()),
        }
    }

    /// Start the persona scheduler for `allocation` (idempotent) and return its lifecycle.
    pub fn start_scheduler(
        &self,
        allocation: GenerationAllocation,
    ) -> Result<tokio::sync::watch::Receiver<super::super::persona_scheduler::EngineState>, String> {
        Ok(self.scheduler(allocation)?.state())
    }

    /// Prepare and enqueue on a blocking worker. The caller owns the returned event receiver.
    pub fn submit_for_persona(
        &self,
        allocation: GenerationAllocation,
        persona_id: Option<uuid::Uuid>,
        turn: TurnPrompt<'_>,
        max_tokens: usize,
        sampler: llama::TurnSampler,
        stop_sequences: &[&str],
        active_loras: &[(String, f32)],
    ) -> Result<TurnAnswer, String> {
        let gen_start = Instant::now();
        let prompt = turn.text;
        let prompt_len_chars = prompt.len();

        // Channel for streaming tokens back from the scheduler.
        let (response_tx, response_rx) = tokio::sync::mpsc::unbounded_channel::<TokenEvent>();

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

        // Tokenized here, on this act's own thread, so the engine thread never tokenizes.
        let (prompt, view_end) = self.held_views.prepare(&self.model, persona_id, prompt, turn.stable_prefix)?;
        let prompt_tokens = prompt.len();
        let req = GenerationRequest {
            prompt,
            view_end,
            max_tokens,
            sampler,
            stop_sequences: stop_sequences.iter().map(|s| s.to_string()).collect(),
            active_loras: resolved_loras,
            response_tx,
            persona_id,
            urgency: turn.urgency,
            request: turn.request.map(super::super::persona_scheduler::RequestId::new),
            submitted: Instant::now(),
        };

        if let Err(refused) = self.scheduler(allocation)?.enqueue(req) {
            return Err(refused.to_string());
        }
        continuum_base::probe!(
            class = "persona.engine.enqueued",
            request = ?turn.request,
            persona = ?persona_id,
            prompt_tokens = prompt_tokens,
            since_submit = ?gen_start.elapsed(),
            "a turn was handed to the scheduler"
        );

        Ok(TurnAnswer { events: response_rx, started: gen_start, prompt_chars: prompt_len_chars, stops: stop_sequences.iter().map(|s| s.to_string()).collect() })
    }
}

/// Dropping a turn drops its receiver even when the native driver is busy.
pub struct TurnAnswer {
    events: tokio::sync::mpsc::UnboundedReceiver<TokenEvent>,
    started: Instant,
    prompt_chars: usize,
    stops: Vec<String>,
}

/// What the model wrote, with what the engine measured of writing it.
#[derive(Debug, Clone)]
pub struct Answer {
    pub text: String,
    pub facts: TurnFacts,
}

impl TurnAnswer {
    /// Until her first token arrives (a test's proof that her turn is running, never a timed guess).
    /// The token is consumed; a test that waits on it does not read the answer's text.
    #[cfg(test)]
    pub(crate) async fn until_first_token(&mut self) {
        while let Some(event) = self.events.recv().await {
            if matches!(event, TokenEvent::Token(_)) {
                return;
            }
        }
    }

    pub async fn collect(mut self) -> Result<Answer, InferenceError> {
        let mut output = String::new();
        loop {
            match self.events.recv().await {
                Some(TokenEvent::Token(piece)) => {
                    output.push_str(&piece);
                }
                Some(TokenEvent::Done(facts)) => {
                    let elapsed = self.started.elapsed();
                    runtime::logger("llamacpp").info(&format!(
                        "Generated {} tokens in {:.3}s ({:.1} tok/s, scheduler={}ms, prompt={}chars)",
                        facts.generated,
                        elapsed.as_secs_f64(),
                        facts.generated as f64 / elapsed.as_secs_f64().max(0.001),
                        facts.elapsed.as_millis(),
                        self.prompt_chars
                    ));
                    // Trim trailing stop sequence(s) — scheduler emits them
                    // before signaling Done.
                    for s in &self.stops {
                        if output.ends_with(s.as_str()) {
                            output.truncate(output.len() - s.len());
                        }
                    }
                    return Ok(Answer { text: output, facts });
                }
                Some(TokenEvent::Exceeded(exceeded)) => {
                    // The Core's own numbers outrank any window cached from a plan: her caller
                    // refits to them, never resends this request.
                    return Err(refit_to(exceeded));
                }
                Some(TokenEvent::Error(e)) => {
                    return Err(InferenceError::Transient(format!("scheduler error: {e}")));
                }
                None => {
                    return Err(InferenceError::Transient("scheduler closed without Done event".to_string()));
                }
            }
        }
    }
}

/// The Core's refusal as the typed error her caller refits on. Counts outside the error's token
/// range are a broken receipt, not a number to clamp (as `serving_guard` types a measured overflow).
fn refit_to(exceeded: crate::inference::backends::persona_scheduler::Exceeded) -> InferenceError {
    match (u32::try_from(exceeded.prompt_tokens), u32::try_from(exceeded.capacity)) {
        (Ok(requested), Ok(available)) => InferenceError::ContextExceeded { requested, available },
        _ => InferenceError::Protocol("the Core's refusal counts exceed the typed error's token range".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a turn the Core refused at its door for want of room reaching her caller as
    // a transient fault (text), so it was resent unchanged; it must arrive as `ContextExceeded` with
    // the Core's own numbers, the one refusal cognition refits on (the M5, 2026-11-21).
    #[tokio::test]
    async fn a_turn_the_core_cannot_fit_reaches_its_caller_as_context_exceeded() {
        let (refusal, events) = tokio::sync::mpsc::unbounded_channel();
        let exceeded = crate::inference::backends::persona_scheduler::Exceeded { prompt_tokens: 24_202, capacity: 24_320 };
        assert!(refusal.send(TokenEvent::Exceeded(exceeded)).is_ok());
        let answer = TurnAnswer { events, started: Instant::now(), prompt_chars: 0, stops: Vec::new() };
        let refused = answer.collect().await;
        assert!(
            matches!(refused, Err(InferenceError::ContextExceeded { requested: 24_202, available: 24_320 })),
            "{refused:?}"
        );
    }
}
