//! A call on a held verb whose contract is not the one she was shown is refused before it runs (card
//! 9cf06ade): never executed under a contract she was not shown, whether its schema changed or only
//! its meaning did. The check is made at dispatch, per call: the contract she knows for the verb
//! (what a served prompt told her, else what her head froze) against the contract her current surface
//! offers, by the identity its build computed once (`WorkingMemory::live_contract`, a lookup and an
//! id compare, never a per-call re-derivation), so a surface replaced while her generation is in
//! flight is caught too. Both of her dispatch routes in `apply_act` (a long command dispatched to
//! the background, and the foreground batch) take the batch only after this gate, so neither
//! bypasses it. A verb her head does not hold (one loaded into her tail, or called by name) runs
//! under its current contract, which its own decoder and authorization enforce as for every call.

use super::observation::{extract_paths, ActStatus, Observation, ToolOutput, ToolVerb};
use crate::ai::types::{CallBatch, Disposed, ToolCall, ToolResult};
use crate::cognition::hands_custody::ContractChange;
use crate::cognition::working_memory::WorkingMemory;

/// A committed call held back, and the contract change she was not shown that holds it.
pub(super) struct HeldCall {
    pub(super) call: ToolCall,
    pub(super) change: ContractChange,
}

/// Each committed call on a held verb whose current contract is not the one she knows.
pub(super) fn held_back(batch: &CallBatch, memory: &WorkingMemory) -> Vec<HeldCall> {
    batch
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            Disposed::Committed(committed) => {
                let canonical = crate::ai::tool_dialect::resolve_wire_name(&committed.call().name).to_string();
                let known = memory.held_contract(&canonical)?.known();
                let now = memory.live_contract(&canonical)?;
                (now.id() != known).then(|| HeldCall { call: committed.call().clone(), change: ContractChange { name: canonical, known, now } })
            }
            _ => None,
        })
        .collect()
}

/// A refused call as she observes it: that it did not run, and what moved.
pub(super) fn refused(call: &ToolCall, change: &ContractChange) -> Observation {
    continuum_base::probe!(
        class = "tool.call.contract_untold",
        verb = %change.name,
        key = %change.key(),
        "a call on a verb whose contract moved, before a served prompt told her: refused, never run under a contract she was not shown"
    );
    Observation {
        call: call.clone(),
        output: ToolOutput {
            image: None,
            additional_images: None,
            result: ToolResult {
                tool_use_id: call.id.clone(),
                content: format!("Not run: {}", change.text()),
                custody: crate::ai::types::ResultCustody::Unsaved,
                is_error: Some(true),
            },
            verb: ToolVerb::classify(&call.name),
            paths: extract_paths(&call.input),
            verdict: crate::sdk_codegen::ActVerdict::Declared(crate::sdk_codegen::ToolVerdict::Failed),
        },
        status: ActStatus::Executed,
    }
}

/// Memory whose held head froze `verb` under a contract its owner no longer declares: the fixture
/// this gate's tests and `apply_act`'s share.
#[cfg(test)]
pub(super) fn holding_an_older(verb: &str) -> WorkingMemory {
    let memory = WorkingMemory::new(8);
    // her live surface, as her faculty shares it: the gate reads each verb's identity from it
    let offers = crate::cognition::persona_tools::native_offers();
    let authorized = offers.iter().map(|offer| &offer.spec).cloned().collect();
    let surface = crate::cognition::tool_surface::ToolSurface::build(offers, authorized, crate::ai::tool_dialect::to_wire_spec, |_| continuum_base::tool_contract::ContractRevision::FIRST);
    memory.set_live_surface(std::sync::Arc::new(surface));
    let none: std::sync::Arc<[crate::ai::types::NativeToolSpec]> = Vec::new().into();
    memory.keep_custody(None, crate::cognition::hands_custody::HandsCustody::Held(crate::cognition::hands_custody::HeldHands {
        generation: 1,
        names: vec![verb.to_string()],
        levels: vec![0],
        reason: crate::cognition::tool_surface::SurfaceReason::Hands,
        moves: Vec::new(),
        waiting: Vec::new(),
        menu: Default::default(),
        whole: std::sync::Arc::clone(&none),
        sent: none,
        sent_tokens: 0,
        demand_tokens: 0,
        contracts: vec![crate::cognition::hands_custody::VerbContract::frozen(Some(continuum_base::tool_contract::ContractId::of(continuum_base::tool_contract::ContractRevision::FIRST, b"an older schema")))],
    }));
    memory
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card 9cf06ade, Codex's review): a call on a held verb whose current contract
    // is not the one she was shown, run anyway; or a call on a verb her head does not hold, held.
    #[test]
    fn a_call_on_a_contract_she_was_not_shown_is_held_back_and_answered() {
        let memory = holding_an_older("work/note");
        let call = |id: &str, name: &str| ToolCall { id: id.into(), name: name.into(), input: serde_json::json!({}) };
        let origin = crate::ai::types::CallOrigin::Generated(crate::ai::types::GenerationEnd::Finished);
        let batch = CallBatch::classify([call("a", "work/note"), call("b", "code_read")], origin);
        let held = held_back(&batch, &memory);
        assert_eq!(held.iter().map(|held| held.call.id.as_str()).collect::<Vec<_>>(), ["a"], "only the held verb whose contract moved");
        let observed = refused(&held[0].call, &held[0].change);
        assert_eq!(observed.output.result.is_error, Some(true));
        assert!(observed.output.result.content.starts_with("Not run:"), "{}", observed.output.result.content);
        assert!(held_back(&batch, &WorkingMemory::new(8)).is_empty(), "no held head holds nothing back");
    }
}
