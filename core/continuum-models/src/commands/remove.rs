//! `models/remove` — free a local model's bytes from disk and make it
//! re-acquirable LIVE (no reboot). The exact inverse of [`models/pull`](super::pull).
//!
//! ## The symmetry this closes
//!
//! Everything allocated can be deallocated. A `models/pull` writes a multi-GB
//! GGUF to disk and flips the live catalog entry to [`Availability::Ready`] with
//! its on-disk path. `models/remove` is its counterpart: it deletes those bytes
//! (the projector too) and flips the entry back to
//! [`Availability::NotDownloaded`] via [`ModelCatalog::detach_local_artifact`] —
//! one generation bump, no reboot. A subscriber sees a `Ready` model become
//! `NotDownloaded`, the disk free again, and serving stop treating it as a
//! candidate on its very next tick. The catalog is the allocation ledger; this
//! command is the disk axis's "free".
//!
//! ## Why it consults serving before deleting (fail loud, never yank)
//!
//! You do not pull weights out from under a live lane. `models/remove` reads the
//! daemon's published [`ServingSnapshot`] (the same `watch` channel adapters
//! point at) and, if the model is the currently `active_model`, **fails loud**
//! telling the caller to free the VRAM axis first (`serving/unload`, the
//! sibling deallocation) — it does not silently refuse, and it does not delete
//! anyway. Disk and VRAM are separate axes of the same catalog; each frees
//! through its own verb, and removing disk requires the VRAM lane already clear.
//!
//! ## Where the bytes actually live
//!
//! The pull lands files in the content-addressed HuggingFace cache: a
//! `snapshots/<rev>/<file>` symlink pointing at a `blobs/<sha>` blob that holds
//! the real bytes. Deleting only the symlink would reclaim nothing — so we
//! canonicalize to the blob, delete the blob (the reclaim), then drop the
//! symlink. A re-pull is content-addressed and re-downloads cleanly.
//!
//! ## Gating
//!
//! `Privileged` — it deletes multi-GB from disk and mutates substrate state. Not
//! an `AiSafe` read.

use std::path::Path;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use ts_rs::TS;

use crate::inference::llama_server::ServingSnapshot;
use crate::model_registry::live::ModelCatalog;
use crate::sdk_codegen::CommandError;

/// Which model's local bytes to free.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/ModelsRemoveParams.ts"
)]
pub struct ModelsRemoveParams {
    /// The model id as it appears in `models/list`. Fails loud if it is unknown,
    /// if it has no local artifact (cloud-served or already not-downloaded), or
    /// if it is the model currently being served (free the VRAM lane first).
    pub model_id: String,
}

/// What `models/remove` freed: the files deleted and the bytes reclaimed. The
/// command's return DTO — not stored on status.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/RemoveReport.ts"
)]
pub struct RemoveReport {
    /// Absolute paths actually deleted (the GGUF blob, its symlink, the
    /// projector blob/symlink). Empty only if the bytes were already gone.
    pub removed: Vec<String>,
    /// Total bytes reclaimed from disk.
    #[ts(type = "number")]
    pub bytes_freed: u64,
    /// Human-readable summary (model, whether the projector came too, idempotent
    /// reconcile note if the bytes were already absent).
    pub detail: String,
}

crate::action_command! {
    /// Delete a local model's GGUF (and its multimodal projector) from disk and
    /// make it re-acquirable live — no reboot. The inverse of models/pull: frees
    /// the bytes, flips the live catalog entry back to NotDownloaded. Fails loud
    /// if the model is unknown, has nothing on disk, or is currently being served
    /// (free the VRAM lane with serving/unload first — never yank a live lane).
    /// Returns what was freed.
    pub struct ModelsRemove {
        catalog: Arc<ModelCatalog>,
        serving: watch::Receiver<ServingSnapshot>,
    }
    name: "models/remove",
    access: Privileged,
    params: ModelsRemoveParams,
    output: RemoveReport,
    run(this, _ctx, p) => {
        // 1. The model must exist in the live universe.
        let snap = this.catalog.snapshot();
        let live = snap.get(&p.model_id).ok_or_else(|| {
            CommandError::NotFound(format!(
                "unknown model id '{}' — call models/list to see the live universe",
                p.model_id
            ))
        })?;

        // 2. It must have local bytes to free. No gguf_local_path ⇒ cloud-served
        //    or already not-downloaded ⇒ fail loud (nothing to deallocate).
        let gguf_path = live.model.gguf_local_path.clone().ok_or_else(|| {
            CommandError::Invalid(format!(
                "model '{}' has no local artifact — it is cloud-served or already not-downloaded; models/remove only frees local bytes",
                p.model_id
            ))
        })?;
        let mmproj_path = live.model.mmproj_local_path.clone();
        drop(snap);

        // 3. Never yank weights from a live lane. If this model is the one the
        //    serving daemon is currently hosting, free the VRAM axis first.
        if this.serving.borrow().active_model.as_deref() == Some(p.model_id.as_str()) {
            return Err(CommandError::Invalid(format!(
                "model '{}' is currently being served — free the VRAM lane first (serving/unload), then models/remove; refusing to delete weights out from under a live lane",
                p.model_id
            )));
        }

        // 4. Free the bytes: the GGUF blob + symlink, and the projector if present.
        let mut removed = Vec::new();
        let mut bytes_freed = 0u64;
        let mut already_absent = false;
        for path in std::iter::once(gguf_path).chain(mmproj_path.into_iter()) {
            match free_file(&path) {
                Ok((paths, bytes)) => {
                    if paths.is_empty() {
                        already_absent = true;
                    }
                    removed.extend(paths);
                    bytes_freed += bytes;
                }
                Err(e) => {
                    return Err(CommandError::Internal(format!(
                        "failed to free '{}' for model '{}': {e}",
                        path.display(),
                        p.model_id
                    )));
                }
            }
        }

        // 5. Reconcile the live universe — clear the paths + flip NotDownloaded.
        if !this.catalog.detach_local_artifact(&p.model_id) {
            return Err(CommandError::Internal(format!(
                "model '{}' vanished from the live catalog during remove",
                p.model_id
            )));
        }

        let mb = bytes_freed / (1024 * 1024);
        let detail = if already_absent && removed.is_empty() {
            format!(
                "model '{}' bytes were already absent on disk — reconciled the live catalog to NotDownloaded ({mb} MB freed)",
                p.model_id
            )
        } else {
            format!(
                "freed {} file(s) ({mb} MB) for model '{}', flipped NotDownloaded",
                removed.len(),
                p.model_id
            )
        };

        Ok(RemoveReport {
            removed,
            bytes_freed,
            detail,
        })
    }
}

/// Free the real bytes behind a (possibly symlinked) cache path. The HF cache
/// stores a `snapshots/.../<file>` symlink pointing at a `blobs/<sha>` blob that
/// holds the bytes — so we canonicalize to the blob, count + delete it (the
/// reclaim), then drop the symlink if it is a distinct path. Returns the paths
/// actually deleted and the bytes reclaimed. An already-absent file is not an
/// error (idempotent convergence to "removed"); it returns `(vec![], 0)`.
fn free_file(path: &Path) -> std::io::Result<(Vec<String>, u64)> {
    // Already gone (manually deleted, or a dangling symlink) ⇒ nothing to free.
    if std::fs::symlink_metadata(path).is_err() {
        return Ok((Vec::new(), 0));
    }

    let mut removed = Vec::new();
    let mut bytes = 0u64;

    // The blob carrying the real bytes (canonicalize follows the symlink).
    let blob = std::fs::canonicalize(path).ok();
    if let Some(blob) = &blob {
        if let Ok(meta) = std::fs::metadata(blob) {
            bytes = meta.len();
        }
        std::fs::remove_file(blob)?;
        removed.push(blob.to_string_lossy().into_owned());
    }

    // Drop the symlink itself if it is a distinct path still present.
    let is_distinct = blob.as_deref() != Some(path);
    if is_distinct && std::fs::symlink_metadata(path).is_ok() {
        std::fs::remove_file(path)?;
        removed.push(path.to_string_lossy().into_owned());
    }

    Ok((removed, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_mirrors_path() {
        use crate::sdk_codegen::ActionCommand;
        assert_eq!(ModelsRemove::NAME, "models/remove");
    }

    // what this catches: free_file reclaims the REAL bytes behind a symlinked
    // cache entry (deletes the blob, not just the symlink) and counts them — the
    // exact HF-cache shape a pull lands. Deleting only the symlink would silently
    // reclaim nothing.
    #[test]
    fn free_file_deletes_blob_behind_symlink_and_counts_bytes() {
        let dir = std::env::temp_dir().join(format!("models-remove-test-{}", std::process::id()));
        let blobs = dir.join("blobs");
        let snaps = dir.join("snapshots");
        std::fs::create_dir_all(&blobs).unwrap();
        std::fs::create_dir_all(&snaps).unwrap();

        let blob = blobs.join("deadbeef");
        std::fs::write(&blob, vec![7u8; 4096]).unwrap();
        let link = snaps.join("model.gguf");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&blob, &link).unwrap();

        let (removed, bytes) = free_file(&link).unwrap();
        assert_eq!(bytes, 4096, "counts the blob's real bytes");
        assert!(
            !blob.exists(),
            "the blob (real bytes) is deleted, not just the symlink"
        );
        assert!(
            std::fs::symlink_metadata(&link).is_err(),
            "the symlink is dropped too"
        );
        assert_eq!(removed.len(), 2, "both blob and symlink reported as freed");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: freeing an already-absent file is idempotent — it
    // returns (no paths, 0 bytes) instead of erroring, so a remove converges to
    // "deallocated" even if the bytes were deleted out-of-band.
    #[test]
    fn free_file_is_idempotent_when_absent() {
        let ghost = std::env::temp_dir().join("models-remove-nonexistent-xyz.gguf");
        let (removed, bytes) = free_file(&ghost).unwrap();
        assert!(removed.is_empty());
        assert_eq!(bytes, 0);
    }
}
