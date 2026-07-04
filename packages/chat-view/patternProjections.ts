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

import type { ListingView, ListingCell, CellStatus } from '@continuum/patterns';
import type { ChatViewModel, MemberKind, RosterMemberVM } from './chatViewModel';

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

/** One roster member → a `Listing` cell (the people-Listing cell template). */
function rosterCell(m: RosterMemberVM): ListingCell {
  const badges = m.runtime ? [m.kind, m.runtime] : [m.kind];
  const status: CellStatus = m.active ? 'active' : 'idle';
  return { id: m.id, title: m.name, glyph: kindGlyph(m.kind), badges, status };
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
