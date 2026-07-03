//! `cognition/score-interaction` — score an input→output interaction for training-data
//! selection (typed, stateless).
//!
//! Oxidized quality-scoring arm: rates a single interaction (0.0–1.0 overall plus the
//! per-factor breakdown — human feedback, task success, substance, correction) so the
//! genome loop can pick high-quality examples to fine-tune on. Holds no module state —
//! [`score_interaction_quality`] is a pure sync free function — so this is a stateless
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand) unit struct: `action_command!`
//! publishes both the descriptor and the runtime object via `inventory`, no `commands()`
//! ceremony.
//!
//! `access: Internal` — substrate cognition IPC the host invokes when curating training
//! data, NOT a persona toolbelt verb.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::persona::domain_classifier::{score_interaction_quality, QualityScore};

/// One interaction to score: the prompt, the produced reply, and optional outcome signals.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ScoreInteractionRequest.ts"
)]
pub struct ScoreInteractionRequest {
    /// The prompt / user input.
    pub input: String,
    /// The response produced for that input.
    pub output: String,
    /// Optional human-feedback signal (a reply, "thanks", a correction).
    #[ts(optional)]
    pub feedback: Option<String>,
    /// Optional task-outcome signal (did the interaction accomplish its goal).
    #[ts(optional)]
    pub task_success: Option<bool>,
}

crate::action_command! {
    /// Score an input→output interaction for training-data selection. Returns an overall
    /// 0.0–1.0 quality score plus the per-factor breakdown (human feedback, task success,
    /// substance, correction). Host-invoked when curating training data; not a persona
    /// toolbelt verb.
    pub struct ScoreInteraction;
    name: "cognition/score-interaction",
    access: Internal,
    params: ScoreInteractionRequest,
    output: QualityScore,
    run(_this, _ctx, p) => {
        Ok(score_interaction_quality(
            &p.input,
            &p.output,
            p.feedback.as_deref(),
            p.task_success,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. score-interaction curates training
    // data on the host side, so it is Internal — registered and grid-routable, never a
    // remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(ScoreInteraction::NAME, "cognition/score-interaction");
        assert_eq!(ScoreInteraction::ACCESS, AccessLevel::Internal);
    }
}
