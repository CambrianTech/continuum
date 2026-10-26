//! `cognition/check-redundancy` — "is my draft response redundant?" check (typed, stateless).
//!
//! Companion to [`super::should_respond`]: after gating says "respond", this asks the
//! model whether a drafted reply merely repeats what's already been said, given the
//! conversation history in the shared [`AIDecisionContext`]. Holds no module state —
//! [`evaluate_redundancy`] is a free async function over the request — so it is a
//! stateless [`ActionCommand`](crate::sdk_codegen::ActionCommand) unit struct.
//!
//! `access: Internal` — host-driven cognition IPC, not a persona toolbelt verb.

use crate::cognition::check_redundancy::{
    evaluate_redundancy, RedundancyCheckRequest, RedundancyDecision,
};
use crate::sdk_codegen::CommandError;

crate::action_command! {
    /// Check whether a drafted response is redundant with the conversation so far.
    /// Runs the redundancy model over the decision context + draft text and returns
    /// is_redundant + reason. Host-invoked in the respond pipeline; not a persona
    /// toolbelt verb.
    pub struct CheckRedundancy;
    name: "cognition/check-redundancy",
    access: Internal,
    params: RedundancyCheckRequest,
    output: RedundancyDecision,
    run(_this, _ctx, req) => {
        evaluate_redundancy(req)
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. Redundancy-check is host-driven
    // cognition IPC in the respond pipeline, so it is Internal — registered and
    // grid-routable, never a remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(CheckRedundancy::NAME, "cognition/check-redundancy");
        assert_eq!(CheckRedundancy::ACCESS, AccessLevel::Internal);
    }
}
