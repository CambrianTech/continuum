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
//! (`dropped_slow`), lines drained while no core was connected (`missed_disconnected`), a
//! connection past the bound (`subscribers_refused`). The first subscriber, the spawning core, gets the lines held from before it (the engine's
//! load banner); every later one starts at the next line, never a replay, so an old fault is
//! never raised again as new. The engine's incarnation is fixed when the spawning core starts the
//! relay (the engine is spawned first, so the core knows it), never taken from a socket client.
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

use super::engine_residency::EngineIncarnation;

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
    /// Connections served at once; one more is refused and counted.
    pub subscribers: usize,
}

impl Default for Config {
    fn default() -> Self {
        // derived-or-floor: a FLOOR on frames held for one slow reader of an engine's log; it bounds memory, never serving.
        let queue = 1024;
        // derived-or-floor: a FLOOR on one frame's text; the archive keeps the whole line, so nothing is lost but the frame's copy.
        let line_bytes = 4096;
        // derived-or-floor: a FLOOR on simultaneous readers of one engine's log: the core, plus room for an operator's look.
        let subscribers = 4;
        Self { queue, line_bytes, heartbeat: Duration::from_secs(1), subscribers }
    }
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
    /// The engine these lines are from, fixed by the core that spawned the relay.
    pub incarnation: EngineIncarnation,
    pub subscribers_refused: u64,
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
    subscribers_refused: AtomicU64,
    relay_started_ms: u64,
    incarnation: EngineIncarnation,
    lines: broadcast::Sender<Frame>,
    /// The lines drained before the first subscriber, held for it: that is the spawning core,
    /// and the engine's load banner is what its watchers read. Taken once; after that a line
    /// nobody is connected for is counted, never held.
    startup: parking_lot::Mutex<Option<std::collections::VecDeque<Frame>>>,
}

impl State {
    fn new(config: Config, incarnation: EngineIncarnation) -> Self {
        let started = continuum_base::time::EpochMs::now().millis();
        let (lines, _) = broadcast::channel(config.queue.max(1));
        Self {
            config,
            seq: AtomicU64::new(0),
            last_line_ms: AtomicU64::new(started),
            dropped_slow: AtomicU64::new(0),
            missed_disconnected: AtomicU64::new(0),
            subscribers_refused: AtomicU64::new(0),
            relay_started_ms: started,
            incarnation,
            lines,
            startup: parking_lot::Mutex::new(Some(std::collections::VecDeque::new())),
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
            incarnation: self.incarnation,
            subscribers_refused: self.subscribers_refused.load(Ordering::Relaxed),
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
        let frame = Frame::Line { seq, text, truncated };
        let mut startup = self.0.startup.lock();
        if let Some(held) = startup.as_mut() {
            if held.len() >= self.0.config.queue.max(1) {
                held.pop_front();
                self.0.missed_disconnected.fetch_add(1, Ordering::Relaxed);
            }
            held.push_back(frame);
            return;
        }
        drop(startup);
        if self.0.lines.send(frame).is_err() {
            self.0.missed_disconnected.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// How a relay ended: the lines it drained.
#[derive(Debug)]
pub struct RelayEnd {
    pub lines: u64,
}

/// Run a relay for `incarnation`: drain `stderr` into the archive at `log` and serve frames on
/// `socket`. Returns when `stderr` ends, that is when every writer has closed it.
pub async fn run<R>(stderr: R, incarnation: EngineIncarnation, log: &Path, socket: &Path, config: Config) -> io::Result<RelayEnd>
where
    R: AsyncRead + Unpin,
{
    let (listener, owned) = Listener::bind(socket).await?;
    let state = Arc::new(State::new(config, incarnation));
    let (shutdown, ended) = tokio::sync::watch::channel(false);
    let serving = tokio::spawn(serve(listener, Arc::clone(&state), ended));
    let drained = super::child_log::pump(stderr, log, Box::new(Publish(Arc::clone(&state))), Arc::new(AtomicU64::new(0))).await;
    // Every writer has closed: every subscriber is ended (its connection closes, so its reader
    // sees the end) before the endpoint goes.
    let _ = shutdown.send(true);
    let _ = serving.await; // a serving task that panicked has nothing left to end
    owned.remove();
    drained.map(|()| RelayEnd { lines: state.seq.load(Ordering::Relaxed) })
}

/// Accept connections up to the bound, each one an owned task, until shutdown; then end every
/// one, blocked writes included, and wait for them.
async fn serve(mut listener: Listener, state: Arc<State>, mut shutdown: tokio::sync::watch::Receiver<bool>) {
    let mut subscribers = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let Ok(stream) = accepted else { continue };
                while subscribers.try_join_next().is_some() {}
                if subscribers.len() >= state.config.subscribers.max(1) {
                    state.subscribers_refused.fetch_add(1, Ordering::Relaxed);
                    drop(stream);
                    continue;
                }
                subscribers.spawn(subscriber(stream, Arc::clone(&state)));
            }
            _ = shutdown.changed() => break,
        }
    }
    subscribers.abort_all();
    while subscribers.join_next().await.is_some() {}
}

async fn write_frame<W: tokio::io::AsyncWrite + Unpin>(stream: &mut W, frame: &Frame) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(frame).map_err(io::Error::other)?; // wire protocol: one JSON frame per line on the relay's local socket
    bytes.push(b'\n');
    stream.write_all(&bytes).await
}

/// One subscriber: a heartbeat first, then (for the first subscriber only) the lines held from
/// before it, then the lines drained from now on, and a heartbeat each period. A subscriber that
/// falls behind loses the oldest frames, counted.
async fn subscriber(stream: ServerStream, state: Arc<State>) {
    let (mut reader, mut writer) = tokio::io::split(stream);
    // Subscribed and the held lines taken under one lock with the drain's hand-off, so no line
    // falls between them or arrives twice.
    let (mut lines, held) = {
        let mut startup = state.startup.lock();
        (state.lines.subscribe(), startup.take())
    };
    if write_frame(&mut writer, &Frame::Heartbeat(state.heartbeat())).await.is_err() {
        return;
    }
    for frame in held.into_iter().flatten() {
        if write_frame(&mut writer, &frame).await.is_err() {
            return;
        }
    }
    let mut beat = tokio::time::interval(state.config.heartbeat);
    beat.tick().await; // the first tick is immediate: the opening heartbeat is already sent
    let mut ignored = [0u8; 64];
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
            // The subscriber sends nothing; its end (or an error) ends this task at once.
            read = tokio::io::AsyncReadExt::read(&mut reader, &mut ignored) => match read {
                Ok(0) | Err(_) => return,
                Ok(_) => continue,
            },
        };
        if write_frame(&mut writer, &frame).await.is_err() {
            return;
        }
    }
}

/// A consumer's side of a relay socket. The relay subscribes a connection before it sends that
/// connection's opening heartbeat, so a consumer receives the lines drained after its first
/// heartbeat; the first consumer also receives the lines held from before any connected. A line
/// drained while nobody is connected after that is counted in `missed_disconnected`.
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
/// `None` (unavailable) when the heartbeat has expired or names another incarnation: a
/// predecessor's clock is never this engine's.
pub fn silent_ms(heartbeat: &Heartbeat, expected: EngineIncarnation, now_ms: u64) -> Option<u64> {
    (now_ms < heartbeat.expires_at_ms && heartbeat.incarnation == expected)
        .then(|| now_ms.saturating_sub(heartbeat.last_line_ms))
}

pub mod follow;
pub mod process;

#[cfg(test)]
mod tests;
