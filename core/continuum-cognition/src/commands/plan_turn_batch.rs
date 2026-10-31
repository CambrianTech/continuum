//! `cognition/plan-turn-batch` — deterministic turn-batching plan (typed, stateless).
//!
//! Pure planning command: no ORM, no inference, no file I/O. The host supplies the
//! trigger, candidate personas, and active RAG-source policies; Rust returns the
//! deterministic turn key, shared-RAG load plan, per-persona work items (generation
//! order/wave, cache keys, budgets, ETAs), and the fan-out/admission policy — so the
//! caller stays a wrapper instead of inventing per-persona batching behavior.
//!
//! Holds no module state — [`plan_turn_batch`] is a pure sync free function — so this is
//! a stateless [`ActionCommand`](crate::sdk_codegen::ActionCommand) unit struct:
//! `action_command!` publishes both the descriptor and the runtime object via `inventory`,
//! no `commands()` ceremony.
//!
//! Wire note: the params ARE a [`RecipeTurnBatchRequest`] (the typed shape), not the
//! legacy `{ request: {...} }` envelope the `handle_command` arm unwrapped with
//! `p.json("request")`. The typed path deserializes the request directly and fails loud
//! on a malformed payload.
//!
//! `access: Internal` — host-driven cognition IPC that plans a turn, NOT a persona
//! toolbelt verb.

use crate::cognition::{plan_turn_batch, RecipeTurnBatchPlan, RecipeTurnBatchRequest};

crate::action_command! {
    /// Plan one cognition turn: given the trigger, candidate personas, and RAG-source
    /// policies, return the deterministic turn key, shared-RAG load plan, per-persona work
    /// items (order/wave, cache keys, budgets, ETAs), and the local-generation fan-out
    /// policy. Pure planning — no inference, ORM, or file I/O. Host-invoked; not a persona
    /// toolbelt verb.
    pub struct PlanTurnBatch;
    name: "cognition/plan-turn-batch",
    access: Internal,
    params: RecipeTurnBatchRequest,
    output: RecipeTurnBatchPlan,
    run(_this, _ctx, req) => {
        Ok(plan_turn_batch(req))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. plan-turn-batch is host-driven turn
    // planning, so it is Internal — registered and grid-routable, never a remote-callable
    // persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(PlanTurnBatch::NAME, "cognition/plan-turn-batch");
        assert_eq!(PlanTurnBatch::ACCESS, AccessLevel::Internal);
    }
}
