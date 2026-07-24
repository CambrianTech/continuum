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
import type { GaugeView } from '@continuum/patterns';
import type { NavViewState, SystemMetricsViewState } from '@continuum/sdk-typescript';
import type { ChatViewModel, MemberKind, MessageRowVM, RosterMemberVM } from './chatViewModel';

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
    title: 'Rooms',
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
    title: 'Rooms',
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
): PanelWidget<SystemPanelView> {
  const body: SystemPanelView = {
    ...(sys ? { gauge: systemGaugeWidget(sys).body } : {}),
    stats: metricsWidget(vm).body,
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
  /** The client build's version string (a real manifest/build stamp — e.g. the web
   *  app's package version). Drives the continuon header's version badge; honestly
   *  absent when the host has none to report. */
  readonly version?: string;
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
  const content: ContentView<ChatContentBody> = {
    purpose: vm.purpose,
    body: { messages: vm.messages, isEmpty: vm.isEmpty },
  };
  const rooms = live?.nav ? roomsListingFromNav(live.nav, vm.roomId) : roomsListing(vm);
  // The left rail = a global widget stack (the README's sidebar): System (SYS
  // gauge, when live) · AI Performance (live team cognition) · Rooms (all
  // rooms/DMs) · Users & Agents (the rich live tiles). Each is one PanelWidget
  // dispatched by kind; the roster stays the participants `Listing`
  // (ROSTER_LISTING_ID) that RAG + mobile ground on.
  const nodes = nodesWidget(live?.sys);
  const left = [
    continuonWidget(vm, live?.version),
    systemPanelWidget(vm, live?.sys),
    ...(nodes ? [nodes] : []),
    listingWidget(rooms),
    listingWidget(rosterListing(vm)),
  ];
  return {
    nav: rooms,
    left,
    content,
    // The right contextual rail: the focused room's info card (purpose,
    // presence, participant mix) — the ContextPanel primitive, finally fed.
    context: { listings: [roomInfoListing(vm)] },
  };
}
