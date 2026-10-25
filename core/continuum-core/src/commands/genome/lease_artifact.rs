//! A leased job's artifact as it is reported to the origin: its files by name, size and
//! SHA-256, never the trainer's path (card 3fd98325, S3-1 of the cross-grid training lease).
//!
//! The trainer's [`TrainingArtifact`] carries a `local_path` that exists only on this node.
//! Passing it through `genome/lease/poll` handed the origin a path to a file it does not have.
//! The manifest is computed once, when the job is first seen completed, and recorded on the
//! lease row: every later report, and every byte served, is checked against that one record.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::genome::fine_tuning::{ArtifactFormat, JobMetrics, TrainingArtifact};
use crate::inference_capability::gguf_fingerprint::lora_base_fingerprint;

/// One file of a returned artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/genome/LeaseArtifactFile.ts")]
#[serde(rename_all = "camelCase")]
pub struct LeaseArtifactFile {
    /// The file's name within the artifact (`adapters.safetensors`, `adapter.gguf`).
    pub name: String,
    #[ts(type = "number")]
    pub bytes: u64,
    /// Lowercase hex SHA-256 of the file's bytes.
    pub sha256: String,
}

/// The base an artifact fits: the registry id it was trained against and the digest of that
/// base's tokenizer and tensor shapes ([`lora_base_fingerprint`]), so the origin refuses an
/// adapter for a base that only shares a name with its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/genome/LeaseArtifactBase.ts")]
#[serde(rename_all = "camelCase")]
pub struct LeaseArtifactBase {
    pub model_id: String,
    pub fingerprint: String,
}

/// A completed leased job's artifact, as the origin sees it: what to pull and how to verify
/// it, in the trainer's own format (the origin converts and adopts it itself).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/genome/LeaseArtifact.ts")]
#[serde(rename_all = "camelCase")]
pub struct LeaseArtifact {
    pub model_id: String,
    /// The base it was trained on, as the origin checks it against its own copy.
    pub base: LeaseArtifactBase,
    pub format: ArtifactFormat,
    pub files: Vec<LeaseArtifactFile>,
    pub metrics: JobMetrics,
}

/// The manifest and where its files live HERE, kept on the lease row. The root never leaves
/// this node: it is what serving the bytes reads, under the recorded digests.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecordedArtifact {
    pub(crate) root: PathBuf,
    pub(crate) artifact: LeaseArtifact,
}

/// Describe a completed artifact trained on `base_model`: the files its format names, each
/// hashed, and the base's fingerprint from the GGUF `base_gguf` finds for it on this node
/// (`None` when it has none), all off the async runtime (finding a GGUF walks directories).
/// `Err(Unreturnable)` is final (no local gene exists); `Err(Unread)` may pass (a file or the
/// base GGUF is unreadable now).
pub(crate) async fn describe(
    artifact: &TrainingArtifact,
    base_model: &str,
    base_gguf: impl FnOnce() -> Option<PathBuf> + Send + 'static,
) -> Result<RecordedArtifact, DescribeError> {
    let payload = artifact.payload().map_err(DescribeError::Unreturnable)?;
    let root = payload.root.clone();
    let base_id = base_model.to_string();
    let (files, fingerprint) = tokio::task::spawn_blocking(move || {
        let files = payload.paths().map(|(name, path)| hash_file(name, &path)).collect::<Result<Vec<_>, String>>()?;
        let gguf = base_gguf().ok_or_else(|| format!("no local GGUF of base {base_id} to fingerprint"))?;
        Ok::<_, String>((files, lora_base_fingerprint(&gguf)?))
    })
    .await
    .map_err(|e| DescribeError::Unread(format!("hashing task failed: {e}")))?
    .map_err(DescribeError::Unread)?;
    Ok(RecordedArtifact {
        root,
        artifact: LeaseArtifact {
            model_id: artifact.model_id.clone(),
            base: LeaseArtifactBase { model_id: base_model.to_string(), fingerprint },
            format: artifact.format,
            files,
            metrics: artifact.metrics.clone(),
        },
    })
}

#[derive(Debug)]
pub(crate) enum DescribeError {
    /// The artifact has no local gene: nothing will make it returnable.
    Unreturnable(String),
    /// A file could not be read now.
    Unread(String),
}

fn hash_file(name: &str, path: &Path) -> Result<LeaseArtifactFile, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let bytes = std::io::copy(&mut file, &mut hasher).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(LeaseArtifactFile { name: name.to_string(), bytes, sha256: format!("{:x}", hasher.finalize()) })
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card 3fd98325): a report that names the trainer's path or skips a
    // file, a digest that is not the file's bytes, or an artifact with nothing to return
    // answered as retryable.
    #[tokio::test]
    async fn a_completed_artifact_is_described_by_name_size_and_digest() {
        let dir = tempfile::tempdir().expect("dir"); // JUSTIFIED: test fixture
        std::fs::write(dir.path().join("adapters.safetensors"), b"weights").expect("write"); // JUSTIFIED: test fixture
        std::fs::write(dir.path().join("adapter_config.json"), b"{}").expect("write"); // JUSTIFIED: test fixture
        std::fs::write(dir.path().join("0000300_adapters.safetensors"), b"checkpoint").expect("write"); // JUSTIFIED: test fixture
        let artifact = TrainingArtifact {
            model_id: "m".into(),
            local_path: Some(dir.path().to_path_buf()),
            format: ArtifactFormat::MlxAdapterDir,
            metrics: JobMetrics::default(),
        };

        let base = dir.path().join("base.gguf");
        crate::commands::genome::test_support::write_base_gguf(&base);
        let recorded = describe(&artifact, "qwen", { let base = base.clone(); move || Some(base) }).await.expect("described"); // JUSTIFIED: test asserts the manifest
        assert_eq!(recorded.artifact.base, LeaseArtifactBase { model_id: "qwen".into(), fingerprint: lora_base_fingerprint(&base).expect("digest") }); // JUSTIFIED: test asserts the base
        assert_eq!(recorded.root, dir.path());
        let names: Vec<_> = recorded.artifact.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["adapters.safetensors", "adapter_config.json"], "the intermediate checkpoint stays here");
        assert_eq!(recorded.artifact.files[0].bytes, 7);
        assert_eq!(recorded.artifact.files[0].sha256, format!("{:x}", Sha256::digest(b"weights")));
        let wire = serde_json::to_string(&recorded.artifact).expect("encode"); // JUSTIFIED: test asserts the wire shape
        assert!(!wire.contains(&dir.path().display().to_string()), "the trainer's path never crosses");

        assert!(matches!(describe(&artifact, "qwen", || None).await, Err(DescribeError::Unread(e)) if e.contains("no local GGUF")), "no base here to fingerprint");
        std::fs::remove_file(dir.path().join("adapter_config.json")).expect("remove"); // JUSTIFIED: test fixture
        assert!(matches!(describe(&artifact, "qwen", { let base = base.clone(); move || Some(base) }).await, Err(DescribeError::Unread(_))));
        let hosted = TrainingArtifact { format: ArtifactFormat::ProviderHosted, ..artifact };
        assert!(matches!(describe(&hosted, "qwen", move || Some(base)).await, Err(DescribeError::Unreturnable(_))));
    }
}
