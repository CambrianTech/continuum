//! Emit: an answer's tokens become text on their own worker, never on the engine thread
//! (CONTINUUM-ENGINE 4.16). The engine pushes what it decoded and goes straight on to its next step;
//! this stage turns each token into its piece, streams it to the answer's caller, and watches for
//! the answer's stop sequences. A stop it sees (or a caller that hung up) raises the answer's cancel
//! flag, which the engine reads at its next step without a lock or a wait; tokens decoded after
//! that are not shown. Everything for one answer arrives in order on one queue, so its end is
//! always delivered after its last piece.

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

/// An answer's emitted text so far, and where it goes.
struct Open {
    sink: UnboundedSender<TokenEvent>,
    stops: Arc<[String]>,
    cancel: Arc<AtomicBool>,
    text: String,
}

pub(super) struct Emit {
    model: Arc<Model>,
    open: HashMap<AnswerSeq, Open>,
}

impl Emit {
    pub(super) fn new(model: Arc<Model>) -> Self {
        Self { model, open: HashMap::new() }
    }
}

impl Stage for Emit {
    type Input = Out;
    const QOS: Qos = Qos::Interactive;

    async fn handle(&mut self, out: Out) {
        match out {
            Out::Open { answer, sink, stops, cancel } => {
                self.open.insert(answer, Open { sink, stops, cancel, text: String::new() });
            }
            Out::Token { answer, token } => {
                let Some(open) = self.open.get_mut(&answer) else { return };
                if open.cancel.load(Ordering::Acquire) {
                    return; // stopped: tokens the engine decoded before it saw the flag are not shown
                }
                let piece = self.model.token_to_piece(token);
                open.text.push_str(&piece);
                let hung_up = open.sink.send(TokenEvent::Token(piece)).is_err();
                if hung_up || open.stops.iter().any(|stop| open.text.ends_with(stop.as_str())) {
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
