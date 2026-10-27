#![cfg(test)]
use super::*;
use std::path::PathBuf;
use tokio::io::{AsyncWriteExt, DuplexStream};

struct Running {
    stderr: Option<DuplexStream>,
    control: DuplexStream,
    socket: PathBuf,
    log: PathBuf,
    relay: tokio::task::JoinHandle<io::Result<RelayEnd>>,
    _dir: tempfile::TempDir,
}

fn quick() -> Config {
    Config { heartbeat: Duration::from_millis(50), ..Config::default() }
}

async fn start(config: Config, log_in_missing_dir: bool) -> Running {
    let dir = tempfile::tempdir().expect("tempdir");
    let socket = dir.path().join("engine.sock");
    let log = if log_in_missing_dir { dir.path().join("missing").join("engine.log") } else { dir.path().join("engine.log") };
    let (stderr, stderr_read) = tokio::io::duplex(64 * 1024);
    let (control, control_read) = tokio::io::duplex(4096);
    let relay = tokio::spawn({
        let (log, socket) = (log.clone(), socket.clone());
        async move { run(stderr_read, control_read, &log, &socket, config).await }
    });
    // Ready when a client reaches it through the host seam (a Windows pipe has no file). The
    // probe hangs up at once, and its subscriber ends on that.
    let mut ready = false;
    for _ in 0..200 {
        if continuum_host::local_stream::connect(&socket).await.is_ok() {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(ready, "the relay serves its endpoint");
    tokio::time::sleep(Duration::from_millis(100)).await;
    Running { stderr: Some(stderr), control, socket, log, relay, _dir: dir }
}

impl Running {
    async fn write(&mut self, line: &str) {
        let stderr = self.stderr.as_mut().expect("stderr still open");
        stderr.write_all(format!("{line}\n").as_bytes()).await.expect("write stderr");
    }
}

async fn next(sub: &mut Subscription) -> Frame {
    tokio::time::timeout(Duration::from_secs(10), sub.next())
        .await
        .expect("a frame arrives")
        .expect("the socket reads")
        .expect("the relay is still serving")
}

/// A subscription that has seen its opening heartbeat: the relay sends it after subscribing, so
/// every line written from here on reaches this subscriber (a line written before it may be
/// drained with nobody subscribed yet, and is counted as missed).
async fn subscribe(socket: &Path) -> Subscription {
    let mut sub = Subscription::connect(socket).await.expect("connect");
    match next(&mut sub).await {
        Frame::Heartbeat(_) => sub,
        other => panic!("a heartbeat comes first, got {other:?}"),
    }
}

/// The next line frame, within a total deadline: heartbeats alone never keep a test waiting.
async fn next_line(sub: &mut Subscription) -> (u64, String, bool) {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Frame::Line { seq, text, truncated } = next(sub).await {
                return (seq, text, truncated);
            }
        }
    })
    .await
    .expect("a line frame arrives within the deadline")
}

/// The first heartbeat satisfying `wanted`, within a total deadline.
async fn heartbeat_where(sub: &mut Subscription, wanted: impl Fn(&Heartbeat) -> bool) -> Heartbeat {
    let mut last = None;
    let found = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Frame::Heartbeat(beat) = next(sub).await {
                if wanted(&beat) {
                    return beat;
                }
                last = Some(beat);
            }
        }
    })
    .await;
    found.unwrap_or_else(|_| panic!("no heartbeat met the condition within the deadline; last: {last:?}"))
}

// what this catches: a relay that reads lines without serving them, serves a consumer a line
// before saying who and where it is, or outlives the writers of the engine's stderr.
#[tokio::test]
async fn lines_reach_a_subscriber_after_a_heartbeat_and_the_relay_ends_with_its_writers() {
    let mut relay = start(quick(), false).await;
    let mut sub = Subscription::connect(&relay.socket).await.expect("connect");
    match next(&mut sub).await {
        Frame::Heartbeat(beat) => {
            assert_eq!(beat.seq_last, 0);
            assert_eq!(beat.incarnation, None, "unbound until the spawning core binds it");
        }
        other => panic!("a heartbeat comes first, got {other:?}"),
    }
    relay.write("load banner").await;
    relay.write("slot 0 ready").await;
    assert_eq!(next_line(&mut sub).await, (1, "load banner".into(), false));
    assert_eq!(next_line(&mut sub).await, (2, "slot 0 ready".into(), false));
    drop(relay.stderr.take());
    let end = relay.relay.await.expect("the relay task").expect("the relay ends cleanly");
    assert_eq!(end.lines, 2);
    // A subscriber still connected sees the end, never heartbeats from a relay that has gone.
    let after = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match sub.next().await {
                Ok(Some(Frame::Heartbeat(_))) => continue,
                other => return other,
            }
        }
    })
    .await
    .expect("the connection ends with the relay");
    assert!(matches!(after, Ok(None) | Err(_)), "{after:?}");
    assert!(continuum_host::local_stream::connect(&relay.socket).await.is_err(), "the endpoint goes with the relay");
    assert_eq!(std::fs::read_to_string(&relay.log).expect("archive"), "load banner\nslot 0 ready\n");
}

// what this catches (Codex on 1d7d6d4c): the incarnation taken from any socket client, or
// rebound later. Only the control handle binds it, once; a second bind is refused and counted.
#[tokio::test]
async fn the_incarnation_is_bound_once_from_the_control_handle() {
    let mut relay = start(quick(), false).await;
    let mut sub = Subscription::connect(&relay.socket).await.expect("connect");
    let first = Incarnation { pid: 4242, started_s: 1_793_000_000 };
    let second = Incarnation { pid: 9999, started_s: 1_793_000_001 };
    for bind in [first, second] {
        let line = format!("{}\n", serde_json::to_string(&bind).expect("serialize"));
        relay.control.write_all(line.as_bytes()).await.expect("write control");
    }
    let beat = heartbeat_where(&mut sub, |b| b.binds_refused == 1).await;
    assert_eq!(beat.incarnation, Some(first), "the first bind holds");
}

// what this catches (Codex on 1d7d6d4c): lines drained with nobody connected hidden, or replayed
// to the next subscriber as if new. They are counted, and a subscriber starts at the next line.
#[tokio::test]
async fn lines_with_nobody_connected_are_counted_and_never_replayed() {
    let mut relay = start(quick(), false).await;
    for line in ["old fault a", "old fault b", "old fault c"] {
        relay.write(line).await;
    }
    for _ in 0..200 {
        if std::fs::read_to_string(&relay.log).map(|s| s.lines().count()).unwrap_or(0) == 3 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let mut sub = Subscription::connect(&relay.socket).await.expect("connect");
    match next(&mut sub).await {
        Frame::Heartbeat(beat) => {
            assert_eq!((beat.seq_last, beat.missed_disconnected), (3, 3), "{beat:?}");
        }
        other => panic!("a heartbeat comes first, got {other:?}"),
    }
    relay.write("new").await;
    assert_eq!(next_line(&mut sub).await, (4, "new".into(), false), "no replay of the three before");
}

// what this catches: a slow subscriber stalling the drain, or losing frames without saying so.
// The drain never waits on it; the oldest frames are dropped and counted.
#[tokio::test]
async fn a_slow_subscriber_loses_the_oldest_frames_counted() {
    let mut relay = start(Config { queue: 4, ..quick() }, false).await;
    let mut sub = Subscription::connect(&relay.socket).await.expect("connect");
    let padding = "p".repeat(100);
    for n in 0..20_000 {
        relay.write(&format!("{n} {padding}")).await;
    }
    let beat = heartbeat_where(&mut sub, |b| b.dropped_slow > 0 && b.seq_last == 20_000).await;
    assert!(beat.dropped_slow > 0);
    let archived = std::fs::read_to_string(&relay.log).expect("archive").lines().count();
    assert_eq!(archived, 20_000, "the archive keeps every line the subscriber lost");
}

// what this catches (Codex on #5533): connections served without bound. Past the declared bound a
// connection is refused and counted.
#[tokio::test]
async fn connections_past_the_bound_are_refused_and_counted() {
    let relay = start(Config { subscribers: 1, ..quick() }, false).await;
    let mut held = Subscription::connect(&relay.socket).await.expect("connect");
    assert!(matches!(next(&mut held).await, Frame::Heartbeat(_)));
    let mut refused = Subscription::connect(&relay.socket).await.expect("the endpoint answers");
    let over = tokio::time::timeout(Duration::from_secs(5), refused.next()).await.expect("the refusal is prompt");
    assert!(matches!(over, Ok(None) | Err(_)), "a connection past the bound is closed: {over:?}");
    let beat = heartbeat_where(&mut held, |b| b.subscribers_refused == 1).await;
    assert_eq!(beat.subscribers_refused, 1);
}

// what this catches: a frame that grows without bound with one line. The frame carries the
// line cut at its bound, flagged; the archive keeps it whole.
#[tokio::test]
async fn a_long_line_is_cut_in_its_frame_and_whole_in_the_archive() {
    let mut relay = start(Config { line_bytes: 4096, ..quick() }, false).await;
    let mut sub = subscribe(&relay.socket).await;
    let long = "x".repeat(10_000);
    relay.write(&long).await;
    let (_, text, truncated) = next_line(&mut sub).await;
    assert_eq!((text.len(), truncated), (4096, true));
    drop(relay.stderr.take());
    relay.relay.await.expect("the relay task").expect("the relay ends cleanly");
    assert_eq!(std::fs::read_to_string(&relay.log).expect("archive").trim_end(), long);
}

// what this catches (Codex on 1d7d6d4c): signals that depend on the archive. With the archive
// unwritable, every line still reaches the subscriber.
#[tokio::test]
async fn an_unwritable_archive_still_delivers_every_line() {
    let mut relay = start(quick(), true).await;
    let mut sub = subscribe(&relay.socket).await;
    relay.write("compute error").await;
    assert_eq!(next_line(&mut sub).await, (1, "compute error".into(), false));
    assert!(!relay.log.exists());
}

// what this catches (Codex on 1d7d6d4c): a predecessor's clock, an unbound relay's or an expired
// heartbeat's read as this engine's silence. Only a current heartbeat naming this incarnation
// answers.
#[test]
fn silence_is_only_this_incarnations_and_only_while_current() {
    let engine = Incarnation { pid: 7, started_s: 100 };
    let beat = |incarnation, expires_at_ms| Heartbeat {
        seq_last: 1,
        dropped_slow: 0,
        missed_disconnected: 0,
        last_line_ms: 1_000,
        relay_started_ms: 500,
        incarnation,
        binds_refused: 0,
        subscribers_refused: 0,
        expires_at_ms,
    };
    assert_eq!(silent_ms(&beat(Some(engine), 5_000), engine, 4_000), Some(3_000));
    assert_eq!(silent_ms(&beat(None, 5_000), engine, 4_000), None, "unbound");
    assert_eq!(silent_ms(&beat(Some(Incarnation { pid: 7, started_s: 99 }), 5_000), engine, 4_000), None, "a predecessor");
    assert_eq!(silent_ms(&beat(Some(engine), 3_000), engine, 4_000), None, "expired");
}
