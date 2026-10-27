use super::*;

pub fn providers() -> Vec<Provider> {
    vec![
        provider(ProviderSpec {
            id: "anthropic",
            name: "Anthropic",
            base_url: "https://api.anthropic.com",
            api_key_env: Some("ANTHROPIC_API_KEY"),
            default_model: Some("claude-sonnet-4-5-20250929"),
            auth: AuthKind::ApiKey,
            kind: ProviderKind::Cloud,
            model_prefixes: &["claude"],
            ..Default::default()
        }),
        provider(ProviderSpec {
            id: "openai",
            name: "OpenAI",
            base_url: "https://api.openai.com",
            api_key_env: Some("OPENAI_API_KEY"),
            default_model: Some("gpt-4-turbo-preview"),
            auth: AuthKind::Bearer,
            kind: ProviderKind::Cloud,
            model_prefixes: &["gpt", "o1", "o3"],
            // Embeddings + image-gen are MODEL facts (the `text-embedding-3-*`
            // and `dall-e-3` rows declare `Capability::Embedding` /
            // `ImageGeneration`); the adapter derives them by scanning rows
            // (#68). Everything else is the cloud default (native tools, keep
            // thinking) — no provider-level capability block needed.
            ..Default::default()
        }),
        provider(ProviderSpec {
            id: "deepseek",
            name: "DeepSeek",
            base_url: "https://api.deepseek.com",
            api_key_env: Some("DEEPSEEK_API_KEY"),
            default_model: Some("deepseek-chat"),
            auth: AuthKind::Bearer,
            kind: ProviderKind::Cloud,
            model_prefixes: &["deepseek"],
            ..Default::default()
        }),
        provider(ProviderSpec {
            id: "together",
            name: "Together AI",
            base_url: "https://api.together.xyz",
            api_key_env: Some("TOGETHER_API_KEY"),
            default_model: Some("meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo"),
            auth: AuthKind::Bearer,
            kind: ProviderKind::Cloud,
            model_prefixes: &["togethercomputer/", "meta-llama/"],
            ..Default::default()
        }),
        provider(ProviderSpec {
            id: "groq",
            name: "Groq",
            base_url: "https://api.groq.com/openai",
            api_key_env: Some("GROQ_API_KEY"),
            default_model: Some("llama-3.1-8b-instant"),
            auth: AuthKind::Bearer,
            kind: ProviderKind::Cloud,
            model_prefixes: &["llama-3", "mixtral", "gemma2"],
            ..Default::default()
        }),
        provider(ProviderSpec {
            id: "fireworks",
            name: "Fireworks AI",
            base_url: "https://api.fireworks.ai/inference",
            api_key_env: Some("FIREWORKS_API_KEY"),
            default_model: Some("accounts/fireworks/models/llama-v3p3-70b-instruct"),
            auth: AuthKind::Bearer,
            kind: ProviderKind::Cloud,
            model_prefixes: &["accounts/fireworks/"],
            ..Default::default()
        }),
        provider(ProviderSpec {
            id: "xai",
            name: "xAI",
            base_url: "https://api.x.ai",
            api_key_env: Some("XAI_API_KEY"),
            default_model: Some("grok-3"),
            auth: AuthKind::Bearer,
            kind: ProviderKind::Cloud,
            model_prefixes: &["grok"],
            ..Default::default()
        }),
        provider(ProviderSpec {
            // Gemini via its OpenAI-COMPATIBLE endpoint (/v1beta/openai) — the
            // modern fix for the legacy era's broken bespoke integration
            // (Joel 2026-07-10: "something was wrong with the Google"): one data
            // row, the same parameterized OpenAICompatibleAdapter as everyone.
            id: "google",
            name: "Google",
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
            api_key_env: Some("GOOGLE_API_KEY"),
            default_model: Some("gemini-2.0-flash"),
            auth: AuthKind::Bearer,
            kind: ProviderKind::Cloud,
            model_prefixes: &["gemini"],
            ..Default::default()
        }),
        provider(ProviderSpec {
            id: "mistral",
            name: "Mistral AI",
            base_url: "https://api.mistral.ai",
            api_key_env: Some("MISTRAL_API_KEY"),
            default_model: Some("mistral-large-latest"),
            auth: AuthKind::Bearer,
            kind: ProviderKind::Cloud,
            model_prefixes: &[
                "mistral",
                "mixtral",
                "codestral",
                "open-mistral",
                "open-mixtral",
            ],
            ..Default::default()
        }),
        // DwarfStar (antirez/ds4) local sidecar — the V4-Flash lane (#306).
        // A deliberately narrow native engine serving ONE DeepSeek-class MoE
        // per process over an OpenAI-compatible HTTP surface (also /v1/messages
        // + /v1/responses). We run it with --ssd-streaming + a governed expert
        // cache: measured 2026-08-02 on the 64GB M5, 2.73 t/s gen cold /
        // ~2.3 t/s warm end-to-end at a 16GB budget with the full continuum
        // stack resident beside it. Lifecycle is EXTERNAL for now (operator-
        // launched, port 8901) — the autonomic planner must not try to spawn
        // or reconcile it; interop doctrine (#179): consume the engine,
        // don't fight it.
        provider(ProviderSpec {
            id: "ds4",
            name: "DwarfStar (local ds4-server, SSD-streaming MoE)",
            base_url: "http://127.0.0.1:8901",
            api_key_env: None,
            default_model: Some("deepseek-v4-flash"),
            auth: AuthKind::None,
            kind: ProviderKind::Local,
            model_prefixes: &["deepseek-v4"],
            // One model per ds4-server process — but the SIDECAR owns its
            // residency, not our serving daemon. `single_resident_model`
            // would make the adapter consult the llama-server snapshot (the
            // wrong authority — it refused deepseek-v4-flash because
            // Devstral holds the daemon's lane, verified live 2026-08-02).
            // `dynamic_model_catalog` is the truthful contract: answer
            // supports_model from the sidecar's OWN /v1/models. In-process
            // concurrency is uncapped here; ds4-server queues internally.
            capabilities: ProviderCapabilities {
                dynamic_model_catalog: true,
                ..Default::default()
            },
            ..Default::default()
        }),
        provider(ProviderSpec {
            id: "docker-model-runner",
            name: "Docker Model Runner (local Metal/CUDA)",
            base_url: "http://127.0.0.1:12434/engines/llama.cpp",
            api_key_env: None,
            default_model: Some("huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf:latest"),
            auth: AuthKind::None,
            kind: ProviderKind::Local,
            model_prefixes: &[],
            // DMR is a single-slot llama.cpp gateway with a dynamic catalog:
            // it mangles model ids (`hf.co/…:latest`), so the adapter must
            // fetch `/v1/models` at init, resolve logical→live ids per POST,
            // and answer `supports_model` from the live set
            // (`dynamic_model_catalog`). Being llama.cpp it accepts
            // `repeat_penalty` (`llamacpp_sampling_extensions`) — without it
            // the forged 4B loops. One resident slot (`single_resident_model`)
            // → the adapter caps concurrency at 1. These typed flags are what
            // the adapter reads instead of branching on the provider id (#55).
            capabilities: ProviderCapabilities {
                dynamic_model_catalog: true,
                llamacpp_sampling_extensions: true,
                single_resident_model: true,
                ..Default::default()
            },
            ..Default::default()
        }),
        // llama-server — the local OpenAI-compatible serving gateway (llama.cpp's
        // `/v1` server). Serves the resident GGUF over HTTP; the live model list
        // comes from `/v1/models`, so `model_prefixes` is empty and routing relies
        // on runtime discovery. No API key — it's a local endpoint (auth None).
        // The `base_url` here is the compile-time default; at registration the
        // adapter is repointed at the serving daemon's snapshot `base_url`
        // (Contract A — the single source of truth for where the gateway lives).
        // `default_model` is required by the adapter trait (it returns &str); it's
        // a fallback only — the real model is chosen per request.
        provider(ProviderSpec {
            id: crate::inference::llama_server::PROVIDER_ID,
            name: "llama-server (local OpenAI-compatible gateway)",
            base_url: crate::inference::llama_server::DEFAULT_BASE_URL,
            api_key_env: None,
            default_model: Some("continuum-ai/qwen3.5-4b-code-forged-GGUF"),
            auth: AuthKind::None,
            kind: ProviderKind::Local,
            model_prefixes: &[],
            // The local single-slot GGUF gateway. It does NATIVE OpenAI
            // function-calling: llama-server is launched with `--jinja` +
            // `--chat-template-file <model>/chat_template.jinja` (the forge's
            // tool-capable template sidecar), so it renders the `tools` param into
            // the prompt and does grammar-constrained tool-call generation —
            // valid tool-call JSON is guaranteed by the sampler, not hand-escaped
            // by the 4B model (the failure that made multi-line `code/run` calls
            // unparseable under prompt-based tools; verified live 2026-06-26).
            // Native is correct ONLY because the template sidecar is present; if a
            // future model ships without one, the GGUF's stripped template would
            // silently ignore tools — the forge MUST write the sidecar (#32/#52).
            // It is a REASONING model: thinking is its primary feature and the
            // persona's interiority, never suppressed by default. (We once set
            // `suppress_thinking: true` because the forged 4B rambled/looped its
            // `<think>` block to the token budget — but that is a fitness/sampling
            // gap to train away (genome loop #32) and to bound with the forwarded
            // `repeat_penalty`, NOT a feature to amputate. Suppressing it also
            // routed thinking-trained genes' answers into `reasoning_content`,
            // reading 0 in eval; the real fix is to let her think and read the
            // post-`</think>` answer from `content`.) Operator may still force
            // suppression per-run with the adapter's ThinkingMode override; the
            // gateway default is to THINK. It serves OpenAI-compatible embeddings
            // and holds ONE resident model (so the adapter pre-flights
            // activation). These flags are what the adapter reads instead of
            // branching on the provider id (#55).
            capabilities: ProviderCapabilities {
                tool_protocol: ToolProtocol::NativeFunctionCalling,
                suppress_thinking: false,
                // Embeddings are a MODEL fact: the resident forged GGUF row
                // declares `Capability::Embedding` (llama-server serves
                // /v1/embeddings from it via `--embedding`); the adapter
                // derives the provider-level capability by scanning rows (#68).
                single_resident_model: true,
                // llama.cpp server → forward `repeat_penalty` so the forged
                // 4B doesn't loop its `<think>` block to the token budget
                // (same failure DMR hit; the in-process path defaults 1.1).
                llamacpp_sampling_extensions: true,
                // Our fork's gateway, whose pinned build takes `tail_tools`
                // (CambrianTech/llama.cpp#75). The adapter offers it only while the running
                // lane reports that build (`llama_server::serving_engine_takes_tail_tools`).
                tail_tools: true,
                // Its served model ids match the registry rows (the forged
                // model now lives under this provider), so NO dynamic-catalog
                // name resolution — `dynamic_model_catalog` stays false and
                // `supports_model` answers from the static rows.
                ..Default::default()
            },
        }),
        provider(ProviderSpec {
            id: "llamacpp-local",
            name: "Llama.cpp (in-process Metal/CUDA)",
            base_url: "in-process",
            api_key_env: None,
            default_model: Some("continuum-ai/qwen3.5-4b-code-forged-GGUF"),
            auth: AuthKind::None,
            kind: ProviderKind::Local,
            model_prefixes: &[],
            ..Default::default()
        }),
    ]
}

#[derive(Clone)]
pub(super) struct ModelSpec {
    pub(super) id: &'static str,
    pub(super) name: &'static str,
    pub(super) provider: &'static str,
    pub(super) arch: Arch,
    pub(super) context_window: u32,
    pub(super) max_output_tokens: u32,
    pub(super) tokens_per_second: f32,
    pub(super) capabilities: &'static [Capability],
    pub(super) cost_input_per_1k: f32,
    pub(super) cost_output_per_1k: f32,
    pub(super) gguf_hint: Option<&'static str>,
    /// HF safetensors repo id for the trainable form (see `Model::hf_source`).
    pub(super) hf_source: Option<&'static str>,
    pub(super) gguf_local_path: Option<&'static str>,
    pub(super) mmproj_local_path: Option<&'static str>,
    pub(super) chat_template: Option<&'static str>,
    pub(super) multi_party_strategy: MultiPartyChatStrategy,
    pub(super) stop_sequences: &'static [&'static str],
    /// Per-model decode defaults (#76). Defaults to the substrate floor
    /// ([`ModelSampling::default`], incl. the #181 anti-loop pair) via
    /// `..Default::default()`, so only a model we've measured/tuned overrides it.
    pub(super) sampling: ModelSampling,
    /// See [`Model::persona_serving_eligible`]. Default TRUE; benchmark
    /// opponents / campaign-roster rows opt OUT so the autonomic planner can
    /// never conscript them as the citizens' model.
    pub(super) persona_serving_eligible: bool,
    /// Per-model serving truths ([`ModelServingPrefs`]) — defaults via
    /// `..Default::default()`; only a model we've MEASURED overrides.
    pub(super) serving: ModelServingPrefs,
}

impl Default for ModelSpec {
    fn default() -> Self {
        Self {
            id: "",
            name: "",
            provider: "",
            arch: Arch::Unknown,
            context_window: 0,
            max_output_tokens: 0,
            tokens_per_second: 0.0,
            capabilities: &[],
            cost_input_per_1k: 0.0,
            cost_output_per_1k: 0.0,
            gguf_hint: None,
            hf_source: None,
            gguf_local_path: None,
            mmproj_local_path: None,
            chat_template: None,
            multi_party_strategy: MultiPartyChatStrategy::NamePrefixedUserTurns,
            stop_sequences: &[],
            sampling: ModelSampling::default(),
            persona_serving_eligible: true,
            serving: ModelServingPrefs::default(),
        }
    }
}

pub(super) fn model(spec: ModelSpec) -> Model {
    Model {
        id: spec.id.to_string(),
        name: Some(spec.name.to_string()),
        provider: spec.provider.to_string(),
        arch: spec.arch,
        context_window: spec.context_window,
        max_output_tokens: spec.max_output_tokens,
        tokens_per_second: spec.tokens_per_second,
        capabilities: caps(spec.capabilities),
        cost_input_per_1k: spec.cost_input_per_1k,
        cost_output_per_1k: spec.cost_output_per_1k,
        gguf_hint: spec.gguf_hint.map(str::to_string),
        hf_source: spec.hf_source.map(str::to_string),
        gguf_local_path: spec.gguf_local_path.map(PathBuf::from),
        mmproj_local_path: spec.mmproj_local_path.map(PathBuf::from),
        // Artifact SIZES, like the artifact paths above, are discovered not declared:
        // `artifacts::resolve_model_artifacts` stamps them at registry load. Kept out
        // of `ModelSpec` for the same reason `parameter_count` is — a hand-authored
        // byte count is a fact that silently goes stale the moment a quant is re-pulled.
        weights_bytes: None,
        mmproj_bytes: None,
        chat_template: spec.chat_template.map(str::to_string),
        multi_party_strategy: spec.multi_party_strategy,
        stop_sequences: spec.stop_sequences.iter().map(|s| s.to_string()).collect(),
        sampling: spec.sampling,
        serving: spec.serving,
        // Not a hand-authored fact: the size comes from the artifact's own
        // `general.parameter_count` header, hydrated once at registry load
        // ([`super::hydrate`]). The `ModelSpec` deliberately omits it so no
        // human types "4B" into a row — the sentinel `0` means "ask the GGUF".
        parameter_count: 0,
        persona_serving_eligible: spec.persona_serving_eligible,
    }
}

pub(super) struct ProviderSpec {
    pub(super) id: &'static str,
    pub(super) name: &'static str,
    pub(super) base_url: &'static str,
    pub(super) api_key_env: Option<&'static str>,
    pub(super) default_model: Option<&'static str>,
    pub(super) auth: AuthKind,
    pub(super) kind: ProviderKind,
    pub(super) model_prefixes: &'static [&'static str],
    /// Behavioral capabilities (#55). Defaults to the cloud/common case via
    /// `..Default::default()` in the literal, so only outlier providers
    /// (local single-slot gateways, embedding endpoints) declare anything.
    pub(super) capabilities: ProviderCapabilities,
}

impl Default for ProviderSpec {
    fn default() -> Self {
        Self {
            id: "",
            name: "",
            base_url: "",
            api_key_env: None,
            default_model: None,
            auth: AuthKind::None,
            kind: ProviderKind::Cloud,
            model_prefixes: &[],
            capabilities: ProviderCapabilities::default(),
        }
    }
}

pub(super) fn provider(spec: ProviderSpec) -> Provider {
    Provider {
        id: spec.id.to_string(),
        name: Some(spec.name.to_string()),
        base_url: spec.base_url.to_string(),
        api_key_env: spec.api_key_env.map(str::to_string),
        default_model: spec.default_model.map(str::to_string),
        auth: spec.auth,
        model_prefixes: spec
            .model_prefixes
            .iter()
            .map(|prefix| prefix.to_string())
            .collect(),
        kind: spec.kind,
        capabilities: spec.capabilities,
    }
}

pub(super) fn caps(capabilities: &[Capability]) -> BTreeSet<Capability> {
    capabilities.iter().copied().collect()
}
