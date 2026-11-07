//! The collapse depth end to end: her view measures her re-reads (`crate::rereads`), its claims
//! carry them, and a start under `crate::depth::choose` renders what the measurement chose.

use super::*;
use crate::cause::{Act, ContentHash, Fingerprint, ToolEffect};
use crate::depth::{choose, Depth, Room};

/// Her reads, one occurrence per act, each a file far larger than its pointer.
struct Reads(Vec<String>);

impl Source<String> for Reads {
    fn name(&self) -> &str {
        "acts"
    }
    fn priority(&self) -> u8 {
        5
    }
    fn items(&self) -> Vec<Item<String>> {
        self.0
            .iter()
            .enumerate()
            .map(|(seq, path)| {
                let body = format!("// {path}\n") + &"fn parse(&mut self) -> Result<Tree, Error> { self.step()?; }\n".repeat(60);
                let act = Act { fingerprint: Fingerprint::of([("code/read", path.as_bytes())]), effect: ToolEffect::Observes };
                Item::rendered(ElementId::of(&[b"read", &seq.to_be_bytes()]), Cause::Unrecorded, seq.to_string(), body.clone(), Content::Hashed(ContentHash::of(&body)))
                    .with_pointer(format!("[#{seq} code/read {path}: collapsed]"))
                    .with_act(act)
            })
            .collect()
    }
}

// what this catches: the depth wired end to end, from her view's own measurement to what a start
// renders. She reads twenty files once and then works between two, re-reading each every other
// act: her measured re-read chance is high at ages 1-2 and nil past them, so a start keeps her two
// working files whole (collapsing them would re-send them most acts) and collapses the twenty she
// never asked for again, although the miss term alone would collapse all of them.
#[test]
fn a_start_keeps_whole_what_she_measurably_re_reads_and_collapses_the_rest() {
    let mut reads = Reads(Vec::new());
    let (mut view, _) = View::start(Vec::new(), &[&reads as &dyn Source<String>], |claims| claims.iter().map(|claim| claim.full_chars).collect()).unwrap(); // unwrap: one grant per claim cannot mismatch
    for n in 0..20 {
        reads.0.push(format!("old{n}.rs"));
        view.advance(&[&reads]);
    }
    for n in 0..20 {
        reads.0.push(if n % 2 == 0 { "a.rs" } else { "b.rs" }.to_owned());
        view.advance(&[&reads]);
    }
    let (all, _) = view.motion().rereads().totals();
    assert_eq!(all, 18, "every working read after the first two re-requests one she had");
    let depth = |claims: &[Claim]| -> Vec<usize> {
        let growing: Vec<_> = claims.iter().map(|claim| claim.growing(claim.full_chars, 1.0)).collect();
        match choose(Room { growable: 100_000, per_act: Some(4_000) }, &growing) {
            Depth::Chosen(chosen) => chosen.shares,
            Depth::Unmeasured => panic!("her re-reads are measured at every age her items have"),
        }
    };
    view.turn_over(Vec::new(), &[&reads], depth).unwrap(); // unwrap: one grant per claim cannot mismatch
    let newest = reads.0.len() - 1;
    assert!(view.renders("acts", &newest.to_string()), "her newest read is whole");
    assert!(view.renders("acts", &(newest - 1).to_string()), "the file she re-reads every other act stays whole");
    assert!(!view.renders("acts", "0"), "a file she read once, forty acts ago, collapses");
    assert!(!view.renders("acts", &(newest - 6).to_string()), "past the ages she re-reads at, her history collapses");
}
