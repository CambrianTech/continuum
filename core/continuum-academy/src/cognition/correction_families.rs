//! The task family a correction made on ordinary work is examined on (ONE-RESIDENT-MODEL §9.6
//! D5, card 9fa1ae13). A correction's examples name a graded task only when it worked one; a
//! reviewed correction of ordinary work records `notATask`, so its family can't be read from
//! them. It is DECLARED here instead, keyed by the real card id its examples carry, with the
//! lesson and the evidence that defined it, so no example is relabeled and the binding is
//! reviewed like code (docs/genome/correction-families.json).

use serde::Deserialize;
use std::collections::BTreeSet;

/// The committed bindings, baked into the binary like the acceptance sets they name.
const BINDINGS: &str = include_str!("../../../../docs/genome/correction-families.json");

/// One reviewed correction and the held-out family that tests its lesson. Each committed entry
/// also carries `lesson` (what the review corrected) and `evidence` (where that was read), for
/// the reviewer of the file; the test below holds both present.
#[derive(Debug, Deserialize)]
struct Binding {
    card: String,
    family: String,
}

/// The families declared for any of `cards` (a correction's examples' card ids). `Err` when the
/// committed bindings don't parse: an unreadable table is a broken build, never "none declared".
pub fn declared<'a>(cards: impl IntoIterator<Item = &'a String>) -> Result<BTreeSet<String>, String> {
    let bindings: Vec<Binding> = serde_json::from_str(BINDINGS) // committed file: docs/genome/correction-families.json
        .map_err(|e| format!("docs/genome/correction-families.json does not parse: {e}"))?;
    let cards: BTreeSet<&String> = cards.into_iter().collect();
    Ok(bindings.into_iter().filter(|b| cards.contains(&b.card)).map(|b| b.family).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a committed binding that does not parse, or names a family this node
    // can't resolve or whose tasks don't parse, found at the first sitting instead of at review.
    #[test]
    fn every_committed_binding_names_a_resolvable_family_of_held_out_tasks() {
        let bindings: Vec<Binding> = serde_json::from_str(BINDINGS).expect("test: the committed bindings parse");
        assert!(!bindings.is_empty());
        let raw: Vec<serde_json::Value> = serde_json::from_str(BINDINGS).expect("test: parses");
        for entry in &raw {
            for field in ["lesson", "evidence"] {
                assert!(entry[field].as_str().is_some_and(|v| !v.trim().is_empty()), "{entry}: a binding states its {field}");
            }
        }
        for b in &bindings {
            let reference = crate::cognition::eval_set::eval_set_reference(&b.family).expect("test: the family resolves");
            let (origin, text) = crate::cognition::gym::resolve_gym(&reference).expect("test: the family's set reads");
            let tasks = crate::cognition::gym::parse_tasks(&text, &origin).expect("test: its tasks parse");
            assert!(!tasks.is_empty(), "{}: an empty family examines nothing", b.family);
            assert!(tasks.iter().all(|t| t.held_out_shell.is_some()), "{}: every task checks the lesson held out", b.family);
        }
        let card = "2de02f40-3058-4b6a-9760-32bcac75219f".to_string();
        assert_eq!(declared([&card]).expect("parses").into_iter().collect::<Vec<_>>(), ["lc-parent-scope"]);
        assert!(declared([&"another-card".to_string()]).expect("parses").is_empty());
    }
}
