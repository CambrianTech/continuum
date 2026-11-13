//! `cognition/configure-rate-limiter` — tune a persona's response rate limits (typed,
//! dep-holding).
//!
//! Sets the persona's [`RateLimiter`](crate::persona) knobs: the minimum spacing between
//! responses and the per-session response cap — the throttle that keeps a persona from
//! flooding a room. Captures the owning module's
//! [`CognitionState`](crate::modules::cognition::CognitionState).
//!
//! `access: Internal` — host-driven cognition IPC, not a persona toolbelt verb.

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::CognitionState;

fn default_min_seconds() -> f64 {
    10.0
}

fn default_max_responses() -> u32 {
    50
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ConfigureRateLimiterParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ConfigureRateLimiterParams {
    /// Persona whose rate limiter is configured.
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// Minimum seconds between responses (default 10.0).
    #[serde(default = "default_min_seconds")]
    #[ts(type = "number")]
    pub min_seconds_between_responses: f64,
    /// Maximum responses per session (default 50).
    #[serde(default = "default_max_responses")]
    #[ts(type = "number")]
    pub max_responses_per_session: u32,
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ConfigureRateLimiterResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ConfigureRateLimiterResult {
    pub configured: bool,
    #[ts(type = "number")]
    pub min_seconds_between_responses: f64,
    #[ts(type = "number")]
    pub max_responses_per_session: u32,
}

crate::action_command! {
    /// Tune the persona's response rate limits (min spacing + per-session cap). The
    /// throttle that keeps a persona from flooding a room. Host-invoked.
    pub struct ConfigureRateLimiter { state: Arc<CognitionState> }
    name: "cognition/configure-rate-limiter",
    access: Internal,
    params: ConfigureRateLimiterParams,
    output: ConfigureRateLimiterResult,
    run(this, _ctx, p) => {
        let mut persona = this.state.get_or_create_persona(p.persona_id);
        persona.rate_limiter.min_seconds_between_responses = p.min_seconds_between_responses;
        persona.rate_limiter.max_responses_per_session = p.max_responses_per_session;

        crate::log_info!(
            "module",
            "cognition",
            "configure-rate-limiter {}: min_seconds={}, max_responses={}",
            p.persona_id,
            p.min_seconds_between_responses,
            p.max_responses_per_session
        );

        Ok(ConfigureRateLimiterResult {
            configured: true,
            min_seconds_between_responses: p.min_seconds_between_responses,
            max_responses_per_session: p.max_responses_per_session,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. configure-rate-limiter is
    // host-driven cognition IPC, so it is Internal — registered and grid-routable,
    // never a remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(
            ConfigureRateLimiter::NAME,
            "cognition/configure-rate-limiter"
        );
        assert_eq!(ConfigureRateLimiter::ACCESS, AccessLevel::Internal);
    }
}
