/**
 * `@continuum/chat-view` — the shared, framework-free chat projection.
 *
 * Two things every Continuum chat client needs identically, single-sourced here
 * so they can't drift between renderers:
 *   - the envelope→state merge (`chatStateFromEnvelope` / `CHAT_KIND` / `ChatState`)
 *     that joins a positron `StateEnvelope` to its `ChatViewState` payload; and
 *   - the pure view model (`chatViewModel` + its VM types) that projects a
 *     snapshot into flat, render-ready rows.
 *
 * Renderers live in the clients (apps/web's Lit `<chat-widget>`, apps/tui's ANSI
 * renderer). This package holds NO transport, NO DOM, NO ANSI — only the wire
 * decode + projection, typed against `@continuum/sdk-typescript`'s view types.
 */

export { CHAT_KIND, chatStateFromEnvelope } from './ChatState';
export { NAV_KIND, navStateFromEnvelope } from './NavState';
export { SYSTEM_METRICS_KIND, systemMetricsFromEnvelope } from './SystemMetricsState';
export { KANBAN_KIND, kanbanStateFromEnvelope } from './KanbanState';
export type { ChatState } from './ChatState';

export { chatViewModel, formatTimeOfDay } from './chatViewModel';
export { historyRowsFromPoll } from './historyProjections';
export type {
  ChatViewModel,
  MemberKind,
  RosterMemberVM,
  LoadoutVM,
  MessageRowVM,
} from './chatViewModel';

// The transcript's digest tier ([[perception-resolution-contract]]): mechanical
// head + tail-summary + repetition-histogram classification of over-threshold
// message bodies, so no message can flood a renderer — human or persona.
export {
  messageDigest,
  DIGEST_OVER_CHARS,
  DIGEST_OVER_LINES,
  DIGEST_HEAD_LINES,
  DIGEST_HEAD_CHARS,
} from './messageDigest';
export type { MessageDigestVM } from './messageDigest';

// The chat activity expressed on the consumer-neutral pattern primitives
// (ACTIVITY-ROOM-PATTERNS.md): the roster IS the `Listing`, and the whole room
// composes into a `Workspace` (nav + left + purpose-keyed content + context).
export {
  rosterListing,
  roomsListing,
  roomsListingFromNav,
  systemGaugeWidget,
  systemPanelWidget,
  nodesWidget,
  continuonWidget,
  roomInfoListing,
  chatWorkspace,
} from './patternProjections';
export type { ChatContentBody, WorkspaceLive } from './patternProjections';

// The persona HOME projections — the profile/brain surface as a purpose-
// dispatched activity (PERSONA_PURPOSE), lit from the SAME live pulse the
// roster tiles draw. `chatWorkspace` dispatches to it off the focused tab's
// kind; these are exported for renderers/tests that build the pieces directly.
export {
  focusedPersonaTab,
  personaContentBody,
  brainRegions,
  personaPathways,
  personaClaims,
  personaFactsListing,
  agoText,
} from './personaProjections';

// The LIVE call-face projections — a room's call grid as a purpose-dispatched
// activity (LIVE_PURPOSE), lit from the SAME StreamDelta token rail the roster's
// speaking ring draws. `chatWorkspace` dispatches to it off the room recipe's
// purpose / a live tab / the Go-live overlay; exported for renderers/tests.
export {
  focusedLiveTab,
  liveFaceOpen,
  liveContentBody,
  liveParticipants,
  liveCaption,
  liveControls,
  captionTail,
  CAPTION_TAIL_CHARS,
} from './liveProjections';
export type { LiveCallOverlay } from './liveProjections';

// Arena projections — real eval-ledger rows → ranked leaderboards + live-run
// strip (the benchmarks-are-the-show face).
export { ARENA_KIND, arenaBoards, arenaContentBody } from './arenaProjections';
export type { ArenaLedgerRow, ArenaViewState } from './arenaProjections';

// The chat activity as a positron app, defined ONCE — mount it on any RenderTarget
// (web/mobile/terminal/RAG). The first real `defineApp` consumer.
export { chatApp } from './chatApp';
