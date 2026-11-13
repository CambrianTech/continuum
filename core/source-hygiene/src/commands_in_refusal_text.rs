//! **A refusal names a command by a typed reference, never by a string in its text.**
//!
//! A refusal she can act on carries its next moves as calls to commands (`sdk_codegen::NextCall`,
//! built from a `VerbRef` to the command's own type): rendered under the name her tool list offers
//! it, and admitted into her next act's tools by the mind she acts through. A command written into
//! the sentence is neither: Aiko on the M5, 2026-11-13 22:40-23:26Z, review card 149f1391, had 8
//! `work/submit` calls refused with "record it with work/note ... close the card with work/state
//! closed" while her tool list offered those verbs as `ledger` and `update_task`, and she repeated
//! the refused call.
//!
//! This rule counts the declared command names (`work/note`, from a `const NAME` or an
//! `action_command!` `name:`) written inside a refusal's text: the arguments of
//! `CommandError::{Invalid, NotFound, Denied, Internal}(..)`, a `#[error(..)]` attribute (a refusal
//! cause's sentence), and a `ClientError::Refused { .. }`. A prose string only: a bare name
//! (`"work/note"`) is an identifier, not a sentence. The count may only fall, and a file a change
//! touches leaves with none: build the refusal with `Refusal::new(cause, NextMoves)` and its calls
//! with `NextCall::new(VerbRef, &params, why)`, or say the move without naming the command.

use std::collections::BTreeSet;

use super::lexer::tokens::{matching_close, tokenize, Kind, Token};
use super::{CrateRule, SourceFile, Violation};

pub struct CommandsInRefusalText;

const RULE: &str = "commands-in-refusal-text";

/// Every namespaced command name the tree declares (`work/note`), read from its declaration.
pub fn declared_commands(corpus: &str) -> BTreeSet<String> {
    corpus
        .lines()
        .filter_map(|line| {
            let code = line.trim();
            let code = code.strip_prefix("pub ").unwrap_or(code); // unwrap_or: a private const reads the same
            let value = ["const NAME: &'static str = ", "const NAME: &str = ", "name: "].iter().find_map(|form| code.strip_prefix(form))?;
            let (name, _) = value.strip_prefix('"')?.split_once('"')?;
            is_command_path(name).then(|| name.to_owned())
        })
        .collect()
}

/// A command path: two or more segments of lowercase letters, digits, `-` and `_`.
fn is_command_path(name: &str) -> bool {
    name.contains('/')
        && name.split('/').all(|segment| {
            !segment.is_empty() && segment.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        })
}

/// The token ranges holding a refusal's text: `CommandError::<Variant>( .. )`, `#[error( .. )]`
/// and `ClientError::Refused { .. }`, each as (first token inside, its closing delimiter).
fn refusal_spans(code: &[Token]) -> Vec<(usize, usize)> {
    const VARIANTS: [&str; 4] = ["Invalid", "NotFound", "Denied", "Internal"];
    let path = |at: usize, owner: &str| {
        at >= 3 && code[at - 3].is_ident(owner) && code[at - 2].is(':') && code[at - 1].is(':')
    };
    let mut spans = Vec::new();
    for at in 0..code.len().saturating_sub(1) {
        let open = at + 1;
        let opens = match code[at].text {
            variant if VARIANTS.contains(&variant) => path(at, "CommandError") && code[open].is('('),
            "Refused" => path(at, "ClientError") && code[open].is('{'),
            "error" => at >= 2 && code[at - 1].is('[') && code[at - 2].is('#') && code[open].is('('),
            _ => false,
        };
        if code[at].kind == Kind::Ident && opens {
            spans.push((open + 1, matching_close(code, open)));
        }
    }
    spans
}

/// The declared commands a prose literal names, each with the line it is on (`line` is the
/// literal's first line).
fn named_in(literal: &str, line: usize, commands: &BTreeSet<String>) -> Vec<(usize, String)> {
    if !literal.contains(char::is_whitespace) {
        return Vec::new();
    }
    let bounded = |c: Option<char>| !c.is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '/' | '-'));
    let mut found = Vec::new();
    for command in commands {
        for (at, _) in literal.match_indices(command.as_str()) {
            if bounded(literal[..at].chars().next_back()) && bounded(literal[at + command.len()..].chars().next()) {
                found.push((line + literal[..at].matches('\n').count(), command.clone()));
            }
        }
    }
    found
}

/// The commands `file`'s refusal text names, against the declared `commands`.
pub fn sites_in(file: &SourceFile, commands: &BTreeSet<String>) -> Vec<Violation> {
    let tokens = tokenize(&file.production);
    let code = &tokens.code;
    let mut sites = Vec::new();
    for (first, close) in refusal_spans(code) {
        for token in code.get(first..close).into_iter().flatten().filter(|t| t.kind == Kind::Literal) {
            for (line, command) in named_in(token.text, token.line, commands) {
                sites.push(Violation { rule: RULE, file: file.repo.clone(), line, source: format!("names `{command}` in a refusal's text") });
            }
        }
    }
    sites
}

impl CrateRule for CommandsInRefusalText {
    fn name(&self) -> &'static str {
        RULE
    }

    fn check_crate(&self, files: &[SourceFile], corpus: &str) -> Vec<Violation> {
        let commands = declared_commands(corpus);
        files.iter().flat_map(|file| sites_in(file, &commands)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan_crate;

    // what this catches: a refusal's text naming a command anywhere, or staying in a file a change
    // touched. The count may only fall.
    #[test]
    fn commands_named_in_refusal_text_fall_and_touched_files_have_none() {
        let violations = scan_crate(&[&CommandsInRefusalText]);
        let advice = || {
            "Build the refusal with its next moves typed: `Refusal::new(cause, NextMoves)`, each call \
             `NextCall::new(VerbRef::in_hands(), &params, why)`. The call renders under the name her \
             tool list offers and joins her next act's tools; a command named in the sentence does \
             neither (Aiko on the M5, 2026-11-13: `work/note` written in, offered to her as `ledger`). \
             Or say the move without naming the command."
                .to_string()
        };
        crate::ratchet::hold_sites("commands_in_refusal_text", &violations, crate::ratchet::Touched::EndsClean, advice);
    }

    // what this catches: the rule missing the shapes it exists for (a command in a refusal's
    // prose, across a line break, in a cause's `#[error]`), or flagging a bare name, a longer
    // command that shares a prefix, or text outside a refusal.
    #[test]
    fn the_rule_flags_commands_in_refusal_prose_and_leaves_names_alone() {
        let commands = declared_commands(
            "    const NAME: &'static str = \"work/note\";\n    name: \"work/state\",\n    const NAME: &str = \"ping\";\n",
        );
        assert_eq!(commands.iter().map(String::as_str).collect::<Vec<_>>(), ["work/note", "work/state"], "namespaced names only");
        let file = SourceFile::for_test(
            "x.rs",
            "fn f() -> CommandError {\n\
             let _ = CommandError::Invalid(format!(\"nothing to submit: record it with work/note \\\n\
             and close it with work/state closed\"));\n\
             let _ = CommandError::Invalid(\"work/note\".into());\n\
             let _ = CommandError::NotFound(format!(\"see work/notes or work/note/x first\"));\n\
             let _ = format!(\"record it with work/note\");\n\
             }\n\
             #[derive(thiserror::Error)]\n\
             enum E { #[error(\"close it with work/state\")] A }\n",
        );
        let sites: Vec<(usize, String)> = sites_in(&file, &commands).into_iter().map(|v| (v.line, v.source)).collect();
        assert_eq!(
            sites,
            [
                (2, "names `work/note` in a refusal's text".to_owned()),
                (3, "names `work/state` in a refusal's text".to_owned()),
                (9, "names `work/state` in a refusal's text".to_owned()),
            ]
        );
    }
}
