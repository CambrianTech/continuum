// This is a placeholder for the actual content of catalog.rs
// The implementation details would be added here based on the instructions

use std::collections::HashMap;

pub struct ModelRegistry {
    pub models: HashMap<String, Model>,
}

pub struct Model {
    pub id: String,
    pub provider: String,
    pub architecture: Arch,
    pub capabilities: Vec<Capability>,
    pub gguf_path: Option<String>,
}

#[derive(Debug)]
pub enum Arch {
    // compacted-19b GGUF model
    ModelEntry {
        id: "compacted-19b",
        provider: Provider::LlamaServer,
        arch: Arch::Qwen3,
        capabilities: vec![Capability::Chat, Capability::ToolUse, Capability::Streaming],
        gguf_path: "genome/models/compacted-19b",
    },
    Qwen3,
    // other architectures...
}

#[derive(Debug)]
pub enum Capability {
    Chat,
    ToolUse,
    Streaming,
    // other capabilities...
}

// Added for compacted-19b GGUF serving
let compacted_19b = ModelEntry {
    id: "compacted-19b",
    provider: Provider::LlamaServer,
    arch: Arch::Qwen3,
    capabilities: Capabilities::Chat | Capabilities::ToolUse | Capabilities::Streaming,
    gguf_path_resolver: GgufPathResolver::from_id_token("compacted-19b"),
};
