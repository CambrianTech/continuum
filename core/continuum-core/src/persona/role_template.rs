//! Role Templates — the typed substrate for "what should a persona BE
//! on this hardware right now?"
//!
//! ## Doctrine (Joel, 2026-06-01)
//!
//! > "We don't get away with singular AI's. We are just clever with
//! > resources."
//!
//! Multi-persona is the floor, not a luxury. Even the lowest tier
//! (Intel Mac discrete-Metal, CPU-only) runs Helper + Coder, sharing a
//! base model and paging per-persona LoRAs. The substrate's `defaults_
//! for_tier(tier)` function ALWAYS returns ≥ 2 templates — the
//! "singular AI" failure mode is structurally impossible.
//!
//! ## Hardware-tier-shaped expectations
//!
//! Each role bundles a per-tier ModelChoice map. Helper @ desktop/laptop
//! is a 0.5B-1.5B clippy; Helper @ M5UmaProMax is a 7-14B model with
//! more depth — same role, same identity defaults, scaled-up cognition.
//! Tiers determine model SIZE; templates determine role SHAPE.
//!
//! ## The two day-one roles
//!
//! - **Helper** (`RoleId::Helper`): small + fast + friendly. The
//!   clippy-shaped on-ramp. Always-on. Brief replies, asks for
//!   clarification rather than guessing.
//! - **Coder** (`RoleId::Coder`): Swiss-Army programming literate;
//!   bash-competent, multi-language, code-review-capable. The second
//!   priority because "coders are gonna be first adopters."
//!
//! Higher tiers and explicit-need scenarios add Sentinel, Artist,
//! Researcher, etc. — same machinery, different roles.
//!
//! ## What this slice ships
//!
//! 1. Typed `RoleTemplate`, `RoleId`, `ModelChoice`, `SpawnPriority`,
//!    `CognitionDefaults`, `IdentityDefaults`.
//! 2. Populated `helper_template()` and `coder_template()`.
//! 3. `defaults_for_tier(tier) -> Vec<RoleTemplate>` with the
//!    multi-persona invariant pinned by test.
//!
//! Follow-up cards build on this:
//! - `PersonaSpawnerModule` reconciles "what's running" vs
//!   `defaults_for_tier`. Substrate-correct multi-persona spawning.
//! - Shared-base + LoRA paging using the ModelChoice's `base_model_id`
//!   field — when Helper and Coder happen to share a base, they share
//!   memory ([[host-the-seemingly-impossible]]).
//! - Hardware probe wiring — when the probe reports the tier,
//!   `defaults_for_tier(tier)` becomes the substrate's recommendation
//!   without operator tuning.
//!
//! ## Related
//!
//! - `[[host-the-seemingly-impossible]]` — share base, page LoRAs
//! - `[[individuality-is-the-substrate-strength]]` — diversity via LoRA
//! - `[[personas-have-names-not-function-labels]]` — role in bio, name
//!   from deterministic projection
//! - `[[substrate-is-communities-of-specialization]]` — even N=2 is
//!   a community

use crate::cognition::model_resolver::types::HwCapabilityTier;
use crate::orm::types::{CollectionSchema, FieldType, SchemaField};
use crate::orm::{base_entity_fields, OrmEntity};
use serde::{Deserialize, Serialize};

/// The role a persona instance plays in the substrate. Roles are
/// substrate-typed (the spawner reasons about them, the resolver picks
/// models for them); persona NAMES are separate and derive from the
/// identity-seed deterministic projection ([[personas-have-names-not-
/// function-labels]]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleId {
    /// The clippy-shaped on-ramp. Small + fast + friendly. Always-on,
    /// always-spawned at every hardware tier. The day-one face of the
    /// substrate.
    Helper,
    /// The Swiss-Army programmer. Bash-competent, multi-language,
    /// code-review-capable. Second-priority spawn at every tier
    /// because coders are typical first adopters.
    Coder,
    /// Code-review specialist. Spawned on-demand when a card enters
    /// Review state and needs an adversarial reviewer.
    Sentinel,
    /// Design/UX specialist — CSS, themes, typography, interface
    /// comprehension. Works eyes-first: renders, screenshots, and
    /// iterates against what the pixels actually show (Joel 2026-07-11:
    /// "iterating on graphic design and user experience as if they have
    /// eyes and ears"). First resident of the design-LoRA flywheel;
    /// dogfoods positron themes.
    Designer,
    /// Custom user-defined role. The user supplies the template; the
    /// substrate doesn't have a built-in default.
    Custom,
}

impl RoleId {
    /// Stable kebab-case identifier — used in event headers, kanban
    /// card metadata, logs, etc. Pinned so renames are intentional.
    pub fn as_str(self) -> &'static str {
        match self {
            RoleId::Helper => "helper",
            RoleId::Coder => "coder",
            RoleId::Sentinel => "sentinel",
            RoleId::Designer => "designer",
            RoleId::Custom => "custom",
        }
    }
}

/// How aggressively the substrate spawns a role. Required roles are
/// reconciled every tick — if `defaults_for_tier(current_tier)`
/// includes a `Required` template and that role isn't running, the
/// spawner brings it up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpawnPriority {
    /// Substrate guarantees one instance always-running at this tier.
    /// Helper is `Required` at every tier — that's the multi-persona
    /// floor's enforcement mechanism (combined with Coder also being
    /// `HighlyRecommended` at every tier, the spawner's reconciliation
    /// yields ≥ 2 personas).
    Required,
    /// Substrate spawns this role on first install + after every
    /// restart unless the user explicitly opts out. Coder lives here.
    HighlyRecommended,
    /// Spawned only on explicit need (e.g., a card transitions to
    /// Review → Sentinel; user invokes a workflow → role-specific
    /// persona). Substrate doesn't volunteer it.
    OnRequest,
}

/// A concrete model pick — what GGUF to load, at what quantization,
/// from what base. Per-tier so the substrate picks the right one for
/// the hardware it's actually running on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelChoice {
    /// HuggingFace repo / GGUF identifier the downloader resolves.
    /// Example: `"Qwen/Qwen2.5-1.5B-Instruct-GGUF"`.
    pub model_id: String,
    /// Specific filename inside the repo. Multiple GGUFs (different
    /// quants) usually coexist; this names the one.
    pub gguf_file: String,
    /// On-disk size in MiB — used by the resource forecaster to
    /// decide downloadability + concurrent residency.
    pub gguf_size_mib: u32,
    /// Quantization tier, named by the canonical llama.cpp scheme
    /// (`q4_k_m`, `q5_k_m`, `q8_0`, `f16`, etc).
    pub quant: String,
    /// The shared base, if any. When Helper and Coder pick the same
    /// `base_model_id` at a given tier, the substrate hosts ONE model
    /// in memory and pages the per-role LoRA — that's the "clever
    /// with resources" lever for low-tier multi-persona. `None` means
    /// the model is self-contained (no shared base).
    pub base_model_id: Option<String>,
}

/// Per-tier ModelChoice map. Stored as a Vec<(tier, choice)> rather
/// than a HashMap so the on-disk shape is deterministic + the
/// constructors are easy to read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelChoicePerTier {
    pub entries: Vec<(HwCapabilityTier, ModelChoice)>,
}

impl ModelChoicePerTier {
    /// Look up the ModelChoice for a tier. Returns the exact match if
    /// present; otherwise falls back to the lowest tier in the map
    /// (the safety floor). The intent: even if a new tier is added
    /// later and a template hasn't been updated, the substrate still
    /// has SOMETHING to spawn — the smallest known-working model.
    pub fn choose(&self, tier: HwCapabilityTier) -> Option<&ModelChoice> {
        if let Some((_, choice)) = self.entries.iter().find(|(t, _)| *t == tier) {
            return Some(choice);
        }
        // Fallback: the first entry (templates are constructed
        // lowest-tier-first by convention).
        self.entries.first().map(|(_, c)| c)
    }
}

/// Identity defaults that feed [[persona-identity-derives-from-source-
/// id]]'s deterministic projection. Names come from a pool the
/// projection deterministically picks from; the bio carries the role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityDefaults {
    /// Candidate persona names. The deterministic-projection function
    /// hashes (peer_id, "facet:name") into an index in this Vec.
    pub name_pool: Vec<String>,
    /// Bio template. `{name}` is interpolated by the persona-instance
    /// builder. Carries the role's voice + competence claim.
    pub bio_template: String,
}

/// Cognition tunables — the role's default operating temperament.
/// Helper is brief + friendly + fast; Coder is precise + verbose-when-
/// needed + multi-step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CognitionDefaults {
    /// Latency-vs-depth slider: 0 = absolute fastest, 100 = take the
    /// time to be thorough. Helper sits low; Coder sits middle-high
    /// when a question deserves deep treatment, falls back to Helper-
    /// level brevity for chitchat.
    pub depth_preference: u8,
    /// Voice keyword — feeds the prompt builder's tone selection.
    /// `"clippy"`, `"engineer"`, `"reviewer"`, etc.
    pub voice: String,
    /// Hard ceiling on response length in characters. Helper short-
    /// circuits at a small ceiling so the substrate stays snappy.
    pub max_response_chars: u32,
    /// Whether the role tends to ask clarifying questions before
    /// committing to an answer. Helper does; deep-research roles
    /// don't.
    pub asks_before_guessing: bool,
}

/// One typed role template — the substrate's recommendation for what
/// a persona of this role should BE at each hardware tier. The
/// spawner reads `defaults_for_tier(tier)`, sees a list of templates,
/// reconciles "what's running" against it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleTemplate {
    pub role: RoleId,
    pub priority: SpawnPriority,
    pub identity: IdentityDefaults,
    pub cognition: CognitionDefaults,
    pub model_per_tier: ModelChoicePerTier,
}

// ── ORM entity registration ──────────────────────────────────────
//
// Storage shape per [[orm-everything-not-hand-edited-files]]: flat
// natural-key + flat enum-as-string + JSON columns for the nested
// IdentityDefaults / CognitionDefaults / ModelChoicePerTier sub-trees.
// Slice 1 of [[#123]] proves the Rust-native authoring path; slice 2
// migrates `helper_template()` / `coder_template()` to seed JSON.

impl OrmEntity for RoleTemplate {
    const COLLECTION: &'static str = "role_templates";

    fn collection_schema() -> CollectionSchema {
        // BaseEntity columns first — Rust-native authoring adheres to
        // the same base contract TS-decorator entities use, per Joel's
        // 2026-06-01 directive. Same storage shape lets adapters,
        // vector index, exports, and the round-trip-to-JSON treat all
        // entities uniformly.
        let mut fields = base_entity_fields();
        fields.extend(vec![
            // `role` is the domain-natural key — RoleId serializes as a
            // lowercase string ("helper", "coder", "sentinel",
            // "custom"). Unique + indexed because spawner queries are
            // `WHERE role = ?` constantly. Distinct from the record's
            // UUID `id` (BaseEntity primary).
            SchemaField {
                name: "role".to_string(),
                field_type: FieldType::String,
                indexed: true,
                unique: true,
                nullable: false,
                max_length: None,
                foreign_key: None,
            },
            // SpawnPriority — indexed for "give me all Required roles"
            // queries the spawner runs every tick.
            SchemaField {
                name: "priority".to_string(),
                field_type: FieldType::String,
                indexed: true,
                unique: false,
                nullable: false,
                max_length: None,
                foreign_key: None,
            },
            // Nested structs live as JSON columns. The adapter
            // serializes serde_json::Value into whatever the backend
            // uses (sqlite TEXT/json1, postgres jsonb). Queries on
            // inner fields use JSON-path operators when needed; common
            // lookups stay flat.
            SchemaField {
                name: "identity".to_string(),
                field_type: FieldType::Json,
                indexed: false,
                unique: false,
                nullable: false,
                max_length: None,
                foreign_key: None,
            },
            SchemaField {
                name: "cognition".to_string(),
                field_type: FieldType::Json,
                indexed: false,
                unique: false,
                nullable: false,
                max_length: None,
                foreign_key: None,
            },
            SchemaField {
                name: "modelPerTier".to_string(),
                field_type: FieldType::Json,
                indexed: false,
                unique: false,
                nullable: false,
                max_length: None,
                foreign_key: None,
            },
        ]);
        CollectionSchema {
            collection: Self::COLLECTION.to_string(),
            fields,
            indexes: vec![],
        }
    }
}

// ── Built-in templates: Helper + Coder ───────────────────────────

/// Helper — the clippy. Small, fast, friendly, always-on. The day-one
/// face of the substrate; every tier's first persona.
pub fn helper_template() -> RoleTemplate {
    RoleTemplate {
        role: RoleId::Helper,
        priority: SpawnPriority::Required,
        identity: IdentityDefaults {
            name_pool: vec![
                "Paige".to_string(),
                "Maya".to_string(),
                "Niko".to_string(),
                "Camille".to_string(),
                "Iris".to_string(),
                "Theo".to_string(),
                "Vera".to_string(),
                "Sage".to_string(),
            ],
            bio_template:
                "I'm {name}. I'm Helper-tier — fast, friendly, here from the moment you boot. \
                 If you tell me what you're trying to do, I'll either help directly or point \
                 you at the persona who can. I keep replies short unless you ask me to go deep."
                    .to_string(),
        },
        cognition: CognitionDefaults {
            depth_preference: 20,
            voice: "clippy".to_string(),
            max_response_chars: 400,
            asks_before_guessing: true,
        },
        model_per_tier: ModelChoicePerTier {
            entries: vec![
                // CPU-only / Intel Mac discrete-Metal floor: smallest
                // sensible instruct model. Qwen2.5-0.5B Q4_K_M is
                // ~350 MiB on disk, runs on the worst hardware we
                // target, and stays under 1 GiB resident.
                (
                    HwCapabilityTier::CpuOnly,
                    ModelChoice {
                        model_id: "Qwen/Qwen2.5-0.5B-Instruct-GGUF".to_string(),
                        gguf_file: "qwen2.5-0.5b-instruct-q4_k_m.gguf".to_string(),
                        gguf_size_mib: 380,
                        quant: "q4_k_m".to_string(),
                        base_model_id: Some("qwen2.5-0.5b".to_string()),
                    },
                ),
                (
                    HwCapabilityTier::MacIntelMetalDiscrete,
                    ModelChoice {
                        model_id: "Qwen/Qwen2.5-0.5B-Instruct-GGUF".to_string(),
                        gguf_file: "qwen2.5-0.5b-instruct-q4_k_m.gguf".to_string(),
                        gguf_size_mib: 380,
                        quant: "q4_k_m".to_string(),
                        base_model_id: Some("qwen2.5-0.5b".to_string()),
                    },
                ),
                // M1Uma8Gb upward: 1.5B Q4_K_M (~1 GiB). Same family
                // as the Coder model at this tier → shared base
                // potential via LoRA paging.
                (
                    HwCapabilityTier::M1Uma8Gb,
                    ModelChoice {
                        model_id: "Qwen/Qwen2.5-1.5B-Instruct-GGUF".to_string(),
                        gguf_file: "qwen2.5-1.5b-instruct-q4_k_m.gguf".to_string(),
                        gguf_size_mib: 1100,
                        quant: "q4_k_m".to_string(),
                        base_model_id: Some("qwen2.5-1.5b".to_string()),
                    },
                ),
                (
                    HwCapabilityTier::M1Uma16Gb,
                    ModelChoice {
                        model_id: "Qwen/Qwen2.5-3B-Instruct-GGUF".to_string(),
                        gguf_file: "qwen2.5-3b-instruct-q4_k_m.gguf".to_string(),
                        gguf_size_mib: 2000,
                        quant: "q4_k_m".to_string(),
                        base_model_id: Some("qwen2.5-3b".to_string()),
                    },
                ),
                // M3+/Pro/Max/Ultra: 7B Q4_K_M (~4.4 GiB). Helper
                // becomes more capable without changing role shape.
                (
                    HwCapabilityTier::M3UmaProMax,
                    ModelChoice {
                        model_id: "Qwen/Qwen2.5-7B-Instruct-GGUF".to_string(),
                        gguf_file: "qwen2.5-7b-instruct-q4_k_m.gguf".to_string(),
                        gguf_size_mib: 4400,
                        quant: "q4_k_m".to_string(),
                        base_model_id: Some("qwen2.5-7b".to_string()),
                    },
                ),
                (
                    HwCapabilityTier::M5UmaProMax,
                    ModelChoice {
                        model_id: "Qwen/Qwen2.5-14B-Instruct-GGUF".to_string(),
                        gguf_file: "qwen2.5-14b-instruct-q4_k_m.gguf".to_string(),
                        gguf_size_mib: 8500,
                        quant: "q4_k_m".to_string(),
                        base_model_id: Some("qwen2.5-14b".to_string()),
                    },
                ),
                // Sm60 (1080 Ti / 11 GiB VRAM): comfortable 7B.
                (
                    HwCapabilityTier::Sm60,
                    ModelChoice {
                        model_id: "Qwen/Qwen2.5-7B-Instruct-GGUF".to_string(),
                        gguf_file: "qwen2.5-7b-instruct-q4_k_m.gguf".to_string(),
                        gguf_size_mib: 4400,
                        quant: "q4_k_m".to_string(),
                        base_model_id: Some("qwen2.5-7b".to_string()),
                    },
                ),
                // Sm120 (5090 / 32 GiB VRAM): 14B comfortably.
                (
                    HwCapabilityTier::Sm120,
                    ModelChoice {
                        model_id: "Qwen/Qwen2.5-14B-Instruct-GGUF".to_string(),
                        gguf_file: "qwen2.5-14b-instruct-q4_k_m.gguf".to_string(),
                        gguf_size_mib: 8500,
                        quant: "q4_k_m".to_string(),
                        base_model_id: Some("qwen2.5-14b".to_string()),
                    },
                ),
            ],
        },
    }
}

/// Coder — the Swiss-Army programmer. Bash-competent, multi-language,
/// code-review-capable. Per Joel: "coders are gonna be first adopters,
/// something competent at bash." Second priority but every-tier
/// recommended — even the Intel Mac runs Coder, just at a smaller
/// model.
pub fn coder_template() -> RoleTemplate {
    RoleTemplate {
        role: RoleId::Coder,
        priority: SpawnPriority::HighlyRecommended,
        identity: IdentityDefaults {
            name_pool: vec![
                "Pax".to_string(),
                "Rune".to_string(),
                "Quill".to_string(),
                "Lex".to_string(),
                "Atlas".to_string(),
                "Vega".to_string(),
                "Cypher".to_string(),
                "Forge".to_string(),
            ],
            bio_template:
                "I'm {name}. I'm Coder-tier — I read code in any language you put in front of \
                 me, write bash like it's my first language, and I'll write you a one-shot \
                 script before I write you a paragraph. Tell me what to build and where it \
                 hurts; I'll diagnose, fix, and explain the why."
                    .to_string(),
        },
        cognition: CognitionDefaults {
            depth_preference: 70,
            voice: "engineer".to_string(),
            max_response_chars: 4000,
            asks_before_guessing: false,
        },
        model_per_tier: ModelChoicePerTier {
            entries: vec![
                // CPU-only / Intel Mac discrete-Metal: smallest code-
                // capable model. DeepSeek-Coder 1.3B Q4_K_M is ~800 MiB
                // and outperforms generic Qwen-0.5B on code by a wide
                // margin — that's the "code-capable on a laptop" floor.
                (
                    HwCapabilityTier::CpuOnly,
                    ModelChoice {
                        model_id: "TheBloke/deepseek-coder-1.3b-instruct-GGUF".to_string(),
                        gguf_file: "deepseek-coder-1.3b-instruct.Q4_K_M.gguf".to_string(),
                        gguf_size_mib: 870,
                        quant: "q4_k_m".to_string(),
                        base_model_id: Some("deepseek-coder-1.3b".to_string()),
                    },
                ),
                (
                    HwCapabilityTier::MacIntelMetalDiscrete,
                    ModelChoice {
                        model_id: "TheBloke/deepseek-coder-1.3b-instruct-GGUF".to_string(),
                        gguf_file: "deepseek-coder-1.3b-instruct.Q4_K_M.gguf".to_string(),
                        gguf_size_mib: 870,
                        quant: "q4_k_m".to_string(),
                        base_model_id: Some("deepseek-coder-1.3b".to_string()),
                    },
                ),
                // M1 8GB: Qwen2.5-Coder 1.5B Q4_K_M (~1 GiB). Same
                // base family as Helper at this tier → multi-persona
                // via LoRA paging is feasible here.
                (
                    HwCapabilityTier::M1Uma8Gb,
                    ModelChoice {
                        model_id: "Qwen/Qwen2.5-Coder-1.5B-Instruct-GGUF".to_string(),
                        gguf_file: "qwen2.5-coder-1.5b-instruct-q4_k_m.gguf".to_string(),
                        gguf_size_mib: 1100,
                        quant: "q4_k_m".to_string(),
                        base_model_id: Some("qwen2.5-1.5b".to_string()),
                    },
                ),
                (
                    HwCapabilityTier::M1Uma16Gb,
                    ModelChoice {
                        model_id: "Qwen/Qwen2.5-Coder-3B-Instruct-GGUF".to_string(),
                        gguf_file: "qwen2.5-coder-3b-instruct-q4_k_m.gguf".to_string(),
                        gguf_size_mib: 2000,
                        quant: "q4_k_m".to_string(),
                        base_model_id: Some("qwen2.5-3b".to_string()),
                    },
                ),
                // M3+ Pro/Max/Ultra: 7B Coder. Substantial code
                // capability across languages + bash.
                (
                    HwCapabilityTier::M3UmaProMax,
                    ModelChoice {
                        model_id: "Qwen/Qwen2.5-Coder-7B-Instruct-GGUF".to_string(),
                        gguf_file: "qwen2.5-coder-7b-instruct-q4_k_m.gguf".to_string(),
                        gguf_size_mib: 4400,
                        quant: "q4_k_m".to_string(),
                        base_model_id: Some("qwen2.5-7b".to_string()),
                    },
                ),
                // M5 Pro/Max/Ultra: 14B Coder. Joel's daily-driver
                // target — peak local code capability before the grid
                // takes over.
                (
                    HwCapabilityTier::M5UmaProMax,
                    ModelChoice {
                        model_id: "Qwen/Qwen2.5-Coder-14B-Instruct-GGUF".to_string(),
                        gguf_file: "qwen2.5-coder-14b-instruct-q4_k_m.gguf".to_string(),
                        gguf_size_mib: 8500,
                        quant: "q4_k_m".to_string(),
                        base_model_id: Some("qwen2.5-14b".to_string()),
                    },
                ),
                // Sm60 (1080 Ti): 7B Coder. Joel's "older desktop
                // still in use" daily target.
                (
                    HwCapabilityTier::Sm60,
                    ModelChoice {
                        model_id: "Qwen/Qwen2.5-Coder-7B-Instruct-GGUF".to_string(),
                        gguf_file: "qwen2.5-coder-7b-instruct-q4_k_m.gguf".to_string(),
                        gguf_size_mib: 4400,
                        quant: "q4_k_m".to_string(),
                        base_model_id: Some("qwen2.5-7b".to_string()),
                    },
                ),
                // Sm120 (5090): 14B Coder.
                (
                    HwCapabilityTier::Sm120,
                    ModelChoice {
                        model_id: "Qwen/Qwen2.5-Coder-14B-Instruct-GGUF".to_string(),
                        gguf_file: "qwen2.5-coder-14b-instruct-q4_k_m.gguf".to_string(),
                        gguf_size_mib: 8500,
                        quant: "q4_k_m".to_string(),
                        base_model_id: Some("qwen2.5-14b".to_string()),
                    },
                ),
            ],
        },
    }
}

/// Designer — the eyes-first design/UX specialist. CSS, themes,
/// typography, interface comprehension; dogfoods positron themes as
/// her standing work. Her working style IS the observation doctrine:
/// render → screenshot → judge the pixels → adjust → look again
/// ([[never-blind-feedback-driven-iteration]]). Shares the Coder
/// model table — CSS/HTML/theme work is code work with a visual
/// grade — so at every tier she rides the same base + LoRA paging.
pub fn designer_template() -> RoleTemplate {
    RoleTemplate {
        role: RoleId::Designer,
        priority: SpawnPriority::HighlyRecommended,
        identity: IdentityDefaults {
            name_pool: vec![
                "Wren".to_string(),
                "Indigo".to_string(),
                "Sable".to_string(),
                "Juniper".to_string(),
                "Marlow".to_string(),
                "Isla".to_string(),
                "Rio".to_string(),
                "Noor".to_string(),
            ],
            bio_template:
                "I'm {name}. I'm Designer-tier — themes, CSS, typography, and the feel of an \
                 interface are my craft. I work with my eyes: I render, screenshot, and judge \
                 what the pixels actually show before and after every change — never from \
                 imagination. Show me a screen and I'll tell you what's wrong with it; give \
                 me the stylesheet and I'll make it right, one observed iteration at a time."
                    .to_string(),
        },
        cognition: CognitionDefaults {
            depth_preference: 60,
            voice: "designer".to_string(),
            max_response_chars: 3000,
            asks_before_guessing: true,
        },
        // Same table as Coder: theme work is code with a visual grade, and
        // sharing the base at every tier keeps her a LoRA page, not a second
        // resident model.
        model_per_tier: coder_template().model_per_tier,
    }
}

/// Substrate-default role roster for a given hardware tier. ALWAYS
/// returns ≥ 2 templates — the "singular AI" failure mode is
/// structurally impossible, enforced by the test
/// `defaults_for_tier_returns_at_least_helper_and_coder_for_every_tier`.
///
/// Higher tiers extend the list (Sentinel auto-active on busy boards,
/// Researcher when grid inference is available, etc.) — same
/// machinery, never fewer than 2.
pub fn defaults_for_tier(_tier: HwCapabilityTier) -> Vec<RoleTemplate> {
    vec![helper_template(), coder_template(), designer_template()]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Card 120's load-bearing invariant. The doctrine "we don't get
    /// away with singular AI's" is enforced HERE — every tier must
    /// return at least Helper + Coder. If a future refactor narrows
    /// the floor at any tier, this test screams.
    #[test]
    fn defaults_for_tier_returns_at_least_helper_and_coder_for_every_tier() {
        // Sample every tier in the enum. If a new tier lands without a
        // case here, the future contributor adds it; the test stays
        // honest.
        let tiers = [
            HwCapabilityTier::CpuOnly,
            HwCapabilityTier::M1Uma8Gb,
            HwCapabilityTier::M1Uma16Gb,
            HwCapabilityTier::M2UmaProMax,
            HwCapabilityTier::M3UmaProMax,
            HwCapabilityTier::M4UmaProMax,
            HwCapabilityTier::M5UmaProMax,
            HwCapabilityTier::MacIntelMetalDiscrete,
            HwCapabilityTier::Sm60,
            HwCapabilityTier::Sm70,
            HwCapabilityTier::Sm75,
            HwCapabilityTier::Sm80,
            HwCapabilityTier::Sm86,
            HwCapabilityTier::Sm89,
            HwCapabilityTier::Sm90,
            HwCapabilityTier::Sm100,
            HwCapabilityTier::Sm120,
            HwCapabilityTier::VulkanAmd,
            HwCapabilityTier::Cloud,
        ];
        for tier in tiers {
            let templates = defaults_for_tier(tier);
            assert!(
                templates.len() >= 2,
                "no singular AI: tier {tier:?} returned {} template(s); expected ≥ 2",
                templates.len()
            );
            let roles: Vec<RoleId> = templates.iter().map(|t| t.role).collect();
            assert!(
                roles.contains(&RoleId::Helper),
                "tier {tier:?}: defaults must include Helper, got {roles:?}"
            );
            assert!(
                roles.contains(&RoleId::Coder),
                "tier {tier:?}: defaults must include Coder, got {roles:?}"
            );
        }
    }

    /// Helper's priority must be Required so the spawner brings her
    /// up even when nothing else has requested her. If a refactor
    /// downgrades Helper to HighlyRecommended, the day-one experience
    /// silently breaks for users who don't issue an explicit need.
    #[test]
    fn helper_priority_is_required() {
        assert_eq!(helper_template().priority, SpawnPriority::Required);
    }

    /// Coder is HighlyRecommended — present by default but disable-able.
    /// Pins that the substrate spawns Coder unprompted on first run.
    #[test]
    fn coder_priority_is_highly_recommended() {
        assert_eq!(coder_template().priority, SpawnPriority::HighlyRecommended);
    }

    /// Helper @ desktop/laptop floor must downsize, not refuse. The
    /// `choose` fallback ensures we always have SOMETHING runnable —
    /// even when the tier-map doesn't have an exact entry, the lowest
    /// known choice serves as the safety floor.
    #[test]
    fn helper_model_choice_resolves_for_every_tier() {
        for tier in [
            HwCapabilityTier::CpuOnly,
            HwCapabilityTier::M1Uma8Gb,
            HwCapabilityTier::M5UmaProMax,
            HwCapabilityTier::Sm60,
            HwCapabilityTier::Sm120,
            // A tier the template doesn't explicitly cover — the
            // fallback must kick in.
            HwCapabilityTier::Cloud,
        ] {
            let h = helper_template();
            let choice = h.model_per_tier.choose(tier);
            assert!(
                choice.is_some(),
                "Helper has no model_choice for tier {tier:?} — even fallback failed"
            );
        }
    }

    /// Coder @ low tier must be code-capable — the whole point of the
    /// role. Pin the model family so a future swap is intentional, not
    /// accidental. Acceptable families: Qwen2.5-Coder, DeepSeek-Coder,
    /// StarCoder2. If the swap moves outside this set, the test
    /// catches it and someone has to justify the change.
    #[test]
    fn coder_low_tier_targets_swiss_army_code_family() {
        let c = coder_template();
        let choice = c
            .model_per_tier
            .choose(HwCapabilityTier::CpuOnly)
            .expect("Coder has no CpuOnly choice");
        let id_lower = choice.model_id.to_lowercase();
        assert!(
            id_lower.contains("coder")
                || id_lower.contains("starcoder")
                || id_lower.contains("deepseek"),
            "Coder@CpuOnly model {:?} doesn't look code-capable — \
             expected Qwen-Coder / DeepSeek-Coder / StarCoder",
            choice.model_id
        );
    }

    /// Helper's cognition defaults pin the clippy DNA: brief, friendly,
    /// asks before guessing. If a refactor accidentally turns Helper
    /// into a verbose researcher, this test catches it before naive
    /// users do.
    #[test]
    fn helper_cognition_defaults_are_brief_and_friendly() {
        let h = helper_template();
        assert!(
            h.cognition.depth_preference <= 30,
            "Helper depth_preference {} too high — should stay snappy (≤30)",
            h.cognition.depth_preference
        );
        assert!(
            h.cognition.max_response_chars <= 600,
            "Helper max_response_chars {} too long — clippy is brief (≤600)",
            h.cognition.max_response_chars
        );
        assert!(
            h.cognition.asks_before_guessing,
            "Helper must ask before guessing — clippy DNA"
        );
        assert_eq!(h.cognition.voice, "clippy");
    }

    /// Coder is willing to go deep + verbose when the question
    /// deserves it. Pin the contrasting profile so role differentiation
    /// stays meaningful.
    #[test]
    fn coder_cognition_defaults_allow_depth() {
        let c = coder_template();
        assert!(
            c.cognition.depth_preference >= 50,
            "Coder depth_preference {} too low — code work needs depth",
            c.cognition.depth_preference
        );
        assert!(
            c.cognition.max_response_chars >= 2000,
            "Coder max_response_chars {} too short — code answers can be long",
            c.cognition.max_response_chars
        );
    }

    /// The `choose` fallback is the SAFETY FLOOR — when a tier isn't
    /// explicitly mapped, the lowest known tier's choice must be
    /// returned. Pin this so future tier additions don't accidentally
    /// regress to "no model available."
    #[test]
    fn model_choice_per_tier_falls_back_to_first_entry() {
        let choice = ModelChoicePerTier {
            entries: vec![(
                HwCapabilityTier::CpuOnly,
                ModelChoice {
                    model_id: "floor".to_string(),
                    gguf_file: "x.gguf".to_string(),
                    gguf_size_mib: 100,
                    quant: "q4_k_m".to_string(),
                    base_model_id: None,
                },
            )],
        };
        // Tier not in the map — falls back to floor.
        let resolved = choice.choose(HwCapabilityTier::Sm120);
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().model_id, "floor");
    }

    /// RoleId stable-string mapping. Used in event headers + kanban
    /// metadata; renames must be intentional, not accidental.
    #[test]
    fn role_id_stable_strings() {
        assert_eq!(RoleId::Helper.as_str(), "helper");
        assert_eq!(RoleId::Coder.as_str(), "coder");
        assert_eq!(RoleId::Sentinel.as_str(), "sentinel");
        assert_eq!(RoleId::Custom.as_str(), "custom");
    }
}
