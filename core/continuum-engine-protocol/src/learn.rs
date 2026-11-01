use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `POST /learn`'s body (fork `server-context.cpp`): one optimizer step on a loaded (shadow)
/// adapter, on a reply, in the slot that served it. The reply is given as a conversation whose
/// last message is the assistant's (`messages`), as token ids after a prefix
/// (`reply_tokens` + `n_prefix`), or as text after a prefix (`reply` + `n_prefix`).
///
/// The core has no caller yet: the in-place dream stage (ONE-RESIDENT-MODEL §8.1, S4) is the
/// first, and its open fork items (a per-mind shadow, learning a past example in a scratch slot,
/// a duty budget on the wire, a snapshot) are tracked on MIND-DATAFLOW-OVERHAUL row 6.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LearnRequest {
    pub id_slot: i32,
    pub lora_id: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lr: Option<f32>,
    #[serde(flatten)]
    pub reply: LearnReply,
}

/// The reply a learn step trains on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum LearnReply {
    /// at least two messages, the last one the assistant's; rendered with the chat template
    Messages {
        messages: Vec<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tools: Option<Value>,
    },
    Tokens { reply_tokens: Vec<i32>, n_prefix: u32 },
    Text { reply: String, n_prefix: u32 },
}

/// What a learn step reports, on `POST /learn` and as the `learn` field of a completion that
/// carried one (`learn_outcome`, server-task.cpp). A refusal inside the step (no such slot, no
/// loaded adapter, a missing backward) is `success: false` with `failure`, not an HTTP error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LearnOutcome {
    pub success: bool,
    pub loss: f64,
    pub n_labels: u32,
    pub t_ms: f64,
    /// the duty budget had no room: retry after `retry_after_ms`
    #[serde(default)]
    pub deferred: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
}
