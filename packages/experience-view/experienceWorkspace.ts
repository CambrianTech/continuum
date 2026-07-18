/**
 * Experience manifest → `@continuum/patterns` `WorkspaceView`.
 *
 * The second consumer (after chat-view) that projects a room into the neutral
 * who/what/where Workspace — but driven by the generic **Experience manifest** rather
 * than a chat-specific view model. The manifest supplies the STRUCTURE (purpose →
 * content dispatch, membership → structural standing); the `"roster"` region payload
 * supplies the rich roster DISPLAY (names, kind glyphs, vitals meters); and the
 * focused content's BODY is injected from whatever payload kind the purpose needs (chat
 * messages, a scoreboard, …). One projection, drawn by any `RenderTarget`.
 *
 * NOTE (first slice): roster → `left`, content dispatched by purpose. Fuller
 * region-DRIVEN placement (each `Region.scope`/`slot` deciding left vs content vs
 * context) and affordance rendering are follow-ups; here the manifest drives purpose +
 * standing, which is the seam this proves.
 */

import type {
  CellStatus,
  ContentView,
  ListingCell,
  ListingView,
  WorkspaceView,
} from '@continuum/patterns';
import type {
  Experience,
  Member,
  RosterSlotView,
  RosterViewState,
  SenderKind,
  Standing,
} from '@continuum/sdk-typescript';

/**
 * The composite state an Experience Workspace projects from — the region payloads a
 * renderer subscribes to, joined into one who/what/where. The `source` (in the client)
 * assembles this from the `"experience"` + `"roster"` + purpose-body kinds; the app
 * never names its transport ([[logical-portability-for-unknown-future-integrations]]).
 */
export interface ExperienceState {
  /** The room's manifest — STRUCTURE. */
  readonly manifest: Experience;
  /** The room's rich roster payload — DISPLAY (names/kinds/vitals). Absent before the
   *  first presence snapshot. */
  readonly roster?: RosterViewState;
  /** The focused content's purpose-specific body (chat messages, a scoreboard, …),
   *  supplied by that purpose's own payload kind. A registered content renderer draws it. */
  readonly contentBody?: unknown;
}

/** Leading glyph per member kind — the neutral human/agent/system discriminant as a
 *  display token the Listing carries (targets draw it, they don't re-derive it). */
function kindGlyph(kind: SenderKind): string {
  switch (kind.kind) {
    case 'human':
      return '🧑';
    case 'agent':
      return '🤖';
    case 'system':
      return '⚙️';
  }
}

/** Manifest membership → `Standing` by peer id — the structural role the manifest adds
 *  on top of the roster's presence data. */
function standingByPeer(membership: readonly Member[]): Map<string, Standing> {
  const byPeer = new Map<string, Standing>();
  for (const member of membership) byPeer.set(member.peerId, member.standing);
  return byPeer;
}

/** One roster slot → a `Listing` cell — rich (name, kind glyph, vitals meters), with the
 *  manifest's structural `standing` overlaid as a badge. Vitals ride as neutral cell
 *  `meters` so a target draws the ACT/genome bars from the Listing alone (LOSSLESS). */
function rosterCell(slot: RosterSlotView, standing: Standing | undefined): ListingCell {
  const badges = standing ? [slot.kind.kind, standing] : [slot.kind.kind];
  const status: CellStatus = slot.active ? 'active' : 'idle';
  const cell: ListingCell = {
    id: slot.member_id,
    title: slot.display_name,
    glyph: kindGlyph(slot.kind),
    badges,
    status,
  };
  return Object.keys(slot.vitals).length > 0 ? { ...cell, meters: slot.vitals } : cell;
}

/** The participant roster as the `Listing` primitive — rich cells from the `"roster"`
 *  payload, standing overlaid from the manifest. Same shape rooms/models use. */
export function rosterListing(manifest: Experience, roster: RosterViewState): ListingView {
  const standing = standingByPeer(manifest.membership);
  return {
    id: 'roster',
    title: 'Users & Agents',
    cells: roster.roster.map((slot) => rosterCell(slot, standing.get(slot.member_id))),
  };
}

/** The nav `Listing` — the rooms/tab primitive (tab bar for a human, channel-attention
 *  for a persona). One focused room today (single active cell grouped by purpose); fills
 *  out to a real tab bar with NO shape change (that is `activity == room == tab`). */
export function roomsListing(manifest: Experience, roster?: RosterViewState): ListingView {
  return {
    id: 'rooms',
    title: 'Rooms',
    cells: [
      {
        id: roster?.room_id ?? manifest.purpose,
        title: manifest.purpose,
        status: 'active',
        group: manifest.purpose,
      },
    ],
  };
}

/** The whole room as a `Workspace`, driven by the manifest: nav (rooms) + left (the rich
 *  roster with standing) + content (dispatched by the manifest's `purpose`, body injected
 *  from the purpose's own payload) + context. The data spine a `RenderTarget` draws on
 *  any surface. */
export function experienceWorkspace(state: ExperienceState): WorkspaceView {
  const content: ContentView = {
    purpose: state.manifest.purpose,
    body: state.contentBody,
  };
  return {
    nav: roomsListing(state.manifest, state.roster),
    left: state.roster ? [rosterListing(state.manifest, state.roster)] : [],
    content,
    context: { listings: [] },
  };
}
