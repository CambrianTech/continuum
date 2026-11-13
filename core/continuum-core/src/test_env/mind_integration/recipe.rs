#![cfg(test)]
//! From `recipe::validate`: the shipped authored round checked against the REAL command
//! registry (core's commands register through inventory, so only core's binary has them),
//! and the `$room` fields checked against core's activity seeding site. Each asserts exactly
//! what it asserted before the move.

use crate::experience::recipe::ExperienceRecipe;
use crate::recipe::validate::*;

fn recipe(json: &str) -> ExperienceRecipe {
    ExperienceRecipe::from_json(json).expect("recipe parses")
}

// what this catches: the shipped authored round must be well-formed against the
// REAL registry on this build — every verb exists, every param is declared and
// typed, every reference resolves. A renamed verb or a dropped param names the
// step here, before a deploy. (This test found round-track's skipped schema
// fields on its first run.)
#[test]
fn the_shipped_authored_round_is_well_formed_against_the_real_registry() {
    let r = recipe(include_str!("../../../../continuum-experience/src/experience/recipes/benchmark-round.json"));
    let issues = pipeline_issues(&r, |name| registry_lookup().get(name).cloned());
    let rendered: Vec<String> = issues.iter().map(ToString::to_string).collect();
    assert!(issues.is_empty(), "benchmark/round has pipeline issues: {rendered:#?}");
}

// what this catches: ROOM_FIELDS is asserted against the seeding site — if the
// birth path seeds a new `$room` field, validation must learn it in the same PR.
#[test]
fn room_fields_match_what_the_birth_path_seeds() {
    let src = include_str!("../../../../continuum-mind/src/activity/spawn_activity_room.rs");
    let seed_site = src.find("\"room\".to_string(),").expect("the $room seed site exists");
    let window = &src[seed_site..seed_site + 400];
    for f in ROOM_FIELDS {
        assert!(window.contains(&format!("\"{f}\"")), "activity.rs seeds $room.{f}");
    }
}
