#![cfg(test)]

use super::*;

fn faculty_with_window(window: u32) -> LlmDeliberationFaculty {
    LlmDeliberationFaculty::new(
        Uuid::new_v4(),
        "Sahar",
        "You are Sahar.",
        Arc::new(HeuristicInferenceAdapter::new()),
    )
    .with_context_window(window)
}

// what this catches: the live 2026-09-10 mute, and it is the THIRD instance of
// one class. A `code/read` / `code/shell` output goes in as REQUIRED input, and
// it was unbounded — so one big result claimed the whole window and
// `fit_messages` refused every turn. Measured: required_payload 18,629 against
// after_reserves 15,688 on a 23,040 lane, grounding only 113 (innocent),
// received = 2 (ONE enormous message), 836 consecutive refusals, zero acts for
// a whole benchmark round.
#[test]
fn an_oversized_result_is_split_not_swallowed_whole() {
    let faculty = faculty_with_window(4_096);
    let huge: String = (0..4_000)
        .map(|i| {
            format!(
                "line {i} of a very large tool result
"
            )
        })
        .collect();
    // (required, demoted) = (the result's ENDING, its earlier part). The ending
    // is what stays required so the wire reads in the original order — see
    // `split_action_result` for why history's position decides this.
    let (required, demoted) = faculty.split_action_result(huge.clone());
    let demoted = demoted.expect("an oversized result must yield a demoted part");

    let cap = 4_096 / LlmDeliberationFaculty::RESULT_SHARE_DENOM as usize;
    let required_cost =
        LlmDeliberationFaculty::messages_cost(std::slice::from_ref(&required));
    assert!(
        required_cost <= cap,
        "the required part must fit its share: {required_cost} > {cap}"
    );

    // NOTHING IS DISCARDED. Head + tail reconstitute the original exactly —
    // the property that separates demotion from truncation, and the one Joel
    // asked for by name after the first cut of #3947 dropped instead of demoted.
    let rebuilt = format!("{}{}", demoted.content_text(), required.content_text());
    assert_eq!(
        rebuilt, huge,
        "demoted + required must reconstitute the result byte-for-byte"
    );
}

// what this catches: THE WIRING, which the three unit tests above cannot see.
// They call `split_action_result` directly, so they stay green even when the
// production build site stops calling it — verified by mutation, and that is
// exactly the blind source-assertion mistake made earlier the same night. This
// one goes through `prompt_view`, the real path, and fails the moment the
// result is required whole again.
#[tokio::test]
async fn a_huge_result_does_not_fault_the_turn_on_the_real_path() {
    use crate::cognition::working_memory::WorkingMemory;

    let window = 8_192u32;
    let wm = Arc::new(WorkingMemory::new(8));
    wm.set_served_window(window);
    // A tool result several times the whole window — an ordinary `code/read`
    // of a large file.
    let huge: String = (0..6_000)
        .map(|i| {
            format!(
                "line {i} of a very large tool result
"
            )
        })
        .collect();
    wm.record_receipt(&huge);

    let faculty = LlmDeliberationFaculty::new(
        Uuid::new_v4(),
        "Sahar",
        "You are Sahar.",
        Arc::new(HeuristicInferenceAdapter::new()),
    )
    .with_context_window(window)
    .with_working_memory(Arc::clone(&wm));

    let ws = Workspace::new("what did that produce?");
    let view = faculty.prompt_view(&ws);
    assert!(
        view.capacity_error.is_none(),
        "one big tool result must not refuse the turn — she holds her card                      while refusing, so this strands a benchmark round: {:?}",
        view.capacity_error
    );
    assert!(
        !view.messages.is_empty(),
        "and she must still be given her result to reason about"
    );
}

// what this catches: a result that FITS must be left completely alone. A split
// that fires unconditionally would push every ordinary tool result into history
// and cost the citizen the outcome of her own act on the very next turn.
#[test]
fn a_result_that_fits_is_untouched() {
    let faculty = faculty_with_window(32_768);
    let small = "ok: wrote 3 lines
"
    .to_string();
    let (required, demoted) = faculty.split_action_result(small.clone());
    assert!(demoted.is_none(), "a small result has nothing to demote");
    assert_eq!(required.content_text(), small);
}

// what this catches: the head must never be empty. If the FIRST line alone
// exceeds the share, it is still required — a citizen handed a demotion notice
// and none of her own result cannot reason about what her hands just did, and
// the honest failure for a genuinely impossible payload is the existing
// capacity fault, not a silently empty head.
#[test]
fn the_first_line_is_required_even_when_it_alone_exceeds_the_share() {
    let faculty = faculty_with_window(1_024);
    let one_giant_line = format!(
        "{}
",
        "x".repeat(20_000)
    );
    let (required, demoted) = faculty.split_action_result(one_giant_line.clone());
    assert!(
        !required.content_text().is_empty(),
        "the required part is never empty — she must see what her act produced"
    );
    let rebuilt = format!(
        "{}{}",
        demoted
            .as_ref()
            .map(|d| d.content_text())
            .unwrap_or_default(),
        required.content_text()
    );
    assert_eq!(rebuilt, one_giant_line, "still nothing discarded");
}

// what this catches: UTF-8 safety. Splitting on a byte or char budget can cut a
// multi-byte sequence in half; line granularity cannot. A panic here would take
// down the cognition cycle on any result containing non-ASCII — which is most
// of them, since the results carry the citizens' own prose.
#[test]
fn splitting_never_cuts_a_multibyte_sequence() {
    let faculty = faculty_with_window(2_048);
    let multibyte: String = (0..2_000)
        .map(|i| {
            format!(
                "строка {i} — ✅ 日本語テキスト
"
            )
        })
        .collect();
    let (required, demoted) = faculty.split_action_result(multibyte.clone());
    let rebuilt = format!(
        "{}{}",
        demoted
            .as_ref()
            .map(|d| d.content_text())
            .unwrap_or_default(),
        required.content_text()
    );
    assert_eq!(rebuilt, multibyte, "multibyte content survives the split");
}

/// Her NEWEST result over her window (prompt/newest_result.rs): Aris on the M5, 2026-10-26.
mod the_newest_result_over_her_window {
    use super::*;
    use crate::ai::types::{ContentPart, MessageContent, ResultCustody};
    use crate::cognition::act_observe::{ActStatus, Observation, ToolOutput, ToolVerb};
    use crate::cognition::working_memory::WorkingMemory;

    /// Her slot on the M5 (2026-10-26).
    const ARIS_WINDOW: u32 = 187_136;
    const HANDLE: &str = "aris-evidence";

    /// Her measured density: her refused requests measured 192k-234k tokens, nearly all of it this
    /// result (340,832 chars, estimated 113,611 at chars/3). The lowest refusal, the most
    /// conservative reading of her density (the incident's own numbers).
    fn aris_calibration() -> PromptCalibration {
        PromptCalibration::observed(113_611, 192_000, None).expect("test: positive observation") // JUSTIFIED: test fixture
    }

    /// A `work/submission` answer the shape of hers: 578 staged rows of UUIDs, pretty JSON.
    fn evidence_answer(rows: usize) -> String {
        let row = |i: usize| {
            json!({
                "revision_id": Uuid::new_v4(), "claim_id": Uuid::new_v4(), "owner": Uuid::new_v4(),
                "staged_at_ms": 1_793_000_000_000u64 + i as u64, "generation_count": 1,
                "generation_request_ids": [Uuid::new_v4().to_string()],
                "matches_submission_claim": i % 289 != 7, "predates_submission": true,
            })
        };
        let rows: Vec<_> = (0..rows).map(row).collect();
        let patch = "+ a line of the submitted patch, as long as an ordinary diff line\n".repeat(1_500);
        serde_json::to_string_pretty(&json!({ "base_sha": "7fd84d40", "staged_evidence": rows, "patch": patch }))
            .expect("test: encode") // JUSTIFIED: test fixture
    }

    /// Her act and its saved result, recorded as the act seam does, and the faculty that renders it.
    fn faculty_after(content: &str) -> LlmDeliberationFaculty {
        let wm = Arc::new(WorkingMemory::new(8));
        wm.set_served_window(ARIS_WINDOW);
        let room = Uuid::new_v4();
        let call = ToolCall { id: "call-aris".into(), name: "work/submission".into(), input: json!({ "include_staged_evidence": true }) };
        wm.record_native_action_statement("Reading the staged evidence for my submission.", room, std::slice::from_ref(&call));
        let observation = Observation {
            call,
            output: ToolOutput {
                image: None,
                additional_images: None,
                result: crate::ai::types::ToolResult { tool_use_id: "call-aris".into(), content: content.into(), is_error: None, custody: Default::default() },
                verb: ToolVerb::classify("work/submission"),
                paths: Vec::new(),
                verdict: Default::default(),
            },
            status: ActStatus::Executed,
        };
        let rendered = observation.render_recency("", &wm.budget());
        wm.record_receipt_typed(&[observation], &rendered, Some(room), ResultCustody::Whole { handle: HANDLE.into() });
        LlmDeliberationFaculty::new(Uuid::new_v4(), "Aris", "You are Aris.", Arc::new(HeuristicInferenceAdapter::new()))
            .with_context_window(ARIS_WINDOW)
            .with_working_memory(wm)
    }

    /// Every tool result's content the view sends, with the ids it answers.
    fn tool_results(view: &DeliberationPromptView) -> Vec<(String, String)> {
        view.messages
            .iter()
            .filter_map(|m| match &m.content {
                MessageContent::Parts(parts) => Some(parts),
                MessageContent::Text(_) => None,
            })
            .flatten()
            .filter_map(|part| match part {
                ContentPart::ToolResult { tool_use_id, content, .. } => Some((tool_use_id.clone(), content.clone())),
                _ => None,
            })
            .collect()
    }

    // what this catches: Aris's stall. A newest result larger than her window was required whole,
    // so every request overflowed and she never acted again. It must render under her window at
    // its summary level, with the handle that reopens it and a line saying why, never dropped.
    #[test]
    fn aris_340k_result_renders_under_her_window_with_its_handle() {
        let whole = evidence_answer(578);
        assert!(whole.chars().count() >= 340_000, "the fixture is her shape: {}", whole.chars().count());
        let faculty = faculty_after(&whole);
        let ws = Workspace::new("what does the evidence say?");
        let view = faculty.prompt_view_with_visual_feedback(&ws, ARIS_WINDOW, aris_calibration(), None);
        assert!(view.capacity_error.is_none() && view.budget_fault.is_none(), "{:?} {:?}", view.capacity_error, view.budget_fault);
        let sent = aris_calibration().charge(view.estimated_prompt_tokens) + view.completion_reserve as usize;
        assert!(sent <= ARIS_WINDOW as usize, "{sent} tokens charged against her {ARIS_WINDOW} window");
        let results = tool_results(&view);
        assert_eq!(results.len(), 1, "the native call/result pair is intact");
        let (id, shown) = &results[0];
        assert_eq!(id, "call-aris");
        assert!(shown.contains("too large to show whole") && shown.contains(&format!("tool/output {HANDLE}")), "{}", &shown[..shown.len().min(600)]);
        assert!(shown.contains("lines between"), "it is her result's summary level, not only a pointer");
        assert!(shown.chars().count() * 4 <= whole.chars().count(), "a smaller level: {} of {}", shown.chars().count(), whole.chars().count());
    }

    // what this catches: the collapse firing on a result that fits, which would cost her the
    // outcome of her own act on the very next turn.
    #[test]
    fn a_newest_result_that_fits_is_untouched() {
        let small = evidence_answer(3);
        let faculty = faculty_after(&small);
        let ws = Workspace::new("what does the evidence say?");
        let view = faculty.prompt_view_with_visual_feedback(&ws, ARIS_WINDOW, aris_calibration(), None);
        assert!(view.capacity_error.is_none());
        assert_eq!(tool_results(&view), vec![("call-aris".to_owned(), small)]);
    }
}
