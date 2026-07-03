/**
 * chatViewModel unit spec — the pure projection from a chat snapshot to the
 * three-panel view model (task #29, WIDGET-AS-STATE-KIND slice 4).
 *
 * Browser-free by design: the projection is a plain function, so these tests pin
 * the who/what/where mapping + the identity/time/presence derivations without a
 * DOM. The Lit template (`renderChat`) and `<chat-widget>` are thin over this, so
 * covering the view model covers the presentation logic.
 */

import { describe, it, expect } from 'vitest';
import { chatViewModel, formatTimeOfDay } from './chatViewModel';
import type { ChatState } from './ChatState';
import type { ChatMessageView, RosterSlotView, SenderKind } from '@continuum/sdk-typescript';

const kind = (k: SenderKind['kind']): SenderKind => ({ kind: k });

const member = (over: Partial<RosterSlotView> = {}): RosterSlotView => ({
  member_id: 'm-1',
  display_name: 'Asha',
  kind: kind('agent'),
  integrations: {},
  provenance: { runtime: '' },
  active: true,
  last_seen_ms: 0,
  ...over,
});

const message = (over: Partial<ChatMessageView> = {}): ChatMessageView => ({
  id: 'msg-1',
  room_id: 'room-1',
  sender_id: 's-1',
  sender_name: 'Joel',
  sender_kind: kind('human'),
  integrations: {},
  provenance: { runtime: '' },
  content: 'hello',
  timestamp: 0,
  ...over,
});

const state = (over: Partial<ChatState> = {}): ChatState => ({
  kind: 'chat',
  revision: 3,
  room_id: 'room-1',
  room_name: 'general',
  messages: [],
  roster: [],
  ...over,
});

describe('chatViewModel', () => {
  // what this catches: the three panels must project from one snapshot —
  // room→header (where), roster→members (who), messages→rows (what). A
  // regression that dropped a facet would blank a whole panel.
  it('projects room, roster, and messages from one snapshot', () => {
    const vm = chatViewModel(
      state({
        roster: [member({ member_id: 'a' }), member({ member_id: 'b', active: false })],
        messages: [message({ id: 'x' }), message({ id: 'y' })],
      }),
    );
    expect(vm.roomName).toBe('general');
    expect(vm.roomId).toBe('room-1');
    expect(vm.members.map((m) => m.id)).toEqual(['a', 'b']);
    expect(vm.messages.map((m) => m.id)).toEqual(['x', 'y']);
    expect(vm.revision).toBe(3);
  });

  // what this catches: activeCount counts only present members — it drives the
  // "N/M here" header and the presence dots. Counting all members (or none)
  // would misreport who is live in the room.
  it('counts active members separately from total', () => {
    const vm = chatViewModel(
      state({
        roster: [member({ active: true }), member({ active: false }), member({ active: true })],
      }),
    );
    expect(vm.memberCount).toBe(3);
    expect(vm.activeCount).toBe(2);
  });

  // what this catches: an empty message list must flag isEmpty so the surface
  // draws an honest empty state — not an error, not a blank void.
  it('flags an empty conversation', () => {
    expect(chatViewModel(state({ messages: [] })).isEmpty).toBe(true);
    expect(chatViewModel(state({ messages: [message()] })).isEmpty).toBe(false);
  });

  // what this catches: the row carries the substrate-resolved sender_name and
  // sender_kind verbatim — the renderer must NOT re-resolve identity. A
  // regression that recomputed the name would drift from the substrate truth.
  it('carries substrate-resolved sender identity onto the row', () => {
    const vm = chatViewModel(
      state({
        messages: [
          message({ sender_name: 'Asha', sender_kind: kind('agent'), provenance: { runtime: 'claude' } }),
        ],
      }),
    );
    expect(vm.messages[0]?.senderName).toBe('Asha');
    expect(vm.messages[0]?.kind).toBe('agent');
    expect(vm.messages[0]?.runtime).toBe('claude');
  });

  // what this catches: unresolved provenance ('') must stay empty so the
  // renderer can suppress the runtime badge — a fabricated origin would mislabel
  // who a citizen really is (an identity/trust concern, [[positron-identity-security-first-class]]).
  it('preserves an unresolved runtime as empty (no fabrication)', () => {
    const vm = chatViewModel(state({ roster: [member({ provenance: { runtime: '' } })] }));
    expect(vm.members[0]?.runtime).toBe('');
  });

  // what this catches: time-of-day formatting must be deterministic (UTC HH:MM,
  // zero-padded) so the view model is testable across machines/timezones.
  it('formats timestamps as zero-padded UTC HH:MM', () => {
    expect(formatTimeOfDay(0)).toBe('00:00');
    expect(formatTimeOfDay(9 * 3600_000 + 5 * 60_000)).toBe('09:05');
    expect(formatTimeOfDay(23 * 3600_000 + 59 * 60_000)).toBe('23:59');
  });
});
