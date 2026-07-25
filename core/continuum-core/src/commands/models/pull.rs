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
        // 1. The model must exist in the live universe.
        let snap = this.catalog.snapshot();
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
        // Route the download to the configured cold-storage HF cache (a big drive,
        // e.g. D:\continuum-cold\huggingface\hub) instead of hf-hub's ~/.cache
        // default on the SYSTEM drive — a multi-GB model must NOT fill C:. This is
        // the download half of the disk-D routing (artifacts read from the same
        // root; pull must WRITE there). huggingface_cache_root() honors HF_HOME env
        // + config.env, so it's automatic on Windows, Linux, and WSL.
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

        // 5. Download — into the HF cache, content-addressed, re-pull is instant.
        let gguf_path = repo.get(&gguf_file).await.map_err(|e| {
            CommandError::Internal(format!("download of '{gguf_file}' from '{repo_id}' failed: {e}"))
        })?;
        let mmproj_path = match &mmproj_file {
            Some(f) => Some(repo.get(f).await.map_err(|e| {
                CommandError::Internal(format!("download of projector '{f}' from '{repo_id}' failed: {e}"))
            })?),
            None => None,
        };
        let bytes = std::fs::metadata(&gguf_path).map(|m| m.len()).unwrap_or(0);

        // 6. Record the artifact onto the live universe — sets the path + flips Ready.
        if !this.catalog.attach_local_artifact(&p.model_id, gguf_path.clone(), mmproj_path) {
            return Err(CommandError::Internal(format!(
                "model '{}' vanished from the live catalog during pull",
                p.model_id
            )));
        }

        let detail = match &mmproj_file {
            Some(f) => format!("pulled {gguf_file} + projector {f} from {repo_id}"),
            None if wants_vision => format!(
                "pulled {gguf_file} from {repo_id}; WARNING: vision model but no mmproj-*.gguf found in repo — vision will be unservable"
            ),
            None => format!("pulled {gguf_file} from {repo_id}"),
        };

        Ok(PullReport {
            gguf_file,
            gguf_path: gguf_path.to_string_lossy().into_owned(),
            mmproj_file,
            bytes,
            detail,
        })
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
