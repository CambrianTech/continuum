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
  PanelWidget,
  MetricsView,
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
  let cell: ListingCell = { id: m.id, title: m.name, glyph: kindGlyph(m.kind), badges, status };
  // Vitals (0–100 meters) and loadout (model·size·ctx labels) both ride the neutral
  // cell so the projection stays LOSSLESS — a target draws both from the Listing alone,
  // no rich view-model crosses the render boundary. Each attaches only when present.
  if (Object.keys(m.vitals).length > 0) cell = { ...cell, meters: m.vitals };
  if (m.loadout) cell = { ...cell, loadout: m.loadout };
  return cell;
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

/** The `AI Performance` rail widget — the room's LIVE team-cognition readout, derived
 *  from the roster's own vitals (no fabricated numbers, no extra pipe): how many are
 *  here, how many are actively thinking (a faculty firing on the live compass), and how
 *  many carry a paged-in genome. A persona whose Reason/Act/Focus lit this tick counts
 *  as thinking — the same signal the tile's compass draws, aggregated. (System resource
 *  + spend metrics — CPU/GPU/$ — are a separate core feed; this ships the honest slice.) */
export function metricsWidget(vm: ChatViewModel): PanelWidget<MetricsView> {
  const agents = vm.members.filter((m) => m.kind === 'agent');
  const here = vm.members.filter((m) => m.active).length;
  const thinking = agents.filter((m) => {
    const v = m.vitals;
    return (v.reason ?? 0) > 40 || (v.act ?? 0) > 40 || (v.focus ?? 0) > 40 || (v.recall ?? 0) > 40;
  }).length;
  const genomes = agents.filter((m) => (m.vitals.genome ?? 0) > 0).length;
  const metrics: MetricsView = {
    stats: [
      { label: 'here', value: String(here), tone: 'accent' },
      { label: 'thinking', value: String(thinking), tone: thinking > 0 ? 'good' : 'muted' },
      { label: 'genome', value: String(genomes), tone: genomes > 0 ? 'good' : 'muted' },
    ],
  };
  return { id: 'metrics', kind: 'metrics', title: 'AI Performance', body: metrics, scope: 'global' };
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
    // The left rail = a global widget stack (the README's sidebar): AI Performance
    // (live team cognition) · Rooms (all rooms/DMs) · Users & Agents (the rich live
    // tiles). Each is one PanelWidget dispatched by kind; the roster stays the
    // participants `Listing` (ROSTER_LISTING_ID) that RAG + mobile ground on.
    left: [
      metricsWidget(vm),
      listingWidget(roomsListing(vm)),
      listingWidget(rosterListing(vm)),
    ],
    content,
    context: { listings: [] },
  };
}
