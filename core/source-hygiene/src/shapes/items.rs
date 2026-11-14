//! Items read from tokens: structs and enums with their fields, a type's generic arguments, and
//! the call chain an expression ends in. The shape rules share these so two rules never read the
//! same struct differently.

use std::ops::Range;

use crate::lexer::tokens::{matching_close, matching_open, Kind, Token};

/// A struct or an enum: its name, the line of its name, and its body's tokens (between the braces
/// or parentheses, exclusive). A unit struct has an empty body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item<'a> {
    pub name: &'a str,
    pub line: usize,
    pub body: Range<usize>,
}

/// One field: its name (`None` in a tuple), its line and its type's tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field<'a> {
    pub name: Option<&'a str>,
    pub line: usize,
    pub ty: Range<usize>,
}

/// One enum variant: its name, its line and its fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant<'a> {
    pub name: &'a str,
    pub line: usize,
    pub fields: Vec<Field<'a>>,
}

/// Every `keyword` item (`struct` or `enum`) in `code`.
pub fn items<'a>(code: &[Token<'a>], keyword: &str) -> Vec<Item<'a>> {
    let mut out = Vec::new();
    for at in 0..code.len() {
        if !code[at].is_ident(keyword) {
            continue;
        }
        let Some(name) = code.get(at + 1).filter(|t| t.kind == Kind::Ident) else { continue };
        let mut i = at + 2;
        if code.get(i).is_some_and(|t| t.is('<')) {
            i = angle_close(code, i) + 1;
        }
        // Past a `where` clause to the body; a `;` first is a unit struct.
        while let Some(t) = code.get(i) {
            if t.is('{') || t.is('(') || t.is(';') {
                break;
            }
            i += 1;
        }
        let body = match code.get(i) {
            Some(t) if t.is('{') || t.is('(') => i + 1..matching_close(code, i),
            _ => i..i,
        };
        out.push(Item { name: name.text, line: name.line, body });
    }
    out
}

/// The fields of a struct body or a variant's body.
pub fn fields<'a>(code: &[Token<'a>], body: Range<usize>) -> Vec<Field<'a>> {
    top_level(code, body)
        .into_iter()
        .filter_map(|segment| {
            let start = past_attributes_and_visibility(code, segment.clone());
            let first = code.get(start).filter(|_| start < segment.end)?;
            let named = first.kind == Kind::Ident
                && code.get(start + 1).is_some_and(|t| t.is(':'))
                && !code.get(start + 2).is_some_and(|t| t.is(':'));
            Some(if named {
                Field { name: Some(first.text), line: first.line, ty: start + 2..segment.end }
            } else {
                Field { name: None, line: first.line, ty: start..segment.end }
            })
        })
        .collect()
}

/// The variants of an enum body.
pub fn variants<'a>(code: &[Token<'a>], body: Range<usize>) -> Vec<Variant<'a>> {
    top_level(code, body)
        .into_iter()
        .filter_map(|segment| {
            let start = past_attributes_and_visibility(code, segment.clone());
            let name = code.get(start).filter(|t| start < segment.end && t.kind == Kind::Ident)?;
            let fields = match code.get(start + 1) {
                Some(t) if start + 1 < segment.end && (t.is('(') || t.is('{')) => fields(code, start + 2..matching_close(code, start + 1)),
                _ => Vec::new(),
            };
            Some(Variant { name: name.text, line: name.line, fields })
        })
        .collect()
}

/// `range` split at its top-level commas: commas inside a group or a type's `<...>` are not
/// separators. Empty segments (a trailing comma) are dropped.
pub fn top_level(code: &[Token], range: Range<usize>) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let (mut start, mut angle, mut at) = (range.start, 0usize, range.start);
    while at < range.end {
        let t = &code[at];
        if t.is('(') || t.is('[') || t.is('{') {
            at = matching_close(code, at) + 1;
            continue;
        }
        if t.is('<') {
            angle += 1;
        } else if t.is('>') && !is_arrow(code, at) {
            angle = angle.saturating_sub(1);
        } else if t.is(',') && angle == 0 {
            if at > start {
                out.push(start..at);
            }
            start = at + 1;
        }
        at += 1;
    }
    if range.end > start {
        out.push(start..range.end);
    }
    out
}

/// The top-level generic arguments of the `<` at `open`.
pub fn generic_args(code: &[Token], open: usize) -> Vec<Range<usize>> {
    top_level(code, open + 1..angle_close(code, open))
}

/// The index of the `>` that closes the `<` at `open`, groups and `->` skipped; the last token
/// when it never closes.
pub fn angle_close(code: &[Token], open: usize) -> usize {
    let mut depth = 0usize;
    let mut at = open;
    while at < code.len() {
        let t = &code[at];
        if t.is('(') || t.is('[') || t.is('{') {
            at = matching_close(code, at) + 1;
            continue;
        }
        if t.is('<') {
            depth += 1;
        } else if t.is('>') && !is_arrow(code, at) {
            depth -= 1;
            if depth == 0 {
                return at;
            }
        }
        at += 1;
    }
    code.len().saturating_sub(1)
}

/// Whether the `>` at `at` is the second half of `->` or `=>`.
fn is_arrow(code: &[Token], at: usize) -> bool {
    at > 0 && (code[at - 1].is('-') || code[at - 1].is('=')) && code[at - 1].line == code[at].line
}

/// Whether the type `ty` is exactly a path ending in one of `names` (`String`,
/// `std::string::String`, `bool`).
pub fn type_is(code: &[Token], ty: Range<usize>, names: &[&str]) -> bool {
    let Some(tokens) = code.get(ty) else { return false };
    let Some(last) = tokens.last() else { return false };
    last.kind == Kind::Ident && names.contains(&last.text) && tokens.iter().all(|t| t.kind == Kind::Ident || t.is(':'))
}

/// The first index of `range` past its outer attributes (`#[...]`) and its visibility
/// (`pub`, `pub(crate)`, `pub(in path)`).
fn past_attributes_and_visibility(code: &[Token], range: Range<usize>) -> usize {
    let mut at = range.start;
    while at < range.end {
        if code[at].is('#') && code.get(at + 1).is_some_and(|t| t.is('[')) {
            at = matching_close(code, at + 1) + 1;
        } else if code[at].is_ident("pub") {
            at += 1;
            if code.get(at).is_some_and(|t| t.is('(')) {
                at = matching_close(code, at) + 1;
            }
        } else {
            break;
        }
    }
    at
}

/// Methods that pass their receiver's value through unchanged in meaning (`x.free_bytes.copied()`
/// is still the free bytes), so a rule naming "what is this a reading of" looks past them.
pub const PASS_THROUGH: &[&str] = &["copied", "cloned", "clone", "as_ref", "as_deref", "as_mut", "flatten", "take", "get", "load"];

/// The name an expression ending at `end` (inclusive) reads: the field or method it ends in,
/// looking back past [`PASS_THROUGH`] calls (`self.stats.free_bytes().copied()` reads
/// `free_bytes`). `None` when it ends in anything else (a literal, an operator).
pub fn read_name<'a>(code: &[Token<'a>], end: usize) -> Option<&'a str> {
    let mut at = end;
    loop {
        let t = code.get(at)?;
        if t.kind == Kind::Ident {
            return Some(t.text);
        }
        if !t.is(')') {
            return None;
        }
        let open = matching_open(code, at)?;
        let method = code.get(open.checked_sub(1)?)?;
        let through = method.kind == Kind::Ident
            && PASS_THROUGH.contains(&method.text)
            && open >= 3
            && code[open - 2].is('.');
        if through {
            at = open - 3;
        } else {
            return (method.kind == Kind::Ident).then_some(method.text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokens::tokenize;

    // what this catches: a field split at a comma inside its type (`HashMap<String, u8>`) or an
    // attribute or `pub(crate)` read as the field's name, which would miscount every struct rule.
    #[test]
    fn fields_split_at_top_level_commas_past_attributes_and_visibility() {
        let src = "struct S<T> where T: Copy { #[serde(default)] pub(crate) a: HashMap<String, u8>, b: bool, f: fn(u8) -> bool }";
        let code = tokenize(src).code;
        let item = &items(&code, "struct")[0];
        let found: Vec<_> = fields(&code, item.body.clone()).into_iter().map(|f| f.name).collect();
        assert_eq!(found, [Some("a"), Some("b"), Some("f")]);
        let code = tokenize("enum E { A(String), B { why: String }, C = 3 }").code;
        let item = &items(&code, "enum")[0];
        let v = variants(&code, item.body.clone());
        assert_eq!(v.iter().map(|v| (v.name, v.fields.len())).collect::<Vec<_>>(), [("A", 1), ("B", 1), ("C", 0)]);
    }

    // what this catches: a read past `.copied()` stopping at the pass-through, so a measurement
    // behind it is never named.
    #[test]
    fn a_read_names_the_value_behind_pass_through_calls() {
        let code = tokenize("x.stats.free_bytes().copied()").code;
        assert_eq!(read_name(&code, code.len() - 1), Some("free_bytes"));
        let code = tokenize("self.available_mib").code;
        assert_eq!(read_name(&code, code.len() - 1), Some("available_mib"));
    }
}
