//! MIND-CONTEXT invariant 4: an unchanged observation arrives as a repeat only while the whole it
//! repeats is in front of her.
use super::*;
use crate::cause::{Act, ContentHash, Fingerprint, ToolEffect};

/// Her acts as one chain, each caused by the one before; each act's content is its raw
/// output's hash, never its labelled rendering (two equal outputs compare equal).
struct Acts(Vec<(&'static str, &'static str, ToolEffect, String)>);

impl Acts {
    fn push(&mut self, verb: &'static str, args: &'static str, effect: ToolEffect, output: &str) {
        self.0.push((verb, args, effect, output.to_owned()));
    }
}

impl Source<String> for Acts {
    fn name(&self) -> &str {
        "acts"
    }
    fn priority(&self) -> u8 {
        5
    }
    fn items(&self) -> Vec<Item<String>> {
        let id = |seq: usize| ElementId::of(&[b"act", &seq.to_be_bytes()]);
        self.0
            .iter()
            .enumerate()
            .map(|(i, (verb, args, effect, output))| {
                let seq = i + 1;
                let cause = if seq == 1 { Cause::Root(crate::cause::Origin::ActivityStart) } else { Cause::Of(id(seq - 1)) };
                let act = Act { fingerprint: Fingerprint::of([(*verb, args.as_bytes())]), effect: *effect };
                Item::rendered(id(seq), cause, seq.to_string(), format!("[#{seq} {verb}] {output}"), Content::Hashed(ContentHash::of(output)))
                    .with_pointer(format!("[#{seq} {verb}: collapsed; reopen with tool/output act={seq}]"))
                    .with_act(act)
            })
            .collect()
    }
    fn repeat(&self, item: &Item<String>, ancestor: &Item<String>) -> Option<String> {
        Some(format!(
            "[#{} repeat of #{}, outcome unchanged; reopen with tool/output act={}]",
            item.key(),
            ancestor.key(),
            ancestor.key()
        ))
    }

    fn fold(&self, _folded: &[Item<String>]) -> Option<String> {
        None // a fixture that does not exercise folding
    }
}

/// A file she reads whole: far larger than its collapsed pointer, as a real read is.
fn body(name: &str) -> String {
    format!("// {name}\n") + &"fn parse(&mut self) -> Result<Tree, Error> { self.step()?; }\n".repeat(60)
}

fn floors(claims: &[Claim]) -> Vec<usize> {
    claims.iter().map(|claim| claim.floor).collect()
}

// what this catches: the M5 2026-10-30 read loop, the same unchanged file arriving whole
// again and again while the first copy is still in front of her: the second read arrives as
// a repeat naming #1, is counted, and never counts as reading a whole.
#[test]
fn an_unchanged_read_repeated_mid_epoch_arrives_as_a_repeat() {
    let mut acts = Acts(Vec::new());
    let (mut view, _) = View::start(&crate::cost::Chars, vec!["identity".into()], &[&acts], all_full).unwrap(); // unwrap: one grant per claim cannot mismatch
    acts.push("code/read", r#"{"path":"a.rs"}"#, ToolEffect::Observes, &body("a.rs"));
    assert_eq!(view.advance(&[&acts]).repeats, 0);
    acts.push("code/read", r#"{"path":"a.rs"}"#, ToolEffect::Observes, &body("a.rs"));
    let advance = view.advance(&[&acts]);
    assert_eq!(advance.repeats, 1);
    let text = rendered(&view);
    assert_eq!(text.matches(&body("a.rs")).count(), 1, "the whole arrives once");
    assert!(text.contains("[#2 repeat of #1, outcome unchanged; reopen with tool/output act=1]"), "{text}");
    assert!(view.renders("acts", "1"));
    assert!(!view.renders("acts", "2"), "a repeat is never a read of a whole");
    // The receipt says so too: #2 was shown as a repeat of #1, never at `Full` (a reader asking
    // "was this result shown whole?" matches `Full` only).
    let id = |seq: usize| ElementId::of(&[b"act", &seq.to_be_bytes()]);
    let levels: Vec<_> = view.receipt().items.iter().filter(|r| r.element.is_some()).map(|r| (r.element, r.level)).collect();
    assert_eq!(levels, [(Some(id(1)), Level::Full), (Some(id(2)), Level::Repeat { of: id(1) })], "{levels:?}");
}

// what this catches (Codex on #5652): a cause cycle among earlier acts (1 -> 2 -> 1, a malformed
// chain) looping the ancestor walk forever when a later act arrives under it. Each step must land
// strictly before the last; the walk runs on a thread so a regression fails instead of hanging.
#[test]
fn a_cause_cycle_beneath_an_arrival_ends_the_walk() {
    let (sent, got) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let id = |seq: usize| ElementId::of(&[b"act", &seq.to_be_bytes()]);
        let read = |seq: usize, cause: Cause, output: &str| {
            let act = Act { fingerprint: Fingerprint::of([("code/read", br#"{"path":"a.rs"}"#.as_slice())]), effect: ToolEffect::Observes };
            Item::rendered(id(seq), cause, seq.to_string(), output.to_owned(), Content::Hashed(ContentHash::of(output))).with_act(act)
        };
        let items = vec![read(1, Cause::Of(id(2)), "one"), read(2, Cause::Of(id(1)), "two"), read(3, Cause::Of(id(2)), "three")];
        let (_, repeated_of) = super::super::repeat::resolve(&Acts(Vec::new()), items, |_| true, |_| true);
        sent.send(repeated_of).ok();
    });
    let repeated_of = got.recv_timeout(std::time::Duration::from_secs(5));
    assert_eq!(repeated_of, Ok(vec![None, None, None]), "the walk ends at the cycle and finds no repeat");
}

// what this catches: the same output from a tool that changes the world read as "nothing
// happened" (Codex: two appends that both answer OK are two appends), and a tool that
// declares nothing ever rendered as a repeat. (A reopen repeats only an identical reopen whose
// whole she holds: `reopening_the_same_output_again_is_the_repeat_and_the_newest_act_is_judged_too`.)
#[test]
fn only_a_declared_observation_can_repeat() {
    for effect in [ToolEffect::Changes, ToolEffect::Undeclared] {
        let mut acts = Acts(Vec::new());
        let (mut view, _) = View::start(&crate::cost::Chars, vec!["identity".into()], &[&acts], all_full).unwrap(); // unwrap: one grant per claim cannot mismatch
        acts.push("tool/x", r#"{"line":"a"}"#, effect, "OK");
        view.advance(&[&acts]);
        acts.push("tool/x", r#"{"line":"a"}"#, effect, "OK");
        assert_eq!(view.advance(&[&acts]).repeats, 0, "{effect:?}");
        assert!(view.renders("acts", "1") && view.renders("acts", "2"), "{effect:?}: both occurrences whole");
    }
}

// what this catches: a repeat pointing at a whole that is not in front of her (collapsed by
// the start, or gone with a turnover), whose reopen would answer another repeat and trap her
// (Codex and Fable on the design). The re-read arrives whole; and a turnover renders an
// earlier repeat as itself.
#[test]
fn a_repeat_needs_its_ancestor_whole_in_this_epoch() {
    let mut acts = Acts(Vec::new());
    acts.push("code/read", r#"{"path":"a.rs"}"#, ToolEffect::Observes, &body("a.rs"));
    acts.push("code/read", r#"{"path":"b.rs"}"#, ToolEffect::Observes, &body("b.rs"));
    let (mut view, start) = View::start(&crate::cost::Chars, vec!["identity".into()], &[&acts], floors).unwrap(); // unwrap: one grant per claim cannot mismatch
    assert_eq!(start.collapsed + start.folded, 2, "the start holds both at their floor");
    assert!(!view.renders("acts", "1"));
    acts.push("code/read", r#"{"path":"a.rs"}"#, ToolEffect::Observes, &body("a.rs"));
    assert_eq!(view.advance(&[&acts]).repeats, 0, "#1 is collapsed: the re-read arrives whole");
    assert!(view.renders("acts", "3"));
    acts.push("code/read", r#"{"path":"a.rs"}"#, ToolEffect::Observes, &body("a.rs"));
    assert_eq!(view.advance(&[&acts]).repeats, 1, "#3 is whole now: #4 repeats it");
    assert!(rendered(&view).contains("[#4 repeat of #3,"));
    view.turn_over(&crate::cost::Chars, vec!["identity".into()], &[&acts], all_full).unwrap(); // unwrap: one grant per claim cannot mismatch
    let text = rendered(&view);
    assert!(!text.contains("repeat of"), "a turnover re-renders every item as itself: {text}");
    assert!(view.renders("acts", "4"));
}

// what this catches: an item already sent whole rewritten as a repeat on a later act, which
// changes text she was already sent and breaks the prefix every act appends to. Two equal
// reads arriving in one append both arrive whole (the first was not yet held), and the second
// keeps its whole rendering for the epoch.
#[test]
fn a_rendering_already_sent_is_never_turned_into_a_repeat() {
    let mut acts = Acts(Vec::new());
    let (mut view, _) = View::start(&crate::cost::Chars, vec!["identity".into()], &[&acts], all_full).unwrap(); // unwrap: one grant per claim cannot mismatch
    acts.push("code/read", r#"{"path":"a.rs"}"#, ToolEffect::Observes, &body("a.rs"));
    acts.push("code/read", r#"{"path":"a.rs"}"#, ToolEffect::Observes, &body("a.rs"));
    assert_eq!(view.advance(&[&acts]).repeats, 0);
    let before = rendered(&view);
    acts.push("code/read", r#"{"path":"b.rs"}"#, ToolEffect::Observes, &body("b.rs"));
    let advance = view.advance(&[&acts]);
    assert_eq!((advance.items, advance.repeats), (1, 0), "only the new read arrives");
    assert!(rendered(&view).starts_with(&before));
}

// what this catches (IntelMac, 2026-11-11: one saved output reopened 20+ times, each arriving
// byte-identical, her input equal to her last): an identical reopen whose whole is in front of her
// arriving whole again. It is the repeat; the FIRST reopen still arrives whole (a reopen never
// answers another repeat), and her newest act is judged the same way before it rides whole.
#[test]
fn reopening_the_same_output_again_is_the_repeat_and_the_newest_act_is_judged_too() {
    let mut acts = Acts(Vec::new());
    let (mut view, _) = View::start(&crate::cost::Chars, vec!["identity".into()], &[&acts], all_full).unwrap(); // unwrap: one grant per claim cannot mismatch
    acts.push("tool/output", r#"{"outputId":"255452cef96d2177"}"#, ToolEffect::Reopens, &body("saved"));
    assert_eq!(view.advance(&[&acts]).repeats, 0, "the first reopen arrives whole");
    assert!(view.renders("acts", "1"));
    acts.push("tool/output", r#"{"outputId":"255452cef96d2177"}"#, ToolEffect::Reopens, &body("saved"));
    assert_eq!(view.advance(&[&acts]).repeats, 1, "reopening it again, its whole in front of her, is the repeat");

    // Her newest act rides whole after her view, outside its sources: the view judges it too.
    let mut newest = Acts(acts.0.clone());
    newest.push("tool/output", r#"{"outputId":"255452cef96d2177"}"#, ToolEffect::Reopens, &body("saved"));
    let item = newest.items().pop().expect("the act just pushed");
    let block = view.repeat_of_newest(&acts, item).expect("her newest identical reopen is a repeat");
    assert!(block.contains("repeat of #1"), "{block}");
    let mut changed = Acts(acts.0.clone());
    changed.push("tool/output", r#"{"outputId":"255452cef96d2177"}"#, ToolEffect::Reopens, "a different body");
    assert!(view.repeat_of_newest(&acts, changed.items().pop().expect("pushed")).is_none(), "a changed outcome arrives whole");
}
