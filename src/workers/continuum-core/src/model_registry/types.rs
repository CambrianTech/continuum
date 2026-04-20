//! Value types for the model registry.
//!
//! No logic lives here. Just the vocabulary that config TOML + Rust code
//! agree on. Everything is `Deserialize` so the loader can parse directly
//! into these from TOML; `Serialize` is provided symmetrically (useful
//! for tests + error messages), not because anything writes TOML back.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::PathBuf;

/// Model architecture family. Typed (not stringly-typed) so call sites
/// use enum matching, not string comparison. Adding a new arch means:
/// (a) add the variant here, (b) add a TOML row with `arch = "new_arch"`.
/// Code that dispatches by arch gets a compile error reminding the author
/// to handle the new variant — precisely the pattern Joel's axiom calls
/// for ("code should NEVER know the model" — code knows the ARCHETYPES
/// via this enum, models are data).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arch {
    Qwen2,
    Qwen3,
    Qwen35,
    Llama,
    Claude,
    Gpt,
    Gemini,
    Grok,
    Deepseek,
    /// Escape hatch for architectures we haven't enumerated yet. Models
    /// tagged `Unknown` cannot be dispatched by arch — callers MUST fall
    /// through to capability checks. Used sparingly.
    Unknown,
}

/// Capabilities a model may advertise. Closed vocabulary; callers check
/// `model.has(Capability::ToolUse)` rather than pattern-matching on arch
/// or id. Adding a capability is a real architectural decision (new kind
/// of task) and should be rare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    TextGeneration,
    Chat,
    ToolUse,
    Vision,
    Streaming,
    FineTuning,
    LoraAdapter,
    ImageGeneration,
    Embedding,
    Reranking,
}

/// HTTP authentication mode for a provider's API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthKind {
    /// `Authorization: Bearer <token>` from env.
    Bearer,
    /// Custom per-provider API key header (e.g. `x-api-key` for Anthropic).
    /// The actual header name is provider-specific and lives in the
    /// adapter's transport code; this variant just signals "needs a key
    /// in a non-bearer shape."
    ApiKey,
    /// No auth (localhost, open endpoints).
    None,
}

/// A single model's metadata. Loaded from TOML; never constructed in code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    /// Canonical id — matches the provider's API request body.
    /// Examples: "claude-sonnet-4-5-20250929", "gpt-4-turbo-preview",
    /// "continuum-ai/qwen3.5-4b-code-forged-GGUF".
    pub id: String,
    /// Foreign key into `Provider.id`.
    pub provider: String,
    pub arch: Arch,
    /// Training-time context window. NOT a tunable — it's the model's
    /// stated capability. Code that needs "how much can I fit?" should
    /// use this; code that needs "how much do I budget?" should subtract
    /// `max_output_tokens + safety_margin`.
    pub context_window: u32,
    pub max_output_tokens: u32,
    /// Decoded tokens per second at single-stream inference. Populated
    /// from adapter reports at load; the TOML value is a reasonable
    /// startup estimate, the live registry updates it post-init.
    pub tokens_per_second: f32,
    /// Sorted set of advertised capabilities. BTreeSet for deterministic
    /// iteration and cheap containment checks.
    #[serde(default)]
    pub capabilities: BTreeSet<Capability>,
    /// Input cost per 1k tokens, USD. 0.0 for local.
    #[serde(default)]
    pub cost_input_per_1k: f32,
    /// Output cost per 1k tokens, USD. 0.0 for local.
    #[serde(default)]
    pub cost_output_per_1k: f32,
    /// Canonical OCI / HF reference for the underlying GGUF, if local.
    /// Example: "huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf".
    /// Absent for cloud models.
    #[serde(default)]
    pub gguf_hint: Option<String>,
    /// Resolved local filesystem path to the GGUF. Populated at registry
    /// load by the loader (via DMR manifest lookup from `gguf_hint`),
    /// NOT by the TOML author. TOML may leave this absent; the loader
    /// fills it if the GGUF is pulled locally.
    #[serde(default)]
    pub gguf_local_path: Option<PathBuf>,
}

impl Model {
    /// True if this model advertises the given capability. Preferred
    /// over any `model.id == "foo"` or `model.id.starts_with("bar")`
    /// check — see CLAUDE.md's adapter axiom.
    pub fn has(&self, cap: Capability) -> bool {
        self.capabilities.contains(&cap)
    }
}

/// A single provider's metadata. Loaded from TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    /// Canonical id. Foreign key for `Model.provider`.
    pub id: String,
    /// Base URL for HTTP requests. For OpenAI-compatible endpoints, the
    /// adapter appends `/v1/chat/completions`; for bespoke APIs, the
    /// adapter knows its own paths.
    pub base_url: String,
    /// Env var name that holds the API key. `None` for providers that
    /// don't need one (localhost). The adapter reads the env var at
    /// request time so key rotations don't require restart.
    #[serde(default)]
    pub api_key_env: Option<String>,
    /// Default model id to use when the caller doesn't specify one.
    /// `None` for providers with dynamic catalogs (DMR) — caller must
    /// specify.
    #[serde(default)]
    pub default_model: Option<String>,
    pub auth: AuthKind,
}
