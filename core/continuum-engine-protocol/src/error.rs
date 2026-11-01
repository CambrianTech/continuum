use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Every non-2xx reply the engine sends: `{"error": {"code", "message", "type"}}`
/// (`format_error_response`, server-common.cpp). The core decides on `type` and the HTTP code,
/// never on `message`, which is for the reader.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EngineErrorBody {
    pub error: EngineError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EngineError {
    pub code: u16,
    pub message: String,
    #[serde(rename = "type")]
    pub kind: ErrorType,
}

/// The engine's error classes. One it does not know is `Unknown`, never guessed into a known one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorType {
    /// 400: the request was refused before anything ran (a documented refusal)
    InvalidRequestError,
    /// 501: the server was started without the capability (no `--train-dir`)
    NotSupportedError,
    /// 503: the model is loading
    UnavailableError,
    #[serde(other)]
    Unknown,
}
