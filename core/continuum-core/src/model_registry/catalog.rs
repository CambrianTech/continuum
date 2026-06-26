//! Curated Rust model catalog.
//!
//! Runtime model truth lives here, not in TypeScript maps or editable TOML.
//! Discovery may propose candidates elsewhere; admission only chooses from
//! this vetted catalog.

use super::loader::{Registry, RegistryError};
use super::types::{
    Arch, AuthKind, Capability, Model, MultiPartyChatStrategy, Provider, ProviderCapabilities,
    ProviderKind, ToolProtocol,
};
use std::collections::BTreeSet;
use std::path::PathBuf;

const QWEN35_CHAT_TEMPLATE: &str = "{% for message in messages %}{{ '<|im_start|>' + message['role'] + '\\n' + message['content'] + '<|im_end|>\\n' }}{% endfor %}{% if add_generation_prompt %}{{ '<|im_start|>assistant\\n' }}{% endif %}";

pub fn registry() -> Result<Registry, RegistryError> {
    Registry::from_catalog(models(), providers())
}

pub fn models() -> Vec<Model> {
    vec![
        model(ModelSpec {
            id: "claude-sonnet-4-5-20250929",
            name: "Claude Sonnet 4.5",
            provider: "anthropic",
            arch: Arch::Claude,
            context_window: 200_000,
            max_output_tokens: 8192,
            tokens_per_second: 50.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Vision,
                Capability::Streaming,
            ],
            cost_input_per_1k: 0.003,
            cost_output_per_1k: 0.015,
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "claude-opus-4-20250514",
            name: "Claude Opus 4",
            provider: "anthropic",
            arch: Arch::Claude,
            context_window: 200_000,
            max_output_tokens: 4096,
            tokens_per_second: 50.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Vision,
                Capability::Streaming,
            ],
            cost_input_per_1k: 0.015,
            cost_output_per_1k: 0.075,
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "claude-3-5-haiku-20250107",
            name: "Claude 3.5 Haiku",
            provider: "anthropic",
            arch: Arch::Claude,
            context_window: 200_000,
            max_output_tokens: 4096,
            tokens_per_second: 50.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Vision,
                Capability::Streaming,
            ],
            cost_input_per_1k: 0.00025,
            cost_output_per_1k: 0.00125,
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "gpt-4-turbo-preview",
            name: "GPT-4 Turbo",
            provider: "openai",
            arch: Arch::Gpt,
            context_window: 128_000,
            max_output_tokens: 4096,
            tokens_per_second: 50.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Vision,
                Capability::Streaming,
            ],
            cost_input_per_1k: 0.01,
            cost_output_per_1k: 0.03,
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "gpt-4o",
            name: "GPT-4o",
            provider: "openai",
            arch: Arch::Gpt,
            context_window: 128_000,
            max_output_tokens: 4096,
            tokens_per_second: 50.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Vision,
                Capability::AudioInput,
                Capability::AudioOutput,
                Capability::Streaming,
            ],
            cost_input_per_1k: 0.005,
            cost_output_per_1k: 0.015,
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "deepseek-chat",
            name: "DeepSeek Chat",
            provider: "deepseek",
            arch: Arch::Deepseek,
            context_window: 128_000,
            max_output_tokens: 8192,
            tokens_per_second: 50.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            cost_input_per_1k: 0.00014,
            cost_output_per_1k: 0.00028,
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "deepseek-reasoner",
            name: "DeepSeek Reasoner",
            provider: "deepseek",
            arch: Arch::Deepseek,
            context_window: 128_000,
            max_output_tokens: 8192,
            tokens_per_second: 50.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            cost_input_per_1k: 0.00055,
            cost_output_per_1k: 0.00219,
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo",
            name: "Llama 3.1 70B (Together)",
            provider: "together",
            arch: Arch::Llama,
            context_window: 131_072,
            max_output_tokens: 4096,
            tokens_per_second: 50.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            cost_input_per_1k: 0.00088,
            cost_output_per_1k: 0.00088,
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "llama-3.1-8b-instant",
            name: "Llama 3.1 8B Instant (Groq)",
            provider: "groq",
            arch: Arch::Llama,
            context_window: 131_072,
            max_output_tokens: 8192,
            tokens_per_second: 50.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            cost_input_per_1k: 0.00005,
            cost_output_per_1k: 0.00008,
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "accounts/fireworks/models/llama-v3p3-70b-instruct",
            name: "Llama 3.3 70B (Fireworks)",
            provider: "fireworks",
            arch: Arch::Llama,
            context_window: 128_000,
            max_output_tokens: 8192,
            tokens_per_second: 50.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            cost_input_per_1k: 0.0009,
            cost_output_per_1k: 0.0009,
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "grok-3",
            name: "Grok 3",
            provider: "xai",
            arch: Arch::Grok,
            context_window: 131_072,
            max_output_tokens: 8192,
            tokens_per_second: 50.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            cost_input_per_1k: 0.003,
            cost_output_per_1k: 0.015,
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "gemini-2.0-flash",
            name: "Gemini 2.0 Flash",
            provider: "google",
            arch: Arch::Gemini,
            context_window: 1_000_000,
            max_output_tokens: 8192,
            tokens_per_second: 50.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Vision,
                Capability::AudioInput,
                Capability::Streaming,
            ],
            cost_input_per_1k: 0.000075,
            cost_output_per_1k: 0.0003,
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "mistral-large-latest",
            name: "Mistral Large",
            provider: "mistral",
            arch: Arch::Mistral,
            context_window: 131_072,
            max_output_tokens: 8192,
            tokens_per_second: 50.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            cost_input_per_1k: 0.002,
            cost_output_per_1k: 0.006,
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "codestral-latest",
            name: "Codestral",
            provider: "mistral",
            arch: Arch::Mistral,
            context_window: 32_768,
            max_output_tokens: 8192,
            tokens_per_second: 60.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            cost_input_per_1k: 0.001,
            cost_output_per_1k: 0.003,
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "docker.io/ai/qwen2.5:7B-Q4_K_M",
            name: "Qwen2.5 7B Q4_K_M (DMR)",
            provider: "docker-model-runner",
            arch: Arch::Qwen2,
            context_window: 32_768,
            max_output_tokens: 4096,
            tokens_per_second: 50.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            gguf_hint: Some("docker.io/ai/qwen2.5:7B-Q4_K_M"),
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "huggingface.co/mlx-community/qwen2.5-7b-instruct-4bit:latest",
            name: "Qwen2.5 7B MLX 4-bit (DMR)",
            provider: "docker-model-runner",
            arch: Arch::Qwen2,
            context_window: 32_768,
            max_output_tokens: 4096,
            tokens_per_second: 50.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/mlx-community/qwen2.5-7b-instruct-4bit"),
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf:latest",
            name: "Qwen3.5 4B Code-Forged (DMR)",
            provider: "docker-model-runner",
            arch: Arch::Qwen35,
            context_window: 262_144,
            max_output_tokens: 32_768,
            tokens_per_second: 50.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf"),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "continuum-ai/qwen3.5-4b-code-forged-GGUF",
            name: "Qwen3.5 4B Code-Forged (in-process)",
            // Provider is "llama-server" (the HTTP llama.cpp /v1 gateway) — NOT
            // "llamacpp-local" (the retiring in-process Metal path, #41) — because
            // this is the row llama-server's `default_model` references and the
            // gateway is what actually serves it (live responses carry
            // provider=llama-server). It MUST be registered under llama-server so
            // `OpenAICompatibleAdapter::from_registry("llama-server")` derives
            // `supports_tools = true` from this model's ToolUse capability (the
            // adapter's "any model under this provider advertises ToolUse" rule).
            // With supports_tools=false the adapter SILENTLY dropped the native
            // `tools` param (openai_adapter.rs gates the tools body on
            // supports_tools && Native), so the 4B model never got the
            // grammar-constrained tool channel and hand-emitted unparseable
            // multi-line `{"tool_call":...}` JSON as prose → demoted to chat →
            // confabulated success. Registering it here is what makes native tool
            // calls actually fire in the live cognition path (verified 2026-06-26).
            provider: "llama-server",
            arch: Arch::Qwen35,
            context_window: 262_144,
            max_output_tokens: 32_768,
            tokens_per_second: 33.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf"),
            chat_template: Some(QWEN35_CHAT_TEMPLATE),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>", "<|endoftext|>"],
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "qwen2-vl-7b-instruct",
            name: "Qwen2-VL-7B-Instruct (in-process)",
            provider: "llamacpp-local",
            arch: Arch::Qwen2,
            context_window: 32_768,
            max_output_tokens: 4096,
            tokens_per_second: 16.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::Vision,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/bartowski/Qwen2-VL-7B-Instruct-GGUF"),
            gguf_local_path: Some("~/models/qwen2-vl-7b/Qwen2-VL-7B-Instruct-Q4_K_M.gguf"),
            mmproj_local_path: Some("~/models/qwen2-vl-7b/mmproj-Qwen2-VL-7B-Instruct-f16.gguf"),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "qwen2.5-omni-7b-instruct",
            name: "Qwen2.5-Omni-7B-Instruct (in-process)",
            provider: "llamacpp-local",
            arch: Arch::Qwen2,
            context_window: 32_768,
            max_output_tokens: 4096,
            tokens_per_second: 220.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::Vision,
                Capability::AudioInput,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/ggml-org/Qwen2.5-Omni-7B-GGUF"),
            gguf_local_path: Some("~/models/qwen2.5-omni-7b/Qwen2.5-Omni-7B-Q4_K_M.gguf"),
            mmproj_local_path: Some("~/models/qwen2.5-omni-7b/mmproj-Qwen2.5-Omni-7B-f16.gguf"),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            ..ModelSpec::default()
        }),
        // LCD model — the substrate's lowest-common-denominator base
        // per [[lcd-model-qwen25-05b-and-foundry-lora]]. Qwen2.5 0.5B
        // Instruct Q4_K_M GGUF, ~468 MiB. Runs on any tier including
        // CPU-only and Intel Mac mac-cpu-only. Substrate slice 13's
        // boot composition asks for this model_id explicitly via
        // `PersonaSpawnerModule::plan_for_tier`.
        model(ModelSpec {
            id: "continuum-ai/qwen2.5-0.5b-instruct-GGUF",
            name: "Qwen2.5-0.5B-Instruct (LCD, in-process)",
            provider: "llamacpp-local",
            arch: Arch::Qwen2,
            context_window: 32_768,
            max_output_tokens: 4096,
            tokens_per_second: 60.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF"),
            gguf_local_path: Some(
                "~/.continuum/genome/models/qwen2.5-0.5b-instruct/qwen2.5-0.5b-instruct-q4_k_m.gguf",
            ),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            ..ModelSpec::default()
        }),
        // The grid's canonical retrieval embedder — Qwen3-Embedding-0.6B
        // (Q8_0 GGUF, ~610 MiB). Served IN-PROCESS by LlamaCppAdapter (GPU
        // forward, last-token pooled). `resolve_recall_embedder` finds this
        // row by (provider=llamacpp-local ∧ Capability::Embedding ∧ gguf on
        // disk) and prefers it over routing embeddings through the chat
        // gateway. DELIBERATELY DECOUPLED from the chat model: recall vectors
        // stay in one stable embedding space regardless of which model the
        // persona's brain runs ([[embeddings-are-per-content-computed-once-shared]]).
        model(ModelSpec {
            id: "continuum-ai/qwen3-embedding-0.6b-GGUF",
            name: "Qwen3-Embedding-0.6B (in-process retrieval embedder)",
            provider: "llamacpp-local",
            arch: Arch::Qwen3,
            context_window: 32_768,
            // Embedding model — produces vectors, never generated tokens.
            max_output_tokens: 0,
            tokens_per_second: 0.0,
            capabilities: &[Capability::Embedding],
            gguf_hint: Some("huggingface.co/Qwen/Qwen3-Embedding-0.6B-GGUF"),
            gguf_local_path: Some(
                "~/.continuum/genome/models/qwen3-embedding-0.6b/Qwen3-Embedding-0.6B-Q8_0.gguf",
            ),
            ..ModelSpec::default()
        }),
    ]
}

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
            // OpenAI exposes `/v1/embeddings` and image generation; everything
            // else is the cloud default (native tools, keep thinking).
            capabilities: ProviderCapabilities {
                supports_embeddings: true,
                supports_image_generation: true,
                ..Default::default()
            },
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
            model_prefixes: &["mistral", "mixtral", "codestral", "open-mistral", "open-mixtral"],
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
            base_url: crate::inference::unsloth_control::DEFAULT_HOST,
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
            // Its forged 4B reasoner rambles unless thinking is suppressed, it
            // serves OpenAI-compatible embeddings, and it holds ONE resident model
            // (so the adapter pre-flights activation). These flags are what the
            // adapter reads instead of branching on the provider id (#55).
            capabilities: ProviderCapabilities {
                tool_protocol: ToolProtocol::NativeFunctionCalling,
                suppress_thinking: true,
                supports_embeddings: true,
                single_resident_model: true,
                // llama.cpp server → forward `repeat_penalty` so the forged
                // 4B doesn't loop its `<think>` block to the token budget
                // (same failure DMR hit; the in-process path defaults 1.1).
                llamacpp_sampling_extensions: true,
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
struct ModelSpec {
    id: &'static str,
    name: &'static str,
    provider: &'static str,
    arch: Arch,
    context_window: u32,
    max_output_tokens: u32,
    tokens_per_second: f32,
    capabilities: &'static [Capability],
    cost_input_per_1k: f32,
    cost_output_per_1k: f32,
    gguf_hint: Option<&'static str>,
    gguf_local_path: Option<&'static str>,
    mmproj_local_path: Option<&'static str>,
    chat_template: Option<&'static str>,
    multi_party_strategy: MultiPartyChatStrategy,
    stop_sequences: &'static [&'static str],
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
            gguf_local_path: None,
            mmproj_local_path: None,
            chat_template: None,
            multi_party_strategy: MultiPartyChatStrategy::NamePrefixedUserTurns,
            stop_sequences: &[],
        }
    }
}

fn model(spec: ModelSpec) -> Model {
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
        gguf_local_path: spec.gguf_local_path.map(PathBuf::from),
        mmproj_local_path: spec.mmproj_local_path.map(PathBuf::from),
        chat_template: spec.chat_template.map(str::to_string),
        multi_party_strategy: spec.multi_party_strategy,
        stop_sequences: spec.stop_sequences.iter().map(|s| s.to_string()).collect(),
    }
}

struct ProviderSpec {
    id: &'static str,
    name: &'static str,
    base_url: &'static str,
    api_key_env: Option<&'static str>,
    default_model: Option<&'static str>,
    auth: AuthKind,
    kind: ProviderKind,
    model_prefixes: &'static [&'static str],
    /// Behavioral capabilities (#55). Defaults to the cloud/common case via
    /// `..Default::default()` in the literal, so only outlier providers
    /// (local single-slot gateways, embedding endpoints) declare anything.
    capabilities: ProviderCapabilities,
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

fn provider(spec: ProviderSpec) -> Provider {
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

fn caps(capabilities: &[Capability]) -> BTreeSet<Capability> {
    capabilities.iter().copied().collect()
}
