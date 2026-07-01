//! `cognition/should-respond` — LLM-gated respond/skip decision (typed, stateless).
//!
//! Oxidized gating arm: runs the gating model over an [`AIDecisionContext`] (persona,
//! room, trigger message, RAG history) and returns a structured [`AIGatingDecision`].
//! It holds no module state — [`evaluate_gating`] is a free async function over the
//! request — so this is a stateless [`ActionCommand`](crate::sdk_codegen::ActionCommand)
//! unit struct: `action_command!` publishes both the descriptor and the runtime object
//! via `inventory`, no `commands()` ceremony.
//!
//! `access: Internal` — this is substrate cognition IPC the host invokes per trigger,
//! NOT a persona toolbelt verb. It is registered and grid-routable, but the trust
//! policy denies remote peers from driving another node's gating decision.

use crate::cognition::should_respond::{evaluate_gating, AIGatingDecision, ShouldRespondRequest};
use crate::sdk_codegen::CommandError;

crate::action_command! {
    /// Decide whether the persona should respond to a triggering message. Runs the
    /// gating model over the decision context (persona, room, trigger, RAG history)
    /// and returns a structured decision: should_respond + confidence + reason +
    /// factors. Host-invoked per trigger; not a persona toolbelt verb.
    pub struct ShouldRespond;
    name: "cognition/should-respond",
    access: Internal,
    params: ShouldRespondRequest,
    output: AIGatingDecision,
    run(_this, _ctx, req) => {
        evaluate_gating(req)
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. `cognition/should-respond` is
    // host-driven cognition IPC (invoked per trigger), so it is Internal — registered
    // and grid-routable, but never a remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(ShouldRespond::NAME, "cognition/should-respond");
        assert_eq!(ShouldRespond::ACCESS, AccessLevel::Internal);
    }
}
