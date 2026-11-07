//! A source a start SHED for want of its floor ([`View::withhold`]): its backlog waits for a start
//! that funds it, never arriving whole on the next act.

use super::*;

/// Atlas's view on the M5, 2026-11-07 09:44:47Z: a kept source beside her thoughts, 287 of them at
/// a mean 942 chars (`delib.view.advance` 09:46:35Z: `thoughts:287/270351`).
fn atlas() -> (Fake, Fake) {
    let mut kept = Fake::new("room-wall", 6);
    kept.push("wall", &"wall post ".repeat(490));
    let mut thoughts = Fake::new("thoughts", 5);
    for n in 0..287 {
        thoughts.push(&format!("thought#{n}"), &format!("thought {n} ").repeat(85));
    }
    (kept, thoughts)
}

fn whole(claims: &[Claim]) -> Vec<usize> {
    claims.iter().map(|claim| claim.full).collect()
}

// what this catches: the M5 on 2026-11-07 (build a947e4ed1), where a start that shed her thoughts
// for want of their floor was followed by an act appending all 287 of them whole (270,351 chars):
// the epoch turned over on every other act, her history re-prefilled (broke_in=history 14 of 19
// acts), and the backlog read as one act's growth, so the start after it reserved nothing. A shed
// source's backlog waits for a start that funds it; only what she thinks after the start arrives.
// The same advance on a view that did not withhold it appends the whole backlog: the defect.
#[test]
fn a_shed_sources_backlog_waits_for_a_start_and_never_turns_the_epoch_over() -> Result<(), GrantMismatch> {
    let (kept, mut thoughts) = atlas();
    let (mut view, _) = View::start(&crate::cost::Chars, vec!["head".into()], &[&kept, &thoughts], whole)?;
    let (mut unwithheld, _) = View::start(&crate::cost::Chars, vec!["head".into()], &[&kept, &thoughts], whole)?;
    // The allocation could not grant her thoughts their floor: the start sheds them.
    view.turn_over(&crate::cost::Chars, vec!["head".into()], &[&kept], whole)?;
    view.withhold(&[&thoughts]);
    unwithheld.turn_over(&crate::cost::Chars, vec!["head".into()], &[&kept], whole)?;
    let before = rendered(&view);
    let newest = "thought 287 ".repeat(85);
    thoughts.push("thought#287", &newest);
    let advance = view.advance(&[&kept, &thoughts]);
    assert_eq!(advance.chars, newest.len(), "only the thought after the start arrives");
    assert!(rendered(&view).starts_with(&before), "the epoch only appended");
    assert_eq!(view.appended_per_act(), Some(newest.len()), "the next start reserves one act's growth, not the backlog");
    assert!(!view.renders("thoughts", "thought#0"), "the shed backlog is not in her view this epoch");
    let backlog = unwithheld.advance(&[&kept, &thoughts]);
    assert!(backlog.chars > 270_000, "without the withhold the backlog arrives whole: {}", backlog.chars);
    Ok(())
}

// what this catches: a withheld item never coming back. A start that funds the shed source shows
// it again, and a start that would show one above its pointer is a restore, so the view takes it
// when a turnover is free.
#[test]
fn a_start_that_funds_a_shed_source_restores_its_backlog() -> Result<(), GrantMismatch> {
    let (kept, thoughts) = atlas();
    let (mut view, _) = View::start(&crate::cost::Chars, vec!["head".into()], &[&kept, &thoughts], whole)?;
    view.turn_over(&crate::cost::Chars, vec!["head".into()], &[&kept], whole)?;
    view.withhold(&[&thoughts]);
    assert!(view.start_would_restore(&crate::cost::Chars, &[&kept, &thoughts], whole)?, "a start funding her thoughts gives them back");
    let start = view.turn_over(&crate::cost::Chars, vec!["head".into()], &[&kept, &thoughts], whole)?;
    assert!(view.renders("thoughts", "thought#0"), "the funded start shows the backlog again");
    assert_eq!(start.restored_items, 287, "each returning thought is receipted as restored");
    assert_eq!(view.advance(&[&kept, &thoughts]), Advance::default(), "nothing is withheld after the start that funded it");
    Ok(())
}
