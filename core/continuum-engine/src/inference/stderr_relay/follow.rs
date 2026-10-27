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
use super::{process, Frame, Subscription};

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
            read(engine, subscription, watch.as_mut()).await;
        }
    }
}

/// Read one connection until it ends, its heartbeat expires, or it names another engine.
async fn read(engine: EngineIncarnation, mut subscription: Subscription, watch: &mut dyn LineWatch) {
    let mut clock: Option<Clock> = None;
    let mut deadline_ms = None;
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
            Frame::Heartbeat(beat) if beat.incarnation == engine => {
                deadline_ms = Some(beat.expires_at_ms);
                clock.get_or_insert_with(|| Clock::start(engine.pid)).0.store(beat.last_line_ms, Ordering::Relaxed);
            }
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

    // what this catches (Codex on 1d7d6d4c): a core restart that leaves an engine unwatched, a
    // second follower of one incarnation, a restarted core shown lines from before it, or a
    // silence clock that outlives the follower that kept it.
    #[tokio::test]
    async fn a_restarted_follower_picks_up_the_relay_and_its_silence_lives_only_with_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (socket, log) = (dir.path().join("engine.sock"), dir.path().join("engine.log"));
        // This test process stands in for the engine, so the follower sees it alive.
        let engine = EngineIncarnation::of(std::process::id(), 65_001).expect("this process's start time");
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
    }
}
