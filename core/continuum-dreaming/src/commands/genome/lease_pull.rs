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

/// The lease's binding, published once in its job directory: the anchor its directory is found
/// by, and what a retry of its attempt must match.
const LEASE_FILE: &str = "lease.json";
/// The request the examiner reads what a returned gene trained on from (`job_dir::read_job_request`).
const REQUEST_FILE: &str = "request.json";
/// The manifest the pull verifies against, pinned once at its start.
const MANIFEST_FILE: &str = "manifest.json";
/// The returned adapter's local name: fixed, so no name a peer sends ever reaches a path here.
const ADAPTER_FILE: &str = "adapter.gguf";

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LeaseBinding {
    peer: Uuid,
    attempt: Uuid,
    request_sha256: String,
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
    /// Placed and verified against the manifest pinned at the pull's start.
    Placed { path: PathBuf, pinned: Box<LeaseArtifact> },
    /// Final: the adapter as reported cannot be had here.
    Failed(String),
}

/// The origin's pulls, one per leased attempt.
///
/// Lifecycle invariant: ONE `LeasePull` writes a given genome root (production builds it once,
/// in `GridPeerAdapter::on_grid` at boot). Its per-attempt locks serialize recording and its map
/// single-flights pulls only among the callers of that one instance (Codex on #5419).
pub struct LeasePull {
    source: Arc<dyn ArtifactSource>,
    base_digest: BaseDigest,
    /// The genome root job directories live under (`job_dir::genome_root` in production).
    root: Arc<dyn Fn() -> Option<PathBuf> + Send + Sync>,
    pulls: Arc<DashMap<Uuid, PullState>>,
    /// One recording at a time per attempt: the scan for an existing directory and the publish
    /// are one step, so two first records under different personas cannot both win (Codex on
    /// #5419). One core runs per node, so an in-process lock covers every writer.
    records: DashMap<Uuid, Arc<std::sync::Mutex<()>>>,
}

impl LeasePull {
    pub fn new(source: Arc<dyn ArtifactSource>, base_digest: BaseDigest, root: Arc<dyn Fn() -> Option<PathBuf> + Send + Sync>) -> Self {
        Self { source, base_digest, root, pulls: Arc::new(DashMap::new()), records: DashMap::new() }
    }

    /// Production: ranges over `transport`, the served base's digest, this node's genome root.
    pub fn on(transport: Arc<dyn LeaseTransport>) -> Self {
        Self::new(Arc::new(TransportSource(transport)), served_base_digest(), Arc::new(crate::genome::fine_tuning::job_dir::genome_root))
    }

    /// Record the lease in the job's own directory, ONCE, before it is sent: its binding (peer,
    /// attempt, the request's digest) and the request itself, each published write-once. A retry
    /// of the same attempt is compared with what is recorded and refused when its payload, peer,
    /// persona or trait differs, so an accepted request is never rewritten and an attempt never
    /// has two directories (Codex on #5419).
    pub fn record_request(&self, peer: Uuid, attempt: Uuid, request: &TrainingJobRequest) -> Result<(), String> {
        use crate::genome::fine_tuning::job_dir::{find_job_dir_under, job_dir_under, publish_job_file};
        let root = (self.root)().ok_or("this node has no genome root to record the lease's request in")?;
        let lock = Arc::clone(self.records.entry(attempt).or_default().value());
        let _recording = lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner); // unwrap_or_else: a panicked recorder left files, which the compare below reads
        let dir = job_dir_under(&root, &request.persona_name, &request.trait_kind, attempt);
        if let Some(recorded) = find_job_dir_under(&root, attempt, LEASE_FILE)? {
            if recorded != dir {
                return Err(format!("attempt {attempt} is recorded for another persona or trait, at {}", recorded.display()));
            }
        }
        let request_bytes = serde_json::to_vec(request) // disk boundary: the request.json read_job_request reads back
            .map_err(|e| format!("the request could not be encoded: {e}"))?;
        let binding = LeaseBinding { peer, attempt, request_sha256: format!("{:x}", Sha256::digest(&request_bytes)) };
        let binding_bytes = serde_json::to_vec(&binding).map_err(|e| e.to_string())?; // disk boundary: the lease's binding
        if let Some(existing) = publish_job_file(&dir, LEASE_FILE, &binding_bytes)? {
            let recorded: LeaseBinding = serde_json::from_slice(&existing) // disk boundary: the binding recorded first
                .map_err(|e| format!("{}: unreadable lease binding: {e}", dir.display()))?;
            if recorded != binding {
                return Err(format!("attempt {attempt} was leased to {} with request {}; refused to {peer} with {}", recorded.peer, recorded.request_sha256, binding.request_sha256));
            }
        }
        // The binding names the request by digest: an existing request.json must be that request.
        match publish_job_file(&dir, REQUEST_FILE, &request_bytes)? {
            None => Ok(()),
            Some(existing) if format!("{:x}", Sha256::digest(&existing)) == binding.request_sha256 => Ok(()),
            Some(_) => Err(format!("{} holds another request than the one attempt {attempt} is bound to", dir.join(REQUEST_FILE).display())),
        }
    }

    /// What a peer's completed lease is HERE: still pulling (Running), placed and verified (a
    /// local Completed the sentinel adopts), or Failed. The first call starts the pull; a pull
    /// that met a transient error ends and the next call starts it again from its partial file.
    pub fn status(&self, peer: Uuid, attempt: Uuid, artifact: LeaseArtifact) -> TrainingStatus {
        // One pull per attempt: the start is decided under the map's entry lock, so two polls
        // at once start one writer (Codex on #5419).
        let state = match self.pulls.entry(attempt) {
            dashmap::mapref::entry::Entry::Occupied(held) => held.get().clone(),
            dashmap::mapref::entry::Entry::Vacant(empty) => {
                let total = artifact.files.iter().map(|f| f.bytes).sum();
                empty.insert(PullState::Running { done: 0, total });
                self.start(peer, attempt, artifact);
                return running(0, total);
            }
        };
        match state {
            PullState::Placed { path, pinned } => TrainingStatus::Completed { artifact: local_artifact(&pinned, path) },
            PullState::Failed(error) => TrainingStatus::Failed { error },
            PullState::Running { done, total } => running(done, total),
        }
    }

    fn start(&self, peer: Uuid, attempt: Uuid, artifact: LeaseArtifact) {
        let (pulls, source, base_digest, root) = (self.pulls.clone(), self.source.clone(), self.base_digest.clone(), self.root.clone());
        tokio::spawn(async move {
            let job = Pull { peer, attempt, artifact, source, pulls: pulls.clone() };
            match job.run(&base_digest, root.as_ref()).await {
                Ok((path, pinned)) => {
                    continuum_base::probe!(
                        class = "genome.lease.returned",
                        peer = %peer,
                        attempt = %attempt,
                        file_sha256 = %pinned.files.first().map_or("", |f| f.sha256.as_str()), // map_or: a placed pull always has its one file; the probe never invents one
                        bytes = pinned.files.first().map_or(0, |f| f.bytes), // map_or: as above
                        base = %pinned.base.model_id,
                        base_source_sha256 = %pinned.base.source_sha256,
                        placed = %path.display(),
                        "a leased adapter came back verified and is placed in its job directory, for the sentinel to adopt"
                    );
                    pulls.insert(attempt, PullState::Placed { path, pinned: Box::new(pinned) });
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
    }
}

fn running(done: u64, total: u64) -> TrainingStatus {
    // Progress through the pull, after training finished: shown, never used to decide.
    let pct = if total == 0 { 100.0 } else { (done as f64 * 100.0 / total as f64) as f32 };
    TrainingStatus::Running { progress_pct: pct, current_epoch: 0 }
}

/// The returned adapter as this node's own artifact: a GGUF LoRA at its placed path.
fn local_artifact(artifact: &LeaseArtifact, path: PathBuf) -> TrainingArtifact {
    // `artifact` is the PINNED manifest the bytes were verified against, never a later poll's.
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
    async fn run(&self, base_digest: &BaseDigest, root: &(dyn Fn() -> Option<PathBuf> + Send + Sync)) -> Result<(PathBuf, LeaseArtifact), PullError> {
        let root = root().ok_or_else(|| PullError::Final("this node has no genome root".into()))?;
        let attempt = self.attempt;
        let dir = tokio::task::spawn_blocking(move || crate::genome::fine_tuning::job_dir::find_job_dir_under(&root, attempt, LEASE_FILE))
            .await
            .map_err(|e| PullError::Transient(e.to_string()))?
            .map_err(PullError::Final)?
            .ok_or_else(|| PullError::Final(format!("no job directory holds attempt {attempt}'s lease: it was never recorded here")))?;
        self.check_binding(&dir).await?;
        let pinned = self.pin(&dir).await?;
        if pinned.format != ArtifactFormat::GgufLora || pinned.files.len() != 1 {
            return Err(PullError::Final(format!("a returned adapter is one GGUF LoRA file, not {:?} in {} files", pinned.format, pinned.files.len())));
        }
        // The base it was trained on must be the base this node serves, by content.
        let (digest, base) = (base_digest.clone(), pinned.base.model_id.clone());
        let ours = tokio::task::spawn_blocking(move || digest(&base)).await.map_err(|e| PullError::Transient(e.to_string()))?.map_err(PullError::Transient)?;
        if ours != pinned.base.source_sha256 {
            return Err(PullError::Final(format!(
                "trained on a base whose content ({}) is not this node's {} ({ours})",
                pinned.base.source_sha256, pinned.base.model_id
            )));
        }
        let file = pinned.files[0].clone();
        let returned = dir.join("returned");
        let placed = returned.join(ADAPTER_FILE);
        if file_digest(&placed).await.is_some_and(|sha| sha == file.sha256) {
            return Ok((placed, pinned));
        }
        let partial = returned.join(format!(".{ADAPTER_FILE}.partial"));
        self.fill(&partial, &file.name, file.bytes).await?;
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
        Ok((placed, pinned))
    }

    /// The directory's lease must be THIS one: bound to this peer and attempt, its request the one
    /// the binding names by digest, and trained on the base the manifest names. Checked before
    /// anything is pinned or pulled (Codex on #5419).
    async fn check_binding(&self, dir: &Path) -> Result<(), PullError> {
        let (dir_owned, peer, attempt, base) = (dir.to_path_buf(), self.peer, self.attempt, self.artifact.base.model_id.clone());
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let binding: LeaseBinding = std::fs::read(dir_owned.join(LEASE_FILE))
                .map_err(|e| e.to_string())
                .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|e| e.to_string())) // disk boundary: the binding recorded before sending
                .map_err(|e| format!("{}: unreadable lease binding: {e}", dir_owned.display()))?;
            if binding.peer != peer || binding.attempt != attempt {
                return Err(format!("attempt {attempt}'s lease is bound to peer {} attempt {}, not {peer}", binding.peer, binding.attempt));
            }
            let request_bytes = std::fs::read(dir_owned.join(REQUEST_FILE)).map_err(|e| format!("{}: {e}", dir_owned.join(REQUEST_FILE).display()))?;
            if format!("{:x}", Sha256::digest(&request_bytes)) != binding.request_sha256 {
                return Err(format!("{} is not the request attempt {attempt} was bound to", dir_owned.join(REQUEST_FILE).display()));
            }
            let request = crate::genome::fine_tuning::job_dir::read_job_request(&dir_owned)?;
            if request.base_model != base {
                return Err(format!("the adapter names base {base}, but attempt {attempt} asked for {}", request.base_model));
            }
            Ok(())
        })
        .await
        .map_err(|e| PullError::Transient(e.to_string()))?
        .map_err(PullError::Final)
    }

    /// The manifest this attempt's pull verifies against: published once at the first pull, and
    /// every later report (a later poll, a restarted origin) must be that same manifest.
    async fn pin(&self, dir: &Path) -> Result<LeaseArtifact, PullError> {
        let bytes = serde_json::to_vec(&self.artifact).map_err(|e| PullError::Final(e.to_string()))?; // disk boundary: the manifest pinned for this attempt
        let dir_owned = dir.to_path_buf();
        let existing = tokio::task::spawn_blocking(move || crate::genome::fine_tuning::job_dir::publish_job_file(&dir_owned, MANIFEST_FILE, &bytes))
            .await
            .map_err(|e| PullError::Transient(e.to_string()))?
            .map_err(PullError::Transient)?;
        let Some(existing) = existing else { return Ok(self.artifact.clone()) };
        let pinned: LeaseArtifact = serde_json::from_slice(&existing) // disk boundary: the manifest pinned first
            .map_err(|e| PullError::Final(format!("{}: unreadable pinned manifest: {e}", dir.display())))?;
        let same = pinned.model_id == self.artifact.model_id
            && pinned.base == self.artifact.base
            && pinned.format == self.artifact.format
            && pinned.files == self.artifact.files;
        if !same {
            return Err(PullError::Final("the peer now reports another artifact than the one pinned for this attempt".into()));
        }
        Ok(pinned)
    }

    /// Append ranges to `partial` until it holds `total` bytes, resuming from what it holds.
    async fn fill(&self, partial: &Path, name: &str, total: u64) -> Result<(), PullError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::genome::lease_artifact::{LeaseArtifactBase, LeaseArtifactFile};
    use crate::genome::fine_tuning::JobMetrics;

    /// A peer's artifact bytes, every range asked logged, and a gate the first range waits on.
    struct Peer {
        data: Vec<u8>,
        offsets: std::sync::Mutex<Vec<u64>>,
        gate: tokio::sync::Semaphore,
    }

    #[async_trait]
    impl ArtifactSource for Peer {
        async fn range(&self, _peer: Uuid, _attempt: Uuid, _file: &str, offset: u64, len: u32) -> Result<Vec<u8>, RangeError> {
            self.offsets.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(offset); // unwrap_or_else: a test fixture's poisoned log still reads
            let _open = self.gate.acquire().await.map_err(|e| RangeError::Transient(e.to_string()))?;
            let start = offset as usize;
            Ok(self.data[start..(start + len as usize).min(self.data.len())].to_vec())
        }
    }

    fn peer(data: Vec<u8>, open: bool) -> Arc<Peer> {
        Arc::new(Peer { data, offsets: std::sync::Mutex::new(Vec::new()), gate: tokio::sync::Semaphore::new(usize::from(open)) })
    }

    fn manifest(name: &str, data: &[u8]) -> LeaseArtifact {
        LeaseArtifact {
            model_id: "engine-local:code:1".into(),
            base: LeaseArtifactBase { model_id: "qwen".into(), source_sha256: "base-sha".into(), fingerprint: "fp".into() },
            format: ArtifactFormat::GgufLora,
            files: vec![LeaseArtifactFile { name: name.into(), bytes: data.len() as u64, sha256: format!("{:x}", Sha256::digest(data)) }],
            metrics: JobMetrics::default(),
        }
    }

    fn pull_over(source: Arc<Peer>, root: &Path) -> LeasePull {
        let root = root.to_path_buf();
        LeasePull::new(source, Arc::new(|_: &str| Ok("base-sha".to_string())), Arc::new(move || Some(root.clone())))
    }

    fn request(persona: &str, dataset_note: &str) -> TrainingJobRequest {
        let mut request = crate::commands::genome::test_support::request_for("qwen");
        request.persona_name = persona.into();
        request.trait_kind = "code".into();
        request.dataset.examples[0].completion = dataset_note.into();
        request
    }

    async fn settled(pull: &LeasePull, peer_id: Uuid, attempt: Uuid, artifact: &LeaseArtifact) -> TrainingStatus {
        for _ in 0..500 {
            match pull.status(peer_id, attempt, artifact.clone()) {
                TrainingStatus::Running { .. } => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
                done => return done,
            }
        }
        panic!("the pull did not settle within 5 s"); // JUSTIFIED: test bound
    }

    // what this catches (Codex on #5419, 1): a retry of an attempt rewriting its accepted request
    // or opening a second directory for it. The same request re-records; another dataset, base,
    // peer, or persona is refused; a restarted origin still holds the original request.
    #[test]
    fn a_lease_is_recorded_once_and_a_changed_retry_is_refused() {
        let root = tempfile::tempdir().expect("root"); // JUSTIFIED: test fixture
        let (peer_id, attempt) = (Uuid::new_v4(), Uuid::new_v4());
        let pull = pull_over(peer(Vec::new(), true), root.path());
        let original = request("kimi", "act");
        pull.record_request(peer_id, attempt, &original).expect("recorded"); // JUSTIFIED: test fixture
        assert!(pull.record_request(peer_id, attempt, &original).is_ok(), "the same retry confirms");
        assert!(pull.record_request(peer_id, attempt, &request("kimi", "another dataset")).is_err());
        let mut other_base = original.clone();
        other_base.base_model = "llama".into();
        assert!(pull.record_request(peer_id, attempt, &other_base).is_err());
        assert!(pull.record_request(Uuid::new_v4(), attempt, &original).is_err(), "another peer");
        assert!(pull.record_request(peer_id, attempt, &request("sahar", "act")).is_err(), "another persona: never a second directory");

        let restarted = pull_over(peer(Vec::new(), true), root.path());
        assert!(restarted.record_request(peer_id, attempt, &request("kimi", "another dataset")).is_err());
        let dir = crate::genome::fine_tuning::job_dir::find_job_dir_under(root.path(), attempt, LEASE_FILE).expect("found").expect("one"); // JUSTIFIED: test fixture
        let kept = crate::genome::fine_tuning::job_dir::read_job_request(&dir).expect("read"); // JUSTIFIED: test fixture
        assert_eq!(kept.dataset.examples[0].completion, "act", "the original request stands");
    }

    // what this catches (Codex on #5419, 2 and 3): a peer-sent file name reaching a local path,
    // two polls at once starting two writers, and a later or restarted report other than the
    // pinned manifest. A manifest naming `../../evil` lands only at `returned/adapter.gguf`;
    // simultaneous polls start one pull (offset 0 asked once); a restarted origin offered a
    // changed manifest refuses it.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_pull_is_single_flight_lands_at_a_fixed_name_and_keeps_its_pinned_manifest() {
        let root = tempfile::tempdir().expect("root"); // JUSTIFIED: test fixture
        let (peer_id, attempt) = (Uuid::new_v4(), Uuid::new_v4());
        let data: Vec<u8> = (0..MAX_ARTIFACT_CHUNK as usize + 10).map(|i| (i % 251) as u8).collect();
        let source = peer(data.clone(), false);
        let pull = Arc::new(pull_over(source.clone(), root.path()));
        pull.record_request(peer_id, attempt, &request("kimi", "act")).expect("recorded"); // JUSTIFIED: test fixture
        let evil = manifest("../../evil", &data);

        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let racers: Vec<_> = (0..2)
            .map(|_| {
                let (pull, barrier, evil) = (pull.clone(), barrier.clone(), evil.clone());
                tokio::spawn(async move {
                    barrier.wait().await;
                    pull.status(peer_id, attempt, evil)
                })
            })
            .collect();
        for racer in racers {
            assert!(matches!(racer.await.expect("joined"), TrainingStatus::Running { .. })); // JUSTIFIED: test asserts the arm
        }
        source.gate.add_permits(1);
        let TrainingStatus::Completed { artifact } = settled(&pull, peer_id, attempt, &evil).await else {
            panic!("the adapter was placed"); // JUSTIFIED: test asserts the arm
        };
        let offsets = source.offsets.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone(); // unwrap_or_else: a test fixture's poisoned log still reads
        assert_eq!(offsets.iter().filter(|&&o| o == 0).count(), 1, "one writer: {offsets:?}");
        let placed = artifact.local_path.expect("placed"); // JUSTIFIED: test asserts the path
        assert!(placed.ends_with("returned/adapter.gguf"), "{}", placed.display());
        assert_eq!(std::fs::read(&placed).expect("read"), data); // JUSTIFIED: test asserts the bytes
        assert!(!root.path().join("evil").exists() && !root.path().parent().is_some_and(|p| p.join("evil").exists()), "no peer name reached a path");

        let restarted = pull_over(peer(data.clone(), true), root.path());
        let changed = manifest("adapter.gguf", b"other bytes");
        assert!(matches!(settled(&restarted, peer_id, attempt, &changed).await, TrainingStatus::Failed { error } if error.contains("pinned")));
    }

    // what this catches (Codex on #5419, delta): two first records of one attempt under
    // different personas both winning, a request.json that is not the one the binding names
    // accepted on a retry, and a pull that imports without checking its lease. Two concurrent
    // changed-persona records: exactly one wins. A tampered request.json fails the retry and the
    // pull. A pull for another peer than the binding, or a manifest naming another base than the
    // request, is final before anything is pinned or pulled.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_lease_records_once_under_a_race_and_a_pull_checks_its_binding() {
        let root = tempfile::tempdir().expect("root"); // JUSTIFIED: test fixture
        let (peer_id, attempt) = (Uuid::new_v4(), Uuid::new_v4());
        let pull = Arc::new(pull_over(peer(b"lora".to_vec(), true), root.path()));
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let racers: Vec<_> = ["kimi", "sahar"]
            .into_iter()
            .map(|persona| {
                let (pull, barrier) = (pull.clone(), barrier.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    pull.record_request(peer_id, attempt, &request(persona, "act")).is_ok()
                })
            })
            .collect();
        let won: Vec<bool> = racers.into_iter().map(|r| r.join().expect("joined")).collect(); // JUSTIFIED: test fixture
        assert_eq!(won.iter().filter(|&&w| w).count(), 1, "exactly one first record: {won:?}");

        let dir = crate::genome::fine_tuning::job_dir::find_job_dir_under(root.path(), attempt, LEASE_FILE).expect("found").expect("one"); // JUSTIFIED: test fixture
        let winner = crate::genome::fine_tuning::job_dir::read_job_request(&dir).expect("read"); // JUSTIFIED: test fixture
        let wrong_base = LeaseArtifact { base: LeaseArtifactBase { model_id: "llama".into(), ..manifest("adapter.gguf", b"lora").base }, ..manifest("adapter.gguf", b"lora") };
        assert!(matches!(settled(&pull, peer_id, attempt, &wrong_base).await, TrainingStatus::Failed { error } if error.contains("asked for")));

        let fresh = Uuid::new_v4();
        let pull = pull_over(peer(b"lora".to_vec(), true), root.path());
        pull.record_request(peer_id, fresh, &winner).expect("recorded"); // JUSTIFIED: test fixture
        assert!(matches!(settled(&pull, Uuid::new_v4(), fresh, &manifest("adapter.gguf", b"lora")).await, TrainingStatus::Failed { error } if error.contains("bound to peer")));

        let tampered = Uuid::new_v4();
        let pull = pull_over(peer(b"lora".to_vec(), true), root.path());
        pull.record_request(peer_id, tampered, &winner).expect("recorded"); // JUSTIFIED: test fixture
        let tampered_dir = crate::genome::fine_tuning::job_dir::find_job_dir_under(root.path(), tampered, LEASE_FILE).expect("found").expect("one"); // JUSTIFIED: test fixture
        std::fs::write(tampered_dir.join(REQUEST_FILE), b"{}").expect("tamper"); // JUSTIFIED: test fixture
        assert!(pull.record_request(peer_id, tampered, &winner).is_err(), "a request.json the binding does not name fails the retry");
        assert!(matches!(settled(&pull, peer_id, tampered, &manifest("adapter.gguf", b"lora")).await, TrainingStatus::Failed { error } if error.contains("not the request")));
    }
}
