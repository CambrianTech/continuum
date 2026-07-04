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
import { rosterListing, roomsListing, chatWorkspace } from './patternProjections';

const vm: ChatViewModel = {
  roomName: 'general',
  roomId: 'room-1',
  purpose: 'chat',
  memberCount: 2,
  activeCount: 1,
  members: [
    { id: 'a', name: 'Asha', kind: 'agent', active: true, runtime: 'persona' },
    { id: 'j', name: 'Joel', kind: 'human', active: false, runtime: '' },
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
    const asha = l.cells[0]!;
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
    expect(ws.left).toHaveLength(1);
    expect(ws.left[0]!.id).toBe('roster');
    expect(ws.content.purpose).toBe('chat');
    const body = ws.content.body as { messages: unknown[]; isEmpty: boolean };
    expect(body.messages).toHaveLength(1);
    expect(body.isEmpty).toBe(false);
    expect(ws.context.listings).toHaveLength(0);
  });
});
