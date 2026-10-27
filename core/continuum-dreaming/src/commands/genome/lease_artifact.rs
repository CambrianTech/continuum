//! A leased job's artifact as it is reported to the origin: its files by name, size and
//! SHA-256, never the trainer's path (card 3fd98325, S3-1 of the cross-grid training lease).
//!
//! The trainer's [`TrainingArtifact`] carries a `local_path` that exists only on this node.
//! Passing it through `genome/lease/poll` handed the origin a path to a file it does not have.
//! The manifest is computed once, when the job is first seen completed, and recorded on the
//! lease row: every later report, and every byte served, is checked against that one record.

use std::io::Seek;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::genome::fine_tuning::{ArtifactFormat, JobMetrics, TrainingArtifact};
use crate::inference::weights_stamp::StampRefusal;
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

/// The base an artifact was trained on: the registry id the lease named, the exact weights
/// the trainer read, and a compatibility digest. The origin requires `source_sha256` to equal
/// its own base's; the header digest alone cannot tell a base from a fine-tuned sibling with
/// the same architecture, tokenizer and shapes (Codex on #5418).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/genome/LeaseArtifactBase.ts")]
#[serde(rename_all = "camelCase")]
pub struct LeaseArtifactBase {
    pub model_id: String,
    /// SHA-256 of the whole base GGUF the adapter was trained against: its identity.
    pub source_sha256: String,
    /// [`lora_base_fingerprint`] of that GGUF: tokenizer and shapes, compatibility evidence only.
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
/// hashed, and the base weights its trainer's engine was launched on (`metrics.trained_on`, the
/// lane's stamp), all off the async runtime. The base is the file the trainer READ: it is
/// hashed only while its path still names the stamped file, never resolved from the registry
/// now (Codex on #5418). `Err(Unreturnable)` is final (no local gene, no recorded base, or the
/// base file is gone from its path); `Err(Unread)` may pass (a file is unreadable now).
///
/// Only a GGUF LoRA is returned: an MLX or PEFT trainer resolves an HF snapshot when it starts
/// and records no revision (card d3889a9e).
pub(crate) async fn describe(artifact: &TrainingArtifact, base_model: &str) -> Result<RecordedArtifact, DescribeError> {
    let payload = artifact.payload().map_err(DescribeError::Unreturnable)?;
    if artifact.format != ArtifactFormat::GgufLora {
        return Err(DescribeError::Unreturnable(format!(
            "a {:?} adapter's training base has no recorded source revision, so the origin could not verify it",
            artifact.format
        )));
    }
    let stamp = artifact.metrics.trained_on.clone().ok_or_else(|| {
        DescribeError::Unreturnable("the trainer recorded no base weights file, so the base it read cannot be named".into())
    })?;
    let root = payload.root.clone();
    let (files, (source_sha256, fingerprint)) = tokio::task::spawn_blocking(move || {
        let files = payload
            .paths()
            .map(|(name, path)| hash_file(name, &path))
            .collect::<Result<Vec<_>, String>>()
            .map_err(DescribeError::Unread)?;
        // Header and bytes through ONE handle proved to be the stamped file around both reads:
        // a base deleted or replaced (even by something that is not a GGUF) is final, never a
        // retryable read error (Codex on #5418).
        let base = stamp
            .read_verified(|file| {
                let fingerprint = lora_base_fingerprint(file)?;
                file.seek(std::io::SeekFrom::Start(0)).map_err(|e| e.to_string())?;
                let mut hasher = Sha256::new();
                std::io::copy(file, &mut hasher).map_err(|e| e.to_string())?;
                Ok((format!("{:x}", hasher.finalize()), fingerprint))
            })
            .map_err(|refusal| match refusal {
                StampRefusal::Changed(why) => DescribeError::Unreturnable(format!("the base the trainer read is gone: {why}")),
                StampRefusal::Unreadable(why) => DescribeError::Unread(why),
            })?;
        Ok::<_, DescribeError>((files, base))
    })
    .await
    .map_err(|e| DescribeError::Unread(format!("hashing task failed: {e}")))??;
    Ok(RecordedArtifact {
        root,
        artifact: LeaseArtifact {
            model_id: artifact.model_id.clone(),
            base: LeaseArtifactBase { model_id: base_model.to_string(), source_sha256, fingerprint },
            format: artifact.format,
            files,
            // The stamp names this node's path: the base crosses as its digests above.
            metrics: JobMetrics { trained_on: None, ..artifact.metrics.clone() },
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

    // what this catches (card 3fd98325, Codex on #5418): a report that names the trainer's
    // path or a digest that is not the bytes; a base named by whatever the path holds at poll
    // time (trained on A, the path replaced by a same-header sibling B before the first poll:
    // refused, never reported as B); an adapter whose base was never recorded, or whose trainer
    // records no source revision, reported as verifiable.
    #[tokio::test]
    async fn a_completed_artifact_is_described_by_its_bytes_and_the_base_its_trainer_read() {
        use crate::inference::weights_stamp::WeightsStamp;
        let dir = tempfile::tempdir().expect("dir"); // JUSTIFIED: test fixture
        let adapter = dir.path().join("adapter.gguf");
        std::fs::write(&adapter, b"lora").expect("write"); // JUSTIFIED: test fixture
        let (base, sibling) = (dir.path().join("base.gguf"), dir.path().join("sibling.gguf"));
        crate::commands::genome::test_support::write_base_gguf(&base, 0.0);
        crate::commands::genome::test_support::write_base_gguf(&sibling, 1.0);
        let stamp = WeightsStamp::of(&base).expect("stamp"); // JUSTIFIED: test fixture
        let artifact = TrainingArtifact {
            model_id: "m".into(),
            local_path: Some(adapter.clone()),
            format: ArtifactFormat::GgufLora,
            metrics: JobMetrics { trained_on: Some(stamp), ..JobMetrics::default() },
        };

        let recorded = describe(&artifact, "qwen").await.expect("described"); // JUSTIFIED: test asserts the manifest
        assert_eq!(recorded.root, dir.path());
        assert_eq!(recorded.artifact.files.len(), 1);
        assert_eq!(recorded.artifact.files[0].sha256, format!("{:x}", Sha256::digest(b"lora")));
        let base_bytes = std::fs::read(&base).expect("read"); // JUSTIFIED: test fixture
        assert_eq!(recorded.artifact.base.source_sha256, format!("{:x}", Sha256::digest(&base_bytes)));
        assert_eq!(recorded.artifact.base.fingerprint, lora_base_fingerprint(&mut std::fs::File::open(&sibling).expect("open")).expect("digest"), "a same-header sibling is compatible, never the same base"); // JUSTIFIED: test asserts the digest
        let wire = serde_json::to_string(&recorded.artifact).expect("encode"); // JUSTIFIED: test asserts the wire shape
        assert!(!wire.contains(&dir.path().display().to_string()), "neither the adapter's nor the base's path crosses");

        std::fs::rename(&sibling, &base).expect("replace"); // JUSTIFIED: test fixture
        assert!(
            matches!(describe(&artifact, "qwen").await, Err(DescribeError::Unreturnable(e)) if e.contains("gone")),
            "trained on A, the path now holds B: refused, never reported as B"
        );
        std::fs::write(&base, b"not a gguf").expect("write"); // JUSTIFIED: test fixture
        assert!(
            matches!(describe(&artifact, "qwen").await, Err(DescribeError::Unreturnable(e)) if e.contains("gone")),
            "replaced by something that is not a GGUF: final, never a retryable header error"
        );
        std::fs::remove_file(&base).expect("remove"); // JUSTIFIED: test fixture
        assert!(matches!(describe(&artifact, "qwen").await, Err(DescribeError::Unreturnable(e)) if e.contains("gone")), "deleted: final");

        let unstamped = TrainingArtifact { metrics: JobMetrics::default(), ..artifact.clone() };
        assert!(matches!(describe(&unstamped, "qwen").await, Err(DescribeError::Unreturnable(e)) if e.contains("no base weights")));
        let mlx = TrainingArtifact { local_path: Some(dir.path().to_path_buf()), format: ArtifactFormat::MlxAdapterDir, ..artifact.clone() };
        assert!(matches!(describe(&mlx, "qwen").await, Err(DescribeError::Unreturnable(e)) if e.contains("source revision")));
        let hosted = TrainingArtifact { format: ArtifactFormat::ProviderHosted, ..artifact.clone() };
        assert!(matches!(describe(&hosted, "qwen").await, Err(DescribeError::Unreturnable(_))));
        std::fs::remove_file(&adapter).expect("remove"); // JUSTIFIED: test fixture
        assert!(matches!(describe(&artifact, "qwen").await, Err(DescribeError::Unread(_))));
    }
}
