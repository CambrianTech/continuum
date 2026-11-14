//! **A caller can match on why it failed** (CONTINUUM-ENGINE.md, RTOS rule 11).
//!
//! A failure flattened to text can only be logged or grepped (and grepping error text is its own
//! ratchet, [`crate::error_text_match`]). The review of 2026-11-14 found typed errors
//! (`VectorIndexError`, `UnitVectorError`) turned back into strings at the very seam that should
//! carry them, and three outcomes of a checkpoint save collapsed into one `Save(String)`. The
//! sites, in library code (a binary's own `main` may print and exit):
//! - `Result<_, String>` and `Result<_, ()>`;
//! - `.map_err(|e| format!(..))` and `.map_err(|e| <..>.to_string())` / `.to_owned()`;
//! - a variant of an error enum (named `..Error` or `..Failure`) holding a bare `String`.

use super::items::{generic_args, items, type_is, variants};
use super::{Shape, Site, Target, Unit};
use crate::lexer::tokens::{matching_close, Token};

pub struct StringlyErrors;

const STRING_ERROR: &str = "a failure typed as a String";
const UNIT_ERROR: &str = "a failure typed as `()`: no reason at all";
const FLATTENED: &str = "a typed failure flattened to text";
const STRING_VARIANT: &str = "an error variant holding a bare String";

impl Shape for StringlyErrors {
    fn name(&self) -> &'static str {
        "stringly_errors"
    }

    fn advice(&self) -> &'static str {
        "Give the failure a type the caller can match: an error enum whose variants carry the typed \
         cause (`#[from] io::Error`, `VectorIndexError`) and the facts (`slot`, `kind`), with \
         `Display` for the words. Wrap a typed error, never format it into a String."
    }

    fn sites(&self, unit: &Unit) -> Vec<Site> {
        if unit.text.target != Target::Library {
            return Vec::new();
        }
        let code = &unit.code;
        let mut out = Vec::new();
        for at in 0..code.len() {
            if let Some(what) = result_type(code, at).or_else(|| flattening(code, at)) {
                out.push(Site { line: code[at].line, what });
            }
        }
        for item in items(code, "enum").into_iter().filter(|e| e.name.ends_with("Error") || e.name.ends_with("Failure")) {
            for variant in variants(code, item.body) {
                if variant.fields.iter().any(|f| type_is(code, f.ty.clone(), &["String"])) {
                    out.push(Site { line: variant.line, what: STRING_VARIANT });
                }
            }
        }
        out.sort_by_key(|s| s.line);
        out
    }
}

/// `Result<T, String>` or `Result<T, ()>` starting at `at`.
fn result_type(code: &[Token], at: usize) -> Option<&'static str> {
    if !code[at].is_ident("Result") || !code.get(at + 1)?.is('<') {
        return None;
    }
    let args = generic_args(code, at + 1);
    let error = args.get(1).filter(|_| args.len() == 2)?;
    if type_is(code, error.clone(), &["String"]) {
        return Some(STRING_ERROR);
    }
    let unit = error.len() == 2 && code[error.start].is('(') && code[error.start + 1].is(')');
    unit.then_some(UNIT_ERROR)
}

/// `.map_err(|..| format!(..))` or `.map_err(|..| <..>.to_string())` starting at the `.` at `at`.
fn flattening(code: &[Token], at: usize) -> Option<&'static str> {
    if !code[at].is('.') || !code.get(at + 1)?.is_ident("map_err") || !code.get(at + 2)?.is('(') || !code.get(at + 3)?.is('|') {
        return None;
    }
    let close = matching_close(code, at + 2);
    let params_end = (at + 4..close).find(|&i| code[i].is('|'))?;
    let body = &code[params_end + 1..close];
    let formats = body.first()?.is_ident("format") && body.get(1)?.is('!');
    let n = body.len();
    let to_text = n >= 4 && body[n - 4].is('.') && (body[n - 3].is_ident("to_string") || body[n - 3].is_ident("to_owned")) && body[n - 2].is('(') && body[n - 1].is(')');
    (formats || to_text).then_some(FLATTENED)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokens::tokenize;
    use crate::shapes::{CrateDir, Half, Text};

    fn found_in(target: Target, src: &str) -> Vec<&'static str> {
        let text = Text { repo: "core/x/src/a.rs".into(), krate: CrateDir("x".into()), half: Half::Production, target, text: src.into() };
        let unit = Unit { text: &text, code: tokenize(&text.text).code, lines: text.text.lines().collect() };
        StringlyErrors.sites(&unit).into_iter().map(|s| s.what).collect()
    }

    fn found(src: &str) -> Vec<&'static str> {
        found_in(Target::Library, src)
    }

    // what this catches: a new failure typed as text in any core library.
    #[test]
    fn stringly_errors_only_fall() {
        crate::shapes::hold(StringlyErrors);
    }

    // what this catches: the predicate missing a shape the 2026-11-14 review found.
    #[test]
    fn every_stringly_error_shape_is_a_site() {
        assert_eq!(found("fn load() -> Result<Arc<CpuMatrix>, String> { todo!() }"), [STRING_ERROR]);
        assert_eq!(found("fn f() -> std::result::Result<Vec<u8>, std::string::String>;"), [STRING_ERROR]);
        assert_eq!(found("fn f() -> Result<(), ()>;"), [UNIT_ERROR]);
        assert_eq!(found("let m = build(x).map_err(|e| format!(\"index: {e}\"))?;"), [FLATTENED]);
        assert_eq!(found("let m = build(x).map_err(|e| e.to_string())?;"), [FLATTENED]);
        assert_eq!(found("enum CheckpointFailure { Save(String), Load { why: String }, Gone }"), [STRING_VARIANT, STRING_VARIANT]);
    }

    // what this catches: a typed failure, a binary's own exit path, or a non-error enum holding text
    // counted as stringly.
    #[test]
    fn typed_failures_programs_and_plain_enums_are_not_sites() {
        assert!(found("fn f() -> Result<String, StoreError>;").is_empty(), "a String success");
        assert!(found("fn f() -> io::Result<String>;").is_empty());
        assert!(found("let m = build(x).map_err(StoreError::Index)?;").is_empty());
        assert!(found("let m = build(x).map_err(|e| StoreError::Index { slot, e })?;").is_empty());
        assert!(found("enum Message { Text(String) }").is_empty(), "not an error");
        assert!(found("enum StoreError { Missing(RecordId), Io(#[from] io::Error) }").is_empty());
        assert!(found_in(Target::Program, "fn main() -> Result<(), String> { Ok(()) }").is_empty(), "a binary's exit");
    }
}
