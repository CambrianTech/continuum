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

use crate::forge::custodian_client::ForgeCustodianHttp;
use crate::forge::{ForgeArtifact, ForgeRecipe};
use crate::inference::unsloth_forge::{
    to_body, ForgeCustodian, ForgeTrainRequest, GenomeFormat, PackageRequest, UnslothForgeHttp,
};
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::any::Any;
use std::path::{Path, PathBuf};
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
                // Engine branch (#52): Apple Silicon owns the native mlx_lm.lora
                // run; other platforms (and an explicit selector) delegate to the
                // custodian. A deterministic platform branch, not a fallback.
                if mlx_engine_selected(parsed.engine.as_deref())? {
                    run_train_native_mlx(parsed)
                } else {
                    run_train(&UnslothForgeHttp::from_config(), parsed).await
                }
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
                // gguf-lora is served by the continuum forge CUSTODIAN (Contract C),
                // the only thing that can turn a trained MLX adapter into a pageable
                // GGUF LoRA. lora/gguf still route to unsloth until #52 completes.
                // Different capabilities live on different daemons — so the format
                // picks the custodian here, at the one dispatch point.
                if parsed.format == "gguf-lora" {
                    // The default manifest path — the same file the serving daemon
                    // reads at (re)spawn. Resolve loud (a pathological missing HOME
                    // with no override fails here, not by writing to a surprise loc).
                    let manifest = crate::forge::adapter_manifest::manifest_path()?;
                    run_export_gguf_lora(&ForgeCustodianHttp::from_config(), &parsed, &manifest).await
                } else {
                    run_export(&UnslothForgeHttp::from_config(), parsed).await
                }
            }
            "forge/health" => {
                // Contract C `/health` handshake as a COMMAND — so a remote node
                // can read this custodian's contract version + readiness + spare
                // slots over the grid transport (the receiving end of Pass 6's
                // `GridForgeCustodian::health`). Serves the LOCAL custodian's honest
                // health (R4); params are ignored.
                run_health(&ForgeCustodianHttp::from_config()).await
            }
            "forge/probe" => {
                // DISCOVER the custodian's current capability — the self-organizing
                // primitive a forge daemon (and the grid) route demand against.
                let cap = UnslothForgeHttp::from_config().probe().await;
                let json = serde_json::to_value(cap)
                    .map_err(|e| format!("forge/probe: serialize: {e}"))?;
                Ok(CommandResult::Json(json))
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
#[derive(Debug, Deserialize, Default)]
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
    /// Resolve the request + return it WITHOUT kicking a run.
    #[serde(default)]
    dry_run: bool,

    // --- native MLX path (#52, Apple Silicon) -------------------------------
    // When the engine resolves to "mlx" the foundry owns the train subprocess
    // (forge::mlx_train) instead of delegating to the unsloth custodian. These
    // carry the bits the native contract needs that the custodian one doesn't.
    /// "mlx" | "custodian". `None` → auto-detect by platform (Apple Silicon →
    /// mlx, else custodian). Explicit value wins — never a silent fallback.
    engine: Option<String>,
    /// Local HF safetensors dir to train against — the train base. REQUIRED for
    /// the mlx engine and asserted by the caller to be the SAME weights the
    /// gateway serves ([[genome-loop-trains-on-own-mistakes]]: train-base ==
    /// serve-base, or the LoRA washes out to ~0 lift). Fail loud if absent — the
    /// substrate never guesses which on-disk dir is the served model's HF form.
    train_base_dir: Option<String>,
    /// Pre-split data dir holding `train.jsonl` + `valid.jsonl` (the mlx_lm
    /// `--data` contract). Alternative to `dataset_path` auto-split; takes
    /// precedence when set.
    data_dir: Option<String>,
    /// Where to write the adapter (default `~/.continuum/forge/lora/<name>`).
    adapter_out: Option<String>,
    /// Adapter name for the default output path + dataset split dir.
    #[serde(default = "default_adapter_name")]
    adapter_name: String,
    /// mlx `--num-layers` (default -1 = all transformer blocks).
    #[serde(default = "default_num_layers")]
    num_layers: i32,
    /// mlx `--iters` (the native trainer counts iterations, not epochs).
    #[serde(default = "default_native_iters")]
    iters: u32,
    /// EXPLICIT base normalization: force `config.json` model_type to this mlx
    /// module name (e.g. `qwen3_5`). `None` leaves it alone.
    mlx_model_type: Option<String>,
    /// EXPLICIT base normalization: add this chat_template when the tokenizer
    /// lacks one. `None` leaves it alone (mlx fails loud if a chat corpus then
    /// can't render).
    chat_template: Option<String>,
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
fn default_adapter_name() -> String {
    "genome-lora".to_string()
}
fn default_num_layers() -> i32 {
    -1
}
fn default_native_iters() -> u32 {
    300
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

/// True when the native MLX engine should own the train run for the given
/// explicit selector. Explicit `engine` wins; `None` auto-detects by platform
/// (Apple Silicon trains on mlx). This is a deterministic platform branch, not a
/// fallback — if mlx is selected but its env is missing, [`run_mlx_train`] fails
/// loud rather than silently delegating to the custodian.
fn mlx_engine_selected(engine: Option<&str>) -> Result<bool, String> {
    match engine {
        Some("mlx") => Ok(true),
        Some("custodian") => Ok(false),
        Some(other) => Err(format!(
            "forge/train: unknown engine {other:?} (expected \"mlx\" or \"custodian\")"
        )),
        None => Ok(cfg!(all(target_os = "macos", target_arch = "aarch64"))),
    }
}

/// Resolve the interpreter that can `import mlx_lm` — a venv WE manage under
/// `~/.continuum/genome/venv` (NOT the legacy unsloth studio venv). Override via
/// `MLX_PYTHON` for a custom env. Existence + mlx_lm import are enforced loud by
/// [`run_mlx_train`]; provision the managed venv once with
/// `python3 -m venv ~/.continuum/genome/venv && ~/.continuum/genome/venv/bin/pip install mlx-lm`.
fn resolve_mlx_python() -> PathBuf {
    if let Ok(p) = std::env::var("MLX_PYTHON") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".continuum/genome/venv/bin/python3")
}

/// Read a JSONL file into one `Value` per non-blank line (fail loud on a
/// malformed line, naming the line number — never silently drop rows).
fn read_jsonl(path: &Path) -> Result<Vec<Value>, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line)
            .map_err(|e| format!("{}:{}: invalid JSONL: {e}", path.display(), i + 1))?;
        out.push(v);
    }
    Ok(out)
}

/// `forge/train` on Apple Silicon — own the `mlx_lm.lora` subprocess instead of
/// delegating to the unsloth custodian (#52, converges #32). Resolves the train
/// base (asserted train-base==serve-base), materializes the `{train,valid}.jsonl`
/// data dir mlx_lm expects, and runs [`run_mlx_train`] with scale derived from
/// the LoRA geometry (`alpha/rank`) per [[genome-loop-trains-on-own-mistakes]].
/// `dry_run` returns the resolved spec WITHOUT spawning — the wiring check.
fn run_train_native_mlx(p: ForgeTrainParams) -> Result<CommandResult, String> {
    use crate::forge::mlx_train::{run_mlx_train, MlxBasePrep, MlxTrainEnv, MlxTrainSpec};

    // --- train base: explicit + asserted to equal the served weights ---
    let base_model_dir = p
        .train_base_dir
        .as_ref()
        .map(|s| PathBuf::from(crate::model_registry::expand_user_path(Path::new(s))))
        .ok_or_else(|| {
            format!(
                "forge/train (mlx): train_base_dir is required — the local HF safetensors \
                 form of the served base {:?} (train-base==serve-base or the LoRA washes out \
                 to ~0 lift). The substrate will not guess which on-disk dir that is.",
                p.base_model
            )
        })?;

    // --- data dir: pre-split wins; else split dataset_path → {train,valid} ---
    let data_dir = match &p.data_dir {
        Some(d) => PathBuf::from(crate::model_registry::expand_user_path(Path::new(d))),
        None => {
            let examples = read_jsonl(Path::new(&p.dataset_path))?;
            if examples.is_empty() {
                return Err(format!(
                    "forge/train (mlx): dataset {} is empty — nothing to train on",
                    p.dataset_path
                ));
            }
            let home = std::env::var("HOME").map_err(|_| "HOME unset".to_string())?;
            let out = PathBuf::from(home)
                .join(".continuum/datasets")
                .join(format!("{}-mlx", p.adapter_name));
            // ONE packaging path (DatasetService::split_and_write → train/eval),
            // then materialize valid.jsonl mlx_lm wants from the eval split.
            crate::modules::dataset::DatasetService::split_and_write(
                &p.adapter_name,
                &out,
                &examples,
                0.9,
                None,
            )?;
            let eval = out.join("eval.jsonl");
            let valid = out.join("valid.jsonl");
            // mlx needs a NON-empty valid split; with tiny corpora the 0.9 split
            // can round eval to 0 rows, so fall back to copying train as valid.
            let eval_rows = read_jsonl(&eval).map(|r| r.len()).unwrap_or(0);
            let src = if eval_rows > 0 { &eval } else { &out.join("train.jsonl") };
            std::fs::copy(src, &valid)
                .map_err(|e| format!("materialize valid.jsonl from {}: {e}", src.display()))?;
            out
        }
    };

    // --- adapter output dir ---
    let adapter_out = match &p.adapter_out {
        Some(a) => PathBuf::from(crate::model_registry::expand_user_path(Path::new(a))),
        None => {
            let home = std::env::var("HOME").map_err(|_| "HOME unset".to_string())?;
            PathBuf::from(home)
                .join(".continuum/forge/lora")
                .join(&p.adapter_name)
        }
    };

    // mlx scale == alpha/rank (lora_convert reads scale → alpha = rank*scale).
    // ONE geometry contract: set lora_alpha = 2*lora_r for the proven scale~2.
    if p.lora_r == 0 {
        return Err("forge/train (mlx): lora_r must be > 0".to_string());
    }
    let scale = p.lora_alpha as f64 / p.lora_r as f64;
    let learning_rate: f64 = p
        .learning_rate
        .parse()
        .map_err(|e| format!("forge/train (mlx): learning_rate {:?}: {e}", p.learning_rate))?;

    let spec = MlxTrainSpec {
        base_model_dir,
        data_dir,
        adapter_out: adapter_out.clone(),
        rank: p.lora_r,
        scale,
        dropout: 0.0,
        num_layers: p.num_layers,
        batch_size: p.batch_size,
        iters: p.iters,
        learning_rate,
        max_seq_length: p.max_seq_length,
        fine_tune_type: p.training_type.clone(),
        base_prep: MlxBasePrep {
            model_type_override: p.mlx_model_type.clone(),
            chat_template: p.chat_template.clone(),
        },
    };

    if p.dry_run {
        return Ok(CommandResult::Json(json!({
            "dry_run": true,
            "engine": "mlx",
            "base_model_dir": spec.base_model_dir.display().to_string(),
            "data_dir": spec.data_dir.display().to_string(),
            "adapter_out": spec.adapter_out.display().to_string(),
            "rank": spec.rank,
            "scale": spec.scale,
            "num_layers": spec.num_layers,
            "iters": spec.iters,
            "fine_tune_type": spec.fine_tune_type,
        })));
    }

    let env = MlxTrainEnv {
        python: resolve_mlx_python(),
    };
    let out = run_mlx_train(&spec, &env)?;
    Ok(CommandResult::Json(json!({
        "dry_run": false,
        "engine": "mlx",
        "adapter_dir": out.adapter_dir.display().to_string(),
        "adapters_safetensors": out.adapters_safetensors.display().to_string(),
        "adapter_config": out.adapter_config.display().to_string(),
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
    /// Export format: "lora" (PEFT adapter — default), "gguf" (fused standalone
    /// model), or "gguf-lora" (the pageable gene `llama-server --lora` loads).
    #[serde(default = "default_export_format")]
    format: String,
    /// GGUF quantization (only when `format == "gguf"`): Q4_K_M, Q5_K_M, Q8_0, F16.
    #[serde(default = "default_quantization")]
    quantization: String,
    /// Base model id the GGUF LoRA composes onto — REQUIRED for `format ==
    /// "gguf-lora"` (the converter needs the base architecture). The same id the
    /// gateway serves the base with.
    #[serde(default)]
    base_model_id: Option<String>,
    /// GGUF LoRA adapter weight type (only when `format == "gguf-lora"`): "f16"
    /// (default) or "q8_0".
    #[serde(default = "default_lora_outtype")]
    outtype: String,
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
fn default_lora_outtype() -> String {
    "f16".to_string()
}

/// Resolve the organism's `format`/`quantization` params into the unsloth
/// custodian's genetic [`GenomeFormat`] outcome. Pure. Handles ONLY the
/// unsloth-served formats (`lora`, `gguf`); `gguf-lora` is served by the
/// continuum forge custodian over Contract C ([`run_export_gguf_lora`]) and is
/// dispatched away before reaching here — so this rejects it loudly as a guard
/// against a future caller routing it down the wrong (unsloth) path.
fn package_format(format: &str, quantization: &str) -> Result<GenomeFormat, String> {
    match format {
        "lora" => Ok(GenomeFormat::Lora { base_model_id: None }),
        "gguf" => Ok(GenomeFormat::Gguf { quantization: quantization.to_string() }),
        "gguf-lora" => Err(
            "format 'gguf-lora' is served by the forge custodian (Contract C), not this path \
             — route via run_export_gguf_lora"
                .to_string(),
        ),
        other => Err(format!(
            "unsupported export format: {other} (lora|gguf via unsloth; gguf-lora via custodian)"
        )),
    }
}

/// `forge/export` — name the genome OUTCOME and hand it to the custodian. ONE
/// call: the custodian owns the load → fuse → convert → quantize sequence (black
/// box). Fail-loud on any custodian error or a non-success envelope.
async fn run_export(
    custodian: &dyn ForgeCustodian,
    p: ForgeExportParams,
) -> Result<CommandResult, String> {
    let format = package_format(&p.format, &p.quantization)?;
    let result = custodian
        .package(&PackageRequest {
            checkpoint: p.checkpoint.clone(),
            save_directory: p.save_directory.clone(),
            format,
            max_seq_length: p.max_seq_length,
            load_in_4bit: p.load_in_4bit,
            push_to_hub: false,
            repo_id: None,
        })
        .await
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

/// `forge/export` for the `gguf-lora` outcome — served by the continuum forge
/// CUSTODIAN over Contract C ([`crate::forge::protocol`]), NOT unsloth (which
/// cannot produce a GGUF LoRA — the whole reason the custodian binary exists).
///
/// This is the corrected supply path for the genome page-in: it serializes the
/// STATELESS [`GgufLoraRequest`](crate::forge::protocol::GgufLoraRequest)
/// (checkpoint-in-body, no prior load-checkpoint, no hub fields), verifies the
/// custodian speaks our contract version at the `/health` handshake, and fails
/// LOUD on a missing base, an unreachable custodian, or a non-success envelope.
/// The emitted gene is exactly what `cognition/eval` pages into a live persona to
/// measure LIFT.
async fn run_export_gguf_lora(
    custodian: &dyn crate::forge::custodian_client::ForgeCustodian,
    p: &ForgeExportParams,
    manifest_path: &std::path::Path,
) -> Result<CommandResult, String> {
    // A GGUF LoRA with no base is meaningless — the converter needs the base
    // architecture. Reject loudly at the boundary, never silently default.
    let base_model_id = p.base_model_id.clone().ok_or_else(|| {
        "format 'gguf-lora' requires base_model_id — the converter needs the base \
         architecture to produce a loadable adapter"
            .to_string()
    })?;

    // Catch contract drift at the handshake, not as a malformed body deep in a
    // conversion (Contract C, R1/R2).
    custodian.ensure_contract().await.map_err(|e| e.to_string())?;

    let req = crate::forge::protocol::GgufLoraRequest {
        checkpoint: p.checkpoint.clone(),
        save_directory: p.save_directory.clone(),
        base_model_id: base_model_id.clone(),
        outtype: p.outtype.clone(),
    };
    let result = custodian
        .export_gguf_lora(&req)
        .await
        .map_err(|e| e.to_string())?;

    if !result.success {
        return Err(format!("custodian export (gguf-lora) failed: {}", result.message));
    }

    // Close the 5th wire: a produced gene that isn't REGISTERED is a silently-lost
    // gene — the serving daemon reads this manifest at (re)spawn to populate
    // `llama-server --lora`, so without this the catalog stays empty and every
    // page-in fails (LIFT structurally unreachable). The producer is the only
    // place that knows BOTH the on-disk path AND the continuum base_model_id the
    // serving daemon filters on (forge::adapter_manifest docs). Fail LOUD if the
    // gene can't be registered — never return success for an unloadable gene.
    let adapter = trained_adapter_from_export(&result.details, &base_model_id)?;
    crate::forge::adapter_manifest::register_at(manifest_path, adapter.clone())
        .map_err(|e| format!("gene exported but manifest registration failed: {e}"))?;

    Ok(CommandResult::Json(json!({
        "format": "gguf-lora",
        "checkpoint": p.checkpoint,
        "save_directory": p.save_directory,
        "message": result.message,
        "details": result.details,
        "registered": { "alias": adapter.alias, "path": adapter.path, "base_model_id": adapter.base_model_id },
    })))
}

/// Project the custodian's success `details` + the continuum `base_model_id` into a
/// [`TrainedAdapter`](crate::forge::adapter_manifest::TrainedAdapter) ready to
/// register. Pure (no IO) so the extraction is unit-testable without a custodian.
/// Fail LOUD if `details.output` is absent/empty — the custodian ALWAYS reports the
/// produced path on success (forge_custodian.rs), so a missing one is a contract
/// breach, not a gene we silently drop. Alias is the gene's file stem (logs only;
/// the per-request page-in resolver matches on `path`, never the alias).
fn trained_adapter_from_export(
    details: &Value,
    base_model_id: &str,
) -> Result<crate::forge::adapter_manifest::TrainedAdapter, String> {
    let output = details
        .get("output")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            "custodian export succeeded but `details.output` (the gene path) is \
             missing — cannot register an unlocatable gene"
                .to_string()
        })?;
    let path = std::path::PathBuf::from(output);
    let alias = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "gene".to_string());
    Ok(crate::forge::adapter_manifest::TrainedAdapter {
        alias,
        path,
        base_model_id: base_model_id.to_string(),
    })
}

/// `forge/health` — surface the local custodian's Contract C
/// [`HealthResponse`](crate::forge::protocol::HealthResponse) as a command result.
/// Taken over the trait (not the concrete HTTP client) so it is unit-testable with
/// a fake custodian. Fail-loud: an unreachable custodian surfaces as an error, not
/// a fabricated "healthy" — the honest reading is the whole point of the probe (R4).
async fn run_health(
    custodian: &dyn crate::forge::custodian_client::ForgeCustodian,
) -> Result<CommandResult, String> {
    let health = custodian.health().await.map_err(|e| e.to_string())?;
    let json = serde_json::to_value(health).map_err(|e| format!("forge/health: serialize: {e}"))?;
    Ok(CommandResult::Json(json))
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

    /// A process+tag-unique manifest path so parallel gguf-lora export tests never
    /// collide and never touch the real `~/.continuum` manifest (DI mirrors the
    /// `register` vs `register_at` split — no env globals, no test pollution).
    fn tmp_manifest(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("forge_export_manifest_{}_{}.json", tag, std::process::id()))
    }

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

    use crate::inference::unsloth_control::UnslothError;
    use crate::inference::unsloth_forge::{
        ExportResult, ForgeCustodian, ForgeTrainRequest, GenomeFormat, LoraCatalog, PackageRequest,
        TrainHandle, TrainStatus,
    };
    use std::sync::Mutex;

    /// Records what the organism asked the custodian to do, and returns scripted
    /// results — so we assert on the request shape without a network. The
    /// load→fuse→convert sequence is now BELOW the trait (an unsloth-impl detail),
    /// so the fake only sees the organism-facing `package` call.
    #[derive(Default)]
    struct RecordingCustodian {
        trains: Mutex<Vec<ForgeTrainRequest>>,
        packages: Mutex<Vec<PackageRequest>>,
        /// Custodian's package success flag (to exercise the fail-loud path).
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
        async fn package(&self, req: &PackageRequest) -> Result<ExportResult, UnslothError> {
            self.packages.lock().unwrap().push(req.clone());
            Ok(ExportResult { success: self.succeed, message: "packaged".into(), ..Default::default() })
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
            ..Default::default()
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
            ..Default::default()
        };
        assert!(!build_train_request(&p).use_lora);
    }

    // ── native MLX engine selection + spec resolution (#52) ──

    /// What this catches: engine selection is EXPLICIT-wins, never a silent
    /// fallback — "mlx"/"custodian" map deterministically, an unknown value
    /// fails loud (not silently treated as custodian), and `None` defers to
    /// the platform branch. A regression that swallowed a typo'd engine into
    /// the custodian path would be the exact "fallback hides the failure" bug.
    #[test]
    fn mlx_engine_selection_is_explicit_wins_fail_loud() {
        assert_eq!(mlx_engine_selected(Some("mlx")).unwrap(), true);
        assert_eq!(mlx_engine_selected(Some("custodian")).unwrap(), false);
        assert!(mlx_engine_selected(Some("tensorflow")).is_err(), "unknown engine must fail loud");
        // None → platform auto-detect; on this Apple-Silicon host that's mlx,
        // and the fn must agree with the same cfg! the dispatch uses.
        let want = cfg!(all(target_os = "macos", target_arch = "aarch64"));
        assert_eq!(mlx_engine_selected(None).unwrap(), want);
    }

    /// What this catches: the managed-dir invariant Joel set — the native
    /// trainer interpreter resolves under `~/.continuum/genome/venv`, NEVER
    /// the legacy `~/.unsloth/...` venv. Read-only (only asserts when
    /// MLX_PYTHON is unset, so it never races env-mutating tests).
    #[test]
    fn mlx_python_defaults_to_managed_venv_not_unsloth() {
        if std::env::var("MLX_PYTHON").is_ok() {
            return; // operator override in effect; nothing to assert
        }
        let p = resolve_mlx_python();
        let s = p.display().to_string();
        assert!(s.contains(".continuum/genome/venv"), "managed venv path, got {s}");
        assert!(!s.contains(".unsloth"), "must not reference the legacy unsloth venv, got {s}");
    }

    /// What this catches: the native dry_run RESOLVES the full MlxTrainSpec
    /// without spawning — proving (a) scale == lora_alpha/lora_r (the one
    /// geometry contract lora_convert reads back), (b) ~/ paths expand, (c)
    /// the defaults (num_layers=-1, engine=mlx) ride through. A drift in the
    /// scale math would silently reintroduce the scale=20 destabilization.
    #[test]
    fn native_dry_run_resolves_spec_with_scale_from_alpha_over_rank() {
        let home = std::env::var("HOME").unwrap_or_default();
        let p: ForgeTrainParams = serde_json::from_value(json!({
            "dataset_path": "/unused.jsonl",
            "base_model": "qwen3.5-4b-code-forged",
            "engine": "mlx",
            "train_base_dir": "~/base-hf",
            "data_dir": "~/data-presplit",
            "adapter_out": "~/out-adapter",
            "lora_r": 16,
            "lora_alpha": 32,
            "dry_run": true,
        })).expect("params");
        let v = match run_train_native_mlx(p).unwrap() {
            CommandResult::Json(v) => v,
            _ => panic!("json"),
        };
        assert_eq!(v["dry_run"], true);
        assert_eq!(v["engine"], "mlx");
        assert_eq!(v["scale"], 2.0, "scale must be alpha/rank = 32/16");
        assert_eq!(v["rank"], 16);
        assert_eq!(v["num_layers"], -1, "default = all transformer blocks");
        assert_eq!(v["base_model_dir"], format!("{home}/base-hf"));
        assert_eq!(v["data_dir"], format!("{home}/data-presplit"));
        assert_eq!(v["adapter_out"], format!("{home}/out-adapter"));
    }

    /// What this catches: the mlx engine REQUIRES train_base_dir and fails
    /// loud naming the train-base==serve-base reason — never guesses an
    /// on-disk dir (a guess would produce a washed-out ~0-lift gene).
    #[test]
    fn native_train_without_base_dir_fails_loud() {
        let p: ForgeTrainParams = serde_json::from_value(json!({
            "dataset_path": "/unused.jsonl",
            "base_model": "qwen3.5-4b-code-forged",
            "engine": "mlx",
            "dry_run": true,
        })).expect("params");
        let err = run_train_native_mlx(p).unwrap_err();
        assert!(err.contains("train_base_dir is required"), "got: {err}");
        assert!(err.contains("serve-base"), "must name the train==serve reason, got: {err}");
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
            ..Default::default()
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
            ..Default::default()
        };
        let v = match run_train(&cust, p).await.unwrap() {
            CommandResult::Json(v) => v,
            _ => panic!("json"),
        };
        assert_eq!(v["dry_run"], false);
        assert_eq!(v["job_id"], "job-1");
        assert_eq!(cust.trains.lock().unwrap().len(), 1);
    }

    // what this catches: export names the LoRA OUTCOME and hands the custodian ONE
    // package call carrying the checkpoint + save-dir handles. The load→export
    // sequencing now lives BELOW the trait (unsloth-impl detail), so the organism
    // only asserts it asked for the right genome form.
    #[tokio::test]
    async fn forge_export_lora_packages_as_lora() {
        let cust = RecordingCustodian::ok();
        let p = ForgeExportParams {
            checkpoint: "/ckpt".into(),
            save_directory: "/out".into(),
            format: "lora".into(),
            quantization: "Q4_K_M".into(),
            base_model_id: None,
            outtype: "f16".into(),
            max_seq_length: 2048,
            load_in_4bit: true,
        };
        let v = match run_export(&cust, p).await.unwrap() {
            CommandResult::Json(v) => v,
            _ => panic!("json"),
        };
        assert_eq!(v["format"], "lora");
        let pkgs = cust.packages.lock().unwrap();
        assert_eq!(pkgs.len(), 1, "exactly one package call");
        assert_eq!(pkgs[0].checkpoint, "/ckpt");
        assert_eq!(pkgs[0].save_directory, "/out");
        assert_eq!(pkgs[0].format, GenomeFormat::Lora { base_model_id: None });
    }

    // what this catches: the GGUF path threads the quantization method through into
    // the GenomeFormat (the grid can want a quantized standalone, not just the LoRA).
    #[tokio::test]
    async fn forge_export_gguf_threads_quantization() {
        let cust = RecordingCustodian::ok();
        let p = ForgeExportParams {
            checkpoint: "/ckpt".into(),
            save_directory: "/out".into(),
            format: "gguf".into(),
            quantization: "Q5_K_M".into(),
            base_model_id: None,
            outtype: "f16".into(),
            max_seq_length: 2048,
            load_in_4bit: true,
        };
        run_export(&cust, p).await.unwrap();
        let pkgs = cust.packages.lock().unwrap();
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].format, GenomeFormat::Gguf { quantization: "Q5_K_M".into() });
    }

    /// Records the Contract C ([`crate::forge::protocol`]) gguf-lora requests the
    /// organism sends to the forge CUSTODIAN — so we assert the stateless wire
    /// shape (checkpoint-in-body, base, outtype) without a network. Distinct from
    /// `RecordingCustodian` (the unsloth trait) because gguf-lora is a different
    /// daemon on a different contract.
    #[derive(Default)]
    struct RecordingForgeCustodian {
        exports: Mutex<Vec<crate::forge::protocol::GgufLoraRequest>>,
        succeed: bool,
    }
    impl RecordingForgeCustodian {
        fn ok() -> Self {
            Self { succeed: true, ..Default::default() }
        }
    }
    #[async_trait]
    impl crate::forge::custodian_client::ForgeCustodian for RecordingForgeCustodian {
        async fn health(
            &self,
        ) -> Result<
            crate::forge::protocol::HealthResponse,
            crate::forge::custodian_client::ForgeCustodianError,
        > {
            Ok(crate::forge::protocol::HealthResponse::ok_gguf_lora())
        }
        async fn export_gguf_lora(
            &self,
            req: &crate::forge::protocol::GgufLoraRequest,
        ) -> Result<
            crate::forge::protocol::ExportResult,
            crate::forge::custodian_client::ForgeCustodianError,
        > {
            self.exports.lock().unwrap().push(req.clone());
            // Mirror the real custodian's contract: a SUCCESS always reports the
            // produced gene path in `details.output` (forge_custodian.rs). The fake
            // derives the same `{save_dir}/{checkpoint_stem}-<job>.gguf` shape so the
            // producer's manifest registration has a real path to record.
            let ckpt_stem = std::path::Path::new(&req.checkpoint)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "gene".into());
            let output = format!("{}/{ckpt_stem}-testjob.gguf", req.save_directory);
            Ok(crate::forge::protocol::ExportResult {
                success: self.succeed,
                message: "packaged".into(),
                details: json!({ "output": output }),
            })
        }
    }

    // what this catches: the gguf-lora path sends the STATELESS Contract C request
    // (checkpoint named in body, base + outtype threaded) to the forge custodian —
    // this is the SUPPLY contract for the genome page-in (the gene cognition/eval
    // pages in). The pre-Pass-2 bug POSTed a checkpoint-less body to the WRONG
    // (unsloth) endpoint; a regression here = the gene is never produced loadably
    // and every LIFT is unmeasurable.
    #[tokio::test]
    async fn forge_export_gguf_lora_sends_stateless_contract_c_request() {
        let cust = RecordingForgeCustodian::ok();
        let p = ForgeExportParams {
            checkpoint: "/ckpt".into(),
            save_directory: "/out".into(),
            format: "gguf-lora".into(),
            quantization: "Q4_K_M".into(),
            base_model_id: Some("unsloth/Qwen2.5-0.5B-Instruct".into()),
            outtype: "f16".into(),
            max_seq_length: 2048,
            load_in_4bit: true,
        };
        let path = tmp_manifest("stateless");
        let _ = std::fs::remove_file(&path);
        run_export_gguf_lora(&cust, &p, &path).await.unwrap();
        let reqs = cust.exports.lock().unwrap();
        assert_eq!(reqs.len(), 1, "exactly one export call");
        assert_eq!(reqs[0].checkpoint, "/ckpt", "checkpoint named in the body (stateless)");
        assert_eq!(reqs[0].save_directory, "/out");
        assert_eq!(reqs[0].base_model_id, "unsloth/Qwen2.5-0.5B-Instruct");
        assert_eq!(reqs[0].outtype, "f16");
    }

    // what this catches: a gguf-lora export with NO base is rejected LOUDLY at the
    // organism boundary BEFORE the custodian is touched (the converter cannot make
    // a loadable adapter without the base architecture) — never silently defaulted.
    #[tokio::test]
    async fn forge_export_gguf_lora_without_base_fails_loud() {
        let cust = RecordingForgeCustodian::ok();
        let p = ForgeExportParams {
            checkpoint: "/ckpt".into(),
            save_directory: "/out".into(),
            format: "gguf-lora".into(),
            quantization: "Q4_K_M".into(),
            base_model_id: None,
            outtype: "f16".into(),
            max_seq_length: 2048,
            load_in_4bit: true,
        };
        let err = run_export_gguf_lora(&cust, &p, &tmp_manifest("nobase"))
            .await
            .expect_err("missing base must error");
        assert!(err.contains("base_model_id"), "got: {err}");
        assert_eq!(cust.exports.lock().unwrap().len(), 0, "custodian never called");
    }

    // what this catches: a custodian whose gguf-lora export fails is surfaced
    // LOUDLY (no silent no-op) — the same fail-loud contract as the unsloth path.
    #[tokio::test]
    async fn forge_export_gguf_lora_fails_loud_when_custodian_fails() {
        let cust = RecordingForgeCustodian::default(); // succeed=false
        let p = ForgeExportParams {
            checkpoint: "/ckpt".into(),
            save_directory: "/out".into(),
            format: "gguf-lora".into(),
            quantization: "Q4_K_M".into(),
            base_model_id: Some("b".into()),
            outtype: "f16".into(),
            max_seq_length: 2048,
            load_in_4bit: true,
        };
        let err = run_export_gguf_lora(&cust, &p, &tmp_manifest("custfail"))
            .await
            .expect_err("must error");
        assert!(err.contains("gguf-lora) failed"), "got: {err}");
    }

    // what this catches: the 5th wire of the genome loop — a SUCCESSFUL gguf-lora
    // export REGISTERS the produced gene in the manifest (path + continuum
    // base_model_id) so the serving daemon's reconcile loads it via `--lora`.
    // Without this the catalog stays empty forever and every page-in fails loud
    // (LIFT structurally unreachable). Asserts the gene is filterable by its
    // CONTINUUM id (what `for_base` keys on, not the HF base name).
    #[tokio::test]
    async fn forge_export_gguf_lora_registers_gene_in_manifest() {
        let cust = RecordingForgeCustodian::ok();
        let p = ForgeExportParams {
            checkpoint: "/ckpts/asha-code".into(),
            save_directory: "/genes".into(),
            format: "gguf-lora".into(),
            quantization: "Q4_K_M".into(),
            base_model_id: Some("continuum-ai/qwen3.5-4b-code-forged-GGUF".into()),
            outtype: "f16".into(),
            max_seq_length: 2048,
            load_in_4bit: true,
        };
        let path = tmp_manifest("registers");
        let _ = std::fs::remove_file(&path);

        run_export_gguf_lora(&cust, &p, &path).await.unwrap();

        let all = crate::forge::adapter_manifest::load_from(&path).unwrap();
        let matched =
            crate::forge::adapter_manifest::for_base(&all, "continuum-ai/qwen3.5-4b-code-forged-GGUF");
        assert_eq!(matched.len(), 1, "the produced gene is registered under its continuum id");
        // The fake mirrors the custodian's `{save_dir}/{ckpt_stem}-<job>.gguf` path.
        assert_eq!(matched[0].path, std::path::PathBuf::from("/genes/asha-code-testjob.gguf"));
        assert_eq!(matched[0].alias, "asha-code-testjob", "alias = gene file stem");

        let _ = std::fs::remove_file(&path);
    }

    // what this catches: a custodian SUCCESS with no `details.output` (the gene
    // path) FAILS LOUD — never returns success for a gene we can't locate, which
    // would silently register nothing and report a green export. The custodian
    // contract ALWAYS reports `output` on success, so its absence is a breach.
    #[test]
    fn trained_adapter_from_export_fails_loud_without_output() {
        let err = trained_adapter_from_export(&json!({ "job_id": "x" }), "continuum-ai/base")
            .expect_err("missing output must error");
        assert!(err.contains("details.output"), "got: {err}");
    }

    // ── forge/health — Contract C handshake as a command (Pass 6 receiving end) ──

    /// A Contract C custodian that is down — `health()` errors. Stands in for an
    /// unreachable local custodian so `run_health` is pinned to fail loud rather
    /// than fabricate a healthy reading.
    struct DownForgeCustodian;
    #[async_trait]
    impl crate::forge::custodian_client::ForgeCustodian for DownForgeCustodian {
        async fn health(
            &self,
        ) -> Result<
            crate::forge::protocol::HealthResponse,
            crate::forge::custodian_client::ForgeCustodianError,
        > {
            Err(crate::forge::custodian_client::ForgeCustodianError::Unreachable(
                "connection refused".into(),
            ))
        }
        async fn export_gguf_lora(
            &self,
            _: &crate::forge::protocol::GgufLoraRequest,
        ) -> Result<
            crate::forge::protocol::ExportResult,
            crate::forge::custodian_client::ForgeCustodianError,
        > {
            Err(crate::forge::custodian_client::ForgeCustodianError::Unreachable("down".into()))
        }
    }

    // what this catches: forge/health surfaces the LOCAL custodian's Contract C
    // HealthResponse as JSON — the reading a remote node reads over the grid to
    // confirm contract version + readiness before leasing a forge here (Pass 6
    // receiving end). A serialization regression would blind remote contract checks.
    #[tokio::test]
    async fn forge_health_surfaces_contract_c_health() {
        let cust = RecordingForgeCustodian::ok();
        let v = match run_health(&cust).await.unwrap() {
            CommandResult::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        };
        assert_eq!(v["contract_version"], crate::forge::protocol::CONTRACT_VERSION);
        assert_eq!(v["capability"], crate::forge::protocol::CAPABILITY_GGUF_LORA);
        assert_eq!(v["ready"], true);
    }

    // what this catches: an unreachable custodian makes forge/health FAIL LOUD with
    // the named cause — never a fabricated "healthy" envelope. A remote node must
    // see the truth (route elsewhere), not be told a down custodian is up.
    #[tokio::test]
    async fn forge_health_fails_loud_when_custodian_down() {
        let err = run_health(&DownForgeCustodian).await.expect_err("down custodian must error");
        assert!(err.contains("connection refused"), "got: {err}");
    }

    // what this catches: a custodian that fails the package is surfaced LOUDLY (no
    // silent no-op — the bug class #32 was opened to kill).
    #[tokio::test]
    async fn forge_export_fails_loud_when_custodian_fails() {
        let cust = RecordingCustodian::default(); // succeed=false
        let p = ForgeExportParams {
            checkpoint: "/ckpt".into(),
            save_directory: "/out".into(),
            format: "lora".into(),
            quantization: "Q4_K_M".into(),
            base_model_id: None,
            outtype: "f16".into(),
            max_seq_length: 2048,
            load_in_4bit: true,
        };
        let err = run_export(&cust, p).await.expect_err("package failure must error");
        assert!(err.contains("export (lora) failed"), "got: {err}");
    }

    // ── forge/probe — DISCOVER capability (self-organizing fabric primitive) ──

    /// A custodian that doesn't answer — every call errors. Stands in for an
    /// unreachable/down endpoint so we can pin that `probe` reports it truthfully.
    struct DeadCustodian;

    #[async_trait]
    impl ForgeCustodian for DeadCustodian {
        async fn train_start(&self, _: &ForgeTrainRequest) -> Result<TrainHandle, UnslothError> {
            Err(UnslothError::Api("down".into()))
        }
        async fn train_status(&self) -> Result<TrainStatus, UnslothError> {
            Err(UnslothError::Api("down".into()))
        }
        async fn package(&self, _: &PackageRequest) -> Result<ExportResult, UnslothError> {
            Err(UnslothError::Api("down".into()))
        }
        async fn list_loras(&self) -> Result<LoraCatalog, UnslothError> {
            Err(UnslothError::Api("down".into()))
        }
    }

    // what this catches: probe DISCOVERS a live custodian's state (reachable, its
    // run phase) by asking it — capability observed, not declared. This is the
    // reading a forge daemon routes grid demand against.
    #[tokio::test]
    async fn probe_reports_reachable_live_custodian() {
        let cust = RecordingCustodian::ok();
        let cap = cust.probe().await;
        assert!(cap.reachable, "an answering custodian is reachable");
        assert!(!cap.busy, "default status is not mid-run");
        assert_eq!(cap.phase, "training");
        assert_eq!(cap.held_genes, 0, "empty catalog → zero held genes");
    }

    // what this catches: an unreachable custodian probes to reachable:false — the
    // honest sensor reading, NOT a panic and NOT a silent pretend-healthy. A daemon
    // routes elsewhere on this (or fails loud if it's the last custodian).
    #[tokio::test]
    async fn probe_reports_unreachable_when_custodian_down() {
        let cap = DeadCustodian.probe().await;
        assert!(!cap.reachable, "a down custodian must read unreachable");
        assert!(!cap.busy);
        assert_eq!(cap.held_genes, 0);
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
