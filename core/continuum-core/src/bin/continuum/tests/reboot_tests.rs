#![cfg(test)]
use super::*;
use serde_json::json;

// Regression for #3929: a first upgrade must leave the legacy core available
// for explicit checkpoint recovery, while normal reboot outcomes may continue.
#[test]
fn legacy_core_requires_explicit_recovery_before_reboot_teardown() {
    use super::GracefulStop;
    let legacy = GracefulStop::LegacyCore("no shutdown handler".into());
    let error = legacy.ensure_teardown_supported(true).unwrap_err();
    assert!(
        legacy.ensure_teardown_supported(false).is_ok(),
        "explicit stop remains available"
    );
    assert!(error.contains("before teardown"));
    assert!(error.contains("checkpoint inspect"));
    assert!(error.contains("checkpoint adopt"));
    // Maintenance must not inherit ordinary stop's permission to force an
    // exit after a missing or incomplete save receipt (5120).
    assert!(legacy.require_durable().is_err());
    // #3940 revision: a core that HAS the rail but cannot attest is fail-closed exactly like
    // LegacyCore — refused before reboot, explicit stop still available, no durable evidence.
    let fault = GracefulStop::CannotAttest("checkpoint not found".into());
    let fault_error = fault.ensure_teardown_supported(true).unwrap_err();
    assert!(fault.ensure_teardown_supported(false).is_ok(), "explicit stop remains available");
    assert!(fault_error.contains("before teardown"));
    assert!(fault.require_durable().is_err());
    for outcome in [
        GracefulStop::NothingRunning,
        GracefulStop::Durable("saved".into()),
        GracefulStop::Incomplete("save failed".into()),
        GracefulStop::NoAnswer("timed out".into()),
    ] {
        assert!(outcome.ensure_teardown_supported(true).is_ok());
        assert_eq!(outcome.require_durable().is_ok(), matches!(outcome,
            GracefulStop::NothingRunning | GracefulStop::Durable(_)));
    }
}

// what this catches: card 67f53b63 — a missing/misspelled prebuilt path
// must not fall through to a source reboot, and --force only changes leases.
#[test]
fn prebuilt_reboot_options_are_explicit_and_strict() {
    let parse = |args: &[&str]| super::RebootOptions::parse(args.iter().map(|s| s.to_string()));
    assert_eq!(parse(&[]).unwrap(), super::RebootOptions::default());
    for args in [
        vec!["--force", "--prebuilt", "release dir/core.exe"],
        vec!["--prebuilt", "release dir/core.exe", "--force"],
    ] {
        let options = parse(&args).unwrap();
        assert!(options.force);
        assert_eq!(
            options.prebuilt.as_deref(),
            Some(std::path::Path::new("release dir/core.exe"))
        );
    }
    for args in [
        vec!["--prebuilt"],
        vec!["--prebuilt", ""],
        vec!["--prebuilt", "--force"],
        vec!["--prebuit", "core.exe"],
        vec!["core.exe"],
        vec!["--force", "--force"],
        vec!["--prebuilt", "one", "--prebuilt", "two"],
        vec!["--force", "--prebuilt", "core.exe", "--from-source"],
        // `--service` ALONE is valid since card 82af11f5 (the warm build supplies the
        // artifact); it is exercised in the valid cases above on Windows.
        vec!["--service", "--service", "--prebuilt", "core.exe"],
        vec!["--validate-only"],
        vec!["--embedding-model", "model.gguf"],
        vec!["--prebuilt", "core.exe", "--embedding-model", "model.gguf"],
        vec![
            "--prebuilt",
            "core.exe",
            "--validate-only",
            "--embedding-model",
        ],
        vec![
            "--prebuilt",
            "core.exe",
            "--validate-only",
            "--embedding-model",
            "one",
            "--embedding-model",
            "two",
        ],
        vec!["--validate-only", "--force", "--prebuilt", "core.exe"],
        vec!["--validate-only", "--service", "--prebuilt", "core.exe"],
        vec![
            "--validate-only",
            "--validate-only",
            "--prebuilt",
            "core.exe",
        ],
    ] {
        assert!(
            parse(&args).is_err(),
            "invalid request must stop at parsing: {args:?}"
        );
    }
    let service = parse(&["--service", "--prebuilt", "core.exe"]);
    let digest = "a".repeat(64);
    let bound_service = parse(&[
        "--service",
        "--prebuilt",
        "core.exe",
        "--service-descriptor-sha",
        &digest,
    ]);
    if cfg!(windows) {
        assert_eq!(
            bound_service.unwrap().service_descriptor_sha.as_deref(),
            Some(digest.as_str())
        );
    } else {
        assert!(bound_service.is_err());
    }
    assert!(parse(&[
        "--prebuilt",
        "core.exe",
        "--validate-only",
        "--service-descriptor-sha",
        &digest
    ])
    .is_err());
    let validate = parse(&["--prebuilt", "core.exe", "--validate-only"]).unwrap();
    assert!(validate.validate_only);
    assert!(!validate.force && !validate.service);
    let embedding = parse(&[
        "--prebuilt",
        "core.exe",
        "--validate-only",
        "--embedding-model",
        "model dir/embed.gguf",
    ])
    .unwrap();
    assert_eq!(
        embedding.embedding_model.as_deref(),
        Some(std::path::Path::new("model dir/embed.gguf"))
    );
    if cfg!(windows) {
        let service = service.unwrap();
        assert!(service.service);
        assert!(!service.force);
    } else {
        assert!(
            service.is_err(),
            "scheduled task deployment is Windows-only"
        );
    }
}

// what this catches: the normal Windows update must refuse a stale task
// BEFORE stopping the live core, even when its description names the new image.
#[test]
fn service_reboot_rejects_stale_action_and_artifact_before_teardown() {
    let tempdir = tempfile::tempdir().unwrap();
    // The directory as an installer receipt names it, BEFORE resolution: on Windows an
    // ordinary path, whose resolved form carries the verbatim `\\?\` prefix.
    let ordinary = tempdir.path().to_path_buf();
    let directory = ordinary.canonicalize().unwrap();
    #[cfg(windows)]
    {
        // Receipt migration and direct reboot share the installer's real
        // exclusive file lease; release permits the next owner to proceed.
        let lease = super::take_install_lease(&directory).unwrap();
        assert!(super::take_install_lease(&directory).is_err());
        drop(lease);
        assert!(super::take_install_lease(&directory).is_ok());
    }
    let artifact = directory.join("continuum-core-server.exe");
    let cli = directory.join("continuum.exe");
    let launcher = directory.join("run-service-hidden.ps1");
    let shell = directory.join("powershell.exe");
    let stale = directory.join("old-core.exe");
    let engine_directory = directory.join("engine-slot");
    std::fs::create_dir(&engine_directory).unwrap();
    let engine = engine_directory.join("llama-server.exe");
    for path in [&artifact, &cli, &launcher, &shell, &stale, &engine] {
        std::fs::write(path, b"installer artifact").unwrap();
    }
    let socket = directory.join("core.sock").display().to_string();
    let description = serde_json::json!({
        "artifact": artifact,
        "socket": socket,
        "cli": cli,
        "launcher": launcher,
        "engine": engine,
        "logDirectory": directory,
    });
    let arguments = format!(
        "-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy RemoteSigned -File \"{}\" -ExecutablePath \"{}\" -CorePath \"{}\" -SocketPath \"{}\" -EnginePath \"{}\" -LogDirectory \"{}\"",
        launcher.display(), cli.display(), artifact.display(), socket, engine.display(), directory.display(),
    );
    let mut task = super::CoreServiceTask {
        installed: None,
        working_directory: String::new(),
        description: description.to_string(),
        command: shell.display().to_string(),
        arguments: arguments.clone(),
        enabled: true,
        state: "Ready".to_string(),
    };
    let candidate = super::PrebuiltCore {
        path: artifact.canonicalize().unwrap(),
        build_sha: "123456789".to_string(),
    };
    task.validate(&candidate, &socket, &shell).unwrap();
    // regression for the 5090 2026-10-06 outage: an engine slot the release names, moved
    // aside, must fail the PRE-stop check (which needs no staged artifact), not only the
    // post-stop `validate`.
    task.runtime_paths_resolve().unwrap();
    let moved = engine_directory.with_file_name("engine-slot.moved");
    std::fs::rename(&engine_directory, &moved).unwrap();
    assert!(task.runtime_paths_resolve().is_err(), "a missing engine refuses before the stop");
    std::fs::rename(&moved, &engine_directory).unwrap();
    // A worker root is part of the registered release, not the scheduler's
    // cwd. Legacy releases omit it; new releases must pass it in the action.
    let mut with_eye = description.clone();
    with_eye["eyeRoot"] = serde_json::json!(directory);
    task.description = with_eye.to_string();
    assert!(task.validate(&candidate, &socket, &shell).is_err());
    task.arguments = format!("{arguments} -EyeRoot \"{}\"", directory.display());
    task.validate(&candidate, &socket, &shell).unwrap();
    with_eye["eyeRoot"] = serde_json::json!("relative/assets");
    task.description = with_eye.to_string();
    assert!(task.validate(&candidate, &socket, &shell).is_err());
    task.description = description.to_string();
    task.arguments = arguments.clone();
    // Engine-only migration can change the selected release without
    // changing its Core artifact; the transfer binds the full descriptor.
    {
        use sha2::{Digest, Sha256};
        let expected = format!("{:x}", Sha256::digest(task.description.as_bytes()));
        task.validate_description_sha(&expected).unwrap();
        let prior = task.description.clone();
        let mut changed: serde_json::Value = serde_json::from_str(&prior).unwrap();
        changed["engine"] = serde_json::json!(directory.join("other-engine.exe"));
        task.description = changed.to_string();
        assert!(task.validate_description_sha(&expected).is_err());
        task.description = prior;
    }
    task.arguments = arguments.replace(
        &artifact.display().to_string(),
        &stale.display().to_string(),
    );
    assert!(task.validate(&candidate, &socket, &shell).is_err());
    task.arguments = arguments;
    assert!(task.validate(&candidate, "different.sock", &shell).is_err());
    task.enabled = false;
    assert!(task.validate(&candidate, &socket, &shell).unwrap_err().contains("disabled"));
    task.enabled = true;
    assert!(task.validate(&candidate, "different.sock", &shell).unwrap_err().contains("different.sock"), "the socket refusal names both sockets");
    // regression for the 5090 2026-10-12 19:31Z (ce84f63ac; Codex traced it): the staged
    // core's path as the installer's receipt names it, RAW, and not already canonical. On
    // Windows that is the ordinary path, whose resolved form carries the verbatim `\\?\`
    // prefix (the failing case); elsewhere a symlinked directory gives the same shape. Once
    // resolved the one way it is the core the descriptor selects; raw, it is refused, and
    // the refusal names both paths.
    let raw = {
        #[cfg(windows)]
        {
            ordinary.join(artifact.file_name().unwrap())
        }
        #[cfg(unix)]
        {
            let link = directory.join("via-link");
            std::os::unix::fs::symlink(&ordinary, &link).unwrap();
            link.join(artifact.file_name().unwrap())
        }
    };
    assert_ne!(raw, super::resolve_service_path(&raw).unwrap(), "the receipt path is not already canonical");
    let staged = super::PrebuiltCore { path: super::resolve_service_path(&raw).unwrap(), build_sha: "123456789".to_string() };
    task.validate(&staged, &socket, &shell).unwrap();
    let unresolved = super::PrebuiltCore { path: raw.clone(), build_sha: "123456789".to_string() };
    let refused = task.validate(&unresolved, &socket, &shell).unwrap_err();
    assert!(refused.contains("selects artifact") && refused.contains(&raw.display().to_string()), "the artifact refusal names both paths: {refused}");
    let mut old_description = description;
    old_description["artifact"] = serde_json::json!(stale);
    task.description = old_description.to_string();
    assert!(task.validate(&candidate, &socket, &shell).unwrap_err().contains("selects artifact"));
}

// what this catches: card 67f53b63 — provenance is checked before a
// candidate can select the direct launch plan, including on installed nodes.
#[tokio::test]
async fn prebuilt_reboot_rejects_unusable_or_mismatched_candidates() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(
        super::PrebuiltCore::prepare(&tmp.path().join("absent-core"))
            .await
            .is_err()
    );
    assert!(super::PrebuiltCore::prepare(tmp.path()).await.is_err());
    let artifact = tmp.path().join("core");
    for report in ["", "unknown", "abc123", "not-a-sha", "def456a"] {
        assert!(super::PrebuiltCore::from_report(
            artifact.clone(),
            report.into(),
            Some(("abc123f", "git HEAD of this checkout"))
        )
        .is_err());
    }
    for report in ["", "unknown", "abc123", "not-a-sha"] {
        assert!(
            super::PrebuiltCore::from_report(artifact.clone(), report.into(), None).is_err()
        );
    }
    let ready =
        super::PrebuiltCore::from_report(artifact, "abc123f0123456789".into(), Some(("abc123f", "git HEAD of this checkout")))
            .unwrap();
    assert_eq!(ready.build_sha, "abc123f0123456789");
}

// Regression for PR3902: failed Git inside a checkout must not become the
// standalone self-anchor. Use real Git metadata and worktree selection;
// process cwd/environment stay local to each command, never global test state.
#[tokio::test]
async fn prebuilt_checkout_lookup_distinguishes_absence_from_failure() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    let standalone = tmp.path().join("standalone");
    let hooks = tmp.path().join("empty-hooks");
    for path in [&repo, &standalone, &hooks] {
        std::fs::create_dir(path).unwrap();
    }
    let git = |cwd: &std::path::Path, args: &[&str]| {
        let output = std::process::Command::new("git")
            .current_dir(cwd)
            .args([
                "-c",
                "user.name=Continuum tests",
                "-c",
                "user.email=tests@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "maintenance.auto=false",
                "-c",
                "gc.auto=0",
            ])
            .arg("-c")
            .arg(format!("core.hooksPath={}", hooks.display()))
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_OBJECT_DIRECTORY")
            .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
            .args(args)
            .output()
            .expect("Git is required for checkout provenance tests");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    };

    git(&repo, &["init", "--quiet"]);
    assert!(
        super::prebuilt_checkout_sha(&repo, None).await.is_err(),
        "a checkout with no readable HEAD cannot self-anchor"
    );
    git(
        &repo,
        &["commit", "--allow-empty", "--quiet", "-m", "fixture"],
    );
    git(&repo, &["checkout", "--detach", "--quiet", "HEAD"]);
    let head = git(&repo, &["rev-parse", "--short", "HEAD"]);
    assert_eq!(
        super::prebuilt_checkout_sha(&repo, None).await.unwrap(),
        Some(head.clone())
    );

    git(
        &repo,
        &[
            "worktree",
            "add",
            "--detach",
            "--quiet",
            "../linked",
            "HEAD",
        ],
    );
    let linked = tmp.path().join("linked");
    let nested = linked.join("src");
    std::fs::create_dir(&nested).unwrap();
    assert!(linked.join(".git").is_file());
    assert_eq!(
        super::prebuilt_checkout_sha(&nested, None).await.unwrap(),
        Some(head.clone())
    );

    // A sibling checkout must not bind a standalone invocation, but an
    // explicit GIT_DIR must bind it, including when that selection is broken.
    let absent = super::prebuilt_checkout_sha(&standalone, None)
        .await
        .unwrap();
    assert!(absent.is_none());
    assert!(super::PrebuiltCore::from_report(
        standalone.join("core"),
        "abc123f".into(),
        absent.as_deref().map(|h| (h, "git HEAD of this checkout")),
    )
    .is_ok());
    assert_eq!(
        super::prebuilt_checkout_sha(&standalone, Some(repo.join(".git").as_os_str()))
            .await
            .unwrap(),
        Some(head),
    );
    assert!(super::prebuilt_checkout_sha(
        &standalone,
        Some(tmp.path().join("missing").as_os_str())
    )
    .await
    .is_err());

    std::fs::write(linked.join(".git"), "gitdir: missing-checkout\n").unwrap();
    assert!(
        super::prebuilt_checkout_sha(&nested, None).await.is_err(),
        "a broken worktree gitfile is not a standalone installation"
    );
}

// what this catches (M5, 2026-10-05 01:55Z): a prebuilt core is CI's build of the
// checkout's BUILD KEY, so a docs-only HEAD on top of a core change must accept the
// key's artifact; HEAD itself is the expectation only when no build input was ever
// touched. Checked against HEAD, every docs-only tip refused in two seconds.
#[tokio::test]
async fn a_prebuilt_is_expected_to_be_the_checkouts_build_key_not_its_head() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    let hooks = tmp.path().join("empty-hooks");
    std::fs::create_dir(&repo).unwrap();
    std::fs::create_dir(&hooks).unwrap();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .current_dir(&repo)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid", "-c", "commit.gpgsign=false"])
            .arg("-c")
            .arg(format!("core.hooksPath={}", hooks.display()))
            .env_remove("GIT_DIR")
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    git(&["init", "-q"]);
    std::fs::create_dir_all(repo.join("core")).unwrap();
    std::fs::write(repo.join("core/lib.rs"), "// core").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "core change"]);
    let key = git(&["rev-parse", "HEAD"]);
    std::fs::write(repo.join("README.md"), "docs only").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "docs only"]);
    let head = git(&["rev-parse", "HEAD"]);
    assert_ne!(key, head);
    let resolved = continuum_cli_lifecycle::prebuilt_artifact::checkout_build_key(&repo, None, &head);
    assert!(matches!(resolved, continuum_cli_lifecycle::prebuilt_artifact::KeyResolution::BuildKey(_)), "{resolved:?}");
    let (expected, source) = (resolved.sha().to_string(), resolved.source());
    assert_eq!(expected, key, "the expectation is the key, not HEAD");
    // The key's artifact is accepted; HEAD's would not have been built by CI at all.
    assert!(super::PrebuiltCore::from_report(repo.join("core-bin"), key.clone(), Some((&expected, source))).is_ok());
    assert!(super::PrebuiltCore::from_report(repo.join("core-bin"), head.clone(), Some((&expected, source))).is_err());
}

// what this catches: card 67f53b63 — an inherited build request, an available
// source script or installed artifact must not redirect an explicit candidate.
// Assert the production Command's path/argv/cwd, not a duplicate launch model.
#[test]
fn prebuilt_reboot_launch_retains_candidate_socket_and_cwd() {
    // The launcher starts media before its service-host child runs. The
    // pre-drain policy must therefore reject elevated/unknown callers,
    // including a stale marker, without attempting recursive relaunch.
    assert!(super::require_prebuilt_runtime_medium(Ok(0x2000), false).is_ok());
    assert!(super::require_prebuilt_runtime_medium(Ok(0x2000), true).is_ok());
    assert!(super::require_prebuilt_runtime_medium(Ok(0x3000), false).unwrap_err().contains("normal-user terminal"));
    assert!(super::require_prebuilt_runtime_medium(Ok(0x3000), true).is_err());
    assert!(super::require_prebuilt_runtime_medium(Err("token unavailable".into()), false).is_err());
    let candidate = super::PrebuiltCore::from_report(
        std::path::PathBuf::from("verified release/core.exe"),
        "abc123f".into(),
        None,
    )
    .unwrap();
    let runtime = continuum_cli_lifecycle::installed_release::Release {
        artifact: "installed/core.exe".into(), socket: "registered socket".into(),
        launcher: "sealed/run-service-hidden.ps1".into(), cli: "sealed/continuum.exe".into(),
        engine: "sealed/engine-c/llama-server.exe".into(), log_directory: "service logs".into(),
        eye_root: Some("installed browser assets".into()),
    };
    for have_script in [false, true] {
        for have_installed in [false, true] {
            for env_from_source in [false, true] {
                // Regression 337b2c67: exercise the SAME command constructor
                // as production, not only the pure plan followed by a
                // disconnected direct-command assertion. Neither inherited
                // source intent nor a discoverable script can build here.
                assert_eq!(super::plan_launch(super::LaunchSource::ExplicitPrebuilt(&candidate, Some(&runtime)), env_from_source, have_script, have_installed), super::LaunchPlan::Prebuilt);
                let command = super::explicit_prebuilt_command(&candidate, "socket from caller", Some(&runtime), std::path::Path::new("powershell.exe"));
                let args = command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>();
                assert_eq!(command.get_program(), "powershell.exe");
                for pair in [
                    ("-File", runtime.launcher.as_str()), ("-ExecutablePath", runtime.cli.as_str()),
                    ("-CorePath", "verified release/core.exe"), ("-SocketPath", "socket from caller"),
                    ("-EnginePath", runtime.engine.as_str()), ("-EyeRoot", "installed browser assets"),
                ] { assert!(args.windows(2).any(|args| args == [pair.0, pair.1]), "missing runtime argument {pair:?}: {args:?}"); }
                assert!(!args.iter().any(|arg| arg.contains("start-server") || arg == "cargo"));
                assert_eq!(
                    super::plan_launch(
                        super::LaunchSource::Prebuilt(&candidate),
                        env_from_source,
                        have_script,
                        have_installed
                    ),
                    super::LaunchPlan::Prebuilt,
                );
            }
        }
    }
    let cmd = super::explicit_prebuilt_command(&candidate, "socket from caller", None, std::path::Path::new("powershell.exe"));
    assert_eq!(cmd.get_program(), candidate.path.as_os_str());
    assert_eq!(
        cmd.get_args().collect::<Vec<_>>(),
        vec![std::ffi::OsStr::new("socket from caller")]
    );
    assert!(
        cmd.get_current_dir().is_none(),
        "inherit repository cwd and its AIRC/workspace scope"
    );
    assert!(super::deploy_verdict(
        Some("def456a"),
        &candidate.build_sha,
        "prebuilt candidate",
        "replacement core"
    )
    .is_err());
    assert!(super::deploy_verdict(
        Some("abc123f"),
        &candidate.build_sha,
        "prebuilt candidate",
        "replacement core"
    )
    .is_ok());
}

// what this catches: an unmatched phrasing falling through to NoAnswer, which passes the reboot guard (#3940).
// Regression for #3940 (revised): any error from a reachable core fails closed — never
// NoAnswer, which used to be the fall-through and passes the reboot guard. Phrasings that
// mean "this name is not in the command registry" are positive evidence of a pre-rail
// core (`LegacyCore`, a first rollout); every other error is a fault inside a supported
// core (`CannotAttest`) and must not be rendered as one. Both fail-closed outcomes are
// refused by `ensure_teardown_supported` before any kill.
#[test]
fn shutdown_errors_fail_closed_never_no_answer() {
    use super::classify_shutdown_error;
    use super::GracefulStop;
    for (rendered, pre_rail) in [
        ("unknown command: system/shutdown", true),
        ("no handler registered for 'system/shutdown'", true),
        // These used to slip through as NoAnswer and pass the reboot guard.
        ("the core refused the request", false),
        ("checkpoint not found", false),
        ("save_state panicked in module X", false),
        ("", false),
    ] {
        let outcome = classify_shutdown_error(rendered);
        match (pre_rail, &outcome) {
            (true, GracefulStop::LegacyCore(reason)) => assert!(reason.contains(rendered)),
            (false, GracefulStop::CannotAttest(reason)) => assert!(reason.contains(rendered)),
            other => panic!("{rendered:?} must fail closed: got {other:?}"),
        }
        // Caller level: both fail-closed outcomes are refused before reboot.
        assert!(outcome.ensure_teardown_supported(true).is_err(), "{rendered:?}");
    }
}

// what this catches: recognition deciding WHETHER to fail closed — it may only pick which of the two labels (#3940).
// Recognition only picks WHICH label, never whether to fail closed: an internal error like
// `checkpoint not found` must still be refused, but must not carry the first-rollout claim.
#[test]
fn pre_rail_label_is_reserved_for_unknown_verb_phrasings() {
    use super::classify_shutdown_error;
    use super::GracefulStop;
    let legacy = match classify_shutdown_error("unknown command: system/shutdown") {
        GracefulStop::LegacyCore(reason) => reason,
        other => panic!("unknown-verb phrasing must be LegacyCore: got {other:?}"),
    };
    assert!(legacy.contains("no system/shutdown verb"));
    assert!(legacy.contains("predates this rail"));
    let fault = match classify_shutdown_error("checkpoint not found") {
        GracefulStop::CannotAttest(reason) => reason,
        other => panic!("internal error must be CannotAttest: got {other:?}"),
    };
    assert!(!fault.contains("no system/shutdown verb"));
    assert!(!fault.contains("predates this rail"));
}

// what this catches: a checkpoint-not-found fault rendered to the operator as FIRST UPGRADE / first rollout (#3940 revision).
// What the reviewer asked for (#3940 revision): a caller/rendering regression test — not
// just an assertion on the classifier's reason string. The checkpoint-not-found path must
// verify BOTH that reboot is refused AND that what the operator sees carries no first-
// rollout claim, while a true LegacyCore still does.
#[test]
fn cannot_attest_refuses_reboot_and_never_renders_as_first_rollout() {
    use super::classify_shutdown_error;
    use super::render_graceful_outcome;
    use super::GracefulStop;
    let outcome = classify_shutdown_error("checkpoint not found");
    match &outcome {
        GracefulStop::CannotAttest(_) => (),
        other => panic!("internal error must be CannotAttest: got {other:?}"),
    }
    // Caller level: reboot is refused before any kill.
    assert!(outcome.ensure_teardown_supported(true).is_err());
    // Rendering level: no first-rollout claim, on stderr like every forced stop.
    let (to_stderr, line) = render_graceful_outcome(&outcome);
    assert!(to_stderr);
    assert!(!line.contains("FIRST UPGRADE"), "{line}");
    assert!(!line.contains("predates"), "{line}");
    assert!(line.contains("checkpoint not found"), "{line}");
    // A true first rollout still says so.
    let (legacy_stderr, legacy_line) = render_graceful_outcome(&GracefulStop::LegacyCore(
        "no shutdown handler".into(),
    ));
    assert!(legacy_stderr && legacy_line.contains("FIRST UPGRADE"));
    // And the clean outcomes keep their placement.
    assert!(!render_graceful_outcome(&GracefulStop::Durable("saved".into())).0);
    assert!(!render_graceful_outcome(&GracefulStop::NothingRunning).0);
}

// what this catches (card 9beab2da, Codex on #5551 at 04b1aa49b): a hold that decoded but whose
// cause is Unknown (the residency record unreadable) mapped to Free, so the handoff proceeded
// past work it could not see. Holds decoded from serving/status's own wire shape: none is Free,
// resident work is LiveWork, an Unknown cause is Unreadable (deferred, forceable).
#[test]
fn a_decoded_hold_of_unknown_cause_is_unreadable_never_free() {
    use continuum_cli_lifecycle::deploy_hold::HoldObservation;
    use continuum_core::inference::llama_server::serving_hold::ServingHold;
    let decode = |held: serde_json::Value| serde_json::from_value::<ServingHold>(held).expect("test: a serving/status held_for");
    assert_eq!(observation_of(None), HoldObservation::Free);
    let resident = decode(json!({"sinceMs": 1, "by": {"kind": "resident", "work": [{
        "job": "8b46862c-a328-4904-9444-2ae9eeab143f", "consumer": "genome-train:8b46862c-a328-4904-9444-2ae9eeab143f",
        "baseModel": "ggml-org/Qwen3.8-27B-GGUF", "enginePid": 37904, "engineStartedS": 1793118524, "port": 58057}]}}));
    assert!(matches!(observation_of(Some(&resident)), HoldObservation::LiveWork(work) if work.contains("genome-train:8b46862c")));
    let unknown = decode(json!({"sinceMs": 1, "by": {"kind": "unknown", "reason": "the residency record is unreadable"}}));
    assert!(matches!(observation_of(Some(&unknown)), HoldObservation::Unreadable(why) if why.contains("residency record is unreadable")));
    let empty = decode(json!({"sinceMs": 1, "by": {"kind": "resident", "work": []}}));
    assert_eq!(observation_of(Some(&empty)), HoldObservation::Free);
}
