//! `inference/capacity` — local inference concurrency cap, single source of truth.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::sdk_codegen::{ActionCommand, CommandError, Ctx};
use crate::system_resources::local_inference_capacity;

/// Params for `inference/capacity` — none (a system fact, no inputs).
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/inference/InferenceCapacityParams.ts"
)]
pub struct InferenceCapacityParams {}

/// Result of `inference/capacity` — how many parallel generate requests the
/// hardware can service at once (matches the BatchScheduler's `n_seq_max`).
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/inference/InferenceCapacityResult.ts"
)]
pub struct InferenceCapacityResult {
    /// Concurrency cap — number of simultaneous generate requests. Always >= 1.
    pub capacity: u64,
}

/// `inference/capacity` — local inference concurrency cap. Stateless, AiSafe.
///
/// Single source of truth for the RAM-scaled concurrency formula (issue #887):
/// admission control and scheduler sizing both read this one value instead of
/// each re-deriving it. When capacity becomes pressure-reactive, this is where
/// the dynamic read lands.
#[derive(Default)]
pub struct InferenceCapacity;

#[async_trait]
impl ActionCommand for InferenceCapacity {
    const NAME: &'static str = "inference/capacity";
    const DESCRIPTION: &'static str =
        "Report the local inference concurrency cap — how many generate requests \
         the hardware can service in parallel (RAM-scaled, matches n_seq_max).";
    type Params = InferenceCapacityParams;
    type Output = InferenceCapacityResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        _params: InferenceCapacityParams,
    ) -> Result<InferenceCapacityResult, CommandError> {
        Ok(InferenceCapacityResult {
            capacity: local_inference_capacity() as u64,
        })
    }
}
crate::register_stateless_command!(InferenceCapacity);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: inference/capacity reports a usable (>=1) concurrency
    // cap from one self-routing file — the single-source-of-truth read (issue
    // #887) survived the move off InferenceModule intact.
    #[tokio::test]
    async fn capacity_is_positive() {
        let out = InferenceCapacity
            .run(&Ctx::default(), InferenceCapacityParams {})
            .await
            .expect("ok");
        assert!(
            out.capacity >= 1,
            "capacity must be >= 1, got {}",
            out.capacity
        );
    }
}
