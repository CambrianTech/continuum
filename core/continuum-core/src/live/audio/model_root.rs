//! Canonical root for downloaded voice/model artifacts (#195).
//!
//! Runtime model artifacts — TTS/STT ONNX, VAD, phonemizer/voice data — live
//! under ONE root. The workers→core/tools restructure moved that root to
//! `tools/models/` (the gitignored download path), but every audio adapter kept
//! stale, CWD-relative `models/…` path constants. So when the core runs from the
//! repo root (its normal CWD), `models/piper/…` pointed at the *tracked* avatar
//! dir — not the voice models in `tools/models/piper/…` — and every local TTS/STT
//! silently reported "model not found". Kokoro even mutated the process CWD
//! (`set_jtag_cwd`) to paper over it — the precise process-CWD dependency this
//! module exists to kill.
//!
//! This is the single source of truth. Adapters resolve
//! [`voice_model_path`]`("piper/…")` — never a bare `models/…` literal, never a
//! per-adapter candidate ladder, never `set_current_dir`.
//!
//! Root resolution (first hit wins; the result is used verbatim):
//!   1. `CONTINUUM_MODELS_DIR` (config.env single-owner) — `start-server.sh` sets
//!      it to an absolute `$REPO_ROOT/tools/models`, making resolution independent
//!      of the process CWD.
//!   2. First existing of `tools/models` then `models`, relative to CWD (the dev
//!      case: the core is launched from the repo root).
//!   3. `~/.continuum/models` (the installed/deployed layout).

use std::path::PathBuf;

/// The canonical root under which downloaded voice model artifacts live.
///
/// Prefer [`voice_model_path`] for resolving a specific artifact; use this only
/// when you need the directory itself.
pub fn voice_model_root() -> PathBuf {
    // 1. Config-owned absolute root (CWD-independent). Single owner per
    //    [[config-env-single-owner]]; the process-env form is the boot-time
    //    injection start-server.sh uses (`export CONTINUUM_MODELS_DIR=$REPO_ROOT/tools/models`)
    //    so a binary launched from any CWD still resolves. config.env wins when
    //    both are set (persistent operator choice over transient boot value).
    let from_cfg = crate::config_env::read("CONTINUUM_MODELS_DIR")
        .or_else(|| std::env::var("CONTINUUM_MODELS_DIR").ok());
    if let Some(dir) = from_cfg {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    // 2. Dev convenience: launched from the repo root, the real download path is
    //    `tools/models`; keep the legacy `models` as a secondary so a repo that
    //    still colocates them resolves too.
    for cand in ["tools/models", "models"] {
        let p = PathBuf::from(cand);
        if p.is_dir() {
            return p;
        }
    }

    // 3. Installed layout — artifacts under the continuum home.
    dirs::home_dir()
        .map(|h| h.join(".continuum").join("models"))
        .unwrap_or_else(|| PathBuf::from("tools/models"))
}

/// Resolve a voice model artifact (file or directory) under the canonical root.
///
/// `rel` is the path *below* the root, e.g. `"piper/en_US-libritts_r-medium.onnx"`
/// or `"kokoro"`. Do not prefix it with `models/` — that prefix is exactly the
/// stale assumption this function removes.
pub fn voice_model_path(rel: &str) -> PathBuf {
    voice_model_root().join(rel)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the resolver must never hand back an empty path or one
    // that still carries the stale `models/` prefix baked in — regression for the
    // #195 CWD-relative-model-root bug that silently broke every local voice model.
    #[test]
    fn voice_model_path_joins_under_root_without_stale_prefix() {
        let p = voice_model_path("piper/en_US-libritts_r-medium.onnx");
        assert!(p.ends_with("piper/en_US-libritts_r-medium.onnx"));
        // The root is non-empty and the rel is appended, not the bare literal.
        assert_ne!(p, PathBuf::from("models/piper/en_US-libritts_r-medium.onnx"));
    }
}
