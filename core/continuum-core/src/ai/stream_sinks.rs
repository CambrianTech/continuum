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
