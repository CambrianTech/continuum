//! The ids persona's grounding sources deliver under: the faculty a workspace candidate
//! carries when a persona source fed it. Persona's sources emit them and cognition names
//! them, so they live here, below both: one constant per source, never a second literal
//! that can drift. Moved
//! from each source's own `SOURCE_ID` so cognition names no persona module.

/// `persona::airc_source`.
///
/// Source identifier — used by budget presets, telemetry, cursor scope checks.
pub const AIRC: &str = "airc";

/// `persona::media_perception_source`.
pub const MEDIA_PERCEPTION: &str = "media-perception";

/// `persona::room_board_source`.
///
/// Source identifier — the deliberation faculty renders this delivery under a
/// `[room-kanban]` header (generic `[<source_id>]` projection). Distinct from
/// her held work (a source of her view) and `room-wall` (the wall, renamed from the
/// misleading `room-board` in #3874).
pub const ROOM_BOARD: &str = "room-kanban";

/// `persona::room_doctrine_source`.
///
/// Source identifier — the service-loop projection routes this delivery
/// into system-prompt grounding (a `[Room operating doctrine]` block).
pub const ROOM_DOCTRINE: &str = "room-doctrine";

/// `persona::room_roster_source`.
///
/// Source identifier — used by budget presets, telemetry, and the
/// service-loop projection that routes this delivery into system-prompt
/// grounding rather than conversation history.
pub const ROOM_ROSTER: &str = "room-roster";

/// `persona::wall_source`.
///
/// Source identifier -- the deliberation faculty renders this delivery under a
/// `[room-wall]` header (generic `[<source_id>]` projection), so this string is
/// what a CITIZEN reads above the pinned posts, not just what a grep finds.
///
/// It was `"room-board"` until #3874. That was not a naming quibble: the WALL
/// announced itself as the BOARD while `room_board_source.rs` -- the work board
/// -- emitted `"room-kanban"`. Every id-first reader landed in the wrong file,
/// and it cost a wrong row in a published census (2026-09-08) plus a landmine
/// warning to a citizen mid-task.
///
/// Renamed ONE WAY on purpose. Swapping both (`room-board` -> the kanban,
/// `wall` -> `room-board`) would have made an EXISTING id change meaning, so a
/// replay corpus or a prompt capture keyed on `room-board` would silently start
/// resolving to a different source. Retiring an id and minting a fresh one has
/// no such window: `room-board` now names nothing, and nothing that reads it
/// gets a wrong answer instead of no answer.
pub const WALL: &str = "room-wall";

/// `persona::workspace_map_source`.
///
/// Source identifier — the deliberation faculty renders this delivery under a
/// `[workspace-map]` grounding block (header is the source id verbatim, same as
/// `room-roster` / `room-doctrine` in the brain path).
pub const WORKSPACE_MAP: &str = "workspace-map";

/// `persona::awareness_source`.
pub const AWARENESS: &str = "awareness";
