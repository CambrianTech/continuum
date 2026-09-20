//! `cognition/validate-response-decision` — validate a composed reply before it ships
//! (typed, stateless).
//!
//! Oxidized validation arm: judges a generated response against the original question
//! (relevance, safety, coherence) and returns a structured
//! [`ValidateResponseDecision`]. Holds no module state —
//! [`evaluate_validate_response`] is a free async function over the request — so this is
//! a stateless [`ActionCommand`](crate::sdk_codegen::ActionCommand) unit struct:
//! `action_command!` publishes both the descriptor and the runtime object via
//! `inventory`, no `commands()` ceremony.
//!
//! Distinct from `cognition/validate-response` (persona-level response validation): this
//! is the standalone decision oxidizer that replaced the TS
//! `AIValidateResponseServerCommand`.
//!
//! `access: Internal` — substrate cognition IPC the host invokes before shipping a turn,
//! NOT a persona toolbelt verb. Registered and grid-routable, but the trust policy
//! denies remote peers from driving another node's validation.

use crate::cognition::validate_response::{
    evaluate_validate_response, ValidateResponseDecision, ValidateResponseRequest,
};
use crate::sdk_codegen::CommandError;

crate::action_command! {
    /// Validate a composed reply against the original question before it ships. Judges
    /// relevance, safety, and coherence and returns a structured decision plus
    /// provenance. Host-invoked before shipping a turn; not a persona toolbelt verb.
    pub struct ValidateResponseDecisionCommand;
    name: "cognition/validate-response-decision",
    access: Internal,
    params: ValidateResponseRequest,
    output: ValidateResponseDecision,
    run(_this, _ctx, req) => {
        evaluate_validate_response(req)
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. `cognition/validate-response-decision`
    // is host-driven cognition IPC (validates a turn before it ships), so it is Internal —
    // registered and grid-routable, but never a remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(
            ValidateResponseDecisionCommand::NAME,
            "cognition/validate-response-decision"
        );
        assert_eq!(
            ValidateResponseDecisionCommand::ACCESS,
            AccessLevel::Internal
        );
    }
}
