/**
 * toMobileScreen spec — the mobile adaptation rule, verified without a simulator.
 *
 * The SAME chatApp WorkspaceView the desktop paints as a three-panel, the mobile rule
 * derives into a phone-native MobileScreen: conversation full-screen (primary), the roster
 * behind a bottom-nav tab (Who), each cell stripped to presence (no dossier badges/meters).
 * A Flutter/Swift/Kotlin painter renders MobileScreen → native widgets; this test proves the
 * RULE is right before any Dart is written. Pixels are the grid's last mile; the rule is here.
 */

import { describe, it, expect } from 'vitest';
import { toMobileScreen } from '@continuum/patterns';
import { chatApp } from './chatApp';
import type { ChatState } from './index';
import type { ChatMessageView, RosterSlotView, SenderKind } from '@continuum/sdk-typescript';

const kind = (k: SenderKind['kind']): SenderKind => ({ kind: k });
const member = (over: Partial<RosterSlotView> = {}): RosterSlotView => ({
  member_id: 'm-1', display_name: 'Asha', kind: kind('agent'), integrations: {},
  provenance: { runtime: 'persona' }, active: true, last_seen_ms: 0,
  vitals: { activity: 42 },
  genes: [],
  loadout: { model: 'devstral-24b', params: 24_000_000_000, context_window: 32_768 },
  ...over,
});
const message = (over: Partial<ChatMessageView> = {}): ChatMessageView => ({
  id: 'msg-1', room_id: 'room-1', sender_id: 's-1', sender_name: 'Asha',
  sender_kind: kind('agent'), integrations: {}, provenance: { runtime: 'persona' },
  content: 'working on vitals', timestamp: 0, ...over,
});
const chatState = (over: Partial<ChatState> = {}): ChatState => ({
  kind: 'chat', revision: 3, room_id: 'room-1', room_name: 'general',
  purpose: 'chat', messages: [], roster: [], ...over,
});

describe('toMobileScreen — the mobile adaptation rule', () => {
  // what this catches: mobile crammed as a shrunk desktop instead of a designed phone UX.
  // The rule must make the conversation primary, push the roster to a bottom-nav tab, and
  // DROP the per-cell dossier (badges/meters) a phone row can't afford — presence, not a
  // dossier. If the rule leaked the three-panel or kept the dossier, the native painter would
  // inherit a bad UX. The rule is verified here; the simulator only paints it.
  it('makes conversation primary, roster a Who tab, and drops the dossier', () => {
    const state = chatState({
      roster: [member({ member_id: 'a', display_name: 'Asha' })],
      messages: [message()],
    });
    const screen = toMobileScreen(chatApp.project(state));

    expect(screen.title).toBe('general'); // app bar = room
    expect(screen.primary.purpose).toBe('chat'); // conversation owns the screen
    // The rail's LISTING widgets become bottom-nav tabs (Rooms + Who); the metrics
    // widget is NOT a tab (a phone shows presence, not a dashboard in the nav).
    expect(screen.tabs.map((t) => t.id)).toEqual(['rooms', 'roster']);

    const rosterTab = screen.tabs.find((t) => t.id === 'roster');
    const rosterCell = rosterTab?.cells[0];
    expect(rosterCell?.title).toBe('Asha'); // presence kept
    expect(rosterCell?.status).toBeDefined();
    expect(rosterCell?.badges).toBeUndefined(); // dossier dropped
    expect(rosterCell?.meters).toBeUndefined(); // vitals meters dropped for the phone
    expect(rosterCell?.loadout).toBeUndefined(); // loadout strip dropped too (dossier)
  });
});
