//! **Her text is rendered from typed data, never authored prose.**
//!
//! Joel, 2026-11-06: one renderer, so an inconsistency is a compile error; "ratchet literals to
//! 0". A sentence written into a string literal in a module whose text reaches a mind is a claim
//! nobody types: it drifts from the code it describes (a verb renamed, a param dropped, a limit
//! changed), and she acts on the stale claim. Text she reads is rendered from the typed thing it
//! is about (a `CommandSpec`'s schema, a `RefusalCause`'s variant, a `Source`'s items), so the
//! words change when the thing does.
//!
//! **What reaches a mind ([`REACHES_A_MIND`]).** Derived from where her prompt is built on canary
//! (2026-11-07): the files that construct her prompt messages (`ChatMessage::text`/`system`
//! in `llm_deliberation_faculty/prompt/{messages,template,view}.rs`), the system-prompt blocks
//! that assembly folds in (`cognition/deliberation_prompt.rs`, `continuum-prompt`'s
//! `prompt_assembly` and `prompt_prefix`, the persona's `build_persona_system_prompt`), the tool
//! surface she is offered (`cognition/tool_surface*`, `llm_deliberation_faculty/tool_surface.rs`)
//! and the kernel's renderers of a command to her (`sdk_codegen`'s `ai_help` manual, `call_form`
//! and `refusal`, which every refused call and `commands/help` render through). A module that
//! starts rendering text to her joins this list in the same change.
//!
//! **Prose, not a label.** A literal is prose when it holds at least [`PROSE_MIN_WORDS`] words
//! (whitespace-separated runs holding a letter, `{..}` placeholders removed). A label names one
//! thing: a role, a section header, a field key ("tool result", "next move"), which is at most
//! three words; a sentence needs a subject, a verb and what it acts on, plus a word that relates
//! them. Literals in an attribute, a log, a panic or an assertion never reach her and are skipped.
//!
//! The count, per crate, may only fall, and a file a change touches leaves with none.

use super::lexer::tokens::{tokenize, Kind, Token};
use super::{SourceFile, SourceRule, Violation};

/// Repository-path prefixes of the modules whose text reaches a resident (see the module doc).
pub const REACHES_A_MIND: &[&str] = &[
    "core/continuum-cognition/src/cognition/llm_deliberation_faculty/prompt/",
    "core/continuum-cognition/src/cognition/llm_deliberation_faculty/tool_surface.rs",
    "core/continuum-cognition/src/cognition/tool_surface",
    "core/continuum-cognition/src/cognition/deliberation_prompt.rs",
    "core/continuum-prompt/src/prompt_assembly",
    "core/continuum-prompt/src/prompt_prefix.rs",
    "core/continuum-mind/src/persona/supervisor/build_persona_system_prompt.rs",
    "core/continuum-kernel/src/sdk_codegen/ai_help.rs",
    "core/continuum-kernel/src/sdk_codegen/call_form.rs",
    "core/continuum-kernel/src/sdk_codegen/refusal.rs",
];

/// The fewest words a literal holds to be prose rather than a label (see the module doc).
pub const PROSE_MIN_WORDS: usize = 4;

/// Macros whose text goes to a log, a panic or a test, never to her.
const NOT_HERS: &[&str] = &[
    "info", "warn", "debug", "trace", "error", "eprintln", "println", "eprint", "print", "panic", "unreachable", "todo",
    "unimplemented", "assert", "assert_eq", "assert_ne", "debug_assert", "debug_assert_eq", "debug_assert_ne", "log", "probe",
];

pub struct ProseLiterals;

/// The words of a string literal's text: whitespace-separated runs holding a letter, with
/// `{..}` placeholders and escapes (`\n`) removed.
pub fn words(literal: &str) -> usize {
    let mut text = String::with_capacity(literal.len());
    let mut depth = 0usize;
    let mut chars = literal.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
            }
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            '\\' => {
                chars.next();
                text.push(' ');
            }
            _ if depth == 0 => text.push(c),
            _ => {}
        }
    }
    text.split_whitespace().filter(|w| w.chars().any(char::is_alphabetic)).count()
}

/// Is the literal at `at` inside an attribute, a log/panic/assert macro, or an `.expect(..)`?
fn not_hers(code: &[Token], at: usize) -> bool {
    let mut depth = 0usize;
    for j in (0..at).rev() {
        let t = &code[j];
        if t.is(')') || t.is(']') || t.is('}') {
            depth += 1;
        } else if t.is('(') || t.is('[') || t.is('{') {
            if depth > 0 {
                depth -= 1;
                continue;
            }
            if t.is('{') {
                return false; // the enclosing block: the statement starts here
            }
            let before = |k: usize| j.checked_sub(k).map(|i| &code[i]);
            let attribute = t.is('[') && (before(1).is_some_and(|b| b.is('#')) || (before(1).is_some_and(|b| b.is('!')) && before(2).is_some_and(|b| b.is('#'))));
            let log_macro = before(1).is_some_and(|b| b.is('!'))
                && before(2).is_some_and(|b| b.kind == Kind::Ident && (NOT_HERS.contains(&b.text) || b.text.starts_with("clog_")));
            let expect = t.is('(') && before(1).is_some_and(|b| b.is_ident("expect"));
            if attribute || log_macro || expect {
                return true;
            }
        } else if t.is(';') && depth == 0 {
            return false;
        }
    }
    false
}

/// The prose literals of `production`: each one's line and word count.
pub fn prose_literals(production: &str) -> Vec<(usize, usize)> {
    let code = tokenize(production).code;
    (0..code.len())
        .filter(|&at| code[at].kind == Kind::Literal && code[at].text.contains('"'))
        .filter_map(|at| {
            let n = words(code[at].text);
            (n >= PROSE_MIN_WORDS && !not_hers(&code, at)).then_some((code[at].line, n))
        })
        .collect()
}

impl SourceRule for ProseLiterals {
    fn name(&self) -> &'static str {
        "prose_literals"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        if !REACHES_A_MIND.iter().any(|m| file.repo.starts_with(m)) {
            return Vec::new();
        }
        let lines: Vec<&str> = file.production.lines().collect();
        prose_literals(&file.production)
            .into_iter()
            .map(|(line, n)| Violation {
                rule: self.name(),
                file: file.repo.clone(),
                line,
                source: format!("{n} words: {}", lines.get(line - 1).map_or("", |l| l.trim())), // map_or: a token's line is always in the text it came from
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;

    fn advice(violations: &[Violation]) -> impl Fn() -> String + '_ {
        move || {
            format!(
                "Her text is rendered from the typed thing it is about (a CommandSpec's schema, a \
                 RefusalCause's variant, a Source's items), never written into a literal.\nFirst {} of {}:\n{}",
                violations.len().min(20),
                violations.len(),
                violations.iter().take(20).map(|v| format!("  {}:{}  {}", v.file, v.line, v.source)).collect::<Vec<_>>().join("\n")
            )
        }
    }

    // what this catches: a new sentence authored into text that reaches a mind, or one left in a
    // touched file.
    #[test]
    fn prose_literals_fall_and_touched_files_have_none() {
        let violations = scan(&[&ProseLiterals]);
        crate::ratchet::hold_sites("prose_literals", &violations, advice(&violations));
        crate::ratchet::touched_clean("prose_literals", &violations, advice(&violations));
    }

    // what this catches: the negative control. Added prose in a module that reaches her is
    // counted, so the ratchet rises and fails; a label, a log, an attribute, placeholders and a
    // module she never reads are not.
    #[test]
    fn added_prose_is_counted_and_labels_and_logs_are_not() {
        let refusal = "core/continuum-kernel/src/sdk_codegen/refusal.rs";
        let count = |repo: &str, src: &str| ProseLiterals.check(&SourceFile { repo: repo.to_owned(), ..SourceFile::for_test("x.rs", src) }).len();
        assert_eq!(count(refusal, r#"let s = format!("You cannot call {verb} here, try another tool");"#), 1);
        assert_eq!(count(refusal, "const HINT: &str = \"Read the file before you edit it.\";"), 1);
        assert_eq!(count(refusal, r#"let h = "next move";"#), 0, "a label");
        assert_eq!(count(refusal, r#"let s = format!("{a} {b} {c} {d}");"#), 0, "placeholders are not words");
        assert_eq!(count(refusal, "tracing::warn!(\n    \"the refusal could not be rendered at all\",\n);"), 0, "a log");
        assert_eq!(count(refusal, r#"#[error("the call failed for a reason here")]"#), 0, "an attribute");
        assert_eq!(count(refusal, r#"let x = y.expect("the schema always has a properties map");"#), 0);
        assert_eq!(count("core/continuum-engine/src/x.rs", r#"let s = "You cannot call this tool here";"#), 0, "not a module she reads");
        assert_eq!(count(refusal, "// \"a comment with many words in it\""), 0);
    }

    // what this catches: the word count drifting on escapes and placeholders.
    #[test]
    fn words_skip_placeholders_and_escapes() {
        assert_eq!(words(r#""call {name} with {args:?}\n""#), 2);
        assert_eq!(words(r#""{{literal}} braces kept""#), 3);
        assert_eq!(words(r##"r#"one two three four"#"##), 4);
    }
}
