//! `serving/plan` — the current serving DECISION (intent), for personas and
//! operators to inspect.
//!
//! Dep-holding: reads the daemon's published serving-plan
//! [`watch::Receiver`] — a cheap borrow, no recompute. The `rationale` field
//! explains the "why" in plain words. This is the intent; `serving/status` is
//! whether the intent became reality.

use serde::Serialize;
use tokio::sync::watch;
use ts_rs::TS;

use crate::cognition::serving_plan::ServingPlan;

/// Params for `serving/plan` — none (the current plan is returned).
#[derive(Debug, Clone, Default, Serialize, serde::Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/serving/ServingPlanParams.ts"
)]
pub struct ServingPlanParams {}

/// Result for `serving/plan` — the current decision, or `null` before the first
/// plan is computed. A named wrapper (the typed registry requires a named Result
/// type; a bare `Option<T>` has no importable TS dependency).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/serving/ServingPlanResult.ts"
)]
pub struct ServingPlanResult {
    /// The serving decision for this host, or `None` before the daemon's first
    /// plan recompute.
    #[ts(optional)]
    pub plan: Option<ServingPlan>,
}

crate::action_command! {
    /// The current serving DECISION for this host: the base model to serve, the
    /// host-fit served context window, the continuous-batching lane count, how many
    /// models stay resident, whether it fits on GPU, and a plain-words rationale.
    /// `null` before the daemon's first plan recompute. The intent; `serving/status`
    /// reports whether it became reality. Read-only.
    pub struct ServingPlanQuery { plan: watch::Receiver<Option<ServingPlan>> }
    name: "serving/plan",
    access: Privileged,
    params: ServingPlanParams,
    output: ServingPlanResult,
    run(this, _ctx, _p) => {
        Ok(ServingPlanResult { plan: this.plan.borrow().clone() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    fn cmd(plan: Option<ServingPlan>) -> ServingPlanQuery {
        let (_tx, rx) = watch::channel(plan);
        ServingPlanQuery { plan: rx }
    }

    // what this catches: name/access wiring — the serving plan is an operator/UI
    // read surface, Privileged, not a persona toolbelt action.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(ServingPlanQuery::NAME, "serving/plan");
        assert!(matches!(
            ServingPlanQuery::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }

    // what this catches: before the first recompute the plan is None — the query
    // surfaces that honestly (null) rather than fabricating a decision.
    #[tokio::test]
    async fn no_plan_yet_returns_none() {
        let out = cmd(None)
            .run(&Ctx::default(), ServingPlanParams::default())
            .await
            .expect("plan read must succeed");
        assert!(
            out.plan.is_none(),
            "no decision before the daemon computes one"
        );
    }

    // what this catches: the body returns the published decision from the captured
    // receiver — a regression that drops the plan instead of surfacing it is caught.
    #[tokio::test]
    async fn returns_the_published_plan() {
        let plan = ServingPlan {
            base_model: crate::cognition::serving_plan::ModelFootprint {
                model_id: "qwen3-coder".into(),
                weights_bytes: 0,
                kv_per_token: 0,
                context_window: 32_768,
                capability_rank: 0,
            },
            served_context_window: 32_768,
            lanes: 2,
            grid_overflow_lanes: 0,
            resident_models: 1,
            fits_on_gpu: true,
            rationale: "fits one lane on the budget".into(),
        };
        let out = cmd(Some(plan.clone()))
            .run(&Ctx::default(), ServingPlanParams::default())
            .await
            .expect("plan read must succeed");
        assert_eq!(out.plan.expect("plan present").base_model_id, "qwen3-coder");
        assert_eq!(
            out.plan.expect("plan present").base_model.model_id,
            "qwen3-coder"
        );
    }
}
