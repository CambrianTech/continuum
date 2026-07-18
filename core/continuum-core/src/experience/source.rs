//! `RoomPurposeSource → Experience` — resolving a room to its manifest from recipe
//! DATA. A room's `purpose` (the existing [`RoomPurposeSource`] seam) is the
//! dispatch key into a `purpose → recipe` table; the recipe projects to the live
//! [`Experience`]. Adding an experience is adding a recipe; the projection code is
//! written once.

use std::collections::HashMap;
use std::sync::Arc;

use uuid::Uuid;

use crate::ipc::room_purpose::{RoomPurposeSource, SharedRoomPurpose};

use super::recipe::ExperienceRecipe;
use super::Experience;

/// Resolves a room to its authored [`Experience`] manifest. Membership (live roster)
/// is filled by the runtime, not this source — the returned manifest carries an
/// empty `membership` for the caller to populate from the room's roster.
pub trait ExperienceSource: Send + Sync {
    /// The manifest for a room, or `None` if this source has no recipe for the
    /// room's purpose (the caller may fall back to another source or a default —
    /// never a silent stand-in, `[[fallbacks-are-illegal-fail-loud]]`).
    fn experience_for(&self, room_id: Uuid) -> Option<Experience>;
}

/// Shared handle to an [`ExperienceSource`].
pub type SharedExperienceSource = Arc<dyn ExperienceSource>;

/// An [`ExperienceSource`] backed entirely by recipe DATA: a `purpose → recipe`
/// table, keyed by the room's purpose (resolved through the injected
/// [`RoomPurposeSource`]). This is the concrete `RoomPurposeSource → Experience`
/// projection — the manifests are recipe content, not Rust builders.
pub struct RecipeExperienceSource {
    /// purpose → authored recipe.
    by_purpose: HashMap<String, ExperienceRecipe>,
    /// room_id → purpose (the existing seam this projection composes on top of).
    purpose: SharedRoomPurpose,
}

impl RecipeExperienceSource {
    /// Build from an explicit set of recipes (e.g. loaded from `system/recipes/*`).
    /// A later recipe with the same purpose replaces an earlier one (last wins),
    /// the same override semantics a recipe loader gives.
    pub fn new(purpose: SharedRoomPurpose, recipes: impl IntoIterator<Item = ExperienceRecipe>) -> Self {
        let by_purpose = recipes
            .into_iter()
            .map(|r| (r.purpose.clone(), r))
            .collect();
        Self { by_purpose, purpose }
    }

    /// The built-in experiences shipped with the core, authored as embedded recipe
    /// JSON (`recipes/*.json`). Fails loud if an embedded recipe is malformed — that
    /// is a build-time authoring bug, pinned by the tests.
    pub fn builtins(purpose: SharedRoomPurpose) -> Self {
        let recipes = [
            include_str!("recipes/benchmark.json"),
            include_str!("recipes/chat.json"),
        ]
        .into_iter()
        .map(|json| {
            ExperienceRecipe::from_json(json).expect("embedded experience recipe must be valid JSON")
        });
        Self::new(purpose, recipes)
    }

    /// The purposes this source can project — the catalogue of known experiences.
    pub fn purposes(&self) -> impl Iterator<Item = &str> {
        self.by_purpose.keys().map(String::as_str)
    }
}

impl ExperienceSource for RecipeExperienceSource {
    fn experience_for(&self, room_id: Uuid) -> Option<Experience> {
        let purpose = self.purpose.purpose_for(room_id);
        // Membership is live roster state — filled by the caller, not the recipe.
        self.by_purpose
            .get(&purpose)
            .map(|recipe| recipe.clone().project(Vec::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experience::RegionScope;
    use crate::modules::grid::node::TrustLevel;

    /// A stub purpose source that reports one fixed purpose for every room — enough
    /// to drive the projection under test without a live room registry.
    struct FixedPurpose(&'static str);
    impl RoomPurposeSource for FixedPurpose {
        fn purpose_for(&self, _room_id: Uuid) -> String {
            self.0.to_string()
        }
    }

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

    // what this catches: an unknown purpose must NOT silently fabricate a manifest —
    // it returns None so the caller decides (fail-loud, never a stand-in).
    #[test]
    fn unknown_purpose_yields_none() {
        let purpose: SharedRoomPurpose = Arc::new(FixedPurpose("no-such-experience"));
        let source = RecipeExperienceSource::builtins(purpose);
        assert!(source.experience_for(Uuid::nil()).is_none());
    }
}
