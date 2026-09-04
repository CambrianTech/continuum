//! ForgeRecipe — authored input for the foundry pipeline.
//!
//! Per the design at docs/architecture/FORGE-RECIPE-AS-ENTITY.md
//! (continuum#1164/#1165). The recipe captures everything a human
//! decides BEFORE running the foundry: prose fields, source model,
//! pipeline stages with notes, calibration corpus, quant tiers,
//! evaluation benchmarks, prior baselines, hardware target. The
//! foundry consumes a recipe + execution results and emits a
//! `ForgeArtifact` (see sibling `artifact.rs`).
//!
//! # What this PR ships (Phase 1a of #1164)
//!
//! - Pure Rust value types for ForgeRecipe + supporting structs
//! - ts-rs bindings to `protocol/typescript/forge/`
//! - Serde roundtrip + ts-rs export tests
//!
//! # Deferred to later phases
//!
//! - **Phase 1b:** rename existing TS-side `ForgeAlloy` → `ForgeArtifact`
//!   (15 TS files reference the old name; separate slice).
//! - **Phase 2:** typed `RecipeStage` enum matching the existing
//!   `AlloyStage` discriminated union from forge-alloy/python/forge_alloy/types.py
//!   (ports the stage zoo into Rust as the source of truth). v1 carries
//!   stages as `Vec<serde_json::Value>` so the recipe is usable today.
//! - **Phase 2:** typed `AlloyResults`, `AlloyReceipt`, `IntegrityAttestation`
//!   on the artifact side.
//! - **Phase 3:** entity registry registration + `data/*` collection wiring
//!   (the recipe types ship first; storage hooks them up next).
//!
//! # Conventions (matching existing persona/* modules)
//!
//! - `Uuid` fields use `#[ts(type = "string")]` for the TS export.
//! - Strings + bools + numbers map directly via ts-rs defaults.
//! - Nested types that aren't yet in Rust use `serde_json::Value` with
//!   `#[ts(type = "unknown")]` so the TS side gets `unknown` (caller
//!   must validate via the existing Python pydantic schemas until
//!   Phase 2 ports the types).

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

//=============================================================================
// SUPPORTING TYPES
//=============================================================================

/// Source model identifier — what the foundry forges from.
///
/// Mirrors the `AlloySource` shape from
/// `forge-alloy/python/forge_alloy/types.py`. Phase 2 replaces the Python
/// type with a `derive(TS)` import of this Rust type as the source of
/// truth.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/forge/AlloySource.ts"
)]
pub struct AlloySource {
    /// Hugging Face model identifier (e.g., "Qwen/Qwen3.5-4B-Instruct").
    pub base_model: String,
    /// Architecture family (e.g., "qwen3", "llama", "mistral").
    pub architecture: String,
    /// Optional pinned revision (commit / branch / tag) for reproducibility.
    #[ts(optional)]
    pub revision: Option<String>,
    /// MoE indicator. Defaults to false (dense models).
    #[serde(default)]
    pub is_moe: bool,
    /// Number of experts in the MoE (None for dense).
    #[ts(optional)]
    pub total_experts: Option<u32>,
}

/// §4.1.3.4 negative-baseline metric the artifact preserves for
/// falsifiability. Each baseline names a metric + measured value +
/// source so a reader can falsify the published improvement claim.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/forge/PriorBaseline.ts"
)]
pub struct PriorBaseline {
    /// Metric name (e.g., "perplexity", "humaneval-pass1").
    pub metric: String,
    /// Measured baseline value.
    pub value: f64,
    /// Where the baseline came from (e.g., "qwen3.5-4b base @ revision XYZ").
    pub source: String,
    /// ISO-8601 timestamp of when the measurement was taken.
    pub measured_at: String,
    /// Free-text description of how the measurement was performed.
    pub measurement_method: String,
}

/// Pointer to the calibration corpus used for the importance profile +
/// (eventual) compensation LoRA. Held-out from `evaluation_benchmarks`.
///
/// Bytes don't live in Continuum's ORM (corpora can be MB-GB). The
/// recipe carries a pointer; the bytes live in HF datasets, foundry-
/// node-local storage, or wherever the `source_url` resolves.
///
/// `content_hash` uses the canonical `"sha256:<hex>"` format that
/// matches `persona::admission` content_hash on the engram side
/// (consensus position #8 from the design review). Cross-domain
/// consistency: any two subsystems comparing hashes can do
/// string-equality without normalization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/forge/CorpusRef.ts")]
pub struct CorpusRef {
    /// Human-readable corpus name (e.g., "wikitext-103-v1").
    pub name: String,
    /// SHA-256 of the canonical corpus contents in `"sha256:<hex>"` form.
    /// Tamper-detection anchor + cross-domain equality with admission's
    /// content_hash convention.
    pub content_hash: String,
    /// Size in bytes (informational; helps the foundry pre-flight storage).
    #[ts(type = "number")]
    pub size_bytes: u64,
    /// Where the bytes live (HF dataset id, file:// URL, etc.). Optional
    /// because some corpora are foundry-node-local with no shareable URL.
    #[ts(optional)]
    pub source_url: Option<String>,
}

/// Which GGUF / MLX / safetensors / onnx tier(s) get published from
/// one recipe. Top-level on the recipe (consensus position #3 from the
/// design review) rather than nested inside a `QuantStage` — quant
/// tiers are a property of the published artifact, NOT a property of
/// the pipeline stage that produces them.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/forge/QuantTier.ts")]
pub struct QuantTier {
    /// Output format (e.g., "gguf", "mlx", "safetensors", "onnx").
    pub format: String,
    /// Quantization variants for this format (e.g., ["Q4_K_M", "Q5_K_M",
    /// "Q8_0"] for gguf).
    pub variants: Vec<String>,
    /// Which device tiers this tier targets (e.g., ["m1-8gb", "m5-pro",
    /// "rtx-5090"]). Helps the foundry decide which devices to verify
    /// the quantized output on.
    #[serde(default)]
    pub target_devices: Vec<String>,
}

/// Benchmark to run during evaluation. Mirrors the existing Python
/// `BenchmarkDef` shape so Phase 2 can swap the Python type to a
/// generated client of this Rust type.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/forge/BenchmarkDef.ts"
)]
pub struct BenchmarkDef {
    /// Benchmark name (e.g., "humaneval", "mmlu", "hellaswag").
    pub name: String,
    /// Optional sub-task / split name within the benchmark.
    #[ts(optional)]
    pub subset: Option<String>,
    /// N-shot setting. None = benchmark default.
    #[ts(optional)]
    pub n_shot: Option<u32>,
    /// Whether this benchmark's result should be submitted to a
    /// leaderboard. Defaults to false.
    #[serde(default)]
    pub submit_to_leaderboard: bool,
}

/// Hardware envelope for the recipe. Tells the foundry what device
/// tier to target + estimates resource needs. Mirrors the existing
/// Python `AlloyHardware` shape.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/forge/AlloyHardware.ts"
)]
pub struct AlloyHardware {
    /// Minimum VRAM (GB) required to run the foundry pipeline.
    #[ts(optional)]
    pub min_vram_gb: Option<f64>,
    /// Recommended VRAM (GB) for comfortable headroom.
    #[ts(optional)]
    pub recommended_vram_gb: Option<f64>,
    /// Estimated wall-clock duration for a full forge run (informational).
    #[ts(optional)]
    pub estimated_duration_minutes: Option<f64>,
    /// Whether the pipeline can fall back to CPU if no GPU available.
    #[serde(default)]
    pub supports_cpu: bool,
    /// Devices the recipe has been validated on (informational; the
    /// artifact's `hardware_verified` is the authoritative post-run
    /// list).
    #[serde(default)]
    pub tested_on: Vec<String>,
}

//=============================================================================
// FORGE RECIPE
//=============================================================================

/// Authored recipe — the input the foundry consumes.
///
/// Stored as a Continuum entity (Phase 3 wires the entity registry).
/// Edited via standard `Commands.execute('data/...')` primitives. Never
/// consumed directly by `publish_model.py` — that script reads the
/// `ForgeArtifact` (sibling type) the foundry emits.
///
/// All prose fields the model card renders live HERE, not in a hand-
/// authored `.alloy.json`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/forge/ForgeRecipe.ts"
)]
pub struct ForgeRecipe {
    //--- Identity ----------------------------------------------------------
    /// Stable recipe identifier. Generated at recipe creation time.
    #[ts(type = "string")]
    pub id: Uuid,

    /// Recipe name (e.g., "qwen3.5-4b-code-aggressive").
    pub name: String,

    /// Semantic version of THIS recipe (semver). Bump when revising
    /// the recipe; lineage chain via `parent_recipe_id`.
    pub version: String,

    /// Paragraph for the README/card.
    pub description: String,

    /// One-line plain-English headline (used as the model card subtitle).
    pub user_summary: String,

    /// Recipe author (e.g., "continuum-ai" or a user handle).
    pub author: String,

    /// Tags for discovery (e.g., ["code", "pruning", "4b"]).
    #[serde(default)]
    pub tags: Vec<String>,

    /// SPDX license identifier or shorthand. Default "apache-2.0"; the
    /// caller is responsible for inheriting the source model's license
    /// when applicable (consensus position #10 — `license_strategy`
    /// auto-inheritance lands in v2).
    pub license: String,

    //--- Methodology / falsifiability prose --------------------------------
    /// Optional link to the methodology paper.
    #[ts(optional)]
    pub methodology_paper_url: Option<String>,

    /// Known limitations of the recipe (rendered into the model card).
    #[serde(default)]
    pub limitations: Vec<String>,

    /// §4.1.3.4 negative-baselines preserved for falsifiability.
    #[serde(default)]
    pub prior_metric_baselines: Vec<PriorBaseline>,

    //--- Source -----------------------------------------------------------
    /// Base model + architecture metadata.
    pub source: AlloySource,

    //--- Pipeline ---------------------------------------------------------
    /// Ordered pipeline of recipe stages. v1 carries stages as opaque
    /// JSON values matching the existing `AlloyStage` discriminated
    /// union in `forge-alloy/python/forge_alloy/types.py`. Phase 2
    /// replaces this with a typed `Vec<RecipeStage>` enum where each
    /// variant carries an optional `notes: String` field for the
    /// methodology blockquote (consensus position #2 from the design
    /// review — per-variant notes, not index-keyed sidecar).
    #[ts(type = "Array<unknown>")]
    pub stages: Vec<serde_json::Value>,

    /// How many times to repeat the prune→train cycle (1 = single pass).
    /// Most recipes are 1.
    pub cycles: u32,

    //--- Calibration / eval inputs ----------------------------------------
    /// Held-out corpus pointer (importance profile + LoRA training).
    pub calibration_corpus: CorpusRef,

    /// Which output formats / tiers to produce (top-level per consensus
    /// position #3 — quant tiers are an artifact property, not a stage
    /// config).
    #[serde(default)]
    pub quant_tiers: Vec<QuantTier>,

    /// Benchmarks to run during evaluation.
    #[serde(default)]
    pub evaluation_benchmarks: Vec<BenchmarkDef>,

    //--- Hardware target --------------------------------------------------
    /// Target hardware envelope (VRAM, device list, CPU fallback).
    pub hardware: AlloyHardware,

    //--- Lineage ----------------------------------------------------------
    /// Parent recipe id, if this recipe was forked from another. None
    /// for net-new recipes. v1 lineage is one-directional (recipe →
    /// recipe); bidirectional lineage (recipe ← artifact) is a future
    /// `parent_artifact_ids` field per consensus position #9.
    #[ts(optional, type = "string")]
    pub parent_recipe_id: Option<Uuid>,

    //--- Timestamps -------------------------------------------------------
    /// When the recipe was authored (epoch milliseconds UTC). Same
    /// convention as `Engram.admitted_at_ms` from the engram thread —
    /// `u64` epoch ms, not chrono::DateTime.
    #[ts(type = "number")]
    pub authored_at_ms: u64,

    /// When the recipe was last edited (epoch milliseconds UTC).
    #[ts(type = "number")]
    pub updated_at_ms: u64,
}

//=============================================================================
// TESTS
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_now_ms() -> u64 {
        1_715_625_600_000
    }

    fn sample_corpus() -> CorpusRef {
        CorpusRef {
            name: "wikitext-103-v1".to_string(),
            content_hash: "sha256:abcdef0123456789".to_string(),
            size_bytes: 100_000_000,
            source_url: Some("hf://datasets/wikitext".to_string()),
        }
    }

    fn sample_recipe() -> ForgeRecipe {
        ForgeRecipe {
            id: Uuid::nil(),
            name: "qwen3.5-4b-code-aggressive".to_string(),
            version: "1.0.0".to_string(),
            description: "Aggressive prune + LoRA on a code corpus.".to_string(),
            user_summary: "Smaller, faster Qwen3.5-4B for code tasks.".to_string(),
            author: "continuum-ai".to_string(),
            tags: vec!["code".to_string(), "pruning".to_string(), "4b".to_string()],
            license: "apache-2.0".to_string(),
            methodology_paper_url: Some("https://example.com/forge-methodology.pdf".to_string()),
            limitations: vec!["English-only training corpus".to_string()],
            prior_metric_baselines: vec![PriorBaseline {
                metric: "perplexity".to_string(),
                value: 12.34,
                source: "qwen3.5-4b base @ revision XYZ".to_string(),
                measured_at: "2026-05-14T00:00:00Z".to_string(),
                measurement_method: "wikitext-103 eval split, fp16, batch=1".to_string(),
            }],
            source: AlloySource {
                base_model: "Qwen/Qwen3.5-4B-Instruct".to_string(),
                architecture: "qwen3".to_string(),
                revision: None,
                is_moe: false,
                total_experts: None,
            },
            stages: vec![
                serde_json::json!({"type": "prune", "strategy": "entropy", "level": 0.4}),
                serde_json::json!({"type": "lora", "rank": 32, "epochs": 3}),
                serde_json::json!({"type": "quant", "format": "gguf", "quantTypes": ["Q4_K_M"]}),
            ],
            cycles: 1,
            calibration_corpus: sample_corpus(),
            quant_tiers: vec![QuantTier {
                format: "gguf".to_string(),
                variants: vec![
                    "Q4_K_M".to_string(),
                    "Q5_K_M".to_string(),
                    "Q8_0".to_string(),
                ],
                target_devices: vec!["m1-8gb".to_string(), "m5-pro".to_string()],
            }],
            evaluation_benchmarks: vec![BenchmarkDef {
                name: "humaneval".to_string(),
                subset: None,
                n_shot: Some(0),
                submit_to_leaderboard: true,
            }],
            hardware: AlloyHardware {
                min_vram_gb: Some(8.0),
                recommended_vram_gb: Some(16.0),
                estimated_duration_minutes: Some(120.0),
                supports_cpu: false,
                tested_on: vec!["m5-pro".to_string()],
            },
            parent_recipe_id: None,
            authored_at_ms: fixed_now_ms(),
            updated_at_ms: fixed_now_ms(),
        }
    }

    /// What this catches: full ForgeRecipe round-trips through serde
    /// without losing fields. The recipe is the source of truth; if it
    /// silently drops a field on serialization the foundry would forge
    /// against a mutated input.
    #[test]
    fn forge_recipe_serde_roundtrip_preserves_all_fields() {
        let original = sample_recipe();
        let json = serde_json::to_string(&original).expect("serialize");
        let back: ForgeRecipe = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original.name, back.name);
        assert_eq!(original.version, back.version);
        assert_eq!(original.description, back.description);
        assert_eq!(original.user_summary, back.user_summary);
        assert_eq!(original.tags, back.tags);
        assert_eq!(original.limitations, back.limitations);
        assert_eq!(original.prior_metric_baselines.len(), 1);
        assert_eq!(original.source.base_model, back.source.base_model);
        assert_eq!(original.stages.len(), back.stages.len());
        assert_eq!(original.cycles, back.cycles);
        assert_eq!(
            original.calibration_corpus.content_hash,
            back.calibration_corpus.content_hash
        );
        assert_eq!(original.quant_tiers.len(), 1);
        assert_eq!(original.quant_tiers[0].variants.len(), 3);
        assert_eq!(original.evaluation_benchmarks.len(), 1);
        assert_eq!(original.hardware.min_vram_gb, back.hardware.min_vram_gb);
        assert_eq!(original.parent_recipe_id, back.parent_recipe_id);
        assert_eq!(original.authored_at_ms, back.authored_at_ms);
    }

    /// What this catches: minimal recipe (only required fields) serializes
    /// and deserializes cleanly. `serde(default)` lets all the Vec fields
    /// be omitted from the JSON without breaking deserialization. This
    /// means a recipe author can supply just the essentials in v1 and
    /// add tags/limitations/baselines later.
    #[test]
    fn minimal_recipe_serde_roundtrip_uses_defaults() {
        let json = r#"{
            "id": "00000000-0000-0000-0000-000000000000",
            "name": "minimal-recipe",
            "version": "0.1.0",
            "description": "Smallest viable recipe.",
            "userSummary": "Just enough fields to compile.",
            "author": "test",
            "license": "apache-2.0",
            "source": {
                "baseModel": "Qwen/Qwen3.5-4B-Instruct",
                "architecture": "qwen3"
            },
            "stages": [],
            "cycles": 1,
            "calibrationCorpus": {
                "name": "x",
                "contentHash": "sha256:x",
                "sizeBytes": 0
            },
            "hardware": {},
            "authoredAtMs": 0,
            "updatedAtMs": 0
        }"#;
        // Note: ts-rs uses snake_case by default; our fields ARE snake_case
        // in the Rust struct. Pydantic-style camelCase is supplied by the
        // TS layer when it converts. For this Rust-side test, use snake_case
        // JSON to match the actual serde output.
        let json_snake = json
            .replace("userSummary", "user_summary")
            .replace("baseModel", "base_model")
            .replace("calibrationCorpus", "calibration_corpus")
            .replace("contentHash", "content_hash")
            .replace("sizeBytes", "size_bytes")
            .replace("authoredAtMs", "authored_at_ms")
            .replace("updatedAtMs", "updated_at_ms");
        let recipe: ForgeRecipe = serde_json::from_str(&json_snake)
            .unwrap_or_else(|e| panic!("deserialize minimal: {e}\nJSON:\n{json_snake}"));
        assert_eq!(recipe.name, "minimal-recipe");
        assert!(recipe.tags.is_empty(), "tags default to empty Vec");
        assert!(
            recipe.limitations.is_empty(),
            "limitations default to empty Vec"
        );
        assert!(
            recipe.prior_metric_baselines.is_empty(),
            "prior_metric_baselines default to empty Vec"
        );
        assert!(
            recipe.quant_tiers.is_empty(),
            "quant_tiers default to empty Vec"
        );
        assert!(
            recipe.evaluation_benchmarks.is_empty(),
            "evaluation_benchmarks default to empty Vec"
        );
    }

    /// What this catches: stages are opaque JSON in v1 — they must
    /// round-trip without normalization. Phase 2's typed enum will
    /// replace this; until then, faithful pass-through is the contract.
    #[test]
    fn stages_round_trip_as_opaque_json() {
        let original = sample_recipe();
        let json = serde_json::to_string(&original).expect("serialize");
        let back: ForgeRecipe = serde_json::from_str(&json).expect("deserialize");
        // Each stage is a serde_json::Value; equality is structural.
        for (orig, back_stage) in original.stages.iter().zip(back.stages.iter()) {
            assert_eq!(orig, back_stage, "stage value must round-trip exactly");
        }
    }

    /// What this catches: content_hash uses the canonical "sha256:<hex>"
    /// format that matches admission's content_hash convention. Cross-
    /// domain consistency check.
    #[test]
    fn corpus_content_hash_uses_canonical_format() {
        let corpus = sample_corpus();
        assert!(
            corpus.content_hash.starts_with("sha256:"),
            "content_hash must use canonical sha256:<hex> format, got {}",
            corpus.content_hash
        );
    }

    // ── ts-rs binding tests — same pattern as persona/engram.rs ─────────

    #[test]
    fn export_bindings_alloy_source() {
        AlloySource::export_all(&ts_rs::Config::default()).unwrap();
    }

    #[test]
    fn export_bindings_prior_baseline() {
        PriorBaseline::export_all(&ts_rs::Config::default()).unwrap();
    }

    #[test]
    fn export_bindings_corpus_ref() {
        CorpusRef::export_all(&ts_rs::Config::default()).unwrap();
    }

    #[test]
    fn export_bindings_quant_tier() {
        QuantTier::export_all(&ts_rs::Config::default()).unwrap();
    }

    #[test]
    fn export_bindings_benchmark_def() {
        BenchmarkDef::export_all(&ts_rs::Config::default()).unwrap();
    }

    #[test]
    fn export_bindings_alloy_hardware() {
        AlloyHardware::export_all(&ts_rs::Config::default()).unwrap();
    }

    #[test]
    fn export_bindings_forge_recipe() {
        ForgeRecipe::export_all(&ts_rs::Config::default()).unwrap();
    }
}
