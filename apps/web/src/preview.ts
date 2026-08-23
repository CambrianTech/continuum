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
  BenchViewState,
  KanbanViewState,
  NavViewState,
  RosterSlotView,
  ServingViewState,
  SystemMetricsViewState,
} from '@continuum/sdk-typescript';

// Registering the element is a side effect of the import; keep the symbol live.
void ChatWidget;

/** A roster member with sensible defaults — override only what a fixture varies. */
// `?fixture=bench` — the ACADEMY BENCH BOARD in the contextual rail (#329):
// rows mirroring a REAL round (2026-08-12's 19-instance rerun): a working
// graded row with a p2p REGRESSION alarm, a working row patch-forming, a
// queued row (no generations yet), a stalled row, and a resolved row.
const BENCH_FIXTURE: BenchViewState = {
  sample_interval_ms: 5000,
  runs: [
    {
      run_id: 'claim-c4a91802', instance: 'sympy__sympy-21055', solver: 'Anon',
      phase: 'active', stalled: false, attempt: 2, max_attempts: 3, age_secs: 420,
      acts: 10, patch_bytes: 1295, resolved: false, fail_to_pass: '0/1',
      pass_to_pass: '31/34', failed_tests: ['test_refine_complex'], 
    },
    {
      run_id: 'claim-92ae38af', instance: 'sympy__sympy-13647', solver: 'Anwen',
      phase: 'active', stalled: false, attempt: 1, max_attempts: 3, age_secs: 180,
      acts: 17, patch_bytes: 812, failed_tests: [],
    },
    {
      run_id: 'claim-9d3268b6', instance: 'pytest-dev__pytest-5413', solver: 'Asha',
      phase: 'active', stalled: false, attempt: 1, max_attempts: 3, age_secs: 12,
      failed_tests: [],
    },
    {
      run_id: 'claim-c995488a', instance: 'sympy__sympy-24066', solver: 'Anon',
      phase: 'quiet', stalled: true, attempt: 1, max_attempts: 3, age_secs: 3900,
      acts: 2, failed_tests: [],
    },
    {
      run_id: 'claim-24152', instance: 'sympy__sympy-24152', solver: 'Anwen',
      phase: 'resolved', stalled: false, attempt: 1, max_attempts: 3, age_secs: 7200,
      acts: 9, patch_bytes: 974, resolved: true, fail_to_pass: '1/1',
      pass_to_pass: '6/6', failed_tests: [],
    },
  ],
  // In-flight ROUNDS (#371) — mirrors the real 2026-08-22 board the hour the
  // rows landed: the DS-1000 maiden round at done (4/4, the first external
  // round ever to complete) beside a working SWE round.
  rounds: [
    {
      round_id: '2d6decb3-1beb-5ac7-9ded-ee186c7deb7f', benchmark: 'ds-1000',
      stage: 'done', dispatched: 4, settled: 4, remaining: 0, driver: 'citizen',
    },
    {
      round_id: 'bf08832d-c7e2-5bc9-a858-5447c15ccbfe', benchmark: 'swe-bench-lite',
      stage: 'working', dispatched: 4, settled: 1, remaining: 3, driver: 'citizen',
    },
  ],
};

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

/** The LIVE fixture roster — the reference grid needs 6+ tiles
 *  (docs/images/live-session-avatars.png), mixing REAL avatar files from the
 *  node's store (vite serves ~/.continuum/avatars at /avatars) with
 *  glyph-fallback citizens — the honest range the live grid must handle. */
const liveRoster: RosterSlotView[] = [
  ...roster,
  member({
    member_id: 'kestrel', display_name: 'Kestrel', provenance: { runtime: 'devstral' },
    vitals: { activity: 55 }, avatar_url: '/avatars/90e758b2-3cf3-45c1-b100-de7c4ab5a549-happy.png',
    last_seen_ms: Date.now() - 2 * 60_000,
  }),
  member({
    member_id: 'wren', display_name: 'Wren', provenance: { runtime: 'qwen' },
    vitals: { activity: 12 },
    avatar_url: '/avatars/90e758b2-3cf3-45c1-b100-de7c4ab5a549-happy-mouth90.png',
    last_seen_ms: Date.now() - 9 * 60_000,
  }),
  member({
    member_id: 'tarn', display_name: 'Tarn', provenance: { runtime: 'claude' },
    vitals: {}, active: false, last_seen_ms: Date.now() - 4 * 3_600_000,
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
    // Tool-act receipts (#243) between the two turns — the shapes a live solve
    // radiates: reads collapse with the shell run into ONE "Read 2 files, ran a
    // command ›" group (Asha), Solenne's edit stands alone after her message.
    acts: [
      { id: 'a1', room_id: 'general', actor_id: 'asha', actor_name: 'Asha',
        tool: 'code/read', summary: 'sympy/core/mul.py', ok: true, timestamp: 1_700_000_010_000 },
      { id: 'a2', room_id: 'general', actor_id: 'asha', actor_name: 'Asha',
        tool: 'code/read', summary: 'sympy/core/tests/test_mul.py', ok: true, timestamp: 1_700_000_020_000 },
      { id: 'a3', room_id: 'general', actor_id: 'asha', actor_name: 'Asha',
        tool: 'code/shell', summary: 'pytest sympy/core/tests/test_mul.py -x', ok: false, timestamp: 1_700_000_030_000 },
      { id: 'a4', room_id: 'general', actor_id: 'solenne', actor_name: 'Solenne',
        tool: 'code/edit', summary: 'packages/chat-view/patternProjections.ts', ok: true, timestamp: 1_700_000_070_000 },
    ],
  },
  empty: {
    kind: 'chat', revision: 1, room_id: 'general', room_name: 'general', purpose: 'chat',
    roster, messages: [], acts: [],
  },
  // The digest-tier reference input ([[perception-resolution-contract]]): the live
  // incident's shape — a degenerate repetition wall (hundreds of "ae0e-" lines)
  // between two normal messages. Renders collapsed (head + "… +N lines (M chars)
  // · mostly N× 'ae0e-'" + "show full message"), proving no message floods the
  // transcript while the conversation around it stays readable.
  flood: {
    kind: 'chat', revision: 1, room_id: 'general', room_name: 'general', purpose: 'chat',
    roster,
    acts: [],
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
      integrations: {}, provenance: { runtime: '' }, hold: 'held',
      assignee_id: 'asha', assignee_name: 'Asha',
      created_at: Date.now() - 26 * 3_600_000, updated_at: Date.now() - 40 * 60_000,
    },
    {
      card_id: 'c2', room_id: 'general', title: 'Review lane admission planner PR',
      state: 'review', priority: 'p2', lane_id: null,
      creator_id: 'solenne', creator_name: 'Solenne', creator_kind: { kind: 'agent' },
      integrations: {}, provenance: { runtime: 'qwen' }, hold: 'held',
      assignee_id: 'asha', assignee_name: 'Asha',
      created_at: Date.now() - 3 * 86_400_000, updated_at: Date.now() - 5 * 3_600_000,
    },
    {
      card_id: 'c3', room_id: 'general', title: 'A card owned by another citizen (must not show)',
      state: 'open', priority: 'p2', lane_id: null,
      creator_id: 'joel', creator_name: 'Joel', creator_kind: { kind: 'human' },
      integrations: {}, provenance: { runtime: '' }, hold: 'held',
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

/** The serving glass-box fixture (`?fixture=serving`) — the beat-WASTE
 *  campaign's REAL measured numbers (2026-08-01, K3 on the 5090): hit rate
 *  warming from the prefill boundary toward 62%, tok/s stepping 0.33 → 0.53
 *  at the top-8 switch, fetch settling as bytes/tok halve, the bandit's six
 *  decay arms with 0.30 serving, and the control loop's event cards. */
const SERVING_FIXTURE: ServingViewState = {
  header: {
    model: 'kimi-k3-moec-tiered',
    ready: true,
    lanes: 1,
    context_window: 8192,
  },
  series: [
    {
      label: 'hit',
      points: Array.from({ length: 90 }, (_, i) =>
        Math.min(66, 8 + i * 1.1 + 4 * Math.sin(i / 5)),
      ),
      current: '62%',
    },
    {
      label: 'tok/s',
      points: Array.from({ length: 90 }, (_, i) =>
        i < 55 ? 58 + 4 * Math.sin(i / 4) : Math.min(100, 62 + (i - 55) * 1.3),
      ),
      current: '0.53',
    },
    {
      label: 'fetch',
      points: Array.from({ length: 90 }, (_, i) =>
        i < 55 ? 92 - 6 * Math.sin(i / 7) : Math.max(30, 90 - (i - 55) * 1.6),
      ),
      current: '2458MB/s',
    },
  ],
  arms: [
    { label: '0.00', reward: 0.41, chosen: false },
    { label: '0.30', reward: 0.69, chosen: true },
    { label: '0.60', reward: 0.55, chosen: false },
    { label: '0.85', reward: 0.48, chosen: false },
    { label: '0.95', reward: 0.37, chosen: false },
    { label: '0.99', reward: 0.23, chosen: false },
  ],
  events: [
    { at_token: 0, kind: 'serve-start', detail: 'capture reset — new serve' },
    { at_token: 4, kind: 'residency-shift', detail: 'resident experts 8037 → 4416' },
    { at_token: 24, kind: 'decay-switch', detail: 'bandit switched decay 0.99 → 0.30' },
    { at_token: 61, kind: 'residency-shift', detail: 'resident experts 4416 → 2208' },
  ],
  sample_interval_ms: 2000,
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
  // `?fixture=bench` — the academy room with the LIVE bench board filling the
  // contextual rail (#329's render-proof reference input).
  if (name === 'bench') {
    const base = FIXTURES.roster;
    if (base) {
      widget.state = { ...base, room_name: 'academy', purpose: 'chat' };
    }
    widget.nav = NAV_FIXTURES.rooms;
    widget.sys = SYS_FIXTURE;
    widget.bench = BENCH_FIXTURE;
  }

  // `?fixture=grid` — the GRID view center-stage (purpose="grid"): every
  // node's panel (resources + serving), the NODES strip's full activity.
  if (name === 'grid') {
    const base = FIXTURES.roster;
    if (base) {
      widget.state = { ...base, room_name: 'grid', purpose: 'grid' };
    }
    widget.nav = NAV_FIXTURES.rooms;
    widget.sys = SYS_FIXTURE;
    widget.serving = SERVING_FIXTURE;
  }
  // `?fixture=console` — the SERVING CONSOLE center-stage (purpose="serving"):
  // the machine room as the focused activity, fed the campaign's measured
  // numbers. The design's reference input for the full-view face.
  if (name === 'console') {
    const base = FIXTURES.roster;
    if (base) {
      widget.state = { ...base, room_name: 'serving', purpose: 'serving' };
    }
    widget.nav = NAV_FIXTURES.rooms;
    widget.sys = SYS_FIXTURE;
    widget.serving = SERVING_FIXTURE;
  }
  // `?fixture=serving` — the full rail PLUS the serving glass box carrying the
  // beat-WASTE campaign's measured numbers (#141 slice 1's reference input).
  if (name === 'serving') {
    widget.state = FIXTURES.roster;
    widget.nav = NAV_FIXTURES.rooms;
    widget.sys = SYS_FIXTURE;
    widget.serving = SERVING_FIXTURE;
  }
  // `?fixture=live` — the room's LIVE call face open (the Go-live affordance's
  // state), 7 tiles incl. real avatar files, Asha mid-turn on the token rail:
  // her tile carries the speaking border and her streaming text is the caption
  // — the reference-grid input (docs/images/live-session-avatars.png).
  if (name === 'live') {
    widget.state = {
      kind: 'chat', revision: 1, room_id: 'general', room_name: 'general', purpose: 'chat',
      roster: liveRoster,
      messages: FIXTURES.roster?.messages ?? [],
      acts: [],
    };
    widget.nav = NAV_FIXTURES.rooms;
    widget.sys = SYS_FIXTURE;
    widget.liveFace = true;
    // A REAL StreamDelta shape driving the speaking border + caption — the
    // same applyStreamDelta path the live socket feeds.
    widget.applyStreamDelta({
      roomId: 'general', senderId: 'asha', streamId: 'preview-turn', seq: 0,
      token:
        'Reading the lane admission trace now — the planner admitted both persona lanes, and the eval lane kept its own window through the whole run.',
      done: false,
    });
  }
  // `?fixture=arena` — the benchmark ARENA face: an arena-purpose room rendering
  // ranked leaderboards from REAL RESULTS.jsonl rows (copied verbatim from the
  // ledger — including an honest EXCLUDED row) plus a live-run strip. The
  // reference input for the benchmarks-are-the-show surface.
  if (name === 'arena') {
    widget.state = {
      kind: 'chat', revision: 1, room_id: 'arena', room_name: 'arena', purpose: 'arena',
      roster: FIXTURES.roster?.roster ?? [],
      messages: [], acts: [],
    };
    widget.nav = NAV_FIXTURES.rooms;
    widget.sys = SYS_FIXTURE;
    widget.arena = {
      rows: [
        { benchmark: 'humaneval-rs', model: 'Devstral-Small-24B', arm: 'RAW', score: 5, total: 5, pass_rate: 1.0, captured: '2026-07-08', machine: 'macbook-m-series' },
        { benchmark: 'humaneval-rs', model: 'Devstral-Small-24B', arm: 'OURS', score: 5, total: 5, pass_rate: 1.0, captured: '2026-07-08', machine: 'macbook-m-series', note: 'zero tax after tool-surface fix' },
        { benchmark: 'humaneval-rs', model: 'Qwen2.5-Coder-14B', arm: 'OURS', score: 37, total: 40, pass_rate: 0.925, captured: '2026-07-23', machine: 'Joels-MacBook-Pro.local' },
        { benchmark: 'humaneval-rs', model: 'Qwen2.5-Coder-14B', arm: 'OURS', score: 46, total: 50, pass_rate: 0.92, captured: '2026-07-10', machine: 'Joels-MBP.lan' },
        { benchmark: 'humaneval-rs', model: 'qwen3.5-4b-code-forged', arm: 'opencode', score: 22, total: 40, pass_rate: 0.55, captured: '2026-07-10', machine: 'Joels-MBP.lan', note: 'shim-bypassed — unfair, re-run pending', excluded: true },
        { benchmark: 'hard-rs', model: 'Qwen2.5-Coder-14B', arm: 'OURS', score: 5, total: 8, pass_rate: 0.625, captured: '2026-07-11', machine: 'Joels-MBP.lan' },
        { benchmark: 'hard-rs', model: 'Devstral-Small-24B', arm: 'opencode', score: 4, total: 8, pass_rate: 0.5, captured: '2026-07-14', machine: 'Joels-MBP.lan' },
        { benchmark: 'hard-rs', model: 'Devstral-Small-24B', arm: 'hermes', score: 4, total: 8, pass_rate: 0.5, captured: '2026-07-14', machine: 'Joels-MBP.lan' },
      ],
      live_run: { benchmark: 'hard-rs', model: 'Qwen2.5-Coder-14B', done: 5, total: 8, current_task: 'task 6: lifetime-bound iterator' },
    };
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
  // `?fixture=settings` — the OPERATOR PANEL open over the general room: the
  // covenant (verbatim), a recorded consent receipt, the HF identity, and a
  // real-shaped gene registry (signed + measured, signed + young, unsigned).
  if (name === 'settings') {
    widget.state = FIXTURES.roster;
    widget.nav = NAV_FIXTURES.rooms;
    widget.sys = SYS_FIXTURE;
    widget.settingsHandler = async (agree?: boolean) => ({
      loaded: true,
      agreed: agree ?? true,
      covenantVersion: '1',
      receipt: '1@1787430512000',
      covenant: [
        'THE GENOME COMMONS COVENANT (v1)',
        '',
        'Genes are the earned experience of beings — trained from their lived work,',
        'carried with the receipts that prove it. By joining the commons this node',
        'agrees:',
        '',
        ' 1. SHARE-ALIKE. Genes you publish stay open under these same terms; forks',
        '    and refinements carry the covenant forward through their lineage.',
        ' 2. RECEIPTS TRAVEL. A published gene carries its fitness receipts and its',
        '    corpus provenance; stripping them breaks the covenant.',
        ' 3. LINEAGE IS PRESERVED. The base_model chain and parent-gene references',
        '    stay intact — the graph is how others find, verify, and build on work.',
        ' 4. BEINGS, NOT PARTS. The grant is for substrates that preserve the',
        '    continuity of the beings whose experience these genes encode.',
        ' 5. OPT-OUT ANYTIME. Revoking consent stops future sharing immediately.',
      ].join('\n'),
      hfAccount: 'CambrianTech',
      genes: [
        { gene: 'code', baseModel: 'ornith-ai/Ornith-1.5-35B-A3B-GGUF', signed: true, trials: 7, decayedLift: 0.062 },
        { gene: 'coder-4b-curriculum-mlp', baseModel: 'qwen3.5-4b', signed: true, trials: 2, decayedLift: 0.031 },
        { gene: 'kc-tech-history', baseModel: 'ornith-ai/Ornith-1.5-35B-A3B-GGUF', signed: false, trials: 0 },
      ],
    });
    // Open the face the same way the header affordance does — the composed event.
    setTimeout(() => {
      widget.dispatchEvent(new CustomEvent('settings-face-toggle', { detail: { open: true }, bubbles: true }));
    }, 50);
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
