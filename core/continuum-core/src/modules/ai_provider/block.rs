use super::*;

// Re-open the AIProviderModule impl block so the rest of the methods
// (parse_request, response_to_json, etc.) stay where they were.
impl AIProviderModule {
    /// The persistent gateway-sync task (card ed3661c4). Follows the serving
    /// daemon's `watch` snapshot for the LIFE of the process: whenever a READY
    /// snapshot's `(base_url, active_model)` differs from what the registry
    /// currently advertises, rebuild the gateway adapter against the live
    /// server (fresh `/v1/models` catalog) and REPLACE the registration.
    /// `initial` seeds the already-registered pair from the boot fast-path so
    /// a warm boot doesn't churn one redundant re-register.
    ///
    /// Failure shape: a build/init error while ready (server mid-warm) retries
    /// on a short interval — bounded work, no lock held across waits, and the
    /// registry keeps its previous (possibly stale) entry until the rebuild
    /// succeeds, at which point it is atomically swapped under the write lock.
    pub(super) fn spawn_gateway_sync(
        registry_arc: Arc<RwLock<AdapterRegistry>>,
        initial: Option<(String, Option<String>)>,
    ) {
        tokio::spawn(async move {
            use crate::runtime::boot_status::{boot_status, BootStatusKind};
            const RETRY: std::time::Duration = std::time::Duration::from_secs(3);
            // The daemon installs the watch at its own init; poll briefly
            // until it exists (boot ordering, not a failure state).
            let mut rx = loop {
                match crate::inference::llama_server::serving_state_receiver() {
                    Some(rx) => break rx,
                    None => tokio::time::sleep(RETRY).await,
                }
            };
            let mut synced = initial;
            let mut announced_first = synced.is_some();
            loop {
                let snap = rx.borrow_and_update().clone();
                if snap.ready {
                    let want = (snap.base_url.clone(), snap.active_model.clone());
                    if synced.as_ref() != Some(&want) {
                        match build_gateway_adapter(want.0.clone(), want.1.as_deref()).await {
                            Ok(a) => {
                                let mut reg = registry_arc.write().await;
                                // Replace, never append: deregister sweeps the
                                // base key AND any #N collision duplicates.
                                reg.deregister(crate::inference::llama_server::PROVIDER_ID);
                                reg.register(Arc::new(a), 9);
                                drop(reg);
                                let short = want
                                    .1
                                    .as_deref()
                                    .map(|m| m.rsplit('/').next().unwrap_or(m))
                                    .unwrap_or("(unknown)");
                                if !announced_first {
                                    announced_first = true;
                                    boot_status(
                                        "inference",
                                        BootStatusKind::Ok,
                                        &format!(
                                            "inference gateway registered (sync) — serving {short} @ {}",
                                            want.0
                                        ),
                                    );
                                } else {
                                    continuum_base::probe!(
                                        class = "ai.gateway_resync",
                                        base_url = want.0.as_str(),
                                        model = short,
                                        "gateway adapter re-synced to the live serving snapshot",
                                    );
                                }
                                synced = Some(want);
                            }
                            Err(e) => {
                                continuum_base::probe!(
                                    class = "ai.gateway_resync_retry",
                                    base_url = want.0.as_str(),
                                    error = e.as_str(),
                                    "gateway rebuild against ready snapshot failed — retrying",
                                );
                                tokio::time::sleep(RETRY).await;
                                continue; // re-read the snapshot and retry
                            }
                        }
                    }
                }
                if rx.changed().await.is_err() {
                    break; // daemon dropped its sender — process shutdown
                }
            }
        });
    }

    /// Module logger: the context-scoped one once `initialize` ran, the
    /// process-wide component logger if something logs before that.
    pub(super) fn log(&self) -> &ModuleLogger {
        self.log
            .get_or_init(|| crate::runtime::logger("ai_provider"))
            .as_ref()
    }

    /// Register all available adapters
    pub(super) async fn register_adapters(&self) -> Result<(), String> {
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

        // Per task #162's Box→Arc migration: init-then-register
        // pattern. The registry stores ready-to-serve adapters; we
        // pay the initialize() cost here BEFORE registration. If an
        // adapter's initialize fails (e.g. API key validation
        // failure), we log and skip — per [[no-fallbacks-ever]] we
        // surface the failure rather than substituting a degraded
        // adapter, and other providers can still register.
        //
        // The pattern: `let mut a = X::new(); a.initialize().await?;
        // registry.register(Arc::new(a), priority);` — eight repeats
        // for the cloud-API set below.

        // Only register adapters that have API keys configured.
        if get_secret("DEEPSEEK_API_KEY").is_some() {
            self.log().info("Registering DeepSeek adapter");
            let mut a = OpenAICompatibleAdapter::from_registry("deepseek");
            match a.initialize().await {
                Ok(()) => registry.register(Arc::new(a), 0),
                Err(e) => self
                    .log()
                    .warn(&format!("DeepSeek initialize failed: {e} — not registered")),
            }
        }

        if get_secret("ANTHROPIC_API_KEY").is_some() {
            self.log().info("Registering Anthropic adapter");
            let mut a = AnthropicAdapter::new();
            match a.initialize().await {
                Ok(()) => registry.register(Arc::new(a), 1),
                Err(e) => self.log().warn(&format!(
                    "Anthropic initialize failed: {e} — not registered"
                )),
            }
        }

        if get_secret("OPENAI_API_KEY").is_some() {
            self.log().info("Registering OpenAI adapter");
            let mut a = OpenAICompatibleAdapter::from_registry("openai");
            match a.initialize().await {
                Ok(()) => registry.register(Arc::new(a), 2),
                Err(e) => self
                    .log()
                    .warn(&format!("OpenAI initialize failed: {e} — not registered")),
            }
        }

        if get_secret("GROQ_API_KEY").is_some() {
            self.log().info("Registering Groq adapter");
            let mut a = OpenAICompatibleAdapter::from_registry("groq");
            match a.initialize().await {
                Ok(()) => registry.register(Arc::new(a), 3),
                Err(e) => self
                    .log()
                    .warn(&format!("Groq initialize failed: {e} — not registered")),
            }
        }

        if get_secret("TOGETHER_API_KEY").is_some() {
            self.log().info("Registering Together adapter");
            let mut a = OpenAICompatibleAdapter::from_registry("together");
            match a.initialize().await {
                Ok(()) => registry.register(Arc::new(a), 4),
                Err(e) => self
                    .log()
                    .warn(&format!("Together initialize failed: {e} — not registered")),
            }
        }

        if get_secret("FIREWORKS_API_KEY").is_some() {
            self.log().info("Registering Fireworks adapter");
            let mut a = OpenAICompatibleAdapter::from_registry("fireworks");
            match a.initialize().await {
                Ok(()) => registry.register(Arc::new(a), 5),
                Err(e) => self.log().warn(&format!(
                    "Fireworks initialize failed: {e} — not registered"
                )),
            }
        }

        if get_secret("XAI_API_KEY").is_some() {
            self.log().info("Registering XAI adapter");
            let mut a = OpenAICompatibleAdapter::from_registry("xai");
            match a.initialize().await {
                Ok(()) => registry.register(Arc::new(a), 6),
                Err(e) => self
                    .log()
                    .warn(&format!("XAI initialize failed: {e} — not registered")),
            }
        }

        if get_secret("GOOGLE_API_KEY").is_some() {
            self.log().info("Registering Google adapter");
            let mut a = OpenAICompatibleAdapter::from_registry("google");
            match a.initialize().await {
                Ok(()) => registry.register(Arc::new(a), 7),
                Err(e) => self
                    .log()
                    .warn(&format!("Google initialize failed: {e} — not registered")),
            }
        }

        if get_secret("MISTRAL_API_KEY").is_some() {
            self.log().info("Registering Mistral adapter");
            let mut a = OpenAICompatibleAdapter::from_registry("mistral");
            match a.initialize().await {
                Ok(()) => registry.register(Arc::new(a), 8),
                Err(e) => self
                    .log()
                    .warn(&format!("Mistral initialize failed: {e} — not registered")),
            }
        }

        // llama-server — the local OpenAI-compatible serving gateway. Registered
        // when the serving daemon has a READY model (Contract A), NOT on the
        // presence of an API key: it's a local endpoint with no credential. Its
        // base_url comes from the daemon's reconciled snapshot (the single source
        // of truth for where the gateway lives), and its live /v1/models catalog
        // decides which models it serves. Additive priority for now — making the
        // gateway the *preferred* route is a separate routing-policy change.
        // No served model → no registration → the boot-status block below fails
        // loud (no local fallback), per the no-fallback rule.
        // Gateway registration is REACTIVE, not one-shot (task #71). A cold large
        // GGUF can take longer than any fixed boot bound to finish warming its slot
        // graphs; the old `await_ready_serving(DEFAULT_SERVING_WAIT)` here timed out
        // and NEVER registered, leaving the core permanently gatewayless even though
        // the daemon brought the model up moments later. Now: a SHORT inline wait
        // registers on the spot for the warm-restart / adoption case; if the model
        // is still loading, a detached watcher registers the instant the daemon's
        // snapshot reports ready — however long the cold load takes. This is NOT a
        // fallback: there is still exactly one inference path (the gateway); it
        // simply registers when its backend is actually ready. The serving daemon
        // remains the loud-failure owner if it can never bring a model up, and an
        // inference call arriving before registration fails loud at `select()` (no
        // stand-in). [[fallbacks-are-illegal-fail-loud]]
        let mut gateway_registered = false;
        let mut gateway_pending = false;
        let mut gateway_synced: Option<(String, Option<String>)> = None;
        if let Some(snap) =
            crate::inference::llama_server::await_ready_serving(GATEWAY_FAST_PATH_WAIT).await
        {
            self.log().info("Registering llama-server gateway adapter");
            match build_gateway_adapter(snap.base_url.clone(), snap.active_model.as_deref()).await {
                Ok(a) => {
                    // Idempotent registration (the 5090 #2 mystery, 2026-07-25):
                    // EVERY gateway registration site deregister-sweeps first, so
                    // no path ordering can ever mint a collision-suffixed twin —
                    // whichever fired first, last-writer replaces.
                    registry.deregister(crate::inference::llama_server::PROVIDER_ID);
                    registry.register(Arc::new(a), 9);
                    gateway_registered = true;
                    gateway_synced = Some((snap.base_url, snap.active_model));
                }
                Err(e) => self.log().warn(&format!(
                    "llama-server initialize failed: {e} — not registered"
                )),
            }
        }
        // Persistent gateway SYNC (card ed3661c4): the adapter must TRACK the
        // daemon's ServingSnapshot, never cache it. The one-shot reactive
        // watcher this replaces registered at first-ready and went away — so a
        // model swap, a relaunch onto a scanned port (the 5090 stale-server
        // repro, 2026-07-24), or a window regrow left the gateway advertising a
        // DEAD server's catalog and select() honestly refusing the model that
        // WAS serving. Now one detached task follows the watch forever: on any
        // (base_url, active_model) change while ready it rebuilds the adapter
        // against the LIVE server (fresh /v1/models catalog) and REPLACES the
        // registration (deregister sweeps `#N` duplicates — never a second
        // entry). This also retires the task-#71 registration race: the sync
        // task IS the reactive registrar, for the first ready and every one
        // after. Same bug class as the frozen-window clamp: a stale cache of a
        // live value; the fix is the same — one live source, followed.
        Self::spawn_gateway_sync(self.registry.clone(), gateway_synced);
        if !gateway_registered {
            // Model still cold-loading; the persistent sync task spawned above
            // registers the instant the daemon's snapshot reports ready —
            // however long the cold load takes — and keeps it synced forever.
            gateway_pending = true;
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
        // NO silent local-inference fallback. Our OWN llama-server is THE inference
        // path; it serves our forged GGUF over /v1. The in-process
        // llama.cpp adapter is OPT-IN ONLY (CONTINUUM_LOCAL_LLAMA=1) so the core never
        // registers a local backend by default and never silently REVERTS to local when
        // the gateway is absent — a missing gateway fails loud at select(), it does not
        // get papered over with local inference ([[no-fallbacks-ever]]).
        let local_llama_opt_in =
            continuum_base::config_env::read("CONTINUUM_LOCAL_LLAMA").as_deref() == Some("1");
        if let Some(reg_arc) = crate::model_registry::try_global().filter(|_| local_llama_opt_in) {
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
                // missing" at request time. Resolve via the ONE resolver
                // (declared path OR the projector beside the GGUF in the
                // HF cache), so a self-provisioned sibling reads as present
                // and we don't false-warn on a model that will serve fine.
                let needs_mmproj = model_meta.has(crate::model_registry::types::Capability::Vision)
                    || model_meta.has(crate::model_registry::types::Capability::AudioInput);
                if needs_mmproj
                    && crate::model_registry::artifacts::resolve_mmproj_for_model(model_meta)
                        .is_none()
                {
                    self.log().info(&format!(
                        "Adapter `{}` declares Vision/AudioInput but no mmproj projector \
                         resolves — none declared, and none sits beside the GGUF in the HF \
                         cache. Multimodal calls will hard-error. Pull the model's `*-GGUF` \
                         repo (its projector ships alongside the GGUF) or add \
                         `mmproj_local_path` to the row.",
                        model_meta.id
                    ));
                }
                self.log().info(&format!(
                    "Registering in-process llama.cpp adapter for model `{}`",
                    model_meta.id
                ));
                // Serve at min(model's trained context, a conservative
                // device-independent KV ceiling). The trained context comes
                // from the Model row (`context_window`, hydrated from the
                // GGUF `context_length` header) — never a per-model constant
                // baked in here. A model advertising a very large trained
                // window would otherwise allocate a multi-GB F16 KV cache
                // per seq on load and reliably fail first-decode with
                // `llama_decode returned -3` on any device that can't fit
                // tens of GB of scratch, so we cap it; a model whose trained
                // window is already below the ceiling serves at its real
                // value rather than a fictitious larger one.
                //
                // The cap is now THE REAL BUDGET, not a placeholder. It used to be
                // `const KV_SAFE_CONTEXT_CEILING: u32 = 32_768`, whose own comment said it
                // was standing in "until task #79 lands: available VRAM (via the
                // ResourceGovernor lease) divided by THIS model's KV bytes/token". #79
                // landed — `footprint_for` gives the per-model KV rate and
                // `governed_host_budget` gives the leased budget — and the placeholder was
                // never removed, so every locally-registered GGUF kept serving cognition at
                // 32k no matter what the machine or the model could hold. A placeholder
                // that outlives its own stated precondition is just a clamp.
                // [[never-hardcode-a-context-window-4k-defaults-destroy-the-moe-thesis]]
                //
                // Degrades honestly: an ungoverned host, or a model whose GGUF can't be
                // sized, serves at the model's own trained window — never a fresh invented
                // ceiling. Over-allocation is what this guards (a huge trained window would
                // allocate multi-GB F16 KV per seq and fail first-decode with
                // `llama_decode returned -3`), and the governed budget guards it correctly.
                let effective_context = kv_safe_context(&model_meta);
                let adapter_base = crate::inference::LlamaCppAdapter::with_model_id(
                    gguf_path.clone(),
                    model_meta.id.clone(),
                )
                .with_context_length(effective_context);

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
                // Priority 0 — wins over DMR for the model ids it
                // claims. Init-then-register per #162: the adapter
                // is built by `build_llamacpp_adapter` already in a
                // ready state (no separate initialize() call is
                // wired through the current build flow; the
                // adapter's constructor handles model load).
                let mut adapter = adapter;
                if let Err(e) = adapter.initialize().await {
                    self.log().warn(&format!(
                        "in-process llama.cpp initialize failed: {e} — not registered"
                    ));
                } else {
                    registry.register(Arc::new(adapter), 0);
                }
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
                // Priority 1 — sits BELOW the in-process llama.cpp
                // adapter (priority 0) so DMR only wins for models
                // LlamaCppAdapter doesn't claim. Critical on Mac M5
                // where DMR's container Metal toolchain is degraded
                // vs the host-built bundled llama.cpp (verified
                // 2026-04-19: 33 tok/s container vs 47 tok/s
                // in-process for the same forge model).
                let mut dmr = Self::build_dmr_adapter(&endpoint);
                if let Err(e) = dmr.initialize().await {
                    self.log().warn(&format!(
                        "DMR adapter initialize failed: {e} — not registered"
                    ));
                } else {
                    registry.register(Arc::from(dmr), 1);
                }
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

        // DwarfStar (ds4) sidecar — the V4-Flash deliberator lane (#306).
        // Same probe-then-register shape as DMR: an OPERATOR-managed local
        // OpenAI-compatible endpoint we consume, never spawn (#179 interop
        // doctrine). Registered at priority 2 — below in-process llama.cpp
        // (0) and DMR (1): ds4 only wins for the models it exclusively
        // claims (deepseek-v4 prefix), so it can never shadow a lane. If
        // the sidecar isn't up at boot it simply isn't registered; watchdog
        // parity (re-register when it appears, deregister when it dies)
        // follows once the lifecycle is governed.
        let ds4_up = std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], 8901)),
            Duration::from_secs(1),
        )
        .is_ok();
        if ds4_up {
            self.log()
                .info("Registering DwarfStar (ds4) sidecar adapter (localhost:8901)");
            let mut ds4 = Box::new(OpenAICompatibleAdapter::from_registry("ds4"))
                as Box<dyn AIProviderAdapter>;
            if let Err(e) = ds4.initialize().await {
                self.log().warn(&format!(
                    "ds4 adapter initialize failed: {e} — not registered"
                ));
            } else {
                registry.register(Arc::from(ds4), 2);
            }
        } else {
            self.log().info(
                "ds4 sidecar not reachable on localhost:8901 — deepseek-v4-flash \
                 unavailable this boot (launch ds4-server and reboot, or wait for \
                 watchdog parity)",
            );
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

        // Per task #162: no `registry.initialize_all()` here —
        // each adapter is initialized inline above before being
        // wrapped in Arc and registered. The registry stores
        // ready-to-serve adapters; lifecycle is the constructor's
        // responsibility.

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

        // Intentional boot assertion: announce the ACTIVE inference path so a silent
        // fallback can NEVER go unnoticed again ("we didn't even know it wasn't
        // serving"). Registration only means key-present + adapter-configured — it
        // does NOT mean a model is being SERVED. So we READ the serving daemon's
        // published ServingSnapshot (the same `watch` seam personas bind on in
        // supervisor slice 2) rather than issuing our own /v1/models probe:
        // subscribers READ the snapshot, they do NOT each probe. The daemon owns
        // serving health; this block owns "is there a served model for the gateway
        // path". A missing/empty/unready serving plan surfaces LOUD, never falsely
        // reported ✓. [[fallbacks-are-illegal-fail-loud]]
        {
            use crate::runtime::boot_status::{boot_status, BootStatusKind};
            let local_note = if local_llama_opt_in {
                " (+ CONTINUUM_LOCAL_LLAMA opt-in ALSO active)"
            } else {
                " — sole inference path"
            };
            if gateway_pending {
                // The model is still cold-loading; the reactive watcher spawned
                // above will register the gateway the instant the daemon reports
                // ready (and emit its own ✓/✗ then). This boot line is honest about
                // the in-progress state — NOT a false ✓, NOT a premature ✗.
                boot_status(
                    "inference",
                    BootStatusKind::Degraded,
                    "serving model still loading — inference gateway will register REACTIVELY when \
                     the daemon reports ready (task #71). The gateway is the sole inference path; \
                     calls before it registers fail loud at select(). No local fallback.",
                );
            } else if !gateway_registered {
                boot_status(
                    "inference",
                    BootStatusKind::Failed,
                    "inference gateway NOT registered — the serving daemon brought up NO ready model \
                     within the readiness bound. Inference gateway REQUIRED; no local fallback.",
                );
            } else {
                // Read the daemon's reconciled snapshot (bounded wait for its first
                // reconcile so a boot race still resolves). One source of truth for
                // "what is served" — no redundant HTTP probe of our own.
                match crate::inference::llama_server::await_ready_serving(
                    crate::inference::llama_server::DEFAULT_SERVING_WAIT,
                )
                .await
                {
                    Some(snap) => {
                        let model = snap.active_model.as_deref().unwrap_or("(unknown)");
                        let short = model.rsplit('/').next().unwrap_or(model);
                        boot_status(
                            "inference",
                            if local_llama_opt_in {
                                BootStatusKind::Degraded
                            } else {
                                BootStatusKind::Ok
                            },
                            &format!(
                                "inference gateway @ {base} — serving {short}{local_note}",
                                base = snap.base_url
                            ),
                        );
                    }
                    None => boot_status(
                        "inference",
                        BootStatusKind::Failed,
                        &format!(
                            "serving daemon brought up NO ready model within {secs}s — load one \
                             or check the serving plan. \
                             Inference gateway REQUIRED; no local fallback.{local_note}",
                            secs = crate::inference::llama_server::DEFAULT_SERVING_WAIT.as_secs()
                        ),
                    ),
                }
            }
        }

        Ok(())
    }
}

impl Default for AIProviderModule {
    fn default() -> Self {
        Self::new()
    }
}
