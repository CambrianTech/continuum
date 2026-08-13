//! `models/pull` — acquire a local model's GGUF from its authoritative source,
//! then make it usable LIVE (no reboot).
//!
//! This is the "acquire" half of the rich API (sibling of `models/try`'s
//! "verify"). It reads the model's `gguf_hint` — the single source of truth for
//! where the artifact lives — resolves the concrete file in the HuggingFace
//! repo, downloads it into the shared HF cache via [`hf_hub`], and records the
//! resolved path onto the live [`ModelCatalog`] entry (flipping it to
//! [`Availability::Ready`] and bumping the snapshot generation). A subscriber
//! sees a `NotDownloaded` model become `Ready`, with its bytes-on-disk location,
//! WITHOUT a reboot.
//!
//! ## Where the data comes from
//!
//! - **what to pull**: the model row's `gguf_hint` (`huggingface.co/<owner>/<repo>`).
//!   Absent ⇒ the model is cloud-served or has no acquirable artifact ⇒ fail loud.
//! - **which file**: the repo's own file listing (`ApiRepo::info().siblings`),
//!   filtered to `.gguf` and chosen by quant preference. The repo is the
//!   authority on what quant tiers exist — we do not hardcode filenames.
//! - **the multimodal projector**: for a [`Capability::Vision`] model, the
//!   `mmproj-*.gguf` sibling, pulled alongside (a vision GGUF is unservable
//!   without it).
//! - **where it lands**: the HF cache (`~/.cache/huggingface/hub/`), the SAME
//!   place the artifact resolver already reads — so a pull is content-addressed
//!   and a re-pull is instant (hf-hub dedups by sha).
//!
//! ## Gating
//!
//! `Privileged` — it performs network I/O, writes multi-GB to disk, and mutates
//! substrate state. Not an `AiSafe` read.

use std::sync::Arc;

use hf_hub::api::tokio::ApiBuilder;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model_registry::live::ModelCatalog;
use crate::model_registry::Capability;
use crate::sdk_codegen::CommandError;

/// Which model to acquire, and (optionally) which quant tier to prefer.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/ModelsPullParams.ts"
)]
pub struct ModelsPullParams {
    /// The model id as it appears in `models/list`. Fails loud if it is unknown,
    /// or if it has no `gguf_hint` (a cloud model has nothing to pull).
    pub model_id: String,
    /// Preferred quant substring, case-insensitive (e.g. `Q4_K_M`, `Q8_0`). When
    /// set, only a file containing it is pulled — fail loud if the repo has no
    /// such tier rather than silently substituting another. When absent, a
    /// balanced default ordering picks the tier.
    #[serde(default)]
    #[ts(optional)]
    pub quant: Option<String>,
    /// Weight format to acquire: `gguf` (derived serving artifact), `safetensors` (the
    /// unquantized SOURCE weights — tuning and quantization input), or `auto`.
    ///
    /// Default `auto` prefers GGUF whenever the repo publishes one, so this parameter never
    /// changes what an existing call resolves to. It exists because GGUF is *derived*: you
    /// cannot LoRA-tune it, cannot re-quantize to an unpublished tier, and cannot forge a
    /// device-fit override from it. A repo that ships only safetensors used to be unacquirable
    /// through the governed path at all, which forced hand-rolled downloads — no ledger, no
    /// resume, no shared cache location.
    #[serde(default)]
    #[ts(optional)]
    pub format: Option<String>,
    /// Fire-and-poll (mirrors `agent/solve --detach`, #86). A frontier GGUF is tens of
    /// GB and takes an HOUR — that MUST NOT hold the command socket. With `detach`, the
    /// call returns a handle NOW (`detached: true`, empty path/bytes) and the real report
    /// lands in `~/.continuum/progress/models-pull-<run_id>.json`. Progress is published
    /// on the bus as `models:pull:progress` per shard so the UI / a persona / Positron can
    /// watch it. Re-running the SAME pull is idempotent: the HF cache is content-addressed,
    /// so completed shards skip instantly and an interrupted pull resumes where it stopped.
    #[serde(default)]
    #[ts(optional)]
    pub detach: Option<bool>,
    /// Correlation id for a detached pull (echoed in the ack, the progress events and the
    /// result file). Omit → minted. Pass the SAME id to resume-and-watch one logical pull.
    #[serde(default)]
    #[ts(optional)]
    pub run_id: Option<String>,
}

/// What `models/pull` landed: the chosen file, where it lives, its size, and the
/// projector if one came too. The command's return DTO — not stored on status.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/PullReport.ts"
)]
pub struct PullReport {
    /// The repo file that was pulled (e.g. `Qwen2-VL-7B-Instruct-Q4_K_M.gguf`).
    pub gguf_file: String,
    /// Absolute path to the GGUF on disk (in the HF cache).
    pub gguf_path: String,
    /// The multimodal projector file, if this is a vision model and one was found.
    #[ts(optional)]
    pub mmproj_file: Option<String>,
    /// Bytes of the main GGUF on disk.
    #[ts(type = "number")]
    pub bytes: u64,
    /// Human-readable summary (repo, chosen tier, whether mmproj came too).
    pub detail: String,
    /// True when this is the immediate ACK of a detached pull — `gguf_path`/`bytes` are
    /// empty/zero. Poll `~/.continuum/progress/models-pull-<run_id>.json` for the real
    /// report, or subscribe to `models:pull:progress` / `models:pull:complete`.
    #[serde(default)]
    pub detached: bool,
    /// Correlation id, present on a detached ack and on every progress event for the run.
    #[serde(default)]
    #[ts(optional)]
    pub run_id: Option<String>,
    /// Why the pull failed, when it did. Present ONLY on a terminal failure record written to
    /// the progress ledger. A detached failure used to write nothing at all, so a watcher polling
    /// the path the ack named could not tell "failed" from "still downloading" — ever. The report
    /// type has to be able to say "this ended badly" or the ledger can only describe success.
    #[serde(default)]
    #[ts(optional)]
    pub error: Option<String>,
}

/// Live byte-level progress for one shard, written to the SAME ledger path the detached ack tells
/// the caller to poll.
///
/// Why this exists: progress was published only on the bus, once per SHARD BOUNDARY. For a model
/// whose middle shard is 47.66 GB that is one event, then silence for hours — and nothing at all
/// in the ledger, which only got written on terminal success. An operator (or a UI, or Positron)
/// had no way to distinguish a live download from a wedged one. Measured the hard way tonight:
/// filesystem metadata cannot substitute, because hf-hub PREALLOCATES the blob to full size, so
/// neither file length nor free-disk moves while it fills — only mtime does, and mtime cannot say
/// how far along it is.
///
/// Throttled by both time and delta so a 76 GB download costs a bounded number of tiny writes.
#[derive(Clone)]
struct PullProgress {
    run_id: Option<String>,
    model_id: String,
    repo_id: String,
    shard: String,
    shard_index: usize,
    shard_count: usize,
    prior_bytes: u64, // bytes completed in EARLIER shards, so percent is whole-pull, not per-shard
    state: Arc<std::sync::Mutex<PullProgressState>>,
}

struct PullProgressState {
    shard_total: u64,
    shard_done: u64,
    last_emit: std::time::Instant,
    last_emit_bytes: u64,
}

impl PullProgress {
    /// Emit at most every 2s AND at least every 256 MB — the first bounds writes on a fast link,
    /// the second keeps a slow link from looking frozen.
    const EMIT_EVERY: std::time::Duration = std::time::Duration::from_secs(2);
    const EMIT_BYTES: u64 = 256 * 1024 * 1024;

    fn emit(&self, force: bool) {
        let (shard_total, shard_done) = {
            let mut st = match self.state.lock() {
                Ok(st) => st,
                Err(_) => return,
            };
            let now = std::time::Instant::now();
            let due = force
                || now.duration_since(st.last_emit) >= Self::EMIT_EVERY
                || st.shard_done.saturating_sub(st.last_emit_bytes) >= Self::EMIT_BYTES;
            if !due {
                return;
            }
            st.last_emit = now;
            st.last_emit_bytes = st.shard_done;
            (st.shard_total, st.shard_done)
        };

        let done = self.prior_bytes + shard_done;
        let payload = serde_json::json!({
            "run_id":         self.run_id,
            "model_id":       self.model_id,
            "repo_id":        self.repo_id,
            "phase":          "downloading",
            "shard":          self.shard,
            "shard_index":    self.shard_index,
            "shard_count":    self.shard_count,
            "shard_bytes":    shard_done,
            "shard_total":    shard_total,
            "bytes_so_far":   done,
            "shard_percent":  if shard_total > 0 {
                                  (shard_done as f64 / shard_total as f64 * 100.0).round()
                              } else { 0.0 },
        });
        if let Some(bus) = crate::runtime::MessageBus::global() {
            bus.publish_async_only("models:pull:progress", payload.clone());
        }
        // The ledger is the POLLABLE half of the contract — the ack names this exact path.
        if let Some(rid) = self.run_id.as_deref() {
            if let (Some(path), Ok(json)) = (
                models_pull_ledger_path(rid),
                serde_json::to_string_pretty(&payload),
            ) {
                let _ = std::fs::write(path, json);
            }
        }
    }
}

impl hf_hub::api::tokio::Progress for PullProgress {
    async fn init(&mut self, size: usize, _filename: &str) {
        if let Ok(mut st) = self.state.lock() {
            st.shard_total = size as u64;
            st.shard_done = 0;
            st.last_emit_bytes = 0;
        }
        self.emit(true);
    }

    async fn update(&mut self, size: usize) {
        if let Ok(mut st) = self.state.lock() {
            st.shard_done = st.shard_done.saturating_add(size as u64);
        }
        self.emit(false);
    }

    async fn finish(&mut self) {
        self.emit(true);
    }
}

/// Result file for a detached pull — the SAME progress-ledger convention `agent/solve`
/// and cognition/eval already use (`~/.continuum/progress/<kind>-<run_id>.json`), so one
/// poller/`Positron` tail serves every long-running command instead of a per-command scheme.
fn models_pull_ledger_path(run_id: &str) -> Option<std::path::PathBuf> {
    let base = std::env::var("CONTINUUM_HOME")
        .map(std::path::PathBuf::from)
        .ok()
        .or_else(|| dirs::home_dir().map(|h| h.join(".continuum")))?;
    let dir = base.join("progress");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join(format!("models-pull-{run_id}.json")))
}

crate::action_command! {
    /// Download a local model's GGUF from its source (HuggingFace) and make it
    /// usable live — no reboot. Picks the quant tier from the repo's actual file
    /// listing (prefer the `quant` param, else a balanced default), pulls the
    /// multimodal projector too for a vision model, and records the on-disk path
    /// onto the live catalog (flips the model to Ready). Returns what landed.
    pub struct ModelsPull {
        catalog: Arc<ModelCatalog>,
    }
    name: "models/pull",
    access: Privileged,
    params: ModelsPullParams,
    output: PullReport,
    run(this, _ctx, p) => {
        // 0. FIRE-AND-POLL (#86 shape, same as `agent/solve --detach`). A frontier GGUF is an
        //    hour of downloading; holding the command socket for that is the bug that pattern
        //    exists to prevent. Ack immediately with a run_id, do the work on a task, and let
        //    watchers follow `models:pull:progress` or poll the shared progress ledger.
        //    Re-running the same pull is safe and resumes: the HF cache is content-addressed.
        if p.detach.unwrap_or(false) {
            // PRE-FLIGHT before the ack. Validation used to live entirely inside `pull_body`, i.e.
            // AFTER the caller had already been handed `detached: true` and a run_id — so pulling a
            // model with no `gguf_hint` returned a cheerful "started detached", then failed in the
            // background with nothing to show for it. MEASURED: `models/pull --model_id
            // deepseek-v4-flash --detach` acked, moved zero bytes, and wrote no ledger; the same
            // command WITHOUT --detach fails loud and correctly. A request that cannot possibly
            // succeed must never be allowed to detach into silence.
            Self::preflight(&this.catalog, &p.model_id)?;

            let run_id = p.run_id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let model_ack = p.model_id.clone();
            let model_failed = p.model_id.clone();
            let catalog = this.catalog.clone();
            let mut inner = p;
            inner.detach = Some(false);
            inner.run_id = Some(run_id.clone());
            let run_id_ack = run_id.clone();
            tokio::spawn(async move {
                match ModelsPull::pull_body(catalog, inner).await {
                    Ok(r) => tracing::info!(run_id = %run_id, bytes = r.bytes, "models/pull detached complete"),
                    Err(e) => {
                        tracing::error!(run_id = %run_id, error = %e, "models/pull detached FAILED");
                        // Write the FAILURE to the same ledger the ack told the caller to poll.
                        // Previously only the success path wrote it (at the end of pull_body), so a
                        // failed detached pull left a watcher polling a path that would never exist
                        // — indistinguishable from "still downloading", forever. A terminal outcome
                        // must always land where the contract said it would, whichever way it went.
                        let failed = PullReport {
                            gguf_file: String::new(),
                            gguf_path: String::new(),
                            mmproj_file: None,
                            bytes: 0,
                            detail: format!("pull of '{model_failed}' FAILED: {e}"),
                            detached: true,
                            run_id: Some(run_id.clone()),
                            error: Some(e.to_string()),
                        };
                        if let (Some(path), Ok(json)) = (
                            models_pull_ledger_path(&run_id),
                            serde_json::to_string_pretty(&failed),
                        ) {
                            let _ = std::fs::write(path, json);
                        }
                        if let Some(bus) = crate::runtime::MessageBus::global() {
                            bus.publish_async_only("models:pull:failed", serde_json::json!({
                                "run_id": run_id, "error": e.to_string(),
                            }));
                        }
                    }
                }
            });
            return Ok(PullReport {
                gguf_file: String::new(),
                gguf_path: String::new(),
                mmproj_file: None,
                bytes: 0,
                detail: format!(
                    "pull of '{model_ack}' started detached — poll ~/.continuum/progress/models-pull-{run_id_ack}.json \
                     or watch models:pull:progress"
                ),
                detached: true,
                run_id: Some(run_id_ack),
                error: None,
            });
        }

        Self::pull_body(this.catalog.clone(), p).await
    }
}

impl ModelsPull {
    /// Can this model be pulled at all? Returns the `gguf_hint`, or the reason it cannot.
    ///
    /// ONE definition of "pullable", called from two places that must not disagree: the detached
    /// ack path (so an impossible request fails synchronously instead of detaching into silence)
    /// and `pull_body` (which needs the hint anyway). Splitting these checks into two copies is
    /// how the detached path ends up accepting requests the inline path rejects.
    fn preflight(catalog: &ModelCatalog, model_id: &str) -> Result<String, CommandError> {
        let snap = catalog.snapshot();
        let live = snap.get(model_id).ok_or_else(|| {
            CommandError::NotFound(format!(
                "unknown model id '{model_id}' — call models/list to see the live universe"
            ))
        })?;
        live.model.gguf_hint.clone().ok_or_else(|| {
            CommandError::Invalid(format!(
                "model '{model_id}' has no gguf_hint — it is cloud-served or has no acquirable \
                 GGUF; models/pull only acquires local models"
            ))
        })
    }

    /// The pull body — deliberately ctx-free (the catalog arrives as an Arc), so it runs
    /// inline OR spawned detached through the SAME code path. Mirrors `AgentSolve::solve_body`.
    async fn pull_body(
        catalog: Arc<ModelCatalog>,
        p: ModelsPullParams,
    ) -> Result<PullReport, CommandError> {
        // 1. The model must exist in the live universe.
        let snap = catalog.snapshot();
        let live = snap.get(&p.model_id).ok_or_else(|| {
            CommandError::NotFound(format!(
                "unknown model id '{}' — call models/list to see the live universe",
                p.model_id
            ))
        })?;

        // 2. It must have an acquirable source. No gguf_hint ⇒ cloud / nothing to pull.
        let hint = live.model.gguf_hint.as_deref().ok_or_else(|| {
            CommandError::Invalid(format!(
                "model '{}' has no gguf_hint — it is cloud-served or has no acquirable GGUF; models/pull only acquires local models",
                p.model_id
            ))
        })?;
        let repo_id = hf_repo_id(hint).ok_or_else(|| {
            CommandError::Invalid(format!(
                "model '{}' gguf_hint '{hint}' is not a huggingface.co/<owner>/<repo> reference — only HF acquisition is supported",
                p.model_id
            ))
        })?;
        let wants_vision = live.model.has(Capability::Vision);
        drop(snap);

        // 3. Ask the repo what files it actually has — the authority on quant tiers.
        //    Route the download cache to the configured cold-storage drive (HF_HOME/hub)
        //    so multi-GB GGUFs land on the big/data drive, NOT the system drive.
        let api = {
            let mut b = ApiBuilder::new().with_token(hf_token());
            if let Some(hub) = crate::model_registry::artifacts::huggingface_cache_root() {
                b = b.with_cache_dir(hub);
            }
            // PARALLEL CHUNKS. hf-hub's ApiBuilder defaults to `max_files: 1` — ONE concurrent
            // 10 MB range request — so a 76 GB model came down a single chunk at a time.
            //
            // MEASURED on BigMama 2026-08-04 against this repo, and the measurement matters more
            // than the number: connection count stops helping almost immediately.
            //   1 conn    0.4 MB/s
            //   8 conns   2.1 MB/s
            //   24 conns  1.8 MB/s   <- WORSE; no gain past ~8
            // and the honest ceiling came from the interface counter, not from foreground probes:
            // TOTAL adapter RX was 4.04 MB/s while the pull ran, i.e. the pull already owned the
            // whole link. That box is on WI-FI (865 Mbps link rate, ~32 Mbit/s actual), so the
            // constraint is the medium, not the chunk count — an ethernet cable is worth more here
            // than any value in this constant.
            //
            // Two earlier readings were wrong and are recorded so nobody re-derives them: a
            // single-connection probe taken WHILE the 8-connection pull was running showed
            // 0.2 MB/s, which made scaling look linear and argued for raising this to 16. Every
            // foreground throughput probe competes with the download it is measuring; only the
            // adapter counter is uncontaminated.
            //
            // Overridable because the right value is a property of the OPERATOR'S link, not of
            // this code — and now that the progress ledger reports real throughput, it can be
            // tuned from evidence instead of argued about.
            const DEFAULT_PARALLEL_CHUNKS: usize = 8;
            let parallel_chunks = crate::config_env::read("CONTINUUM_HF_PARALLEL_CHUNKS")
                .and_then(|v| v.trim().parse::<usize>().ok())
                .filter(|n| *n >= 1 && *n <= 64)
                .unwrap_or(DEFAULT_PARALLEL_CHUNKS);
            tracing::info!(
                probe_class = "models.pull.parallelism",
                chunks = parallel_chunks,
                "hf download parallelism"
            );
            b = b.with_max_files(parallel_chunks);
            b.build()
                .map_err(|e| CommandError::Internal(format!("hf-hub init failed: {e}")))?
        };
        let repo = api.model(repo_id.clone());
        let info = repo
            .info()
            .await
            .map_err(|e| CommandError::Internal(format!("could not list repo '{repo_id}': {e}")))?;
        let files: Vec<String> = info.siblings.into_iter().map(|s| s.rfilename).collect();

        // 4. Choose the weight format, then the entrypoint file (and, for vision GGUF, the
        //    projector). `auto` prefers GGUF whenever the repo publishes one, so every repo that
        //    resolved before this parameter existed resolves to the same file it always did.
        let format = PullFormat::resolve(p.format.as_deref(), &files)?;
        let (main_file, companions) = match format {
            PullFormat::Gguf => (pick_gguf(&files, p.quant.as_deref())?, Vec::new()),
            PullFormat::Safetensors => {
                // A quant tier names a GGUF tier; there is no such thing in the source format.
                // Silently ignoring it would hand back bf16 weights while the caller believed
                // they had asked for Q4 — so refuse instead of quietly disregarding the ask.
                if let Some(q) = p.quant.as_deref() {
                    return Err(CommandError::Invalid(format!(
                        "quant '{q}' is meaningless for format 'safetensors' — quant tiers name \
                         GGUF tiers, and safetensors is the unquantized source format. Drop \
                         --quant, or pull format 'gguf'."
                    )));
                }
                (pick_safetensors(&files)?, source_companions(&files))
            }
        };
        let mmproj_file = if wants_vision && format == PullFormat::Gguf {
            pick_mmproj(&files)
        } else {
            None
        };

        // 5. Download the FULL shard set (giants ship as N shards) with retry/backoff, into the
        //    HF cache (content-addressed → completed shards are skipped instantly on a retry, so
        //    a mid-set failure resumes from the dropped shard, not from zero).
        let shard_set = expand_shard_set(&main_file, &files);
        let shard_count = shard_set.len();
        let mut gguf_path = None;
        let mut bytes = 0u64;
        for (shard_idx, shard) in shard_set.iter().enumerate() {
            // OBSERVABILITY: a tens-of-GB pull is otherwise a silent hour. One event per shard
            // boundary lets the UI / a persona / Positron show real progress and lets an operator
            // tell "still going" from "wedged". Cheap (N events for N shards, not per byte) and
            // Noop-safe: no bus (headless/tests) => no cost. Carries run_id so a detached pull's
            // events correlate with its ledger file.
            // Byte-level progress, not one event per shard boundary. The old shape emitted a
            // single event as a 47.66 GB shard STARTED and then nothing until it finished, which
            // is indistinguishable from a wedged download for hours. The reporter below writes the
            // ledger the detached ack names, so `poll ~/.continuum/progress/models-pull-<id>.json`
            // finally means something while the pull is running.
            let reporter = PullProgress {
                run_id: p.run_id.clone(),
                model_id: p.model_id.clone(),
                repo_id: repo_id.clone(),
                shard: shard.clone(),
                shard_index: shard_idx + 1,
                shard_count,
                prior_bytes: bytes,
                state: Arc::new(std::sync::Mutex::new(PullProgressState {
                    shard_total: 0,
                    shard_done: 0,
                    last_emit: std::time::Instant::now(),
                    last_emit_bytes: 0,
                })),
            };
            let dl = download_with_retry(&repo, &repo_id, shard, Some(reporter)).await?;
            bytes += std::fs::metadata(&dl).map(|m| m.len()).unwrap_or(0);
            if gguf_path.is_none() {
                gguf_path = Some(dl); // shard 1 (sorted) is llama.cpp's load entrypoint
            }
        }
        let gguf_path = gguf_path.expect("expand_shard_set never returns empty");
        let mmproj_path = match &mmproj_file {
            Some(f) => Some(download_with_retry(&repo, &repo_id, f, None).await?),
            None => None,
        };
        // Config / tokenizer / index sidecars for a source checkout. Kilobytes next to a
        // multi-hundred-GB weight set, but without them the checkout is not loadable by
        // anything, so they are part of the artifact rather than an optional extra.
        for companion in &companions {
            let dl = download_with_retry(&repo, &repo_id, companion, None).await?;
            bytes += std::fs::metadata(&dl).map(|m| m.len()).unwrap_or(0);
        }

        // 6. Record the artifact onto the live universe — sets the path + flips Ready.
        //
        // ONLY for GGUF. A safetensors checkout is the SOURCE format: the serving path loads
        // GGUF, so attaching a `.safetensors` here would flip the model to Ready and every
        // subsequent serve attempt would fail at load, with the catalog insisting the model
        // was fine. Acquired-but-not-servable is the honest state, and the detail line below
        // says so rather than leaving the caller to discover it at serve time.
        if format == PullFormat::Gguf
            && !catalog.attach_local_artifact(&p.model_id, gguf_path.clone(), mmproj_path)
        {
            return Err(CommandError::Internal(format!(
                "model '{}' vanished from the live catalog during pull",
                p.model_id
            )));
        }

        let shards_note = if shard_count > 1 {
            format!(" ({shard_count} shards)")
        } else {
            String::new()
        };
        let detail = match format {
            PullFormat::Safetensors => format!(
                "pulled {main_file}{shards_note} + {} companion file(s) from {repo_id} — SOURCE \
                 format. NOT servable as-is: the serving path loads GGUF, so this checkout is \
                 tuning/quantization input and the catalog entry stays not-downloaded until a \
                 GGUF is forged from it.",
                companions.len()
            ),
            PullFormat::Gguf => match &mmproj_file {
                Some(f) => {
                    format!("pulled {main_file}{shards_note} + projector {f} from {repo_id}")
                }
                None if wants_vision => format!(
                    "pulled {main_file}{shards_note} from {repo_id}; WARNING: vision model but no mmproj-*.gguf found in repo — vision will be unservable"
                ),
                None => format!("pulled {main_file}{shards_note} from {repo_id}"),
            },
        };

        let report = PullReport {
            gguf_file: main_file,
            gguf_path: gguf_path.to_string_lossy().into_owned(),
            mmproj_file,
            bytes,
            detail,
            detached: false,
            run_id: p.run_id.clone(),
            error: None,
        };
        // A detached pull writes its real report to the shared progress ledger and announces
        // completion on the bus; an inline pull just returns (both publish, so a watcher sees
        // the same terminal event either way).
        if let Some(rid) = p.run_id.as_deref() {
            if let (Some(path), Ok(json)) = (
                models_pull_ledger_path(rid),
                serde_json::to_string_pretty(&report),
            ) {
                let _ = std::fs::write(path, json);
            }
        }
        if let Some(bus) = crate::runtime::MessageBus::global() {
            if let Ok(v) = serde_json::to_value(&report) {
                bus.publish_async_only("models:pull:complete", v);
            }
        }
        Ok(report)
    }
}

/// Turn a `gguf_hint` into the `<owner>/<repo>` id hf-hub's API needs. Returns
/// `None` for a non-HuggingFace hint (e.g. a `docker.io/...` reference) — the
/// caller fails loud naming the unsupported scheme rather than guessing.
fn hf_repo_id(hint: &str) -> Option<String> {
    let body = hint.strip_prefix("huggingface.co/")?;
    let body = body.split(':').next()?.trim_matches('/');
    let parts: Vec<&str> = body.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() < 2 {
        return None;
    }
    // Keep exactly owner/repo (the last two path segments).
    Some(format!(
        "{}/{}",
        parts[parts.len() - 2],
        parts[parts.len() - 1]
    ))
}

/// The HF auth token for gated repos, from the environment. `None` for public
/// repos (the common case). We read the standard `HF_TOKEN` var.
fn hf_token() -> Option<String> {
    std::env::var("HF_TOKEN").ok().filter(|t| !t.is_empty())
}

/// Quant tiers we prefer when the caller does not name one, best-balance first.
/// Q4_K_M is the standard "good enough, half the size" local default.
const QUANT_PREFERENCE: &[&str] = &["q4_k_m", "q4_k_s", "q5_k_m", "q5_k_s", "q8_0", "q6_k"];

/// Weight-file extensions that participate in the `-<idx>-of-<total>` shard convention.
/// Deliberately a SHORT allow-list rather than "any extension": a repo also ships
/// `…-00001-of-00002.json` style sidecars, and treating an arbitrary extension as shardable
/// would sweep those into the weight set.
const SHARDABLE_EXTS: &[&str] = &[".gguf", ".safetensors"];

/// Non-weight files a source-format (safetensors) checkout needs to be loadable at all:
/// architecture config, tokenizer, chat template, and the weight index that maps tensor
/// names to shards. Matched case-insensitively by exact name or suffix.
///
/// Why an explicit list and not "everything that isn't a weight": a source repo also carries
/// READMEs, `.gitattributes`, preview images, and frequently a SECOND full copy of the weights
/// in another format. Pulling "everything" turns a 60 GB acquisition into 200 GB.
const SOURCE_COMPANIONS: &[&str] = &[
    "config.json",
    "generation_config.json",
    "preprocessor_config.json",
    "processor_config.json",
    "tokenizer.json",
    "tokenizer_config.json",
    "tokenizer.model",
    "special_tokens_map.json",
    "vocab.json",
    "merges.txt",
    "chat_template.jinja",
    "chat_template.json",
    "model.safetensors.index.json",
];

/// A large GGUF is published SHARDED — `<base>-00001-of-000NN.gguf`, `…-00002-of-000NN.gguf`,
/// … (GLM-5.2 UD-IQ1_M is 6 shards, Kimi-K2.7 is 8, K3 will be more). llama.cpp loads shard 1
/// and finds the rest BY NAME in the same dir — but only if they were actually pulled. Given
/// the chosen file and the full repo listing, return EVERY shard in its set; for a single-file
/// model, `[chosen]`. Without this, `pull` fetches one shard of six and the model is silently
/// unloadable — the exact failure mode that made me babysit a manual download.
fn expand_shard_set(chosen: &str, all_files: &[String]) -> Vec<String> {
    // Shard suffix: `-<idx>-of-<total><ext>` with `<idx>` all-digits just before `-of-`.
    //
    // The extension is DERIVED from `chosen`, not hardcoded to `.gguf`. Safetensors publish
    // with the identical `-00001-of-00021` convention (`model-00001-of-00021.safetensors`),
    // so the whole shard walker — and with it the byte-level progress, the ledger, and the
    // resume-from-the-shard-that-dropped property — already worked for source weights and was
    // excluded by one string literal. Generalising the predicate is the entire change; GGUF
    // behaviour is bit-identical because the derived extension IS `.gguf` for a GGUF.
    let Some(of_at) = chosen.rfind("-of-") else {
        return vec![chosen.to_string()];
    };
    let before_of = &chosen[..of_at];
    let idx_is_digits = before_of
        .rsplit('-')
        .next()
        .map(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .unwrap_or(false);
    // The artifact extension, lowercased, e.g. `.gguf` or `.safetensors`. A shard set is only
    // a set if every member carries the SAME extension — mixing formats in one set would be
    // a different artifact, not another shard.
    let Some(dot_at) = chosen.rfind('.') else {
        return vec![chosen.to_string()];
    };
    let ext = chosen[dot_at..].to_lowercase();
    if !idx_is_digits || !SHARDABLE_EXTS.contains(&ext.as_str()) {
        return vec![chosen.to_string()];
    }
    // base = everything up to (not incl.) the `-` before `<idx>`; total_suffix = `-of-<total><ext>`.
    let Some(idx_dash) = before_of.rfind('-') else {
        return vec![chosen.to_string()];
    };
    let base = &chosen[..idx_dash];
    let total_suffix = &chosen[of_at..];
    let mut set: Vec<String> = all_files
        .iter()
        .filter(|f| {
            f.starts_with(base) && f.ends_with(total_suffix) && f.to_lowercase().ends_with(&ext)
        })
        .cloned()
        .collect();
    set.sort();
    if set.is_empty() {
        vec![chosen.to_string()]
    } else {
        set
    }
}

/// Download one file with retry + exponential backoff. hf-hub is content-addressed, so a
/// completed shard is skipped instantly on a retry — meaning a mid-set failure resumes from
/// the shard that dropped, not from zero. Transient network / rate-limit hiccups on a
/// multi-hundred-GB pull are the norm, not the exception; one `.get()` with no retry (the old
/// path) turned any blip into a total-command failure a human had to restart.
async fn download_with_retry(
    repo: &hf_hub::api::tokio::ApiRepo,
    repo_id: &str,
    file: &str,
    progress: Option<PullProgress>,
) -> Result<std::path::PathBuf, CommandError> {
    // Cache probe FIRST, exactly as `ApiRepo::get` does internally (cache hit → return, else
    // download). Done here rather than calling `get` so the download half can carry a progress
    // reporter — `get` hard-codes the no-op one. Losing this probe would cost the property the
    // whole resume story rests on: a completed shard skips INSTANTLY on a re-run instead of being
    // re-fetched, which is what makes re-running an interrupted 76 GB pull cheap.
    if let Some(hub) = crate::model_registry::artifacts::huggingface_cache_root() {
        if let Some(hit) = hf_hub::Cache::new(hub)
            .repo(hf_hub::Repo::model(repo_id.to_string()))
            .get(file)
        {
            return Ok(hit);
        }
    }
    const MAX_ATTEMPTS: u32 = 5;
    let mut backoff = std::time::Duration::from_secs(2);
    let mut last_err = String::new();
    for attempt in 1..=MAX_ATTEMPTS {
        let attempt_result = match progress.clone() {
            Some(p) => repo.download_with_progress(file, p).await,
            None => repo.get(file).await,
        };
        match attempt_result {
            Ok(p) => return Ok(p),
            Err(e) => {
                last_err = e.to_string();
                if attempt < MAX_ATTEMPTS {
                    tracing::warn!(
                        probe_class = "models.pull.retry",
                        file = file,
                        attempt = attempt,
                        max = MAX_ATTEMPTS,
                        error = %last_err,
                        "shard download failed — retrying with backoff",
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(std::time::Duration::from_secs(60));
                }
            }
        }
    }
    Err(CommandError::Internal(format!(
        "download of '{file}' failed after {MAX_ATTEMPTS} attempts: {last_err}"
    )))
}

/// Which weight format a pull acquires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PullFormat {
    /// Derived serving artifact. What the serving path loads.
    Gguf,
    /// Source weights, as the model author published them. Tuning / quantization input.
    Safetensors,
}

impl PullFormat {
    /// Resolve the caller's `format` against what the repo actually publishes.
    ///
    /// `auto` (and absent) prefers GGUF whenever the repo has one — that is what makes this
    /// parameter purely additive: every repo that resolved before it existed resolves to the
    /// same file now. An explicit format that the repo does not publish fails loud NAMING what
    /// the repo does carry, rather than falling back to the other format; a caller who asked
    /// for source weights and silently received a Q4 GGUF would have unusable tuning input and
    /// no indication why.
    fn resolve(requested: Option<&str>, files: &[String]) -> Result<Self, CommandError> {
        let has_gguf = files
            .iter()
            .any(|f| f.to_lowercase().ends_with(".gguf") && !is_mmproj(f));
        let has_st = files
            .iter()
            .any(|f| f.to_lowercase().ends_with(".safetensors"));
        let available = || {
            let mut which = Vec::new();
            if has_gguf {
                which.push("gguf");
            }
            if has_st {
                which.push("safetensors");
            }
            if which.is_empty() {
                "none (repo publishes no gguf or safetensors weights)".to_string()
            } else {
                which.join(", ")
            }
        };
        match requested.map(str::trim).map(str::to_lowercase).as_deref() {
            None | Some("") | Some("auto") => {
                if has_gguf {
                    Ok(Self::Gguf)
                } else if has_st {
                    Ok(Self::Safetensors)
                } else {
                    Err(CommandError::NotFound(
                        "repo publishes no .gguf or .safetensors weight files to pull".to_string(),
                    ))
                }
            }
            Some("gguf") if has_gguf => Ok(Self::Gguf),
            Some("safetensors") if has_st => Ok(Self::Safetensors),
            Some(f @ ("gguf" | "safetensors")) => Err(CommandError::NotFound(format!(
                "format '{f}' requested but the repo publishes: {}",
                available()
            ))),
            Some(other) => Err(CommandError::Invalid(format!(
                "unknown format '{other}' — expected 'gguf', 'safetensors', or 'auto'"
            ))),
        }
    }
}

/// Choose the main model GGUF from a repo's file list. A projector (`mmproj-*`)
/// is never the main file. With a requested quant, only a matching file is
/// acceptable (fail loud otherwise — never silently substitute a different
/// tier). Without one, walk the preference ladder, then fall back to the first
/// GGUF deterministically.
fn pick_gguf(files: &[String], requested: Option<&str>) -> Result<String, CommandError> {
    let mut ggufs: Vec<&String> = files
        .iter()
        .filter(|f| f.to_lowercase().ends_with(".gguf"))
        .filter(|f| !is_mmproj(f))
        .collect();
    ggufs.sort(); // deterministic tie-break

    if ggufs.is_empty() {
        return Err(CommandError::NotFound(
            "repo has no .gguf model file to pull".to_string(),
        ));
    }

    if let Some(want) = requested {
        let want = want.to_lowercase();
        return ggufs
            .iter()
            .find(|f| f.to_lowercase().contains(&want))
            .map(|f| (*f).clone())
            .ok_or_else(|| {
                let avail: Vec<&str> = ggufs.iter().map(|f| f.as_str()).collect();
                CommandError::NotFound(format!(
                    "no GGUF matching quant '{want}' in repo; available: {}",
                    avail.join(", ")
                ))
            });
    }

    for tier in QUANT_PREFERENCE {
        if let Some(f) = ggufs.iter().find(|f| f.to_lowercase().contains(tier)) {
            return Ok((*f).clone());
        }
    }
    Ok(ggufs[0].clone())
}

/// Choose the entrypoint safetensors file — the SOURCE weight format, as published.
///
/// Why the source format is acquirable at all: GGUF is a *derived serving artifact*. You
/// cannot LoRA-tune a GGUF, you cannot re-quantize to a tier nobody published, and you cannot
/// forge a device-fit override from one. An acquisition layer that reaches only the derived
/// form structurally blocks the foundry, so every model we might want to tune or re-quant had
/// to be fetched by hand outside the governed path — which is exactly how an artifact lands in
/// the wrong place with no ledger and no resume.
///
/// Sorting makes shard `-00001-of-000NN` the entrypoint deterministically, which is what
/// [`expand_shard_set`] needs to recover the rest of the set; an unsharded repo yields its
/// single `model.safetensors`. Consolidated/duplicate copies (`…consolidated…`) are excluded
/// so a repo shipping both layouts doesn't get pulled twice.
fn pick_safetensors(files: &[String]) -> Result<String, CommandError> {
    let mut sts: Vec<&String> = files
        .iter()
        .filter(|f| f.to_lowercase().ends_with(".safetensors"))
        .filter(|f| !f.to_lowercase().contains("consolidated"))
        .collect();
    sts.sort();
    sts.first().map(|f| (*f).clone()).ok_or_else(|| {
        CommandError::NotFound("repo has no .safetensors weight file to pull".to_string())
    })
}

/// The config / tokenizer / index sidecars that make a safetensors checkout loadable.
/// Derived from the repo's actual listing — never a hardcoded assumption about what a repo
/// ships, so a repo missing `tokenizer.model` simply yields fewer companions rather than
/// failing a download for a file that was never there.
fn source_companions(files: &[String]) -> Vec<String> {
    let mut out: Vec<String> = files
        .iter()
        .filter(|f| {
            let base = f.rsplit('/').next().unwrap_or(f).to_lowercase();
            SOURCE_COMPANIONS.contains(&base.as_str()) || base.ends_with(".index.json")
        })
        .cloned()
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The multimodal projector sibling, if present. Sharded GGUFs aside, a vision
/// repo ships exactly one `mmproj-*.gguf`.
fn pick_mmproj(files: &[String]) -> Option<String> {
    let mut found: Vec<&String> = files.iter().filter(|f| is_mmproj(f)).collect();
    found.sort();
    found.first().map(|f| (*f).clone())
}

fn is_mmproj(filename: &str) -> bool {
    let lower = filename.to_lowercase();
    lower.ends_with(".gguf") && lower.contains("mmproj")
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: THE multi-shard correctness bug. A giant GGUF ships as N shards;
    // picking one and pulling only it leaves the model silently unloadable. expand_shard_set
    // must return EVERY shard of the chosen file's set — and must NOT false-positive a
    // single-file model (or a filename that merely contains "-of-" without a digit index).
    #[test]
    fn shard_set_expands_all_shards_but_leaves_single_files_alone() {
        let sharded = vec![
            "UD-IQ1_M/GLM-5.2-UD-IQ1_M-00001-of-00006.gguf".to_string(),
            "UD-IQ1_M/GLM-5.2-UD-IQ1_M-00002-of-00006.gguf".to_string(),
            "UD-IQ1_M/GLM-5.2-UD-IQ1_M-00003-of-00006.gguf".to_string(),
            "UD-IQ1_M/GLM-5.2-UD-IQ1_M-00004-of-00006.gguf".to_string(),
            "UD-IQ1_M/GLM-5.2-UD-IQ1_M-00005-of-00006.gguf".to_string(),
            "UD-IQ1_M/GLM-5.2-UD-IQ1_M-00006-of-00006.gguf".to_string(),
            "UD-Q4_K_M/GLM-5.2-UD-Q4_K_M-00001-of-00011.gguf".to_string(), // a DIFFERENT quant set
        ];
        let set = expand_shard_set(&sharded[0], &sharded);
        assert_eq!(set.len(), 6, "all 6 IQ1 shards, and NOT the Q4 set");
        assert!(set.contains(&sharded[5]), "the last shard is included");
        assert!(
            !set.contains(&sharded[6]),
            "a different quant's shards are excluded"
        );

        // Single-file model → just itself.
        let single = vec!["qwen3-coder-compacted.Q4_K_M.gguf".to_string()];
        assert_eq!(expand_shard_set(&single[0], &single), single);

        // "-of-" with no digit index before it is NOT a shard (don't false-positive).
        let not_shard = vec!["model-proof-of-concept.gguf".to_string()];
        assert_eq!(expand_shard_set(&not_shard[0], &not_shard), not_shard);
    }

    // what this catches: the shard walker's extension is DERIVED, not hardcoded to `.gguf`.
    // Safetensors publish with the identical `-00001-of-000NN` convention, so a hardcoded
    // `.gguf` predicate silently reduced a 21-shard source checkout to ONE shard — the same
    // unloadable-model failure the test above pins for GGUF. Also pins that a shard set never
    // mixes extensions and never sweeps in a same-named `.index.json` sidecar.
    #[test]
    fn shard_set_walks_safetensors_and_never_mixes_extensions() {
        let files: Vec<String> = vec![
            "model-00001-of-00003.safetensors",
            "model-00002-of-00003.safetensors",
            "model-00003-of-00003.safetensors",
            "model-00001-of-00003.gguf", // a converted copy — a DIFFERENT artifact
            "model.safetensors.index.json",
            "config.json",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();

        let set = expand_shard_set(&files[0], &files);
        assert_eq!(set.len(), 3, "all 3 safetensors shards: {set:?}");
        assert!(
            set.iter().all(|f| f.ends_with(".safetensors")),
            "a shard set never mixes formats: {set:?}"
        );

        // The GGUF entrypoint in the SAME listing still resolves to only GGUF shards.
        let gguf_set = expand_shard_set(&files[3], &files);
        assert_eq!(gguf_set, vec!["model-00001-of-00003.gguf".to_string()]);
    }

    // what this catches: `auto` must resolve exactly the way it did before the `format`
    // parameter existed — GGUF whenever the repo publishes one. If this regresses, every
    // existing pull silently starts fetching unquantized source weights instead (hundreds of
    // GB, and not servable). Also pins that an explicitly requested format the repo lacks
    // fails loud NAMING what is there, instead of falling back to the other format.
    #[test]
    fn format_auto_prefers_gguf_and_explicit_misses_fail_loud() {
        let both: Vec<String> = vec!["m-Q4_K_M.gguf", "model.safetensors", "config.json"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let st_only: Vec<String> = vec!["model.safetensors", "config.json"]
            .into_iter()
            .map(str::to_string)
            .collect();

        assert_eq!(PullFormat::resolve(None, &both).unwrap(), PullFormat::Gguf);
        assert_eq!(
            PullFormat::resolve(Some("auto"), &both).unwrap(),
            PullFormat::Gguf
        );
        // Only safetensors published → auto reaches it rather than failing, which is the
        // whole point: a source-only repo used to be unacquirable through the command at all.
        assert_eq!(
            PullFormat::resolve(None, &st_only).unwrap(),
            PullFormat::Safetensors
        );
        assert_eq!(
            PullFormat::resolve(Some("safetensors"), &both).unwrap(),
            PullFormat::Safetensors
        );

        // Asked for GGUF, repo has none → refuse, and SAY what the repo actually has.
        let err = PullFormat::resolve(Some("gguf"), &st_only).unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("safetensors"),
            "names what IS available: {msg}"
        );

        // An unknown format is a caller error, not a silent default.
        assert!(PullFormat::resolve(Some("onnx"), &both).is_err());

        // A repo with neither is a hard miss.
        assert!(PullFormat::resolve(None, &["README.md".to_string()]).is_err());
    }

    // what this catches: a source checkout is only loadable with its config/tokenizer/index
    // sidecars, and pulling "everything that isn't a weight" would drag in READMEs, images,
    // and frequently a second full copy of the weights — turning a 60 GB acquisition into
    // 200 GB. Pins that the companion set is exactly the loadable-checkout files.
    #[test]
    fn source_companions_take_the_loadable_set_and_nothing_else() {
        let files: Vec<String> = vec![
            "model-00001-of-00002.safetensors",
            "model.safetensors.index.json",
            "config.json",
            "tokenizer.json",
            "tokenizer_config.json",
            "chat_template.jinja",
            "README.md",
            "preview.png",
            ".gitattributes",
            "original/consolidated.00.pth",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();

        let got = source_companions(&files);
        assert_eq!(
            got,
            vec![
                "chat_template.jinja".to_string(),
                "config.json".to_string(),
                "model.safetensors.index.json".to_string(),
                "tokenizer.json".to_string(),
                "tokenizer_config.json".to_string(),
            ],
            "exactly the loadable-checkout sidecars"
        );
        assert!(
            !got.iter().any(|f| f.ends_with(".safetensors")),
            "weights come from the shard walker, never from the companion set"
        );
    }

    // what this catches: `pick_safetensors` must return shard 1 so the shard walker can
    // recover the set, and must skip a `consolidated` duplicate copy — a repo shipping both
    // layouts would otherwise be pulled twice.
    #[test]
    fn safetensors_pick_is_shard_one_and_skips_consolidated_copies() {
        let files: Vec<String> = vec![
            "model-00002-of-00002.safetensors",
            "model-00001-of-00002.safetensors",
            "consolidated/consolidated.safetensors",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        assert_eq!(
            pick_safetensors(&files).unwrap(),
            "model-00001-of-00002.safetensors"
        );

        assert!(
            pick_safetensors(&["config.json".to_string()]).is_err(),
            "no weights → fail loud, never return a config as the model"
        );
    }

    // what this catches: the hint parser yields the owner/repo id hf-hub needs
    // for a HuggingFace hint, strips a `:tag`, and returns None (⇒ caller fails
    // loud) for a non-HF scheme like docker.io — we never feed a bogus id to the
    // download API.
    #[test]
    fn hint_parses_hf_and_rejects_non_hf() {
        assert_eq!(
            hf_repo_id("huggingface.co/bartowski/Qwen2-VL-7B-Instruct-GGUF"),
            Some("bartowski/Qwen2-VL-7B-Instruct-GGUF".to_string())
        );
        assert_eq!(
            hf_repo_id("huggingface.co/mlx-community/qwen2.5-7b-instruct-4bit"),
            Some("mlx-community/qwen2.5-7b-instruct-4bit".to_string())
        );
        assert!(
            hf_repo_id("docker.io/ai/qwen2.5:7B-Q4_K_M").is_none(),
            "a docker hint is not HF-acquirable — must be rejected, not guessed"
        );
    }

    // what this catches: quant selection honors an explicit request, never picks
    // a projector as the main file, prefers Q4_K_M by default, and fails loud
    // (not silent-substitute) when a requested quant is absent.
    #[test]
    fn quant_selection_is_explicit_and_loud() {
        let files = vec![
            "Qwen2-VL-7B-Instruct-Q4_K_M.gguf".to_string(),
            "Qwen2-VL-7B-Instruct-Q8_0.gguf".to_string(),
            "mmproj-Qwen2-VL-7B-Instruct-f16.gguf".to_string(),
            "README.md".to_string(),
        ];
        // Default ⇒ Q4_K_M (the balanced default), never the projector.
        assert_eq!(
            pick_gguf(&files, None).unwrap(),
            "Qwen2-VL-7B-Instruct-Q4_K_M.gguf"
        );
        // Explicit request honored.
        assert_eq!(
            pick_gguf(&files, Some("q8_0")).unwrap(),
            "Qwen2-VL-7B-Instruct-Q8_0.gguf"
        );
        // Absent requested tier ⇒ fail loud, never substitute.
        assert!(matches!(
            pick_gguf(&files, Some("q2_k")),
            Err(CommandError::NotFound(_))
        ));
        // The projector is picked separately.
        assert_eq!(
            pick_mmproj(&files).unwrap(),
            "mmproj-Qwen2-VL-7B-Instruct-f16.gguf"
        );
    }

    // what this catches: a repo with no model GGUF fails loud rather than
    // returning a non-gguf file or panicking on an empty list.
    #[test]
    fn no_gguf_fails_loud() {
        let files = vec!["README.md".to_string(), "config.json".to_string()];
        assert!(matches!(
            pick_gguf(&files, None),
            Err(CommandError::NotFound(_))
        ));
        assert!(pick_mmproj(&files).is_none());
    }

    #[test]
    fn name_mirrors_path() {
        use crate::sdk_codegen::ActionCommand;
        assert_eq!(ModelsPull::NAME, "models/pull");
    }
}
