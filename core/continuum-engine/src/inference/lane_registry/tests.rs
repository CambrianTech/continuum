//! Lane registry regressions, compiled only by the parent test module.
#![cfg(test)]

use super::*;

// what this catches (Astra, #4069): a destructive census must fail CLOSED. A `.lane`
// file that does not parse is a lane the lenient read silently drops — and a sweep
// reading that inventory would delete the page dir of a lane it never saw. The
// lenient read stays lenient (admission counts); the checked read refuses.
#[test]
fn a_malformed_record_fails_the_checked_census_and_only_that_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let good = LaneRecord {
        pid: 1,
        port: 1,
        role: LaneRole::Ephemeral,
        model: "m".into(),
        context_window: 1,
        lanes: 1,
        page_dir: None,
        engine_bin: None,
        weights: None,
        started_s: 0,
        placement: None,
    };
    record_in(dir.path(), &good).expect("write");
    assert_eq!(records_checked_in(dir.path()).expect("clean registry").len(), 1);
    std::fs::write(dir.path().join("garbage.lane"), "{not json").expect("write");
    assert_eq!(records_in(dir.path()).len(), 1, "the lenient read drops it");
    assert!(records_checked_in(dir.path()).is_err(), "the checked read refuses");
    let missing = dir.path().join("never-made");
    assert!(records_checked_in(&missing).expect("missing dir is empty, not an error").is_empty());
}

// what this catches (Codex on #5270): a placement written over a lane that changed after
// it was read. The restamp writes only onto the exact record it was given; a replaced
// record is left as it is, a removed one is never recreated, and another live lane's
// record survives (record_in would have cleared it).
#[test]
fn a_restamp_writes_only_onto_the_record_it_read() {
    use crate::inference::llama_server::LanePlacement;
    let dir = tempfile::tempdir().expect("tempdir");
    let kept = LaneRecord { started_s: 7, ..rec(10, 58057, LaneRole::Live) };
    record_in(dir.path(), &kept).expect("write");
    let other = LaneRecord { started_s: 9, ..rec(11, 58058, LaneRole::Ephemeral) };
    record_in(dir.path(), &other).expect("write");
    assert!(restamp_placement_in(dir.path(), &kept, LanePlacement::Cpu).expect("restamp"));
    let after = records_in(dir.path());
    assert!(after.contains(&LaneRecord { placement: Some(LanePlacement::Cpu), ..kept.clone() }));
    assert!(after.contains(&other), "no other record is touched");
    assert!(!restamp_placement_in(dir.path(), &kept, LanePlacement::Gpu).expect("restamp"), "the record changed since it was read");
    let replaced = LaneRecord { started_s: 8, ..kept.clone() };
    assert!(!restamp_placement_in(dir.path(), &replaced, LanePlacement::Cpu).expect("restamp"), "another incarnation's read");
    remove_in(dir.path(), kept.pid);
    assert!(!restamp_placement_in(dir.path(), &kept, LanePlacement::Cpu).expect("restamp"), "a removed record");
    assert_eq!(records_in(dir.path()), vec![other], "a removed record is never recreated");
}

/// A unique temp lanes dir per test — NEVER the real `~/.continuum/run/lanes/`,
/// so a live core's registry is untouched and parallel tests don't collide (#7
/// isolation rule).
fn temp_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "continuum-lane-registry-test-{tag}-{}",
        std::process::id()
    ))
}

fn rec(pid: u32, port: u16, role: LaneRole) -> LaneRecord {
    LaneRecord {
        pid,
        port,
        role,
        model: "test-model".into(),
        // A realistic shape: these tests exercise sweep/reap, but a record with a
        // real shape is what production writes, so the fixture matches it.
        context_window: 16_384,
        lanes: 4,
        page_dir: None,
        engine_bin: None,
        weights: None,
        started_s: 0,
        placement: None,
    }
}

// what this catches: failed/asynchronous termination must not erase the
// registry or claim success; a later observed exit permits cleanup.
#[test]
fn reap_retains_record_until_exit_is_observed() {
    let dir = tempfile::tempdir().expect("temporary registry");
    let live = rec(std::process::id(), 58200, LaneRole::Live);
    record_in(dir.path(), &live).expect("record");
    let path = record_path(dir.path(), live.pid);
    assert_eq!(finish_reap(&path, &live), SweepOutcome::ExitUnconfirmed { pid: live.pid, port: live.port });
    assert!(path.exists(), "survivor ownership must remain recoverable");
    // A second Live registration deliberately supersedes the first. Use
    // an independent ephemeral record to test cleanup isolation.
    let dead = rec(u32::MAX, 58201, LaneRole::Ephemeral);
    record_in(dir.path(), &dead).expect("dead record");
    let dead_path = record_path(dir.path(), dead.pid);
    assert!(!continuum_cli_lifecycle::process::is_alive(dead.pid));
    assert_eq!(finish_reap(&dead_path, &dead), SweepOutcome::ReapedEphemeral { pid: dead.pid, port: dead.port });
    assert!(!dead_path.exists());
    assert!(path.exists(), "cleaning a dead lane must preserve the survivor");
}

// A true survivor remains owned; later exit and PID reuse cannot erase a replacement.
#[tokio::test]
async fn shutdown_confirmation_waits_for_exit_and_preserves_replacements() {
    const CHILD: &str = "CONTINUUM_LANE_EXIT_FIXTURE";
    if std::env::var_os(CHILD).is_some() {
        use std::io::Read;
        std::io::stdin()
            .read_to_end(&mut Vec::new())
            .expect("fixture input");
        return;
    }
    use std::process::Stdio;
    use std::time::Duration;
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let mut command =
        tokio::process::Command::from(continuum_cli_lifecycle::process::quiet_command(
            std::env::current_exe().expect("fixture executable"),
        ));
    let mut child = command
        .args(["--exact", "inference::lane_registry::tests::shutdown_confirmation_waits_for_exit_and_preserves_replacements", "--nocapture"])
        .env(CHILD, "1").stdin(Stdio::piped()).stdout(Stdio::null()).kill_on_drop(true)
        .spawn().expect("fixture child");
    let pid = child.id().expect("child pid");
    let mut table = System::new();
    table.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[Pid::from_u32(pid)]),
        true,
        ProcessRefreshKind::nothing(),
    );
    let started_s = table
        .process(Pid::from_u32(pid))
        .expect("live child")
        .start_time();
    let lane = LaneRecord {
        started_s,
        ..rec(pid, 58202, LaneRole::Ephemeral)
    };
    let identity = lane_identity(&lane);
    let dir = tempfile::tempdir().expect("registry");
    record_in(dir.path(), &lane).expect("record");
    let path = record_path(dir.path(), pid);
    let pending = || vec![(path.clone(), lane.clone())];
    let unresolved = || {
        vec![SweepOutcome::ExitUnconfirmed {
            pid,
            port: lane.port,
        }]
    };
    assert_eq!(
        confirm_pending(unresolved(), pending(), continuum_process::KILL_POLL)
            .await
            .expect("confirmation"),
        unresolved()
    );
    assert!(path.exists(), "true survivor ownership retained");
    let denied = finish_signal(&path, &lane, Err("native access denied (5)".into()));
    assert_eq!(denied, SweepOutcome::TerminationRefused {
        pid, started_s, reason: "native access denied (5)".into(),
    });
    assert!(path.exists(), "denied termination retains exact ownership evidence");

    let recycled = crate::system_resources::process_tree::ProcessIdentity {
        start_time: started_s + 1,
        ..identity
    };
    assert!(!continuum_process::signal_if_same(recycled).expect("replaced identity is not signalled"));
    assert!(child.try_wait().expect("child status").is_none());
    assert!(
        continuum_process::confirm_gone(vec![recycled], continuum_process::KILL_POLL)
            .await
            .is_empty()
    );
    let input = child.stdin.take().expect("owned input");
    let release = async {
        tokio::time::sleep(Duration::from_millis(250)).await;
        drop(input);
        child.wait().await.expect("fixture exit")
    };
    let (outcomes, status) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(
            confirm_pending(unresolved(), pending(), Duration::from_secs(5)),
            release
        )
    })
    .await
    .expect("bounded child completion");
    assert_eq!(
        outcomes.expect("confirmation"),
        vec![SweepOutcome::ReapedEphemeral {
            pid,
            port: lane.port
        }]
    );
    assert!(status.success());
    assert!(!path.exists());
    let replacement = LaneRecord {
        started_s: started_s + 1,
        ..lane.clone()
    };
    record_in(dir.path(), &replacement).expect("replacement");
    assert_eq!(
        confirm_pending(unresolved(), pending(), continuum_process::KILL_POLL)
            .await
            .expect("confirmation"),
        unresolved()
    );
    assert_eq!(
        records_checked_in(dir.path()).expect("retained"),
        vec![replacement]
    );
    // Exercise the actual native signal adapter on an owned disposable child;
    // the earlier denied result is an injected policy case, not an OS denial.
    let mut signalled = command.spawn().expect("second fixture child");
    let signal_pid = signalled.id().expect("child pid");
    table.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[Pid::from_u32(signal_pid)]), true,
        ProcessRefreshKind::nothing(),
    );
    let signal_identity = crate::system_resources::process_tree::ProcessIdentity {
        pid: signal_pid,
        start_time: table.process(Pid::from_u32(signal_pid)).expect("live child").start_time(),
    };
    assert!(continuum_process::signal_if_same(signal_identity).expect("native termination accepted"));
    tokio::time::timeout(Duration::from_secs(5), signalled.wait())
        .await.expect("bounded signalled exit").expect("reaped child");

}

// what this catches: record_in → the file exists and round-trips through JSON;
// remove_in deletes it and is idempotent. The accounting is useless if a
// recorded lane can't be read back or cleaned up.
#[test]
fn record_round_trips_and_remove_is_idempotent() {
    let dir = temp_dir("roundtrip");
    let _ = std::fs::remove_dir_all(&dir);
    let r = rec(4242, 58200, LaneRole::Ephemeral);
    record_in(&dir, &r).expect("record");

    let raw = std::fs::read_to_string(record_path(&dir, 4242)).expect("read");
    let back: LaneRecord = serde_json::from_str(&raw).expect("parse");
    assert_eq!(back, r, "record round-trips through JSON");

    remove_in(&dir, 4242);
    assert!(!record_path(&dir, 4242).exists(), "removed");
    remove_in(&dir, 4242); // idempotent — second remove must not panic
    let _ = std::fs::remove_dir_all(&dir);
}

// what this catches: a LIVE lane is singular — recording a new live lane
// SUPERSEDES a prior live record (the reclaimed predecessor), leaving exactly
// one live record. Without this, a stale live record naming a dead-then-reused
// pid would linger forever (sweep never reaps live records). Ephemeral records
// present at the same time are untouched.
#[test]
fn recording_a_live_lane_supersedes_the_prior_live_record() {
    let dir = temp_dir("live-exclusive");
    let _ = std::fs::remove_dir_all(&dir);
    // Prior boot's live lane + an unrelated ephemeral record.
    record_in(&dir, &rec(24784, 58057, LaneRole::Live)).expect("old live");
    record_in(&dir, &rec(30000, 58201, LaneRole::Ephemeral)).expect("ephemeral");
    // New boot's live lane records itself.
    record_in(&dir, &rec(24975, 58057, LaneRole::Live)).expect("new live");

    assert!(
        !record_path(&dir, 24784).exists(),
        "prior live record superseded"
    );
    assert!(record_path(&dir, 24975).exists(), "new live record present");
    assert!(
        record_path(&dir, 30000).exists(),
        "ephemeral record untouched"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// what this catches: THE safety invariant — an ALIVE ephemeral record whose pid
// is NOT a llama-server (here our own test-runner pid) is NEVER killed; sweep
// returns RemovedReused, drops the stale file, and our process survives. A
// regression to "kill whatever the registry names" would SIGKILL an unrelated
// reused-pid process — the blind-kill this guards against.
#[test]
fn sweep_never_kills_a_non_llama_ephemeral() {
    let dir = temp_dir("reused");
    let _ = std::fs::remove_dir_all(&dir);
    let me = std::process::id();
    record_in(&dir, &rec(me, 58200, LaneRole::Ephemeral)).expect("record");

    let outcomes = sweep_in(&dir, SweepMode::Boot);
    assert_eq!(outcomes, vec![SweepOutcome::RemovedReused { pid: me }]);
    assert!(
        continuum_cli_lifecycle::process::is_alive(me),
        "sweep must never kill a non-llama pid"
    );
    assert!(!record_path(&dir, me).exists(), "stale record removed");
    let _ = std::fs::remove_dir_all(&dir);
}

// what this catches: a live LIVE-role record is LEFT for lane_pidfile — sweep
// must not touch the persona lane's process OR its record (the two mechanisms
// must compose, not double-reap the living persona's own server).
#[test]
fn sweep_leaves_the_live_lane_alone() {
    let dir = temp_dir("live");
    let _ = std::fs::remove_dir_all(&dir);
    let me = std::process::id();
    record_in(&dir, &rec(me, 58057, LaneRole::Live)).expect("record");

    let outcomes = sweep_in(&dir, SweepMode::Boot);
    assert_eq!(outcomes, vec![SweepOutcome::LeftLive { pid: me }]);
    assert!(
        record_path(&dir, me).exists(),
        "live record left in place for lane_pidfile"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// what this catches: a record naming a definitely-dead pid is garbage-collected
// (RemovedDead), never a kill attempt on a recycled number — regardless of role.
#[test]
fn sweep_removes_dead_records() {
    let dir = temp_dir("dead");
    let _ = std::fs::remove_dir_all(&dir);
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("spawn true");
    let dead = child.id();
    child.wait().expect("reap");
    record_in(&dir, &rec(dead, 58200, LaneRole::Ephemeral)).expect("record");

    let outcomes = sweep_in(&dir, SweepMode::Boot);
    // (Tiny PID-reuse window is acceptable in a unit test; assert the invariant
    // "a reaped pid is GC'd or safely treated as reused, never a kill+reap of
    // the living".)
    assert!(
        matches!(outcomes.as_slice(), [SweepOutcome::RemovedDead { pid }] if *pid == dead)
            || matches!(outcomes.as_slice(), [SweepOutcome::RemovedReused { .. }]),
        "dead pid must be RemovedDead (or a safe RemovedReused), got {outcomes:?}"
    );
    assert!(!record_path(&dir, dead).exists(), "stale record removed");
    let _ = std::fs::remove_dir_all(&dir);
}

// what this catches: the boot/shutdown difference, on the ONE record where they
// disagree. A LIVE record is LEFT at boot (it may be adoptable) and REAPED at
// shutdown (nothing is adoptable by a core that is exiting). Regression for the
// 2026-08-17 M5 incident: `stop` never swept lanes at all, so shutting Continuum
// down left llama-servers holding VRAM and `reboot` (= stop + start) could not
// clear them. Uses OUR OWN pid as the recorded lane: it is definitely alive and
// definitely NOT a llama-server, so the never-blind-kill guard must classify it
// RemovedReused under Shutdown — proving the shutdown path still refuses to
// signal a process it cannot positively identify, while boot still returns
// LeftLive without even looking. Asserting we are still alive afterwards is the
// real safety claim.
#[test]
fn shutdown_reaps_the_live_lane_that_boot_leaves_alone() {
    let me = std::process::id();

    let boot_dir = temp_dir("mode-boot");
    let _ = std::fs::remove_dir_all(&boot_dir);
    record_in(&boot_dir, &rec(me, 58057, LaneRole::Live)).expect("record");
    assert_eq!(
        sweep_in(&boot_dir, SweepMode::Boot),
        vec![SweepOutcome::LeftLive { pid: me }],
        "boot defers the live lane to lane_pidfile's adopt-or-reap"
    );
    assert!(
        record_path(&boot_dir, me).exists(),
        "boot must KEEP the live record so the next pass can still see it"
    );

    let stop_dir = temp_dir("mode-shutdown");
    let _ = std::fs::remove_dir_all(&stop_dir);
    record_in(&stop_dir, &rec(me, 58057, LaneRole::Live)).expect("record");
    let outcomes = sweep_in(&stop_dir, SweepMode::Shutdown);
    assert!(
        !matches!(outcomes.as_slice(), [SweepOutcome::LeftLive { .. }]),
        "shutdown must NEVER leave a live lane running, got {outcomes:?}"
    );
    assert_eq!(
        outcomes,
        vec![SweepOutcome::RemovedReused { pid: me }],
        "our own pid is alive but is not a llama-server — the never-blind-kill \
         guard must hold on the shutdown path too"
    );
    assert!(
        continuum_cli_lifecycle::process::is_alive(me),
        "the shutdown sweep must never signal a non-llama process"
    );
    assert!(!record_path(&stop_dir, me).exists(), "record cleared");

    let _ = std::fs::remove_dir_all(&boot_dir);
    let _ = std::fs::remove_dir_all(&stop_dir);
}

// what this catches: an unparseable `.lane` file is removed as garbage, never
// acted on — a corrupt record can't wedge the sweep or trigger a bogus kill.
#[test]
fn sweep_removes_unparseable_files() {
    let dir = temp_dir("garbage");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let junk = dir.join("99999.lane");
    std::fs::write(&junk, "not json at all").expect("write junk");

    let outcomes = sweep_in(&dir, SweepMode::Boot);
    assert_eq!(
        outcomes,
        vec![SweepOutcome::RemovedUnparseable { path: junk.clone() }]
    );
    assert!(!junk.exists(), "garbage file removed");
    let _ = std::fs::remove_dir_all(&dir);
}

// what this catches: a missing registry dir is a clean no-op (normal first
// boot), not an error or a panic.
#[test]
fn sweep_missing_dir_is_noop() {
    let dir = temp_dir("absent");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(sweep_in(&dir, SweepMode::Boot).is_empty());
}

/// The census an admission decision reads. Nested per the one-mod rule.
mod census_for_admission {
    use super::*;

    // what this catches: `records_in` must return EVERY recorded lane, both roles,
    // and must not be confused by junk in the directory. The admission gate decides
    // by this list, so a census that silently drops a live sibling admits a second
    // engine onto a device that is already full — the 58091/58092/58093 shape.
    #[test]
    fn the_census_returns_every_recorded_lane_of_both_roles() {
        let dir = std::env::temp_dir().join(format!("lanes-census-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp registry");

        for (pid, port, role) in [
            (4001u32, 58057u16, LaneRole::Live),
            (4002, 58091, LaneRole::Ephemeral),
            (4003, 58092, LaneRole::Ephemeral),
        ] {
            record_in(
                &dir,
                &LaneRecord {
                    pid,
                    port,
                    role,
                    model: "some/model".into(),
                    context_window: 8192,
                    lanes: 1,
                    page_dir: None,
                    engine_bin: None,
                    weights: None,
                    started_s: 0,
                    placement: None,
                },
            )
            .expect("record");
        }
        // Junk beside the records must be ignored, never counted or panicked on.
        std::fs::write(dir.join("notes.txt"), "not a lane").expect("junk");
        std::fs::write(dir.join("9999.lane"), "{ not json").expect("garbage record");

        let mut got = records_in(&dir);
        got.sort_by_key(|r| r.pid);
        assert_eq!(
            got.iter().map(|r| r.pid).collect::<Vec<_>>(),
            vec![4001, 4002, 4003],
            "every well-formed record is in the census, and nothing else is"
        );
        assert_eq!(
            got.iter().filter(|r| r.role == LaneRole::Ephemeral).count(),
            2,
            "the ephemeral siblings are the ones an admission decision weighs"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: the census must be a READ. Reconciling records against
    // liveness belongs to the caller, which owns the never-blind-kill identity
    // check — if this deleted records it would be signalling on pids it never
    // verified, the exact rule `lane_pidfile` and `sweep_in` both obey.
    #[test]
    fn the_census_never_deletes_what_it_reads() {
        let dir = std::env::temp_dir().join(format!("lanes-nondestr-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp registry");
        record_in(
            &dir,
            &LaneRecord {
                pid: 4242,
                port: 58091,
                role: LaneRole::Ephemeral,
                model: "some/model".into(),
                context_window: 8192,
                lanes: 1,
                page_dir: None,
                engine_bin: None,
                weights: None,
                started_s: 0,
                placement: None,
            },
        )
        .expect("record");

        assert_eq!(records_in(&dir).len(), 1);
        assert_eq!(records_in(&dir).len(), 1, "reading twice changes nothing");
        assert!(
            dir.join("4242.lane").exists(),
            "the record survives being read — reaping is the caller's decision"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
