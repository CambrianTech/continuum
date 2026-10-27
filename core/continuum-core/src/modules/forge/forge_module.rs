use super::*;

/// Holds the [`MessageBus`] (captured in `initialize`) so native training jobs can
/// EMIT `forge.train.*` lifecycle events onto airc — consumers SUBSCRIBE, never poll.
#[derive(Default)]
pub struct ForgeModule {
    pub(super) bus: RwLock<Option<Arc<MessageBus>>>,
}

impl ForgeModule {
    pub fn new() -> Self {
        Self::default()
    }

    /// The captured bus, if `initialize` has run. `None` in a bare unit test —
    /// [`crate::forge::mlx_job::spawn_train_job`] then simply skips the bus emit
    /// (the watch still updates, so `forge/train-status` stays honest).
    pub(super) fn bus(&self) -> Option<Arc<MessageBus>> {
        self.bus.read().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct ForgeRunParams {
    pub(super) recipe: ForgeRecipe,
    /// Hardware node label (e.g., "m5-pro@local", "rtx-5090@bigmama").
    /// Stub records this in the artifact's hardware_verified for trace
    /// purposes; Phase 5+ will actually dispatch to the named node.
    #[serde(default)]
    pub(super) hardware_node: Option<String>,
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
                // Native mlx_lm.lora on Apple Silicon — FIRE-AND-EMIT: returns a handle
                // immediately, `forge.train.*` events flow over airc (no poll). A
                // non-mlx engine routes to a grid-peer custodian (task #52 follow-up),
                // never a fallback ([[fallbacks-are-illegal-fail-loud]]).
                if mlx_engine_selected(parsed.engine.as_deref())? {
                    run_train_native_mlx(parsed, self.bus())
                } else {
                    Err(
                        "forge/train: native forge trains via mlx on Apple Silicon; a \
                         non-mlx engine must route to a grid-peer custodian (task #52 \
                         follow-up) — there is no fallback"
                            .to_string(),
                    )
                }
            }
            "forge/train-status" => {
                // A READ of the native job's last published watch value — the
                // fire-and-EMIT contract, never a poll against a gateway.
                let status = crate::forge::mlx_job::current_train_status();
                let json = serde_json::to_value(status)
                    .map_err(|e| format!("forge/train-status: serialize: {e}"))?;
                Ok(CommandResult::Json(json))
            }
            "forge/export" => {
                let parsed: ForgeExportParams = serde_json::from_value(params)
                    .map_err(|e| format!("forge/export: invalid params: {e}"))?;
                // gguf-lora is served by the continuum forge CUSTODIAN (Contract C),
                // the only thing that can turn a trained MLX adapter into a pageable
                // GGUF LoRA.
                // Different capabilities live on different daemons — so the format
                // picks the custodian here, at the one dispatch point.
                if parsed.format == "gguf-lora" {
                    // The default manifest path — the same file the serving daemon
                    // reads at (re)spawn. Resolve loud (a pathological missing HOME
                    // with no override fails here, not by writing to a surprise loc).
                    let manifest = crate::genome::adapter_manifest::manifest_path()?;
                    // The trinity split: the canonical base_model_id (→ manifest, eval)
                    // resolves IN-CORE to the safetensors hf_source (→ custodian convert).
                    let base_model_id = parsed.base_model_id.clone().ok_or_else(|| {
                        "format 'gguf-lora' requires base_model_id — the converter needs \
                         the base architecture to produce a loadable adapter"
                            .to_string()
                    })?;
                    let hf_base = crate::model_registry::artifacts::resolve_hf_source_for_model_id(
                        &base_model_id,
                    )?;
                    run_export_gguf_lora(
                        &ForgeCustodianHttp::from_config(),
                        &parsed,
                        &hf_base,
                        &manifest,
                    )
                    .await
                } else {
                    Err(format!(
                        "forge/export: native forge packages the pageable 'gguf-lora' gene \
                         (the unit the grid trades + llama-server pages in); format {:?} is \
                         not supported natively — no fallback",
                        parsed.format
                    ))
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
                // DISCOVER this node's native forge capability — the self-organizing
                // primitive the grid routes training demand against. Sourced from the
                // live job watch + the on-disk genome dir, no gateway.
                let cap = native_forge_capability();
                let json = serde_json::to_value(cap)
                    .map_err(|e| format!("forge/probe: serialize: {e}"))?;
                Ok(CommandResult::Json(json))
            }
            "forge/decide" => {
                let parsed: DecideParams = serde_json::from_value(params)
                    .map_err(|e| format!("forge/decide: invalid params: {e}"))?;
                Ok(CommandResult::Json(decide_assemble_or_train(&parsed)))
            }
            "forge/publish" => {
                // Publish a validated, lift-gated layer to the shared market. ACL
                // reserves this Owner-only (network publish is a consent-gated
                // action — the autonomous loop adopts LOCALLY, a human promotes to
                // the public market). Validation + the lift gate are enforced INSIDE
                // PublishRequest::build (#99 slice 2a); the destination is a
                // swappable Publisher adapter (HF today, grid tomorrow).
                let parsed: ForgePublishParams = serde_json::from_value(params)
                    .map_err(|e| format!("forge/publish: invalid params: {e}"))?;
                run_publish(parsed).await
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
pub(super) fn synthesize_stub_artifact(
    recipe: &ForgeRecipe,
    hardware_node: Option<&str>,
) -> Result<ForgeArtifact, String> {
    let now_ms = continuum_base::time::EpochMs::read().map_err(|e| format!("system time before epoch: {e}"))?.millis();

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
// The foundry's byte work — the train/valid split, fuse → convert → quantize,
// and the output BYTES — belongs to the custodian, not the organism. continuum
// holds POLICY (`forge/decide`) and HANDLES (a job id, a save dir). Closes the coordination↔learning flywheel: a persona's room work becomes a
// dataset (`dataset/from-turns`) → custodian fine-tunes a LoRA genome → it pages
// into the genome. See memory model-endpoint-fabric-adapter-router,
// compute-lease-boundary,
// coordination-learning-flywheel.
//=============================================================================
/// Inputs for `forge/train`. The dataset is the JSONL `dataset/from-turns`
/// emits; the base model + LoRA geometry come from a `ForgeRecipe` (or directly).
/// `dry_run` resolves the custodian request body and returns it WITHOUT kicking a
/// run — the wiring check, replacing the old engine `--dry-run` (the custodian
/// picks the engine, materializes the split, and owns the output bytes).
#[derive(Debug, Deserialize, Default)]
pub(super) struct ForgeTrainParams {
    /// Local training dataset path (the `dataset/from-turns` JSONL). The
    /// custodian reads it and owns the train/valid split.
    pub(super) dataset_path: String,
    /// Base model to fine-tune (the same id the gateway serves).
    pub(super) base_model: String,
    /// Training type — "lora" (the genome layer) by default.
    #[serde(default = "default_training_type")]
    pub(super) training_type: String,
    #[serde(default = "default_lora_r")]
    pub(super) lora_r: u32,
    #[serde(default = "default_lora_alpha")]
    pub(super) lora_alpha: u32,
    /// The custodian takes the learning rate as a STRING (e.g. "2e-4").
    #[serde(default = "default_learning_rate")]
    pub(super) learning_rate: String,
    #[serde(default = "default_batch_size")]
    pub(super) batch_size: u32,
    #[serde(default = "default_max_seq_length")]
    pub(super) max_seq_length: u32,
    /// Resolve the request + return it WITHOUT kicking a run.
    #[serde(default)]
    pub(super) dry_run: bool,

    // --- native MLX path (#52, Apple Silicon) -------------------------------
    // When the engine resolves to "mlx" the foundry owns the train subprocess
    // (forge::mlx_train) instead of delegating to a custodian. These
    // carry the bits the native contract needs that the custodian one doesn't.
    /// "mlx" | "custodian". `None` → auto-detect by platform (Apple Silicon →
    /// mlx, else custodian). Explicit value wins — never a silent fallback.
    pub(super) engine: Option<String>,
    /// Local HF safetensors dir to train against — the train base. REQUIRED for
    /// the mlx engine and asserted by the caller to be the SAME weights the
    /// gateway serves ([[genome-loop-trains-on-own-mistakes]]: train-base ==
    /// serve-base, or the LoRA washes out to ~0 lift). Fail loud if absent — the
    /// substrate never guesses which on-disk dir is the served model's HF form.
    pub(super) train_base_dir: Option<String>,
    /// Pre-split data dir holding `train.jsonl` + `valid.jsonl` (the mlx_lm
    /// `--data` contract). Alternative to `dataset_path` auto-split; takes
    /// precedence when set.
    pub(super) data_dir: Option<String>,
    /// Where to write the adapter (default `~/.continuum/forge/lora/<name>`).
    pub(super) adapter_out: Option<String>,
    /// Adapter name for the default output path + dataset split dir.
    #[serde(default = "default_adapter_name")]
    pub(super) adapter_name: String,
    /// mlx `--num-layers` (default -1 = all transformer blocks).
    #[serde(default = "default_num_layers")]
    pub(super) num_layers: i32,
    /// mlx `--iters` (the native trainer counts iterations, not epochs).
    #[serde(default = "default_native_iters")]
    pub(super) iters: u32,
    /// EXPLICIT base normalization: force `config.json` model_type to this mlx
    /// module name (e.g. `qwen3_5`). `None` leaves it alone.
    pub(super) mlx_model_type: Option<String>,
    /// EXPLICIT base normalization: add this chat_template when the tokenizer
    /// lacks one. `None` leaves it alone (mlx fails loud if a chat corpus then
    /// can't render).
    pub(super) chat_template: Option<String>,
    /// LoRA target module suffixes (mlx `lora_parameters.keys`). Defaults to the
    /// convert-safe MLP set (`mlp.{gate,up,down}_proj`) — the only modules whose
    /// LoRA factors llama.cpp can convert to GGUF-lora for qwen3.5 (attention
    /// `attn_qkv` factors hit `_reorder_v_heads` NotImplementedError). An EMPTY
    /// list (override) hands targeting to mlx_lm's own default — useful only for a
    /// base whose attention IS convertible. See `MlxTrainSpec::target_keys`.
    #[serde(default = "default_lora_target_keys")]
    pub(super) lora_target_keys: Vec<String>,
    /// mlx `--grad-checkpoint`: recompute activations in the backward pass rather
    /// than holding them all resident. Output-equivalent (identical gradients);
    /// trades ~20-30% compute for a large drop in peak Metal working-set. Default
    /// ON because on Apple-Silicon unified memory the working-set (∝ num_layers ×
    /// seq_len at backward), not total RAM, is the binding constraint — all-layer
    /// LoRA at a useful seq_len OOMs without it. See `MlxTrainSpec::grad_checkpoint`.
    #[serde(default = "default_grad_checkpoint")]
    pub(super) grad_checkpoint: bool,
}

pub(super) fn default_training_type() -> String {
    "lora".to_string()
}

pub(super) fn default_lora_r() -> u32 {
    16
}

pub(super) fn default_lora_alpha() -> u32 {
    16
}

pub(super) fn default_learning_rate() -> String {
    "2e-4".to_string()
}

pub(super) fn default_batch_size() -> u32 {
    1
}

pub(super) fn default_max_seq_length() -> u32 {
    2048
}

pub(super) fn default_adapter_name() -> String {
    "genome-lora".to_string()
}

pub(super) fn default_num_layers() -> i32 {
    -1
}

pub(super) fn default_grad_checkpoint() -> bool {
    true
}

pub(super) fn default_native_iters() -> u32 {
    300
}

/// The convert-safe LoRA target set for the genome loop: MLP projections only.
/// Attention (`attn_qkv`) LoRA factors are unconvertible to GGUF-lora for qwen3.5
/// (llama.cpp `_reorder_v_heads` NotImplementedError on a low-rank V factor), so a
/// gene must target these to be servable. Proven by the first servable gene
/// (`coder-4b-mlp`). Override with an empty list only for a convert-safe-attention
/// base.
pub(super) fn default_lora_target_keys() -> Vec<String> {
    vec![
        "mlp.gate_proj".to_string(),
        "mlp.up_proj".to_string(),
        "mlp.down_proj".to_string(),
    ]
}

/// True when the native MLX engine should own the train run for the given
/// explicit selector. Explicit `engine` wins; `None` auto-detects by platform
/// (Apple Silicon trains on mlx). This is a deterministic platform branch, not a
/// fallback — if mlx is selected but its env is missing, [`run_mlx_train`] fails
/// loud rather than silently delegating to the custodian.
pub(super) fn mlx_engine_selected(engine: Option<&str>) -> Result<bool, String> {
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
/// `~/.continuum/genome/venv`. Override via
/// `MLX_PYTHON` for a custom env. Existence + mlx_lm import are enforced loud by
/// [`run_mlx_train`]; provision the managed venv once with
/// `python3 -m venv ~/.continuum/genome/venv && ~/.continuum/genome/venv/bin/pip install mlx-lm`.
pub(super) fn resolve_mlx_python() -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("MLX_PYTHON") {
        return Ok(PathBuf::from(p));
    }
    crate::paths::ContinuumLayout::home()
        .map(|layout| layout.genome_venv_python())
        .map_err(|e| format!("no managed MLX venv: MLX_PYTHON is unset and the continuum home is unresolved ({e})"))
}

/// The on-disk root where native mlx runs write forged adapters
/// (`~/.continuum/forge/lora/<adapter_name>`) — matches `run_train_native_mlx`.
pub(super) fn native_genome_dir() -> String {
    std::env::var("HOME")
        .map(|h| format!("{h}/.continuum/forge/lora"))
        .unwrap_or_else(|_| ".continuum/forge/lora".to_string())
}

/// This node's native forge capability — the self-organizing primitive the grid
/// routes training demand against. OBSERVED (the live job watch + the on-disk
/// genome dir), never declared by config. `busy` reflects a run in flight;
/// `held_genes` counts the adapters already forged on this host.
pub(super) fn native_forge_capability() -> crate::forge::protocol::ForgeCapability {
    let s = crate::forge::mlx_job::current_train_status();
    let outputs_dir = native_genome_dir();
    let held_genes = std::fs::read_dir(&outputs_dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .count()
        })
        .unwrap_or(0);
    crate::forge::protocol::ForgeCapability {
        reachable: true,
        busy: s.is_training_running,
        phase: if s.phase.is_empty() {
            "idle".to_string()
        } else {
            s.phase
        },
        held_genes,
        outputs_dir,
    }
}

/// Read a JSONL file into one `Value` per non-blank line (fail loud on a
/// malformed line, naming the line number — never silently drop rows).
pub(super) fn read_jsonl(path: &Path) -> Result<Vec<Value>, String> {
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
/// delegating to a custodian (#52, converges #32). Resolves the train
/// base (asserted train-base==serve-base), materializes the `{train,valid}.jsonl`
/// data dir mlx_lm expects, and runs [`run_mlx_train`] with scale derived from
/// the LoRA geometry (`alpha/rank`) per [[genome-loop-trains-on-own-mistakes]].
/// `dry_run` returns the resolved spec WITHOUT spawning — the wiring check.
pub(super) fn run_train_native_mlx(
    p: ForgeTrainParams,
    bus: Option<Arc<MessageBus>>,
) -> Result<CommandResult, String> {
    use crate::genome::fine_tuning::mlx_train::{run_mlx_train, MlxBasePrep, MlxTrainEnv, MlxTrainSpec};

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
            let out = crate::modules::dataset::default_datasets_root()
                .map_err(|e| e.to_string())?
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
            let src = if eval_rows > 0 {
                &eval
            } else {
                &out.join("train.jsonl")
            };
            std::fs::copy(src, &valid)
                .map_err(|e| format!("materialize valid.jsonl from {}: {e}", src.display()))?;
            out
        }
    };

    // --- adapter output dir ---
    let adapter_out = match &p.adapter_out {
        Some(a) => PathBuf::from(crate::model_registry::expand_user_path(Path::new(a))),
        None => crate::paths::continuum_home()?
            .join("forge/lora")
            .join(&p.adapter_name),
    };

    // mlx scale == alpha/rank (lora_convert reads scale → alpha = rank*scale).
    // ONE geometry contract: set lora_alpha = 2*lora_r for the proven scale~2.
    if p.lora_r == 0 {
        return Err("forge/train (mlx): lora_r must be > 0".to_string());
    }
    let scale = p.lora_alpha as f64 / p.lora_r as f64;
    let learning_rate: f64 = p.learning_rate.parse().map_err(|e| {
        format!(
            "forge/train (mlx): learning_rate {:?}: {e}",
            p.learning_rate
        )
    })?;

    let spec = MlxTrainSpec {
        base_model_dir,
        data_dir,
        adapter_out: adapter_out.clone(),
        rank: p.lora_r,
        scale,
        dropout: 0.0,
        num_layers: p.num_layers,
        target_keys: p.lora_target_keys.clone(),
        batch_size: p.batch_size,
        iters: p.iters,
        learning_rate,
        max_seq_length: p.max_seq_length,
        grad_checkpoint: p.grad_checkpoint,
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
            "max_seq_length": spec.max_seq_length,
            "grad_checkpoint": spec.grad_checkpoint,
            "fine_tune_type": spec.fine_tune_type,
            "target_keys": spec.target_keys,
        })));
    }

    let env = MlxTrainEnv {
        python: resolve_mlx_python()?,
    };
    // FIRE-AND-EMIT: spawn the (blocking) mlx_lm.lora run as a tracked job on a
    // blocking thread; return the handle NOW. Lifecycle (training → completed/
    // failed) publishes over the watch (`forge/train-status` reads it) AND the airc
    // bus (`forge.train.*`) so the L3 completion sentinel + grid peers SUBSCRIBE —
    // no poll, never block the caller on a multi-minute train (also task #86).
    let job_id = format!("mlx-{}", p.adapter_name);
    let out_dir = adapter_out.display().to_string();
    crate::forge::mlx_job::spawn_train_job(job_id.clone(), bus, move |on_progress| {
        run_mlx_train(&spec, &env, on_progress)
    });
    Ok(CommandResult::Json(json!({
        "engine": "mlx",
        "jobId": job_id,
        "phase": "training",
        "message": "native mlx_lm.lora training started — subscribe to forge.train.done (no poll)",
        "adapterOut": out_dir,
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
/// dir (a `forge/train` output);
/// `save_directory` is where the custodian writes the packaged artifact
/// (custodian-owned). Both stay custodian-side — the organism passes handles.
#[derive(Debug, Deserialize)]
pub(super) struct ForgeExportParams {
    #[serde(default)]
    pub(super) checkpoint_format: crate::forge::protocol::AdapterCheckpointFormat,
    /// The trained checkpoint directory (loaded into the custodian's exporter).
    pub(super) checkpoint: String,
    /// Where the custodian writes the export (custodian-owned path).
    pub(super) save_directory: String,
    /// Export format: "lora" (PEFT adapter — default), "gguf" (fused standalone
    /// model), or "gguf-lora" (the pageable gene `llama-server --lora` loads).
    #[serde(default = "default_export_format")]
    pub(super) format: String,
    /// Base model id the GGUF LoRA composes onto — REQUIRED for `format ==
    /// "gguf-lora"` (the converter needs the base architecture). The same id the
    /// gateway serves the base with.
    #[serde(default)]
    pub(super) base_model_id: Option<String>,
    /// GGUF LoRA adapter weight type (only when `format == "gguf-lora"`): "f16"
    /// (default) or "q8_0".
    #[serde(default = "default_lora_outtype")]
    pub(super) outtype: String,
}

pub(super) fn default_export_format() -> String {
    "lora".to_string()
}

pub(super) fn default_lora_outtype() -> String {
    "f16".to_string()
}
