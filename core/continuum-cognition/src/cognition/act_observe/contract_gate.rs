//! A call on a held verb whose contract moved since her head froze, and that no served prompt has
//! told her, is refused before it runs (card 9cf06ade): never executed under a contract she was not
//! shown, whether its schema changed or only its meaning did. Both of her dispatch routes in
//! `apply_act` (a long command dispatched to the background, and the foreground batch) read this one
//! gate first, so neither bypasses it. A verb she WAS told about runs, under its current contract,
//! which its command's own decoder and its authorization enforce as for every call.

use super::observation::{extract_paths, ActStatus, Observation, ToolOutput, ToolVerb};
use crate::ai::types::{CallBatch, Disposed, ToolCall, ToolResult};
use crate::cognition::hands_custody::ContractChange;

/// Each committed call naming a verb whose contract change she has not been told, with that change.
pub(super) fn held_back(batch: &CallBatch, untold: &[ContractChange]) -> Vec<(ToolCall, ContractChange)> {
    if untold.is_empty() {
        return Vec::new();
    }
    batch
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            Disposed::Committed(committed) => {
                let canonical = crate::ai::tool_dialect::resolve_wire_name(&committed.call().name);
                untold.iter().find(|change| change.name == canonical).map(|change| (committed.call().clone(), change.clone()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::hands_custody::LiveContract;

    // what this catches (card 9cf06ade, Codex's review): a call on a verb whose contract moved,
    // before a served prompt told her, run anyway; or a call on any other verb held with it.
    #[test]
    fn a_call_on_an_untold_contract_is_held_back_and_answered_with_what_moved() {
        let change = ContractChange { name: "work/note".into(), known: None, now: LiveContract::Removed };
        let call = |id: &str, name: &str| ToolCall { id: id.into(), name: name.into(), input: serde_json::json!({}) };
        let origin = crate::ai::types::CallOrigin::Generated(crate::ai::types::GenerationEnd::Finished);
        let batch = CallBatch::classify([call("a", "work_note"), call("b", "code_read")], origin);
        let held = held_back(&batch, std::slice::from_ref(&change));
        assert_eq!(held.iter().map(|(call, _)| call.id.as_str()).collect::<Vec<_>>(), ["a"], "only the untold verb's call");
        let observed = refused(&held[0].0, &held[0].1);
        assert_eq!(observed.output.result.is_error, Some(true));
        assert!(observed.output.result.content.starts_with("Not run:"), "{}", observed.output.result.content);
        assert!(held_back(&batch, &[]).is_empty(), "nothing untold holds nothing");
    }
}
