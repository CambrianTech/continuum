//! The operator's serving pin, on disk (cards 3160b3d0 / 9552a01e).
//!
//! `serving/pin` is durable intent — "this host serves THIS model" — but the
//! daemon held it only in a `watch` channel, so every reboot lost it and the
//! boot planner picked whatever fit the outgoing server's memory (a 14B coder,
//! a 27B on two lanes, an embedding model on the 5090 — measured 2026-09-06,
//! five re-pins by hand on the M5 in one day). One JSON file under the state
//! dir, written on pin, removed on unpin, read ONCE at daemon construction so
//! the very first plan is computed under the pin. Loud on a corrupt file
//! (discarded, probe), silent on a missing one (the ordinary unpinned host).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FILE: &str = "serving-pin.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredServingPin {
    pub model_id: String,
    pub set_at_ms: u64,
}

/// The pin file as a VALUE the daemon and the `serving/pin` · `serving/unpin`
/// commands hold — never a path resolved from a global at the moment of the write.
///
/// The global was the bug (2026-09-16): `save()` resolved `~/.continuum` on its own,
/// so the `serving/pin` unit test — which drives the production command — wrote the
/// REAL machine's pin file on every `cargo test`, to whichever model sorts first in
/// the catalog (`AtomicChat/Qwen3.8-Flash-Next-GGUF`). Every test run on every dev
/// box re-pinned an undownloaded model for that box's next boot: the #3955 brick,
/// the M5's "five re-pins by hand in one day", the 18:23Z write ten minutes before
/// the 5090 crashed. A handle has no such reach: production builds it under the
/// continuum home once; a test builds it under a tempdir and cannot touch the box.
#[derive(Debug, Clone)]
pub struct ServingPinStore {
    path: PathBuf,
}

impl ServingPinStore {
    /// The one production constructor: the pin file under the continuum home.
    pub fn under_home(home: &Path) -> Self {
        Self {
            path: home.join("state").join(FILE),
        }
    }

    /// A store at an explicit file — tests, and any caller that owns its own root.
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read the stored pin. Missing file = `None` silently; unreadable or corrupt
    /// = `None` with a probe naming the file — never a guessed model.
    pub fn load(&self) -> Option<StoredServingPin> {
        let bytes = match std::fs::read(&self.path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(e) => {
                crate::probe!(
                    class = "serving.pin.store_unreadable",
                    path = %self.path.display(),
                    error = %e,
                    "serving pin file unreadable — booting unpinned, not guessing"
                );
                return None;
            }
        };
        match serde_json::from_slice::<StoredServingPin>(&bytes) {
            Ok(pin) => Some(pin),
            Err(e) => {
                crate::probe!(
                    class = "serving.pin.store_corrupt",
                    path = %self.path.display(),
                    error = %e,
                    "serving pin file corrupt — booting unpinned, not guessing"
                );
                None
            }
        }
    }

    /// Persist the pin (atomic temp + rename). A failed save is loud and non-fatal:
    /// the live pin is set either way; only the next boot loses it.
    pub fn save(&self, model_id: &str) {
        let pin = StoredServingPin {
            model_id: model_id.to_string(),
            set_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0), // unwrap_or: a clock before 1970 is impossible on a running host; 0 only labels the record
        };
        let path = &self.path;
        let result = (|| -> std::io::Result<()> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
            std::fs::write(
                &tmp,
                serde_json::to_vec_pretty(&pin).map_err(std::io::Error::other)?,
            )?;
            std::fs::rename(&tmp, path)
        })();
        match result {
            Ok(()) => crate::probe!(
                class = "serving.pin.stored",
                model_id = %model_id,
                path = %path.display(),
                "serving pin persisted — survives the next reboot"
            ),
            Err(e) => tracing::error!(
                model_id = %model_id,
                path = %path.display(),
                error = %e,
                "serving pin NOT persisted — the pin is live now but the next reboot loses it"
            ),
        }
    }

    /// Remove the stored pin (unpin). Missing = fine.
    pub fn clear(&self) {
        match std::fs::remove_file(&self.path) {
            Ok(()) => {
                crate::probe!(class = "serving.pin.cleared", path = %self.path.display(), "stored serving pin removed")
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!(path = %self.path.display(), error = %e, "stored serving pin could not be removed")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (3160b3d0 / 9552a01e): a pin that does not come back after a
    // restart. Save → load round-trips the model id; clear → load is None; a corrupt
    // file loads as None (unpinned, never a guessed model).
    #[test]
    fn a_pin_survives_a_restart_and_a_corrupt_file_never_becomes_a_model() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ServingPinStore::under_home(dir.path());
        assert!(store.load().is_none(), "missing = unpinned");
        store.save("ornith-ai/Ornith-1.5-35B-A3B-GGUF");
        assert_eq!(
            store.load().map(|p| p.model_id).as_deref(),
            Some("ornith-ai/Ornith-1.5-35B-A3B-GGUF")
        );
        std::fs::write(store.path(), b"{ not json").expect("corrupt");
        assert!(store.load().is_none(), "corrupt = unpinned, not guessed");
        store.save("x/y");
        store.clear();
        assert!(store.load().is_none(), "cleared = unpinned");
    }

    // what this catches (2026-09-16): the store reaching for a global home. The
    // production command's unit test wrote the REAL machine's pin file on every
    // `cargo test` because `save()` resolved `~/.continuum` itself. A handle built
    // from a path has no way to do that; this pins the absence of the reach.
    #[test]
    fn the_store_never_resolves_a_home_on_its_own() {
        let src = include_str!("serving_pin_store.rs");
        let production = src.split("#[cfg(test)]").next().unwrap_or(src);
        for reach in [
            "continuum_home",
            "home_dir",
            "CONTINUUM_HOME",
            "CONTINUUM_ROOT",
        ] {
            assert!(
                !production.contains(reach),
                "the pin store must be handed its path, never resolve one: found `{reach}`"
            );
        }
    }
}
