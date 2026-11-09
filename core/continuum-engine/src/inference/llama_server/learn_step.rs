//! One learning step on her slot, in the engine that serves her (card 4114a2c6; fork #89's past
//! example on the integration line).
//!
//! An example she lived is learned in place: the engine serves its prompt on her slot (free when
//! the slot still holds it, one prefill when it does not) and learns her reply under her shadow,
//! admitted by her grant and inside her optimizer residency and the engine's workspace residency.
//! The messages are the core's own wire framing of the call (`request_body::wire_messages`), so
//! learning sees exactly what serving framed.

use super::learn_ledger::{GrantId, LearningEngine};
use super::learn_residency::ResidencyId;
use super::*;

/// A lived example to learn: the conversation as served, its last message the reply to learn.
#[derive(Debug, Clone, PartialEq)]
pub struct LearnExample {
    pub messages: Vec<serde_json::Value>,
    /// The tools she was offered, as served (none for a conversation without tools).
    pub tools: Option<serde_json::Value>,
    pub slot: u32,
    pub lora_id: i64,
    pub grant: GrantId,
    pub optimizer_residency: ResidencyId,
    pub workspace_residency: ResidencyId,
    pub learning_rate: f32,
}

/// What the engine did with one learning step.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct LearnOutcome {
    pub success: bool,
    /// Optimizer updates this step committed to her shadow, as the engine counted them.
    pub committed_steps: i64,
    /// The engine failed mid-step and quarantined her shadow: its parameters may hold partial
    /// writes, so it is never snapshotted or trusted again (refusal `effects_uncertain`).
    pub effects_uncertain: bool,
    /// The ledger's refusal name, `none` when it was admitted.
    pub refusal: String,
    pub loss: f64,
    pub n_labels: i64,
    pub t_ms: f64,
    pub max_chunk_ms: f64,
    pub restoration_ms: f64,
    #[serde(default)]
    pub deferred: bool,
    #[serde(default)]
    pub retry_after_ms: Option<i64>,
    #[serde(default)]
    pub failure: Option<String>,
}

impl LearningEngine {
    /// `POST /learn`: learn `example` on its slot. The engine's answer is the step's outcome (a
    /// refusal or deferral is an outcome, not an error); a transport failure is the error.
    pub async fn learn(&self, example: &LearnExample) -> Result<LearnOutcome, LlamaServerError> {
        let mut body = serde_json::json!({
            "messages": example.messages,
            "id_slot": example.slot,
            "lora_id": example.lora_id,
            "grant": example.grant.to_string(),
            "optimizer_residency": example.optimizer_residency.to_string(),
            "workspace_residency": example.workspace_residency.to_string(),
            "lr": example.learning_rate,
        });
        if let Some(tools) = &example.tools {
            body["tools"] = tools.clone();
        }
        let resp = self
            .client
            .post(format!("{}/learn", self.root))
            .json(&body)
            .send()
            .await
            .map_err(|e| LlamaServerError::Unreachable(e.to_string()))?;
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(|e| LlamaServerError::Unreachable(e.to_string()))?;
        if !status.is_success() {
            return Err(LlamaServerError::Unreachable(format!("/learn refused ({status}): {}", String::from_utf8_lossy(&bytes))));
        }
        serde_json::from_slice(&bytes) // engine HTTP wire: the learn outcome
            .map_err(|e| LlamaServerError::Unreachable(format!("/learn answered no outcome: {e}")))
    }
}

