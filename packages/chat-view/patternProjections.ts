/**
 * Chat → pattern-primitive projections.
 *
 * Proves ACTIVITY-ROOM-PATTERNS.md's thesis on the chat activity: the roster is not a
 * bespoke widget — it is the `Listing` primitive (`@continuum/patterns`). This maps
 * the chat view model's members into a `ListingView` (the people-Listing), so the same
 * rows render on any `RenderTarget`: web member-cards, an ANSI list, or a persona's
 * grounding block. The projection resolves the display fields (glyph, badges, status);
 * a target only draws them.
 */

import { HERE_NOW_LISTING_ID, WORKING_NOW_LISTING_ID, listingWidget } from '@continuum/patterns';
import type {
  ListingView,
  ListingCell,
  CellStatus,
  ContentView,
  ContinuonView,
  SystemPanelView,
  WorkspaceView,
  PanelWidget,
  MetricsView,
} from '@continuum/patterns';
import type { GaugeView, ServingPanelView } from '@continuum/patterns';
import type {
  BenchViewState,
  KanbanViewState,
  NavTab,
  NavViewState,
  ServingViewState,
  SystemMetricsViewState,
} from '@continuum/sdk-typescript';
import type {
  ChatViewModel,
  MemberKind,
  MessageRowVM,
  RosterMemberVM,
  TranscriptRowVM,
} from './chatViewModel';
import { ARENA_PURPOSE, BENCH_PURPOSE, CANVAS_PURPOSE, GRID_PURPOSE, LIVE_PURPOSE, PERSONA_PURPOSE, SERVING_PURPOSE, SETTINGS_PURPOSE, contentFamilyOf, type ArenaContentBody as ArenaContentBodyT, type BenchContentBody, type CanvasContentBody, type GridContentBody, type GridNodeVM, type ServingContentBody, type ServingNodeVM, type SettingsContentBody } from '@continuum/patterns';
import type { LiveContentBody, PersonaContentBody } from '@continuum/patterns';
import {
  focusedPersonaTab,
  personaContentBody,
  personaFactsListing,
} from './personaProjections';
import { liveContentBody, liveFaceOpen, type LiveCallOverlay } from './liveProjections';
import { benchContentBody } from './benchProjections';
import { arenaContentBody, type ArenaViewState } from './arenaProjections';
import { canvasContentBody, type CanvasViewState } from './canvasProjections';

/** Leading glyph per member kind — the neutral human/agent/system discriminant, as a
 *  display token the Listing carries (targets draw it, they don't re-derive it). */
function kindGlyph(kind: MemberKind): string {
  switch (kind) {
    case 'human':
      return '🧑';
    case 'agent':
      return '🤖';
    case 'system':
      return '⚙️';
  }
}

/** One roster member → a `Listing` cell (the people-Listing cell template). The
 *  member's genome-energy vitals ride along as neutral cell `meters` so a target draws
 *  the ACT bars from the Listing alone — the projection is LOSSLESS, no rich view-model
 *  crosses the render boundary. */
function rosterCell(m: RosterMemberVM): ListingCell {
  const badges = m.runtime ? [m.kind, m.runtime] : [m.kind];
  const status: CellStatus = m.active ? 'active' : 'idle';
  let cell: ListingCell = { id: m.id, title: m.name, glyph: kindGlyph(m.kind), badges, status };
  // Vitals (0–100 meters), loadout (model·size·ctx labels), and recency all ride the
  // neutral cell so the projection stays LOSSLESS — a target draws them from the
  // Listing alone, no rich view-model crosses the render boundary. Each attaches
  // only when present.
  if (Object.keys(m.vitals).length > 0) cell = { ...cell, meters: m.vitals };
  if (m.loadout) cell = { ...cell, loadout: m.loadout };
  if (m.lastSeenMs > 0) cell = { ...cell, lastActiveMs: m.lastSeenMs };
  if (m.avatarUrl) cell = { ...cell, image: m.avatarUrl };
  if (m.genes && m.genes.length > 0) cell = { ...cell, genes: m.genes };
  // Identity card (#262): pronouns · role as the one-line subtitle, bio as
  // the prose detail — each only when the member actually published a card.
  const idLine = [m.pronouns, m.roleLabel].filter(Boolean).join(' · ');
  if (idLine) cell = { ...cell, subtitle: idLine };
  if (m.bio) cell = { ...cell, detail: m.bio };
  return cell;
}

/** Live work summary for one member off the kanban board — the roster row's
 *  who-is-working-what fact (peer visibility for humans; the same projection
 *  later feeds citizen perception). Badges only when there IS work: `⚒ N`
 *  (live-held cards), the hottest priority when any is P0/P1, and `lapsed`
 *  when every claim's lease expired (takeable, not busy — the 2026-08-06
 *  six-citizens-stalled distinction). */
function workBadges(board: KanbanViewState | undefined, memberId: string): string[] {
  if (!board) return [];
  const mine = board.cards.filter(
    (c) => c.assignee_id === memberId && c.state !== 'merged' && c.state !== 'closed',
  );
  if (mine.length === 0) return [];
  const held = mine.filter((c) => c.hold === 'held');
  const badges: string[] = [`⚒ ${held.length > 0 ? held.length : mine.length}`];
  const hot = mine.some((c) => c.priority === 'p0')
    ? 'P0'
    : mine.some((c) => c.priority === 'p1')
      ? 'P1'
      : undefined;
  if (hot) badges.push(hot);
  if (held.length === 0) badges.push('lapsed');
  return badges;
}

/** The chat activity's `who` panel projected as the `Listing` primitive. Same shape
 *  the rooms/DMs list and Foundry's model list use — one primitive, different data.
 *  With the board feed attached, each row also carries its live work badges. */
export function rosterListing(vm: ChatViewModel, board?: KanbanViewState): ListingView {
  return {
    id: 'roster',
    title: 'Users & Agents',
    cells: vm.members.map((m) => {
      const work = workBadges(board, m.id);
      const cell = rosterCell(m);
      return work.length > 0 ? { ...cell, badges: [...(cell.badges ?? []), ...work] } : cell;
    }),
  };
}

/** The nav `Listing` — the rooms-Listing that is the tab bar for a human and the
 *  channel-attention set for a persona (one nav primitive over room-space,
 *  ACTIVITY-ROOM-PATTERNS.md). Today the client holds one focused room, so this is a
 *  single active cell; when the client tracks multiple rooms/DMs it fills out with no
 *  shape change (that is the point of `activity == room == tab`). */
export function roomsListing(vm: ChatViewModel): ListingView {
  return {
    id: 'rooms',
    title: 'Activities',
    cells: [{ id: vm.roomId, title: vm.roomName, status: 'active', group: vm.purpose }],
  };
}

/** The rooms-Listing from the citizen's live `kind="nav"` view — the room SET
 *  (POSITRON-WIDGET-SOPHISTICATION.md brick 1), superseding the single-cell
 *  `roomsListing` whenever the nav subscription has delivered. Each open tab is a
 *  cell: the focused room (the one the chat view is showing) draws active, unread
 *  rides the neutral `count` (a badge pill on web, `(3 new)` in RAG), and the
 *  tab's target kind is the `group` facet — the All/Rooms/DMs filter is a facet
 *  over groups, not a new widget. */
/** A run room's rail label from the bench view — what the round IS, not its
 *  raw name: `verified · working · 9/12 in hands`, `mini · paused · 1/4 settled`.
 *  Matched by the round's run room name; a tab without a round keeps its name. */
export function benchRoomLabel(
  tab: { readonly title: string },
  bench: BenchViewState | undefined,
): { readonly title: string; readonly subtitle: string } | undefined {
  const round = bench?.rounds.find((r) => r.run_room !== '' && r.run_room === tab.title);
  if (round === undefined) return undefined;
  const suite = round.benchmark.replace(/^swe-bench-/, '').replace(/-/g, ' ');
  const held = round.cards.filter((c) => c.owner !== '' && c.board_state !== 'closed').length;
  const stage = round.stage.toLowerCase();
  const progress =
    stage === 'working' ? `${held}/${round.dispatched} in hands` : `${round.settled}/${round.dispatched} settled`;
  return { title: `${suite} · ${stage}`, subtitle: progress };
}

export function roomsListingFromNav(
  nav: NavViewState,
  focusedRoomId: string,
  bench?: BenchViewState,
): ListingView {
  // Working rounds lead, finished/paused ones trail; everything else keeps the
  // nav's own order. A view choice over one truth — never a hidden row.
  const rank = (tab: NavTab): number => {
    const label = benchRoomLabel(tab, bench);
    if (label === undefined) return 1;
    return label.title.endsWith('working') ? 0 : 2;
  };
  const ordered = [...nav.open_tabs].sort((a, b) => rank(a) - rank(b));
  return {
    id: 'rooms',
    title: 'Activities',
    cells: ordered.map((tab): ListingCell => {
      const bl = benchRoomLabel(tab, bench);
      const cell: ListingCell = {
        id: tab.id,
        // A child activity draws its LINEAGE label (`<instance> · <card>`), not
        // the raw room name — the rail-tree IA (#2632 slice b); a bench run room
        // draws what the round is.
        title: bl ? bl.title : tab.display_label !== '' ? tab.display_label : tab.title,
        status: tab.id === focusedRoomId ? 'active' : 'idle',
        group: tab.kind,
        // The strip's membership: opened by the citizen (nav truth), never
        // "every room the daemon knows" (49 identical tabs, live 2026-09-03).
        ...(tab.opened || tab.id === focusedRoomId ? { opened: true } : {}),
        ...(tab.parent_ref !== '' ? { parent: tab.parent_ref } : {}),
        // The room's recipe-defined activity purpose, carried verbatim as the
        // description line ([[room-purpose-is-per-recipe-not-an-enum]]).
        // Empty = unresolved — no subtitle drawn, never a fabricated blurb.
        ...(bl ? { subtitle: bl.subtitle } : tab.purpose ? { subtitle: tab.purpose } : {}),
      };
      return tab.unread > 0 ? { ...cell, count: tab.unread } : cell;
    }),
  };
}

/** Digest one message row into a ticker line: `sender: head…` — truncated hard so
 *  the header ticker stays a glanceable log strip, never a second transcript. */
function tickerLine(msg: MessageRowVM, max = 34): string {
  const head = msg.content.replace(/\s+/g, ' ').trim();
  const clipped = head.length > max ? `${head.slice(0, max - 1)}…` : head;
  return `${msg.senderName}: ${clipped}`;
}

/** The `continuon` rail header — the wordmark + breathing status mark + a compact
 *  live ticker of the room's latest turns (the old header's tiny scrolling log,
 *  reborn from data the chat state already carries — no new pipe, no fabrication).
 *  `version` is threaded from the host (a real manifest/build stamp) and honestly
 *  absent until it is. */
export function continuonWidget(
  vm: ChatViewModel,
  version?: string,
  feed?: string,
): PanelWidget<ContinuonView> {
  const body: ContinuonView = {
    wordmark: 'continuum',
    tagline: 'ai workforce construction',
    ...(version ? { version } : {}),
    // Newest last, last three turns — the ticker reads bottom-fresh like a log tail.
    ticker: vm.messages.slice(-3).map((m) => tickerLine(m)),
    alive: vm.members.some((m) => m.active),
    // The connection status rides the continuon (with the favicon), never a
    // text banner — the orb is the status channel by design.
    ...(feed ? { feed } : {}),
  };
  return { id: 'continuon', kind: 'continuon', title: 'Continuum', body, scope: 'global' };
}

/** The `AI Performance` rail widget — the room's LIVE team-cognition readout, derived
 *  from the roster's own vitals (no fabricated numbers, no extra pipe): how many are
 *  here, how many are actively thinking (a faculty firing on the live compass), and how
 *  many carry a paged-in genome. A persona whose Reason/Act/Focus lit this tick counts
 *  as thinking — the same signal the tile's compass draws, aggregated. (System resource
 *  + spend metrics — CPU/GPU/$ — are a separate core feed; this ships the honest slice.) */
export function metricsWidget(vm: ChatViewModel): PanelWidget<MetricsView> {
  const agents = vm.members.filter((m) => m.kind === 'agent');
  const here = vm.members.filter((m) => m.active).length;
  const thinking = agents.filter((m) => {
    const v = m.vitals;
    return (v.reason ?? 0) > 40 || (v.act ?? 0) > 40 || (v.focus ?? 0) > 40 || (v.recall ?? 0) > 40;
  }).length;
  const genomes = agents.filter((m) => (m.vitals.genome ?? 0) > 0).length;
  const metrics: MetricsView = {
    stats: [
      { label: 'here', value: String(here), tone: 'accent' },
      { label: 'thinking', value: String(thinking), tone: thinking > 0 ? 'good' : 'muted' },
      { label: 'genome', value: String(genomes), tone: genomes > 0 ? 'good' : 'muted' },
    ],
  };
  return { id: 'metrics', kind: 'metrics', title: 'AI Performance', body: metrics, scope: 'global' };
}

/** The SYS gauge rail widget — the node's live CPU/MEM window (brick 2), a pure
 *  reshaping of the core-carried `kind="system-metrics"` view onto the neutral
 *  `GaugeView`: labels uppercased for the legend, points passed through
 *  losslessly. The core owns sampling, normalization, and formatting; here we
 *  only adapt the wire shape to the widget vocabulary. */
export function systemGaugeWidget(sys: SystemMetricsViewState): PanelWidget<GaugeView> {
  const gauge: GaugeView = {
    series: sys.series.map((s) => ({
      label: s.label.toUpperCase(),
      points: s.points,
      current: s.current,
    })),
    sampleIntervalMs: sys.sample_interval_ms,
  };
  return { id: 'sys-gauge', kind: 'gauge', title: 'System', body: gauge, scope: 'global' };
}

/** The SERVING glass-box rail widget (#141 slice 1: the beat-WASTE control
 *  loop on screen) — a pure reshaping of the core-carried `kind="serving"`
 *  view onto the neutral `ServingPanelView`. The core owns sampling,
 *  normalization, formatting, and event carding; here we only adapt wire
 *  shape to widget vocabulary. `undefined` before the feed delivers — the
 *  widget honestly joins the rail when the core starts publishing. */
export function servingWidget(serving?: ServingViewState): PanelWidget<ServingPanelView> | undefined {
  if (!serving) return undefined;
  const body: ServingPanelView = {
    ...(serving.header
      ? {
          header: {
            ...(serving.header.model !== undefined ? { model: serving.header.model } : {}),
            ready: serving.header.ready,
            lanes: serving.header.lanes,
            contextWindow: serving.header.context_window,
            ...(serving.header.degraded_reason !== undefined
              ? { degradedReason: serving.header.degraded_reason }
              : {}),
          },
        }
      : {}),
    ...(serving.series.length > 0
      ? {
          gauge: {
            series: serving.series.map((s) => ({
              label: s.label.toUpperCase(),
              points: s.points,
              current: s.current,
            })),
            sampleIntervalMs: serving.sample_interval_ms,
          },
        }
      : {}),
    arms: serving.arms.map((a) => ({ label: a.label, reward: a.reward, chosen: a.chosen })),
    events: serving.events.map((e) => ({ atToken: e.at_token, kind: e.kind, detail: e.detail })),
  };
  return { id: 'serving', kind: 'serving', title: 'Serving', body, scope: 'global' };
}

/** The serving CONSOLE content body — per-node panels for the center-stage
 *  ops face (`purpose === SERVING_PURPOSE`). Today: the local node (named
 *  from the metrics feed's host name); grid peers join as the cross-grid
 *  serving feed lands (#283) with zero shape change. `feedLive` is true only
 *  when the serving subscription has actually delivered. */
export function servingContentBody(
  serving?: ServingViewState,
  node?: string,
): ServingContentBody {
  const nodes: ServingNodeVM[] = serving
    ? [
        {
          node: node ?? 'this node',
          local: true,
          view: servingWidget(serving)?.body ?? { arms: [], events: [] },
        },
      ]
    : [];
  return { nodes, feedLive: serving !== undefined };
}

/** The GRID content body — every node's full panel for the center-stage
 *  SCADA view (`purpose === GRID_PURPOSE`; the NODES strip is its portal).
 *  Today: the local node with its resource window + serving loop; grid
 *  peers join as attestation/cross-grid feeds land (#257/#283) with zero
 *  shape change. */
export function gridContentBody(
  sys?: SystemMetricsViewState,
  serving?: ServingViewState,
): GridContentBody {
  const any = sys !== undefined || serving !== undefined;
  const nodes: GridNodeVM[] = any
    ? [
        {
          node: sys?.node ?? 'this node',
          local: true,
          ...(sys ? { resources: systemGaugeWidget(sys).body } : {}),
          ...(serving ? { serving: servingWidget(serving)?.body ?? undefined } : {}),
        },
      ]
    : [];
  return { nodes, feedLive: any };
}

/** The NODES strip (the factory sidebar's "1/1 nodes online"): every grid node
 *  this surface can honestly attest, as a `status` widget whose body is the one
 *  `Listing` primitive. Today that is exactly THIS node — attested by its live
 *  `kind="system-metrics"` feed carrying the OS-reported host name; connected
 *  peers join as cells when a peer-presence feed exists to attest them.
 *  `undefined` (no feed / no host name) = no strip — honest, never a
 *  fabricated "1/1 online". */
export function nodesWidget(sys?: SystemMetricsViewState): PanelWidget<ListingView> | undefined {
  if (!sys?.node) return undefined;
  const listing: ListingView = {
    id: 'nodes',
    title: 'Nodes',
    cells: [{ id: 'local', title: sys.node, subtitle: 'this node', status: 'active' }],
  };
  return { id: 'nodes', kind: 'status', title: 'Nodes', body: listing, scope: 'global' };
}

/** The TWO-FACED system panel (the old sidebar's SYS|AI header): one widget
 *  carrying BOTH the node's resource gauge (SYS face, from the live
 *  `kind="system-metrics"` view when delivered) and the team-cognition stats
 *  (AI face, derived from the roster's own vitals). A target draws a real
 *  toggle between the faces; which face shows is renderer state. Composes the
 *  two existing projections — no new numbers, no new pipe. */
export function systemPanelWidget(
  vm: ChatViewModel,
  sys?: SystemMetricsViewState,
  serving?: ServingViewState,
): PanelWidget<SystemPanelView> {
  const body: SystemPanelView = {
    ...(sys ? { gauge: systemGaugeWidget(sys).body } : {}),
    stats: metricsWidget(vm).body,
    // The HUD's SRV face — compact summary, portal to the serving console
    // activity (the FULL view). One graph control on the left, per the
    // console doctrine; faces cycle or pin in the renderer.
    ...(serving ? { serving: servingWidget(serving)?.body ?? undefined } : {}),
  };
  return { id: 'system', kind: 'system', title: 'System', body, scope: 'global' };
}

/** The chat room's contextual panel listing — room facts + participants summary
 *  (the right rail's "Room info" card), every line derived from state the
 *  surface already holds: purpose, presence counts, and the agent/human mix.
 *  No created-at line — the substrate doesn't carry a room birth timestamp yet,
 *  so none is drawn (honest-absent, never a fabricated date). */
export function roomInfoListing(vm: ChatViewModel): ListingView {
  const agents = vm.members.filter((m) => m.kind === 'agent').length;
  const humans = vm.members.filter((m) => m.kind === 'human').length;
  return {
    id: 'room-info',
    title: 'Room',
    cells: [
      { id: 'purpose', title: vm.purpose, subtitle: 'purpose' },
      {
        id: 'presence',
        title: `${vm.memberCount} members · ${vm.activeCount} active`,
        subtitle: 'presence',
      },
      { id: 'mix', title: `${agents} agents · ${humans} humans`, subtitle: 'participants' },
    ],
  };
}

/** The live extras a host wires in beside the chat snapshot — each optional and
 *  independent, each honestly absent until its subscription delivers (never a
 *  fabricated placeholder). One options object, not a growing positional list
 *  ([[structs-by-reference-not-massive-param-lists]]). */
export interface WorkspaceLive {
  /** Every member the surface has ever seen, node-wide (each room's roster
   *  folded in as it arrives) — so a persona page opened from a room she is
   *  NOT in still finds her presence and vitals. The focused room's roster is
   *  tried first; this is the fallback, never a second truth. */
  readonly directory?: readonly RosterMemberVM[];
  /** The citizen's `kind="nav"` view — upgrades the rooms rail to the room set. */
  readonly nav?: NavViewState;
  /** The node's `kind="system-metrics"` view — adds the SYS gauge widget. */
  readonly sys?: SystemMetricsViewState;
  /** The node's `kind="serving"` view — adds the serving glass-box widget
   *  (#141 slice 1). Honestly absent until the subscription delivers. */
  readonly serving?: ServingViewState;
  /** The node's `kind="kanban"` work board — feeds the persona home's claims
   *  feed (cards by assignee). Honestly absent until the subscription delivers. */
  readonly board?: KanbanViewState;
  /** The client build's version string (a real manifest/build stamp — e.g. the web
   *  app's package version). Drives the continuon header's version badge; honestly
   *  absent when the host has none to report. */
  readonly version?: string;
  /** The state feed's connection status — drives the continuon orb + favicon
   *  (the designed status channel). Absent = unknown/connecting. */
  readonly feed?: string;
  /** The widget-owned live-call overlay (Go-live face state + the StreamDelta
   *  token rail + captions toggle) — renderer state threaded through so the
   *  live face projects from REAL signals. Absent = no live face requested. */
  readonly call?: LiveCallOverlay;
  /** The widget-owned SETTINGS overlay: `open` = the header's Settings
   *  affordance is active; `body` = the fetched covenant/HF/registry state
   *  (absent while the fetch is in flight — the face renders its awaiting
   *  frame). The SAME core verbs the terminal uses feed and mutate it. */
  readonly settings?: { readonly open: boolean; readonly body?: SettingsContentBody };
  /** The `kind="arena"` eval-ledger view — feeds an arena-purpose room's
   *  leaderboards. Honestly absent until the feed delivers. */
  readonly arena?: ArenaViewState;
  /** The node's `kind="bench"` benchmark board (#329) — adds the academy
   *  right-rail bench widget. Honestly absent until the feed delivers. */
  readonly bench?: BenchViewState;
  /** The `kind="canvas"` design-bench observation feed — feeds a
   *  canvas-purpose run room's live artifact render (DESIGN-BENCH-VISUAL-
   *  CRAFT.md §5). Honestly absent until the feed delivers. */
  readonly canvas?: CanvasViewState;
}

/** The chat activity's `Content` body — the conversation. `Content` is keyed by the
 *  room's `purpose` (here `vm.purpose`, `"chat"`), so a target's registered chat
 *  renderer draws these rows; a foundry room would carry a different purpose + body. */
/** The ACADEMY LANDING's content — the campus page, not a chat log (Joel,
 *  2026-08-30: "how a main academy page should look, maybe more of a
 *  landing?"). The live board is the hero; the room's own chat rides below
 *  as a disclosure. Counts feed the hero strip. */
export const ACADEMY_PURPOSE = 'academy';
export interface AcademyContentBody {
  readonly bench: BenchContentBody;
  readonly chat: ChatContentBody;
  readonly memberCount: number;
  readonly activeCount: number;
}

export interface ChatContentBody {
  readonly messages: readonly MessageRowVM[];
  /** The full interleaved transcript (speech + collapsed act receipts, #243) —
   *  what the center pane draws. `messages` stays for speech-only consumers. */
  readonly transcript: readonly TranscriptRowVM[];
  readonly isEmpty: boolean;
}

/** The whole chat room as a `Workspace` — nav (rooms) + left (people) + content
 *  (the conversation, dispatched by `purpose`) + an empty context panel. This is the
 *  data spine a `RenderTarget` draws; every activity projects its own `Workspace` the
 *  same way, so the shell is identical and only content/context vary.
 *
 *  `live` carries the optional live extras (nav → the real room set with unread;
 *  sys → the SYS gauge). Each is honestly absent until its subscription
 *  delivers — the fallback is LESS data honestly shown, never invented data
 *  ([[fallbacks-are-illegal-fail-loud]]). */
export function chatWorkspace(vm: ChatViewModel, live?: WorkspaceLive): WorkspaceView {
  // Content dispatch keys off the FOCUSED TAB'S KIND (tabs==rooms==activities):
  // a persona-kind current tab renders the persona HOME (purpose "persona")
  // while the chat projection stays pinned to the room underneath — the same
  // registry dispatch as chat/foundry, never a parallel route. No persona tab →
  // the room's own purpose-keyed content, unchanged.
  // The SETTINGS face wins outright while open — the operator asked for the
  // panel; every other face resumes on close.
  const settingsBody: SettingsContentBody | undefined = live?.settings?.open
    ? live.settings.body ?? {
        loaded: false,
        agreed: false,
        covenantVersion: '',
        covenant: '',
        genes: [],
      }
    : undefined;
  const persona = focusedPersonaTab(live?.nav);
  const personaBody = persona
    ? personaContentBody(vm, persona, live?.board, live?.directory)
    : undefined;
  // The LIVE face ([[LIVE_PURPOSE]]): a room's call grid, dispatched through the
  // SAME registry when the room's recipe purpose is "live", a live-purpose tab
  // is focused, or the reader opened the Go-live face — honest entries only.
  // A focused persona tab still wins (the citizen navigated away from the room).
  const liveBody: LiveContentBody | undefined =
    !personaBody &&
    (liveFaceOpen(vm, live?.nav, live?.call) || contentFamilyOf(vm.purpose) === LIVE_PURPOSE)
      ? liveContentBody(vm, live?.call)
      : undefined;
  // The ARENA face: a room whose recipe purpose is "arena" renders ranked
  // leaderboards from the eval-ledger feed — same registry dispatch, and the
  // frame renders (awaiting rows) even before the feed delivers.
  const arenaBody =
    !personaBody && !liveBody && vm.purpose === ARENA_PURPOSE
      ? arenaContentBody(live?.arena ?? { rows: [] }, live?.arena !== undefined)
      : undefined;
  // The SERVING console face: a serving-purpose room renders the full
  // center-stage ops console (console doctrine — the graphical full view
  // lives HERE, never crammed into rails).
  const servingBody =
    !personaBody && !liveBody && !arenaBody && vm.purpose === SERVING_PURPOSE
      ? servingContentBody(live?.serving, live?.sys?.node ?? undefined)
      : undefined;
  // The GRID face: the NODES strip's full activity — every node's panel.
  const gridBody =
    !personaBody && !liveBody && !arenaBody && !servingBody && vm.purpose === GRID_PURPOSE
      ? gridContentBody(live?.sys, live?.serving)
      : undefined;
  // The BENCH face (#431): a room whose recipe purpose is in the benchmark
  // FAMILY (`benchmark/hard-rs` — the dispatched run rooms) renders the live
  // board. Before this branch existed those rooms fell through to a chat body
  // under an unregistered purpose and painted `Interface error` — a dispatched
  // round's room was unrenderable, the scoreboard region unreachable.
  const benchBody =
    !personaBody &&
    !liveBody &&
    !arenaBody &&
    !servingBody &&
    !gridBody &&
    contentFamilyOf(vm.purpose) === BENCH_PURPOSE
      ? benchContentBody(live?.bench)
      : undefined;
  // The persona home carries HER live benchmark runs — profile = identity +
  // cognition + the work itself (Joel: "all the cognitive and profile pages").
  const personaWithWork =
    personaBody && live?.bench
      ? {
          ...personaBody,
          runs: benchContentBody(live.bench).runs.filter(
            (r) => r.persona.toLowerCase() === personaBody.name.toLowerCase(),
          ),
        }
      : personaBody;
  // The ACADEMY LANDING: the default campus room renders as a landing —
  // live board center-stage, chat as the secondary layer — never a stale
  // transcript posing as the main page.
  const academyBody: AcademyContentBody | undefined =
    !personaBody &&
    !liveBody &&
    !arenaBody &&
    !servingBody &&
    !gridBody &&
    !benchBody &&
    vm.roomName.toLowerCase() === 'academy'
      ? {
          bench: benchContentBody(live?.bench),
          chat: { messages: vm.messages, transcript: vm.transcript, isEmpty: vm.isEmpty },
          memberCount: vm.memberCount,
          activeCount: vm.activeCount,
        }
      : undefined;
  // The CANVAS face (DESIGN-BENCH-VISUAL-CRAFT.md §5): a design-bench run
  // room's canvas region renders the persona's page LIVE — the frame renders
  // (the awaiting stage) even before the first observation delivers, exactly
  // like arena's pre-feed frame.
  const canvasBody =
    !personaBody &&
    !liveBody &&
    !arenaBody &&
    !servingBody &&
    !gridBody &&
    !benchBody &&
    contentFamilyOf(vm.purpose) === CANVAS_PURPOSE
      ? canvasContentBody(live?.canvas)
      : undefined;
  const content:
    | ContentView<ChatContentBody>
    | ContentView<PersonaContentBody>
    | ContentView<LiveContentBody>
    | ContentView<ArenaContentBodyT>
    | ContentView<ServingContentBody>
    | ContentView<GridContentBody>
    | ContentView<BenchContentBody>
    | ContentView<AcademyContentBody>
    | ContentView<CanvasContentBody>
    | ContentView<SettingsContentBody> = settingsBody
    ? { purpose: SETTINGS_PURPOSE, body: settingsBody }
    : personaBody
    ? { purpose: PERSONA_PURPOSE, body: personaWithWork ?? personaBody }
    : liveBody
      ? { purpose: LIVE_PURPOSE, body: liveBody }
      : arenaBody
        ? { purpose: ARENA_PURPOSE, body: arenaBody }
        : servingBody
          ? { purpose: SERVING_PURPOSE, body: servingBody }
          : gridBody
            ? { purpose: GRID_PURPOSE, body: gridBody }
            : benchBody
              ? { purpose: BENCH_PURPOSE, body: benchBody }
              : canvasBody
                ? { purpose: CANVAS_PURPOSE, body: canvasBody }
                : academyBody
                  ? { purpose: ACADEMY_PURPOSE, body: academyBody }
                  : {
                      purpose: vm.purpose,
                      body: { messages: vm.messages, transcript: vm.transcript, isEmpty: vm.isEmpty },
                    };
  // The ACTIVE nav cell follows the citizen's current tab: the persona tab
  // when a persona home is focused, else the chat room on screen.
  const roomsBase = live?.nav
    ? roomsListingFromNav(live.nav, persona?.id ?? vm.roomId, live?.bench)
    : roomsListing(vm);
  // AMBIENT PULSE (Joel, 2026-08-31: "see live benchmarks, events, etc
  // everywhere as this dynamic system operates"): the academy's rail cell
  // carries the node's live work heartbeat — visible from ANY room, one
  // glance, one click to the campus.
  const workingNow =
    live?.bench?.runs.filter((r) => r.phase === 'active' || r.phase === 'queued').length ?? 0;
  const rooms: ListingView =
    workingNow > 0
      ? {
          ...roomsBase,
          cells: roomsBase.cells.map((c) =>
            c.title.toLowerCase() === 'academy'
              ? { ...c, subtitle: `${workingNow} working now`, badges: [...(c.badges ?? []), 'live'] }
              : c,
          ),
        }
      : roomsBase;
  // The left rail = a global widget stack (the README's sidebar): System (SYS
  // gauge, when live) · AI Performance (live team cognition) · Rooms (all
  // rooms/DMs) · Users & Agents (the rich live tiles). Each is one PanelWidget
  // dispatched by kind; the roster stays the participants `Listing`
  // (ROSTER_LISTING_ID) that RAG + mobile ground on.
  const nodes = nodesWidget(live?.sys);
  // LEFT = NAVIGATION + the ONE HUD graph control, nothing more (console
  // doctrine: little real estate, rooms/users never pushed down; every
  // graph is a FACE of the one HUD — cycling or pinned — and details take
  // you to the full center-stage activity).
  const left = [
    continuonWidget(vm, live?.version, live?.feed),
    systemPanelWidget(vm, live?.sys, live?.serving),
    ...(nodes ? [nodes] : []),
    listingWidget(rooms),
    listingWidget(rosterListing(vm, live?.board)),
  ];
  // No bench rail: the board is CONTENT — the academy landing and the run
  // rooms render it center-stage; the right column belongs to the focused
  // activity's own context (Joel: "the right column is for the content
  // itself").

  return {
    nav: rooms,
    left,
    content,
    // The right contextual rail follows the focused activity: persona FACTS
    // (model · presence · genes · last active · claims) on a persona home,
    // else the room's info card — the ContextPanel primitive, activity-scoped.
    context: {
      listings: [personaBody ? personaFactsListing(personaBody) : roomInfoListing(vm)],
      // PAGES-IA slices 3+4: the rail carries the FOCUSED page's instruments.
      // Run room → its round's lifecycle stats; solve room → the run's card
      // facts + the worker. Composed from existing widget kinds, honest-absent.
      ...(personaWithWork === undefined || personaWithWork === null
        ? (() => {
            // Instruments first (round/card lifecycle on run/solve rooms),
            // then the room's PEOPLE (here-now / working-now doors) — the
            // chat-room answer to "what do I do and monitor here".
            const w = [...roomContextWidgets(vm, live), ...chatRoomPresenceWidgets(vm, live)];
            return w.length > 0 ? { widgets: w } : {};
          })()
        : {}),
      // PROFILE RIGHT-RAIL INSTRUMENTS (Joel: "good potential for righthand
      // widgets in the profile pages") — composed from EXISTING widget kinds
      // (metrics stat-rows + listing cells), each honestly absent until its
      // data is: RECORD (verdict identity), ENGINE (live speed needles as
      // numbers), ACTIVE WORK (her runs as door cells).
      ...(personaWithWork
        ? { widgets: personaContextWidgets(personaWithWork) }
        : {}),
    },
  };
}

/** CHAT-ROOM rail instruments (Joel 2026-08-31: "think about what you want to
 *  DO and monitor in each activity" — the rail was an info card and blank).
 *  A chat room is PEOPLE, so the rail answers the two live questions:
 *
 *  - **Here now** — the members attached and awake, each cell a DOOR to their
 *    persona home (the roster-pick verb; humans lead, then citizens).
 *  - **Working now** — this room's members with live runs, each cell a DOOR
 *    to the run's solve room (the run-card verb).
 *
 *  Honest-absent throughout: nobody active → no widget; no live runs (or no
 *  bench feed) → no widget; a run without a minted solve room stays doorless
 *  and is skipped rather than half-opening. Real presence + ledger data only —
 *  never fabricated rows. */
function chatRoomPresenceWidgets(
  vm: ChatViewModel,
  live?: WorkspaceLive,
): PanelWidget<MetricsView | ListingView>[] {
  const widgets: PanelWidget<MetricsView | ListingView>[] = [];
  const here = vm.members.filter((m) => m.active);
  if (here.length > 0) {
    const lead = [...here].sort((a, b) =>
      a.kind === b.kind ? a.name.localeCompare(b.name) : a.kind === 'human' ? -1 : 1,
    );
    widgets.push({
      id: HERE_NOW_LISTING_ID,
      kind: 'listing',
      title: 'Here now',
      scope: 'activity',
      body: {
        id: HERE_NOW_LISTING_ID,
        title: `Here now · ${here.length}`,
        cells: lead.slice(0, 12).map((m) => ({
          id: m.id,
          title: m.name,
          subtitle: [m.kind, m.runtime].filter(Boolean).join(' · '),
        })),
      },
    });
  }
  const memberById = new Map(vm.members.map((m) => [m.id, m.name]));
  const working = (live?.bench?.runs ?? []).filter(
    (r) =>
      (r.phase === 'active' || r.phase === 'queued') &&
      r.solver !== undefined &&
      memberById.has(r.solver) &&
      r.solve_room !== undefined,
  );
  if (working.length > 0) {
    widgets.push({
      id: WORKING_NOW_LISTING_ID,
      kind: 'listing',
      title: 'Working now',
      scope: 'activity',
      body: {
        id: WORKING_NOW_LISTING_ID,
        title: `Working now · ${working.length}`,
        cells: working.slice(0, 8).map((r) => ({
          id: r.solve_room as string,
          group: 'room',
          title: memberById.get(r.solver as string) ?? (r.solver as string),
          subtitle: r.instance ?? r.run_id,
          badges: [r.phase, ...(r.acts !== undefined ? [`${r.acts} acts`] : [])],
        })),
      },
    });
  }
  return widgets;
}

/** Compact seconds → "12s" / "3m" / "2h" — rail legibility, not precision. */
function formatAge(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  return `${Math.floor(seconds / 3600)}h`;
}

/** Run/solve room rail instruments (PAGES-IA slices 3+4): a run ROOM shows
 *  its round's lifecycle; a solve room shows its card + worker. Existing
 *  widget kinds only ([[compression]]). */
function roomContextWidgets(vm: ChatViewModel, live?: WorkspaceLive): PanelWidget<MetricsView | ListingView>[] {
  const widgets: PanelWidget<MetricsView | ListingView>[] = [];
  const bench = live?.bench;
  if (!bench) return widgets;
  const rounds = bench.rounds ?? [];
  const round = rounds.find((r) => r.round_id === vm.roomId);
  if (round) {
    // The verdict is the thrash-vs-grind sensor (2026-09-01: `working 0/8`
    // was pixel-identical for three hours of workspace-less narration and a
    // healthy grind). Core-pronounced; empty on a pre-verdict wire → omitted.
    const verdict = round.verdict ?? '';
    const unstarted = (round.cards ?? []).filter((c) => c.state === 'unstarted').length;
    widgets.push({
      id: 'room-round',
      kind: 'metrics',
      title: 'Round',
      scope: 'activity',
      body: {
        stats: [
          { label: 'STAGE', value: round.stage, tone: round.stage === 'working' ? 'accent' : 'muted' },
          ...(verdict !== '' && verdict !== round.stage
            ? [{
                label: 'VERDICT',
                value:
                  verdict === 'grinding' && round.idle_secs != null
                    ? `grinding · ${formatAge(round.idle_secs)}`
                    : verdict,
                tone: (verdict === 'grinding' ? 'good' : 'warn') as 'good' | 'warn',
              }]
            : []),
          { label: 'SETTLED', value: `${round.settled}/${round.dispatched}`, tone: 'good' },
          ...(unstarted > 0
            ? [{ label: 'NOT STARTED', value: String(unstarted), tone: 'warn' as const }]
            : []),
          { label: 'DRIVER', value: round.driver, tone: round.driver === 'citizen' ? 'good' : 'warn' },
        ],
      },
    });
    // THE CARD BOARD, in the rail (Joel 2026-09-01: "there should be so many
    // righthand widgets here… so as to let us know wtf is wrong"). One cell
    // per card — including the unstarted ones, which is exactly the "wtf":
    // a card with no run row used to render as nothing anywhere.
    const cards = round.cards ?? [];
    if (cards.length > 0) {
      // Assignees arrive as persona uuids from the tracker; the room roster
      // knows their names — resolve, fall back to the compact id.
      const nameOf = new Map(vm.members.map((m) => [m.id, m.name]));
      widgets.push({
        id: 'room-round-cards',
        kind: 'listing',
        title: 'Cards',
        scope: 'activity',
        body: {
          id: 'room-round-cards',
          title: `Cards · ${cards.length}`,
          cells: cards.slice(0, 16).map((c) => ({
            id: c.card_id,
            title: c.instance !== '' ? c.instance : c.card_id.slice(0, 8),
            // BOARD truth first: who HOLDS the card (seen live 2026-09-03: a round nine
            // citizens were working read "unassigned" from the tracker's assignee).
            subtitle:
              (c.owner ?? '') !== ''
                ? c.owner
                : c.assignee === ''
                  ? 'unassigned'
                  : (nameOf.get(c.assignee) ?? c.assignee.slice(0, 8)),
            badges: [
              (c.board_state ?? '') !== '' && c.state === 'unstarted' ? c.board_state : c.state,
              ...(c.acts != null ? [`${c.acts} acts`] : []),
              ...(c.patch_bytes != null ? [`${c.patch_bytes}B patch`] : []),
              ...(c.last_act_secs != null ? [`${formatAge(c.last_act_secs)} ago`] : []),
            ],
          })),
        },
      });
    }
    // VERDICTS — this round's settled truth (the grade tail, visible).
    const verdicts = (bench.runs ?? []).filter(
      (r) => r.round_id === round.round_id && (r.resolved !== undefined || r.phase === 'failed'),
    );
    if (verdicts.length > 0) {
      widgets.push({
        id: 'room-round-verdicts',
        kind: 'listing',
        title: 'Verdicts',
        scope: 'activity',
        body: {
          id: 'room-round-verdicts',
          title: `Verdicts · ${verdicts.length}`,
          cells: verdicts.slice(0, 10).map((r) => ({
            id: r.run_id,
            title: r.instance ?? r.run_id,
            subtitle: r.resolved === true ? '✓ resolved' : '✗ missed',
            badges: [
              ...(r.fail_to_pass !== undefined ? [`f2p ${r.fail_to_pass}`] : []),
              ...(r.pass_to_pass !== undefined ? [`p2p ${r.pass_to_pass}`] : []),
              ...r.failed_tests.slice(0, 1),
            ],
          })),
        },
      });
    }
  }
  const run = (bench.runs ?? []).find((r) => r.solve_room === vm.roomId);
  if (run) {
    widgets.push({
      id: 'room-card',
      kind: 'metrics',
      title: 'This work',
      scope: 'activity',
      body: {
        stats: [
          { label: 'STATE', value: run.phase, tone: run.phase === 'resolved' ? 'good' : run.phase === 'failed' ? 'warn' : 'accent' },
          ...(run.acts !== undefined ? [{ label: 'ACTS', value: String(run.acts), tone: 'muted' as const }] : []),
          ...(run.solver !== undefined ? [{ label: 'WORKER', value: run.solver, tone: 'accent' as const }] : []),
        ],
      },
    });
  }
  return widgets;
}

/** The profile's right-rail instrument stack. Every widget renders from an
 *  existing kind — no new renderer, no new wire type ([[compression]]). */
function personaContextWidgets(body: PersonaContentBody): PanelWidget<MetricsView | ListingView>[] {
  const widgets: PanelWidget<MetricsView | ListingView>[] = [];
  // RECORD — the verdict identity, as tone-colored stats.
  const runs = body.runs ?? [];
  const settled = runs.filter((r) => r.state === 'resolved' || r.state === 'failed');
  if (settled.length > 0) {
    const wins = settled.filter((r) => r.state === 'resolved').length;
    // The FORM CURVE: a rolling resolve-rate over the settled sequence
    // (window 3) — the shape of recent growth, oldest to newest.
    const form: number[] = settled.map((_, i) => {
      const win = settled.slice(Math.max(0, i - 2), i + 1);
      const w = win.filter((r) => r.state === 'resolved').length;
      return Math.round((w / win.length) * 100);
    });
    widgets.push({
      id: 'p-record',
      kind: 'metrics',
      title: 'Record',
      scope: 'activity',
      body: {
        ...(form.length > 1 ? { spark: form } : {}),
        stats: [
          { label: 'RESOLVED', value: String(wins), tone: 'good' },
          { label: 'SETTLED', value: String(settled.length), tone: 'muted' },
          { label: 'RATE', value: `${Math.round((wins / settled.length) * 100)}%`, tone: wins > 0 ? 'accent' : 'warn' },
        ],
      },
    });
  }
  // ENGINE — the speed pulse as readable numbers beside the tile needles.
  const tps = body.vitals['tps'];
  const pfx = body.vitals['pfx'];
  if (tps !== undefined || pfx !== undefined) {
    widgets.push({
      id: 'p-engine',
      kind: 'metrics',
      title: 'Engine',
      scope: 'activity',
      body: {
        stats: [
          ...(tps !== undefined ? [{ label: 'DECODE', value: `${tps}%`, tone: 'accent' as const }] : []),
          ...(pfx !== undefined ? [{ label: 'PREFILL', value: `${pfx}%`, tone: 'accent' as const }] : []),
          ...(body.vitals['activity'] !== undefined
            ? [{ label: 'ACT', value: String(body.vitals['activity']), tone: 'muted' as const }]
            : []),
        ],
      },
    });
  }
  // ACTIVE WORK — live runs as door cells (select routes like a room pick).
  const live = runs.filter((r) => r.state === 'working' || r.state === 'grading' || r.state === 'queued');
  if (live.length > 0) {
    widgets.push({
      id: 'p-active-work',
      kind: 'listing',
      title: 'Active work',
      scope: 'activity',
      body: {
        id: 'p-active-work',
        title: 'Active work',
        cells: live.map((r) => ({
          id: r.roomId ?? r.runId,
          title: r.instance,
          subtitle: r.state,
          status: 'active' as const,
          ...(r.roomId !== undefined ? { group: 'room' } : {}),
        })),
      },
    });
  }
  return widgets;
}
