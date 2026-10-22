use super::super::*;

/// llama-server evaluation and fleet rows (BTL, Hermes-4, Qwen3, phi-4, Qwen2.5 coders).
pub(super) fn llama_server_catalog() -> Vec<Model> {
    vec![
        // BTL-4-Compact — an OUTSIDE claim we intend to falsify, not adopt. Added to make it
        // measurable on our own harness rather than argued about from a model card.
        //
        // Every field here is read from the artifact's own declarations, never the card:
        //   • arch: the base repo's config.json says `Qwen3_5MoeForConditionalGeneration` —
        //     256 experts, 8 per token, 40 layers, hidden 2048. So `qwen3_5_moe`, a routed MoE.
        //     The card describes it as "35B dense", which contradicts its OWN config; that
        //     discrepancy is the first thing to check and the reason for the skepticism.
        //   • context_window: `max_position_embeddings: 262144` from that same config. The live
        //     served window is still the planner's business (model-max ∩ VRAM-KV fit, #31) —
        //     the operator running this reported ~250k "was costing you KV", which is exactly
        //     the intersection our planner is supposed to compute instead of hardcoding.
        //   • Arch::Unknown is deliberate: qwen3_5_moe is not an enumerated arch here, and
        //     llama-server reads the real arch from the GGUF header. Naming a wrong enum to
        //     avoid an `Unknown` would be a guess wearing a type.
        //
        // The Compact build is TEXT-ONLY: the base repo carries a vision tower (a nested vision
        // config with hidden_size 1152 → out_hidden_size 2048), and this GGUF drops it. Hence no
        // Vision capability and no mmproj — `badtheorylabs/BTL-4-Compact` ships exactly one file,
        // `BTL-4-IQ2_XXS.gguf` (9,967,966,240 bytes, unsharded). If the vision half is ever
        // wanted, `bartowski/badtheorylabs_BTL-4-GGUF` carries mmproj + a full IQ2 ladder.
        //
        // chat_template: None + --jinja, because BTL-4 uses a
        // `<tool_call><function=name><parameter=x>` DSL rather than stock Qwen's JSON form; the
        // correct template ships INSIDE the GGUF. Overriding it here — or letting llama.cpp fall
        // back to a builtin — silently disables every tool call the model tries to make.
        //
        // NOT persona_serving_eligible: unevaluated third-party weights must not be adopted by
        // the autonomic planner. It reaches this model only by explicit id, for evals. The repo
        // ships its own `eval/bfcl_compact.py` + `eval/probe_tools.py`, so their methodology can
        // be run alongside our held-out suite — the point being that a model which has learned
        // the benchmark scores well on THEIR harness and poorly on ours.
        model(ModelSpec {
            id: "badtheorylabs/BTL-4-Compact",
            name: "BTL-4-Compact IQ2_XXS (third-party claim — evaluate, do not adopt)",
            provider: "llama-server",
            arch: Arch::Unknown, // qwen3_5_moe — llama-server reads it from the GGUF header
            context_window: 262_144,
            max_output_tokens: 8192,
            tokens_per_second: 0.0, // UNMEASURED on this grid — do not fill in an estimate
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/badtheorylabs/BTL-4-Compact"),
            chat_template: None, // the tool-call DSL template is embedded in the GGUF (--jinja)
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
            hf_source: Some("NousResearch/Hermes-4.3-36B"),
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
            hf_source: Some("Qwen/Qwen3-32B"),
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
            hf_source: Some("Qwen/Qwen3-Coder-30B-A3B-Instruct"),
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
            hf_source: Some("microsoft/phi-4"),
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
            hf_source: Some("Qwen/Qwen2.5-Coder-32B-Instruct"),
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
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/bartowski/Qwen2.5-Coder-3B-Instruct-GGUF"),
            chat_template: Some(QWEN35_CHAT_TEMPLATE),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>", "<|endoftext|>"],
            hf_source: Some("Qwen/Qwen2.5-Coder-3B-Instruct"),
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
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/bartowski/Qwen2.5-Coder-1.5B-Instruct-GGUF"),
            chat_template: Some(QWEN35_CHAT_TEMPLATE),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>", "<|endoftext|>"],
            hf_source: Some("Qwen/Qwen2.5-Coder-1.5B-Instruct"),
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
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/bartowski/Qwen2.5-3B-Instruct-GGUF"),
            chat_template: Some(QWEN35_CHAT_TEMPLATE),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>", "<|endoftext|>"],
            hf_source: Some("Qwen/Qwen2.5-3B-Instruct"),
            ..ModelSpec::default()
        }),
        // NOTE: benchmark OPPONENTS (Hermes, cloud models) are DELIBERATELY absent
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
        //     hf_source: Some("Qwen/Qwen2.5-Coder-32B-Instruct"),
        //     ..ModelSpec::default()
        // }),
    ]
}
