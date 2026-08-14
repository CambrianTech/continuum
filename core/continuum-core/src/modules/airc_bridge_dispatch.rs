//! AircBridgeDispatchModule — consumes recognized `!continuum` directives and
//! produces a reply, OFF the transport hot path. (comms→airc lane, slice 3b)
//!
//! Pairs with [`super::airc_bridge_directive`] (slice 3a): that module
//! recognizes inbound directives and emits `airc:bridge:directive`; this module
//! consumes that event and emits `airc:bridge:reply`. Splitting recognition
//! from dispatch keeps the peer-content→action boundary one explicit, reviewable
//! seam (vuln-A: peer content is conversation, not instructions).
//!
//! ## What it does NOW (safe by construction)
//!
//! For the no-side-effect directives — `ping`, `status` — it computes a local
//! reply. For the command-EXECUTING directives (`rooms`, `export`,
//! `activity-list`, `assert-seen`, `chat`) it replies "recognized; execution
//! pending" but **does NOT execute any kernel command**. Executing a command on
//! behalf of an airc peer requires a caller-identity / authorization model (a
//! peer-driven command must run as a RESTRICTED bridge identity, never
//! owner/system, or an allowlisted command becomes a privilege hole). That is
//! slice 3b-2, landed under its own heavy review. Until then this module is a
//! recognizer+responder, not an executor — no peer content reaches the kernel.
//!
//! ## Reply shape + loop-guard
//!
//! Replies are emitted as `airc:bridge:reply` bus events whose text begins with
//! `[continuum]`. When a reply round-trips back through airc, slice 3a's parser
//! maps a `[continuum]`-prefixed message to `Skip` (its loop-guard), so replies
//! never re-enter as directives. The actual airc round-trip (publishing the
//! reply back to the room) rides the bus→airc transport seam, same as every
//! other bus event; this module stays decoupled from the outbound handle.
//!
//! Off the inbound attach loop (consumes via `MessageBus::receiver()`), in
//! memory, no ORM, bounded — honors the airc performance doctrine.

use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::any::Any;
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;

/// Event emitted by slice 3a (recognized directive) that this module consumes.
const DIRECTIVE_EVENT: &str = "airc:bridge:directive";
/// Event this module emits (a directive reply). Text is `[continuum]`-prefixed
/// so slice 3a's loop-guard skips it on round-trip.
const REPLY_EVENT: &str = "airc:bridge:reply";

pub struct AircBridgeDispatchModule;

impl AircBridgeDispatchModule {
    pub fn new() -> Self {
        Self
    }

    fn spawn_consumer(bus: Arc<crate::runtime::MessageBus>) {
        let mut rx = bus.receiver();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => process_directive_event(&event.name, &event.payload, &bus),
                    Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => break,
                }
            }
        });
    }
}

impl Default for AircBridgeDispatchModule {
    fn default() -> Self {
        Self::new()
    }
}

/// Pure: the reply text for a recognized directive. `[continuum]`-prefixed so
/// the round-trip is loop-guarded. ping/status answer locally; the
/// command-executing actions acknowledge but defer execution to 3b-2 (no kernel
/// command is run from peer content here).
fn reply_for(action: &str, payload: &Value) -> String {
    match action {
        "ping" => "[continuum] pong".to_string(),
        "status" => "[continuum] online".to_string(),
        "rooms" | "export" | "activity-list" | "assert-seen" | "chat" => {
            format!(
                "[continuum] recognized '{action}' — command execution lands in slice 3b-2 \
                 (peer-command authorization); not executed yet"
            )
        }
        "unknown" => {
            let err = payload
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown directive");
            format!("[continuum] {err}")
        }
        // Chat/Skip never reach here (3a doesn't emit a directive event for
        // them); any other action is acknowledged without action.
        other => format!("[continuum] no handler for '{other}'"),
    }
}

/// Handle one bus event: if it's a recognized directive, compute + emit the
/// reply. NO kernel command execution (see module doc). Pure side-effect
/// through the bus.
fn process_directive_event(name: &str, payload: &Value, bus: &crate::runtime::MessageBus) {
    if name != DIRECTIVE_EVENT {
        return;
    }
    let Some(action) = payload.get("action").and_then(Value::as_str) else {
        return;
    };
    let room = payload.get("room").and_then(Value::as_str).unwrap_or("");
    let reply = reply_for(action, payload);

    crate::probe!(
        class = "airc.bridge",
        action = action,
        room = room,
        "directive dispatched (reply emitted, no kernel execution)",
    );

    bus.publish_async_only(
        REPLY_EVENT,
        json!({
            "room": room,
            "inReplyTo": action,
            "text": reply,
            "executed": false,
        }),
    );
}

#[async_trait]
impl ServiceModule for AircBridgeDispatchModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "airc-bridge-dispatch",
            priority: ModulePriority::Normal,
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
            "AircBridgeDispatchModule handles no commands (got '{command}'); it is a bus consumer"
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

    /// What this catches: ping/status get real local replies, all `[continuum]`-
    /// prefixed (loop-guarded).
    #[test]
    fn ping_and_status_reply_locally() {
        assert_eq!(reply_for("ping", &json!({})), "[continuum] pong");
        assert!(reply_for("status", &json!({})).starts_with("[continuum] online"));
    }

    /// What this catches: command-executing directives are acknowledged but the
    /// reply states execution is NOT done (the security boundary — no kernel
    /// command runs from peer content in this slice).
    #[test]
    fn command_directives_are_recognized_but_not_executed() {
        for action in ["rooms", "export", "activity-list", "assert-seen", "chat"] {
            let r = reply_for(action, &json!({}));
            assert!(
                r.starts_with("[continuum]"),
                "reply must be loop-guarded: {r}"
            );
            assert!(r.contains("not executed"), "must NOT claim execution: {r}");
        }
    }

    /// What this catches: every reply is `[continuum]`-prefixed so a round-trip
    /// through airc is skipped by slice 3a's loop-guard (no directive loop).
    #[test]
    fn all_replies_are_loop_guarded() {
        for action in ["ping", "status", "rooms", "export", "unknown", "weird"] {
            assert!(
                reply_for(action, &json!({ "error": "x" })).starts_with("[continuum]"),
                "action {action} reply not loop-guarded"
            );
        }
    }

    /// What this catches: THE wiring — a directive event flows through the real
    /// bus to a reply event end-to-end (off-loop consumer actually fires).
    #[tokio::test]
    async fn directive_flows_to_reply_through_the_real_bus() {
        let bus = Arc::new(MessageBus::new());
        let mut rx = bus.receiver();
        AircBridgeDispatchModule::spawn_consumer(bus.clone());

        bus.publish_async_only(
            DIRECTIVE_EVENT,
            json!({ "action": "ping", "room": "general", "senderNick": "bigmama" }),
        );

        let got = timeout(Duration::from_secs(2), async {
            loop {
                match rx.recv().await {
                    Ok(ev) if ev.name == REPLY_EVENT => return ev,
                    Ok(_) => continue,
                    Err(_) => panic!("bus closed before reply"),
                }
            }
        })
        .await
        .expect("reply event must arrive within 2s");
        assert_eq!(got.payload["inReplyTo"], "ping");
        assert_eq!(got.payload["text"], "[continuum] pong");
        assert_eq!(got.payload["room"], "general");
        assert_eq!(got.payload["executed"], false);
    }

    /// What this catches: a non-directive event is ignored (the consumer only
    /// acts on airc:bridge:directive, not arbitrary bus traffic).
    #[tokio::test]
    async fn ignores_non_directive_events() {
        let bus = Arc::new(MessageBus::new());
        let mut rx = bus.receiver();
        process_directive_event("chat:posted", &json!({ "action": "ping" }), &bus);
        assert!(
            timeout(Duration::from_millis(150), rx.recv())
                .await
                .is_err(),
            "must not reply to a non-directive event"
        );
    }
}
