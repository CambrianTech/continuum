//! **A resident log, ring, cache or store has a measured bound and a drain.**
//!
//! Joel, 2026-11-10: "As usual you guys never use recency and take all history into memory...
//! like some intro to programming JavaScript project." "Drains and not just appending." "Memory
//! and processing we are limited on severely." Measured that day on the M5: each resident's
//! action log held her whole life (4,522-5,686 records), its own doc said nothing bounded it, and
//! her checkpoint rewrote all of it, 12-46 MB of JSON, every ~15 s per mind.
//!
//! Two shapes count, both read from source:
//! - **A store that only grows.** A `Vec`/`VecDeque` field of a type named as a resident store
//!   (`…Log`, `…Ring`, `…Cache`, `…Store`, `…Archive`, `…History`, `…Buffer`) that its file pushes
//!   to and never drains: no `pop`/`drain`/`truncate`/`retain`/`remove`/`split_off`/`dedup`/
//!   `swap_remove` on it, and no `mem::take`/`mem::replace` of it. `clear` is not a drain: a reset
//!   is not a bound.
//! - **A store that says it is unbounded**: a comment saying one of [`UNBOUNDED_SAYS`] (that
//!   nothing in it bounds it, that a record stays while her mind does, that it is reloaded in
//!   full).
//!
//! The count may only fall, and a touched file leaves with none: bound it by what its consumer
//! measures (her view's fold edge, a pressure owner's shed) and drain it, oldest first.

use super::{lexer::split_code_and_comment, SourceFile, SourceRule, Violation};

const RULE: &str = "resident-bounds";

/// A type named as one of these holds a resident collection.
const STORE_SUFFIXES: &[&str] = &["Log", "Ring", "Cache", "Store", "Archive", "History", "Buffer"];

/// What a comment says when it declares the collection it documents unbounded.
const UNBOUNDED_SAYS: &[&str] = &["nothing here bounds it", "for as long as her mind lives", "is restored whole"];

/// Calls that grow a collection, and the ones that drain it.
const GROWS: &[&str] = &["push", "push_back", "push_front", "extend"];
const DRAINS: &[&str] = &["pop", "pop_front", "pop_back", "drain", "truncate", "retain", "retain_mut", "remove", "split_off", "dedup", "dedup_by_key", "swap_remove"];

pub struct ResidentBounds;

/// The store type a `struct` line opens, when its name ends like one.
pub fn store_struct(code: &str) -> Option<&str> {
    let after = code.split_once("struct ")?.1;
    let name: &str = after.split(|c: char| !(c.is_alphanumeric() || c == '_')).next()?;
    STORE_SUFFIXES.iter().any(|suffix| name.ends_with(suffix) && name != *suffix).then_some(name)
}

/// The field a line declares as a `Vec` or `VecDeque`.
pub fn collection_field(code: &str) -> Option<&str> {
    let (left, right) = code.split_once(':')?;
    let ty = right.trim_start();
    let ty = ty.strip_prefix("std::collections::").unwrap_or(ty); // unwrap_or: an unqualified type is the same type
    if !(ty.starts_with("Vec<") || ty.starts_with("VecDeque<")) {
        return None;
    }
    let name = left.trim().rsplit(' ').next()?;
    (!name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_')).then_some(name)
}

/// Whether `code` calls one of `calls` on `.field`.
fn calls_on(code: &str, field: &str, calls: &[&str]) -> bool {
    let receiver = format!(".{field}.");
    code.match_indices(&receiver).any(|(at, _)| {
        let rest = &code[at + receiver.len()..];
        calls.iter().any(|call| rest.strip_prefix(*call).is_some_and(|tail| tail.starts_with('(')))
    })
}

/// Whether `code` moves the whole of `.field` out (`mem::take` / `mem::replace`).
fn moves_out(code: &str, field: &str) -> bool {
    ["mem::take(&mut ", "mem::replace(&mut "].iter().any(|call| {
        code.match_indices(*call).any(|(at, _)| {
            let target = &code[at + call.len()..];
            let target = target.split([')', ',']).next().unwrap_or(target); // unwrap_or: split always yields its first piece
            target.ends_with(&format!(".{field}")) || target == field
        })
    })
}

/// Whether a comment declares its collection unbounded.
pub fn says_unbounded(comment: &str) -> bool {
    let comment = comment.to_ascii_lowercase();
    UNBOUNDED_SAYS.iter().any(|says| comment.contains(says))
}

impl SourceRule for ResidentBounds {
    fn name(&self) -> &'static str {
        RULE
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        let lines: Vec<(usize, &str, &str)> = file
            .production_lines()
            .map(|(line, text)| {
                let (code, comment) = split_code_and_comment(text);
                (line, code, comment.unwrap_or_default()) // unwrap_or_default: a line with no comment says nothing
            })
            .collect();
        let violation = |line: usize, text: String| Violation { rule: RULE, file: file.repo.clone(), line, source: text };
        let mut found = Vec::new();
        let mut store: Option<&str> = None;
        for (line, code, comment) in &lines {
            if says_unbounded(comment) {
                found.push(violation(*line, format!("declares itself unbounded: {}", comment.trim())));
            }
            if let Some(name) = store_struct(code).filter(|_| code.trim_end().ends_with('{')) {
                store = Some(name);
                continue;
            }
            if code.trim_start().starts_with('}') {
                store = None;
                continue;
            }
            let Some(owner) = store else { continue };
            let Some(field) = collection_field(code) else { continue };
            let grows = lines.iter().any(|(_, code, _)| calls_on(code, field, GROWS));
            let drains = lines.iter().any(|(_, code, _)| calls_on(code, field, DRAINS) || moves_out(code, field));
            if grows && !drains {
                found.push(violation(*line, format!("{owner}.{field} only grows: {}", code.trim())));
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;

    // what this catches: a resident log that only appends (the M5, 2026-11-10: 4,522-5,686 action
    // records per resident, whole, in RAM and in a 12-46 MB checkpoint rewritten every ~15 s), or
    // one staying in a file a change touched. The count may only fall.
    #[test]
    fn resident_stores_without_a_drain_fall_and_touched_files_have_none() {
        let violations = scan(&[&ResidentBounds]);
        let advice = || {
            "A resident log, ring, cache or store is bounded by what its consumer measures (her view's \
             fold edge, the memory owner's shed) and drains oldest first; what leaves goes to a store \
             that drains too (working_memory/archive.rs)."
                .to_string()
        };
        crate::ratchet::hold_sites("resident_stores_without_a_drain", &violations, crate::ratchet::Touched::EndsClean, advice);
    }

    // what this catches: the predicate missing a push-only store, flagging one that drains, or
    // reading a reset as a bound.
    #[test]
    fn the_predicate_flags_a_store_that_only_grows_and_passes_one_that_drains() {
        let only_grows = "pub struct ActionLog {\n    records: VecDeque<Record>,\n}\nfn push(&mut self, r: Record) { self.records.push_back(r); }\n";
        let file = SourceFile::for_test("a.rs", only_grows);
        assert_eq!(ResidentBounds.check(&file).len(), 1, "push-only store");
        let reset = format!("{only_grows}fn clear(&mut self) {{ self.records.clear(); }}\n");
        assert_eq!(ResidentBounds.check(&SourceFile::for_test("a.rs", &reset)).len(), 1, "clear is a reset, not a bound");
        for drain in ["self.records.pop_front();", "self.records.drain(..n);", "let all = std::mem::take(&mut self.records);"] {
            let drains = format!("{only_grows}fn drain(&mut self) {{ {drain} }}\n");
            assert!(ResidentBounds.check(&SourceFile::for_test("a.rs", &drains)).is_empty(), "{drain} drains");
        }
        let not_a_store = "pub struct Batch {\n    items: Vec<Item>,\n}\nfn add(&mut self, i: Item) { self.items.push(i); }\n";
        assert!(ResidentBounds.check(&SourceFile::for_test("a.rs", not_a_store)).is_empty(), "a type not named as a store");
    }

    // what this catches: a doc that tells the reader nothing bounds a store passing unread, or a
    // phrase in code (not a comment) counted.
    #[test]
    fn the_predicate_flags_a_store_that_says_it_is_unbounded() {
        assert!(says_unbounded(" A log from records. Nothing here bounds it: a record stays"));
        assert!(says_unbounded(" keeps its handle for as long as her mind lives"));
        assert!(!says_unbounded(" bounded by her view's fold edge"));
        let file = SourceFile::for_test("a.rs", "let s = \"nothing here bounds it\";\n");
        assert!(ResidentBounds.check(&file).is_empty(), "a string in code is not a declaration");
        assert_eq!(store_struct("pub(super) struct ThoughtLog {"), Some("ThoughtLog"));
        assert_eq!(store_struct("struct Log {"), None, "a bare suffix names no store");
        assert_eq!(collection_field("    pub records: std::collections::VecDeque<Record>,"), Some("records"));
        assert_eq!(collection_field("    count: usize,"), None);
    }
}
