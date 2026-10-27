#![cfg(test)]
//! From `experience`: affordance authorization computed from the REAL ACL.

use std::sync::Arc;

use uuid::Uuid;

use crate::experience::room_purpose::SharedRoomPurpose;
use crate::experience::source::test_support::FixedPurpose;
use crate::experience::source::*;
use crate::experience::*;
use crate::routing::trust_level::TrustLevel;

// what this catches: the whole recipe-as-data projection. A room whose purpose
// is "benchmark/hard-rs" must resolve — through DATA alone — to the benchmark
// manifest, AND the observe affordance's who_may must be COMPUTED from the ACL
// (Provisional), never carried in the JSON. If someone tried to author who_may
// in a recipe, this still reflects the real ACL; if the projection stopped
// computing it, this fails.
#[test]
fn purpose_projects_to_the_recipe_manifest_with_computed_authz() {
    let purpose: SharedRoomPurpose = Arc::new(FixedPurpose("benchmark/hard-rs"));
    let source = RecipeExperienceSource::builtins(purpose);

    let exp = source
        .experience_for(Uuid::nil())
        .expect("benchmark purpose resolves to a recipe");

    assert_eq!(exp.purpose, "benchmark/hard-rs");
    // Regions came straight from the recipe DATA — three, all activity-scoped.
    assert_eq!(exp.regions.len(), 3);
    assert!(exp.regions.iter().all(|r| r.scope == RegionScope::Activity));
    // The explicit level-3 layout survived JSON round-trip.
    assert!(exp.layout.is_some());
    // Authorization was COMPUTED at projection, not authored: observe is AiSafe
    // → Provisional per the live ACL.
    let observe = exp
        .affordances
        .iter()
        .find(|a| a.verb == "observe")
        .expect("recipe declared observe");
    assert_eq!(observe.who_may, Some(TrustLevel::Provisional));
    // Membership is runtime state — the recipe carries none.
    assert!(exp.membership.is_empty());
}

// what this catches: `who_may` must be DERIVED from the ACL, never hand-set —
// otherwise an affordance could advertise a trust tier the door doesn't enforce
// (or under-advertise and confuse a caller). `cognition/observe` is AiSafe, which
// the ACL maps to Provisional; if that mapping regresses, this fails.
#[test]
fn observe_affordance_authz_tracks_the_real_acl() {
    let bench = benchmark_experience(
        crate::identity::PeerId::new(),
        crate::identity::PeerId::new(),
    );
    let observe = bench
        .affordances
        .iter()
        .find(|a| a.verb == "observe")
        .expect("benchmark offers observe");
    assert_eq!(observe.command, "cognition/observe");
    assert_eq!(
        observe.who_may,
        Some(TrustLevel::Provisional),
        "observe is AiSafe → Provisional; who_may must equal required_trust(command)"
    );
}
