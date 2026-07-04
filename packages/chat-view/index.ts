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
export type { ChatState } from './ChatState';

export { chatViewModel, formatTimeOfDay } from './chatViewModel';
export type {
  ChatViewModel,
  MemberKind,
  RosterMemberVM,
  MessageRowVM,
} from './chatViewModel';

// The chat activity expressed on the consumer-neutral pattern primitives
// (ACTIVITY-ROOM-PATTERNS.md): the roster IS the `Listing` primitive.
export { rosterListing } from './patternProjections';
