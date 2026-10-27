//! An engine's stderr, drained by a relay that outlives the core that started it (card 1d7d6d4c).
//!
//! The core drained each engine's stderr itself ([`super::child_log`]), so a core restart that
//! adopted a running engine left nobody reading it: on enrico, 2026-10-27, engine-c ran from
//! 03:35Z to 10:44Z with no log, and its stall at 08:27Z could not be read. The relay drains the
//! engine's stderr through the same bounded pump into the same capped archive, and serves what it
//! drains on a per-engine socket, so whichever core is running reads the engine's lines, its
//! fault signals and how long it has been silent, whether it spawned that engine or adopted it.
//!
//! The socket carries signals separately from the archive: a disk that fails still delivers
//! every line to the core. Every loss is counted, never hidden: a subscriber that falls behind
//! (`dropped_slow`), lines drained while no core was connected (`missed_disconnected`), a bind
//! refused. A subscriber starts at the next line, never a replay, so an old fault is never
//! raised again as new. The engine's incarnation is taken once, from the spawning core's own
//! inherited handle, never from a socket client; until then every signal reads as unbound.
//! The relay ends when every holder of the engine's stderr has closed it.

use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use continuum_host::local_stream::{self, ClientStream, Listener, ServerStream};
use tokio::sync::broadcast;

/// The relay's bounds.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// Frames held for a subscriber that has not read them; past this the oldest are dropped
    /// and counted.
    pub queue: usize,
    /// The longest line a frame carries; the archive keeps the whole line.
    pub line_bytes: usize,
    /// How often a heartbeat is sent; a heartbeat is current for three of these.
    pub heartbeat: Duration,
}

impl Default for Config {
    fn default() -> Self {
        // derived-or-floor: a FLOOR on frames held for one slow reader of an engine's log; it bounds memory, never serving.
        let queue = 1024;
        // derived-or-floor: a FLOOR on one frame's text; the archive keeps the whole line, so nothing is lost but the frame's copy.
        let line_bytes = 4096;
        Self { queue, line_bytes, heartbeat: Duration::from_secs(1) }
    }
}

/// One engine incarnation: its pid and its OS start time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incarnation {
    pub pid: u32,
    pub started_s: u64,
}

/// What the relay says about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Heartbeat {
    /// The sequence of the last line drained.
    pub seq_last: u64,
    pub dropped_slow: u64,
    pub missed_disconnected: u64,
    /// When the engine last wrote a line; the relay's own start until it has written one.
    pub last_line_ms: u64,
    pub relay_started_ms: u64,
    /// The engine these lines are from; `None` until the spawning core has bound it.
    pub incarnation: Option<Incarnation>,
    pub binds_refused: u64,
    /// After this a consumer reads the relay's signals as unavailable.
    pub expires_at_ms: u64,
}

/// One frame on the socket, one JSON object per line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Frame {
    Line { seq: u64, text: String, truncated: bool },
    Heartbeat(Heartbeat),
}

struct State {
    config: Config,
    seq: AtomicU64,
    last_line_ms: AtomicU64,
    dropped_slow: AtomicU64,
    missed_disconnected: AtomicU64,
    binds_refused: AtomicU64,
    relay_started_ms: u64,
    incarnation: parking_lot::Mutex<Option<Incarnation>>,
    lines: broadcast::Sender<Frame>,
}

impl State {
    fn new(config: Config) -> Self {
        let started = continuum_base::time::EpochMs::now().millis();
        let (lines, _) = broadcast::channel(config.queue.max(1));
        Self {
            config,
            seq: AtomicU64::new(0),
            last_line_ms: AtomicU64::new(started),
            dropped_slow: AtomicU64::new(0),
            missed_disconnected: AtomicU64::new(0),
            binds_refused: AtomicU64::new(0),
            relay_started_ms: started,
            incarnation: parking_lot::Mutex::new(None),
            lines,
        }
    }

    fn heartbeat(&self) -> Heartbeat {
        let expires = u64::try_from(self.config.heartbeat.as_millis().saturating_mul(3)).unwrap_or(u64::MAX); // unwrap_or: an expiry past u64 ms never expires early
        Heartbeat {
            seq_last: self.seq.load(Ordering::Relaxed),
            dropped_slow: self.dropped_slow.load(Ordering::Relaxed),
            missed_disconnected: self.missed_disconnected.load(Ordering::Relaxed),
            last_line_ms: self.last_line_ms.load(Ordering::Relaxed),
            relay_started_ms: self.relay_started_ms,
            incarnation: *self.incarnation.lock(),
            binds_refused: self.binds_refused.load(Ordering::Relaxed),
            expires_at_ms: continuum_base::time::EpochMs::now().millis().saturating_add(expires),
        }
    }
}

/// PURE: `line` cut to at most `bytes` on a character boundary, and whether it was cut.
fn cut(line: &str, bytes: usize) -> (String, bool) {
    if line.len() <= bytes {
        return (line.to_string(), false);
    }
    let mut end = bytes;
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    (line[..end].to_string(), true)
}

/// Each drained line, published to whoever is connected.
struct Publish(Arc<State>);

impl super::child_log::LineWatch for Publish {
    fn observe(&mut self, line: &str) {
        let seq = self.0.seq.fetch_add(1, Ordering::Relaxed) + 1;
        self.0.last_line_ms.store(continuum_base::time::EpochMs::now().millis(), Ordering::Relaxed);
        let (text, truncated) = cut(line, self.0.config.line_bytes);
        if self.0.lines.send(Frame::Line { seq, text, truncated }).is_err() {
            self.0.missed_disconnected.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// How a relay ended: the lines it drained.
#[derive(Debug)]
pub struct RelayEnd {
    pub lines: u64,
}

/// Run a relay: drain `stderr` into the archive at `log`, serve frames on `socket`, and take the
/// engine's incarnation only from `control`, the spawning core's inherited handle. Returns when
/// `stderr` ends, that is when every writer has closed it.
pub async fn run<R, C>(stderr: R, control: C, log: &Path, socket: &Path, config: Config) -> io::Result<RelayEnd>
where
    R: AsyncRead + Unpin,
    C: AsyncRead + Unpin + Send + 'static,
{
    let listener = Listener::bind(socket)?;
    let state = Arc::new(State::new(config));
    let accept = tokio::spawn(serve(listener, Arc::clone(&state)));
    let bind = tokio::spawn(read_bind(control, Arc::clone(&state)));
    let drained = super::child_log::pump(stderr, log, Box::new(Publish(Arc::clone(&state))), Arc::new(AtomicU64::new(0))).await;
    accept.abort();
    bind.abort();
    local_stream::remove(socket);
    drained.map(|()| RelayEnd { lines: state.seq.load(Ordering::Relaxed) })
}

/// The first incarnation the spawning core writes binds the relay; any later one is refused and
/// counted.
async fn read_bind<C: AsyncRead + Unpin>(control: C, state: Arc<State>) {
    let mut lines = BufReader::new(control).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(incarnation) = serde_json::from_str::<Incarnation>(&line) else { // wire protocol: the spawning core's bind on its inherited control handle
            state.binds_refused.fetch_add(1, Ordering::Relaxed);
            continue;
        };
        let mut bound = state.incarnation.lock();
        match *bound {
            None => *bound = Some(incarnation),
            Some(_) => {
                state.binds_refused.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

async fn serve(mut listener: Listener, state: Arc<State>) {
    while let Ok(stream) = listener.accept().await {
        tokio::spawn(subscriber(stream, Arc::clone(&state)));
    }
}

async fn write_frame(stream: &mut ServerStream, frame: &Frame) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(frame).map_err(io::Error::other)?; // wire protocol: one JSON frame per line on the relay's local socket
    bytes.push(b'\n');
    stream.write_all(&bytes).await
}

/// One subscriber: a heartbeat first, then the lines drained from now on, and a heartbeat each
/// period. A subscriber that falls behind loses the oldest frames, counted.
async fn subscriber(mut stream: ServerStream, state: Arc<State>) {
    let mut lines = state.lines.subscribe();
    if write_frame(&mut stream, &Frame::Heartbeat(state.heartbeat())).await.is_err() {
        return;
    }
    let mut beat = tokio::time::interval(state.config.heartbeat);
    beat.tick().await; // the first tick is immediate: the opening heartbeat is already sent
    loop {
        let frame = tokio::select! {
            received = lines.recv() => match received {
                Ok(frame) => frame,
                Err(broadcast::error::RecvError::Lagged(lost)) => {
                    state.dropped_slow.fetch_add(lost, Ordering::Relaxed);
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => return,
            },
            _ = beat.tick() => Frame::Heartbeat(state.heartbeat()),
        };
        if write_frame(&mut stream, &frame).await.is_err() {
            return;
        }
    }
}

/// A consumer's side of a relay socket.
pub struct Subscription {
    frames: tokio::io::Lines<BufReader<ClientStream>>,
}

impl Subscription {
    pub async fn connect(socket: &Path) -> io::Result<Self> {
        Ok(Self { frames: BufReader::new(local_stream::connect(socket).await?).lines() })
    }

    /// The next frame, or `None` when the relay has ended.
    pub async fn next(&mut self) -> io::Result<Option<Frame>> {
        match self.frames.next_line().await? {
            Some(line) => serde_json::from_str(&line).map(Some).map_err(io::Error::other), // wire protocol: one JSON frame per line on the relay's local socket
            None => Ok(None),
        }
    }
}

/// PURE: how long the engine named `expected` has been silent at `now_ms`, from a heartbeat.
/// `None` (unavailable) when the heartbeat has expired, names no incarnation yet, or names
/// another one: a predecessor's clock is never this engine's.
pub fn silent_ms(heartbeat: &Heartbeat, expected: Incarnation, now_ms: u64) -> Option<u64> {
    (now_ms < heartbeat.expires_at_ms && heartbeat.incarnation == Some(expected))
        .then(|| now_ms.saturating_sub(heartbeat.last_line_ms))
}

#[cfg(test)]
mod tests;
