//! Native Apple-Silicon LoRA training — the OWNED train step of the genome loop
//! (#52, converging with #32). The downstream half of the pipeline already lives
//! in [`super::lora_convert`] (`mlx_to_gguf_lora`: MLX adapter → PEFT transpose →
//! GGUF-lora). This module owns the UPSTREAM half: producing that MLX adapter by
//! running Apple's `mlx_lm.lora` trainer as a Rust-owned subprocess, instead of
//! delegating the run to the unsloth HTTP custodian (the NVIDIA path).
//!
//! ## Why native, why here
//!
//! Per [[unsloth-mlx-train-broken-on-mac]] LoRA training works on this Mac via
//! `mlx_lm.lora` (Apple's native MLX trainer), and `forge/train` should route to
//! it on Apple Silicon. This is the Rust owner of that subprocess: it builds the
//! argv, writes the LoRA hyperparameter config, validates the IO, spawns, and
//! fails loud on any non-zero exit or missing artifact — never emitting a partial
//! adapter and calling it success (the same discipline `mlx_to_gguf_lora` already
//! follows for the convert step).
//!
//! ## The two invariants the genome loop demands ([[genome-loop-trains-on-own-mistakes]])
//!
//! 1. **train-base == serve-base.** The previous gene was INERT (lift ≈ 0.0)
//!    because the LoRA deltas were computed against `unsloth/Qwen3.5-4B` but
//!    applied to the *forged* served base — different weights, so the deltas
//!    washed out. The caller MUST pass [`MlxTrainSpec::base_model_dir`] = the HF
//!    safetensors form of the EXACT model the gateway serves (for the live
//!    persona base that is `continuum-ai/qwen3.5-4b-code-forged`, whose GGUF is
//!    what llama-server loads). This module does not guess the base — it trains
//!    on whatever trainable base it is handed, and the wiring is responsible for
//!    making it the served one.
//! 2. **scale ~2, not 20.** The prior checkpoints hardcoded `scale: 20.0`
//!    (PEFT `lora_alpha = rank * 20`), which over-amplifies the adapter and
//!    destabilises coherence. MLX stores `lora_parameters.scale` (== alpha/rank);
//!    [`read_mlx_lora_hparams`](super::lora_convert::read_mlx_lora_hparams) reads
//!    it back as `alpha = rank * scale`. So writing `scale: 2.0` here is exactly
//!    the sane amplification the convert step then carries into the GGUF-lora.
//!
//! ## No Python in .rs ([[no-python-in-rs-files]])
//!
//! This module spawns `python3 -m mlx_lm lora …` (a subprocess invocation, which
//! is sanctioned) and writes a YAML config file (YAML, not Python). There is no
//! inline Python source anywhere in this file.

use std::path::{Path, PathBuf};

/// Where the MLX trainer interpreter lives. Explicit (no hidden default): the
/// interpreter must be able to `import mlx_lm`, and guessing one would be a silent
/// locality fallback. Mirrors [`super::lora_convert::ConvertEnv`]'s discipline.
#[derive(Debug, Clone)]
pub struct MlxTrainEnv {
    /// Interpreter with `mlx_lm` importable (the Apple MLX env). Invoked as
    /// `<python> -m mlx_lm lora …`.
    pub python: PathBuf,
}

/// One LoRA training run's hyperparameters + IO paths. The base model dir MUST be
/// the trainable (HF safetensors) form of the SERVED base — see the module-level
/// `train-base == serve-base` invariant.
#[derive(Debug, Clone)]
pub struct MlxTrainSpec {
    /// HF safetensors dir of the base to fine-tune (= serve-base, trainable form).
    pub base_model_dir: PathBuf,
    /// Dir containing `train.jsonl` + `valid.jsonl` (mlx_lm's `--data` contract).
    pub data_dir: PathBuf,
    /// `--adapter-path`: where `adapters.safetensors` + `adapter_config.json` land.
    pub adapter_out: PathBuf,
    /// LoRA rank (`lora_parameters.rank`).
    pub rank: u32,
    /// LoRA scale (`lora_parameters.scale`, == alpha/rank). ~2.0, NEVER 20.
    pub scale: f64,
    /// LoRA dropout (`lora_parameters.dropout`).
    pub dropout: f64,
    /// `--num-layers` to fine-tune; `-1` = all layers.
    pub num_layers: i32,
    /// LoRA target module suffixes (`lora_parameters.keys`). EMPTY → mlx_lm's
    /// own default target set, which for qwen3.5 includes the fused attention
    /// projection (`attn_qkv`). That is the trap: a LoRA factor on the V slice of
    /// a fused QKV cannot be reordered HF→GGUF by llama.cpp's lora converter
    /// (`_reorder_v_heads` → `NotImplementedError` — the row size can't be
    /// reshaped on a low-rank factor), so an attention-targeting gene trains fine
    /// but is UNCONVERTIBLE → unservable. The genome loop therefore targets the
    /// MLP projections only (`mlp.{gate,up,down}_proj`), which carry the bulk of
    /// learnable skill and convert cleanly — the proven-good set the first
    /// servable gene (`coder-4b-mlp`) used. When the converter learns to reorder
    /// V-heads on a LoRA factor, this set can broaden; it is caller-supplied, not
    /// a hardcoded substrate constant.
    pub target_keys: Vec<String>,
    /// `--batch-size`.
    pub batch_size: u32,
    /// `--iters` (training steps).
    pub iters: u32,
    /// `--learning-rate`.
    pub learning_rate: f64,
    /// `--max-seq-length`.
    pub max_seq_length: u32,
    /// `--grad-checkpoint`: recompute layer activations in the backward pass
    /// instead of holding them all in memory. Output-EQUIVALENT (same gradients,
    /// recomputed not stored) — it trades ~20-30% compute for a large drop in
    /// peak working-set. On Apple-Silicon unified memory the binding constraint
    /// is the Metal command-buffer working set (peak ∝ num_layers × seq_len at
    /// the backward step), NOT total RAM — so all-layer LoRA at a useful seq_len
    /// OOMs without this even with most of system RAM free. Default ON for the
    /// genome loop (memory is the constraint, the weights are identical).
    pub grad_checkpoint: bool,
    /// `--fine-tune-type` — "lora" (the genome layer) by default.
    pub fine_tune_type: String,
    /// EXPLICIT normalizations a GGUF-published base's HF form may need before
    /// mlx_lm can train it (see [`prepare_base_for_mlx`]). Never guessed — the
    /// caller, who knows the served base, supplies them or the run fails loud.
    pub base_prep: MlxBasePrep,
}

/// Normalizations a GGUF-published model's HF safetensors form needs before
/// `mlx_lm.lora` can train it. Discovered live against
/// `continuum-ai/qwen3.5-4b-code-forged` — a model published primarily as GGUF,
/// whose HF form (the trainable serve-base) carries two mlx-incompatibilities:
///
///   1. **model_type dispatch.** mlx maps `config.json` `model_type` →
///      `mlx_lm.models.<model_type>`. The forged base declares
///      `qwen3_5_text` (the HF multimodal *text-tower* name), for which there is
///      no mlx module — but the base architecture itself is `qwen3_5`, which mlx
///      fully supports (hybrid linear/full attention included). So the type must
///      be rewritten to the mlx module name.
///   2. **chat_template.** The forged tokenizer ships no `chat_template`, so a
///      chat-`{messages}` corpus can't be rendered. The served GGUF uses ChatML;
///      the same template, supplied explicitly, lets mlx render the corpus.
///
/// Both are caller-supplied — the substrate NEVER infers an architecture nor
/// invents a template ([[no-hardcoded-heuristics-to-steer-cognition]] / fail-loud
/// doctrine). A `None` field means "leave it alone"; if mlx then can't dispatch
/// or render, mlx itself fails loud. The right long-run home for these is the
/// forge PUBLISH step (emit an already-mlx-trainable HF base); until then the
/// train step normalizes its own input.
#[derive(Debug, Clone, Default)]
pub struct MlxBasePrep {
    /// If set, force `config.json` `model_type` to this mlx module name (e.g.
    /// rewrite `qwen3_5_text` → `qwen3_5`). Idempotent: a no-op when already set.
    pub model_type_override: Option<String>,
    /// If set AND the tokenizer has no `chat_template`, write this Jinja template
    /// into `tokenizer_config.json`. Never overwrites an existing template.
    pub chat_template: Option<String>,
}

/// What a completed train run produced — the inputs `mlx_to_gguf_lora` consumes.
#[derive(Debug, Clone)]
pub struct MlxTrainOutput {
    /// The `--adapter-path` dir.
    pub adapter_dir: PathBuf,
    /// `adapters.safetensors` inside it.
    pub adapters_safetensors: PathBuf,
    /// `adapter_config.json` inside it.
    pub adapter_config: PathBuf,
}

/// The minimal mlx_lm config YAML carrying the LoRA hyperparameters that have no
/// direct CLI flag (`rank`/`scale`/`dropout`). Pure — unit-tested so the scale~2
/// invariant is pinned independently of a real run. Everything else rides as an
/// explicit CLI flag (see [`build_train_args`]).
pub fn build_lora_config_yaml(spec: &MlxTrainSpec) -> String {
    render_lora_parameters_yaml(spec.rank, spec.scale, spec.dropout, &spec.target_keys)
}

/// The pure `lora_parameters` YAML block — the ONE place the substrate encodes the
/// mlx_lm LoRA knobs that have NO CLI flag (`rank`/`scale`/`dropout`/`keys`). EVERY
/// training path renders through here — the forge primitive above AND the genome
/// [`crate::genome::fine_tuning`] `FineTuningAdapter` — so the scale~2 invariant
/// (NEVER mlx_lm's built-in default 20.0, which over-bakes a gene ~10×) holds
/// uniformly. Without a `-c` config carrying these, mlx_lm silently substitutes its
/// own defaults: that is exactly the bug this seam exists to prevent.
///
/// `keys` EMPTY → no `keys:` line, leaving mlx_lm on its own default target set
/// (the explicit "let the trainer decide" escape hatch); a non-empty set pins the
/// convert-safe targets (see [`MlxTrainSpec::target_keys`]).
pub fn render_lora_parameters_yaml(rank: u32, scale: f64, dropout: f64, keys: &[String]) -> String {
    // Render scale/dropout as floats even when integral (`2` → `2.0`) so the YAML
    // is unambiguously numeric and self-documenting about being a scale, not a count.
    let mut yaml = format!(
        "lora_parameters:\n  rank: {}\n  scale: {:.1}\n  dropout: {}\n",
        rank, scale, dropout
    );
    // Rendered as a YAML flow list of quoted suffixes (mlx_lm matches them against
    // module names).
    if !keys.is_empty() {
        let quoted = keys
            .iter()
            .map(|k| format!("\"{k}\""))
            .collect::<Vec<_>>()
            .join(", ");
        yaml.push_str(&format!("  keys: [{quoted}]\n"));
    }
    yaml
}

/// Build the `python -m mlx_lm lora …` argv. Pure + testable so the exact command
/// continuum runs is pinned without spawning a trainer. `config_path` is the YAML
/// from [`build_lora_config_yaml`].
pub fn build_train_args(spec: &MlxTrainSpec, config_path: &Path) -> Vec<String> {
    let mut args = vec![
        "-m".into(),
        "mlx_lm".into(),
        "lora".into(),
        "--model".into(),
        spec.base_model_dir.to_string_lossy().into_owned(),
        "--train".into(),
        "--data".into(),
        spec.data_dir.to_string_lossy().into_owned(),
        "--fine-tune-type".into(),
        spec.fine_tune_type.clone(),
        "--num-layers".into(),
        spec.num_layers.to_string(),
        "--batch-size".into(),
        spec.batch_size.to_string(),
        "--iters".into(),
        spec.iters.to_string(),
        "--learning-rate".into(),
        format!("{}", spec.learning_rate),
        "--max-seq-length".into(),
        spec.max_seq_length.to_string(),
        "--adapter-path".into(),
        spec.adapter_out.to_string_lossy().into_owned(),
        "-c".into(),
        config_path.to_string_lossy().into_owned(),
    ];
    // `--grad-checkpoint` is a presence flag (no value). Appended last so the
    // ordered-prefix assertions in the arg tests stay stable.
    if spec.grad_checkpoint {
        args.push("--grad-checkpoint".into());
    }
    args
}

/// Apply the EXPLICIT, caller-supplied [`MlxBasePrep`] normalizations to an HF
/// base dir in place, idempotently. Returns the list of changes made (empty when
/// the base was already mlx-ready) so the caller can log/inspect what it touched.
/// Pure JSON edits — no Python, no inference, no guessing. Fails loud only on a
/// genuinely malformed config (unreadable / unparseable JSON), never silently.
pub fn prepare_base_for_mlx(
    base_model_dir: &Path,
    prep: &MlxBasePrep,
) -> Result<Vec<String>, String> {
    let mut changes = Vec::new();

    // 1. model_type dispatch rewrite (e.g. qwen3_5_text → qwen3_5).
    if let Some(want) = &prep.model_type_override {
        let path = base_model_dir.join("config.json");
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        let mut cfg: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("parse {}: {e}", path.display()))?;
        let cur = cfg
            .get("model_type")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        if cur.as_deref() != Some(want.as_str()) {
            cfg["model_type"] = serde_json::Value::String(want.clone());
            let pretty = serde_json::to_string_pretty(&cfg)
                .map_err(|e| format!("serialize config.json: {e}"))?;
            std::fs::write(&path, pretty)
                .map_err(|e| format!("write {}: {e}", path.display()))?;
            changes.push(format!(
                "config.json model_type {:?} → {:?}",
                cur.as_deref().unwrap_or("(absent)"),
                want
            ));
        }
    }

    // 2. chat_template — only ADD when absent (never overwrite a real template).
    if let Some(template) = &prep.chat_template {
        let path = base_model_dir.join("tokenizer_config.json");
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        let mut cfg: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("parse {}: {e}", path.display()))?;
        let has = cfg
            .get("chat_template")
            .map(|v| !v.is_null())
            .unwrap_or(false);
        if !has {
            cfg["chat_template"] = serde_json::Value::String(template.clone());
            let pretty = serde_json::to_string_pretty(&cfg)
                .map_err(|e| format!("serialize tokenizer_config.json: {e}"))?;
            std::fs::write(&path, pretty)
                .map_err(|e| format!("write {}: {e}", path.display()))?;
            changes.push("tokenizer_config.json: added chat_template".to_string());
        }
    }

    Ok(changes)
}

/// Generous wall-clock TTL for the training job's governed VRAM/UMA lease (#56/G2).
/// A LoRA forge can run many minutes to hours; the RAII guard releases the moment
/// `run_mlx_train` returns (the child has exited by then). This TTL is ONLY the
/// self-healing backstop that returns the reservation to the board if the whole
/// PROCESS is SIGKILLed mid-train without running `Drop`.
const FORGE_TRAIN_LEASE_TTL_MS: u64 = 6 * 60 * 60 * 1000;

/// Multiplier on the base weight bytes to estimate a LoRA/DoRA training job's peak
/// UMA residency: the base is loaded resident, plus optimizer moments for the LoRA
/// params (small), forward/backward activations, AND the loss-step logits matrix —
/// `seq_len × vocab × f32`, which for a 128k-vocab model on the long multi-turn
/// trajectories genome/teach emits is GIGABYTES per sequence and dominates peak.
/// Receipt (2026-07-23): ×1.3 sized a Devstral-24B q4 job at ~17GB, the governor
/// granted it against 18GB available, and Metal OOM'd at `Calculating loss 0%` —
/// taking the LIVE serving lane down into a model-reload 503 with it. ×2.0 would
/// have refused that job, which is this estimate's contract: coarse-but-honest,
/// over-estimating spills the job to "wait", never OOM.
const TRAIN_FOOTPRINT_FACTOR: f64 = 2.0;

/// Estimate a training job's peak UMA footprint from the base model on disk: sum of
/// the base dir's `*.safetensors` bytes (≈ resident weight bytes) × [`TRAIN_FOOTPRINT_FACTOR`].
/// `None` when the dir has no safetensors to size against (an un-normalized or empty
/// base) — the caller treats that as "can't size, proceed ungoverned" rather than
/// blocking a job whose cost is unknown.
fn estimate_train_footprint_bytes(base_model_dir: &std::path::Path) -> Option<u64> {
    let mut weight_bytes = 0u64;
    let entries = std::fs::read_dir(base_model_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("safetensors") {
            if let Ok(meta) = std::fs::metadata(&path) {
                weight_bytes = weight_bytes.saturating_add(meta.len());
            }
        }
    }
    if weight_bytes == 0 {
        return None;
    }
    Some((weight_bytes as f64 * TRAIN_FOOTPRINT_FACTOR) as u64)
}

/// Ask the ONE resource authority for a training job's VRAM/UMA slot (#56/G2), the
/// forge sibling of the eval lane's `acquire_eval_lane_slot`. A `mlx_lm.lora` job
/// loads the FULL base into unified memory — on a UMA box that is VRAM contention
/// with the live persona lanes, and it used to launch with ZERO governor check
/// (invisible pressure that could OOM a live video-chat mid-forge). Now it leases:
///   granted  → hold the guard for the whole run so serving sees the bytes as taken
///   refused  → fail LOUD with the governed numbers (a background forge must not OOM
///              the machine — retry when pressure clears; the L3 flywheel re-attempts)
///   unsized / ungoverned → proceed unleased (can't size, or no daemon on this node)
fn acquire_train_slot(
    base_model_dir: &std::path::Path,
) -> Result<Option<crate::resources::LeaseGuard>, String> {
    use crate::resources::{LeaseError, LeaseRequest, ReclaimPolicy, ResourceDaemon, ResourceKind};
    let Some(daemon) = ResourceDaemon::global() else {
        return Ok(None); // ungoverned node — proceed unleased, behavior unchanged
    };
    let Some(footprint) = estimate_train_footprint_bytes(base_model_dir) else {
        crate::probe!(
            class = "forge.mlx_train.govern",
            base = %base_model_dir.display(),
            "could not size training footprint (no safetensors) — proceeding UNGOVERNED"
        );
        return Ok(None);
    };
    let req = LeaseRequest {
        consumer_id: "forge-train".to_string(),
        kind: ResourceKind::Vram,
        bytes: footprint,
        ttl_ms: FORGE_TRAIN_LEASE_TTL_MS,
        // A bare lease with no preemption handler must stay Pinned: the RAII guard is
        // the real release, so the board must never free the accounting while the
        // subprocess still holds the UMA. Real yield-under-pressure (checkpoint + pause
        // the forge when live serving needs the bytes) is the piece-3 follow-up.
        reclaim_policy: ReclaimPolicy::Pinned,
    };
    match daemon.acquire_guarded(&req) {
        Ok(guard) => {
            crate::probe!(
                class = "forge.mlx_train.govern",
                footprint_bytes = footprint,
                "governor granted a training lease"
            );
            Ok(Some(guard))
        }
        Err(LeaseError::InsufficientCapacity { available, requested, .. }) => Err(format!(
            "governor refused the training lease: needs {requested}B of VRAM/UMA but only \
             {available}B is available (live serving + other consumers hold the rest). \
             Refusing to launch mlx_lm.lora rather than OOM the machine mid-forge — free \
             VRAM (quit a game, let a benchmark lane finish) and retry."
        )),
        Err(e) => Err(format!(
            "governor lease error acquiring a training slot ({e:?}) — refusing to launch \
             ungoverned rather than risk an OOM"
        )),
    }
}

/// Run the native MLX LoRA trainer end-to-end: validate the env + IO contract,
/// normalize the base for mlx ([`prepare_base_for_mlx`]), write the LoRA config,
/// spawn `mlx_lm.lora`, and verify the adapter landed. Fail-loud on every
/// precondition and on a non-zero exit — never returns an `Ok` pointing at a
/// partial/absent adapter.
pub fn run_mlx_train(
    spec: &MlxTrainSpec,
    env: &MlxTrainEnv,
    on_progress: &(dyn Fn(u64, Option<f64>) + Sync),
) -> Result<MlxTrainOutput, String> {
    // --- preconditions (fail AT the missing precondition, naming it) ---
    if !env.python.is_file() {
        return Err(format!(
            "MLX trainer interpreter {} not found. Provision the continuum-managed venv: \
             `python3 -m venv ~/.continuum/genome/venv && \
             ~/.continuum/genome/venv/bin/pip install mlx-lm` (or set MLX_PYTHON to an env \
             that can `import mlx_lm`).",
            env.python.display()
        ));
    }
    let base_config = spec.base_model_dir.join("config.json");
    if !base_config.is_file() {
        return Err(format!(
            "train base {} has no config.json — not an HF model dir (train-base==serve-base \
             requires the served model's safetensors form)",
            spec.base_model_dir.display()
        ));
    }

    // Normalize the base for mlx BEFORE spawning (model_type dispatch +
    // chat_template). Caller-supplied + idempotent; a no-op when already ready.
    let prep_changes = prepare_base_for_mlx(&spec.base_model_dir, &spec.base_prep)?;
    for change in &prep_changes {
        crate::probe!(
            class = "forge.mlx_train.base_prep",
            change = %change,
            "normalized HF base for mlx_lm before training"
        );
    }

    let train_jsonl = spec.data_dir.join("train.jsonl");
    let valid_jsonl = spec.data_dir.join("valid.jsonl");
    if !train_jsonl.is_file() {
        return Err(format!(
            "data dir {} has no train.jsonl (mlx_lm --data contract)",
            spec.data_dir.display()
        ));
    }
    if !valid_jsonl.is_file() {
        return Err(format!(
            "data dir {} has no valid.jsonl — mlx_lm needs a validation split (use \
             split_and_write with a non-zero split, then rename eval.jsonl → valid.jsonl)",
            spec.data_dir.display()
        ));
    }
    if spec.fine_tune_type != "lora" && spec.fine_tune_type != "dora" {
        // `full` is intentionally rejected here: the genome loop trades pageable
        // LoRA/DoRA genes, not full-weight forks. Fail loud rather than silently
        // produce a non-pageable artifact the convert step can't handle.
        return Err(format!(
            "unsupported fine_tune_type {:?} — the genome loop forges pageable lora/dora genes",
            spec.fine_tune_type
        ));
    }

    std::fs::create_dir_all(&spec.adapter_out)
        .map_err(|e| format!("create adapter out dir {}: {e}", spec.adapter_out.display()))?;

    // --- write the LoRA hyperparameter config next to the adapter ---
    let config_path = spec.adapter_out.join("mlx_train_config.yaml");
    std::fs::write(&config_path, build_lora_config_yaml(spec))
        .map_err(|e| format!("write MLX train config {}: {e}", config_path.display()))?;

    // --- ask the ONE governor for this job's VRAM/UMA slot BEFORE spawning (#56/G2) ---
    // A `mlx_lm.lora` job loads the full base into unified memory; on a UMA box that is
    // real contention with the live persona lanes. Held for the WHOLE run (dropped when
    // this fn returns, AFTER `child.wait()` below — free the process, then the accounting)
    // so a concurrent serving tick sees the training bytes as taken and won't tier up into
    // them. Fails LOUD if the governor can't fit it — never OOM a live video-chat mid-forge.
    let _train_lease = acquire_train_slot(&spec.base_model_dir)?;

    // --- spawn the trainer, STREAMING stdout for live progress ---
    // mlx_lm.lora prints `Iter N: Train loss X, …` lines as it trains; we parse them
    // and publish step/loss to `on_progress` so `forge/train-status` shows the gene
    // forming in real time (the "watch it learn" glass box). stderr is drained on a
    // side thread (avoids a pipe-full deadlock) and kept for the failure message.
    let args = build_train_args(spec, &config_path);
    let mut child = std::process::Command::new(&env.python)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn mlx_lm.lora: {e}"))?;

    let stderr = child.stderr.take();
    let stderr_handle = std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = String::new();
        if let Some(mut s) = stderr {
            let _ = s.read_to_string(&mut buf);
        }
        buf
    });

    if let Some(stdout) = child.stdout.take() {
        use std::io::BufRead;
        for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some((step, loss)) = parse_mlx_progress(&line) {
                on_progress(step, loss);
            }
        }
    }

    let status = child.wait().map_err(|e| format!("wait mlx_lm.lora: {e}"))?;
    let stderr_out = stderr_handle.join().unwrap_or_default();
    if !status.success() {
        return Err(format!("mlx_lm.lora failed ({status}):\n{stderr_out}"));
    }

    // --- verify the adapter actually landed (no partial-success) ---
    let adapters = spec.adapter_out.join("adapters.safetensors");
    let adapter_config = spec.adapter_out.join("adapter_config.json");
    if !adapters.is_file() {
        return Err(format!(
            "mlx_lm.lora reported success but {} does not exist",
            adapters.display()
        ));
    }
    if !adapter_config.is_file() {
        return Err(format!(
            "mlx_lm.lora reported success but {} does not exist",
            adapter_config.display()
        ));
    }
    Ok(MlxTrainOutput {
        adapter_dir: spec.adapter_out.clone(),
        adapters_safetensors: adapters,
        adapter_config,
    })
}

/// Parse an mlx_lm.lora progress line into `(step, loss)`. mlx_lm prints e.g.
/// `Iter 10: Train loss 2.345, Learning Rate 1.000e-05, …`. Returns `None` for any
/// non-train-loss line (val lines, banners) — a parse miss is silent because
/// progress is best-effort; the terminal result on disk is the truth.
fn parse_mlx_progress(line: &str) -> Option<(u64, Option<f64>)> {
    let rest = line.trim().strip_prefix("Iter ")?;
    let (iter_str, after) = rest.split_once(':')?;
    let step: u64 = iter_str.trim().parse().ok()?;
    let loss = after
        .split_once("Train loss ")
        .and_then(|(_, tail)| tail.trim_start().split([',', ' ']).next())
        .and_then(|n| n.parse::<f64>().ok())?;
    Some((step, Some(loss)))
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the mlx_lm progress parser reads real step/loss from a
    // Train-loss line and returns None for Val-only / banner lines — so live
    // `forge/train-status` shows honest training progress and never a garbage step.
    #[test]
    fn parses_mlx_train_loss_lines_only() {
        assert_eq!(
            parse_mlx_progress("Iter 10: Train loss 2.345, Learning Rate 1.000e-05, It/sec 1.2"),
            Some((10, Some(2.345)))
        );
        assert_eq!(
            parse_mlx_progress("Iter 20: Val loss 2.1, Val took 3.4s"),
            None,
            "val-only lines carry no train loss → skipped"
        );
        assert_eq!(parse_mlx_progress("Starting training..."), None);
        assert_eq!(parse_mlx_progress("Iter oops: Train loss 1.0"), None);
    }

    // what this catches: #56/G2 — a training job's governed footprint is sized from the
    // base's on-disk safetensors × the activation/optimizer factor, so the governor can
    // refuse a forge that wouldn't fit alongside live serving instead of OOMing the box.
    // A dir with no safetensors sizes to None → "can't size, proceed ungoverned" (never
    // block a job whose cost is unknown); non-safetensors files are ignored.
    #[test]
    fn train_footprint_sums_safetensors_times_factor() {
        use std::io::Write;
        let dir = tempfile::tempdir().expect("tmp");
        // 1000 + 3000 bytes of "weights" across two shards, plus noise the sizer ignores.
        let mut a = std::fs::File::create(dir.path().join("model-00001.safetensors")).unwrap();
        a.write_all(&vec![0u8; 1000]).unwrap();
        let mut b = std::fs::File::create(dir.path().join("model-00002.safetensors")).unwrap();
        b.write_all(&vec![0u8; 3000]).unwrap();
        std::fs::write(dir.path().join("config.json"), b"{}").unwrap(); // ignored
        let est = estimate_train_footprint_bytes(dir.path()).expect("sizes");
        assert_eq!(est, (4000.0 * TRAIN_FOOTPRINT_FACTOR) as u64, "sum(safetensors) × factor");

        // No safetensors → None (can't size → proceed ungoverned, never block).
        let empty = tempfile::tempdir().unwrap();
        std::fs::write(empty.path().join("config.json"), b"{}").unwrap();
        assert_eq!(estimate_train_footprint_bytes(empty.path()), None);
    }

    fn spec() -> MlxTrainSpec {
        MlxTrainSpec {
            base_model_dir: PathBuf::from("/base"),
            data_dir: PathBuf::from("/data"),
            adapter_out: PathBuf::from("/out"),
            rank: 16,
            scale: 2.0,
            dropout: 0.0,
            num_layers: -1,
            target_keys: vec![
                "mlp.gate_proj".into(),
                "mlp.up_proj".into(),
                "mlp.down_proj".into(),
            ],
            batch_size: 1,
            iters: 100,
            learning_rate: 2e-4,
            max_seq_length: 2048,
            grad_checkpoint: true,
            fine_tune_type: "lora".into(),
            base_prep: MlxBasePrep::default(),
        }
    }

    // what this catches: the scale~2 invariant ([[genome-loop-trains-on-own-mistakes]])
    // — the config must carry scale 2.0 (not the old destabilising 20.0) and the
    // requested rank, rendered as a float so it reads as an amplification not a count.
    #[test]
    fn config_yaml_pins_scale_two_and_rank() {
        let y = build_lora_config_yaml(&spec());
        assert!(y.contains("rank: 16"), "yaml: {y}");
        assert!(y.contains("scale: 2.0"), "yaml: {y}");
        assert!(y.contains("dropout: 0"), "yaml: {y}");
    }

    // what this catches: the convert-safe target set is carried into the mlx config
    // as `lora_parameters.keys`. The genome loop MUST target MLP projections only —
    // an attention (`attn_qkv`) LoRA trains but is unconvertible to GGUF-lora
    // (llama.cpp's _reorder_v_heads NotImplementedError on a low-rank V factor), so
    // a gene that omitted these keys would be a dead-end gene. If this regressed to
    // not emitting keys, the loop would silently forge unservable attention genes.
    #[test]
    fn config_yaml_carries_convert_safe_mlp_keys() {
        let y = build_lora_config_yaml(&spec());
        assert!(y.contains("keys: ["), "yaml missing keys list: {y}");
        assert!(y.contains("\"mlp.gate_proj\""), "yaml: {y}");
        assert!(y.contains("\"mlp.up_proj\""), "yaml: {y}");
        assert!(y.contains("\"mlp.down_proj\""), "yaml: {y}");
        // The attention projection MUST NOT be a default target — that is the
        // unconvertible trap this whole key-set exists to avoid.
        assert!(!y.contains("attn_qkv"), "attention target leaked in: {y}");
    }

    // what this catches: an EMPTY target set leaves mlx_lm on its own default (no
    // `keys:` line) — the explicit "let the trainer decide" escape hatch, distinct
    // from the loop's convert-safe default. If this regressed to always emitting a
    // (possibly empty) keys list, mlx_lm would target nothing and train a no-op gene.
    #[test]
    fn config_yaml_omits_keys_when_target_set_empty() {
        let mut s = spec();
        s.target_keys.clear();
        let y = build_lora_config_yaml(&s);
        assert!(!y.contains("keys:"), "empty target set must omit keys: {y}");
    }

    // what this catches: the SHARED render seam the genome FineTuningAdapter emits
    // through (genome/fine_tuning/mlx_lora_adapter.rs::lora_config_yaml). Both paths
    // MUST land the same scale~2 invariant; a regression here that dropped the scale
    // line (or rounded it to an int count) would let mlx_lm fall back to its built-in
    // scale 20.0 on the genome path — the over-bake that served gibberish and got a
    // good gene wrongly rejected. Empty keys → no `keys:` line (genome default).
    #[test]
    fn render_seam_pins_scale_and_omits_empty_keys() {
        let y = render_lora_parameters_yaml(8, 2.0, 0.0, &[]);
        assert!(y.contains("rank: 8"), "yaml: {y}");
        assert!(y.contains("scale: 2.0"), "yaml: {y}");
        assert!(!y.contains("keys:"), "empty keys must omit the line: {y}");
    }

    // what this catches: the argv mirrors the mlx_lm.lora CLI contract — train mode,
    // the base model, the data dir, lora fine-tune type, and the config carrying the
    // LoRA hyperparameters. A drift in any flag name silently no-ops the trainer or
    // trains the wrong thing.
    #[test]
    fn train_args_carry_the_mlx_contract() {
        let cfg = PathBuf::from("/out/mlx_train_config.yaml");
        let a = build_train_args(&spec(), &cfg);
        let joined = a.join(" ");
        assert!(a.starts_with(&["-m".to_string(), "mlx_lm".to_string(), "lora".to_string()]));
        assert!(joined.contains("--train"));
        assert!(joined.contains("--model /base"));
        assert!(joined.contains("--data /data"));
        assert!(joined.contains("--fine-tune-type lora"));
        assert!(joined.contains("--adapter-path /out"));
        assert!(joined.contains("-c /out/mlx_train_config.yaml"));
        assert!(joined.contains("--num-layers -1"));
        // spec() defaults grad_checkpoint = true → the presence flag is appended.
        assert!(joined.contains("--grad-checkpoint"));
    }

    // what this catches: --grad-checkpoint is a PRESENCE flag gated on the spec bool —
    // emitted when on, absent when off (mlx_lm rejects `--grad-checkpoint false`, so it
    // must never appear with a value). The OOM fix depends on it being passed; turning it
    // off must drop it entirely, not pass a falsey value.
    #[test]
    fn grad_checkpoint_is_a_gated_presence_flag() {
        let cfg = PathBuf::from("/out/mlx_train_config.yaml");
        let mut off = spec();
        off.grad_checkpoint = false;
        assert!(!build_train_args(&off, &cfg).iter().any(|a| a == "--grad-checkpoint"));
        let mut on = spec();
        on.grad_checkpoint = true;
        let on_args = build_train_args(&on, &cfg);
        assert!(on_args.iter().any(|a| a == "--grad-checkpoint"));
        // never paired with a value token
        assert!(!on_args.iter().any(|a| a == "false" || a == "true"));
    }

    // what this catches: run_mlx_train fails loud (not silently) when the train base
    // isn't a real HF model dir — the train-base==serve-base guard. Uses a missing
    // interpreter path so the very first precondition trips without spawning anything.
    #[test]
    fn run_fails_loud_on_missing_interpreter() {
        let env = MlxTrainEnv {
            python: PathBuf::from("/nonexistent/python3"),
        };
        let err = run_mlx_train(&spec(), &env, &|_, _| {}).unwrap_err();
        assert!(err.contains("interpreter"), "err: {err}");
    }

    // what this catches: the two real normalizations the forged GGUF-form base
    // needed before mlx_lm could train it (proven live: model_type qwen3_5_text →
    // qwen3_5 dispatch + an added chat_template). prepare_base_for_mlx must rewrite
    // the model_type, ADD the missing template, report both changes, and be a no-op
    // on a second pass (idempotent) while never clobbering an existing template.
    #[test]
    fn prepare_base_normalizes_model_type_and_adds_chat_template() {
        let dir = std::env::temp_dir().join(format!(
            "mlx_prep_test_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.json"),
            r#"{"model_type":"qwen3_5_text","hidden_size":2560}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("tokenizer_config.json"),
            r#"{"eos_token":"<|im_end|>"}"#,
        )
        .unwrap();

        let prep = MlxBasePrep {
            model_type_override: Some("qwen3_5".into()),
            chat_template: Some("{{ TEMPLATE }}".into()),
        };
        let changes = prepare_base_for_mlx(&dir, &prep).unwrap();
        assert_eq!(changes.len(), 2, "expected both normalizations: {changes:?}");

        let cfg: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("config.json")).unwrap())
                .unwrap();
        assert_eq!(cfg["model_type"], "qwen3_5");
        // untouched fields survive the rewrite
        assert_eq!(cfg["hidden_size"], 2560);
        let tok: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join("tokenizer_config.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(tok["chat_template"], "{{ TEMPLATE }}");

        // second pass is a no-op (idempotent) and does NOT clobber the template
        let again = prepare_base_for_mlx(&dir, &prep).unwrap();
        assert!(again.is_empty(), "second pass should be a no-op: {again:?}");

        std::fs::remove_dir_all(&dir).ok();
    }
}
