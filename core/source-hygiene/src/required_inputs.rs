//! **A required input is enforced by its type, never complained about at run time.**
//!
//! Joel, 2026-11-02: "Optional parameters are insane with messages or errors. Have the courage to
//! enforce it at the contract and make it easy and automatic"; "We should never have the ability
//! to complain about a parameter we require"; "For everything ... We use rust because it's
//! compile time safe."
//!
//! A required parameter, context or dependency is a required field, a constructor argument, a
//! non-empty type or a typestate builder: the value cannot exist without it. The pattern this
//! counts is the opposite, an `Option` (or a late-bound slot, or an untyped map) plus a run-time
//! message that it was missing: `ok_or("missing ...")`, `Err("X is required")`, `expect("not
//! installed")`. Measured on canary 0dff31c53: 116 production sites across 15 crates.
//!
//! The count only falls, and a file a change touches leaves with none: its owner replaces the
//! Option with the type that cannot be missing. Never by making the complaint typed or louder.

use super::{SourceFile, SourceRule, Violation};

/// Where a complaint is raised or returned.
const COMPLAINT_SITES: &[&str] = &[
    "ok_or", "Err(", "bail!", "anyhow!", ".expect(", "panic!", "format!(", "probe!(",
];

/// What a complaint about a required input says.
const MISSING_PHRASES: &[&str] = &[
    " missing",
    "missing ",
    "is required",
    "required but",
    "not installed",
    "not set",
    "must be provided",
    "must be set",
    "not provided",
];

pub struct RequiredInputComplaints;

impl SourceRule for RequiredInputComplaints {
    fn name(&self) -> &'static str {
        "required_input_complaints"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        // Build scripts read cargo's environment, an input whose type is not ours.
        if file.rel.ends_with("build.rs") {
            return Vec::new();
        }
        file.production
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim_start().starts_with("//"))
            .filter(|(_, l)| COMPLAINT_SITES.iter().any(|site| l.contains(site)))
            .filter(|(_, l)| {
                literals(l).any(|lit| {
                    let lit = lit.to_ascii_lowercase();
                    MISSING_PHRASES.iter().any(|p| lit.contains(p))
                })
            })
            .map(|(i, l)| Violation {
                rule: self.name(),
                file: file.repo.clone(),
                line: i + 1,
                source: l.trim().to_string(),
            })
            .collect()
    }
}

/// The string literals on a line (between unescaped double quotes).
fn literals(line: &str) -> impl Iterator<Item = &str> {
    line.split('"').skip(1).step_by(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{scan, SourceFile};

    /// What this catches: a NEW run-time complaint about a required input. If this fails because
    /// you added one, make the input required by its type (a required field, a constructor
    /// argument, a non-empty type, a typestate builder) instead of an Option plus a message. A
    /// file you touch leaves with none.
    #[test]
    fn required_input_complaints_only_fall() {
        let violations = scan(&[&RequiredInputComplaints]);
        let count = violations.len();
                let advice = || {
            format!(
                "required_input_complaints rose to {count}: a required input is enforced by its \
                 type, never an Option plus a run-time 'missing'/'required'/'not installed' \
                 message (Joel, 2026-11-02). First {} in tree order:\n{}",
                violations.len().min(10),
                violations
                    .iter()
                    .take(10)
                    .map(|v| format!("  {}:{}  {}", v.file, v.line, v.source))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };
        crate::ratchet::hold_sites("required_input_complaints", &violations, advice);
        crate::ratchet::touched_clean("required_input_complaints", &violations, || {
            "a file this change touches still complains at run time about a required input: \
             make the input required by its type"
                .to_string()
        });
    }

    // what this catches: the predicate drifting either way. It flags a complaint about a
    // required input at a complaint site, and spares a comment, a non-complaint string and a
    // build script.
    #[test]
    fn the_predicate_flags_complaints_about_required_inputs() {
        let rule = RequiredInputComplaints;
        let flagged = |rel: &str, src: &str| rule.check(&SourceFile::for_test(rel, src)).len();
        assert_eq!(
            flagged("a.rs", r#"let id = p.id.ok_or("id is required")?;"#),
            1
        );
        assert_eq!(
            flagged(
                "a.rs",
                r#"self.slot.cloned().ok_or_else(|| "executor not installed".to_string())"#
            ),
            1
        );
        assert_eq!(
            flagged("a.rs", r#"return Err(format!("Missing {key}"));"#),
            1
        );
        assert_eq!(
            flagged("a.rs", r#"// ok_or("x is required") in a comment"#),
            0,
            "a comment"
        );
        assert_eq!(
            flagged("a.rs", r#"let s = "the room has no doctrine";"#),
            0,
            "not a complaint site"
        );
        assert_eq!(
            flagged("a.rs", r#"Err(format!("unknown verb {verb}"))"#),
            0,
            "not about a missing input"
        );
        assert_eq!(
            flagged(
                "x/build.rs",
                r#"env::var("OUT_DIR").expect("OUT_DIR must be set")"#
            ),
            0,
            "a build script"
        );
    }
}
