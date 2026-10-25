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
/// hashed, and the base the GGUF `base_gguf` finds for it on this node (`None` when it has
/// none), all off the async runtime (finding a GGUF walks directories, hashing it reads it).
/// `Err(Unreturnable)` is final; `Err(Unread)` may pass (a file or the base is unreadable now).
///
/// Only a GGUF LoRA is returned: its trainer read the base GGUF this node serves, so that
/// file's digest is the training base's identity. An MLX or PEFT trainer resolves an HF
/// snapshot when it starts and records no revision, so any revision read here afterwards
/// would be a guess; those stay here until their trainers record it.
pub(crate) async fn describe(
    artifact: &TrainingArtifact,
    base_model: &str,
    base_gguf: impl FnOnce() -> Option<PathBuf> + Send + 'static,
) -> Result<RecordedArtifact, DescribeError> {
    let payload = artifact.payload().map_err(DescribeError::Unreturnable)?;
    if artifact.format != ArtifactFormat::GgufLora {
        return Err(DescribeError::Unreturnable(format!(
            "a {:?} adapter's training base has no recorded source revision, so the origin could not verify it",
            artifact.format
        )));
    }
    let root = payload.root.clone();
    let base_id = base_model.to_string();
    let (files, (source_sha256, fingerprint)) = tokio::task::spawn_blocking(move || {
        let files = payload.paths().map(|(name, path)| hash_file(name, &path)).collect::<Result<Vec<_>, String>>()?;
        let gguf = base_gguf().ok_or_else(|| format!("no local GGUF of base {base_id} to identify"))?;
        let source = hash_file("base", &gguf)?.sha256;
        Ok::<_, String>((files, (source, lora_base_fingerprint(&gguf)?)))
    })
    .await
    .map_err(|e| DescribeError::Unread(format!("hashing task failed: {e}")))?
    .map_err(DescribeError::Unread)?;
    Ok(RecordedArtifact {
        root,
        artifact: LeaseArtifact {
            model_id: artifact.model_id.clone(),
            base: LeaseArtifactBase { model_id: base_model.to_string(), source_sha256, fingerprint },
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

    // what this catches (card 3fd98325, Codex on #5418): a report that names the trainer's
    // path or a digest that is not the bytes; a base identified only by its header, so a
    // fine-tuned sibling with the same architecture, tokenizer and shapes passes as the base;
    // an adapter whose training base was never recorded reported as verifiable.
    #[tokio::test]
    async fn a_completed_artifact_is_described_by_its_bytes_and_its_exact_base() {
        let dir = tempfile::tempdir().expect("dir"); // JUSTIFIED: test fixture
        let adapter = dir.path().join("adapter.gguf");
        std::fs::write(&adapter, b"lora").expect("write"); // JUSTIFIED: test fixture
        let (base, sibling) = (dir.path().join("base.gguf"), dir.path().join("sibling.gguf"));
        crate::commands::genome::test_support::write_base_gguf(&base, 0.0);
        crate::commands::genome::test_support::write_base_gguf(&sibling, 1.0);
        let artifact = TrainingArtifact { model_id: "m".into(), local_path: Some(adapter.clone()), format: ArtifactFormat::GgufLora, metrics: JobMetrics::default() };
        let at = |path: &Path| { let path = path.to_path_buf(); move || Some(path) };

        let recorded = describe(&artifact, "qwen", at(&base)).await.expect("described"); // JUSTIFIED: test asserts the manifest
        assert_eq!(recorded.root, dir.path());
        assert_eq!(recorded.artifact.files.len(), 1);
        assert_eq!(recorded.artifact.files[0].bytes, 4);
        assert_eq!(recorded.artifact.files[0].sha256, format!("{:x}", Sha256::digest(b"lora")));
        let wire = serde_json::to_string(&recorded.artifact).expect("encode"); // JUSTIFIED: test asserts the wire shape
        assert!(!wire.contains(&dir.path().display().to_string()), "the trainer's path never crosses");

        let on_sibling = describe(&artifact, "qwen", at(&sibling)).await.expect("described"); // JUSTIFIED: test asserts the manifest
        assert_eq!(recorded.artifact.base.fingerprint, on_sibling.artifact.base.fingerprint, "same header: compatible");
        assert_ne!(recorded.artifact.base.source_sha256, on_sibling.artifact.base.source_sha256, "other weights: another base");

        assert!(matches!(describe(&artifact, "qwen", || None).await, Err(DescribeError::Unread(e)) if e.contains("no local GGUF")));
        let mlx = TrainingArtifact { local_path: Some(dir.path().to_path_buf()), format: ArtifactFormat::MlxAdapterDir, ..artifact.clone() };
        assert!(matches!(describe(&mlx, "qwen", at(&base)).await, Err(DescribeError::Unreturnable(e)) if e.contains("source revision")));
        let hosted = TrainingArtifact { format: ArtifactFormat::ProviderHosted, ..artifact.clone() };
        assert!(matches!(describe(&hosted, "qwen", at(&base)).await, Err(DescribeError::Unreturnable(_))));
        std::fs::remove_file(&adapter).expect("remove"); // JUSTIFIED: test fixture
        assert!(matches!(describe(&artifact, "qwen", at(&base)).await, Err(DescribeError::Unread(_))));
    }
}
