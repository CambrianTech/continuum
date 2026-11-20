//! Live-watch regressions, compiled only for Unix tests.
#![cfg(test)]
#![cfg(unix)]

use super::*;
use super::super::{record_in, tests::{temp_dir, rec}};

// what this catches: the live-lane watch page IO's retained custody waits on (Codex on #6099). A waiter that
// subscribed while no lane was verified must WAKE when an adopted engine verifies; a recycled pid running another
// llama-server (same image, another start time) must never be published; a removal must clear the watch in the
// same locked step. A REAL process named llama-server stands in for the engine.
#[cfg(unix)]
#[tokio::test]
async fn the_live_lane_watch_publishes_only_the_verified_incarnation() {
    let dir = temp_dir("live-watch");
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("test: a clean dir");
    }
    std::fs::create_dir_all(&dir).expect("test: dir");
    let pidfile = dir.join("lane.pid");
    let program = dir.join("llama-server");
    // /bin/sh, not /bin/sleep: a multi-call coreutils (one binary dispatching on its name) exits at once when its
    // copy is named llama-server, and the loop keeps the shell itself alive (no exec into a child image)
    std::fs::copy("/bin/sh", &program).expect("test: a process named llama-server");
    let mut child = std::process::Command::new(&program)
        .args(["-c", "while :; do sleep 1; done"])
        .spawn()
        .expect("test: spawn");
    let pid = child.id();
    let started_s = crate::inference::engine_residency::process_start_s(pid).expect("test: its start time");
    let live = LiveLane::new();
    let mut waiter = live.subscribe();
    std::fs::write(&pidfile, pid.to_string()).expect("test: pidfile");

    // the same image under the same pid but another start time: a recycled pid, never published
    record_in(&dir, &LaneRecord { started_s: started_s + 1, ..rec(pid, 58_081, LaneRole::Live) }).expect("test: record");
    sync_live_locked(&dir, &pidfile, &live);
    assert!(!waiter.has_changed().expect("test: watch open"), "a start-time mismatch publishes nothing");
    assert_eq!(*waiter.borrow(), None);

    // the adopted engine verifies: the waiter that subscribed at None wakes with its exact incarnation
    record_in(&dir, &LaneRecord { started_s, ..rec(pid, 58_081, LaneRole::Live) }).expect("test: record");
    sync_live_locked(&dir, &pidfile, &live);
    assert!(
        waiter.has_changed().expect("test: watch open"),
        "the waiter wakes (pidfile pid {:?}, is_llama_server {}, start now {:?} vs recorded {started_s})",
        lane_pidfile::read_at(&pidfile),
        lane_process::is_llama_server(pid),
        crate::inference::engine_residency::process_start_s(pid),
    );
    assert_eq!(*waiter.borrow_and_update(), Some(crate::inference::engine_residency::EngineIncarnation { pid, started_s, port: 58_081 }));

    let engine = crate::inference::engine_residency::EngineIncarnation { pid, started_s, port: 58_081 };
    assert!(live.trusts(Some(engine)), "a published engine whose exit is awaited is trusted without a probe");

    // the record removed: cleared in the same step
    std::fs::remove_file(record_path(&dir, pid)).expect("test: remove the record");
    sync_live_locked(&dir, &pidfile, &live);
    assert!(waiter.has_changed().expect("test: watch open"), "the removal wakes the waiter");
    assert_eq!(*waiter.borrow_and_update(), None, "no live lane once its record is gone");

    // published again; then the engine EXITS with its pidfile and record unchanged: the exit revokes it
    record_in(&dir, &LaneRecord { started_s, ..rec(pid, 58_081, LaneRole::Live) }).expect("test: record");
    sync_live_locked(&dir, &pidfile, &live);
    assert_eq!(*waiter.borrow_and_update(), Some(engine));
    child.kill().expect("test: end the fixture process");
    child.wait().expect("test: reap it");
    tokio::time::timeout(std::time::Duration::from_secs(10), waiter.wait_for(Option::is_none)) // test watchdog only
        .await
        .expect("the engine's exit revokes it, with its files unchanged")
        .expect("test: watch open");
    assert!(!live.trusts(Some(engine)), "a dead engine is never trusted from its unchanged files");
    std::fs::remove_dir_all(&dir).expect("test: clean up");
}

// what this catches (Codex on #6099 c41d1e29f): a cancelled exit waiter leaving its engine trusted forever. The
// waiter's guard is built before the spawn, so a runtime dropped before the task's first poll still clears it,
// and only its own watch generation.
#[cfg(unix)]
#[test]
fn an_exit_watch_whose_waiter_never_runs_stops_being_trusted() {
    let dir = temp_dir("live-watch-abort");
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("test: a clean dir");
    }
    std::fs::create_dir_all(&dir).expect("test: dir");
    let pidfile = dir.join("lane.pid");
    let program = dir.join("llama-server");
    std::fs::copy("/bin/sh", &program).expect("test: a process named llama-server");
    let mut child = std::process::Command::new(&program)
        .args(["-c", "while :; do sleep 1; done"])
        .spawn()
        .expect("test: spawn");
    let pid = child.id();
    let started_s = crate::inference::engine_residency::process_start_s(pid).expect("test: its start time");
    std::fs::write(&pidfile, pid.to_string()).expect("test: pidfile");
    record_in(&dir, &LaneRecord { started_s, ..rec(pid, 58_082, LaneRole::Live) }).expect("test: record");
    let engine = crate::inference::engine_residency::EngineIncarnation { pid, started_s, port: 58_082 };

    let live = LiveLane::new();
    // a runtime nobody drives: the waiter is spawned but never polled
    let runtime = tokio::runtime::Builder::new_current_thread().build().expect("test: runtime");
    {
        let _entered = runtime.enter();
        let _writes = writes();
        sync_live_locked(&dir, &pidfile, &live);
    }
    assert_eq!(*live.subscribe().borrow(), Some(engine), "armed, then published");
    assert!(live.trusts(Some(engine)), "trusted while its waiter exists");
    drop(runtime); // the waiter is dropped before its first poll
    assert!(!live.trusts(Some(engine)), "its guard cleared the watch: no longer trusted without a probe");

    child.kill().expect("test: end the fixture process");
    child.wait().expect("test: reap it");
    std::fs::remove_dir_all(&dir).expect("test: clean up");
}

// what this catches (Codex on #6099 b4c75bc57): a persistent exit-watch failure spinning. A waiter that ends
// Unobservable must leave the engine published but untrusted and NEVER re-arm it: repeated syncs make one arm
// attempt in all, not a thread per sync.
#[cfg(unix)]
#[tokio::test]
async fn an_unobservable_exit_watch_is_armed_once_and_never_trusted() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct FailingWatcher(std::sync::Arc<AtomicUsize>);
    impl EngineExitWatcher for FailingWatcher {
        fn arm(
            &self,
            pid: u32,
            _is_expected: &dyn Fn() -> bool,
        ) -> Result<std::pin::Pin<Box<dyn std::future::Future<Output = continuum_host::process_exit::ExitObserved> + Send>>, continuum_host::process_exit::ArmRefusal> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(Box::pin(std::future::ready(continuum_host::process_exit::ExitObserved::Unobservable(
                continuum_host::process_exit::Unobservable::WatcherLost { pid },
            ))))
        }
    }

    let dir = temp_dir("live-watch-unobservable");
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("test: a clean dir");
    }
    std::fs::create_dir_all(&dir).expect("test: dir");
    let pidfile = dir.join("lane.pid");
    let program = dir.join("llama-server");
    std::fs::copy("/bin/sh", &program).expect("test: a process named llama-server");
    let mut child = std::process::Command::new(&program)
        .args(["-c", "while :; do sleep 1; done"])
        .spawn()
        .expect("test: spawn");
    let pid = child.id();
    let started_s = crate::inference::engine_residency::process_start_s(pid).expect("test: its start time");
    std::fs::write(&pidfile, pid.to_string()).expect("test: pidfile");
    record_in(&dir, &LaneRecord { started_s, ..rec(pid, 58_083, LaneRole::Live) }).expect("test: record");
    let engine = crate::inference::engine_residency::EngineIncarnation { pid, started_s, port: 58_083 };

    let arms = std::sync::Arc::new(AtomicUsize::new(0));
    let live = LiveLane::with_watcher(Box::new(FailingWatcher(arms.clone())));
    let sync = |live: &std::sync::Arc<LiveLane>| {
        let _writes = writes();
        sync_live_locked(&dir, &pidfile, live);
    };
    sync(&live);
    tokio::task::yield_now().await; // the waiter runs: its wait ends Unobservable
    for _ in 0..3 {
        sync(&live);
        tokio::task::yield_now().await;
    }
    assert_eq!(arms.load(Ordering::SeqCst), 1, "one arm attempt in all: an unobservable engine is never re-armed");
    assert_eq!(*live.subscribe().borrow(), Some(engine), "still published: it is verified live on every sync");
    assert!(!live.trusts(Some(engine)), "never trusted without a probe");

    child.kill().expect("test: end the fixture process");
    child.wait().expect("test: reap it");
    std::fs::remove_dir_all(&dir).expect("test: clean up");
}
