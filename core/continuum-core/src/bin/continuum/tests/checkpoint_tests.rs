#![cfg(test)]
use super::*;

// What this catches (card 9f160b78): missing/truncated process evidence and
// a live PID-file process must never authorize offline memory replacement.
#[test]
fn checkpoint_offline_evidence_keeps_unknown_distinct_from_absent() {
    use super::CoreProcessEvidence;
    use std::ffi::OsStr;
    use std::path::Path;
    let own = (41, OsStr::new("continuum"), None);
    let other = (42, OsStr::new("launcher"), None);
    let evidence = CoreProcessEvidence::from_processes(41, [own, other]).unwrap();
    assert!(evidence.ensure_offline(None).is_ok());
    assert!(evidence.ensure_offline(Some(99)).is_ok());
    assert_eq!(
        evidence.ensure_offline(Some(42)).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert!(CoreProcessEvidence::from_processes(41, [other]).is_err());
    let truncated = (43, OsStr::new("continuum-core-"), None);
    assert!(CoreProcessEvidence::from_processes(41, [own, truncated]).is_err());
    let resolved = (
        43,
        OsStr::new("continuum-core-"),
        Some(Path::new("/bin/continuum-core-server")),
    );
    assert!(CoreProcessEvidence::from_processes(41, [own, resolved])
        .unwrap()
        .ensure_offline(None)
        .is_err());
    // The uncertain truncated name must not broaden process termination.
    assert!(!super::process_matches_fragment(
        truncated.1,
        truncated.2,
        "continuum-core-server"
    ));
    assert!(super::process_matches_fragment(
        resolved.1,
        resolved.2,
        "continuum-core-server"
    ));
    assert!(
        !super::process_matches_fragment(
            OsStr::new("python"),
            Some(Path::new("/continuum-core-server-fixtures/python")),
            "continuum-core-server"
        ),
        "parent directories must not turn an unrelated executable into a core kill target"
    );
}

// What this catches (card 9f160b78): inspect --plan <absent volatile.json>
// must not write a valid plan into a Persona's actual memory file.
#[test]
fn inspection_plan_cannot_occupy_checkpoint_or_evidence_storage() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let source_dir = root.join("legacy").join(uuid::Uuid::new_v4().to_string());
    let persona_dir = root.join("personas").join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::create_dir_all(&persona_dir).unwrap();
    let sibling = root.join("legacy").join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&sibling).unwrap();
    let source = source_dir.join("volatile.json");
    let destination = persona_dir.join("volatile.json");
    for invalid in [
        &source,
        &destination,
        &persona_dir.join(".volatile.lock"),
        &source_dir.join("plan.json"),
        &sibling.join("volatile.json"),
    ] {
        assert!(super::checkpoint_plan_output(invalid, &source, &destination).is_err());
        assert!(
            !invalid.exists(),
            "inspection must leave managed state absent"
        );
    }
    let outside = root.join("plan.json");
    assert_eq!(
        super::checkpoint_plan_output(&outside, &source, &destination).unwrap(),
        outside
    );
}

// What this catches (card 9f160b78): a malformed recovery invocation must
// never select/adopt a checkpoint or imply that legacy writers are stopped.
#[test]
fn checkpoint_recovery_requires_explicit_selection_and_offline_precondition() {
    let parse =
        |args: &[&str]| super::CheckpointCommand::parse(args.iter().map(|arg| arg.to_string()));
    let persona = "68d231fb-1b99-47ea-8615-14538906817a";
    assert!(matches!(
        parse(&[
            "inspect",
            "--source",
            "old/volatile.json",
            "--persona-id",
            persona,
            "--plan",
            "plan.json"
        ])
        .unwrap(),
        super::CheckpointCommand::Inspect { .. }
    ));
    assert_eq!(
        parse(&["adopt", "--plan", "plan.json", "--legacy-writers-stopped"]).unwrap(),
        super::CheckpointCommand::Adopt {
            plan: "plan.json".into()
        }
    );
    for args in [
        vec![],
        vec!["adopt", "--plan", "plan.json"],
        vec!["adopt", "--plan", "plan.json", "--force"],
        vec![
            "adopt",
            "--plan",
            "plan.json",
            "--plan",
            "other.json",
            "--legacy-writers-stopped",
        ],
        vec![
            "adopt",
            "--plan",
            "plan.json",
            "--legacy-writers-stopped",
            "--legacy-writers-stopped",
        ],
        vec![
            "inspect",
            "--source",
            "old/volatile.json",
            "--plan",
            "plan.json",
        ],
        vec![
            "inspect",
            "--source",
            "old/volatile.json",
            "--persona-id",
            "invalid",
            "--plan",
            "plan.json",
        ],
        vec![
            "inspect",
            "--source",
            "--persona-id",
            persona,
            "--plan",
            "plan.json",
        ],
    ] {
        assert!(parse(&args).is_err(), "must refuse {args:?}");
    }
}
