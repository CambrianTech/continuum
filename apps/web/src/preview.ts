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
import type {
  KanbanViewState,
  NavViewState,
  RosterSlotView,
  SystemMetricsViewState,
} from '@continuum/sdk-typescript';

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
  genes: [],
  ...over,
});

/** A rich, honest roster — real model loadouts + live cognition vitals — so the
 *  glass-box tile renders every element (compass, genome, LOADOUT strip) exactly
 *  as it would from a live `persona:vitals` fold. Not `?demo` fabrication: this
 *  is a named fixture, the design's reference input. */
const roster: RosterSlotView[] = [
  member({
    member_id: 'asha', display_name: 'Asha', provenance: { runtime: 'devstral' },
    // `activity` is the live radiator's always-on tempo key — the fixture carries it
    // so the preview exercises the SAME vocabulary `vitals_emitter` radiates.
    vitals: { activity: 72, queue: 38, focus: 62, reason: 88, recall: 40, act: 20, genome: 50, speed: 70, size: 40 },
    genes: ['rust-hands', 'tool-fluency', 'code-review'],
    loadout: { model: 'devstral-24b', params: 24_000_000_000, context_window: 32_768 },
    last_seen_ms: Date.now() - 55 * 60_000, // "55m ago" — the reference stamp
    // Exercises the avatar-image path (vite serves ~/.continuum/avatars at
    // /avatars); a machine without the file degrades to the glyph — honest.
    // The peer-id-keyed file (the LIVE emitter's path) — the older asha.png is
    // an unlit sceneless render (black face); scene-lit renders are card 80ef4131.
    avatar_url: '/avatars/90e758b2-3cf3-45c1-b100-de7c4ab5a549.png',
  }),
  member({
    member_id: 'solenne', display_name: 'Solenne', provenance: { runtime: 'qwen' },
    vitals: { activity: 40, queue: 12, focus: 30, reason: 45, recall: 80, act: 66, genome: 33, speed: 55, size: 55 },
    genes: ['web-design', 'tool-fluency'],
    loadout: { model: 'qwen3-coder-30b', params: 30_500_000_000, context_window: 262_144 },
    last_seen_ms: Date.now() - 3 * 60_000,
    avatar_url: '/avatars/luna.png',
  }),
  member({
    member_id: 'anwen', display_name: 'Anwen', provenance: { runtime: 'claude' },
    // An idle-but-resident persona: activity present at 0 → the tile still draws the
    // (empty) ACT bar, the always-visible readout the old INT/NRG/QUE row set.
    // An idle-but-resident persona: activity+queue present at 0 → the tile still
    // draws the (empty) ACT/QUE tracks — the reference's empty-QUE row; genome 0
    // with no genes → four dark equipment slots.
    vitals: { activity: 0, queue: 0, focus: 20, reason: 30, recall: 25, act: 90, genome: 0, speed: 88, size: 95 },
    loadout: { model: 'claude-opus-4-8', params: 671_000_000_000, context_window: 1_000_000 },
    last_seen_ms: Date.now() - 26 * 3_600_000, // "1d ago"
    avatar_url: '/avatars/sakurada.png',
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
  // The digest-tier reference input ([[perception-resolution-contract]]): the live
  // incident's shape — a degenerate repetition wall (hundreds of "ae0e-" lines)
  // between two normal messages. Renders collapsed (head + "… +N lines (M chars)
  // · mostly N× 'ae0e-'" + "show full message"), proving no message floods the
  // transcript while the conversation around it stays readable.
  flood: {
    kind: 'chat', revision: 1, room_id: 'general', room_name: 'general', purpose: 'chat',
    roster,
    messages: [
      message({ id: 'f1', content: 'Deploying the lane planner now — trace incoming.' }),
      message({
        id: 'f2', sender_id: 'solenne', sender_name: 'Solenne', provenance: { runtime: 'qwen' },
        content: ['lane admission trace follows:', ...Array<string>(240).fill('ae0e-')].join('\n'),
        timestamp: 1_700_000_060_000,
      }),
      message({
        id: 'f3', sender_id: 'anwen', sender_name: 'Anwen', provenance: { runtime: 'claude' },
        content: 'That trace looks degenerate — repetition brick fired, restarting the lane.',
        timestamp: 1_700_000_120_000,
      }),
    ],
  },
};

/** The persona-home reference input (`?fixture=persona`): the citizen's nav
 *  view focused on Asha's persona-kind tab + a work board with her claims —
 *  the SAME shapes the live substrate serves, so the persona surface renders
 *  every section (hero, brain HUD, genome shelf, claims, writings frame)
 *  exactly as it would from a live nav/select on her roster tile. */
const PERSONA_NAV: NavViewState = {
  user_id: 'joel',
  current_tab: 'asha',
  open_tabs: [
    { id: 'general', title: 'general', kind: 'room', unread: 0, purpose: 'chat' },
    { id: 'dev-updates', title: 'dev-updates', kind: 'room', unread: 3, purpose: 'chat' },
    { id: 'asha', title: 'Asha', kind: 'persona', unread: 0, purpose: 'persona' },
  ],
  last_read: { general: 1_700_000_060_000 },
  bookmarks: [],
};

const PERSONA_BOARD: KanbanViewState = {
  room_id: 'general',
  lanes: [],
  cards: [
    {
      card_id: 'c1', room_id: 'general', title: 'Wire the persona home claims feed',
      state: 'in_progress', priority: 'p1', lane_id: null,
      creator_id: 'joel', creator_name: 'Joel', creator_kind: { kind: 'human' },
      integrations: {}, provenance: { runtime: '' },
      assignee_id: 'asha', assignee_name: 'Asha',
      created_at: Date.now() - 26 * 3_600_000, updated_at: Date.now() - 40 * 60_000,
    },
    {
      card_id: 'c2', room_id: 'general', title: 'Review lane admission planner PR',
      state: 'review', priority: 'p2', lane_id: null,
      creator_id: 'solenne', creator_name: 'Solenne', creator_kind: { kind: 'agent' },
      integrations: {}, provenance: { runtime: 'qwen' },
      assignee_id: 'asha', assignee_name: 'Asha',
      created_at: Date.now() - 3 * 86_400_000, updated_at: Date.now() - 5 * 3_600_000,
    },
    {
      card_id: 'c3', room_id: 'general', title: 'A card owned by another citizen (must not show)',
      state: 'open', priority: 'p2', lane_id: null,
      creator_id: 'joel', creator_name: 'Joel', creator_kind: { kind: 'human' },
      integrations: {}, provenance: { runtime: '' },
      assignee_id: 'solenne', assignee_name: 'Solenne',
      created_at: Date.now() - 86_400_000, updated_at: Date.now() - 60_000,
    },
  ],
};

/** The citizen's nav view for the `rooms` fixture — the live room SET the rooms
 *  rail draws (brick 1): focused room + two more with unread. The same shape the
 *  per-user substrate serves under `kind="nav"`. */
const NAV_FIXTURES: Record<string, NavViewState> = {
  rooms: {
    user_id: 'joel',
    current_tab: 'general',
    open_tabs: [
      { id: 'general', title: 'general', kind: 'room', unread: 0, purpose: 'chat' },
      { id: 'dev-updates', title: 'dev-updates', kind: 'room', unread: 3, purpose: 'chat' },
      { id: 'foundry', title: 'foundry', kind: 'room', unread: 12, purpose: 'foundry' },
    ],
    last_read: { general: 1_700_000_060_000 },
    bookmarks: [],
  },
};

/** The SYS gauge fixture — a plausible 3-minute CPU/MEM window (deterministic
 *  waves, not random) so the sparkline + legend render their reference look. */
const SYS_FIXTURE: SystemMetricsViewState = {
  series: [
    {
      label: 'cpu',
      points: Array.from({ length: 90 }, (_, i) => 30 + 25 * Math.sin(i / 6) + (i % 7) * 2),
      current: '58%',
    },
    {
      label: 'mem',
      points: Array.from({ length: 90 }, (_, i) => 55 + 10 * Math.sin(i / 14)),
      current: '25.3/32G',
    },
  ],
  sample_interval_ms: 2000,
  node: 'bigmama.local',
};

function main(): void {
  const name = new URLSearchParams(location.search).get('fixture') ?? 'rooms';
  const state = FIXTURES[name] ?? FIXTURES.roster;

  const widget = document.createElement('chat-widget');
  widget.state = state;
  // Same real version stamp as the live entry — the preview is byte-faithful.
  widget.version = `v${__APP_VERSION__}`;
  // `?fixture=rooms` renders the roster state PLUS the nav room set and the SYS
  // gauge — the full left-rail reference input. Other fixtures leave the live
  // extras honest-absent.
  if (name === 'rooms') {
    widget.state = FIXTURES.roster;
    widget.nav = NAV_FIXTURES.rooms;
    widget.sys = SYS_FIXTURE;
  }
  // `?fixture=persona` — Asha's persona-kind tab focused: the persona HOME
  // renders in the center (hero + brain HUD + genome shelf + claims) while the
  // room state stays pinned underneath, exactly the live nav/select shape.
  if (name === 'persona') {
    widget.state = FIXTURES.roster;
    widget.nav = PERSONA_NAV;
    widget.sys = SYS_FIXTURE;
    widget.board = PERSONA_BOARD;
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
