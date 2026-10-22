//! The served per-slot window, remembered ACROSS RUNS so the KV page store's
//! geometry key is stable and a citizen's pages survive a reboot.
//!
//! Measured 2026-09-13 on one host + one model: 83,968 → 102,656 → 126,464 →
//! 138,240 per slot across four boots (the planner re-derived the window from the
//! moving measured demand each time). The page dir is keyed `<model>--c<window>`
//! and the spawn sweeps every other generation, so NO page ever survived a
//! reboot — every citizen re-prefilled cold after every deploy. Joel: "virtual
//! disk paging … managed and maintained across runs." The serving lifecycle records
//! settled candidates; successful terminal usage observes an exact generation.
//! Both updates belong to this store. The last observed window is only a cache
//! preference: current workload and admission decide what can serve after restart.
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FILE: &str = "served-window.json";

/// Geometry observed under the endpoint owner's generation lease, not a planner proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServedGeometry {
    pub model_id: String,
    pub generation: uuid::Uuid,
    pub per_slot_window: u32,
    pub lanes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredServedWindow {
    pub model_id: String,
    pub per_slot_window: u32,
    /// Observed lane count, never a cap on a future allocation. Zero is unknown.
    #[serde(default)]
    pub lanes: u32,
    /// Compatibility projection of the observed prompt median. Legacy records
    /// also wrote requested demand here; only observed_prompt_tokens is evidence.
    /// Diagnostic history, not a substitute for the current workload's requirement.
    #[serde(default)]
    pub typical_prompt_tokens: u32,
    /// Accepted provider counts. Absent on legacy records whose typical field
    /// mixed unmet demand with actual service; that field is not observed evidence.
    #[serde(default)]
    pub observed_prompt_tokens: Option<u32>,
    pub set_at_ms: u64,
    /// Current settled launch. It does not replace the last observed geometry
    /// until successful usage from this exact generation observes it.
    #[serde(default)]
    pub candidate: Option<ServedGeometry>,
}

impl StoredServedWindow {
    /// Return internally consistent observed cache geometry, not a capacity claim.
    /// Zero lanes denotes a first candidate without successful service yet.
    pub fn cache_geometry(&self) -> Option<(u32, u32)> {
        if self.lanes == 0 {
            return None;
        }
        if self.observed_prompt_tokens.is_some_and(|tokens| self.per_slot_window < tokens) {
            return None;
        }
        Some((self.per_slot_window, self.lanes))
    }
}

fn path_under(home: &Path) -> PathBuf {
    home.join("state").join(FILE)
}

fn default_path() -> Option<PathBuf> {
    crate::paths::continuum_home()
        .ok()
        .map(|h| path_under(&h))
}

/// Historical cache geometry. The caller must check model and current workload fit.
pub fn load_geometry() -> Option<StoredServedWindow> {
    default_path().and_then(|p| load_from(&p))
}
/// The remembered window for `model_id`, if the store holds one for THAT model.
/// Missing or foreign = `None` silently; corrupt = `None` with a probe.
pub fn load_for(model_id: &str) -> Option<u32> {
    default_path().and_then(|p| load_from(&p)).filter(|s| s.model_id == model_id && s.cache_geometry().is_some()).map(|s| s.per_slot_window)
}

pub fn load_from(path: &Path) -> Option<StoredServedWindow> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            continuum_base::probe!(
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
            continuum_base::probe!(
                class = "serving.window.store_corrupt",
                path = %path.display(),
                "served-window store corrupt — discarded; planning fresh"
            );
            let _ = std::fs::remove_file(path);
            None
        }
    }
}

/// Record a settled launch separately from the last successfully observed geometry.
/// The serving reconciler calls this under its existing ServingOperation guard;
/// authority planning only reads the store and never races a second writer.
pub fn record_reconciled(geometry: ServedGeometry, settled_now: bool, observed_prompt: Option<u32>) {
    if let Some(path) = default_path() {
        record_reconciled_to(&path, geometry, settled_now, observed_prompt);
    }
}

fn record_reconciled_to(path: &Path, geometry: ServedGeometry, settled_now: bool, observed_prompt: Option<u32>) {
    if settled_now {
        save_candidate_to(path, geometry.clone());
    }
    // Terminal usage commonly arrives after the one-shot settlement edge. Every
    // subsequent reconcile may observe it; candidate identity rejects early/stale usage.
    if let Some(tokens) = observed_prompt {
        observe_to(path, &geometry, tokens);
    }
}

fn save_candidate_to(path: &Path, geometry: ServedGeometry) {
    let mut stored = load_from(path).unwrap_or_else(|| StoredServedWindow {
        model_id: geometry.model_id.clone(),
        per_slot_window: geometry.per_slot_window,
        lanes: 0, // This launch has no successful observation yet.
        typical_prompt_tokens: 0,
        observed_prompt_tokens: None,
        set_at_ms: continuum_base::time::EpochMs::now().millis(),
        candidate: None,
    });
    let candidate = geometry;
    // Settlement repeats; a later tick cannot rewrite an owned generation's shape.
    if stored.candidate.as_ref().is_some_and(|old| old.generation == candidate.generation) {
        return;
    }
    if stored.candidate.as_ref() != Some(&candidate) {
        stored.candidate = Some(candidate);
        write_record(path, &stored);
    }
}

/// Record usage only for the settled generation that produced it. Successful
/// service does not certify that this geometry satisfies future workloads.
fn observe_to(path: &Path, geometry: &ServedGeometry, tokens: u32) {
    let Some(mut stored) = load_from(path) else { return };
    let Some(_) = stored.candidate.as_ref().filter(|c| *c == geometry) else { return };
    if tokens == 0 || tokens > geometry.per_slot_window || geometry.lanes == 0
    {
        return;
    }
    if stored.model_id == geometry.model_id && stored.per_slot_window == geometry.per_slot_window
        && stored.lanes == geometry.lanes && stored.observed_prompt_tokens == Some(tokens)
    {
        return;
    }
    stored.model_id = geometry.model_id.clone();
    stored.per_slot_window = geometry.per_slot_window;
    stored.lanes = geometry.lanes;
    stored.typical_prompt_tokens = tokens;
    stored.observed_prompt_tokens = Some(tokens);
    stored.set_at_ms = continuum_base::time::EpochMs::now().millis();
    write_record(path, &stored);
}
fn write_record(path: &Path, stored: &StoredServedWindow) {
    let model_id = stored.model_id.as_str();
    let per_slot_window = stored.per_slot_window;
    let lanes = stored.lanes;
    let typical_prompt_tokens = stored.typical_prompt_tokens;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_vec_pretty(&stored).map(|b| std::fs::write(path, b)) {
        Ok(Ok(())) => continuum_base::probe!(
            class = "serving.window.stored",
            model = model_id,
            per_slot_window = per_slot_window as u64,
            lanes = lanes as u64,
            typical_prompt_tokens = typical_prompt_tokens as u64,
            "serving geometry recorded — observed cache preference, not a future capacity constraint"
        ),
        _ => continuum_base::probe!(
            class = "serving.window.store_failed",
            path = %path.display(),
            "could not write the served-window store — the next boot re-derives the window"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::planning::serving_plan::{plan_serving_at_rest, HostBudget, ModelFootprint, ServingDemand};
    use crate::inference::planning::window_allocator::LaneRequirement;

    #[test]
    fn observed_collapse_survives_restart_as_history_not_an_allocation_limit() {
        let home = tempfile::tempdir().unwrap();
        let path = path_under(home.path());
        let model = ModelFootprint { model_id: "qwen-27b".into(), weights_bytes: 19 << 30,
            kv_per_token: 33_000, context_window: 262_144, capability_rank: 9, fixed_per_lane_bytes: 0 };
        let host = HostBudget { usable_bytes: 30 << 30, perf_cores: 8 };
        let initial = plan_serving_at_rest(&host, std::slice::from_ref(&model), None,
            &ServingDemand::new(2, Some(70_000)).with_requirements(vec![
                LaneRequirement::declared(uuid::Uuid::new_v4(), 70_000),
                LaneRequirement::declared(uuid::Uuid::new_v4(), 70_000)])).unwrap();
        let large = ServedGeometry { model_id: model.model_id.clone(), generation: uuid::Uuid::new_v4(),
            per_slot_window: initial.served_context_window, lanes: initial.lanes };
        record_reconciled_to(&path, large.clone(), false, Some(60_000));
        assert!(load_from(&path).is_none(), "usage cannot settle an unrecorded launch");
        record_reconciled_to(&path, large.clone(), true, None);
        assert_eq!(load_from(&path).unwrap().cache_geometry(), None);
        // A long turn finishes after settlement: the next reconcile must consume
        // its terminal sample without requiring another settlement edge.
        record_reconciled_to(&path, large.clone(), false, Some(60_000));
        let pressure = HostBudget { usable_bytes: model.peak_resident_bytes(8192, 1), perf_cores: 8 };
        let pressured = plan_serving_at_rest(&pressure, std::slice::from_ref(&model), None,
            &ServingDemand::new(2, Some(70_000))).unwrap();
        assert_eq!(pressured.lanes, 1);
        assert!(pressured.served_context_window < large.per_slot_window);
        let collapse = ServedGeometry { generation: uuid::Uuid::new_v4(), per_slot_window: pressured.served_context_window,
            lanes: pressured.lanes, ..large.clone() };
        record_reconciled_to(&path, collapse.clone(), true, None);
        assert_eq!(load_from(&path).unwrap().cache_geometry(), Some((70_000, 2)));
        // Another endpoint/model, the prior generation, and a rewritten same-generation
        // shape cannot certify the current candidate's usage.
        for foreign in [large.clone(), ServedGeometry { model_id: "neighbor".into(), ..collapse.clone() },
            ServedGeometry { generation: uuid::Uuid::new_v4(), ..collapse.clone() }] {
            observe_to(&path, &foreign, 1000);
        }
        save_candidate_to(&path, ServedGeometry { lanes: 2, ..collapse.clone() });
        assert_eq!(load_from(&path).unwrap().candidate, Some(collapse.clone()));
        record_reconciled_to(&path, collapse.clone(), false, Some(2048));
        let restarted = load_from(&path).unwrap();
        assert_eq!(restarted.cache_geometry(), Some((collapse.per_slot_window, 1)));
        assert_eq!(restarted.observed_prompt_tokens, Some(2048));
        // Both real allocation branches use the same current demand and already
        // credited budget for cold boot and settled reconciliation.
        for requirements in [vec![], vec![LaneRequirement::declared(uuid::Uuid::new_v4(), 70_000),
            LaneRequirement::declared(uuid::Uuid::new_v4(), 70_000)]] {
            // Legacy callers express their current per-lane requirement through
            // the measured prompt median (56k plus existing headroom = 70k).
            // Without it they may legitimately choose two narrower lanes.
            let demand = ServingDemand::new(2, Some(70_000)).with_sent_median(Some(56_000))
                .with_sticky_window(Some(restarted.per_slot_window))
                .with_requirements(requirements);
            let cold = plan_serving_at_rest(&host, std::slice::from_ref(&model), None, &demand).unwrap();
            let settled = plan_serving_at_rest(&host, std::slice::from_ref(&model), Some(&model.model_id), &demand).unwrap();
            assert_eq!(cold.lanes, 2);
            assert!(cold.served_context_window >= 70_000);
            assert_eq!((cold.lanes, cold.served_context_window), (settled.lanes, settled.served_context_window));
            assert!(model.peak_resident_bytes(cold.served_context_window, cold.lanes) <= host.usable_bytes);
            let tight = HostBudget { usable_bytes: model.peak_resident_bytes(cold.served_context_window, 1), ..host.clone() };
            let edge = plan_serving_at_rest(&tight, std::slice::from_ref(&model), None, &demand).unwrap();
            assert_eq!(edge.lanes, 1);
            assert!(model.peak_resident_bytes(edge.served_context_window, edge.lanes) <= tight.usable_bytes);
        }
    }

    #[test]
    fn legacy_and_corrupt_records_are_not_new_observation_evidence() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("window.json");
        std::fs::write(&path, r#"{"model_id":"legacy","per_slot_window":60000,"lanes":2,"typical_prompt_tokens":1000,"set_at_ms":1}"#).unwrap();
        let legacy = load_from(&path).unwrap();
        assert_eq!(legacy.observed_prompt_tokens, None);
        assert_eq!(legacy.cache_geometry(), Some((60000, 2)));
        std::fs::write(&path, "broken").unwrap();
        assert!(load_from(&path).is_none());
    }
}
