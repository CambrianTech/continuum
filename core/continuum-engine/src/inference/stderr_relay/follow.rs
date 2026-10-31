//! The core's side of a relay: whichever core is running follows each engine's relay, feeds
//! every line to the engine's watchers and keeps the engine's silence clock from its heartbeats,
//! for an engine it spawned and one it adopted alike (card 1d7d6d4c).
//!
//! The clock exists only while a current heartbeat names this incarnation: before the first one,
//! after one expires, or with the relay gone, the engine's silence reads as unknown, never as
//! zero and never as a predecessor's.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use super::super::child_log::{self, LineWatch};
use super::super::engine_residency::{EngineIncarnation, Liveness};
use super::{process, Frame, Heartbeat, Subscription};

/// The incarnations a task of this process follows, so a second request for one is a no-op.
static FOLLOWING: LazyLock<parking_lot::Mutex<HashSet<EngineIncarnation>>> = LazyLock::new(Default::default);

/// Follow `engine`'s relay until the engine is gone: every line through `watch`, every
/// heartbeat into its silence clock. Reconnects while the engine lives, so a relay reached late
/// or a connection lost is picked up again. Returns false when this process already follows it,
/// or the host has no relay.
pub fn follow(engine: EngineIncarnation, watch: Box<dyn LineWatch>) -> bool {
    let Some(socket) = process::endpoint(engine.port) else {
        tracing::warn!(target: "continuum_core::inference::llama_server", probe_class = "serving.llama.stderr_unlogged",
            port = engine.port, "no home directory for the engine's relay: its lines are not followed");
        return false;
    };
    continuum_host::output_handoff::SUPPORTED && follow_at(engine, socket, watch).is_some()
}

/// [`follow`] at the relay endpoint `socket`: the task, or `None` when it is already followed.
fn follow_at(engine: EngineIncarnation, socket: PathBuf, watch: Box<dyn LineWatch>) -> Option<tokio::task::JoinHandle<()>> {
    let following = Following::claim(engine)?;
    Some(tokio::spawn(async move {
        let _following = following;
        follow_until_gone(engine, &socket, watch).await;
    }))
}

/// A relay this core just spawned, proven ready: a connection whose opening heartbeat names
/// `engine`. The spawning core is the relay's first subscriber, so this connection is the one
/// the engine's held startup lines go to; [`follow_ready`] keeps reading it.
pub struct Ready {
    subscription: Subscription,
    opening: Heartbeat,
}

/// Wait, until `within` passes, for the relay `relay` to serve `engine` at `socket`. Refused
/// when the relay exits first (a failed bind, a path it cannot own), when its endpoint answers
/// for another engine, or when the time passes: the caller must not serve that engine.
pub async fn ready(engine: EngineIncarnation, socket: &Path, relay: &mut tokio::process::Child, within: Duration) -> Result<Ready, String> {
    let deadline = tokio::time::Instant::now() + within;
    // derived-or-floor: a POLL cadence while one local process starts and binds; it bounds how late readiness is noticed, never a budget
    let mut poll = tokio::time::interval(Duration::from_millis(20));
    loop {
        poll.tick().await;
        if let Some(status) = relay.try_wait().map_err(|e| format!("the engine's log relay could not be polled: {e}"))? {
            return Err(format!("the engine's log relay exited before serving: {status}"));
        }
        // The connect and the opening heartbeat share the one deadline: an endpoint that never
        // accepts or never speaks is refused when it passes (Codex on #5536).
        let connected = tokio::time::timeout_at(deadline, Subscription::connect(socket)).await;
        if let Ok(Ok(mut subscription)) = connected {
            let opening = tokio::time::timeout_at(deadline, subscription.next()).await;
            return match opening {
                Ok(Ok(Some(Frame::Heartbeat(opening)))) if opening.incarnation == engine => Ok(Ready { subscription, opening }),
                Ok(Ok(Some(Frame::Heartbeat(other)))) => Err(format!("the endpoint answers for another engine ({:?}), not {engine:?}", other.incarnation)),
                Ok(Ok(Some(Frame::Line { .. }))) => Err("the relay sent a line before its opening heartbeat".to_string()),
                Ok(Ok(None)) | Ok(Err(_)) => Err("the relay hung up before its opening heartbeat".to_string()),
                Err(_) => Err(format!("the relay sent no opening heartbeat within {within:?}")),
            };
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("the engine's log relay did not serve within {within:?}"));
        }
    }
}

/// Follow `engine` from a [`Ready`] connection, then as [`follow`] does: the follower's task, or
/// `None` when this process already follows it. Ending the task is what a core exiting does to it.
pub fn follow_ready(engine: EngineIncarnation, socket: PathBuf, ready: Ready, watch: Box<dyn LineWatch>) -> Option<tokio::task::JoinHandle<()>> {
    let following = Following::claim(engine)?;
    Some(tokio::spawn(async move {
        let _following = following;
        let mut watch = watch;
        read(engine, ready.subscription, Some(ready.opening), watch.as_mut()).await;
        follow_until_gone(engine, &socket, watch).await;
    }))
}

/// This process's claim to follow one incarnation, given up however its task ends (an abort
/// included), so a later request can follow it again.
struct Following(EngineIncarnation);

impl Following {
    fn claim(engine: EngineIncarnation) -> Option<Self> {
        // The lock is released before a guard exists: a guard dropped under it would take it again.
        let claimed = FOLLOWING.lock().insert(engine);
        claimed.then(|| Self(engine))
    }
}

impl Drop for Following {
    fn drop(&mut self) {
        FOLLOWING.lock().remove(&self.0);
    }
}

async fn follow_until_gone(engine: EngineIncarnation, socket: &Path, mut watch: Box<dyn LineWatch>) {
    // The relay heartbeats every second; a reconnect on the same cadence costs one failed
    // connect per second for an engine whose relay is down.
    let mut retry = tokio::time::interval(Duration::from_secs(1));
    retry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        retry.tick().await;
        if engine.liveness() == Liveness::Dead {
            return;
        }
        if let Ok(subscription) = Subscription::connect(socket).await {
            read(engine, subscription, None, watch.as_mut()).await;
        }
    }
}

/// Read one connection until it ends, its heartbeat expires, or it names another engine.
/// `opening` is a heartbeat already read from it.
async fn read(engine: EngineIncarnation, mut subscription: Subscription, opening: Option<Heartbeat>, watch: &mut dyn LineWatch) {
    let mut clock: Option<Clock> = None;
    let mut deadline_ms = None;
    let mut keep = |beat: &Heartbeat, deadline_ms: &mut Option<u64>| {
        *deadline_ms = Some(beat.expires_at_ms);
        clock.get_or_insert_with(|| Clock::start(engine.pid)).0.store(beat.last_line_ms, Ordering::Relaxed);
    };
    if let Some(beat) = opening.filter(|beat| beat.incarnation == engine) {
        keep(&beat, &mut deadline_ms);
    }
    loop {
        let now = continuum_base::time::EpochMs::now().millis();
        // Before the first heartbeat, wait no longer than one relay heartbeat bound would.
        let wait = deadline_ms.map_or(Duration::from_secs(3), |deadline: u64| Duration::from_millis(deadline.saturating_sub(now)));
        let frame = match tokio::time::timeout(wait, subscription.next()).await {
            Ok(Ok(Some(frame))) => frame,
            _ => break, // ended, unreadable, or silent past its heartbeat's expiry
        };
        match frame {
            Frame::Line { text, .. } => watch.observe(&text),
            Frame::Heartbeat(beat) if beat.incarnation == engine => keep(&beat, &mut deadline_ms),
            Frame::Heartbeat(_) => break, // another engine's relay on this port: never this one's signals
        }
    }
}

/// The engine's silence clock while a connection's heartbeats keep it, retired however the read
/// ends (a cancelled one included): an unread engine's silence is unknown, never stale.
struct Clock(Arc<AtomicU64>, u32);

impl Clock {
    fn start(pid: u32) -> Self {
        Self(child_log::start_clock(Some(pid)), pid)
    }
}

impl Drop for Clock {
    fn drop(&mut self) {
        child_log::retire_clock(Some(self.1), &self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

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

    /// A stand-in engine: a live process of its own, so each test's incarnation and silence
    /// clock are its own (keyed by pid, a shared pid would let one test's follower answer
    /// another's check).
    #[cfg(unix)]
    async fn stand_in_engine(port: u16) -> (tokio::process::Child, EngineIncarnation) {
        let child = live_child();
        let pid = child.id().expect("a running child has a pid");
        for _ in 0..100 {
            if let Some(engine) = EngineIncarnation::of(pid, port) {
                return (child, engine);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the stand-in engine's start time is readable");
    }

    // what this catches (Codex on 1d7d6d4c): a core restart that leaves an engine unwatched, a
    // second follower of one incarnation, a restarted core shown lines from before it, or a
    // silence clock that outlives the follower that kept it.
    // The relay runs only where the host hands it the engine's stderr (unix); this spawns a process.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_restarted_follower_picks_up_the_relay_and_its_silence_lives_only_with_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (socket, log) = (dir.path().join("engine.sock"), dir.path().join("engine.log"));
        let (mut stand_in, engine) = stand_in_engine(65_001).await;
        let (mut stderr, drained) = tokio::io::duplex(64 * 1024);
        let config = super::super::Config { heartbeat: Duration::from_millis(50), ..super::super::Config::default() };
        let relay = tokio::spawn({
            let (log, socket) = (log.clone(), socket.clone());
            async move { super::super::run(drained, engine, &log, &socket, config).await }
        });
        let first_seen = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let first = follow_at(engine, socket.clone(), Box::new(Record(Arc::clone(&first_seen)))).expect("the first claim");
        assert!(follow_at(engine, socket.clone(), Box::new(())).is_none(), "one follower per incarnation");
        stderr.write_all(b"banner\n").await.expect("write");
        until("the first core sees the banner", || first_seen.lock().contains(&"banner".to_string())).await;
        until("its heartbeats keep the silence clock", || child_log::silent_ms(engine.pid).is_some()).await;

        first.abort();
        assert!(first.await.is_err_and(|e| e.is_cancelled()));
        assert_eq!(child_log::silent_ms(engine.pid), None, "the clock goes with its follower");
        stderr.write_all(b"while no core ran\n").await.expect("write");
        until("the relay drains it with nobody connected", || std::fs::read_to_string(&log).is_ok_and(|l| l.contains("while no core ran"))).await;

        let second_seen = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let second = follow_at(engine, socket.clone(), Box::new(Record(Arc::clone(&second_seen)))).expect("the claim was given up");
        until("the restarted core's silence clock", || child_log::silent_ms(engine.pid).is_some()).await;
        stderr.write_all(b"after restart\n").await.expect("write");
        until("the restarted core sees new lines", || second_seen.lock().contains(&"after restart".to_string())).await;
        assert_eq!(*second_seen.lock(), vec!["after restart".to_string()], "nothing from before it");

        drop(stderr);
        relay.await.expect("the relay task").expect("the relay ends cleanly");
        until("the clock goes with the relay", || child_log::silent_ms(engine.pid).is_none()).await;
        second.abort();
        let _ = stand_in.kill().await;
    }

    // The relay runs only where the host hands it the engine's stderr (unix); these spawn processes.
    #[cfg(unix)]
    /// A stand-in relay process for the readiness checks: alive until killed.
    fn live_child() -> tokio::process::Child {
        tokio::process::Command::new("sleep").arg("30").spawn().expect("spawn a stand-in relay")
    }

    // what this catches (Codex on #5536): a relay that spawned but never served (a failed bind,
    // a path it cannot own) taken as ready, so the engine serves with nobody reading it.
    // The relay runs only where the host hands it the engine's stderr (unix); these spawn processes.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_relay_that_exits_before_serving_is_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let engine = EngineIncarnation { pid: 4242, started_s: 1_793_000_000, port: 65_002 };
        let mut relay = tokio::process::Command::new("sh").args(["-c", "exit 3"]).spawn().expect("spawn");
        let refused = ready(engine, &dir.path().join("engine.sock"), &mut relay, Duration::from_secs(10)).await;
        assert!(refused.as_ref().is_err_and(|e| e.contains("exited before serving")), "{:?}", refused.err());
    }

    // what this catches (Codex on #5536): readiness taken from an endpoint that serves another
    // engine (a predecessor's relay still on the path), so this engine's lines go nowhere.
    // The relay runs only where the host hands it the engine's stderr (unix); these spawn processes.
    #[cfg(unix)]
    #[tokio::test]
    async fn an_endpoint_answering_for_another_engine_is_refused_and_a_matching_one_is_followed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (socket, log) = (dir.path().join("engine.sock"), dir.path().join("engine.log"));
        let (mut stand_in, engine) = stand_in_engine(65_003).await;
        let predecessor = EngineIncarnation { started_s: engine.started_s - 1, ..engine };
        let (mut stderr, drained) = tokio::io::duplex(64 * 1024);
        let config = super::super::Config { heartbeat: Duration::from_millis(50), ..super::super::Config::default() };
        let relay = tokio::spawn({
            let (log, socket) = (log.clone(), socket.clone());
            async move { super::super::run(drained, engine, &log, &socket, config).await }
        });
        stderr.write_all(b"banner\n").await.expect("write");
        let mut child = live_child();
        let refused = ready(predecessor, &socket, &mut child, Duration::from_secs(10)).await;
        assert!(refused.as_ref().is_err_and(|e| e.contains("another engine")), "{:?}", refused.err());

        // That refused probe was the first subscriber, so the banner went to it; a fresh relay
        // shows the matching path from the start.
        drop(stderr);
        relay.await.expect("the relay task").expect("the relay ends cleanly");
        let (mut stderr, drained) = tokio::io::duplex(64 * 1024);
        let relay = tokio::spawn({
            let (log, socket) = (log.clone(), socket.clone());
            async move { super::super::run(drained, engine, &log, &socket, config).await }
        });
        stderr.write_all(b"banner\n").await.expect("write");
        let proven = ready(engine, &socket, &mut child, Duration::from_secs(10)).await.expect("ready for this engine");
        let seen = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let follower = follow_ready(engine, socket.clone(), proven, Box::new(Record(Arc::clone(&seen)))).expect("the first follower");
        until("the spawning core's follower gets the held banner", || seen.lock().contains(&"banner".to_string())).await;
        let _ = child.kill().await;
        drop(stderr);
        relay.await.expect("the relay task").expect("the relay ends cleanly");
        follower.abort();
        let _ = stand_in.kill().await;
    }

    // what this catches (Codex on #5536): the readiness bound not enforced, because an endpoint
    // that takes the connection (into its backlog) and never speaks held the launch past it.
    // The relay runs only where the host hands it the engine's stderr (unix); these spawn processes.
    #[cfg(unix)]
    #[tokio::test]
    async fn an_endpoint_that_never_speaks_is_refused_within_the_bound() {
        let dir = tempfile::tempdir().expect("tempdir");
        let socket = dir.path().join("engine.sock");
        let _silent = std::os::unix::net::UnixListener::bind(&socket).expect("a listener that never accepts");
        let engine = EngineIncarnation { pid: 4242, started_s: 1_793_000_000, port: 65_004 };
        let mut child = live_child();
        let started = std::time::Instant::now();
        let refused = ready(engine, &socket, &mut child, Duration::from_millis(300)).await;
        assert!(refused.is_err(), "a silent endpoint is never ready");
        assert!(started.elapsed() < Duration::from_secs(5), "refused at the bound, not after it: {:?}", started.elapsed());
        let _ = child.kill().await;
    }
}
