//! Typed command dispatch over a `Transport`.

use std::sync::Arc;

use serde::{de::DeserializeOwned, Serialize};

use crate::error::ClientError;
use crate::transport::Transport;

/// Issues commands against a continuum substrate over `T`.
pub struct CommandClient<T: Transport> {
    transport: Arc<T>,
}

impl<T: Transport> CommandClient<T> {
    pub(crate) fn new(transport: Arc<T>) -> Self {
        Self { transport }
    }

    /// Execute a command with typed params and result. Serializes params
    /// at the boundary; deserializes the result; surfaces substrate
    /// refusal as `ClientError::Refused`.
    pub async fn execute<P, R>(&self, command: &str, params: P) -> Result<R, ClientError>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        let params_value = serde_json::to_value(params)?;
        let result_value = self.transport.request(command, params_value).await?;
        Ok(serde_json::from_value(result_value)?)
    }
}
