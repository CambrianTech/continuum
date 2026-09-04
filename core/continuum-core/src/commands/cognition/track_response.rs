//! `cognition/track-response` — record that the persona just responded in a room
//! (typed, dep-holding).
//!
//! Bumps the persona's per-room response counter in its rate limiter (the signal the
//! gating pipeline reads to pace how often it speaks), lazily creating the persona via
//! `get_or_create_persona`. Captures the owning module's
//! [`CognitionState`](crate::modules::cognition::CognitionState).
//!
//! `access: Internal` — host-driven cognition IPC, not a persona toolbelt verb.

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::CognitionState;
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/TrackResponseParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct TrackResponseParams {
    /// Persona that responded.
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// Room the response landed in.
    #[ts(type = "string")]
    pub room_id: Uuid,
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/TrackResponseResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct TrackResponseResult {
    pub tracked: bool,
    #[ts(type = "number")]
    pub response_count: u32,
}

crate::action_command! {
    /// Record that the persona just responded in `room_id`, bumping its per-room
    /// response counter (paces future gating). Host-invoked.
    pub struct TrackResponse { state: Arc<CognitionState> }
    name: "cognition/track-response",
    access: Internal,
    params: TrackResponseParams,
    output: TrackResponseResult,
    run(this, _ctx, p) => {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| CommandError::Internal(format!("system clock before UNIX epoch: {e}")))?
            .as_millis() as u64;

        let mut persona = this.state.get_or_create_persona(p.persona_id);
        persona.rate_limiter.track_response(p.room_id, now_ms);
        let response_count = persona.rate_limiter.response_count(p.room_id);

        crate::log_info!(
            "module",
            "cognition",
            "track-response {}: room={}, count={}",
            p.persona_id,
            p.room_id,
            response_count
        );

        Ok(TrackResponseResult { tracked: true, response_count })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. track-response is host-driven
    // cognition IPC, so it is Internal — registered and grid-routable, never a
    // remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(TrackResponse::NAME, "cognition/track-response");
        assert_eq!(TrackResponse::ACCESS, AccessLevel::Internal);
    }
}
