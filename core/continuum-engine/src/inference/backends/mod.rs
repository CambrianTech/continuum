//! Model backends — the llama.cpp adapter/scheduler, the GGUF header reader
//! (`read_gguf_metadata`), and the sampling vocabulary (`SamplingConfig`,
//! `JSON_GRAMMAR`) the adapters share.

pub mod llamacpp;
pub mod llamacpp_scheduler;

// Feature selection belongs here; the adapter owns its platform availability
// through its module-level gate. Keep that platform contract in one place.
#[cfg(feature = "mlx")]
pub mod mlx_adapter;

use std::path::Path;

// ─── Unified Text Generation ─────────────────────────────────────────────────

/// Sampling configuration for text generation.
/// All fields are required — no silent defaults.
#[derive(Debug, Clone)]
pub struct SamplingConfig {
    /// Temperature for softmax sampling. 0.0 = greedy (argmax).
    pub temperature: f64,
    /// Repetition penalty applied on logits (llama.cpp style). 1.0 = disabled.
    pub repeat_penalty: f32,
    /// Top-k sampling: keep only the k highest-probability tokens. 0 = disabled.
    pub top_k: usize,
    /// Top-p (nucleus) sampling: keep smallest set of tokens with cumulative prob >= p. 1.0 = disabled.
    pub top_p: f64,
    /// GBNF grammar (e.g. JSON shape). When Some, scheduler attaches it
    /// to the sampler chain BEFORE temp/dist so output is constrained to
    /// match the grammar. None = unconstrained. Set by adapters when the
    /// caller's request_format demands a structured shape (JsonObject).
    pub grammar: Option<String>,
}

impl SamplingConfig {
    /// Config for code generation: greedy, moderate repeat penalty.
    pub fn code() -> Self {
        Self {
            temperature: 0.0,
            repeat_penalty: 1.1,
            top_k: 0,
            top_p: 1.0,
            grammar: None,
        }
    }
    /// Config for chat: slight creativity, standard repeat penalty.
    pub fn chat() -> Self {
        Self {
            temperature: 0.6,
            repeat_penalty: 1.1,
            top_k: 40,
            top_p: 0.95,
            grammar: None,
        }
    }
}

/// Built-in JSON grammar (GBNF) — produces a valid JSON object. Used when
/// callers request `response_format: JsonObject`. Keep this aligned with the
/// vendored llama.cpp `grammars/json.gbnf`.
pub const JSON_GRAMMAR: &str = r#"
root   ::= object
value  ::= object | array | string | number | ("true" | "false" | "null") ws

object ::=
  "{" ws (
            string ":" ws value
    ("," ws string ":" ws value)*
  )? "}" ws

array  ::=
  "[" ws (
            value
    ("," ws value)*
  )? "]" ws

string ::=
  "\"" (
    [^"\\\x7F\x00-\x1F] |
    "\\" (["\\bfnrt] | "u" [0-9a-fA-F]{4})
  )* "\"" ws

number ::= ("-"? ([0-9] | [1-9] [0-9]{0,15})) ("." [0-9]+)? ([eE] [-+]? [0-9] [1-9]{0,15})? ws
ws ::= | " " | "\n" [ \t]{0,20}
"#;

// ─── GGUF Metadata ───────────────────────────────────────────────────────────

pub use crate::inference_capability::gguf_keys::GgufMetadata;

/// Read common metadata from a GGUF file without loading weights.
pub fn read_gguf_metadata(path: &Path) -> Result<GgufMetadata, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("Failed to open GGUF: {e}"))?;
    let content = crate::inference_capability::gguf_keys::read_bounded_header(&mut file, &|| false)
        .map_err(|e| format!("Failed to read GGUF: {e}"))?;

    crate::inference_capability::gguf_keys::common_metadata(&content, path)
}

// ─── Helpers ─────────────────────────────────────────────────────────────────
