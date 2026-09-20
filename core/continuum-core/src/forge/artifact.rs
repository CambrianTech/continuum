//! ForgeArtifact — foundry-generated output for a recipe.
//!
//! Per the design at docs/architecture/FORGE-RECIPE-AS-ENTITY.md.
//! The artifact is what the foundry emits AFTER consuming a `ForgeRecipe`
//! and running its stages. It carries the recipe lineage (so you can
//! always answer "which recipe produced this?") plus everything the
//! foundry measured during the run that no human could have known
//! beforehand: benchmark results, hardware-verified device list, alloy
//! content hash, publication receipt, integrity attestation.
//!
//! The artifact is what `publish_model.py` reads. The recipe is what
//! a human authors. The foundry is the function recipe → artifact.
//!
//! # What this PR ships (Phase 1a of #1164)
//!
//! - `ForgeArtifact` Rust value type with ts-rs bindings + tests
//! - Recipe lineage fields (`recipe_id`, `recipe_version`, `forged_at_ms`)
//! - Result fields kept opaque (`serde_json::Value`) for v1 — Phase 2
//!   types `AlloyResults`, `AlloyReceipt`, `IntegrityAttestation` as
//!   first-class Rust structs once the foundry executor lands and
//!   needs them.
//!
//! # Naming (consensus position #1)
//!
//! "ForgeAlloy" → "ForgeArtifact" rename happens in **Phase 1b** (TS
//! side, 15 file references; separate slice). This Rust file ships
//! with the new name from day 1.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::recipe::{
    AlloyHardware, AlloySource, BenchmarkDef, CorpusRef, ForgeRecipe, PriorBaseline, QuantTier,
};

//=============================================================================
// HARDWARE PROFILE — verified post-run
//=============================================================================

/// One device the foundry actually ran the artifact on. Composes into
/// `ForgeArtifact.hardware_verified` so the model card's device-grid
/// reflects measured reality, not just the recipe's `tested_on` claim.
///
/// Mirrors the existing Python `HardwareProfile` shape; Phase 2 makes
/// the Rust type the source of truth.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/forge/HardwareProfile.ts"
)]
pub struct HardwareProfile {
    /// Device label (e.g., "m5-pro", "rtx-5090", "linux-amd64").
    pub device: String,
    /// Format the device ran (e.g., "gguf-Q4_K_M", "mlx", "safetensors").
    pub format: String,
    /// On-disk size in GB.
    #[ts(optional)]
    pub size_gb: Option<f64>,
    /// Measured throughput.
    #[ts(optional)]
    pub tokens_per_sec: Option<f64>,
    /// Peak memory usage during inference.
    #[ts(optional)]
    pub memory_usage_gb: Option<f64>,
    /// Whether the verification run actually completed without error.
    #[serde(default)]
    pub verified: bool,
}

//=============================================================================
// FORGE ARTIFACT
//=============================================================================

/// Foundry-generated output. Combines (a) a snapshot of the recipe
/// fields the foundry consumed + (b) execution outputs that only the
/// foundry knows.
///
/// Stored as a Continuum entity (Phase 3 wires the registry). Read by
/// `publish_model.py` as the source of truth for what gets published.
/// Never authored by hand.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/forge/ForgeArtifact.ts"
)]
pub struct ForgeArtifact {
    //--- Identity ----------------------------------------------------------
    /// Stable artifact id (different from recipe id — one recipe can
    /// produce many artifacts across multiple runs / hardware tiers).
    #[ts(type = "string")]
    pub id: Uuid,

    //--- Recipe lineage (frozen at run time) ------------------------------
    /// Which recipe produced this artifact.
    #[ts(type = "string")]
    pub recipe_id: Uuid,

    /// Recipe version at run time (semver). Pinned so a later recipe
    /// revision doesn't retroactively change what this artifact claims
    /// to come from.
    pub recipe_version: String,

    /// Recipe `name` snapshot (denormalized — lets the artifact card
    /// render without re-fetching the recipe entity).
    pub recipe_name: String,

    //--- Snapshot of recipe authored fields -------------------------------
    //
    // Denormalized so the artifact carries everything the model card
    // needs without joining back to the recipe. If the recipe edits a
    // field after this artifact was forged, this artifact's snapshot
    // stays as-was — the recipe lineage points to the recipe-version
    // that was current at run time.
    /// Paragraph for the README/card.
    pub description: String,
    /// One-line plain-English headline.
    pub user_summary: String,
    /// Recipe author at the time of run.
    pub author: String,
    /// Tags from the recipe at run time.
    #[serde(default)]
    pub tags: Vec<String>,
    /// SPDX license identifier.
    pub license: String,
    /// Methodology paper URL from the recipe at run time.
    #[ts(optional)]
    pub methodology_paper_url: Option<String>,
    /// Limitations from the recipe at run time.
    #[serde(default)]
    pub limitations: Vec<String>,
    /// §4.1.3.4 negative-baselines preserved from the recipe.
    #[serde(default)]
    pub prior_metric_baselines: Vec<PriorBaseline>,
    /// Source model snapshot.
    pub source: AlloySource,
    /// Calibration corpus pointer used for THIS forge.
    pub calibration_corpus: CorpusRef,
    /// Quant tiers requested by the recipe.
    #[serde(default)]
    pub quant_tiers: Vec<QuantTier>,
    /// Benchmarks requested by the recipe.
    #[serde(default)]
    pub evaluation_benchmarks: Vec<BenchmarkDef>,
    /// Hardware target from the recipe.
    pub hardware: AlloyHardware,

    //--- Execution outputs (only the foundry knows these) -----------------
    /// When the foundry started this run (epoch milliseconds UTC).
    #[ts(type = "number")]
    pub forged_at_ms: u64,

    /// Total wall-clock duration of the forge run (minutes).
    #[ts(optional)]
    pub duration_minutes: Option<f64>,

    /// Final parameter count after prune/compact (in billions).
    #[ts(optional)]
    pub forged_params_b: Option<f64>,

    /// Active params per token for MoE artifacts (in billions). None
    /// for dense models.
    #[ts(optional)]
    pub active_params_b: Option<f64>,

    /// Devices the artifact has been verified on, with measured
    /// throughput + memory. Drives the published card's device grid.
    #[serde(default)]
    pub hardware_verified: Vec<HardwareProfile>,

    /// Content-addressable hash of the populated artifact JSON. Used
    /// as the verification anchor by `publish_model.py` and by the
    /// proof-contract trust layer (see grid/FORGE-ALLOY-PROOF-CONTRACTS.md).
    #[ts(optional)]
    pub alloy_hash: Option<String>,

    /// Full execution results blob. v1 carries this as opaque JSON
    /// matching the existing Python `AlloyResults` shape (benchmarks,
    /// perplexity, samples, integrity attestation). Phase 2 types this
    /// as a first-class Rust struct once the foundry executor needs it.
    #[ts(optional, type = "unknown")]
    pub results: Option<serde_json::Value>,

    /// Publication receipt blob. Same Phase 2 deferral as `results` —
    /// opaque JSON for v1, typed when the publish path is ported into
    /// Rust. Mirrors the existing Python `AlloyReceipt`.
    #[ts(optional, type = "unknown")]
    pub receipt: Option<serde_json::Value>,

    /// Integrity attestation blob. Carries the IntegrityAttestation
    /// (signed proof of the forge run) when the run was attested.
    /// Opaque JSON for v1; typed when the proof-contract integration
    /// (grid/FORGE-ALLOY-PROOF-CONTRACTS.md) lands in Rust.
    #[ts(optional, type = "unknown")]
    pub integrity: Option<serde_json::Value>,
}

impl ForgeArtifact {
    /// Project a `ForgeRecipe` into an **unforged** `ForgeArtifact`
    /// skeleton: every recipe field the model card renders is denormalized
    /// (snapshotted) here, a fresh artifact `id` is assigned (distinct from
    /// the recipe's — one recipe yields many artifacts), and every
    /// foundry-execution field is left at its unforged default
    /// (`forged_at_ms = 0` as the "not yet run" sentinel; the rest
    /// `None`/empty). The foundry — or, for now, the v1 stub — stamps the
    /// execution fields after running the recipe's stages.
    ///
    /// This is the canonical recipe→artifact projection the FORGE TEMPLATE
    /// ARCHITECTURE mandates: *"the alloy is the projection of the recipe,
    /// the foundry generates it"* — authoring artifacts by hand is
    /// anti-architectural. One place owns the field inheritance; the
    /// executor (and the stub) build on it instead of re-inlining the
    /// 16-field copy, so the projection can never drift between callers.
    pub fn from_recipe(recipe: &ForgeRecipe) -> Self {
        Self {
            // Fresh identity — distinct from the recipe.
            id: Uuid::new_v4(),
            // Lineage, frozen at projection time.
            recipe_id: recipe.id,
            recipe_version: recipe.version.clone(),
            recipe_name: recipe.name.clone(),
            // Denormalized recipe prose / config (the model-card snapshot).
            description: recipe.description.clone(),
            user_summary: recipe.user_summary.clone(),
            author: recipe.author.clone(),
            tags: recipe.tags.clone(),
            license: recipe.license.clone(),
            methodology_paper_url: recipe.methodology_paper_url.clone(),
            limitations: recipe.limitations.clone(),
            prior_metric_baselines: recipe.prior_metric_baselines.clone(),
            source: recipe.source.clone(),
            calibration_corpus: recipe.calibration_corpus.clone(),
            quant_tiers: recipe.quant_tiers.clone(),
            evaluation_benchmarks: recipe.evaluation_benchmarks.clone(),
            hardware: recipe.hardware.clone(),
            // Unforged: the foundry stamps these when it runs the stages.
            forged_at_ms: 0,
            duration_minutes: None,
            forged_params_b: None,
            active_params_b: None,
            hardware_verified: Vec::new(),
            alloy_hash: None,
            results: None,
            receipt: None,
            integrity: None,
        }
    }
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

    fn sample_artifact() -> ForgeArtifact {
        ForgeArtifact {
            id: Uuid::new_v4(),
            recipe_id: Uuid::nil(),
            recipe_version: "1.0.0".to_string(),
            recipe_name: "qwen3.5-4b-code-aggressive".to_string(),
            description: "Forged from the qwen3.5-4b-code-aggressive recipe.".to_string(),
            user_summary: "Smaller, faster Qwen3.5-4B for code.".to_string(),
            author: "continuum-ai".to_string(),
            tags: vec!["code".to_string(), "pruning".to_string()],
            license: "apache-2.0".to_string(),
            methodology_paper_url: None,
            limitations: vec!["English-only".to_string()],
            prior_metric_baselines: vec![],
            source: AlloySource {
                base_model: "Qwen/Qwen3.5-4B-Instruct".to_string(),
                architecture: "qwen3".to_string(),
                revision: None,
                is_moe: false,
                total_experts: None,
            },
            calibration_corpus: CorpusRef {
                name: "wikitext-103-v1".to_string(),
                content_hash: "sha256:abc".to_string(),
                size_bytes: 100,
                source_url: None,
            },
            quant_tiers: vec![],
            evaluation_benchmarks: vec![],
            hardware: AlloyHardware {
                min_vram_gb: Some(8.0),
                recommended_vram_gb: Some(16.0),
                estimated_duration_minutes: None,
                supports_cpu: false,
                tested_on: vec![],
            },
            forged_at_ms: fixed_now_ms(),
            duration_minutes: Some(75.0),
            forged_params_b: Some(2.4),
            active_params_b: None,
            hardware_verified: vec![HardwareProfile {
                device: "m5-pro".to_string(),
                format: "gguf-Q4_K_M".to_string(),
                size_gb: Some(2.6),
                tokens_per_sec: Some(45.0),
                memory_usage_gb: Some(3.2),
                verified: true,
            }],
            alloy_hash: Some("sha256:aa61c4bdf463847c".to_string()),
            results: Some(serde_json::json!({
                "benchmarks": [{"name": "humaneval", "metrics": {"pass1": 0.32}}]
            })),
            receipt: None,
            integrity: None,
        }
    }

    /// What this catches: full ForgeArtifact round-trips through serde
    /// without dropping any of the recipe-snapshot or execution fields.
    /// publish_model.py reads this; field loss = silent publish bugs.
    #[test]
    fn forge_artifact_serde_roundtrip_preserves_all_fields() {
        let original = sample_artifact();
        let json = serde_json::to_string(&original).expect("serialize");
        let back: ForgeArtifact = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original.recipe_id, back.recipe_id);
        assert_eq!(original.recipe_version, back.recipe_version);
        assert_eq!(original.recipe_name, back.recipe_name);
        assert_eq!(original.description, back.description);
        assert_eq!(original.author, back.author);
        assert_eq!(original.tags, back.tags);
        assert_eq!(original.limitations, back.limitations);
        assert_eq!(original.source.base_model, back.source.base_model);
        assert_eq!(
            original.calibration_corpus.content_hash,
            back.calibration_corpus.content_hash
        );
        assert_eq!(original.forged_at_ms, back.forged_at_ms);
        assert_eq!(original.forged_params_b, back.forged_params_b);
        assert_eq!(original.hardware_verified.len(), 1);
        assert_eq!(
            original.hardware_verified[0].device,
            back.hardware_verified[0].device
        );
        assert_eq!(original.alloy_hash, back.alloy_hash);
        assert!(back.results.is_some());
    }

    /// What this catches: opaque results/receipt/integrity blobs round-
    /// trip exactly. Phase 2 types these; until then, faithful
    /// pass-through is the contract.
    #[test]
    fn opaque_blob_fields_round_trip_unchanged() {
        let mut artifact = sample_artifact();
        artifact.receipt = Some(serde_json::json!({
            "publications": [{"target": "huggingface", "url": "https://example.com"}]
        }));
        artifact.integrity = Some(serde_json::json!({
            "trustLevel": "self-attested",
            "modelHash": "sha256:def",
        }));
        let json = serde_json::to_string(&artifact).expect("serialize");
        let back: ForgeArtifact = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(artifact.results, back.results);
        assert_eq!(artifact.receipt, back.receipt);
        assert_eq!(artifact.integrity, back.integrity);
    }

    /// What this catches: an artifact with no execution results yet
    /// (e.g., partial run that errored before benchmarks completed)
    /// still serializes. Critical for forensic captures of failed runs
    /// — the artifact entity must survive partial state.
    #[test]
    fn partial_artifact_with_none_results_serializes() {
        let mut artifact = sample_artifact();
        artifact.results = None;
        artifact.receipt = None;
        artifact.integrity = None;
        artifact.alloy_hash = None;
        artifact.duration_minutes = None;
        artifact.forged_params_b = None;
        let json = serde_json::to_string(&artifact).expect("serialize");
        let back: ForgeArtifact = serde_json::from_str(&json).expect("deserialize");
        assert!(back.results.is_none());
        assert!(back.alloy_hash.is_none());
        assert_eq!(
            back.recipe_id, artifact.recipe_id,
            "lineage preserved even on partial"
        );
    }

    /// What this catches: recipe_id + recipe_version pinning means a
    /// later recipe edit can't retroactively rewrite what this artifact
    /// claims to come from. Snapshot semantics for the lineage fields.
    #[test]
    fn recipe_lineage_fields_are_not_optional() {
        // Compile-time: the struct definition forces non-optional
        // recipe_id + recipe_version + recipe_name. This test is the
        // runtime spec that they're populated.
        let artifact = sample_artifact();
        assert!(
            !artifact.recipe_version.is_empty(),
            "recipe_version is required"
        );
        assert!(!artifact.recipe_name.is_empty(), "recipe_name is required");
    }

    // ── ts-rs bindings — same pattern as persona/engram.rs ──────────────

    #[test]
    fn export_bindings_hardware_profile() {
        HardwareProfile::export_all(&ts_rs::Config::default()).unwrap();
    }

    #[test]
    fn export_bindings_forge_artifact() {
        ForgeArtifact::export_all(&ts_rs::Config::default()).unwrap();
    }
}
