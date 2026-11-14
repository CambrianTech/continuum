//! **An identity is a type, never a string** (CONTINUUM-ENGINE.md, RTOS rule 11).
//!
//! A field read by name out of a JSON value re-parses a tool's input on every use and fails at run
//! time on a typo the compiler could have caught (`act.call.input.get("file_path")` beside a typed
//! `EditInput`); a map keyed by `String` lets any spelling be a key (`fields.remove("embedding")`
//! beside the `EMBEDDING_FIELD` constant; review of 2026-11-14). The sites:
//! - `.get("..")` with a string literal, in a text that uses `serde_json` or a `Value`, outside
//!   wire code (see [`is_wire`]: the code that decodes the wire is where names are read once);
//! - a struct field, in library code, typed `HashMap<String, _>` or `BTreeMap<String, _>`
//!   (directly or inside a wrapper, `Arc<RwLock<HashMap<String, _>>>`).

use super::items::{fields, generic_args, items, type_is};
use super::{Shape, Site, Target, Unit};
use crate::lexer::tokens::{Kind, Token};

pub struct StringIdentities;

const JSON_BY_NAME: &str = "a field read by name from a JSON value";
const STRING_KEYED: &str = "a map keyed by String in a struct";

/// Directory or file-stem segments that mark wire code: the decoders of what crosses a process or
/// network boundary, where a field is read by its name once and turned into a type.
const WIRE_SEGMENTS: &[&str] = &["wire", "ipc", "protocol"];

impl Shape for StringIdentities {
    fn name(&self) -> &'static str {
        "string_identities"
    }

    fn advice(&self) -> &'static str {
        "Decode once at the seam into a typed struct (`#[derive(Deserialize)] struct EditInput { \
         file_path: PathBuf }`) and key maps by an id type or an enum (`HashMap<RecordId, _>`, \
         `Field::Embedding`), never by a spelling."
    }

    fn sites(&self, unit: &Unit) -> Vec<Site> {
        let code = &unit.code;
        let mut out = Vec::new();
        let json = code.iter().any(|t| t.is_ident("serde_json") || t.is_ident("Value"));
        if json && !is_wire(&unit.text.repo) {
            for at in 0..code.len() {
                if json_field_by_name(code, at) {
                    out.push(Site { line: code[at].line, what: JSON_BY_NAME });
                }
            }
        }
        if unit.text.target == Target::Library {
            for item in items(code, "struct") {
                for field in fields(code, item.body) {
                    if string_keyed_map(code, field.ty.clone()) {
                        out.push(Site { line: field.line, what: STRING_KEYED });
                    }
                }
            }
        }
        out.sort_by_key(|s| s.line);
        out
    }
}

/// Whether `repo` is wire code: a `*-protocol` crate, or a path segment or file stem named for the
/// wire.
pub fn is_wire(repo: &str) -> bool {
    let krate = repo.split('/').nth(1).unwrap_or(""); // unwrap_or: a repo path is `core/<crate>/...`
    krate.ends_with("-protocol")
        || repo.split('/').any(|segment| {
            let stem = segment.strip_suffix(".rs").unwrap_or(segment); // unwrap_or: a directory has no extension
            WIRE_SEGMENTS.iter().any(|w| stem == *w || stem.ends_with(&format!("_{w}")) || stem.starts_with(&format!("{w}_")))
        })
}

/// `.get("..")` starting at the `.` at `at`.
fn json_field_by_name(code: &[Token], at: usize) -> bool {
    code[at].is('.')
        && code.get(at + 1).is_some_and(|t| t.is_ident("get"))
        && code.get(at + 2).is_some_and(|t| t.is('('))
        && code.get(at + 3).is_some_and(|t| t.kind == Kind::Literal && (t.text.starts_with('"') || t.text.starts_with("r#") || t.text.starts_with("r\"")))
        && code.get(at + 4).is_some_and(|t| t.is(')'))
}

/// Whether the type `ty` holds a `HashMap` or `BTreeMap` keyed by `String`, at any depth.
fn string_keyed_map(code: &[Token], mut ty: std::ops::Range<usize>) -> bool {
    ty.any(|at| {
        (code[at].is_ident("HashMap") || code[at].is_ident("BTreeMap"))
            && code.get(at + 1).is_some_and(|t| t.is('<'))
            && generic_args(code, at + 1).first().is_some_and(|key| type_is(code, key.clone(), &["String"]))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokens::tokenize;
    use crate::shapes::{CrateDir, Half, Text};

    fn found_at(repo: &str, target: Target, src: &str) -> Vec<&'static str> {
        let text = Text { repo: repo.into(), krate: CrateDir("x".into()), half: Half::Production, target, text: src.into() };
        let unit = Unit { text: &text, code: tokenize(&text.text).code, lines: text.text.lines().collect() };
        StringIdentities.sites(&unit).into_iter().map(|s| s.what).collect()
    }

    fn found(src: &str) -> Vec<&'static str> {
        found_at("core/x/src/views.rs", Target::Library, src)
    }

    // what this catches: a new identity spelled as a string in any core crate.
    #[test]
    fn string_identities_only_fall() {
        crate::shapes::hold(StringIdentities);
    }

    // what this catches: the predicate missing a shape the 2026-11-14 review found.
    #[test]
    fn json_reads_by_name_and_string_keyed_maps_are_sites() {
        assert_eq!(found("use serde_json::Value;\nfn f(v: &Value) { v.get(\"file_path\").and_then(|p| p.as_str()); }"), [JSON_BY_NAME]);
        assert_eq!(found("pub struct Filter { fields: HashMap<String, FieldFilter> }"), [STRING_KEYED]);
        assert_eq!(found("struct Live { by_peer: Arc<RwLock<BTreeMap<String, Peer>>> }"), [STRING_KEYED]);
    }

    // what this catches: wire decoders, typed keys, a non-JSON lookup, or a program counted.
    #[test]
    fn wire_code_typed_keys_and_programs_are_not_sites() {
        assert!(found_at("core/continuum-tool-protocol/src/a.rs", Target::Library, "use serde_json::Value; fn f(v: &Value) { v.get(\"x\"); }").is_empty());
        assert!(found_at("core/x/src/ipc/decode.rs", Target::Library, "use serde_json::Value; fn f(v: &Value) { v.get(\"x\"); }").is_empty());
        assert!(found("fn f(m: &Map) { m.get(\"x\"); }").is_empty(), "no JSON in sight");
        assert!(found("use serde_json::Value; fn f(v: &Value) { v.get(key); }").is_empty(), "not a literal");
        assert!(found("struct S { by_id: HashMap<RecordId, String> }").is_empty(), "a typed key");
        assert!(found_at("core/x/src/main.rs", Target::Program, "struct Args { env: HashMap<String, String> }").is_empty());
    }
}
