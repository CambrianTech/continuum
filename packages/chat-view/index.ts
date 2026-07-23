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
export type { ChatState } from './ChatState';

export { chatViewModel, formatTimeOfDay } from './chatViewModel';
export type {
  ChatViewModel,
  MemberKind,
  RosterMemberVM,
  LoadoutVM,
  MessageRowVM,
} from './chatViewModel';

// The chat activity expressed on the consumer-neutral pattern primitives
// (ACTIVITY-ROOM-PATTERNS.md): the roster IS the `Listing`, and the whole room
// composes into a `Workspace` (nav + left + purpose-keyed content + context).
export { rosterListing, roomsListing, roomsListingFromNav, chatWorkspace } from './patternProjections';
export type { ChatContentBody } from './patternProjections';

// The chat activity as a positron app, defined ONCE — mount it on any RenderTarget
// (web/mobile/terminal/RAG). The first real `defineApp` consumer.
export { chatApp } from './chatApp';
