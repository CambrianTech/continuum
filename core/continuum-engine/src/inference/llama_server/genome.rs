//! Genome responsibility of the llama-server process owner.
use super::*;
use std::collections::BTreeMap;
use super::genome_catalog::{Add, CatalogChange, CatalogEnd, CatalogRefusal, CatalogStep, Retire};
/// Whether the serving engine accepts `--train-dir`, read off its own `--help` by the same
/// once-per-binary probe that reads its KV cache types. An engine older than the flag REJECTS
/// it and the lane does not start (2026-09-27: #4438 on canary would have put `--train-dir` on
/// the 5090's lane while its engine was still b10765-965d38a90, because the deploy consumer
/// converges the core but not the engine). The record is the binary the probe last asked with its
/// answer, because the answer is about that binary alone: a promoted binary (or another endpoint's)
/// is not asked until it is asked itself (Codex on #5817). "Not asked yet" and "no" decide
/// differently: a launch reads both as no (in-engine training is never worth a lane), but an
/// in-place gene change asks first (IntelMac 2026-11-07: a core adopting a live engine read the
/// unasked flag as no 71 ms before the probe answered yes, and relaunched the engine cold).
pub(super) static ENGINE_PROBES: parking_lot::Mutex<BTreeMap<String, ProbeOutcome>> = parking_lot::Mutex::new(BTreeMap::new());

/// What the support probe got from one engine binary: the one record of both "was it asked" and
/// "what did it say", per binary, so asking one binary never touches another's (Codex on #5817: two
/// single-slot records, a failed probe of B left A marked asked with B's "no", and A read "no").
/// One entry per installed engine binary: a handful, never growing with use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProbeOutcome {
    /// It answered (`answered` or `did_not_enumerate`): never asked again.
    Answered { train_dir: bool, minds: bool },
    /// It was asked and did not answer: no, for now, and asked again at its next use.
    Unanswered,
}

/// What `bin` said about `--train-dir`: `None` until `bin` itself was asked.
pub(super) fn train_dir_support(bin: &str) -> Option<bool> {
    answer_for(&ENGINE_PROBES.lock(), bin)
}

/// PURE: what the record says about `bin`: its answer, no when it did not answer, nothing when it
/// was never asked.
pub(super) fn answer_for(probes: &BTreeMap<String, ProbeOutcome>, bin: &str) -> Option<bool> {
    probes.get(bin).map(|outcome| matches!(outcome, ProbeOutcome::Answered { train_dir: true, .. }))
}

/// PURE: what a decision may rest on: `bin`'s answer when it answered, else nothing (never asked,
/// or asked without an answer), and the caller asks it now.
pub(super) fn train_dir_known(probes: &BTreeMap<String, ProbeOutcome>, bin: &str) -> Option<bool> {
    match probes.get(bin) {
        Some(ProbeOutcome::Answered { train_dir, .. }) => Some(*train_dir),
        _ => None,
    }
}

/// PURE: whether `bin` needs asking: never asked, or asked without an answer.
pub(super) fn needs_asking(probes: &BTreeMap<String, ProbeOutcome>, bin: &str) -> bool {
    !matches!(probes.get(bin), Some(ProbeOutcome::Answered { .. }))
}

/// Whether `bin` accepts `--train-dir`. A launch reads "not asked yet" as no: the one answer that
/// cannot keep a lane from starting.
pub fn engine_accepts_train_dir(bin: &str) -> bool {
    train_dir_support(bin) == Some(true)
}


/// The engine lists the flag as an option (not merely mentions the word somewhere).
pub(super) fn engine_help_lists_train_dir(help: &str) -> bool {
    help.lines().any(|l| l.trim_start().starts_with("--train-dir"))
}

/// The engine keeps resident minds as parked sequences (`--minds`, fork #139): a mind switch is a sequence
/// move, never a serialization.
pub(super) fn engine_help_lists_minds(help: &str) -> bool {
    help.lines().any(|l| l.trim_start().starts_with("--minds"))
}

/// Whether `bin` accepts `--minds`. "Not asked yet" reads as no: an older engine refuses to start with it.
pub fn engine_accepts_minds(bin: &str) -> bool {
    matches!(ENGINE_PROBES.lock().get(bin), Some(ProbeOutcome::Answered { minds: true, .. }))
}


/// Where every lane's `POST /train` writes its adapters (`--train-dir`). One directory
/// for all lanes: a job names its output `<job-uuid>.gguf`, and the engine trainer moves
/// the finished adapter into the job's own directory, so this holds only runs in flight
/// and the leftovers of a crashed one (tracked as `engine-train`, with its eviction
/// decision in `disk_eviction`). `None` without a home directory: training is then off,
/// never pointed at a temp dir nothing governs.
pub fn engine_train_dir() -> Option<PathBuf> {
    crate::paths::continuum_home().ok().map(|h| h.join("cache").join("engine-train"))
}


/// PURE: the bare file name a gene is staged under inside the engine's --train-dir, the only
/// folder `POST /lora-adapters/load` reads: `gene-<sha256 of its genome path, hex>.gguf`. The full
/// hash is the gene's identity (two genes never share one), the name is the same for the same gene
/// on every call, so the adapter catalog maps back to its genome path by recomputing it, and at
/// 74 bytes it is always inside the engine's name confinement (`confine_out`, at most 200 bytes, no
/// separators, no "..").
pub(crate) fn gene_stage_name(gene: &std::path::Path) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(gene.as_os_str().as_encoded_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("gene-{hex}.gguf")
}


/// PURE: an engine's catalog as genome paths. An adapter loaded at launch is listed by its own
/// path; one loaded in place by the name it was staged under (`gene_stage_name`), matched back
/// to the target's gene that stages to it; anything else keeps the engine's path (a gene the
/// target dropped, to retire). Sorted like `ServingTarget::adapter_paths`, so the two compare.
pub(super) fn genome_paths_of(catalog: &[(i64, String)], genes: &[String]) -> Vec<String> {
    let mut paths: Vec<String> = catalog
        .iter()
        .map(|(_, path)| {
            if genes.contains(path) {
                return path.clone();
            }
            let base = std::path::Path::new(path).file_name().and_then(|n| n.to_str());
            match genes.iter().find(|gene| base == Some(gene_stage_name(std::path::Path::new(gene.as_str())).as_str())) {
                Some(gene) => gene.clone(),
                None => path.clone(),
            }
        })
        .collect();
    paths.sort();
    paths.dedup();
    paths
}


/// Reading a LoRA's tensors from disk onto the device: seconds for a large one, never the
/// 5 s probe bound.
pub(super) const LORA_LOAD_TIMEOUT: Duration = Duration::from_secs(120);


/// Put `gene` into the engine's --train-dir as `into`: a hard link (no copy, no second
/// footprint) when both sit on one filesystem, else a copy. Always re-staged: a retrained
/// gene of the same rank has the same size, so an existing file proves nothing about which
/// weights it holds (Cormac on #4467).
pub(super) fn stage_gene(gene: &std::path::Path, into: &std::path::Path) -> std::io::Result<()> {
    std::fs::metadata(gene)?; // the gene must exist: fail loud, never stage a stale leftover
    match std::fs::remove_file(into) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    std::fs::hard_link(gene, into).or_else(|_| std::fs::copy(gene, into).map(|_| ()))
}


/// What an in-place genome change came to (card 49b5e806, charter S1; 4.10 P4a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenomeInPlace {
    /// The running engine now serves the target's adapter set: no relaunch.
    Adopted,
    /// This engine cannot load at runtime (no --train-dir, or no learning ledger): relaunch.
    Unsupported,
    /// A retiring adapter is active in a turn in flight: the change stopped there, with every
    /// step before it applied and recorded, and the rest waits a tick (a relaunch here would
    /// kill that turn).
    Busy,
    /// The catalog's owner refused: the engine and every resident's KV stay, the lane is Degraded.
    Refused(CatalogRefusal),
}

impl LlamaServerProcess {
    /// The in-place genome change against an engine whose --train-dir is `dir`: decide what the
    /// target adds and drops, stage the additions, and hand every mutation to the catalog's owner,
    /// bound to the engine identity read before the catalog. Each applied step is recorded.
    pub(super) async fn converge_genome(
        &self,
        dir: &std::path::Path,
        target: &ServingTarget,
        operation: &crate::inference::serving_lifecycle::ServingOperation,
    ) -> Result<GenomeInPlace, LlamaServerError> {
        let Some(owner) = super::genome_catalog::owner() else {
            return Ok(GenomeInPlace::Refused(CatalogRefusal::NoOwner));
        };
        let Some(process) = LlamaServerControl::served_incarnation(self) else {
            return Ok(GenomeInPlace::Refused(CatalogRefusal::EngineReplaced));
        };
        let engine = LearningEngine::of(process);
        // identity first: every step below is bound to this engine, read before its catalog
        let ledger = match engine.learn_ledger().await? {
            LedgerAnswer::Ledger(snapshot) => snapshot.engine,
            LedgerAnswer::NoLedger => return Ok(GenomeInPlace::Unsupported),
        };
        let Some(active) = self.served_adapters.lock().unwrap().clone() else { // JUSTIFIED: poison means a prior panic while recording the served set.
            return Err(LlamaServerError::Unreachable("the engine's adapter set was never read: converge reads it first (active_adapters)".into()));
        };
        let desired = target.adapter_paths();
        let removed: Vec<&String> = active.iter().filter(|p| !desired.contains(p)).collect();
        let catalog = if removed.is_empty() { Vec::new() } else { self.lora_catalog().await? };
        let mut retire = Vec::with_capacity(removed.len());
        for gene in removed {
            let stage = gene_stage_name(std::path::Path::new(gene.as_str()));
            let id = catalog.iter().find_map(|(id, path)| {
                let base = std::path::Path::new(path).file_name().and_then(|n| n.to_str());
                (path == gene || base == Some(stage.as_str())).then_some(*id)
            });
            match id {
                Some(id) => retire.push(Retire { gene: gene.clone(), id }),
                None => self.record_served(gene, false)?, // not loaded: nothing to retire, and not served
            }
        }
        let mut add = Vec::new();
        for gene in desired.iter().filter(|p| !active.contains(p)) {
            let name = gene_stage_name(std::path::Path::new(gene.as_str()));
            let staged = dir.join(&name);
            let (from, into) = (PathBuf::from(gene.as_str()), staged.clone());
            tokio::task::spawn_blocking(move || stage_gene(&from, &into))
                .await
                .map_err(|e| LlamaServerError::Spawn(format!("staging a gene panicked: {e}")))?
                .map_err(|e| LlamaServerError::AdapterNotFound(format!("{name}: {e}")))?;
            add.push(Add { gene: gene.clone(), name, staged });
        }
        let outcome = owner.converge(CatalogChange { operation, engine, ledger, retire, add }).await;
        for step in &outcome.applied {
            match step {
                CatalogStep::Retired(gene) => self.record_served(gene, false)?,
                CatalogStep::Added(gene) => self.record_served(gene, true)?,
            }
        }
        Ok(match outcome.end {
            CatalogEnd::Done => GenomeInPlace::Adopted,
            CatalogEnd::Busy => GenomeInPlace::Busy,
            CatalogEnd::Refused(why) => GenomeInPlace::Refused(why),
        })
    }

    /// Record that the engine holds (or no longer holds) `gene`, so an early stop leaves the served
    /// set naming exactly what the engine holds (Codex on #4467).
    fn record_served(&self, gene: &str, held: bool) -> Result<(), LlamaServerError> {
        let mut guard = self.served_adapters.lock().unwrap(); // JUSTIFIED: poison means a prior panic while recording the served set.
        let Some(served) = guard.as_mut() else {
            return Err(LlamaServerError::Unreachable("the engine's adapter set was never read: converge reads it first (active_adapters)".into()));
        };
        served.retain(|p| p != gene);
        if held {
            served.push(gene.to_string());
            served.sort();
        }
        Ok(())
    }

    /// `GET /lora-adapters` as `(id, path)`: the engine's adapter catalog. Retired ids
    /// are not listed (fork #26).
    pub(super) async fn lora_catalog(&self) -> Result<Vec<(i64, String)>, LlamaServerError> {
        let url = format!("{}/lora-adapters", self.root);
        let resp = self
            .client
            .get(&url)
            .timeout(PROBE_TIMEOUT)
            .send()
            .await
            .map_err(|e| LlamaServerError::Unreachable(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(LlamaServerError::Unreachable(format!("/lora-adapters status {}", resp.status())));
        }
        let body: serde_json::Value =
            resp.json().await.map_err(|e| LlamaServerError::Unreachable(e.to_string()))?;
        // A malformed answer is an error, never an empty set: an empty read is what relaunched
        // an engine serving 3 adapters (IntelMac 2026-11-07, from=0).
        let entries = body.as_array().ok_or_else(|| LlamaServerError::Unreachable("/lora-adapters answered no array".into()))?;
        entries
            .iter()
            .map(|e| match (e.get("id").and_then(|v| v.as_i64()), e.get("path").and_then(|v| v.as_str())) {
                (Some(id), Some(path)) => Ok((id, path.to_string())),
                _ => Err(LlamaServerError::Unreachable(format!("/lora-adapters entry without an id and a path: {e}"))),
            })
            .collect()
    }
}

impl LlamaServerProcess {

    /// Set every loaded LoRA adapter's GLOBAL scale to 0.0 (dormant catalog —
    /// per-request activation only). Best-effort: a failure logs loud but does
    /// not fail bringup (a lane with active adapters still serves; it is the
    /// degraded-not-dead case, and the log names it).
    pub async fn zero_adapter_scales(&self) {
        let root = self.v1_url.trim_end_matches('/').trim_end_matches("/v1");
        let url = format!("{root}/lora-adapters");
        let list: Vec<serde_json::Value> = match self.client.get(&url).send().await {
            // `.unwrap_or_default()` turned a malformed/HTML error body into an EMPTY
            // list — indistinguishable from "this lane has no adapters", so dormancy
            // silently no-ops and the genome stays hot at full scale.
            Ok(r) => match r.json().await {
                Ok(list) => list,
                Err(e) => {
                    tracing::warn!(target: "continuum_core::inference::llama_server", error = %e,
                        "genome dormancy: /lora-adapters returned a body we cannot parse \
                         — adapter scales were NOT zeroed (this is not an empty adapter set)"
                    );
                    return;
                }
            },
            Err(e) => {
                tracing::warn!(target: "continuum_core::inference::llama_server", error = %e, "genome dormancy: could not read /lora-adapters");
                return;
            }
        };
        let zeroed: Vec<serde_json::Value> = list
            .iter()
            .filter_map(|a| a.get("id").and_then(|i| i.as_i64()))
            .map(|id| serde_json::json!({"id": id, "scale": 0.0}))
            .collect();
        if zeroed.is_empty() {
            return;
        }
        match self.client.post(&url).json(&zeroed).send().await {
            Ok(r) if r.status().is_success() => {
                tracing::info!(target: "continuum_core::inference::llama_server", adapters = zeroed.len(),
                    probe_class = "serving.genome.dormant",
                    "genome catalog loaded DORMANT — all adapter scales zeroed; per-request activation only"
                );
            }
            Ok(r) => {
                tracing::warn!(target: "continuum_core::inference::llama_server", status = %r.status(), "genome dormancy: scale-zero POST refused")
            }
            Err(e) => tracing::warn!(target: "continuum_core::inference::llama_server", error = %e, "genome dormancy: scale-zero POST failed"),
        }
    }
}
