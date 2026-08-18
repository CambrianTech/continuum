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

/// The workspace a claimed card points at, or `None` when the card is not a staged
/// benchmark card (the ordinary case).
///
/// `card_titles` is every title she currently holds — resolution is over the WHOLE held
/// set rather than one card, because the question a work turn asks is "where are my hands
/// supposed to be", and that has one answer for the turn. Two different held cards
/// matching two different staged instances is the same ambiguity as one title matching
/// two, and refuses identically.
pub fn workspace_for_held_cards<'a, I>(peer: &uuid::Uuid, card_titles: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = &'a str>,
{
    let staged = staged_instances(peer);
    if staged.is_empty() {
        return None;
    }
    let titles: Vec<&str> = card_titles.into_iter().collect();
    let mut hits: Vec<&String> = staged
        .iter()
        .filter(|inst| titles.iter().any(|t| t.contains(inst.as_str())))
        .collect();
    hits.dedup();
    match hits.as_slice() {
        [one] => staging_root(peer).map(|root| root.join(one.as_str())),
        [] => None,
        many => {
            // Two staged instances named in her held titles. Rooting at either would put
            // her hands in a repo the other card is not about — and a diff taken there
            // scores a false zero for the one she was actually working. Refuse loudly.
            crate::probe!(
                class = "persona.work.staged_ambiguous",
                peer = %peer,
                matches = many.len(),
                candidates = many
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
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

    // what this catches: the containment rule is what binds a card to a repo, and it is
    // by construction (dispatch writes the title from the instance). A title that names
    // the instance resolves; an unrelated title does not, so an ordinary chat card never
    // silently re-roots a citizen's hands into a benchmark checkout.
    #[test]
    fn a_title_naming_a_staged_instance_selects_it_and_others_do_not() {
        // Pure over the matching rule — the disk half is exercised by the live path, and
        // a temp-dir fixture here would be testing `read_dir`, not the decision.
        let staged = ["sympy__sympy-24152".to_string(), "flask-4045".to_string()];
        let titles = ["benchmark: sympy__sympy-24152 — fix the printer"];
        let hit: Vec<&String> = staged
            .iter()
            .filter(|i| titles.iter().any(|t| t.contains(i.as_str())))
            .collect();
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0], "sympy__sympy-24152");

        let unrelated = ["let's discuss the roadmap"];
        let none: Vec<&String> = staged
            .iter()
            .filter(|i| unrelated.iter().any(|t| t.contains(i.as_str())))
            .collect();
        assert!(
            none.is_empty(),
            "an ordinary card must never re-root her hands"
        );
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
