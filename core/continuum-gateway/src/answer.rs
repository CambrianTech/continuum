//! A streamed answer: the SSE body OWNS the generation, so the agent's connection is the
//! generation's lifetime.
//!
//! Codex on #5541 (34672ec73): a generation spawned beside the body outlived a departed agent
//! (the engine request and its lane kept running) while its capture said cancelled, and two
//! unbounded queues let a slow agent accumulate a whole answer in memory. Here there is no
//! task and no gateway queue. The body polls the generation itself, and only when the agent
//! has taken every frame already encoded:
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
//!   are taken, so the engine reads no further from its stream. What waits is bounded by the
//!   engine's own stream batching unit: the chunks ONE poll of the generation produced (one
//!   read of the engine's SSE connection; llama-server sends one token per event), plus the
//!   frames one chunk encodes to. Behind that, the agent's socket and hyper's write buffer
//!   push back on the body, and the engine's connection's receive window on llama-server.

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
    /// emptied before the generation is polled again, so it holds one poll's chunks at most.
    received: UnboundedReceiver<GenerationChunk>,
    /// The frames of one chunk (or the opening or closing), until the agent takes them.
    encoded: VecDeque<Frame>,
}

impl<W: Wire> Answer<W> {
    /// `generation` sends its chunks to the sender `received` drains, and owns everything that
    /// must end with it (the engine request and its admission, the capture lease).
    pub(crate) fn new(
        mut encoder: W,
        generation: impl Future<Output = Result<TextGenerationResponse, RouteError>> + Send + 'static,
        received: UnboundedReceiver<GenerationChunk>,
    ) -> Self {
        let encoded = encoder.opened().into();
        Self {
            encoder,
            generation: Some(Box::pin(generation)),
            received,
            encoded,
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
                    while let Ok(chunk) = this.received.try_recv() {
                        this.encoded.extend(this.encoder.chunk(&chunk));
                    }
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

    /// Pending exactly once, then ready: an engine step between two chunks.
    async fn step() {
        let mut stepped = false;
        std::future::poll_fn(|cx| {
            if stepped {
                return Poll::Ready(());
            }
            stepped = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        })
        .await
    }

    /// A generation shaped as the engine's: it holds a slot permit for its whole life (the
    /// adapter's `TurnAdmission`), sends `chunks` tokens counting each, then waits forever (a
    /// long decode). Returns the answer, the permit's pool and the sent count.
    fn generation(chunks: usize) -> (Answer<Chat>, Arc<Semaphore>, Arc<AtomicUsize>) {
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
                if sink.send(GenerationChunk::Token(format!("t{i}"))).is_err() {
                    return Err(RouteError::Engine("test: sink closed".into()));
                }
                counted.fetch_add(1, Ordering::SeqCst);
                step().await;
            }
            std::future::pending::<()>().await;
            Err(RouteError::Engine("test: unreachable".into()))
        };
        (
            Answer::new(Chat::streamer("qwen", "chatcmpl_1"), generation, received),
            slots,
            sent,
        )
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
    // memory (the unbounded chunk and event queues of 34672ec73).
    #[tokio::test]
    async fn a_stalled_agent_stops_generation_at_one_chunk_ahead() {
        let (mut answer, _slots, sent) = generation(10_000);
        for _ in 0..3 {
            assert!(matches!(poll(&mut answer), Poll::Ready(Some(_))));
        }
        let taken = 2; // the opening frame, then two tokens
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
        let mut answer = Answer::new(Chat::streamer("qwen", "chatcmpl_1"), generation, received);
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
}
