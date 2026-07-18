//! The core→client back-channel that lets a connected eye-node fulfil a
//! [`Provided`](crate::sdk_codegen::WireShape::Provided) command
//! (`perception/observe`, `interface/screenshot`).
//!
//! # Why this exists
//!
//! The socket IPC is otherwise strictly **client→core request / core→client
//! response**: a client sends `{command, params, requestId}`, the core answers.
//! A `Provided` command inverts that — the headless core *cannot* render or
//! capture (no browser, no display on a rack), so it must call BACK into a
//! connected client that holds the eye. That is a core-originated request awaiting
//! a client reply, which the base protocol has no path for.
//!
//! This module adds the missing half without disturbing the request path:
//!
//! - [`IpcClientProvider`] implements [`ProvidedCommandProvider`]. Its `fulfill`
//!   pushes a `provideCall` frame down the connection's existing writer thread
//!   (the ONE owner of the socket write half — frame-atomic with normal
//!   responses) and awaits the client's `provideResult` via a per-connection
//!   correlation map. It is registered in the shared
//!   [`ProviderRegistry`](crate::runtime::ProviderRegistry) when a client sends
//!   `provider/register`, and dropped on disconnect.
//! - [`register_provider`] handles the `provider/register` handshake (validates
//!   the client only claims `Provided` commands, builds the provider, binds it).
//! - [`complete_provide_result`] routes a client's `provideResult` reply back to
//!   the waiting `fulfill` by core-allocated `callId`.
//!
//! The correlation here is **core-owned** (`next_id` + `pending`), distinct from
//! the client-owned `requestId` on the request path — the two never collide
//! because they live on opposite directions of the wire.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::{json, Value};
use tokio::sync::oneshot;

use super::protocol::Response;
use super::Outbound;
use crate::runtime::{ProvidedCommandProvider, ProviderRegistry};
use crate::sdk_codegen::{command_registry, WireShape};

/// How long a persona's observe waits for the eye-node before failing loud. A
/// cold browser launch + `networkidle` + full-page screenshot is seconds, not
/// minutes; 120s is generous headroom, not a silent hang — when it elapses the
/// caller learns the eye-node never answered (never a fabricated observation).
const PROVIDE_CALL_TIMEOUT: Duration = Duration::from_secs(120);

/// Pending core→client calls for ONE connection, keyed by core-allocated call id.
/// The reader loop removes-and-completes an entry when the matching
/// `provideResult` arrives; `fulfill` removes it on timeout/disconnect.
pub(super) type PendingCalls = Arc<DashMap<u64, oneshot::Sender<Result<Value, String>>>>;

/// One connection's registrations: the provider it built and the command names it
/// bound. Kept by the connection so it can `unregister_matching` on disconnect.
pub(super) type ConnRegistration = (Arc<dyn ProvidedCommandProvider>, Vec<String>);

/// A provider backed by a live IPC client connection. Holds a clone of the
/// connection's writer channel + its correlation state; `fulfill` forwards the
/// call over the wire and awaits the reply.
pub(super) struct IpcClientProvider {
    label: String,
    /// The connection's writer channel — a `provideCall` here is framed and sent
    /// by the writer thread, interleaved atomically with command responses.
    outbound: Sender<Outbound>,
    pending: PendingCalls,
    next_id: Arc<AtomicU64>,
}

impl IpcClientProvider {
    pub(super) fn new(
        label: String,
        outbound: Sender<Outbound>,
        pending: PendingCalls,
        next_id: Arc<AtomicU64>,
    ) -> Self {
        Self {
            label,
            outbound,
            pending,
            next_id,
        }
    }
}

#[async_trait]
impl ProvidedCommandProvider for IpcClientProvider {
    async fn fulfill(&self, command: &str, params: Value) -> Result<Value, String> {
        let call_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (reply_tx, reply_rx) = oneshot::channel();
        self.pending.insert(call_id, reply_tx);

        // Push the call to the client over its writer thread. A send error means
        // the writer (and thus the connection) is gone — fail loud, don't hang.
        if let Err(e) = self.outbound.send(Outbound::ProvideCall {
            call_id,
            command: command.to_string(),
            params,
        }) {
            self.pending.remove(&call_id);
            return Err(format!(
                "eye-node '{}' is no longer connected (writer closed): {e}",
                self.label
            ));
        }

        match tokio::time::timeout(PROVIDE_CALL_TIMEOUT, reply_rx).await {
            // The client answered — surface its bare Result verbatim (success
            // value or its own error string). The reader already removed pending.
            Ok(Ok(outcome)) => outcome,
            // The connection dropped before answering (reader loop exited, sender
            // side of the oneshot dropped).
            Ok(Err(_recv)) => {
                self.pending.remove(&call_id);
                Err(format!(
                    "eye-node '{}' disconnected before answering '{command}'",
                    self.label
                ))
            }
            // The client is connected but wedged — bound the wait, fail loud.
            Err(_elapsed) => {
                self.pending.remove(&call_id);
                Err(format!(
                    "eye-node '{}' did not answer '{command}' within {}s",
                    self.label,
                    PROVIDE_CALL_TIMEOUT.as_secs()
                ))
            }
        }
    }

    fn label(&self) -> &str {
        &self.label
    }
}

/// Handle a `provider/register` handshake from a connected client. Reads
/// `commands` + `label` (from a nested `params` object, or top-level as a
/// fallback), validates every claimed command is actually `Provided` (a client
/// cannot fulfil a substrate `ServiceModule` command), builds an
/// [`IpcClientProvider`] bound to this connection, and registers it. On success
/// the built provider + its command names are pushed onto `registrations` so the
/// connection can `unregister_matching` them at disconnect. Returns the wire
/// [`Response`] to send back on the request channel.
pub(super) fn register_provider(
    registry: &Arc<ProviderRegistry>,
    outbound: &Sender<Outbound>,
    pending: &PendingCalls,
    next_id: &Arc<AtomicU64>,
    msg: &Value,
    registrations: &mut Vec<ConnRegistration>,
) -> Response {
    // SDK sends `{command, params:{...}, requestId}`; tolerate top-level too.
    let params = msg.get("params").unwrap_or(msg);

    let commands: Vec<String> = params
        .get("commands")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|c| c.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    if commands.is_empty() {
        return Response::error(
            "provider/register requires a non-empty `commands` array (the Provided \
             capabilities this client fulfils, e.g. [\"perception/observe\", \
             \"interface/screenshot\"])"
                .to_string(),
        );
    }

    let label = params
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or("eye-node")
        .to_string();

    // A client may only provide `Provided`-wire commands. Anything else (a normal
    // substrate command) is refused loud — accepting it would let a client shadow
    // a real ServiceModule, which the interceptor would never even route to it.
    let provided: HashSet<&'static str> = command_registry()
        .into_iter()
        .filter(|d| d.wire == WireShape::Provided)
        .map(|d| d.name)
        .collect();
    let rejected: Vec<&String> = commands
        .iter()
        .filter(|c| !provided.contains(c.as_str()))
        .collect();
    if !rejected.is_empty() {
        return Response::error(format!(
            "provider/register rejected non-Provided command(s): {rejected:?}. Only \
             Provided capabilities (an eye-node verb like perception/observe or \
             interface/screenshot) can be client-fulfilled; everything else is a \
             substrate ServiceModule the core runs itself."
        ));
    }

    let provider: Arc<dyn ProvidedCommandProvider> = Arc::new(IpcClientProvider::new(
        label.clone(),
        outbound.clone(),
        Arc::clone(pending),
        Arc::clone(next_id),
    ));
    let refs: Vec<&str> = commands.iter().map(String::as_str).collect();
    registry.register(&refs, Arc::clone(&provider));
    registrations.push((provider, commands.clone()));

    Response::success(json!({
        "success": true,
        "registered": commands,
        "label": label,
    }))
}

/// Route a client's `provideResult` reply back to the waiting `fulfill`, keyed by
/// the core-allocated `callId`. A malformed reply (no `callId`) or a stale one
/// (call already resolved/timed out) is dropped — the loud signal in that case is
/// the `fulfill` timeout, not a second error here.
pub(super) fn complete_provide_result(pending: &PendingCalls, msg: &Value) {
    let Some(call_id) = msg.get("callId").and_then(Value::as_u64) else {
        return;
    };
    let Some((_, reply_tx)) = pending.remove(&call_id) else {
        return;
    };

    let success = msg
        .get("success")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let outcome = if success {
        Ok(msg.get("result").cloned().unwrap_or(Value::Null))
    } else {
        Err(msg
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("eye-node reported failure with no error message")
            .to_string())
    };
    // Receiver may already be gone if fulfill timed out in the same instant — a
    // dropped send is harmless (the caller already got its timeout error).
    let _ = reply_tx.send(outcome);
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the whole point of the back-channel — fulfill pushes a
    // ProvideCall over the writer channel AND, when the matching provideResult
    // completes the pending oneshot, returns the client's bare result verbatim.
    // A persona's perception/observe reaching an eye-node rides exactly this.
    #[tokio::test]
    async fn fulfill_forwards_a_call_and_returns_the_clients_result() {
        let (tx, rx) = std::sync::mpsc::channel::<Outbound>();
        let pending: PendingCalls = Arc::new(DashMap::new());
        let next_id = Arc::new(AtomicU64::new(1));
        let provider = IpcClientProvider::new(
            "test-eye".into(),
            tx,
            Arc::clone(&pending),
            Arc::clone(&next_id),
        );

        let fut = tokio::spawn(async move {
            provider
                .fulfill("perception/observe", json!({ "target": "https://x" }))
                .await
        });

        // The provider pushed a ProvideCall — read it off the writer channel.
        let out = tokio::task::spawn_blocking(move || rx.recv())
            .await
            .unwrap()
            .expect("a ProvideCall frame must be sent");
        let call_id = match out {
            Outbound::ProvideCall {
                call_id,
                command,
                params,
            } => {
                assert_eq!(command, "perception/observe");
                assert_eq!(params["target"], "https://x");
                call_id
            }
            _ => panic!("expected an Outbound::ProvideCall"),
        };

        // Simulate the client's reply.
        complete_provide_result(
            &pending,
            &json!({ "type": "provideResult", "callId": call_id, "success": true, "result": { "ok": 1 } }),
        );

        let result = fut.await.unwrap().expect("fulfill should succeed");
        assert_eq!(result["ok"], 1);
    }

    // what this catches: a wedged/gone client fails loud, named — never a silent
    // hang or fabricated observation. Here the writer half is dropped so the send
    // itself fails; the error names the eye-node.
    #[tokio::test]
    async fn fulfill_fails_loud_when_the_client_connection_is_gone() {
        let (tx, rx) = std::sync::mpsc::channel::<Outbound>();
        drop(rx); // writer thread gone → send fails
        let provider = IpcClientProvider::new(
            "dead-eye".into(),
            tx,
            Arc::new(DashMap::new()),
            Arc::new(AtomicU64::new(1)),
        );
        let err = provider
            .fulfill("interface/screenshot", json!({}))
            .await
            .expect_err("a gone client must fail loud");
        assert!(err.contains("dead-eye"), "names the eye-node: {err}");
        assert!(err.contains("no longer connected"), "explains why: {err}");
    }

    // what this catches: a client cannot register itself as the provider of a
    // normal substrate command (code/read) — only Provided commands are
    // client-fulfillable. Guards the shadowing hole.
    #[test]
    fn register_rejects_a_non_provided_command() {
        let registry = Arc::new(ProviderRegistry::new());
        let (tx, _rx) = std::sync::mpsc::channel::<Outbound>();
        let pending: PendingCalls = Arc::new(DashMap::new());
        let next_id = Arc::new(AtomicU64::new(1));
        let mut regs = Vec::new();

        let reply = register_provider(
            &registry,
            &tx,
            &pending,
            &next_id,
            &json!({ "command": "provider/register", "params": { "commands": ["code/read"], "label": "x" } }),
            &mut regs,
        );

        assert!(!reply.success, "a non-Provided command must be rejected");
        assert!(regs.is_empty(), "nothing gets tracked on rejection");
        assert!(registry.is_empty(), "nothing gets bound on rejection");
    }

    // what this catches: a valid provider/register binds the command in the
    // registry, tracks the registration for disconnect cleanup, and
    // unregister_matching removes exactly that binding.
    #[test]
    fn register_a_provided_command_binds_then_unbinds_it() {
        let registry = Arc::new(ProviderRegistry::new());
        let (tx, _rx) = std::sync::mpsc::channel::<Outbound>();
        let pending: PendingCalls = Arc::new(DashMap::new());
        let next_id = Arc::new(AtomicU64::new(1));
        let mut regs: Vec<ConnRegistration> = Vec::new();

        let reply = register_provider(
            &registry,
            &tx,
            &pending,
            &next_id,
            &json!({ "command": "provider/register", "params": { "commands": ["perception/observe"], "label": "eye" } }),
            &mut regs,
        );

        assert!(reply.success);
        assert_eq!(regs.len(), 1);
        assert!(registry.provider_for("perception/observe").is_some());

        let (provider, commands) = &regs[0];
        registry.unregister_matching(commands, provider);
        assert!(
            registry.provider_for("perception/observe").is_none(),
            "disconnect must drop this connection's binding"
        );
    }

    // what this catches: a stale/duplicate provideResult (its call already
    // resolved) is a harmless no-op — complete on an empty pending map must not
    // panic. Protects against a client double-replying or replying after timeout.
    #[test]
    fn completing_an_unknown_call_is_a_harmless_no_op() {
        let pending: PendingCalls = Arc::new(DashMap::new());
        complete_provide_result(
            &pending,
            &json!({ "type": "provideResult", "callId": 999, "success": true, "result": {} }),
        );
        // no panic, nothing to assert beyond survival
        assert!(pending.is_empty());
    }
}
