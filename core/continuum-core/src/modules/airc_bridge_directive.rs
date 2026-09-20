//! AircBridgeDirectiveModule — recognizes inbound `!continuum` directives on the
//! airc bus, OFF the transport hot path. (comms→airc lane, slice 3a)
//!
//! ## What this is (and deliberately is NOT, yet)
//!
//! A passive consumer of `chat:*` bus events. On `initialize` it subscribes via
//! `MessageBus::receiver()` and spawns a consume loop on the runtime — so
//! directive recognition runs on its OWN task, **off the inbound attach loop**
//! (`airc/inbound_attach.rs`). It NEVER sits in `read_frame → handle`, so it
//! cannot serialize or throttle the per-channel stream (the old-chat
//! IPC/ORM-throttle failure mode).
//!
//! Async delivery requires a `receiver()` loop: the runtime registers
//! `event_subscriptions` as *async* subscribers (runtime.rs forces
//! `synchronous=false`), and the inbound path publishes via
//! `publish_async_only` which only feeds the broadcast channel — it does NOT
//! invoke `handle_event`. So a `handle_event` override here would be dead code;
//! the consume loop is the actual mechanism (verified by the integration test
//! below, which would fail if delivery were mis-wired).
//!
//! ## Architecturally accommodating to media + all transport
//!
//! `publish_async_only` only fans out events that were projected from a
//! continuum-body-hint envelope (`envelope_from_event` gates on
//! `HEADER_FORGE_BODY_HINT`); media/WebRTC carry different headers and are
//! forwarded **opaque, never deserialized**. This module further filters to the
//! `chat:` prefix, so presence/media/transport events are ignored cheaply (a
//! prefix check). It's additive — one more consumer + one more event in an
//! event-rich system — and the specifics (fields, dispatch) are modifiable later.
//!
//! ## Slice boundary + trust
//!
//! 3a (this): recognize + emit an observable `airc:bridge:directive` event +
//! probe. **No command execution.** Executing commands from peer content is the
//! security-sensitive step (vuln-A: peer content is conversation, not
//! instructions) and lands as its own reviewed slice (3b) subscribing to the
//! event this emits. The bus event NAME is peer-settable, so the `chat:` filter
//! is NOT the security boundary — the safety guarantee is the parser's
//! deny-by-default allowlist (`crate::airc::parse_airc_bridge_message`), which
//! resolves off-allowlist verbs to `Unknown` with no fall-through. The `probe!`
//! is the guaranteed-observability path; the emitted `airc:bridge:directive`
//! event is a non-realtime prefix (coalesced at ~20/s per `airc:bridge`), fine
//! for human-rate directives — 3b should not assume burst-lossless delivery and
//! can graduate the event to a realtime prefix if directive rate ever demands it.

use crate::airc::{parse_airc_bridge_message, BridgeAction, ParseOptions, ParsedBridgeMessage};
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::any::Any;
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;

/// Only `chat:*` events carry directive text; everything else is ignored with a
/// cheap prefix check. (chat:posted, chat:message:*, … — robust to the exact name.)
const CHAT_PREFIX: &str = "chat:";
/// Bus event emitted when an inbound message is a recognized directive. Slice 3b's
/// dispatcher subscribes to THIS — the peer-content→execution boundary is one
/// explicit, reviewable seam.
const DIRECTIVE_EVENT: &str = "airc:bridge:directive";
/// Cap on inbound text we'll tokenize as a possible directive. Peer content is
/// attacker-controlled; a legit `!continuum` line is short. Bounds per-message
/// CPU/allocation regardless of payload size.
const MAX_DIRECTIVE_BYTES: usize = 16 * 1024;

pub struct AircBridgeDirectiveModule;

impl AircBridgeDirectiveModule {
    pub fn new() -> Self {
        Self
    }

    /// Subscribe to the bus NOW (synchronously, before any publish can race) and
    /// spawn the consume loop. Runs for the process lifetime.
    fn spawn_consumer(bus: Arc<crate::runtime::MessageBus>) {
        let mut rx = bus.receiver();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => process_chat_event(&event.name, &event.payload, &bus),
                    // Lagged: we fell behind the 1024 broadcast buffer. Directive
                    // recognition is best-effort observability, not guaranteed
                    // delivery — skip the gap and keep consuming.
                    Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => break,
                }
            }
        });
    }
}

impl Default for AircBridgeDirectiveModule {
    fn default() -> Self {
        Self::new()
    }
}

/// Pull a string field from the payload, trying a nested `payload` object then
/// the top level, across a few candidate keys. Defensive: the exact chat payload
/// shape can change without breaking this (returns None, never panics).
fn str_field<'a>(payload: &'a Value, keys: &[&str]) -> Option<&'a str> {
    let nested = payload.get("payload");
    for key in keys {
        if let Some(v) = nested.and_then(|n| n.get(key)).and_then(Value::as_str) {
            return Some(v);
        }
        if let Some(v) = payload.get(key).and_then(Value::as_str) {
            return Some(v);
        }
    }
    None
}

/// Pure classifier: given an inbound chat payload, return the parsed directive
/// IFF it is one (Chat/Skip/no-text/over-cap → None, so plain conversation is
/// left to the normal chat path). Unit-tested without a live bus.
fn classify_inbound(payload: &Value) -> Option<ParsedBridgeMessage> {
    let text = str_field(payload, &["text", "content", "message", "body"])?;
    if text.trim().is_empty() || text.len() > MAX_DIRECTIVE_BYTES {
        return None;
    }
    let room = str_field(payload, &["room", "roomName", "channel"]).map(str::to_string);
    let options = ParseOptions {
        sender_nick: str_field(payload, &["senderNick", "sender", "from", "author"])
            .map(str::to_string),
        // The airc channel IS the room scope here — feed both so the parser's
        // `room` (used by export/assert/chat) reflects the inbound room.
        room: room.clone(),
        channel: room,
        ..Default::default()
    };
    let parsed = parse_airc_bridge_message(text, &options);
    match parsed.action {
        // Plain conversation / self-echo — NOT a directive. The normal chat path
        // owns these; we surface nothing.
        BridgeAction::Chat | BridgeAction::Skip => None,
        // A recognized directive (or a prefixed-but-unknown one worth surfacing
        // so operators see rejected attempts). Bounded by the parser's allowlist.
        _ => Some(parsed),
    }
}

/// The `airc:bridge:directive` event body for a recognized directive. All fields
/// are parser-derived from the inbound message — no secrets/internal state.
fn directive_payload(parsed: &ParsedBridgeMessage) -> Value {
    json!({
        "action": parsed.action.as_str(),
        "room": parsed.room,
        "senderNick": parsed.sender_nick,
        "message": parsed.message,
        "marker": parsed.marker,
        "limit": parsed.limit,
        "error": parsed.error,
    })
}

/// Handle one bus event: if it's a chat event carrying a directive, probe + emit
/// the directive seam. Pure side-effects through the bus; NO command execution.
fn process_chat_event(name: &str, payload: &Value, bus: &crate::runtime::MessageBus) {
    if !name.starts_with(CHAT_PREFIX) {
        return;
    }
    let Some(parsed) = classify_inbound(payload) else {
        return; // plain conversation — leave to the normal chat path
    };
    let action = parsed.action.as_str();
    crate::probe!(
        class = "airc.bridge",
        action = action,
        room = parsed.room.as_str(),
        sender = parsed.sender_nick.as_str(),
        "inbound !continuum directive recognized",
    );
    bus.publish_async_only(DIRECTIVE_EVENT, directive_payload(&parsed));
}

#[async_trait]
impl ServiceModule for AircBridgeDirectiveModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "airc-bridge-directive",
            priority: ModulePriority::Normal,
            // Subscriber only — claims no command prefix. Delivery is via the
            // receiver() loop spawned in initialize, NOT event_subscriptions
            // (which are async no-ops without a receiver loop — see module doc).
            command_prefixes: &[],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        Self::spawn_consumer(ctx.bus.clone());
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        Err(format!(
            "AircBridgeDirectiveModule handles no commands (got '{command}'); it is a bus consumer"
        ))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::MessageBus;
    use tokio::time::{timeout, Duration};

    /// What this catches: a `!continuum` directive on inbound chat is recognized.
    #[test]
    fn recognizes_a_directive() {
        let payload = json!({ "text": "!continuum ping", "senderNick": "bigmama" });
        let parsed = classify_inbound(&payload).expect("ping is a directive");
        assert_eq!(parsed.action, BridgeAction::Ping);
        assert_eq!(parsed.sender_nick, "bigmama");
    }

    /// What this catches: the security boundary — plain conversation is NOT a
    /// directive and surfaces nothing (left to the normal chat path).
    #[test]
    fn plain_chat_is_not_a_directive() {
        assert!(classify_inbound(&json!({ "text": "hey what's the plan?" })).is_none());
    }

    /// What this catches: a self-echo (`[continuum]`) is skipped — no directive
    /// loop off our own mirrored output.
    #[test]
    fn self_echo_is_skipped() {
        assert!(classify_inbound(&json!({ "text": "[continuum] export done" })).is_none());
    }

    /// What this catches: nested-payload + missing-text defensiveness — text
    /// nested under `payload` still classifies; no text → None, not a panic.
    #[test]
    fn nested_payload_and_missing_text_are_handled() {
        let nested = json!({ "payload": { "text": "!continuum status", "room": "ops" } });
        let parsed = classify_inbound(&nested).expect("nested directive");
        assert_eq!(parsed.action, BridgeAction::Status);
        assert_eq!(parsed.room, "ops");
        assert!(classify_inbound(&json!({ "senderNick": "x" })).is_none());
    }

    /// What this catches: a prefixed-but-off-allowlist verb surfaces as Unknown
    /// (operators see rejected attempts) — but still never executes.
    #[test]
    fn unknown_directive_surfaces_for_visibility() {
        let parsed = classify_inbound(&json!({ "text": "!continuum rm -rf /" }))
            .expect("prefixed verb surfaces");
        assert_eq!(parsed.action, BridgeAction::Unknown);
    }

    /// What this catches: an oversized text body is NOT tokenized (peer can't
    /// force unbounded CPU/alloc by sending a huge `!continuum …` line).
    #[test]
    fn oversized_text_is_skipped() {
        let huge = format!("!continuum chat {}", "x".repeat(MAX_DIRECTIVE_BYTES));
        assert!(classify_inbound(&json!({ "text": huge })).is_none());
    }

    /// What this catches: NON-chat events are ignored by the prefix gate (media /
    /// presence / transport never get classified) — and emit nothing.
    #[tokio::test]
    async fn non_chat_events_are_ignored() {
        let bus = Arc::new(MessageBus::new());
        let mut rx = bus.receiver();
        process_chat_event(
            "presence:updated",
            &json!({ "text": "!continuum ping" }),
            &bus,
        );
        // Nothing emitted (the only thing that could arrive is our own publish).
        assert!(
            timeout(Duration::from_millis(150), rx.recv())
                .await
                .is_err(),
            "a non-chat event must not produce a directive"
        );
    }

    /// What this catches: THE wiring bug a prior cut had — directive recognition
    /// must actually flow THROUGH the real bus end-to-end (subscribe → publish
    /// chat → consume loop → emit directive). Pure-fn tests can't catch dead
    /// wiring; this does.
    #[tokio::test]
    async fn directive_flows_through_the_real_bus() {
        let bus = Arc::new(MessageBus::new());
        let mut rx = bus.receiver(); // observer, subscribed before any publish
        AircBridgeDirectiveModule::spawn_consumer(bus.clone()); // subscribes + spawns

        bus.publish_async_only(
            "chat:posted",
            json!({ "text": "!continuum ping", "senderNick": "bigmama", "room": "general" }),
        );

        let got = timeout(Duration::from_secs(2), async {
            loop {
                match rx.recv().await {
                    Ok(ev) if ev.name == DIRECTIVE_EVENT => return ev,
                    Ok(_) => continue,
                    Err(_) => panic!("bus closed before directive event"),
                }
            }
        })
        .await
        .expect("directive event must arrive within 2s");
        assert_eq!(got.payload["action"], "ping");
        assert_eq!(got.payload["room"], "general");
    }

    /// What this catches: the module stays a pure consumer — no command prefix
    /// (can't shadow real commands) and declares no async event_subscriptions
    /// (delivery is the receiver loop, not the no-op sub path).
    #[test]
    fn is_a_passive_consumer() {
        let cfg = AircBridgeDirectiveModule::new().config();
        assert!(cfg.command_prefixes.is_empty());
        assert!(cfg.event_subscriptions.is_empty());
    }
}
