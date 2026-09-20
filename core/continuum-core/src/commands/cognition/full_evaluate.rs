//! `cognition/full-evaluate` — the unified 6-gate response evaluation (typed, dep-holding).
//!
//! ONE IPC call that replaces the five legacy TS gates: response-cap, sleep-mode,
//! self-message, fast-path, and deferred-LLM. Takes the persona's live cognition state
//! (rate limiter + sleep state + engine + message cache) under a single [`DashMap`] read
//! lock and runs [`evaluator::full_evaluate`](crate::persona::evaluator::full_evaluate) over
//! the request, returning the decision (should-respond, gate, confidence, social signals).
//! Captures the owning module's [`CognitionState`](crate::modules::cognition::CognitionState).
//!
//! Wire note: the params ARE a [`FullEvaluateRequest`] — the whole payload deserializes in
//! one step, matching the legacy arm that read each field via `p.uuid`/`p.str`/`p.bool_or`
//! and parsed `sender_type` by hand. `SenderType` is `#[serde(rename_all = "lowercase")]`,
//! so `"human"`/`"persona"`/`"agent"`/`"system"` deserialize exactly as the old
//! `parse_sender_type` mapped them, and serde fails loud on an unknown variant (same as the
//! legacy `Err`). The three fields the legacy arm defaulted (`persona_unique_id`,
//! `is_voice`, `sender_is_human`) carry `#[serde(default)]` so an omitting caller is still
//! accepted.
//!
//! Fail-loud note: a request naming a persona with no live cognition engine is a
//! `CommandError::NotFound` (never lazily creates one — this is a read of live state).
//!
//! `access: Internal` — host-driven cognition IPC (the chat gate calls it per candidate
//! responder), not a persona toolbelt verb.

use std::sync::Arc;

use crate::modules::cognition::CognitionState;
use crate::persona::evaluator::{self, FullEvaluateRequest, FullEvaluateResult};
use crate::sdk_codegen::CommandError;

crate::action_command! {
    /// Run the unified 6-gate response evaluation for one persona against one message:
    /// response-cap, sleep-mode, self-message, fast-path, and deferred-LLM gates under a
    /// single lock. Returns should-respond, the deciding gate, confidence, and the social
    /// signals passed to the LLM as context. Host-invoked per candidate responder; not a
    /// persona toolbelt verb.
    pub struct FullEvaluate { state: Arc<CognitionState> }
    name: "cognition/full-evaluate",
    access: Internal,
    params: FullEvaluateRequest,
    output: FullEvaluateResult,
    run(this, _ctx, req) => {
        // Single lock — atomic access to engine + rate_limiter + sleep_state + cache.
        // Read of live state: fail loud if the persona has no cognition engine.
        let persona = this
            .state
            .personas
            .get(&req.persona_id)
            .ok_or_else(|| CommandError::NotFound(format!("No cognition for {}", req.persona_id)))?;

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let result = evaluator::full_evaluate(
            &req,
            &persona.rate_limiter,
            &persona.sleep_state,
            &persona.engine,
            &persona.message_cache,
            now_ms,
        );

        crate::log_info!(
            "module",
            "cognition",
            "full-evaluate {}: respond={}, gate={}, confidence={:.2} ({:.2}ms)",
            req.persona_id,
            result.should_respond,
            result.gate,
            result.confidence,
            result.decision_time_ms
        );

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. full-evaluate is host-driven gating
    // cognition IPC, so it is Internal — registered and grid-routable, never a
    // remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(FullEvaluate::NAME, "cognition/full-evaluate");
        assert_eq!(FullEvaluate::ACCESS, AccessLevel::Internal);
    }
}
