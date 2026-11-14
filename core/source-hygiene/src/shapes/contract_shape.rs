//! **The contract says what a call takes: grouped, borrowed, its mutability and its absence in the
//! signature** (CONTINUUM-ENGINE.md, RTOS rule 11).
//!
//! Joel, 2026-11-14: "group params as part of structs when they belong together. Pass by reference
//! and assign mutability and optional at contract." The sites:
//! - a library fn with five or more parameters (`self` not counted): values that travel together
//!   are a struct;
//! - a library fn with three or more consecutive parameters of one primitive type
//!   (`fn plan(ctx: u32, floor: u32, wish: u32)`): swappable at every call site, and the compiler
//!   cannot tell;
//! - `.clone()` in argument position (`f(x.clone())`), anywhere: a copy made so a call can read it,
//!   where the contract should borrow. Lexical: a callee that keeps the value is counted too, and a
//!   capitalized constructor (`Some(x.clone())`, `Arc::new` is lowercase and counted) is not;
//! - a `pub fn` parameter taking an owned `String` or `Vec<_>`: an owned buffer a reader would
//!   borrow as `&str` or `&[T]` (a builder that keeps it is counted too; the type cannot say which).
//!
//! `&mut` parameters never mutated are out of reach of a token rule and not counted.

use super::items::top_level;
use super::{Shape, Site, Target, Unit};
use crate::lexer::tokens::{matching_close, matching_open, Kind, Token};

pub struct ContractShape;

const MANY_PARAMS: &str = "five or more parameters: values that travel together are a struct";
const SWAPPABLE: &str = "three or more consecutive parameters of one primitive type: swappable";
const CLONED_ARGUMENT: &str = "a clone passed into a call: the contract should borrow";
const OWNED_BUFFER: &str = "a pub fn taking an owned String or Vec: a reader borrows";

/// Five parameters: Joel's "group params as part of structs when they belong together", at the
/// count where a call site stops being readable without names.
const MANY: usize = 5;
/// Three of one type in a row: two can be a pair (`width, height`), three are a list nobody checks.
const SWAPPABLE_RUN: usize = 3;

/// Types whose values are interchangeable at a call site.
const PRIMITIVES: &[&str] = &["u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize", "f32", "f64", "bool", "char"];

impl Shape for ContractShape {
    fn name(&self) -> &'static str {
        "contract_shape"
    }

    fn advice(&self) -> &'static str {
        "Put what travels together in a struct (`PlanInput { ctx, floor, wish }`), give each \
         primitive a newtype (`Tokens`, `Slot`) so a swap does not compile, borrow what the callee \
         only reads (`&str`, `&[T]`, `&T`), and say mutability (`&mut`) and absence (`Option<T>`) in \
         the signature."
    }

    fn sites(&self, unit: &Unit) -> Vec<Site> {
        let code = &unit.code;
        let library = unit.text.target == Target::Library;
        let mut out = Vec::new();
        for at in 0..code.len() {
            if code[at].is_ident("fn") && library {
                out.extend(signature_sites(code, at));
            } else if cloned_argument(code, at) {
                out.push(Site { line: code[at].line, what: CLONED_ARGUMENT });
            }
        }
        out
    }
}

/// The sites of the fn declared at `at`: its parameter count, a swappable run, owned buffers.
fn signature_sites(code: &[Token], at: usize) -> Vec<Site> {
    let Some(name) = code.get(at + 1).filter(|t| t.kind == Kind::Ident) else { return Vec::new() };
    let mut open = at + 2;
    if code.get(open).is_some_and(|t| t.is('<')) {
        open = super::items::angle_close(code, open) + 1;
    }
    if !code.get(open).is_some_and(|t| t.is('(')) {
        return Vec::new();
    }
    let close = matching_close(code, open);
    let params: Vec<_> = top_level(code, open + 1..close).into_iter().filter(|p| !is_self(code, p)).map(|p| param_type(code, &p)).collect();
    let mut out = Vec::new();
    let site = |what| Site { line: name.line, what };
    if params.len() >= MANY {
        out.push(site(MANY_PARAMS));
    }
    let primitive = |ty: &Option<std::ops::Range<usize>>| ty.as_ref().and_then(|r| code.get(r.start..r.end)).filter(|t| t.len() == 1 && PRIMITIVES.contains(&t[0].text)).map(|t| t[0].text);
    let longest_run = params.windows(SWAPPABLE_RUN).any(|w| {
        let first = primitive(&w[0]);
        first.is_some() && w.iter().all(|p| primitive(p) == first)
    });
    if longest_run {
        out.push(site(SWAPPABLE));
    }
    if is_pub(code, at) {
        for ty in params.iter().flatten() {
            let owned = code.get(ty.start).is_some_and(|t| t.is_ident("String") || t.is_ident("Vec"));
            if owned {
                out.push(Site { line: code[ty.start].line, what: OWNED_BUFFER });
            }
        }
    }
    out
}

/// Whether a parameter is the receiver: `self`, `&self`, `&mut self`, `mut self`, `self: Arc<Self>`.
fn is_self(code: &[Token], param: &std::ops::Range<usize>) -> bool {
    code.get(param.start..param.end).unwrap_or_default().iter().take(4).any(|t| t.is_ident("self")) // unwrap_or_default: a reversed range reads as empty
}

/// A parameter's type: the tokens after its pattern's top-level `:` (never a path's `::`).
fn param_type(code: &[Token], param: &std::ops::Range<usize>) -> Option<std::ops::Range<usize>> {
    let mut at = param.start;
    while at < param.end {
        let t = &code[at];
        if t.is('(') || t.is('[') || t.is('{') {
            at = matching_close(code, at) + 1;
            continue;
        }
        let path = code.get(at + 1).is_some_and(|n| n.is(':')) || (at > 0 && code[at - 1].is(':'));
        if t.is(':') && !path {
            return Some(at + 1..param.end);
        }
        at += 1;
    }
    None
}

/// Whether the fn at `at` is `pub`: past `async`, `const`, `unsafe` and `extern "abi"` back to a
/// `pub` or `pub(..)`.
fn is_pub(code: &[Token], at: usize) -> bool {
    let mut i = at;
    while i > 0 {
        let prev = &code[i - 1];
        if prev.is_ident("async") || prev.is_ident("const") || prev.is_ident("unsafe") || prev.is_ident("extern") || prev.kind == Kind::Literal {
            i -= 1;
        } else if prev.is(')') {
            return matching_open(code, i - 1).and_then(|o| o.checked_sub(1)).is_some_and(|p| code[p].is_ident("pub"));
        } else {
            return prev.is_ident("pub");
        }
    }
    false
}

/// Whether `.clone()` starts at `at` and is a whole argument of a call: followed by `,` or `)`, in
/// a parenthesized list that a lowercase callee or a macro opens.
fn cloned_argument(code: &[Token], at: usize) -> bool {
    let clone = code[at].is('.')
        && code.get(at + 1).is_some_and(|t| t.is_ident("clone"))
        && code.get(at + 2).is_some_and(|t| t.is('('))
        && code.get(at + 3).is_some_and(|t| t.is(')'))
        && code.get(at + 4).is_some_and(|t| t.is(',') || t.is(')'));
    if !clone {
        return false;
    }
    let Some(open) = enclosing_open(code, at) else { return false };
    let Some(callee) = open.checked_sub(1).map(|c| &code[c]) else { return false };
    code[open].is('(')
        && (callee.is('!') || callee.is('>') || (callee.kind == Kind::Ident && callee.text.starts_with(|c: char| c.is_ascii_lowercase() || c == '_') && !KEYWORDS.contains(&callee.text)))
}

/// Words that open a parenthesized expression without calling anything.
const KEYWORDS: &[&str] = &["if", "while", "match", "return", "in", "let", "for", "move", "else", "break"];

/// The opener of the innermost group holding `at`.
fn enclosing_open(code: &[Token], at: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = at;
    while i > 0 {
        i -= 1;
        let t = &code[i];
        if t.is(')') || t.is(']') || t.is('}') {
            depth += 1;
        } else if t.is('(') || t.is('[') || t.is('{') {
            if depth == 0 {
                return Some(i);
            }
            depth -= 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokens::tokenize;
    use crate::shapes::{CrateDir, Half, Text};

    fn found_in(target: Target, src: &str) -> Vec<&'static str> {
        let text = Text { repo: "core/x/src/a.rs".into(), krate: CrateDir("x".into()), half: Half::Production, target, text: src.into() };
        let unit = Unit { text: &text, code: tokenize(&text.text).code, lines: text.text.lines().collect() };
        ContractShape.sites(&unit).into_iter().map(|s| s.what).collect()
    }

    fn found(src: &str) -> Vec<&'static str> {
        found_in(Target::Library, src)
    }

    // what this catches: a new loose contract in any core crate.
    #[test]
    fn contract_shape_only_falls() {
        crate::shapes::hold(ContractShape);
    }

    // what this catches: the predicate missing a shape Joel named.
    #[test]
    fn loose_contracts_are_sites() {
        assert_eq!(found("fn f(&self, a: A, b: B, c: C, d: D, e: E) {}"), [MANY_PARAMS]);
        assert_eq!(found("fn plan(name: &str, ctx: u32, floor: u32, wish: u32) {}"), [SWAPPABLE]);
        assert_eq!(found("fn g() { send(msg.clone(), 3); }"), [CLONED_ARGUMENT]);
        assert_eq!(found("fn g() { let a = Arc::new(cfg.clone()); }"), [CLONED_ARGUMENT]);
        assert_eq!(found("pub fn set(&mut self, name: String, rows: Vec<Row>) {}"), [OWNED_BUFFER, OWNED_BUFFER]);
        assert_eq!(found("pub(crate) async fn load(path: String) {}"), [OWNED_BUFFER]);
    }

    // what this catches: grouped, borrowed or typed contracts counted, or a program's signature.
    #[test]
    fn grouped_borrowed_and_typed_contracts_are_not_sites() {
        assert!(found("fn f(&mut self, input: PlanInput, a: u32, b: u32) {}").is_empty());
        assert!(found("fn f(ctx: Tokens, floor: Tokens, wish: Tokens) {}").is_empty(), "newtypes");
        assert!(found("fn f(a: u32, b: f64, c: u32) {}").is_empty(), "not one type in a row");
        assert!(found("pub fn read(name: &str, rows: &[Row]) {}").is_empty());
        assert!(found("fn private(name: String) {}").is_empty(), "not pub");
        assert!(found("fn g() { let x = Some(v.clone()); let y = v.clone(); }").is_empty(), "a constructor; a binding");
        assert!(found("fn g() { x.clone().into_iter(); }").is_empty(), "a receiver");
        assert!(found_in(Target::Program, "fn main(a: u8, b: u8, c: u8, d: u8, e: u8) {}").is_empty());
    }
}
