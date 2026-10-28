//! `serving/handoff-close` and `serving/handoff-reopen`: a core handoff closes training admission
//! before it reads the node's hold, so no run is admitted between that read and the stop
//! (card 91921d0f, Codex on #5551). The close lives in the engine's lifecycle gate
//! ([`crate::inference::serving_lifecycle::LifecycleGate::close_for_handoff`]); these are its
//! command surface for the CLI that performs the handoff.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::inference::serving_lifecycle::{HandoffCloseOutcome, HandoffOwner, LifecycleGate};

/// Params for `serving/handoff-close`: the process performing the handoff, which owns the close
/// (its death releases it).
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/serving/ServingHandoffCloseParams.ts")]
pub struct ServingHandoffCloseParams {
    pub owner_pid: u32,
}

/// What the close came to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/serving/ServingHandoffClose.ts")]
pub enum ServingHandoffClose {
    /// Admission is closed until `serving/handoff-reopen` with this token, or the owner's death.
    Closed {
        #[ts(type = "string")]
        token: uuid::Uuid,
    },
    /// A serving operation (a relaunch, a placement move) holds the gate now: nothing is closed.
    /// The handoff waits and asks again.
    Busy,
    /// Another handoff, whose process still runs, holds admission closed: nothing changed.
    Taken {
        #[serde(rename = "ownerPid")]
        owner_pid: u32,
    },
    /// This core has no serving lifecycle, so it admits no in-engine training: nothing to close.
    NoServing,
}

crate::action_command! {
    /// Close in-engine training admission for a core handoff, owned by the calling process. Any
    /// admission deciding now finishes first (its hold is then in `serving/status`), and later
    /// ones wait until `serving/handoff-reopen` or the owner's death. Operator-only.
    pub struct ServingHandoffCloseCommand;
    name: "serving/handoff-close",
    access: Privileged,
    params: ServingHandoffCloseParams,
    output: ServingHandoffClose,
    run(_this, _ctx, p) => {
        let Some(gate) = LifecycleGate::global() else { return Ok(ServingHandoffClose::NoServing) };
        let owner = HandoffOwner::of(p.owner_pid).ok_or_else(|| crate::sdk_codegen::CommandError::Invalid(format!(
            "process {} has no readable start time, so its death could never release the close", p.owner_pid
        )))?;
        Ok(match gate.close_for_handoff(owner) {
            HandoffCloseOutcome::Closed { token } => ServingHandoffClose::Closed { token },
            HandoffCloseOutcome::Busy => ServingHandoffClose::Busy,
            HandoffCloseOutcome::Taken { owner } => ServingHandoffClose::Taken { owner_pid: owner.pid },
        })
    }
}

/// Params for `serving/handoff-reopen`: the token its close returned.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/serving/ServingHandoffReopenParams.ts")]
pub struct ServingHandoffReopenParams {
    #[ts(type = "string")]
    pub token: uuid::Uuid,
}

/// Whether that close stood and is now reopened. `false` on a core that never held it (a
/// restarted or restored core boots open) or after a newer close replaced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/serving/ServingHandoffReopen.ts")]
pub struct ServingHandoffReopen {
    pub reopened: bool,
}

crate::action_command! {
    /// Reopen in-engine training admission after a handoff that did not stop the core (refused,
    /// deferred or failed before the stop). Operator-only.
    pub struct ServingHandoffReopenCommand;
    name: "serving/handoff-reopen",
    access: Privileged,
    params: ServingHandoffReopenParams,
    output: ServingHandoffReopen,
    run(_this, _ctx, p) => {
        let reopened = LifecycleGate::global().is_some_and(|gate| gate.reopen_after_handoff(p.token));
        Ok(ServingHandoffReopen { reopened })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the handoff pair is an operator surface (it stops a citizen's training
    // from being admitted), Privileged, never a persona tool.
    #[test]
    fn name_and_access_wired() {
        use crate::sdk_codegen::{AccessLevel, ActionCommand};
        assert_eq!(ServingHandoffCloseCommand::NAME, "serving/handoff-close");
        assert_eq!(ServingHandoffReopenCommand::NAME, "serving/handoff-reopen");
        assert!(matches!(ServingHandoffCloseCommand::ACCESS, AccessLevel::Privileged));
        assert!(matches!(ServingHandoffReopenCommand::ACCESS, AccessLevel::Privileged));
    }
}
