//! The files a training artifact IS, for an artifact that leaves the node that trained it
//! (card 3fd98325, S3 of the cross-grid training lease).
//!
//! A trainer's output directory holds more than its artifact: MLX saves intermediate
//! checkpoints (`0000300_adapters.safetensors`) beside the final weights. Returning "the
//! directory" would ship those, and returning "the path" names a file on another machine.
//! The format owns which files make the artifact, so every node agrees on what crosses.

use std::path::PathBuf;

use super::types::{ArtifactFormat, TrainingArtifact};

/// An artifact's files, each by its name within `root`: what a receiver rebuilds as
/// `<its own dir>/<name>` and hands to the same conversion a local artifact takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactPayload {
    pub root: PathBuf,
    pub names: Vec<String>,
}

impl ArtifactPayload {
    pub fn paths(&self) -> impl Iterator<Item = (&str, PathBuf)> {
        self.names.iter().map(|name| (name.as_str(), self.root.join(name)))
    }
}

impl TrainingArtifact {
    /// The files this artifact consists of. A GGUF LoRA is its one file; an MLX or PEFT
    /// adapter is its final weights and config (the converter reads both). A Candle skeleton
    /// or a provider-hosted model has no local gene to return, which is an answer, not a
    /// retry: nothing will ever make it returnable.
    pub fn payload(&self) -> Result<ArtifactPayload, String> {
        let path = self
            .local_path
            .clone()
            .ok_or_else(|| format!("artifact {} has no local weights to return", self.model_id))?;
        let dir = |names: &[&str]| ArtifactPayload { root: path.clone(), names: names.iter().map(|n| (*n).to_string()).collect() };
        match self.format {
            ArtifactFormat::GgufLora => {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or_else(|| format!("gguf artifact path {} names no file", path.display()))?
                    .to_string();
                let root = path
                    .parent()
                    .map(PathBuf::from)
                    .ok_or_else(|| format!("gguf artifact path {} has no directory", path.display()))?;
                Ok(ArtifactPayload { root, names: vec![name] })
            }
            ArtifactFormat::MlxAdapterDir => Ok(dir(&["adapters.safetensors", "adapter_config.json"])),
            ArtifactFormat::PeftAdapterDir => Ok(dir(&["adapter_model.safetensors", "adapter_config.json"])),
            ArtifactFormat::CandleSafetensors | ArtifactFormat::ProviderHosted => {
                Err(format!("a {:?} artifact has no local gene to return", self.format))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::fine_tuning::types::JobMetrics;

    fn artifact(format: ArtifactFormat, path: Option<&str>) -> TrainingArtifact {
        TrainingArtifact { model_id: "m".into(), local_path: path.map(PathBuf::from), format, metrics: JobMetrics::default() }
    }

    // what this catches (card 3fd98325): an artifact returned as "its directory" (MLX
    // intermediate checkpoints ride along) or as a bare path (a file on another machine), or
    // an unreturnable artifact answered as if a retry could help.
    #[test]
    fn an_artifact_returns_only_the_files_its_format_names() {
        let gguf = artifact(ArtifactFormat::GgufLora, Some("/jobs/j1/adapter.gguf")).payload().expect("gguf"); // JUSTIFIED: test asserts the payload
        assert_eq!(gguf, ArtifactPayload { root: "/jobs/j1".into(), names: vec!["adapter.gguf".into()] });

        let mlx = artifact(ArtifactFormat::MlxAdapterDir, Some("/jobs/j2/adapters")).payload().expect("mlx"); // JUSTIFIED: test asserts the payload
        assert_eq!(mlx.names, ["adapters.safetensors", "adapter_config.json"]);
        assert_eq!(mlx.paths().next().map(|(_, p)| p), Some(PathBuf::from("/jobs/j2/adapters/adapters.safetensors")));

        let peft = artifact(ArtifactFormat::PeftAdapterDir, Some("/jobs/j3/out")).payload().expect("peft"); // JUSTIFIED: test asserts the payload
        assert_eq!(peft.names, ["adapter_model.safetensors", "adapter_config.json"]);

        assert!(artifact(ArtifactFormat::ProviderHosted, Some("/x")).payload().is_err());
        assert!(artifact(ArtifactFormat::CandleSafetensors, Some("/x")).payload().is_err());
        assert!(artifact(ArtifactFormat::GgufLora, None).payload().is_err());
    }
}
