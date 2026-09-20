//! Rust validation via `syn` — the same in-process contract the Python validator honors.
//!
//! Reuses the `syn` 2.x already in the dependency graph rather than adding a second major
//! version; compiling two copies of a parser to answer one question is the kind of cost
//! that adds up into the thing we are trying to beat.
//!
//! # Why a Rust file needs this gate at all
//!
//! `cargo check` catches a broken `.rs` file eventually — but "eventually" is the problem.
//! The edit gate's job is to refuse the write, so the file on disk is never in the broken
//! state and the persona's NEXT read shows working code. Without it she edits, reads back
//! her own damage, and reasons from it; the compiler's verdict arrives an act or two later
//! when the trail is cold. This is a substrate whose personas mostly write Rust, and it
//! had a Python-only syntax gate.
//!
//! # What it can and cannot answer
//!
//! `parse_check` only. The other two questions are Python-shaped and honestly answered
//! `None` here:
//!
//! - **unbound calls** — Rust has no module-level `NameError`. An undefined path is a
//!   compile error, which `cargo check` reports precisely and with better diagnostics than
//!   anything reimplemented here. Duplicating it would be a second, worse source of truth.
//! - **displaced docstrings** — `///` is an attribute on the item that follows it. Code
//!   inserted between a doc comment and its item does not silently demote the doc the way
//!   it does in Python; it moves or fails to compile. There is no silent-damage case.
//!
//! Returning `None` for those is the trait working as designed: a validator states the
//! limits of its own analysis instead of guessing.

use super::{SyntaxFault, SyntaxValidator};

pub struct RustValidator;

impl SyntaxValidator for RustValidator {
    fn language(&self) -> &'static str {
        "Rust"
    }

    fn parse_check(&self, source: &str) -> Result<(), SyntaxFault> {
        match syn::parse_file(source) {
            Ok(_) => Ok(()),
            Err(err) => {
                // `span-locations` gives real line numbers on a parse error. Without it the
                // span collapses to line 0, which would be a fabricated location — report
                // None rather than point her at a line that is not the fault.
                let line = err.span().start().line;
                Err(SyntaxFault {
                    message: err.to_string(),
                    line: (line > 0).then_some(line),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const V: RustValidator = RustValidator;

    // what this catches: the gate now covers the language this substrate is WRITTEN in.
    // Before this, a persona could write a `.rs` file that did not parse and the edit gate
    // had "no opinion" — she would read her own broken code back and reason from it.
    #[test]
    fn a_rust_file_that_does_not_parse_is_rejected() {
        let valid = "fn add(a: u32, b: u32) -> u32 {\n    a + b\n}\n";
        assert!(V.parse_check(valid).is_ok());

        // The Rust analogue of the flask break: a statement wedged into a signature.
        let broken = "fn add(a: u32, if x { } b: u32) -> u32 {\n    a + b\n}\n";
        let fault = V
            .parse_check(broken)
            .expect_err("a statement inside a parameter list is not Rust");
        assert!(
            fault.line.is_some(),
            "a refusal must localize the fault: {fault}"
        );
    }

    // what this catches: an unclosed brace — the shape a truncated write leaves behind.
    // This is exactly what the solo SWE-bench run produced (a file ending mid-body with no
    // trailing newline), and the gate must refuse it rather than write it.
    #[test]
    fn a_truncated_file_is_rejected() {
        let truncated = "fn main() {\n    let x = 1;\n";
        assert!(V.parse_check(truncated).is_err());
    }

    // what this catches: the honest limits. Rust answers `None` to the two Python-shaped
    // questions rather than an empty Vec — "no opinion" must never read as "analyzed,
    // nothing found", which is how a gate silently stops gating.
    #[test]
    fn rust_states_its_limits_rather_than_guessing() {
        let src = "fn f() { g(); }\n";
        assert!(
            V.unbound_calls(src).is_none(),
            "an undefined path is a COMPILE error; cargo check owns that verdict"
        );
        assert!(
            V.displaced_docstrings(src, src).is_none(),
            "/// is an attribute on the following item — there is no silent-demotion case"
        );
    }
}
