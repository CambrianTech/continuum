//! Value types for the model registry.
//!
//! No logic lives here. Just the vocabulary the hand-authored Rust catalog
//! (`catalog.rs`) and the code that reads it agree on. `Deserialize` /
//! `Serialize` are derived symmetrically — useful for tests, error
//! messages, and the ts-rs wire exports — not because anything parses or
//! writes a config file. (There is no TOML loader; that was deleted in #77.)

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::PathBuf;

/// Model architecture family. Typed (not stringly-typed) so call sites
/// use enum matching, not string comparison. Adding a new arch means:
/// (a) add the variant here, (b) add a catalog row with `arch = "new_arch"`.
/// Code that dispatches by arch gets a compile error reminding the author
/// to handle the new variant — precisely the pattern Joel's axiom calls
/// for ("code should NEVER know the model" — code knows the ARCHETYPES
/// via this enum, models are data).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    ts_rs::TS,
    schemars::JsonSchema,
)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/Arch.ts"
)]
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
    /// Mistral's own dense + MoE architecture line: Mistral 7B,
    /// Mixtral 8x7B / 8x22B, Codestral, Mistral Large. Distinct from
    /// Llama because of MoE routing semantics + tokenizer differences.
    Mistral,
    /// Escape hatch for architectures we haven't enumerated yet. Models
    /// tagged `Unknown` cannot be dispatched by arch — callers MUST fall
    /// through to capability checks. Used sparingly.
    Unknown,
}

/// Capabilities a model may advertise. Closed vocabulary; callers check
/// `model.has(Capability::ToolUse)` rather than pattern-matching on arch
/// or id. Adding a capability is a real architectural decision (new kind
/// of task) and should be rare.
///
/// Wire-exported via ts-rs because `PersonaContext` (recipe layer) and
/// the `cognition/respond` IPC payload both carry capability vocab as
/// a list of these values. TS hosts read/write the same kebab-case
/// strings serde produces.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    ts_rs::TS,
    schemars::JsonSchema,
)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/Capability.ts"
)]
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

/// Where a provider runs its inference. Resolver consumes this to honor
/// `LocalOrCloudPolicy` without needing a hardcoded provider-id list.
/// Providers default to [`ProviderKind::Cloud`] so adding a new cloud
/// provider catalog row doesn't require an explicit `kind` line; local
/// providers MUST declare `kind = "local"` explicitly.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Default,
    Serialize,
    Deserialize,
    ts_rs::TS,
)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/ProviderKind.ts"
)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// In-process or localhost backend. Inference runs on this host's
    /// hardware (CPU / GPU / unified memory). Examples: `llamacpp-local`,
    /// `docker-model-runner`.
    Local,
    /// Remote HTTP API. Inference runs off-host; this provider counts
    /// toward `TargetSilicon::Cloud` admission. Default for new providers.
    #[default]
    Cloud,
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

/// How tools are exchanged with a model/endpoint — the ONE tool-protocol
/// vocabulary across the whole substrate. Lives in `model_registry` (the
/// lowest layer) because a provider DECLARES its protocol in the catalog and
/// `ai` layers the rendering/parsing behavior on top (see
/// `ai::json_in_prompt_tools` — `tool_prompt`/`parse_text_call` hang off this
/// type). There is no second tool-protocol enum: the old `ProviderToolProtocol`
/// (registry) + `ai::ToolCallProtocol` (advertised) + `json_in_prompt_tools`'s
/// local copy all collapsed here (#69).
///
/// Both an endpoint's catalog declaration (`ProviderCapabilities.tool_protocol`)
/// AND an adapter's advertised surface (`AdapterCapabilities.tool_call_protocol`)
/// are this same enum — the concept is identical, so the type is too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolProtocol {
    /// Native OpenAI/Anthropic function-calling — the endpoint honors the
    /// `tools` request param and returns structured `tool_calls`/`tool_use`
    /// the substrate consumes directly. The cloud common case, hence the
    /// default (so a new cloud provider needs zero declaration; the adapter
    /// floor sets `None` explicitly instead of leaning on this default).
    #[default]
    NativeFunctionCalling,
    /// Tools described in the prompt; the model emits a JSON tool-call in
    /// its text output, which the substrate parses. The universal floor for
    /// local GGUF gateways that ignore the OpenAI `tools` param (proven
    /// 2026-06-21 against the forged 4B served over llama-server).
    JsonInPrompt,
    /// No tool calling at all — pure-text/embedding adapters (heuristic,
    /// embedding-only models). Tools are neither offered nor parsed.
    None,
}

/// Provider-level behavioral capabilities — the growable surface the
/// adapter consults INSTEAD of branching on `provider.id == "..."`.
///
/// Every field defaults to the cloud/common case, so adding a new provider
/// needs zero capability declaration; only the outliers (a local single-
/// slot gateway, an embedding endpoint) flip a flag. The surface GROWS by
/// adding a `#[serde(default)]` field here, NEVER by adding an id branch in
/// the adapter — that is the whole point of this struct (#55). The adapter
/// reads these; it does not decide them (same contract as
/// [`MultiPartyChatStrategy`]). One source of truth for "what does this
/// endpoint do," lifted out of the four `id == "unsloth"` / `id == "openai"`
/// branches that used to live in `OpenAICompatibleAdapter`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    /// How the endpoint exchanges tool calls. See [`ToolProtocol`].
    #[serde(default)]
    pub tool_protocol: ToolProtocol,
    /// Suppress the model's chain-of-thought by default. Some local
    /// reasoning GGUFs ramble/loop in their thinking (latency + the
    /// runaway-leak failure mode) yet answer correctly without it; cloud
    /// reasoning models manage their own and leave this `false`. An
    /// operator can still force thinking back on per-run (the adapter's
    /// `UNSLOTH_THINKING=on` escape hatch). Default: `false` (keep thinking).
    #[serde(default)]
    pub suppress_thinking: bool,
    // NOTE: embedding + image-generation are NOT here. They're capability
    // FACTS of a MODEL, not behavioral traits of a provider — a provider
    // "supports embeddings" only because it serves a model that does. They
    // live as `Capability::Embedding` / `Capability::ImageGeneration` on the
    // model rows and the adapter derives the provider-level capability by
    // SCANNING those rows (`models_for_provider(..).any(|m| m.has(..))`) —
    // uniform with how ToolUse and Vision are already derived (#68). One axis:
    // this struct carries BEHAVIOR (how the endpoint talks); `capabilities`
    // on the model carries WHAT it can do.
    /// The endpoint serves ONE resident model and IGNORES the request's
    /// `model` field (a single-slot local gateway that idle-unloads to
    /// free VRAM). The adapter must pre-flight model activation before each
    /// generation to guarantee the right brain is resident, failing loud if
    /// it cannot. Multi-model / cloud endpoints route by the request's
    /// `model` id and leave this `false`. Default: `false`.
    #[serde(default)]
    pub single_resident_model: bool,
    /// The endpoint publishes a DYNAMIC model catalog over `/v1/models`
    /// whose ids differ from the registry's logical ids (e.g. Docker Model
    /// Runner mangles `continuum-ai/qwen3.5-4b-code-forged-GGUF` into
    /// `huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf:latest`).
    /// The adapter must (1) fetch that live catalog at init, (2) resolve a
    /// logical id to the live id before each POST, and (3) answer
    /// `supports_model` from the live set — because what's available depends
    /// on `docker model pull` history, not a static row. A provider whose
    /// served model ids match the registry (cloud APIs, llama-server's
    /// resident GGUF) leaves this `false` and routes by the static rows.
    /// Default: `false`.
    #[serde(default)]
    pub dynamic_model_catalog: bool,
    /// The endpoint is a llama.cpp-family server that accepts llama.cpp's
    /// NATIVE sampling extension fields (`repeat_penalty`, etc.) on top of
    /// the OpenAI-standard body. Without `repeat_penalty` a llama.cpp server
    /// runs it at its default of 1.0 (disabled) and small forged reasoners
    /// loop — reprinting the same `<think>` paragraph until they burn the
    /// token budget without emitting a real reply (verified on the forged 4B
    /// over both DMR and llama-server). Cloud OpenAI-compatible providers
    /// reject the non-standard field, so they leave this `false`. Default:
    /// `false`.
    #[serde(default)]
    pub llamacpp_sampling_extensions: bool,
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
    ///
    /// Deprecated 2026-04-24 — produced echo-loops + name-prefix leaks
    /// because qwen3.5 reads the flattened transcript as a continuation
    /// pattern. Kept in the enum for backward-compat / experimentation;
    /// new model-registry entries should prefer `ProperChatMlSingleParty`.
    SingleUserTurnFlattenedHistory,
    /// Proper ChatML alternation for single-party-trained models. Walks
    /// the history and:
    ///   - own-persona prior turns become `role: assistant`
    ///   - human messages become `role: user`
    ///   - other-persona turns are DROPPED (single-party models cannot
    ///     handle multi-party — pretending they can is the bug
    ///     `SingleUserTurnFlattenedHistory` was working around)
    /// No closing-cue instruction is appended; the chat template's
    /// assistant-prefill signals the model to write the next assistant
    /// turn. Joel 2026-04-24, task #75: "no band aids — take the
    /// engineering path." This is the engineering path: shape the prompt
    /// for the model's actual training distribution rather than post-
    /// processing its output.
    ///
    /// Cost: personas on single-party models are honestly blind to
    /// other AI peers in the room. That's a real loss of cross-AI
    /// collaboration but it's an HONEST exposure of the model-capability
    /// constraint, not a workaround. Multi-party-capable models
    /// (Claude / GPT) keep `NamePrefixedUserTurns` and continue to see
    /// every speaker.
    ProperChatMlSingleParty,
}

/// Per-model sampling defaults (#76) — the model-level decode knobs, tuned per
/// blessed model and validated, living on the ONE `Model` row exactly like
/// `chat_template` / `stop_sequences` / `multi_party_strategy`. This is the
/// curated-catalog home Joel wants for "a few acceptable models tuned to our
/// grid's best cases": one place, data-driven, overridable per model, so the
/// forge/eval flywheel can WRITE measured values here instead of a human typing
/// magic numbers into an adapter ([[anti-loop-sampling-windowed-vs-unwindowed]],
/// [[no-hardcoded-heuristics-to-steer-cognition]]).
///
/// Deliberately does NOT carry `max_new_tokens`: response LENGTH is a ROLE
/// concern (a helper answers short, a researcher long), not a model fact — the
/// profile builder combines these model knobs with the role's budget.
///
/// `Default` is the SINGLE SOURCE of the substrate floor (the same values
/// `SamplingProfile::chat_defaults` projects), so a row that omits `sampling`
/// behaves exactly as the pre-#76 global default did — an unblessed model is
/// never worse off, and the anti-loop floor (#181) applies to every model.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSampling {
    /// Softmax temperature. Lower = more deterministic.
    pub temperature: f32,
    /// Top-K filter. 0 = disabled.
    pub top_k: u32,
    /// Nucleus sampling threshold.
    pub top_p: f32,
    /// Windowed repetition penalty (scans the last `repeat_last_n` tokens).
    pub repeat_penalty: f32,
    /// Window width for `repeat_penalty`, widened past llama.cpp's default 64
    /// so a loop whose span exceeds 64 tokens is still caught (#181). 0 = off.
    pub repeat_last_n: u32,
    /// Unwindowed repetition guard (whole-sequence token frequency) — catches
    /// gap-separated loops the window misses (#181). 0.0 = off.
    pub frequency_penalty: f32,
}

/// Per-model SERVING truths (2026-08-24) — how this model behaves on a llama
/// lane, stamped where measured. The substrate serves MANY models; anything a
/// lane decision branches on lives HERE as model truth, never as a blanket
/// policy that bakes one model's quirk into the architecture (Joel: "we are not
/// designing around one model but many").
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelServingPrefs {
    /// Load the vision projector on the MAIN persona lane. Default FALSE:
    /// llama-server hard-disables `--cache-reuse` on a multimodal lane
    /// ("cache_reuse is not supported by multimodal"), and chunk reuse is worth
    /// ~60-90s of re-prefill per act to every persona. Sight then routes through
    /// the vision sidecar + description bridge — the same path every text-only
    /// mind uses. A VL-first deployment (live-call heavy, KV economy secondary)
    /// opts IN per model.
    pub mmproj_on_main_lane: bool,
    /// Can this model's KV cache shift (llama context-shift / cache_reuse
    /// realignment)? `None` = unverified. `Some(false)` = measured incapable
    /// (hybrid/SWA attention — llama logs "cache_reuse is not supported by this
    /// context"): prompt shaping must then treat the prefix as EXTEND-ONLY,
    /// because any interior mutation re-prefills everything after it.
    pub kv_shiftable: Option<bool>,
}

impl Default for ModelServingPrefs {
    fn default() -> Self {
        Self {
            mmproj_on_main_lane: false,
            kv_shiftable: None,
        }
    }
}

impl Default for ModelSampling {
    /// The substrate floor — conservative chat defaults + the #181 anti-loop
    /// pair. The ONE place these numbers live; `SamplingProfile::chat_defaults`
    /// projects from here so there is never a second copy to drift.
    fn default() -> Self {
        Self {
            temperature: 0.6,
            top_k: 40,
            top_p: 0.95,
            repeat_penalty: 1.1,
            repeat_last_n: 320,
            frequency_penalty: 0.3,
        }
    }
}

/// A single model's metadata. Constructed by the Rust model catalog.
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
    /// from adapter reports at load; the catalog value is a reasonable
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
    /// HF safetensors repo id for the *trainable* form of this model.
    /// The GGUF (`gguf_hint` / `gguf_local_path`) is the SERVING artifact;
    /// MLX/PEFT fine-tuning needs the original safetensors weights instead.
    /// One canonical registry id (the row's `id`, GGUF-resolvable) threads all
    /// three consumers: serving resolves the GGUF off this row, while training
    /// (`mlx_lm.lora --model`) and the forge custodian's HF→PEFT→GGUF convert
    /// resolve the safetensors dir THROUGH this field. Without it there is no
    /// bridge from the canonical id to the HF cache and the training/convert
    /// lanes can't locate base weights. Absent for cloud models (they expose
    /// no local trainable form) and for serving-only local rows.
    /// Example: "unsloth/Qwen2.5-0.5B-Instruct".
    #[serde(default)]
    pub hf_source: Option<String>,
    /// Resolved local filesystem path to the GGUF. Populated at registry
    /// load by the artifact resolver from `gguf_hint`, local model roots,
    /// or an explicit path if one exists. TOML should normally leave this
    /// absent for portable models; the loader fills it when the artifact is
    /// already pulled locally.
    #[serde(default)]
    pub gguf_local_path: Option<PathBuf>,
    /// Local filesystem path to the multimodal projector GGUF (mmproj).
    /// Required for vision/audio-capable local models — the projector
    /// encodes raw image / audio bytes into tokens compatible with this
    /// model's embedding space. Without it, `Capability::Vision` /
    /// `AudioInput` declarations are unenforceable on the local path
    /// because the model can only consume text tokens. Cloud models
    /// (Anthropic, OpenAI) handle their own multimodal projection
    /// server-side and leave this absent.
    #[serde(default)]
    pub mmproj_local_path: Option<PathBuf>,
    /// Size on disk of the resolved GGUF, in bytes. Hydrated ONCE by
    /// [`resolve_model_artifacts`](crate::model_registry::artifacts::resolve_model_artifacts)
    /// at the same moment `gguf_local_path` is resolved — the artifact's path and its
    /// size are one fact discovered together, so they are stored together.
    ///
    /// It lives on the row because it is an input to EVERY residency estimate
    /// (`weights + lanes × kv(window) + compute reserve`), and those estimates run on
    /// the governor's accounting tick. Deriving it by `stat`ing the file per call —
    /// which is what `footprint_for` used to do — means a syscall per poll for a number
    /// that cannot change while the path is valid, and it puts filesystem I/O on a hot
    /// path that must not block. `None` means "not resolved yet", never "zero bytes".
    #[serde(default)]
    pub weights_bytes: Option<u64>,
    /// Size on disk of the resolved multimodal projector, in bytes. Same hydration and
    /// the same reason as [`weights_bytes`](Self::weights_bytes) — and it is a REAL
    /// residency term the weights alone omit: a vision lane loads the projector
    /// alongside the model, so an estimate that counts only the GGUF under-reports a
    /// vision lane by the projector's whole size.
    #[serde(default)]
    pub mmproj_bytes: Option<u64>,
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
    /// Per-model decode defaults (#76) — temperature/top-k/top-p + the #181
    /// anti-loop pair, pre-resolved off this row into the persona's
    /// `SamplingProfile` (like `chat_template`/`stop_sequences` above). Omitted
    /// rows get [`ModelSampling::default`] (the substrate floor) via serde, so a
    /// row that doesn't tune sampling is byte-identical to the pre-#76 default.
    #[serde(default)]
    pub sampling: ModelSampling,
    /// Per-model serving truths (see [`ModelServingPrefs`]) — lane decisions
    /// branch on the ROW, never on a hardcoded model name. Omitted rows get the
    /// defaults (text-only main lane, unverified kv-shift) via serde.
    #[serde(default)]
    pub serving: ModelServingPrefs,
    /// Whether the autonomic serving planner may pick this row to host the
    /// PERSONAS. Benchmark opponents and campaign-roster rows carry real Ready
    /// GGUFs (so the matrix can serve them on demand) but must NEVER be
    /// conscripted into the citizens' serving plan — the planner picked
    /// Hermes-4.3 as the persona model TWICE (2026-07-12) purely because it
    /// was the largest Ready artifact on disk, silently re-homing every mind
    /// onto the opponent's flagship and confounding a day of measurements.
    /// `serving/pin` BYPASSES this flag: an explicit operator pin is consent,
    /// the autonomic tick is not ([[first-neighborhood-and-model-scale-consent]]).
    /// Defaults TRUE (a normal model is servable); opponent rows opt OUT.
    #[serde(default = "default_true")]
    pub persona_serving_eligible: bool,
    /// Total trained parameter count, as the artifact itself declares it
    /// (`general.parameter_count` in the GGUF header, or the provider
    /// `/v1/models` listing where one exposes it). `0` is the absent
    /// sentinel — hydrated once at registry load from the authoritative
    /// source and kept on the row, NEVER re-derived from a name substring
    /// (`"4b"`, `"7b"`) at a call site. This is the size fact model-fit
    /// selection reads to pick "the largest model that fits this host":
    /// param count × bytes-per-param (the quant) is the weight footprint.
    /// Cloud models whose API doesn't report a count stay at `0` — an
    /// honest "unknown", not a guess.
    #[serde(default)]
    pub parameter_count: u64,
}

/// Serde default for `Model::persona_serving_eligible` — absent means servable.
fn default_true() -> bool {
    true
}

impl Model {
    /// True if this model advertises the given capability. Preferred
    /// over any `model.id == "foo"` or `model.id.starts_with("bar")`
    /// check — see CLAUDE.md's adapter axiom.
    pub fn has(&self, cap: Capability) -> bool {
        self.capabilities.contains(&cap)
    }

    /// Parameter count expressed in billions for display / fit math, or
    /// `None` when the count is the absent sentinel (`0`). Derived from the
    /// stored authoritative count — never parsed from the model name.
    pub fn parameter_count_billions(&self) -> Option<f32> {
        (self.parameter_count > 0).then(|| self.parameter_count as f32 / 1e9)
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
    /// Where this provider runs inference. See [`ProviderKind`]. Defaults
    /// to `Cloud` when omitted in the catalog — local providers must declare
    /// `kind = "local"` explicitly so adding a new cloud provider doesn't
    /// require touching this field.
    #[serde(default)]
    pub kind: ProviderKind,
    /// Behavioral capabilities of this provider's endpoint — the growable
    /// surface the adapter reads instead of branching on `id` (#55). See
    /// [`ProviderCapabilities`]. Defaults to the cloud/common case, so cloud
    /// providers declare nothing; local gateways flip the outlier flags.
    #[serde(default)]
    pub capabilities: ProviderCapabilities,
}

impl Provider {
    /// Display name for logs + errors. Falls back to id when TOML omits `name`.
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }
}
