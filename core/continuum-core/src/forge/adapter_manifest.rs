//! Trained-adapter manifest — the source of truth for which forged genes are
//! loadable into the serving catalog, keyed by the CONTINUUM base model id.
//!
//! Why a manifest and not a directory scan: a gene's PEFT `adapter_config.json`
//! records `base_model_name_or_path` as the HuggingFace base name (e.g.
//! `unsloth/Qwen3.5-4B`), NOT the continuum registry id the serving daemon
//! actually serves (`continuum-ai/qwen3.5-4b-code-forged-GGUF`). The two never
//! string-match, so the gene→served-model association cannot be GUESSED from the
//! bytes on disk — guessing it (then loading a mismatched `--lora`) would crash
//! the spawn, the wrong kind of fail-loud. The association must be RECORDED by
//! the producer, which knows BOTH ids: `forge/export` with `format = gguf-lora`
//! is REQUIRED to carry the continuum `base_model_id` (the same id the gateway
//! serves the base with). This manifest is that record — the minimal
//! ForgeArtifact registry the forge-template architecture (CLAUDE.md) calls for.
//!
//! The serving daemon READS it at (re)spawn to populate `llama-server --lora`
//! flags; an adapter-SET change (a new gene registered, or one retired) forces a
//! relaunch because llama.cpp has no hot-load API. Page-in/out *within* a loaded
//! set stays a per-request `"lora":[{id,scale}]` body field and never relaunches.
//!
//! Persistence is a single JSON array under the forge custody root. Path
//! resolution (where genes live) is deployment shape, config-overridable like
//! other custody paths — unlike the substrate THRESHOLDS that steer cognition,
//! which stay `const`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One trained gene registered as loadable into the serving catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrainedAdapter {
    /// The gene's name — how a page-in request and logs refer to it.
    pub alias: String,
    /// Absolute on-disk path to the GGUF-lora (`llama-server --lora` loads it).
    pub path: PathBuf,
    /// The CONTINUUM base model id this gene was trained for (the id the gateway
    /// serves the base with) — the association the serving daemon filters on.
    /// This is NOT the PEFT `base_model_name_or_path` (an HF id); see the module
    /// docs for why the producer must record the continuum id explicitly.
    pub base_model_id: String,
}

/// Resolve the manifest path. `CONTINUUM_ADAPTER_MANIFEST` overrides (deployment
/// shape); otherwise it lives next to the genes under the forge custody root.
/// A missing HOME with no override is pathological — fail loud rather than
/// silently writing the manifest into a surprise location.
pub fn manifest_path() -> Result<PathBuf, String> {
    if let Some(raw) = crate::config_env::read("CONTINUUM_ADAPTER_MANIFEST") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }
    let home = dirs::home_dir()
        .ok_or_else(|| "cannot resolve home directory for the adapter manifest".to_string())?;
    Ok(home.join(".continuum/forge/gguf-lora/manifest.json"))
}

/// Read the manifest at `path`. A missing file is NOT an error — zero registered
/// genes is the legitimate boot state (serve base-only). A present-but-unparsable
/// file IS an error (corruption surfaces loudly).
pub fn load_from(path: &Path) -> Result<Vec<TrainedAdapter>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| format!("parse adapter manifest {}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("read adapter manifest {}: {e}", path.display())),
    }
}

/// Register `entry` in the manifest at `path`, replacing any prior entry with the
/// same `path` (a re-export of the same gene updates in place). Atomic: write to
/// a temp sibling, then rename over the manifest.
pub fn register_at(path: &Path, entry: TrainedAdapter) -> Result<(), String> {
    let mut list = load_from(path)?;
    list.retain(|a| a.path != entry.path);
    list.push(entry);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create manifest dir {}: {e}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(&list)
        .map_err(|e| format!("serialize adapter manifest: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .map_err(|e| format!("rename {} → {}: {e}", tmp.display(), path.display()))?;
    Ok(())
}

/// Pure filter: the genes registered for `base_model_id`. The whole reason the
/// manifest exists — the continuum id match the on-disk PEFT config can't give.
pub fn for_base(all: &[TrainedAdapter], base_model_id: &str) -> Vec<TrainedAdapter> {
    all.iter()
        .filter(|a| a.base_model_id == base_model_id)
        .cloned()
        .collect()
}

/// Load + filter against the default manifest path. Convenience for the serving
/// daemon's reconcile.
pub fn load() -> Result<Vec<TrainedAdapter>, String> {
    load_from(&manifest_path()?)
}

/// Register against the default manifest path.
pub fn register(entry: TrainedAdapter) -> Result<(), String> {
    register_at(&manifest_path()?, entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_manifest(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "adapter_manifest_{}_{}.json",
            tag,
            std::process::id()
        ))
    }

    // what this catches: a missing manifest is the legitimate boot state (no
    // genes trained yet) → empty, NOT an error. If this regressed to an error,
    // a fresh node would fail to serve base-only.
    #[test]
    fn missing_manifest_loads_empty() {
        let path = tmp_manifest("missing");
        let _ = std::fs::remove_file(&path);
        assert_eq!(load_from(&path).unwrap(), Vec::new());
    }

    // what this catches: register → load round-trips, and re-registering the same
    // PATH replaces in place (a re-export updates the gene, never duplicates it).
    #[test]
    fn register_round_trips_and_dedups_by_path() {
        let path = tmp_manifest("roundtrip");
        let _ = std::fs::remove_file(&path);

        register_at(
            &path,
            TrainedAdapter {
                alias: "coder-v1".into(),
                path: PathBuf::from("/genes/coder.gguf"),
                base_model_id: "continuum-ai/qwen3.5-4b-code-forged-GGUF".into(),
            },
        )
        .unwrap();
        // Re-export the SAME path with a new alias — must replace, not duplicate.
        register_at(
            &path,
            TrainedAdapter {
                alias: "coder-v2".into(),
                path: PathBuf::from("/genes/coder.gguf"),
                base_model_id: "continuum-ai/qwen3.5-4b-code-forged-GGUF".into(),
            },
        )
        .unwrap();

        let list = load_from(&path).unwrap();
        assert_eq!(list.len(), 1, "same path must dedup");
        assert_eq!(list[0].alias, "coder-v2", "latest registration wins");

        let _ = std::fs::remove_file(&path);
    }

    // what this catches: `for_base` filters by the CONTINUUM id, not the HF base
    // name. This is the entire reason the manifest exists — a directory scan
    // reading PEFT `base_model_name_or_path` (an HF id) could never make this
    // association, so loading the wrong gene's `--lora` would crash the spawn.
    #[test]
    fn for_base_filters_by_continuum_id() {
        let all = vec![
            TrainedAdapter {
                alias: "coder".into(),
                path: PathBuf::from("/genes/coder.gguf"),
                base_model_id: "continuum-ai/qwen3.5-4b-code-forged-GGUF".into(),
            },
            TrainedAdapter {
                alias: "vision".into(),
                path: PathBuf::from("/genes/vision.gguf"),
                base_model_id: "continuum-ai/some-other-base".into(),
            },
        ];
        let matched = for_base(&all, "continuum-ai/qwen3.5-4b-code-forged-GGUF");
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].alias, "coder");
    }

    // what this catches: a present-but-corrupt manifest fails LOUD (parse error),
    // never silently reads as empty (which would drop every registered gene and
    // serve base-only as if nothing were trained).
    #[test]
    fn corrupt_manifest_fails_loud() {
        let path = tmp_manifest("corrupt");
        std::fs::write(&path, "{ not json ]").unwrap();
        let err = load_from(&path).expect_err("corrupt manifest must error");
        assert!(err.contains("parse adapter manifest"), "got: {err}");
        let _ = std::fs::remove_file(&path);
    }
}
