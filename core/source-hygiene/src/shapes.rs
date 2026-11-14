//! **Rust written like JavaScript, counted in every core crate** (CONTINUUM-ENGINE.md, RTOS rules
//! 10 and 11).
//!
//! Joel, 2026-11-14: "We're gonna have to fix all of this eventually. I can't even keep track of
//! the thousands of these you've made." The agents wrote these shapes, so the tracking is
//! mechanical, never a person's memory. Five rules, each a token predicate over the shared lexer
//! ([`crate::lexer::tokens`]: strings, comments and char literals are never code), each held twice:
//!
//! | Rule | The shape |
//! |---|---|
//! | [`waits::WaitsThatDecideFlow`] | a sleep, an interval, a periodic tick, a `Duration` constant, or a timeout whose `Err` side does not fail |
//! | [`unknown_as_value::UnknownAsValue`] | "not measured" stored as `0`, `0.0`, `MAX` or hidden by `.max(1)` |
//! | [`stringly_errors::StringlyErrors`] | a failure typed as `String` or `()` in library code |
//! | [`state_as_flags::StateAsFlags`] | one state spread across `bool` fields, or `Option` fields matched together as a tuple |
//! | [`string_identities::StringIdentities`] | a JSON field read by name outside wire code, a map keyed by `String` in a struct |
//!
//! # Where they count
//!
//! Every `core/` workspace member, read from the audited tree's manifest: `src/` split into its
//! production and test halves (the same split every rule here uses, [`crate::lexer::production_half`]),
//! and `tests/` as test code. Not only the audited set: a crate outside the per-file gates is where
//! these shapes would hide. Upstream ports under a `vendored/` directory are skipped, as
//! [`crate::time_bounds`] skips them: their shapes are not ours to restyle.
//!
//! # How they hold
//!
//! Each rule holds `<name>` (production) and `<name>_in_tests` through [`crate::ratchet::hold_sites`]:
//! the total may only fall, judged against the base tree by the same rules; every crate's count is
//! recorded beside it, so the receipt names where the debt lives and a sweep can start floor-up.
//! A touched file keeps its old sites ([`HeldOnly::SweptByCrate`]) and may add none.

pub mod items;
pub mod state_as_flags;
pub mod string_identities;
pub mod stringly_errors;
pub mod unknown_as_value;
pub mod waits;

use std::sync::OnceLock;

use crate::crate_policy::{self, PolicyError};
use crate::lexer::tokens::{tokenize, Token};
use crate::ratchet::{hold_sites, HeldOnly, Touched};
use crate::{collect_files, scanned_checkout, SourceFile, Violation};

/// Which half of the code a text is: what ships, or what only a test build compiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Half {
    Production,
    Test,
}

/// What kind of target a file builds into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A library's source: what other crates call.
    Library,
    /// A binary's own source (`src/bin/`, `src/main.rs`) or an integration test (`tests/`).
    Program,
}

/// A crate's directory under `core/`: the key a per-crate fact is filed under.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CrateDir(pub String);

/// One half of one file, its line numbers kept (the other half's lines are blank).
pub struct Text {
    /// Repository path, `core/<crate>/src/...`: what a site names and a crate is counted by.
    pub repo: String,
    pub krate: CrateDir,
    pub half: Half,
    pub target: Target,
    pub text: String,
}

/// A text as a rule reads it: its code tokens, and its lines for the site's message and for a
/// reason marker.
pub struct Unit<'a> {
    pub text: &'a Text,
    pub code: Vec<Token<'a>>,
    pub lines: Vec<&'a str>,
}

impl<'a> Unit<'a> {
    pub fn of(text: &'a Text) -> Self {
        Self { text, code: tokenize(&text.text).code, lines: text.text.lines().collect() }
    }

    fn source(&self, line: usize) -> &str {
        self.lines.get(line.wrapping_sub(1)).map_or("", |l| l.trim()) // map_or: a token's line is always inside its text
    }
}

/// One site: where, and which shape (also its message).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Site {
    pub line: usize,
    pub what: &'static str,
}

/// What a shape rule supplies. The corpus, the halves, the counting and the holding are the
/// driver's ([`hold`]).
pub trait Shape {
    fn name(&self) -> &'static str;
    /// What to write instead: printed when the count rises.
    fn advice(&self) -> &'static str;
    /// Whether the rule needs facts from the whole corpus before it judges one text (which fields
    /// of a crate's structs are `Option`s). Most need none, and are spared a second pass.
    const LEARNS: bool = false;
    /// Take what the rule needs from one text, in a pass over every text before any is judged.
    fn learn(&mut self, _unit: &Unit) {}
    fn sites(&self, unit: &Unit) -> Vec<Site>;
}

/// The suffix of a rule's test-code ratchet.
pub const IN_TESTS: &str = "_in_tests";

/// Hold `shape` over every core crate: its production total and its test total, each only falling.
pub fn hold(mut shape: impl Shape) {
    let (name, what_instead) = (shape.name(), shape.advice());
    let (production, tests) = measure(&mut shape);
    // The positive control: a rule over the whole core that finds nothing has a broken predicate
    // or an empty corpus, never a clean core. When a rule truly reaches zero it moves to
    // `ratchet::zero`, which holds a gone shape at zero.
    assert!(!(production.is_empty() && tests.is_empty()), "{name} found no site in any core crate: its predicate or its corpus is broken");
    let advice = |sites: &[Violation]| {
        let shown = sites.iter().take(SITES_SHOWN).map(|v| format!("  {}:{}  {}", v.file, v.line, v.source)).collect::<Vec<_>>();
        format!("{what_instead}\nFirst {} of {} (not necessarily new):\n{}", shown.len(), sites.len(), shown.join("\n"))
    };
    hold_sites(name, &production, Touched::HeldOnly(HeldOnly::SweptByCrate), || advice(&production));
    hold_sites(&format!("{name}{IN_TESTS}"), &tests, Touched::HeldOnly(HeldOnly::SweptByCrate), || advice(&tests));
}

/// How many sites a failure prints: where a sweep starts, not the whole list.
const SITES_SHOWN: usize = 15;

/// Every site of `shape` in the core, as (production, test) violations.
pub fn measure<S: Shape>(shape: &mut S) -> (Vec<Violation>, Vec<Violation>) {
    let texts = match corpus() {
        Ok(texts) => texts,
        Err(e) => panic!("the core's crates cannot be listed, so nothing would be counted: {e}"),
    };
    // One text tokenized at a time: the core is ~8M tokens, and every ratchet test runs at once.
    if S::LEARNS {
        for text in texts {
            shape.learn(&Unit::of(text));
        }
    }
    let (mut production, mut tests) = (Vec::new(), Vec::new());
    for text in texts {
        let unit = Unit::of(text);
        for site in shape.sites(&unit) {
            let violation = Violation {
                rule: shape.name(),
                file: unit.text.repo.clone(),
                line: site.line,
                source: format!("{}: {}", site.what, unit.source(site.line)),
            };
            match unit.text.half {
                Half::Production => production.push(violation),
                Half::Test => tests.push(violation),
            }
        }
    }
    (production, tests)
}

/// Every core crate's code, read once per run (the five rules share it).
fn corpus() -> Result<&'static [Text], PolicyError> {
    static CORPUS: OnceLock<Result<Vec<Text>, PolicyError>> = OnceLock::new();
    CORPUS.get_or_init(read_corpus).as_ref().map(Vec::as_slice).map_err(Clone::clone)
}

fn read_corpus() -> Result<Vec<Text>, PolicyError> {
    let (members, _) = crate_policy::workspace()?;
    let checkout = scanned_checkout();
    let mut out = Vec::new();
    for member in members {
        let krate = CrateDir(member.dir.clone());
        for file in collect_files(&checkout, &format!("core/{}/src", member.dir)) {
            if file.rel.contains("vendored/") {
                continue;
            }
            let target = if file.rel.starts_with("bin/") || file.rel == "main.rs" { Target::Program } else { Target::Library };
            let test = test_half(&file);
            out.push(Text { repo: file.repo.clone(), krate: krate.clone(), half: Half::Test, target, text: test });
            out.push(Text { repo: file.repo, krate: krate.clone(), half: Half::Production, target, text: file.production });
        }
        for file in collect_files(&checkout, &format!("core/{}/tests", member.dir)) {
            out.push(Text { repo: file.repo, krate: krate.clone(), half: Half::Test, target: Target::Program, text: file.raw });
        }
    }
    assert!(!out.is_empty(), "no core crate has any source: the scan would count nothing and pass");
    Ok(out)
}

/// The lines of `file` its production half blanked (test mods, test fixtures, a whole test file),
/// at their own line numbers, every other line blank.
fn test_half(file: &SourceFile) -> String {
    let production: Vec<&str> = file.production.lines().collect();
    file.raw
        .lines()
        .enumerate()
        .map(|(i, raw)| match production.get(i) {
            Some(kept) if *kept == raw => "",
            _ => raw,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: test code counted as production (or dropped), which would charge a
    // test's sleeps to the shipped total, or hide them from the test total.
    #[test]
    fn a_file_splits_into_halves_at_their_own_lines() {
        let raw = "fn live() {}\n#[cfg(test)]\nmod tests {\n    fn t() {}\n}\n";
        let file = SourceFile::fixture("x.rs", &crate::lexer::production_half(raw), raw);
        let test = test_half(&file);
        assert_eq!(test.lines().nth(3), Some("    fn t() {}"), "the test line keeps its number");
        assert_eq!(test.lines().next(), Some(""), "a production line is blank in the test half");
        assert!(!file.production.contains("fn t()"));
    }
}
