//! Genome responsibility of the llama-server process owner.
use super::*;
/// Whether the serving engine accepts `--train-dir`, read off its own `--help` by the same
/// once-per-process probe that reads its KV cache types. An engine older than the flag REJECTS
/// it and the lane does not start (2026-09-27: #4438 on canary would have put `--train-dir` on
/// the 5090's lane while its engine was still b10765-965d38a90, because the deploy consumer
/// converges the core but not the engine). Unknown — no probe, no answer — is NO: in-engine
/// training is never worth a lane.
pub(super) static ENGINE_TRAIN_DIR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);


pub fn engine_accepts_train_dir() -> bool {
    ENGINE_TRAIN_DIR.load(std::sync::atomic::Ordering::Relaxed) // unprobed or unanswered is false: the one answer that cannot keep a lane from starting
}


/// The engine lists the flag as an option (not merely mentions the word somewhere).
pub(super) fn engine_help_lists_train_dir(help: &str) -> bool {
    help.lines().any(|l| l.trim_start().starts_with("--train-dir"))
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


/// PURE: the bare file name a gene is staged under inside the engine's --train-dir, the
/// only folder `POST /lora-adapters/load` reads (a bare `.gguf` name: no separators, no
/// "..", no control characters, at most 200 bytes). Keyed by the gene's own file name so the
/// adapter catalog can be matched back to the genome path; `None` when that name cannot be
/// made one (the gene then rides a relaunch, as before). The full path's hash rides in the
/// name, so two genes that share a file name in different folders never collide.
pub(crate) fn gene_stage_name(gene: &std::path::Path) -> Option<String> {
    use sha2::Digest;
    let stem = gene.file_stem()?.to_str()?;
    let clean: String = stem
        .chars()
        .take(120)
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') { c } else { '_' })
        .collect();
    let digest = sha2::Sha256::digest(gene.as_os_str().as_encoded_bytes());
    let tag: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
    clean
        .chars()
        .any(|c| c.is_ascii_alphanumeric())
        .then(|| format!("gene-{clean}-{tag}.gguf"))
}


/// One `/lora-adapters/{load,unload}` answer.
pub(super) enum LoraPost {
    Done,
    /// The engine has no such route: it predates runtime adapter load.
    NoRoute,
    /// The adapter is active in a slot that is decoding: retry next tick.
    Busy,
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


/// What an in-place genome change came to (card 49b5e806, charter S1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenomeInPlace {
    /// The running engine now serves the target's adapter set: no relaunch.
    Adopted,
    /// This engine cannot load at runtime (no --train-dir, or it predates
    /// `/lora-adapters/load`), or a gene has no stageable name: relaunch, as before.
    Unsupported,
    /// A retiring adapter is active in a turn in flight: the change stopped there, with every
    /// step before it applied and recorded, and the rest waits a tick (a relaunch here would
    /// kill that turn).
    Busy,
}

impl LlamaServerProcess {
    /// ONE fetch of the root-level `/props` (beside `/health`) — the server's own
    /// metadata channel: served window, slot count, modalities, and (fork
    /// 3ca60da3c) measured weight residency. A connection error means nothing is
    /// up (normal pre-spawn) → Unreachable.
    /// The in-place genome change against an engine whose --train-dir is `dir`: retire what
    /// the target dropped, then stage and load what it added, recording each accepted step.
    pub(super) async fn converge_genome(
        &self,
        dir: &std::path::Path,
        target: &ServingTarget,
    ) -> Result<GenomeInPlace, LlamaServerError> {
        let active = self.served_adapters.lock().unwrap().clone(); // JUSTIFIED: poison means a prior panic while recording the served set.
        let desired = target.adapter_paths();
        let added: Vec<&String> = desired.iter().filter(|p| !active.contains(p)).collect();
        let removed: Vec<&String> = active.iter().filter(|p| !desired.contains(p)).collect();
        let mut staged = Vec::with_capacity(added.len());
        for gene in &added {
            let Some(name) = gene_stage_name(std::path::Path::new(gene.as_str())) else {
                return Ok(GenomeInPlace::Unsupported);
            };
            staged.push((PathBuf::from(gene.as_str()), name));
        }
        // Every step the engine accepts is recorded at once, so an early return (a busy
        // retire, an error) leaves the served set naming exactly what the engine holds;
        // the next tick converges the rest (Codex on #4467: two retires, the second busy,
        // left the first advertised although the engine had freed it).
        let record = |gene: &str, held: bool| {
            let mut served = self.served_adapters.lock().unwrap(); // JUSTIFIED: poison means a prior panic while recording the served set.
            served.retain(|p| p != gene);
            if held {
                served.push(gene.to_string());
                served.sort();
            }
        };
        // Retire first, so a refused retire (a turn holds the gene) stops the change before
        // any new gene joins.
        if !removed.is_empty() {
            let catalog = self.lora_catalog().await?;
            for gene in &removed {
                let stage = gene_stage_name(std::path::Path::new(gene.as_str()));
                let id = catalog.iter().find_map(|(id, path)| {
                    let base = std::path::Path::new(path).file_name().and_then(|n| n.to_str());
                    (path == *gene || (stage.is_some() && base == stage.as_deref())).then_some(*id)
                });
                let Some(id) = id else {
                    record(gene, false); // not loaded: nothing to retire, and not served
                    continue;
                };
                match self.lora_post("unload", serde_json::json!({ "id": id })).await? {
                    LoraPost::Done => {
                        record(gene, false);
                        if let Some(name) = &stage {
                            let _ = std::fs::remove_file(dir.join(name)); // best effort: the engine freed it; a leftover link is re-used by the next load of the same gene
                        }
                    }
                    LoraPost::Busy => return Ok(GenomeInPlace::Busy),
                    LoraPost::NoRoute => return Ok(GenomeInPlace::Unsupported),
                }
            }
        }
        for (gene, name) in staged {
            let into = dir.join(&name);
            let gene_key = gene.to_string_lossy().into_owned();
            tokio::task::spawn_blocking(move || stage_gene(&gene, &into))
                .await
                .map_err(|e| LlamaServerError::Spawn(format!("staging a gene panicked: {e}")))?
                .map_err(|e| LlamaServerError::AdapterNotFound(format!("{name}: {e}")))?;
            match self.lora_post("load", serde_json::json!({ "name": name })).await? {
                LoraPost::Done => record(&gene_key, true),
                LoraPost::Busy => return Ok(GenomeInPlace::Busy),
                LoraPost::NoRoute => return Ok(GenomeInPlace::Unsupported),
            }
        }
        Ok(GenomeInPlace::Adopted)
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
        Ok(body
            .as_array()
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|e| {
                        Some((e.get("id")?.as_i64()?, e.get("path")?.as_str()?.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default()) // unwrap_or_default: a non-array body lists nothing loaded
    }

    /// `POST /lora-adapters/{load,unload}`. 404 = an engine without the route (older than
    /// fork #26); 503 = the adapter is active in a turn in flight (unload only).
    pub(super) async fn lora_post(&self, verb: &str, body: serde_json::Value) -> Result<LoraPost, LlamaServerError> {
        let url = format!("{}/lora-adapters/{verb}", self.root);
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .timeout(LORA_LOAD_TIMEOUT)
            .send()
            .await
            .map_err(|e| LlamaServerError::Unreachable(e.to_string()))?;
        match resp.status().as_u16() {
            200..=299 => Ok(LoraPost::Done),
            404 | 405 => Ok(LoraPost::NoRoute),
            503 => Ok(LoraPost::Busy),
            status => {
                let text = resp.text().await.unwrap_or_default(); // unwrap_or_default: the status alone still names the failure
                Err(LlamaServerError::Unreachable(format!("/lora-adapters/{verb} status {status}: {text}")))
            }
        }
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
