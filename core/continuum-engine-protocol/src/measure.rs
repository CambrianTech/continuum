use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::train::{EngineExample, Fit};

/// `POST /train/measure`'s body: how each example fits a window, measured by the engine's own
/// renderer before a run is sent (fork #51).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MeasureRequest {
    pub examples: Vec<EngineExample>,
    /// a multiple of 256, at least 256, at most the model's trained context
    pub window: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fit: Option<Fit>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MeasureReply {
    pub window: u32,
    pub examples: Vec<MeasuredExample>,
}

/// One example as `/train/measure` reports it (fork #51).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MeasuredExample {
    pub index: usize,
    /// rendered tokens before anything is dropped
    pub tokens_full: u64,
    /// the smallest window it trains in: the head plus her last exchange
    pub tokens_min: u64,
    /// tokens as it would train at this window (0 when skipped)
    pub tokens_trained: u64,
    pub fit: ExampleFit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExampleFit {
    Kept,
    Truncated,
    Skipped,
}
