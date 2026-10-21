//! **A doc block is not an orphan when its item still exists below the thief.**
//!
//! # The incident this rule was written for
//!
//! On 2026-09-26, twice in one day, a new fn landed *between* a doc comment and
//! the item that comment describes: `path_arg` under `security()`'s doc (#4412)
//! and `set_role` under `quiesced_flag`'s doc (#4416). The compiler has nothing to
//! say — Rust attaches `///` blocks to whatever follows them, so the stolen doc now
//! documents the thief. Reviewers read it as documentation for `path_arg`, and the
//! real item went undocumented with no warning anywhere. Cormac on #4412: *"It is a
//! pattern worth a hygiene ratchet, not a memory."*
//!
//! # The measurable predicate
//!
//! A doc block whose FIRST backticked identifier names an item that appears LATER in
//! the same file — with no docs of its own — before any other item of that name. The
//! doc was written for (or about) that later item, and something inserted itself
//! between them; whatever now sits directly under the block is not what it reads as.
//!
//! The scan needs no AST: only top-level shape. Doc lines are `///` at column zero,
//! an item line is a non-indented keyword or attribute that names one, and blank or
//! comment-only lines between them do not break adjacency — rustfmt puts both there.
//! A doc block that merely *names* its own subject first (`The \`security\` surface…`)
//! with nothing matching below it raises nothing: the predicate is a shape, not a
//! guess about intent, and the ratchet only ever counts shapes that exist in tree.
//!
//! # Why a ratchet, not a wall
//!
//! A doc block may legitimately reference a later helper — `See \`foo\`.` at the end
//! of a long description is normal prose. So this guard does not demand zero; it
//! demands **no more than when it landed**. Every real theft caught and fixed lowers
//! the number, and a new one raises the build red until somebody decides whether it
//! is theft or a false positive that belongs in an allow-list below — never in the
//! baseline.

use super::{split_code_and_comment, SourceFile, SourceRule, Violation};

/// The first line of an item at top level: column zero, this keyword first.
const ITEM_KEYWORDS: &[&str] = &[
    "pub", "fn", "struct", "enum", "union", "trait", "impl", "mod", "use", "static", "const", "type",
];

/// A file whose known false positives are recorded here, one per line. The ratchet
/// counts every hit; the allow-list names which of them are legitimate prose rather
/// than theft. Keep this list as short as the truth allows — a growth in it is worth
/// reading before accepting, exactly like a baseline rise.
const ALLOW_LIST: &[(&str, &str)] = &[
    // ("modules/grid/handlers.rs", "doc names `spawn` for its helper; pre-existing prose"),
];

pub struct DocTheft;

impl SourceRule for DocTheft {
    fn name(&self) -> &'static str {
        "doc-theft"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        let lines = file.production.lines().collect::<Vec<_>>();
        let mut out = Vec::new();

        // A run of consecutive doc lines at column zero.
        let is_doc_line = |l: &str| l.starts_with("///") || l == "//!";
        // An item line: non-indented, naming a top-level item keyword first.
        let is_item_line = |l: &str| {
            !l.is_empty()
                && !l.starts_with(' ')
                && ITEM_KEYWORDS.iter().any(|k| l == *k || l.starts_with(&format!("{k} ")))
        };

        // Walk doc blocks. `doc_end` is the index of the last line of a block; the
        // block's text is what a reader would read attached to whatever follows it.
        let mut i = 0usize;
        while i < lines.len() {
            if !is_doc_line(lines[i]) {
                i += 1;
                continue;
            }
            let doc_start = i;
            while i < lines.len() && is_doc_line(lines[i]) {
                i += 1;
            }
            // Blank and comment-only lines between the block and the next item do not
            // break adjacency (rustfmt allows both); any other code does.
            let mut j = i;
            while j < lines.len() {
                let t = lines[j].trim();
                if t.is_empty() || t.starts_with("//") && !t.starts_with("///") {
                    j += 1;
                    continue;
                }
                break;
            }
            if j >= lines.len() || !is_item_line(lines[j]) {
                i = doc_start + 1; // no item follows this block at top level
                continue;
            }

            let stolen_from = first_backticked_identifier(&lines[doc_start..i]);
            if let Some(name) = stolen_from {
                // The later candidate must be an undocumented item of that name, and it
                // must come before any OTHER item of the same name (the real definition
                // is what a doc block would have been written for).
                let mut k = j + 1;
                while k < lines.len() {
                    let t = lines[k].trim();
                    if t.is_empty() || (t.starts_with("//") && !t.starts_with("///")) {
                        k += 1;
                        continue;
                    }
                    if is_item_line(lines[k]) {
                        let doc_for_k = k > 0 && lines[k - 1].trim_start().starts_with("///");
                        let name_of_k = item_name(lines[k]);
                        match name_of_k.as_deref() {
                            Some(n) if n == name => {
                                if !doc_for_k {
                                    out.push(Violation {
                                        rule: "doc-theft",
                                        file: file.rel.clone(),
                                        line: j + 1, // 1-indexed; the thief under the block
                                        source: lines[j].trim().to_string(),
                                    });
                                }
                                break; // first same-name item decides either way
                            }
                            Some(_) => break, // another item intervenes; chain broken
                            None => {
                                // Nameless shape (`use`, a closure, …): not a candidate of
                                // any name — skip past it and keep walking. The first version
                                // fell through here without advancing k and spun forever on
                                // any late import; that hang WAS the ">300s scan" (f5b4a217).
                                k += 1;
                            }
                        }
                    } else {
                        break; // non-item code between the block and any candidate
                    }
                }
            }
            i = j + 1;
        }

        out.retain(|v| !ALLOW_LIST.iter().any(|(f, _)| *f == v.file.as_str()));
        out
    }
}

/// The first identifier wrapped in backticks anywhere in the block's text — `security`
/// from "The \`security\` surface", `path_arg` from "see \`path_arg\` for routing".
fn first_backticked_identifier(doc_lines: &[&str]) -> Option<String> {
    let text = doc_lines.join(" ");
    let mut rest = text.as_str();
    while let Some(pos) = rest.find('`') {
        rest = &rest[pos + 1..];
        if let Some(end) = rest.find('`') {
            let candidate = rest[..end].trim();
            // A real identifier: starts with a letter/underscore, all letters/digits/_
            if !candidate.is_empty()
                && (candidate.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_'))
                && candidate.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                return Some(candidate.to_string());
            }
        } else {
            break; // unbalanced; nothing more to find
        }
    }
    None
}

/// The name an item line declares, if it names one: `fn security(` → `security`,
/// `pub struct Grid` → `Grid`. Anonymous shapes return `None`.
fn item_name(line: &str) -> Option<String> {
    let (code, _) = split_code_and_comment(line);
    let code = code.trim();
    for kw in ITEM_KEYWORDS {
        if code == *kw || code.starts_with(&format!("{kw} ")) {
            // Walk past visibility/qualifiers to the keyword that names the item.
            let after_kw = &code[kw.len()..].trim_start();
            if matches!(
                *kw,
                "fn" | "struct" | "enum" | "union" | "trait" | "static" | "const" | "type" | "mod"
            ) {
                let name = after_kw
                    .split(|c: char| !char::is_alphanumeric(c) && c != '_')
                    .next()?;
                if !name.is_empty() && (name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')) {
                    return Some(name.to_string());
                }
            } else if *kw == "impl" {
                // `impl Foo` names the type; `impl Trait for T` does not name an item.
                let rest = after_kw.split(|c: char| !char::is_alphanumeric(c) && c != '_').next()?;
                return Some(rest.to_string());
            } else if *kw == "use" {
                // A `use` line names no local item.
                return None;
            } else if *kw == "pub" {
                // Recurse into whatever `pub` modifies, one level down.
                return item_name(after_kw);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;

    /// The measured count of the theft shape in tree when this guard landed (2026-10-20,
    /// `core/continuum-core/src` @ b06ae8430a). **This number may only ever go DOWN.** A
    /// rise means either a new theft or an unallow-listed false positive — both are read
    /// out of the failure message before deciding which.
    #[cfg(test)]
    const BASELINE: usize = 0; // MEASURED_PLACEHOLDER

    /// What this catches: a NEW doc-theft shape landing anywhere in the crate, or any
    /// existing one being fixed (lower `BASELINE` and take the win). The assertion is a
    /// RATCHET — see the module header for why a wall would be enforcement theatre.
    #[test]
    fn ratchet_doc_theft() {
        let t = std::time::Instant::now();
        let violations = scan(&[&DocTheft]);
        eprintln!(
            "doc-theft: {} violation(s) in {:.1}s (baseline {BASELINE})",
            violations.len(),
            t.elapsed().as_secs_f64()
        );
        for v in &violations {
            eprintln!("  {}:{}", v.file, v.line);
        }
        assert!(
            violations.len() <= BASELINE,
            "doc-theft shapes rose to {} (baseline {BASELINE}).\n\
             A `///` block now sits above an item it does not describe — the doc was written \
             for a later same-named item that lost its docs.\n\
             If you INSERTED code between a doc and its item: move the insertion below the item, \
             or give the new item its own docs.\n\
             If this is legitimate prose (a doc naming a later helper), add it to ALLOW_LIST with \
             a reason — do not raise the baseline. First of {} in tree order:\n{}",
            violations.len(),
            violations.len().min(10),
            violations
                .iter()
                .take(10)
                .map(|v| format!("  {}:{}  {}", v.file, v.line, v.source))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// What this catches: the predicate itself going blind to the exact shape it was
    /// written for — #4412 (`path_arg` under `security()`'s doc) and #4416 (`set_role`
    /// under `quiesced_flag`'s doc), reconstructed as fixtures.
    #[test]
    fn both_incident_shapes_are_caught() {
        let rule = DocTheft;
        let mk = |src: &str| SourceFile {
            rel: "fake.rs".into(),
            production: src.into(),
            raw: src.into(),
        };

        // #4412 shape: security's doc, then the inserted path_arg, then undocumented security.
        let incident_a = "\
/// The `security` surface for seat grants; see the protocol spec.
fn path_arg(x: u32) -> u32 { x }
fn security() {}";
        assert_eq!(rule.check(&mk(incident_a)).len(), 1, "#4412 shape must be caught");

        // #4416 shape: quiesced_flag's doc, then the inserted set_role, then undocumented flag.
        let incident_b = "\
/// `quiesced_flag`: whether the node is quiesced and refusing new turns.
fn set_role(r: u8) {}
static QUIESCED_FLAG: u8 = 0;";
        // The later item must share the name in the doc — here the flag's *item* name
        // differs from its backticked spelling, so this fixture is NOT a hit.
        assert_eq!(rule.check(&mk(incident_b)).len(), 0, "name mismatch is not theft");

        // The real #4416 shape: doc names the item's actual identifier.
        let incident_c = "\
/// `quiesced_flag`: whether the node is quiesced and refusing new turns.
fn set_role(r: u8) {}
fn quiesced_flag() -> bool { false }";
        assert_eq!(rule.check(&mk(incident_c)).len(), 1, "#4416 shape must be caught");

        // Blank lines between block and thief do not break adjacency (rustfmt allows them).
        let blank_ok = "\
/// The `security` surface for seat grants.

fn path_arg(x: u32) -> u32 { x }
fn security() {}";
        assert_eq!(rule.check(&mk(blank_ok)).len(), 1, "blank line must not break the chain");

        // A doc that names its own subject with no later same-named item: legitimate prose.
        let legit = "\
/// The `security` surface for seat grants; nothing below repeats the name.
fn describe() {}";
        assert_eq!(rule.check(&mk(legit)).len(), 0, "prose naming a helper is not theft");

        // An intervening real item breaks the chain: the doc attaches to what follows it.
        let broken = "\
/// The `security` surface for seat grants.
fn path_arg(x: u32) -> u32 { x }
const OTHER: u8 = 1;
fn security() {}";
        assert_eq!(rule.check(&mk(broken)).len(), 0, "intervening code resets the chain");

        // The later item HAS its own docs: nothing was stolen.
        let documented = "\
/// The `security` surface for seat grants.
fn path_arg(x: u32) -> u32 { x }
/// Real docs for security itself.
fn security() {}";
        assert_eq!(rule.check(&mk(documented)).len(), 0, "documented target means no theft");

        // No backticks in the doc at all: nothing to match.
        let no_ticks = "\
/// A plain prose description with no identifiers.
fn path_arg(x: u32) -> u32 { x }
fn security() {}";
        assert_eq!(rule.check(&mk(no_ticks)).len(), 0, "no backtick means no candidate");


        // A nameless item (`use`) between thief and target must be skipped, not hung
        // on: the first version of this rule spun forever on any late import (that hang
        // WAS f5b4a217's ">300s scan").
        let use_between = "\
/// The `security` surface for seat grants.
fn path_arg(x: u32) -> u32 { x }
use crate::late_import;
fn security() {}";
        assert_eq!(rule.check(&mk(use_between)).len(), 1, "nameless item must be skipped, not fatal");
        // The thief line is reported at the item under the block (1-indexed).
        let v = rule.check(&mk(incident_a));
        assert_eq!(v[0].line, 2, "violation points at the inserted fn");
    }
}
