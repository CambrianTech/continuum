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
    /// Model accepts image input natively (raw pixels / base64). When
    /// absent, the sensory bridge classifies images via
    /// VisionDescriptionService → text → text-only model. CLAUDE.md
    /// "Sensory Architecture" — every persona sees, regardless of
    /// base model capability.
    Vision,
    /// Model accepts audio input natively (raw waveform / base64
    /// encoded). When absent, STT transcribes upstream → text-only
    /// model. New 2026-04-20 — was missing entirely; sensory bridge
    /// can't honor "every persona hears" without registry knowing
    /// who's audio-native vs needs-the-bridge.
    AudioInput,
    /// Model generates audio output natively (e.g. GPT-4o-audio,
    /// Gemini 2.5 native audio). When absent, TTS synthesizes
    /// downstream from the text response. New 2026-04-20.
    AudioOutput,
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

/// How prompt_assembly should shape multi-party chat history when
/// rendering a turn for this model. Single source of truth for
/// model-specific chat-shaping per the OOP-adapter rule (CLAUDE.md
/// "compression principle"): one decision lives in one place.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MultiPartyChatStrategy {
    /// Each speaker becomes its own user-role message with `Speaker:`
    /// prefix. Works for cloud models (Claude, GPT, etc.) trained on
    /// rich multi-party + multi-role distributions.
    #[default]
    NamePrefixedUserTurns,
    /// All history collapses into ONE user turn — a single block of
    /// transcript text — then the current message is appended in the
    /// same turn. The chat template sees system + one user, matching
    /// the user→assistant alternation that single-party-trained models
    /// like qwen3.5 expect.
    SingleUserTurnFlattenedHistory,
}

/// A single model's metadata. Loaded from TOML; never constructed in code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    /// Canonical id — matches the provider's API request body.
    /// Examples: "claude-sonnet-4-5-20250929", "gpt-4-turbo-preview",
    /// "continuum-ai/qwen3.5-4b-code-forged-GGUF".
    pub id: String,
    /// Display name for UIs and logs. Short, human-readable.
    /// Example: "Claude Sonnet 4.5" for id "claude-sonnet-4-5-20250929".
    /// If TOML omits it, loader falls back to the id (loud + ugly;
    /// encourages filling it in). Models aren't required to have it but
    /// any model whose label ever surfaces to a user probably should.
    #[serde(default)]
    pub name: Option<String>,
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
    /// Jinja chat template the adapter feeds to llama.cpp's renderer.
    /// Source of truth ordering: (1) template embedded in the GGUF's
    /// own metadata (`tokenizer.chat_template`), (2) this field, (3)
    /// hard error — never a built-in default, because llama.cpp's
    /// generic chatml uses boundary tokens that subtly differ from
    /// qwen3.5's training set (verified 2026-04-20: the mismatch
    /// manifested as `<|im_end|>` fragments bleeding into chat output).
    /// Adapters MUST NOT carry a per-model template as a constant; if
    /// the GGUF lacks one and TOML lacks one too, the right fix is to
    /// re-forge the GGUF with the template embedded, not to patch code.
    #[serde(default)]
    pub chat_template: Option<String>,
    /// How prompt_assembly should shape multi-party chat history for
    /// this model. Different models were trained on different chat
    /// distributions; sending a shape they didn't see causes silent
    /// failures (qwen3.5 emits 1-3 char EOG response when given 5+
    /// consecutive user-role messages with name prefixes — verified
    /// 2026-04-20 via tests/persona_respond_replay.rs).
    ///
    /// Source of truth lives here in the registry, not duplicated in
    /// adapter or prompt-assembly code. Adapters consume this — they
    /// don't decide it.
    #[serde(default)]
    pub multi_party_strategy: MultiPartyChatStrategy,
    /// Text-form stop sequences to apply at the scheduler boundary.
    /// Necessary when the GGUF's `tokenizer.ggml.eos_token_id` is
    /// wrong/missing for chat use — the model emits the chat-template
    /// terminator (e.g. `<|im_end|>`) as a real token but `is_eog_token`
    /// returns false because the EOS id in metadata doesn't match the
    /// chat-end token. Verified 2026-04-20 with qwen3.5-4b-code-forged:
    /// metadata reports eos_token_id=248046 (wrong); model emits 151645
    /// (`<|im_end|>`); scheduler had no way to stop. Listing the stop
    /// strings here lets the adapter pass them through to the scheduler's
    /// existing stop-sequence loop. Forge recipe should set the right
    /// EOS id in the GGUF at next bake; until then this is the bridge.
    #[serde(default)]
    pub stop_sequences: Vec<String>,
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
    /// Human-readable display name used in logs + error messages.
    /// Absent means "fall back to id" — fine for internal provider ids
    /// that happen to read well ("openai") but looks cramped for
    /// compounds ("docker-model-runner" vs "Docker Model Runner").
    #[serde(default)]
    pub name: Option<String>,
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
    /// Static id prefixes this provider's models match — lets
    /// `supports_model` answer "could future gpt-5 go here" without the
    /// TOML listing every historical id. Cloud providers with stable
    /// family naming use this; dynamic catalogs (DMR) leave it empty and
    /// dispatch via live /v1/models probes instead.
    #[serde(default)]
    pub model_prefixes: Vec<String>,
}

impl Provider {
    /// Display name for logs + errors. Falls back to id when TOML omits `name`.
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }
}
