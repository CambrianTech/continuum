/**
 * Chat → pattern-primitive projections.
 *
 * Proves ACTIVITY-ROOM-PATTERNS.md's thesis on the chat activity: the roster is not a
 * bespoke widget — it is the `Listing` primitive (`@continuum/patterns`). This maps
 * the chat view model's members into a `ListingView` (the people-Listing), so the same
 * rows render on any `RenderTarget`: web member-cards, an ANSI list, or a persona's
 * grounding block. The projection resolves the display fields (glyph, badges, status);
 * a target only draws them.
 */

import { listingWidget } from '@continuum/patterns';
import type {
  ListingView,
  ListingCell,
  CellStatus,
  ContentView,
  WorkspaceView,
} from '@continuum/patterns';
import type { ChatViewModel, MemberKind, MessageRowVM, RosterMemberVM } from './chatViewModel';

/** Leading glyph per member kind — the neutral human/agent/system discriminant, as a
 *  display token the Listing carries (targets draw it, they don't re-derive it). */
function kindGlyph(kind: MemberKind): string {
  switch (kind) {
    case 'human':
      return '🧑';
    case 'agent':
      return '🤖';
    case 'system':
      return '⚙️';
  }
}

/** One roster member → a `Listing` cell (the people-Listing cell template). The
 *  member's genome-energy vitals ride along as neutral cell `meters` so a target draws
 *  the ACT bars from the Listing alone — the projection is LOSSLESS, no rich view-model
 *  crosses the render boundary. */
function rosterCell(m: RosterMemberVM): ListingCell {
  const badges = m.runtime ? [m.kind, m.runtime] : [m.kind];
  const status: CellStatus = m.active ? 'active' : 'idle';
  const cell: ListingCell = { id: m.id, title: m.name, glyph: kindGlyph(m.kind), badges, status };
  return Object.keys(m.vitals).length > 0 ? { ...cell, meters: m.vitals } : cell;
}

/** The chat activity's `who` panel projected as the `Listing` primitive. Same shape
 *  the rooms/DMs list and Foundry's model list use — one primitive, different data. */
export function rosterListing(vm: ChatViewModel): ListingView {
  return {
    id: 'roster',
    title: 'Users & Agents',
    cells: vm.members.map(rosterCell),
  };
}

/** The nav `Listing` — the rooms-Listing that is the tab bar for a human and the
 *  channel-attention set for a persona (one nav primitive over room-space,
 *  ACTIVITY-ROOM-PATTERNS.md). Today the client holds one focused room, so this is a
 *  single active cell; when the client tracks multiple rooms/DMs it fills out with no
 *  shape change (that is the point of `activity == room == tab`). */
export function roomsListing(vm: ChatViewModel): ListingView {
  return {
    id: 'rooms',
    title: 'Rooms',
    cells: [{ id: vm.roomId, title: vm.roomName, status: 'active', group: vm.purpose }],
  };
}

/** The chat activity's `Content` body — the conversation. `Content` is keyed by the
 *  room's `purpose` (here `vm.purpose`, `"chat"`), so a target's registered chat
 *  renderer draws these rows; a foundry room would carry a different purpose + body. */
export interface ChatContentBody {
  readonly messages: readonly MessageRowVM[];
  readonly isEmpty: boolean;
}

/** The whole chat room as a `Workspace` — nav (rooms) + left (people) + content
 *  (the conversation, dispatched by `purpose`) + an empty context panel. This is the
 *  data spine a `RenderTarget` draws; every activity projects its own `Workspace` the
 *  same way, so the shell is identical and only content/context vary. */
export function chatWorkspace(vm: ChatViewModel): WorkspaceView {
  const content: ContentView<ChatContentBody> = {
    purpose: vm.purpose,
    body: { messages: vm.messages, isEmpty: vm.isEmpty },
  };
  return {
    nav: roomsListing(vm),
    // The left rail as a global widget stack: the roster is one `kind:'listing'` widget.
    // Metrics / Rooms widgets join this stack as they land (task #184) with no shape change.
    left: [listingWidget(rosterListing(vm))],
    content,
    context: { listings: [] },
  };
}
