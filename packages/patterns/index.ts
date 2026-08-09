/**
 * `@continuum/patterns` — the consumer-neutral widget pattern contract.
 *
 * The precedence-winning code expression of `docs/architecture/ACTIVITY-ROOM-PATTERNS.md`.
 * A "widget" is NOT a component; it is a **pattern** — a projection of who/what/where
 * that renders to any consumer. This package holds only the **shapes** (the "props")
 * and the **render contract**; it has NO DOM, NO ANSI, NO RAG specifics, and no
 * dependencies. Each surface (apps/web Lit, apps/tui ANSI, the Rust persona RAG
 * renderer, a future mobile shell) implements a `RenderTarget` over these shapes.
 *
 * The four primitives — everything decomposes into these:
 *   - `Listing`      — a repeating list (users, rooms/DMs, HF-models, cohorts…). One
 *                      primitive, reused everywhere a list appears.
 *   - `Content`      — the centre, **dispatched by room purpose** (a MIME-type handler).
 *   - `ContextPanel` — the right-hand activity widgets (often themselves Listings).
 *   - `Workspace`    — the shell: the rooms-`Listing` (tab bar == channel-attention)
 *                      + left listings + focused-room content + context panel.
 *
 * The thesis: `activity == airc room == content == tab`, and RAG is a peer
 * `RenderTarget` alongside web/mobile/terminal — so the human's UI and the persona's
 * grounding are ONE projection that cannot drift.
 */

// The persona home's neutral Content body (`purpose === PERSONA_PURPOSE`) — the
// profile/brain surface as a purpose-dispatched activity, never a special view.
export { PERSONA_PURPOSE } from './personaContent';

// The live call face's neutral Content body (`purpose === LIVE_PURPOSE`) — a
// room's call grid as a purpose-dispatched activity, never a special view.
export { LIVE_PURPOSE } from './liveContent';

// The benchmark arena's neutral Content body (`purpose === ARENA_PURPOSE`) —
// leaderboards + live-run strip from REAL eval ledger rows, never a mockup.
export { ARENA_PURPOSE } from './arenaContent';
export type {
  ArenaContentBody,
  ArenaResultRowVM,
  ArenaBoardVM,
  ArenaLiveRunVM,
} from './arenaContent';

// The serving-ops CONSOLE's neutral Content body (`purpose === SERVING_PURPOSE`)
// — per-node control-loop panels, center-stage full view (console doctrine).
export { SERVING_PURPOSE } from './servingContent';
export type { ServingContentBody, ServingNodeVM } from './servingContent';

// The GRID's neutral Content body (`purpose === GRID_PURPOSE`) — the NODES
// strip's full activity: every node's resources + serving, SCADA-style.
export { GRID_PURPOSE } from './gridContent';
export type { GridContentBody, GridNodeVM } from './gridContent';

// The Academy's live BENCHMARK BOARD (`purpose === BENCH_PURPOSE`) — one row
// per run (operator + citizen-claimed), progress-not-liveness (#374/#329).
export { BENCH_PURPOSE } from './benchContent';
export type { BenchContentBody, BenchRunVM, BenchRunState, BenchVerdictVM } from './benchContent';
export type {
  LiveContentBody,
  LiveParticipantVM,
  LiveCaptionVM,
  LiveControlsVM,
} from './liveContent';
export type {
  PersonaContentBody,
  PersonaBrainRegionVM,
  PersonaRegionFact,
  PersonaPathwayVM,
  PersonaClaimVM,
  PersonaWritingVM,
} from './personaContent';

// ── Listing ────────────────────────────────────────────────────────────────

/** Presence/liveness of a listing cell, when it has one (a member is active/idle;
 *  a room has unread; a model is loaded). `'none'` = the cell carries no status dot. */
export type CellStatus = 'active' | 'idle' | 'none';

/** One already-projected row of a `Listing`. Display fields only — the projection
 *  (e.g. `rosterListing` in `@continuum/chat-view`) has already resolved names,
 *  glyphs, and badges from the domain state; a `RenderTarget` only draws these. */
export interface ListingCell {
  /** Stable identity of the item this cell represents (member id, room id, model id). */
  readonly id: string;
  /** Primary label (a person's name, a room name, a model name). */
  readonly title: string;
  /** Optional secondary line (a role, a last-seen, a param count). */
  readonly subtitle?: string;
  /** Optional leading glyph — an emoji/icon token (the avatar in a people listing). */
  readonly glyph?: string;
  /** Optional avatar/thumbnail IMAGE URL (or data URI) for this item — a persona's
   *  stored portrait, a model's card art. The glyph remains the fallback a target
   *  draws when the image is absent or fails to load — honest-absent, never a
   *  broken-image box ([[fallbacks-are-illegal-fail-loud]] applies to fabrication,
   *  not to degrading to the glyph the cell already carries). */
  readonly image?: string;
  /** Optional short tags (kind, runtime, provider) — the target styles them. */
  readonly badges?: readonly string[];
  /** Optional presence/liveness; `'none'` draws no status indicator. */
  readonly status?: CellStatus;
  /** Optional attention count (a room's unread, a board's open cards) — a target
   *  draws it as a badge pill (web) or `(3 new)` (RAG/terminal). Absent or 0 =
   *  no badge. Numeric so every surface formats its own idiom — the same
   *  lossless-enrichment path `meters` took. */
  readonly count?: number;
  /** Optional grouping/category key (the "bookmarked menus + categories" axis). */
  readonly group?: string;
  /** Optional longer description of the item — a citizen's published BIO, a
   *  model's card description, a room's charter line. The prose sibling of
   *  `subtitle` (one line vs a sentence); a target surfaces it as hover text
   *  or a detail pane. Absent = none published — honest-absent, never a
   *  fabricated blurb (#262). */
  readonly detail?: string;
  /** Optional named gauges (0–100), drawn as bars/meters by a target — a member's
   *  genome-energy vitals, a model's download %, a room's activity. Keeps the neutral
   *  cell LOSSLESS so rich rows (the roster's ACT meters) survive the projection instead
   *  of forcing the rich view-model across the boundary. Empty/absent = no meters. */
  readonly meters?: Readonly<Record<string, number>>;
  /** Optional **loadout** — the model backing this item (`model · size · ctx`), carried
   *  losslessly for a target to draw as a caption strip. The label sibling of `meters`:
   *  `meters` are 0–100 gauges, a loadout is capability text/counts. Absent = none
   *  reported (a human, a room, a model-less row) — the target draws no strip. */
  readonly loadout?: CellLoadout;
  /** Optional recency fact — WHEN this item was last active (epoch ms, the raw
   *  `last_seen_ms` presence signal). RAW so every surface formats its own idiom
   *  (`"55m ago"` on web, `(55m)` in RAG). Absent or 0 = unknown — the target
   *  draws no stamp, never a fabricated recency. */
  readonly lastActiveMs?: number;
  /** Optional NAMES of the item's loaded skill overlays (a persona's paged-in
   *  LoRA genes), in load order — the label half of a `meters.genome` count, so
   *  a target names each lit genome segment (tooltip) instead of drawing an
   *  anonymous chip. Absent/empty = none loaded/reported — honest-absent,
   *  never fabricated labels. */
  readonly genes?: readonly string[];
}

/** A roster cell's model loadout — the display facts of an AI member's backend. RAW
 *  numbers (`params: 24_000_000_000`, `contextWindow: 32768`); the target formats the
 *  unit (`24B`, `32k`). Every field optional — honest-absent, never a fabricated model. */
export interface CellLoadout {
  readonly model?: string;
  readonly params?: number;
  readonly contextWindow?: number;
}

/** The `Listing` pattern — a repeating list. The SAME shape backs the users list,
 *  the rooms/DMs list, Foundry's HF-model list, and (in RAG) a categorized grounding
 *  menu. Different data + cells, one primitive, every surface. */
export interface ListingView {
  /** Which listing this is (`"roster"`, `"rooms"`, `"hf-models"`) — targets may key on it. */
  readonly id: string;
  /** Panel header ("Users & Agents", "Rooms", "Models"). */
  readonly title: string;
  /** The rows, already projected. */
  readonly cells: readonly ListingCell[];
}

// ── Content (dispatched by room purpose) ─────────────────────────────────────

/** The `Content` pattern — the centre of a workspace, selected by the room's
 *  **purpose** (the content-type / MIME key: `"chat"`, `"foundry"`, `"scada"`…).
 *  `body` is purpose-specific and opaque here; a per-purpose renderer registered on
 *  the target interprets it. This is what makes "any activity is a room": the purpose
 *  chooses the transform, not a bespoke container. */
export interface ContentView<Body = unknown> {
  /** The room purpose — the dispatch key. */
  readonly purpose: string;
  /** Purpose-specific payload the registered content renderer understands. */
  readonly body: Body;
}

// ── ContextPanel + Workspace ─────────────────────────────────────────────────

/** The right-hand `ContextPanel` — activity-scoped supporting widgets. Today a set
 *  of `Listing`s (Foundry's HF-models, a chat thread's participants); richer widget
 *  kinds join this union as they land. */
export interface ContextPanelView {
  readonly listings: readonly ListingView[];
}

// ── PanelWidget (the left rail's global widget stack) ────────────────────────

/** One widget in a panel stack — the left rail's `Metrics` (resources + spend),
 *  `Rooms`, `Users & Agents`, and (later) `Continuon` / `Status`. A widget is
 *  dispatched by `kind` EXACTLY as `Content` is dispatched by `purpose`: the rail is
 *  heterogeneous (a chart is not a list), so a target draws each widget through its
 *  `WidgetRegistry`. A `Listing` is simply `kind:'listing'` whose `body` is a
 *  `ListingView` — the roster/rooms lists stay the one `Listing` primitive, now as
 *  one widget kind among several. This is what makes the left rail a real, reorderable,
 *  resizable stack instead of a hardcoded roster ([[app-shell-layout-left-global-right-per-activity]]). */
export interface PanelWidget<Body = unknown> {
  /** Stable identity — the key per-user layout state (height / order / collapse) is
   *  stored under, so a resized/reordered rail survives across sessions and devices. */
  readonly id: string;
  /** Dispatch key: `'listing' | 'metrics' | 'rooms' | 'status' | 'continuon'`. A target
   *  looks this up in its `WidgetRegistry` and **fails loud** on an unknown kind. */
  readonly kind: string;
  /** Panel header a target draws as the widget's title bar ("AI Performance", "Rooms"). */
  readonly title: string;
  /** kind-specific payload the registered widget renderer understands (a `ListingView`
   *  for `'listing'`, a metrics snapshot for `'metrics'`, …). Opaque here. */
  readonly body: Body;
  /** Where the widget lives: `'global'` persists across every activity (the left rail
   *  is the same in chat, metrics, and the call), `'activity'` is scoped to the focused
   *  room. Default `'global'`. */
  readonly scope?: 'global' | 'activity';
}

/** The stable id the participants `Listing` carries — the ONE listing a persona
 *  grounds on (who is here) and mobile surfaces as its "Who" tab, distinct from a
 *  Rooms/DMs listing that may share the rail. Single-sourced so the RAG + mobile rules
 *  can find the roster among several listing widgets without a magic string each. */
export const ROSTER_LISTING_ID = 'roster';

/** A metrics `PanelWidget` body — a small, pre-formatted readout the rail draws as a
 *  labelled stat row (+ an optional sparkline). The projection owns the numbers AND
 *  their formatting (units, precision); a target only paints. Drives the left rail's
 *  "AI Performance / team cognition" widget. */
export interface MetricStat {
  /** Short uppercase label ("HERE", "THINKING", "TOK", "COST"). */
  readonly label: string;
  /** Pre-formatted value string ("4", "18k", "$0.00", "58%"). */
  readonly value: string;
  /** Optional semantic tone a target maps to colour — separate from the accent hue. */
  readonly tone?: 'good' | 'warn' | 'accent' | 'muted';
}

/** The `metrics` widget body — a stat row + optional 0..=100 sparkline series. */
export interface MetricsView {
  readonly stats: readonly MetricStat[];
  /** Optional time-series (0..=100) a target draws as a sparkline; absent = no history. */
  readonly spark?: readonly number[];
}

/** One named series of a `gauge` widget — a rolling 0..=100 window plus the
 *  display-ready current reading. The projection owns normalization AND
 *  formatting; a target draws a polyline (web), block bars (terminal), or the
 *  `CPU 58% · MEM 25/32G` grounding line (RAG) from the same fields. */
export interface GaugeSeries {
  /** Short uppercase-able label ("CPU", "MEM", "GPU"). */
  readonly label: string;
  /** Rolling normalized samples 0..=100, oldest → newest (bounded upstream). */
  readonly points: readonly number[];
  /** Pre-formatted current reading ("58%", "25.3/32G"). */
  readonly current: string;
}

/** The `gauge` widget body — a multi-series live graph (brick 2 of
 *  POSITRON-WIDGET-SOPHISTICATION.md: the old sidebar's SYS sparkline). A
 *  sibling of `MetricsView`: metrics is a stat ROW, gauge is a windowed GRAPH. */
export interface GaugeView {
  readonly series: readonly GaugeSeries[];
  /** Sample cadence (ms) — lets a target label the window span from data. */
  readonly sampleIntervalMs?: number;
}

/** The `system` widget body — the rail's TWO-FACED system panel (the old
 *  sidebar's SYS|AI header): the node's resource gauge (SYS face) and the live
 *  team-cognition stats (AI face) as one widget, so a target can draw a real
 *  toggle between them instead of stacking two half-panels. `gauge` is honestly
 *  absent until the node's metrics feed delivers; `stats` derives from state the
 *  surface already holds. Which face shows is renderer state (a lens), never
 *  projection state. */
export interface SystemPanelView {
  /** The node's resource window (CPU/MEM/GPU) — the SYS face. Absent = the
   *  feed hasn't delivered; a target disables that face, honestly. */
  readonly gauge?: GaugeView;
  /** The live team-cognition stat row — the AI face. */
  readonly stats: MetricsView;
  /** The serving summary — the SRV face of the HUD (compact; the FULL view
   *  is the serving console activity, which this face is the portal to).
   *  Absent = no serving feed; the face disables, honestly. */
  readonly serving?: ServingPanelView;
}

/** One bandit arm of the `serving` widget — the pager's learned-decay dial. */
export interface ServingArm {
  /** Arm label (the decay constant, "0.99"…"0.00"). */
  readonly label: string;
  /** EMA reward 0..=1 — the bandit's belief for this arm. */
  readonly reward: number;
  /** True for the arm currently serving predictions. */
  readonly chosen: boolean;
}

/** One pager event card of the `serving` widget — a discrete control-loop
 *  moment (serve start, decay switch, residency shift) rendered as an
 *  activity card. */
export interface ServingEvent {
  /** Decode-token index the event fired at. */
  readonly atToken: number;
  /** Event class slug ("serve-start" | "decay-switch" | "residency-shift"). */
  readonly kind: string;
  /** Human one-liner, formatted at the source. */
  readonly detail: string;
}

/** The `serving` widget body — the node's live inference glass box: what model
 *  is up, and when the MoE pager streams, the control loop itself (hit rate /
 *  tok-s / fetch series, bandit arms, event cards). Every section honestly
 *  absent until its feed delivers — a header-only body is a plain
 *  serving-health widget, never a fabricated gauge. */
export interface ServingPanelView {
  /** Serving header line. Absent = the daemon has not published yet. */
  readonly header?: {
    readonly model?: string;
    readonly ready: boolean;
    readonly lanes: number;
    readonly contextWindow: number;
    readonly degradedReason?: string;
  };
  /** Pager time-series (hit %, tok/s, fetch). Absent = no capture feed. */
  readonly gauge?: GaugeView;
  /** Bandit arm beliefs; empty until the decision feed carries them. */
  readonly arms: readonly ServingArm[];
  /** Recent pager event cards, oldest → newest, bounded upstream. */
  readonly events: readonly ServingEvent[];
}

/** The `continuon` widget body — the rail's identity header (the top-left mark of
 *  POSITRON-PURE-ROOMS-BRIEF.md: "alive — a slow-breathing mark"). Wordmark + an
 *  optional version badge + a compact live-activity ticker. The projection owns the
 *  ticker's formatting (digested, newest last); a target only paints — web draws a
 *  breathing dot + scrolling mono lines, terminal a title line, RAG nothing (no
 *  grounding value). Every field honest: no ticker lines yet = a quiet header,
 *  never fabricated activity. */
export interface ContinuonView {
  /** The product wordmark ("continuum"). */
  readonly wordmark: string;
  /** Optional strapline under the wordmark. */
  readonly tagline?: string;
  /** Optional version badge ("v0.1.0") — from a REAL version source (a package
   *  manifest, a core build stamp), never a hardcoded literal in a renderer. */
  readonly version?: string;
  /** Compact live-activity lines (newest last, already digested/truncated by the
   *  projection). Empty = nothing observed yet — honest quiet. */
  readonly ticker: readonly string[];
  /** Whether the substrate feed is live — drives the breathing mark's state. */
  readonly alive: boolean;
}

/** Wrap a `ListingView` as a `kind:'listing'` `PanelWidget` — the common case (the
 *  roster, a rooms list). Keeps constructors terse and single-sources the wrapping so
 *  the widget id/title default to the listing's own ([[compression]]). */
export function listingWidget(
  view: ListingView,
  over?: Partial<Omit<PanelWidget<ListingView>, 'kind' | 'body'>>,
): PanelWidget<ListingView> {
  return {
    id: over?.id ?? view.id,
    kind: 'listing',
    title: over?.title ?? view.title,
    body: view,
    scope: over?.scope ?? 'global',
  };
}

/** The `Workspace` shell — the whole who/what/where for one focused room. `nav` is
 *  the rooms-`Listing`: the tab bar for a human, the channel-attention set for a
 *  persona. `left`/`content`/`context` are the three zones. */
export interface WorkspaceView {
  /** The rooms-`Listing` — tab bar == persona channel-attention (one nav primitive). */
  readonly nav: ListingView;
  /** The left rail — a GLOBAL, reorderable/resizable stack of `PanelWidget`s (Metrics,
   *  Rooms, Users & Agents, …), identical across every activity. A roster is one widget
   *  (`kind:'listing'`), not the whole rail. */
  readonly left: readonly PanelWidget[];
  /** The focused room's content, dispatched by its purpose. */
  readonly content: ContentView;
  /** The right-hand supporting widgets for the focused activity. */
  readonly context: ContextPanelView;
}

/** Per-user workspace LAYOUT state — the citizen's arrangement of the shell,
 *  designed ONCE for every pointer-capable target (desktop web, iPad, a future
 *  native shell all render the same drag affordances from this one shape).
 *  Presentation state, not domain truth: a host persists it per user (today a
 *  local store; the airc per-(user,scope) state row when the layout verbs land
 *  — same migration path as the nav focus store) and re-applies it on mount.
 *  Grows widget order/heights/collapse (task #185) without a shape break. */
export interface WorkspaceLayout {
  /** Left rail width in px; absent = the target's default. Targets clamp to
   *  their own sane min/max — the value is intent, not law. */
  readonly whoWidth?: number;
  /** Right context-panel width in px; absent = the target's default. */
  readonly contextWidth?: number;
}

// ── RenderTarget — the consumer-neutral render contract ──────────────────────

/** A per-purpose content renderer: given the `ContentView.body` and its purpose,
 *  produce this target's output. Registered on a `ContentRegistry` — the MIME table. */
export type ContentRenderer<Out, Body = unknown> = (body: Body, purpose: string) => Out;

/** The content-dispatch table for one target: `purpose → renderer`. `render` looks up
 *  by `view.purpose` and **fails loud** on an unregistered purpose — an unknown
 *  activity is a wiring bug, never a silent blank ([[fallbacks-are-illegal-fail-loud]]). */
export interface ContentRegistry<Out> {
  register<Body>(purpose: string, renderer: ContentRenderer<Out, Body>): void;
  render(view: ContentView): Out;
}

/** Target-native fragments the HOST composes into the workspace shell. The
 *  compose bar (and its error strips) is host-owned — it needs the input state
 *  and send handler the pure projection can't hold — but it BELONGS inside the
 *  center column (the Discord geometry: full-height rails, center-scoped
 *  header/transcript/composer). This slot lets the host hand that fragment to
 *  the target, which places it; a target with no such slot ignores it. */
export interface WorkspaceChrome<Out> {
  /** Bottom of the center column — the compose bar + transient error strips. */
  readonly centerFooter?: Out;
}

/** A consumer that renders the patterns to its own output type `Out` — web
 *  (a Lit `TemplateResult`), terminal (a `string` of cells), mobile (a native node),
 *  RAG (a grounding block `string`). One contract, every surface. The `content`
 *  method dispatches through the target's `ContentRegistry`, so adding an activity is
 *  registering a content renderer — never a new target. */
export interface RenderTarget<Out> {
  listing(view: ListingView): Out;
  content(view: ContentView): Out;
  contextPanel(view: ContextPanelView): Out;
  /** Draw one left-rail `PanelWidget`, dispatched by its `kind` through the target's
   *  `WidgetRegistry` — the panel-stack analogue of `content` dispatching by purpose.
   *  Adding a rail widget is registering a renderer, never a new target method. */
  widget(view: PanelWidget): Out;
  workspace(view: WorkspaceView, chrome?: WorkspaceChrome<Out>): Out;
}

// ── WidgetRegistry — the left-rail dispatch table (mirrors ContentRegistry) ───

/** A per-kind widget renderer: given a `PanelWidget`, produce this target's output.
 *  Registered on a `WidgetRegistry` — the widget-kind table. */
export type WidgetRenderer<Out, Body = unknown> = (widget: PanelWidget<Body>) => Out;

/** The widget-dispatch table for one target: `kind → renderer`. `render` looks up by
 *  `widget.kind` and **fails loud** on an unregistered kind — an unknown rail widget is
 *  a wiring bug, never a silent blank ([[fallbacks-are-illegal-fail-loud]]). */
export interface WidgetRegistry<Out> {
  register<Body>(kind: string, renderer: WidgetRenderer<Out, Body>): void;
  render(widget: PanelWidget): Out;
}

/** Build an empty widget-dispatch table for a target. Fail-loud on unknown kind. */
export function createWidgetRegistry<Out>(): WidgetRegistry<Out> {
  const table = new Map<string, WidgetRenderer<Out>>();
  return {
    register<Body>(kind: string, renderer: WidgetRenderer<Out, Body>): void {
      if (table.has(kind)) {
        throw new Error(`widget renderer already registered for kind "${kind}"`);
      }
      table.set(kind, renderer as WidgetRenderer<Out>);
    },
    render(widget: PanelWidget): Out {
      const renderer = table.get(widget.kind);
      if (!renderer) {
        const known = [...table.keys()].join(', ') || '(none)';
        throw new Error(
          `no widget renderer for panel-widget kind "${widget.kind}" — registered: ${known}`,
        );
      }
      return renderer(widget);
    },
  };
}

// ── defineApp / mount — define an app ONCE, render it on every modality ───────

/**
 * An app, **defined once**: given domain `State`, `project` it to the neutral
 * who/what/where `WorkspaceView` — plus its `universe` (look/lore key a target maps
 * to styling). That is the ENTIRE app: purely neutral, ZERO dependency on the SDK,
 * the DOM, Flutter, or any target. The same `AppDefinition` mounts to web (Lit),
 * mobile (Flutter), terminal (ANSI), and RAG — the frameworks paint, positron
 * defines ([[three-separable-layers-recipe-positron-universe]]).
 */
export interface AppDefinition<State> {
  /** Domain state → the neutral view-model. The app's "what to show". */
  readonly project: (state: State) => WorkspaceView;
  /** Universe key (look/lore); a target maps it to its styling. Optional. */
  readonly universe?: string;
}

/** A live data source: pushes `State` on every change, returns an unsubscribe.
 *  Injected at `mount` so the SAME app runs against a real core, a replay, or a test
 *  fixture — the app never names its source ([[logical-portability-for-unknown-future-integrations]]). */
export type AppSource<State> = (onState: (state: State) => void) => () => void;

/** A surface sink: receives the target's rendered `Out` on each change and puts it on
 *  screen (DOM replace, ANSI write, a RAG buffer). Target-specific; the app is not. */
export type AppSink<Out> = (out: Out) => void;

/**
 * The framework seam: normalize/return a typed app definition. Identity today; the
 * single place future validation (universe resolution, activity-coverage checks)
 * hangs off — build now in a shape that welcomes the future.
 */
export function defineApp<State>(def: AppDefinition<State>): AppDefinition<State> {
  return def;
}

/**
 * Mount an app onto ONE modality: `source → project → target.workspace → sink`.
 * The SAME `app` + `source` mounts to ANY `RenderTarget` — **define once, render
 * everywhere.** Returns a teardown (unsubscribes the source).
 *
 * ```ts
 * const app = defineApp({ project: chatWorkspace });          // once
 * mount(app, sdkSource, webTarget,     el.replaceChildren);   // web (Lit)
 * mount(app, sdkSource, flutterTarget, flutterSink);          // mobile (Flutter)
 * mount(app, sdkSource, ragTarget,     ragBuffer);            // agents (RAG)
 * ```
 */
export function mount<State, Out>(
  app: AppDefinition<State>,
  source: AppSource<State>,
  target: RenderTarget<Out>,
  sink: AppSink<Out>,
): () => void {
  return source((state) => { sink(target.workspace(app.project(state))); });
}

/**
 * `createRagTarget` — a reusable **RAG adaptation rule**, authored ONCE, that any app
 * inherits. It proves the automatic-per-surface model ([[best-ux-per-portal-not-identical-projection]]):
 * given the SAME semantic `WorkspaceView` that web renders as a three-panel and terminal as
 * WHO/WHAT sections, this derives an entirely different, surface-appropriate output — a
 * **concise grounding block for a persona's LLM context** — automatically, by RULE, not by
 * per-app design. The rule: *room + who + primary content; DROP the nav, the secondary
 * listings, the context chrome* — only what an agent needs to act now. `content` dispatches
 * the primary body through the caller's registry (the app supplies a concise renderer).
 */
export function createRagTarget(content: ContentRegistry<string>): RenderTarget<string> {
  const names = (v: ListingView): string => v.cells.map((c) => c.title).join(', ');
  // The roster is the participants `Listing` (id === ROSTER_LISTING_ID) — a persona
  // grounds on WHO is present, not on a Rooms/metrics widget that may share the rail.
  const rosterOf = (ws: WorkspaceView): ListingView | undefined => {
    const w = ws.left.find(
      (widget) => widget.kind === 'listing' && (widget.body as ListingView).id === ROSTER_LISTING_ID,
    );
    return w ? (w.body as ListingView) : undefined;
  };
  return {
    // In a grounding block a Listing collapses to just its members' names — no rows, no chrome.
    listing: (view: ListingView): string => names(view),
    content: (view: ContentView): string => content.render(view),
    // Context chrome is dropped entirely — an agent grounds on who + what, not side widgets.
    contextPanel: (_view: ContextPanelView): string => '',
    // A rail widget collapses to its listing's names (roster) or nothing (metrics/status
    // carry no grounding value for a persona acting in the room).
    widget: (view: PanelWidget): string =>
      view.kind === 'listing' ? names(view.body as ListingView) : '',
    workspace: (ws: WorkspaceView): string => {
      const room = ws.nav.cells[0]?.title ?? 'a room';
      const roster = rosterOf(ws);
      const who = roster && roster.cells.length > 0 ? names(roster) : 'no one else';
      return `You are in "${room}" with ${who}.\n${content.render(ws.content)}`;
    },
  };
}

// ── Mobile adaptation rule — authored + tested ONCE, every native painter inherits it ──

/** One bottom-nav destination on the mobile shell: a secondary listing (Who / Where) the
 *  phone reveals one tab at a time — never all panels crammed at once. */
export interface MobileTab {
  readonly id: string;
  readonly title: string;
  readonly cells: readonly ListingCell[];
}

/** The MOBILE adaptation of a `WorkspaceView`: the primary content owns the screen; the
 *  secondary listings become bottom-nav tabs; per-cell **dossier** detail (badges, meters,
 *  subtitle) is DROPPED — a phone shows presence, not a dossier. A native painter (Flutter,
 *  Swift, Kotlin) consumes THIS; the rule that produces it lives + is tested here, so the
 *  native app is a thin painter, not a place UX decisions are re-made
 *  ([[best-ux-per-portal-not-identical-projection]]). */
export interface MobileScreen {
  readonly title: string;
  readonly primary: ContentView;
  readonly tabs: readonly MobileTab[];
}

/**
 * `toMobileScreen` — the `@media (modality: mobile)` rule as a pure, testable function.
 * Derives a phone-native layout from the SAME neutral `WorkspaceView` the desktop paints as
 * a three-panel: conversation full-screen (`primary`), the who/where listings behind a
 * bottom nav (`tabs`), each cell stripped to presence essentials (id, title, glyph, status)
 * — the dossier badges/meters a desktop row can afford are dropped. Authored once; the
 * Flutter/Swift/Kotlin painter renders `MobileScreen` → native widgets. Verifiable without a
 * simulator — the rule is logic, the pixels are the grid's last mile.
 */
export function toMobileScreen(ws: WorkspaceView): MobileScreen {
  const presenceOnly = (c: ListingCell): ListingCell => ({
    id: c.id,
    title: c.title,
    ...(c.glyph !== undefined ? { glyph: c.glyph } : {}),
    ...(c.status !== undefined ? { status: c.status } : {}),
  });
  const tab = (v: ListingView): MobileTab => ({
    id: v.id,
    title: v.title,
    cells: v.cells.map(presenceOnly),
  });
  return {
    title: ws.nav.cells[0]?.title ?? '',
    primary: ws.content,
    // Only `Listing` widgets become bottom-nav tabs (roster → "Who"); a phone doesn't
    // put the metrics/status chrome in the nav — presence, not a dashboard.
    tabs: ws.left
      .filter((w) => w.kind === 'listing')
      .map((w) => tab(w.body as ListingView)),
  };
}

/** Build an empty content-dispatch table for a target. Fail-loud on unknown purpose. */
export function createContentRegistry<Out>(): ContentRegistry<Out> {
  const table = new Map<string, ContentRenderer<Out>>();
  return {
    register<Body>(purpose: string, renderer: ContentRenderer<Out, Body>): void {
      if (table.has(purpose)) {
        throw new Error(`content renderer already registered for purpose "${purpose}"`);
      }
      table.set(purpose, renderer as ContentRenderer<Out>);
    },
    render(view: ContentView): Out {
      const renderer = table.get(view.purpose);
      if (!renderer) {
        const known = [...table.keys()].join(', ') || '(none)';
        throw new Error(
          `no content renderer for room purpose "${view.purpose}" — registered: ${known}`,
        );
      }
      return renderer(view.body, view.purpose);
    },
  };
}
