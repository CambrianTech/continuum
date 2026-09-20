//! Typed command dispatch over a `Transport`.

use std::sync::Arc;

use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::ClientError;
use crate::transport::Transport;

/// Issues commands against a continuum substrate over `T`.
///
/// Carries the optional `context_id` of the scope it was handed out from
/// (`Connection::scoped(ctx).commands()`). When set, `execute` stamps it into
/// the request envelope as the `contextId` sibling — the third ID tier — so the
/// substrate scopes per-context state (per-room memory, per-thread recall)
/// without the caller threading it onto every call. Identity (`userId` /
/// `sessionId`) is kernel-injected from the connection; only `contextId` is
/// client-supplied, so only it is stamped here.
pub struct CommandClient<T: Transport> {
    transport: Arc<T>,
    context_id: Option<Uuid>,
}

impl<T: Transport> CommandClient<T> {
    /// Build a command client over `transport`, optionally scoped to a
    /// conversation/room. When `context_id` is `Some`, `execute` stamps it into
    /// every request envelope (the third ID tier). `Connection::commands` is the
    /// only caller — it passes the connection's scope.
    pub(crate) fn with_context(transport: Arc<T>, context_id: Option<Uuid>) -> Self {
        Self {
            transport,
            context_id,
        }
    }

    /// Value-native dispatch — the zero-waste path. For callers that already hold
    /// a `serde_json::Value` (a persona's `ToolExecutor`, the recipe walker, any
    /// substrate-internal command), the typed [`execute`] would `to_value` /
    /// `from_value` a `Value` *into an identical `Value`* — two full tree walks of
    /// pure waste. This path stamps the scope and forwards the `Value` straight to
    /// the transport: **copy only where a boundary forces it** (the wire, in
    /// `AircIpcTransport`; never at all for `InProcessTransport`), never on a
    /// same-type round-trip. Identity is kernel-injected; only `contextId` is
    /// client-stamped here.
    ///
    /// [`execute`]: CommandClient::execute
    pub async fn execute_value(
        &self,
        command: &str,
        mut params: Value,
    ) -> Result<Value, ClientError> {
        self.stamp_context(&mut params);
        stamp_actor_kind(&mut params);
        self.transport.execute(command, params).await
    }

    /// Execute a command with typed params and result. The typed boundary is the
    /// ONE place a serde round-trip is genuinely required (arbitrary `P`/`R`);
    /// the scope-stamp + transport hop are delegated to [`execute_value`] so there
    /// is a single dispatch path. Surfaces substrate refusal as
    /// [`ClientError::Refused`]. Callers already holding a `Value` should call
    /// [`execute_value`] directly to skip the round-trip entirely.
    pub async fn execute<P, R>(&self, command: &str, params: P) -> Result<R, ClientError>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        let params_value = serde_json::to_value(params)?;
        let result_value = self.execute_value(command, params_value).await?;
        Ok(serde_json::from_value(result_value)?)
    }

    /// Stamp `contextId` into the request envelope when this client is scoped.
    /// The envelope reads it as a sibling of the flattened params
    /// (`command_envelope.rs`), so it must be an object — which command params
    /// always are. A non-object params value (degenerate) is left untouched
    /// rather than silently wrapped: better the scope is visibly absent than
    /// the params shape corrupted.
    fn stamp_context(&self, params: &mut Value) {
        let Some(context_id) = self.context_id else {
            return;
        };
        if let Value::Object(map) = params {
            map.insert(
                "contextId".to_string(),
                Value::String(context_id.to_string()),
            );
        }
    }
}


/// Stamp the CLAIMED actor kind when this process is driven by an AI agent
/// session (Claude Code / Codex / etc — detected from the agent runtimes' own
/// env markers). Attribution, not authentication: the core's caller-less
/// sender chain resolves an `"agent"` claim to the AGENT self-peer, so an
/// agent's probes and chat never wear the human operator's name (Joel,
/// 2026-09-01: "the chat history is clearly attributing shit you did to me").
/// A human at a bare terminal has none of these markers and stays the operator.
fn stamp_actor_kind(params: &mut Value) {
    let agent_env = std::env::var_os("CLAUDECODE").is_some()
        || std::env::var_os("AI_AGENT").is_some()
        || std::env::var_os("CLAUDE_CODE_ENTRYPOINT").is_some()
        || std::env::var_os("CODEX_SANDBOX").is_some();
    if !agent_env {
        return;
    }
    if let Value::Object(map) = params {
        map.entry("actorKind".to_string())
            .or_insert_with(|| Value::String("agent".to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockTransport;
    use serde_json::json;
    use std::sync::Mutex;

    // what this catches: execute_value forwards the Value to the transport with
    // contextId stamped (when scoped) and returns the transport's Value verbatim —
    // NO serde round-trip. This is the zero-waste path the persona ToolExecutor +
    // recipe walker take; a regression that reintroduced to_value/from_value would
    // still pass functionally but burn CPU, so this also documents the contract.
    #[tokio::test]
    async fn execute_value_stamps_scope_and_forwards_verbatim() {
        let seen: Arc<Mutex<Option<Value>>> = Arc::new(Mutex::new(None));
        let cap = Arc::clone(&seen);
        let mock = MockTransport::new();
        mock.respond_to("code/read", move |params| {
            *cap.lock().unwrap() = Some(params);
            Ok(json!({ "content": "hi" }))
        });
        let ctx = Uuid::new_v4();
        let client = CommandClient::with_context(Arc::new(mock), Some(ctx));

        let out = client
            .execute_value("code/read", json!({ "path": "a.rs" }))
            .await
            .expect("execute_value");

        // result returned verbatim
        assert_eq!(out, json!({ "content": "hi" }));
        // params reached the transport with the scope stamped
        let sent = seen.lock().unwrap().clone().expect("transport saw params");
        assert_eq!(sent["path"], "a.rs");
        assert_eq!(sent["contextId"], json!(ctx.to_string()));
    }
}
