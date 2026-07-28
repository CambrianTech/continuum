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
  vitals: {},
  genes: [],
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
  purpose: 'chat',
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

  // what this catches: live vitals (energy/attention/compute) must ride the
  // roster projection so the card can draw the genome-energy meters — and an
  // absent map (a human, a remote peer) OR an older core that omits the field
  // entirely both fold to {} (no meters), never fabricated bars.
  it('projects member vitals, defaulting an absent map to empty', () => {
    const vm = chatViewModel(
      state({
        roster: [
          member({ member_id: 'a', vitals: { energy: 80, attention: 90 } }),
          member({ member_id: 'b', vitals: {} }),
          member({ member_id: 'c', vitals: undefined as unknown as Record<string, number> }),
        ],
      }),
    );
    expect(vm.members[0]?.vitals).toEqual({ energy: 80, attention: 90 });
    expect(vm.members[1]?.vitals).toEqual({});
    expect(vm.members[2]?.vitals).toEqual({});
  });

  // what this catches: a member's LOADOUT (model · size · ctx) must project onto
  // the VM with the wire's snake_case `context_window` mapped to camel
  // `contextWindow`, dropping empty fields; a member with no loadout carries
  // `undefined` (the card draws no strip), never a fabricated model. This is the
  // "model size, context size" the glass-box tile surfaces.
  it('projects a member loadout, camel-casing the window and omitting when absent', () => {
    const vm = chatViewModel(
      state({
        roster: [
          member({
            member_id: 'a',
            loadout: { model: 'devstral-24b', params: 24_000_000_000, context_window: 32_768 },
          }),
          member({ member_id: 'b' }), // no loadout
          member({ member_id: 'c', loadout: { model: '', params: 0, context_window: 0 } }),
        ],
      }),
    );
    expect(vm.members[0]?.loadout).toEqual({
      model: 'devstral-24b',
      params: 24_000_000_000,
      contextWindow: 32_768,
    });
    expect(vm.members[1]?.loadout).toBeUndefined();
    // all-empty fields collapse to no loadout — honest absent, not an empty strip.
    expect(vm.members[2]?.loadout).toBeUndefined();
  });

  // what this catches: gene NAMES (the label half of the numeric `genome`
  // vital) project onto the VM only when the radiator reported any — an empty
  // list (base model) and an older core omitting the field both read as
  // absent, so the tile never draws fabricated gene labels.
  it('projects gene names, omitting when empty or unreported', () => {
    const vm = chatViewModel(
      state({
        roster: [
          member({ member_id: 'a', genes: ['rust-hands', 'tool-fluency'] }),
          member({ member_id: 'b', genes: [] }),
          member({ member_id: 'c' }),
        ],
      }),
    );
    expect(vm.members[0]?.genes).toEqual(['rust-hands', 'tool-fluency']);
    expect(vm.members[1]?.genes).toBeUndefined();
    expect(vm.members[2]?.genes).toBeUndefined();
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
