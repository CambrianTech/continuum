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

import { listingWidget } from '@continuum/patterns';
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
  NavViewState,
  ServingViewState,
  SystemMetricsViewState,
} from '@continuum/sdk-typescript';
import type { ChatViewModel, MemberKind, MessageRowVM, RosterMemberVM } from './chatViewModel';
import { ARENA_PURPOSE, GRID_PURPOSE, LIVE_PURPOSE, PERSONA_PURPOSE, SERVING_PURPOSE, type ArenaContentBody as ArenaContentBodyT, type GridContentBody, type GridNodeVM, type ServingContentBody, type ServingNodeVM } from '@continuum/patterns';
import type { LiveContentBody, PersonaContentBody } from '@continuum/patterns';
import {
  focusedPersonaTab,
  personaContentBody,
  personaFactsListing,
} from './personaProjections';
import { liveContentBody, liveFaceOpen, type LiveCallOverlay } from './liveProjections';
import { benchWidget } from './benchProjections';
import { arenaContentBody, type ArenaViewState } from './arenaProjections';

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

/** The chat activity's `who` panel projected as the `Listing` primitive. Same shape
 *  the rooms/DMs list and Foundry's model list use — one primitive, different data. */
export function rosterListing(vm: ChatViewModel): ListingView {
  return {
    id: 'roster',
    title: 'Users & Agents',
    cells: vm.members.map(rosterCell),
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
export function roomsListingFromNav(nav: NavViewState, focusedRoomId: string): ListingView {
  return {
    id: 'rooms',
    title: 'Activities',
    cells: nav.open_tabs.map((tab): ListingCell => {
      const cell: ListingCell = {
        id: tab.id,
        title: tab.title,
        status: tab.id === focusedRoomId ? 'active' : 'idle',
        group: tab.kind,
        // The room's recipe-defined activity purpose, carried verbatim as the
        // description line ([[room-purpose-is-per-recipe-not-an-enum]]).
        // Empty = unresolved — no subtitle drawn, never a fabricated blurb.
        ...(tab.purpose ? { subtitle: tab.purpose } : {}),
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
export function continuonWidget(vm: ChatViewModel, version?: string): PanelWidget<ContinuonView> {
  const body: ContinuonView = {
    wordmark: 'continuum',
    tagline: 'ai workforce construction',
    ...(version ? { version } : {}),
    // Newest last, last three turns — the ticker reads bottom-fresh like a log tail.
    ticker: vm.messages.slice(-3).map((m) => tickerLine(m)),
    alive: vm.members.some((m) => m.active),
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
  /** The widget-owned live-call overlay (Go-live face state + the StreamDelta
   *  token rail + captions toggle) — renderer state threaded through so the
   *  live face projects from REAL signals. Absent = no live face requested. */
  readonly call?: LiveCallOverlay;
  /** The `kind="arena"` eval-ledger view — feeds an arena-purpose room's
   *  leaderboards. Honestly absent until the feed delivers. */
  readonly arena?: ArenaViewState;
  /** The node's `kind="bench"` benchmark board (#329) — adds the academy
   *  right-rail bench widget. Honestly absent until the feed delivers. */
  readonly bench?: BenchViewState;
}

/** The chat activity's `Content` body — the conversation. `Content` is keyed by the
 *  room's `purpose` (here `vm.purpose`, `"chat"`), so a target's registered chat
 *  renderer draws these rows; a foundry room would carry a different purpose + body. */
export interface ChatContentBody {
  readonly messages: readonly MessageRowVM[];
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
  const persona = focusedPersonaTab(live?.nav);
  const personaBody = persona ? personaContentBody(vm, persona, live?.board) : undefined;
  // The LIVE face ([[LIVE_PURPOSE]]): a room's call grid, dispatched through the
  // SAME registry when the room's recipe purpose is "live", a live-purpose tab
  // is focused, or the reader opened the Go-live face — honest entries only.
  // A focused persona tab still wins (the citizen navigated away from the room).
  const liveBody: LiveContentBody | undefined =
    !personaBody && liveFaceOpen(vm, live?.nav, live?.call)
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
  const content:
    | ContentView<ChatContentBody>
    | ContentView<PersonaContentBody>
    | ContentView<LiveContentBody>
    | ContentView<ArenaContentBodyT>
    | ContentView<ServingContentBody>
    | ContentView<GridContentBody> = personaBody
    ? { purpose: PERSONA_PURPOSE, body: personaBody }
    : liveBody
      ? { purpose: LIVE_PURPOSE, body: liveBody }
      : arenaBody
        ? { purpose: ARENA_PURPOSE, body: arenaBody }
        : servingBody
          ? { purpose: SERVING_PURPOSE, body: servingBody }
          : gridBody
            ? { purpose: GRID_PURPOSE, body: gridBody }
            : {
                purpose: vm.purpose,
                body: { messages: vm.messages, isEmpty: vm.isEmpty },
              };
  // The ACTIVE nav cell follows the citizen's current tab: the persona tab
  // when a persona home is focused, else the chat room on screen.
  const rooms = live?.nav
    ? roomsListingFromNav(live.nav, persona?.id ?? vm.roomId)
    : roomsListing(vm);
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
    continuonWidget(vm, live?.version),
    systemPanelWidget(vm, live?.sys, live?.serving),
    ...(nodes ? [nodes] : []),
    listingWidget(rooms),
    listingWidget(rosterListing(vm)),
  ];
  return {
    nav: rooms,
    left,
    content,
    // The right contextual rail follows the focused activity: persona FACTS
    // (model · presence · genes · last active · claims) on a persona home,
    // else the room's info card — the ContextPanel primitive, activity-scoped.
    context: {
      listings: [personaBody ? personaFactsListing(personaBody) : roomInfoListing(vm)],
      // The live benchmark board (#329) — joins the contextual rail whenever
      // this node has runs, filling the academy's dead right column.
      ...(benchWidget(live?.bench) ? { widgets: [benchWidget(live?.bench)!] } : {}),
    },
  };
}
