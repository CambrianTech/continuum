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
use std::sync::atomic::{AtomicBool, Ordering};

use dashmap::DashMap;
use tokio::sync::broadcast;
use tokio::sync::watch;

// A bounded live ring, shared by every inference modality. Oversize chunks fail
// at publication; lag is explicit at receipt, never a silent partial answer.
pub const GENERATION_RING_CAPACITY: usize = 256;
pub const MAX_GENERATION_CHUNK_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestPhase {
    Started,
    Finished(crate::ai::types::FinishReason),
    Aborted,
}

struct RequestOwner<'a> {
    sink: &'a GenerationSink,
    request_id: std::sync::Arc<str>,
    finished: bool,
    retired: watch::Sender<bool>,
}

impl Drop for RequestOwner<'_> {
    fn drop(&mut self) {
        self.retired.send_replace(true);
        if !self.finished {
            let _ = self.sink.send(GenerationChunk::RequestBoundary {
                request_id: self.request_id.clone(), phase: RequestPhase::Aborted,
            });
        }
        // Publish retirement before allowing a successor to start on this ring.
        self.sink.request_active.store(false, Ordering::Release);
    }
}

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
    request_active: std::sync::Arc<AtomicBool>,
    retired: Option<watch::Sender<bool>>,
    text_presentation: bool,
}

pub struct GenerationReceiver {
    rx: broadcast::Receiver<GenerationChunk>,
    cancelled: watch::Sender<bool>,
    text_presentation: bool,
}

pub fn channel() -> (GenerationSink, GenerationReceiver) {
    let (tx, rx) = broadcast::channel(GENERATION_RING_CAPACITY);
    let cancelled = watch::channel(false).0;
    (GenerationSink { tx, cancelled: cancelled.clone(), observing: true,
        request_active: std::sync::Arc::new(AtomicBool::new(false)), retired: None,
        text_presentation: false },
     GenerationReceiver { rx, cancelled, text_presentation: false })
}

/// Optional progressive text display may retire its preview after lag without
/// cancelling useful inference. Native media is refused at publication, even if
/// its packet would fall out of the ring. Policy is fixed before either handle
/// escapes; dropping the receiver still cancels the owning turn.
pub(crate) fn text_presentation_channel() -> (GenerationSink, GenerationReceiver) {
    let (mut sink, mut receiver) = channel();
    sink.text_presentation = true;
    receiver.text_presentation = true;
    (sink, receiver)
}

impl GenerationSink {
    /// Attribute one model attempt on the existing ring, including future drop.
    /// The turn may keep this sink alive for subsequent act/observe requests.
    pub async fn run_request<F, Make>(&self, request_id: std::sync::Arc<str>, make_generation: Make)
        -> Result<crate::ai::types::TextGenerationResponse, crate::ai::inference_error::InferenceError>
    where F: std::future::Future<Output = Result<crate::ai::types::TextGenerationResponse,
        crate::ai::inference_error::InferenceError>>,
        Make: FnOnce(GenerationSink) -> F,
    {
        // Boundary-framed chunks have one attribution owner per ring. Parallel
        // model attempts use independent sinks; interleaving here loses identity.
        if self.retired.is_some() {
            return Err("Nested request ownership is not supported".to_string().into());
        }
        self.request_active.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "Inference stream already has an active request".to_string())?;
        let retired = watch::channel(false).0;
        let mut owner = RequestOwner { sink: self, request_id, finished: true, retired: retired.clone() };
        self.send(GenerationChunk::RequestBoundary {
            request_id: owner.request_id.clone(), phase: RequestPhase::Started,
        })?;
        owner.finished = false;
        let mut producer = self.clone();
        producer.retired = Some(retired);
        let generation = make_generation(producer);
        // The shared boundary owns cancellation even when a provider is waiting
        // for its next packet and has not attempted another sink publication.
        let result = tokio::select! {
            biased;
            _ = self.closed() => Err("Inference stream consumer cancelled".to_string().into()),
            result = generation => result,
        };
        // Revoke every provider clone before the terminal boundary is published.
        owner.retired.send_replace(true);
        if let Ok(response) = &result {
            self.send(GenerationChunk::RequestBoundary {
                request_id: owner.request_id.clone(),
                phase: RequestPhase::Finished(response.finish_reason),
            })?;
            owner.finished = true;
        }
        result
    }
    /// Explicit text-only drain; never selects a different provider wire mode.
    pub fn discard() -> Self {
        let (mut sink, receiver) = channel();
        sink.observing = false;
        drop(receiver);
        sink
    }
    pub fn is_closed(&self) -> bool {
        !self.observing || *self.cancelled.borrow()
            || self.retired.as_ref().is_some_and(|state| *state.borrow())
    }
    pub async fn closed(&self) {
        if self.observing {
            let mut state = self.cancelled.subscribe();
            let retired = async {
                match &self.retired {
                    Some(state) => { let mut state = state.subscribe(); let _ = state.wait_for(|closed| *closed).await; }
                    None => std::future::pending::<()>().await,
                }
            };
            tokio::select! { _ = state.wait_for(|cancelled| *cancelled) => {}, _ = retired => {} }
        }
        else { std::future::pending::<()>().await }
    }
    pub fn send(&self, chunk: GenerationChunk) -> Result<(), String> {
        if self.text_presentation && matches!(chunk, GenerationChunk::Media(_)) {
            return Err("Optional text presentation cannot consume native media".into());
        }
        // Keep the read guard until publication ends. Retirement obtains the
        // write guard before emitting the boundary, so stale chunks cannot race
        // past that boundary. Neither side holds this guard across an await.
        let retired = self.retired.as_ref().map(|state| state.borrow());
        if retired.as_ref().is_some_and(|state| **state) {
            return Err("Inference request producer retired".into());
        }
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
        if matches!(result, Err(broadcast::error::RecvError::Lagged(_)))
            && !self.text_presentation { self.cancelled.send_replace(true); }
        result
    }
    pub fn try_recv(&mut self) -> Result<GenerationChunk, broadcast::error::TryRecvError> {
        let result = self.rx.try_recv();
        if matches!(result, Err(broadcast::error::TryRecvError::Lagged(_)))
            && !self.text_presentation { self.cancelled.send_replace(true); }
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

fn sinks() -> &'static DashMap<Uuid, Option<GenerationSink>> {
    static SINKS: OnceLock<DashMap<Uuid, Option<GenerationSink>>> = OnceLock::new();
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
pub fn register(stream_id: Uuid, sink: GenerationSink) -> Result<StreamSinkGuard, String> {
    match sinks().entry(stream_id) {
        dashmap::mapref::entry::Entry::Vacant(entry) => {
            entry.insert(Some(sink));
            Ok(StreamSinkGuard { stream_id })
        }
        dashmap::mapref::entry::Entry::Occupied(_) =>
            Err("Inference stream ID already has an active owner".into()),
    }
}

/// The sink registered for `stream_id`, taken exactly once — the command that
/// generates is the one consumer. `None` when no hop registered (a plain call).
pub fn take(stream_id: Uuid) -> Option<GenerationSink> {
    // Keep the reservation until the request guard drops. Otherwise a duplicate
    // could reuse the ID after take, and the first guard would remove its sink.
    sinks().get_mut(&stream_id).and_then(|mut sink| sink.take())
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
        let guard = register(id, tx).unwrap();
        let (duplicate, _) = channel();
        assert!(register(id, duplicate).is_err(), "cannot replace an untaken sink");
        assert_eq!(stream_id_of(&serde_json::json!({ "streamId": id.to_string() })), Some(id));
        assert_eq!(stream_id_of(&serde_json::json!({ "model": "qwen" })), None);
        let taken = take(id).expect("registered"); // JUSTIFIED: the invariant under test
        assert!(take(id).is_none(), "taken exactly once");
        let (duplicate, _) = channel();
        assert!(register(id, duplicate).is_err(), "taken sink still owns its ID");
        taken.send(GenerationChunk::Token("hi".into())).expect("receiver open"); // JUSTIFIED: the invariant under test
        drop(taken);
        assert_eq!(rx.try_recv().ok(), Some(GenerationChunk::Token("hi".into())));
        drop(guard);
        let (successor, _) = channel();
        drop(register(id, successor).expect("released ID can be reused"));

        let id2 = Uuid::new_v4();
        let (tx2, mut rx2) = channel();
        let guard2 = register(id2, tx2).unwrap();
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

        // Optional text display is the explicit exception. Overload can retire
        // its preview, but must neither kill inference nor conceal native media.
        let (tx, mut rx) = text_presentation_channel();
        assert!(tx.send(GenerationChunk::Media(std::sync::Arc::new(MediaChunk {
            sequence: 0, presentation_time_us: 0, mime_type: "audio/pcm".into(),
            data: std::sync::Arc::from([0u8, 0]),
        }))).unwrap_err().contains("cannot consume native media"));
        for _ in 0..=GENERATION_RING_CAPACITY { tx.send(GenerationChunk::Token("x".into())).unwrap(); }
        assert!(matches!(rx.recv().await, Err(broadcast::error::RecvError::Lagged(1))));
        assert!(!tx.is_closed());
        tx.send(GenerationChunk::Token("useful inference continues".into())).unwrap();
        assert!(matches!(rx.try_recv(), Err(broadcast::error::TryRecvError::Lagged(1))));
        assert!(!tx.is_closed());
        drop(rx);
        tx.closed().await;
        assert!(tx.is_closed(), "turn ownership still cancels optional presentation");
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

    // A turn retains its sink across multiple attempts. Dropping one attempt
    // cannot imply successful completion or borrow a provider's response ID.
    #[tokio::test]
    async fn request_boundaries_survive_abort_and_keep_submitted_identity() {
        use crate::ai::types::{FinishReason, TextGenerationResponse};
        let (sink, mut receiver) = channel();
        let first: std::sync::Arc<str> = "submitted-first".into();
        let mut stale_producer = None;
        let mut attempt = Box::pin(sink.run_request(first.clone(), |producer| {
            stale_producer = Some(producer);
            std::future::pending()
        }));
        tokio::select! {
            event = receiver.recv() => assert_eq!(event.unwrap(), GenerationChunk::RequestBoundary {
                request_id: first.clone(), phase: RequestPhase::Started,
            }),
            _ = &mut attempt => panic!("pending model cannot complete"),
        }
        let clone = sink.clone();
        let overlap = clone.run_request("overlapping-attempt".into(), |_| async {
            panic!("an overlapping provider must not be polled");
        }).await;
        assert!(overlap.is_err(), "cloned sinks share request attribution ownership");
        assert!(receiver.try_recv().is_err(), "rejected attempt cannot alter the live stream");
        drop(attempt);
        let stale_producer = stale_producer.unwrap();
        assert!(stale_producer.is_closed(), "attempt drop revokes retained provider clones");
        assert!(stale_producer.send(GenerationChunk::Token("late first attempt".into())).is_err());
        tokio::time::timeout(std::time::Duration::from_secs(1), stale_producer.closed())
            .await.expect("retirement wakes retained producer waiters");
        assert_eq!(receiver.recv().await.unwrap(), GenerationChunk::RequestBoundary {
            request_id: first, phase: RequestPhase::Aborted,
        });
        for reason in [FinishReason::Length, FinishReason::Stop] {
            let id: std::sync::Arc<str> = uuid::Uuid::new_v4().to_string().into();
            sink.run_request(id.clone(), |_| async { Ok(TextGenerationResponse {
                text: String::new(), finish_reason: reason, model: "fixture".into(),
                provider: "fixture".into(), usage: Default::default(), response_time_ms: 0,
                request_id: "different-provider-id".into(), content: None, tool_calls: None,
                reasoning: None, routing: None, error: None, timing: None,
            }) }).await.unwrap();
            assert_eq!(receiver.recv().await.unwrap(), GenerationChunk::RequestBoundary {
                request_id: id.clone(), phase: RequestPhase::Started,
            });
            assert_eq!(receiver.recv().await.unwrap(), GenerationChunk::RequestBoundary {
                request_id: id, phase: RequestPhase::Finished(reason),
            });
        }
        assert!(receiver.try_recv().is_err(), "no duplicate abort after completion");
        assert!(!sink.is_closed(), "attempt closure is not turn closure");

        // Provider failure must retire this attempt without leaking diagnostics
        // into presentation or marking the partial answer as successful.
        let failed_id: std::sync::Arc<str> = "provider-failed".into();
        let result = sink.run_request(failed_id.clone(), |producer| async move {
            producer.send(GenerationChunk::Token("partial".into()))?;
            Err("provider diagnostic must not become speech".to_string().into())
        }).await;
        assert!(result.is_err());
        assert_eq!(receiver.recv().await.unwrap(), GenerationChunk::RequestBoundary {
            request_id: failed_id.clone(), phase: RequestPhase::Started,
        });
        assert_eq!(receiver.recv().await.unwrap(), GenerationChunk::Token("partial".into()));
        assert_eq!(receiver.recv().await.unwrap(), GenerationChunk::RequestBoundary {
            request_id: failed_id, phase: RequestPhase::Aborted,
        });
        assert!(receiver.try_recv().is_err(), "no success or diagnostic content after failure");

        // A quiet provider must release its future when presentation disappears;
        // waiting for the next token to discover cancellation can take minutes.
        let cancelled = sink.run_request("consumer-gone".into(), |_| std::future::pending());
        let disconnect = async {
            assert_eq!(receiver.recv().await.unwrap(), GenerationChunk::RequestBoundary {
                request_id: "consumer-gone".into(), phase: RequestPhase::Started,
            });
            drop(receiver);
        };
        let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            tokio::join!(cancelled, disconnect)
        }).await.expect("consumer cancellation must wake a quiet provider");
        assert!(result.is_err());
    }

}
