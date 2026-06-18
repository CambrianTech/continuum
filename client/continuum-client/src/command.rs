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

    /// Execute a command with typed params and result. Serializes params
    /// at the boundary; deserializes the result; surfaces substrate
    /// refusal as `ClientError::Refused`.
    pub async fn execute<P, R>(&self, command: &str, params: P) -> Result<R, ClientError>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        let mut params_value = serde_json::to_value(params)?;
        self.stamp_context(&mut params_value);
        let result_value = self.transport.execute(command, params_value).await?;
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
