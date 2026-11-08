//! **An id is structured data: a `Uuid` or a typed newtype, never a string.**
//!
//! Joel, 2026-10-31: "ids are structured data"; earlier, 2026-08-26: "We do NOT use strings for
//! keys … the moment you start prepending strings to UUIDs is the moment it falls apart." An id
//! held as a `String` can be any text: a short form, a name, a prefixed composite, a stale
//! rendering. Two owners then compare two spellings of the same thing and disagree, and every
//! reader re-parses it. A typed id is parsed once at the boundary and is only ever itself.
//!
//! Two ratchets, both per crate, both may only fall:
//! - `string_id_fields`: a field of a production `struct` named `id`, `*_id` or `*_ids` whose
//!   type is a string ([`STRINGS`]) or a container of one ([`CONTAINERS`]). Fix: type it as the
//!   id it is (`Uuid`, `PersonaId`, `RoomId`, airc's typed ids), parsed where it enters.
//! - `hand_rolled_short_ids`: an id's short form rendered by hand (`to_string()[..8]`,
//!   `chars().take(8)`, `&simple[..8]`) or a private helper for it. Fix: `id.shown()` on a typed
//!   airc id or `airc_core::shown_id::shown_form(uuid)`; parse with `Shown<T>`. This rule first
//!   lived in #5389 (`shown_ids.rs`, unmerged, 2026-10-31); it moved here, beside the field rule,
//!   with per-crate counts, so #5389 is the migration and carries no rule of its own.

use super::lexer::tokens::{matching_close, tokenize, Kind, Token};
use super::{split_code_and_comment, SourceFile, SourceRule, Violation};

/// Types that hold an id as text.
pub const STRINGS: &[&str] = &["String", "&str", "Box<str>", "Arc<str>", "Rc<str>"];

/// Containers whose element is counted as the field's type (`Option<String>`, `Vec<String>`).
pub const CONTAINERS: &[&str] = &["Option", "Vec", "VecDeque", "HashSet", "BTreeSet"];

pub struct StringIdFields;
pub struct HandRolledShortIds;

/// Does a field name name an id?
fn names_an_id(name: &str) -> bool {
    name == "id" || name.ends_with("_id") || name.ends_with("_ids")
}

/// The type's text with lifetimes removed (`&'a str` -> `&str`), as the tokens spell it.
fn type_text(tokens: &[Token]) -> String {
    let mut out = String::new();
    let mut k = 0;
    while k < tokens.len() {
        if tokens[k].is('\'') {
            k += 2; // a lifetime: the quote and its name
            if tokens.get(k).is_some_and(|t| t.is(',')) {
                k += 1;
            }
            continue;
        }
        out.push_str(tokens[k].text);
        k += 1;
    }
    out
}

/// Is `ty` a string, or one container of a string?
pub fn is_string_type(ty: &str) -> bool {
    if STRINGS.contains(&ty) {
        return true;
    }
    CONTAINERS.iter().any(|c| {
        ty.strip_prefix(c).and_then(|rest| rest.strip_prefix('<')).and_then(|rest| rest.strip_suffix('>')).is_some_and(|inner| STRINGS.contains(&inner))
    })
}

/// The `{ .. }` bodies of brace structs in `code`, as `(open, close)`.
fn struct_bodies(code: &[Token]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for at in 0..code.len() {
        if !(code[at].is_ident("struct") && code.get(at + 1).is_some_and(|t| t.kind == Kind::Ident)) {
            continue;
        }
        let mut angle = 0i32;
        for k in at + 2..code.len() {
            let t = &code[k];
            if t.is('<') {
                angle += 1;
            } else if t.is('>') && !code[k - 1].is('-') {
                angle -= 1;
            } else if angle == 0 && (t.is(';') || t.is('(')) && !code[at + 2..k].iter().any(|t| t.is_ident("where")) {
                break; // a unit or tuple struct
            } else if angle == 0 && t.is('{') {
                out.push((k, matching_close(code, k)));
                break;
            }
        }
    }
    out
}

/// The string-typed id fields of `production`: each field's line, name and type.
pub fn string_id_fields(production: &str) -> Vec<(usize, String, String)> {
    let code = tokenize(production).code;
    let mut out = Vec::new();
    for (open, close) in struct_bodies(&code) {
        let mut k = open + 1;
        while k < close {
            while code[k].is('#') && k + 1 < close {
                let bracket = if code[k + 1].is('!') { k + 2 } else { k + 1 };
                k = matching_close(&code, bracket) + 1; // an attribute
            }
            if k < close && code[k].is_ident("pub") {
                k += 1;
                if code[k].is('(') {
                    k = matching_close(&code, k) + 1;
                }
            }
            if k >= close {
                break;
            }
            let (name, colon) = (&code[k], code.get(k + 1));
            let mut end = k + 1;
            let mut depth = 0i32;
            while end < close {
                let t = &code[end];
                if t.is('(') || t.is('[') || t.is('{') || t.is('<') {
                    depth += 1;
                } else if t.is(')') || t.is(']') || t.is('}') || (t.is('>') && !code[end - 1].is('-')) {
                    depth -= 1;
                } else if t.is(',') && depth == 0 {
                    break;
                }
                end += 1;
            }
            if name.kind == Kind::Ident && colon.is_some_and(|t| t.is(':')) && names_an_id(name.text) {
                let ty = type_text(&code[k + 2..end]);
                if is_string_type(&ty) {
                    out.push((name.line, name.text.to_owned(), ty));
                }
            }
            k = end + 1;
        }
    }
    out
}

impl SourceRule for StringIdFields {
    fn name(&self) -> &'static str {
        "string_id_fields"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        string_id_fields(&file.production)
            .into_iter()
            .map(|(line, name, ty)| Violation { rule: self.name(), file: file.repo.clone(), line, source: format!("{name}: {ty}") })
            .collect()
    }
}

/// Whether a code line renders an id's leading characters by hand, or defines a private
/// short-id helper (#5389's predicate, kept whole).
pub fn hand_rolled(code: &str) -> bool {
    let slices = ["to_string()[..8]", "chars().take(8)", "[..SHORT_ID_LEN]", "&simple[..8]", "&short[..8]", "_id[..8]", "id_str[..8]", "uuid[..8]"];
    let helpers = ["fn short8(", "fn short_id(", "fn id8(", "fn short_uuid(", "let short = |"];
    slices.iter().chain(helpers.iter()).any(|p| code.contains(p))
}

impl SourceRule for HandRolledShortIds {
    fn name(&self) -> &'static str {
        "hand_rolled_short_ids"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        file.production_lines()
            .filter(|(_, l)| hand_rolled(split_code_and_comment(l).0))
            .map(|(n, l)| Violation { rule: self.name(), file: file.repo.clone(), line: n, source: l.trim().to_string() })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;

    fn first(violations: &[Violation]) -> String {
        violations.iter().take(20).map(|v| format!("  {}:{}  {}", v.file, v.line, v.source)).collect::<Vec<_>>().join("\n")
    }

    // what this catches: a new string-typed id field anywhere the gates audit.
    #[test]
    fn string_id_fields_never_increase() {
        let violations = scan(&[&StringIdFields]);
        crate::ratchet::hold_sites("string_id_fields", &violations, || {
            format!(
                "A field that holds an id holds it typed (Uuid, PersonaId, RoomId, airc's typed ids), \
                 parsed once where it enters, never as text.\nFirst {} of {}:\n{}",
                violations.len().min(20),
                violations.len(),
                first(&violations)
            )
        });
    }

    // what this catches: a new hand-rolled short-id rendering beside the one owner.
    #[test]
    fn hand_rolled_short_ids_never_increase() {
        let violations = scan(&[&HandRolledShortIds]);
        crate::ratchet::hold_sites("hand_rolled_short_ids", &violations, || {
            format!(
                "Render with id.shown() or airc_core::shown_id::shown_form(uuid); parse with \
                 airc_core::shown_id::Shown<T>.\nFirst {} of {}:\n{}",
                violations.len().min(20),
                violations.len(),
                first(&violations)
            )
        });
    }

    // what this catches: the negative control. An added string id field is counted, so the
    // ratchet rises and fails; a typed id, a non-id string and a tuple struct are not.
    #[test]
    fn an_added_string_id_field_is_counted_and_typed_ids_are_not() {
        let count = |src: &str| StringIdFields.check(&SourceFile::for_test("x.rs", src)).len();
        assert_eq!(count("pub struct Card {\n    pub id: String,\n    pub room_id: Option<String>,\n}"), 2);
        assert_eq!(count("struct Batch<'a> { #[serde(default)] pub(crate) persona_ids: Vec<String>, peer_id: &'a str }"), 2);
        assert_eq!(count("struct Typed { id: Uuid, room_id: RoomId, name: String, ids: HashMap<String, Uuid> }"), 0);
        assert_eq!(count("struct Wrapper(pub String);\nstruct Unit;"), 0, "tuple and unit structs have no named fields");
        assert_eq!(count("struct G<F: Fn(u8) -> u8> where F: Send { cb: F, card_id: Box<str> }"), 1, "generics and where clauses");
        assert_eq!(count("fn f(card_id: String) {}"), 0, "a parameter is not a field");
        assert_eq!(count("// struct C { id: String }"), 0, "a comment is not code");
    }

    // what this catches: the short-id predicate drifting (#5389's cases, kept).
    #[test]
    fn each_hand_rolled_form_is_counted_and_the_owner_is_not() {
        let check = |src: &str| HandRolledShortIds.check(&SourceFile::for_test("x.rs", src)).len();
        assert_eq!(check("let s = &id.to_string()[..8];"), 1);
        assert_eq!(check("let s: String = id.to_string().chars().take(8).collect();"), 1);
        assert_eq!(check("fn short8(id: &Uuid) -> String {"), 1);
        assert_eq!(check("format!(\"peer-{}\", &simple[..8])"), 1);
        assert_eq!(check("let s = &persona_id[..8];"), 1);
        assert_eq!(check("let s = airc_core::shown_id::shown_form(id);"), 0);
        assert_eq!(check("let first: Vec<_> = items.iter().take(8).collect();"), 0);
        assert_eq!(check("// to_string()[..8] in a comment"), 0);
    }
}
