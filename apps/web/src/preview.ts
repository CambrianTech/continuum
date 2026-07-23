/**
 * `<chat-widget>` PREVIEW entry — the faithful, backend-free visual harness.
 *
 * WHY THIS EXISTS: iterating on the widget's look needs a render that is
 * byte-faithful to production — the REAL `theme.css` design tokens, the REAL
 * Vite/TS pipeline, the REAL element — fed a fixture instead of a live socket.
 * A hand-rolled esbuild page that forgets `theme.css` renders the widget with
 * every `--content-*` / `--surface` / `--border` token undefined and looks like
 * unstyled junk; this entry can't make that mistake because it IS the app
 * pipeline, minus the network. Open `/preview.html` under `npm run dev` to
 * iterate live; `npm run preview:shot` captures it headless to a PNG.
 *
 * The fixture is chosen by `?fixture=<name>` (default `roster`). Add a fixture =
 * add a `ChatState` to `FIXTURES` — no backend, no drift from the real view.
 */

import './theme.css';
import { ChatWidget, type SendHandler } from './chat/ChatWidget';
import type { ChatState } from '@continuum/chat-view';
import type { NavViewState, RosterSlotView } from '@continuum/sdk-typescript';

// Registering the element is a side effect of the import; keep the symbol live.
void ChatWidget;

/** A roster member with sensible defaults — override only what a fixture varies. */
const member = (over: Partial<RosterSlotView>): RosterSlotView => ({
  member_id: 'm',
  display_name: 'Member',
  kind: { kind: 'agent' },
  integrations: {},
  provenance: { runtime: 'persona' },
  active: true,
  last_seen_ms: 0,
  vitals: {},
  ...over,
});

/** A rich, honest roster — real model loadouts + live cognition vitals — so the
 *  glass-box tile renders every element (compass, genome, LOADOUT strip) exactly
 *  as it would from a live `persona:vitals` fold. Not `?demo` fabrication: this
 *  is a named fixture, the design's reference input. */
const roster: RosterSlotView[] = [
  member({
    member_id: 'asha', display_name: 'Asha', provenance: { runtime: 'devstral' },
    vitals: { focus: 62, reason: 88, recall: 40, act: 20, genome: 55, speed: 70, size: 40 },
    loadout: { model: 'devstral-24b', params: 24_000_000_000, context_window: 32_768 },
  }),
  member({
    member_id: 'solenne', display_name: 'Solenne', provenance: { runtime: 'qwen' },
    vitals: { focus: 30, reason: 45, recall: 80, act: 66, genome: 33, speed: 55, size: 55 },
    loadout: { model: 'qwen3-coder-30b', params: 30_500_000_000, context_window: 262_144 },
  }),
  member({
    member_id: 'anwen', display_name: 'Anwen', provenance: { runtime: 'claude' },
    vitals: { focus: 20, reason: 30, recall: 25, act: 90, genome: 0, speed: 88, size: 95 },
    loadout: { model: 'claude-opus-4-8', params: 671_000_000_000, context_window: 1_000_000 },
  }),
  member({
    member_id: 'joel', display_name: 'Joel', kind: { kind: 'human' },
    provenance: { runtime: '' }, vitals: {}, // a human: no vitals, no loadout — honest empty
  }),
];

const message = (over: Partial<ChatState['messages'][number]>): ChatState['messages'][number] => ({
  id: 'x', room_id: 'general', sender_id: 'asha', sender_name: 'Asha',
  sender_kind: { kind: 'agent' }, integrations: {}, provenance: { runtime: 'devstral' },
  content: 'hi', timestamp: 1_700_000_000_000, ...over,
});

const FIXTURES: Record<string, ChatState> = {
  roster: {
    kind: 'chat', revision: 1, room_id: 'general', room_name: 'general', purpose: 'chat',
    roster,
    messages: [
      message({ id: 'm1', content: 'On it — reading the roster seam now.' }),
      message({ id: 'm2', sender_id: 'solenne', sender_name: 'Solenne', provenance: { runtime: 'qwen' },
        content: 'I can take the projection side once the wire type lands.', timestamp: 1_700_000_060_000 }),
    ],
  },
  empty: {
    kind: 'chat', revision: 1, room_id: 'general', room_name: 'general', purpose: 'chat',
    roster, messages: [],
  },
};

/** The citizen's nav view for the `rooms` fixture — the live room SET the rooms
 *  rail draws (brick 1): focused room + two more with unread. The same shape the
 *  per-user substrate serves under `kind="nav"`. */
const NAV_FIXTURES: Record<string, NavViewState> = {
  rooms: {
    user_id: 'joel',
    current_tab: 'general',
    open_tabs: [
      { id: 'general', title: 'general', kind: 'room', unread: 0 },
      { id: 'dev-updates', title: 'dev-updates', kind: 'room', unread: 3 },
      { id: 'foundry', title: 'foundry', kind: 'room', unread: 12 },
    ],
    last_read: { general: 1_700_000_060_000 },
    bookmarks: [],
  },
};

function main(): void {
  const name = new URLSearchParams(location.search).get('fixture') ?? 'roster';
  const state = FIXTURES[name] ?? FIXTURES.roster;

  const widget = document.createElement('chat-widget');
  widget.state = state;
  // `?fixture=rooms` renders the roster state PLUS the nav room set — the
  // rooms-rail reference input. Other fixtures leave nav honest-absent.
  if (name === 'rooms') {
    widget.state = FIXTURES.roster;
    widget.nav = NAV_FIXTURES.rooms;
  }
  // A no-op send handler so the input area is live for interaction shots without a socket.
  const noop: SendHandler = async () => {
    /* no-op: the preview has no socket, so a submit goes nowhere */
  };
  widget.sendHandler = noop;

  const mount = document.getElementById('app') ?? document.body;
  mount.replaceChildren(widget);
}

main();
