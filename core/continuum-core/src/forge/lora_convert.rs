//! MLX → PEFT LoRA conversion — the byte-exact transpose kernel of the genome
//! page-in bridge.
//!
//! Apple's `mlx_lm.lora` trainer (the Mac LoRA path, see memory
//! `unsloth-mlx-train-broken-on-mac`) writes an `adapters.safetensors` in MLX's
//! own key/shape convention. Every downstream consumer of a LoRA adapter
//! (`convert_lora_to_gguf.py`, PEFT, the genome page-in) speaks the HuggingFace
//! PEFT convention. The translation between them is a DETERMINISTIC key-rename +
//! 2-D transpose — proven live (memory `model-endpoint-fabric-adapter-router`):
//!
//! | MLX                                   | shape   | PEFT                                                      | shape   |
//! |---------------------------------------|---------|----------------------------------------------------------|---------|
//! | `model.layers.N.<mod>.lora_a`         | (in, r) | `base_model.model.model.layers.N.<mod>.lora_A.weight`    | (r, in) |
//! | `model.layers.N.<mod>.lora_b`         | (r, out)| `base_model.model.model.layers.N.<mod>.lora_B.weight`    | (out, r)|
//!
//! Both matrices transpose; the prefix gains `base_model.model.model.` and the
//! `lora_a`/`lora_b` suffix becomes `lora_A.weight`/`lora_B.weight`.
//!
//! This was a throwaway python script (`mlx_to_peft.py`); it is now a pure-Rust
//! kernel so step 1 of the bridge carries NO python (only llama.cpp's
//! `convert_lora_to_gguf.py` remains, and that is the custodian's, run as a
//! subprocess — see memory `no-python-in-rs-files`). The transpose is done in
//! raw byte space per element width, so it preserves the source dtype EXACTLY
//! (no silent F16→F32 widening — that would be a fidelity fallback).
//!
//! This is a custodian-side byte transform (it owns the produced bytes). It is
//! deliberately NOT reachable from `modules/forge.rs` (the organism command):
//! the organism declares the gene form it wants; a custodian process runs this.

use safetensors::tensor::TensorView;
use safetensors::{Dtype, SafeTensors};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// What the conversion produced, for the custodian's result envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeftConversion {
    /// The written `adapter_model.safetensors`.
    pub adapter_path: PathBuf,
    /// The written `adapter_config.json`.
    pub config_path: PathBuf,
    /// Number of tensors written (`lora_A` + `lora_B` for every target module).
    pub tensor_count: usize,
    /// The rank derived from (and verified against) the tensor shapes.
    pub rank: usize,
    /// The leaf target modules discovered (e.g. `q_proj`, `v_proj`), sorted.
    pub target_modules: Vec<String>,
}

/// Convert an MLX `adapters.safetensors` into a HuggingFace PEFT adapter
/// directory (`adapter_model.safetensors` + `adapter_config.json`).
///
/// `rank` is DERIVED from the tensor shapes and the caller's `expected_rank` is
/// verified against it — a mismatch fails loud (a wrong rank silently accepted
/// would poison the gene). `lora_alpha` is an explicit input, never defaulted:
/// it is a training hyperparameter the recipe owns (`alpha/r` == the page-in
/// scale), and guessing it would be a silent-fidelity fallback.
pub fn mlx_adapters_to_peft(
    mlx_safetensors: &Path,
    out_dir: &Path,
    base_model_name: &str,
    expected_rank: usize,
    lora_alpha: u32,
) -> Result<PeftConversion, String> {
    let data = std::fs::read(mlx_safetensors)
        .map_err(|e| format!("read MLX adapters {}: {e}", mlx_safetensors.display()))?;
    let tensors = SafeTensors::deserialize(&data)
        .map_err(|e| format!("parse MLX adapters {}: {e}", mlx_safetensors.display()))?;

    // Transposed bytes must outlive the TensorViews that borrow them.
    let mut converted: Vec<(String, Vec<usize>, Dtype, Vec<u8>)> = Vec::new();
    let mut target_modules: BTreeSet<String> = BTreeSet::new();
    let mut derived_rank: Option<usize> = None;

    for (name, view) in tensors.tensors() {
        // MLX keys look like `model.layers.N.<mod>.lora_a|lora_b`.
        let Some(body) = name.strip_prefix("model.") else {
            return Err(format!(
                "unexpected MLX tensor key {name:?} — every key must start with `model.`"
            ));
        };
        let (module_path, suffix) = body
            .rsplit_once('.')
            .ok_or_else(|| format!("MLX key {name:?} has no `.lora_a`/`.lora_b` suffix"))?;

        let shape = view.shape().to_vec();
        if shape.len() != 2 {
            return Err(format!(
                "LoRA tensor {name:?} is {}-D; only 2-D LoRA matrices are supported",
                shape.len()
            ));
        }
        let (rows, cols) = (shape[0], shape[1]);
        let dtype = view.dtype();
        let elem = elem_size(dtype)
            .ok_or_else(|| format!("LoRA tensor {name:?} has unsupported dtype {dtype:?}"))?;

        // PEFT key: `base_model.model.model.<module_path>.lora_{A,B}.weight`.
        let peft_suffix = match suffix {
            // MLX lora_a is (in, r) → PEFT lora_A is (r, in); rank = cols.
            "lora_a" => {
                note_rank(&mut derived_rank, cols, &name)?;
                "lora_A.weight"
            }
            // MLX lora_b is (r, out) → PEFT lora_B is (out, r); rank = rows.
            "lora_b" => {
                note_rank(&mut derived_rank, rows, &name)?;
                "lora_B.weight"
            }
            other => {
                return Err(format!(
                    "MLX key {name:?} has suffix {other:?}; expected `lora_a` or `lora_b`"
                ))
            }
        };
        let peft_key = format!("base_model.model.model.{module_path}.{peft_suffix}");

        // Leaf target module, e.g. `layers.0.self_attn.q_proj` → `q_proj`.
        if let Some(leaf) = module_path.rsplit('.').next() {
            target_modules.insert(leaf.to_string());
        }

        let transposed = transpose_2d(view.data(), rows, cols, elem);
        converted.push((peft_key, vec![cols, rows], dtype, transposed));
    }

    if converted.is_empty() {
        return Err(format!(
            "no LoRA tensors found in {} — not an MLX adapter?",
            mlx_safetensors.display()
        ));
    }
    let rank = derived_rank.expect("non-empty converted set always sets the rank");
    if rank != expected_rank {
        return Err(format!(
            "rank mismatch: tensors imply r={rank} but the recipe declared r={expected_rank}"
        ));
    }

    std::fs::create_dir_all(out_dir)
        .map_err(|e| format!("create out dir {}: {e}", out_dir.display()))?;

    let views: std::collections::HashMap<&str, TensorView> = converted
        .iter()
        .map(|(k, shape, dtype, bytes)| {
            let v = TensorView::new(*dtype, shape.clone(), bytes)
                .map_err(|e| format!("TensorView for {k:?}: {e}"))?;
            Ok((k.as_str(), v))
        })
        .collect::<Result<_, String>>()?;

    let adapter_path = out_dir.join("adapter_model.safetensors");
    safetensors::tensor::serialize_to_file(views, None, &adapter_path)
        .map_err(|e| format!("write {}: {e}", adapter_path.display()))?;

    let targets: Vec<String> = target_modules.into_iter().collect();
    let config = serde_json::json!({
        "peft_type": "LORA",
        "task_type": "CAUSAL_LM",
        "r": rank,
        "lora_alpha": lora_alpha,
        "lora_dropout": 0.0,
        "bias": "none",
        "fan_in_fan_out": false,
        "inference_mode": true,
        "target_modules": targets,
        "base_model_name_or_path": base_model_name,
    });
    let config_path = out_dir.join("adapter_config.json");
    std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&config).expect("static json never fails to serialize"),
    )
    .map_err(|e| format!("write {}: {e}", config_path.display()))?;

    Ok(PeftConversion {
        adapter_path,
        config_path,
        tensor_count: converted.len(),
        rank,
        target_modules: targets,
    })
}

/// Record the rank seen on one tensor; fail loud if two tensors disagree (a mix
/// of ranks in one adapter is corruption, not something to silently average).
fn note_rank(slot: &mut Option<usize>, rank: usize, name: &str) -> Result<(), String> {
    match slot {
        None => {
            *slot = Some(rank);
            Ok(())
        }
        Some(prev) if *prev == rank => Ok(()),
        Some(prev) => Err(format!(
            "inconsistent LoRA rank: {name:?} implies r={rank} but earlier tensors implied r={prev}"
        )),
    }
}

/// Byte-exact 2-D transpose: `out[j, i] = in[i, j]`, moving whole elements of
/// `elem` bytes. dtype-agnostic, so it preserves the source precision exactly.
fn transpose_2d(data: &[u8], rows: usize, cols: usize, elem: usize) -> Vec<u8> {
    let mut out = vec![0u8; data.len()];
    for i in 0..rows {
        for j in 0..cols {
            let src = (i * cols + j) * elem;
            let dst = (j * rows + i) * elem;
            out[dst..dst + elem].copy_from_slice(&data[src..src + elem]);
        }
    }
    out
}

/// Element width in bytes for the dtypes a LoRA adapter can carry.
fn elem_size(dtype: Dtype) -> Option<usize> {
    Some(match dtype {
        Dtype::F64 | Dtype::I64 | Dtype::U64 => 8,
        Dtype::F32 | Dtype::I32 | Dtype::U32 => 4,
        Dtype::F16 | Dtype::BF16 | Dtype::I16 | Dtype::U16 => 2,
        Dtype::I8 | Dtype::U8 | Dtype::BOOL => 1,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use safetensors::tensor::TensorView;

    /// Build a minimal MLX-style adapters.safetensors in a temp dir: one layer,
    /// one module (`q_proj`), F32, with lora_a (in=3, r=2) and lora_b (r=2,
    /// out=4) holding sentinel values we can check survive the transpose.
    fn write_mlx_fixture(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        // lora_a: shape (3,2), row-major [[0,1],[2,3],[4,5]]
        let a: Vec<f32> = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        // lora_b: shape (2,4), row-major [[0,1,2,3],[4,5,6,7]]
        let b: Vec<f32> = (0..8).map(|x| x as f32).collect();
        let a_bytes: Vec<u8> = a.iter().flat_map(|f| f.to_le_bytes()).collect();
        let b_bytes: Vec<u8> = b.iter().flat_map(|f| f.to_le_bytes()).collect();
        let views: std::collections::HashMap<&str, TensorView> = [
            (
                "model.layers.0.self_attn.q_proj.lora_a",
                TensorView::new(Dtype::F32, vec![3, 2], &a_bytes).unwrap(),
            ),
            (
                "model.layers.0.self_attn.q_proj.lora_b",
                TensorView::new(Dtype::F32, vec![2, 4], &b_bytes).unwrap(),
            ),
        ]
        .into_iter()
        .collect();
        let path = dir.join("adapters.safetensors");
        safetensors::tensor::serialize_to_file(views, None, &path).unwrap();
        path
    }

    fn read_f32_tensor(st: &SafeTensors, key: &str) -> (Vec<usize>, Vec<f32>) {
        let v = st.tensor(key).unwrap();
        let data: Vec<f32> = v
            .data()
            .chunks(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        (v.shape().to_vec(), data)
    }

    // what this catches: the FULL bridge contract in one shot — keys are renamed
    // to the PEFT convention, both matrices are transposed (shape AND element
    // order), rank is derived from shapes, target_modules is the leaf name, and
    // the config carries the explicit alpha + base. If any half of the transpose
    // regresses, the forged gene loads into llama.cpp wrong (or not at all) and
    // every genome-loop LIFT is silently corrupt. This is the page-in supply
    // kernel; it has to be exact.
    #[test]
    fn mlx_adapter_converts_to_peft_with_transposed_weights() {
        let tmp = std::env::temp_dir().join(format!("lora_convert_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let mlx = write_mlx_fixture(&tmp);
        let out = tmp.join("peft");

        let conv =
            mlx_adapters_to_peft(&mlx, &out, "unsloth/Qwen2.5-0.5B-Instruct", 2, 40).unwrap();

        assert_eq!(conv.rank, 2, "rank derived from shapes");
        assert_eq!(conv.tensor_count, 2);
        assert_eq!(conv.target_modules, vec!["q_proj".to_string()]);

        let data = std::fs::read(&conv.adapter_path).unwrap();
        let st = SafeTensors::deserialize(&data).unwrap();

        // lora_a (3,2) → lora_A (2,3); element [i,j] → [j,i].
        let (a_shape, a) = read_f32_tensor(
            &st,
            "base_model.model.model.layers.0.self_attn.q_proj.lora_A.weight",
        );
        assert_eq!(a_shape, vec![2, 3], "lora_A transposed to (r, in)");
        // original row-major (3,2) [[0,1],[2,3],[4,5]] transposed (2,3) = [[0,2,4],[1,3,5]]
        assert_eq!(a, vec![0.0, 2.0, 4.0, 1.0, 3.0, 5.0]);

        // lora_b (2,4) → lora_B (4,2).
        let (b_shape, b) = read_f32_tensor(
            &st,
            "base_model.model.model.layers.0.self_attn.q_proj.lora_B.weight",
        );
        assert_eq!(b_shape, vec![4, 2], "lora_B transposed to (out, r)");
        // (2,4) [[0,1,2,3],[4,5,6,7]] transposed (4,2) = [[0,4],[1,5],[2,6],[3,7]]
        assert_eq!(b, vec![0.0, 4.0, 1.0, 5.0, 2.0, 6.0, 3.0, 7.0]);

        // config carries the explicit alpha + derived target + base.
        let cfg: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&conv.config_path).unwrap()).unwrap();
        assert_eq!(cfg["r"], 2);
        assert_eq!(cfg["lora_alpha"], 40);
        assert_eq!(cfg["base_model_name_or_path"], "unsloth/Qwen2.5-0.5B-Instruct");
        assert_eq!(cfg["target_modules"], serde_json::json!(["q_proj"]));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    // what this catches: a declared rank that disagrees with the actual tensors
    // is rejected LOUDLY (a wrong rank silently accepted poisons the gene). This
    // is the fail-loud-not-fallback guard on the supply kernel.
    #[test]
    fn rank_mismatch_fails_loud() {
        let tmp =
            std::env::temp_dir().join(format!("lora_convert_rank_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let mlx = write_mlx_fixture(&tmp);
        let out = tmp.join("peft");

        // tensors imply r=2; declare r=8.
        let err = mlx_adapters_to_peft(&mlx, &out, "base", 8, 160)
            .expect_err("rank mismatch must error");
        assert!(err.contains("rank mismatch"), "got: {err}");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
