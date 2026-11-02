//! A streamed answer: the SSE body OWNS the generation, so the agent's connection is the
//! generation's lifetime.
//!
//! Codex on #5541 (34672ec73): a generation spawned beside the body outlived a departed agent
//! (the engine request and its lane kept running) while its capture said cancelled, and two
//! unbounded queues let a slow agent accumulate a whole answer in memory. Here there is no
//! task. The body polls the generation itself, and only when the agent has taken every frame
//! already encoded:
//!
//! - **The agent leaves:** hyper drops the body, which drops the generation future. That is
//!   the engine's one cancellation path, the same one a resident's abandoned turn takes
//!   (`continuum-cognition` `generation_drop.rs`): the adapter's future owns its
//!   `TurnAdmission` (`continuum-engine` `ai/openai_adapter/stream.rs`, `_admission`), whose
//!   `Drop` releases the slot pin and permit (`inference/turn_admission.rs`, `impl Drop`), and
//!   its HTTP response, whose close llama-server answers with `srv stop: cancel task`. The
//!   capture lease rides inside the generation future, so its `Drop` records the call as
//!   cancelled at the same moment, never while the engine still runs.
//! - **The agent is slow:** the generation is not polled until the frames of the last chunk
//!   are taken, and every producer on the local serving path yields after each chunk it sends
//!   (`continuum-engine` `ai/stream_sinks.rs`, `deliver`), so one poll sends at most one
//!   chunk. The gateway holds that chunk and the frames it encodes to, nothing more. What
//!   else waits is the engine's own and is bytes, not chunks: the unparsed rest of its last
//!   network read (the adapter's SSE buffer, hyper's read buffer). Behind that the engine
//!   connection's receive window stops llama-server writing, and the agent's socket and
//!   hyper's write buffer push back on this body. The engine's sink type is unbounded
//!   (`AIProviderAdapter::generate_stream`); the bound comes from `deliver`, so a producer
//!   that sent around it would break it. Pinned against the real producer by
//!   `continuum-engine` `ai/openai_adapter/tests/a_ready_burst_*` and here by
//!   `a_stalled_agent_stops_a_ready_burst_at_one_chunk_ahead`.
//! - **The node stops:** `abandoned` ends an answer that is being read, with the reason. An
//!   agent that stopped reading never polls this body again, so the stop also cuts its
//!   connection (`crate::sever`): hyper drops the body and the generation with it.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use continuum_engine::ai::adapter::GenerationChunk;
use continuum_engine::ai::TextGenerationResponse;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::route::RouteError;
use crate::wire::{Frame, Wire};

type Generation = Pin<Box<dyn Future<Output = Result<TextGenerationResponse, RouteError>> + Send>>;

/// One streamed answer's frames, in order: what the format opens with, each chunk as the
/// engine decodes it, then what closes it. Ends after the closing frames.
pub(crate) struct Answer<W: Wire> {
    encoder: W,
    /// `None` once it has returned (its closing frames are queued).
    generation: Option<Generation>,
    /// The engine's sink for this generation (its streaming contract is an unbounded sender);
    /// emptied before the generation is polled again, and one poll sends at most one chunk
    /// (`stream_sinks::deliver`), so it holds one chunk at most.
    received: UnboundedReceiver<GenerationChunk>,
    /// The frames of one chunk (or the opening or closing), until the agent takes them.
    encoded: VecDeque<Frame>,
    /// Resolves when the module stops the gateway (`crate::listener`); polled only while the
    /// generation runs.
    abandoned: Pin<Box<dyn Future<Output = ()> + Send>>,
}

impl<W: Wire> Answer<W> {
    /// `generation` sends its chunks to the sender `received` drains, and owns everything that
    /// must end with it (the engine request and its admission, the capture lease). When
    /// `abandoned` resolves first, the generation is dropped and the answer closes as failed.
    pub(crate) fn new(
        mut encoder: W,
        generation: impl Future<Output = Result<TextGenerationResponse, RouteError>> + Send + 'static,
        received: UnboundedReceiver<GenerationChunk>,
        abandoned: impl Future<Output = ()> + Send + 'static,
    ) -> Self {
        let encoded = encoder.opened().into();
        Self {
            encoder,
            generation: Some(Box::pin(generation)),
            received,
            encoded,
            abandoned: Box::pin(abandoned),
        }
    }

    /// Encode every chunk the generation already sent.
    fn encode_received(&mut self) {
        while let Ok(chunk) = self.received.try_recv() {
            self.encoded.extend(self.encoder.chunk(&chunk));
        }
    }

    /// The frames that close an answer the generation returned.
    fn closing(&mut self, outcome: Result<TextGenerationResponse, RouteError>) -> Vec<Frame> {
        match outcome {
            Ok(response) => self.encoder.finished(&response),
            Err(RouteError::Engine(why)) => self.encoder.failed(&why),
            Err(RouteError::UnknownModel { model, .. }) => self
                .encoder
                .failed(&format!("model `{model}` is not served here")),
        }
    }
}

impl<W: Wire> tokio_stream::Stream for Answer<W> {
    type Item = Frame;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Frame>> {
        let this = self.get_mut();
        loop {
            if let Some(frame) = this.encoded.pop_front() {
                return Poll::Ready(Some(frame));
            }
            match this.received.try_recv() {
                Ok(chunk) => {
                    this.encoded.extend(this.encoder.chunk(&chunk));
                    continue;
                }
                // Empty: nothing decoded is waiting. Disconnected: the generation returned and
                // dropped its sender; every chunk it sent was taken above.
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => {}
            }
            if this.generation.is_some() && this.abandoned.as_mut().poll(cx).is_ready() {
                // The node is stopping. Dropping the generation is the engine's one
                // cancellation path, and its lease records the call as cancelled.
                this.generation = None;
                this.encode_received();
                this.encoded
                    .extend(this.encoder.failed(crate::stop::STOPPING));
                continue;
            }
            let Some(generation) = this.generation.as_mut() else {
                return Poll::Ready(None);
            };
            match generation.as_mut().poll(cx) {
                // It may have sent chunks before pending: the loop encodes them first.
                Poll::Pending if this.received.is_empty() => return Poll::Pending,
                Poll::Pending => {}
                Poll::Ready(outcome) => {
                    this.generation = None;
                    // Its last chunks precede its closing frames.
                    this.encode_received();
                    let closing = this.closing(outcome);
                    this.encoded.extend(closing);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::chat::Chat;
    use continuum_engine::ai::stream_sinks::deliver;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::mpsc::UnboundedSender;
    use tokio::sync::{OwnedSemaphorePermit, Semaphore};
    use tokio_stream::Stream;

    /// Poll the body once, as hyper does when it can write.
    fn poll(answer: &mut Answer<Chat>) -> Poll<Option<Frame>> {
        let mut cx = Context::from_waker(std::task::Waker::noop());
        Pin::new(answer).poll_next(&mut cx)
    }

    /// A generation shaped as the engine's: it holds a slot permit for its whole life (the
    /// adapter's `TurnAdmission`), sends `chunks` tokens through the engine's `deliver` with
    /// nothing else between them (a READY burst: every token already decoded, as one network
    /// read of many events is), counting each, then waits forever (a long decode); the node
    /// stops when `abandoned` resolves. Returns the answer, the permit's pool and the sent
    /// count.
    fn stopping_generation(
        chunks: usize,
        abandoned: impl Future<Output = ()> + Send + 'static,
    ) -> (Answer<Chat>, Arc<Semaphore>, Arc<AtomicUsize>) {
        let slots = Arc::new(Semaphore::new(1));
        let permit: OwnedSemaphorePermit = slots
            .clone()
            .try_acquire_owned()
            .expect("test: one free slot");
        let sent = Arc::new(AtomicUsize::new(0));
        let (sink, received): (UnboundedSender<GenerationChunk>, _) =
            tokio::sync::mpsc::unbounded_channel();
        let counted = sent.clone();
        let generation = async move {
            let _admission = permit;
            for i in 0..chunks {
                counted.fetch_add(1, Ordering::SeqCst);
                deliver(&sink, GenerationChunk::Token(format!("t{i}"))).await;
            }
            std::future::pending::<()>().await;
            Err(RouteError::Engine("test: unreachable".into()))
        };
        (
            Answer::new(
                Chat::streamer("qwen", "chatcmpl_1"),
                generation,
                received,
                abandoned,
            ),
            slots,
            sent,
        )
    }

    /// The same, on a node that keeps serving.
    fn generation(chunks: usize) -> (Answer<Chat>, Arc<Semaphore>, Arc<AtomicUsize>) {
        stopping_generation(chunks, std::future::pending())
    }

    // what this catches: an agent that leaves during prefill (before any token) leaving its
    // generation running and its slot held, as the spawned generation of 34672ec73 did.
    #[tokio::test]
    async fn an_agent_gone_before_the_first_chunk_cancels_the_generation_and_frees_its_slot() {
        let (mut answer, slots, sent) = generation(0);
        assert!(
            matches!(poll(&mut answer), Poll::Ready(Some(_))),
            "the opening frame"
        );
        assert!(
            poll(&mut answer).is_pending(),
            "prefilling: nothing to send yet"
        );
        assert_eq!(
            slots.available_permits(),
            0,
            "the generation holds its slot while it runs"
        );
        drop(answer);
        assert_eq!(
            slots.available_permits(),
            1,
            "the agent left: the generation was dropped with its slot"
        );
        assert_eq!(sent.load(Ordering::SeqCst), 0);
    }

    // what this catches: an agent that leaves midstream leaving the decode running to its end.
    #[tokio::test]
    async fn an_agent_gone_midstream_cancels_the_generation_and_frees_its_slot() {
        let (mut answer, slots, sent) = generation(3);
        let mut frames = 0;
        while let Poll::Ready(Some(_)) = poll(&mut answer) {
            frames += 1;
        }
        assert_eq!(frames, 4, "the opening frame and three tokens");
        assert_eq!(slots.available_permits(), 0, "still decoding");
        drop(answer);
        assert_eq!(
            slots.available_permits(),
            1,
            "the agent left: the slot is free"
        );
        assert_eq!(sent.load(Ordering::SeqCst), 3);
    }

    // what this catches: a slow agent letting generation run ahead and pile its answer up in
    // memory (the unbounded chunk and event queues of 34672ec73), including when the engine
    // has many chunks READY at once (one network read of many events) and nothing yields
    // between them but the engine's own `deliver`.
    #[tokio::test]
    async fn a_stalled_agent_stops_a_ready_burst_at_one_chunk_ahead() {
        let (mut answer, _slots, sent) = generation(10_000);
        for _ in 0..3 {
            assert!(matches!(poll(&mut answer), Poll::Ready(Some(_))));
        }
        let taken = 2; // the opening frame, then two tokens
        assert!(
            answer.received.len() <= 1,
            "the gateway holds at most one chunk"
        );
        // The agent stops reading. Nothing else may drive the generation.
        for _ in 0..1_000 {
            tokio::task::yield_now().await;
        }
        let ahead = sent.load(Ordering::SeqCst) - taken;
        assert!(
            ahead <= 1,
            "generation ran {ahead} chunks ahead of a stalled agent"
        );
    }

    // what this catches: a finished generation's last chunks arriving after its closing frames,
    // or the stream never ending.
    #[tokio::test]
    async fn a_finished_answer_sends_its_last_chunks_then_closes_and_ends() {
        let (sink, received) = tokio::sync::mpsc::unbounded_channel();
        let generation = async move {
            sink.send(GenerationChunk::Token("done".into()))
                .map_err(|_| RouteError::Engine("test: closed".into()))?;
            Err(RouteError::Engine("lane died".into()))
        };
        let mut answer = Answer::new(
            Chat::streamer("qwen", "chatcmpl_1"),
            generation,
            received,
            std::future::pending(),
        );
        let mut frames = Vec::new();
        while let Poll::Ready(Some(frame)) = poll(&mut answer) {
            frames.push(frame.data);
        }
        assert!(poll(&mut answer).is_ready(), "ended, not pending");
        assert!(
            frames[1].contains("done"),
            "the token before the close: {frames:?}"
        );
        assert!(
            frames[2..].iter().any(|f| f.contains("lane died")),
            "the failure closes it: {frames:?}"
        );
    }

    // what this catches: a gateway stop that leaves an answer in flight decoding on (its
    // engine slot held, its agent waiting) after the module let the listener go, or that
    // cuts the stream without telling the agent why.
    #[tokio::test]
    async fn a_stopping_node_ends_an_answer_in_flight_and_frees_its_slot() {
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let abandoned = async move {
            // Err: the test dropped its sender, which stops the node all the same.
            match stopped.await {
                Ok(()) | Err(_) => {}
            }
        };
        let (mut answer, slots, _sent) = stopping_generation(1, abandoned);
        assert!(
            matches!(poll(&mut answer), Poll::Ready(Some(_))),
            "the opening frame"
        );
        assert!(
            matches!(poll(&mut answer), Poll::Ready(Some(_))),
            "the token"
        );
        assert!(poll(&mut answer).is_pending(), "decoding on");
        assert_eq!(slots.available_permits(), 0);
        stop.send(()).expect("test: the answer is listening");
        let mut closing = Vec::new();
        while let Poll::Ready(Some(frame)) = poll(&mut answer) {
            closing.push(frame.data);
        }
        assert_eq!(
            slots.available_permits(),
            1,
            "stopped: the generation and its slot are gone"
        );
        assert!(
            closing.iter().any(|f| f.contains(crate::stop::STOPPING)),
            "the agent is told why: {closing:?}"
        );
        assert!(poll(&mut answer).is_ready(), "the answer ended");
    }
}
