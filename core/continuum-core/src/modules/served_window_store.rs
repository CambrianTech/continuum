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
    /// The lane count that served beside that window — the other half of the geometry a
    /// cold boot plans first (9/18: a window alone let the boot launch 8 lanes at 28k).
    /// Absent on an older record (default 0 = unknown): the window still carries over.
    #[serde(default)]
    pub lanes: u32,
    /// The typical prompt (median sent tokens, residents AND leased-in) the seat served at
    /// its last steady plan — the per-lane FLOOR a cold boot starts from. Without it the
    /// floor is the 16,384 bootstrap prior until enough prompts land: the 5090 on
    /// 2026-09-19 00:3xZ planned 2 × 17k for coders sending ~30k, because the leased-in
    /// sample ring is process-static and empty at boot. 0 = unknown (older record).
    #[serde(default)]
    pub typical_prompt_tokens: u32,
    pub set_at_ms: u64,
}

impl StoredServedWindow {
    /// The geometry a cold boot may plan FIRST — or `None` when the record is a COLLAPSE,
    /// not a steady state. A window below the typical prompt the same record remembers
    /// cannot have served a turn: it is what the plan shed to under pressure (the M5 at
    /// 2026-09-20 07:52Z: 1 × 2,048 with residents sending ~30k; the 5090 the same
    /// afternoon, refusing every 17–31k prompt against a 2,048 slot). Remembering it as
    /// "the last steady geometry" seeds the NEXT boot with the collapse — its lane count
    /// caps the boot's choice at one lane and its window joins the floor — so a node that
    /// fell over once boots into the fall. An unknown typical (0, an older record) never
    /// disqualifies: the window carries over as before. Zero lanes is no geometry.
    pub fn steady_geometry(&self) -> Option<(u32, u32)> {
        if self.lanes == 0 {
            return None;
        }
        if self.typical_prompt_tokens > 0 && self.per_slot_window < self.typical_prompt_tokens {
            return None;
        }
        Some((self.per_slot_window, self.lanes))
    }
}

fn path_under(home: &Path) -> PathBuf {
    home.join("state").join(FILE)
}

fn default_path() -> Option<PathBuf> {
    crate::commands::benchmark::continuum_home()
        .ok()
        .map(|h| path_under(&h))
}

/// THE LAST STEADY GEOMETRY this host served, whatever the model — a cold boot's first
/// choice (the caller checks the model is still a candidate). Missing/corrupt = `None`.
pub fn load_geometry() -> Option<StoredServedWindow> {
    default_path().and_then(|p| load_from(&p))
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

/// Remember what actually served — the window AND the lane count. Called by the spawn;
/// idempotent. Keeps the record's typical prompt (the plan tick owns that half).
pub fn save(model_id: &str, per_slot_window: u32, lanes: u32) {
    if let Some(p) = default_path() {
        let typical = load_from(&p).filter(|s| s.model_id == model_id).map_or(0, |s| s.typical_prompt_tokens);
        save_to(&p, model_id, per_slot_window, lanes, typical);
    }
}

/// Remember the typical prompt the seat serves (the plan tick, once measured); the
/// window and lanes halves are kept. Idempotent; a 0 never overwrites a measurement.
pub fn save_typical_prompt(model_id: &str, typical_prompt_tokens: u32) {
    if typical_prompt_tokens == 0 {
        return;
    }
    if let Some(p) = default_path() {
        let Some(s) = load_from(&p).filter(|s| s.model_id == model_id) else { return };
        save_to(&p, model_id, s.per_slot_window, s.lanes, typical_prompt_tokens);
    }
}

pub fn save_to(path: &Path, model_id: &str, per_slot_window: u32, lanes: u32, typical_prompt_tokens: u32) {
    if load_from(path).is_some_and(|s| {
        s.model_id == model_id && s.per_slot_window == per_slot_window && s.lanes == lanes && s.typical_prompt_tokens == typical_prompt_tokens
    }) {
        return;
    }
    let set_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0); // JUSTIFIED unwrap_or: a clock before the epoch stamps 0 — the stamp is informational, the window is the fact
    let stored = StoredServedWindow { model_id: model_id.to_string(), per_slot_window, lanes, typical_prompt_tokens, set_at_ms };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_vec_pretty(&stored).map(|b| std::fs::write(path, b)) {
        Ok(Ok(())) => crate::probe!(
            class = "serving.window.stored",
            model = model_id,
            per_slot_window = per_slot_window as u64,
            lanes = lanes as u64,
            typical_prompt_tokens = typical_prompt_tokens as u64,
            "served geometry remembered (window, lanes, typical prompt) — the next boot plans it first, so KV pages carry over and the first launch is the steady one"
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

    // what this catches: a collapsed geometry must not seed the next boot as the steady one.
    // Regression for the M5 (2026-09-20 08:43Z booted 1 × 2,048 from the record its 07:52Z
    // collapse wrote) and the 5090 (serving 1 × 2,048 all afternoon, refusing 17–31k prompts):
    // a window below the typical prompt the same record remembers is a fall, not a state.
    #[test]
    fn a_window_below_the_typical_prompt_is_a_collapse_not_a_steady_geometry() {
        let rec = |window: u32, lanes: u32, typical: u32| StoredServedWindow {
            model_id: "m".into(),
            per_slot_window: window,
            lanes,
            typical_prompt_tokens: typical,
            set_at_ms: 1,
        };
        assert_eq!(rec(2_048, 1, 30_000).steady_geometry(), None, "the collapse seeds nothing");
        assert_eq!(rec(67_072, 2, 30_000).steady_geometry(), Some((67_072, 2)), "a window that served the residents' prompts is steady");
        assert_eq!(rec(2_048, 1, 0).steady_geometry(), Some((2_048, 1)), "an unknown typical never disqualifies");
        assert_eq!(rec(67_072, 0, 30_000).steady_geometry(), None, "zero lanes is no geometry");
        assert_eq!(rec(30_000, 1, 30_000).steady_geometry(), Some((30_000, 1)), "a window equal to the typical served it");
    }

    // what this catches: the store answering for a different model (a foreign
    // window would key the page dir wrong), and a corrupt file surviving a read.
    #[test]
    fn the_store_answers_only_for_its_own_model_and_discards_corruption() {
        let dir = tempfile::tempdir().unwrap(); // JUSTIFIED unwrap: test scaffolding
        let path = dir.path().join("state").join("served-window.json");
        save_to(&path, "a/model", 65_536, 2, 0);
        let s = load_from(&path).expect("stored");
        assert_eq!((s.model_id.as_str(), s.per_slot_window, s.lanes), ("a/model", 65_536, 2));
        // what this catches (2026-09-19, the 5090's 2 × 17k): the typical prompt is the
        // record's third half — set by the plan tick, kept by the spawn's save, never
        // overwritten by a 0, and answering 0 = unknown on an older record.
        save_to(&path, "a/model", 65_536, 2, 30_000);
        assert_eq!(load_from(&path).map(|s| s.typical_prompt_tokens), Some(30_000));
        save_to(&path, "a/model", 67_340, 1, load_from(&path).map_or(0, |s| s.typical_prompt_tokens));
        assert_eq!(load_from(&path).map(|s| (s.per_slot_window, s.lanes, s.typical_prompt_tokens)), Some((67_340, 1, 30_000)), "a spawn keeps the measured prompt");
        // An older record (window only) still carries its window; lanes read as unknown (0).
        std::fs::write(&path, br#"{"model_id":"a/model","per_slot_window":40000,"set_at_ms":1}"#).unwrap(); // JUSTIFIED unwrap: test scaffolding
        let old = load_from(&path).expect("old record reads");
        assert_eq!((old.per_slot_window, old.lanes, old.typical_prompt_tokens), (40_000, 0, 0));
        std::fs::write(&path, b"{not json").unwrap(); // JUSTIFIED unwrap: test scaffolding
        assert!(load_from(&path).is_none(), "corrupt store reads as absent");
        assert!(!path.exists(), "and is discarded");
    }
}
