//! **One state is one type, never a set of flags** (CONTINUUM-ENGINE.md, RTOS rule 11).
//!
//! Two `bool`s hold four states whether or not four exist: `Demand { in_flight, settled }` rebuilt
//! a `Phase` enum into flags, and `in_flight && settled` became representable. Parallel `Option`s
//! matched as a tuple are the same shape with a `_ =>` catch-all for the combinations nobody meant
//! (`match (record.text, record.handle, record.stored_chars)`; review of 2026-11-14). The sites:
//! - a struct with two or more `bool` (or `AtomicBool`) fields, at the struct;
//! - a `match (a, b, ..)` whose tuple reads two or more `Option` fields of one struct declared in
//!   the same crate, at the `match`.
//!
//! The second is crate-wide: the struct and the match may sit in different files, so the rule
//! learns every crate's `Option`-field sets first ([`Shape::learn`]).

use std::collections::{BTreeMap, BTreeSet};

use super::items::{fields, items, read_name, top_level, type_is};
use super::{CrateDir, Shape, Site, Unit};
use crate::lexer::tokens::{matching_close, Token};

/// The `Option` field names of each struct with two or more of them, filed by crate.
#[derive(Default)]
pub struct StateAsFlags {
    optional_fields: BTreeMap<CrateDir, Vec<BTreeSet<FieldName>>>,
}

/// A struct field's name, as the tuple-match check compares it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FieldName(String);

const FLAG_STRUCT: &str = "a struct holding one state as two or more bools";
const OPTION_TUPLE: &str = "Option fields matched together: one state spread across fields";

/// The flag types a struct field can be.
const FLAGS: &[&str] = &["bool", "AtomicBool"];

impl Shape for StateAsFlags {
    fn name(&self) -> &'static str {
        "state_as_flags"
    }

    fn advice(&self) -> &'static str {
        "Name the states: an enum whose variants are exactly the combinations that exist \
         (`enum ThoughtBody { Resident(String), Shed { handle, chars } }`, `Demand { phase: Phase }`), \
         so an impossible combination does not compile and a match needs no catch-all."
    }

    const LEARNS: bool = true;

    fn learn(&mut self, unit: &Unit) {
        for item in items(&unit.code, "struct") {
            let optional: BTreeSet<FieldName> = fields(&unit.code, item.body)
                .into_iter()
                .filter(|f| unit.code.get(f.ty.start).is_some_and(|t| t.is_ident("Option")))
                .filter_map(|f| f.name.map(|n| FieldName(n.to_owned())))
                .collect();
            if optional.len() >= 2 {
                self.optional_fields.entry(unit.text.krate.clone()).or_default().push(optional);
            }
        }
    }

    fn sites(&self, unit: &Unit) -> Vec<Site> {
        let code = &unit.code;
        let mut out: Vec<Site> = items(code, "struct")
            .into_iter()
            .filter(|item| fields(code, item.body.clone()).iter().filter(|f| type_is(code, f.ty.clone(), FLAGS)).count() >= 2)
            .map(|item| Site { line: item.line, what: FLAG_STRUCT })
            .collect();
        let structs = self.optional_fields.get(&unit.text.krate).map_or(&[][..], Vec::as_slice); // map_or: a crate with no such struct has none to match
        for at in 0..code.len() {
            if code[at].is_ident("match") && code.get(at + 1).is_some_and(|t| t.is('(')) && matches_options_together(code, at + 1, structs) {
                out.push(Site { line: code[at].line, what: OPTION_TUPLE });
            }
        }
        out.sort_by_key(|s| s.line);
        out
    }
}

/// Whether the tuple opened at `open` (a match scrutinee) reads two or more `Option` fields of one
/// of `structs`.
fn matches_options_together(code: &[Token], open: usize, structs: &[BTreeSet<FieldName>]) -> bool {
    let close = matching_close(code, open);
    // A tuple scrutinee is followed by the arms' block; `match (x) {` with one element is a group.
    let elements = top_level(code, open + 1..close);
    if elements.len() < 2 || !code.get(close + 1).is_some_and(|t| t.is('{')) {
        return false;
    }
    let names: Vec<FieldName> = elements
        .iter()
        .filter_map(|e| e.end.checked_sub(1).and_then(|end| read_name(code, end)))
        .map(|n| FieldName(n.to_owned()))
        .collect();
    structs.iter().any(|optional| names.iter().filter(|n| optional.contains(*n)).count() >= 2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokens::tokenize;
    use crate::shapes::{Half, Target, Text};

    fn found(src: &str) -> Vec<&'static str> {
        let text = Text { repo: "core/x/src/a.rs".into(), krate: CrateDir("x".into()), half: Half::Production, target: Target::Library, text: src.into() };
        let unit = Unit { text: &text, code: tokenize(&text.text).code, lines: text.text.lines().collect() };
        let mut rule = StateAsFlags::default();
        rule.learn(&unit);
        rule.sites(&unit).into_iter().map(|s| s.what).collect()
    }

    // what this catches: a new state spread across flags in any core crate.
    #[test]
    fn state_as_flags_only_falls() {
        crate::shapes::hold(StateAsFlags::default());
    }

    // what this catches: the predicate missing a shape the 2026-11-14 review found.
    #[test]
    fn flags_and_options_matched_together_are_sites() {
        assert_eq!(found("pub struct Demand { pub in_flight: bool, settled: bool, tokens: u32 }"), [FLAG_STRUCT]);
        assert_eq!(found("struct Cache { in_flight: AtomicBool, stuck_said: AtomicBool }"), [FLAG_STRUCT]);
        assert_eq!(
            found("struct Record { text: Option<String>, handle: Option<Handle>, stored_chars: Option<usize> }\n\
                   fn f(r: Record) { match (r.text, r.handle.as_ref(), r.stored_chars) { (Some(t), _, _) => t, _ => d() } }"),
            [OPTION_TUPLE]
        );
    }

    // what this catches: one flag, Options of different structs, or a plain tuple match counted as
    // one state in pieces.
    #[test]
    fn one_flag_and_unrelated_tuples_are_not_sites() {
        assert!(found("struct S { enabled: bool, n: u32 }").is_empty());
        assert!(found("struct A { a: Option<u8>, n: u8 }\nfn f() { match (x.a, y.n) { _ => () } }").is_empty());
        assert!(found("struct R { a: Option<u8>, b: Option<u8> }\nfn f() { match (left, right) { _ => () } }").is_empty());
        assert!(found("struct R { a: Option<u8>, b: Option<u8> }\nfn f() { match (r.a) { _ => () } }").is_empty(), "a group, not a tuple");
    }
}
