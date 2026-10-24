//! **An id is shown one way: airc's `shown_form`.**
//!
//! Joel, 2026-10-24: "Need to develop more common patterns and types so you're not able to make
//! all these mismatches. We literally made a shortuuid | uuid for reuse everywhere." It exists:
//! `airc_core::shown_id` — `Shown<T>` (an id as typed at a boundary: full, or the leading hex a
//! surface shows, refused in the type's own words), `Shown::resolve` (a miss or a tie refused with
//! what was found, never guessed) and `shown_form` (the ONE producer of the short rendering; each
//! typed id's `shown()` calls it). The core used it zero times. Measured with this predicate on
//! canary 2026-10-24: 62 hand-rolled renderings (`to_string()[..8]`, `chars().take(8)`) in 43
//! files, and 16 private helpers (`short8`, `short_id`, `id8`, …), each free to differ.
//!
//! The count may only fall: render an id with `id.shown()` (a typed airc id) or
//! `airc_core::shown_id::shown_form(uuid)`, parse one with `Shown<T>`.

use super::{SourceFile, SourceRule, Violation};

pub struct HandRolledShortIds;

/// Whether a code line renders an id's leading 8 characters by hand, or defines a private
/// short-id helper.
fn hand_rolled(code: &str) -> bool {
    let slices = ["to_string()[..8]", "simple().to_string()[..8]", "chars().take(8)", "[..SHORT_ID_LEN]"];
    let helpers = ["fn short8(", "fn short_id(", "fn id8(", "fn short_uuid(", "let short = |"];
    slices.iter().chain(helpers.iter()).any(|p| code.contains(p))
}

impl SourceRule for HandRolledShortIds {
    fn name(&self) -> &'static str {
        "hand_rolled_short_ids"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        file.production_lines()
            .filter(|(_, l)| hand_rolled(crate::split_code_and_comment(l).0))
            .map(|(n, l)| Violation { rule: self.name(), file: file.rel.clone(), line: n, source: l.trim().to_string() })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;

    // what this catches: a new private short-id rendering beside the one owner, the drift that
    // let surfaces disagree on what an id looks like (and let verbs refuse what a surface showed).
    #[test]
    fn hand_rolled_short_ids_never_increase() {
        let violations = scan(&[&HandRolledShortIds]);
        let count = violations.len();
        crate::ratchet::hold("hand_rolled_short_ids", count, || {
            format!(
                "hand-rolled short ids rose to {count}. Render with id.shown() or \
                 airc_core::shown_id::shown_form(uuid); parse with airc_core::shown_id::Shown<T>.\nFirst {} of {count}:\n{}",
                count.min(20),
                violations.iter().take(20).map(|v| format!("  {}:{}  {}", v.file, v.line, v.source)).collect::<Vec<_>>().join("\n")
            )
        });
    }

    // what this catches: the predicate drifting. Each hand-rolled form is counted; the owner's
    // call and an unrelated `take(8)` on something that is not chars are not.
    #[test]
    fn each_hand_rolled_form_is_counted_and_the_owner_is_not() {
        let check = |src: &str| HandRolledShortIds.check(&SourceFile::for_test("x.rs", src)).len();
        assert_eq!(check("let s = &id.to_string()[..8];"), 1);
        assert_eq!(check("let s: String = id.to_string().chars().take(8).collect();"), 1);
        assert_eq!(check("fn short8(id: &Uuid) -> String {"), 1);
        assert_eq!(check("let short = |id: Uuid| id.to_string();"), 1);
        assert_eq!(check("let s = airc_core::shown_id::shown_form(id);"), 0);
        assert_eq!(check("let first: Vec<_> = items.iter().take(8).collect();"), 0);
        assert_eq!(check("// to_string()[..8] in a comment"), 0);
    }
}
