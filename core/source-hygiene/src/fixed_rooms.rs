//! **No code names a room. Every activity is a room, and rooms come and go as data.**
//!
//! Joel, 2026-11-08: "Why do you hard code their rooms?", "The whole point of this system is for
//! rooms to be used all the time, created, joined, closed, like anything else", "Literally a new
//! benchmark: room. A settings screen; room. A project: room, every damn thing is a room."
//! (docs/architecture/ACTIVITY-ROOM-PATTERNS.md: activity == room == content == tab.)
//!
//! A room named in code is a fixed place every node, every user and every resident inherits:
//! `CITIZEN_COMMONS_ROOM = "academy"` put every new resident in one room for good, where nobody
//! working elsewhere saw her work. A room is chosen by the activity that opens it, the recipe that
//! declares it, or whoever hosts her, and reaches code as data.
//!
//! The reserved rooms continuum ships (`general`, `academy`, ...) are legitimate constants, declared
//! ONCE in `continuum_base::reserved_rooms` so moving one is one line (Joel: "General and academy as
//! well as some others are literally reserved and shipped constants", "Ideally in one place too so
//! we don't have to change a ton to move"). This rule counts production lines elsewhere that spell
//! a reserved room's name on a line about a room (the line also says `room`; a bare `"academy"`
//! passes it). The count may only fall, and a touched file leaves with none: refer to the
//! constant, or take the room from the activity, the recipe or the host's configuration.

use super::{lexer::split_code_and_comment, SourceFile, SourceRule, Violation};

/// The one file a reserved room's name is spelled in.
const OWNER: &str = "core/continuum-base/src/reserved_rooms.rs";

/// The reserved rooms' names (declared in [`OWNER`]).
const ROOM_LITERALS: &[&str] = &["\"academy\"", "\"general\""];

pub struct FixedRooms;

/// Whether `code` names a standing room by a string literal, on a line about a room (so a topic
/// domain also called "general" is not a room).
pub fn names_a_room(code: &str) -> bool {
    code.to_ascii_lowercase().contains("room") && ROOM_LITERALS.iter().any(|room| code.contains(room))
}

impl SourceRule for FixedRooms {
    fn name(&self) -> &'static str {
        "fixed-rooms"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        if file.repo == OWNER {
            return Vec::new();
        }
        file.production_lines()
            .filter_map(|(line_no, line)| {
                let (code, _) = split_code_and_comment(line);
                names_a_room(code).then(|| Violation {
                    rule: "fixed-rooms",
                    file: file.repo.clone(),
                    line: line_no,
                    source: code.trim().to_string(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;

    // what this catches: a new room named in code, or one staying in a file a change touched.
    // The count may only fall.
    #[test]
    fn fixed_rooms_fall_and_touched_files_have_none() {
        let violations = scan(&[&FixedRooms]);
        let advice = || {
            "Every activity is a room, and rooms come and go as data: take the room from the \
             activity that opened it, the recipe that declares it, or the host's configuration, \
             never a name in code (docs/architecture/ACTIVITY-ROOM-PATTERNS.md)."
                .to_string()
        };
        crate::ratchet::hold_sites("fixed_rooms", &violations, crate::ratchet::Touched::EndsClean, advice);
    }

    // what this catches: the predicate missing a room literal, or flagging a word in prose.
    #[test]
    fn the_predicate_flags_a_room_literal_and_leaves_words_alone() {
        assert!(names_a_room(r#"pub const CITIZEN_COMMONS_ROOM: &str = "academy";"#));
        assert!(names_a_room(r#"room_name: "general".into(),"#));
        assert!(!names_a_room("use crate::academy::exam;"), "a module path is not a room");
        assert!(!names_a_room(r#"let kind = "general purpose";"#), "a phrase is not a room's name");
        assert!(!names_a_room(r#"fallback_domain: "general".to_string(),"#), "a topic domain is not a room");
    }
}
