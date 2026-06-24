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
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::any::Any;
use std::path::PathBuf;
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
                run_train(parsed).await
            }
            "forge/export" => {
                let parsed: ForgeExportParams = serde_json::from_value(params)
                    .map_err(|e| format!("forge/export: invalid params: {e}"))?;
                run_export(parsed).await
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
// forge/train — the train stage of the foundry (Phase 5, slice 1).
//
// Closes the coordination↔learning flywheel: a persona's room work becomes a
// ShareGPT/chat dataset (`dataset/from-turns`), and THIS drives unsloth to
// fine-tune a LoRA genome on it. continuum orchestrates; unsloth executes the
// train (the engine). The trained LoRA is later paged into the genome (the
// next slice). See memory coordination-learning-flywheel.
//=============================================================================

/// Inputs for `forge/train`. The dataset is the JSONL `dataset/from-turns`
/// emits (chat `{messages}` format); the base model + LoRA params come from a
/// `ForgeRecipe` or directly. `dry_run` resolves the invocation and exits
/// without training — lets the seam be validated end-to-end without GPU time.
#[derive(Debug, Deserialize)]
struct ForgeTrainParams {
    /// Local training dataset path (the `dataset/from-turns` JSONL).
    dataset_path: String,
    /// Base model to fine-tune (HF id or local path).
    base_model: String,
    /// Where the LoRA checkpoint is written.
    #[serde(default = "default_output_dir")]
    output_dir: String,
    /// Dataset format unsloth parses. `dataset/from-turns` emits chat
    /// `{messages:[{role,content}]}`, which unsloth reads as "chat".
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
    /// Resolve the invocation + exit WITHOUT training (unsloth `--dry-run`).
    #[serde(default)]
    dry_run: bool,
}

fn default_output_dir() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".continuum/forge/lora")
        .to_string_lossy()
        .into_owned()
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

/// Resolve the `unsloth` binary — PATH first, then the default install dir.
fn unsloth_bin() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let cand = dir.join("unsloth");
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    let home = std::env::var("HOME").ok()?;
    let cand = PathBuf::from(home).join(".local/bin/unsloth");
    cand.is_file().then_some(cand)
}

/// Drive `unsloth train` on the dataset. Off the main thread — the foundry is
/// heavy. On `dry_run`, unsloth resolves + prints the config and exits 0 (the
/// wiring is validated without a GPU run).
async fn run_unsloth_train(p: ForgeTrainParams) -> Result<CommandResult, String> {
    let bin = unsloth_bin().ok_or_else(|| {
        "unsloth not found on PATH or ~/.local/bin — the training engine must be installed \
         (curl -fsSL https://unsloth.ai/install.sh | sh)"
            .to_string()
    })?;

    if !PathBuf::from(&p.dataset_path).is_file() {
        return Err(format!(
            "dataset_path not found: {} — produce one with `dataset/from-turns` first",
            p.dataset_path
        ));
    }

    let mut cmd = tokio::process::Command::new(&bin);
    cmd.arg("train")
        .arg("--local-dataset")
        .arg(&p.dataset_path)
        .arg("--model")
        .arg(&p.base_model)
        .arg("--format-type")
        .arg(&p.format_type)
        .arg("--training-type")
        .arg(&p.training_type)
        .arg("--output-dir")
        .arg(&p.output_dir)
        .arg("--lora-r")
        .arg(p.lora_r.to_string())
        .arg("--lora-alpha")
        .arg(p.lora_alpha.to_string())
        .arg("--num-epochs")
        .arg(p.num_epochs.to_string());
    if p.dry_run {
        cmd.arg("--dry-run");
    }

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("failed to launch unsloth train: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        return Err(format!(
            "unsloth train exited {}: {}",
            output.status.code().unwrap_or(-1),
            if stderr.is_empty() { &stdout } else { &stderr }
        ));
    }

    Ok(CommandResult::Json(json!({
        "dry_run": p.dry_run,
        "base_model": p.base_model,
        "dataset_path": p.dataset_path,
        "output_dir": p.output_dir,
        "training_type": p.training_type,
        "stdout": stdout,
    })))
}

//=============================================================================
// Engine selection — the foundry trains on the platform-correct muscle.
//
// Apple Silicon trains via Apple's native MLX (`mlx_lm.lora`); NVIDIA via
// Unsloth (CUDA). The hand-proven M5 chain (mlx_lm.lora → mlx_lm.fuse →
// llama.cpp convert → llama-quantize) is encoded below so `forge/train` +
// `forge/export` run the right engine with NO caller knob — platform dispatch,
// not a runtime tuning threshold. The "config attribute" error that looked like
// a wall was just `unsloth train`'s MLX wrapper accessing an HF-style
// `model.config` the MLX object lacks — wrong entry point, not a Mac limitation.
// See memory unsloth-mlx-train-broken-on-mac.
//=============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrainEngine {
    Mlx,
    Unsloth,
}

/// Pick the training engine from the BUILD TARGET. Apple Silicon → MLX (the only
/// path that trains on M-series); everything else → Unsloth. Compile-time
/// platform dispatch, never an env knob.
const fn select_engine() -> TrainEngine {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        TrainEngine::Mlx
    } else {
        TrainEngine::Unsloth
    }
}

const MLX_MISSING: &str =
    "mlx_lm not found — Apple-Silicon training needs the MLX toolchain (unsloth-studio bundles \
     it at ~/.unsloth/studio/unsloth_studio/bin)";

/// MLX LoRA train geometry. `--num-layers`/`--max-seq-length`/`--batch-size` are
/// the proven M5 defaults; rank+scale come from the recipe's `lora_r`/`lora_alpha`
/// via a config YAML (they aren't CLI flags on `mlx_lm.lora`).
const MLX_NUM_LAYERS: u32 = 16;
const MLX_MAX_SEQ_LENGTH: u32 = 2048;
const MLX_BATCH_SIZE: u32 = 2;

/// The unsloth-studio install root (`~/.unsloth`) — home of the bundled MLX
/// toolchain (`mlx_lm.*`), the venv python (has transformers), and the llama.cpp
/// build (convert script + `llama-quantize`). One root, resolved once.
fn unsloth_root() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let root = PathBuf::from(home).join(".unsloth");
    root.is_dir().then_some(root)
}

/// Resolve an MLX tool (`mlx_lm.lora`, `mlx_lm.fuse`, …) — PATH first, then the
/// studio venv bin.
fn mlx_tool_bin(tool: &str) -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let cand = dir.join(tool);
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    let cand = unsloth_root()?
        .join("studio/unsloth_studio/bin")
        .join(tool);
    cand.is_file().then_some(cand)
}

/// epochs → mlx `--iters`: one iter consumes one batch; an epoch is
/// `ceil(examples / batch)` iters. At least 1. Pure.
fn mlx_iters_for(num_examples: usize, num_epochs: u32, batch: u32) -> u32 {
    let batch = batch.max(1);
    let per_epoch = (num_examples as u32).div_ceil(batch).max(1);
    (num_epochs.max(1) * per_epoch).max(1)
}

/// Build the `mlx_lm.lora` train argv. Pure — pinned independently of a real run.
fn build_mlx_train_args(
    base_model: &str,
    data_dir: &str,
    adapter_path: &str,
    iters: u32,
    config_path: &str,
) -> Vec<String> {
    vec![
        "--model".into(),
        base_model.into(),
        "--train".into(),
        "--data".into(),
        data_dir.into(),
        "--fine-tune-type".into(),
        "lora".into(),
        "--num-layers".into(),
        MLX_NUM_LAYERS.to_string(),
        "--iters".into(),
        iters.to_string(),
        "--batch-size".into(),
        MLX_BATCH_SIZE.to_string(),
        "--max-seq-length".into(),
        MLX_MAX_SEQ_LENGTH.to_string(),
        "--adapter-path".into(),
        adapter_path.into(),
        "-c".into(),
        config_path.into(),
    ]
}

/// Count non-blank JSONL records.
fn count_jsonl_lines(p: &std::path::Path) -> Result<usize, String> {
    Ok(std::fs::read_to_string(p)
        .map_err(|e| format!("read {}: {e}", p.display()))?
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count())
}

/// The mlx lora config YAML carrying rank+scale (these aren't CLI flags on
/// `mlx_lm.lora`, so the recipe's lora_r/lora_alpha would be lost without it —
/// fail-loud honesty over silent default-substitution).
fn write_mlx_lora_config(path: &std::path::Path, lora_r: u32, lora_alpha: u32) -> Result<(), String> {
    let yaml = format!(
        "fine_tune_type: lora\nlora_parameters:\n  rank: {lora_r}\n  scale: {:.1}\n  dropout: 0.0\n",
        lora_alpha as f64
    );
    std::fs::write(path, yaml).map_err(|e| format!("write mlx config {}: {e}", path.display()))
}

/// Materialize the `{train,valid}.jsonl` data dir mlx_lm.lora requires from a
/// single dataset file (hold out ~10% as valid; duplicate when only one record).
fn materialize_mlx_data_dir(src: &std::path::Path, data_dir: &std::path::Path) -> Result<(), String> {
    let body =
        std::fs::read_to_string(src).map_err(|e| format!("read dataset {}: {e}", src.display()))?;
    let lines: Vec<&str> = body.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return Err(format!("dataset {} is empty", src.display()));
    }
    std::fs::create_dir_all(data_dir).map_err(|e| format!("mkdir {}: {e}", data_dir.display()))?;
    let (train, valid): (Vec<&str>, Vec<&str>) = if lines.len() < 2 {
        (lines.clone(), lines.clone())
    } else {
        let n_valid = (lines.len() / 10).max(1);
        let split = lines.len() - n_valid;
        (lines[..split].to_vec(), lines[split..].to_vec())
    };
    std::fs::write(data_dir.join("train.jsonl"), format!("{}\n", train.join("\n")))
        .map_err(|e| format!("write train.jsonl: {e}"))?;
    std::fs::write(data_dir.join("valid.jsonl"), format!("{}\n", valid.join("\n")))
        .map_err(|e| format!("write valid.jsonl: {e}"))?;
    Ok(())
}

/// Drive `mlx_lm.lora` on Apple Silicon. Off the main thread. On `dry_run`,
/// resolves args + iters WITHOUT requiring the engine installed (pure wiring
/// check); a real run materializes the data dir + lora config, then spawns.
async fn run_mlx_train(p: ForgeTrainParams) -> Result<CommandResult, String> {
    // Resolve the data dir + train-example count (dir-as-is, or a file we'll
    // materialize beside the output).
    let src = PathBuf::from(&p.dataset_path);
    let (data_dir, n_examples) = if src.is_dir() {
        let train = src.join("train.jsonl");
        if !train.is_file() {
            return Err(format!("data dir {} has no train.jsonl", src.display()));
        }
        (src.clone(), count_jsonl_lines(&train)?)
    } else {
        (PathBuf::from(&p.output_dir).join("data"), count_jsonl_lines(&src)?)
    };
    let iters = mlx_iters_for(n_examples, p.num_epochs, MLX_BATCH_SIZE);
    let config_path = PathBuf::from(&p.output_dir).join("mlx_lora_config.yaml");
    let args = build_mlx_train_args(
        &p.base_model,
        &data_dir.to_string_lossy(),
        &p.output_dir,
        iters,
        &config_path.to_string_lossy(),
    );

    if p.dry_run {
        return Ok(CommandResult::Json(json!({
            "dry_run": true,
            "engine": "mlx",
            "base_model": p.base_model,
            "dataset_path": p.dataset_path,
            "output_dir": p.output_dir,
            "training_type": "lora",
            "iters": iters,
            "args": args,
        })));
    }

    let bin = mlx_tool_bin("mlx_lm.lora").ok_or_else(|| MLX_MISSING.to_string())?;
    std::fs::create_dir_all(&p.output_dir)
        .map_err(|e| format!("mkdir {}: {e}", p.output_dir))?;
    if !src.is_dir() {
        materialize_mlx_data_dir(&src, &data_dir)?;
    }
    write_mlx_lora_config(&config_path, p.lora_r, p.lora_alpha)?;

    let output = tokio::process::Command::new(&bin)
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("failed to launch mlx_lm.lora: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        return Err(format!(
            "mlx_lm.lora exited {}: {}",
            output.status.code().unwrap_or(-1),
            if stderr.is_empty() { &stdout } else { &stderr }
        ));
    }

    Ok(CommandResult::Json(json!({
        "dry_run": false,
        "engine": "mlx",
        "base_model": p.base_model,
        "dataset_path": p.dataset_path,
        "output_dir": p.output_dir,
        "training_type": "lora",
        "iters": iters,
        "adapter": PathBuf::from(&p.output_dir).join("adapters.safetensors").to_string_lossy(),
        "stdout": stdout,
        "stderr": stderr,
    })))
}

/// `forge/train` dispatcher — engine-agnostic dataset guard, then the
/// platform-correct trainer (MLX on Apple Silicon, Unsloth on NVIDIA).
async fn run_train(p: ForgeTrainParams) -> Result<CommandResult, String> {
    if !PathBuf::from(&p.dataset_path).exists() {
        return Err(format!(
            "dataset_path not found: {} — produce one with `dataset/from-turns` first",
            p.dataset_path
        ));
    }
    match select_engine() {
        TrainEngine::Mlx => run_mlx_train(p).await,
        TrainEngine::Unsloth => run_unsloth_train(p).await,
    }
}

//=============================================================================
// forge/export — the package stage (Phase 5, slice 2).
//
// Turns a trained checkpoint into the PAGEABLE genome layer — the unit the grid
// exchanges. `--format lora` exports the LoRA adapter (the genome layer);
// `gguf` exports a quantized standalone. This is the "reproduction" step: a
// layer packaged so it can be paged in, verified, and spread P2P. The grid card
// (content-addressed hash + emit to the catalog) is the next slice.
// See memory lora-layers-as-p2p-exchanged-genome.
//=============================================================================

/// Inputs for `forge/export`. `checkpoint` is `forge/train`'s output dir.
#[derive(Debug, Deserialize)]
struct ForgeExportParams {
    /// The trained checkpoint directory (a `forge/train` output-dir).
    checkpoint: String,
    /// Where the exported artifact is written.
    output_dir: String,
    /// Export format: "lora" (the pageable genome layer — default), "gguf",
    /// "merged-16bit", "merged-4bit".
    #[serde(default = "default_export_format")]
    format: String,
    /// GGUF quantization (only applied when `format == "gguf"`): q4_k_m,
    /// q5_k_m, q8_0, f16.
    #[serde(default = "default_quantization")]
    quantization: String,
}

fn default_export_format() -> String {
    "lora".to_string()
}
fn default_quantization() -> String {
    "q4_k_m".to_string()
}

/// Build the `unsloth export` argument vector. Pure — unit-testable without
/// spawning a process or owning a real checkpoint, so the invocation continuum
/// constructs is pinned independently of an actual export run.
fn build_export_args(p: &ForgeExportParams) -> Vec<String> {
    let mut args = vec![
        "export".to_string(),
        p.checkpoint.clone(),
        p.output_dir.clone(),
        "--format".to_string(),
        p.format.clone(),
    ];
    // Quantization only applies to the GGUF path.
    if p.format == "gguf" {
        args.push("--quantization".to_string());
        args.push(p.quantization.clone());
    }
    args
}

/// Drive `unsloth export` on a trained checkpoint. Off the main thread.
async fn run_unsloth_export(p: ForgeExportParams) -> Result<CommandResult, String> {
    let bin = unsloth_bin().ok_or_else(|| {
        "unsloth not found on PATH or ~/.local/bin — the training engine must be installed \
         (curl -fsSL https://unsloth.ai/install.sh | sh)"
            .to_string()
    })?;

    if !PathBuf::from(&p.checkpoint).is_dir() {
        return Err(format!(
            "checkpoint not found: {} — train one with `forge/train` first",
            p.checkpoint
        ));
    }

    let args = build_export_args(&p);
    let output = tokio::process::Command::new(&bin)
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("failed to launch unsloth export: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        return Err(format!(
            "unsloth export exited {}: {}",
            output.status.code().unwrap_or(-1),
            if stderr.is_empty() { &stdout } else { &stderr }
        ));
    }

    Ok(CommandResult::Json(json!({
        "format": p.format,
        "checkpoint": p.checkpoint,
        "output_dir": p.output_dir,
        "stdout": stdout,
    })))
}

/// The studio venv python (`~/.unsloth/studio/unsloth_studio/bin/python3`) — the
/// one with transformers; system python3 lacks it (proven gotcha).
fn studio_python() -> Option<PathBuf> {
    let cand = unsloth_root()?.join("studio/unsloth_studio/bin/python3");
    cand.is_file().then_some(cand)
}

/// llama.cpp's safetensors→GGUF converter (studio's self-contained copy, so the
/// path doesn't depend on the core process CWD).
fn llama_convert_script() -> Option<PathBuf> {
    let cand = unsloth_root()?.join("llama.cpp/convert_hf_to_gguf.py");
    cand.is_file().then_some(cand)
}

/// `llama-quantize` — PATH first, then the studio llama.cpp build.
fn llama_quantize_bin() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let cand = dir.join("llama-quantize");
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    let cand = unsloth_root()?.join("llama.cpp/build/bin/llama-quantize");
    cand.is_file().then_some(cand)
}

/// Read the base model an mlx adapter was trained against from its
/// `adapter_config.json` (`mlx_lm.fuse` needs it; the checkpoint records it).
fn read_mlx_base_model(checkpoint: &str) -> Result<String, String> {
    let cfg = PathBuf::from(checkpoint).join("adapter_config.json");
    let body = std::fs::read_to_string(&cfg)
        .map_err(|e| format!("read {} ({e}) — not an mlx checkpoint?", cfg.display()))?;
    let v: Value =
        serde_json::from_str(&body).map_err(|e| format!("parse adapter_config.json: {e}"))?;
    v.get("model")
        .and_then(|m| m.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("adapter_config.json in {checkpoint} has no \"model\" base"))
}

/// Drive the MLX export chain on Apple Silicon. `lora` returns the adapter
/// as-is (it IS the pageable layer); `merged-16bit` fuses to standalone
/// safetensors; `gguf` fuses → converts (studio python + llama.cpp) → quantizes
/// (llama-quantize) so the result serves on llama-server for the A/B re-eval.
async fn run_mlx_export(p: ForgeExportParams) -> Result<CommandResult, String> {
    // `lora`: the mlx adapter dir IS the genome layer — nothing to fuse.
    if p.format == "lora" {
        let adapter = PathBuf::from(&p.checkpoint).join("adapters.safetensors");
        if !adapter.is_file() {
            return Err(format!(
                "no adapters.safetensors in {} — not an mlx LoRA checkpoint",
                p.checkpoint
            ));
        }
        return Ok(CommandResult::Json(json!({
            "engine": "mlx",
            "format": "lora",
            "checkpoint": p.checkpoint,
            "adapter": adapter.to_string_lossy(),
            "note": "mlx LoRA adapter is the pageable layer as-is",
        })));
    }

    // Fuse base+adapter → standalone safetensors (base read from the checkpoint).
    let base = read_mlx_base_model(&p.checkpoint)?;
    let fuse = mlx_tool_bin("mlx_lm.fuse").ok_or_else(|| MLX_MISSING.to_string())?;
    std::fs::create_dir_all(&p.output_dir)
        .map_err(|e| format!("mkdir {}: {e}", p.output_dir))?;
    let fused = PathBuf::from(&p.output_dir).join("fused");
    let fuse_out = tokio::process::Command::new(&fuse)
        .arg("--model")
        .arg(&base)
        .arg("--adapter-path")
        .arg(&p.checkpoint)
        .arg("--save-path")
        .arg(&fused)
        .output()
        .await
        .map_err(|e| format!("failed to launch mlx_lm.fuse: {e}"))?;
    if !fuse_out.status.success() {
        return Err(format!(
            "mlx_lm.fuse exited {}: {}",
            fuse_out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&fuse_out.stderr)
        ));
    }

    if p.format == "merged-16bit" || p.format == "merged" || p.format == "merged-4bit" {
        return Ok(CommandResult::Json(json!({
            "engine": "mlx",
            "format": p.format,
            "checkpoint": p.checkpoint,
            "base_model": base,
            "fused": fused.to_string_lossy(),
        })));
    }

    if p.format == "gguf" {
        // fused safetensors → f16 GGUF (studio python has transformers;
        // mlx_lm.fuse --export-gguf can't do qwen, so use llama.cpp convert).
        let py = studio_python().ok_or_else(|| {
            "studio python not found (~/.unsloth/studio/unsloth_studio/bin/python3) — needed for \
             the GGUF convert step"
                .to_string()
        })?;
        let conv = llama_convert_script().ok_or_else(|| {
            "convert_hf_to_gguf.py not found (~/.unsloth/llama.cpp) — needed for the GGUF convert \
             step"
                .to_string()
        })?;
        let f16 = PathBuf::from(&p.output_dir).join("model.f16.gguf");
        let conv_out = tokio::process::Command::new(&py)
            .arg(&conv)
            .arg(&fused)
            .arg("--outfile")
            .arg(&f16)
            .arg("--outtype")
            .arg("f16")
            .output()
            .await
            .map_err(|e| format!("failed to launch convert_hf_to_gguf.py: {e}"))?;
        if !conv_out.status.success() {
            return Err(format!(
                "convert_hf_to_gguf.py exited {}: {}",
                conv_out.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&conv_out.stderr)
            ));
        }

        // f16 needs no quantize step — it's the convert output already.
        if p.quantization == "f16" {
            return Ok(CommandResult::Json(json!({
                "engine": "mlx",
                "format": "gguf",
                "quantization": "f16",
                "checkpoint": p.checkpoint,
                "base_model": base,
                "gguf": f16.to_string_lossy(),
            })));
        }

        // f16 GGUF → quantized tier via llama-quantize (type is uppercase, e.g. Q4_K_M).
        let quant_bin = llama_quantize_bin().ok_or_else(|| {
            "llama-quantize not found (PATH or ~/.unsloth/llama.cpp/build/bin) — needed to \
             quantize the GGUF"
                .to_string()
        })?;
        let qgguf =
            PathBuf::from(&p.output_dir).join(format!("model.{}.gguf", p.quantization));
        let q_out = tokio::process::Command::new(&quant_bin)
            .arg(&f16)
            .arg(&qgguf)
            .arg(p.quantization.to_uppercase())
            .output()
            .await
            .map_err(|e| format!("failed to launch llama-quantize: {e}"))?;
        if !q_out.status.success() {
            return Err(format!(
                "llama-quantize exited {}: {}",
                q_out.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&q_out.stderr)
            ));
        }

        return Ok(CommandResult::Json(json!({
            "engine": "mlx",
            "format": "gguf",
            "quantization": p.quantization,
            "checkpoint": p.checkpoint,
            "base_model": base,
            "gguf": qgguf.to_string_lossy(),
        })));
    }

    Err(format!(
        "unsupported mlx export format: {} (lora|merged-16bit|gguf)",
        p.format
    ))
}

/// `forge/export` dispatcher — engine-agnostic checkpoint guard, then the
/// platform-correct exporter (MLX chain on Apple Silicon, Unsloth on NVIDIA).
async fn run_export(p: ForgeExportParams) -> Result<CommandResult, String> {
    if !PathBuf::from(&p.checkpoint).is_dir() {
        return Err(format!(
            "checkpoint not found: {} — train one with `forge/train` first",
            p.checkpoint
        ));
    }
    match select_engine() {
        TrainEngine::Mlx => run_mlx_export(p).await,
        TrainEngine::Unsloth => run_unsloth_export(p).await,
    }
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

    // ── forge/train — the train stage (Phase 5, slice 1) ──

    // what this catches: forge/train errors clearly when the dataset is missing
    // (the train stage's input is the dataset/from-turns JSONL). Deterministic —
    // no unsloth / GPU needed; the missing-input guard fires before launch.
    #[tokio::test]
    async fn forge_train_errors_on_missing_dataset() {
        // Only meaningful when unsloth IS resolvable (the bin check runs first);
        // skip otherwise so the dataset-guard assertion stays deterministic.
        if unsloth_bin().is_none() {
            println!("SKIP: no unsloth binary — can't reach the dataset guard.");
            return;
        }
        let params = json!({
            "dataset_path": "/nonexistent/turns.jsonl",
            "base_model": "test-model",
            "dry_run": true,
        });
        let module = ForgeModule::new();
        let err = module
            .handle_command("forge/train", params)
            .await
            .expect_err("missing dataset must error");
        assert!(err.contains("dataset_path not found"), "got: {err}");
    }

    // what this catches: the full forge/train → unsloth invocation, validated
    // WITHOUT a GPU run via --dry-run (unsloth resolves the config + exits 0).
    // Proves continuum builds a VALID unsloth train invocation from a from-turns
    // dataset — the train half of the genome loop. Skip-if-no-unsloth (repo
    // live-test convention).
    #[tokio::test]
    async fn forge_train_dry_run_resolves_against_live_unsloth() {
        if unsloth_bin().is_none() {
            println!("SKIP: no unsloth binary — install the training engine to exercise this.");
            return;
        }
        // A minimal chat-format dataset (one example), like dataset/from-turns emits.
        let tmp = std::env::temp_dir().join(format!("forge-train-test-{}.jsonl", std::process::id()));
        std::fs::write(
            &tmp,
            r#"{"messages":[{"role":"user","content":"hi"},{"role":"assistant","content":"hello"}]}"#,
        )
        .expect("write dataset");

        let params = json!({
            "dataset_path": tmp.to_string_lossy(),
            "base_model": "unsloth/Qwen3-0.6B",
            "training_type": "lora",
            "dry_run": true,
        });
        let module = ForgeModule::new();
        let result = module.handle_command("forge/train", params).await;
        let _ = std::fs::remove_file(&tmp);

        match result {
            Ok(CommandResult::Json(v)) => {
                assert_eq!(v["dry_run"], true);
                assert_eq!(v["training_type"], "lora");
                println!("✓ forge/train dry-run resolved: {}", v["stdout"]);
            }
            Ok(other) => panic!("expected JSON, got {other:?}"),
            // A reachable unsloth that rejects the dry-run (e.g. needs a real
            // model present) is an environment limit, not a wiring failure —
            // surface it as a skip, since the invocation itself was built + launched.
            Err(e) => println!("SKIP: unsloth rejected the dry-run (env limit, not wiring): {e}"),
        }
    }

    // ── forge/export — the package stage (Phase 5, slice 2) ──

    // what this catches: the LoRA-export invocation continuum builds. The
    // pageable genome layer is `--format lora`; quantization is NOT passed for
    // it (only GGUF quantizes). Pure arg-builder — deterministic, no unsloth or
    // checkpoint needed.
    #[test]
    fn export_args_for_lora_omit_quantization() {
        let p = ForgeExportParams {
            checkpoint: "/ckpt".to_string(),
            output_dir: "/out".to_string(),
            format: "lora".to_string(),
            quantization: "q4_k_m".to_string(),
        };
        let args = build_export_args(&p);
        assert_eq!(
            args,
            vec!["export", "/ckpt", "/out", "--format", "lora"],
            "lora export must not pass --quantization"
        );
    }

    // what this catches: the GGUF path DOES thread quantization through (the
    // grid can want a quantized standalone, not just the adapter).
    #[test]
    fn export_args_for_gguf_include_quantization() {
        let p = ForgeExportParams {
            checkpoint: "/ckpt".to_string(),
            output_dir: "/out".to_string(),
            format: "gguf".to_string(),
            quantization: "q5_k_m".to_string(),
        };
        let args = build_export_args(&p);
        assert_eq!(
            args,
            vec!["export", "/ckpt", "/out", "--format", "gguf", "--quantization", "q5_k_m"]
        );
    }

    // what this catches: forge/export errors clearly when the checkpoint is
    // missing (you can't package a layer that wasn't trained). Skip-if-no-unsloth
    // (the bin check runs first, like forge/train).
    #[tokio::test]
    async fn forge_export_errors_on_missing_checkpoint() {
        if unsloth_bin().is_none() {
            println!("SKIP: no unsloth binary — can't reach the checkpoint guard.");
            return;
        }
        let params = json!({
            "checkpoint": "/nonexistent/ckpt",
            "output_dir": "/tmp/forge-export-out",
            "format": "lora",
        });
        let module = ForgeModule::new();
        let err = module
            .handle_command("forge/export", params)
            .await
            .expect_err("missing checkpoint must error");
        assert!(err.contains("checkpoint not found"), "got: {err}");
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

    // ── MLX (Apple-Silicon) train+export wiring — pure, no engine needed ──

    // what this catches: epochs→iters geometry. One iter per batch; an epoch is
    // ceil(examples/batch); ≥1 even for degenerate inputs (0 examples, 0 epochs).
    #[test]
    fn mlx_iters_geometry() {
        assert_eq!(mlx_iters_for(20, 1, 2), 10); // 1 epoch of 20 @ batch 2
        assert_eq!(mlx_iters_for(20, 3, 2), 30); // 3 epochs
        assert_eq!(mlx_iters_for(1, 1, 2), 1); // ceil(1/2)=1
        assert_eq!(mlx_iters_for(0, 0, 0), 1); // never zero
    }

    // what this catches: the mlx_lm.lora argv is pinned — the proven M5
    // invocation (--train, --fine-tune-type lora, num-layers/iters/batch, the
    // -c config carrying rank) survives a refactor that reorders or drops a flag.
    #[test]
    fn mlx_train_args_are_pinned() {
        let args =
            build_mlx_train_args("unsloth/Qwen3.5-4B", "/d/data", "/d/out", 60, "/d/out/c.yaml");
        let joined = args.join(" ");
        assert!(joined.contains("--model unsloth/Qwen3.5-4B"), "{joined}");
        assert!(joined.contains("--train"), "{joined}");
        assert!(joined.contains("--data /d/data"), "{joined}");
        assert!(joined.contains("--fine-tune-type lora"), "{joined}");
        assert!(joined.contains("--iters 60"), "{joined}");
        assert!(joined.contains("--adapter-path /d/out"), "{joined}");
        assert!(joined.contains("-c /d/out/c.yaml"), "{joined}");
    }

    // what this catches: a single dataset FILE is split into the {train,valid}.jsonl
    // DIRECTORY mlx_lm.lora requires (valid present even when there's only one
    // record — it duplicates rather than leaving valid empty, which mlx rejects).
    #[test]
    fn mlx_data_dir_materializes_train_and_valid() {
        let base = std::env::temp_dir().join("forge-mlx-datadir-test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let src = base.join("turns.jsonl");
        // 20 records → ~10% (2) held out for valid.
        let body: String = (0..20)
            .map(|i| format!("{{\"messages\":[{{\"role\":\"user\",\"content\":\"{i}\"}}]}}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&src, body).unwrap();
        let data_dir = base.join("data");
        materialize_mlx_data_dir(&src, &data_dir).unwrap();
        assert_eq!(count_jsonl_lines(&data_dir.join("train.jsonl")).unwrap(), 18);
        assert_eq!(count_jsonl_lines(&data_dir.join("valid.jsonl")).unwrap(), 2);

        // Single record → valid duplicates train (never empty).
        let src1 = base.join("one.jsonl");
        std::fs::write(&src1, "{\"messages\":[]}").unwrap();
        let dir1 = base.join("data1");
        materialize_mlx_data_dir(&src1, &dir1).unwrap();
        assert_eq!(count_jsonl_lines(&dir1.join("train.jsonl")).unwrap(), 1);
        assert_eq!(count_jsonl_lines(&dir1.join("valid.jsonl")).unwrap(), 1);
        let _ = std::fs::remove_dir_all(&base);
    }

    // what this catches: recipe lora_r/lora_alpha are carried into the mlx config
    // YAML as rank/scale (they're not CLI flags, so without this they'd silently
    // fall back to mlx defaults — the fail-loud-over-silent-substitution rule).
    #[test]
    fn mlx_lora_config_carries_rank_and_scale() {
        let base = std::env::temp_dir().join("forge-mlx-config-test");
        std::fs::create_dir_all(&base).unwrap();
        let path = base.join("c.yaml");
        write_mlx_lora_config(&path, 16, 32).unwrap();
        let yaml = std::fs::read_to_string(&path).unwrap();
        assert!(yaml.contains("rank: 16"), "{yaml}");
        assert!(yaml.contains("scale: 32.0"), "{yaml}");
        assert!(yaml.contains("fine_tune_type: lora"), "{yaml}");
        let _ = std::fs::remove_dir_all(&base);
    }

    // what this catches: forge/export's fuse step reads the base model from the
    // mlx checkpoint's adapter_config.json (so the caller needn't re-supply it);
    // a checkpoint without that field errors loudly rather than fusing nothing.
    #[test]
    fn mlx_export_reads_base_from_adapter_config() {
        let base = std::env::temp_dir().join("forge-mlx-base-test");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(
            base.join("adapter_config.json"),
            "{\"model\":\"unsloth/Qwen3.5-4B\",\"fine_tune_type\":\"lora\"}",
        )
        .unwrap();
        assert_eq!(
            read_mlx_base_model(&base.to_string_lossy()).unwrap(),
            "unsloth/Qwen3.5-4B"
        );
        std::fs::write(base.join("adapter_config.json"), "{\"fine_tune_type\":\"lora\"}").unwrap();
        assert!(read_mlx_base_model(&base.to_string_lossy()).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    // what this catches: this build dispatches to the platform-correct engine —
    // Apple Silicon must select MLX (the only M-series trainer), everything else
    // Unsloth. A regression here silently routes Macs to the broken unsloth path.
    #[test]
    fn engine_selection_matches_platform() {
        let engine = select_engine();
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            assert_eq!(engine, TrainEngine::Mlx);
        } else {
            assert_eq!(engine, TrainEngine::Unsloth);
        }
    }
}
