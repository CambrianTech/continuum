//! The ids persona's grounding sources deliver under: the faculty a workspace candidate
//! carries when a persona source fed it. Cognition matches on them (the prompt's churn
//! order, the held-work predicates) and persona's sources emit them, so they live here,
//! below both: one constant per source, never a second literal that can drift. Moved
//! from each source's own `SOURCE_ID` so cognition names no persona module.

/// `persona::active_work_source`.
///
/// Source identifier — used by budget presets, telemetry, and the projection that
/// routes this delivery into system-prompt grounding (standing framing).
/// The deliberation faculty's grounding floor keys on the SAME name to give a held
/// claim's card content first call on the reservation.
pub const ACTIVE_WORK: &str = "active-work";

/// `persona::airc_source`.
///
/// Source identifier — used by budget presets, telemetry, cursor scope checks.
pub const AIRC: &str = "airc";

/// `persona::engram_source`.
///
/// Source identifier — referenced by budget presets and telemetry.
pub const ENGRAMS: &str = "engrams";

/// `persona::media_perception_source`.
pub const MEDIA_PERCEPTION: &str = "media-perception";

/// The `[mission]` grounding block (generic `[<source_id>]` projection). The retired
/// `agent/solve` pinned the task brief under it (the #390 anchor-loss fix); the id stays
/// because the prompt's churn order still classifies that block as standing framing.
pub const MISSION: &str = "mission";

/// `persona::room_board_source`.
///
/// Source identifier — the deliberation faculty renders this delivery under a
/// `[room-kanban]` header (generic `[<source_id>]` projection). Distinct from
/// `active-work` (own claims) and `room-wall` (the wall, renamed from the
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

/// True when an [active-work] contribution's rendered content names a card this
/// persona currently HOLDS in progress. The writer is persona's `active_work_source`
/// renderer (which writes `card <id8> [{state:?}] "<title>" …` for HELD cards only —
/// the lost-claim and rejection facts carry no state tag); its test pins this reader to
/// that writer so the wire format and its one reader cannot drift apart. Matching our
/// OWN emitted format is protocol decoding, not inference about prose. Consumed by the
/// deliberation prompt to choose the working-presence contract on undirected turns — a
/// structural claim-state fact.
pub fn renders_held_in_progress(active_work_content: &str) -> bool {
    active_work_content.contains("[InProgress]")
}

/// True when the content names a card this persona HOLDS in either held state —
/// `[Claimed]` or `[InProgress]` (the two states `card_holder` treats as held).
/// Measured 2026-09-15 06:33Z on the M5: 11 held cards read Claimed and 3
/// InProgress, so a predicate keyed on InProgress alone saw no work at all — every
/// non-directed lane grant in 25 minutes was Ambient (#4058's receipt). The lane
/// gate keys on THIS; the working-presence contract keeps its InProgress key.
pub fn renders_held_card(active_work_content: &str) -> bool {
    active_work_content.contains("[InProgress]") || active_work_content.contains("[Claimed]")
}
