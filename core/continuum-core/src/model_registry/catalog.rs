//! Curated Rust model catalog.
//!
//! Runtime model truth lives here, not in TypeScript maps or editable TOML.
//! Discovery may propose candidates elsewhere; admission only chooses from
//! this vetted catalog.

use super::registry::{Registry, RegistryError};
use super::types::{
    Arch, AuthKind, Capability, Model, ModelSampling, MultiPartyChatStrategy, Provider,
    ProviderCapabilities, ProviderKind, ToolProtocol,
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
        // OpenAI's embedding + image lanes as MODEL rows (#68): the provider's
        // embedding/image capability is derived by the adapter SCANNING these
        // — there is no provider-level `supports_embeddings` bool. These are
        // capability-routed (not arch-dispatched), so `Arch::Gpt` just groups
        // them with the OpenAI family.
        model(ModelSpec {
            id: "text-embedding-3-small",
            name: "OpenAI Text Embedding 3 Small",
            provider: "openai",
            arch: Arch::Gpt,
            context_window: 8_192,
            max_output_tokens: 0,
            tokens_per_second: 0.0,
            capabilities: &[Capability::Embedding],
            cost_input_per_1k: 0.00002,
            cost_output_per_1k: 0.0,
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "dall-e-3",
            name: "OpenAI DALL·E 3",
            provider: "openai",
            arch: Arch::Gpt,
            context_window: 4_096,
            max_output_tokens: 0,
            tokens_per_second: 0.0,
            capabilities: &[Capability::ImageGeneration],
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
                // llama-server serves /v1/embeddings from the resident model
                // (`--embedding`), so the embedding capability is a fact of
                // THIS row — the adapter scans it to advertise the provider's
                // embedding lane (#68), replacing the old provider-level bool.
                Capability::Embedding,
            ],
            gguf_hint: Some("huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf"),
            chat_template: Some(QWEN35_CHAT_TEMPLATE),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>", "<|endoftext|>"],
            ..ModelSpec::default()
        }),
        // Teacher model for the genome cold-start loop ([[genome-loop-trains-on-own-mistakes]]).
        // The forged 4B is too narrow a teacher — it solves easy gym tasks first-try
        // (zero corrections) and never converges on hard ones (no corpus), so its
        // fail-then-fix-to-green band (the only band that teaches the self-correct
        // reflex) is too thin. Qwen2.5-Coder-14B is meaningfully stronger at the
        // failure frontier: it fails harder tasks, reads the real rustc error, and
        // fixes to green — exactly the correction trajectories `genome/teach` distils.
        // Registered under "llama-server" so `genome/teach`'s
        // `select(Some("llama-server"), Some(model), …)` can serve it on a lane;
        // corpus generation is an offline batch (serving/pin it, generate, pin the
        // 4B back) so the live personas resume on their base. ~9 GB at Q4_K_M.
        model(ModelSpec {
            id: "continuum-ai/qwen2.5-coder-14b-instruct-GGUF",
            name: "Qwen2.5-Coder-14B-Instruct (teacher)",
            provider: "llama-server",
            arch: Arch::Qwen2,
            context_window: 32_768,
            max_output_tokens: 8192,
            tokens_per_second: 12.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/bartowski/Qwen2.5-Coder-14B-Instruct-GGUF"),
            // gguf_local_path DERIVED, not hardcoded: resolve_gguf matches this id to
            // its dir under ~/.continuum/genome/models/ (identity-token subset) and
            // falls back to the HF cache via gguf_hint — no baked absolute path or quant.
            chat_template: Some(QWEN35_CHAT_TEMPLATE),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>", "<|endoftext|>"],
            ..ModelSpec::default()
        }),
        // DEVSTRAL SMALL 2507 — the AGENTIC coder (Mistral-Small-3.1 base, 24B, 68% SWE-bench
        // Verified, runs on a 32GB Mac at Q4_K_M ~14GB). The 14B coder ACTS but loops/mis-plans at
        // repo scale (glass-boxed); Devstral is built for the search→read→edit→verify arc. FIRST
        // Arch::Mistral row. `chat_template: None` → use the GGUF's embedded Tekken template
        // (--jinja renders tools). `multi_party_strategy` is the one field to serve-validate against
        // Tekken (starting with the single-party collapse the coders use); stop uses Mistral's `</s>`.
        model(ModelSpec {
            id: "unsloth/Devstral-Small-2507-GGUF",
            name: "Devstral-Small-2507 (agentic coder)",
            provider: "llama-server",
            arch: Arch::Mistral,
            context_window: 131_072,
            max_output_tokens: 8192,
            tokens_per_second: 10.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/unsloth/Devstral-Small-2507-GGUF"),
            // The trainable HF safetensors base (mistralai upstream) — what the
            // genome forge trains LoRA against; the GGUF above is serving-only.
            // Without this, genome/job-create fails loud ("no hf_source"), which
            // blocked the first lived-curriculum train (recall-trust, 2026-07-10).
            hf_source: Some("mistralai/Devstral-Small-2507"),
            chat_template: None,
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["</s>"],
            // #181 REFERENCE ROW (#76): Devstral-Small is the model that exhibited
            // the reasoning-channel repetition loop (identical wrong code block ~5×,
            // 14k tokens to the length cap, empty answer). Its sampling is pinned
            // EXPLICITLY to the substrate anti-loop floor so this row is the
            // documented ESCALATION POINT: if the floor proves insufficient once
            // measured live post-reboot, the fix is bumping THESE numbers
            // (repeat_last_n → wider, frequency_penalty → stronger) here — a
            // one-line per-model tune, not a re-plumb. Values == floor today, so
            // the reboot cleanly measures the floor as planned
            // ([[anti-loop-sampling-windowed-vs-unwindowed]]).
            sampling: ModelSampling::default(),
            ..ModelSpec::default()
        }),
        // QWEN2.5-VL-7B — the VISION lane's model (#106): personas' eyes for live mode.
        // The FIRST llama-server-provider row with Capability::Vision, which makes it the
        // first VL model the serving daemon can actually bring up (`--mmproj` spawn path +
        // the /props `modalities.vision` readiness gate). Why THIS model:
        //   - ggml-org repo = maintained by the llama.cpp org itself; ships the GGUF AND its
        //     `mmproj-*-f16.gguf` projector in ONE repo, so `models/pull` acquires both in a
        //     single command (its Vision-capability mmproj-sibling logic) and
        //     `find_mmproj_beside` resolves the projector with zero per-machine path edits.
        //   - Qwen2.5-VL-7B is the small end of the current VL frontier that still carries
        //     real tool use — a live-mode citizen must SEE *and* ACT, so the vision lane's
        //     model keeps ToolUse rather than being a caption-only 2-3B.
        //   - ~4.7 GB Q4_K_M + ~1.4 GB f16 projector: fits an M-series lane comfortably.
        // capability_rank (GB + tool bonus) leaves the 14B coder the autonomic pick, so this
        // row never hijacks the live lane by surprise — the operator brings vision up with
        // `models/pull` + `serving/pin` (or it wins on hosts where it IS the best fit).
        model(ModelSpec {
            id: "ggml-org/Qwen2.5-VL-7B-Instruct-GGUF",
            name: "Qwen2.5-VL-7B-Instruct (vision — the persona eye lane)",
            provider: "llama-server",
            arch: Arch::Qwen2,
            context_window: 32_768,
            max_output_tokens: 8192,
            tokens_per_second: 20.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Vision,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/ggml-org/Qwen2.5-VL-7B-Instruct-GGUF"),
            // Trainable HF base for the genome forge (vision LoRA is future work,
            // but the row follows the Devstral pattern so it's ready when it lands).
            hf_source: Some("Qwen/Qwen2.5-VL-7B-Instruct"),
            // Embedded template + --jinja (same pattern as Devstral/Hermes): the
            // ggml-org GGUF carries Qwen2.5-VL's own ChatML-with-vision template.
            chat_template: None,
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>"],
            ..ModelSpec::default()
        }),
        // Hermes-3-Llama-3.1-8B — the OPPONENT, made first-class. A general (non-coder) model we
        // benchmark AGAINST; giving it a real catalog row lets it flow through OURS (base_model_id)
        // and opencode like any other model, so the head-to-head is model-through-harness fair, not
        // a hardcoded reference column. Llama-3.1 arch; Hermes ships a ChatML template embedded in
        // the GGUF, so chat_template: None + --jinja (same pattern as Devstral's Tekken).
        model(ModelSpec {
            id: "NousResearch/Hermes-3-Llama-3.1-8B-GGUF",
            name: "Hermes-3-Llama-3.1-8B (opponent)",
            provider: "llama-server",
            arch: Arch::Llama,
            context_window: 131_072,
            max_output_tokens: 8192,
            tokens_per_second: 30.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/bartowski/Hermes-3-Llama-3.1-8B-GGUF"),
            chat_template: None,
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>", "<|eot_id|>"],
            persona_serving_eligible: false, // opponent: benchmark-only, never the citizens' model
            ..ModelSpec::default()
        }),
        // DeepSeek-V4-Flash via the ds4 sidecar (#306; launched 2026-08-02,
        // running on this M5 the same night). 304B MoE (256 experts × 48
        // layers), uniform-2bit routed experts + Q8 decision spine, layer-
        // dependent compressed attention (KV ≈ hundreds of KB — near-free).
        // MEASURED here: 2.73 t/s gen cold / ~2.3 t/s warm end-to-end at a
        // 16GB expert-cache budget with the full stack resident; first-shot
        // correct Rust (compile-graded 3/3) on the merge_intervals probe.
        // context_window is the MODEL's capability; the live served window
        // comes from the adapter/live serve per #50 (tonight's serve: 8192).
        // NOT persona_serving_eligible yet: the sidecar's lifecycle is
        // operator-managed (no governed spawn/reconcile), so the autonomic
        // planner must not adopt her — evals reach her by explicit model id.
        // Flip deliberately once lifecycle is governed.
        model(ModelSpec {
            id: "deepseek-v4-flash",
            name: "DeepSeek-V4-Flash 304B (ds4 SSD-streaming, deliberator)",
            provider: "ds4",
            arch: Arch::Unknown, // CSA+HCA hybrid — served by ds4, never by llama-server
            context_window: 1_000_000,
            max_output_tokens: 8192,
            tokens_per_second: 2.3, // measured warm end-to-end, 2026-08-02
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            chat_template: None, // ds4-server renders its own template
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &[],
            persona_serving_eligible: false,
            ..ModelSpec::default()
        }),
        // ── The campaign roster (benchmarks/HERMES-CAMPAIGN.md) ──
        // Opponents + community champions for the 64GB-class matrix. Arch + context
        // read from each GGUF's OWN header at add time (#74 — never guessed):
        // Hermes-4.3-36B is seed_oss (ByteDance Seed-OSS base), not a Llama/Qwen.
        // All serve via llama-server (--jinja, template embedded in GGUF); GGUFs
        // land under genome/models/ and resolve by id-token derivation.
        model(ModelSpec {
            id: "NousResearch/Hermes-4.3-36B-GGUF",
            name: "Hermes-4.3-36B (opponent — their current flagship-mid)",
            provider: "llama-server",
            arch: Arch::Unknown, // seed_oss — not yet an enumerated arch; llama-server reads the header
            context_window: 524_288,
            max_output_tokens: 8192,
            tokens_per_second: 8.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/NousResearch/Hermes-4.3-36B-GGUF"),
            chat_template: None,
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>"],
            persona_serving_eligible: false, // opponent flagship: the planner conscripted this TWICE (2026-07-12)
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "Qwen/Qwen3-32B-GGUF",
            name: "Qwen3-32B (aider's 64GB-class local ceiling: 40.0% polyglot)",
            provider: "llama-server",
            arch: Arch::Qwen3,
            context_window: 32_768,
            max_output_tokens: 8192,
            tokens_per_second: 9.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/bartowski/Qwen_Qwen3-32B-GGUF"),
            chat_template: None,
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>"],
            persona_serving_eligible: false, // benchmark reference row (aider 64GB-class ceiling)
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "unsloth/Qwen3-Coder-30B-A3B-Instruct-GGUF",
            name: "Qwen3-Coder-30B-A3B (community champion — MoE, 3B active)",
            provider: "llama-server",
            arch: Arch::Qwen3, // qwen3moe header; MoE routing handled by llama-server
            context_window: 262_144,
            max_output_tokens: 8192,
            tokens_per_second: 25.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/unsloth/Qwen3-Coder-30B-A3B-Instruct-GGUF"),
            chat_template: None,
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>"],
            persona_serving_eligible: false, // campaign row; eligibility revisit gated on #126 consent ranges
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "bartowski/phi-4-GGUF",
            name: "Phi-4-14B (roster — MS small-model line)",
            provider: "llama-server",
            arch: Arch::Unknown, // phi3 header — not yet an enumerated arch
            context_window: 16_384,
            max_output_tokens: 8192,
            tokens_per_second: 14.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/bartowski/phi-4-GGUF"),
            chat_template: None,
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>"],
            persona_serving_eligible: false, // benchmark roster row (no ToolUse — unfit for citizens anyway)
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "bartowski/Qwen2.5-Coder-32B-Instruct-GGUF",
            name: "Qwen2.5-Coder-32B (aider's published 16.4% polyglot — replicate then beat)",
            provider: "llama-server",
            arch: Arch::Qwen2,
            context_window: 32_768,
            max_output_tokens: 8192,
            tokens_per_second: 9.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/bartowski/Qwen2.5-Coder-32B-Instruct-GGUF"),
            chat_template: None,
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>"],
            persona_serving_eligible: false, // benchmark reference row (aider replicate-then-beat)
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "bartowski/Qwen2.5-Coder-7B-Instruct-GGUF",
            name: "Qwen2.5-Coder-7B (roster — small coder tier)",
            provider: "llama-server",
            arch: Arch::Qwen2,
            context_window: 32_768,
            max_output_tokens: 8192,
            tokens_per_second: 30.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/bartowski/Qwen2.5-Coder-7B-Instruct-GGUF"),
            // Trainable base for the genome forge (mlx LoRA trains against HF
            // safetensors, not the serving quant) — the battery's benchmark
            // base becomes gene-forgeable (coder-verify-reflex, 2026-07-23).
            hf_source: Some("Qwen/Qwen2.5-Coder-7B-Instruct"),
            chat_template: None,
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>"],
            ..ModelSpec::default()
        }),
        // The Qwen2.5-Coder SIZE LADDER — 0.5B / 1.5B / 3B, so a weak box serves what it can and
        // we can chart small→large on the same benchmark (what a MacBook — or a Pi — gets away
        // with). plan_serving still picks the largest that FITS, so these only serve where the
        // 14B/32B won't. Same Qwen2 arch + ChatML template; tiny GGUFs (~0.4–2 GB Q4_K_M).
        model(ModelSpec {
            id: "continuum-ai/qwen2.5-coder-3b-instruct-GGUF",
            name: "Qwen2.5-Coder-3B-Instruct",
            provider: "llama-server",
            arch: Arch::Qwen2,
            context_window: 32_768,
            max_output_tokens: 8192,
            tokens_per_second: 45.0,
            capabilities: &[Capability::TextGeneration, Capability::Chat, Capability::ToolUse, Capability::Streaming],
            gguf_hint: Some("huggingface.co/bartowski/Qwen2.5-Coder-3B-Instruct-GGUF"),
            chat_template: Some(QWEN35_CHAT_TEMPLATE),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>", "<|endoftext|>"],
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "continuum-ai/qwen2.5-coder-1.5b-instruct-GGUF",
            name: "Qwen2.5-Coder-1.5B-Instruct",
            provider: "llama-server",
            arch: Arch::Qwen2,
            context_window: 32_768,
            max_output_tokens: 8192,
            tokens_per_second: 70.0,
            capabilities: &[Capability::TextGeneration, Capability::Chat, Capability::ToolUse, Capability::Streaming],
            gguf_hint: Some("huggingface.co/bartowski/Qwen2.5-Coder-1.5B-Instruct-GGUF"),
            chat_template: Some(QWEN35_CHAT_TEMPLATE),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>", "<|endoftext|>"],
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "continuum-ai/qwen2.5-coder-0.5b-instruct-GGUF",
            name: "Qwen2.5-Coder-0.5B-Instruct",
            provider: "llama-server",
            arch: Arch::Qwen2,
            context_window: 32_768,
            max_output_tokens: 8192,
            tokens_per_second: 110.0,
            capabilities: &[Capability::TextGeneration, Capability::Chat, Capability::ToolUse, Capability::Streaming],
            gguf_hint: Some("huggingface.co/bartowski/Qwen2.5-Coder-0.5B-Instruct-GGUF"),
            chat_template: Some(QWEN35_CHAT_TEMPLATE),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>", "<|endoftext|>"],
            ..ModelSpec::default()
        }),
        // A GENERAL (non-coder) model for the CATEGORY axis — same size class as a coder, so a
        // chart shows specialist-vs-generalist at equal size (the model-fit thesis, measured).
        model(ModelSpec {
            id: "continuum-ai/qwen2.5-3b-instruct-GGUF",
            name: "Qwen2.5-3B-Instruct (general)",
            provider: "llama-server",
            arch: Arch::Qwen2,
            context_window: 32_768,
            max_output_tokens: 8192,
            tokens_per_second: 45.0,
            capabilities: &[Capability::TextGeneration, Capability::Chat, Capability::ToolUse, Capability::Streaming],
            gguf_hint: Some("huggingface.co/bartowski/Qwen2.5-3B-Instruct-GGUF"),
            chat_template: Some(QWEN35_CHAT_TEMPLATE),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>", "<|endoftext|>"],
            ..ModelSpec::default()
        }),
        // NOTE: benchmark OPPONENTS (Hermes, unsloth, cloud models) are DELIBERATELY absent
        // from this catalog — we never depend on either, ever. They are scored as external,
        // optional /v1 endpoints by the standalone harness in `benchmarks/coder/`, which
        // imports nothing from the product. Zero coupling by construction.
        //
        // Qwen2.5-Coder-32B — downloaded (~20 GB Q4_K_M) and the KV fit-gate fix lets it
        // serve, but DELIBERATELY NOT registered yet: measured live it DECLINES a directed
        // coding task (emits a bare "PASS") instead of acting, so serving it as the live
        // coder is strictly WORSE than the 14B, which reliably uses its tools. Kept verbatim
        // so re-enabling is a one-line uncomment ONCE its act-reflex is trained (the genome
        // loop over the DoD + recovery harness). Until then plan_serving keeps the 14B — the
        // model that actually works.
        // model(ModelSpec {
        //     id: "continuum-ai/qwen2.5-coder-32b-instruct-GGUF",
        //     name: "Qwen2.5-Coder-32B-Instruct",
        //     provider: "llama-server",
        //     arch: Arch::Qwen2,
        //     context_window: 32_768,
        //     max_output_tokens: 8192,
        //     tokens_per_second: 8.0,
        //     capabilities: &[Capability::TextGeneration, Capability::Chat, Capability::ToolUse, Capability::Streaming],
        //     gguf_hint: Some("huggingface.co/bartowski/Qwen2.5-Coder-32B-Instruct-GGUF"),
        //     chat_template: Some(QWEN35_CHAT_TEMPLATE),
        //     multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
        //     stop_sequences: &["<|im_end|>", "<|endoftext|>"],
        //     ..ModelSpec::default()
        // }),
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
            // Trainable safetensors base for the L3 genome loop. The `-GGUF` repo
            // above is serving-only; MLX/PEFT fine-tuning + the custodian convert
            // resolve THIS HF repo (cached, MLX-ready) through the canonical id.
            hf_source: Some("unsloth/Qwen2.5-0.5B-Instruct"),
            // gguf_local_path DERIVED from the id under genome/models (see coder-14b above).
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
            // gguf_local_path DERIVED from the id under genome/models (see coder-14b above).
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
            model_prefixes: &["mistral", "mixtral", "codestral", "open-mistral", "open-mixtral"],
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
            // One model per ds4-server process; the engine owns residency.
            capabilities: ProviderCapabilities {
                single_resident_model: true,
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
    /// HF safetensors repo id for the trainable form (see `Model::hf_source`).
    hf_source: Option<&'static str>,
    gguf_local_path: Option<&'static str>,
    mmproj_local_path: Option<&'static str>,
    chat_template: Option<&'static str>,
    multi_party_strategy: MultiPartyChatStrategy,
    stop_sequences: &'static [&'static str],
    /// Per-model decode defaults (#76). Defaults to the substrate floor
    /// ([`ModelSampling::default`], incl. the #181 anti-loop pair) via
    /// `..Default::default()`, so only a model we've measured/tuned overrides it.
    sampling: ModelSampling,
    /// See [`Model::persona_serving_eligible`]. Default TRUE; benchmark
    /// opponents / campaign-roster rows opt OUT so the autonomic planner can
    /// never conscript them as the citizens' model.
    persona_serving_eligible: bool,
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
        hf_source: spec.hf_source.map(str::to_string),
        gguf_local_path: spec.gguf_local_path.map(PathBuf::from),
        mmproj_local_path: spec.mmproj_local_path.map(PathBuf::from),
        chat_template: spec.chat_template.map(str::to_string),
        multi_party_strategy: spec.multi_party_strategy,
        stop_sequences: spec.stop_sequences.iter().map(|s| s.to_string()).collect(),
        sampling: spec.sampling,
        // Not a hand-authored fact: the size comes from the artifact's own
        // `general.parameter_count` header, hydrated once at registry load
        // ([`super::hydrate`]). The `ModelSpec` deliberately omits it so no
        // human types "4B" into a row — the sentinel `0` means "ask the GGUF".
        parameter_count: 0,
        persona_serving_eligible: spec.persona_serving_eligible,
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
