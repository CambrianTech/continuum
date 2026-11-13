//! The learning event stream (card 7fc7e2ce): a learning mutation the core would otherwise wait out
//! behind the work it controls (a 2,048-token prefill chunk ran ~40 s on a CPU node while the core
//! waited 4 s, so every learner stop read as unreachable) is posted as a TICKET that the engine
//! answers at once, and its terminal result is pushed on `GET /learn/events`. One subscription per
//! engine incarnation; a mutation waits on its ticket's event, never on a poll or a timer.
//!
//! Custody (Codex on the card): emission is not consumption, and a dropped stream is not the
//! engine's death. A ticket whose result did not arrive is [`TicketOutcome::Unknown`], which the
//! caller treats as unconfirmed (custody keeps what it held, and the physical grant, adapter or
//! residency is read again under the same incarnation), never as success.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use futures::StreamExt;
#[cfg(any(test, feature = "test-fixtures"))]
use axum::response::IntoResponse;

use crate::inference::engine_residency::EngineIncarnation;
use crate::inference::sse_frames::{data_lines, SseFrames};

/// A ticketed mutation's terminal result, as the engine pushed it.
#[derive(Debug, Clone, PartialEq)]
pub enum TicketOutcome {
    /// The engine completed it; its reply, exactly as the untagged route would have answered.
    Done(serde_json::Value),
    /// The engine refused or failed it; its error, exactly as the untagged route would have answered.
    Failed(serde_json::Value),
    /// The result never reached this core (the stream ended or dropped it): unconfirmed.
    Unknown(String),
}

#[derive(Default)]
struct Pending {
    /// Handles waiting for their ticket's result.
    waiting: HashMap<i64, tokio::sync::oneshot::Sender<TicketOutcome>>,
    /// Results that reached the stream before their handle registered (the engine answers the
    /// ticket and may finish the task before the HTTP reply carrying its id is read). Each is
    /// claimed within the same call that posted it; all go when the stream ends.
    early: HashMap<i64, TicketOutcome>,
    /// Why the stream ended, once it has: a handle that asks afterwards is answered at once.
    ended: Option<String>,
}

/// One engine incarnation's learning event stream, shared by every ticket posted to it.
pub(crate) struct LearnEvents {
    pending: parking_lot::Mutex<Pending>,
}

impl LearnEvents {
    /// The subscription for `engine`, opened if none is live: returns only once the engine has
    /// confirmed the subscription, so a ticket posted after it can never be dropped for want of one.
    pub(crate) async fn of(engine: EngineIncarnation, client: &reqwest::Client) -> Result<Arc<Self>, String> {
        if let Some(live) = streams().lock().get(&engine) {
            return Ok(Arc::clone(live));
        }
        let response = client
            .get(format!("{}/learn/events", engine.root_url()))
            .send()
            .await
            .map_err(|e| format!("the learning event stream did not open: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("the learning event stream was refused: {}", response.status()));
        }
        let mut body = response.bytes_stream();
        let mut frames = SseFrames::default();
        // the engine's first event confirms the subscription (and names the incarnation it serves)
        loop {
            let Some(chunk) = body.next().await else {
                return Err("the learning event stream ended before it subscribed".to_owned());
            };
            frames.push(&chunk.map_err(|e| format!("the learning event stream broke before it subscribed: {e}"))?);
            if let Some(event) = frames.next_event() {
                if data_lines(&event).any(|data| data.contains("\"subscribed\":true")) {
                    break;
                }
                return Err(format!("the learning event stream opened without subscribing: {event}"));
            }
        }
        let events = Arc::new(Self { pending: parking_lot::Mutex::new(Pending::default()) });
        streams().lock().insert(engine, Arc::clone(&events));
        let mut reader = Subscription {
            engine,
            events: Arc::clone(&events),
            why: "the learning event stream's reader was cancelled with the runtime that ran it".to_owned(),
        };
        tokio::spawn(async move {
            reader.why = reader.events.read(body, frames).await;
            continuum_base::probe!(class = "learn.events.ended", engine = ?engine, reason = %reader.why,
                "the learning event stream ended: every waiting ticket is unknown, the next mutation resubscribes");
        });
        Ok(events)
    }

    /// The ticket's result: one already pushed, or the next one pushed for it, or Unknown when the
    /// stream ends first.
    pub(crate) async fn outcome(&self, ticket: i64) -> TicketOutcome {
        let receiver = {
            let mut pending = self.pending.lock();
            if let Some(early) = pending.early.remove(&ticket) {
                return early;
            }
            if let Some(why) = &pending.ended {
                return TicketOutcome::Unknown(why.clone());
            }
            let (sender, receiver) = tokio::sync::oneshot::channel();
            pending.waiting.insert(ticket, sender);
            receiver
        };
        match receiver.await {
            Ok(outcome) => outcome,
            Err(_) => TicketOutcome::Unknown("the learning event stream ended before this ticket's result".to_owned()),
        }
    }

    /// Route every pushed result until the stream ends; the reason it ended.
    async fn read<S, B>(&self, mut body: S, mut frames: SseFrames) -> String
    where
        S: futures::Stream<Item = Result<B, reqwest::Error>> + Unpin,
        B: AsRef<[u8]>,
    {
        loop {
            while let Some(event) = frames.next_event() {
                for data in data_lines(&event) {
                    match serde_json::from_str::<Pushed>(data) {
                        Ok(pushed) => self.deliver(pushed),
                        Err(error) => continuum_base::probe!(class = "learn.events.unparsed", error = %error, data = %data,
                            "a learning event the core cannot read; its ticket, if any, stays unknown"),
                    }
                }
            }
            match body.next().await {
                Some(Ok(chunk)) => frames.push(chunk.as_ref()),
                Some(Err(error)) => return format!("the learning event stream broke: {error}"),
                None => return "the engine closed the learning event stream".to_owned(),
            }
        }
    }

    fn deliver(&self, pushed: Pushed) {
        let outcome = if pushed.error { TicketOutcome::Failed(pushed.result) } else { TicketOutcome::Done(pushed.result) };
        let mut pending = self.pending.lock();
        match pending.waiting.remove(&pushed.ticket) {
            // a handle that stopped waiting (its caller was cancelled) leaves the result unread
            Some(waiting) => drop(waiting.send(outcome)),
            None => drop(pending.early.insert(pushed.ticket, outcome)),
        }
    }

    fn end(&self, why: &str) {
        let mut pending = self.pending.lock();
        pending.ended = Some(why.to_owned());
        pending.early.clear();
        for (_, waiting) in pending.waiting.drain() {
            drop(waiting.send(TicketOutcome::Unknown(why.to_owned())));
        }
    }
}

/// A live subscription's reader: however it stops, the stream closing or its task cancelled with
/// the runtime that ran it, its drop retires the subscription. Cleanup written after the read's
/// `await` never runs for a cancelled task, which left a dead subscription in the map and every
/// later ticket waiting on it forever (the IntelMac's live-engine run, 2026-11-11 18:13Z: the
/// second test seated a learner, posted its ticket and waited an hour at 0% CPU).
struct Subscription {
    engine: EngineIncarnation,
    events: Arc<LearnEvents>,
    why: String,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        {
            let mut streams = streams().lock();
            // a newer subscription for the same incarnation is not this one's to retire
            if streams.get(&self.engine).is_some_and(|live| Arc::ptr_eq(live, &self.events)) {
                streams.remove(&self.engine);
            }
        }
        self.events.end(&self.why);
    }
}

/// One pushed event: `{ticket, error, result}`.
#[derive(serde::Deserialize)]
struct Pushed {
    ticket: i64,
    error: bool,
    result: serde_json::Value,
}

/// The live subscription per engine incarnation.
fn streams() -> &'static parking_lot::Mutex<HashMap<EngineIncarnation, Arc<LearnEvents>>> {
    static STREAMS: OnceLock<parking_lot::Mutex<HashMap<EngineIncarnation, Arc<LearnEvents>>>> = OnceLock::new();
    STREAMS.get_or_init(Default::default)
}

/// A fake engine that tickets as the real one does (fork #112), for tests of anything that stops a
/// learner: a POST carrying `"ticket": true` is answered `{ticket}` at once, the wrapped route runs,
/// and its reply is pushed as that ticket's event on `GET /learn/events`. Untagged requests pass
/// through unchanged.
#[cfg(any(test, feature = "test-fixtures"))]
pub fn ticketing(app: axum::Router) -> axum::Router {
    use axum::body::{Body, Bytes};
    let (events, _) = tokio::sync::broadcast::channel::<String>(1024);
    let tickets = Arc::new(std::sync::atomic::AtomicI64::new(1));
    let stream_of = events.clone();
    let stream = axum::routing::get(move || {
        let receiver = stream_of.subscribe();
        async move {
            let first = futures::stream::once(async { Ok::<Bytes, std::convert::Infallible>(Bytes::from("data: {\"subscribed\":true}\n\n")) });
            let rest = futures::stream::unfold(receiver, |mut receiver| async move {
                match receiver.recv().await {
                    Ok(event) => Some((Ok(Bytes::from(event)), receiver)),
                    Err(_) => None,
                }
            });
            axum::response::Response::builder()
                .header("content-type", "text/event-stream")
                .body(Body::from_stream(first.chain(rest)))
                .expect("test: an event stream response") // test fixture
        }
    });
    let ticketed = axum::middleware::from_fn(move |request: axum::extract::Request, next: axum::middleware::Next| {
        let (events, tickets) = (events.clone(), Arc::clone(&tickets));
        async move {
            let (parts, body) = request.into_parts();
            let bytes = axum::body::to_bytes(body, usize::MAX).await.expect("test: a request body"); // test fixture
            let flagged = serde_json::from_slice::<serde_json::Value>(&bytes).is_ok_and(|body| body.get("ticket") == Some(&serde_json::Value::Bool(true)));
            let request = axum::extract::Request::from_parts(parts, Body::from(bytes));
            if !flagged {
                return next.run(request).await;
            }
            let ticket = tickets.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            tokio::spawn(async move {
                let reply = next.run(request).await;
                let ok = reply.status().is_success();
                let body = axum::body::to_bytes(reply.into_body(), usize::MAX).await.expect("test: a reply body"); // test fixture
                let result: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null); // test fixture: a non-JSON reply pushes null
                drop(events.send(format!("data: {}\n\n", serde_json::json!({ "ticket": ticket, "error": !ok, "result": result }))));
            });
            axum::Json(serde_json::json!({ "ticket": ticket })).into_response()
        }
    });
    app.route("/learn/events", stream).layer(ticketed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pushed(ticket: i64, error: bool) -> Pushed {
        Pushed { ticket, error, result: serde_json::json!({ "ticket_was": ticket }) }
    }

    // what this catches (Codex on card 7fc7e2ce): a result that reaches the stream before the HTTP
    // reply carrying its ticket id lost, a refusal read as success, or a waiter left hanging (or
    // told "success") when the stream ends.
    #[tokio::test]
    async fn every_ticket_gets_its_result_or_an_unknown_never_a_guess() {
        let events = LearnEvents { pending: parking_lot::Mutex::new(Pending::default()) };
        events.deliver(pushed(7, false)); // before its handle registered
        assert_eq!(events.outcome(7).await, TicketOutcome::Done(serde_json::json!({ "ticket_was": 7 })), "an early result is held for its handle");
        events.deliver(pushed(8, true));
        assert!(matches!(events.outcome(8).await, TicketOutcome::Failed(_)), "a refusal is a failure, never success");
        let waiting = events.outcome(9);
        tokio::pin!(waiting);
        assert!(futures::poll!(&mut waiting).is_pending(), "no result yet: the handle waits on the event");
        events.end("the engine closed the learning event stream");
        assert!(matches!(waiting.await, TicketOutcome::Unknown(_)), "the stream ended: unconfirmed, never success");
    }

    // what this catches: a reader cancelled with the runtime that ran it (one test's runtime, or a
    // core shutting down) leaving a dead subscription in the process-wide map, so every later
    // ticket waits on it forever: the IntelMac's live-engine run hung an hour this way, 2026-11-11.
    #[test]
    fn a_reader_cancelled_with_its_runtime_retires_its_subscription() {
        let engine = EngineIncarnation { pid: 4243, started_s: 1_793_000_001, port: 1 };
        let events = Arc::new(LearnEvents { pending: parking_lot::Mutex::new(Pending::default()) });
        streams().lock().insert(engine, Arc::clone(&events));
        let reader = Subscription { engine, events: Arc::clone(&events), why: "cancelled".to_owned() };
        let readers = tokio::runtime::Builder::new_current_thread().build().expect("test: a runtime"); // test fixture
        readers.spawn(async move {
            let _reader = reader;
            std::future::pending::<()>().await
        });
        readers.block_on(tokio::task::yield_now()); // the reader is running
        let waiters = tokio::runtime::Builder::new_current_thread().enable_time().build().expect("test: a runtime"); // test fixture
        let mut waiting = Box::pin(events.outcome(9));
        assert!(waiters.block_on(async { futures::poll!(&mut waiting) }).is_pending(), "no result yet");
        drop(readers);
        assert!(!streams().lock().contains_key(&engine), "the dead subscription left the map: the next ticket resubscribes");
        // derived-or-floor: a bound on a test that hung for an hour; the answer is immediate
        let bound = std::time::Duration::from_secs(10);
        let answered = waiters.block_on(async { tokio::time::timeout(bound, waiting).await }).expect("the waiter is answered, not left hanging");
        assert!(matches!(answered, TicketOutcome::Unknown(_)), "a cancelled reader's tickets are unknown, never success");
        assert!(matches!(waiters.block_on(events.outcome(10)), TicketOutcome::Unknown(_)), "a handle asking after the end is answered at once");
    }
}
