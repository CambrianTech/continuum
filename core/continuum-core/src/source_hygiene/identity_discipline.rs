//! **Identity discipline: no nil rooms in production cognition, no
//! string-composite id keys in the serving path.**
//!
//! Both laws are Joel's, stated 2026-08-26 while the #425 arc closed, and both
//! are the kind prose does not hold:
//!
//! 1. *"Shouldn't be possible to create activities without rooms."* The
//!    [`ActivityRoom`](crate::identity::ActivityRoom) witness closes the front
//!    door at the type level, but a `Uuid::nil()` literal placed next to a room
//!    field is how the back door reopens — it is exactly how 13,209 roomless
//!    turns (35% of one citizen's cognition) accumulated invisibly.
//! 2. *"We do NOT use strings for keys, we use roomId … the moment you start
//!    prepending strings to UUIDs is the moment it falls apart."* The KV slot
//!    lease shipped keyed on `format!("{persona}@{room}")` for half a day
//!    before this rule existed. Typed `(PersonaId, RoomId)` structs key maps;
//!    a formatted composite is JS-brain and drifts.
//!
//! Ratchet doctrine (same as [`super::unwrap_justification`]): the baselines
//! below may only ever go DOWN. Raising one to green a build is defeating the
//! guard.

use super::{SourceFile, SourceRule, Violation};

/// Production `Uuid::nil()` literals on room-adjacent lines when this guard
/// landed (2026-08-26), after the ActivityRoom sweep. The survivors are each
/// deliberate (watch-channel sentinels skipped before cognition, fixture
/// builders) — listed so a NEW one has to argue, not sneak.
const BASELINE_NIL_ROOM_LINES: usize = 4;

/// Directories whose production halves carry cognition turns — the trees where
/// a nil room becomes an invisible turn.
const COGNITION_TREES: [&str; 3] = ["cognition/", "persona/", "commands/"];

pub struct NoNilRoomInProductionCognition;

impl SourceRule for NoNilRoomInProductionCognition {
    fn name(&self) -> &'static str {
        "no_nil_room_enters_production_cognition"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        if !COGNITION_TREES.iter().any(|t| file.rel.starts_with(t)) {
            return Vec::new();
        }
        file.production_lines()
            .filter(|(_, l)| {
                let t = l.trim_start();
                // Code only — a doc line QUOTING the banned shape is documentation,
                // not a violation.
                if t.starts_with("//") {
                    return false;
                }
                let lower = l.to_ascii_lowercase();
                l.contains("Uuid::nil()") && lower.contains("room")
            })
            .map(|(line, l)| Violation {
                rule: self.name(),
                file: file.rel.clone(),
                line,
                source: l.trim().to_string(),
            })
            .collect()
    }
}

/// String-composite id keys (`format!` gluing values with `@`) in the serving /
/// inference path: ZERO. The `persona@room` slot-lease key this rule was
/// written against died with the typed `ActivityKey` registry (slice B1,
/// same day). This stays 0 forever.
const BASELINE_STRING_COMPOSITE_KEYS: usize = 0;

/// The serving/inference trees where id-keyed maps live.
const SERVING_TREES: [&str; 2] = ["ai/", "inference/"];

pub struct NoStringCompositeIdKeys;

impl SourceRule for NoStringCompositeIdKeys {
    fn name(&self) -> &'static str {
        "no_string_composite_id_keys_in_serving"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        if !SERVING_TREES.iter().any(|t| file.rel.starts_with(t)) {
            return Vec::new();
        }
        file.production_lines()
            .filter(|(_, l)| l.contains("format!") && l.contains("}@{"))
            .map(|(line, l)| Violation {
                rule: self.name(),
                file: file.rel.clone(),
                line,
                source: l.trim().to_string(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_hygiene::scan;

    // what this catches: a NEW `Uuid::nil()` landing beside a room field in
    // production cognition — the back door to #425 after ActivityRoom closed the
    // front one. If this fails on your change: name a real room (mint or rejoin),
    // or move the fixture into the test half.
    #[test]
    fn nil_room_literals_never_rise() {
        let violations = scan(&[&NoNilRoomInProductionCognition]);
        assert!(
            violations.len() <= BASELINE_NIL_ROOM_LINES,
            "production nil-room literals rose to {} (baseline {BASELINE_NIL_ROOM_LINES}).\n\
             An activity without a room is unrepresentable (#425) — mint or rejoin an \
             ActivityRoom instead.\nNew offenders:\n{}",
            violations.len(),
            violations
                .iter()
                .take(10)
                .map(|v| format!("  {}:{} — {}", v.file, v.line, v.source))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    // what this catches: a NEW `format!("…{a}@{b}…")` composite key in the
    // serving path — the string-key regression Joel banned outright. Keys are
    // typed structs of UUIDs; maps key on the struct.
    #[test]
    fn string_composite_id_keys_never_rise() {
        let violations = scan(&[&NoStringCompositeIdKeys]);
        assert!(
            violations.len() <= BASELINE_STRING_COMPOSITE_KEYS,
            "string-composite id keys rose to {} (baseline {BASELINE_STRING_COMPOSITE_KEYS}).\n\
             Key on a typed struct of UUIDs, never a formatted string.\nOffenders:\n{}",
            violations.len(),
            violations
                .iter()
                .map(|v| format!("  {}:{} — {}", v.file, v.line, v.source))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    // what this catches: the predicates counting the wrong thing in either
    // direction — a rule that over-reports cries wolf and gets deleted; one that
    // under-reports lets the defect back in.
    #[test]
    fn predicates_match_exactly_the_banned_shapes() {
        let nil = NoNilRoomInProductionCognition;
        let hit = SourceFile::for_test("cognition/x.rs", "    room_id: Uuid::nil(),");
        let comment = SourceFile::for_test("cognition/x.rs", "    // room_id of Uuid::nil() is banned");
        assert_eq!(nil.check(&comment).len(), 0, "a doc line quoting the shape is not code");
        assert_eq!(nil.check(&hit).len(), 1, "nil beside a room field is flagged");
        let miss = SourceFile::for_test("cognition/x.rs", "    parent: Uuid::nil(),");
        assert_eq!(nil.check(&miss).len(), 0, "nil away from rooms is not this rule's business");
        let other_tree = SourceFile::for_test("runtime/x.rs", "    room_id: Uuid::nil(),");
        assert_eq!(nil.check(&other_tree).len(), 0, "scoped to cognition trees");

        let keys = NoStringCompositeIdKeys;
        let bad = SourceFile::for_test("ai/x.rs", r#"let k = format!("{persona}@{room}");"#);
        assert_eq!(keys.check(&bad).len(), 1, "the persona@room composite is flagged");
        let fine = SourceFile::for_test("ai/x.rs", r#"let m = format!("{user}@{host}: {err}");"#);
        assert_eq!(
            keys.check(&fine).len(),
            1,
            "any format-glued @-composite in serving is suspect — an error MESSAGE \
             belongs outside the id-key trees or built without the {{a}}@{{b}} shape"
        );
    }
}
