//! ForgeModule — IPC commands for the foundry pipeline.
//!
//! Phase 4 of continuum#1164 (design at FORGE-RECIPE-AS-ENTITY.md).
//! v1 is a stub: `forge/run` accepts a `ForgeRecipe` payload and
//! returns a synthetic `ForgeArtifact` populated with placeholder
//! execution outputs. Real stage execution (prune / train / lora /
//! quant / eval) lands in Phase 5+ when the foundry executor is
//! ported into Rust.
//!
//! Commands:
//! - `forge/run`: Take a ForgeRecipe + hardware node label, return a
//!   stub ForgeArtifact with `recipe_id` lineage + `forged_at_ms`
//!   timestamp + an `alloy_hash` derived from the recipe's content
//!   hash. Caller persists the artifact via `data/upsert` against
//!   the `forge_artifacts` collection (Phase 3 #1180 wired the entity
//!   registration).
//!
//! Stub semantics for Phase 4:
//! - No models are loaded.
//! - No stages execute.
//! - No HuggingFace publishing.
//! - The artifact's `results` / `receipt` / `integrity` fields stay
//!   `None`. `hardware_verified` is empty.
//! - `alloy_hash` is `"sha256:stub-<recipe_id_short>"` so the
//!   placeholder is identifiable but doesn't collide with real hashes.
//!
//! This proves the IPC reachability + recipe→artifact transformation
//! shape end-to-end without claiming to forge anything. Phase 5
//! replaces the stub with the real executor.

use crate::forge::{ForgeArtifact, ForgeRecipe};
use crate::inference::unsloth_forge::{
    to_body, ExportGgufRequest, ExportLoraRequest, ForgeCustodian, ForgeTrainRequest,
    LoadCheckpointRequest, UnslothForgeHttp,
};
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::any::Any;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct ForgeModule;

impl ForgeModule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ForgeModule {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct ForgeRunParams {
    recipe: ForgeRecipe,
    /// Hardware node label (e.g., "m5-pro@local", "rtx-5090@bigmama").
    /// Stub records this in the artifact's hardware_verified for trace
    /// purposes; Phase 5+ will actually dispatch to the named node.
    #[serde(default)]
    hardware_node: Option<String>,
}

#[async_trait]
impl ServiceModule for ForgeModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "forge",
            priority: ModulePriority::Normal,
            command_prefixes: &["forge/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        match command {
            "forge/run" => {
                let parsed: ForgeRunParams = serde_json::from_value(params)
                    .map_err(|e| format!("forge/run: invalid params: {e}"))?;

                let artifact =
                    synthesize_stub_artifact(&parsed.recipe, parsed.hardware_node.as_deref())?;
                let json = serde_json::to_value(&artifact)
                    .map_err(|e| format!("forge/run: serialize artifact: {e}"))?;
                Ok(CommandResult::Json(json))
            }
            "forge/train" => {
                let parsed: ForgeTrainParams = serde_json::from_value(params)
                    .map_err(|e| format!("forge/train: invalid params: {e}"))?;
                run_train(&UnslothForgeHttp::from_config(), parsed).await
            }
            "forge/train-status" => {
                let status = UnslothForgeHttp::from_config()
                    .train_status()
                    .await
                    .map_err(|e| e.to_string())?;
                let json = serde_json::to_value(status)
                    .map_err(|e| format!("forge/train-status: serialize: {e}"))?;
                Ok(CommandResult::Json(json))
            }
            "forge/export" => {
                let parsed: ForgeExportParams = serde_json::from_value(params)
                    .map_err(|e| format!("forge/export: invalid params: {e}"))?;
                run_export(&UnslothForgeHttp::from_config(), parsed).await
            }
            "forge/decide" => {
                let parsed: DecideParams = serde_json::from_value(params)
                    .map_err(|e| format!("forge/decide: invalid params: {e}"))?;
                Ok(CommandResult::Json(decide_assemble_or_train(&parsed)))
            }
            other => Err(format!("Unknown forge command: {other}")),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Synthesize a stub `ForgeArtifact` from a recipe. Phase 4 placeholder
/// — real foundry execution lands in Phase 5+. Caller persists the
/// returned artifact via `data/upsert` against `forge_artifacts`.
fn synthesize_stub_artifact(
    recipe: &ForgeRecipe,
    hardware_node: Option<&str>,
) -> Result<ForgeArtifact, String> {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("system time before epoch: {e}"))?
        .as_millis() as u64;

    // Derive an identifiable stub hash from the recipe id (first 16 hex
    // chars). Real Phase 5 hash will be sha256 of the populated alloy
    // content. Stub format prefix avoids collision with real hashes.
    let stub_hash = format!(
        "sha256:stub-{}",
        recipe
            .id
            .simple()
            .to_string()
            .chars()
            .take(16)
            .collect::<String>()
    );

    // Start from the canonical recipe→artifact projection (all inherited
    // fields, fresh id, unforged execution fields), then stamp the few
    // execution facts the v1 stub can supply: the run-start time, a
    // recognizable stub hash, and the requested (unverified) hardware node.
    // Phase 5+ replaces this with real stage execution filling the rest.
    let mut artifact = ForgeArtifact::from_recipe(recipe);
    artifact.forged_at_ms = now_ms;
    artifact.alloy_hash = Some(stub_hash);
    artifact.hardware_verified = hardware_node
        .map(|node| {
            vec![crate::forge::HardwareProfile {
                device: node.to_string(),
                format: "stub".to_string(),
                size_gb: None,
                tokens_per_sec: None,
                memory_usage_gb: None,
                verified: false,
            }]
        })
        .unwrap_or_default();
    Ok(artifact)
}

//=============================================================================
// forge/train + forge/export — DELEGATED to the model CUSTODIAN (#32).
//
// The foundry's byte work — engine selection (MLX on Apple Silicon / Unsloth-CUDA
// on NVIDIA), the train/valid split, fuse → convert → quantize, and the output
// BYTES — belongs to the custodian (unsloth studio), not the organism. continuum
// holds POLICY (`forge/decide`) and HANDLES (a job id, a save dir); it never
// spawns `mlx_lm.lora`/`mlx_lm.fuse`/llama.cpp nor writes under
// `~/.continuum/forge`. We delegate over the custodian's HTTP forge surface
// (`inference::unsloth_forge`), exactly as model load/unload delegates via
// `inference::unsloth_control`. This DELETES the ~675 lines of subprocess +
// byte-write trespass that were the GGUF-gibberish + adapter no-op bug class.
// Closes the coordination↔learning flywheel: a persona's room work becomes a
// dataset (`dataset/from-turns`) → custodian fine-tunes a LoRA genome → it pages
// into the genome. See memory model-endpoint-fabric-adapter-router,
// unsloth-universal-model-gateway, compute-lease-boundary,
// coordination-learning-flywheel.
//=============================================================================

/// Inputs for `forge/train`. The dataset is the JSONL `dataset/from-turns`
/// emits; the base model + LoRA geometry come from a `ForgeRecipe` (or directly).
/// `dry_run` resolves the custodian request body and returns it WITHOUT kicking a
/// run — the wiring check, replacing the old engine `--dry-run` (the custodian
/// picks the engine, materializes the split, and owns the output bytes).
#[derive(Debug, Deserialize)]
struct ForgeTrainParams {
    /// Local training dataset path (the `dataset/from-turns` JSONL). The
    /// custodian reads it and owns the train/valid split.
    dataset_path: String,
    /// Base model to fine-tune (the same id the gateway serves).
    base_model: String,
    /// Dataset format the custodian parses (from-turns emits chat `{messages}`).
    #[serde(default = "default_format_type")]
    format_type: String,
    /// Training type — "lora" (the genome layer) by default.
    #[serde(default = "default_training_type")]
    training_type: String,
    #[serde(default = "default_lora_r")]
    lora_r: u32,
    #[serde(default = "default_lora_alpha")]
    lora_alpha: u32,
    #[serde(default = "default_num_epochs")]
    num_epochs: u32,
    /// The custodian takes the learning rate as a STRING (e.g. "2e-4").
    #[serde(default = "default_learning_rate")]
    learning_rate: String,
    #[serde(default = "default_batch_size")]
    batch_size: u32,
    #[serde(default = "default_grad_accum")]
    gradient_accumulation_steps: u32,
    #[serde(default = "default_max_seq_length")]
    max_seq_length: u32,
    #[serde(default = "default_load_in_4bit")]
    load_in_4bit: bool,
    /// Resolve the custodian request + return it WITHOUT kicking a run.
    #[serde(default)]
    dry_run: bool,
}

fn default_format_type() -> String {
    "chat".to_string()
}
fn default_training_type() -> String {
    "lora".to_string()
}
fn default_lora_r() -> u32 {
    16
}
fn default_lora_alpha() -> u32 {
    16
}
fn default_num_epochs() -> u32 {
    1
}
fn default_learning_rate() -> String {
    "2e-4".to_string()
}
fn default_batch_size() -> u32 {
    1
}
fn default_grad_accum() -> u32 {
    1
}
fn default_max_seq_length() -> u32 {
    2048
}
fn default_load_in_4bit() -> bool {
    true
}

/// Build the custodian train request from the params. Pure — unit-testable
/// without the custodian, so the body continuum POSTs is pinned independently of
/// a real run (the genome knobs ride EXPLICITLY; `use_lora` follows the type).
fn build_train_request(p: &ForgeTrainParams) -> ForgeTrainRequest {
    ForgeTrainRequest {
        model_name: p.base_model.clone(),
        training_type: p.training_type.clone(),
        format_type: p.format_type.clone(),
        local_datasets: vec![p.dataset_path.clone()],
        num_epochs: p.num_epochs,
        learning_rate: p.learning_rate.clone(),
        batch_size: p.batch_size,
        gradient_accumulation_steps: p.gradient_accumulation_steps,
        max_seq_length: p.max_seq_length,
        load_in_4bit: p.load_in_4bit,
        use_lora: p.training_type == "lora",
        lora_r: p.lora_r,
        lora_alpha: p.lora_alpha,
        lora_dropout: 0.0,
    }
}

/// `forge/train` — delegate the run to the custodian. `dry_run` returns the
/// resolved request body (the wiring check); otherwise the custodian kicks the
/// run off (fire-and-poll — the custodian owns the long-running training; poll
/// `forge/train-status`). Fail-loud on an unreachable/erroring custodian.
async fn run_train(
    custodian: &dyn ForgeCustodian,
    p: ForgeTrainParams,
) -> Result<CommandResult, String> {
    let req = build_train_request(&p);
    if p.dry_run {
        return Ok(CommandResult::Json(json!({
            "dry_run": true,
            "request": to_body(&req),
        })));
    }
    let handle = custodian.train_start(&req).await.map_err(|e| e.to_string())?;
    let status = custodian.train_status().await.map_err(|e| e.to_string())?;
    Ok(CommandResult::Json(json!({
        "dry_run": false,
        "job_id": handle.job_id,
        "message": handle.message,
        "status": serde_json::to_value(status).map_err(|e| e.to_string())?,
    })))
}

//=============================================================================
// forge/export — the package stage, DELEGATED to the custodian.
//
// Turns a trained checkpoint into the PAGEABLE genome layer — the unit the grid
// exchanges. The custodian loads the checkpoint into its exporter, then packages
// it (`lora` = the adapter as-is; `gguf` = fuse → convert → quantize, which the
// custodian owns — this is why continuum must NOT hand-run convert_lora_to_gguf).
// The grid card (content-addressed hash + catalog emit) is the next slice.
// See memory lora-layers-as-p2p-exchanged-genome.
//=============================================================================

/// Inputs for `forge/export`. `checkpoint` is the custodian-owned trained run
/// dir (a `forge/train` output under `~/.unsloth/studio/outputs`);
/// `save_directory` is where the custodian writes the packaged artifact
/// (custodian-owned). Both stay custodian-side — the organism passes handles.
#[derive(Debug, Deserialize)]
struct ForgeExportParams {
    /// The trained checkpoint directory (loaded into the custodian's exporter).
    checkpoint: String,
    /// Where the custodian writes the export (custodian-owned path).
    save_directory: String,
    /// Export format: "lora" (the pageable genome layer — default) or "gguf".
    #[serde(default = "default_export_format")]
    format: String,
    /// GGUF quantization (only when `format == "gguf"`): Q4_K_M, Q5_K_M, Q8_0, F16.
    #[serde(default = "default_quantization")]
    quantization: String,
    #[serde(default = "default_max_seq_length")]
    max_seq_length: u32,
    #[serde(default = "default_load_in_4bit")]
    load_in_4bit: bool,
}

fn default_export_format() -> String {
    "lora".to_string()
}
fn default_quantization() -> String {
    "Q4_K_M".to_string()
}

/// `forge/export` — delegate packaging to the custodian. Loads the checkpoint
/// into the custodian's exporter, then packages it as a LoRA or a quantized GGUF.
/// Fail-loud on any custodian error or a non-success envelope.
async fn run_export(
    custodian: &dyn ForgeCustodian,
    p: ForgeExportParams,
) -> Result<CommandResult, String> {
    // The export endpoints operate on the LOADED checkpoint, so load it first.
    let loaded = custodian
        .load_checkpoint(&LoadCheckpointRequest {
            checkpoint_path: p.checkpoint.clone(),
            max_seq_length: p.max_seq_length,
            load_in_4bit: p.load_in_4bit,
        })
        .await
        .map_err(|e| e.to_string())?;
    if !loaded.success {
        return Err(format!(
            "custodian load-checkpoint failed for {}: {}",
            p.checkpoint, loaded.message
        ));
    }

    let result = match p.format.as_str() {
        "lora" => {
            custodian
                .export_lora(&ExportLoraRequest {
                    save_directory: p.save_directory.clone(),
                    push_to_hub: false,
                    repo_id: None,
                    base_model_id: None,
                })
                .await
        }
        "gguf" => {
            custodian
                .export_gguf(&ExportGgufRequest {
                    save_directory: p.save_directory.clone(),
                    quantization_method: p.quantization.clone(),
                    push_to_hub: false,
                    repo_id: None,
                })
                .await
        }
        other => {
            return Err(format!(
                "unsupported export format: {other} (lora|gguf) — the custodian packages these"
            ))
        }
    }
    .map_err(|e| e.to_string())?;

    if !result.success {
        return Err(format!("custodian export ({}) failed: {}", p.format, result.message));
    }
    Ok(CommandResult::Json(json!({
        "format": p.format,
        "checkpoint": p.checkpoint,
        "save_directory": p.save_directory,
        "message": result.message,
        "details": result.details,
    })))
}

//=============================================================================
// forge/decide — "assemble the best self, or train" (the product-thesis decision)
//
// The request has been scored against the trust-scoped market: each candidate
// (an existing model / LoRA genome) has a measured `score` from `vdd/score`, and
// the current self has a `baseline`. THIS decides, deterministically: adopt the
// best candidate that clears the adopt margin (assemble — zero training), else
// the market came up short → TRAIN a fresh layer to fill the gap. Pure — the
// candidates' scores come from elsewhere; this is the decision the whole
// "ask anything → assemble best self, or train" loop turns on.
// See docs/genome/SELF-EVOLVING-GENOME.md + memory ask-anything-assemble-best-self-or-train.
//=============================================================================

/// One scored candidate from the (trust-scoped) market search: a model or LoRA
/// genome, with its measured eval `score` (from `vdd/score`).
#[derive(Debug, Clone, Deserialize)]
struct Candidate {
    label: String,
    /// Held-out eval score ∈ [0,1] (the `vdd/score` of assembling this candidate).
    score: f64,
}

#[derive(Debug, Deserialize)]
struct DecideParams {
    /// The current self's score on the same held-out set (base / status quo).
    baseline: f64,
    /// Scored candidates from the market (may be empty → nothing to assemble).
    #[serde(default)]
    candidates: Vec<Candidate>,
    /// Minimum lift over baseline to bother ADOPTING a candidate rather than
    /// training. Below this, the market hasn't earned the swap → train.
    #[serde(default = "default_adopt_margin")]
    adopt_margin: f64,
}

fn default_adopt_margin() -> f64 {
    0.02
}

/// Pure decision: assemble the best-scoring candidate if it clears
/// `baseline + adopt_margin`; otherwise train. Returns the decision + the lift
/// the winning (or best-available) candidate would provide.
fn decide_assemble_or_train(p: &DecideParams) -> Value {
    // Best available candidate by measured capability (score). Cost-weighted
    // value-density is the eviction/composition decision, not this one.
    let best = p
        .candidates
        .iter()
        .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal));

    match best {
        Some(c) if c.score - p.baseline >= p.adopt_margin => serde_json::json!({
            "action": "assemble",
            "chosen": c.label,
            "score": c.score,
            "baseline": p.baseline,
            "lift": c.score - p.baseline,
            "reason": "market candidate clears the adopt margin — assemble (zero training)",
        }),
        Some(c) => serde_json::json!({
            "action": "train",
            "best_candidate": c.label,
            "best_score": c.score,
            "baseline": p.baseline,
            "best_lift": c.score - p.baseline,
            "gap": p.adopt_margin - (c.score - p.baseline),
            "reason": "no market candidate clears the adopt margin — forage + train to fill the gap",
        }),
        None => serde_json::json!({
            "action": "train",
            "baseline": p.baseline,
            "reason": "market returned no candidates — train from the best available base",
        }),
    }
}

//=============================================================================
// TESTS
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::{AlloyHardware, AlloySource, CorpusRef};
    use uuid::Uuid;

    fn synthetic_recipe() -> ForgeRecipe {
        ForgeRecipe {
            id: Uuid::new_v4(),
            name: "test-recipe".to_string(),
            version: "0.1.0".to_string(),
            description: "test".to_string(),
            user_summary: "test summary".to_string(),
            author: "test".to_string(),
            tags: vec!["test".to_string()],
            license: "apache-2.0".to_string(),
            methodology_paper_url: None,
            limitations: vec![],
            prior_metric_baselines: vec![],
            source: AlloySource {
                base_model: "test-model".to_string(),
                architecture: "test-arch".to_string(),
                revision: None,
                is_moe: false,
                total_experts: None,
            },
            stages: vec![],
            cycles: 1,
            calibration_corpus: CorpusRef {
                name: "test-corpus".to_string(),
                content_hash: "sha256:test".to_string(),
                size_bytes: 0,
                source_url: None,
            },
            quant_tiers: vec![],
            evaluation_benchmarks: vec![],
            hardware: AlloyHardware {
                min_vram_gb: None,
                recommended_vram_gb: None,
                estimated_duration_minutes: None,
                supports_cpu: false,
                tested_on: vec![],
            },
            parent_recipe_id: None,
            authored_at_ms: 0,
            updated_at_ms: 0,
        }
    }

    // ── ForgeArtifact::from_recipe — the recipe→artifact projection ──

    /// What this catches: `from_recipe` denormalizes every recipe field the
    /// model card renders (lineage + prose + config), and assigns a FRESH
    /// artifact id distinct from the recipe's. This is the single-source
    /// projection the stub (and the future foundry executor) build on;
    /// dropping a field here silently strips it from every artifact.
    #[test]
    fn from_recipe_projects_inherited_fields_with_fresh_id() {
        let recipe = synthetic_recipe();
        let a = ForgeArtifact::from_recipe(&recipe);

        assert_ne!(a.id, recipe.id, "artifact id must be fresh (1:N)");
        // Lineage.
        assert_eq!(a.recipe_id, recipe.id);
        assert_eq!(a.recipe_version, recipe.version);
        assert_eq!(a.recipe_name, recipe.name);
        // Denormalized prose / config snapshot.
        assert_eq!(a.description, recipe.description);
        assert_eq!(a.user_summary, recipe.user_summary);
        assert_eq!(a.author, recipe.author);
        assert_eq!(a.tags, recipe.tags);
        assert_eq!(a.license, recipe.license);
        assert_eq!(a.methodology_paper_url, recipe.methodology_paper_url);
        assert_eq!(a.limitations, recipe.limitations);
        assert_eq!(a.source.base_model, recipe.source.base_model);
        assert_eq!(a.calibration_corpus.content_hash, recipe.calibration_corpus.content_hash);
        assert_eq!(a.quant_tiers.len(), recipe.quant_tiers.len());
        assert_eq!(a.evaluation_benchmarks.len(), recipe.evaluation_benchmarks.len());
        assert_eq!(a.hardware.supports_cpu, recipe.hardware.supports_cpu);
    }

    /// What this catches: `from_recipe` returns an UNFORGED skeleton — every
    /// foundry-execution field at its "not yet run" default (forged_at_ms=0,
    /// the rest None/empty). The foundry/stub stamps these AFTER running
    /// stages; if the projection pre-filled them it would fabricate
    /// execution facts no run produced.
    #[test]
    fn from_recipe_leaves_execution_fields_unforged() {
        let a = ForgeArtifact::from_recipe(&synthetic_recipe());
        assert_eq!(a.forged_at_ms, 0, "unforged sentinel");
        assert!(a.duration_minutes.is_none());
        assert!(a.forged_params_b.is_none());
        assert!(a.active_params_b.is_none());
        assert!(a.hardware_verified.is_empty());
        assert!(a.alloy_hash.is_none());
        assert!(a.results.is_none());
        assert!(a.receipt.is_none());
        assert!(a.integrity.is_none());
    }

    /// What this catches: stub artifact carries the recipe's lineage
    /// (recipe_id + recipe_version + recipe_name) frozen at synthesis
    /// time. If a Phase 5+ refactor accidentally drops the lineage,
    /// the artifact would lose its provenance anchor.
    #[test]
    fn stub_artifact_carries_recipe_lineage() {
        let recipe = synthetic_recipe();
        let recipe_id = recipe.id;
        let artifact = synthesize_stub_artifact(&recipe, None).expect("synth");
        assert_eq!(artifact.recipe_id, recipe_id);
        assert_eq!(artifact.recipe_version, "0.1.0");
        assert_eq!(artifact.recipe_name, "test-recipe");
    }

    /// What this catches: stub artifact has its OWN id, not the recipe's.
    /// Multiple artifacts can come from one recipe (re-runs on different
    /// hardware) and each must be distinguishable.
    #[test]
    fn stub_artifact_has_distinct_id_from_recipe() {
        let recipe = synthetic_recipe();
        let artifact = synthesize_stub_artifact(&recipe, None).expect("synth");
        assert_ne!(
            artifact.id, recipe.id,
            "artifact id MUST differ from recipe id (1:N relationship)"
        );
    }

    /// What this catches: alloy_hash uses the canonical "sha256:..."
    /// prefix matching admission's content_hash convention. Stub
    /// includes "stub-" suffix so it's distinguishable from real hashes
    /// in the wild.
    #[test]
    fn stub_alloy_hash_is_canonical_with_stub_marker() {
        let recipe = synthetic_recipe();
        let artifact = synthesize_stub_artifact(&recipe, None).expect("synth");
        let hash = artifact.alloy_hash.expect("stub hash present");
        assert!(hash.starts_with("sha256:stub-"), "got: {hash}");
    }

    /// What this catches: hardware_node parameter (when set) lands in
    /// hardware_verified as a stub HardwareProfile. Phase 5+ will
    /// actually dispatch + populate real measurements; for now the
    /// caller sees their requested node echoed back.
    #[test]
    fn stub_artifact_records_requested_hardware_node() {
        let recipe = synthetic_recipe();
        let artifact = synthesize_stub_artifact(&recipe, Some("m5-pro@local")).expect("synth");
        assert_eq!(artifact.hardware_verified.len(), 1);
        assert_eq!(artifact.hardware_verified[0].device, "m5-pro@local");
        assert_eq!(artifact.hardware_verified[0].format, "stub");
        assert!(
            !artifact.hardware_verified[0].verified,
            "stub is not verified"
        );
    }

    /// What this catches: with no hardware_node, hardware_verified
    /// stays empty (vs an entry with empty device label). Caller can
    /// distinguish "no hw requested" from "hw requested but no metrics".
    #[test]
    fn stub_artifact_without_hardware_node_is_empty_verified() {
        let recipe = synthetic_recipe();
        let artifact = synthesize_stub_artifact(&recipe, None).expect("synth");
        assert!(artifact.hardware_verified.is_empty());
    }

    /// What this catches: Phase 4 fields that Phase 5+ will populate
    /// (results, receipt, integrity, duration, params_b) all start as
    /// None on the stub. A Phase 5 refactor that accidentally fills
    /// them with placeholder data would silently claim measurements
    /// that didn't happen.
    #[test]
    fn stub_artifact_phase5_fields_are_none() {
        let recipe = synthetic_recipe();
        let artifact = synthesize_stub_artifact(&recipe, Some("m5-pro@local")).expect("synth");
        assert!(artifact.results.is_none());
        assert!(artifact.receipt.is_none());
        assert!(artifact.integrity.is_none());
        assert!(artifact.duration_minutes.is_none());
        assert!(artifact.forged_params_b.is_none());
        assert!(artifact.active_params_b.is_none());
    }

    // ── forge/train + forge/export — DELEGATED to the custodian (#32) ──
    //
    // The byte work moved to the custodian, so these tests pin the two halves the
    // organism still owns: the REQUEST shape it sends, and that export sequences
    // load-checkpoint → package. A recording fake stands in for the HTTP
    // custodian (the real wire is unit-tested in inference::unsloth_forge).

    use crate::inference::unsloth_forge::{
        ExportGgufRequest, ExportLoraRequest, ExportResult, ForgeCustodian, ForgeTrainRequest,
        LoadCheckpointRequest, LoraCatalog, TrainHandle, TrainStatus,
    };
    use crate::inference::unsloth_control::UnslothError;
    use std::sync::Mutex;

    /// Records what the organism asked the custodian to do, and returns scripted
    /// results — so we assert on the request shape + call ORDER without a network.
    #[derive(Default)]
    struct RecordingCustodian {
        trains: Mutex<Vec<ForgeTrainRequest>>,
        loads: Mutex<Vec<LoadCheckpointRequest>>,
        loras: Mutex<Vec<ExportLoraRequest>>,
        ggufs: Mutex<Vec<ExportGgufRequest>>,
        /// Custodian's load/export success flag (to exercise the fail-loud path).
        succeed: bool,
    }

    impl RecordingCustodian {
        fn ok() -> Self {
            Self { succeed: true, ..Default::default() }
        }
    }

    #[async_trait]
    impl ForgeCustodian for RecordingCustodian {
        async fn train_start(
            &self,
            req: &ForgeTrainRequest,
        ) -> Result<TrainHandle, UnslothError> {
            self.trains.lock().unwrap().push(req.clone());
            Ok(TrainHandle { job_id: "job-1".into(), message: "started".into() })
        }
        async fn train_status(&self) -> Result<TrainStatus, UnslothError> {
            Ok(TrainStatus { job_id: "job-1".into(), phase: "training".into(), ..Default::default() })
        }
        async fn load_checkpoint(
            &self,
            req: &LoadCheckpointRequest,
        ) -> Result<ExportResult, UnslothError> {
            self.loads.lock().unwrap().push(req.clone());
            Ok(ExportResult { success: self.succeed, message: "loaded".into(), ..Default::default() })
        }
        async fn export_lora(
            &self,
            req: &ExportLoraRequest,
        ) -> Result<ExportResult, UnslothError> {
            self.loras.lock().unwrap().push(req.clone());
            Ok(ExportResult { success: self.succeed, message: "lora".into(), ..Default::default() })
        }
        async fn export_gguf(
            &self,
            req: &ExportGgufRequest,
        ) -> Result<ExportResult, UnslothError> {
            self.ggufs.lock().unwrap().push(req.clone());
            Ok(ExportResult { success: self.succeed, message: "gguf".into(), ..Default::default() })
        }
        async fn list_loras(&self) -> Result<LoraCatalog, UnslothError> {
            Ok(LoraCatalog::default())
        }
    }

    // what this catches: the recipe's genome knobs (base model, type, rank/alpha,
    // epochs) ride EXPLICITLY in the custodian body — never silently fall to the
    // custodian's defaults (the fail-loud-over-silent-substitution rule). Pure.
    #[test]
    fn train_request_carries_genome_knobs_explicitly() {
        let p = ForgeTrainParams {
            dataset_path: "/turns.jsonl".into(),
            base_model: "unsloth/Qwen3-0.6B".into(),
            format_type: "chat".into(),
            training_type: "lora".into(),
            lora_r: 32,
            lora_alpha: 64,
            num_epochs: 3,
            learning_rate: "2e-4".into(),
            batch_size: 1,
            gradient_accumulation_steps: 1,
            max_seq_length: 2048,
            load_in_4bit: true,
            dry_run: true,
        };
        let req = build_train_request(&p);
        assert_eq!(req.model_name, "unsloth/Qwen3-0.6B");
        assert_eq!(req.training_type, "lora");
        assert!(req.use_lora, "lora type → use_lora true");
        assert_eq!(req.lora_r, 32);
        assert_eq!(req.lora_alpha, 64);
        assert_eq!(req.num_epochs, 3);
        assert_eq!(req.local_datasets, vec!["/turns.jsonl".to_string()]);
    }

    // what this catches: a full fine-tune does NOT set use_lora (the type drives
    // the flag — so a "full" recipe doesn't accidentally request a LoRA).
    #[test]
    fn full_finetune_does_not_request_lora() {
        let p = ForgeTrainParams {
            dataset_path: "/d.jsonl".into(),
            base_model: "m".into(),
            format_type: "chat".into(),
            training_type: "full".into(),
            lora_r: 16,
            lora_alpha: 16,
            num_epochs: 1,
            learning_rate: "2e-4".into(),
            batch_size: 1,
            gradient_accumulation_steps: 1,
            max_seq_length: 2048,
            load_in_4bit: true,
            dry_run: true,
        };
        assert!(!build_train_request(&p).use_lora);
    }

    // what this catches: dry_run resolves the request WITHOUT contacting the
    // custodian (the wiring check — no run kicked off, nothing recorded).
    #[tokio::test]
    async fn forge_train_dry_run_does_not_call_custodian() {
        let cust = RecordingCustodian::ok();
        let p = ForgeTrainParams {
            dataset_path: "/turns.jsonl".into(),
            base_model: "m".into(),
            format_type: "chat".into(),
            training_type: "lora".into(),
            lora_r: 16,
            lora_alpha: 16,
            num_epochs: 1,
            learning_rate: "2e-4".into(),
            batch_size: 1,
            gradient_accumulation_steps: 1,
            max_seq_length: 2048,
            load_in_4bit: true,
            dry_run: true,
        };
        let v = match run_train(&cust, p).await.unwrap() {
            CommandResult::Json(v) => v,
            _ => panic!("json"),
        };
        assert_eq!(v["dry_run"], true);
        assert_eq!(v["request"]["model_name"], "m");
        assert!(cust.trains.lock().unwrap().is_empty(), "dry_run must not POST");
    }

    // what this catches: a live train DELEGATES to the custodian (records the
    // request, returns the handle's job_id) — the organism never spawns a trainer.
    #[tokio::test]
    async fn forge_train_delegates_to_custodian() {
        let cust = RecordingCustodian::ok();
        let p = ForgeTrainParams {
            dataset_path: "/turns.jsonl".into(),
            base_model: "m".into(),
            format_type: "chat".into(),
            training_type: "lora".into(),
            lora_r: 16,
            lora_alpha: 16,
            num_epochs: 1,
            learning_rate: "2e-4".into(),
            batch_size: 1,
            gradient_accumulation_steps: 1,
            max_seq_length: 2048,
            load_in_4bit: true,
            dry_run: false,
        };
        let v = match run_train(&cust, p).await.unwrap() {
            CommandResult::Json(v) => v,
            _ => panic!("json"),
        };
        assert_eq!(v["dry_run"], false);
        assert_eq!(v["job_id"], "job-1");
        assert_eq!(cust.trains.lock().unwrap().len(), 1);
    }

    // what this catches: export LOADS the checkpoint first, THEN packages it as a
    // LoRA (the custodian's export ops act on the loaded checkpoint, not a path).
    #[tokio::test]
    async fn forge_export_lora_loads_then_packages() {
        let cust = RecordingCustodian::ok();
        let p = ForgeExportParams {
            checkpoint: "/ckpt".into(),
            save_directory: "/out".into(),
            format: "lora".into(),
            quantization: "Q4_K_M".into(),
            max_seq_length: 2048,
            load_in_4bit: true,
        };
        let v = match run_export(&cust, p).await.unwrap() {
            CommandResult::Json(v) => v,
            _ => panic!("json"),
        };
        assert_eq!(v["format"], "lora");
        assert_eq!(cust.loads.lock().unwrap().len(), 1, "must load checkpoint first");
        assert_eq!(cust.loads.lock().unwrap()[0].checkpoint_path, "/ckpt");
        assert_eq!(cust.loras.lock().unwrap().len(), 1, "then export lora");
        assert_eq!(cust.loras.lock().unwrap()[0].save_directory, "/out");
        assert!(cust.ggufs.lock().unwrap().is_empty(), "lora path must not call gguf");
    }

    // what this catches: the GGUF path threads the quantization method through to
    // the custodian (the grid can want a quantized standalone, not just the LoRA).
    #[tokio::test]
    async fn forge_export_gguf_threads_quantization() {
        let cust = RecordingCustodian::ok();
        let p = ForgeExportParams {
            checkpoint: "/ckpt".into(),
            save_directory: "/out".into(),
            format: "gguf".into(),
            quantization: "Q5_K_M".into(),
            max_seq_length: 2048,
            load_in_4bit: true,
        };
        run_export(&cust, p).await.unwrap();
        assert_eq!(cust.ggufs.lock().unwrap().len(), 1);
        assert_eq!(cust.ggufs.lock().unwrap()[0].quantization_method, "Q5_K_M");
    }

    // what this catches: a custodian that fails the load is surfaced LOUDLY (no
    // silent no-op export of a checkpoint that didn't load — the bug class #32
    // was opened to kill).
    #[tokio::test]
    async fn forge_export_fails_loud_when_load_fails() {
        let cust = RecordingCustodian::default(); // succeed=false
        let p = ForgeExportParams {
            checkpoint: "/ckpt".into(),
            save_directory: "/out".into(),
            format: "lora".into(),
            quantization: "Q4_K_M".into(),
            max_seq_length: 2048,
            load_in_4bit: true,
        };
        let err = run_export(&cust, p).await.expect_err("load failure must error");
        assert!(err.contains("load-checkpoint failed"), "got: {err}");
        assert!(cust.loras.lock().unwrap().is_empty(), "must not export after a failed load");
    }

    // ── forge/decide — "assemble best self, or train" (product-thesis decision) ──

    // what this catches: when a market candidate clears the adopt margin, ASSEMBLE
    // it (zero training) and report the lift. This is the "shop before you build"
    // fast path — the whole point of not starting from zero.
    #[tokio::test]
    async fn decide_assembles_when_market_candidate_clears_margin() {
        let module = ForgeModule::new();
        let params = json!({
            "baseline": 0.60,
            "adopt_margin": 0.02,
            "candidates": [
                {"label": "peer-A/rust-lora", "score": 0.72},
                {"label": "peer-B/rust-lora", "score": 0.81},
            ]
        });
        let v = match module.handle_command("forge/decide", params).await.unwrap() {
            CommandResult::Json(v) => v,
            _ => panic!("json"),
        };
        assert_eq!(v["action"], "assemble");
        assert_eq!(v["chosen"], "peer-B/rust-lora"); // best score wins
        assert!((v["lift"].as_f64().unwrap() - 0.21).abs() < 1e-9);
    }

    // what this catches: when no candidate clears the margin, the market came up
    // short → TRAIN (forage + forge), reporting the gap to close. This is the slow
    // path the fitness GAP triggers.
    #[tokio::test]
    async fn decide_trains_when_market_falls_short() {
        let module = ForgeModule::new();
        let params = json!({
            "baseline": 0.80,
            "adopt_margin": 0.05,
            "candidates": [ {"label": "peer/old-lora", "score": 0.81} ] // +0.01 < 0.05
        });
        let v = match module.handle_command("forge/decide", params).await.unwrap() {
            CommandResult::Json(v) => v,
            _ => panic!("json"),
        };
        assert_eq!(v["action"], "train");
        assert_eq!(v["best_candidate"], "peer/old-lora");
        assert!((v["gap"].as_f64().unwrap() - 0.04).abs() < 1e-9);
    }

    // what this catches: an empty market (nothing to assemble) → train from base.
    #[tokio::test]
    async fn decide_trains_when_market_empty() {
        let module = ForgeModule::new();
        let v = match module
            .handle_command("forge/decide", json!({ "baseline": 0.5, "candidates": [] }))
            .await
            .unwrap()
        {
            CommandResult::Json(v) => v,
            _ => panic!("json"),
        };
        assert_eq!(v["action"], "train");
    }

}
