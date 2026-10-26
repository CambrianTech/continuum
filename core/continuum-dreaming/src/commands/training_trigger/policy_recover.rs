//! An operator's evidence-bound correction of a returned submission's policy.
//! The original acceptance and examples remain immutable; the durable owner records
//! and applies the verified ancestor policy, without submitting or flushing anything.
use crate::modules::training_trigger::TrainingTriggerState;
use crate::sdk_codegen::CommandError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/training_trigger/ReturnPolicyEdge.ts"
)]
pub struct ReturnPolicyEdge {
    #[ts(type = "string")]
    pub dispatch_id: Uuid,
    #[ts(type = "string")]
    pub job_id: Uuid,
    /// None is a whole return; Some names the exact ascending parked indices.
    pub parked: Option<Vec<usize>>,
    pub chunk_index: usize,
}

#[derive(Debug, Clone, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/training_trigger/PolicyRecoverParams.ts"
)]
pub struct PolicyRecoverParams {
    #[ts(type = "string")]
    pub submission_id: Uuid,
    #[serde(default)]
    #[ts(optional)]
    pub expected_batch_sha256: Option<String>,
    /// Ordered from the pending return backwards to the original policy authority.
    pub edges: Vec<ReturnPolicyEdge>,
    pub reason: String,
    /// Inspect validates the same evidence without recording a correction.
    #[serde(default)]
    pub apply: bool,
}

#[derive(Debug, Serialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/training_trigger/PolicyRecoverOutcome.ts"
)]
pub struct PolicyRecoverOutcome {
    #[ts(type = "string")]
    pub submission_id: Uuid,
    pub batch_sha256: String,
    pub examples: usize,
    pub applied: bool,
    pub replayed: bool,
}

crate::action_command! {
    /// Correct a pending return's policy from exact durable return lineage. Owner-only;
    /// retains original acceptance/examples, never submits, flushes, or retrains.
    pub struct TrainingTriggerPolicyRecover { state: Arc<TrainingTriggerState>, }
    name: "genome/training-trigger/policy-recover",
    access: Privileged,
    params: PolicyRecoverParams,
    output: PolicyRecoverOutcome,
    run(this, ctx, p) => {
        // The executor's own policy resolves her trust (the grid's on a node); one that cannot
        // resolve it, or no executor, refuses: an owner-only operation is never assumed.
        let executor = this.state.executor.require()
            .map_err(|e| CommandError::Denied(format!("policy recovery requires the operator, and trust is unresolvable here: {e}")))?;
        match executor.caller_trust(ctx.caller.as_ref()) {
            Some(crate::routing::trust_level::TrustLevel::Owner) => {}
            Some(_) => return Err(CommandError::Denied("policy recovery requires the operator".into())),
            None => return Err(CommandError::Denied("policy recovery requires the operator, and this executor's policy cannot resolve trust".into())),
        }
        let actor = crate::persona::operator_peer::acting_now(ctx, "genome/training-trigger/policy-recover")?.peer;
        this.state.recover_return_policy(p, actor).await.map_err(CommandError::Invalid)
    }
}
