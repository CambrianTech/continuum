//! A mind's rooms and cards: who holds which card, the handoff record written at a seam, the room
//! directory and roster, her private mind room, presence glyphs and the bounded per-room ledger.
//! Cut out of continuum-cognition as a leaf of its crate tree (docs/architecture/
//! COGNITION-CRATE-TREE.md, step 2); cognition re-exports each module under its old path.

pub mod card_holder;
pub mod card_facts;
/// Each work verb's precondition over (card, actor): what may execute, and what she is shown she
/// can do (STATE-DRIVEN-SURFACE.md).
pub mod work_preconditions;
/// What on a board no reviewer can judge yet, and the next move it owes her.
pub mod review_queue;
/// The handoff record: her state at the seam, written before a stop, read first on wake
/// (card 49b5e806).
pub mod handoff;
pub mod room_directory;
pub mod room_substrates;
pub mod room_roster;
pub mod mind_room;
pub mod presence_glyph;
/// Bounded per-room diagnostics state, with the eviction decision CLAUDE.md
/// requires of anything that accumulates (#3903 review).
pub mod bounded_room_ledger;
