//! **The serving planner's floor is not a window.**
//!
//! `continuum_engine::inference::planning::MIN_SERVE_CTX` (2,048 tokens) is the smallest lane
//! the PLANNER will serve: a floor for sizing, placing and launching lanes. Everywhere else it
//! was used as a stand-in for "the served window is not known yet", and a stand-in number
//! poses as a measurement. On the M5 (2026-10-29) residents materialized before the engine
//! was ready were bound at 2,048; working memory sized its results share from it and deleted
//! result bodies, so residents saw 36 of 40 results as bare headers.
//!
//! What counts: one violation per line of code (tests included, comments not) that names
//! `MIN_SERVE_CTX` outside the planner modules below. It counts the NAME only: an alias of it or
//! a literal 2048 standing in for a window is not caught here. An unknown window is represented as
//! unknown (`None`), never as this number. Counted by grep on 0f92b593f (2026-10-29): 28 lines in
//! 15 files, all in the mind, the prompt budget, the academy and their tests. It falls to 0
//! as the window becomes the turn's admission grant (one owner) instead of a cached copy.

use super::{split_code_and_comment, SourceFile, SourceRule, Violation};

/// The planner: the only modules where the floor means what it says. Paths as audited
/// (module crates are unprefixed): continuum-engine's planning and single-purpose lanes,
/// continuum-serving's consumer and daemon, continuum-grid's placement, and continuum-core's
/// provisioning placement planner. Directories match as a slash-delimited prefix and files
/// exactly, so a sibling (`grid/placement.rs_extra`, `modules/serving_daemon_x.rs`) is not
/// exempt (Codex on #5593).
const PLANNER_DIRS: &[&str] = &[
    "inference/planning/",
    "inference/single_purpose_lane/",
    "modules/serving_daemon/",
];
const PLANNER_FILES: &[&str] = &[
    "modules/serving_consumer.rs",
    "modules/serving_daemon.rs",
    "grid/placement.rs",
    "provisioning/placement_planner.rs",
];

fn in_the_planner(rel: &str) -> bool {
    PLANNER_DIRS.iter().any(|dir| rel.starts_with(dir)) || PLANNER_FILES.contains(&rel)
}

const FLOOR: &str = "MIN_SERVE_CTX";

pub struct PlannerFloorOutsideThePlanner;

impl SourceRule for PlannerFloorOutsideThePlanner {
    fn name(&self) -> &'static str {
        "planner_floor_outside_the_planner"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        if in_the_planner(&file.rel) {
            return Vec::new();
        }
        file.raw
            .lines()
            .enumerate()
            .filter(|(_, line)| split_code_and_comment(line).0.contains(FLOOR))
            .map(|(i, line)| Violation {
                rule: self.name(),
                file: file.rel.clone(),
                line: i + 1,
                source: line.trim().to_string(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;

    // what this catches: a new stand-in window. A mind, prompt or academy line (or a test
    // binding a mind at it) that reads the planner's floor as if it were her served window.
    // Represent the unknown window as unknown; never raise the count.
    #[test]
    fn the_planner_floor_outside_the_planner_never_rises() {
        let violations = scan(&[&PlannerFloorOutsideThePlanner]);
        let count = violations.len();
        crate::ratchet::hold("planner_floor_outside_the_planner", count, || {
            format!(
                "MIN_SERVE_CTX is named outside the serving planner on {count} lines. It is the \
                 smallest lane the planner serves, not a window: an unknown window is `None`, and \
                 a turn's window comes from what serving reports for it.\n{}",
                violations
                    .iter()
                    .take(20)
                    .map(|v| format!("  {}:{}  {}", v.file, v.line, v.source))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        });
    }

    // what this catches: the predicate drifting. A mind-side binding at the floor and a test
    // using it are flagged; the planner's own modules and a comment naming it are not.
    #[test]
    fn a_mind_side_use_is_flagged_and_the_planner_is_not() {
        let flagged = |rel: &str, src: &str| {
            PlannerFloorOutsideThePlanner
                .check(&SourceFile::for_test(rel, src))
                .len()
        };
        assert_eq!(
            flagged(
                "cognition/llm_deliberation_faculty.rs",
                "binding: model_binding(adapter, None, crate::inference::planning::MIN_SERVE_CTX),"
            ),
            1,
            "a mind bound at the planner floor"
        );
        assert_eq!(
            flagged(
                "persona/spawner_module.rs",
                "served_context_window: MIN_SERVE_CTX,\nlet w = x.max(MIN_SERVE_CTX);"
            ),
            2
        );
        assert_eq!(
            flagged("cognition/x.rs", "// MIN_SERVE_CTX was the stand-in"),
            0,
            "a comment"
        );
        assert_eq!(
            flagged("cognition/x.rs", "let w = 2048; // not MIN_SERVE_CTX"),
            0,
            "a trailing comment"
        );
        assert_eq!(
            flagged(
                "inference/planning/serving_plan/max_lanes.rs",
                "wanted.min(fits).max(MIN_SERVE_CTX)"
            ),
            0,
            "the planner"
        );
        assert_eq!(
            flagged(
                "modules/serving_daemon/launch_window.rs",
                "fits.max(MIN_SERVE_CTX)"
            ),
            0,
            "the serving daemon"
        );
        assert_eq!(
            flagged("grid/placement.rs", "window >= MIN_SERVE_CTX"),
            0,
            "grid placement"
        );
    }

    // what this catches (Codex on #5593): the exemption widening to a sibling. A file named like a
    // planner file plus a suffix, or a neighbour of the daemon directory, is checked like any other.
    #[test]
    fn a_sibling_of_a_planner_path_is_not_exempt() {
        let flagged = |rel: &str| {
            PlannerFloorOutsideThePlanner
                .check(&SourceFile::for_test(rel, "let w = MIN_SERVE_CTX;"))
                .len()
        };
        assert_eq!(flagged("grid/placement.rs"), 0, "the planner file itself");
        assert_eq!(
            flagged("modules/serving_daemon/launch_window.rs"),
            0,
            "inside the daemon directory"
        );
        assert_eq!(
            flagged("modules/serving_daemon.rs"),
            0,
            "the daemon module file"
        );
        assert_eq!(flagged("grid/placement.rs_extra"), 1, "a file sibling");
        assert_eq!(
            flagged("modules/serving_daemon_x.rs"),
            1,
            "a directory sibling"
        );
        assert_eq!(
            flagged("modules/serving_consumer.rs_backup"),
            1,
            "a backup sibling"
        );
    }
}
