//! Which on-disk workspace does a citizen's claimed card point at?
//!
//! ONE answer, three callers. A staged benchmark instance is a real git checkout under
//! `<home>/citizens/peers/<peer>/workspace/swe/<instance>`, written by `benchmark/swe-setup`
//! at dispatch. Three places need to resolve it and, before this module, two of them each
//! walked that directory themselves:
//!
//! - `persona/roster` — reports the staged list as the REUSE signal (dispatch found the
//!   checkout and skipped cloning)
//! - `modules/work.rs::dispatch_staged_swe_solve` — matches a claimed card to its instance
//! - `persona/service_loop` — roots her HANDS at that instance for a work turn
//!
//! The third is why this module exists. A citizen working her claimed card must ACT IN THE
//! REPO, and the layout knowledge that makes that possible was previously inline in a
//! benchmark dispatcher — so the live path could not reuse it without importing the
//! bypass it is meant to replace.
//!
//! ## The matching rule, and why it refuses rather than guesses
//!
//! A card matches an instance when the card's TITLE CONTAINS the instance directory name
//! (`sympy__sympy-24152`). Dispatch writes the title, so the containment is by
//! construction, not inference.
//!
//! Zero matches → `None`: an ordinary non-bench card, and her hands stay where they are.
//! MORE than one match → `None` AND a probe: two staged instances whose names both appear
//! in one title is a staging defect, and picking either would root her hands in a repo her
//! card is not about — silently scoring a false zero against the other. Refusing is the
//! honest outcome and the probe says which candidates collided.

use std::path::PathBuf;

/// Where a citizen's staged benchmark checkouts live.
///
/// Not configurable and not guessed: this mirrors exactly what `benchmark/swe-setup`
/// writes. A single expression of the layout, so a change to staging cannot leave a reader
/// looking in a directory the writer stopped using.
pub fn staging_root(peer: &uuid::Uuid) -> Option<PathBuf> {
    let home = crate::commands::benchmark::continuum_home().ok()?;
    Some(
        home.join("citizens")
            .join("peers")
            .join(peer.to_string())
            .join("workspace")
            .join("swe"),
    )
}

/// Every benchmark instance actually staged in this citizen's workspace, name-sorted.
///
/// Counts only directories that carry a `.git` — a real checkout, not an empty shell left
/// by an interrupted clone. Best effort by design: a missing home or unreadable directory
/// yields an empty list, never an error, because every caller is answering "what is here
/// right now" and none of them should fail because staging has not run yet.
pub fn staged_instances(peer: &uuid::Uuid) -> Vec<String> {
    let Some(root) = staging_root(peer) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().join(".git").exists())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    out.sort();
    out
}

/// Which staged instance a set of card titles points at — the full answer, including the
/// two ways there isn't one.
///
/// Callers need to tell these apart. A work turn treats [`None`](CardWorkspace::None) as
/// "an ordinary card, hands stay put" and [`Ambiguous`](CardWorkspace::Ambiguous) as a
/// staging defect worth reporting; a claim dispatcher wants the instance NAME as well as
/// the path (its era venv is keyed by it). An `Option<PathBuf>` erased both distinctions,
/// which is why the second caller kept its own copy of the walk.
#[derive(Debug, PartialEq, Eq)]
pub enum CardWorkspace {
    /// Exactly one staged instance is named by these titles.
    One { instance: String, path: PathBuf },
    /// No staged instance is named — the ordinary, non-benchmark case.
    None,
    /// More than one. Never resolved: rooting at either would put hands in a repo the
    /// other card is not about, and a diff taken there scores a false zero for the one
    /// actually being worked.
    Ambiguous { candidates: Vec<String> },
}

/// Resolve card titles against this citizen's staged checkouts.
///
/// `card_titles` is every title in play — resolution is over the WHOLE set rather than one
/// card, because the question a work turn asks is "where are my hands supposed to be", and
/// that has one answer for the turn. Two different held cards matching two different
/// staged instances is the same ambiguity as one title matching two, and refuses
/// identically.
pub fn resolve_for_titles<'a, I>(peer: &uuid::Uuid, card_titles: I) -> CardWorkspace
where
    I: IntoIterator<Item = &'a str>,
{
    let staged = staged_instances(peer);
    let titles: Vec<&str> = card_titles.into_iter().collect();
    match select(&staged, &titles) {
        Selection::One(instance) => match staging_root(peer) {
            Some(root) => CardWorkspace::One {
                path: root.join(&instance),
                instance,
            },
            None => CardWorkspace::None,
        },
        Selection::None => CardWorkspace::None,
        Selection::Ambiguous(candidates) => CardWorkspace::Ambiguous { candidates },
    }
}

/// The matching rule alone, with the filesystem taken out of it.
///
/// Split from [`resolve_for_titles`] so the rule is TESTED rather than restated: the
/// disk half needs a fixture, the decision does not, and a test that re-derives the
/// containment check in its own body cannot fail when the real one changes.
#[derive(Debug, PartialEq, Eq)]
enum Selection {
    One(String),
    None,
    Ambiguous(Vec<String>),
}

fn select(staged: &[String], titles: &[&str]) -> Selection {
    let mut hits: Vec<&String> = staged
        .iter()
        .filter(|inst| titles.iter().any(|t| t.contains(inst.as_str())))
        .collect();
    hits.dedup();
    match hits.as_slice() {
        [one] => Selection::One((*one).clone()),
        [] => Selection::None,
        many => Selection::Ambiguous(many.iter().map(|s| (*s).clone()).collect()),
    }
}

/// The workspace a claimed card points at, or `None` when there isn't exactly one — the
/// projection a WORK TURN wants, which only needs "root here or leave her hands alone".
///
/// Ambiguity probes here rather than at [`resolve_for_titles`] because this is the caller
/// that would silently score a false zero: it roots hands and a diff is taken afterwards.
/// A dispatcher matching on the enum reports ambiguity in its own vocabulary instead.
pub fn workspace_for_held_cards<'a, I>(peer: &uuid::Uuid, card_titles: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = &'a str>,
{
    match resolve_for_titles(peer, card_titles) {
        CardWorkspace::One { path, .. } => Some(path),
        CardWorkspace::None => None,
        CardWorkspace::Ambiguous { candidates } => {
            crate::probe!(
                class = "persona.work.staged_ambiguous",
                peer = %peer,
                matches = candidates.len(),
                candidates = candidates.join(","),
                "held cards name MULTIPLE staged instances — refusing to guess which repo \
                 her hands belong in; she works in her own workspace this turn"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn staged(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| (*s).to_string()).collect()
    }

    // what this catches: the containment rule is what binds a card to a repo, and it is
    // by construction (dispatch writes the title from the instance). A title that names
    // the instance resolves; an unrelated title does not, so an ordinary chat card never
    // silently re-roots a citizen's hands into a benchmark checkout.
    #[test]
    fn a_title_naming_a_staged_instance_selects_it_and_others_do_not() {
        let staged = staged(&["sympy__sympy-24152", "psf__requests-2148"]);
        assert_eq!(
            select(&staged, &["benchmark: sympy__sympy-24152 — fix the printer"]),
            Selection::One("sympy__sympy-24152".to_string())
        );
        assert_eq!(
            select(&staged, &["let's discuss the roadmap"]),
            Selection::None,
            "an ordinary card must never re-root her hands"
        );
    }

    // what this catches: the ambiguity arm collapsing into a pick. Two staged instances
    // named across the titles in play must REFUSE — rooting at either puts hands in a repo
    // the other card is not about, and the diff taken there scores a false zero for the one
    // actually being worked. Both callers depend on this staying distinguishable from
    // "nothing staged": the work turn probes and leaves her hands alone, the claim
    // dispatcher declines to fire a solve.
    #[test]
    fn titles_naming_two_staged_instances_refuse_rather_than_pick() {
        let staged = staged(&["sympy__sympy-24152", "psf__requests-2148"]);
        let both = select(
            &staged,
            &["bench: sympy__sympy-24152", "bench: psf__requests-2148"],
        );
        match both {
            Selection::Ambiguous(c) => assert_eq!(c.len(), 2, "both candidates must be named"),
            other => panic!("two matches must refuse, got {other:?}"),
        }
    }

    // what this catches: nothing staged at all — the ordinary case for every non-benchmark
    // citizen, which must resolve without the rule ever finding a hit.
    #[test]
    fn a_citizen_with_nothing_staged_selects_nothing() {
        assert_eq!(select(&[], &["bench: sympy__sympy-24152"]), Selection::None);
    }

    // what this catches: a citizen with NO staged instances resolves to None without
    // touching the filesystem layout at all — the ordinary case for every non-benchmark
    // citizen, and the one that must stay free.
    #[test]
    fn a_citizen_with_nothing_staged_has_no_card_workspace() {
        let peer = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"nothing-staged-fixture");
        assert!(workspace_for_held_cards(&peer, ["benchmark: anything"]).is_none());
    }
}
