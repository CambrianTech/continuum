#![cfg(test)]
//! An unpriced mind's need (#5833, Cormac): with no measured act time her act chooses no priced
//! window, so no need sample is recorded; what the governor reads as her need is her irreducible
//! window, recorded on every act, never her wish (the untrimmed assembly with headroom).

use super::*;
use crate::cognition::working_set::{requirements_for, WorkingSetRegistry};
use crate::inference::planning::window_allocator::Measure;

// what this catches (Cormac on #5833 at 867179538): an unpriced mind's need is her irreducible
// window, recorded as a measured governed size — the governor reads that, never the wish target.
#[test]
fn an_unpriced_minds_need_is_her_irreducible_window_never_her_wish() {
    let home = tempfile::tempdir().expect("isolated fixture storage");
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let (window, persona) = (262_144u32, Uuid::new_v4());
    let reg = WorkingSetRegistry::new();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    // No working memory: her act time is unmeasured, so her act is unpriced.
    let faculty = LlmDeliberationFaculty::new(persona, "Aiko", "You are Aiko.", adapter)
        .with_context_window(window)
        .with_tools(vec![read_tool()])
        .with_working_set(reg.clone());
    let mut ws = writing_workspace(crate::identity::ActivityRoom::mint(), 4, true);
    // Enrichment far past what she must hold: whole in her wish, collapsible in her act.
    ws.broadcast.push(Contribution::context(FacultyId::Recall, "a file she read once, whole. ".repeat(20_000), 0.6, "recall").trailing());
    let view = faculty.prompt_view(&ws);
    assert!(view.capacity_error.is_none(), "precondition: the act is held");
    let demand = reg.demand_of(persona).expect("her act recorded its measurements");
    let floor = demand.floor_tokens.expect("her irreducible window is recorded on every act");
    assert!(demand.need_recent.iter().all(|sample| sample.value() == 0), "unpriced: no need sample invented");
    let requirement = &requirements_for(&[persona], &reg)[0];
    let wish = requirement.measure.target().map(|t| t.get()).expect("her wish is measured");
    assert!(wish > floor, "precondition: her wish ({wish}) is far above what she must hold ({floor})");
    let Measure::Measured { governed } = requirement.measure else { panic!("her irreducible window is a measured need: {requirement:?}") };
    assert_eq!(governed.value().get(), floor, "the governor's need for her is her irreducible window, never the wish");
    assert!(floor <= window, "inside the trained context and her grant: {floor}");
}
