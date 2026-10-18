#![cfg(test)]
//! Tests for `benchmark/swe-grade` verdict conversion and candidate-diff helpers.

use super::*;

// what this catches: name/access wiring — grading is a read on the AiSafe surface, so a
// persona can score her own work without an operator in the loop.
#[test]
fn swe_grade_name_and_access_wired() {
    assert_eq!(BenchmarkSweGrade::NAME, "benchmark/swe-grade");
    assert!(matches!(BenchmarkSweGrade::ACCESS, AccessLevel::AiSafe));
}

// what this catches: a verdict that could not run must NOT read as a scored zero. Tallying
// an environment failure as "the model failed" is how a broken harness becomes a number.
#[test]
fn an_errored_verdict_is_an_absence_not_a_zero() {
    let v = SweVerdict {
        instance_id: "x__y-1".into(),
        error: Some("could not install repo".into()),
        ..Default::default()
    };
    let r = SweGradeResult::from((v, 0));
    assert!(!r.resolved);
    assert!(r.error.is_some(), "the reason must survive to the caller");
    assert_eq!(
        r.fail_to_pass_total, 0,
        "no tests ran, so nothing was attempted"
    );
}

// what this catches: the experience-stream producer must attribute a graded
// workspace to the RIGHT citizen and stay silent for non-citizen paths — a
// wrong peer dir would file her lesson into someone else's mind (#319).
#[test]
fn citizen_peer_dir_resolves_from_workspace_path_or_not_at_all() {
    let p = std::path::Path::new(
        "/Users/x/.continuum/citizens/peers/fe4dac17-aaaa-4bbb-8ccc-000000000001/workspace/swe/pallets__flask-4992",
    );
    let d = citizen_peer_dir_of(p).expect("workspace path resolves");
    assert!(d.ends_with("citizens/peers/fe4dac17-aaaa-4bbb-8ccc-000000000001"));
    assert!(
        citizen_peer_dir_of(std::path::Path::new("/tmp/swe-work/repo")).is_none(),
        "an operator scratch tree is nobody's experience"
    );
    assert!(
        citizen_peer_dir_of(std::path::Path::new("/x/citizens/peers")).is_none(),
        "the pair with no uuid after it must not resolve"
    );
}

// what this catches: the gate is reported, not just enforced. A caller tallying results
// must be able to EXCLUDE ungradeable instances rather than counting them as failures.
#[test]
fn the_gate_verdict_reaches_the_caller() {
    let v = SweVerdict {
        instance_id: "x__y-1".into(),
        gate_ok: false,
        error: Some("UNGRADEABLE — FAIL_TO_PASS already passes".into()),
        ..Default::default()
    };
    let r = SweGradeResult::from((v, 512));
    assert!(!r.gate_ok);
    assert_eq!(
        r.patch_bytes, 512,
        "patch size is reported even when the tree is void"
    );
}
