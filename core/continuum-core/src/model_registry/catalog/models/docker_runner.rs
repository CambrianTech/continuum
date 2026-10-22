use super::super::*;

/// Docker Model Runner rows.
pub(super) fn docker_runner() -> Vec<Model> {
    vec![
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
            hf_source: Some("Qwen/Qwen2.5-7B-Instruct"),
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
            hf_source: Some("Qwen/Qwen2.5-7B-Instruct"),
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
    ]
}
