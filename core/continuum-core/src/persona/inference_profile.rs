//! `PersonaInferenceProfile` — substrate-resolved inference parameters
//! per persona.
//!
//! ## Doctrine
//!
//! Per [[intent-driven-api-not-hot-patches]] (Joel, 2026-06-01):
//! > "Less hacking around. More intent."
//!
//! Every adapter — `LlamaCppAdapter`, `AnthropicAdapter`,
//! `OpenAICompatibleAdapter`, future `OpenClawAdapter` /
//! `HermesAdapter` / etc — takes the SAME small profile shape.
//! `PersonaSpawnerModule` (#121) is the single place that derives the
//! profile from `(role_template, hw_tier_descriptor, model_meta,
//! persona_state)`; adapters consume the resolved values.
//!
//! This is the load-bearing reason for the profile's existence: ONE
//! derivation location, MANY consumers. Without it, every adapter
//! grows its own walk through the persona graph (different defaults,
//! different field ordering, divergent debugging surface).
//!
//! ## What gets pre-resolved into the profile
//!
//! Knobs the substrate KNOWS from the persona's declared intent:
//! - which model the role + tier picked
//! - how much context the role's cognition profile wants
//! - how big a prompt the persona will realistically submit
//!   (RAG-built prompts cap at the role's
//!   `cognition_defaults.max_response_chars` budget input side)
//! - how many concurrent sequences (single-tenant persona = 1;
//!   shared-base + LoRA paging host = many)
//! - GPU offload depth (derived from hw_tier_descriptor)
//! - sampling defaults (from role's cognition profile)
//! - per-model knobs (chat_template, stop_sequences) — pre-resolved
//!   so adapters don't re-query the registry on every call
//!
//! ## What stays in the model registry (TOML)
//!
//! The MODEL's intrinsic properties — `arch`, `chat_template`,
//! `stop_sequences`, `multi_party_strategy`, `gguf_local_path`,
//! `context_window` (model's trained limit). The registry is the
//! source of truth for per-model facts ([[orm-everything-not-hand-
//! edited-files]]); the profile carries pre-resolved values into the
//! adapter without forcing a round-trip.
//!
//! ## References
//!
//! - [[intent-driven-api-not-hot-patches]] — the doctrine this serves
//! - [[lcd-model-qwen25-05b-and-foundry-lora]] — the LCD model the
//!   spawner picks for Compat tier
//! - [[no-fallbacks-ever]] — if the profile can't be derived (no model
//!   for the tier, no GGUF on disk for a local adapter), substrate
//!   HARD ERRORS with diagnosis instead of silently degrading
//! - #121 PersonaSpawnerModule — the producer of profiles
//! - #133 LCD-first substrate spawn path — the slice that lands this

use crate::persona::hw_tier_descriptor::HwTierCategory;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ts_rs::TS;
use uuid::Uuid;

/// Sampling defaults derived from the persona's role cognition profile.
/// Per-call overrides are still possible at the inference command
/// layer; this is the substrate's "what the persona wants by default."
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/SamplingProfile.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct SamplingProfile {
    /// Softmax temperature. Lower = more deterministic, higher = more
    /// varied. Helper-shape personas (depth ≤ 30) usually 0.5–0.7;
    /// engineer/researcher shapes 0.7–0.9; creative shapes 0.9–1.1.
    pub temperature: f32,
    /// Top-K filter. 0 = disabled; typical 20–80.
    pub top_k: u32,
    /// Nucleus sampling threshold. Typical 0.9–0.95.
    pub top_p: f32,
    /// Repeat penalty. 1.0 = off; typical 1.05–1.15 for chat. Windowed
    /// over the last `repeat_last_n` tokens.
    pub repeat_penalty: f32,
    /// Window (trailing tokens) `repeat_penalty` scans. Widened past
    /// llama.cpp's default 64 so a loop whose span exceeds 64 tokens is
    /// still caught (#181). 0 = disabled.
    #[ts(type = "number")]
    pub repeat_last_n: u32,
    /// Unwindowed repetition guard — penalizes a token by its whole-
    /// sequence frequency, catching gap-separated loops the windowed
    /// penalty misses (#181). 0.0 = off. llama.cpp-family gateways only.
    pub frequency_penalty: f32,
    /// Maximum tokens to generate per response. Derived from role's
    /// `max_response_chars` divided by approximate chars-per-token
    /// (typically 4 for English).
    pub max_new_tokens: u32,
}

/// Response-length fallback (tokens) when the role doesn't specify a budget.
/// A ROLE concern, not a model fact — so it lives here, not on `ModelSampling`.
pub const DEFAULT_MAX_NEW_TOKENS: u32 = 512;

impl SamplingProfile {
    /// Project a persona sampling profile from the model's row-level decode
    /// defaults (#76) plus the role's response budget. This is the ONE seam
    /// that combines the two sources: model-level knobs (temperature, top-k/p,
    /// the #181 anti-loop pair) come from [`ModelSampling`] on the `Model` row;
    /// `max_new_tokens` is the role's length budget. Blessed/tuned models carry
    /// their own `ModelSampling`; unblessed rows carry the floor.
    pub fn from_model(
        m: &crate::model_registry::types::ModelSampling,
        max_new_tokens: u32,
    ) -> Self {
        Self {
            temperature: m.temperature,
            top_k: m.top_k,
            top_p: m.top_p,
            repeat_penalty: m.repeat_penalty,
            repeat_last_n: m.repeat_last_n,
            frequency_penalty: m.frequency_penalty,
            max_new_tokens,
        }
    }

    /// Conservative chat defaults — the substrate floor. Projects from
    /// [`ModelSampling::default`] so the floor numbers (incl. the #181 anti-loop
    /// pair) live in exactly ONE place and can never drift from the Model row's
    /// default. Suitable when there is no model row (tests, scripted adapters).
    pub fn chat_defaults() -> Self {
        Self::from_model(
            &crate::model_registry::types::ModelSampling::default(),
            DEFAULT_MAX_NEW_TOKENS,
        )
    }
}

/// Errors a profile producer can return when it can't derive a complete
/// profile from the persona's declared intent.
///
/// Per [[no-fallbacks-ever]], the substrate REFUSES to construct a
/// silently-degraded profile (e.g., picking a wrong model because the
/// declared one is missing, defaulting to a tiny context to fit weak
/// hardware, etc.). Every miss is named.
///
/// `Eq` not derived — `InsufficientHeadroom` carries `f32` fields and
/// floats can hold NaN. PartialEq is enough for tests.
#[derive(Debug, Clone, PartialEq)]
pub enum InferenceProfileError {
    /// The persona's role template references a model_id but the model
    /// registry has no row for it.
    UnknownModel { model_id: String, role_id: String },
    /// The model registry row exists but `gguf_local_path` is None and
    /// the adapter is local-inference (would need a GGUF on disk).
    NoLocalGguf {
        model_id: String,
        gguf_hint: Option<String>,
    },
    /// The hw_tier_descriptor doesn't carry enough headroom for the
    /// model's declared minimum. E.g., role wants a 7B but tier
    /// `maxParamsBFits = 3.0`. Caller can route via grid or refuse.
    InsufficientHeadroom {
        model_id: String,
        tier_id: String,
        required_params_b: f32,
        tier_max_params_b: f32,
    },
}

impl std::fmt::Display for InferenceProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownModel { model_id, role_id } => write!(
                f,
                "PersonaInferenceProfile: role '{}' references model '{}' \
                 not found in registry. Either add the model row in \
                 the Rust catalog (catalog.rs) or update the role_template.",
                role_id, model_id
            ),
            Self::NoLocalGguf {
                model_id,
                gguf_hint,
            } => {
                write!(
                    f,
                    "PersonaInferenceProfile: local-inference profile for '{}' \
                     needs a resolved gguf_local_path. ",
                    model_id
                )?;
                match gguf_hint {
                    Some(hint) => write!(
                        f,
                        "Hint says '{}' — pull the artifact or set \
                         gguf_local_path explicitly.",
                        hint
                    ),
                    None => write!(
                        f,
                        "No gguf_hint set either; add either field to the \
                         model registry row."
                    ),
                }
            }
            Self::InsufficientHeadroom {
                model_id,
                tier_id,
                required_params_b,
                tier_max_params_b,
            } => write!(
                f,
                "PersonaInferenceProfile: model '{}' needs ≥{:.1}B params; \
                 tier '{}' only fits up to {:.1}B locally. Route via grid \
                 inference or pick a smaller model for this tier.",
                model_id, required_params_b, tier_id, tier_max_params_b
            ),
        }
    }
}

impl std::error::Error for InferenceProfileError {}

/// Substrate-resolved inference parameters per persona.
///
/// The `PersonaSpawnerModule` derives this from (role_template,
/// hw_tier_descriptor, model_meta, persona_state) and hands it to the
/// chosen adapter. Every adapter — local llama.cpp, cloud Anthropic /
/// OpenAI, future OpenClaw / Hermes — takes this same shape; no
/// adapter walks the persona graph itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PersonaInferenceProfile.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct PersonaInferenceProfile {
    /// Persona's UUID — for tracing, observability, log correlation.
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// Display name — shows up in inference command logs and grids.
    pub persona_name: String,

    /// Model registry id (e.g. `"continuum-ai/qwen2.5-0.5b-instruct-GGUF"`).
    /// Adapter uses this to log + report what's loaded; resolution
    /// already happened upstream.
    pub model_id: String,
    /// Pre-resolved on-disk GGUF path. `None` for cloud-routed
    /// adapters; mandatory for local llama.cpp.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gguf_local_path: Option<PathBuf>,

    /// Hardware class the persona is running on. Adapter uses this to
    /// pick device-specific tunings (e.g., disable Metal on Compat
    /// when [[#131]]'s Metal hang fix isn't landed yet).
    pub tier_category: HwTierCategory,
    /// Stable tier id (e.g. `"mac_intel_metal_discrete"`). Carried for
    /// diagnostics; the category is the routing key.
    pub tier_id: String,

    /// Context window the persona uses at runtime — typically smaller
    /// than the model's `context_window` (trained limit). Derived from
    /// role's depth preference + tier headroom; bounds the KV cache.
    pub context_length: u32,
    /// Maximum prompt size the persona realistically submits in one
    /// batch. Drives compute-graph reservation in the scheduler. Per
    /// the #130 finding: RAG-built persona prompts are 200-500 tokens
    /// today, so 512 is a conservative default; richer RAG context
    /// pushes higher.
    pub n_ubatch: u32,
    /// Logical batch size — typically equal to context_length or
    /// capped by hardware. Affects prompt-fill throughput.
    pub n_batch: u32,
    /// Concurrent sequence count. 1 for single-persona; higher for
    /// shared-base + LoRA paging hosts ([[#122]]).
    pub n_seq_max: u32,
    /// GPU offload depth. -1 = all layers on GPU; 0 = CPU-only; N =
    /// N bottom layers on GPU, rest on CPU. Derived from
    /// `tier_descriptor.localVideoCapable` AND substrate's awareness
    /// of any per-tier known-bad inference paths (e.g., #131 forces 0
    /// on Compat until the Metal init hang lands a fix).
    pub n_gpu_layers: i32,

    /// Sampling defaults from the role's cognition profile.
    pub sampling: SamplingProfile,

    /// Chat template — pre-resolved from the model registry row so the
    /// adapter doesn't re-query on every call. None means
    /// "model embeds chat_template in its GGUF metadata; let llama.cpp
    /// use that."
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_template: Option<String>,
    /// Stop sequences. Empty vec = rely on model's EOG token.
    #[serde(default)]
    pub stop_sequences: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SamplingProfile::chat_defaults yields the same numbers the
    /// backend's `SamplingConfig::chat()` uses today, so substituting
    /// the profile path doesn't change persona behavior.
    #[test]
    fn chat_defaults_match_backend_chat_config() {
        let s = SamplingProfile::chat_defaults();
        assert_eq!(s.temperature, 0.6);
        assert_eq!(s.top_k, 40);
        assert_eq!(s.top_p, 0.95);
        assert_eq!(s.repeat_penalty, 1.1);
        assert_eq!(s.max_new_tokens, 512);
    }

    /// Round-trips through serde without dropping fields. camelCase on
    /// the wire so TS consumers get a natural shape.
    #[test]
    fn profile_serde_roundtrip_uses_camel_case() {
        let profile = PersonaInferenceProfile {
            persona_id: Uuid::nil(),
            persona_name: "Paige".to_string(),
            model_id: "continuum-ai/qwen2.5-0.5b-instruct-GGUF".to_string(),
            gguf_local_path: Some(PathBuf::from("/tmp/qwen.gguf")),
            tier_category: HwTierCategory::Compat,
            tier_id: "mac_intel_metal_discrete".to_string(),
            context_length: 2048,
            n_ubatch: 512,
            n_batch: 2048,
            n_seq_max: 1,
            n_gpu_layers: 0,
            sampling: SamplingProfile::chat_defaults(),
            chat_template: Some("{% for ... %}".to_string()),
            stop_sequences: vec!["<|im_end|>".to_string()],
        };
        let json = serde_json::to_string(&profile).expect("serialize");
        // camelCase markers
        assert!(json.contains("\"personaId\":"));
        assert!(json.contains("\"personaName\":\"Paige\""));
        assert!(json.contains("\"modelId\":"));
        assert!(json.contains("\"ggufLocalPath\":"));
        assert!(json.contains("\"tierCategory\":\"compat\""));
        assert!(json.contains("\"contextLength\":2048"));
        assert!(json.contains("\"nUbatch\":512"));
        assert!(json.contains("\"nGpuLayers\":0"));
        assert!(json.contains("\"chatTemplate\":"));
        assert!(json.contains("\"stopSequences\":[\"<|im_end|>\"]"));
        let back: PersonaInferenceProfile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, profile);
    }

    /// Optional fields skipped when None — keeps wire shape tight.
    #[test]
    fn optional_fields_omitted_when_none() {
        let profile = PersonaInferenceProfile {
            persona_id: Uuid::nil(),
            persona_name: "Paige".to_string(),
            model_id: "claude-sonnet-4-5".to_string(),
            gguf_local_path: None,
            tier_category: HwTierCategory::Cloud,
            tier_id: "cloud".to_string(),
            context_length: 200000,
            n_ubatch: 512,
            n_batch: 200000,
            n_seq_max: 1,
            n_gpu_layers: -1,
            sampling: SamplingProfile::chat_defaults(),
            chat_template: None,
            stop_sequences: vec![],
        };
        let json = serde_json::to_string(&profile).expect("serialize");
        assert!(!json.contains("ggufLocalPath"));
        assert!(!json.contains("chatTemplate"));
        // stopSequences defaults to empty Vec — still present (Vec<T>
        // doesn't have a skip_serializing_if; that's fine, empty array
        // is unambiguous on the wire).
        assert!(json.contains("\"stopSequences\":[]"));
    }

    /// InferenceProfileError variants render with actionable diagnoses.
    #[test]
    fn error_messages_name_what_went_wrong() {
        let err = InferenceProfileError::UnknownModel {
            model_id: "nonexistent/model".to_string(),
            role_id: "helper".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("helper"), "names the role: {msg}");
        assert!(msg.contains("nonexistent/model"), "names the model: {msg}");
        assert!(
            msg.contains("catalog.rs"),
            "points at the registry: {msg}"
        );

        let err = InferenceProfileError::NoLocalGguf {
            model_id: "continuum-ai/qwen2.5-0.5b".to_string(),
            gguf_hint: Some("huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF".to_string()),
        };
        let msg = err.to_string();
        assert!(msg.contains("gguf_local_path"), "names the missing field");
        assert!(
            msg.contains("Qwen2.5-0.5B-Instruct-GGUF"),
            "echoes the hint"
        );

        let err = InferenceProfileError::InsufficientHeadroom {
            model_id: "qwen-7b".to_string(),
            tier_id: "cpu_only".to_string(),
            required_params_b: 7.0,
            tier_max_params_b: 1.5,
        };
        let msg = err.to_string();
        assert!(msg.contains("7.0"));
        assert!(msg.contains("1.5"));
        assert!(msg.contains("grid inference"));
    }
}
