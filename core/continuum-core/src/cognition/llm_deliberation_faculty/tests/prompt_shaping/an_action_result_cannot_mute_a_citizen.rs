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
