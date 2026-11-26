//! **A hot path allocates, copies and encodes nothing it does not have to** (Joel, 2026-11-25: "Criticize
//! every clone vs reference, every string encoding, every transfer, every cpu iteration ... Never ever
//! use json or strings for high bandwidth operations or barriers").
//!
//! A file declares itself a hot path with a line `//! hot-path: <why>` (the lane decides at every
//! step; the Core submits every step). In such a file, in production code, each of these is a site
//! unless its own line says why it is not on the hot path, with `// cold: <why>` (setup, a refusal, a
//! path that runs once):
//! - a copy: `.clone()`, `.to_vec()`, `.to_owned()`, `.to_string()`, `.collect()` into an owned buffer;
//! - text: `format!`, `String::`, `serde_json`;
//! - an allocation: `vec![..]`, `Vec::new` and `Vec::with_capacity`, `Box::new`.
//! `Arc::clone(&x)` (a handle, not a copy) is not a site. Zero: a hot file leaves with none.

use super::{Shape, Site, Target, Unit};

pub struct HotPathCopies;

/// The header a hot file carries.
pub const MARKER: &str = "//! hot-path:";
/// What a line says when its site is not on the hot path.
pub const COLD: &str = "// cold:";

const COPY: &str = "a copy on a hot path: borrow, slice, index or share a handle";
const TEXT: &str = "text on a hot path: typed binary, ids, fixed byte buffers";
const ALLOCATION: &str = "an allocation on a hot path: a buffer owned once and reused";

impl Shape for HotPathCopies {
    fn name(&self) -> &'static str {
        "hot_path_copies"
    }

    fn advice(&self) -> &'static str {
        "Read where the bytes lie (a borrow, a slice, an index, an Arc handle); keep binary as binary \
         (ids, typed records, fixed byte tails); own each buffer once and reuse it. A site that truly \
         runs off the hot path says why on its own line with `// cold: <why>`."
    }

    fn sites(&self, unit: &Unit) -> Vec<Site> {
        if unit.text.target != Target::Library || !unit.lines.iter().any(|line| line.trim_start().starts_with(MARKER)) {
            return Vec::new();
        }
        let code = &unit.code;
        let cold = |line: usize| unit.lines.get(line.wrapping_sub(1)).is_some_and(|text| text.contains(COLD));
        let mut out = Vec::new();
        for at in 0..code.len() {
            let what = site(code, at);
            if let Some(what) = what.filter(|_| !cold(code[at].line)) {
                out.push(Site { line: code[at].line, what });
            }
        }
        out
    }
}

/// The site that starts at `at`, if one does.
fn site(code: &[crate::lexer::tokens::Token], at: usize) -> Option<&'static str> {
    let token = &code[at];
    let next = |n: usize| code.get(at + n);
    let method = |name: &str| token.is('.') && next(1).is_some_and(|t| t.is_ident(name)) && next(2).is_some_and(|t| t.is('('));
    let path = |ty: &str, item: &str| token.is_ident(ty) && next(1).is_some_and(|t| t.is(':')) && next(2).is_some_and(|t| t.is(':')) && next(3).is_some_and(|t| t.is_ident(item));
    let mac = |name: &str| token.is_ident(name) && next(1).is_some_and(|t| t.is('!'));
    if ["clone", "to_vec", "to_owned", "to_string", "collect"].iter().any(|name| method(name)) {
        return Some(COPY);
    }
    if mac("format") || token.is_ident("serde_json") || (token.is_ident("String") && next(1).is_some_and(|t| t.is(':'))) {
        return Some(TEXT);
    }
    if mac("vec") || path("Vec", "new") || path("Vec", "with_capacity") || path("Box", "new") {
        return Some(ALLOCATION);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::{measure, CrateDir, Half, Text};

    fn found(src: &str) -> Vec<&'static str> {
        let text = Text { repo: "core/x/src/a.rs".into(), krate: CrateDir("x".into()), half: Half::Production, target: Target::Library, text: src.into() };
        HotPathCopies.sites(&Unit::of(&text)).into_iter().map(|site| site.what).collect()
    }

    // what this catches: a copy, text or an allocation in a file that declared itself a hot path; and
    // the rule reaching files that never declared one, or refusing a site its line marks cold.
    #[test]
    fn a_hot_file_has_no_unjustified_copy_text_or_allocation() {
        let hot = "//! hot-path: every step\nfn f(a: &[i32]) {\n    let b = a.to_vec();\n    let s = format!(\"{a:?}\");\n    let v = Vec::new();\n    let h = std::sync::Arc::clone(&x);\n    let w = Vec::with_capacity(8); // cold: once at start\n}\n";
        assert_eq!(found(hot), [COPY, TEXT, ALLOCATION], "to_vec, format!, Vec::new; Arc::clone is a handle; the cold line is justified");
        assert!(found("fn f(a: &[i32]) { let b = a.to_vec(); }").is_empty(), "a file that is not hot is not judged");
    }

    // what this catches: any unjustified copy, text or allocation in a file marked hot.
    #[test]
    fn hot_path_copies_is_zero() {
        let mut rule = HotPathCopies;
        let (production, _) = measure(&mut rule);
        crate::ratchet::zero(rule.name(), &production, || rule.advice().to_owned());
    }
}
