//! The served per-slot window, remembered ACROSS RUNS so the KV page store's
//! geometry key is stable and a citizen's pages survive a reboot.
//!
//! Measured 2026-09-13 on one host + one model: 83,968 → 102,656 → 126,464 →
//! 138,240 per slot across four boots (the planner re-derived the window from the
//! moving measured demand each time). The page dir is keyed `<model>--c<window>`
//! and the spawn sweeps every other generation, so NO page ever survived a
//! reboot — every citizen re-prefilled cold after every deploy. Joel: "virtual
//! disk paging … managed and maintained across runs." The serving lifecycle records
//! settled candidates; successful terminal usage qualifies an exact generation.
//! Both updates belong to this store. Planning reads the last qualified geometry,
//! which survives temporary pressure without treating a small success as recovery.
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

/// The launch owner's decision, retained with the generation through settlement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GeometryQualification {
    #[default]
    Unknown,
    Unconstrained,
    PressureLimited,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettledGeometry {
    pub geometry: ServedGeometry,
    #[serde(default)]
    pub qualification: GeometryQualification,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredServedWindow {
    pub model_id: String,
    pub per_slot_window: u32,
    /// The lane count that served beside that window — the other half of the geometry a
    /// cold boot plans first (9/18: a window alone let the boot launch 8 lanes at 28k).
    /// Absent on an older record (default 0 = unknown): the window still carries over.
    #[serde(default)]
    pub lanes: u32,
    /// Compatibility projection of the observed prompt median. Legacy records
    /// also wrote requested demand here; only observed_prompt_tokens is evidence.
    /// The typical prompt (residents AND leased-in) the seat served at
    /// its last steady plan — the per-lane FLOOR a cold boot starts from. Without it the
    /// floor is the 16,384 bootstrap prior until enough prompts land: the 5090 on
    /// 2026-09-19 00:3xZ planned 2 × 17k for coders sending ~30k, because the leased-in
    /// sample ring is process-static and empty at boot. 0 = unknown (older record).
    #[serde(default)]
    pub typical_prompt_tokens: u32,
    /// Accepted provider counts. Absent on legacy records whose typical field
    /// mixed unmet demand with actual service; that field is not observed evidence.
    #[serde(default)]
    pub observed_prompt_tokens: Option<u32>,
    pub set_at_ms: u64,
    /// Current settled launch. It does not replace the last qualified geometry
    /// until successful usage from this exact generation qualifies it.
    #[serde(default)]
    pub candidate: Option<SettledGeometry>,
}

impl StoredServedWindow {
    /// Return retained restart geometry. Zero lanes denotes an unqualified first
    /// candidate. An impossible recorded observation is rejected as inconsistent
    /// legacy data; that check cannot detect pressure collapse. New qualification
    /// happens in observe_to using the launch owner's capacity provenance.
    pub fn steady_geometry(&self) -> Option<(u32, u32)> {
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

/// THE LAST STEADY GEOMETRY this host served, whatever the model — a cold boot's first
/// choice (the caller checks the model is still a candidate). Missing/corrupt = `None`.
pub fn load_geometry() -> Option<StoredServedWindow> {
    default_path().and_then(|p| load_from(&p))
}
/// The remembered window for `model_id`, if the store holds one for THAT model.
/// Missing or foreign = `None` silently; corrupt = `None` with a probe.
pub fn load_for(model_id: &str) -> Option<u32> {
    default_path().and_then(|p| load_from(&p)).filter(|s| s.model_id == model_id && s.steady_geometry().is_some()).map(|s| s.per_slot_window)
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

/// Record a settled launch separately from the geometry qualified by successful work.
pub fn save(geometry: ServedGeometry, qualification: GeometryQualification) {
    if let Some(path) = default_path() {
        save_candidate_to(&path, geometry, qualification);
    }
}

fn save_candidate_to(path: &Path, geometry: ServedGeometry, qualification: GeometryQualification) {
    let mut stored = load_from(path).unwrap_or_else(|| StoredServedWindow {
        model_id: geometry.model_id.clone(),
        per_slot_window: geometry.per_slot_window,
        lanes: 0, // An unobserved first launch is not a qualified boot geometry.
        typical_prompt_tokens: 0,
        observed_prompt_tokens: None,
        set_at_ms: crate::time::EpochMs::now().millis(),
        candidate: None,
    });
    let candidate = SettledGeometry { geometry, qualification };
    // Settlement repeats. A later planning tick cannot relabel the admission
    // reason of a generation that is already recorded.
    if stored.candidate.as_ref().is_some_and(|old| old.geometry.generation == candidate.geometry.generation) {
        return;
    }
    if stored.candidate.as_ref() != Some(&candidate) {
        stored.candidate = Some(candidate);
        write_record(path, &stored);
    }
}

/// A successful median certifies only the settled generation that produced it.
/// The launch owner's capacity decision qualifies geometry; prompt sizes cannot
/// classify pressure because serving censors them at the window it admitted.
pub fn save_observed_prompt(geometry: &ServedGeometry, tokens: u32) {
    if let Some(path) = default_path() {
        observe_to(&path, geometry, tokens);
    }
}

fn observe_to(path: &Path, geometry: &ServedGeometry, tokens: u32) {
    let Some(mut stored) = load_from(path) else { return };
    let Some(candidate) = stored.candidate.as_ref().filter(|c| &c.geometry == geometry) else { return };
    if tokens == 0 || tokens > geometry.per_slot_window || geometry.lanes == 0
        || candidate.qualification != GeometryQualification::Unconstrained
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
    stored.set_at_ms = crate::time::EpochMs::now().millis();
    write_record(path, &stored);
}
#[cfg(test)]
pub fn save_to(path: &Path, model_id: &str, per_slot_window: u32, lanes: u32, typical_prompt_tokens: u32) {
    let observed_prompt_tokens = (typical_prompt_tokens > 0).then_some(typical_prompt_tokens);
    if load_from(path).is_some_and(|s| {
        s.model_id == model_id && s.per_slot_window == per_slot_window && s.lanes == lanes && s.observed_prompt_tokens == observed_prompt_tokens && s.typical_prompt_tokens == typical_prompt_tokens
    }) {
        return;
    }
    let set_at_ms = crate::time::EpochMs::now().millis(); // pre-1970 reads 0: a clock before the epoch stamps 0 — the stamp is informational, the window is the fact
    let stored = StoredServedWindow { model_id: model_id.to_string(), per_slot_window, lanes, typical_prompt_tokens, observed_prompt_tokens, set_at_ms, candidate: None };
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
        Ok(Ok(())) => crate::probe!(
            class = "serving.window.stored",
            model = model_id,
            per_slot_window = per_slot_window as u64,
            lanes = lanes as u64,
            typical_prompt_tokens = typical_prompt_tokens as u64,
            "serving geometry lifecycle recorded — candidate and qualified restart evidence remain distinct"
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

    // Real planning and persistence in sequence: the same small launch is
    // temporary pressure on the original host, but a valid optimum after the
    // physical hardware/mode budget changes. Prompt sizes cannot tell them apart.
    #[test]
    fn physical_fit_distinguishes_first_boot_pressure_from_legitimate_resize() {
        use crate::cognition::serving_plan::{plan_serving, HostBudget, ModelFootprint, ServingDemand};
        use crate::cognition::window_allocator::LaneRequirement;
        let model = ModelFootprint { model_id: "qwen-27b".into(), weights_bytes: 19 << 30,
            kv_per_token: 33_000, context_window: 262_144, capability_rank: 9, fixed_per_lane_bytes: 0 };
        let physical = HostBudget { usable_bytes: model.peak_resident_bytes(70_000, 1), perf_cores: 1 };
        let squeezed = HostBudget { usable_bytes: model.peak_resident_bytes(8_192, 1), perf_cores: 1 };
        let demand = ServingDemand::new(1, None).with_requirements(vec![
            LaneRequirement::declared(uuid::Uuid::new_v4(), 70_000).with_need(2_048),
        ]);
        let healthy = plan_serving(&physical, std::slice::from_ref(&model), &demand).expect("physical plan");
        let small = plan_serving(&squeezed, std::slice::from_ref(&model), &demand).expect("live plan");
        assert!(small.fits_on_gpu && small.served_context_window < healthy.served_context_window);
        assert_eq!(healthy.pressure_limited(&physical, &demand), Some(false));
        assert_eq!(small.pressure_limited(&physical, &demand), Some(true));
        assert_eq!(small.pressure_limited(&squeezed, &demand), Some(false), "hardware/mode change is a new physical optimum");
        let dir = tempfile::tempdir().expect("owner store");
        let path = dir.path().join(FILE);
        let geometry = |plan: &crate::cognition::serving_plan::ServingPlan| ServedGeometry {
            model_id: model.model_id.clone(), generation: uuid::Uuid::new_v4(),
            per_slot_window: plan.served_context_window, lanes: plan.lanes,
        };
        let qualification = |plan: &crate::cognition::serving_plan::ServingPlan, budget: &HostBudget| match plan.pressure_limited(budget, &demand) {
            Some(true) => GeometryQualification::PressureLimited,
            Some(false) => GeometryQualification::Unconstrained,
            None => GeometryQualification::Unknown,
        };
        let first = geometry(&small);
        save_candidate_to(&path, first.clone(), qualification(&small, &physical));
        observe_to(&path, &first, 1_024);
        assert_eq!(load_from(&path).expect("first boot reload").steady_geometry(), None);
        let large = geometry(&healthy);
        save_candidate_to(&path, large.clone(), qualification(&healthy, &physical));
        observe_to(&path, &large, 30_000);
        let pressure = geometry(&small);
        save_candidate_to(&path, pressure.clone(), qualification(&small, &physical));
        observe_to(&path, &pressure, 1_024);
        assert_eq!(load_from(&path).expect("pressure restart").steady_geometry(), Some((large.per_slot_window, large.lanes)));
        let resized = geometry(&small);
        save_candidate_to(&path, resized.clone(), qualification(&small, &squeezed));
        observe_to(&path, &resized, 1_024);
        assert_eq!(load_from(&path).expect("changed physical budget restart").steady_geometry(), Some((resized.per_slot_window, resized.lanes)));
    }

    // Regression for #5050: small successful work on a collapsed launch must
    // not erase the last geometry qualified by larger successful work.
    #[test]
    fn qualified_geometry_survives_collapse_small_success_and_restart() {
        use crate::cognition::working_set::PromptSample;
        let dir = tempfile::tempdir().expect("temporary owner store");
        let path = dir.path().join(FILE);
        let large = ServedGeometry { model_id: "m".into(), generation: uuid::Uuid::new_v4(), per_slot_window: 62_720, lanes: 1 };
        let collapsed = ServedGeometry { generation: uuid::Uuid::new_v4(), per_slot_window: 2_048, ..large.clone() };
        let observe = |geometry: &ServedGeometry, tokens| PromptSample::Served {
            endpoint: "local".into(), tokens, geometry: geometry.clone(),
        };
        save_candidate_to(&path, large.clone(), GeometryQualification::Unconstrained);
        assert_eq!(load_from(&path).expect("candidate").steady_geometry(), None);
        let samples = vec![observe(&large, 30_000)];
        observe_to(&path, &large, PromptSample::geometry_median(&samples, "local", &large).expect("large usage"));
        save_candidate_to(&path, collapsed.clone(), GeometryQualification::PressureLimited);
        let mut samples = samples;
        samples.push(observe(&collapsed, 1_024));
        let foreign = ServedGeometry { model_id: "other".into(), ..collapsed.clone() };
        samples.push(observe(&foreign, 1_500));
        samples.push(PromptSample::Served { endpoint: "neighbor".into(), tokens: 1_900, geometry: collapsed.clone() });
        let median = PromptSample::geometry_median(&samples, "local", &collapsed).expect("small successful usage");
        assert_eq!(median, 1_024, "foreign model, neighbor and old generation do not certify this launch");
        observe_to(&path, &collapsed, median);
        observe_to(&path, &large, 500); // late old-generation completion
        observe_to(&path, &foreign, 500);
        let restarted = load_from(&path).expect("restart reloads durable qualification");
        assert_eq!(restarted.steady_geometry(), Some((62_720, 1)));
        assert_eq!(restarted.observed_prompt_tokens, Some(30_000));
        assert_eq!(restarted.candidate.as_ref().map(|c| &c.geometry), Some(&collapsed));
        let first_path = dir.path().join("first-pressure.json");
        save_candidate_to(&first_path, collapsed.clone(), GeometryQualification::PressureLimited);
        observe_to(&first_path, &collapsed, median);
        assert_eq!(load_from(&first_path).expect("first pressure launch").steady_geometry(), None, "temporary pressure cannot seed even the first boot");
        // A genuinely fitting replacement remains eligible; this is not a max-window rule.
        let fitted = ServedGeometry { generation: uuid::Uuid::new_v4(), per_slot_window: 40_000, ..large };
        save_candidate_to(&path, fitted.clone(), GeometryQualification::PressureLimited);
        observe_to(&path, &fitted, 32_000);
        assert_eq!(load_from(&path).expect("pressure above prior usage").steady_geometry(), Some((62_720, 1)));
        save_candidate_to(&path, fitted.clone(), GeometryQualification::Unconstrained);
        observe_to(&path, &fitted, 32_000);
        assert_eq!(load_from(&path).expect("same generation cannot be relabeled").steady_geometry(), Some((62_720, 1)));
        let fitted = ServedGeometry { generation: uuid::Uuid::new_v4(), ..fitted };
        save_candidate_to(&path, fitted.clone(), GeometryQualification::Unknown);
        observe_to(&path, &fitted, 32_000);
        assert_eq!(load_from(&path).expect("unknown provenance").steady_geometry(), Some((62_720, 1)));
        let fitted = ServedGeometry { generation: uuid::Uuid::new_v4(), ..fitted };
        save_candidate_to(&path, fitted.clone(), GeometryQualification::Unconstrained);
        observe_to(&path, &fitted, 32_000);
        assert_eq!(load_from(&path).expect("qualified replacement").steady_geometry(), Some((40_000, 1)));
        let expanded = ServedGeometry { generation: uuid::Uuid::new_v4(), lanes: 2, ..fitted.clone() };
        save_candidate_to(&path, expanded.clone(), GeometryQualification::Unconstrained);
        observe_to(&path, &expanded, 32_000);
        let one_lane = ServedGeometry { generation: uuid::Uuid::new_v4(), lanes: 1, ..expanded };
        save_candidate_to(&path, one_lane.clone(), GeometryQualification::PressureLimited);
        observe_to(&path, &one_lane, 1_024);
        assert_eq!(load_from(&path).expect("lane pressure survives restart").steady_geometry(), Some((40_000, 2)));
        // A changed workload may legitimately use less than prior successful
        // usage. Only an unconstrained owner decision permits this replacement.
        let collapsed = ServedGeometry { generation: uuid::Uuid::new_v4(), ..collapsed };
        save_candidate_to(&path, collapsed.clone(), GeometryQualification::Unconstrained);
        observe_to(&path, &collapsed, 1_024);
        assert_eq!(load_from(&path).expect("new workload").steady_geometry(), Some((2_048, 1)));
    }

    // Structural compatibility check, not the lifecycle collapse regression above.
    #[test]
    fn inconsistent_legacy_observation_and_unqualified_geometry_are_rejected() {
        let rec = |window: u32, lanes: u32, typical: u32| StoredServedWindow {
            model_id: "m".into(),
            per_slot_window: window,
            lanes,
            typical_prompt_tokens: typical,
            observed_prompt_tokens: Some(typical),
            set_at_ms: 1,
            candidate: None,
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
