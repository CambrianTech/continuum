#![cfg(test)]
use super::*;

// what this catches (Step 3, run-18057-f1): a receipt recorded via
// `record_receipt_typed` carries the TYPED acts so `active_act()` reads the tool
// result by FIELD (call.id-correlated to result.tool_use_id) instead of re-parsing
// the `[action #n]` prose — AND the rendered `text` stays byte-identical to the
// legacy `record_receipt` path (the #205 KV-stability invariant). `recent_acts()`
// exposes every act in the window for the typed predicates (Step 4).
#[test]
fn typed_receipt_threads_the_act_by_field_and_keeps_text_byte_stable() {
    use crate::ai::types::{ToolCall, ToolResult};
    use crate::cognition::act_observe::{ActStatus, Observation, ToolOutput, ToolVerb};

    let obs = Observation {
        call: ToolCall {
            id: "call-42".into(),
            name: "code/search".into(),
            input: serde_json::json!({ "query": "needle" }),
        },
        output: ToolOutput {
            image: None,
            additional_images: None,
            result: ToolResult {
                tool_use_id: "call-42".into(),
                content: "match at foo.rs:42".into(),
                is_error: None,
                custody: crate::ai::types::ResultCustody::Unsaved,
            },
            verb: ToolVerb::Search,
            paths: Vec::new(),
            verdict: Default::default(),
        },
        status: ActStatus::Executed,
    };
    let rendered = "code/search({\"query\":\"needle\"})\nResult:\nmatch at foo.rs:42\n\n";

    let typed = WorkingMemory::new(8);
    typed.record_receipt_typed(std::slice::from_ref(&obs), rendered, None, crate::ai::types::ResultCustody::Unsaved);
    let legacy = WorkingMemory::new(8);
    legacy.record_receipt(rendered);
    assert_eq!(
        typed.recent(),
        ["[action #1] code/search(query=needle)"],
        "the trail line is the act's call, stable however its result reads (#205)"
    );
    assert_eq!(typed.last_action_full().map(|(_, full)| full), Some(rendered.trim().to_owned()), "the result is whole in its record");

    let active = typed
        .active_act()
        .expect("the typed act threads through the receipt");
    assert_eq!(
        active.call.id, active.output.result.tool_use_id,
        "correlated by id, not positional index"
    );
    assert!(
        active.output.result.content.contains("match at foo.rs:42"),
        "the tool RESULT re-enters by the TYPED field, not a re-parsed [action #n] head"
    );
    assert_eq!(
        typed.recent_acts().len(),
        1,
        "the batch's act is in the window"
    );
    assert!(
        legacy.active_act().is_none(),
        "a legacy string receipt carries no typed act — the two channels are distinct"
    );

    // Native visual feedback must carry ALL calls of the same active batch,
    // never the last call alone or an earlier screenshot after settlement.
    let mut second = obs.clone();
    second.call.id = "call-43".into();
    second.output.result.tool_use_id = second.call.id.clone();
    let room = Uuid::new_v4();
    let original_calls = vec![obs.call.clone(), second.call.clone()];
    typed.record_native_action_statement("", room, &original_calls);
    assert!(typed.action_statement(room).is_none(), "tool-only turns do not inherit old public speech");
    typed.record_receipt_typed(&[obs.clone(), second], "two results", Some(room), crate::ai::types::ResultCustody::Unsaved);
    let (seq, _) = typed.active_action_full().unwrap();
    let batch = typed.active_action_observations(seq);
    assert_eq!(batch.len(), 2);
    assert_eq!(batch[0].call.id, "call-42");
    assert_eq!(batch[1].call.id, "call-43");
    assert_eq!(typed.active_action_native_calls(seq).len(), 2);
    let restored = WorkingMemory::new(8);
    restored.restore(serde_json::from_slice(&serde_json::to_vec(&typed.snapshot()).unwrap()).unwrap());
    assert_eq!(restored.active_action_native_calls(seq)[1].id, "call-43");
    // A receipt consumes its decision once, even if later calls reuse an ID.
    restored.record_receipt_typed(&[obs.clone()], "later unrecorded call", Some(room), crate::ai::types::ResultCustody::Unsaved);
    let next = restored.active_action_full().unwrap().0;
    assert!(restored.active_action_native_calls(next).is_empty());
    restored.record_native_action_statement("", room, &original_calls);
    restored.record_receipt_typed(&[obs.clone()], "other activity", Some(Uuid::new_v4()), crate::ai::types::ResultCustody::Unsaved);
    assert!(restored.active_action_native_calls(restored.active_action_full().unwrap().0).is_empty());
    restored.record_native_action_statement("", room, &original_calls);
    restored.record_native_action_statement("", room, &[]); // next decision uses a legacy envelope
    restored.record_receipt_typed(&[obs.clone()], "legacy after failed native action", Some(room), crate::ai::types::ResultCustody::Unsaved);
    assert!(restored.active_action_native_calls(restored.active_action_full().unwrap().0).is_empty());
    restored.record_native_action_statement("", room, &original_calls);
    restored.set_scope(Some("another-root".into()));
    restored.record_receipt_typed(&[obs.clone()], "other root", Some(room), crate::ai::types::ResultCustody::Unsaved);
    assert!(restored.active_action_native_calls(restored.active_action_full().unwrap().0).is_empty());
    assert!(typed.active_action_observations(seq - 1).is_empty());
    typed.set_scope(Some("another-workspace".into()));
    assert!(typed.active_action_observations(seq).is_empty());
    assert!(typed.active_action_native_calls(seq).is_empty());
    typed.set_scope(None);
    typed.record_settlement("observed", None);
    assert!(typed.active_action_observations(seq).is_empty());
    typed.record_receipt_typed(&[obs], "new result", None, crate::ai::types::ResultCustody::Unsaved);
    assert!(typed.active_action_observations(seq).is_empty());
    let (new_seq, _) = typed.active_action_full().unwrap();
    assert_eq!(typed.active_action_observations(new_seq).len(), 1);
    typed.clear();
    assert!(typed.active_action_observations(new_seq).is_empty());
}

// e731: identical result text from different activities must retain its
// actual room and every operation in its batch, including after restart.
#[test]
fn result_provenance_survives_cross_activity_render_and_checkpoint() {
    use crate::ai::types::{ToolCall, ToolResult};
    use crate::cognition::act_observe::{ActStatus, Observation, ToolOutput, ToolVerb};

    let observation = |id: &str, name: &str, handle: Option<&str>| Observation {
        call: ToolCall {
            id: id.into(),
            name: name.into(),
            input: serde_json::json!({}),
        },
        output: ToolOutput {
            image: None,
            additional_images: None,
            result: ToolResult {
                tool_use_id: id.into(),
                content: "same result".into(),
                is_error: None,
                custody: handle.map_or(crate::ai::types::ResultCustody::Unsaved, |h| {
                    crate::ai::types::ResultCustody::Whole { handle: h.to_owned() }
                }),
            },
            verb: ToolVerb::classify(name),
            paths: Vec::new(),
            verdict: Default::default(),
        },
        status: ActStatus::Executed,
    };
    let wm = WorkingMemory::new(8);
    let room_a = Uuid::new_v4();
    let room_b = Uuid::new_v4();
    wm.record_receipt_typed(
        &[
            observation("a1", "code/search", Some("search-output")),
            observation("a2", "code/read", None),
        ],
        "same result",
        Some(room_a),
        saved("search-output"),
    );
    wm.record_receipt_typed(
        &[observation("b1", "code/run", None)],
        "same result",
        Some(room_b),
        crate::ai::types::ResultCustody::Unsaved,
    );
    // #2 is the active result: it renders whole as the full-result message below, so
    // the ring shows only #1 rather than paying for #2 twice.
    let expected = vec![
        format!("[result #1; room {room_a}; operation code/search, code/read; output handle: tool/output search-output] same result"),
    ];
    let messages = wm.recent_results_messages();
    assert_eq!(
        messages.len(),
        2,
        "banner plus the one result that is not the active full result"
    );
    assert_eq!(&messages[1..], expected.as_slice());
    let (seq, full) = wm
        .active_action_full()
        .expect("test: latest receipt is active");
    assert_eq!(
        wm.full_result_message(seq, &full),
        format!("Full result of your most recent action (#2; room {room_b}; operation code/run):\nsame result")
    );

    let encoded = serde_json::to_value(wm.snapshot()).expect("test: snapshot serializes");
    let restored = WorkingMemory::new(8);
    restored.restore(serde_json::from_value(encoded).expect("test: recorded snapshot decodes"));
    assert_eq!(restored.recent_results_messages(), messages);
    assert_eq!(
        restored.full_result_message(seq, &full),
        wm.full_result_message(seq, &full)
    );

    // A shed collapses old results; its pointers must still tell
    // the mind which activity and operation the retained handle belongs to.
    for _ in 0..4 {
        record_saved(&restored, &"x".repeat(BODY));
    }
    restored.record_settlement("reported", None);
    shed(&restored, 0);
    assert!(restored.recent_results_messages().join("\n").contains(
        &format!("[result #1; room {room_a}; operation code/search, code/read; output handle: tool/output search-output — collapsed]")
    ));

    // Regression 735a3183: Kimi's 274-line read disappeared after a poll;
    // other reads survived only as their last ~36 lines. A retained handle
    // must replace the body as a unit, never decorate an arbitrary suffix.
    let read = (387..=660)
        .map(|line| format!("{line}: complete source line with enough context to exceed the old per-entry share\n"))
        .collect::<String>();
    let memory = WorkingMemory::new(8);
    let mut read_observation = observation("read-274", "code/read", Some("read-274-output"));
    read_observation.output.result.content = read.clone();
    memory.record_receipt_typed(&[read_observation], &read, Some(room_a), saved("read-274-output"));
    memory.record_receipt_typed(
        &[observation("poll-next", "shell/poll", None)],
        "process still running",
        Some(room_a),
        saved("poll-next-output"),
    );
    let assert_whole_or_handle = |memory: &WorkingMemory| {
        let view = memory.recent_results_messages().join("\n");
        assert!(view.contains("read-274-output"), "the read remains addressable");
        assert!(view.contains(&room_a.to_string()), "the view retains its room");
        assert!(
            // The ring stores the receipt trimmed of surrounding whitespace, which is not
            // a cut: every line of the read must be there.
            !view.contains("660: complete source line") || view.contains(read.trim()),
            "a suffix of a source read must never masquerade as the retained result"
        );
    };
    assert_whole_or_handle(&memory);
    memory.record_settlement("work remains on this card", None);
    assert_whole_or_handle(&memory);
    let restarted = WorkingMemory::new(8);
    restarted.restore(memory.snapshot());
    assert_whole_or_handle(&restarted);
}

// what this catches (Step 3 back-compat): an OLD volatile.json — written before
// the WmEntry.acts field existed — restores WITHOUT panic, the `acts` defaulting
// to empty via `#[serde(default)]` (the same contract as saved_at_ms/build_sha).
// A grid-sync peer or a pre-deploy snapshot must never wedge the mind on restore.
#[test]
// what this catches: the #414 deprivation regressing — her acts must survive the shared ring's
// churn, every one of them kept in her record (no character share cuts the oldest: what of
// their bodies stays in RAM is the one memory owner's to shed), and the record must round-trip
// the volatile snapshot. The measured failure: a 2,863-act citizen whose window showed ONE act
// read her own starved history as "I have nothing to contribute".
fn her_acts_survive_ring_churn_and_roundtrip_the_snapshot() {
    let wm = WorkingMemory::new(2);
    let lines = |wm: &WorkingMemory| -> Vec<String> { wm.actions.lock().records().filter_map(|record| record.head.clone()).collect() };
    wm.record_receipt("I ran code/shell(ls) Result: ok");
    for i in 0..10 {
        wm.record_fact(&format!("chatty fact {i}"));
    }
    assert!(!wm.has_receipt(), "precondition: the ring entry must age out under churn");
    assert_eq!(lines(&wm).len(), 1, "her act survives the churn");
    assert!(lines(&wm)[0].contains("code/shell(ls)"));
    for i in 0..2000 {
        wm.record_receipt(&format!("I ran code/read(f{i}) Result: ok"));
    }
    let kept = lines(&wm);
    assert_eq!(kept.len(), 2001, "every act is kept, oldest first");
    assert!(kept.last().is_some_and(|line| line.contains("f1999")), "{:?}", kept.last());
    let fresh = WorkingMemory::new(2);
    fresh.restore(wm.snapshot());
    assert_eq!(lines(&fresh), kept);
}

#[test]
fn an_old_snapshot_without_acts_restores_without_panic() {
    // A hand-authored legacy snapshot: entries have NO `acts` key at all.
    let legacy_json = r#"{
        "entries": [
            { "kind": "Thought", "text": "thinking about the tokenizer" },
            { "kind": { "Receipt": { "n": 1 } }, "text": "[action #1] code/list → ok" }
        ],
        "last_action": [1, "code/list → ok"],
        "action_fps": ["code/list|{}"],
        "next_action_seq": 2
    }"#;
    let snap: VolatileSnapshot =
        serde_json::from_str(legacy_json).expect("legacy snapshot deserializes");
    let fresh = WorkingMemory::new(8);
    fresh.restore(snap);
    // Both legacy entries survive the round-trip (asserted by content, not a raw
    // count — restore ALSO appends a `[resumed]` wake-orientation fact by design,
    // #147/#165: a restore IS an interruption, so she wakes oriented, never blank).
    let texts = fresh.recent();
    assert!(
        texts
            .iter()
            .any(|t| t.contains("thinking about the tokenizer")),
        "the legacy thought restored"
    );
    assert!(
        texts.iter().any(|t| t.contains("[action #1] code/list")),
        "the legacy receipt restored"
    );
    assert!(
        fresh.pinned_facts().iter().any(|t| t.starts_with("[resumed]")),
        "restore pins the wake-orientation notice"
    );
    assert!(
        fresh.recent_acts().is_empty(),
        "a legacy receipt has no typed acts — defaulted empty, never a panic"
    );
    assert!(
        fresh.has_receipt(),
        "the receipt kind still survives the trip"
    );
}

#[test]
fn note_action_fingerprint_counts_identical_repeats() {
    let wm = WorkingMemory::new(8);
    assert_eq!(
        wm.note_action_fingerprint("code/search|{\"pattern\":\"x\"}"),
        1
    );
    assert_eq!(
        wm.note_action_fingerprint("code/search|{\"pattern\":\"x\"}"),
        2
    );
    assert_eq!(
        wm.note_action_fingerprint("code/search|{\"pattern\":\"x\"}"),
        3
    );
    // a DIFFERENT call is its own first occurrence, not a repeat of the above
    assert_eq!(wm.note_action_fingerprint("code/read|{\"file\":\"a\"}"), 1);
    // back to the original — still counted across the window
    assert_eq!(
        wm.note_action_fingerprint("code/search|{\"pattern\":\"x\"}"),
        4
    );
}

// what this catches: the loop-awareness COUNT must survive a tiny recency window
// (regression for the 2026-07-14 Atlas x38 pinned-at-3 spiral). The windowed
// `action_fps` alone caps the count at the window size the instant other acts
// interleave — a deepening loop then reads "3 times" forever and conveys none of the
// spiral. The DURABLE per-session count must keep climbing to 38.
//
// Pins its OWN small capacity on purpose: the invariant is "durable count beats the
// window", so the window must be small REGARDLESS of what the live budget derives.
// It used to borrow the live default, which coupled a durability test to a capacity
// number that has since changed.
#[test]
fn action_fingerprint_count_escalates_past_the_tiny_recency_window() {
    // Literal, not a named const: the hardcoded-bound guard rightly polices NAMED
    // size constants in cognition (they become de-facto bounds reused elsewhere).
    // This 3 is test DATA — the deliberately tiny window the invariant is about.
    let wm = WorkingMemory::new(3);
    let tree = "code/tree|{\"max_depth\":1}";
    let mut last = 0;
    for _ in 0..38 {
        last = wm.note_action_fingerprint(tree);
        // three OTHER distinct calls per cycle — more than capacity(3), so a
        // window-only count would evict `tree` before it recurs and never exceed 1.
        wm.note_action_fingerprint("code/read|{\"f\":\"a\"}");
        wm.note_action_fingerprint("code/list|{\"p\":\".\"}");
        wm.note_action_fingerprint("code/search|{\"q\":\"x\"}");
    }
    assert_eq!(
        last, 38,
        "durable count must reflect all 38 identical issues, not cap at the window size"
    );
    // The recent-window tally (action_verb_tally) stays CORRECTLY windowed — it's
    // the recent-shape channel, not the durable-repeat channel.
    assert!(
        wm.action_verb_tally().len() <= 3,
        "verb tally stays bounded to the recency window"
    );
}
