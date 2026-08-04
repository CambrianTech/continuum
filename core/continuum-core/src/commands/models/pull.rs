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
#[ts(export, export_to = "../../../protocol/typescript/model_registry/ModelsPullParams.ts")]
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
#[ts(export, export_to = "../../../protocol/typescript/model_registry/PullReport.ts")]
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
            b.build()
                .map_err(|e| CommandError::Internal(format!("hf-hub init failed: {e}")))?
        };
        let repo = api.model(repo_id.clone());
        let info = repo.info().await.map_err(|e| {
            CommandError::Internal(format!("could not list repo '{repo_id}': {e}"))
        })?;
        let files: Vec<String> = info.siblings.into_iter().map(|s| s.rfilename).collect();

        // 4. Choose the main GGUF (and, for vision, the projector).
        let gguf_file = pick_gguf(&files, p.quant.as_deref())?;
        let mmproj_file = if wants_vision { pick_mmproj(&files) } else { None };

        // 5. Download the FULL shard set (giants ship as N shards) with retry/backoff, into the
        //    HF cache (content-addressed → completed shards are skipped instantly on a retry, so
        //    a mid-set failure resumes from the dropped shard, not from zero).
        let shard_set = expand_shard_set(&gguf_file, &files);
        let shard_count = shard_set.len();
        let mut gguf_path = None;
        let mut bytes = 0u64;
        for (shard_idx, shard) in shard_set.iter().enumerate() {
            // OBSERVABILITY: a tens-of-GB pull is otherwise a silent hour. One event per shard
            // boundary lets the UI / a persona / Positron show real progress and lets an operator
            // tell "still going" from "wedged". Cheap (N events for N shards, not per byte) and
            // Noop-safe: no bus (headless/tests) => no cost. Carries run_id so a detached pull's
            // events correlate with its ledger file.
            if let Some(bus) = crate::runtime::MessageBus::global() {
                bus.publish_async_only(
                    "models:pull:progress",
                    serde_json::json!({
                        "run_id":      p.run_id,
                        "model_id":    p.model_id,
                        "repo_id":     repo_id,
                        "shard":       shard,
                        "shard_index": shard_idx + 1,
                        "shard_count": shard_count,
                        "bytes_so_far": bytes,
                    }),
                );
            }
            let dl = download_with_retry(&repo, shard).await?;
            bytes += std::fs::metadata(&dl).map(|m| m.len()).unwrap_or(0);
            if gguf_path.is_none() {
                gguf_path = Some(dl); // shard 1 (sorted) is llama.cpp's load entrypoint
            }
        }
        let gguf_path = gguf_path.expect("expand_shard_set never returns empty");
        let mmproj_path = match &mmproj_file {
            Some(f) => Some(download_with_retry(&repo, f).await?),
            None => None,
        };

        // 6. Record the artifact onto the live universe — sets the path + flips Ready.
        if !catalog.attach_local_artifact(&p.model_id, gguf_path.clone(), mmproj_path) {
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
        let detail = match &mmproj_file {
            Some(f) => format!("pulled {gguf_file}{shards_note} + projector {f} from {repo_id}"),
            None if wants_vision => format!(
                "pulled {gguf_file}{shards_note} from {repo_id}; WARNING: vision model but no mmproj-*.gguf found in repo — vision will be unservable"
            ),
            None => format!("pulled {gguf_file}{shards_note} from {repo_id}"),
        };

        let report = PullReport {
            gguf_file,
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

/// A large GGUF is published SHARDED — `<base>-00001-of-000NN.gguf`, `…-00002-of-000NN.gguf`,
/// … (GLM-5.2 UD-IQ1_M is 6 shards, Kimi-K2.7 is 8, K3 will be more). llama.cpp loads shard 1
/// and finds the rest BY NAME in the same dir — but only if they were actually pulled. Given
/// the chosen file and the full repo listing, return EVERY shard in its set; for a single-file
/// model, `[chosen]`. Without this, `pull` fetches one shard of six and the model is silently
/// unloadable — the exact failure mode that made me babysit a manual download.
fn expand_shard_set(chosen: &str, all_files: &[String]) -> Vec<String> {
    // Shard suffix: `-<idx>-of-<total>.gguf` with `<idx>` all-digits just before `-of-`.
    let Some(of_at) = chosen.rfind("-of-") else {
        return vec![chosen.to_string()];
    };
    let before_of = &chosen[..of_at];
    let idx_is_digits = before_of
        .rsplit('-')
        .next()
        .map(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .unwrap_or(false);
    if !idx_is_digits || !chosen.to_lowercase().ends_with(".gguf") {
        return vec![chosen.to_string()];
    }
    // base = everything up to (not incl.) the `-` before `<idx>`; total_suffix = `-of-<total>.gguf`.
    let Some(idx_dash) = before_of.rfind('-') else {
        return vec![chosen.to_string()];
    };
    let base = &chosen[..idx_dash];
    let total_suffix = &chosen[of_at..];
    let mut set: Vec<String> = all_files
        .iter()
        .filter(|f| {
            f.starts_with(base) && f.ends_with(total_suffix) && f.to_lowercase().ends_with(".gguf")
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
    file: &str,
) -> Result<std::path::PathBuf, CommandError> {
    const MAX_ATTEMPTS: u32 = 5;
    let mut backoff = std::time::Duration::from_secs(2);
    let mut last_err = String::new();
    for attempt in 1..=MAX_ATTEMPTS {
        match repo.get(file).await {
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
        assert!(!set.contains(&sharded[6]), "a different quant's shards are excluded");

        // Single-file model → just itself.
        let single = vec!["qwen3-coder-compacted.Q4_K_M.gguf".to_string()];
        assert_eq!(expand_shard_set(&single[0], &single), single);

        // "-of-" with no digit index before it is NOT a shard (don't false-positive).
        let not_shard = vec!["model-proof-of-concept.gguf".to_string()];
        assert_eq!(expand_shard_set(&not_shard[0], &not_shard), not_shard);
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
