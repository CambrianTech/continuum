//! The reserved rooms continuum ships: the ONE place their names are spelled.
//!
//! Every activity is a room that comes and goes as data (docs/architecture/ACTIVITY-ROOM-PATTERNS.md),
//! except these few, which every node has from the start. They are declared here once, so moving or
//! renaming one is one line, and code everywhere else refers to the constant, never the name
//! (the `fixed_rooms` source-hygiene rule fails a room name spelled anywhere else).

/// airc's own lobby: where a bare airc scope lands when nothing says otherwise.
pub const GENERAL: &str = "general";

/// The learning tree's root: benchmarks, training and coursework live inside it
/// (`airc://room:academy/benchmarks/<run>`).
pub const ACADEMY: &str = "academy";

/// Every reserved room, for a reader that lists or protects them.
pub const ALL: &[&str] = &[GENERAL, ACADEMY];
