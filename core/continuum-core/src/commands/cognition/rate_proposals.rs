//! `cognition/rate-proposals` — AI peer-review rating of response proposals (typed,
//! stateless).
//!
//! Oxidized rater arm (continuum#1289 PR-2): given the original message, recent
//! conversation, and the competing response proposals, runs the reviewer model to score
//! each proposal (0.0..1.0, should-post, reasoning). Holds no module state —
//! [`rate_proposals_with_ai`] is a free async function over the request — so this is a
//! stateless [`ActionCommand`](crate::sdk_codegen::ActionCommand) unit struct:
//! `action_command!` publishes both the descriptor and the runtime object via `inventory`,
//! no `commands()` ceremony.
//!
//! Fail-loud note: a rater that produces no usable judgment surfaces as
//! `CommandError::Internal`; the chat substrate skips peer-review for that round rather
//! than fabricating a degraded score. No fallback rating is invented here.
//!
//! `access: Internal` — substrate cognition IPC the host invokes during a peer-review
//! pass, NOT a persona toolbelt verb.

use crate::cognition::rate_proposals::{
    rate_proposals_with_ai, RateProposalsRequest, RateProposalsResponse,
};
use crate::sdk_codegen::CommandError;

crate::action_command! {
    /// Score competing response proposals for a turn via AI peer review. Given the
    /// original message, recent conversation, and proposals, returns each proposal's
    /// score, should-post flag, and reasoning. Host-invoked during a peer-review pass;
    /// not a persona toolbelt verb.
    pub struct RateProposals;
    name: "cognition/rate-proposals",
    access: Internal,
    params: RateProposalsRequest,
    output: RateProposalsResponse,
    run(_this, _ctx, req) => {
        rate_proposals_with_ai(req)
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. rate-proposals is host-driven
    // peer-review cognition IPC, so it is Internal — registered and grid-routable, never
    // a remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(RateProposals::NAME, "cognition/rate-proposals");
        assert_eq!(RateProposals::ACCESS, AccessLevel::Internal);
    }
}
