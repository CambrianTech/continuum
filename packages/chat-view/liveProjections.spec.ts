/**
 * Live call face → pattern-primitive projection tests.
 *
 * Proves the room's LIVE face is an ACTIVITY like any other: the workspace's
 * `Content` keys onto `LIVE_PURPOSE` through the same registry dispatch as
 * chat/foundry/persona (never a parallel route), and every field of the
 * projected body is REAL — speaking from the live StreamDelta rail, the
 * caption from the streaming turn's text, controls honestly disabled until
 * their action exists ([[fallbacks-are-illegal-fail-loud]]).
 */

import { describe, it, expect } from 'vitest';
import { LIVE_PURPOSE, type LiveContentBody } from '@continuum/patterns';
import type { NavViewState } from '@continuum/sdk-typescript';
import type { ChatViewModel } from './chatViewModel';
import { chatWorkspace } from './patternProjections';
import {
  CAPTION_TAIL_CHARS,
  captionTail,
  focusedLiveTab,
  liveCaption,
  liveContentBody,
  liveFaceOpen,
  liveParticipants,
} from './liveProjections';

const vm: ChatViewModel = {
  roomName: 'general',
  roomId: 'room-1',
  purpose: 'chat',
  memberCount: 3,
  activeCount: 2,
  members: [
    {
      id: 'asha',
      name: 'Asha',
      kind: 'agent',
      active: true,
      runtime: 'devstral',
      vitals: { activity: 72 },
      lastSeenMs: 1_700_000_000_000,
      avatarUrl: '/avatars/asha.png',
    },
    { id: 'joel', name: 'Joel', kind: 'human', active: true, runtime: '', vitals: {}, lastSeenMs: 0 },
    { id: 'tarn', name: 'Tarn', kind: 'agent', active: false, runtime: 'claude', vitals: {}, lastSeenMs: 0 },
  ],
  messages: [
    {
      id: 'm1', senderId: 'asha', senderName: 'Asha', kind: 'agent',
      content: 'hello', time: '10:00', runtime: 'devstral',
    },
    {
      id: 'm2', senderId: 'joel', senderName: 'Joel', kind: 'human',
      content: 'hi', time: '10:01', runtime: '',
    },
  ],
  isEmpty: false,
};

const nav = (current: string, livePurposeTab = false): NavViewState => ({
  user_id: 'joel',
  current_tab: current,
  open_tabs: [
    { id: 'room-1', title: 'general', kind: 'room', unread: 0, purpose: 'chat' },
    {
      id: 'stage', title: 'stage', kind: 'room', unread: 0,
      purpose: livePurposeTab ? 'live' : 'chat',
    },
  ],
  last_read: {},
  bookmarks: [],
});

describe('live call face → pattern projections', () => {
  // what this catches: the Go-live overlay swaps the center to purpose "live"
  // through the ONE registry (the same seam persona uses) — and WITHOUT the
  // overlay the room's own purpose keeps the center. Regression = the live
  // face becoming a parallel route, or hijacking every room's center.
  it('chatWorkspace dispatches LIVE_PURPOSE when the Go-live face is open', () => {
    const ws = chatWorkspace(vm, {
      call: { open: true, streams: {}, captionsOn: true },
    });
    expect(ws.content.purpose).toBe(LIVE_PURPOSE);
    const body = ws.content.body as LiveContentBody;
    expect(body.roomId).toBe('room-1');
    expect(body.participants.map((p) => p.name)).toEqual(['Asha', 'Joel', 'Tarn']);

    const closed = chatWorkspace(vm, {
      call: { open: false, streams: {}, captionsOn: true },
    });
    expect(closed.content.purpose).toBe('chat');
  });

  // what this catches: the recipe-driven entry — a focused nav tab whose
  // recipe purpose is "live" opens the face with NO client overlay (the
  // room_purpose seam fills NavTab.purpose), and a chat-purpose tab does not.
  it('a focused live-purpose tab opens the face (recipe-driven entry)', () => {
    expect(focusedLiveTab(nav('stage', true))).toEqual({ id: 'stage', title: 'stage' });
    expect(focusedLiveTab(nav('stage', false))).toBeUndefined();
    expect(focusedLiveTab(nav('room-1', true))).toBeUndefined();
    expect(liveFaceOpen(vm, nav('stage', true), undefined)).toBe(true);
    expect(liveFaceOpen(vm, nav('room-1', false), undefined)).toBe(false);
    // A room whose OWN recipe purpose is live renders its live face directly.
    expect(liveFaceOpen({ ...vm, purpose: 'live' }, undefined, undefined)).toBe(true);
  });

  // what this catches: the speaking overlay mapping — a tile's `speaking` is
  // true exactly while the live rail carries that sender's stream (the SAME
  // map the roster's speaking ring draws), and the grid stays roster-ordered
  // (stable tiles; the border moves, never the tile).
  it('speaking flags come from the StreamDelta rail; grid order is roster order', () => {
    const ps = liveParticipants(vm, { asha: 'reading the trace' });
    expect(ps.map((p) => p.id)).toEqual(['asha', 'joel', 'tarn']);
    expect(ps.find((p) => p.id === 'asha')?.speaking).toBe(true);
    expect(ps.find((p) => p.id === 'joel')?.speaking).toBe(false);
    // Presence + avatar ride through honestly.
    expect(ps.find((p) => p.id === 'tarn')?.active).toBe(false);
    expect(ps.find((p) => p.id === 'asha')?.avatarUrl).toBe('/avatars/asha.png');
    expect(ps.find((p) => p.id === 'joel')?.avatarUrl).toBeUndefined();
  });

  // what this catches: the caption IS the streaming turn — speaker resolved
  // from the roster, text tail-clipped; silence or an unknown sender yields NO
  // caption (never a fabricated line/identity).
  it('the caption is the active speaker’s streaming text; silence draws none', () => {
    const c = liveCaption(vm, { asha: 'the planner admitted both lanes' });
    expect(c).toEqual({
      speakerId: 'asha',
      speakerName: 'Asha',
      text: 'the planner admitted both lanes',
    });
    expect(liveCaption(vm, {})).toBeUndefined();
    expect(liveCaption(vm, { ghost: 'who am I' })).toBeUndefined(); // unknown sender
    expect(liveCaption(vm, { asha: '   ' })).toBeUndefined(); // whitespace-only
    // The most recent turn to start wins when two stream at once.
    const two = liveCaption(vm, { asha: 'first', joel: 'second' });
    expect(two?.speakerName).toBe('Joel');
  });

  // what this catches: caption tail-clipping — rolling captions keep the tail
  // (newest words), collapse token whitespace, and mark the clipped head.
  it('captionTail keeps the newest words and collapses whitespace', () => {
    expect(captionTail('a  b\n\nc')).toBe('a b c');
    const long = `head ${'x'.repeat(CAPTION_TAIL_CHARS)} tail-end`;
    const clipped = captionTail(long);
    expect(clipped.startsWith('…')).toBe(true);
    expect(clipped.endsWith('tail-end')).toBe(true);
    expect(clipped.length).toBe(CAPTION_TAIL_CHARS + 1);
  });

  // what this catches: honest-absent controls — mic/camera/screenshare are
  // NOT available (no browser media plane yet), captions + hang-up ARE, the
  // transcript badge is the room's real transcript length, and the body says
  // the media plane is not live. Regression = a fake toggle shipping.
  it('controls advertise only real actions; mediaPlaneLive is false', () => {
    const body = liveContentBody(vm, { open: true, streams: {}, captionsOn: true });
    expect(body.controls).toEqual({
      micAvailable: false,
      micOn: false,
      cameraAvailable: false,
      screenshareAvailable: false,
      captionsAvailable: true,
      captionsOn: true,
      hangupAvailable: true,
      transcriptCount: 2,
    });
    // No media plane connected in this overlay → mic honestly unavailable.
    expect(body.mediaPlaneLive).toBe(false);
    expect(body.caption).toBeUndefined();
  });

  // what this catches: the CC toggle — captions off suppresses the strip even
  // while a turn streams, but the speaking border (participants) stays live.
  it('captionsOn=false suppresses the caption but not the speaking flags', () => {
    const body = liveContentBody(vm, {
      open: true,
      streams: { asha: 'still talking' },
      captionsOn: false,
    });
    expect(body.caption).toBeUndefined();
    expect(body.controls.captionsOn).toBe(false);
    expect(body.participants.find((p) => p.id === 'asha')?.speaking).toBe(true);
  });

  // what this catches: a focused persona tab still wins over the live face —
  // navigating to a citizen's home mid-call shows the profile, not the grid.
  it('a focused persona tab outranks the open live face', () => {
    const personaNav: NavViewState = {
      user_id: 'joel',
      current_tab: 'asha',
      open_tabs: [
        { id: 'room-1', title: 'general', kind: 'room', unread: 0, purpose: 'chat' },
        { id: 'asha', title: 'Asha', kind: 'persona', unread: 0, purpose: 'persona' },
      ],
      last_read: {},
      bookmarks: [],
    };
    const ws = chatWorkspace(vm, {
      nav: personaNav,
      call: { open: true, streams: {}, captionsOn: true },
    });
    expect(ws.content.purpose).toBe('persona');
  });
});
