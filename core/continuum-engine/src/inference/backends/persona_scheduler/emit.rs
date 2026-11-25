//! Emit: an answer's tokens are streamed and watched on their own worker, never on the engine thread
//! (CONTINUUM-ENGINE 4.16). The engine pushes what it decoded and goes straight on to its next step;
//! this stage streams each token id to the answer's caller (who decodes the whole answer once) and watches for
//! the answer's stop sequences. A stop it sees (or a caller that hung up) raises the answer's cancel
//! flag, which the engine reads at its next step without a lock or a wait; tokens decoded after
//! that are not shown. Everything for one answer arrives in order on one queue, so its end is
//! always delivered after its last piece.
//!
//! Stops are sorted once when the answer opens. A stop that is one control token (`<|im_end|>`)
//! is watched by its id: its piece is no text, so it could never be seen as text. Every other stop
//! is watched as bytes on the answer's tail, kept only as long as the longest stop, so an answer
//! never grows a copy of itself to be watched (Joel via Fable, 2026-11-25). A text stop is never
//! turned into token ids: what precedes it changes how it tokenizes, so an id match would miss it.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use continuum_sage::{Qos, Stage};
use llama::Model;
use tokio::sync::mpsc::UnboundedSender;

use super::TokenEvent;

/// One answer being emitted, by the engine's count of answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct AnswerSeq(pub u64);

/// What the engine hands Emit.
pub(super) enum Out {
    /// An answer starts: where its pieces go, the sequences that end it, and the flag that tells
    /// the engine to stop decoding for it.
    Open { answer: AnswerSeq, sink: UnboundedSender<TokenEvent>, stops: Arc<[String]>, cancel: Arc<AtomicBool> },
    /// The next token the engine decoded for it.
    Token { answer: AnswerSeq, token: i32 },
    /// The answer ended (done or refused), after its last token.
    End { answer: AnswerSeq, event: TokenEvent },
}

/// An answer's stops, sorted when it opens.
#[derive(Debug, Default)]
struct Stops {
    /// Stops that are exactly one control token, watched by id.
    tokens: Box<[i32]>,
    /// The other stops, watched on the answer's tail bytes.
    texts: Box<[Box<[u8]>]>,
}

impl Stops {
    /// Sort `stops`: `control` names the one control token a stop is, or `None` for any other.
    fn of(stops: &[String], control: impl Fn(&str) -> Option<i32>) -> Self {
        let (mut tokens, mut texts) = (Vec::new(), Vec::new());
        for stop in stops.iter().filter(|stop| !stop.is_empty()) {
            match control(stop) {
                Some(token) => tokens.push(token),
                None => texts.push(stop.as_bytes().into()),
            }
        }
        Self { tokens: tokens.into(), texts: texts.into() }
    }

    /// The bytes of the longest text stop: all of the tail that can ever end with one.
    fn longest(&self) -> usize {
        self.texts.iter().map(|stop| stop.len()).max().unwrap_or(0) // unwrap_or: no text stop needs no tail
    }
}

/// The last bytes of an answer, held in one buffer sized when it opens.
#[derive(Debug)]
struct Tail {
    bytes: Box<[u8]>,
    len: usize,
}

impl Tail {
    fn with_capacity(capacity: usize) -> Self {
        Self { bytes: vec![0; capacity].into(), len: 0 }
    }

    /// Append `piece`, keeping only the newest bytes the buffer holds.
    fn push(&mut self, piece: &[u8]) {
        let capacity = self.bytes.len();
        if piece.len() >= capacity {
            self.bytes.copy_from_slice(&piece[piece.len() - capacity..]);
            self.len = capacity;
            return;
        }
        let keep = self.len.min(capacity - piece.len());
        self.bytes.copy_within(self.len - keep..self.len, 0);
        self.bytes[keep..keep + piece.len()].copy_from_slice(piece);
        self.len = keep + piece.len();
    }

    fn ends_with(&self, stop: &[u8]) -> bool {
        self.bytes[..self.len].ends_with(stop)
    }
}

/// An answer being emitted: where it goes, what ends it, its tail, and the buffer each token's
/// bytes are rendered into (only when a text stop needs them; it grows only to the longest piece).
struct Open {
    sink: UnboundedSender<TokenEvent>,
    stops: Stops,
    cancel: Arc<AtomicBool>,
    tail: Tail,
    piece: Vec<u8>,
}

impl Open {
    /// Whether `token`, whose piece was just added to the tail, ends the answer.
    fn stopped_by(&self, token: i32) -> bool {
        self.stops.tokens.contains(&token) || self.stops.texts.iter().any(|stop| self.tail.ends_with(stop))
    }
}

pub(super) struct Emit {
    model: Arc<Model>,
    open: HashMap<AnswerSeq, Open>,
}

impl Emit {
    pub(super) fn new(model: Arc<Model>) -> Self {
        Self { model, open: HashMap::new() }
    }

    /// The one control token `stop` is, if it is one.
    fn control_token(&self, stop: &str) -> Option<i32> {
        match self.model.tokenize(stop, false, true).as_deref() {
            Ok([token]) if self.model.is_control_token(*token) => Some(*token),
            _ => None,
        }
    }
}

impl Stage for Emit {
    type Input = Out;
    const QOS: Qos = Qos::Interactive;

    async fn handle(&mut self, out: Out) {
        match out {
            Out::Open { answer, sink, stops, cancel } => {
                let stops = Stops::of(&stops, |stop| self.control_token(stop));
                let tail = Tail::with_capacity(stops.longest());
                self.open.insert(answer, Open { sink, stops, cancel, tail, piece: Vec::new() });
            }
            Out::Token { answer, token } => {
                let Some(open) = self.open.get_mut(&answer) else { return };
                if open.cancel.load(Ordering::Acquire) {
                    return; // stopped: tokens the engine decoded before it saw the flag are not shown
                }
                // only an answer with a text stop renders its bytes, into its own buffer
                if !open.stops.texts.is_empty() {
                    match self.model.piece_into(token, &mut open.piece) {
                        Ok(piece) => open.tail.push(piece),
                        Err(refused) => continuum_base::probe!(class = "persona.engine.piece_refused", token, reason = %refused,
                            "a token's bytes were not rendered: its text stops are not checked on it"),
                    }
                }
                let hung_up = open.sink.send(TokenEvent::Token(token)).is_err();
                if hung_up || open.stopped_by(token) {
                    open.cancel.store(true, Ordering::Release);
                }
            }
            Out::End { answer, event } => {
                let ended = self.open.remove(&answer).map(|open| open.sink.send(event).is_ok());
                if ended == Some(false) {
                    continuum_base::probe!(class = "persona.engine.caller_gone", "a turn's caller hung up before its end");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
