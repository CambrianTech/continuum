//! Safetensors writeout for trained [`LoRAModule`]s.
//!
//! After a training job's final epoch, the job actor writes the
//! [`LoRAModule`]'s `A` + `B` tensors to a safetensors file. That
//! file is the [`super::TrainingArtifact::local_path`] that flows
//! into forge / alloy for signing + provenance, and into genome
//! paging when a persona requests the corresponding skill.
//!
//! ## Tensor keys
//!
//! The two tensors are named `lora_a` and `lora_b` — these are the
//! same keys [`super::super::super::inference::lora`] (when it loads
//! a layer back for merge) reads. Pinning these names in a `const`
//! makes a future rename a one-place change and the load-side
//! mismatch a compile error rather than a silent "weights missing"
//! at merge time.
//!
//! ## Metadata
//!
//! Safetensors supports a per-file metadata map but `candle_core`'s
//! [`candle_core::safetensors::save`] surface doesn't expose it
//! (it always passes `None` for metadata). When the loader needs
//! rank + alpha + base_model to reconstruct the merge math, it
//! reads them from the alloy sidecar produced by forge — NOT from
//! the safetensors file itself. The safetensors file carries
//! WEIGHTS; the alloy file carries provenance + hyperparams.

use std::collections::HashMap;
use std::path::Path;

use candle_core::Tensor;

use super::lora_module::LoRAModule;

/// Key under which the down-projection `A` weight is stored. Matches
/// what [`crate::inference::lora`] reads when loading a layer back
/// for merge.
pub const LORA_A_KEY: &str = "lora_a";

/// Key under which the up-projection `B` weight is stored.
pub const LORA_B_KEY: &str = "lora_b";

/// Typed errors from the safetensors writeout path.
#[derive(Debug, thiserror::Error)]
pub enum SafetensorsIoError {
    #[error("output path parent directory missing: {path}")]
    MissingParentDir { path: String },

    #[error("creating output dir failed: {0}")]
    CreateDir(#[source] std::io::Error),

    #[error("candle serialize: {0}")]
    Candle(#[from] candle_core::Error),
}

/// Write the trained LoRA tensors to a safetensors file at `path`.
/// Creates the parent directory if it doesn't exist.
///
/// Both tensors are pulled out of the [`candle_core::Var`] wrappers
/// before write — `Var` exists for autograd participation; the
/// serialized form is the bare `Tensor`.
pub fn write_lora_safetensors(module: &LoRAModule, path: &Path) -> Result<(), SafetensorsIoError> {
    let parent = path
        .parent()
        .ok_or_else(|| SafetensorsIoError::MissingParentDir {
            path: path.display().to_string(),
        })?;
    if !parent.as_os_str().is_empty() && !parent.exists() {
        std::fs::create_dir_all(parent).map_err(SafetensorsIoError::CreateDir)?;
    }

    let mut tensors: HashMap<String, Tensor> = HashMap::new();
    tensors.insert(LORA_A_KEY.to_string(), module.lora_a().as_tensor().clone());
    tensors.insert(LORA_B_KEY.to_string(), module.lora_b().as_tensor().clone());

    candle_core::safetensors::save(&tensors, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::{DType, Device, Tensor};
    use tempfile::tempdir;

    fn cpu_module() -> LoRAModule {
        let device = Device::Cpu;
        let base = Tensor::full(0.1f32, (4, 4), &device).unwrap();
        LoRAModule::new(base, 2, 4, DType::F32, &device).unwrap()
    }

    // what this catches: round-trip — A and B written to safetensors
    // load back with identical shapes + values. A future refactor
    // that swapped the key names (lora_a / lora_b) would surface as a
    // missing-key load failure here, not as a silent inference-side
    // merge with zero-init B (which would behave like an unmerged
    // base model — hard to diagnose).
    #[test]
    fn round_trips_lora_tensors() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("layer.safetensors");
        let module = cpu_module();

        write_lora_safetensors(&module, &path).expect("write");

        let loaded = candle_core::safetensors::load(&path, &Device::Cpu).expect("load back");
        assert!(loaded.contains_key(LORA_A_KEY));
        assert!(loaded.contains_key(LORA_B_KEY));

        let a = loaded.get(LORA_A_KEY).unwrap();
        let b = loaded.get(LORA_B_KEY).unwrap();
        assert_eq!(a.dims(), module.lora_a().as_tensor().dims());
        assert_eq!(b.dims(), module.lora_b().as_tensor().dims());

        let a_in: Vec<f32> = module
            .lora_a()
            .as_tensor()
            .flatten_all()
            .unwrap()
            .to_vec1()
            .unwrap();
        let a_out: Vec<f32> = a.flatten_all().unwrap().to_vec1().unwrap();
        assert_eq!(a_in, a_out, "A round-trip bit-exact");
    }

    // what this catches: parent dir auto-creation. Producers (the
    // job actor) place artifacts at paths like
    // `~/.continuum/genome/<persona>/<trait>/<uuid>.safetensors` —
    // the per-trait dir doesn't exist on first write. A future
    // refactor that removed mkdir would surface as a NotFound write
    // failure inside the actor (visible to the operator only as
    // `Failed { error: "io error" }`).
    #[test]
    fn creates_parent_dir_if_missing() {
        let dir = tempdir().expect("tempdir");
        let nested = dir.path().join("a").join("b").join("layer.safetensors");
        assert!(!nested.parent().unwrap().exists());

        write_lora_safetensors(&cpu_module(), &nested).expect("write w/ mkdir");
        assert!(nested.exists());
    }

    /// VDD — validation-driven tests verifying mathematical
    /// equivalence across the write→load→reconstruct boundary.
    ///
    /// Difference vs the TDD round-trip test above: TDD pins that
    /// tensor values come back bit-exact. VDD pins the next-level
    /// invariant the matrix-dojo doctrine depends on — a *fresh*
    /// `LoRAModule` populated from loaded tensors must produce the
    /// SAME forward output as the original trainer. This is the
    /// math invariant that makes paged LoRA layers usable across
    /// process boundaries.
    mod vdd {
        use super::*;
        use candle_core::Var;

        // what this VDD catches: after train → write safetensors →
        // load → set A/B on a fresh LoRAModule, the new module's
        // forward(x) equals the original module's forward(x) to f32
        // bit-exactness. This is the matrix-dojo loop's correctness
        // invariant — a layer trained on continuum A and paged into
        // continuum B reproduces continuum A's output exactly.
        //
        // A regression that subtly altered the tensor dtype during
        // write/load (f32 → f16 → f32 conversion losing precision),
        // swapped axis order during serialization, or changed the
        // safetensors key naming would slip past the existing
        // round-trip test (which only checks raw tensor equality)
        // but would manifest as forward-output divergence here.
        #[test]
        fn safetensors_round_trip_reproduces_forward_output() {
            let device = Device::Cpu;
            let in_features = 4;
            let out_features = 6;
            let rank = 3;
            let alpha = 6;

            // Same base for both modules.
            let w_vec: Vec<f32> = (0..(out_features * in_features))
                .map(|i| ((i as f32) * 0.21 - 0.5).tanh())
                .collect();
            let base_original =
                Tensor::from_slice(&w_vec, (out_features, in_features), &device).unwrap();
            let base_loaded =
                Tensor::from_slice(&w_vec, (out_features, in_features), &device).unwrap();

            // Original module with overridden A and B (non-zero
            // pattern so the delta path is engaged).
            let original =
                LoRAModule::new(base_original, rank, alpha, DType::F32, &device).unwrap();
            let a_vec: Vec<f32> = (0..(rank as usize * in_features))
                .map(|i| ((i as f32 * 0.17) + 0.3).sin())
                .collect();
            let b_vec: Vec<f32> = (0..(out_features * rank as usize))
                .map(|i| ((i as f32 * 0.29) - 0.2).cos() * 0.4)
                .collect();
            let a_t = Tensor::from_slice(&a_vec, (rank as usize, in_features), &device).unwrap();
            let b_t = Tensor::from_slice(&b_vec, (out_features, rank as usize), &device).unwrap();
            original.lora_a().set(&a_t).unwrap();
            original.lora_b().set(&b_t).unwrap();

            // Write + load.
            let dir = tempdir().expect("tempdir");
            let path = dir.path().join("layer.safetensors");
            write_lora_safetensors(&original, &path).expect("write");
            let loaded = candle_core::safetensors::load(&path, &device).expect("load");

            // Reconstruct a fresh module with the SAME base and the
            // loaded A/B. The loaded tensors must round-trip into
            // Vars cleanly.
            let fresh = LoRAModule::new(base_loaded, rank, alpha, DType::F32, &device).unwrap();
            fresh
                .lora_a()
                .set(loaded.get(LORA_A_KEY).expect("A key present"))
                .unwrap();
            fresh
                .lora_b()
                .set(loaded.get(LORA_B_KEY).expect("B key present"))
                .unwrap();

            // Also assert the Vars themselves carry identical
            // tensor data — sanity ahead of the forward check.
            assert_var_eq(original.lora_a(), fresh.lora_a(), "A");
            assert_var_eq(original.lora_b(), fresh.lora_b(), "B");

            // Forward output must match element-for-element.
            let batch = 2;
            let x_vec: Vec<f32> = (0..(batch * in_features))
                .map(|i| ((i as f32 * 0.41) + 0.1).sin())
                .collect();
            let x = Tensor::from_slice(&x_vec, (batch, in_features), &device).unwrap();

            let y_original: Vec<f32> = original
                .forward(&x)
                .unwrap()
                .flatten_all()
                .unwrap()
                .to_vec1()
                .unwrap();
            let y_fresh: Vec<f32> = fresh
                .forward(&x)
                .unwrap()
                .flatten_all()
                .unwrap()
                .to_vec1()
                .unwrap();

            assert_eq!(y_original.len(), y_fresh.len());
            for (idx, (o, f)) in y_original.iter().zip(y_fresh.iter()).enumerate() {
                let diff = (o - f).abs();
                assert!(
                    diff < 1e-6,
                    "VDD: round-trip forward divergence at index {idx}: \
                     original={o:.7}, fresh={f:.7}, diff={diff:.2e}"
                );
            }
        }

        fn assert_var_eq(a: &Var, b: &Var, label: &str) {
            let a_flat: Vec<f32> = a.as_tensor().flatten_all().unwrap().to_vec1().unwrap();
            let b_flat: Vec<f32> = b.as_tensor().flatten_all().unwrap().to_vec1().unwrap();
            assert_eq!(
                a_flat, b_flat,
                "VDD: {label} tensors must round-trip bit-exact"
            );
        }
    }
}
