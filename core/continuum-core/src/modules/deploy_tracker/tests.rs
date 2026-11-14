#![cfg(test)]
//! The deploy tracker module's tests (moved out whole so the module stays under 1,000 lines).

use super::*;

// what this catches (the IntelMac, 2026-11-07, card fd833a10): a deploy launched while live
// work holds the engine. The consumer's handoff refuses then, but every launch spends one of
// the actuator's three attempts, so three refusals behind a training run gave the tip up for
// good after the run ended. While the hold names live work nothing is launched and no attempt
// is recorded; the first action after the release launches attempt one.
#[tokio::test]
async fn a_held_engine_launches_nothing_and_its_release_launches_attempt_one() {
    use crate::inference::llama_server::serving_hold::{HeldWork, HoldCause, ServingHold};
    use crate::inference::llama_server::ServingSnapshot;
    use super::super::deploy_actuator::RecordingSpawner;
    let Ok(dir) = tempfile::tempdir() else { panic!("a temp dir") };
    let user_home = dir.path().join("home");
    let root = user_home.join(".continuum");
    assert!(std::fs::create_dir_all(&root).is_ok());
    // A launch through the spawner leaves a receipt carrying its pid (a failed one carries none).
    let actuator = Arc::new(DeployActuator::with_spawner(user_home, root.clone(), None, Box::new(RecordingSpawner::new(Some(4242)))));
    let training = ServingHold {
        since_ms: 1,
        by: HoldCause::Resident {
            work: vec![HeldWork {
                job: uuid::Uuid::from_u128(0xf44e),
                consumer: "genome-train:f44eda06".into(),
                base_model: "continuum-ai/qwen2.5-coder-1.5b-instruct-GGUF".into(),
                engine_pid: 70964,
                engine_started_s: 1,
                port: 58057,
            }],
        },
    };
    let (serving_tx, serving) = tokio::sync::watch::channel(ServingSnapshot { held_for: Some(training), ..ServingSnapshot::empty() });
    let tracker = DeployTrackerModule {
        source: Box::new(GitGhDeploySource::from_env()),
        state_dir: root.join("state"),
        root,
        stranded_reported: parking_lot::Mutex::new(None),
        actuator: actuator.clone(),
        serving,
        running: running_sha(),
        state_writer: None,
    };
    let tip = "aef1f6b362201e84000702fd9f74e9eab31defc8";
    let now = 1_794_000_000_000;
    for tick in 0..3 {
        assert!(tracker.actuate(DeployRequest::new(tip, now), tip, true, now + tick * 3_600_000).await.is_ok());
    }
    assert!(actuator.receipt().is_none(), "held: nothing launched, no attempt recorded");
    serving_tx.send_modify(|snapshot| snapshot.held_for = None);
    assert!(tracker.actuate(DeployRequest::new(tip, now), tip, false, now + 4 * 3_600_000).await.is_ok());
    assert_eq!(actuator.receipt().and_then(|receipt| receipt.pid), Some(4242), "released: launched");
    assert_eq!(actuator.receipt().map(|receipt| receipt.attempt), Some(1), "the first launch is attempt one");
}

/// A source that answers one fixed tip, for driving whole ticks without git or gh.
struct FixedTip(&'static str, Checks);

#[async_trait]
impl DeploySource for FixedTip {
    async fn tip(&self) -> Result<Option<(String, Checks)>, String> {
        Ok(Some((self.0.to_string(), self.1)))
    }
}

// what this catches (the M5, 2026-11-14: three hand deploys in a few hours while the tracker
// ticked): the whole tick, not only `decide`. A green tip newer than the running build records
// exactly ONE DeployRequest and launches the consumer once across ticks; the running build at
// the tip records none and launches nothing; a tip with no published core records no request
// but leaves its typed wait for the health line, and a deploy clears it.
#[tokio::test]
async fn a_newer_green_tip_requests_one_deploy_and_the_running_tip_requests_none() {
    use crate::inference::llama_server::ServingSnapshot;
    use crate::system_resources::disk_reporters::TrackedDir;
    use continuum_cli_lifecycle::deploy_health::{self, DeployHealth};
    use super::super::deploy_actuator::RecordingSpawner;
    const RUNNING: &str = "a9b055a83";
    const NEWER: &str = "b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0";
    // Launches are read off the actuator's receipt: a launch through the spawner carries its pid.
    let tracker_on = |root: &std::path::Path, source: FixedTip| {
        let state_dir = root.join("state");
        assert!(std::fs::create_dir_all(&state_dir).is_ok());
        let actuator = Arc::new(DeployActuator::with_spawner(root.join("home"), root.to_path_buf(), None, Box::new(RecordingSpawner::new(Some(4242)))));
        let (_serving_tx, serving) = tokio::sync::watch::channel(ServingSnapshot::empty());
        DeployTrackerModule {
            source: Box::new(source),
            root: root.to_path_buf(),
            state_writer: Some(TrackedDir::new("state", root.join("state")).writer()),
            state_dir,
            stranded_reported: parking_lot::Mutex::new(None),
            actuator,
            serving,
            running: RUNNING,
        }
    };
    let read_request = |root: &std::path::Path| read_deploy_request(&root.join("state"));

    // Newer and green: one request, one launch, and the second tick re-stamps nothing.
    let Ok(dir) = tempfile::tempdir() else { panic!("a temp dir") };
    let tracker = tracker_on(dir.path(), FixedTip(NEWER, Checks::Green));
    assert!(tracker.tick().await.is_ok());
    let Some(first) = read_request(dir.path()) else { panic!("a newer green tip records a request") };
    assert_eq!(first.tip_sha, NEWER);
    let Some(launch) = tracker.actuator.receipt() else { panic!("the consumer is launched for the request") };
    assert_eq!((launch.pid, launch.attempt), (Some(4242), 1), "launched through the spawner, attempt one");
    assert!(tracker.tick().await.is_ok());
    assert_eq!(read_request(dir.path()), Some(first), "exactly one request: the second tick keeps the first");
    assert_eq!(tracker.actuator.receipt().map(|again| (again.spawned_ms, again.attempt)), Some((launch.spawned_ms, 1)), "and one launch: the second tick launches nothing");
    assert!(!dir.path().join("state").join(TRACKER_STANDING_FILE).exists(), "a deploy wanted is no standing block");

    // The running build IS the tip: no request, no launch, no block.
    let Ok(dir) = tempfile::tempdir() else { panic!("a temp dir") };
    let tracker = tracker_on(dir.path(), FixedTip("a9b055a83614d90f029818e32333160a064583bc", Checks::Green));
    assert!(tracker.tick().await.is_ok());
    assert_eq!(read_request(dir.path()), None, "the running tip requests nothing");
    assert!(tracker.actuator.receipt().is_none(), "and launches nothing");
    assert!(matches!(deploy_health::read(dir.path(), RUNNING), DeployHealth::NoRequest));

    // No published core yet: no request, and the health line names the typed wait.
    let Ok(dir) = tempfile::tempdir() else { panic!("a temp dir") };
    let tracker = tracker_on(dir.path(), FixedTip(NEWER, Checks::Unpublished));
    assert!(tracker.tick().await.is_ok());
    assert_eq!(read_request(dir.path()), None, "nothing published: nothing requested");
    assert!(tracker.actuator.receipt().is_none(), "and nothing launched");
    assert!(
        matches!(deploy_health::read(dir.path(), RUNNING), DeployHealth::NotRequested { standing } if standing.verdict == DeployVerdict::AwaitingPublication),
        "the wait reaches the health line"
    );
}

fn verdict(check_runs: &str, workflow_runs: Option<&str>) -> Checks {
    parse_tip_checks(check_runs, workflow_runs).checks
}

// what this catches: GitHub's check-runs body maps to the same verdict track-canary
// used — a hard-failed run is red, a not-completed run is pending, all-completed is
// green, and NO runs is unknown (a tip with no CI must WAIT, never deploy). With no
// workflow-runs read the rule is the all-checks rule it always was.
#[test]
fn check_runs_map_to_the_same_verdict_as_the_shell_owner() {
    assert_eq!(verdict(r#"{"check_runs":[{"status":"completed","conclusion":"success"}]}"#, None), Checks::Green);
    assert_eq!(verdict(r#"{"check_runs":[{"status":"completed","conclusion":"success"},{"status":"in_progress"}]}"#, None), Checks::Pending);
    assert_eq!(verdict(r#"{"check_runs":[{"status":"completed","conclusion":"failure"}]}"#, None), Checks::Red);
    assert_eq!(verdict(r#"{"check_runs":[{"status":"completed","conclusion":"cancelled"}]}"#, None), Checks::Red);
    assert_eq!(verdict(r#"{"check_runs":[]}"#, None), Checks::Unknown, "no CI yet = wait, never green");
    assert_eq!(verdict("not json", None), Checks::Unknown);
}

// what this catches (card 2d333f25; the canary tip 6d342745b, 2026-09-20 00:06Z): a
// weekly SCHEDULED audit failed and landed its check-run on the tip alongside the
// push's three green suites. The all-checks rule read RED and every deploy on the
// fleet refused. The tip's verdict is its OWN push's suites: the schedule's suite is
// excluded, the tip is green. The same body with the runs read absent still reads red
// (degraded, not a lie); a runs read that answers with NO own suite is unknown, never
// green by absence; and a failure in an OWN suite is still red. The shape is the live
// API's on that sha (7 check-runs, 4 workflow runs), abbreviated.
#[test]
fn a_scheduled_audits_failure_is_not_the_tips_verdict() {
    let checks = r#"{"check_runs":[
        {"name":"audit","status":"completed","conclusion":"failure","check_suite":{"id":96070172760}},
        {"name":"cargo test","status":"completed","conclusion":"success","check_suite":{"id":96068500514}},
        {"name":"drift guard","status":"completed","conclusion":"success","check_suite":{"id":96068500520}},
        {"name":"install smoke","status":"completed","conclusion":"success","check_suite":{"id":96068500507}}
    ]}"#;
    let runs = r#"{"workflow_runs":[
        {"id":35477870572,"event":"schedule","check_suite_id":96070172760},
        {"id":35477208032,"event":"push","check_suite_id":96068500520},
        {"id":35477208024,"event":"push","check_suite_id":96068500507},
        {"id":35477208029,"event":"push","check_suite_id":96068500514}
    ]}"#;
    let read = parse_tip_checks(checks, Some(runs));
    assert_eq!(read, TipChecks { checks: Checks::Green, total: 4, excluded: 1, filtered: true }, "the schedule's failure is excluded: green");
    assert_eq!(parse_tip_checks(checks, None), TipChecks { checks: Checks::Red, total: 4, excluded: 0, filtered: false }, "no runs read = every check counts, as before");
    assert_eq!(parse_tip_checks(checks, Some("not json")), TipChecks { checks: Checks::Red, total: 4, excluded: 0, filtered: false }, "an unparseable runs read = degraded, not green");
    assert_eq!(verdict(checks, Some(r#"{"workflow_runs":[{"id":1,"event":"schedule","check_suite_id":96070172760}]}"#)), Checks::Unknown, "only a schedule suite = no own suite yet = wait, never green by absence");
    let own_failure = r#"{"check_runs":[{"status":"completed","conclusion":"failure","check_suite":{"id":96068500514}}]}"#;
    assert_eq!(verdict(own_failure, Some(runs)), Checks::Red, "a failure in the tip's own suite is the tip's verdict");
    let pending = r#"{"check_runs":[{"status":"in_progress","check_suite":{"id":96068500514}},{"status":"completed","conclusion":"failure","check_suite":{"id":96070172760}}]}"#;
    assert_eq!(verdict(pending, Some(runs)), Checks::Pending, "the schedule's failure does not pre-empt the push's pending suite");
}

// what this catches (card 7c0990b0): the promote-main workflow runs on the default
// branch and can be started by hand — `workflow_dispatch`, an OWN-suite event — so the
// event filter alone would let its failure read as the tip's. A run of that workflow
// is excluded by PATH whatever its event; the push's own suites still decide, and a
// sha whose only own suite is promote-main's is unknown, never green by absence.
#[test]
fn a_promote_main_run_is_never_the_tips_verdict_whatever_its_event() {
    let checks = r#"{"check_runs":[
        {"name":"promote-main","status":"completed","conclusion":"failure","check_suite":{"id":11}},
        {"name":"cargo test","status":"completed","conclusion":"success","check_suite":{"id":22}}
    ]}"#;
    let runs = r#"{"workflow_runs":[
        {"id":1,"event":"workflow_dispatch","path":".github/workflows/promote-main.yml","check_suite_id":11},
        {"id":2,"event":"push","path":".github/workflows/continuum-rust-tests.yml","check_suite_id":22}
    ]}"#;
    assert_eq!(parse_tip_checks(checks, Some(runs)), TipChecks { checks: Checks::Green, total: 2, excluded: 1, filtered: true }, "a dispatched promote-main failure is excluded by path: green");
    let in_flight = r#"{"check_runs":[
        {"name":"promote-main","status":"in_progress","check_suite":{"id":11}},
        {"name":"cargo test","status":"completed","conclusion":"success","check_suite":{"id":22}}
    ]}"#;
    assert_eq!(verdict(in_flight, Some(runs)), Checks::Green, "a promote-main run in flight is not a pending tip");
    let only_promote = r#"{"workflow_runs":[{"id":1,"event":"push","path":".github/workflows/promote-main.yml","check_suite_id":11}]}"#;
    assert_eq!(verdict(checks, Some(only_promote)), Checks::Unknown, "promote-main as the only suite = no own suite = wait, never green by absence");
    let own_failure = r#"{"check_runs":[{"name":"cargo test","status":"completed","conclusion":"failure","check_suite":{"id":22}}]}"#;
    assert_eq!(verdict(own_failure, Some(runs)), Checks::Red, "the push's own failure is still the tip's verdict");
}

// what this catches (M5 2026-10-04, 01:36Z-04:00Z): the core-binaries workflow (#4691)
// runs 44-73 min per macOS leg on every canary push and cancels the previous tip's
// run; counted as the tip's verdict it held the tracker at ChecksPending across five
// merges and nothing deployed for 2.5 h. It publishes an artifact; it judges no code.
// A tip whose own suites are green is green while core-binaries is in flight or
// cancelled, and a sha whose only suite is core-binaries is unknown, never green.
#[test]
fn a_core_binaries_run_is_never_the_tips_verdict() {
    let runs = r#"{"workflow_runs":[
        {"id":1,"event":"push","path":".github/workflows/core-binaries.yml","check_suite_id":31},
        {"id":2,"event":"push","path":".github/workflows/continuum-rust-tests.yml","check_suite_id":22}
    ]}"#;
    let in_flight = r#"{"check_runs":[
        {"name":"core binaries (macos-arm64)","status":"in_progress","check_suite":{"id":31}},
        {"name":"core binaries (macos-x86_64)","status":"in_progress","check_suite":{"id":31}},
        {"name":"cargo test","status":"completed","conclusion":"success","check_suite":{"id":22}}
    ]}"#;
    assert_eq!(
        parse_tip_checks(in_flight, Some(runs)),
        TipChecks { checks: Checks::Green, total: 3, excluded: 2, filtered: true },
        "core-binaries in flight is not a pending tip"
    );
    let cancelled = r#"{"check_runs":[
        {"name":"core binaries (macos-arm64)","status":"completed","conclusion":"cancelled","check_suite":{"id":31}},
        {"name":"cargo test","status":"completed","conclusion":"success","check_suite":{"id":22}}
    ]}"#;
    assert_eq!(verdict(cancelled, Some(runs)), Checks::Green, "a cancelled core-binaries run (the next push superseded it) is not a red tip");
    let only_binaries = r#"{"workflow_runs":[{"id":1,"event":"push","path":".github/workflows/core-binaries.yml","check_suite_id":31}]}"#;
    assert_eq!(verdict(in_flight, Some(only_binaries)), Checks::Unknown, "core-binaries as the only suite = no own suite = wait, never green by absence");
}

// what this catches (card 28463ba7, enrico 2026-10-27): the cache warm-up cancelled when a
// newer canary push queued behind it, read as a red commit, so a published build of it
// (791328aab, 8d6656f11) could never be authorized and the node ran stale code. The warm-up
// judges no code: a commit whose tests are green is green whether it is queued or cancelled.
#[test]
fn a_cancelled_cache_warm_up_is_never_the_tips_verdict() {
    let runs = r#"{"workflow_runs":[
        {"id":1,"event":"push","path":".github/workflows/rust-warm-cache.yml","check_suite_id":41},
        {"id":2,"event":"push","path":".github/workflows/continuum-rust-tests.yml","check_suite_id":22}
    ]}"#;
    let cancelled = r#"{"check_runs":[
        {"name":"Warm the shared Rust build cache","status":"completed","conclusion":"cancelled","check_suite":{"id":41}},
        {"name":"cargo test -p continuum-core --lib","status":"completed","conclusion":"success","check_suite":{"id":22}}
    ]}"#;
    assert_eq!(
        parse_tip_checks(cancelled, Some(runs)),
        TipChecks { checks: Checks::Green, total: 2, excluded: 1, filtered: true },
        "a superseded cache warm-up is not a red commit"
    );
    let queued = r#"{"check_runs":[
        {"name":"Warm the shared Rust build cache","status":"queued","check_suite":{"id":41}},
        {"name":"cargo test -p continuum-core --lib","status":"completed","conclusion":"success","check_suite":{"id":22}}
    ]}"#;
    assert_eq!(verdict(queued, Some(runs)), Checks::Green, "a queued cache warm-up is not a pending commit");
    // The exclusion is the warm-up's own suite only: a REAL test cancelled still reads red.
    let real_cancelled = r#"{"check_runs":[
        {"name":"Warm the shared Rust build cache","status":"completed","conclusion":"success","check_suite":{"id":41}},
        {"name":"cargo test -p continuum-core --lib","status":"completed","conclusion":"cancelled","check_suite":{"id":22}}
    ]}"#;
    assert_eq!(verdict(real_cancelled, Some(runs)), Checks::Red, "a cancelled real test is still a red commit");
    // Nothing gating left once the warm-up is excluded: wait, never green by absence.
    let warm_only = r#"{"workflow_runs":[{"id":1,"event":"push","path":".github/workflows/rust-warm-cache.yml","check_suite_id":41}]}"#;
    assert_eq!(verdict(cancelled, Some(warm_only)), Checks::Unknown, "the warm-up as the only suite = unknown");
}
