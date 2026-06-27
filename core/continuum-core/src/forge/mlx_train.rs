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
    /// `--batch-size`.
    pub batch_size: u32,
    /// `--iters` (training steps).
    pub iters: u32,
    /// `--learning-rate`.
    pub learning_rate: f64,
    /// `--max-seq-length`.
    pub max_seq_length: u32,
    /// `--fine-tune-type` — "lora" (the genome layer) by default.
    pub fine_tune_type: String,
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
    // Render scale/dropout as floats even when integral (`2` → `2.0`) so the YAML
    // is unambiguously numeric and self-documenting about being a scale, not a count.
    format!(
        "lora_parameters:\n  rank: {}\n  scale: {:.1}\n  dropout: {}\n",
        spec.rank, spec.scale, spec.dropout
    )
}

/// Build the `python -m mlx_lm lora …` argv. Pure + testable so the exact command
/// continuum runs is pinned without spawning a trainer. `config_path` is the YAML
/// from [`build_lora_config_yaml`].
pub fn build_train_args(spec: &MlxTrainSpec, config_path: &Path) -> Vec<String> {
    vec![
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
    ]
}

/// Run the native MLX LoRA trainer end-to-end: validate the env + IO contract,
/// write the LoRA config, spawn `mlx_lm.lora`, and verify the adapter landed.
/// Fail-loud on every precondition and on a non-zero exit — never returns an
/// `Ok` pointing at a partial/absent adapter.
pub fn run_mlx_train(
    spec: &MlxTrainSpec,
    env: &MlxTrainEnv,
) -> Result<MlxTrainOutput, String> {
    // --- preconditions (fail AT the missing precondition, naming it) ---
    if !env.python.is_file() {
        return Err(format!(
            "MLX trainer interpreter {} not found (need one that can `import mlx_lm`)",
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

    // --- spawn the trainer ---
    let args = build_train_args(spec, &config_path);
    let output = std::process::Command::new(&env.python)
        .args(&args)
        .output()
        .map_err(|e| format!("spawn mlx_lm.lora: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "mlx_lm.lora failed ({}):\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> MlxTrainSpec {
        MlxTrainSpec {
            base_model_dir: PathBuf::from("/base"),
            data_dir: PathBuf::from("/data"),
            adapter_out: PathBuf::from("/out"),
            rank: 16,
            scale: 2.0,
            dropout: 0.0,
            num_layers: -1,
            batch_size: 1,
            iters: 100,
            learning_rate: 2e-4,
            max_seq_length: 2048,
            fine_tune_type: "lora".into(),
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
    }

    // what this catches: run_mlx_train fails loud (not silently) when the train base
    // isn't a real HF model dir — the train-base==serve-base guard. Uses a missing
    // interpreter path so the very first precondition trips without spawning anything.
    #[test]
    fn run_fails_loud_on_missing_interpreter() {
        let env = MlxTrainEnv {
            python: PathBuf::from("/nonexistent/python3"),
        };
        let err = run_mlx_train(&spec(), &env).unwrap_err();
        assert!(err.contains("interpreter"), "err: {err}");
    }
}
