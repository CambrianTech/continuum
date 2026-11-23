//! Model backends — the llama.cpp adapter/scheduler, the GGUF header reader
//! (`read_gguf_metadata`), and the sampling vocabulary (`SamplingConfig`,
//! `JSON_GRAMMAR`) the adapters share.

pub mod llamacpp;
pub mod persona_scheduler;

use std::path::Path;

use candle_core::quantized::gguf_file;

// ─── Unified Text Generation ─────────────────────────────────────────────────

/// The knobs a request states for its sampling. The turn's sampler (`llama::TurnSampler`) takes
/// them; everything else, including any tool grammar and think budget, comes from the render that
/// produced her prompt.
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
    /// GBNF grammar a turn with no chat render must match (the caller's `response_format`
    /// JsonObject). A rendered turn's grammar is the render's.
    pub grammar: Option<String>,
}

impl SamplingConfig {
    /// Config for code generation: greedy, moderate repeat penalty.
    pub fn code() -> Self {
        Self { temperature: 0.0, repeat_penalty: 1.1, top_k: 0, top_p: 1.0, grammar: None }
    }
    /// Config for chat: slight creativity, standard repeat penalty.
    pub fn chat() -> Self {
        Self { temperature: 0.6, repeat_penalty: 1.1, top_k: 40, top_p: 0.95, grammar: None }
    }

    /// The knobs as the turn's sampler takes them.
    pub fn knobs(&self) -> llama::SamplingKnobs {
        llama::SamplingKnobs {
            temperature: self.temperature as f32,
            // A top_k at or past the vocabulary keeps all of it, so one past i32 means the same.
            top_k: self.top_k.min(i32::MAX as usize) as i32,
            top_p: self.top_p as f32,
            repeat_penalty: self.repeat_penalty,
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

/// GGUF metadata extracted before backend construction.
pub struct GgufMetadata {
    pub architecture: String,
    pub context_length: usize,
    pub model_name: Option<String>,
}

/// Read common metadata from a GGUF file without loading weights.
pub fn read_gguf_metadata(path: &Path) -> Result<GgufMetadata, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("Failed to open GGUF: {e}"))?;
    let content =
        gguf_file::Content::read(&mut file).map_err(|e| format!("Failed to read GGUF: {e}"))?;

    // general.architecture is REQUIRED — silently falling back to "llama" would
    // route a qwen/mistral/phi/etc. model through the wrong backend and produce
    // garbage output or outright crash. Rule-2 violation (fallbacks are illegal)
    // fixed 2026-04-23. If a GGUF is missing this metadata, that's a broken file,
    // not a thing to paper over. Read via the ONE shared canonical-key reader.
    let architecture =
        crate::inference_capability::gguf_keys::architecture(&content).ok_or_else(|| {
            format!(
                "GGUF {} is missing required metadata key 'general.architecture' — cannot \
             determine backend. Silent fallback to 'llama' has been removed; fix the \
             GGUF file or re-export it with proper metadata.",
                path.display()
            )
        })?;

    // context_length via the shared reader: architecture-specific key first,
    // then the historical `llama.context_length` fallback — the ONE place that
    // policy is defined. If neither exists, that's a broken GGUF, not a thing
    // to guess 4096 for.
    let context_length =
        crate::inference_capability::gguf_keys::context_length(&content, &architecture)
            .map(|v| v as usize)
            .ok_or_else(|| {
                format!(
                    "GGUF {} (architecture={architecture}) is missing context_length metadata \
             (tried '{architecture}.context_length' and 'llama.context_length'). Silent \
             fallback to 4096 has been removed; fix the GGUF file.",
                    path.display()
                )
            })?;

    let model_name = crate::inference_capability::gguf_keys::general_name(&content);

    Ok(GgufMetadata {
        architecture,
        context_length,
        model_name,
    })
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

