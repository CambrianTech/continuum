//! `AircIpcTransport` — the canonical local-substrate `Transport` impl.
//!
//! Wraps an `Arc<airc_lib::Airc>` pointed at a continuum-core-server's
//! peer. Speaks the substrate's command-envelope wire shape via
//! `continuum-airc-protocol`, identical to what the server-side
//! `command_handler.rs` consumes — no wire drift possible because both
//! ends import the same types.
//!
//! Event protocol (subscribe → ack → deliver → unsubscribe) is wired
//! via the shared protocol-crate helpers `resolve_subscribe`,
//! `decode_subscribe_ack`, `decode_deliver_frame`, `matches_subscription`,
//! `resolve_unsubscribe`. Substrate's `AircEventTransport` composes the
//! same helpers — zero wire drift between client and substrate.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use airc_core::{Body, Headers, MentionTarget, PeerId, TranscriptEvent};
use airc_lib::Airc;
use airc_protocol::{HEADER_AIRC_CORRELATION_ID, HEADER_AIRC_REPLY_TO};
use continuum_airc_protocol::command::{
    AircCommandRequest, AircCommandResponse, COMMAND_REQUEST_BODY_HINT, COMMAND_RESPONSE_BODY_HINT,
    HEADER_COMMAND_ENV, HEADER_COMMAND_KIND, HEADER_COMMAND_PATH, HEADER_COMMAND_STATUS,
    HEADER_CONTINUUM_BODY_HINT, KIND_PEER,
};
use continuum_airc_protocol::event as event_proto;
use futures::StreamExt as _;
use uuid::Uuid;

use crate::error::ClientError;
use crate::event::EventStream;
use crate::transport::{ServeHandler, Transport};

/// Serve-side state for `provide`/`revoke`: the registry of handlers this
/// client serves, plus a one-time flag for the inbound serve loop. Shared
/// (`Arc`) so a clone of the transport sees the same registrations and the loop
/// dispatches against the live map.
#[derive(Default)]
struct ServeState {
    handlers: Mutex<HashMap<String, std::sync::Arc<dyn ServeHandler>>>,
    loop_started: AtomicBool,
}

/// Default in-flight event buffer for a single subscription.
///
/// 64 is the substrate-side default (`DEFAULT_DELIVERY_QUEUE_CAPACITY`
/// in routing/airc_event_transport.rs). A slow consumer applies
/// back-pressure on the per-subscription filter task once the channel
/// is full; the publisher continues delivering to other subscriptions.
const DEFAULT_DELIVERY_QUEUE_CAPACITY: usize = 64;

/// Default round-trip deadline. Re-export of the shared
/// `continuum_airc_protocol::DEFAULT_COMMAND_DEADLINE` so client and
/// substrate agree by import, not literal duplication. Override with
/// [`AircIpcTransport::with_deadline`] for long LLM generations.
pub use continuum_airc_protocol::command::DEFAULT_COMMAND_DEADLINE as DEFAULT_DEADLINE;

/// Local substrate transport over airc IPC.
///
/// Clone is cheap (one Arc + Copy for the PeerId/Duration/Atomic).
pub struct AircIpcTransport {
    airc: Arc<Airc>,
    target: PeerId,
    deadline: Duration,
    closed: Arc<AtomicBool>,
    serve: Arc<ServeState>,
}

impl std::fmt::Debug for AircIpcTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AircIpcTransport")
            .field("target", &self.target)
            .field("deadline", &self.deadline)
            .field("closed", &self.closed.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl AircIpcTransport {
    /// Build a transport against an existing airc handle + the
    /// substrate's peer UUID.
    pub fn new(airc: Arc<Airc>, target_peer: Uuid) -> Self {
        Self {
            airc,
            target: PeerId(target_peer),
            deadline: DEFAULT_DEADLINE,
            closed: Arc::new(AtomicBool::new(false)),
            serve: Arc::new(ServeState::default()),
        }
    }

    /// Replace the default deadline. Builder-style.
    pub fn with_deadline(mut self, deadline: Duration) -> Self {
        self.deadline = deadline;
        self
    }

    fn build_headers(request: &AircCommandRequest) -> airc_core::Headers {
        let mut headers = airc_core::Headers::new();
        headers.insert(HEADER_COMMAND_PATH.to_string(), request.path.clone());
        headers.insert(HEADER_COMMAND_KIND.to_string(), request.kind.clone());
        if let Some(env) = &request.env {
            headers.insert(HEADER_COMMAND_ENV.to_string(), env.clone());
        }
        headers.insert(
            HEADER_CONTINUUM_BODY_HINT.to_string(),
            COMMAND_REQUEST_BODY_HINT.to_string(),
        );
        headers
    }

    fn decode_reply(reply_body: Option<Body>) -> Result<Value, ClientError> {
        let reply_body = reply_body.ok_or_else(|| {
            ClientError::Transport(
                "reply has no body (peer-side handler must attach Body::Json)".to_string(),
            )
        })?;

        let response_value = match reply_body {
            Body::Json(v) => v,
            Body::Binary(_) => {
                return Err(ClientError::Transport(
                    "reply body was Binary; expected Json (AircCommandResponse is JSON)"
                        .to_string(),
                ));
            }
        };

        let response: AircCommandResponse = serde_json::from_value(response_value)?;

        response
            .into_result()
            .map_err(|message| ClientError::Refused {
                command: "<unknown>".to_string(),
                reason: message,
            })
    }
}

#[async_trait]
impl Transport for AircIpcTransport {
    async fn execute(&self, command: &str, params: Value) -> Result<Value, ClientError> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(ClientError::Closed);
        }

        // Client side always uses the "peer" route kind — we're
        // dispatching at a specific substrate peer over its IPC.
        let request =
            AircCommandRequest::new(command.to_string(), KIND_PEER.to_string(), None, params);

        let body_value = serde_json::to_value(&request)?;
        let body = Body::Json(body_value);
        let headers = Self::build_headers(&request);

        let pending = self
            .airc
            .request(
                MentionTarget::Peer(self.target),
                headers,
                body,
                self.deadline,
            )
            .await
            .map_err(|e| ClientError::Transport(format!("airc request failed: {e}")))?;

        let reply = self
            .airc
            .await_reply(pending)
            .await
            .map_err(|e| ClientError::Transport(format!("await_reply failed: {e}")))?;

        Self::decode_reply(reply.body).map_err(|e| match e {
            ClientError::Refused { reason, .. } => ClientError::Refused {
                command: command.to_string(),
                reason,
            },
            other => other,
        })
    }

    async fn subscribe(&self, topic: &str) -> Result<EventStream, ClientError> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(ClientError::Closed);
        }

        // ── 1. Open the airc event stream FIRST, before sending the
        //       peer-side subscribe request. Per substrate-side
        //       AircEventTransport's PR #1529 reviewer 1 BLOCK: opening
        //       the stream AFTER the ack creates a window where the
        //       peer accepts the subscription but the client hasn't
        //       armed the receiver, so early Deliver frames are
        //       dropped silently. Arm first; close-on-failure if the
        //       request itself fails.
        let event_stream = self
            .airc
            .subscribe()
            .await
            .map_err(|e| ClientError::Transport(format!("airc subscribe stream open: {e}")))?;

        // ── 2. Build & send the subscribe envelope via the shared
        //       protocol helpers. Same wire shape the substrate's
        //       AircEventPublisher accepts.
        let (target, headers, body) = event_proto::resolve_subscribe(self.target, topic, None)
            .map_err(ClientError::Transport)?;

        let pending = self
            .airc
            .request(target, headers, body, self.deadline)
            .await
            .map_err(|e| ClientError::Transport(format!("airc subscribe request: {e}")))?;

        let reply = self
            .airc
            .await_reply(pending)
            .await
            .map_err(|e| ClientError::Transport(format!("airc subscribe await_reply: {e}")))?;

        let ack = event_proto::decode_subscribe_ack(reply.body).map_err(ClientError::Transport)?;
        let subscription_id = ack.subscription_id;
        let publisher = self.target;

        // ── 3. Spawn the per-subscription filter task. Forwards
        //       payloads matching this subscription's id+publisher,
        //       exits cleanly on receiver-drop, and sends unsubscribe
        //       on exit so the peer-side state cleans up too.
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Value, ClientError>>(
            DEFAULT_DELIVERY_QUEUE_CAPACITY,
        );
        let airc_for_task = Arc::clone(&self.airc);
        let deadline = self.deadline;
        tokio::spawn(async move {
            use futures::StreamExt as _;
            let mut event_stream = event_stream;
            loop {
                let event = tokio::select! {
                    biased;
                    // Receiver dropped → consumer abandoned the
                    // stream. Exit promptly without waiting for the
                    // next airc frame; this closes the quiet-topic
                    // leak window the substrate's PR #1529 reviewer
                    // 2 BLOCK 3 identified.
                    _ = tx.closed() => break,
                    next = event_stream.next() => {
                        match next {
                            Some(Ok(e)) => e,
                            // Stream lag — substrate broadcasts a
                            // typed error; we skip and continue.
                            // Sequence gaps remain visible to the
                            // caller via AircEventDeliver.sequence.
                            Some(Err(_)) => continue,
                            // airc stream closed (daemon shutdown,
                            // wire teardown). Exit + unsubscribe.
                            None => break,
                        }
                    }
                };

                // Cheap demux: peer_id + body_hint + subscription_id.
                // Drops non-matching frames without parsing the body.
                if !event_proto::matches_subscription(&event, subscription_id, publisher) {
                    continue;
                }

                match event_proto::decode_deliver_frame(&event) {
                    Ok(deliver) => {
                        // Forward the payload to the consumer's
                        // EventStream. If send fails the receiver
                        // dropped — break out and unsubscribe.
                        if tx.send(Ok(deliver.payload)).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => {
                        // Frame passed the cheap demux but didn't
                        // decode. Either the publisher produced a
                        // malformed frame or there's a wire-shape
                        // skew somewhere. We drop the frame; the
                        // caller sees a sequence gap. The substrate
                        // doesn't surface decode errors to the
                        // consumer either (same behavior).
                    }
                }
            }

            // ── Unsubscribe on exit. Fire-and-await-ack so the
            //    peer-side subscription registry releases the
            //    entry. We're already in a shutdown path; if the
            //    unsubscribe itself fails (peer down, deadline,
            //    etc.) there's nothing useful to do — the
            //    subscription will time out peer-side eventually.
            if let Ok((target, headers, body)) =
                event_proto::resolve_unsubscribe(publisher, subscription_id)
            {
                if let Ok(pending) = airc_for_task.request(target, headers, body, deadline).await {
                    let _ = airc_for_task.await_reply(pending).await;
                }
            }
        });

        // ── 4. Return the EventStream wrapping the receiver. Drop on
        //       this stream cascades through the spawned task's
        //       tx.closed() → break → unsubscribe.
        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn emit(&self, class: &str, payload: Value) -> Result<(), ClientError> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(ClientError::Closed);
        }
        // Publish via the same shared protocol helper the substrate's
        // EventPublishAdapter receives — zero wire drift (the publish twin of
        // the subscribe path above).
        let (target, headers, body) = event_proto::resolve_publish(self.target, class, payload)
            .map_err(ClientError::Transport)?;
        let pending = self
            .airc
            .request(target, headers, body, self.deadline)
            .await
            .map_err(|e| ClientError::Transport(format!("airc publish request: {e}")))?;
        let reply = self
            .airc
            .await_reply(pending)
            .await
            .map_err(|e| ClientError::Transport(format!("airc publish await_reply: {e}")))?;
        // Decode the ack to surface a malformed reply; the fan-out count is
        // informational (emit fires into the fan-out — the caller doesn't branch
        // on how many subscribers received it).
        let _ack = event_proto::decode_publish_ack(reply.body).map_err(ClientError::Transport)?;
        Ok(())
    }

    async fn provide(
        &self,
        command: &str,
        handler: Arc<dyn ServeHandler>,
    ) -> Result<(), ClientError> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(ClientError::Closed);
        }
        self.serve
            .handlers
            .lock()
            .expect("serve handlers lock poisoned")
            .insert(command.to_string(), handler);
        // Start the inbound serve loop exactly once, on first provide — the
        // client-side twin of persona/command_inbound_pump. A second airc
        // subscribe is broadcast-shape (every subscriber gets every event), so
        // it coexists with the event-subscribe path without contention.
        if !self.serve.loop_started.swap(true, Ordering::Relaxed) {
            tokio::spawn(serve_loop(Arc::clone(&self.airc), Arc::clone(&self.serve)));
        }
        Ok(())
    }

    async fn revoke(&self, command: &str) -> Result<(), ClientError> {
        // Idempotent: removing an unprovided command is a no-op. The serve loop
        // keeps running (cheap) and simply ignores commands with no handler.
        self.serve
            .handlers
            .lock()
            .expect("serve handlers lock poisoned")
            .remove(command);
        Ok(())
    }

    async fn close(&self) -> Result<(), ClientError> {
        // Idempotent: first close wins, later calls return Closed.
        if self.closed.swap(true, Ordering::Relaxed) {
            return Err(ClientError::Closed);
        }
        // airc-lib's Airc handle is shared; dropping our Arc when this
        // transport goes out of scope releases our reference. Other
        // holders (the substrate, sibling transports) keep theirs.
        Ok(())
    }
}

// ── Serve side (provide) — the client-side twin of command_inbound_pump ──
//
// Mirrors core/continuum-core/src/routing/command_handler.rs, opposite role:
// instead of dispatching inbound commands to the local CommandExecutor, this
// dispatches to a handler the SDK PROVIDED, then replies the same
// `AircCommandResponse` wire (zero drift — same protocol types as `request`).

/// A decoded inbound command-request envelope, ready to dispatch + reply.
struct InboundCommand {
    path: String,
    params: Value,
    /// From `airc.reply_to` — where the response ships.
    reply_to: PeerId,
    /// From `airc.correlation_id` — pairs the reply with the caller's await.
    correlation_id: Uuid,
}

/// Decode an inbound command-request `TranscriptEvent`. `None` (skip) if it's
/// not a command request or is missing the reply addressing — never a panic.
fn parse_inbound_command(event: &TranscriptEvent) -> Option<InboundCommand> {
    let reply_to = PeerId(
        event
            .headers
            .get(HEADER_AIRC_REPLY_TO)?
            .parse::<Uuid>()
            .ok()?,
    );
    let correlation_id = event
        .headers
        .get(HEADER_AIRC_CORRELATION_ID)?
        .parse::<Uuid>()
        .ok()?;
    let Body::Json(value) = event.body.as_ref()? else {
        return None;
    };
    let request: AircCommandRequest = serde_json::from_value(value.clone()).ok()?;
    Some(InboundCommand {
        path: request.path,
        params: request.params,
        reply_to,
        correlation_id,
    })
}

/// Ship an `AircCommandResponse` back to the caller, stamping the same status +
/// body-hint headers the substrate's command_handler uses.
async fn send_serve_reply(
    airc: &Airc,
    reply_to: PeerId,
    correlation_id: Uuid,
    response: &AircCommandResponse,
) {
    let body = match serde_json::to_value(response) {
        Ok(v) => Body::Json(v),
        Err(e) => {
            eprintln!("continuum-client serve reply: serialize AircCommandResponse failed: {e}");
            return;
        }
    };
    let mut headers = Headers::new();
    headers.insert(
        HEADER_COMMAND_STATUS.to_string(),
        response.status_header_value().to_string(),
    );
    headers.insert(
        HEADER_CONTINUUM_BODY_HINT.to_string(),
        COMMAND_RESPONSE_BODY_HINT.to_string(),
    );
    if let Err(e) = airc.reply(reply_to, correlation_id, headers, body).await {
        eprintln!("continuum-client serve reply: airc.reply failed: {e}");
    }
}

/// The inbound serve loop: subscribe to the broadcast stream, and for each
/// command-request envelope whose path this client provides, dispatch to the
/// handler and reply. Dispatch is spawned per request so a slow handler (a
/// screenshot, a sensor read) doesn't stall the loop. Exits when the airc
/// stream ends (daemon disconnect) — per no-fallbacks, a lost subscribe ends
/// the loop loudly rather than silently degrading.
async fn serve_loop(airc: Arc<Airc>, serve: Arc<ServeState>) {
    let mut stream = match airc.subscribe().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("continuum-client serve loop: airc subscribe failed: {e}");
            return;
        }
    };
    while let Some(item) = stream.next().await {
        let event = match item {
            Ok(e) => e,
            Err(_) => continue, // stream lag — skip
        };
        // Only command-request envelopes; everything else (events, chat) skipped.
        if event
            .headers
            .get(HEADER_CONTINUUM_BODY_HINT)
            .map(String::as_str)
            != Some(COMMAND_REQUEST_BODY_HINT)
        {
            continue;
        }
        let Some(parsed) = parse_inbound_command(&event) else {
            continue;
        };
        // Only answer commands THIS client provides; others are for someone else.
        let handler = serve
            .handlers
            .lock()
            .expect("serve handlers lock poisoned")
            .get(&parsed.path)
            .cloned();
        let Some(handler) = handler else {
            continue;
        };
        let airc = Arc::clone(&airc);
        tokio::spawn(async move {
            let response = match handler.handle(parsed.params).await {
                Ok(value) => AircCommandResponse::ok(value),
                Err(e) => AircCommandResponse::error(e.to_string()),
            };
            send_serve_reply(&airc, parsed.reply_to, parsed.correlation_id, &response).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_headers_includes_path_kind_and_hint() {
        let req = AircCommandRequest::new(
            "ai/inference/generate".to_string(),
            "peer".to_string(),
            None,
            serde_json::json!({"prompt": "hi"}),
        );
        let headers = AircIpcTransport::build_headers(&req);
        assert_eq!(
            headers.get(HEADER_COMMAND_PATH).map(|s| s.as_str()),
            Some("ai/inference/generate")
        );
        assert_eq!(
            headers.get(HEADER_COMMAND_KIND).map(|s| s.as_str()),
            Some("peer")
        );
        assert_eq!(
            headers.get(HEADER_CONTINUUM_BODY_HINT).map(|s| s.as_str()),
            Some(COMMAND_REQUEST_BODY_HINT)
        );
        assert!(
            !headers.contains_key(HEADER_COMMAND_ENV),
            "no env should mean no env header"
        );
    }

    #[test]
    fn build_headers_adds_env_when_set() {
        let req = AircCommandRequest::new(
            "interface/screenshot".to_string(),
            "peer".to_string(),
            Some("vr".to_string()),
            Value::Null,
        );
        let headers = AircIpcTransport::build_headers(&req);
        assert_eq!(
            headers.get(HEADER_COMMAND_ENV).map(|s| s.as_str()),
            Some("vr")
        );
    }

    #[test]
    fn decode_reply_ok_returns_value() {
        let resp = AircCommandResponse::ok(serde_json::json!({"text": "hello"}));
        let body = Body::Json(serde_json::to_value(resp).unwrap());
        let decoded = AircIpcTransport::decode_reply(Some(body)).expect("decode");
        assert_eq!(decoded, serde_json::json!({"text": "hello"}));
    }

    #[test]
    fn decode_reply_error_returns_refused() {
        let resp = AircCommandResponse::error("policy denied");
        let body = Body::Json(serde_json::to_value(resp).unwrap());
        let err = AircIpcTransport::decode_reply(Some(body)).unwrap_err();
        match err {
            ClientError::Refused { reason, .. } => assert_eq!(reason, "policy denied"),
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[test]
    fn decode_reply_no_body_returns_transport_error() {
        let err = AircIpcTransport::decode_reply(None).unwrap_err();
        assert!(matches!(err, ClientError::Transport(_)));
    }

    #[test]
    fn decode_reply_binary_body_returns_transport_error() {
        let err = AircIpcTransport::decode_reply(Some(Body::Binary(vec![1, 2, 3]))).unwrap_err();
        assert!(matches!(err, ClientError::Transport(_)));
    }
}
