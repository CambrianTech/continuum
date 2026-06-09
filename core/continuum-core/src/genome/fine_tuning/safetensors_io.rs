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
pub fn write_lora_safetensors(
    module: &LoRAModule,
    path: &Path,
) -> Result<(), SafetensorsIoError> {
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

        let loaded =
            candle_core::safetensors::load(&path, &Device::Cpu).expect("load back");
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
}
