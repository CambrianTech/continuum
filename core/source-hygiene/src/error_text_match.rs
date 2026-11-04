//! **A decision is made on a typed value, never on an error's text.**
//!
//! Joel, 2026-11-04: "always give me enums and ints ... they were grepping out error texts.
//! String matching is insane" and "It's slow and cpu eater". A match on text breaks silently when
//! the wording changes, and it formats the error into a new string and scans it on every call,
//! where a variant is one compare. The text is a rendering of the variant, for people and logs.
//!
//! This rule counts production lines that decide on text: `.to_string().contains(..)`, and
//! `.contains(..)`, `.starts_with(..)` or `.ends_with(..)` called on an error-like value
//! (`err`, `e`, `error`, `msg`, `message`, `stderr`, `stdout`, `reason`). The count may only fall,
//! and a file a change touches leaves with none: give the error a variant and match on it.

use super::{lexer::split_code_and_comment, SourceFile, SourceRule, Violation};

/// The receivers whose text is an error or a process's report, not data.
const ERROR_LIKE: &[&str] = &["err", "e", "error", "msg", "message", "stderr", "stdout", "reason"];

/// The text predicates a decision is made with.
const PREDICATES: &[&str] = &[".contains(", ".starts_with(", ".ends_with("];

pub struct ErrorTextMatch;

/// The identifier that ends `code` (letters, digits, `_`), if any.
fn trailing_ident(code: &str) -> &str {
    let start = code
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '_')
        .last()
        .map_or(code.len(), |(i, _)| i); // map_or: no identifier before the call is the empty receiver
    &code[start..]
}

/// Whether `code` decides on an error's text.
pub fn matches_error_text(code: &str) -> bool {
    PREDICATES.iter().any(|predicate| {
        code.match_indices(predicate).any(|(at, _)| {
            let before = &code[..at];
            before.ends_with(".to_string()") || ERROR_LIKE.contains(&trailing_ident(before))
        })
    })
}

impl SourceRule for ErrorTextMatch {
    fn name(&self) -> &'static str {
        "error-text-match"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        file.production_lines()
            .filter_map(|(line_no, line)| {
                let (code, _) = split_code_and_comment(line);
                matches_error_text(code).then(|| Violation {
                    rule: "error-text-match",
                    file: file.repo.clone(),
                    line: line_no,
                    source: code.trim().to_string(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;

    // what this catches: a decision on an error's text landing anywhere, or staying in a file a
    // change touched. The count may only fall.
    #[test]
    fn error_text_matches_fall_and_touched_files_have_none() {
        let violations = scan(&[&ErrorTextMatch]);
        let advice = || {
            "Give the error a variant (an enum, or the library's own kind) and match on it; its text \
             is only its Display. A match on text breaks when the wording changes and scans a \
             freshly formatted string on every call."
                .to_string()
        };
        crate::ratchet::hold_sites("error_text_matches", &violations, advice);
        crate::ratchet::touched_clean("error_text_matches", &violations, advice);
    }

    // what this catches: the predicate missing the shapes it exists for, or flagging data.
    #[test]
    fn the_predicate_flags_error_text_and_leaves_data_alone() {
        assert!(matches_error_text(r#"if e.to_string().contains("no such table") {"#));
        assert!(matches_error_text(r#"if err.contains("refused") {"#));
        assert!(matches_error_text(r#"stderr.starts_with("fatal")"#));
        assert!(!matches_error_text(r#"if names.contains(&name) {"#), "a collection is data");
        assert!(!matches_error_text(r#"if path.starts_with(root) {"#), "a path is data");
    }
}
