//! `cognition/check-adequacy` — post-inference check for a redundant answer (typed,
//! stateless).
//!
//! Oxidized adequacy arm: given the original question and the recent AI responses already
//! on the wire, decides whether one of them already answers it well enough that this
//! persona should stay quiet (with confidence, reason, and the responder that covered it).
//! Holds no module state — [`check_response_adequacy`] is a pure sync free function — so
//! this is a stateless [`ActionCommand`](crate::sdk_codegen::ActionCommand) unit struct:
//! `action_command!` publishes both the descriptor and the runtime object via `inventory`,
//! no `commands()` ceremony.
//!
//! Fail-loud note: the legacy arm `filter_map(..ok())`-dropped malformed response entries
//! silently. The typed `Vec<RecentResponse>` deserialize rejects a malformed batch up
//! front instead — a bad payload is a caller bug, not a silently-shrunk check.
//!
//! `access: Internal` — substrate cognition IPC the host invokes after inference, NOT a
//! persona toolbelt verb.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::persona::evaluator::{check_response_adequacy, AdequacyResult, RecentResponse};

/// The original question plus the recent responses to weigh it against.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/CheckAdequacyRequest.ts"
)]
pub struct CheckAdequacyRequest {
    /// The original question being answered.
    pub original_text: String,
    /// Recent AI responses already on the wire to check for redundancy.
    pub responses: Vec<RecentResponse>,
}

crate::action_command! {
    /// Check whether a recent AI response already adequately answers the original question
    /// (so this persona can stay quiet). Returns adequacy, confidence, reason, and the
    /// responder that covered it. Host-invoked after inference; not a persona toolbelt verb.
    pub struct CheckAdequacy;
    name: "cognition/check-adequacy",
    access: Internal,
    params: CheckAdequacyRequest,
    output: AdequacyResult,
    run(_this, _ctx, p) => {
        Ok(check_response_adequacy(&p.original_text, &p.responses))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. check-adequacy is host-driven
    // post-inference cognition IPC, so it is Internal — registered and grid-routable,
    // never a remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(CheckAdequacy::NAME, "cognition/check-adequacy");
        assert_eq!(CheckAdequacy::ACCESS, AccessLevel::Internal);
    }
}
