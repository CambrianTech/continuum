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
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use crate::ai::adapter::GenerationChunk;

/// The params key a hop stamps so the command streams into its registered sink.
pub const STREAM_ID_PARAM: &str = "streamId";

/// Consume incremental output while awaiting the authoritative whole response.
/// The caller owns both futures: cancellation drops both, and completion does
/// not wait for stray sender clones. No detached reader or buffered transcript.
pub async fn drain_generation<F, Fut, T>(generate: F) -> T
where
    F: FnOnce(UnboundedSender<GenerationChunk>) -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let (sink, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let generation = generate(sink);
    tokio::pin!(generation);
    loop {
        tokio::select! {
            result = &mut generation => return result,
            chunk = receiver.recv() => {
                if chunk.is_none() {
                    return generation.await;
                }
            }
        }
    }
}

fn sinks() -> &'static DashMap<Uuid, UnboundedSender<GenerationChunk>> {
    static SINKS: OnceLock<DashMap<Uuid, UnboundedSender<GenerationChunk>>> = OnceLock::new();
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
pub fn register(stream_id: Uuid, sink: UnboundedSender<GenerationChunk>) -> StreamSinkGuard {
    sinks().insert(stream_id, sink);
    StreamSinkGuard { stream_id }
}

/// The sink registered for `stream_id`, taken exactly once — the command that
/// generates is the one consumer. `None` when no hop registered (a plain call).
pub fn take(stream_id: Uuid) -> Option<UnboundedSender<GenerationChunk>> {
    sinks().remove(&stream_id).map(|(_, s)| s)
}

/// Hand one chunk to a generation's consumer, then give the consumer the turn before the
/// producer parses further. Every producer the local serving path runs sends through this:
/// `inference::sse_stream` (llama-server's stream) and `AIProviderAdapter`'s default
/// `generate_stream` (a one-shot backend's whole answer).
///
/// The sink is unbounded so decode never blocks on a reader, but an unbounded send never
/// returns `Pending` either: one network read of llama-server's stream carries many events,
/// and a producer that parsed them all in one poll left a whole read's worth of chunks
/// waiting before its consumer ran. Yielding after each send makes every chunk its own poll,
/// so a consumer that polls the generation only when it can take more (the driver gateway's
/// streamed answer) holds at most one chunk; the rest stays unread in the engine's stream,
/// where the socket's receive window pushes back on the server. A resident's generation
/// pays one reschedule per chunk. Pinned against a ready burst through the real producer by
/// `ai/openai_adapter/tests/a_ready_burst_reaches_the_consumer_one_chunk_per_poll.rs`.
///
/// A send to a closed sink is not a failure: a caller that wants only the assembled answer
/// drops its receiver, and generation continues for the response it returns.
pub async fn deliver(sink: &UnboundedSender<GenerationChunk>, chunk: GenerationChunk) {
    if sink.send(chunk).is_ok() {
        tokio::task::yield_now().await;
    }
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

    #[tokio::test]
    async fn whole_response_drain_keeps_consumer_alive_and_ends_with_response()
        -> Result<(), Box<dyn std::error::Error>>
    {
        let (retained, retained_rx) = tokio::sync::oneshot::channel();
        let response = drain_generation(|sink| async move {
            sink.send(GenerationChunk::Token("progress".into()))?;
            retained.send(sink.clone()).map_err(|_| "retain sender")?;
            tokio::task::yield_now().await;
            assert!(!sink.is_closed(), "whole-response call is still consuming");
            Ok::<_, Box<dyn std::error::Error>>("terminal")
        }).await?;
        assert_eq!(response, "terminal");
        assert!(retained_rx.await?.is_closed(), "terminal response drops the reader despite sender clones");
        Ok(())
    }

    #[tokio::test]
    async fn cancelling_whole_response_drops_its_consumer()
        -> Result<(), Box<dyn std::error::Error>>
    {
        let (retained, retained_rx) = tokio::sync::oneshot::channel();
        let mut generation = Box::pin(drain_generation(|sink| async move {
            let _ = retained.send(sink.clone());
            std::future::pending::<()>().await;
        }));
        assert!(futures::poll!(&mut generation).is_pending());
        let sink = retained_rx.await?;
        assert!(!sink.is_closed());
        drop(generation);
        assert!(sink.is_closed());
        Ok(())
    }

    // what this catches: the join key works exactly once and leaves nothing
    // behind — a second `take` is None, a dropped guard clears an untaken entry
    // (so the hop's receiver closes instead of waiting forever), and a params
    // object without the key is a plain call.
    #[test]
    fn a_registered_sink_is_taken_once_and_a_dropped_guard_clears_it() {
        let id = Uuid::new_v4();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
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
        let (tx2, mut rx2) = tokio::sync::mpsc::unbounded_channel::<GenerationChunk>();
        let guard2 = register(id2, tx2);
        drop(guard2);
        assert!(take(id2).is_none(), "the guard cleared the untaken entry");
        assert!(
            matches!(rx2.try_recv(), Err(tokio::sync::mpsc::error::TryRecvError::Disconnected)),
            "the hop's receiver closes when nobody took the sink"
        );
    }
}
