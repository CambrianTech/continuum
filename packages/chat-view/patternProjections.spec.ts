/**
 * Chat → pattern-primitive projection tests.
 *
 * Proves the chat activity expresses itself on the consumer-neutral primitives
 * (ACTIVITY-ROOM-PATTERNS.md): the roster is a `Listing`, the room is a nav
 * `Listing` cell, and the whole room composes into a `Workspace` whose `Content`
 * is keyed by the room's `purpose`. Projections take the flat `ChatViewModel`, so
 * these fixtures need no wire types.
 */

import { describe, it, expect } from 'vitest';
import type { ChatViewModel } from './chatViewModel';
import {
  rosterListing,
  roomsListing,
  roomsListingFromNav,
  continuonWidget,
  systemPanelWidget,
  chatWorkspace,
  type ChatContentBody,
} from './patternProjections';
import type { NavViewState } from '@continuum/sdk-typescript';

const vm: ChatViewModel = {
  roomName: 'general',
  roomId: 'room-1',
  purpose: 'chat',
  memberCount: 2,
  activeCount: 1,
  members: [
    {
      id: 'a',
      name: 'Asha',
      kind: 'agent',
      active: true,
      runtime: 'persona',
      vitals: { energy: 80, attention: 90 },
      loadout: { model: 'devstral-24b', params: 24_000_000_000, contextWindow: 32_768 },
      lastSeenMs: 1_700_000_000_000,
    },
    { id: 'j', name: 'Joel', kind: 'human', active: false, runtime: '', vitals: {}, lastSeenMs: 0 },
  ],
  messages: [
    { id: 'm1', senderId: 'a', senderName: 'Asha', kind: 'agent', content: 'hi', time: '00:00', runtime: 'persona' },
  ],
  isEmpty: false,
};

describe('chat → pattern projections', () => {
  // what this catches: the roster projects to the people-`Listing` — the cell
  // template resolves glyph/badges/status so a target only draws them.
  it('projects the roster into the people Listing', () => {
    const l = rosterListing(vm);
    expect(l.id).toBe('roster');
    expect(l.title).toBe('Users & Agents');
    expect(l.cells).toHaveLength(2);
    const [asha] = l.cells;
    expect(asha).toMatchObject({
      id: 'a',
      title: 'Asha',
      glyph: '🤖',
      status: 'active',
      badges: ['agent', 'persona'],
    });
    // a human with no runtime carries just its kind badge, idle status
    expect(l.cells[1]).toMatchObject({ title: 'Joel', glyph: '🧑', status: 'idle', badges: ['human'] });
  });

  // what this catches: a member's vitals ride along as neutral cell `meters`, so a
  // target draws the ACT bars from the Listing alone — the projection is LOSSLESS and
  // the rich view-model never has to cross the render boundary. A member with no vitals
  // carries no `meters` key (absent, not an empty object), so targets draw no meter.
  it('carries member vitals as neutral cell meters, absent when empty', () => {
    const cells = rosterListing(vm).cells;
    expect(cells[0]?.meters).toEqual({ energy: 80, attention: 90 });
    expect(cells[1]).not.toHaveProperty('meters');
  });

  // what this catches: a member's LOADOUT (model·size·ctx) rides the neutral cell
  // losslessly — the SAME design as `meters` — so a target draws the strip from the
  // Listing alone. A member with no loadout carries no `loadout` key (absent, never
  // a fabricated model).
  it('carries member loadout as a neutral cell field, absent when none', () => {
    const cells = rosterListing(vm).cells;
    expect(cells[0]?.loadout).toEqual({
      model: 'devstral-24b',
      params: 24_000_000_000,
      contextWindow: 32_768,
    });
    expect(cells[1]).not.toHaveProperty('loadout');
  });

  // what this catches: a member's recency (last_seen_ms) rides the neutral cell as
  // `lastActiveMs` — the tile's "55m ago" stamp (card 2661a1b1) — and an unreported
  // recency (0) attaches no key, so no fabricated stamp is ever drawn.
  it('carries member recency as lastActiveMs, absent when unreported', () => {
    const cells = rosterListing(vm).cells;
    expect(cells[0]?.lastActiveMs).toBe(1_700_000_000_000);
    expect(cells[1]).not.toHaveProperty('lastActiveMs');
  });

  // what this catches: the focused room projects to the nav `Listing` (the tab
  // bar / channel-attention), carrying its purpose as the cell group.
  it('projects the focused room into the nav Listing', () => {
    const nav = roomsListing(vm);
    expect(nav.id).toBe('rooms');
    expect(nav.cells).toHaveLength(1);
    expect(nav.cells[0]).toMatchObject({ id: 'room-1', title: 'general', status: 'active', group: 'chat' });
  });

  // what this catches: the whole room composes into a Workspace — nav + left
  // people-Listing + Content keyed by the room's purpose + an (empty) context.
  // This is the data spine a RenderTarget draws; foundry slots in as a different
  // purpose + content body with NO shell change.
  it('composes the room into a Workspace with purpose-keyed Content', () => {
    const ws = chatWorkspace(vm);
    expect(ws.nav.id).toBe('rooms');
    // The left rail is a global widget stack: Continuon (identity header) ·
    // System (the two-faced SYS|AI panel) · Rooms (listing) · Users & Agents
    // (listing). The roster is the participants listing (id 'roster').
    expect(ws.left).toHaveLength(4);
    expect(ws.left.map((w) => w.kind)).toEqual(['continuon', 'system', 'listing', 'listing']);
    const roster = ws.left.find(
      (w) => w.kind === 'listing' && (w.body as { id: string }).id === 'roster',
    );
    expect(roster).toBeDefined();
    expect(roster?.id).toBe('roster');
    expect(ws.content.purpose).toBe('chat');
    // `WorkspaceView.content.body` is deliberately opaque (`ContentView<unknown>`) at
    // the workspace boundary; a chat-aware test narrows to the exported chat body.
    const body = ws.content.body as ChatContentBody;
    expect(body.messages).toHaveLength(1);
    expect(body.isEmpty).toBe(false);
    expect(ws.context.listings).toHaveLength(0);
  });

  // what this catches: the continuon header is projected purely from data the chat
  // state already carries — the ticker digests the LAST turns (newest last, hard
  // truncation so the header never becomes a second transcript), `alive` reflects
  // real presence, and the version badge attaches only when the host reported one
  // (honest-absent, never a fabricated stamp).
  it('projects the continuon header with a digested ticker and honest version', () => {
    const longVm: ChatViewModel = {
      ...vm,
      messages: [
        ...vm.messages,
        { id: 'm2', senderId: 'a', senderName: 'Asha', kind: 'agent', time: '00:01', runtime: 'persona',
          content: 'a very long line that must be clipped hard because the ticker is a log strip, not a transcript' },
      ],
    };
    const w = continuonWidget(longVm, 'v0.1.0');
    expect(w.kind).toBe('continuon');
    expect(w.body.wordmark).toBe('continuum');
    expect(w.body.version).toBe('v0.1.0');
    expect(w.body.alive).toBe(true);
    expect(w.body.ticker).toHaveLength(2);
    expect(w.body.ticker[0]).toBe('Asha: hi');
    expect(w.body.ticker[1]?.length).toBeLessThanOrEqual('Asha: '.length + 34);
    expect(w.body.ticker[1]?.endsWith('…')).toBe(true);
    // No version reported → no badge field at all.
    expect(continuonWidget(vm).body).not.toHaveProperty('version');
    // The workspace puts the continuon at the TOP of the rail.
    expect(chatWorkspace(vm).left[0]?.kind).toBe('continuon');
  });

  // what this catches: the two-faced System panel composes the EXISTING gauge +
  // metrics projections (no new numbers): without the live sys feed the gauge
  // face is honestly absent (no fabricated flat graph), with it the reshaped
  // series ride in; the AI stats derive from the roster either way.
  it('composes the system panel with an honestly-absent gauge until sys delivers', () => {
    const dry = systemPanelWidget(vm);
    expect(dry.kind).toBe('system');
    expect(dry.body).not.toHaveProperty('gauge');
    expect(dry.body.stats.stats.map((s) => s.label)).toEqual(['here', 'thinking', 'genome']);
    const wet = systemPanelWidget(vm, {
      series: [{ label: 'cpu', points: [10, 20], current: '20%' }],
      sample_interval_ms: 2000,
    });
    expect(wet.body.gauge?.series[0]).toMatchObject({ label: 'CPU', current: '20%' });
  });

  // what this catches: brick 1 (POSITRON-WIDGET-SOPHISTICATION.md) — the live
  // nav view supersedes the single-cell rooms listing with the real room SET:
  // the focused room draws active (not cells[0] by accident of order), unread
  // rides the neutral `count` (absent at 0 — no fabricated badge), and the tab
  // kind is the group facet the All/Rooms/DMs filter runs over.
  it('projects the nav room set with unread counts and focused-room highlight', () => {
    const nav: NavViewState = {
      user_id: 'joel',
      current_tab: 'room-2',
      open_tabs: [
        { id: 'room-1', title: 'general', kind: 'room', unread: 0, purpose: 'chat' },
        { id: 'room-2', title: 'dev-updates', kind: 'room', unread: 4, purpose: '' },
      ],
      last_read: { 'room-1': 1 },
      bookmarks: [],
    };
    // The chat view is focused on room-1 — THAT drives the active cell, because
    // the rooms rail highlights what the surface is showing.
    const listing = roomsListingFromNav(nav, 'room-1');
    expect(listing.id).toBe('rooms');
    expect(listing.cells).toHaveLength(2);
    expect(listing.cells[0]).toMatchObject({ id: 'room-1', status: 'active', group: 'room' });
    expect(listing.cells[0]?.count).toBeUndefined();
    expect(listing.cells[1]).toMatchObject({ id: 'room-2', status: 'idle', count: 4 });
    // The recipe-defined purpose rides as the description line; an unresolved
    // (empty) purpose attaches no subtitle — honest, never a fabricated blurb.
    expect(listing.cells[0]?.subtitle).toBe('chat');
    expect(listing.cells[1]).not.toHaveProperty('subtitle');
    // And the workspace swaps the nav-derived listing into BOTH the nav slot and
    // the rooms rail widget when nav is present.
    const ws = chatWorkspace(vm, { nav });
    expect(ws.nav.cells).toHaveLength(2);
    const rooms = ws.left.find(
      (w) => w.kind === 'listing' && (w.body as { id: string }).id === 'rooms',
    );
    expect((rooms?.body as { cells: unknown[] }).cells).toHaveLength(2);
  });
});
