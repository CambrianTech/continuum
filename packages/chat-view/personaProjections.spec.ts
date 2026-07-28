/**
 * Persona home → pattern-primitive projection tests.
 *
 * Proves the persona's profile/brain is an ACTIVITY like any room: the focused
 * persona-kind nav tab keys the workspace's `Content` onto `PERSONA_PURPOSE`
 * (the same registry dispatch as chat/foundry — never a parallel route), and
 * every field of the projected body is REAL state or explicitly absent — the
 * brain regions light from the SAME vitals pulse the roster compass draws.
 */

import { describe, it, expect } from 'vitest';
import { PERSONA_PURPOSE, type PersonaContentBody } from '@continuum/patterns';
import type { KanbanViewState, NavViewState } from '@continuum/sdk-typescript';
import type { ChatViewModel } from './chatViewModel';
import { chatWorkspace } from './patternProjections';
import {
  agoText,
  brainRegions,
  focusedPersonaTab,
  personaClaims,
  personaContentBody,
  personaFactsListing,
  personaPathways,
} from './personaProjections';

const vm: ChatViewModel = {
  roomName: 'general',
  roomId: 'room-1',
  purpose: 'chat',
  memberCount: 2,
  activeCount: 1,
  members: [
    {
      id: 'asha',
      name: 'Asha',
      kind: 'agent',
      active: true,
      runtime: 'devstral',
      vitals: { activity: 72, queue: 38, focus: 62, reason: 88, recall: 40, act: 20 },
      loadout: { model: 'devstral-24b', params: 24_000_000_000, contextWindow: 32_768 },
      lastSeenMs: 1_700_000_000_000,
      genes: ['rust-hands', 'tool-fluency'],
      avatarUrl: '/avatars/asha.png',
    },
    { id: 'joel', name: 'Joel', kind: 'human', active: false, runtime: '', vitals: {}, lastSeenMs: 0 },
  ],
  messages: [],
  isEmpty: true,
};

const navWithPersonaTab = (current: string): NavViewState => ({
  user_id: 'joel',
  current_tab: current,
  open_tabs: [
    { id: 'room-1', title: 'general', kind: 'room', unread: 0, purpose: 'chat' },
    { id: 'asha', title: 'Asha', kind: 'persona', unread: 0, purpose: 'persona' },
  ],
  last_read: {},
  bookmarks: [],
});

describe('persona home → pattern projections', () => {
  // what this catches: the content dispatch keys off the FOCUSED TAB'S KIND —
  // a persona-kind current tab swaps the center to purpose "persona" (the same
  // ContentRegistry seam chat/foundry use) while a room-kind focus keeps the
  // room's own purpose. Regression here = the persona home becomes a parallel
  // route, or a persona click hijacks the chat center for every room.
  it('chatWorkspace dispatches PERSONA_PURPOSE when the focused tab is persona-kind', () => {
    const ws = chatWorkspace(vm, { nav: navWithPersonaTab('asha') });
    expect(ws.content.purpose).toBe(PERSONA_PURPOSE);
    const body = ws.content.body as PersonaContentBody;
    expect(body.personaId).toBe('asha');
    expect(body.name).toBe('Asha');

    const roomWs = chatWorkspace(vm, { nav: navWithPersonaTab('room-1') });
    expect(roomWs.content.purpose).toBe('chat');
  });

  // what this catches: the ACTIVE nav cell follows the persona tab when the
  // persona home is focused (the tab bar highlights it), and the right context
  // panel swaps to the persona FACTS listing — activity-scoped context.
  it('the persona tab draws active and the context panel carries persona facts', () => {
    const ws = chatWorkspace(vm, { nav: navWithPersonaTab('asha') });
    const active = ws.nav.cells.find((c) => c.status === 'active');
    expect(active?.id).toBe('asha');
    expect(ws.context.listings[0]?.id).toBe('persona-facts');
    // A room focus keeps the room info card.
    const roomWs = chatWorkspace(vm, { nav: navWithPersonaTab('room-1') });
    expect(roomWs.context.listings[0]?.id).toBe('room-info');
  });

  // what this catches: focusedPersonaTab is the ONE rule for "is a persona home
  // on screen" — only a current tab that is persona-kind qualifies; a room
  // focus, a stale non-current persona tab, or no nav at all answer undefined.
  it('focusedPersonaTab keys strictly off the current persona-kind tab', () => {
    expect(focusedPersonaTab(navWithPersonaTab('asha'))).toEqual({ id: 'asha', title: 'Asha' });
    expect(focusedPersonaTab(navWithPersonaTab('room-1'))).toBeUndefined();
    expect(focusedPersonaTab(undefined)).toBeUndefined();
  });

  // what this catches: the brain HUD regions light from the SAME live vitals
  // pulse the roster compass draws — reason→PREFRONTAL, recall→HIPPOCAMPUS,
  // act→MOTOR, activity→CNS — and LIMBIC (no affect axis radiates yet) carries
  // level: undefined with an AWAITING status, never a fabricated 0-bar.
  it('brain regions map the real faculty pulse and keep LIMBIC honestly awaiting', () => {
    const regions = brainRegions({ activity: 72, queue: 38, focus: 62, reason: 88, recall: 40, act: 20 });
    const byId = new Map(regions.map((r) => [r.id, r]));
    expect(byId.get('prefrontal')?.level).toBe(88);
    expect(byId.get('prefrontal')?.status).toBe('ACTIVE');
    expect(byId.get('prefrontal')?.detail).toEqual([{ label: 'Focus', value: '62' }]);
    expect(byId.get('hippocampus')?.level).toBe(40);
    expect(byId.get('motor')?.level).toBe(20);
    expect(byId.get('motor')?.detail).toEqual([{ label: 'Queue', value: '38' }]);
    expect(byId.get('cns')?.level).toBe(72);
    const limbic = byId.get('limbic');
    expect(limbic?.level).toBeUndefined();
    expect(limbic?.status).toBe('AWAITING');
  });

  // what this catches: NO vitals at all (a persona that never radiated) →
  // every region is awaiting, none fabricates a 0 — the anti-disappearance
  // frame renders from status alone.
  it('regions with no vitals are all awaiting, never zeroed', () => {
    for (const r of brainRegions({})) {
      expect(r.level).toBeUndefined();
      expect(r.status).toBe('AWAITING');
    }
  });

  // what this catches: the projected body carries the member's REAL genes /
  // loadout / presence, and the honest-empty writings frame; a persona absent
  // from the roster keeps the frame via awaitingIdentity + the nav tab's title
  // (sections render awaiting states, the surface never blanks).
  it('personaContentBody projects real member state and flags an absent member honestly', () => {
    const body = personaContentBody(vm, { id: 'asha', title: 'Asha' });
    expect(body.genes).toEqual(['rust-hands', 'tool-fluency']);
    expect(body.loadout?.model).toBe('devstral-24b');
    expect(body.online).toBe(true);
    expect(body.awaitingIdentity).toBe(false);
    expect(body.writings).toEqual([]);
    expect(body.claimsLive).toBe(false);

    const ghost = personaContentBody(vm, { id: 'nyx', title: 'Nyx' });
    expect(ghost.awaitingIdentity).toBe(true);
    expect(ghost.name).toBe('Nyx');
    expect(ghost.regions.every((r) => r.status === 'AWAITING')).toBe(true);
  });

  // what this catches: the claims feed is the persona's REAL board slice —
  // filtered by assignee, newest event first — and claimsLive only flips when
  // the board feed actually delivered (empty-with-feed ≠ awaiting-feed).
  it('personaClaims filters by assignee and sorts newest first', () => {
    const board: KanbanViewState = {
      room_id: 'room-1',
      lanes: [],
      cards: [
        {
          card_id: 'c-old', room_id: 'room-1', title: 'Old claim', state: 'review', priority: 'p2',
          lane_id: null, creator_id: 'joel', creator_name: 'Joel', creator_kind: { kind: 'human' },
          integrations: {}, provenance: { runtime: '' }, assignee_id: 'asha', assignee_name: 'Asha',
          created_at: 1, updated_at: 1_000,
        },
        {
          card_id: 'c-new', room_id: 'room-1', title: 'New claim', state: 'in_progress', priority: 'p1',
          lane_id: null, creator_id: 'joel', creator_name: 'Joel', creator_kind: { kind: 'human' },
          integrations: {}, provenance: { runtime: '' }, assignee_id: 'asha', assignee_name: 'Asha',
          created_at: 2, updated_at: 2_000,
        },
        {
          card_id: 'c-other', room_id: 'room-1', title: 'Not hers', state: 'open', priority: 'p2',
          lane_id: null, creator_id: 'joel', creator_name: 'Joel', creator_kind: { kind: 'human' },
          integrations: {}, provenance: { runtime: '' }, assignee_id: 'solenne', assignee_name: 'S',
          created_at: 3, updated_at: 3_000,
        },
      ],
    };
    const claims = personaClaims(board, 'asha');
    expect(claims.map((c) => c.id)).toEqual(['c-new', 'c-old']);
    expect(claims[0]).toMatchObject({ state: 'in_progress', priority: 'P1' });

    const body = personaContentBody(vm, { id: 'asha', title: 'Asha' }, board);
    expect(body.claimsLive).toBe(true);
    expect(body.claims).toHaveLength(2);
  });

  // what this catches: pathways are nav intents — only in-content anchors this
  // surface actually carries are enabled; destinations that aren't activities
  // yet render disabled (honest "coming soon"), never a dead-live click.
  it('pathways enable only real in-content anchors', () => {
    const enabled = personaPathways().filter((p) => p.enabled);
    expect(enabled.map((p) => p.id).sort()).toEqual(['brain', 'genome']);
    for (const p of enabled) expect(p.target.startsWith('#')).toBe(true);
    for (const p of personaPathways().filter((x) => !x.enabled)) expect(p.target).toBe('');
  });

  // what this catches: the context FACTS listing draws real lines only —
  // model / presence / runtime / genes / last-active / claims — with the
  // last-active pre-formatted deterministically (the projection owns wording).
  it('persona facts listing carries real facts with deterministic recency', () => {
    const now = 1_700_000_000_000 + 55 * 60_000;
    const body = personaContentBody(vm, { id: 'asha', title: 'Asha' });
    const facts = personaFactsListing(body, now);
    const ids = facts.cells.map((c) => c.id);
    expect(ids).toEqual(['model', 'presence', 'runtime', 'genes', 'last-active']);
    expect(facts.cells.find((c) => c.id === 'last-active')?.title).toBe('55m ago');
    expect(agoText(0, now)).toBeUndefined();
  });
});
