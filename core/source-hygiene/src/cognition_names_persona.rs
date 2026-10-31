//! The mind split's ratchet (plan phase 2, crates by the placement map): production code in
//! `cognition/` that names a `persona` module. Cognition is cut into its own crate below
//! persona (`continuum-cognition`), so every such line is a coupling the cut cannot pass
//! through: a fact persona owns that belongs below it (moved down), or a verb cognition calls
//! that persona owns (inverted through `cognition::extensions`, which persona fills at boot).
//!
//! What counts: one violation per production line in `cognition/` that names `persona` after
//! `crate::` (a path or a grouped import). Tests are not counted: a test that drives persona
//! moves with the persona cut, and a moved test file (`#![cfg(test)]`) is whole test code.
//! Per line, so a pure move cannot change the count; only removing a coupling lowers it.
//! Measured on canary a51bc55bd (2026-10-24): 108 lines in 34 files. The cut lands at 0.

use super::{lexer::split_code_and_comment, SourceFile, SourceRule, Violation};

pub struct CognitionNamesPersona;

impl SourceRule for CognitionNamesPersona {
    fn name(&self) -> &'static str {
        "cognition-names-persona"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        if !file.rel.starts_with("cognition/") || file.is_moved_test() {
            return Vec::new();
        }
        file.production_lines()
            .filter(|(_, line)| names_persona(split_code_and_comment(line).0))
            .map(|(line_no, line)| Violation {
                rule: "cognition-names-persona",
                file: file.rel.clone(),
                line: line_no,
                source: line.trim().to_string(),
            })
            .collect()
    }
}

/// Whether one line of code names `crate::persona` (a path or a group head).
fn names_persona(code: &str) -> bool {
    let mut rest = code;
    while let Some(at) = rest.find("crate::") {
        rest = &rest[at + "crate::".len()..];
        if crate::layer_discipline::named_modules(rest).contains(&"persona") {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;

    // what this catches: a cognition line reaching up into persona, by path or in a group,
    // counted once per line; a comment, a test and a file outside cognition are not counted.
    #[test]
    fn a_cognition_line_naming_persona_is_one_violation() {
        let f = SourceFile::for_test(
            "cognition/x.rs",
            "use crate::persona::engram::Engram;\nfn f() { crate::persona::a::b(); crate::persona::c::d(); }\nuse crate::{cognition::y, persona::z};\n// crate::persona::in_prose\nfn g() { crate::cognition::ok(); }\n",
        );
        assert_eq!(CognitionNamesPersona.check(&f).len(), 3);
        let outside = SourceFile::for_test("persona/x.rs", "use crate::persona::engram::Engram;\n");
        assert!(CognitionNamesPersona.check(&outside).is_empty());
        let moved = SourceFile::for_test("cognition/x/tests.rs", "#![cfg(test)]\nuse crate::persona::engram::Engram;\n");
        assert!(CognitionNamesPersona.check(&moved).is_empty());
        let near = SourceFile::for_test("cognition/x.rs", "use crate::cognition::persona_tools::x;\nfn f() { super::super::persona_workspace::y(); }\n");
        assert!(CognitionNamesPersona.check(&near).is_empty(), "persona_tools and persona_workspace are cognition's own");
    }

    // what this catches: cognition coupling back up into persona after the split started.
    // The cognition crate cut needs this at 0; it may only fall.
    #[test]
    fn cognition_names_persona_never_rises() {
        let violations = scan(&[&CognitionNamesPersona]);
        let mut files: Vec<&str> = violations.iter().map(|v| v.file.as_str()).collect();
        files.dedup();
        eprintln!("cognition names persona: {} lines in {} files", violations.len(), files.len());
        crate::ratchet::hold("cognition_names_persona", violations.len(), || {
            let newest: Vec<String> = violations.iter().take(8).map(|v| format!("  {}:{}: {}", v.file, v.line, v.source)).collect();
            format!(
                "cognition now names persona on more lines. Move the fact down into cognition, or \
                 invert the verb through cognition::extensions (persona installs it at boot):\n{}",
                newest.join("\n")
            )
        });
    }
}
