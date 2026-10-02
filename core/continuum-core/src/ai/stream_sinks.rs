//! In-flight generation sinks, keyed by stream id — the join key between a wire
//! hop that wants the tokens LIVE and the one `ai/generate` command that produces
//! them.
//!
//! Inference is a stream, not a promise (`AIProviderAdapter::generate_stream` is
//! the primitive; `generate_text` is its drain). A remote hop that awaited the
//! whole answer under one deadline was the recipe for the 600 s deaths of
//! 2026-09-17: a lane that IS working and one that is dead look identical for
//! ten minutes. With the tokens on the wire, liveness is the next chunk, and the
//! requester renders as the answer forms.
//!
//! The command runs through the executor — ACL, interceptors, the caller's
//! identity — exactly as before; the only addition is `streamId` in the params,
//! which the command resolves HERE to the sink the hop registered. The registry
//! holds nothing after the call: the guard removes the entry on drop, and a
//! sender the command never took is dropped with the guard, which closes the
//! receiver the hop is draining.
use std::sync::OnceLock;

use dashmap::DashMap;
use tokio::sync::broadcast;
use tokio::sync::watch;

// A bounded live ring, shared by every inference modality. Oversize chunks fail
// at publication; lag is explicit at receipt, never a silent partial answer.
pub const GENERATION_RING_CAPACITY: usize = 256;
pub const MAX_GENERATION_CHUNK_BYTES: usize = 64 * 1024;

/// A packet in the existing inference stream. Timing is media time, not wall time.
/// The session/stream ID is owned by the enclosing sink and AIRC headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaChunk {
    pub sequence: u64,
    pub presentation_time_us: u64,
    pub mime_type: String,
    pub data: std::sync::Arc<[u8]>,
}

#[derive(Clone, Debug)]
pub struct GenerationSink {
    tx: broadcast::Sender<GenerationChunk>,
    cancelled: watch::Sender<bool>,
    observing: bool,
}

pub struct GenerationReceiver {
    rx: broadcast::Receiver<GenerationChunk>,
    cancelled: watch::Sender<bool>,
}

pub fn channel() -> (GenerationSink, GenerationReceiver) {
    let (tx, rx) = broadcast::channel(GENERATION_RING_CAPACITY);
    let cancelled = watch::channel(false).0;
    (GenerationSink { tx, cancelled: cancelled.clone(), observing: true },
     GenerationReceiver { rx, cancelled })
}

impl GenerationSink {
    /// Explicit text-only drain; never selects a different provider wire mode.
    pub fn discard() -> Self {
        let (mut sink, receiver) = channel();
        sink.observing = false;
        drop(receiver);
        sink
    }
    pub fn is_closed(&self) -> bool { !self.observing || *self.cancelled.borrow() }
    pub async fn closed(&self) {
        if self.observing { let mut state = self.cancelled.subscribe(); let _ = state.wait_for(|cancelled| *cancelled).await; }
        else { std::future::pending::<()>().await }
    }
    pub fn send(&self, chunk: GenerationChunk) -> Result<(), String> {
        if !self.observing {
            return if matches!(chunk, GenerationChunk::Media(_)) {
                Err("Native media requires an observing stream consumer; text-only drain cannot discard it".into())
            } else { Ok(()) };
        }
        if *self.cancelled.borrow() { return Err("Inference stream consumer cancelled".into()); }
        if chunk.byte_len() > MAX_GENERATION_CHUNK_BYTES {
            return Err("Inference stream chunk exceeds shared ring byte limit".into());
        }
        self.tx.send(chunk).map(|_| ()).map_err(|_| "Inference stream consumer disconnected".into())
    }
}

impl GenerationReceiver {
    pub async fn recv(&mut self) -> Result<GenerationChunk, broadcast::error::RecvError> {
        let result = self.rx.recv().await;
        if matches!(result, Err(broadcast::error::RecvError::Lagged(_))) { self.cancelled.send_replace(true); }
        result
    }
    pub fn try_recv(&mut self) -> Result<GenerationChunk, broadcast::error::TryRecvError> {
        let result = self.rx.try_recv();
        if matches!(result, Err(broadcast::error::TryRecvError::Lagged(_))) { self.cancelled.send_replace(true); }
        result
    }
}
impl Drop for GenerationReceiver {
    fn drop(&mut self) { self.cancelled.send_replace(true); }
}
use uuid::Uuid;

use crate::ai::adapter::GenerationChunk;

/// The params key a hop stamps so the command streams into its registered sink.
pub const STREAM_ID_PARAM: &str = "streamId";

fn sinks() -> &'static DashMap<Uuid, GenerationSink> {
    static SINKS: OnceLock<DashMap<Uuid, GenerationSink>> = OnceLock::new();
    SINKS.get_or_init(DashMap::new)
}

/// Removes the registration on drop — a hop that gives up (deadline, peer gone)
/// leaves nothing behind for the next stream id to collide with.
pub struct StreamSinkGuard {
    stream_id: Uuid,
}

impl Drop for StreamSinkGuard {
    fn drop(&mut self) {
        sinks().remove(&self.stream_id);
    }
}

/// Register the sink a command should stream into when its params carry
/// `streamId == stream_id`. Held for the life of the call.
pub fn register(stream_id: Uuid, sink: GenerationSink) -> StreamSinkGuard {
    sinks().insert(stream_id, sink);
    StreamSinkGuard { stream_id }
}

/// The sink registered for `stream_id`, taken exactly once — the command that
/// generates is the one consumer. `None` when no hop registered (a plain call).
pub fn take(stream_id: Uuid) -> Option<GenerationSink> {
    sinks().remove(&stream_id).map(|(_, s)| s)
}

/// The stream id a caller stamped on the params, if any.
pub fn stream_id_of(params: &serde_json::Value) -> Option<Uuid> {
    params
        .get(STREAM_ID_PARAM)
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the join key works exactly once and leaves nothing
    // behind — a second `take` is None, a dropped guard clears an untaken entry
    // (so the hop's receiver closes instead of waiting forever), and a params
    // object without the key is a plain call.
    #[test]
    fn a_registered_sink_is_taken_once_and_a_dropped_guard_clears_it() {
        let id = Uuid::new_v4();
        let (tx, mut rx) = channel();
        let guard = register(id, tx);
        assert_eq!(stream_id_of(&serde_json::json!({ "streamId": id.to_string() })), Some(id));
        assert_eq!(stream_id_of(&serde_json::json!({ "model": "qwen" })), None);
        let taken = take(id).expect("registered"); // JUSTIFIED: the invariant under test
        assert!(take(id).is_none(), "taken exactly once");
        taken.send(GenerationChunk::Token("hi".into())).expect("receiver open"); // JUSTIFIED: the invariant under test
        drop(taken);
        assert_eq!(rx.try_recv().ok(), Some(GenerationChunk::Token("hi".into())));
        drop(guard);

        let id2 = Uuid::new_v4();
        let (tx2, mut rx2) = channel();
        let guard2 = register(id2, tx2);
        drop(guard2);
        assert!(take(id2).is_none(), "the guard cleared the untaken entry");
        assert!(
            matches!(rx2.try_recv(), Err(broadcast::error::TryRecvError::Closed)),
            "the hop's receiver closes when nobody took the sink"
        );
    }
    // what this catches: a slow consumer cannot turn any inference modality into
    // an unbounded queue; lag is terminal and dropping a consumer wakes cancellation.
    #[tokio::test]
    async fn ring_is_bounded_and_loss_cancels_the_producer() {
        let (tx, mut rx) = channel();
        for _ in 0..=GENERATION_RING_CAPACITY { tx.send(GenerationChunk::Token("x".into())).unwrap(); }
        assert!(matches!(rx.recv().await, Err(broadcast::error::RecvError::Lagged(1))));
        tx.closed().await;
        assert!(tx.send(GenerationChunk::Token("cannot continue".into())).is_err());
        let (tx, rx) = channel();
        assert!(tx.send(GenerationChunk::Token("x".repeat(MAX_GENERATION_CHUNK_BYTES + 1))).is_err());
        drop(rx);
        tx.closed().await;
        assert!(tx.is_closed());
    }

    // what this catches: an unwired media consumer cannot report successful
    // generation after silently throwing away the native model's audio/image.
    #[test]
    fn text_only_drain_refuses_native_media() {
        let sink = GenerationSink::discard();
        assert!(sink.send(GenerationChunk::Token("text remains supported".into())).is_ok());
        let media = MediaChunk {
            sequence: 0,
            presentation_time_us: 0,
            mime_type: "audio/pcm;rate=24000;channels=1;format=s16le".into(),
            data: std::sync::Arc::from([0u8, 0]),
        };
        assert!(sink.send(GenerationChunk::Media(std::sync::Arc::new(media)))
            .unwrap_err().contains("observing stream consumer"));
    }

}
