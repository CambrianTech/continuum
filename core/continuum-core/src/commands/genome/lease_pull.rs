//! The origin's half of returning a leased adapter (card 92cd2251, S3-2 of the cross-grid
//! training lease).
//!
//! A peer reports a completed lease by its recorded manifest ([`LeaseArtifact`]). The origin
//! pulls each file by ranges into a private partial file beside the job's request, verifies its
//! size and SHA-256 against the manifest, checks the base the adapter was trained on is the
//! base this node serves (by content), and only then places it. Its poll then answers a local
//! `Completed`, so the completion sentinel, the adoption and the examiner run unchanged on a
//! file that is this node's own.
//!
//! The pull runs off the poll (a poll only starts it and reads its progress), one per attempt.
//! A pull interrupted by a restart resumes from what its partial file holds; the whole file is
//! hashed before it is placed, so a resumed prefix is checked like any other byte.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine as _;
use dashmap::DashMap;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::grid_peer_adapter::LeaseTransport;
use super::lease::{LeaseArtifactChunk, LeaseArtifactParams, MAX_ARTIFACT_CHUNK};
use super::lease_artifact::LeaseArtifact;
use crate::genome::fine_tuning::{ArtifactFormat, TrainingArtifact, TrainingJobRequest, TrainingStatus};

/// A range of a peer's recorded artifact could not be had.
#[derive(Debug, Clone, PartialEq)]
pub enum RangeError {
    /// The peer has no such artifact, file or range, or the file changed there: final.
    Refused(String),
    /// It may be readable later.
    Transient(String),
}

/// Where a leased adapter's bytes come from: ranges of a peer's recorded artifact. Over airc
/// commands today ([`TransportSource`]); a stream transport is another implementation.
#[async_trait]
pub trait ArtifactSource: Send + Sync {
    async fn range(&self, peer: Uuid, attempt: Uuid, file: &str, offset: u64, len: u32) -> Result<Vec<u8>, RangeError>;
}

/// Ranges asked with `genome/lease/artifact` over the lease's own transport.
pub struct TransportSource(pub Arc<dyn LeaseTransport>);

#[async_trait]
impl ArtifactSource for TransportSource {
    async fn range(&self, peer: Uuid, attempt: Uuid, file: &str, offset: u64, len: u32) -> Result<Vec<u8>, RangeError> {
        let params = serde_json::to_value(LeaseArtifactParams { attempt, file: file.to_string(), offset, len }) // lease wire boundary
            .map_err(|e| RangeError::Refused(e.to_string()))?;
        let reply = self.0.call(peer, "genome/lease/artifact", params).await.map_err(RangeError::Transient)?;
        match serde_json::from_value::<LeaseArtifactChunk>(reply) { // lease wire boundary: the peer's answer
            Ok(LeaseArtifactChunk::Bytes { data }) => base64::engine::general_purpose::STANDARD
                .decode(data) // lease wire boundary: artifact bytes crossed the grid as JSON text
                .map_err(|e| RangeError::Refused(format!("peer {peer} sent bytes that are not base64: {e}"))),
            Ok(LeaseArtifactChunk::Refused { reason }) => Err(RangeError::Refused(format!("peer {peer}: {reason}"))),
            Ok(LeaseArtifactChunk::Unknown { reason }) => Err(RangeError::Transient(format!("peer {peer}: {reason}"))),
            Err(e) => Err(RangeError::Transient(format!("peer {peer} answered genome/lease/artifact unreadably: {e}"))),
        }
    }
}

/// This node's content digest of a base model's weights, as it would serve them.
pub type BaseDigest = Arc<dyn Fn(&str) -> Result<String, String> + Send + Sync>;

/// The base digest from the registry's GGUF of the base, hashed through one handle proved to be
/// the file it stamped (`WeightsStamp::read_verified`).
pub fn served_base_digest() -> BaseDigest {
    Arc::new(|base: &str| {
        let gguf = crate::model_registry::artifacts::resolve_gguf_for_model_id(base)
            .ok_or_else(|| format!("no local GGUF of base {base} to compare against"))?;
        let stamp = crate::inference::weights_stamp::WeightsStamp::of(&gguf).map_err(|e| format!("{}: {e}", gguf.display()))?;
        stamp
            .read_verified(|file| {
                let mut hasher = Sha256::new();
                std::io::copy(file, &mut hasher).map_err(|e| e.to_string())?;
                Ok(format!("{:x}", hasher.finalize()))
            })
            .map_err(|refusal| format!("{refusal:?}"))
    })
}

/// Where a pull stands.
#[derive(Debug, Clone)]
enum PullState {
    Running { done: u64, total: u64 },
    Placed(PathBuf),
    /// Final: the adapter as reported cannot be had here.
    Failed(String),
}

/// The origin's pulls, one per leased attempt.
pub struct LeasePull {
    source: Arc<dyn ArtifactSource>,
    base_digest: BaseDigest,
    /// The genome root job directories live under (`job_dir::genome_root` in production).
    root: Arc<dyn Fn() -> Option<PathBuf> + Send + Sync>,
    pulls: Arc<DashMap<Uuid, PullState>>,
}

impl LeasePull {
    pub fn new(source: Arc<dyn ArtifactSource>, base_digest: BaseDigest, root: Arc<dyn Fn() -> Option<PathBuf> + Send + Sync>) -> Self {
        Self { source, base_digest, root, pulls: Arc::new(DashMap::new()) }
    }

    /// Production: ranges over `transport`, the served base's digest, this node's genome root.
    pub fn on(transport: Arc<dyn LeaseTransport>) -> Self {
        Self::new(Arc::new(TransportSource(transport)), served_base_digest(), Arc::new(crate::genome::fine_tuning::job_dir::genome_root))
    }

    /// Record the leased job's request in its own directory, before the lease is sent: the
    /// directory a returned adapter lands in and the examiner reads what it trained on from.
    pub fn record_request(&self, attempt: Uuid, request: &TrainingJobRequest) -> Result<(), String> {
        let root = (self.root)().ok_or("this node has no genome root to record the lease's request in")?;
        let dir = crate::genome::fine_tuning::job_dir::job_dir_under(&root, &request.persona_name, &request.trait_kind, attempt);
        crate::genome::fine_tuning::job_dir::write_job_request(&dir, request)
    }

    /// What a peer's completed lease is HERE: still pulling (Running), placed and verified (a
    /// local Completed the sentinel adopts), or Failed. The first call starts the pull; a pull
    /// that met a transient error ends and the next call starts it again from its partial file.
    pub fn status(&self, peer: Uuid, attempt: Uuid, artifact: LeaseArtifact) -> TrainingStatus {
        let state = self.pulls.get(&attempt).map(|s| s.clone());
        match state {
            Some(PullState::Placed(path)) => TrainingStatus::Completed { artifact: local_artifact(&artifact, path) },
            Some(PullState::Failed(error)) => TrainingStatus::Failed { error },
            Some(PullState::Running { done, total }) => running(done, total),
            None => {
                let total = artifact.files.iter().map(|f| f.bytes).sum();
                self.pulls.insert(attempt, PullState::Running { done: 0, total });
                let (pulls, source, base_digest, root) = (self.pulls.clone(), self.source.clone(), self.base_digest.clone(), self.root.clone());
                tokio::spawn(async move {
                    let job = Pull { peer, attempt, artifact, source, pulls: pulls.clone() };
                    match job.run(&base_digest, root.as_ref()).await {
                        Ok(path) => {
                            pulls.insert(attempt, PullState::Placed(path));
                        }
                        Err(PullError::Final(why)) => {
                            continuum_base::probe!(class = "genome.lease.return_refused", peer = %peer, attempt = %attempt, reason = %why, "a leased adapter could not be returned here: final");
                            pulls.insert(attempt, PullState::Failed(why));
                        }
                        Err(PullError::Transient(why)) => {
                            continuum_base::probe!(class = "genome.lease.return_interrupted", peer = %peer, attempt = %attempt, reason = %why, "a leased adapter's pull stopped; the next poll resumes it");
                            pulls.remove(&attempt);
                        }
                    }
                });
                running(0, total)
            }
        }
    }
}

fn running(done: u64, total: u64) -> TrainingStatus {
    // Progress through the pull, after training finished: shown, never used to decide.
    let pct = if total == 0 { 100.0 } else { (done as f64 * 100.0 / total as f64) as f32 };
    TrainingStatus::Running { progress_pct: pct, current_epoch: 0 }
}

/// The returned adapter as this node's own artifact: a GGUF LoRA at its placed path.
fn local_artifact(artifact: &LeaseArtifact, path: PathBuf) -> TrainingArtifact {
    TrainingArtifact { model_id: artifact.model_id.clone(), local_path: Some(path), format: ArtifactFormat::GgufLora, metrics: artifact.metrics.clone() }
}

#[derive(Debug, PartialEq)]
enum PullError {
    Final(String),
    Transient(String),
}

struct Pull {
    peer: Uuid,
    attempt: Uuid,
    artifact: LeaseArtifact,
    source: Arc<dyn ArtifactSource>,
    pulls: Arc<DashMap<Uuid, PullState>>,
}

impl Pull {
    async fn run(&self, base_digest: &BaseDigest, root: &(dyn Fn() -> Option<PathBuf> + Send + Sync)) -> Result<PathBuf, PullError> {
        if self.artifact.format != ArtifactFormat::GgufLora || self.artifact.files.len() != 1 {
            return Err(PullError::Final(format!("a returned adapter is one GGUF LoRA file, not {:?} in {} files", self.artifact.format, self.artifact.files.len())));
        }
        let root = root().ok_or_else(|| PullError::Final("this node has no genome root".into()))?;
        let attempt = self.attempt;
        let dir = tokio::task::spawn_blocking(move || crate::genome::fine_tuning::job_dir::find_job_dir_under(&root, attempt))
            .await
            .map_err(|e| PullError::Transient(e.to_string()))?
            .ok_or_else(|| PullError::Final(format!("no job directory holds attempt {attempt}'s request: the lease was never recorded here")))?;
        // The base it was trained on must be the base this node serves, by content.
        let (digest, base) = (base_digest.clone(), self.artifact.base.model_id.clone());
        let ours = tokio::task::spawn_blocking(move || digest(&base)).await.map_err(|e| PullError::Transient(e.to_string()))?.map_err(PullError::Transient)?;
        if ours != self.artifact.base.source_sha256 {
            return Err(PullError::Final(format!(
                "trained on a base whose content ({}) is not this node's {} ({ours})",
                self.artifact.base.source_sha256, self.artifact.base.model_id
            )));
        }
        let file = &self.artifact.files[0];
        let returned = dir.join("returned");
        let placed = returned.join(&file.name);
        if file_digest(&placed).await.is_some_and(|sha| sha == file.sha256) {
            return Ok(placed);
        }
        let partial = returned.join(format!(".{}.partial", file.name));
        self.fill(&partial, file.bytes).await?;
        let sha = file_digest(&partial).await.ok_or_else(|| PullError::Transient(format!("{} unreadable after the pull", partial.display())))?;
        if sha != file.sha256 {
            let _ = tokio::fs::remove_file(&partial).await; // the bytes are wrong; nothing of them is kept
            return Err(PullError::Final(format!("{}'s bytes hash to {sha}, not the reported {}", file.name, file.sha256)));
        }
        let (from, to) = (partial.clone(), placed.clone());
        tokio::task::spawn_blocking(move || continuum_host::durable::replace(&from, &to))
            .await
            .map_err(|e| PullError::Transient(e.to_string()))?
            .map_err(|e| PullError::Transient(format!("{}: {e}", placed.display())))?;
        Ok(placed)
    }

    /// Append ranges to `partial` until it holds `total` bytes, resuming from what it holds.
    async fn fill(&self, partial: &Path, total: u64) -> Result<(), PullError> {
        use tokio::io::AsyncWriteExt;
        let io = |e: std::io::Error| PullError::Transient(format!("{}: {e}", partial.display()));
        if let Some(parent) = partial.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(io)?;
        }
        let mut out = tokio::fs::OpenOptions::new().create(true).append(true).open(partial).await.map_err(io)?;
        let mut done = out.metadata().await.map_err(io)?.len();
        if done > total {
            // More than the file holds: not a prefix of it. Start again.
            out.set_len(0).await.map_err(io)?;
            done = 0;
        }
        let name = &self.artifact.files[0].name;
        while done < total {
            let want = u32::try_from((total - done).min(u64::from(MAX_ARTIFACT_CHUNK))).map_err(|e| PullError::Final(e.to_string()))?;
            let bytes = self.source.range(self.peer, self.attempt, name, done, want).await.map_err(|e| match e {
                RangeError::Refused(why) => PullError::Final(why),
                RangeError::Transient(why) => PullError::Transient(why),
            })?;
            if bytes.len() != want as usize {
                return Err(PullError::Final(format!("peer {} sent {} bytes of {name} at {done}, asked {want}", self.peer, bytes.len())));
            }
            out.write_all(&bytes).await.map_err(io)?;
            done += u64::from(want);
            self.pulls.insert(self.attempt, PullState::Running { done, total });
        }
        out.sync_all().await.map_err(io)
    }
}

/// SHA-256 of the file at `path`, or `None` when it cannot be read.
async fn file_digest(path: &Path) -> Option<String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut file = std::fs::File::open(&path).ok()?;
        let mut hasher = Sha256::new();
        std::io::copy(&mut file, &mut hasher).ok()?;
        Some(format!("{:x}", hasher.finalize()))
    })
    .await
    .ok()
    .flatten()
}
