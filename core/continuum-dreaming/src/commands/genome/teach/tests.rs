#![cfg(test)]

use super::*;

/// what this catches (the coursework smoke on IntelMac, 2026-10-07): a default lesson set
/// that only resolves when the process happens to stand in the repo root. A running core's
/// cwd is not the repo, and neither is a test's (the crate dir), so the old repo-relative
/// default failed in both. With no source named, the selector must still return tested
/// lessons, from the copy embedded in the binary.
#[test]
fn the_default_lesson_set_resolves_wherever_the_process_stands() {
    let tasks = lesson_tasks(None, None, None, true).expect("the default set resolves");
    assert!(!tasks.is_empty(), "the default set has lessons");
    assert!(tasks.iter().all(|t| t.test.is_some()), "every default lesson carries its test");
}

// what this catches: the command's wire name mirrors its file path
// (commands/genome/teach.rs → "genome/teach"). The name keys cu, the persona
// tool surface, and the grid; drift breaks "file tree IS the namespace".
#[test]
fn teach_command_name_mirrors_path() {
    assert_eq!(GenomeTeach::NAME, "genome/teach");
    assert_eq!(GenomeTeach::ACCESS, AccessLevel::Privileged);
}
