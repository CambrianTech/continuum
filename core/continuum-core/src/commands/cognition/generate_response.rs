//! `cognition/generate-response` — local response generation (typed, stateless).
//!
//! Oxidized generation arm: composes the persona's reply by running the response
//! model over an [`AIDecisionContext`](crate::cognition::should_respond::AIDecisionContext)
//! (reused as the gating context), under a Rust-owned admission policy. Holds no module
//! state — [`evaluate_response`] is a free async function over the request — so this is a
//! stateless [`ActionCommand`](crate::sdk_codegen::ActionCommand) unit struct:
//! `action_command!` publishes both the descriptor and the runtime object via
//! `inventory`, no `commands()` ceremony.
//!
//! `access: Internal` — substrate cognition IPC the host invokes to generate a turn,
//! NOT a persona toolbelt verb. Registered and grid-routable, but the trust policy
//! denies remote peers from driving another node's generation.

use crate::cognition::generate_response::{
    evaluate_response, GenerateResponseRequest, GenerateResponseResult,
};
use crate::sdk_codegen::CommandError;

crate::action_command! {
    /// Generate the persona's reply for a turn. Runs the response model over the
    /// decision context under a Rust-owned admission policy and returns the composed
    /// text plus generation provenance. Host-invoked per turn; not a persona toolbelt
    /// verb.
    pub struct GenerateResponse;
    name: "cognition/generate-response",
    access: Internal,
    params: GenerateResponseRequest,
    output: GenerateResponseResult,
    run(_this, _ctx, req) => {
        evaluate_response(req)
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. `cognition/generate-response` is
    // host-driven cognition IPC (invoked per turn), so it is Internal — registered and
    // grid-routable, but never a remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(GenerateResponse::NAME, "cognition/generate-response");
        assert_eq!(GenerateResponse::ACCESS, AccessLevel::Internal);
    }
}
