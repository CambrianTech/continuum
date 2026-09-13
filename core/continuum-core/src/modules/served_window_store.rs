//! The served per-slot window, remembered ACROSS RUNS so the KV page store's
//! geometry key is stable and a citizen's pages survive a reboot.
//!
//! Measured 2026-09-13 on one host + one model: 83,968 → 102,656 → 126,464 →
//! 138,240 per slot across four boots (the planner re-derived the window from the
//! moving measured demand each time). The page dir is keyed `<model>--c<window>`
//! and the spawn sweeps every other generation, so NO page ever survived a
//! reboot — every citizen re-prefilled cold after every deploy. Joel: "virtual
//! disk paging … managed and maintained across runs." One writer (the spawn, the
//! place that knows what actually served), one reader (the plan, which prefers a
//! stored window that still fits over a fresh derivation).
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FILE: &str = "served-window.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredServedWindow {
    pub model_id: String,
    pub per_slot_window: u32,
    pub set_at_ms: u64,
}

fn path_under(home: &Path) -> PathBuf {
    home.join("state").join(FILE)
}

fn default_path() -> Option<PathBuf> {
    crate::commands::benchmark::continuum_home()
        .ok()
        .map(|h| path_under(&h))
}

/// The remembered window for `model_id`, if the store holds one for THAT model.
/// Missing or foreign = `None` silently; corrupt = `None` with a probe.
pub fn load_for(model_id: &str) -> Option<u32> {
    default_path().and_then(|p| load_from(&p)).filter(|s| s.model_id == model_id).map(|s| s.per_slot_window)
}

pub fn load_from(path: &Path) -> Option<StoredServedWindow> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            crate::probe!(
                class = "serving.window.store_unreadable",
                path = %path.display(),
                error = %e,
                "served-window store unreadable — planning fresh (pages will not carry over)"
            );
            return None;
        }
    };
    match serde_json::from_slice::<StoredServedWindow>(&bytes) {
        Ok(s) if s.per_slot_window > 0 => Some(s),
        _ => {
            crate::probe!(
                class = "serving.window.store_corrupt",
                path = %path.display(),
                "served-window store corrupt — discarded; planning fresh"
            );
            let _ = std::fs::remove_file(path);
            None
        }
    }
}

/// Remember what actually served. Called by the spawn; idempotent.
pub fn save(model_id: &str, per_slot_window: u32) {
    if let Some(p) = default_path() {
        save_to(&p, model_id, per_slot_window);
    }
}

pub fn save_to(path: &Path, model_id: &str, per_slot_window: u32) {
    if load_from(path).is_some_and(|s| s.model_id == model_id && s.per_slot_window == per_slot_window) {
        return;
    }
    let set_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0); // JUSTIFIED unwrap_or: a clock before the epoch stamps 0 — the stamp is informational, the window is the fact
    let stored = StoredServedWindow { model_id: model_id.to_string(), per_slot_window, set_at_ms };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_vec_pretty(&stored).map(|b| std::fs::write(path, b)) {
        Ok(Ok(())) => crate::probe!(
            class = "serving.window.stored",
            model = model_id,
            per_slot_window = per_slot_window as u64,
            "served per-slot window remembered — the next boot plans this geometry first, so KV pages carry over"
        ),
        _ => crate::probe!(
            class = "serving.window.store_failed",
            path = %path.display(),
            "could not write the served-window store — the next boot re-derives the window"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the store answering for a different model (a foreign
    // window would key the page dir wrong), and a corrupt file surviving a read.
    #[test]
    fn the_store_answers_only_for_its_own_model_and_discards_corruption() {
        let dir = tempfile::tempdir().unwrap(); // JUSTIFIED unwrap: test scaffolding
        let path = dir.path().join("state").join("served-window.json");
        save_to(&path, "a/model", 65_536);
        let s = load_from(&path).expect("stored");
        assert_eq!((s.model_id.as_str(), s.per_slot_window), ("a/model", 65_536));
        std::fs::write(&path, b"{not json").unwrap(); // JUSTIFIED unwrap: test scaffolding
        assert!(load_from(&path).is_none(), "corrupt store reads as absent");
        assert!(!path.exists(), "and is discarded");
    }
}
