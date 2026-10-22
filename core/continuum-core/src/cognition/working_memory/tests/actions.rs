#![cfg(test)]
use super::*;

// what this catches: THE starvation fix — a large tool result comes back to the mind
// in FULL (so it can count/read/scan it), while the rolling trail keeps only the head
// (byte-stable proprioception). Older acts drop to head-only; a fresh act's full result
// replaces the prior. This is why a persona reading a 130-line file can now answer about
// the whole file instead of looping on the doc-comment head.
#[tokio::test]
async fn latest_action_returns_full_result_trail_keeps_head() {
    let wm = Arc::new(WorkingMemory::new(8));
    // Truncation bounds are fractions of a LIVE window now; an unknown-window memory
    // is unbounded by contract, so a clipping test must declare the window it measures.
    wm.set_served_window(16_384);
    let big = format!("line0\n{}", "x".repeat(5_000)); // > WM_ACTION_HEAD_CHARS
    wm.record_receipt(&big);

    // The full result is available whole.
    let (seq, full) = wm.last_action_full().expect("latest act kept");
    assert_eq!(seq, 1);
    assert_eq!(
        full, big,
        "the mind gets the WHOLE result, not a truncated stub"
    );

    // The rolling trail carries only the head (KV-stable proprioception).
    let trail = wm.recent();
    assert_eq!(trail.len(), 1);
    assert!(trail[0].starts_with("[action #1]"));
    assert!(
        trail[0].chars().count() < big.chars().count(),
        "trail entry is head-truncated, not the whole result"
    );

    // The full result is PINNED by the message builder (#392) as its own durable
    // trailing block — it is no longer part of the evictable faculty bid.
    let pinned = wm
        .pinned_active_result_block()
        .expect("the whole result is pinned for the mind");
    assert!(
        pinned.contains("Full result of your most recent action (#1; room unrecorded; operation unrecorded):"),
        "the whole result reaches the mind"
    );
    assert!(
        pinned.contains(&"x".repeat(5_000)),
        "and it's the FULL body"
    );

    // A second act replaces the full slot; the first survives only as a trail head.
    wm.record_receipt("small follow-up");
    let (seq2, _) = wm.last_action_full().unwrap();
    assert_eq!(seq2, 2, "latest-full tracks the most recent act");
}

// what this catches: a consumer re-cut (the #392 pinned-block clip until 2026-10-12). The
// producer fits a result (`spill::LivePin::whole_or_handle`: whole, or its handle, or whole with
// a failed save said first); working memory shows exactly that, never a head of it,
// even when it is over the window's share.
#[tokio::test]
async fn the_pinned_result_is_what_the_producer_handed_over_never_re_cut() {
    let wm = Arc::new(WorkingMemory::new(8));
    wm.set_served_window(16_384);
    let over_share = "z".repeat(wm.budget().latest_action_chars() + 4_000);
    wm.record_receipt(&over_share);
    let pinned = wm
        .pinned_active_result_block()
        .expect("the result surfaces");
    assert!(pinned.ends_with(&over_share), "whole, as handed over");
}

// what this catches: #139/#165 — the FULL action result rides in the prompt only while
// the mind is inside the act→observe loop (active). Once the persona SETTLES (speaks),
// the raw result has done its "what next" job and drops out of the prompt so a 2k-token
// code/tree dump stops re-prefilling on every subsequent conversational turn. The trail
// HEAD survives (proprioception), and a NEW act re-activates the full channel.
#[tokio::test]
async fn full_result_drops_out_after_the_persona_settles_but_a_new_act_reactivates() {
    let wm = Arc::new(WorkingMemory::new(8));
    // Truncation bounds are fractions of a LIVE window now; an unknown-window memory
    // is unbounded by contract, so a clipping test must declare the window it measures.
    wm.set_served_window(16_384);
    let dump = format!("code/tree result\n{}", "n".repeat(3_000)); // > WM_ACTION_HEAD_CHARS
    wm.record_receipt(&dump);

    // The proprioceptive trail head lives in the faculty contribution; the FULL result
    // is the PINNED block the message builder appends (#392). This test spans both.
    let faculty_trail = |wm: Arc<WorkingMemory>| async move {
        WorkingMemoryFaculty::new(wm)
            .contribute(&Workspace::new("tick"))
            .await
            .expect("bids")
            .content
            .clone()
    };

    // Active (acted, not yet spoken): the full result is present in the pinned block.
    assert!(
        wm.pinned_active_result_block()
            .expect("active result is pinned")
            .contains("Full result of your most recent action"),
        "while active, the mind sees the whole result it just fetched"
    );

    // The persona SETTLES → the full result drops out; the trail head remains.
    wm.record_settlement("the workspace has a wordstats crate", None);
    assert!(
        wm.active_action_full().is_none(),
        "a settled result is no longer active"
    );
    assert!(
        wm.pinned_active_result_block().is_none(),
        "after settling, the 2k-token dump stops riding along the prompt (#139/#165)"
    );
    assert!(
        faculty_trail(wm.clone()).await.contains("[action #1]"),
        "but the proprioceptive trail head survives — the mind still knows it acted"
    );

    // A NEW act re-activates the full channel (higher seq beats the settled boundary).
    let dump2 = format!("code/read result\n{}", "m".repeat(3_000));
    wm.record_receipt(&dump2);
    assert!(
        wm.active_action_full().map(|(s, _)| s) == Some(2),
        "the fresh act is active again"
    );
    assert!(
        wm.pinned_active_result_block()
            .expect("the fresh act's result is pinned")
            .contains("Full result of your most recent action (#2; room unrecorded; operation unrecorded):"),
        "the new act's result is surfaced whole for its own what-next decision"
    );
}

// what this catches: the ASYNC feedback channel — a dispatched (background) command
// streams status back by handle CONTINUOUSLY (running → done), updating in place; two
// concurrent handles don't clobber; the faculty surfaces both so the mind sees what it
// sent away. This is what lets a persona fire a sentinel/compile/debugger and fold the
// result in when it lands, without ever blocking.
#[tokio::test]
async fn dispatched_commands_stream_back_by_handle() {
    let wm = Arc::new(WorkingMemory::new(8));
    let compile = Uuid::from_u128(1);
    let sentinel = Uuid::from_u128(2);

    // Two sentinels in flight, streaming continuously.
    wm.record_dispatch_event(
        compile,
        "compile core",
        "building…",
        DispatchStatus::Running,
    );
    wm.record_dispatch_event(
        sentinel,
        "research task",
        "searching…",
        DispatchStatus::Running,
    );
    // A progress update on the compile updates IN PLACE (still one handle).
    wm.record_dispatch_event(compile, "compile core", "linking…", DispatchStatus::Running);
    let snap = wm.dispatched_snapshot();
    assert_eq!(snap.len(), 2, "two distinct handles, no clobber");
    let c = snap.iter().find(|(h, ..)| *h == compile).unwrap();
    assert_eq!(c.2, "linking…", "progress updated in place");
    assert_eq!(c.3, DispatchStatus::Running);

    // The compile finishes — terminal Done with its result.
    wm.record_dispatch_event(
        compile,
        "compile core",
        "0 errors, 0 warnings",
        DispatchStatus::Done,
    );
    let snap = wm.dispatched_snapshot();
    let c = snap.iter().find(|(h, ..)| *h == compile).unwrap();
    assert_eq!(c.3, DispatchStatus::Done);
    assert_eq!(c.2, "0 errors, 0 warnings");

    // The faculty surfaces both dispatched handles to the mind.
    let rendered = WorkingMemoryFaculty::new(wm.clone())
        .contribute(&Workspace::new("a fresh burst"))
        .await
        .expect("bids when dispatched work exists")
        .content
        .clone();
    assert!(
        rendered.contains("Commands you dispatched"),
        "the mind sees its sentinels"
    );
    assert!(
        rendered.contains("compile core [done]: 0 errors"),
        "finished result shown"
    );
    assert!(
        rendered.contains("research task [running]"),
        "in-flight shown"
    );
}

// what this catches: record/recent round-trips, blank reasoning is ignored, and
// the buffer ROLLS at capacity (oldest dropped) — it's a scratchpad, not a log.
#[test]
fn records_recent_and_rolls_at_capacity() {
    let wm = WorkingMemory::new(2);
    assert!(wm.is_empty());
    wm.record("first thought");
    wm.record("   "); // blank → ignored
    wm.record("second thought");
    assert_eq!(wm.recent(), vec!["first thought", "second thought"]);
    // third pushes out the oldest (capacity 2).
    wm.record("third thought");
    assert_eq!(wm.recent(), vec!["second thought", "third thought"]);
}

// what this catches: THE loop-break property. A repeated identical action must
// produce DISTINCT working-memory entries (monotonic #n stamp) so the perception
// window the faculty bids changes across a repeat — otherwise greedy decode
// re-emits the identical Act forever (the deterministic acting loop that the
// engram dedup + suppressed-thinking left invisible). Regression for
// [[act-results-need-a-recency-channel-not-semantic-recall]].
#[test]
fn repeated_action_yields_distinct_stamped_entries() {
    let wm = WorkingMemory::new(3);
    wm.record_receipt("I ran code/search(pattern=foo) -> 0 matches");
    wm.record_receipt("I ran code/search(pattern=foo) -> 0 matches"); // identical act
    let recent = wm.recent();
    assert_eq!(recent.len(), 2);
    assert_ne!(
        recent[0], recent[1],
        "identical repeated act must read DISTINCT in working memory"
    );
    assert!(recent[0].starts_with("[action #1] "));
    assert!(recent[1].starts_with("[action #2] "));
    // blank action ignored (no fabricated proprioception).
    wm.record_receipt("   ");
    assert_eq!(wm.recent().len(), 2);
}

// what this catches: record_settlement lays down a WM_SETTLEMENT_PREFIX boundary
// in the rolling buffer (with the answer head as honest "I already answered X"
// proprioception), and a blank answer still marks the boundary. This is the marker
// the repeat-perception guard scopes to, separating a spin (identical call before
// any answer) from legitimate re-use of the same tool after answering.
#[test]
fn record_settlement_marks_a_boundary_in_the_buffer() {
    let room = Uuid::new_v4();
    let other = Uuid::new_v4();
    let scoped = WorkingMemory::new(8);
    scoped.set_scope(Some("root-a".into()));
    scoped.retain_truncated_emission(room, 3587);
    scoped.retain_truncated_emission(room, 4000);
    scoped.retain_truncated_emission(room, 3000);
    assert_eq!(scoped.pending_emission_peak(room), 4000);
    assert_eq!(scoped.pending_emission_peak(other), 0);
    scoped.record_settlement("another activity answered", Some(other));
    assert_eq!(scoped.pending_emission_peak(room), 4000);
    scoped.set_scope(Some("root-b".into()));
    assert_eq!(scoped.pending_emission_peak(room), 0);
    scoped.record_settlement("another workspace answered", Some(room));
    scoped.set_scope(Some("root-a".into()));
    assert_eq!(scoped.pending_emission_peak(room), 4000);
    let encoded = serde_json::to_vec(&scoped.snapshot()).unwrap();
    let restored = WorkingMemory::new(8);
    restored.restore(serde_json::from_slice(&encoded).unwrap());
    restored.set_scope(Some("root-a".into()));
    assert_eq!(restored.pending_emission_peak(room), 4000, "restart retains the same concern evidence");
    restored.record_settlement("authored answer", Some(room));
    assert_eq!(restored.pending_emission_peak(room), 0, "existing settlement releases, not proof of task completion");
    scoped.clear();
    assert_eq!(scoped.pending_emission_peak(room), 0);
    // Abandoned concerns share the existing WM capacity, not an immortal map.
    let bounded = WorkingMemory::new(2);
    for index in 0..12 {
        bounded.set_scope(Some(format!("abandoned-{index}")));
        bounded.retain_truncated_emission(room, 3587);
        assert!(bounded.snapshot().pending_emissions.len() <= 2);
    }
    bounded.set_scope(Some("abandoned-0".into()));
    assert_eq!(bounded.pending_emission_peak(room), 0, "discarded concern cannot resurrect its old peak");
    bounded.set_scope(Some("abandoned-11".into()));
    for _ in 0..12 { bounded.record("intermediate read observation"); }
    assert_eq!(bounded.pending_emission_peak(room), 3587, "reads do not discharge active truncated demand");
    let smaller = WorkingMemory::new(1);
    smaller.restore(bounded.snapshot());
    smaller.set_scope(Some("abandoned-11".into()));
    assert_eq!(smaller.pending_emission_peak(room), 3587);
    assert_eq!(smaller.snapshot().pending_emissions.len(), 1);
    smaller.set_scope(Some("abandoned-10".into()));
    assert_eq!(smaller.pending_emission_peak(room), 0, "restore honors current WM capacity");
    let wm = WorkingMemory::new(4);
    wm.record_receipt("I ran commands/list({}) -> 100 commands");
    wm.record_settlement("there are 100 commands", None);
    let recent = wm.recent();
    assert_eq!(recent.len(), 2);
    assert!(
        recent[1].starts_with(WM_SETTLEMENT_PREFIX),
        "the settlement is marked with the shared prefix so the guard can find it"
    );
    assert!(
        recent[1].contains("there are 100 commands"),
        "carries the answer head as proprioception"
    );
    // A blank answer still marks the boundary (bare prefix, no fabricated text).
    wm.record_settlement("   ", None);
    let recent = wm.recent();
    assert_eq!(recent.last().unwrap(), WM_SETTLEMENT_PREFIX);
}

// what this catches: clear() empties the volatile scratch (disjoint-concern
// boundary, e.g. between eval tasks) but does NOT reset the monotonic action
// stamp — so a post-clear act can never alias a pre-clear stamp.
#[test]
fn clear_empties_buffer_but_keeps_stamps_monotonic() {
    let wm = WorkingMemory::new(3);
    wm.record_receipt("ran A");
    wm.record_receipt("ran B");
    assert_eq!(wm.recent().len(), 2);
    wm.clear();
    assert!(wm.is_empty(), "clear drops the disjoint concern's traces");
    wm.record_receipt("ran C");
    let recent = wm.recent();
    assert_eq!(recent.len(), 1);
    assert!(
        recent[0].starts_with("[action #3] "),
        "stamp stays monotonic across clear (no alias with pre-clear #1/#2), got {:?}",
        recent[0]
    );
}

// what this catches: capacity is floored at 1 — a 0 never wedges the buffer into
// dropping everything.
#[test]
fn capacity_floored_at_one() {
    let wm = WorkingMemory::new(0);
    wm.record("a");
    wm.record("b");
    assert_eq!(wm.recent(), vec!["b"], "floor-1 buffer keeps the latest");
}
