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

fn path_under(home: &Path) -> PathBuf {
    home.join("state").join(FILE)
}

/// The pin file under the continuum home, or `None` when the home is unknown
/// (the daemon then runs unpinned, as before, and says so).
fn default_path() -> Option<PathBuf> {
    crate::commands::benchmark::continuum_home()
        .ok()
        .map(|h| path_under(&h))
}

/// Read the stored pin. Missing file = `None` silently; unreadable or corrupt
/// = `None` with a probe naming the file — never a guessed model.
pub fn load() -> Option<StoredServingPin> {
    default_path().and_then(|p| load_from(&p))
}

pub fn load_from(path: &Path) -> Option<StoredServingPin> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            crate::probe!(
                class = "serving.pin.store_unreadable",
                path = %path.display(),
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
                path = %path.display(),
                error = %e,
                "serving pin file corrupt — booting unpinned, not guessing"
            );
            None
        }
    }
}

/// Persist the pin (atomic temp + rename). A failed save is loud and non-fatal:
/// the live pin is set either way; only the next boot loses it.
pub fn save(model_id: &str) {
    let Some(path) = default_path() else {
        return;
    };
    save_to(&path, model_id);
}

pub fn save_to(path: &Path, model_id: &str) {
    let pin = StoredServingPin {
        model_id: model_id.to_string(),
        set_at_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0), // unwrap_or: a clock before 1970 is impossible on a running host; 0 only labels the record
    };
    let result = (|| -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
        std::fs::write(&tmp, serde_json::to_vec_pretty(&pin).map_err(std::io::Error::other)?)?;
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
pub fn clear() {
    if let Some(path) = default_path() {
        clear_at(&path);
    }
}

pub fn clear_at(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => crate::probe!(class = "serving.pin.cleared", path = %path.display(), "stored serving pin removed"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!(path = %path.display(), error = %e, "stored serving pin could not be removed"),
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
        let path = dir.path().join("state").join("serving-pin.json");
        assert!(load_from(&path).is_none(), "missing = unpinned");
        save_to(&path, "ornith-ai/Ornith-1.5-35B-A3B-GGUF");
        assert_eq!(
            load_from(&path).map(|p| p.model_id).as_deref(),
            Some("ornith-ai/Ornith-1.5-35B-A3B-GGUF")
        );
        std::fs::write(&path, b"{ not json").expect("corrupt");
        assert!(load_from(&path).is_none(), "corrupt = unpinned, not guessed");
        save_to(&path, "x/y");
        clear_at(&path);
        assert!(load_from(&path).is_none(), "cleared = unpinned");
    }
}
