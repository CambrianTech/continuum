//! In-process source validation — ONE parse answers every question the edit gate asks.
//!
//! # Why this module exists
//!
//! The edit gate used to shell out to `python3` **three times per edit**:
//! `python3 -m py_compile` (does it parse), `python3 -c <AST analyzer>` (does it call a
//! name it never imports), and a third `python3 -c` (did the edit displace a docstring).
//! A persona doing a 21-act investigation paid dozens of interpreter cold starts, inside
//! the daemon, in a shipped binary.
//!
//! That is banned, and the reason is competitive rather than aesthetic (Joel, 2026-08-05):
//! *"Python will turn this into the shitty competitors we are trying to beat. Same with
//! node. Core is entirely rust."* Every agent framework we intend to beat is a Python or
//! Node stack, and their latency and footprint are not incidental — they are what that
//! substrate costs. Measured here at roughly an order of magnitude. A misfit grid doing
//! frontier work cannot pay a 10× tax on its own edit path.
//! [[no-python-in-rs-files]]
//!
//! **A Rust crate that PARSES Python is not Python.** The ban is on the runtime, not on
//! the grammar. `rustpython-parser` is pure Rust, links into the binary, and answers all
//! three questions from a single in-process parse — no subprocess, no cold start, and no
//! silent hole when the host has no interpreter.
//!
//! # The shape
//!
//! A trait with one implementation per language, resolved by file extension — the
//! polymorphism pattern the codebase mandates, not a `match` on extension inside the edit
//! path. A language that cannot answer a question returns `None` ("no opinion"), which is
//! how a validator stays honest about its own limits instead of guessing.
//!
//! Adding a language is a new file implementing [`SyntaxValidator`] plus one line in
//! [`validator_for`]. Nothing in `file_engine` changes.

pub mod python;
pub mod rust;

use std::path::Path;

/// A parse failure, in the persona's terms: what broke and where.
///
/// Carries the parser's own message because a real `SyntaxError` naming a line is
/// unambiguous where a lexical proxy is not. A bracket-balance heuristic was built for
/// this once, falsified by its own test, and thrown away — the closing paren is still
/// present in the failure shape that actually occurs, so delimiters stay balanced while
/// the file is unparseable. "Valid text at this location" and "valid program" are
/// different predicates and only a parser decides the second.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxFault {
    /// Parser message, already human-readable ("invalid syntax. Got unexpected token 'if'").
    pub message: String,
    /// 1-based line when the parser localizes the fault.
    pub line: Option<usize>,
}

impl std::fmt::Display for SyntaxFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: {}", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

/// What one language's parser can tell the edit gate, in-process.
///
/// Every method that a given language cannot answer returns `None` rather than an empty
/// answer: "I have no opinion" and "I looked and found nothing" are different claims, and
/// collapsing them is how a gate silently stops gating.
pub trait SyntaxValidator: Send + Sync {
    /// Language name for messages ("Python").
    fn language(&self) -> &'static str;

    /// Does this source parse? The gate's primary question.
    fn parse_check(&self, source: &str) -> Result<(), SyntaxFault>;

    /// Names this source CALLS but never binds or imports — a `NameError` waiting to run.
    /// The syntax check cannot see these: the file parses fine and fails at runtime.
    ///
    /// `None` = this language has no such analysis. `Some(vec![])` = analyzed, none found.
    fn unbound_calls(&self, _source: &str) -> Option<Vec<String>> {
        None
    }

    /// Functions whose docstring an edit DISPLACED — code inserted between `def` and the
    /// string, so it is now a bare expression statement and no longer a docstring.
    /// Compares before/after so only newly-broken ones are reported.
    fn displaced_docstrings(&self, _before: &str, _after: &str) -> Option<Vec<String>> {
        None
    }

    /// Code an edit inserted INSIDE a string literal — a write that parses and does nothing.
    ///
    /// The loudest failures are the ones that break the parse; this is the quiet one. On
    /// `pallets__flask-4045` (2026-08-06) the model reasoned the fix correctly and wrote it
    /// into the middle of the class docstring, deleting ~17 lines of API docs to make room:
    ///
    /// ```text
    /// -    :param static_url_path: The url to serve static files from.
    /// +        name = blueprint_name
    /// +        if '.' in name:
    /// +            raise ValueError("Blueprint names cannot contain dots")
    /// ```
    ///
    /// `parse_check` returns Ok — it IS valid Python, the "code" is just text in a
    /// triple-quoted string. `displaced_docstrings` returns nothing — the docstring is
    /// still structurally a docstring. Every gate we had was silent, the tests failed, and
    /// the score was charged to the model's intelligence rather than to the write path.
    ///
    /// A file that will not parse screams. A file that parses and does nothing has to be
    /// TOLD, or the act→observe loop closes on a lie
    /// ([[the-act-observe-circuit-only-closes-if-the-receipt-shows-the-result]]).
    ///
    /// `None` = this language has no such analysis. `Some(vec![])` = analyzed, none found.
    fn inert_insertions(&self, _before: &str, _after: &str) -> Option<Vec<InertInsertion>> {
        None
    }
}

/// Code that landed inside a string literal instead of in the program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InertInsertion {
    /// 1-based line in the NEW content where the inert text starts.
    pub line: usize,
    /// The first inserted line, for the persona to recognize her own edit.
    pub first_line: String,
    /// How many inserted lines landed inside the literal.
    pub lines: usize,
}

impl std::fmt::Display for InertInsertion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "line {}: {} line(s) starting `{}`",
            self.line,
            self.lines,
            self.first_line.trim()
        )
    }
}

/// The validator for this path's language, or `None` when we have no parser for it.
///
/// `None` is the honest default and must stay that way — most extensions have no checker,
/// and silence is what makes the warning mean something when it appears.
pub fn validator_for(path: &Path) -> Option<&'static dyn SyntaxValidator> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("py" | "pyi") => Some(&python::PythonValidator),
        Some("rs") => Some(&rust::RustValidator),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: extension → validator routing, and that an unknown extension
    // yields NO validator rather than a default one. A gate that guesses at a language it
    // cannot parse is worse than no gate.
    #[test]
    fn validator_resolves_by_extension_and_stays_silent_otherwise() {
        assert!(validator_for(Path::new("a/b/c.py")).is_some());
        assert!(validator_for(Path::new("stubs.pyi")).is_some());
        assert!(validator_for(Path::new("main.rs")).is_some(), "this substrate is WRITTEN in Rust — it must gate its own language");
        assert!(validator_for(Path::new("README.md")).is_none());
        assert!(validator_for(Path::new("no_extension")).is_none());
    }

    // what this catches: the fault renders with its line when localized, so a persona
    // reading the refusal knows WHERE, not just THAT.
    #[test]
    fn fault_display_names_the_line_when_known() {
        let with = SyntaxFault {
            message: "bad".into(),
            line: Some(12),
        };
        assert_eq!(with.to_string(), "line 12: bad");
        let without = SyntaxFault {
            message: "bad".into(),
            line: None,
        };
        assert_eq!(without.to_string(), "bad");
    }
}
