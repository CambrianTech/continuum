//! An engine's log relay, the real binary, end to end (card 1d7d6d4c, Codex's merge gate on
//! #5536): `continuum-core-server --log-relay` spawned into a stand-in engine's process group,
//! proven ready for that incarnation, its startup banner delivered to the spawning core, then a
//! core exit and a fresh core following the same relay through the adoption path, and the relay
//! ending with its engine. The relay runs only where the host hands it the engine's stderr (unix).
#![cfg(unix)]

use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use continuum_core::inference::child_log::{self, LineWatch};
use continuum_core::inference::engine_residency::EngineIncarnation;
use continuum_core::inference::stderr_relay::{follow, process};
use continuum_host::output_handoff::{spawn_reader, Handoff};

struct Record(Arc<parking_lot::Mutex<Vec<String>>>);

impl LineWatch for Record {
    fn observe(&mut self, line: &str) {
        self.0.lock().push(line.to_string());
    }
}

async fn until(what: &str, check: impl Fn() -> bool) {
    for _ in 0..1000 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("{what} within 10 s");
}

// what this catches (Codex on #5536): a relay that works in-process but not as the shipped binary
// (its `--log-relay` dispatch, its argument contract, its home-derived endpoint), a relay outside
// its engine's group, a startup banner lost before the spawning core subscribed, an engine that
// goes unwatched once the core that spawned it exits, and a relay that outlives its engine.
#[tokio::test]
async fn the_real_relay_serves_its_engine_and_a_restarted_core_follows_it() {
    // The relay derives its archive and endpoint from HOME; this file is its own test binary and
    // holds this one test, so setting it here reaches the relay and nothing else.
    let home = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(home.path().join(".continuum").join("logs")).expect("logs dir");
    std::env::set_var("HOME", home.path());
    let port = 65_010;

    // A stand-in engine: a banner, then a numbered line every 100 ms, in its own process group as
    // launch spawns the real one.
    let mut engine_child = tokio::process::Command::new("sh")
        .args(["-c", "echo banner >&2; i=0; while true; do i=$((i+1)); echo line $i >&2; sleep 0.1; done"])
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .expect("spawn the stand-in engine");
    let pid = engine_child.id().expect("a running engine has a pid");
    let mut engine = None;
    for _ in 0..100 {
        engine = EngineIncarnation::of(pid, port);
        if engine.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let engine = engine.expect("the stand-in engine's start time is readable");

    let relay_binary = Path::new(env!("CARGO_BIN_EXE_continuum-core-server"));
    let stderr = engine_child.stderr.take().expect("the engine's stderr is piped");
    let Handoff::Reader(mut relay) = spawn_reader(relay_binary, process::to_args(engine), stderr, pid).expect("spawn the relay") else {
        panic!("unix hands the engine's stderr to the relay");
    };
    let relay_pid = relay.id().expect("a running relay has a pid");
    let pgid = std::process::Command::new("ps").args(["-o", "pgid=", "-p", &relay_pid.to_string()]).output().expect("ps");
    assert_eq!(String::from_utf8_lossy(&pgid.stdout).trim(), pid.to_string(), "the relay is in its engine's group");

    // The spawning core: readiness from this incarnation, and the banner held for it.
    let socket = process::endpoint(port).expect("the endpoint under HOME");
    let ready = follow::ready(engine, &socket, &mut relay, Duration::from_secs(10)).await.expect("the real relay serves this engine");
    let first_seen = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let first = follow::follow_ready(engine, socket.clone(), ready, Box::new(Record(Arc::clone(&first_seen)))).expect("the first follower");
    until("the spawning core gets the banner and lines", || {
        let seen = first_seen.lock();
        seen.first().is_some_and(|l| l == "banner") && seen.iter().any(|l| l.starts_with("line "))
    })
    .await;
    until("its heartbeats keep the engine's silence clock", || child_log::silent_ms(pid).is_some()).await;

    // The core exits: its follower ends, and with it the engine's clock in this process.
    first.abort();
    assert!(first.await.is_err_and(|e| e.is_cancelled()));
    assert_eq!(child_log::silent_ms(pid), None, "the clock goes with the core that kept it");

    // A restarted core adopts the engine and follows the same relay: new lines, never a replay.
    let second_seen = Arc::new(parking_lot::Mutex::new(Vec::new()));
    assert!(follow::follow(engine, Box::new(Record(Arc::clone(&second_seen)))), "the adoption path follows the relay");
    until("the restarted core gets the engine's new lines", || second_seen.lock().iter().any(|l| l.starts_with("line "))).await;
    until("and keeps its silence clock", || child_log::silent_ms(pid).is_some()).await;
    assert!(!second_seen.lock().iter().any(|l| l == "banner"), "a later core is never replayed the banner");
    let archive = std::fs::read_to_string(home.path().join(".continuum").join("logs").join(format!("llama-server-{port}.log"))).expect("the relay's archive");
    assert!(archive.starts_with("banner\n"), "the archive holds the engine's lines from its first");

    // The engine ends: every writer of its stderr closes, and the relay ends with it.
    engine_child.kill().await.expect("kill the stand-in engine");
    let status = tokio::time::timeout(Duration::from_secs(10), relay.wait()).await.expect("the relay ends with its engine").expect("the relay's exit");
    assert!(status.success(), "the relay ends cleanly at its engine's end: {status}");
}
