//! Her act's reads with no range share her view's measured per-act headroom (plan S1; replay
//! #5488): each gets an equal part as its page budget, and `code/read` answers a read larger
//! than its part with the first page and a line naming the rest
//! ([`PageBudget`](crate::code::file_engine::PageBudget)). The tool answers a smaller question;
//! her view never cuts a result.
//!
//! The headroom is the view's own measurement ([`WorkingMemory::view_headroom`], the same
//! number `start_grants` reserves for one act's growth), never a constant: until an act of the
//! epoch is measured there is none, and every read is whole. Each new epoch starts unmeasured,
//! so a page size that fell with her acts is tested whole again at every turnover.
//!
//! Only the DISPATCHED copy carries the budget. Her own calls, which repeat identity is built
//! from (`ToolCall::canonical_input`), stay exactly as she made them.
//!
//! [`WorkingMemory::view_headroom`]: crate::cognition::working_memory::WorkingMemory::view_headroom

use std::borrow::Cow;

use continuum_hands::modules::code_commands::CodeRead;

use crate::ai::types::ToolCall;
use crate::code::file_engine::PageBudget;
use crate::sdk_codegen::ActionCommand;

fn reads_unranged(call: &ToolCall) -> bool {
    crate::ai::tool_dialect::resolve_wire_name(&call.name) == CodeRead::NAME && !PageBudget::names_a_range(&call.input)
}

/// `calls` as the hands should run them: each `code/read` with no range carrying its equal
/// share of `headroom`. Borrowed unchanged when nothing is measured or nothing reads unranged.
pub(super) fn with_page_budgets(calls: &[ToolCall], headroom: Option<usize>) -> Cow<'_, [ToolCall]> {
    let reads = calls.iter().filter(|call| reads_unranged(call)).count();
    let Some(budget) = headroom.zip((reads > 0).then_some(reads)).and_then(|(chars, reads)| PageBudget::new(chars / reads)) else {
        return Cow::Borrowed(calls);
    };
    Cow::Owned(
        calls
            .iter()
            .cloned()
            .map(|mut call| {
                if reads_unranged(&call) {
                    budget.stamp(&mut call.input);
                }
                call
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(id: &str, name: &str, input: serde_json::Value) -> ToolCall {
        ToolCall { id: id.into(), name: name.into(), input }
    }

    fn budget_of(call: &ToolCall) -> Option<u64> {
        call.input.get(PageBudget::FIELD).and_then(serde_json::Value::as_u64)
    }

    // what this catches: a read paged before her view has measured an act (a constant in
    // disguise), or a budget stamped on her calls when nothing reads unranged.
    #[test]
    fn with_no_measured_headroom_every_read_is_whole() {
        let calls = vec![call("a", "code/read", serde_json::json!({"file_path": "src/lib.rs"}))];
        let dispatched = with_page_budgets(&calls, None);
        assert!(matches!(dispatched, Cow::Borrowed(_)));
        assert_eq!(budget_of(&dispatched[0]), None);
    }

    // what this catches: the act's unranged reads not sharing the headroom equally, a ranged
    // read or another verb being paged, or her own calls (what repeat detection reads) changed.
    #[test]
    fn unranged_reads_split_the_headroom_and_nothing_else_is_touched() {
        let calls = vec![
            call("a", "code/read", serde_json::json!({"file_path": "a.rs"})),
            call("b", "code/read", serde_json::json!({"file_path": "b.rs", "start_line": 40, "end_line": 80})),
            call("c", "code/read", serde_json::json!({"file_path": "c.rs"})),
            call("d", "code/shell", serde_json::json!({"cmd": "ls"})),
        ];
        let dispatched = with_page_budgets(&calls, Some(9_000));
        let budgets: Vec<Option<u64>> = dispatched.iter().map(budget_of).collect();
        assert_eq!(budgets, vec![Some(4_500), None, Some(4_500), None]);
        assert!(calls.iter().all(|call| budget_of(call).is_none()), "her own calls stay as she made them");
    }
}
